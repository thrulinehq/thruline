//! BUILD-SPEC §9's hosted App, minus the sockets.
//!
//! # Why the App is split in two
//!
//! §9's App receives webhooks, finds a receipt, verifies it, evaluates policy
//! and posts a check run. Exactly one of those steps needs a network, and it is
//! the last one. Everything before it is a pure function of bytes — and a pure
//! function is a thing that can be tested, in a service where the interesting
//! failures are all security failures.
//!
//! So this crate holds the decisions and **names no HTTP crate at all**; the
//! socket shell lives in `services/github-app`, outside the workspace, where it
//! cannot bring a TLS stack into the engine's build. `tests/no_network.rs`
//! keeps the boundary from eroding.
//!
//! # The three ways a webhook endpoint gets owned
//!
//! 1. **Parsing before authenticating.** A JSON parser is a large attack
//!    surface and an unauthenticated one is a public attack surface.
//!    [`Webhook::authenticate`] takes raw bytes and returns a type; nothing in
//!    this crate parses a body without one, because the parsed form is only
//!    reachable through it.
//! 2. **A non-constant-time comparison** of the signature, which leaks the
//!    expected value one byte at a time. The comparison here is
//!    `subtle::ConstantTimeEq`.
//! 3. **Trusting a field in the payload.** A webhook body is written by
//!    whoever sent it. Repository names, installation ids and SHAs are used
//!    only after being checked against the shapes GitHub can actually produce
//!    — a repository "full name" that is a path traversal is refused here
//!    rather than at whatever joins it to a directory later.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

pub mod badge;
pub mod check_run;
pub mod comment;
pub mod fetch;
pub mod receipt;
pub mod receipt_source;
pub mod ruling;
#[cfg(test)]
mod tests;
pub mod verify_json;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WebhookError {
    #[error("no `X-Hub-Signature-256` header")]
    MissingSignature,
    #[error("the `X-Hub-Signature-256` header is not `sha256=<64 hex characters>`")]
    MalformedSignature,
    #[error("the signature does not match the body")]
    BadSignature,
    #[error("no `X-GitHub-Event` header")]
    MissingEvent,
    #[error("the body is not JSON: {0}")]
    NotJson(String),
    #[error("`{field}` is missing or is not the shape GitHub sends: {found}")]
    Untrusted { field: &'static str, found: String },
}

/// A webhook whose signature has been checked. **The only way to a parsed
/// body.**
#[derive(Debug, Clone)]
pub struct Webhook {
    event: String,
    payload: serde_json::Value,
}

impl Webhook {
    /// Verifies the HMAC and only then parses.
    ///
    /// `secret` is the App's webhook secret. `signature` is the raw
    /// `X-Hub-Signature-256` header value, `sha256=` prefix included.
    pub fn authenticate(
        event: Option<&str>,
        signature: Option<&str>,
        secret: &[u8],
        body: &[u8],
    ) -> Result<Webhook, WebhookError> {
        let event = event.ok_or(WebhookError::MissingEvent)?;
        let signature = signature.ok_or(WebhookError::MissingSignature)?;
        let hex = signature
            .strip_prefix("sha256=")
            .ok_or(WebhookError::MalformedSignature)?;
        if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(WebhookError::MalformedSignature);
        }
        let mut want = [0u8; 32];
        for (i, byte) in want.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
                .map_err(|_| WebhookError::MalformedSignature)?;
        }
        let mut mac = <HmacSha256 as KeyInit>::new_from_slice(secret)
            .map_err(|_| WebhookError::BadSignature)?;
        mac.update(body);
        let have = mac.finalize().into_bytes();
        // Constant time. A byte-by-byte comparison here leaks the expected
        // digest to anyone willing to time a few thousand requests.
        if have.as_slice().ct_eq(&want).unwrap_u8() != 1 {
            return Err(WebhookError::BadSignature);
        }
        // Parsed only now. An unauthenticated JSON parse is a public attack
        // surface, and the type system is what keeps that order: `payload` is
        // private and only this constructor fills it.
        let payload: serde_json::Value =
            serde_json::from_slice(body).map_err(|e| WebhookError::NotJson(e.to_string()))?;
        Ok(Webhook {
            event: event.to_string(),
            payload,
        })
    }

    pub fn event(&self) -> &str {
        &self.event
    }

    pub fn payload(&self) -> &serde_json::Value {
        &self.payload
    }

    /// The §9 events this App acts on. Anything else is acknowledged and
    /// ignored — an App that errors on events it did not ask for gets its
    /// deliveries disabled.
    ///
    /// # The `action` matters, and both directions of getting it wrong are real
    ///
    /// A `pull_request` delivery arrives for `opened`, `synchronize`,
    /// `reopened`, `closed`, `edited`, `assigned`, `labeled`, `milestoned` and
    /// more. Acting on all of them means re-verifying a receipt and re-posting
    /// a check run because someone changed the title, and posting a check onto
    /// a pull request that is already closed.
    ///
    /// **But `labeled` and `unlabeled` must stay in.** §9's
    /// `require_human_ack` is satisfied by the `tests-changed-ok` label, so
    /// adding that label is precisely the event that can turn a `neutral` into
    /// a pass — and a gate that ignores it leaves a reviewer applying a label
    /// that never takes effect, with nothing saying why.
    ///
    /// `ready_for_review` is in for the same reason: a draft becoming ready is
    /// the moment a repository starts caring about the answer.
    pub fn is_actionable(&self) -> bool {
        let action = self
            .payload
            .get("action")
            .and_then(|a| a.as_str())
            .unwrap_or("");
        match self.event.as_str() {
            "pull_request" => matches!(
                action,
                "opened"
                    | "synchronize"
                    | "reopened"
                    | "ready_for_review"
                    | "labeled"
                    | "unlabeled"
            ),
            // `check_suite` arrives as `requested` / `rerequested` /
            // `completed`. The last is GitHub telling us a suite finished,
            // which is not a request to check anything.
            "check_suite" => matches!(action, "requested" | "rerequested"),
            _ => false,
        }
    }

    /// What the App needs from the delivery, each checked against the shape
    /// GitHub can actually produce.
    pub fn subject(&self) -> Result<Subject, WebhookError> {
        let p = &self.payload;
        let repo = p
            .pointer("/repository/full_name")
            .and_then(|v| v.as_str())
            .ok_or(WebhookError::Untrusted {
                field: "repository.full_name",
                found: "absent".to_string(),
            })?;
        if !is_repo_full_name(repo) {
            return Err(WebhookError::Untrusted {
                field: "repository.full_name",
                found: repo.to_string(),
            });
        }
        let head_sha = p
            .pointer("/pull_request/head/sha")
            .or_else(|| p.pointer("/check_suite/head_sha"))
            .and_then(|v| v.as_str())
            .ok_or(WebhookError::Untrusted {
                field: "head_sha",
                found: "absent".to_string(),
            })?;
        if !is_sha(head_sha) {
            return Err(WebhookError::Untrusted {
                field: "head_sha",
                found: head_sha.to_string(),
            });
        }
        let installation = p
            .pointer("/installation/id")
            .and_then(serde_json::Value::as_u64)
            .ok_or(WebhookError::Untrusted {
                field: "installation.id",
                found: "absent".to_string(),
            })?;
        Ok(Subject {
            repo: repo.to_string(),
            head_sha: head_sha.to_string(),
            installation,
            pull_request: p
                .pointer("/pull_request/number")
                .and_then(serde_json::Value::as_u64),
        })
    }
}

/// `owner/name`, and nothing that could be a path.
///
/// GitHub's own rules are narrower than this; the point is not to reproduce
/// them but to refuse everything that could become a directory traversal or an
/// argument when this value is later joined to a path or a URL.
pub fn is_repo_full_name(s: &str) -> bool {
    let mut parts = s.split('/');
    let (Some(owner), Some(name), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let ok = |p: &str| {
        !p.is_empty()
            && p.len() <= 100
            && p != "."
            && p != ".."
            && !p.starts_with('-')
            && p.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
    };
    ok(owner) && ok(name)
}

/// 40 hex characters (SHA-1) or 64 (SHA-256).
pub fn is_sha(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// What one delivery is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject {
    pub repo: String,
    pub head_sha: String,
    pub installation: u64,
    pub pull_request: Option<u64>,
}

/// The `X-Hub-Signature-256` GitHub would send for `body`.
///
/// # Why the receiver ships the signer
///
/// A fixture harness has to be able to produce a delivery this receiver
/// **accepts**, or the authentication path is the one part of the pipeline no
/// test ever runs — and it is the part where being wrong is a compromise rather
/// than a bug. Putting it here rather than reimplementing HMAC in the harness
/// means the two halves cannot drift: if [`Webhook::authenticate`] changes what
/// it expects, this changes with it.
///
/// It is not a secret-handling convenience. The secret is the caller's, this
/// returns a header value, and nothing here stores or logs either.
pub fn sign_body(secret: &[u8], body: &[u8]) -> String {
    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(secret).expect("hmac accepts any key");
    mac.update(body);
    let digest = mac.finalize().into_bytes();
    let mut out = String::with_capacity(7 + 64);
    out.push_str("sha256=");
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}
