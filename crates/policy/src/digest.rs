//! The digest of the policy a receipt was produced under (BUILD-SPEC §8.1).
//!
//! # The failure this module exists to close
//!
//! `receipt_cmd` wrote `policy_hash: String::new()` and the verifier's first
//! match arm was `(_, "") => not_applicable`. Measured on 2026-09-04: a policy
//! rewritten to its **literal opposite** — `require_receipt` flipped,
//! `require_reexecuted` flipped, `allow_skips` flipped, `stop_gate` disabled,
//! `reexec.mode` changed — produced **byte-identical** `Verified.` output at
//! exit 0. §8 binds the policy into the receipt precisely so that cannot
//! happen; it was bound to nothing.
//!
//! # Why the digest is over the PARSED policy, not the file bytes
//!
//! The thing a reviewer needs pinned is the *rules that were in force*, not the
//! comments around them. Hashing raw bytes would flip the digest on a
//! whitespace edit — a false failure — and, worse, would happily produce a
//! digest for a file that **does not parse**, i.e. for rules nothing could
//! evaluate. Hashing [`Policy::canonical`] means the digest exists only where
//! the rules exist, and an unparseable policy is a loud error at receipt time
//! rather than a hash of gibberish.
//!
//! # Why "no policy file" is a digest and not an absence
//!
//! A repository with no `.thruline/policy.yml` was still judged under
//! [`Policy::default`], which is a real policy with real rules. Recording that
//! as an empty string made "judged under the defaults" indistinguishable from
//! "the producer did not fill this in", and the verifier could not tell the two
//! apart either. So absence gets [`absent_digest`]: a well-known, constant,
//! domain-separated hash. Every receipt states a policy; none can be silent.
//!
//! `absent` and a file that happens to parse to the defaults are deliberately
//! **different** digests. The rules coincide, but deleting the policy file is
//! exactly the mutation this check exists to catch, and a verifier that reports
//! it as unchanged would be the same collapse one hop away.

use std::path::{Path, PathBuf};

use crate::decisions::{OnStale, OnUnverifiable, RequireHold};
use crate::{ClaimRule, Policy, ReexecMode};

/// Domain tag. Separates a policy digest from every other sha256 in the system
/// and separates the two `state` words below from each other.
const DOMAIN: &str = "thruline.policy.v1";

/// Where a repository's policy lives. One definition, used by the producer and
/// by the verifier, so the two cannot look in different places.
pub fn policy_path(repo: &Path) -> PathBuf {
    repo.join(".thruline").join("policy.yml")
}

fn digest_of(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(text.as_bytes());
    let mut hex = String::from("sha256:");
    for b in h.finalize() {
        hex.push_str(&format!("{b:02x}"));
    }
    hex
}

/// The digest recorded when the repository has **no** policy file.
///
/// Constant, well known, and *not* the empty string — see the module docs.
pub fn absent_digest() -> String {
    digest_of(&format!("{DOMAIN}\nabsent\n"))
}

/// A policy digest could not be produced. **Never silently downgraded to
/// [`absent_digest`]** — "the file is broken" and "there is no file" are
/// different facts, and collapsing them would hide a policy the repository
/// intended to enforce.
#[derive(Debug, thiserror::Error)]
pub enum DigestError {
    #[error("{path} could not be read: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} does not parse, so the policy in force cannot be named: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: crate::ParseError,
    },
}

/// The digest of the policy in force for `repo`.
///
/// `Ok(absent_digest())` when there is no policy file; an **error** when there
/// is one and it cannot be read or parsed.
pub fn digest_for_repo(repo: &Path) -> Result<String, DigestError> {
    let path = policy_path(repo);
    match std::fs::read_to_string(&path) {
        Ok(text) => Policy::parse(&text)
            .map(|p| p.digest())
            .map_err(|source| DigestError::Parse { path, source }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(absent_digest()),
        Err(source) => Err(DigestError::Read { path, source }),
    }
}

impl Policy {
    /// The digest a receipt records for this policy.
    pub fn digest(&self) -> String {
        digest_of(&format!("{DOMAIN}\npresent\n{}", self.canonical()))
    }

    /// Every active field, in one fixed order. The optional Decisions block
    /// is appended only when enabled, and templates only when nonempty,
    /// preserving existing policy digests for files that do not opt in.
    ///
    /// Every enabled field is written in a fixed order. For an enabled block,
    /// omitted settings and explicit defaults produce the same text because
    /// they are the same rules. Template names follow map order; their lines
    /// retain their given order. [`crate::tests`] renders an enabled policy
    /// to guard against dropping a field from this digest.
    pub fn canonical(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("version: {}\n", self.version));
        out.push_str(&format!("require_receipt: {}\n", self.require_receipt));
        out.push_str("claims:\n");
        // `claims` is a BTreeMap, so this order is the map's, not the file's.
        for (name, rule) in &self.claims {
            out.push_str(&format!("  {name}:\n"));
            out.push_str(&claim_rule(rule));
        }
        out.push_str("promises:\n");
        out.push_str(&format!(
            "  block_on_open: {}\n  comment_open: {}\n",
            self.promises.block_on_open, self.promises.comment_open
        ));
        out.push_str("stop_gate:\n");
        out.push_str(&format!(
            "  enabled: {}\n  max_stop_blocks: {}\n",
            self.stop_gate.enabled, self.stop_gate.max_stop_blocks
        ));
        out.push_str("reexec:\n");
        out.push_str(&format!(
            "  mode: {}\n  reruns: {}\n  image: {}\n",
            match self.reexec.mode {
                ReexecMode::Claimed => "claimed",
                ReexecMode::Full => "full",
            },
            self.reexec.reruns,
            self.reexec.image
        ));
        if let Some(decisions) = self.decisions {
            out.push_str("decisions:\n");
            out.push_str(&format!(
                "  require_hold: {}\n  on_unverifiable: {}\n  on_stale: {}\n",
                match decisions.require_hold {
                    RequireHold::All => "all",
                    RequireHold::Blocking => "blocking",
                    RequireHold::None => "none",
                },
                match decisions.on_unverifiable {
                    OnUnverifiable::Block => "block",
                    OnUnverifiable::Warn => "warn",
                },
                match decisions.on_stale {
                    OnStale::Recompute => "recompute",
                    OnStale::Block => "block",
                },
            ));
        }
        if !self.templates.is_empty() {
            out.push_str("templates:\n");
            for (name, template) in &self.templates {
                // JSON strings and arrays keep quotes, delimiters, control
                // characters and line boundaries distinct in the preimage.
                out.push_str(&format!(
                    "  {}:\n    lines: {}\n",
                    serde_json::json!(name),
                    serde_json::json!(template.lines),
                ));
            }
        }
        out
    }
}

fn claim_rule(rule: &ClaimRule) -> String {
    let paths = match &rule.required_when_paths {
        // The globs are order-significant to nobody, but the digest must not
        // move because a file listed them differently, so they are sorted.
        Some(p) => {
            let mut p = p.clone();
            p.sort();
            format!("[{}]", p.join(","))
        }
        None => "always".to_string(),
    };
    let coverage = match rule.min_changed_line_coverage {
        Some(n) => n.to_string(),
        None => "none".to_string(),
    };
    format!(
        "    required_when_paths: {paths}\n\
             require_reexecuted: {}\n\
             require_evidence: {}\n\
             require_human_ack: {}\n\
             min_changed_line_coverage: {coverage}\n\
             allow_skips: {}\n",
        rule.require_reexecuted, rule.require_evidence, rule.require_human_ack, rule.allow_skips
    )
}
