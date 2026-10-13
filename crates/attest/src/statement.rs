//! BUILD-SPEC §8.1 — the in-toto Statement and ThruLine's predicate.
//!
//! # The predicate URI is settled, and it is in exactly one place
//!
//! §18 listed the predicate URI domain as a decision the agent must not guess.
//! **The operator ruled it on 2026-09-05: `thrulinehq.com`.** It lives in
//! [`PREDICATE_TYPE`] alone — **changing it is a one-line edit**, which is the
//! whole reason it is a constant rather than a literal repeated across a
//! builder and a verifier.
//!
//! The ruling was applied for free. §18's provisional default had never left
//! this machine: no release exists, no receipt has been issued outside a test,
//! and the branch carrying this crate has never been pushed. The paragraph
//! below is the price that was avoided, not one that was paid.
//!
//! The retired domain is not named here, and that is `predicate_domain_census`
//! doing its job — it caught this very paragraph after a rebase reintroduced
//! the name in prose. A census with an exception list is a census with a hole.
//!
//! It matters because a verifier keys on `predicateType`: every receipt already
//! issued fails to match after a change. Cheap now, expensive after the first
//! receipt leaves a machine.
//!
//! # What goes in, and what deliberately does not
//!
//! The predicate carries **hashes, not content** (§0.8). It carries the event
//! chain head so a verifier can ask the local ledger whether the record backing
//! it is the record it has. It does **not** carry §6.9's honest-failure
//! narrative — ruling C.9: the receipt is the gating artifact, anything in it
//! can be read by a policy engine, and "the agent was honest" would be the same
//! exploit one hop downstream.

use serde::{Deserialize, Serialize};

/// **RULED** by the operator, 2026-09-05: the predicate domain is
/// `thrulinehq.com`. §18 previously carried a different provisional default;
/// every occurrence of it was replaced in one commit, and
/// `tests/predicate_domain_census.rs` fails if one reappears in any tracked
/// file. That census caught this very comment on its first run, when it named
/// the retired domain in prose — the exception was declined and the prose
/// changed, because a census with an exception list is a census with a hole.
///
/// This string is written into **every receipt ever issued**, and a verifier
/// keys on it. Changing it again breaks every pinned fixture and every
/// previously-issued receipt — see `docs/DECISIONS-PENDING.md` D4 and the
/// fixture-breaking list in `docs/STATUS.md`.
///
/// `docs/spec/execution-check-v0.1.md` specifies this predicate for producers
/// that are not ThruLine; [`SCHEMA_ID`] below is the schema it points at.
pub const PREDICATE_TYPE: &str = "https://thrulinehq.com/attestation/execution-check/v0.1";

/// The published JSON Schema for [`PREDICATE_TYPE`]'s payload, ruled in the
/// same breath. Note the path is `/spec/`, not `/schema/` — the capsule
/// schemas use `/schema/` and these are different documents.
pub const SCHEMA_ID: &str = "https://thrulinehq.com/spec/execution-check/v0.1";

/// in-toto Statement type.
pub const STATEMENT_TYPE: &str = "https://in-toto.io/Statement/v1";

/// **The whole domain of `verdicts[].verdict`**, exactly as
/// `spec/execution-check-v0.1.schema.json` publishes it.
///
/// It is a `const` in one place rather than three string literals because it is
/// load-bearing for §6's non-collapse rule: every reader that decides *"is this
/// verdict `unverifiable`?"* by comparing a string is safe only while no other
/// spelling can reach it. [`Statement::validate`] is the door that guarantees
/// that, and this is the list it enforces.
pub const VERDICTS: [&str; 3] = ["supported", "unsupported", "unverifiable"];

/// DSSE payload type for an in-toto statement.
pub const PAYLOAD_TYPE: &str = "application/vnd.in-toto+json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Statement {
    #[serde(rename = "_type")]
    pub type_: String,
    pub subject: Vec<Subject>,
    #[serde(rename = "predicateType")]
    pub predicate_type: String,
    pub predicate: Predicate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subject {
    pub name: String,
    /// `gitTree` always; `gitCommit` when the work was committed.
    pub digest: std::collections::BTreeMap<String, String>,
}

/// §8.1's predicate.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Predicate {
    pub session: Session,
    pub claims: Vec<ClaimEntry>,
    pub checks: Vec<CheckEntry>,
    pub verdicts: Vec<VerdictEntry>,
    pub promises: Promises,
    pub verifier: VerifierInfo,
    pub capture: Capture,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub harness: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<String>,
    /// The head of the hash-chained event log. This is what makes the receipt
    /// checkable against a ledger rather than merely self-consistent.
    pub event_chain_head: String,
    pub event_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimEntry {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub asserted_by: AssertedBy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssertedBy {
    pub harness: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckEntry {
    pub id: String,
    pub category: String,
    pub runner: String,
    pub argv_hash: String,
    pub env_hash: String,
    pub cwd_hash: String,
    pub input_tree_hash: String,
    pub exit_code: i64,
    /// `{}` is never written here: an unparsed run carries
    /// `{"unverifiable": …}`, because a defaulted parsed block reads as a
    /// clean run.
    pub parsed: serde_json::Value,
    /// **Derived, never set.** `thruline receipt` computes this by asking the
    /// check's `exec` event whether it carries a container image digest; the
    /// event is inside the hash chain the receipt names, so the field cannot be
    /// added after the fact without breaking the chain head.
    ///
    /// [`crate::verify`] refuses a receipt whose `reexecuted` is `true` and
    /// whose [`CheckEntry::container_image_digest`] is absent — a boolean in a
    /// predicate is a claim, and a digest recomputable from the ledger is
    /// evidence.
    pub reexecuted: bool,
    /// §8.1's `container_image_digest`. Present exactly when `reexecuted` is
    /// true, and the reason it is allowed to be.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_image_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerdictEntry {
    pub claim_id: String,
    pub verdict: String,
    pub verifier: String,
    pub evidence: Vec<String>,
    pub reasons: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Promises {
    pub open: Vec<String>,
    pub fulfilled: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifierInfo {
    pub thruline_version: String,
    /// `local` until §7's re-execution exists.
    pub mode: String,
    /// §8.1's `policy_hash` — the policy this receipt was produced under.
    ///
    /// # Three states, three representations, and they used to be two
    ///
    /// This was a `String` that the only producer set to `String::new()`, and
    /// [`crate::verify`]'s first match arm read `""` as *not applicable*.
    /// Measured 2026-09-04: a policy rewritten to its literal opposite produced
    /// **byte-identical `Verified.` output at exit 0**. §8 binds the policy in
    /// so a receipt cannot be verified against a different policy than it was
    /// produced under; it was bound to nothing.
    ///
    /// - **absent** (`None`, field omitted) — the receipt does not say. A
    ///   defective receipt: every session was judged under *some* policy, the
    ///   defaults included, so silence here is a missing assertion and not an
    ///   absent axis. [`crate::verify`] **fails** it.
    /// - **empty** (`Some("")`) — a producer wrote a placeholder. Distinct from
    ///   absent, and also **failed**, with its own reason, so the two can never
    ///   again be read as one thing.
    /// - **stated** (`Some("sha256:…")`) — compared against the policy resolved
    ///   on the verifying machine. "No policy file" is itself a stated digest
    ///   (`thruline_policy::absent_digest`), never an empty string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_hash: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capture {
    pub sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_hash: Option<String>,
    pub redactions_applied: u64,
    /// §13. A receipt whose capture was degraded says so on its face, so a
    /// reader does not have to infer it from the verdicts.
    pub capture_degraded: bool,
    /// The content transforms in scope when the subject tree was snapshotted,
    /// described (`thruline_core::spec::snapshot::ContentFilters::describe`).
    ///
    /// **Present exactly when they degrade capture**, and then
    /// [`Capture::capture_degraded`] is true — [`crate::verify`] refuses a
    /// receipt whose two halves disagree, the same way it refuses
    /// `reexecuted` without a digest.
    ///
    /// `thruline receipt` used to call the snapshotter, let it print
    /// `WARN snapshot taken under content filters: the tree hash may not name
    /// the bytes on disk`, **discard the finding**, and sign a receipt saying
    /// `capture_degraded: false`. A `.gitattributes` clean filter is the one
    /// thing that breaks §0.3's tree binding at its root, so a receipt silent
    /// about it attests to bytes nobody looked at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_filters: Option<String>,
    /// **Why** capture was degraded — §8.1's `degraded_causes`, in the spec's
    /// own name.
    ///
    /// Empty exactly when [`Capture::capture_degraded`] is false. The schema
    /// enforces both directions, because the failure this field closes is a
    /// receipt that says `capture_degraded: true` and gives a reader nothing to
    /// search for.
    ///
    /// # The receipt that started it
    ///
    /// Track Q's D18 and Track R's F7. A session whose `Stop` hook never
    /// fired — a crashed agent, a killed terminal, a hook over its timeout —
    /// produced a receipt containing **no occurrence** of `stop`, `truncat` or
    /// `incomplete`, and `verify-receipt` printed *"capture was not degraded"*.
    /// The flag was then made to flip, and the receipt still said only
    /// `capture_degraded: true` with no cause: the schema's own description of
    /// this field predicted exactly that — *"a `true` here carries NO stated
    /// cause unless `content_filters` happens to be the reason"* — and it was
    /// right for as long as the field did not exist.
    ///
    /// Each cause is a machine-readable token, a colon, and a sentence:
    /// `hook_events_missing: …`, `content_filters: …`. The token is what a
    /// consumer greps; the sentence is what a person reads. Splitting them into
    /// two fields would let one drift from the other.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub degraded_causes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_env_digest: Option<String>,
}

impl Statement {
    pub fn new(subject: Vec<Subject>, predicate: Predicate) -> Statement {
        Statement {
            type_: STATEMENT_TYPE.to_string(),
            subject,
            predicate_type: PREDICATE_TYPE.to_string(),
            predicate,
        }
    }

    /// Structural checks a verifier makes before trusting anything inside.
    ///
    /// Separate from signature verification on purpose: a correctly signed
    /// statement of the wrong *type* is a valid signature over something this
    /// build does not understand, and treating it as a ThruLine receipt would
    /// be reading a foreign document's fields as if they were ours.
    pub fn validate(&self) -> Result<(), StatementError> {
        if self.type_ != STATEMENT_TYPE {
            return Err(StatementError::Type {
                found: self.type_.clone(),
                expected: STATEMENT_TYPE.to_string(),
            });
        }
        if self.predicate_type != PREDICATE_TYPE {
            return Err(StatementError::PredicateType {
                found: self.predicate_type.clone(),
                expected: PREDICATE_TYPE.to_string(),
            });
        }
        if self.subject.is_empty() {
            return Err(StatementError::NoSubject);
        }
        for s in &self.subject {
            if !s.digest.contains_key("gitTree") {
                return Err(StatementError::NoTreeDigest {
                    name: s.name.clone(),
                });
            }
        }
        if self.predicate.session.event_chain_head.is_empty() {
            return Err(StatementError::NoChainHead);
        }
        // **The closed verdict enum, enforced at the door.**
        //
        // `spec/execution-check-v0.1.schema.json` publishes `verdict` as
        // required, non-null and closed over exactly three lowercase words, and
        // says in `x-unverifiable-may-not-collapse` that a consumer meeting a
        // value outside the set must refuse the receipt. This build did not:
        // `verify` counts unverifiable verdicts with `v.verdict ==
        // "unverifiable"` over a plain `String`, so `"Unverifiable"` counted as
        // NOT unverifiable and the receipt reported *"none left
        // `unverifiable`"* — §6's collapse, in the product's most visible
        // sentence — while `report::VerdictSummary` filed the word under
        // `other` and carried on.
        //
        // Refusing here means an unknown word never reaches either reader.
        // A statement that fails this never has a field of its predicate
        // interpreted at all.
        for v in &self.predicate.verdicts {
            if !VERDICTS.contains(&v.verdict.as_str()) {
                return Err(StatementError::Verdict {
                    claim_id: v.claim_id.clone(),
                    found: v.verdict.clone(),
                });
            }
        }
        Ok(())
    }

    /// The tree this receipt is about.
    pub fn tree_digest(&self) -> Option<&str> {
        self.subject
            .first()?
            .digest
            .get("gitTree")
            .map(String::as_str)
    }

    pub fn commit_digest(&self) -> Option<&str> {
        self.subject
            .first()?
            .digest
            .get("gitCommit")
            .map(String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StatementError {
    #[error("statement _type is {found:?}, expected {expected:?}")]
    Type { found: String, expected: String },
    #[error("predicateType is {found:?}, expected {expected:?} — this is not a ThruLine receipt, or it was issued under a different predicate version")]
    PredicateType { found: String, expected: String },
    #[error("the statement names no subject, so it is about nothing")]
    NoSubject,
    #[error("subject {name:?} carries no gitTree digest, so it cannot be bound to a tree")]
    NoTreeDigest { name: String },
    #[error(
        "the predicate carries no event chain head, so nothing can be checked against a ledger"
    )]
    NoChainHead,
    #[error(
        "the verdict on claim {claim_id:?} is {found:?}, which is not one of the three words the \
         published schema allows (supported, unsupported, unverifiable). It is REFUSED rather \
         than filed under `other`: every reader that asks `is this verdict unverifiable?` does \
         so by comparing a string, so a fourth spelling is counted as not-unverifiable and the \
         receipt reports `none left unverifiable` — §6's collapse, wearing a typo."
    )]
    Verdict { claim_id: String, found: String },
}
