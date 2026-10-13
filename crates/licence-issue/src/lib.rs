//! T-DIST-6 — licence issuance, prepared and not deployed.
//!
//! The buyer's failure mode this crate exists to prevent is *doesn't trust
//! it*: paying and getting a licence the product rejects, or being charged
//! twice and handed two licences for one order. So:
//!
//! - [`issue`] writes T-DIST-4's format (`crates/licence/FORMAT.md`) through
//!   T-DIST-4's own [`thruline_licence::seal`], and the tests verify every
//!   licence it writes with T-DIST-4's own [`thruline_licence::verify`];
//! - a webhook is believed only after its signature checks over the **raw
//!   body** ([`Provider::receive`] verifies, then parses — never the reverse);
//! - a provider product id maps to a plan only through the committed mapping
//!   ([`config`]); an unknown id is refused, never defaulted;
//! - the replay store ([`store`]) answers a replayed event, or a second event
//!   for an order already paid, with the **byte-identical first licence**, and
//!   a refused event writes nothing to it.
//!
//! WHAT IS NOT HERE. No socket: the HTTP shell is `tools/licence-issuer`,
//! outside the workspace. No key: the caller passes the signing key and the
//! webhook secret in. No clock: the caller passes `now`. No delivery to the
//! buyer: that is a founder decision (`docs/loop/dist/HANDOFF.md`).
//!
//! **Choosing a provider is a one-file change**: the `providers` line of
//! `products.toml`. Each provider's webhook rules live in exactly one file,
//! `lemonsqueezy.rs` or `paddle.rs`; see `README.md`.

use ed25519_dalek::SigningKey;
use hmac::{Hmac, KeyInit, Mac};
use serde::Serialize;
use sha2::Sha256;

pub mod config;
pub mod fixtures;
pub mod lemonsqueezy;
pub mod paddle;
pub mod store;

pub use config::{Config, Grant};
pub use store::Store;

/* ── the provider interface ────────────────────────────────────────────── */

/// The request headers, as received. Names compare case-insensitively.
#[derive(Clone, Debug, Default)]
pub struct Headers(Vec<(String, String)>);

impl Headers {
    pub fn new() -> Headers {
        Headers(Vec::new())
    }

    pub fn with(mut self, name: &str, value: &str) -> Headers {
        self.0.push((name.to_string(), value.to_string()));
        self
    }

    pub fn push(&mut self, name: &str, value: &str) {
        self.0.push((name.to_string(), value.to_string()));
    }

    /// The first value of `name`, compared case-insensitively.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// One paid order, as a provider reported it, after its signature checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaidOrder {
    /// The provider's name as the mapping file and the URL spell it.
    pub provider: &'static str,
    /// The provider's id for this delivery of this event.
    pub event_id: String,
    /// The provider's id for the order (Lemon Squeezy order, Paddle transaction).
    pub order_id: String,
    /// The id the mapping file keys on (Lemon Squeezy variant, Paddle price).
    pub product_id: String,
    /// Whether the provider reports this as a test-mode payment.
    pub test_mode: bool,
    /// When the provider says it was paid, as the provider wrote it.
    pub paid_at: String,
    /// The machine fingerprint the buyer gave at checkout, if any. Without one
    /// the licence is not tied to a machine.
    pub machine: Option<String>,
}

/// What a signed webhook turned out to be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A completed payment: a licence may be issued for it.
    Paid(PaidOrder),
    /// A genuine event that is not a completed payment (a refund, an update, a
    /// subscription event). Nothing is issued and nothing is written.
    NotAPayment { event_type: String },
}

/// One payment provider's webhook rules. Each implementation lives in its own
/// file, and nothing outside that file knows its header names or body shape.
pub trait Provider: Sync {
    /// The provider's name, as `products.toml` and `/webhook/<name>` spell it.
    fn name(&self) -> &'static str;

    /// Check the signature over the exact raw body and the headers. Nothing
    /// in the body is read before this passes.
    fn verify(
        &self,
        secret: &[u8],
        headers: &Headers,
        body: &[u8],
        now: i64,
    ) -> Result<(), Refusal>;

    /// Read a body whose signature has already been checked.
    fn parse(&self, body: &[u8]) -> Result<Event, Refusal>;

    /// Verify, then parse. The order is the point: callers use this, so no
    /// caller can parse a body it has not verified.
    fn receive(
        &self,
        secret: &[u8],
        headers: &Headers,
        body: &[u8],
        now: i64,
    ) -> Result<Event, Refusal> {
        self.verify(secret, headers, body, now)?;
        self.parse(body)
    }
}

/// Every provider this build knows. Adding one is one new file and one line
/// here; **choosing** among them is the `providers` line of `products.toml`.
pub const PROVIDERS: &[&dyn Provider] = &[&lemonsqueezy::LemonSqueezy, &paddle::Paddle];

/// The provider called `name`, if this build knows one.
pub fn provider(name: &str) -> Option<&'static dyn Provider> {
    PROVIDERS.iter().copied().find(|p| p.name() == name)
}

/// HMAC-SHA256 of `parts` (concatenated) under `secret`, compared in constant
/// time to the hex tag a provider sent. Both providers call this and nothing
/// else to decide whether a signature matches.
pub(crate) fn hmac_matches(secret: &[u8], parts: &[&[u8]], tag_hex: &str) -> bool {
    let Some(tag) = unhex(tag_hex.trim()) else {
        return false;
    };
    let Ok(mut mac) = <Hmac<Sha256> as KeyInit>::new_from_slice(secret) else {
        return false;
    };
    for part in parts {
        mac.update(part);
    }
    mac.verify_slice(&tag).is_ok()
}

/// HMAC-SHA256 of `parts` under `secret`, as lowercase hex. The signing half,
/// used by each provider's `sign` (tests and a founder's local dry run).
pub(crate) fn hmac_hex(secret: &[u8], parts: &[&[u8]]) -> String {
    match <Hmac<Sha256> as KeyInit>::new_from_slice(secret) {
        Ok(mut mac) => {
            for part in parts {
                mac.update(part);
            }
            thruline_licence::to_hex(&mac.finalize().into_bytes())
        }
        Err(_) => String::new(),
    }
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.is_empty() || !text.len().is_multiple_of(2) {
        return None;
    }
    let digit = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    };
    text.as_bytes()
        .chunks(2)
        .map(|pair| Some(digit(pair[0])? << 4 | digit(pair[1])?))
        .collect()
}

/* ── refusals ──────────────────────────────────────────────────────────── */

/// Why nothing was issued. Each has its own plain sentence, and no two share
/// one. None of them accuses the buyer; the request is the subject.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    #[error("No payment provider called \"{0}\" is set up in this issuer, so nothing was issued.")]
    UnknownProvider(String),
    #[error("The request carries no {0} signature, so nothing was issued.")]
    MissingSignature(&'static str),
    #[error("The signature does not match this request's body, so nothing was issued.")]
    BadSignature,
    #[error(
        "The signature's timestamp is more than {0} seconds from this machine's clock, so nothing was issued."
    )]
    StaleSignature(i64),
    #[error("The body isn't in the shape this provider sends ({0}), so nothing was issued.")]
    Malformed(String),
    #[error("This event (\"{0}\") is not a completed payment, so there is nothing to issue.")]
    NotAPayment(String),
    #[error(
        "This is a test-mode payment, and this issuer is set up for real payments only, so nothing was issued."
    )]
    TestMode,
    #[error(
        "No plan is set up for {provider} product \"{id}\", so nothing was issued. An unknown product is never given a plan by default; add it to products.toml."
    )]
    UnknownProduct { provider: String, id: String },
    #[error(
        "The machine fingerprint given at checkout is not 64 lowercase hex characters, so nothing was issued."
    )]
    BadMachine,
    #[error(
        "The record of issued licences could not be read or written ({0}), so nothing was issued."
    )]
    Store(String),
    #[error("The licence could not be written ({0}), so nothing was issued.")]
    Seal(String),
}

impl Refusal {
    /// The HTTP status the shell answers with. A genuine event that is not a
    /// payment is a success (the provider must not retry it); a bad signature
    /// is 401; a request the issuer understood and will not honour is 422.
    pub fn http_status(&self) -> u16 {
        match self {
            Refusal::NotAPayment(_) => 202,
            Refusal::UnknownProvider(_) => 404,
            Refusal::MissingSignature(_) | Refusal::BadSignature | Refusal::StaleSignature(_) => {
                401
            }
            Refusal::Malformed(_) => 400,
            Refusal::TestMode | Refusal::UnknownProduct { .. } | Refusal::BadMachine => 422,
            Refusal::Store(_) | Refusal::Seal(_) => 500,
        }
    }
}

/* ── signing ───────────────────────────────────────────────────────────── */

/// A paid order with the plan and duration the mapping gave it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entitled {
    pub order: PaidOrder,
    pub grant: Grant,
}

/// The payload, in FORMAT.md's field order. Serialised once, compactly, and
/// the resulting bytes are the bytes stored and signed.
#[derive(Serialize)]
struct Payload<'a> {
    format: u64,
    licence_id: &'a str,
    product: &'a str,
    plan: &'a str,
    issued_at: &'a str,
    expires_at: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    machine: Option<&'a str>,
}

/// Write a licence for `order` in T-DIST-4's format, signed by `signing`.
///
/// `issued_at` is `now`; `expires_at` is `now` plus the grant's days. The
/// licence id is a fresh ULID (its random part means a second mint for one
/// order could never be mistaken for the first). `now` is seconds since the
/// Unix epoch.
pub fn issue(order: &Entitled, signing: &SigningKey, now: i64) -> Result<Vec<u8>, Refusal> {
    let secs = u64::try_from(now).map_err(|_| Refusal::Seal("the clock is before 1970".into()))?;
    let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
    let licence_id = ulid::Ulid::from_datetime(at).to_string();
    let expires = now + i64::from(order.grant.days) * 86_400;
    let (issued_at, expires_at) = (
        thruline_licence::timestamp(now),
        thruline_licence::timestamp(expires),
    );
    let payload = Payload {
        format: thruline_licence::FORMAT,
        licence_id: &licence_id,
        product: thruline_licence::PRODUCT,
        plan: order.grant.plan.as_str(),
        issued_at: &issued_at,
        expires_at: &expires_at,
        machine: order.order.machine.as_deref(),
    };
    let json = serde_json::to_string(&payload).map_err(|e| Refusal::Seal(e.to_string()))?;
    let sealed = thruline_licence::seal(json.as_bytes(), signing)
        .ok_or_else(|| Refusal::Seal("the payload is not one line".into()))?;
    // FORMAT.md allows `#` lines; the one here tells the buyer what to do.
    let text = sealed.replacen(
        &format!("{}\n", thruline_licence::HEADER),
        &format!(
            "{}\n# Install with: thruline licence install <this file>\n",
            thruline_licence::HEADER
        ),
        1,
    );
    Ok(text.into_bytes())
}

/* ── the issuer ────────────────────────────────────────────────────────── */

/// A webhook secret, and whether it is the provider's sandbox (test) secret.
/// Its value is never printed: `Debug` says only whether one is held.
#[derive(Clone)]
pub struct WebhookSecret {
    value: Vec<u8>,
    sandbox: bool,
}

impl WebhookSecret {
    /// A live secret.
    pub fn live(value: impl Into<Vec<u8>>) -> WebhookSecret {
        WebhookSecret {
            value: value.into(),
            sandbox: false,
        }
    }

    /// A sandbox secret: every event it verifies counts as test mode. Paddle
    /// marks test payments by environment rather than in the body, so this is
    /// how a Paddle sandbox event is told apart.
    pub fn sandbox(value: impl Into<Vec<u8>>) -> WebhookSecret {
        WebhookSecret {
            value: value.into(),
            sandbox: true,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }
}

impl std::fmt::Debug for WebhookSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WebhookSecret(<held>, sandbox: {})", self.sandbox)
    }
}

/// What the issuer handed back for one accepted event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issued {
    pub provider: &'static str,
    pub order_id: String,
    pub licence_id: String,
    /// The licence file's bytes: byte-identical on every replay.
    pub licence: Vec<u8>,
    /// True when these bytes were issued for an earlier event.
    pub replayed: bool,
}

/// The mapping, the replay store and the signing key, together.
pub struct Issuer {
    config: Config,
    store: Store,
    signing: SigningKey,
}

impl Issuer {
    pub fn new(config: Config, store: Store, signing: SigningKey) -> Issuer {
        Issuer {
            config,
            store,
            signing,
        }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// One webhook, end to end: provider enabled → signature → parse → test
    /// mode → replay (by event, then by order) → mapping → sign → record.
    ///
    /// Every refusal before "record" leaves the store exactly as it was.
    pub fn handle(
        &mut self,
        provider_name: &str,
        secret: &WebhookSecret,
        headers: &Headers,
        body: &[u8],
        now: i64,
    ) -> Result<Issued, Refusal> {
        let provider = provider(provider_name)
            .filter(|p| self.config.enabled(p.name()))
            .ok_or_else(|| Refusal::UnknownProvider(provider_name.to_string()))?;
        if secret.is_empty() {
            return Err(Refusal::BadSignature);
        }
        let mut order = match provider.receive(&secret.value, headers, body, now)? {
            Event::Paid(order) => order,
            Event::NotAPayment { event_type } => return Err(Refusal::NotAPayment(event_type)),
        };
        order.test_mode |= secret.sandbox;
        if order.test_mode && !self.config.accept_test_mode() {
            return Err(Refusal::TestMode);
        }
        if let Some(machine) = &order.machine {
            let ok = machine.len() == 64
                && machine
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
            if !ok {
                return Err(Refusal::BadMachine);
            }
        }
        let config = &self.config;
        let signing = &self.signing;
        self.store.issue_once(&order, now, |order| {
            let grant = config
                .grant(order.provider, &order.product_id)
                .ok_or_else(|| Refusal::UnknownProduct {
                    provider: order.provider.to_string(),
                    id: order.product_id.clone(),
                })?;
            let entitled = Entitled {
                order: order.clone(),
                grant,
            };
            issue(&entitled, signing, now)
        })
    }
}

/// The licence id inside licence bytes this crate wrote, for the store's
/// uniqueness column. `None` for bytes that are not one of ours.
pub(crate) fn licence_id_of(licence: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(licence).ok()?;
    let payload = text.lines().find_map(|l| l.strip_prefix("payload: "))?;
    let v: serde_json::Value = serde_json::from_str(payload).ok()?;
    v.get("licence_id")?.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_only_the_right_tag() {
        let tag = hmac_hex(b"k", &[b"a", b"b"]);
        assert!(hmac_matches(b"k", &[b"ab"], &tag));
        assert!(hmac_matches(b"k", &[b"ab"], &tag.to_uppercase()));
        assert!(!hmac_matches(b"k", &[b"ac"], &tag));
        assert!(!hmac_matches(b"j", &[b"ab"], &tag));
        assert!(!hmac_matches(b"k", &[b"ab"], ""));
        assert!(!hmac_matches(b"k", &[b"ab"], "zz"));
    }

    #[test]
    fn every_provider_is_found_by_its_own_name_and_no_other() {
        for p in PROVIDERS {
            assert_eq!(provider(p.name()).map(|q| q.name()), Some(p.name()));
        }
        assert!(provider("stripe").is_none());
        assert_eq!(PROVIDERS.len(), 2);
    }

    #[test]
    fn a_secret_never_prints() {
        let s = WebhookSecret::live("visible-if-broken");
        assert!(!format!("{s:?}").contains("visible"));
    }

    #[test]
    fn every_refusal_has_its_own_sentence() {
        let all = [
            Refusal::UnknownProvider("x".into()),
            Refusal::MissingSignature("X-Signature"),
            Refusal::BadSignature,
            Refusal::StaleSignature(300),
            Refusal::Malformed("x".into()),
            Refusal::NotAPayment("x".into()),
            Refusal::TestMode,
            Refusal::UnknownProduct {
                provider: "p".into(),
                id: "x".into(),
            },
            Refusal::BadMachine,
            Refusal::Store("x".into()),
            Refusal::Seal("x".into()),
        ];
        let sentences: std::collections::BTreeSet<String> =
            all.iter().map(|r| r.to_string()).collect();
        assert_eq!(sentences.len(), all.len());
        for s in &sentences {
            assert!(s.ends_with('.'), "{s}");
        }
    }
}
