//! §37.6 (A209) — the canonical Recovery Capsule, v2 (MANDATE-v4 §15).
//!
//! The v2 capsule is the §15 document: canonical JSON is the authority and
//! the human-readable markdown is derived from it by ONE renderer — no
//! independent facts. v1 (`capsule.rs`) is untouched and stays the readable
//! compatibility surface; v2 is the versioned replacement the mandate's §5
//! carve-out licenses, produced beside it, never instead of it.
//!
//! Contracts this module carries:
//!
//! 1. **Immutable identity.** `capsule_id` is the SHA-256 of the TLCAP_V2
//!    canonical serialization of every content field — the id itself is the
//!    one field excluded from its own preimage, by construction. The sealed
//!    document's byte digest (its CAS key in the protected store) travels
//!    beside the capsule, never inside it. Changing ANY hashed field fails
//!    verification: the id no longer recomputes ([`verify_capsule`]).
//! 2. **Sealed means sealed.** A later source-session event cannot mutate an
//!    existing capsule: the store is content-addressed, so new state is a new
//!    document with a new id, linked by `lineage`. Nothing here updates.
//! 3. **Five §15 categories, structurally distinct.** Established facts,
//!    agent assertions, unknowns, open obligations and recommendations are
//!    separate typed arrays end-to-end — schema, Rust, renderer. There is no
//!    shared bucket a renderer could blend.
//! 4. **Receipts terminate at raw evidence only.** Every factual line
//!    carries an [`observation::SourceReceipt`], whose terminal is one of
//!    §11's six arms. No capsule-id arm exists, so a capsule citing itself
//!    is unrepresentable. This compiler emits `Transcript` terminals for
//!    everything derived from the recorded transcript and
//!    `SnapshotManifest` terminals for everything derived from a cited
//!    snapshot; it never emits `ProcessExecution` — that arm means an
//!    *independently captured* execution, and a transcript-recorded command
//!    is not one. (The evidence-binding join that would earn that arm needs
//!    the §12 repository identity a live workspace observation supplies;
//!    rescue observes no workspace by contract. Named seam for Stage 7.)
//! 5. **Masking before persistence, quarantine on failure** (§15, §34).
//!    Every rendered string channel is masked BEFORE the document exists as
//!    bytes. A masking failure quarantines the element — the raw value is
//!    replaced, a named marker lands in `incomplete_evidence`, and the
//!    completeness declaration goes `incomplete`. The raw value reaches no
//!    output, error path included: the failure seam carries no content.
//! 6. **§5 is absolute in capsule copy.** A no-proof note on an agent
//!    assertion is the advisory sentence, verbatim
//!    ([`ADVISORY_NO_PROOF`]) — never an authoritative fact, never a block.
//! 7. **The trim discloses what it withheld** (§15). The token target is
//!    2,000; trimming walks [`TRIM_ORDER`] only — the categories in
//!    [`NEVER_TRIMMED`] are correctness-critical and are never dropped to
//!    meet a size target.
//!
//! The TLCAP_V2 canonical layout (length-prefixed: u64 LE byte length +
//! UTF-8; `opt` = 0x00 absent / 0x01 + lp present; u8 bools; u64 LE counts
//! and numbers; field-tag literals; `lp("complete")` trailer) is documented
//! field-by-field on [`canonical_bytes`] and pinned by python3 vectors
//! computed before this encoder existed (`a209_capsule_v2.rs`).

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::candidates;
use crate::chain::sha256_hex;
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::observation::{
    append, AppendOutcome, Completeness, DerivedRef, NewObservation, ObservationType,
    ReceiptTerminal, SourceReceipt,
};
use crate::protected_store::ProtectedStore;
use crate::redact::Treatment;
use crate::snapshot::WorkspaceSnapshot;

/// The canonical-layout version literal. Changing any byte of the layout is
/// a migration to TLCAP_V3, and the pinned vectors make that loud.
pub const CAPSULE_FORMAT: &str = "TLCAP_V2";
/// The schema version of the v2 document.
pub const CAPSULE_V2_VERSION: u64 = 2;
/// The stable-obligation-id preimage version literal.
pub const OBLIGATION_ID_FORMAT: &str = "TLOBL_V1";
/// §5's advisory phrasing, verbatim and mandatory for any no-proof note.
pub const ADVISORY_NO_PROOF: &str =
    "claim observed; required supporting proof was not found within the stated search scope";
/// §15's token target. A budget, not a cap: correctness-critical categories
/// are never trimmed to meet it.
pub const DEFAULT_TOKEN_BUDGET: u64 = 2_000;
/// How many rows a category source query materializes before the compile
/// discloses a ceiling instead of pretending completeness.
const CATEGORY_CEILING: usize = 200;
/// `current_failures` is never trimmed for size, so its ceiling is where
/// its honesty lives: without a resolution model, "current" cannot mean
/// "every failure ever recorded" — the first live seal read 84 all-time
/// failed commands as current and measured 6,854 tokens against the 2,000
/// target. The category carries the NEWEST failures up to this ceiling and
/// the elision is disclosed as an incomplete-evidence marker, exactly like
/// every other ceiling.
pub const FAILURE_CEILING: usize = 20;
/// What a quarantined element's text becomes. Fixed — carries no content.
pub const QUARANTINE_TEXT: &str = "[quarantined: masking failed — content withheld]";

/// Trim order under the token budget — first listed, first trimmed, always
/// from the end of the array. Pinned by test.
pub const TRIM_ORDER: [&str; 6] = [
    "agent_assertions",
    "established_work",
    "unknowns",
    "original_task",
    "decisions_constraints",
    "recommendations",
];
/// Categories the trim may never touch: failures, obligations, the dirty
/// state a continuation needs, and the incompleteness disclosures
/// themselves. Identity, citation, redaction and completeness fields are
/// not arrays and are structurally untouchable by the trim.
pub const NEVER_TRIMMED: [&str; 4] = [
    "current_failures",
    "open_obligations",
    "continuation_state",
    "incomplete_evidence",
];

/* ── the document ───────────────────────────────────────────────────── */

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratorV2 {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeV2 {
    pub kind: String,
    pub id: String,
}

/// Source agent and session (§15). For a project scope the session is the
/// most recent recorded one — the capsule's sections cite their own
/// sessions line by line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceV2 {
    pub agent: String,
    pub session: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryV2 {
    pub resolves: bool,
    pub repo_id: Option<String>,
    pub basis: Option<String>,
}

/// Workspace identity (§12's `wsid1:` string) where the caller observed
/// one; an absent identity states its basis rather than guessing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceV2 {
    pub identity: Option<String>,
    pub basis: String,
}

/// Snapshot ID and digest (§15), completeness attached — a capsule citing
/// a snapshot never assumes the capture held still.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotCitationV2 {
    pub snapshot_id: String,
    pub doc_sha256: String,
    /// The TLSNAP_V1 byte-layer digest, when the snapshot carries one.
    pub manifest_digest: Option<String>,
    pub complete: bool,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub taken_at: String,
}

/// Creation cutoff (§15): the chain position and moment this capsule was
/// sealed at. Later source-session output cannot change it — new state is a
/// new capsule with `lineage`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CutoffV2 {
    pub chain_position: String,
    pub event_total: u64,
    pub created_at: String,
    pub statement: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskLine {
    pub text: String,
    pub receipt: SourceReceipt,
}

/// Category 1 — established facts: receipted, §9-stated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstablishedFact {
    pub text: String,
    /// A §9 state — `RECORD_SUPPORTED` from this compiler; never `ASSERTED`.
    pub state: String,
    pub receipt: SourceReceipt,
}

/// Category 2 — agent assertions not established. Always `ASSERTED`; any
/// no-proof note is the §5 advisory sentence and nothing stronger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentAssertion {
    pub text: String,
    pub state: String,
    pub advisory: Option<String>,
    pub receipt: SourceReceipt,
}

/// Category 3 — known unknowns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnknownItem {
    pub text: String,
    pub state: String,
    pub receipt: SourceReceipt,
}

/// Category 4 — open obligations, each with a stable id deterministic over
/// its SOURCE identity ([`obligation_id`]) — never over wording.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenObligation {
    pub item_id: String,
    pub kind: String,
    pub origin: String,
    pub artifact_key: String,
    pub text: String,
    pub receipt: SourceReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailureLine {
    pub text: String,
    pub receipt: SourceReceipt,
}

/// An incomplete-evidence marker (§15): overflowed tool results, source
/// ceilings, quarantined elements. Named, never silent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncompleteMarker {
    pub marker: String,
    pub reason: String,
}

/// Dirty and untracked work needed for continuation — snapshot-cited,
/// never inlined bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationRef {
    pub text: String,
    pub receipt: SourceReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionLine {
    pub text: String,
    pub receipt: SourceReceipt,
}

/// Category 5 — recommendations, labelled as recommendations. Not factual
/// lines: each carries the rule that produced it, not a receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recommendation {
    pub text: String,
    pub label: String,
    pub basis: String,
}

/// Why a category array is empty — v1's Law 2, carried forward: a measured
/// zero and a structural gap are different facts.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyDecl {
    pub category: String,
    pub kind: String,
    pub reason: String,
}

/// Redaction declaration (§15): the treatment RECEIVED, never a property
/// held — v1's SecretsPolicy stance, unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedactionV2 {
    pub treatment: Treatment,
    pub statement: String,
    pub structural_exclusions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletenessV2 {
    /// `"complete"` or `"incomplete"`.
    pub state: String,
    pub reasons: Vec<String>,
}

/// One trim, disclosed: which category, how many items, how many tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrimDisclosure {
    pub category: String,
    pub items_withheld: u64,
    pub tokens_withheld: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasurementV2 {
    /// ceil(chars/4) over the rendered markdown, measured with this field
    /// zeroed and then stamped (documented; a re-measure reads ±1 token).
    pub token_estimate: u64,
    pub budget: u64,
    pub trimmed: Vec<TrimDisclosure>,
}

/// The §15 document. Every field always serializes — the canonical JSON is
/// total, so a reader never infers anything from an absent key. Unknown
/// fields REFUSE at parse (`deny_unknown_fields`), deliberately inverting
/// v1's Law 4: v2's document is a closed, hash-covered contract, and a
/// field this reader cannot cover with the id is a document it cannot
/// verify — refusing loudly beats silently re-serializing a lossy copy. A
/// future producer changes `format` and ships TLCAP_V3.
///
/// **The refusal is applied at every level** (A209 correction). It was
/// originally on the root struct alone, so a key injected into any nested
/// object — `generator`, `scope`, `cutoff`, an `open_obligations[]` entry —
/// parsed, verified and was silently dropped. Never a tamper hole (the id
/// covers every field that IS present), but the stated rationale did not
/// hold below the root, and now it does. The one deliberate exception is
/// [`SourceReceipt`], whose flattened `extra` map absorbs unknown receipt
/// fields ON PURPOSE and carries them INSIDE the hash — preservation with
/// coverage, which is the property the root refusal exists to guarantee.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapsuleV2 {
    pub capsule_version: u64,
    pub format: String,
    /// Content-addressed identity — the one field outside its own preimage.
    pub capsule_id: String,
    pub generator: GeneratorV2,
    pub scope: ScopeV2,
    pub source: SourceV2,
    pub repository: RepositoryV2,
    pub workspace: WorkspaceV2,
    pub snapshot: Option<SnapshotCitationV2>,
    pub cutoff: CutoffV2,
    pub lineage: Option<String>,
    pub original_task: Vec<TaskLine>,
    pub established_work: Vec<EstablishedFact>,
    pub agent_assertions: Vec<AgentAssertion>,
    pub unknowns: Vec<UnknownItem>,
    pub open_obligations: Vec<OpenObligation>,
    pub current_failures: Vec<FailureLine>,
    pub incomplete_evidence: Vec<IncompleteMarker>,
    pub continuation_state: Vec<ContinuationRef>,
    pub decisions_constraints: Vec<DecisionLine>,
    pub recommendations: Vec<Recommendation>,
    pub category_empties: Vec<EmptyDecl>,
    pub redaction: RedactionV2,
    pub completeness: CompletenessV2,
    pub measurement: MeasurementV2,
}

/* ── canonical serialization ────────────────────────────────────────── */

fn lp(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u64).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn opt(out: &mut Vec<u8>, v: Option<&str>) {
    match v {
        None => out.push(0x00),
        Some(s) => {
            out.push(0x01);
            lp(out, s);
        }
    }
}

fn u64le(out: &mut Vec<u8>, n: u64) {
    out.extend_from_slice(&n.to_le_bytes());
}

fn u8b(out: &mut Vec<u8>, b: bool) {
    out.push(u8::from(b));
}

fn receipt_json(r: &SourceReceipt) -> Result<String> {
    r.to_canonical_json()
}

/// The TLCAP_V2 canonical preimage of a capsule's content — everything
/// except `capsule_id`. Layout, in order (lp = u64 LE length + UTF-8;
/// opt = 0x00 | 0x01+lp; u8 bools; u64 LE numbers and counts):
///
/// ```text
/// lp(format) u64(capsule_version)
/// lp("generator") lp(name) lp(version)
/// lp("scope") lp(kind) lp(id)
/// lp("source") lp(agent) lp(session)
/// lp("repository") u8(resolves) opt(repo_id) opt(basis)
/// lp("workspace") opt(identity) lp(basis)
/// lp("snapshot") 0x00 | 0x01 lp(snapshot_id) lp(doc_sha256)
///                opt(manifest_digest) u8(complete) opt(head) opt(branch)
///                lp(taken_at)
/// lp("cutoff") lp(chain_position) u64(event_total) lp(created_at)
///              lp(statement)
/// lp("lineage") opt(lineage)
/// lp("task")         cnt { lp(text) lp(receipt_json) }
/// lp("established")  cnt { lp(text) lp(state) lp(receipt_json) }
/// lp("assertions")   cnt { lp(text) lp(state) opt(advisory)
///                          lp(receipt_json) }
/// lp("unknowns")     cnt { lp(text) lp(state) lp(receipt_json) }
/// lp("obligations")  cnt { lp(item_id) lp(kind) lp(origin)
///                          lp(artifact_key) lp(text) lp(receipt_json) }
/// lp("failures")     cnt { lp(text) lp(receipt_json) }
/// lp("incomplete_evidence") cnt { lp(marker) lp(reason) }
/// lp("continuation") cnt { lp(text) lp(receipt_json) }
/// lp("decisions")    cnt { lp(text) lp(receipt_json) }
/// lp("recommendations") cnt { lp(text) lp(label) lp(basis) }
/// lp("empties")      cnt { lp(category) lp(kind) lp(reason) }   # sorted
/// lp("redaction") lp(treatment_json) lp(statement)
///                 cnt { lp(exclusion) }                # sorted + deduped
/// lp("completeness") lp(state) cnt { lp(reason) }      # document order
/// lp("measurement") u64(token_estimate) u64(budget)
///                   cnt { lp(category) u64(items) u64(tokens) }
/// lp("complete")
/// ```
///
/// Item arrays keep document order — their order is meaning (compile
/// priority feeds the trim). `empties` and `structural_exclusions` are
/// normalized here, so arrival order is nuisance for them. Receipts are
/// embedded as their own canonical JSON ([`SourceReceipt`]'s sorted-map
/// encoding), so an unknown receipt field is hash-covered too.
///
/// This layout is an on-disk contract: the pinned python3 vectors in
/// `a209_capsule_v2.rs` were computed before this encoder existed, and any
/// change here is a versioned migration to TLCAP_V3, never a refactor.
pub fn canonical_bytes(c: &CapsuleV2) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(4096);
    lp(&mut out, &c.format);
    u64le(&mut out, c.capsule_version);
    lp(&mut out, "generator");
    lp(&mut out, &c.generator.name);
    lp(&mut out, &c.generator.version);
    lp(&mut out, "scope");
    lp(&mut out, &c.scope.kind);
    lp(&mut out, &c.scope.id);
    lp(&mut out, "source");
    lp(&mut out, &c.source.agent);
    lp(&mut out, &c.source.session);
    lp(&mut out, "repository");
    u8b(&mut out, c.repository.resolves);
    opt(&mut out, c.repository.repo_id.as_deref());
    opt(&mut out, c.repository.basis.as_deref());
    lp(&mut out, "workspace");
    opt(&mut out, c.workspace.identity.as_deref());
    lp(&mut out, &c.workspace.basis);
    lp(&mut out, "snapshot");
    match &c.snapshot {
        None => out.push(0x00),
        Some(s) => {
            out.push(0x01);
            lp(&mut out, &s.snapshot_id);
            lp(&mut out, &s.doc_sha256);
            opt(&mut out, s.manifest_digest.as_deref());
            u8b(&mut out, s.complete);
            opt(&mut out, s.head.as_deref());
            opt(&mut out, s.branch.as_deref());
            lp(&mut out, &s.taken_at);
        }
    }
    lp(&mut out, "cutoff");
    lp(&mut out, &c.cutoff.chain_position);
    u64le(&mut out, c.cutoff.event_total);
    lp(&mut out, &c.cutoff.created_at);
    lp(&mut out, &c.cutoff.statement);
    lp(&mut out, "lineage");
    opt(&mut out, c.lineage.as_deref());
    lp(&mut out, "task");
    u64le(&mut out, c.original_task.len() as u64);
    for t in &c.original_task {
        lp(&mut out, &t.text);
        lp(&mut out, &receipt_json(&t.receipt)?);
    }
    lp(&mut out, "established");
    u64le(&mut out, c.established_work.len() as u64);
    for e in &c.established_work {
        lp(&mut out, &e.text);
        lp(&mut out, &e.state);
        lp(&mut out, &receipt_json(&e.receipt)?);
    }
    lp(&mut out, "assertions");
    u64le(&mut out, c.agent_assertions.len() as u64);
    for a in &c.agent_assertions {
        lp(&mut out, &a.text);
        lp(&mut out, &a.state);
        opt(&mut out, a.advisory.as_deref());
        lp(&mut out, &receipt_json(&a.receipt)?);
    }
    lp(&mut out, "unknowns");
    u64le(&mut out, c.unknowns.len() as u64);
    for u in &c.unknowns {
        lp(&mut out, &u.text);
        lp(&mut out, &u.state);
        lp(&mut out, &receipt_json(&u.receipt)?);
    }
    lp(&mut out, "obligations");
    u64le(&mut out, c.open_obligations.len() as u64);
    for o in &c.open_obligations {
        lp(&mut out, &o.item_id);
        lp(&mut out, &o.kind);
        lp(&mut out, &o.origin);
        lp(&mut out, &o.artifact_key);
        lp(&mut out, &o.text);
        lp(&mut out, &receipt_json(&o.receipt)?);
    }
    lp(&mut out, "failures");
    u64le(&mut out, c.current_failures.len() as u64);
    for f in &c.current_failures {
        lp(&mut out, &f.text);
        lp(&mut out, &receipt_json(&f.receipt)?);
    }
    lp(&mut out, "incomplete_evidence");
    u64le(&mut out, c.incomplete_evidence.len() as u64);
    for m in &c.incomplete_evidence {
        lp(&mut out, &m.marker);
        lp(&mut out, &m.reason);
    }
    lp(&mut out, "continuation");
    u64le(&mut out, c.continuation_state.len() as u64);
    for t in &c.continuation_state {
        lp(&mut out, &t.text);
        lp(&mut out, &receipt_json(&t.receipt)?);
    }
    lp(&mut out, "decisions");
    u64le(&mut out, c.decisions_constraints.len() as u64);
    for d in &c.decisions_constraints {
        lp(&mut out, &d.text);
        lp(&mut out, &receipt_json(&d.receipt)?);
    }
    lp(&mut out, "recommendations");
    u64le(&mut out, c.recommendations.len() as u64);
    for r in &c.recommendations {
        lp(&mut out, &r.text);
        lp(&mut out, &r.label);
        lp(&mut out, &r.basis);
    }
    let mut empties = c.category_empties.clone();
    empties.sort();
    lp(&mut out, "empties");
    u64le(&mut out, empties.len() as u64);
    for e in &empties {
        lp(&mut out, &e.category);
        lp(&mut out, &e.kind);
        lp(&mut out, &e.reason);
    }
    lp(&mut out, "redaction");
    lp(&mut out, &serde_json::to_string(&c.redaction.treatment)?);
    lp(&mut out, &c.redaction.statement);
    let mut excl = c.redaction.structural_exclusions.clone();
    excl.sort();
    excl.dedup();
    u64le(&mut out, excl.len() as u64);
    for e in &excl {
        lp(&mut out, e);
    }
    lp(&mut out, "completeness");
    lp(&mut out, &c.completeness.state);
    u64le(&mut out, c.completeness.reasons.len() as u64);
    for r in &c.completeness.reasons {
        lp(&mut out, r);
    }
    lp(&mut out, "measurement");
    u64le(&mut out, c.measurement.token_estimate);
    u64le(&mut out, c.measurement.budget);
    u64le(&mut out, c.measurement.trimmed.len() as u64);
    for t in &c.measurement.trimmed {
        lp(&mut out, &t.category);
        u64le(&mut out, t.items_withheld);
        u64le(&mut out, t.tokens_withheld);
    }
    lp(&mut out, "complete");
    Ok(out)
}

/// The content-addressed capsule id: SHA-256 of [`canonical_bytes`],
/// lowercase hex.
pub fn capsule_id(c: &CapsuleV2) -> Result<String> {
    Ok(sha256_hex(&canonical_bytes(c)?))
}

/// Verify a capsule against its own identity: format, version, and the id
/// recomputed from the canonical preimage. Any changed hashed field — which
/// is every content field — fails here.
pub fn verify_capsule(c: &CapsuleV2) -> Result<()> {
    if c.format != CAPSULE_FORMAT {
        return Err(CoreError::Integrity(format!(
            "capsule format is {:?}, not {CAPSULE_FORMAT}",
            c.format
        )));
    }
    if c.capsule_version != CAPSULE_V2_VERSION {
        return Err(CoreError::Integrity(format!(
            "capsule version is {}, not {CAPSULE_V2_VERSION}",
            c.capsule_version
        )));
    }
    let recomputed = capsule_id(c)?;
    if recomputed != c.capsule_id {
        return Err(CoreError::Integrity(
            "capsule verification failed: a hashed field does not match the capsule id".to_string(),
        ));
    }
    Ok(())
}

/// The stable unresolved-item id (§15, §34): SHA-256 over the item's SOURCE
/// identity — `lp(TLOBL_V1) lp(origin) lp(kind) lp(artifact_key)` — never
/// over its wording. Rewording an obligation cannot move its id; a
/// different source cannot share one.
pub fn obligation_id(origin: &str, kind: &str, artifact_key: &str) -> String {
    let mut pre = Vec::with_capacity(64);
    lp(&mut pre, OBLIGATION_ID_FORMAT);
    lp(&mut pre, origin);
    lp(&mut pre, kind);
    lp(&mut pre, artifact_key);
    format!("obl1:{}", sha256_hex(&pre))
}

/* ── the one renderer ───────────────────────────────────────────────── */

fn terminal_note(t: &ReceiptTerminal) -> String {
    match t {
        ReceiptTerminal::Transcript { file, line } => format!("transcript {file}:{line}"),
        ReceiptTerminal::Filesystem { path, sha256 } => match sha256 {
            Some(sha) => format!("filesystem {path} · sha256 {}", head(sha)),
            None => format!("filesystem {path}"),
        },
        ReceiptTerminal::ProcessExecution { command, exit_code } => match exit_code {
            Some(code) => format!("captured execution `{command}` · exit {code}"),
            None => format!("captured execution `{command}`"),
        },
        ReceiptTerminal::SnapshotManifest { manifest_sha256 } => {
            format!("snapshot manifest {}", head(manifest_sha256))
        }
        ReceiptTerminal::ProviderResponse { provider, .. } => format!("provider {provider}"),
        ReceiptTerminal::HumanAdjudication {
            adjudicator,
            adjudicated_at,
        } => format!("adjudicated by {adjudicator} at {adjudicated_at}"),
    }
}

fn head(s: &str) -> String {
    s.chars().take(16).collect::<String>() + "…"
}

fn line_task(t: &TaskLine) -> String {
    format!("- {}\n  [{}]", t.text, terminal_note(&t.receipt.terminal))
}
fn line_established(e: &EstablishedFact) -> String {
    format!(
        "- {} · {}\n  [{}]",
        e.text,
        e.state,
        terminal_note(&e.receipt.terminal)
    )
}
fn line_assertion(a: &AgentAssertion) -> String {
    let advisory = a
        .advisory
        .as_deref()
        .map(|s| format!(" · {s}"))
        .unwrap_or_default();
    format!(
        "- {} · {}{advisory}\n  [{}]",
        a.text,
        a.state,
        terminal_note(&a.receipt.terminal)
    )
}
fn line_unknown(u: &UnknownItem) -> String {
    format!(
        "- {} · {}\n  [{}]",
        u.text,
        u.state,
        terminal_note(&u.receipt.terminal)
    )
}
fn line_obligation(o: &OpenObligation) -> String {
    format!(
        "- {} · {}\n  [{}]",
        o.item_id,
        o.text,
        terminal_note(&o.receipt.terminal)
    )
}
fn line_failure(f: &FailureLine) -> String {
    format!("- {}\n  [{}]", f.text, terminal_note(&f.receipt.terminal))
}
fn line_marker(m: &IncompleteMarker) -> String {
    format!("- {} — {}", m.marker, m.reason)
}
fn line_continuation(t: &ContinuationRef) -> String {
    format!("- {}\n  [{}]", t.text, terminal_note(&t.receipt.terminal))
}
fn line_decision(d: &DecisionLine) -> String {
    format!("- {}\n  [{}]", d.text, terminal_note(&d.receipt.terminal))
}
fn line_recommendation(r: &Recommendation) -> String {
    format!("- {} · {} — {}", r.label, r.text, r.basis)
}

fn empty_note(c: &CapsuleV2, category: &str, out: &mut String) {
    if let Some(e) = c.category_empties.iter().find(|e| e.category == category) {
        let kind = match e.kind.as_str() {
            "no_source_object" => "nothing in the record can answer this",
            "source_queried_and_empty" => "looked, and found none",
            "withheld" => "held back",
            other => other,
        };
        out.push_str(&format!("*({kind} — {})*\n", e.reason));
    }
    out.push('\n');
}

/// The markdown view — derived from the canonical document and from nothing
/// else. One selection, two views: this renderer holds no fact of its own.
pub fn render_markdown(c: &CapsuleV2) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Recovery Capsule v2 · {}\n\n", c.scope.id));
    out.push_str(&format!("Capsule id: `{}`\n", c.capsule_id));
    match (&c.repository.repo_id, c.repository.resolves) {
        (Some(id), true) => out.push_str(&format!("Repository: `{id}`\n")),
        _ => out
            .push_str("Repository: this scope does not resolve to a repository — never guessed\n"),
    }
    match &c.workspace.identity {
        Some(id) => out.push_str(&format!("Workspace: `{id}`\n")),
        None => out.push_str(&format!(
            "Workspace: not observed — {}\n",
            c.workspace.basis
        )),
    }
    match &c.snapshot {
        Some(s) => out.push_str(&format!(
            "Snapshot: {} · doc sha256 {} · {}{}\n",
            s.snapshot_id,
            head(&s.doc_sha256),
            if s.complete {
                "complete"
            } else {
                "INCOMPLETE — capture again before trusting it"
            },
            s.manifest_digest
                .as_deref()
                .map(|d| format!(" · byte manifest {}", head(d)))
                .unwrap_or_default(),
        )),
        None => {
            out.push_str("Snapshot: none was taken beside this capsule — stated, never implied\n")
        }
    }
    out.push_str(&format!(
        "Source: {} · session {}\nCutoff: chain position {} over {} events · sealed {}\n",
        c.source.agent,
        c.source.session,
        head(&c.cutoff.chain_position),
        c.cutoff.event_total,
        c.cutoff.created_at,
    ));
    match &c.lineage {
        Some(prior) => out.push_str(&format!("Lineage: supersedes capsule `{}`\n", head(prior))),
        None => out.push_str("Lineage: no prior capsule for this scope is on record\n"),
    }
    out.push_str(&format!("\n{}\n\n", c.cutoff.statement));

    out.push_str("## Original task\n\n");
    for t in &c.original_task {
        out.push_str(&line_task(t));
        out.push('\n');
    }
    empty_note(c, "original_task", &mut out);

    out.push_str("## Established facts — established completed work\n\n");
    for e in &c.established_work {
        out.push_str(&line_established(e));
        out.push('\n');
    }
    empty_note(c, "established_work", &mut out);

    out.push_str("## Agent assertions — not established\n\n");
    for a in &c.agent_assertions {
        out.push_str(&line_assertion(a));
        out.push('\n');
    }
    empty_note(c, "agent_assertions", &mut out);

    out.push_str("## Known unknowns\n\n");
    for u in &c.unknowns {
        out.push_str(&line_unknown(u));
        out.push('\n');
    }
    empty_note(c, "unknowns", &mut out);

    out.push_str("## Open obligations — acknowledge or reject each by id\n\n");
    for o in &c.open_obligations {
        out.push_str(&line_obligation(o));
        out.push('\n');
    }
    empty_note(c, "open_obligations", &mut out);

    out.push_str("## Current failures\n\n");
    for f in &c.current_failures {
        out.push_str(&line_failure(f));
        out.push('\n');
    }
    empty_note(c, "current_failures", &mut out);

    out.push_str("## Incomplete evidence markers\n\n");
    for m in &c.incomplete_evidence {
        out.push_str(&line_marker(m));
        out.push('\n');
    }
    empty_note(c, "incomplete_evidence", &mut out);

    out.push_str("## Dirty and untracked work needed for continuation\n\n");
    for t in &c.continuation_state {
        out.push_str(&line_continuation(t));
        out.push('\n');
    }
    empty_note(c, "continuation_state", &mut out);

    out.push_str("## Decisions and constraints\n\n");
    for d in &c.decisions_constraints {
        out.push_str(&line_decision(d));
        out.push('\n');
    }
    empty_note(c, "decisions_constraints", &mut out);

    out.push_str("## Recommendations — labelled, never facts\n\n");
    for r in &c.recommendations {
        out.push_str(&line_recommendation(r));
        out.push('\n');
    }
    empty_note(c, "recommendations", &mut out);

    out.push_str("## Redaction declaration\n\n");
    out.push_str(&format!(
        "{}\n{}\n",
        c.redaction.statement,
        c.redaction.treatment.statement()
    ));
    for e in &c.redaction.structural_exclusions {
        out.push_str(&format!("- {e}\n"));
    }
    out.push('\n');

    out.push_str("## Completeness declaration\n\n");
    out.push_str(&format!("State: {}\n", c.completeness.state));
    for r in &c.completeness.reasons {
        out.push_str(&format!("- {r}\n"));
    }
    out.push('\n');

    out.push_str("## Measurement\n\n");
    out.push_str(&format!(
        "Token estimate: {} · budget {}\n",
        c.measurement.token_estimate, c.measurement.budget
    ));
    for t in &c.measurement.trimmed {
        out.push_str(&format!(
            "- trimmed {}: {} item(s), {} token(s) withheld — disclosed, never silent\n",
            t.category, t.items_withheld, t.tokens_withheld
        ));
    }
    out.push('\n');

    out.push_str(
        "## About this capsule\n\nThis document is an offer of context. Receiving it \
         establishes nothing about a successor's state or actions; every acceptance \
         fact is its own separately recorded observation.\n",
    );
    out
}

/// ceil(chars/4) over the render.
///
/// The measurement is inherently self-referential: `token_estimate` is
/// inside the capsule id's preimage, so it must be stamped BEFORE the id
/// exists — yet the render prints the id. Measuring against the real
/// document is therefore impossible, and the two substitutions here make
/// the estimate exact anyway: the estimate field is zeroed (it would
/// otherwise count its own digits) and the id slot is filled with a
/// 64-character placeholder, because every real `capsule_id` is exactly 64
/// hex characters.
///
/// A209's first version left the id slot EMPTY, which understated every
/// live capsule by ~17 tokens (2,501 stamped against 2,517 rendered).
/// Corrected here; the register records the original claim and the
/// correction rather than editing the old number away.
fn measured_tokens(c: &CapsuleV2) -> u64 {
    let mut measurable = c.clone();
    measurable.measurement.token_estimate = 0;
    measurable.capsule_id = "0".repeat(64);
    (render_markdown(&measurable).chars().count() as u64).div_ceil(4)
}

/* ── compile ────────────────────────────────────────────────────────── */

/// What the caller knows that the rows do not. Everything optional abstains
/// honestly when absent — nothing here is guessed inside the compiler.
#[derive(Debug, Clone, Default)]
pub struct CompileContext<'a> {
    pub snapshot: Option<&'a WorkspaceSnapshot>,
    /// The `wsid1:` identity the caller observed, where it observed one.
    pub workspace_identity: Option<String>,
    /// Why the identity is present or absent — always stated.
    pub workspace_basis: Option<String>,
    /// The prior capsule id for this scope, where one exists.
    pub lineage: Option<String>,
    /// Token budget; `None` means [`DEFAULT_TOKEN_BUDGET`].
    pub budget: Option<u64>,
}

/// The production masker: [`crate::redact::mask`], which cannot fail.
/// The seam exists so the §15 quarantine path is testable with a masker
/// that CAN fail — and the failure type carries no content, so the raw
/// value has no route into any output.
pub type MaskFn<'m> = dyn FnMut(&str) -> Option<(String, BTreeSet<String>, u64)> + 'm;

/// Compile the §15 capsule for one project scope, masked and trimmed, with
/// its content-addressed id stamped. Deterministic: same rows, same
/// context, same `created_at` → byte-identical capsule.
pub fn compile(
    ledger: &Ledger,
    project_id: &str,
    ctx: &CompileContext<'_>,
    created_at: &str,
) -> Result<CapsuleV2> {
    let mut production = |s: &str| Some(crate::redact::mask(s));
    compile_with_masker(ledger, project_id, ctx, created_at, &mut production)
}

/// [`compile`] with the masking seam exposed. A `None` from the masker is a
/// masking failure for that element: the element is quarantined, the
/// capsule goes incomplete, and the raw value reaches nothing.
pub fn compile_with_masker(
    ledger: &Ledger,
    project_id: &str,
    ctx: &CompileContext<'_>,
    created_at: &str,
    mask_one: &mut MaskFn<'_>,
) -> Result<CapsuleV2> {
    let budget = ctx.budget.unwrap_or(DEFAULT_TOKEN_BUDGET);
    let cands = candidates::derive(ledger, project_id)?;
    let (chain_position, event_total) = ledger.compute_project_head(project_id)?;

    // ── source agent and session ─────────────────────────────────────
    use rusqlite::OptionalExtension;
    let agent: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT source FROM projects WHERE id = ?1",
            [project_id],
            |r| r.get(0),
        )
        .optional()?;
    let session: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT id FROM sessions WHERE project_id = ?1 ORDER BY started_at DESC, id DESC LIMIT 1",
            [project_id],
            |r| r.get(0),
        )
        .optional()?;

    // ── repository identity ──────────────────────────────────────────
    let repo_row: Option<(Option<String>, Option<String>)> = ledger
        .conn()
        .query_row(
            "SELECT repo_id, basis FROM workspace_identities WHERE project_id = ?1",
            [project_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let repository = match repo_row {
        Some((Some(repo_id), basis)) => RepositoryV2 {
            resolves: true,
            repo_id: Some(repo_id),
            basis,
        },
        Some((None, basis)) => RepositoryV2 {
            resolves: false,
            repo_id: None,
            basis,
        },
        None => RepositoryV2 {
            resolves: false,
            repo_id: None,
            basis: Some("no identity row".to_string()),
        },
    };

    let mut incomplete_evidence: Vec<IncompleteMarker> = Vec::new();
    // A209-C5/B7-7: the treatment folds the records of values actually
    // PLACED in this capsule, not of every candidate in the project. A
    // credential in a succeeded command that no category renders must not
    // produce a treatment claiming a mask the artifact does not contain.
    let mut upstream = crate::redact::MaskRecord::default();

    // ── original task ────────────────────────────────────────────────
    let mut original_task: Vec<TaskLine> = Vec::new();
    {
        let mut seen: Vec<(&str, u64, &candidates::OpeningAsk)> = Vec::new();
        for ask in &cands.opening_asks {
            match seen.iter_mut().find(|(text, _, _)| *text == ask.text) {
                Some((_, count, _)) => *count += 1,
                None => seen.push((&ask.text, 1, ask)),
            }
        }
        ceiling_marker(
            &mut incomplete_evidence,
            "original_task",
            seen.len(),
            CATEGORY_CEILING,
        );
        for (text, count, ask) in seen.into_iter().take(CATEGORY_CEILING) {
            let continues = if ask.continuation {
                " · continues an earlier run"
            } else {
                ""
            };
            let repeats = if count > 1 {
                format!(" · {count} sessions opened with this same text")
            } else {
                String::new()
            };
            upstream.merge(&ask.mask);
            original_task.push(TaskLine {
                text: format!(
                    "opening ask \u{201c}{}\u{201d}{continues}{repeats}",
                    text.trim()
                ),
                receipt: transcript_receipt(ledger, &ask.event_id, Some(&ask.session_id))?,
            });
        }
    }

    // ── established facts (category 1) ───────────────────────────────
    // Supported claims: RECORD_SUPPORTED per the NORTHSTAR-v4 bridge —
    // captured evidence supports the bounded claim. The §12-identity join
    // to evidence bindings is a named Stage-7 seam (module doc, item 4).
    let mut established_work = Vec::new();
    let (supported_rows, supported_total) = claim_rows(ledger, project_id, "supported")?;
    ceiling_marker(
        &mut incomplete_evidence,
        "established_work",
        supported_total,
        CATEGORY_CEILING,
    );
    for (label, anchor, sess, currency) in supported_rows {
        upstream.merge(label.record());
        established_work.push(EstablishedFact {
            text: label
                .map_rendered(|t| format!("\u{201c}{}\u{201d} · {currency}", t.trim()))
                .into_text(),
            state: "RECORD_SUPPORTED".to_string(),
            receipt: transcript_receipt(ledger, &anchor, Some(&sess))?,
        });
    }

    // ── agent assertions (category 2) ────────────────────────────────
    // The agent's own claims, state ASSERTED (§9: stated, not
    // independently established). The no-proof note is §5's advisory
    // sentence, verbatim — never a verdict, never a block.
    let mut agent_assertions = Vec::new();
    let (asserted_rows, asserted_total) = claim_rows(ledger, project_id, "unsupported")?;
    ceiling_marker(
        &mut incomplete_evidence,
        "agent_assertions",
        asserted_total,
        CATEGORY_CEILING,
    );
    for (label, anchor, sess, _currency) in asserted_rows {
        upstream.merge(label.record());
        agent_assertions.push(AgentAssertion {
            text: label
                .map_rendered(|t| format!("\u{201c}{}\u{201d}", t.trim()))
                .into_text(),
            state: "ASSERTED".to_string(),
            advisory: Some(ADVISORY_NO_PROOF.to_string()),
            receipt: transcript_receipt(ledger, &anchor, Some(&sess))?,
        });
    }

    // ── known unknowns (category 3) ──────────────────────────────────
    let mut unknowns = Vec::new();
    let (unknown_rows, unknown_total) = claim_rows(ledger, project_id, "unknown")?;
    ceiling_marker(
        &mut incomplete_evidence,
        "unknowns",
        unknown_total,
        CATEGORY_CEILING,
    );
    for (label, anchor, sess, _currency) in unknown_rows {
        upstream.merge(label.record());
        unknowns.push(UnknownItem {
            text: label
                .map_rendered(|t| format!("\u{201c}{}\u{201d}", t.trim()))
                .into_text(),
            state: "UNKNOWN".to_string(),
            receipt: transcript_receipt(ledger, &anchor, Some(&sess))?,
        });
    }
    if let Some(snap) = ctx.snapshot {
        unknowns.push(UnknownItem {
            text: format!("workspace snapshot: {}", snap.processes.gaps),
            state: "UNKNOWN".to_string(),
            receipt: snapshot_receipt_for(snap),
        });
    }

    // ── open obligations (category 4) ────────────────────────────────
    let mut open_obligations = Vec::new();
    ceiling_marker(
        &mut incomplete_evidence,
        "open_obligations",
        cands.tails.len(),
        CATEGORY_CEILING,
    );
    for tail in cands.tails.iter().take(CATEGORY_CEILING) {
        let receipt = last_edit_receipt(ledger, &tail.session_id)?;
        // The id is computed from the artifact key AS IT WILL BE PERSISTED
        // — masked first, never after. Stage 7's acknowledgment protocol
        // recomputes item ids from the persisted (origin, kind,
        // artifact_key) triple, so an id derived from a pre-masking value
        // would stop being reproducible from the capsule the successor
        // actually holds. Latent while this kind's key is empty; wired
        // correctly now rather than left as a trap for the stage that
        // first populates it.
        let artifact_key = match mask_one("") {
            Some((masked, _, _)) => masked,
            None => QUARANTINE_TEXT.to_string(),
        };
        open_obligations.push(OpenObligation {
            item_id: obligation_id(&tail.session_id, "uncommitted_edit_run", &artifact_key),
            kind: "uncommitted_edit_run".to_string(),
            origin: tail.session_id.clone(),
            artifact_key,
            text: format!(
                "{} edits after the last commit — unresolved · {}",
                tail.edits_after_last_boundary,
                tail.files.join(", ")
            ),
            receipt,
        });
    }

    // ── current failures ─────────────────────────────────────────────
    let ops = crate::operations::derive(ledger, project_id)?;
    let failed: Vec<_> = ops.iter().filter(|o| o.outcome == "failed").collect();
    ceiling_marker(
        &mut incomplete_evidence,
        "current_failures",
        failed.len(),
        FAILURE_CEILING,
    );
    let mut current_failures = Vec::new();
    for op in failed.into_iter().rev().take(FAILURE_CEILING) {
        upstream.merge(&op.mask);
        current_failures.push(FailureLine {
            text: format!(
                "`{}` · failed · replay class {}",
                op.command_preview,
                op.replay_class.as_str()
            ),
            receipt: transcript_receipt(ledger, &op.event_id, Some(&op.session_id))?,
        });
    }

    // ── incomplete evidence: overflowed tool results ─────────────────
    let overflow_count = count_overflow_refs(ledger, project_id)?;
    if overflow_count > 0 {
        incomplete_evidence.push(IncompleteMarker {
            marker: "overflowed_tool_results".to_string(),
            reason: format!(
                "{overflow_count} tool result(s) overflow to side files not ingested into \
                 the ledger; searches over this record are INCOMPLETE_SEARCH with respect \
                 to them"
            ),
        });
    }

    // ── continuation state — snapshot-cited, never inlined bytes ─────
    let mut continuation_state = Vec::new();
    if let Some(snap) = ctx.snapshot {
        let receipt = snapshot_receipt_for(snap);
        let t = &snap.tree;
        for (paths, class) in [
            (&t.staged, "staged"),
            (&t.unstaged, "changed"),
            (&t.untracked, "untracked"),
        ] {
            if !paths.is_empty() {
                continuation_state.push(ContinuationRef {
                    text: format!(
                        "{} {class} file(s): {}",
                        paths.len(),
                        name_a_few(paths.iter().map(|f| f.path.as_str()))
                    ),
                    receipt: receipt.clone(),
                });
            }
        }
        if !t.deleted.is_empty() {
            continuation_state.push(ContinuationRef {
                text: format!(
                    "{} deleted file(s): {}",
                    t.deleted.len(),
                    name_a_few(t.deleted.iter().map(String::as_str))
                ),
                receipt: receipt.clone(),
            });
        }
        if !snap.complete {
            incomplete_evidence.push(IncompleteMarker {
                marker: "snapshot_incomplete".to_string(),
                reason: "the cited workspace snapshot is incomplete — the tree moved during \
                         capture; capture again before trusting it"
                    .to_string(),
            });
        }
    }

    // ── decisions and constraints ────────────────────────────────────
    // No decision/constraint object exists in the model (measured at A173
    // and still true) — the category renders empty with that reason rather
    // than the schema inventing one.
    let decisions_constraints: Vec<DecisionLine> = Vec::new();

    // ── recommendations (category 5) ─────────────────────────────────
    let mut recommendations = Vec::new();
    if !open_obligations.is_empty() {
        recommendations.push(Recommendation {
            text: "acknowledge or reject each open obligation by its item id before \
                   continuing the work"
                .to_string(),
            label: "recommendation".to_string(),
            basis: "compiler rule: open obligations are present".to_string(),
        });
    }
    if ctx.snapshot.is_some() {
        recommendations.push(Recommendation {
            text: "compare the live tree against the cited snapshot before mutating \
                   anything (thruline snapshot --verify, thruline restore)"
                .to_string(),
            label: "recommendation".to_string(),
            basis: "compiler rule: a workspace snapshot is cited".to_string(),
        });
    }

    // ── assemble, mask, trim, declare, stamp ─────────────────────────
    let workspace = WorkspaceV2 {
        identity: ctx.workspace_identity.clone(),
        basis: ctx
            .workspace_basis
            .clone()
            .unwrap_or_else(|| match ctx.workspace_identity {
                Some(_) => "supplied by the caller that observed it".to_string(),
                None => "no workspace root was supplied to this compile; the identity is \
                         abstained, never guessed"
                    .to_string(),
            }),
    };
    let snapshot = ctx.snapshot.map(|s| SnapshotCitationV2 {
        snapshot_id: s.id.clone(),
        doc_sha256: snapshot_doc_digest(s),
        manifest_digest: s.protected.as_ref().map(|p| p.manifest_digest.clone()),
        complete: s.complete,
        head: s.git.as_ref().and_then(|g| g.head.clone()),
        branch: s.git.as_ref().and_then(|g| g.branch.clone()),
        taken_at: s.taken_at.clone(),
    });

    let mut capsule = CapsuleV2 {
        capsule_version: CAPSULE_V2_VERSION,
        format: CAPSULE_FORMAT.to_string(),
        capsule_id: String::new(),
        generator: GeneratorV2 {
            name: "thruline".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        scope: ScopeV2 {
            kind: "project".to_string(),
            id: project_id.to_string(),
        },
        source: SourceV2 {
            agent: agent.unwrap_or_else(|| "unknown".to_string()),
            session: session.unwrap_or_else(|| "none-recorded".to_string()),
        },
        repository,
        workspace,
        snapshot,
        cutoff: CutoffV2 {
            chain_position,
            event_total,
            created_at: created_at.to_string(),
            statement: "Sealed at creation: later source-session output cannot change this \
                        capsule. New state requires a new capsule, linked by lineage."
                .to_string(),
        },
        lineage: ctx.lineage.clone(),
        original_task,
        established_work,
        agent_assertions,
        unknowns,
        open_obligations,
        current_failures,
        incomplete_evidence,
        continuation_state,
        decisions_constraints,
        recommendations,
        category_empties: Vec::new(),
        redaction: RedactionV2 {
            treatment: Treatment::none(),
            // Precise about its own coverage (A209 correction). The first
            // version said "every rendered string", which was not true of
            // the envelope: masking covers the ten category arrays, their
            // receipts' string payloads, and the three externally-sourced
            // envelope fields. What it does not cover is named here rather
            // than implied away — an overstated coverage claim is the same
            // false-calm shape as an overstated absence claim.
            statement: "Detect-and-mask ran BEFORE this document was persisted, over every \
                        category line and its receipt, and over the externally-sourced \
                        envelope fields (repository basis, workspace basis, snapshot \
                        branch). Identifiers this product publishes — scope, session, \
                        repository and workspace identities, snapshot ids, digests, item \
                        ids — and this compiler's own constant strings are not scanned. \
                        This records the treatment received, never a property held."
                .to_string(),
            structural_exclusions: vec![
                "environment variable values are never read; snapshots record names only"
                    .to_string(),
                "file contents are never copied; artifacts are cited by digest, never inlined"
                    .to_string(),
            ],
        },
        completeness: CompletenessV2 {
            state: "complete".to_string(),
            reasons: Vec::new(),
        },
        measurement: MeasurementV2 {
            token_estimate: 0,
            budget,
            trimmed: Vec::new(),
        },
    };

    mask_capsule(&mut capsule, mask_one, &upstream);
    trim_to_budget(&mut capsule);
    declare_empties(&mut capsule);
    if capsule
        .incomplete_evidence
        .iter()
        .any(|m| m.marker == "snapshot_incomplete")
    {
        note_incomplete(&mut capsule, "the cited workspace snapshot is incomplete");
    }
    capsule.measurement.token_estimate = measured_tokens(&capsule);
    capsule.capsule_id = capsule_id(&capsule)?;
    Ok(capsule)
}

/// Disclose a source ceiling: the category holds more than the compile
/// materialized, and the elision is named rather than silent.
fn ceiling_marker(
    markers: &mut Vec<IncompleteMarker>,
    category: &str,
    total: usize,
    ceiling: usize,
) {
    if total > ceiling {
        markers.push(IncompleteMarker {
            marker: "category_ceiling".to_string(),
            reason: format!(
                "category {category} holds {total} item(s); only the newest {ceiling} were \
                 read into this compile"
            ),
        });
    }
}

fn name_a_few<'a>(paths: impl Iterator<Item = &'a str>) -> String {
    let all: Vec<&str> = paths.collect();
    if all.len() <= 8 {
        all.join(", ")
    } else {
        format!("{} … and {} more", all[..8].join(", "), all.len() - 8)
    }
}

/// The same content digest the v1 capsule cites for a C2 snapshot doc.
fn snapshot_doc_digest(s: &WorkspaceSnapshot) -> String {
    sha256_hex(
        serde_json::to_string_pretty(s)
            .unwrap_or_default()
            .as_bytes(),
    )
}

fn snapshot_receipt_for(s: &WorkspaceSnapshot) -> SourceReceipt {
    let manifest_sha256 = s
        .protected
        .as_ref()
        .map(|p| p.manifest_digest.clone())
        .unwrap_or_else(|| snapshot_doc_digest(s));
    let mut receipt = SourceReceipt::new(ReceiptTerminal::SnapshotManifest { manifest_sha256 });
    receipt.derived_refs.push(DerivedRef {
        table: "workspace_snapshots".to_string(),
        id: s.id.clone(),
    });
    receipt
}

/// A transcript-terminated receipt for one recorded event: the §11 first
/// arm, raw file and line, with the event row riding along as context.
fn transcript_receipt(
    ledger: &Ledger,
    event_id: &str,
    session_id: Option<&str>,
) -> Result<SourceReceipt> {
    use rusqlite::OptionalExtension;
    let row: Option<(String, i64)> = ledger
        .conn()
        .query_row(
            "SELECT raw_file, raw_line FROM events WHERE id = ?1",
            [event_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (file, line) = row.ok_or_else(|| {
        CoreError::Integrity(format!(
            "event {event_id} has no stored raw receipt — a capsule line cannot cite it"
        ))
    })?;
    let mut receipt = SourceReceipt::new(ReceiptTerminal::Transcript {
        file,
        line: line.max(0) as u64,
    });
    receipt.derived_refs.push(DerivedRef {
        table: "events".to_string(),
        id: event_id.to_string(),
    });
    if let Some(sess) = session_id {
        receipt.derived_refs.push(DerivedRef {
            table: "sessions".to_string(),
            id: sess.to_string(),
        });
    }
    Ok(receipt)
}

/// The receipt for an unresolved edit run: the transcript line of the LAST
/// edit in that session — a real raw position, not a session-id stand-in.
fn last_edit_receipt(ledger: &Ledger, session_id: &str) -> Result<SourceReceipt> {
    use rusqlite::OptionalExtension;
    let row: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT id FROM events WHERE session_id = ?1 AND kind = 'tool_use'
               AND tool_name IN ('Edit','Write','MultiEdit','NotebookEdit')
             ORDER BY seq DESC LIMIT 1",
            [session_id],
            |r| r.get(0),
        )
        .optional()?;
    match row {
        Some(event_id) => transcript_receipt(ledger, &event_id, Some(session_id)),
        None => Err(CoreError::Integrity(format!(
            "session {session_id} has an unresolved edit run but no edit event — the \
             derivation and the rows disagree"
        ))),
    }
}

/// Claims at one state, newest first: (label, anchor_event_id, session,
/// currency) up to the category ceiling, plus the true total so the caller
/// can disclose the elision.
/// A claim row's label arrives from `nodes.label`, which `derive` masked
/// at write time — but the record did not survive the column, so the label
/// is [`crate::redact::Masked::unknown`]: a value this capsule cannot
/// account for. That renders `Treatment::Unknown` and closes the consent
/// gate, which is the honest outcome and the one A209-C6/B8-2 measured
/// missing (the capsule rendered `[masked: aws]` under "nothing matched").
type ClaimRow = (crate::redact::Masked, String, String, String);
/// Wrap a stored label. Already-masked content whose record was lost is
/// `unknown`; content with no marker went through the same masker and
/// matched nothing, so it is masked again (a no-op) to obtain a real
/// record rather than an assumed one.
fn label_from_column(stored: String) -> crate::redact::Masked {
    if stored.contains("[masked: ") {
        crate::redact::Masked::unknown(stored)
    } else {
        crate::redact::Masked::mask(&stored)
    }
}

fn claim_rows(ledger: &Ledger, project_id: &str, state: &str) -> Result<(Vec<ClaimRow>, usize)> {
    let total: i64 = ledger.conn().query_row(
        "SELECT COUNT(*) FROM nodes WHERE project_id = ?1 AND sort_key = ?2",
        rusqlite::params![project_id, state],
        |r| r.get(0),
    )?;
    let mut stmt = ledger.conn().prepare(
        "SELECT label, anchor_event_id, session_id, currency FROM nodes
          WHERE project_id = ?1 AND sort_key = ?2
          ORDER BY anchor_event_id DESC LIMIT ?3",
    )?;
    let rows = stmt.query_map(
        rusqlite::params![project_id, state, CATEGORY_CEILING as i64],
        |r| {
            Ok((
                label_from_column(r.get::<_, String>(0)?),
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        },
    )?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok((out, total.max(0) as usize))
}

/// Count overflow references in this project's tool results: SQL prefilter
/// on the marker, then the real parser confirms each candidate.
fn count_overflow_refs(ledger: &Ledger, project_id: &str) -> Result<u64> {
    let mut stmt = ledger.conn().prepare(
        "SELECT tool_result FROM events
          WHERE project_id = ?1 AND kind = 'tool_result'
            AND tool_result LIKE '%persisted-output%'",
    )?;
    let rows = stmt.query_map([project_id], |r| r.get::<_, Option<String>>(0))?;
    let mut n = 0u64;
    for row in rows {
        if let Some(envelope) = row? {
            if crate::overflow::in_tool_result_envelope(&envelope).is_some() {
                n += 1;
            }
        }
    }
    Ok(n)
}

/* ── masking, before the document exists as bytes ───────────────────── */

fn note_incomplete(c: &mut CapsuleV2, reason: &str) {
    c.completeness.state = "incomplete".to_string();
    let reason = reason.to_string();
    if !c.completeness.reasons.contains(&reason) {
        c.completeness.reasons.push(reason);
    }
}

/// Mask every rendered string channel in place: category item texts, the
/// advisory notes, marker reasons, and the receipt terminals' own string
/// payloads (a transcript path or recorded command travels through the
/// same detector as any other string). A masking failure quarantines the
/// element and marks the capsule incomplete; the failure path carries no
/// content, so the raw value cannot leak through it.
struct MaskState {
    fired: BTreeSet<String>,
    masked: u64,
    quarantined: Vec<String>,
}

fn mask_text(state: &mut MaskState, mask_one: &mut MaskFn<'_>, category: &str, text: &mut String) {
    match mask_one(text) {
        Some((new, fired, n)) => {
            if n > 0 {
                *text = new;
                state.fired.extend(fired);
                state.masked += n;
            }
        }
        None => {
            *text = QUARANTINE_TEXT.to_string();
            state.quarantined.push(category.to_string());
        }
    }
}

fn mask_receipt(
    state: &mut MaskState,
    mask_one: &mut MaskFn<'_>,
    category: &str,
    receipt: &mut SourceReceipt,
) {
    match &mut receipt.terminal {
        ReceiptTerminal::Transcript { file, .. } => mask_text(state, mask_one, category, file),
        ReceiptTerminal::Filesystem { path, .. } => mask_text(state, mask_one, category, path),
        ReceiptTerminal::ProcessExecution { command, .. } => {
            mask_text(state, mask_one, category, command)
        }
        ReceiptTerminal::SnapshotManifest { .. }
        | ReceiptTerminal::ProviderResponse { .. }
        | ReceiptTerminal::HumanAdjudication { .. } => {}
    }
}

fn mask_capsule(
    c: &mut CapsuleV2,
    mask_one: &mut MaskFn<'_>,
    upstream: &crate::redact::MaskRecord,
) {
    let mut st = MaskState {
        fired: BTreeSet::new(),
        masked: 0,
        quarantined: Vec::new(),
    };

    for t in &mut c.original_task {
        mask_text(&mut st, mask_one, "original_task", &mut t.text);
        mask_receipt(&mut st, mask_one, "original_task", &mut t.receipt);
    }
    for e in &mut c.established_work {
        mask_text(&mut st, mask_one, "established_work", &mut e.text);
        mask_receipt(&mut st, mask_one, "established_work", &mut e.receipt);
    }
    for a in &mut c.agent_assertions {
        mask_text(&mut st, mask_one, "agent_assertions", &mut a.text);
        if let Some(adv) = &mut a.advisory {
            mask_text(&mut st, mask_one, "agent_assertions", adv);
        }
        mask_receipt(&mut st, mask_one, "agent_assertions", &mut a.receipt);
    }
    for u in &mut c.unknowns {
        mask_text(&mut st, mask_one, "unknowns", &mut u.text);
        mask_receipt(&mut st, mask_one, "unknowns", &mut u.receipt);
    }
    for o in &mut c.open_obligations {
        mask_text(&mut st, mask_one, "open_obligations", &mut o.text);
        mask_text(&mut st, mask_one, "open_obligations", &mut o.artifact_key);
        mask_receipt(&mut st, mask_one, "open_obligations", &mut o.receipt);
    }
    for f in &mut c.current_failures {
        mask_text(&mut st, mask_one, "current_failures", &mut f.text);
        mask_receipt(&mut st, mask_one, "current_failures", &mut f.receipt);
    }
    for m in &mut c.incomplete_evidence {
        mask_text(&mut st, mask_one, "incomplete_evidence", &mut m.reason);
    }
    for t in &mut c.continuation_state {
        mask_text(&mut st, mask_one, "continuation_state", &mut t.text);
        mask_receipt(&mut st, mask_one, "continuation_state", &mut t.receipt);
    }
    for d in &mut c.decisions_constraints {
        mask_text(&mut st, mask_one, "decisions_constraints", &mut d.text);
        mask_receipt(&mut st, mask_one, "decisions_constraints", &mut d.receipt);
    }
    for r in &mut c.recommendations {
        mask_text(&mut st, mask_one, "recommendations", &mut r.text);
        mask_text(&mut st, mask_one, "recommendations", &mut r.basis);
    }

    // The externally-sourced envelope fields (A209 correction). These
    // three carry text this machine did not author — a git branch name and
    // the two identity bases — so they are masked like any other rendered
    // string.
    //
    // What is NOT masked, stated correctly (A209-C round 3 — the earlier
    // comment claimed everything else was "a compiler constant, an id this
    // product publishes, or a digest", which is FALSE of `scope.id`):
    // `scope.id` is a project id, and `ledger.rs`'s `project_id_for`
    // derives it from the workspace directory path, so on this machine 50
    // of 96 project ids are munged absolute paths. It renders as the
    // markdown H1 and sits in the id preimage. It is left unmasked
    // deliberately: it is the capsule's scope key, masking it would break
    // the identity a successor matches on, and `redaction.statement` names
    // scope as unscanned rather than implying coverage. Live reachability
    // measured 0 of 96 — no project id on this machine carries a
    // credential shape — and the decision to mask or hash it is registered
    // as open rather than taken silently here.
    if let Some(basis) = &mut c.repository.basis {
        mask_text(&mut st, mask_one, "repository_basis", basis);
    }
    mask_text(&mut st, mask_one, "workspace_basis", &mut c.workspace.basis);
    if let Some(snapshot) = &mut c.snapshot {
        if let Some(branch) = &mut snapshot.branch {
            mask_text(&mut st, mask_one, "snapshot_branch", branch);
        }
    }

    let MaskState {
        mut fired,
        mut masked,
        mut quarantined,
    } = st;

    // Masks applied UPSTREAM arrive as a VALUE (A209-C4), merged by the
    // caller into `upstream` before this pass. Nothing is inferred from
    // document content: a `[masked: …]` literal in prose is prose.
    masked += upstream.masked;
    fired.extend(upstream.families.iter().cloned());

    c.redaction.treatment = if !upstream.complete {
        // A209-C6/B8-6: an incomplete record renders UNKNOWN, which closes
        // the consent gate. This is the production path Treatment::Unknown
        // lacked through two rounds of claiming otherwise.
        Treatment::unknown(
            "content in this capsule was masked upstream and the record did not survive",
        )
    } else if masked == 0 {
        Treatment::none()
    } else {
        let patterns: Vec<String> = fired.iter().cloned().collect();
        Treatment::Ran {
            detector: crate::redact::DETECTOR_VERSION.to_string(),
            masked,
            statement: format!(
                "detector {} · {masked} masked · patterns: {} · best-effort, not a \
                 guarantee: a secret in a shape the detector does not know is still here",
                crate::redact::DETECTOR_VERSION,
                patterns.join(", ")
            ),
            patterns,
        }
    };

    if !quarantined.is_empty() {
        quarantined.sort();
        quarantined.dedup();
        for category in &quarantined {
            c.incomplete_evidence.push(IncompleteMarker {
                marker: "masking_failure".to_string(),
                reason: format!(
                    "an element in category {category} could not be masked and was withheld \
                     — the raw value was never emitted"
                ),
            });
        }
        note_incomplete(
            c,
            "masking failed for at least one element; each was withheld, never emitted",
        );
    }
}

/* ── the disclosed trim ─────────────────────────────────────────────── */

fn category_len(c: &CapsuleV2, category: &str) -> usize {
    match category {
        "agent_assertions" => c.agent_assertions.len(),
        "established_work" => c.established_work.len(),
        "unknowns" => c.unknowns.len(),
        "original_task" => c.original_task.len(),
        "decisions_constraints" => c.decisions_constraints.len(),
        "recommendations" => c.recommendations.len(),
        _ => 0,
    }
}

fn pop_category(c: &mut CapsuleV2, category: &str) -> Option<u64> {
    let tokens = |s: String| (s.chars().count() as u64).div_ceil(4);
    match category {
        "agent_assertions" => c.agent_assertions.pop().map(|i| tokens(line_assertion(&i))),
        "established_work" => c
            .established_work
            .pop()
            .map(|i| tokens(line_established(&i))),
        "unknowns" => c.unknowns.pop().map(|i| tokens(line_unknown(&i))),
        "original_task" => c.original_task.pop().map(|i| tokens(line_task(&i))),
        "decisions_constraints" => c
            .decisions_constraints
            .pop()
            .map(|i| tokens(line_decision(&i))),
        "recommendations" => c
            .recommendations
            .pop()
            .map(|i| tokens(line_recommendation(&i))),
        _ => None,
    }
}

/// Trim to the token budget, walking [`TRIM_ORDER`] and disclosing every
/// withheld item. [`NEVER_TRIMMED`] categories are structurally unreachable
/// here — [`pop_category`] has no arm for them. Going over budget with
/// nothing left to trim is a disclosed state, not an error: §15 forbids
/// dropping a correctness-critical fact to meet the target.
fn trim_to_budget(c: &mut CapsuleV2) {
    let budget = c.measurement.budget;
    for category in TRIM_ORDER {
        let mut withheld_items = 0u64;
        let mut withheld_tokens = 0u64;
        // Always keep the first item of a category: a trim that erases a
        // whole category silently changes its meaning from "trimmed" to
        // "absent". The empty declaration machinery owns real absence.
        while measured_tokens(c) > budget && category_len(c, category) > 1 {
            match pop_category(c, category) {
                Some(tokens) => {
                    withheld_items += 1;
                    withheld_tokens += tokens;
                }
                None => break,
            }
        }
        if withheld_items > 0 {
            c.measurement.trimmed.push(TrimDisclosure {
                category: category.to_string(),
                items_withheld: withheld_items,
                tokens_withheld: withheld_tokens,
            });
        }
        if measured_tokens(c) <= budget {
            break;
        }
    }
}

fn declare_empties(c: &mut CapsuleV2) {
    let mut empties = Vec::new();
    let mut declare = |empty: bool, category: &str, kind: &str, reason: &str| {
        if empty {
            empties.push(EmptyDecl {
                category: category.to_string(),
                kind: kind.to_string(),
                reason: reason.to_string(),
            });
        }
    };
    let queried = "source_queried_and_empty";
    declare(
        c.original_task.is_empty(),
        "original_task",
        queried,
        "no session in this scope opens with a recorded request",
    );
    declare(
        c.established_work.is_empty(),
        "established_work",
        queried,
        "no claim in this scope is backed by evidence in the record",
    );
    declare(
        c.agent_assertions.is_empty(),
        "agent_assertions",
        queried,
        "no claim in this scope lacks matching evidence",
    );
    declare(
        c.unknowns.is_empty(),
        "unknowns",
        queried,
        "nothing in this scope was recorded but unreadable",
    );
    declare(
        c.open_obligations.is_empty(),
        "open_obligations",
        queried,
        "every recorded edit run in this scope ends at a commit boundary",
    );
    declare(
        c.current_failures.is_empty(),
        "current_failures",
        queried,
        "no recorded side-effect-shaped command in this scope failed",
    );
    declare(
        c.incomplete_evidence.is_empty(),
        "incomplete_evidence",
        queried,
        "no overflow references, ceilings, or quarantined elements",
    );
    declare(
        c.continuation_state.is_empty(),
        "continuation_state",
        if c.snapshot.is_some() {
            queried
        } else {
            "no_source_object"
        },
        if c.snapshot.is_some() {
            "the cited snapshot records a clean tree"
        } else {
            "no snapshot was taken beside this capsule, so no dirty state is cited"
        },
    );
    declare(
        c.decisions_constraints.is_empty(),
        "decisions_constraints",
        "no_source_object",
        "no decision or constraint object exists in the model; nothing is invented here",
    );
    declare(
        c.recommendations.is_empty(),
        "recommendations",
        queried,
        "no compiler rule produced a recommendation for this scope",
    );
    c.category_empties = empties;
}

/* ── sealing ────────────────────────────────────────────────────────── */

/// A sealed capsule: the canonical bytes as persisted, and their SHA-256 —
/// the CAS key the protected store filed them under.
#[derive(Debug, Clone)]
pub struct SealedCapsule {
    pub capsule: CapsuleV2,
    pub bytes: Vec<u8>,
    pub digest: String,
}

/// Seal a compiled capsule: verify its id, serialize the canonical JSON,
/// and file the bytes content-addressed in the protected store. Sealing is
/// digest-only this stage — the M5 collector-key signing machinery is bound
/// to the assured-capture lifecycle and is deliberately not contorted onto
/// a standalone document; signatures are a later hardening, and the
/// hash-chained CAPSULE_CREATED observation pins the digest either way.
pub fn seal(store: &ProtectedStore, capsule: &CapsuleV2) -> Result<SealedCapsule> {
    verify_capsule(capsule)?;
    let bytes = serde_json::to_vec(capsule)?;
    let (blob, _newly_written) = store.put_bytes(&bytes)?;
    Ok(SealedCapsule {
        capsule: capsule.clone(),
        bytes,
        digest: blob.sha256,
    })
}

/// Parse sealed bytes and verify the capsule id recomputes. The byte-level
/// digest check belongs to the caller that knows which digest was promised
/// (the store's `get` already refuses content that fails its own name).
pub fn open_sealed_bytes(bytes: &[u8]) -> Result<CapsuleV2> {
    let capsule: CapsuleV2 = serde_json::from_slice(bytes)?;
    verify_capsule(&capsule)?;
    Ok(capsule)
}

/// Load a sealed capsule from the store by its byte digest, verifying both
/// the bytes (the store re-hashes) and the capsule id (recomputed).
pub fn open_from_store(store: &ProtectedStore, digest: &str) -> Result<CapsuleV2> {
    open_sealed_bytes(&store.get(digest)?)
}

/// The most recent sealed capsule id for a repository identity, read from
/// the observation chain — the lineage anchor for the next capsule.
pub fn latest_capsule_id(ledger: &Ledger, repository_identity: &str) -> Result<Option<String>> {
    let records = crate::observation::of_type(ledger, ObservationType::CapsuleCreated)?;
    for record in records.iter().rev() {
        if record.repository_identity != repository_identity {
            continue;
        }
        let receipt = record.receipt()?;
        if let Some(id) = receipt.extra.get("capsule_id").and_then(|v| v.as_str()) {
            return Ok(Some(id.to_string()));
        }
    }
    Ok(None)
}

/// The lineage anchor for a project scope: the most recent sealed capsule
/// id recorded against the project's resolved repository identity. `None`
/// when the scope resolves to no repository or nothing is on record —
/// abstention, never a guess.
pub fn lineage_for_project(ledger: &Ledger, project_id: &str) -> Result<Option<String>> {
    use rusqlite::OptionalExtension;
    let repo_id: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT repo_id FROM workspace_identities WHERE project_id = ?1",
            [project_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    match repo_id {
        Some(rid) => latest_capsule_id(ledger, &rid),
        None => Ok(None),
    }
}

/* ── the §11 observation writers ────────────────────────────────────── */

/// Recording abstains — loudly, with the reason — when the capsule's scope
/// resolves to no repository, because the envelope's `repository_identity`
/// is NOT NULL and a placeholder would be a lie the chain preserves
/// forever. The `record_snapshot_created` shape, verbatim.
#[derive(Debug)]
pub enum CapsuleRecordOutcome {
    Recorded(Box<AppendOutcome>),
    NoRepositoryIdentity { reason: String },
}

fn capsule_receipt(sealed: &SealedCapsule, artifact_path: &str) -> SourceReceipt {
    let mut receipt = SourceReceipt::new(ReceiptTerminal::Filesystem {
        path: artifact_path.to_string(),
        sha256: Some(sealed.digest.clone()),
    });
    if let Some(snap) = &sealed.capsule.snapshot {
        receipt.derived_refs.push(DerivedRef {
            table: "workspace_snapshots".to_string(),
            id: snap.snapshot_id.clone(),
        });
    }
    receipt.extra.insert(
        "capsule_id".to_string(),
        sealed.capsule.capsule_id.clone().into(),
    );
    receipt
        .extra
        .insert("capsule_digest".to_string(), sealed.digest.clone().into());
    receipt
        .extra
        .insert("format".to_string(), CAPSULE_FORMAT.into());
    if let Some(lineage) = &sealed.capsule.lineage {
        receipt
            .extra
            .insert("lineage".to_string(), lineage.clone().into());
    }
    receipt
}

fn record_capsule_observation(
    ledger: &Ledger,
    sealed: &SealedCapsule,
    observation_type: ObservationType,
    mut receipt: SourceReceipt,
    observed_at: &str,
    source_session: &str,
) -> Result<CapsuleRecordOutcome> {
    let Some(repository_identity) = sealed.capsule.repository.repo_id.as_deref() else {
        return Ok(CapsuleRecordOutcome::NoRepositoryIdentity {
            reason: "this scope resolves to no repository — the capsule stands, and the \
                     observation is not recorded rather than recorded against a guess"
                .to_string(),
        });
    };
    receipt.extra.insert(
        "scope_project".to_string(),
        sealed.capsule.scope.id.clone().into(),
    );
    let outcome = append(
        ledger,
        &NewObservation {
            observation_type,
            source_adapter: "thruline",
            source_session,
            repository_identity,
            workspace_identity: sealed.capsule.workspace.identity.as_deref(),
            observed_at,
            completeness: if sealed.capsule.completeness.state == "complete" {
                Completeness::Complete
            } else {
                Completeness::Incomplete
            },
            receipt: &receipt,
        },
    )?;
    Ok(CapsuleRecordOutcome::Recorded(Box::new(outcome)))
}

/// Append `CAPSULE_CREATED` for a sealed capsule. The receipt terminates at
/// the sealed artifact on disk — path plus the canonical bytes' own SHA-256
/// (its CAS key in the protected store); the capsule id and digest ride in
/// the hash-covered extras. Identical content re-recorded is the standing
/// row (§33 idempotency) — an observation is a fact recorded once, never a
/// verification timestamp.
pub fn record_capsule_created(
    ledger: &Ledger,
    sealed: &SealedCapsule,
    artifact_path: &Path,
    observed_at: &str,
    source_session: &str,
) -> Result<CapsuleRecordOutcome> {
    let receipt = capsule_receipt(sealed, &artifact_path.display().to_string());
    record_capsule_observation(
        ledger,
        sealed,
        ObservationType::CapsuleCreated,
        receipt,
        observed_at,
        source_session,
    )
}

/// Append `CAPSULE_OFFERED` when a sealed capsule is staged for a named
/// agent. Staging is an offer and nothing more — acceptance, digest match,
/// identity match and acknowledgment are each their own future observation
/// (Stage 7's writers), never inferred from this one.
pub fn record_capsule_offered(
    ledger: &Ledger,
    sealed: &SealedCapsule,
    agent: &str,
    artifact_path: &Path,
    observed_at: &str,
    source_session: &str,
) -> Result<CapsuleRecordOutcome> {
    let mut receipt = capsule_receipt(sealed, &artifact_path.display().to_string());
    receipt
        .extra
        .insert("offered_to_agent".to_string(), agent.into());
    record_capsule_observation(
        ledger,
        sealed,
        ObservationType::CapsuleOffered,
        receipt,
        observed_at,
        source_session,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_encoding_primitives_are_length_prefixed_and_tagged() {
        let mut out = Vec::new();
        lp(&mut out, "ab");
        assert_eq!(out, [2, 0, 0, 0, 0, 0, 0, 0, b'a', b'b']);
        let mut o = Vec::new();
        opt(&mut o, None);
        assert_eq!(o, [0x00]);
        let mut o2 = Vec::new();
        opt(&mut o2, Some(""));
        assert_eq!(
            o2,
            [0x01, 0, 0, 0, 0, 0, 0, 0, 0],
            "None and Some(\"\") differ"
        );
    }

    #[test]
    fn obligation_ids_are_source_deterministic_and_wording_free() {
        let a = obligation_id("s1", "uncommitted_edit_run", "");
        let b = obligation_id("s1", "uncommitted_edit_run", "");
        assert_eq!(a, b);
        assert!(a.starts_with("obl1:"));
        assert_ne!(a, obligation_id("s2", "uncommitted_edit_run", ""));
        assert_ne!(a, obligation_id("s1", "other_kind", ""));
        assert_ne!(a, obligation_id("s1", "uncommitted_edit_run", "k"));
    }
}
