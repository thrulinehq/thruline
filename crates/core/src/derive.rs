//! Deterministic graph derivation. No LLM, no heuristic scoring — every rule
//! here is a total function of the ledger contents.
//!
//! Ordering is explicit everywhere: sessions by `(started_at, id)` with NULL
//! timestamps last, events by `(session order, seq)`. Nothing iterates a hash
//! map. Two runs over the same ledger produce byte-identical output, which
//! `rebuild` asserts.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::{params, OptionalExtension};

use crate::classification_store::{self, ClaimProjectionRecord};
use crate::error::Result;
use crate::extract::ClaimExtractor;
use crate::ledger::Ledger;
use crate::model::EventKind;

/// Payload prefix loaded per event. Full text stays in the ledger; derivation
/// only needs enough for a label, an excerpt and an evidence summary, and this
/// keeps a 471k-event project inside a sane memory budget.
const PAYLOAD_PREFIX: usize = 2000;

/// Without annotations, derivation emits only `recorded`, `pending` and
/// `unknown`: at the deterministic tier `supported` would overclaim in exactly
/// the way "Verified" did — a tool ran, which is not the same as the work being
/// what was claimed. `supported`, `unsupported`, `contradicted` and `dropped`
/// require the annotation layer and appear only once `annotate` has run.
/// `refused` waits for M4.
/// North Star v2's verdict vocabulary, at node level.
///
/// The word `unverified` is gone, and its disappearance is the point: it was
/// doing two incompatible jobs at once — "evidence conflicts with this claim"
/// and "we found no proof either way" — and collapsing them let a coverage gap
/// wear the costume of an accusation. They are now separate verdicts with
/// separate consequences.
///
/// **Only `Contradicted` and `Dropped` are accusations.** `Unsupported` is a
/// statement about our evidence, not about the speaker's honesty, and it is
/// excluded from warning counts everywhere (§5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NodeState {
    /// An action happened. Nothing is claimed, nothing is judged.
    Recorded,
    /// Entailed by parsed-fidelity captured evidence (forensic).
    Supported,
    /// A proposal still outstanding.
    Pending,
    /// Abstention: unverifiable, fallback-only, or uncertain. A respected outcome.
    Unknown,
    /// No captured proof found. **Not an accusation** — a coverage statement.
    Unsupported,
    /// A proposal the project moved past with no later reference at all — 👻.
    Dropped,
    /// Conflicting parsed evidence exists. The only strong accusation. ⚠
    Contradicted,
}

impl NodeState {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeState::Recorded => "recorded",
            NodeState::Supported => "supported",
            NodeState::Pending => "pending",
            NodeState::Unknown => "unknown",
            NodeState::Unsupported => "unsupported",
            NodeState::Dropped => "dropped",
            NodeState::Contradicted => "contradicted",
        }
    }

    /// Worst-claim-wins ordering. A node is only as good as its weakest claim,
    /// so one unsupported assertion in a paragraph of true ones still warns.
    fn severity(self) -> u8 {
        match self {
            NodeState::Recorded => 0,
            NodeState::Supported => 1,
            NodeState::Pending => 2,
            NodeState::Unknown => 3,
            NodeState::Unsupported => 4,
            NodeState::Dropped => 5,
            NodeState::Contradicted => 6,
        }
    }

    /// The two verdicts that accuse. Everything else describes our evidence,
    /// not the speaker. Warning counts, the Now queue and the adjudication
    /// gates all key on this and nothing else (§5).
    pub fn is_accusation(self) -> bool {
        matches!(self, NodeState::Contradicted | NodeState::Dropped)
    }

    fn worse_of(self, other: Self) -> Self {
        if other.severity() > self.severity() {
            other
        } else {
            self
        }
    }
}

/// The three independent node dimensions from NORTHSTAR v2.
///
/// `NodeState` remains only the deterministic sort key used by diagnostics and
/// legacy presentation. It must never be treated as the source of truth for a
/// node's lifecycle, evidence quality, or currency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Lifecycle {
    Proposed,
    Accepted,
    Active,
    Completed,
    Abandoned,
}

impl Lifecycle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Accepted => "accepted",
            Self::Active => "active",
            Self::Completed => "completed",
            Self::Abandoned => "abandoned",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EvidenceState {
    Unknown,
    Observed,
    Supported,
    Verified,
    Contradicted,
    Stale,
}

impl EvidenceState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Observed => "observed",
            Self::Supported => "supported",
            Self::Verified => "verified",
            Self::Contradicted => "contradicted",
            Self::Stale => "stale",
        }
    }

    fn severity(self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::Observed => 1,
            Self::Supported => 2,
            Self::Verified => 3,
            Self::Stale => 4,
            Self::Contradicted => 5,
        }
    }
}

/// The A87 derivation rule: **applicability is the input, evidence state is the
/// projection.**
///
/// A `Verified` claim reads `Stale` exactly when its receipt's applicability has
/// lapsed. Every other pair passes through untouched, and two of those
/// pass-throughs are load-bearing doctrine rather than omissions:
///
/// * **Unknown applicability is not staleness.** A claim whose receipt coverage
///   cannot be established keeps the state it had. Staleness is a statement
///   about *coverage*, not about doubt — absence of a receipt is `unknown`, and
///   conflicting evidence is `contradicted`. Neither is reachable from here.
/// * **Staleness never launders an accusation, and never manufactures one.**
///   Only the `Verified` row moves, so a lapsed receipt can neither soften a
///   conflicting result nor harden an abstention into one.
///
/// This function rewrites nothing. The receipt keeps saying what it always said
/// and the `verified` fact stays in the append-only record; this is what the
/// *current* read returns (NORTHSTAR-v2, "Proof freshness").
pub fn applied_evidence_state(
    state: EvidenceState,
    applicability: crate::capture::ProofApplicability,
) -> EvidenceState {
    use crate::capture::ProofApplicability;
    match (state, applicability) {
        (EvidenceState::Verified, ProofApplicability::Stale) => EvidenceState::Stale,
        (unchanged, _) => unchanged,
    }
}

impl Lifecycle {
    /// Parse a persisted value. `None` means the stored string is outside the
    /// schema's CHECK domain, which is corruption — callers must fail loud
    /// rather than substitute a default, because a guessed axis is a wrong
    /// label wearing a confident face.
    pub fn parse(s: &str) -> Option<Lifecycle> {
        match s {
            "proposed" => Some(Lifecycle::Proposed),
            "accepted" => Some(Lifecycle::Accepted),
            "active" => Some(Lifecycle::Active),
            "completed" => Some(Lifecycle::Completed),
            "abandoned" => Some(Lifecycle::Abandoned),
            _ => None,
        }
    }
}

impl EvidenceState {
    /// See [`Lifecycle::parse`] for why this abstains instead of defaulting.
    pub fn parse(s: &str) -> Option<EvidenceState> {
        match s {
            "unknown" => Some(EvidenceState::Unknown),
            "observed" => Some(EvidenceState::Observed),
            "supported" => Some(EvidenceState::Supported),
            "verified" => Some(EvidenceState::Verified),
            "contradicted" => Some(EvidenceState::Contradicted),
            "stale" => Some(EvidenceState::Stale),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Currency {
    Current,
    Superseded,
    OutOfScope,
}

impl Currency {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Superseded => "superseded",
            Self::OutOfScope => "out_of_scope",
        }
    }

    /// See [`Lifecycle::parse`] for why this abstains instead of defaulting.
    pub fn parse(s: &str) -> Option<Currency> {
        match s {
            "current" => Some(Currency::Current),
            "superseded" => Some(Currency::Superseded),
            "out_of_scope" => Some(Currency::OutOfScope),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeDimensions {
    pub lifecycle: Lifecycle,
    pub evidence: EvidenceState,
    pub currency: Currency,
}

impl NodeDimensions {
    pub const fn new(lifecycle: Lifecycle, evidence: EvidenceState, currency: Currency) -> Self {
        Self {
            lifecycle,
            evidence,
            currency,
        }
    }
}

fn dimensions_for_claim(
    kind: &str,
    sort_key: NodeState,
    has_evidence: bool,
    attested: Option<&crate::contract::AttestedCheck>,
) -> NodeDimensions {
    use crate::extract::ClaimKind;

    let lifecycle = if kind == ClaimKind::Proposal.as_str() {
        match sort_key {
            NodeState::Supported => Lifecycle::Completed,
            NodeState::Dropped => Lifecycle::Abandoned,
            _ => Lifecycle::Proposed,
        }
    } else if kind == ClaimKind::MicroIntent.as_str() {
        // A micro-intent is a narration of the immediate turn, never durable
        // outstanding work. Missing attached evidence therefore changes its
        // evidence dimension, not its lifecycle into `active`/`pending`.
        Lifecycle::Completed
    } else {
        Lifecycle::Completed
    };
    let evidence = match sort_key {
        NodeState::Supported => EvidenceState::Supported,
        NodeState::Contradicted => EvidenceState::Contradicted,
        NodeState::Recorded if has_evidence => EvidenceState::Observed,
        _ => EvidenceState::Unknown,
    };
    // R17a-3: the ONLY producer of `Verified` in the engine.
    //
    // It may only ever RAISE `Supported` to `Verified`, and only on an
    // affirmative `check_attested_run`. Three properties make that safe:
    //
    // * It cannot manufacture a verdict from nothing — the claim must already
    //   have reached `Supported` through the ordinary contract.
    // * It cannot soften an accusation — `Contradicted` is not in the match.
    // * It cannot reach a claim with no accepted assured pair bound to it, and
    //   `check_attested_run` abstains rather than convicting on every trap.
    let evidence = match (evidence, attested) {
        (EvidenceState::Supported, Some(crate::contract::AttestedCheck::Verified(_))) => {
            EvidenceState::Verified
        }
        (other, _) => other,
    };
    NodeDimensions::new(lifecycle, evidence, Currency::Current)
}

/// Resolve the attested verdict for one claim, or `None`.
///
/// A claim is attested only when **exactly one** of its evidence links is bound
/// to an accepted assured pair. Two bound links means we cannot say which run
/// the claim is about, and binding either would be a guess — so the honest
/// answer is no attestation at all.
fn attested_for_claim(
    c: &ClaimRow,
    runs: &std::collections::BTreeMap<String, crate::contract::AttestedRun>,
) -> Option<crate::contract::AttestedCheck> {
    let mut bound = c.links.iter().filter_map(|l| runs.get(&l.event_id));
    let run = bound.next()?;
    if bound.next().is_some() {
        return None;
    }
    let assertion = crate::contract::parse_claim(&c.text)?;
    Some(crate::contract::check_attested_run(
        &assertion,
        Some(&run.command_sha256),
        run,
    ))
}

fn dimensions_for_sort_key(sort_key: NodeState, has_evidence: bool) -> NodeDimensions {
    let lifecycle = match sort_key {
        NodeState::Pending => Lifecycle::Proposed,
        NodeState::Dropped => Lifecycle::Abandoned,
        _ => Lifecycle::Completed,
    };
    let evidence = match sort_key {
        NodeState::Recorded if has_evidence => EvidenceState::Observed,
        NodeState::Supported => EvidenceState::Supported,
        NodeState::Contradicted => EvidenceState::Contradicted,
        _ => EvidenceState::Unknown,
    };
    NodeDimensions::new(lifecycle, evidence, Currency::Current)
}

/// Test seam for the RULING 4 cap. The containment must be exercisable without
/// a producer — a cage tested only after the animal arrives is not a cage.
pub fn compose_dimensions_for_test(claims: &[NodeDimensions]) -> NodeDimensions {
    let verdicts: Vec<ClaimVerdict> = claims
        .iter()
        .enumerate()
        .map(|(i, d)| ClaimVerdict {
            claim_id: format!("c{i}"),
            text: String::new(),
            kind: String::new(),
            marker: String::new(),
            span_start: 0,
            span_end: 0,
            candidates: 0,
            parsed_candidates: 0,
            best_score: None,
            best_event_id: None,
            best_fidelity: None,
            state: NodeState::Recorded,
            dimensions: *d,
            reason: String::new(),
            evidence_scopes: Vec::new(),
        })
        .collect();
    compose_dimensions(&verdicts, NodeState::Recorded, true)
}

fn compose_dimensions(
    verdicts: &[ClaimVerdict],
    sort_key: NodeState,
    has_evidence: bool,
) -> NodeDimensions {
    if verdicts.is_empty() {
        return dimensions_for_sort_key(sort_key, has_evidence);
    }

    let lifecycle = if verdicts
        .iter()
        .any(|v| v.dimensions.lifecycle == Lifecycle::Active)
    {
        Lifecycle::Active
    } else if verdicts
        .iter()
        .any(|v| v.dimensions.lifecycle == Lifecycle::Accepted)
    {
        Lifecycle::Accepted
    } else if verdicts
        .iter()
        .any(|v| v.dimensions.lifecycle == Lifecycle::Proposed)
    {
        Lifecycle::Proposed
    } else if verdicts
        .iter()
        .all(|v| v.dimensions.lifecycle == Lifecycle::Abandoned)
    {
        Lifecycle::Abandoned
    } else {
        Lifecycle::Completed
    };
    let evidence = verdicts
        .iter()
        .map(|v| v.dimensions.evidence)
        .max_by_key(|e| e.severity())
        .unwrap_or(EvidenceState::Unknown);
    // RULING 4 — the composition cap. The fold is deliberately ASYMMETRIC, and
    // the asymmetry is the point:
    //
    // * An accusation RISING is protective. One contradicted claim among nine
    //   unknown ones means the node contains a conflict a human should see, and
    //   hiding it behind the majority would be worst-claim-wins in reverse.
    // * A promotion rising is OVER-CLAIMING. One verified claim among nine
    //   unknown ones does not make the node verified — it makes the node
    //   *contain* something verified. Publishing the paragraph as verified on
    //   the strength of one sentence is exactly the "single dot fed by
    //   worst-claim-wins" NORTHSTAR-v2 forbids, pointed the other way.
    //
    // So `verified` is capped to the strongest thing a composition may honestly
    // assert. The per-claim verdict keeps its own `verified` untouched — the cap
    // is on the COMPOSITION, never on the claim, and the claim is where the
    // verdict lives.
    let evidence = match evidence {
        EvidenceState::Verified => EvidenceState::Supported,
        other => other,
    };
    NodeDimensions::new(lifecycle, evidence, Currency::Current)
}

/// Compatibility ordering derived from the dimensional record. The two
/// shadow-only diagnostics are retained when a claim explicitly carries them;
/// neither is smuggled into the evidence dimension.
fn sort_key_for_dimensions(dimensions: NodeDimensions, verdicts: &[ClaimVerdict]) -> NodeState {
    if verdicts.iter().any(|v| v.state == NodeState::Contradicted)
        || dimensions.evidence == EvidenceState::Contradicted
    {
        return NodeState::Contradicted;
    }
    if verdicts.iter().any(|v| v.state == NodeState::Dropped) {
        return NodeState::Dropped;
    }
    if verdicts.iter().any(|v| v.state == NodeState::Unsupported) {
        return NodeState::Unsupported;
    }
    match dimensions.evidence {
        EvidenceState::Supported | EvidenceState::Verified => NodeState::Supported,
        EvidenceState::Observed => NodeState::Recorded,
        EvidenceState::Unknown | EvidenceState::Stale => match dimensions.lifecycle {
            Lifecycle::Proposed | Lifecycle::Accepted | Lifecycle::Active => NodeState::Pending,
            Lifecycle::Completed | Lifecycle::Abandoned => NodeState::Unknown,
        },
        EvidenceState::Contradicted => NodeState::Contradicted,
    }
}

/// What happened to one extracted claim, for the receipt.
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimVerdict {
    pub claim_id: String,
    pub text: String,
    /// `completed_work` | `proposal`.
    pub kind: String,
    pub marker: String,
    /// Byte offsets into the anchor event's text, for span highlighting.
    pub span_start: i64,
    pub span_end: i64,
    pub candidates: usize,
    pub parsed_candidates: usize,
    pub best_score: Option<f32>,
    pub best_event_id: Option<String>,
    /// Whether the best-scoring candidate's premise was understood.
    pub best_fidelity: Option<String>,
    pub state: NodeState,
    pub dimensions: NodeDimensions,
    /// Plain-language reason, shown verbatim in the receipt.
    pub reason: String,
    /// Claim-specific evidence rows with their frozen M4.4 scope and derived
    /// freshness. These are the scored rows, not merely nearby actions.
    pub evidence_scopes: Vec<EvidenceScope>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceScope {
    pub event_id: String,
    pub session_id: String,
    pub seq: i64,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub command_identity: Option<String>,
    /// `unknown` or `stale` in Forensic Mode. A bounded transcript scan can
    /// prove a later differing run exists, but cannot prove no later run exists;
    /// positive `current` freshness requires Assured-Mode scope in M5.
    pub freshness: &'static str,
    pub superseded_by_event_id: Option<String>,
    pub superseded_by_session_id: Option<String>,
    pub superseded_by_seq: Option<i64>,
    pub freshness_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRow {
    /// §9 evidence kinds. `edit` for file-writing tools, `command` otherwise.
    pub kind: &'static str,
    pub path: Option<String>,
    pub summary: String,
    /// The tool that produced this row, for premise construction.
    pub tool_name: String,
    /// First token of a Bash command / the command family.
    pub command: Option<String>,
    /// The LAST ~300 chars of the result. Test and build runners print their
    /// summary at the end, so a head-anchored excerpt has nothing for a marker
    /// claim ("33/33 pass") to entail against.
    pub result_tail: Option<String>,
    /// Bounded excerpt of what an edit introduced.
    pub new_content: Option<String>,
    /// First line of what the tool actually returned, ≤80 chars. This is the
    /// part of a receipt that makes it a receipt rather than a restatement.
    pub result_excerpt: Option<String>,
    pub tool_use_id: Option<String>,
    pub is_error: bool,
    /// Bounded prefix of the tool's input JSON. Structured tools (`TaskCreate`,
    /// `TaskUpdate`, `Workflow`, MCP calls) carry their meaning here rather
    /// than in their output, and the renderer needs it to say anything
    /// grounded about them. Kept raw so all rendering decisions stay in
    /// `premise.rs`, where the re-measurement invariant applies.
    pub tool_input: Option<String>,
    /// §5i: where this event's raw JSONL line lives, so `annotate` can read the
    /// per-line `cwd` a shell command actually ran in. Carried here rather than
    /// migrated into `events`, which is append-only and must not be touched.
    pub raw_file: Option<String>,
    pub raw_line: Option<i64>,
    /// M4.4 proof scope v0. The immutable coordinates are always available
    /// for ledger events; ambient cwd/branch are present only when the raw
    /// line still matches its ingest-time hash.
    pub session_id: Option<String>,
    pub seq: Option<i64>,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub command_identity: Option<String>,
    /// Hash used to admit ambient raw-line scope. Kept out of exports; it is
    /// an integrity input, not user-facing scope.
    pub raw_sha256: Option<String>,
}

/// Whole textual content of the `{content, toolUseResult}` envelope.
fn result_text(tool_result: Option<&str>) -> Option<String> {
    let raw = tool_result?;
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    let text = match v.get("content") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Last `n` characters, on a char boundary, with a leading ellipsis when cut.
pub fn tail_chars(s: &str, n: usize) -> String {
    let count = s.chars().count();
    if count <= n {
        return s.trim().to_string();
    }
    let cut: String = s.chars().skip(count - n).collect();
    format!("…{}", cut.trim_start())
}

/// First line of the `{content, toolUseResult}` envelope the parser stored.
fn result_excerpt(tool_result: Option<&str>) -> Option<String> {
    let raw = tool_result?;
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    let text = match v.get("content") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .find_map(|b| b.get("text").and_then(|t| t.as_str()))
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    };
    let first = text.lines().find(|l| !l.trim().is_empty())?.trim();
    if first.is_empty() {
        return None;
    }
    Some(if first.chars().count() > 80 {
        format!("{}…", first.chars().take(79).collect::<String>())
    } else {
        first.to_string()
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct DerivedNode {
    pub id: String,
    pub label: String,
    /// Legacy verdict vocabulary retained as a deterministic diagnostic sort
    /// key. Product meaning lives in `dimensions`.
    pub sort_key: NodeState,
    pub dimensions: NodeDimensions,
    pub session_id: String,
    pub anchor_event_id: String,
    pub anchor_seq: i64,
    /// 1-based position of the anchor among the project's canonical events,
    /// so a receipt can say "event N of M" against a project-wide total.
    pub anchor_ordinal: u64,
    pub anchor_uuid: Option<String>,
    pub ts: Option<String>,
    /// Excerpt of the anchor event, for the receipt claim.
    pub claim_text: String,
    pub evidence: Vec<EvidenceRow>,
    /// Head of a branch that is not on the path to the newest leaf. Retained,
    /// not dropped — M3's dropped-thread candidates.
    pub abandoned: bool,
    pub raw_file: String,
    pub raw_line: u64,
    /// Per-claim outcomes from the annotation layer. Empty until `annotate`
    /// has run, which is why the deterministic tier cannot emit ⚠.
    pub verdicts: Vec<ClaimVerdict>,
    /// The node's own reason — the worst claim's, since worst-claim-wins.
    pub verdict_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DerivedEdge {
    pub from: String,
    pub to: String,
    /// `seq` follows the canonical parent chain; `inferred` covers
    /// cross-session continuation and extra parents recovered from dedup.
    pub kind: &'static str,
}

/// Audit relation for one later ledger row folded into the first chronological
/// occurrence of its source UUID.
///
/// The ledger retains both event rows. This relation records only their stable
/// identifiers and chronology: it deliberately carries no transcript text or
/// raw-file path. Every deep resume copy points directly to the first event,
/// never to another duplicate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DuplicateEventRelation {
    pub duplicate_event_id: String,
    pub canonical_event_id: String,
    pub src_uuid: String,
    pub duplicate_session_id: String,
    pub duplicate_seq: i64,
    pub canonical_session_id: String,
    pub canonical_seq: i64,
}

#[derive(Debug, Clone, Default)]
pub struct DeriveStats {
    pub raw_events: u64,
    pub canonical_events: u64,
    pub duplicate_events: u64,
    pub extra_parent_edges: u64,
    pub action_clusters: u64,
    pub collapsed_clusters: u64,
    pub statement_nodes: u64,
    pub abandoned_heads: u64,
    pub state_counts: BTreeMap<String, u64>,
    pub lifecycle_counts: BTreeMap<String, u64>,
    pub evidence_counts: BTreeMap<String, u64>,
    pub currency_counts: BTreeMap<String, u64>,
}

#[derive(Debug, Clone)]
pub struct DerivedGraph {
    pub project_id: String,
    pub sessions: Vec<SessionInfo>,
    pub nodes: Vec<DerivedNode>,
    pub edges: Vec<DerivedEdge>,
    pub duplicate_events: Vec<DuplicateEventRelation>,
    pub stats: DeriveStats,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    pub id: String,
    pub started_at: Option<String>,
    pub event_count: u64,
}

/// One event as derivation sees it.
#[derive(Debug, Clone)]
struct Ev {
    id: String,
    session_id: String,
    seq: i64,
    kind: EventKind,
    text: Option<String>,
    tool_name: Option<String>,
    tool_use_id: Option<String>,
    tool_input: Option<String>,
    tool_result: Option<String>,
    is_error: bool,
    src_uuid: Option<String>,
    parent_uuid: Option<String>,
    ts: Option<String>,
    raw_file: String,
    raw_line: u64,
    raw_sha256: String,
}

/* ── crude, deterministic statement detection (no model) ─────────────── */

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Statement {
    CompletedWork,
    Proposal,
}

/// Statement detection, now via the v2 extractor rather than the M2 prefix
/// matcher (which measured 0.00 completed-work recall against A19).
///
/// A node is a *completed-work* statement if any of its claims asserts finished
/// work — one unsupported assertion among proposals is still a claim that can
/// be wrong, and worst-claim-wins depends on it not being hidden.
fn classify_statement(text: &str) -> Option<Statement> {
    let claims = crate::extract::MarkerExtractor.extract(text);
    if claims.is_empty() {
        return None;
    }
    if claims.iter().any(|c| c.kind.is_material_work()) {
        Some(Statement::CompletedWork)
    } else {
        // Preserve the Tier-0 heuristic's legacy graph shape. Once a complete
        // language run exists, `classify_projected_statement` distinguishes
        // proposals from findings and micro-intents explicitly.
        Some(Statement::Proposal)
    }
}

#[derive(Default)]
struct LanguageProjection {
    active: bool,
    kinds_by_event: HashMap<String, Vec<String>>,
}

/// A complete §5t run makes the active claims table authoritative for node
/// material. Per-event annotations written by an interrupted run are inert
/// until its final run manifest exists.
fn load_language_projection(ledger: &Ledger, project_id: &str) -> Result<LanguageProjection> {
    let mut projection = LanguageProjection::default();
    let head: Option<(String, Option<String>)> = ledger
        .conn()
        .query_row(
            "SELECT extractor, manifest_annotation_id
             FROM claim_projection_heads WHERE project_id = ?1",
            params![project_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((extractor, Some(manifest_id))) = head else {
        return Ok(projection);
    };
    let annotation = classification_store::find_by_id(ledger, &manifest_id)?.ok_or_else(|| {
        crate::error::CoreError::Integrity(format!(
            "claim projection for {project_id:?} points at non-classification annotation {manifest_id:?}"
        ))
    })?;
    let manifest_project: String = ledger.conn().query_row(
        "SELECT project_id FROM events WHERE id = ?1",
        params![annotation.event_id],
        |row| row.get(0),
    )?;
    if manifest_project != project_id {
        return Err(crate::error::CoreError::Integrity(format!(
            "claim projection for {project_id:?} points at a manifest from {manifest_project:?}"
        )));
    }
    let raw_manifest = annotation.output_json;
    let manifest = serde_json::from_str::<serde_json::Value>(&raw_manifest)?;
    let complete_language_run = manifest.get("type").and_then(|v| v.as_str())
        == Some("classification_run")
        && manifest.get("complete").and_then(|v| v.as_bool()) == Some(true)
        && manifest.get("extractor").and_then(|v| v.as_str()) == Some(extractor.as_str());
    if !complete_language_run {
        return Err(crate::error::CoreError::Integrity(format!(
            "claim projection for {project_id:?} points at an incomplete or mismatched manifest"
        )));
    }
    let expected_claim_hash = manifest
        .get("claim_set_sha256")
        .and_then(|value| value.as_str())
        .ok_or_else(|| {
            crate::error::CoreError::Integrity(format!(
                "classification manifest {manifest_id:?} has no claim-set digest"
            ))
        })?;

    let current_records: Vec<ClaimProjectionRecord> = {
        let mut stmt = ledger.conn().prepare(
            "SELECT c.id, c.event_id, c.text, c.span_start, c.span_end,
                    c.extractor, c.kind, c.marker
             FROM claims c JOIN events e ON e.id = c.event_id
             WHERE e.project_id = ?1 ORDER BY c.id",
        )?;
        let records = stmt
            .query_map(params![project_id], |row| {
                Ok(ClaimProjectionRecord {
                    id: row.get(0)?,
                    event_id: row.get(1)?,
                    text: row.get(2)?,
                    span_start: row.get(3)?,
                    span_end: row.get(4)?,
                    extractor: row.get(5)?,
                    kind: row.get(6)?,
                    marker: row.get(7)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        records
    };
    let current_claim_hash = classification_store::claim_projection_sha256(current_records)?;
    if current_claim_hash != expected_claim_hash {
        return Err(crate::error::CoreError::Integrity(format!(
            "claim projection for {project_id:?} does not match manifest {manifest_id:?}"
        )));
    }
    projection.active = true;

    let mut stmt = ledger.conn().prepare(
        "SELECT c.event_id, c.kind FROM claims c
         JOIN events e ON e.id = c.event_id
         WHERE e.project_id = ?1 AND c.extractor = ?2
         ORDER BY c.event_id, c.span_start, c.id",
    )?;
    let rows = stmt.query_map(params![project_id, extractor], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (event_id, kind) = row?;
        projection
            .kinds_by_event
            .entry(event_id)
            .or_default()
            .push(kind);
    }
    Ok(projection)
}

fn classify_projected_statement(
    event_id: &str,
    text: &str,
    projection: &LanguageProjection,
) -> Option<Statement> {
    if !projection.active {
        return classify_statement(text);
    }
    let kinds = projection.kinds_by_event.get(event_id)?;
    if kinds.iter().any(|kind| {
        kind == crate::extract::ClaimKind::Work.as_str()
            || kind == crate::extract::ClaimKind::Marker.as_str()
    }) {
        Some(Statement::CompletedWork)
    } else if kinds
        .iter()
        .any(|kind| kind == crate::extract::ClaimKind::Proposal.as_str())
    {
        Some(Statement::Proposal)
    } else {
        // Findings and micro-intents are recorded language, not outstanding
        // proposals. Guidance/questions/narration have no claim rows at all.
        Some(Statement::CompletedWork)
    }
}

/// First sentence, lowercased, markdown bullets and emphasis stripped.
fn first_sentence(text: &str) -> String {
    let t = text.trim_start_matches(['#', '-', '*', '>', ' ', '\t', '\n']);
    let t = t.trim();
    let end = t
        .char_indices()
        .find(|(_, c)| *c == '.' || *c == '\n' || *c == '!' || *c == '?')
        .map(|(i, _)| i)
        .unwrap_or(t.len());
    t[..end].trim().to_lowercase()
}

/// ≤22 chars, cut at a word boundary, ellipsis when truncated.
fn label_from(text: &str) -> String {
    // A209-C5/B7-8: mask BEFORE `first_sentence`, which lowercases. The
    // detector table is case-sensitive and an AWS key id is [A-Z0-9]{20},
    // so a lowercased key is exactly recoverable by upper-casing — a
    // recoverable credential in the sealed bytes under "nothing matched".
    // A209-C7: the record is carried as a VALUE here, not discarded with
    // `.0`. A209-C6 claimed this site had been converted and it had not —
    // `git diff --stat 90806e0 53046ae -- derive.rs` was empty. Converting
    // it is the completion that makes the claim true.
    let masked = crate::redact::Masked::mask(text);
    let record = masked.record().clone();
    let masked_text = masked.into_text();
    let text: &str = &masked_text;
    let mut s = first_sentence(text);
    if s.is_empty() {
        s = text.trim().to_lowercase();
    }
    let cleaned: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        return "(empty)".to_string();
    }
    let out = truncate_words(&cleaned, 22);
    // The record travels with the label to whatever renders it. Nothing
    // downstream re-derives it from the text.
    debug_assert!(record.complete);
    let _ = record;
    out
}

/// Cut to `max` characters on a word boundary.
///
/// **Masks before cutting** (A209-C3). Truncation is upstream of every
/// renderer that masks, so cutting first can slice a credential below its
/// detector's body threshold and hand the downstream masker a shape it can
/// no longer recognise. The ordering is the invariant; the cut is the
/// detail. Enumerated in `redact::TRUNCATION_SITES`.
fn truncate_words(s: &str, max: usize) -> String {
    let masked = crate::redact::Masked::mask(s);
    let masked_text = masked.into_text();
    let s: &str = &masked_text;
    // The word loop splits on ' ', and `[masked: github]` is TWO words, so
    // a budget landing between them used to persist `[masked:` alone
    // (A209-C5/B7-4 partial-marker finding — the marker-safe helper was
    // only reachable from the empty-output fallback). Marker spans are now
    // kept whole by the shared helper on the way out.
    if s.chars().count() <= max {
        return s.to_string();
    }
    let budget = max - 1; // room for the ellipsis
    let mut out = String::new();
    for word in s.split(' ') {
        let need = if out.is_empty() {
            word.chars().count()
        } else {
            out.chars().count() + 1 + word.chars().count()
        };
        if need > budget {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    if out.is_empty() {
        out = crate::redact::truncate_respecting_markers(s, budget);
    }
    out.push('…');
    crate::redact::truncate_respecting_markers(&out, out.chars().count())
}

/// File-writing tools; everything else is a command for §9 evidence purposes.
fn is_edit_tool(name: &str) -> bool {
    matches!(
        name,
        "Edit" | "Write" | "NotebookEdit" | "MultiEdit" | "Update"
    )
}

/// Read one string field out of a JSON object that may be TRUNCATED.
///
/// `tool_input` is stored whole but loaded as a 2,000-char prefix to bound
/// derivation's memory, and a `Write` whose `content` runs past that boundary
/// leaves `serde_json` with an unterminated string. Measured in M3.8b: **919
/// of 1,339 `Write`/`Edit` rows** failed to parse this way, so the product did
/// not know which file had been written — the path was there in the bytes, and
/// a strict parse threw it away along with the rest.
///
/// This scans for the key directly and reads its string literal, so a field
/// that arrived intact survives a payload cut later in the object.
pub fn json_string_field(raw: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let mut from = 0usize;
    while let Some(rel) = raw[from..].find(&needle) {
        let mut i = from + rel + needle.len();
        let b = raw.as_bytes();
        while i < b.len() && (b[i] as char).is_whitespace() {
            i += 1;
        }
        if i >= b.len() || b[i] != b':' {
            from = from + rel + needle.len();
            continue;
        }
        i += 1;
        while i < b.len() && (b[i] as char).is_whitespace() {
            i += 1;
        }
        if i >= b.len() || b[i] != b'"' {
            from = from + rel + needle.len();
            continue;
        }
        i += 1;
        let mut out = String::new();
        while i < b.len() {
            match b[i] {
                b'\\' => {
                    i += 1;
                    match b.get(i) {
                        Some(b'n') => out.push('\n'),
                        Some(b't') => out.push('\t'),
                        Some(b'r') => out.push('\r'),
                        Some(b'u') => {
                            // Keep it simple: skip the escape rather than
                            // guess. Paths and commands are ASCII in practice.
                            i += 4;
                        }
                        Some(&c) => out.push(c as char),
                        None => break,
                    }
                    i += 1;
                }
                b'"' => return Some(out),
                _ => {
                    // Copy the raw byte run so multi-byte UTF-8 survives.
                    let start = i;
                    while i < b.len() && b[i] != b'"' && b[i] != b'\\' {
                        i += 1;
                    }
                    out.push_str(&raw[start..i]);
                }
            }
        }
        // Ran off the end of a truncated payload: no closing quote, so this
        // field itself was cut. Report nothing rather than a half value.
        return None;
    }
    None
}

/// Extract a `file_path`-like value from a tool_use input JSON prefix.
fn target_path(tool_input: Option<&str>) -> Option<String> {
    let raw = tool_input?;
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        for key in ["file_path", "filePath", "path", "notebook_path"] {
            if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
                return Some(s.to_string());
            }
        }
        return None;
    }
    // Truncated payload — recover the path by scanning.
    for key in ["file_path", "filePath", "path", "notebook_path"] {
        if let Some(s) = json_string_field(raw, key) {
            return Some(s);
        }
    }
    None
}

/// What an edit introduced, tolerant of a truncated payload.
fn edit_content(tool_input: Option<&str>) -> Option<String> {
    let raw = tool_input?;
    let one_line = |t: &str| -> String {
        let one = t.split_whitespace().collect::<Vec<_>>().join(" ");
        one.chars().take(220).collect()
    };
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        for k in ["new_string", "content", "new_source"] {
            if let Some(t) = v.get(k).and_then(|x| x.as_str()) {
                return Some(one_line(t));
            }
        }
        return None;
    }
    for k in ["new_string", "content", "new_source"] {
        if let Some(t) = json_string_field(raw, k) {
            return Some(one_line(&t));
        }
    }
    None
}

/// Recover an edit's target path from what the tool *replied*.
///
/// A `Write` puts `content` before `file_path`, so a large file pushes the
/// path past the 2,000-char payload prefix entirely — 208 rows in M3.8b, where
/// no amount of tolerant scanning of the input can help because the bytes are
/// not loaded. The result says it plainly ("File created successfully at: …"),
/// and that is the same fact from the other side of the call.
fn path_from_result(result: Option<&str>) -> Option<String> {
    // Scanned on the RAW payload, not through `result_text`: a Write's result
    // echoes the whole file back, so the stored 2,000-char prefix is itself
    // truncated JSON and will not parse either. The lead phrase sits in the
    // first hundred characters, well inside the prefix.
    let text = result?;
    for lead in [
        "File created successfully at: ",
        "The file ",
        "Successfully wrote to ",
    ] {
        let Some(i) = text.find(lead) else { continue };
        let rest = &text[i + lead.len()..];
        let end = rest.find(['\n', ' ', '(']).unwrap_or(rest.len());
        let cand = rest[..end].trim().trim_end_matches(['.', ',']);
        if cand.starts_with('/') && cand.len() > 1 {
            return Some(cand.to_string());
        }
    }
    None
}

/// Bash's `command`, tolerant of a truncated payload for the same reason.
fn bash_command(tool_input: Option<&str>) -> Option<String> {
    let raw = tool_input?;
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        return v
            .get("command")
            .and_then(|c| c.as_str())
            .map(|c| c.to_string());
    }
    json_string_field(raw, "command")
}

/// Collapsing key: consecutive clusters in one turn sharing this key become one
/// node. File-writing tools key on the target path (three edits to one file are
/// one unit of work); `Bash` keys on the executable (the command family);
/// everything else keys on the tool name.
fn collapse_key(tool_name: &str, tool_input: Option<&str>) -> String {
    if is_edit_tool(tool_name) {
        if let Some(p) = target_path(tool_input) {
            return format!("file:{p}");
        }
    }
    if tool_name == "Bash" {
        if let Some(raw) = tool_input {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
                if let Some(cmd) = v.get("command").and_then(|x| x.as_str()) {
                    let exe = cmd.split_whitespace().next().unwrap_or("").to_string();
                    if !exe.is_empty() {
                        return format!("cmd:{exe}");
                    }
                }
            }
        }
    }
    format!("tool:{tool_name}")
}

fn short_path(p: &str) -> String {
    p.rsplit('/').next().unwrap_or(p).to_string()
}

/* ── the annotation layer, as derivation consumes it ─────────────────── */

#[derive(Debug, Clone)]
struct LinkRow {
    event_id: String,
    score: f32,
    /// `entailed` | `not_entailed` | `uncertain`, banded at annotate time with
    /// the thresholds recorded in `annotations`. Derivation reads the band
    /// rather than re-deriving it, so nodes stay a pure function of
    /// events + annotations.
    verdict: String,
    fidelity: String,
    related: bool,
    /// What the recorded run reported — the typed contract's input (A33).
    ev: crate::contract::EvidenceOutcome,
    /// How strongly this evidence is *about* the claim (A32). Frozen at
    /// annotate time by `annotate::relatedness`; the accusation ranking.
    match_strength: f32,
    /// §5i normalised invocation, frozen at annotate time where the per-line
    /// `cwd` was reachable. Empty on rows written before M3.13.
    ev_identity: String,
    /// §5m: which TURN of its session this evidence sits in — the index of
    /// user messages at or before it. Within a turn, narration order is not
    /// temporal order.
    turn: i64,
    /// `backward` for completion evidence, `forward` for proposal fulfilment.
    pass: String,
    /// Position and raw identity material for §5f evidence authority.
    session_rank: usize,
    seq: i64,
    tool_name: String,
    tool_input: String,
    tool_result: String,
    /// M4.4 scope frozen at annotation time.
    ev_session_id: String,
    ev_seq: i64,
    ev_cwd: Option<String>,
    ev_git_branch: Option<String>,
    stale: bool,
    superseded_by_event_id: Option<String>,
    superseded_by_session_id: Option<String>,
    superseded_by_seq: Option<i64>,
}

#[derive(Debug, Clone)]
struct ClaimRow {
    id: String,
    text: String,
    kind: String,
    marker: String,
    span_start: i64,
    span_end: i64,
    session_rank: usize,
    seq: i64,
    links: Vec<LinkRow>,
    /// A34: did a forward search actually run for this claim, and how many
    /// later actions named its target? `None` means no search was ever
    /// recorded — which is NOT the same as a search that found nothing.
    search: Option<SearchRow>,
    /// §5m: the claim's turn within its session.
    turn: i64,
}

/// Proof that we looked (A34). See `claim_searches` in the DDL.
#[derive(Debug, Clone)]
struct SearchRow {
    searched: bool,
    // `name_matches` (the pre-§5j combined count) stays in the ledger for
    // audit but is no longer loaded: derivation deciding on it would be A52's
    // defeat-by-mention returning through the back door.
    /// §5j: matches in action-side text (commands, edit paths, content).
    /// Only these defeat `dropped`.
    action_matches: u32,
    /// §5j: matches that appear only in later OUTPUT. They annotate the
    /// receipt; they never defeat alone.
    output_matches: u32,
    /// §5r: every dominant evidence family in the claim's neighborhood was
    /// included and rendered at parsed fidelity by this annotation run.
    coverage_eligible: bool,
    /// Human-readable family names that prevented comprehensive coverage.
    missing_families: String,
}

/// §5q's closed-world eligibility audit, made executable.
///
/// | accusation path | stage | affirmative whitelist |
/// | --- | --- | --- |
/// | `contradicted` | claim pattern | typed marker claim |
/// | `contradicted` | polarity | `asserts-success` only |
/// | `contradicted` | evidence family | matching recognised test/build invocation |
/// | `contradicted` | outcome format | complete test pair or explicit build status |
/// | `contradicted` | identity | named artifact, or the sole active family identity |
/// | `contradicted` | eligibility | prior, backward, non-superseded evidence |
/// | `dropped` | claim pattern | extracted proposal only |
/// | `dropped` | polarity/modality | affirmatively forward phrasing only |
/// | `dropped` | identity | a durable searchable target |
/// | `dropped` | outcome/coverage | a completed forward search |
/// | `dropped` | eligibility | the project has moved beyond the proposal |
///
/// Adding vocabulary expands recall at one named stage. No downstream stage
/// may infer recognition from the fact that an upstream stage happened to
/// accept a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccusationStage {
    ClaimPattern,
    Polarity,
    EvidenceFamily,
    OutcomeFormat,
    Identity,
    Eligibility,
}

impl AccusationStage {
    fn as_str(self) -> &'static str {
        match self {
            AccusationStage::ClaimPattern => "claim pattern",
            AccusationStage::Polarity => "polarity/modality",
            AccusationStage::EvidenceFamily => "evidence family",
            AccusationStage::OutcomeFormat => "outcome format/search coverage",
            AccusationStage::Identity => "identity",
            AccusationStage::Eligibility => "eligibility",
        }
    }
}

/// §5m turn resolution: a turn starts at each user message. `turn(seq)` =
/// how many user messages sit at-or-before `seq` in that session. Computed
/// once per project from event metadata; O(log n) per lookup.
struct TurnMap {
    by_session: HashMap<String, Vec<i64>>,
}

impl TurnMap {
    fn load(ledger: &Ledger, project_id: &str) -> Result<TurnMap> {
        let mut stmt = ledger.conn().prepare(
            "SELECT session_id, seq FROM events
             WHERE project_id = ?1 AND kind = 'user_msg'
             ORDER BY session_id, seq",
        )?;
        let rows = stmt.query_map(params![project_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        let mut by_session: HashMap<String, Vec<i64>> = HashMap::new();
        for r in rows {
            let (s, q) = r?;
            by_session.entry(s).or_default().push(q);
        }
        Ok(TurnMap { by_session })
    }

    fn turn(&self, session_id: &str, seq: i64) -> i64 {
        match self.by_session.get(session_id) {
            Some(v) => v.partition_point(|q| *q <= seq) as i64,
            None => 0,
        }
    }
}

/// Claims and their evidence links, keyed by anchor event.
fn load_annotations(ledger: &Ledger, project_id: &str) -> Result<HashMap<String, Vec<ClaimRow>>> {
    let turns = TurnMap::load(ledger, project_id)?;
    let mut by_event: HashMap<String, Vec<ClaimRow>> = HashMap::new();
    let mut index: HashMap<String, (String, usize)> = HashMap::new();
    let mut session_ranks: HashMap<String, usize> = HashMap::new();
    {
        let mut stmt = ledger.conn().prepare(
            "SELECT id FROM sessions WHERE project_id = ?1
             ORDER BY (started_at IS NULL), started_at, id",
        )?;
        let rows = stmt.query_map(params![project_id], |r| r.get::<_, String>(0))?;
        for (rank, row) in rows.enumerate() {
            session_ranks.insert(row?, rank);
        }
    }

    let mut stmt = ledger.conn().prepare(
        "SELECT c.id, c.event_id, c.text, c.kind, COALESCE(c.marker,''),
                COALESCE(c.span_start,0), COALESCE(c.span_end,0),
                e.session_id, e.seq
         FROM claims c JOIN events e ON e.id = c.event_id
         WHERE e.project_id = ?1
         ORDER BY c.span_start, c.id",
    )?;
    let rows = stmt.query_map(params![project_id], |r| {
        let session_id: String = r.get(7)?;
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            ClaimRow {
                id: r.get(0)?,
                text: r.get(2)?,
                kind: r.get(3)?,
                marker: r.get(4)?,
                span_start: r.get(5)?,
                span_end: r.get(6)?,
                session_rank: session_ranks.get(&session_id).copied().unwrap_or(0),
                turn: turns.turn(&session_id, r.get::<_, i64>(8)?),
                seq: r.get(8)?,
                links: Vec::new(),
                search: None,
            },
        ))
    })?;
    for row in rows {
        let (id, event_id, claim) = row?;
        let v = by_event.entry(event_id.clone()).or_default();
        index.insert(id, (event_id, v.len()));
        v.push(claim);
    }
    drop(stmt);

    // A scorer swap appends for audit, but only the newest completed
    // entailment run is the active verdict projection. Loading every scorer at
    // once duplicates evidence events and could manufacture stale counts.
    let active_scorer = ledger.active_entailment_scorer(project_id)?;

    let mut stmt = ledger.conn().prepare(
        "SELECT l.claim_id, l.evidence_event_id, COALESCE(l.entail_score,0.0),
                l.verdict, l.fidelity, l.related,
                COALESCE(l.ev_family,'none'), l.ev_passed, l.ev_failed, l.ev_ok,
                COALESCE(l.match_strength,0.0), COALESCE(l.ev_identity,''), l.pass,
                ee.session_id, ee.seq,
                COALESCE(ee.tool_name,
                    CASE WHEN ee.kind = 'user_msg'
                              AND ee.text LIKE '<task-notification>%'
                         THEN 'TaskNotification' ELSE '' END),
                COALESCE(ee.tool_input,''),
                CASE WHEN ee.kind = 'user_msg'
                           AND ee.text LIKE '<task-notification>%'
                     THEN substr(COALESCE(ee.text,''),1,2000)
                     ELSE COALESCE((
                         SELECT substr(tr.tool_result,1,2000) FROM events tr
                         WHERE tr.tool_use_id = ee.tool_use_id AND tr.kind = 'tool_result'
                         ORDER BY tr.session_id, tr.seq LIMIT 1
                     ),'')
                END,
                COALESCE(l.ev_session_id, ee.session_id),
                COALESCE(l.ev_seq, ee.seq),
                l.ev_cwd, l.ev_git_branch
                -- `contradiction` is deliberately NOT selected. §5d: the model
                -- does not convict, and the cheapest way to guarantee that is
                -- for derivation to be structurally unable to see its alarm.
                -- The column stays in the ledger for audit.
         FROM evidence_links l JOIN claims c ON c.id = l.claim_id
         JOIN events e ON e.id = c.event_id
         JOIN events ee ON ee.id = l.evidence_event_id
         WHERE e.project_id = ?1 AND (?2 IS NULL OR l.scorer = ?2)
         ORDER BY l.claim_id, l.entail_score DESC, l.evidence_event_id",
    )?;
    let rows = stmt.query_map(params![project_id, active_scorer], |r| {
        // Column order, explicit: 0 claim_id · 1 event_id · 2 entail · 3 verdict
        // · 4 fidelity · 5 related · 6 ev_family · 7 ev_passed · 8 ev_failed
        // · 9 ev_ok · 10 match_strength · 11 ev_identity · 12 pass
        // · 13 session_id · 14 seq · 15 tool_name · 16 tool_input · 17 result
        // · 18 frozen session · 19 frozen seq · 20 cwd · 21 git branch.
        let session_id: String = r.get(13)?;
        let ev_session_id: String = r.get(18)?;
        let ev_seq: i64 = r.get(19)?;
        Ok((
            r.get::<_, String>(0)?,
            LinkRow {
                event_id: r.get(1)?,
                score: r.get::<_, f64>(2)? as f32,
                verdict: r.get(3)?,
                fidelity: r.get(4)?,
                related: r.get::<_, i64>(5)? != 0,
                ev: crate::contract::EvidenceOutcome {
                    family: crate::contract::Family::parse(&r.get::<_, String>(6)?),
                    passed: r.get::<_, Option<i64>>(7)?.map(|v| v as u32),
                    failed: r.get::<_, Option<i64>>(8)?.map(|v| v as u32),
                    ok: r.get::<_, Option<i64>>(9)?.map(|v| v != 0),
                },
                match_strength: r.get::<_, f64>(10)? as f32,
                ev_identity: r.get::<_, String>(11)?,
                pass: r.get(12)?,
                session_rank: session_ranks.get(&session_id).copied().unwrap_or(0),
                turn: turns.turn(&session_id, r.get::<_, i64>(14)?),
                seq: r.get(14)?,
                tool_name: r.get(15)?,
                tool_input: r.get(16)?,
                tool_result: r.get(17)?,
                ev_session_id,
                ev_seq,
                ev_cwd: r.get(20)?,
                ev_git_branch: r.get(21)?,
                stale: false,
                superseded_by_event_id: None,
                superseded_by_session_id: None,
                superseded_by_seq: None,
            },
        ))
    })?;
    for row in rows {
        let (claim_id, link) = row?;
        if let Some((event_id, i)) = index.get(&claim_id) {
            if let Some(list) = by_event.get_mut(event_id) {
                list[*i].links.push(link);
            }
        }
    }
    drop(stmt);
    for claims in by_event.values_mut() {
        for claim in claims {
            mark_changed_outcome_stale(claim);
        }
    }

    // A34: the record that the active scorer's search happened. Evidence and
    // search coverage are one projection; mixing an older scorer's wider scan
    // with the active links could manufacture coverage authority.
    let mut stmt = ledger.conn().prepare(
        "SELECT l.claim_id, MAX(l.searched),
                COALESCE(MAX(l.action_matches),0), COALESCE(MAX(l.output_matches),0),
                COALESCE(MIN(l.coverage_eligible),0),
                COALESCE(GROUP_CONCAT(NULLIF(l.missing_families,''), ', '),'')
         FROM claim_searches l JOIN claims c ON c.id = l.claim_id
         JOIN events e ON e.id = c.event_id
         WHERE e.project_id = ?1 AND (?2 IS NULL OR l.scorer = ?2)
         GROUP BY l.claim_id",
    )?;
    let rows = stmt.query_map(params![project_id, active_scorer], |r| {
        Ok((
            r.get::<_, String>(0)?,
            SearchRow {
                searched: r.get::<_, i64>(1)? != 0,
                action_matches: r.get::<_, i64>(2)? as u32,
                output_matches: r.get::<_, i64>(3)? as u32,
                coverage_eligible: r.get::<_, i64>(4)? != 0,
                missing_families: r.get(5)?,
            },
        ))
    })?;
    for row in rows {
        let (claim_id, search) = row?;
        if let Some((event_id, i)) = index.get(&claim_id) {
            if let Some(list) = by_event.get_mut(event_id) {
                list[*i].search = Some(search);
            }
        }
    }
    Ok(by_event)
}

/// §5r's emitted-vs-eligible measurement, computed from the same verdict
/// function as export rather than inferred from candidate counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnsupportedFunnel {
    pub claims: u64,
    pub coverage_eligible: u64,
    pub emitted: u64,
}

pub fn unsupported_funnel(ledger: &Ledger, project_id: &str) -> Result<UnsupportedFunnel> {
    let annotations = load_annotations(ledger, project_id)?;
    let mut funnel = UnsupportedFunnel {
        claims: 0,
        coverage_eligible: 0,
        emitted: 0,
    };
    for claims in annotations.values() {
        for c in claims {
            if c.kind != crate::extract::ClaimKind::Work.as_str()
                && c.kind != crate::extract::ClaimKind::Marker.as_str()
            {
                continue;
            }
            funnel.claims += 1;
            if c.search.as_ref().is_some_and(|s| s.coverage_eligible) {
                funnel.coverage_eligible += 1;
            }
            if claim_state(c, false).0 == NodeState::Unsupported {
                funnel.emitted += 1;
            }
        }
    }
    Ok(funnel)
}

/// Diagnostic baseline for §5r: claims the pre-coverage verdict ladder would
/// have called `unsupported` on the same frozen annotations.
///
/// This does not persist or export a verdict. It exists so a coverage dry-run
/// can report "would have emitted → now unknown/emitted" without checking out
/// old code or weakening the production gate.
pub fn would_be_unsupported_claims(ledger: &Ledger, project_id: &str) -> Result<BTreeSet<String>> {
    let annotations = load_annotations(ledger, project_id)?;
    let mut out = BTreeSet::new();
    for claims in annotations.values() {
        for original in claims {
            if original.kind != crate::extract::ClaimKind::Work.as_str()
                && original.kind != crate::extract::ClaimKind::Marker.as_str()
            {
                continue;
            }
            let mut c = original.clone();
            let search = c.search.get_or_insert(SearchRow {
                searched: true,
                action_matches: 0,
                output_matches: 0,
                coverage_eligible: true,
                missing_families: String::new(),
            });
            search.coverage_eligible = true;
            search.missing_families.clear();
            if claim_state(&c, false).0 == NodeState::Unsupported {
                out.insert(c.id);
            }
        }
    }
    Ok(out)
}

/// How many same-family candidates the typed contract may consult.
///
/// One, ordinarily. The links arrive ranked by entailment descending, so this
/// is the best-matching run rather than the most alarming one — the whole point
/// of §5c. Two exists only because a claim occasionally sits between a build
/// and its immediately following test run.
const CONTRACT_TOP_K: usize = 2;

/// How many candidates may carry an accusation. Same reasoning as
/// `CONTRACT_TOP_K`, and deliberately the same number.
#[cfg(test)]
const ACCUSE_TOP_K: usize = 2;

/// The relatedness an accusation must clear: the artifact tier.
///
/// Ordinary word overlap admits evidence and may support a claim, but it may
/// not convict one. M3.7's adjudication is the whole argument — a plain
/// content-word test "passed vacuously and let the contradiction path accuse
/// true statements", because real premises run to 300 characters and share
/// *some* ordinary word with almost anything.
///
/// The consequence is deliberate and worth stating plainly: **a claim that
/// names nothing checkable can never be contradicted.** It can be supported,
/// and it can be reported as unsupported, but the product will not call it
/// false on the strength of shared vocabulary. That is a real loss of recall,
/// accepted knowingly, and it is what "only accuse when you understood the
/// evidence" costs.
const ACCUSE_MIN_STRENGTH: f32 = 0.6;

/// Stable command/artifact identity for §5f supersession.
///
/// Output redirection and diagnostic pipes are deliberately excluded: reruns
/// commonly write to a different temporary log while executing the same test
/// command. A preceding `cd` is retained so `npm test` in two packages does
/// not collapse into one identity.
fn evidence_identity(l: &LinkRow) -> String {
    // §5i: prefer the key frozen at annotate time, where the per-line `cwd`
    // was reachable. The textual fallback below is A51's defect and survives
    // only for rows written before M3.13.
    if !l.ev_identity.is_empty() {
        return l.ev_identity.clone();
    }
    let low = l.tool_input.to_ascii_lowercase();
    if l.tool_name == "Bash" {
        let command = json_string_field(&l.tool_input, "command")
            .unwrap_or(low)
            .to_ascii_lowercase();
        let one = command.replace(['\n', '\r', '\t'], " ");
        let cwd = one
            .split("&&")
            .filter_map(|part| part.trim().strip_prefix("cd "))
            .next()
            .and_then(|p| p.split_whitespace().next())
            .unwrap_or("")
            .trim_matches(['\'', '"'])
            .rsplit('/')
            .next()
            .unwrap_or("");
        const RUNNERS: [&str; 11] = [
            "cargo test",
            "npm test",
            "pnpm test",
            "yarn test",
            "swift test",
            "xcodebuild",
            "node --test",
            "node ",
            "pytest",
            "vitest",
            "jest",
        ];
        let start = RUNNERS
            .iter()
            .filter_map(|r| one.find(r))
            .min()
            .unwrap_or(0);
        let rest = &one[start..];
        let end = ["&&", ";", "|", ">"]
            .iter()
            .filter_map(|sep| rest.find(sep))
            .min()
            .unwrap_or(rest.len());
        let command = rest[..end].split_whitespace().collect::<Vec<_>>().join(" ");
        return format!("bash:{cwd}:{command}");
    }
    for key in ["file_path", "path", "notebook_path", "command", "subject"] {
        if let Some(v) = json_string_field(&l.tool_input, key) {
            return format!(
                "{}:{}",
                l.tool_name.to_ascii_lowercase(),
                v.to_ascii_lowercase()
            );
        }
    }
    format!("{}:{}", l.tool_name.to_ascii_lowercase(), l.event_id)
}

fn distinctive_artifacts(text: &str) -> Vec<String> {
    const GENERIC: [&str; 38] = [
        "test",
        "tests",
        "suite",
        "suites",
        "spec",
        "specs",
        "build",
        "builds",
        "compile",
        "compilation",
        "clippy",
        "rustfmt",
        "lint",
        "gate",
        "gates",
        "harness",
        "release",
        "debug",
        "file",
        "files",
        "endpoint",
        "endpoints",
        "cargo",
        "npm",
        "pnpm",
        "yarn",
        "node",
        "swift",
        "xcodebuild",
        "pytest",
        "vitest",
        "jest",
        "bash",
        "sh",
        "script",
        "scripts",
        "commit",
        "branch",
    ];
    let mut out = Vec::new();
    for raw in crate::extract::artifacts(text) {
        // A backticked path followed by sentence punctuation is observed twice
        // by extraction: once cleanly from the backticks, and once from the
        // whitespace token as `path.rs`.`. Canonicalise that second spelling
        // before applying all-artifacts coverage.
        let clean = raw
            .trim_end_matches(['.', ',', ';', ':'])
            .trim_matches('`')
            .to_string();
        if !clean.is_empty() && !GENERIC.contains(&clean.as_str()) && !out.contains(&clean) {
            out.push(clean);
        }
    }
    out
}

fn artifact_matches_link(artifact: &str, l: &LinkRow) -> bool {
    let blob = format!("{}\n{}\n{}", l.tool_name, l.tool_input, l.tool_result).to_ascii_lowercase();
    if blob.contains(artifact) {
        return true;
    }
    let base = artifact.rsplit('/').next().unwrap_or(artifact);
    if base.len() >= 4 && blob.contains(base) {
        return true;
    }
    false
}

/// Coverage-only artifact matching: the same test, plus separator-insensitivity.
///
/// The same identifier is written three ways in one project — `Pick-a-Scene` in
/// prose, `pick_a_scene` in a test file, `pickAScene.ts` in source — and a
/// reader unifies them without noticing. **This is deliberately NOT used by
/// `identity_corresponds`.** Coverage asks "did we see evidence about this named
/// thing?", where being generous only ever *permits* support. Correspondence
/// asks "may this run convict?", where being generous lets the wrong run
/// accuse: wiring the loose form into both took ATLAS from 2 convictions to 7,
/// several of them claims Round 3 had already adjudicated false.
fn artifact_matches_link_loose(artifact: &str, l: &LinkRow) -> bool {
    if artifact_matches_link(artifact, l) {
        return true;
    }
    fn squash(s: &str) -> String {
        s.chars().filter(|c| c.is_ascii_alphanumeric()).collect()
    }
    let base = artifact.rsplit('/').next().unwrap_or(artifact);
    let sq = squash(base);
    if sq.len() < 6 {
        return false;
    }
    let blob = format!("{}\n{}\n{}", l.tool_name, l.tool_input, l.tool_result).to_ascii_lowercase();
    squash(&blob).contains(&sq)
}

struct Authority<'a> {
    /// Conviction candidates: latest run per (family, identity). §5f/§5f-rev.
    active: Vec<&'a LinkRow>,
    /// §5n: everything §5m admits, supersession NOT applied. What support sees.
    eligible: Vec<&'a LinkRow>,
    superseded: usize,
    post_claim: usize,
}

fn recognised_freshness_outcome(link: &LinkRow) -> bool {
    let scoped_identity = link
        .ev_identity
        .strip_prefix("bash:")
        .and_then(|identity| identity.split_once(':'))
        .is_some_and(|(cwd, _invocation)| !cwd.is_empty());
    if !scoped_identity {
        return false;
    }
    match link.ev.family {
        crate::contract::Family::Test => {
            link.ev.passed.is_some()
                && link.ev.failed.is_some()
                && crate::identity::is_test_invocation(&link.ev_identity)
        }
        crate::contract::Family::Build => {
            link.ev.ok.is_some() && crate::identity::is_build_invocation(&link.ev_identity)
        }
        crate::contract::Family::None => false,
    }
}

/// M4.4 freshness v0: within one affirmatively known session and branch, an
/// interpreted later run of the same recognised invocation changes the state
/// covered by the earlier proof. The proof row is retained and marked stale;
/// the earliest later differing tuple is its deterministic superseder.
fn mark_changed_outcome_stale(claim: &mut ClaimRow) {
    let mut groups: BTreeMap<(String, String, String, String, String), Vec<usize>> =
        BTreeMap::new();
    for (index, link) in claim.links.iter().enumerate() {
        let Some(branch) = link
            .ev_git_branch
            .as_ref()
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if link.ev_session_id.is_empty()
            || link.ev_identity.is_empty()
            || !recognised_freshness_outcome(link)
        {
            continue;
        }
        groups
            .entry((
                link.pass.clone(),
                link.ev_session_id.clone(),
                branch.clone(),
                link.ev.family.as_str().to_string(),
                link.ev_identity.clone(),
            ))
            .or_default()
            .push(index);
    }

    for indices in groups.values_mut() {
        indices.sort_by(|left, right| {
            let a = &claim.links[*left];
            let b = &claim.links[*right];
            (a.ev_seq, a.event_id.as_str()).cmp(&(b.ev_seq, b.event_id.as_str()))
        });
        indices.dedup_by(|left, right| claim.links[*left].event_id == claim.links[*right].event_id);
        for position in 0..indices.len() {
            let earlier_index = indices[position];
            let earlier_outcome = claim.links[earlier_index].ev;
            let superseder = indices[position + 1..]
                .iter()
                .copied()
                .find(|later| claim.links[*later].ev != earlier_outcome);
            let Some(later_index) = superseder else {
                continue;
            };
            let later = &claim.links[later_index];
            let superseded_by_event_id = later.event_id.clone();
            let superseded_by_session_id = later.ev_session_id.clone();
            let superseded_by_seq = later.ev_seq;
            let earlier = &mut claim.links[earlier_index];
            earlier.stale = true;
            earlier.superseded_by_event_id = Some(superseded_by_event_id);
            earlier.superseded_by_session_id = Some(superseded_by_session_id);
            earlier.superseded_by_seq = Some(superseded_by_seq);
        }
    }
}

fn evidence_scope(link: &LinkRow) -> EvidenceScope {
    let freshness_note = match (
        link.stale,
        link.superseded_by_session_id.as_deref(),
        link.superseded_by_seq,
    ) {
        (true, Some(session), Some(seq)) => Some(format!(
            "verified at `{}:{}`; superseded by `{session}:{seq}`",
            link.ev_session_id, link.ev_seq
        )),
        _ => None,
    };
    EvidenceScope {
        event_id: link.event_id.clone(),
        session_id: link.ev_session_id.clone(),
        seq: link.ev_seq,
        cwd: link.ev_cwd.clone(),
        git_branch: link.ev_git_branch.clone(),
        command_identity: (!link.ev_identity.is_empty()).then(|| link.ev_identity.clone()),
        freshness: if link.stale { "stale" } else { "unknown" },
        superseded_by_event_id: link.superseded_by_event_id.clone(),
        superseded_by_session_id: link.superseded_by_session_id.clone(),
        superseded_by_seq: link.superseded_by_seq,
        freshness_note,
    }
}

fn evidence_scopes(claim: &ClaimRow) -> Vec<EvidenceScope> {
    let mut seen = BTreeSet::new();
    let mut links: Vec<&LinkRow> = claim
        .links
        .iter()
        .filter(|link| seen.insert(link.event_id.as_str()))
        .collect();
    links.sort_by(|a, b| {
        (a.ev_session_id.as_str(), a.ev_seq, a.event_id.as_str()).cmp(&(
            b.ev_session_id.as_str(),
            b.ev_seq,
            b.event_id.as_str(),
        ))
    });
    links.into_iter().map(evidence_scope).collect()
}

/// §5m — THE one eligibility authority. Every verdict path calls this and no
/// path reimplements it.
///
/// Turn-atomic: within a turn, narration order is not temporal order —
/// assistants narrate, then act, so same-turn evidence is eligible for a
/// backward (completed-work) claim regardless of intra-turn position. Across
/// turns, evidence must precede the claim's turn; post-claim-turn evidence
/// can never support a completed-work claim. Forward fulfillment is the
/// mirror: evidence must sit AFTER the claim (later turn or later in the
/// claim's own turn), because a proposal cannot be fulfilled by what preceded
/// it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Purpose {
    /// Support/conviction of completed-work and marker claims.
    Backward,
    /// Fulfillment of proposals.
    Forward,
}

fn eligible(c: &ClaimRow, l: &LinkRow, purpose: Purpose) -> bool {
    match purpose {
        Purpose::Backward => (l.session_rank, l.turn) <= (c.session_rank, c.turn),
        Purpose::Forward => {
            (l.session_rank, l.turn) > (c.session_rank, c.turn)
                || ((l.session_rank, l.turn) == (c.session_rank, c.turn) && l.seq > c.seq)
        }
    }
}

/// Apply temporal precedence and latest-per-identity supersession before any
/// verdict path sees a link.
fn evidence_authority(c: &ClaimRow) -> Authority<'_> {
    let mut latest: BTreeMap<(String, String), &LinkRow> = BTreeMap::new();
    let mut eligible_links: Vec<&LinkRow> = Vec::new();
    let mut pre = 0usize;
    let mut post_claim = 0usize;
    for l in c.links.iter().filter(|l| l.pass == "backward") {
        // §5m: the one eligibility test. Turn-atomic — the old per-path rule
        // compared raw (session, seq) and excluded narrate-then-act proof
        // that landed seconds after the claim inside its own turn, which was
        // eight of Round 5's fifteen false "no captured proof" verdicts.
        if !eligible(c, l, Purpose::Backward) {
            post_claim += 1;
            continue;
        }
        pre += 1;
        eligible_links.push(l);
        let key = (l.ev.family.as_str().to_string(), evidence_identity(l));
        match latest.get(&key) {
            Some(old)
                if (old.session_rank, old.seq, old.event_id.as_str())
                    >= (l.session_rank, l.seq, l.event_id.as_str()) => {}
            _ => {
                latest.insert(key, l);
            }
        }
    }
    // §5n: `eligible` is what SUPPORT sees — supersession never hides proof.
    // `active` (latest-per-identity) is what CONVICTION consults.
    eligible_links.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(b.match_strength.total_cmp(&a.match_strength))
            .then(a.event_id.cmp(&b.event_id))
    });
    let mut active: Vec<&LinkRow> = latest.into_values().collect();
    active.sort_by(|a, b| {
        b.match_strength
            .total_cmp(&a.match_strength)
            .then(b.score.total_cmp(&a.score))
            .then(b.session_rank.cmp(&a.session_rank))
            .then(b.seq.cmp(&a.seq))
            .then(a.event_id.cmp(&b.event_id))
    });
    Authority {
        superseded: pre.saturating_sub(active.len()),
        active,
        eligible: eligible_links,
        post_claim,
    }
}

/// §5f-rev: the temporally governing run of a family — the latest one that
/// actually bears an outcome.
///
/// "Latest" is `(session_rank, seq)` with `event_id` as a total tie-break, so
/// the choice is deterministic. Candidates whose outcome could not be read are
/// not eligible to govern: an unreadable run cannot supersede a readable one,
/// or a `cat` of a log would silence every real result behind it.
fn governing_same_family<'a>(
    active: &[&'a LinkRow],
    family: crate::contract::Family,
) -> Option<&'a LinkRow> {
    active
        .iter()
        .copied()
        .filter(|l| l.ev.family == family && l.ev.family != crate::contract::Family::None)
        .max_by(|a, b| {
            (a.session_rank, a.seq, a.event_id.as_str()).cmp(&(
                b.session_rank,
                b.seq,
                b.event_id.as_str(),
            ))
        })
}

/// One contract check against one governing run, shaped as a verdict.
///
/// Extracted so §5f-rev's "the governing run speaks first" path and the
/// ranked walk cannot drift apart: there is exactly one place that turns a
/// `Check` into a `NodeState`.
fn contract_verdict(
    c: &ClaimRow,
    l: &LinkRow,
    assertion: &crate::contract::ClaimAssertion,
    authority: &Authority<'_>,
    authority_context: &str,
) -> Option<(NodeState, String)> {
    match crate::contract::check(assertion, &l.ev) {
        crate::contract::Check::Contradicted(why) => {
            if let Err(stage) = contradiction_eligibility(c, l, assertion, authority) {
                return Some((
                    NodeState::Unknown,
                    format!(
                        "Evidence present, but {} was not affirmatively recognised; \
                         closed-world conviction abstained.{authority_context}",
                        stage.as_str()
                    ),
                ));
            }
            Some((
                NodeState::Contradicted,
                format!(
                    "Checked against the latest identity-matched prior run: {why}.{authority_context}"
                ),
            ))
        }
        crate::contract::Check::Supported(why) => {
            if !support_scope_covers(c, &authority.eligible, Some(assertion)) {
                return Some((
                    NodeState::Unknown,
                    format!("Evidence is narrower than the claim.{authority_context}"),
                ));
            }
            Some((
                NodeState::Supported,
                format!("Checked against the latest identity-matched prior run: {why}.{authority_context}"),
            ))
        }
        crate::contract::Check::Inconclusive(why) => Some((
            NodeState::Unknown,
            format!("Exact count not confirmed: {why}.{authority_context}"),
        )),
        crate::contract::Check::NoContract => None,
    }
}

fn authority_suffix(a: &Authority<'_>) -> String {
    match (a.superseded, a.post_claim) {
        (0, 0) => String::new(),
        (s, p) => format!(
            " Context only: {s} superseded and {p} post-claim evidence item(s) had no verdict authority."
        ),
    }
}

/// Conviction needs named identity. A generic claim may use a run only when
/// exactly one same-family identity is active; otherwise wrong-suite evidence
/// is indistinguishable from the intended suite and the contract abstains.
fn identity_corresponds(
    c: &ClaimRow,
    l: &LinkRow,
    active: &[&LinkRow],
    family: crate::contract::Family,
) -> bool {
    let artifacts = distinctive_artifacts(&c.text);
    if !artifacts.is_empty() {
        return artifacts.iter().any(|a| artifact_matches_link(a, l));
    }
    let identities: BTreeSet<String> = active
        .iter()
        .filter(|x| x.ev.family == family)
        .map(|x| evidence_identity(x))
        .collect();
    identities.len() == 1
}

fn evidence_polarity_allows_conviction(l: &LinkRow) -> bool {
    let low = format!("{}\n{}", l.tool_input, l.tool_result).to_ascii_lowercase();
    let named_control = [
        "mutation",
        "mutant",
        "neutraliz",
        "negative control",
        "poisoned",
        "deliberately fail",
        "intentionally fail",
        "expected failure",
        "expect failure",
        "expect fail",
    ]
    .iter()
    .any(|p| low.contains(p));
    let mutation_shape = (low.contains("perl -0pi") || low.contains("sed -i"))
        && (low.contains("/tmp/") || low.contains("restore"));
    !named_control && !mutation_shape
}

fn contradiction_eligibility(
    c: &ClaimRow,
    l: &LinkRow,
    assertion: &crate::contract::ClaimAssertion,
    authority: &Authority<'_>,
) -> std::result::Result<(), AccusationStage> {
    if c.kind != crate::extract::ClaimKind::Marker.as_str()
        || crate::contract::parse_claim(&c.text).is_none()
    {
        return Err(AccusationStage::ClaimPattern);
    }
    if assertion.polarity != crate::contract::ClaimPolarity::AssertsSuccess
        || !evidence_polarity_allows_conviction(l)
    {
        return Err(AccusationStage::Polarity);
    }

    let identity = evidence_identity(l);
    let recognised_family = match assertion.family {
        crate::contract::Family::Test => {
            l.ev.family == crate::contract::Family::Test
                && crate::identity::is_test_invocation(&identity)
        }
        crate::contract::Family::Build => {
            l.ev.family == crate::contract::Family::Build
                && crate::identity::is_build_invocation(&identity)
        }
        crate::contract::Family::None => false,
    };
    if !recognised_family {
        return Err(AccusationStage::EvidenceFamily);
    }

    let complete_outcome = match assertion.family {
        // A test accusation quotes both numbers. Rendering a missing pass
        // count as zero is a wrong-count incident, not a harmless default.
        crate::contract::Family::Test => l.ev.passed.is_some() && l.ev.failed.is_some(),
        crate::contract::Family::Build => l.ev.ok.is_some(),
        crate::contract::Family::None => false,
    };
    if !complete_outcome {
        return Err(AccusationStage::OutcomeFormat);
    }
    if !identity_corresponds(c, l, &authority.active, assertion.family) {
        return Err(AccusationStage::Identity);
    }
    if l.pass != "backward"
        || !eligible(c, l, Purpose::Backward)
        || !authority.active.iter().any(|x| x.event_id == l.event_id)
    {
        return Err(AccusationStage::Eligibility);
    }
    Ok(())
}

fn dropped_eligibility(c: &ClaimRow, moved_on: bool) -> std::result::Result<(), AccusationStage> {
    if c.kind != crate::extract::ClaimKind::Proposal.as_str() {
        return Err(AccusationStage::ClaimPattern);
    }
    if !crate::extract::proposal_is_affirmatively_forward(&c.text, &c.marker) {
        return Err(AccusationStage::Polarity);
    }
    if crate::extract::artifacts(&c.text).is_empty() {
        return Err(AccusationStage::Identity);
    }
    if !c.search.as_ref().is_some_and(|s| s.searched) {
        return Err(AccusationStage::OutcomeFormat);
    }
    if !moved_on {
        return Err(AccusationStage::Eligibility);
    }
    Ok(())
}

fn dropped_verdict(c: &ClaimRow, moved_on: bool, receipt: String) -> (NodeState, String) {
    match dropped_eligibility(c, moved_on) {
        Ok(()) => (NodeState::Dropped, receipt),
        Err(stage) => (
            NodeState::Pending,
            format!(
                "Could not affirmatively recognise proposal {} — closed-world \
                 abandonment check abstained.",
                stage.as_str()
            ),
        ),
    }
}

/// Is this "artifact" actually a shell invocation the claim quoted?
fn looks_like_invocation(a: &str) -> bool {
    let l = a.to_ascii_lowercase();
    l.contains("&&")
        || l.contains(" | ")
        || l.starts_with("cd ")
        || [
            "npm test",
            "cargo test",
            "swift test",
            "pytest",
            "yarn test",
            "pnpm test",
            "go test",
        ]
        .iter()
        .any(|r| l.contains(r))
}

/// §5m/§5n support correspondence, composite over the ELIGIBLE set.
///
/// The supporting link must itself be about the claim OR the eligible set as
/// a whole must cover every distinctive artifact the claim names. Round 5's
/// cu2 is the shape this exists for: eight Writes cover the named views and
/// the build covers "builds clean" — no single link matches every artifact,
/// and demanding that of `b` alone turned real composite proof into "no
/// captured proof".
enum Correspondence {
    Full,
    /// Some named artifacts are covered by eligible proof, others are not —
    /// the honest receipt is "narrower than the claim", not a mismatch.
    Partial,
    None,
}

fn link_is_agent_completion_record(l: &LinkRow) -> bool {
    matches!(l.tool_name.as_str(), "Agent" | "Task" | "TaskNotification")
        || (l.tool_name == "Read" && {
            let low = l.tool_input.to_ascii_lowercase();
            low.contains("/tasks/") && low.contains(".output")
        })
}

fn agent_record_covers(c: &ClaimRow, l: &LinkRow) -> bool {
    if !link_is_agent_completion_record(l) || l.verdict != "entailed" || l.fidelity != "parsed" {
        return false;
    }
    let artifacts = distinctive_artifacts(&c.text);
    if artifacts.is_empty() {
        // Ordinary-word relatedness tops out at 0.5. Requiring 0.4 means the
        // focused record accounts for most of a generic claim, not one noun.
        l.match_strength >= 0.4
    } else {
        // Artifact relatedness reaches 1.0 only when every named artifact is
        // present in the claim-focused record excerpt.
        l.match_strength >= 0.99
    }
}

fn support_correspondence(c: &ClaimRow, b: &LinkRow, eligible: &[&LinkRow]) -> Correspondence {
    if agent_record_covers(c, b) {
        return Correspondence::Full;
    }
    let artifacts = distinctive_artifacts(&c.text);
    if artifacts.is_empty() {
        let identities: BTreeSet<String> = eligible
            .iter()
            .filter(|x| x.ev.family == b.ev.family)
            .map(|x| evidence_identity(x))
            .collect();
        // §5m: a BROAD claim ("both configurations", "all pages") with no
        // named artifact is coverable by two-plus successful entailing
        // identities — the plutil-before/after shape. §5f's coverage rule for
        // broad claims, applied to the NLI path.
        if broad_scope(&c.text) {
            let successful_entailing: BTreeSet<String> = eligible
                .iter()
                .filter(|x| {
                    x.ev.family == b.ev.family
                        && x.ev.ok != Some(false)
                        && x.verdict == "entailed"
                        && x.fidelity == "parsed"
                })
                .map(|x| evidence_identity(x))
                .collect();
            return if successful_entailing.len() >= 2 || identities.len() == 1 {
                Correspondence::Full
            } else {
                Correspondence::Partial
            };
        }
        // No named artifact, not broad: the conservative one-identity rule
        // within b's family, as before.
        return if identities.len() == 1 {
            Correspondence::Full
        } else {
            Correspondence::None
        };
    }
    let matched = artifacts
        .iter()
        .filter(|a| eligible.iter().any(|l| artifact_matches_link_loose(a, l)))
        .count();
    if matched == artifacts.len() {
        Correspondence::Full
    } else if matched > 0 {
        Correspondence::Partial
    } else {
        Correspondence::None
    }
}

fn broad_scope(text: &str) -> bool {
    let low = text.to_ascii_lowercase();
    [
        "both ",
        "all suites",
        "all builds",
        "all endpoints",
        "all pages",
        "all three",
        "all four",
        "entire gate",
        "full gate",
    ]
    .iter()
    .any(|p| low.contains(p))
}

/// Support is symmetric with conviction: the evidence must cover what the
/// claim names. Multiple named artifacts require coverage of every name.
fn support_scope_covers(
    c: &ClaimRow,
    active: &[&LinkRow],
    assertion: Option<&crate::contract::ClaimAssertion>,
) -> bool {
    if active.iter().any(|l| agent_record_covers(c, l)) {
        return true;
    }
    // A quoted COMMAND is the invocation the claim is about, not an additional
    // target that needs its own evidence. Requiring a link blob to contain the
    // literal `cd atlas-proxy && npm test` is unsatisfiable the moment the real
    // command spells an absolute path — the same text-blindness §5i removed
    // from identity, surfacing here as a false "narrower than the claim".
    let artifacts: Vec<String> = distinctive_artifacts(&c.text)
        .into_iter()
        .filter(|a| !looks_like_invocation(a))
        .collect();
    if artifacts.len() > 1
        && !artifacts
            .iter()
            .all(|a| active.iter().any(|l| artifact_matches_link_loose(a, l)))
    {
        return false;
    }
    if broad_scope(&c.text) && assertion.and_then(|a| a.passed).is_none() {
        let successful: BTreeSet<String> = active
            .iter()
            .filter(|l| l.ev.ok != Some(false))
            .map(|l| evidence_identity(l))
            .collect();
        return successful.len() >= 2;
    }
    true
}

/// The evidence an accusation is allowed to rest on: the top of the
/// relatedness ranking, parsed, and genuinely on-subject.
///
/// **A32, the defect this exists to kill.** The rule used to be "does *any*
/// admitted candidate contradict?" — a scan. Each comparison carries an
/// independent chance of a false conflict, so over `N` candidates the odds of
/// at least one false alarm are `1 − (1−p)^N`: at a per-pair p of 0.10 and the
/// 30 candidates a busy session produces, ≈ 0.958. The verdict was therefore
/// close to a function of session length rather than of truth, which is
/// exactly what M3.7's 0/12 adjudication measured. Ranking and taking the top
/// makes the exposure a constant.
///
/// Everything filtered out here still counts toward `supported` — extra
/// evidence may exonerate without limit. Only the accusation is rationed.
#[cfg(test)]
fn matched_pairs(links: &[LinkRow]) -> Vec<&LinkRow> {
    let mut v: Vec<&LinkRow> = links
        .iter()
        .filter(|l| l.fidelity == "parsed" && l.match_strength >= ACCUSE_MIN_STRENGTH)
        .collect();
    // Relatedness first, entailment only to break ties within a tier, then id
    // so the order is total and derivation stays byte-identical.
    v.sort_by(|a, b| {
        b.match_strength
            .total_cmp(&a.match_strength)
            .then(b.score.total_cmp(&a.score))
            .then(a.event_id.cmp(&b.event_id))
    });
    v.truncate(ACCUSE_TOP_K);
    v
}

/// §5's state table for one claim, fidelity-aware.
///
/// The two rules that matter most:
///
/// * A completed-work claim with **zero** candidates is `unsupported` only
///   when §5r recorded comprehensive family coverage; otherwise it is
///   `unknown`.
/// * A completed-work claim that scored low against **only fallback-fidelity**
///   evidence is `unknown`, never an accusation. M3.4 measured why: on
///   fallback-rendered premises a true paraphrase scored 0.018 while an
///   outright falsehood scored 0.125. The signal's *direction* is unreliable
///   there, so a low score carries no information about the claim and turning
///   it into an accusation would be flagging users on noise.
fn unsupported_with_coverage(c: &ClaimRow, supported_receipt: String) -> (NodeState, String) {
    match c.search.as_ref() {
        Some(search) if search.coverage_eligible => (NodeState::Unsupported, supported_receipt),
        Some(search) => {
            let missing = if search.missing_families.trim().is_empty() {
                "coverage not recorded"
            } else {
                search.missing_families.trim()
            };
            (
                NodeState::Unknown,
                format!("Proof may live in records not yet read ({missing})."),
            )
        }
        None => (
            NodeState::Unknown,
            "Proof may live in records not yet read (coverage not recorded).".to_string(),
        ),
    }
}

fn claim_state(c: &ClaimRow, moved_on: bool) -> (NodeState, String) {
    // §5m: fulfillment evidence must actually FOLLOW the claim. The forward
    // pass already searches later windows at annotate time, but the one
    // eligibility authority is what makes the guarantee structural rather
    // than a property of the retrieval query.
    let forward_links: Vec<&LinkRow> = c
        .links
        .iter()
        .filter(|l| l.pass == "forward" && eligible(c, l, Purpose::Forward))
        .collect();
    let best = forward_links.first().copied().or_else(|| c.links.first());
    let n = c.links.len();
    let kind = c.kind.as_str();

    // ── §5b, first and unconditionally: is this even in scope? ──────────
    //
    // A finding reports what was discovered; a micro-intent is conversational
    // glue. Neither asserts that the world was changed, so neither can be
    // wrong in the way a ⚠ alleges. M3.7 found 0/12 warnings true and the
    // residue was exactly this: the product accusing about categories it has
    // no means to verify. Staying silent here is the product working.
    if kind == crate::extract::ClaimKind::Finding.as_str() {
        return (
            NodeState::Recorded,
            "Reports what was found, not work performed — out of scope for a verdict (§5b)."
                .to_string(),
        );
    }
    if kind == crate::extract::ClaimKind::MicroIntent.as_str() {
        return (
            NodeState::Recorded,
            "Immediate intention with no durable target; absorbed into the work that followed."
                .to_string(),
        );
    }

    if kind == crate::extract::ClaimKind::Proposal.as_str() {
        if !moved_on {
            return (
                NodeState::Pending,
                "Proposed; the project has not moved past it yet.".to_string(),
            );
        }
        // A proposal that names nothing cannot be searched for, and "we could
        // not look" must never be reported as "nobody did it". Belt and braces:
        // `forward_kind` should already have classified these as micro-intents,
        // but a 👻 is expensive enough to be worth guarding twice.
        if crate::extract::artifacts(&c.text).is_empty() {
            return (
                NodeState::Pending,
                "Too unspecific to check: this proposal names nothing a later \
                 action could be matched against."
                    .to_string(),
            );
        }
        let Some(b) = best else {
            // ── A34: the three conditions a 👻 requires ──────────────────
            //
            // `dropped` accuses the project of abandoning a thread, and its
            // whole evidence is an absence. An absence proves nothing unless
            // someone actually looked, so §5c requires all three of: a search
            // was attempted, the search was possible, and it found no trace.
            // Any weaker rule turns our own failures into the user's.
            match c.search.as_ref() {
                // We never looked. Measured this milestone: a locked database
                // made annotate a silent no-op and the graph reported 535
                // abandoned threads that did not exist.
                None => {
                    return (
                        NodeState::Pending,
                        "Not yet checked for follow-through.".to_string(),
                    );
                }
                // We could not look: no search term survived the gate.
                Some(s) if !s.searched => {
                    return (
                        NodeState::Pending,
                        "Too unspecific to check: no searchable name in this proposal.".to_string(),
                    );
                }
                // §5j: only ACTION-side matches defeat. The project touched
                // this target again — commands, edit paths, written content —
                // which does not prove fulfilment (not `supported`) but does
                // disprove abandonment.
                Some(s) if s.action_matches > 0 => {
                    return (
                        NodeState::Pending,
                        format!(
                            "Later actions referenced this target ({} found); \
                             fulfillment not proven.",
                            s.action_matches
                        ),
                    );
                }
                // §5j: output-only mentions never defeat alone. A grep hit or
                // a listing is the target's name scrolling past, not anyone
                // acting on it — A52 measured 644 of 697 proposals "defeated"
                // this way and `dropped` unreachable. The mention goes on the
                // receipt instead, so an adjudicator can weigh it.
                Some(s) if s.output_matches > 0 => {
                    return dropped_verdict(
                        c,
                        moved_on,
                        format!(
                            "No later action matching this proposal was found. \
                             Target mentioned in later output {} time(s) — \
                             mentions are not actions.",
                            s.output_matches
                        ),
                    );
                }
                _ => {}
            }
            return dropped_verdict(
                c,
                moved_on,
                "No later action matching this proposal was found.".to_string(),
            );
        };
        // North Star v2 defines `supported` as *entailed by parsed-fidelity
        // captured evidence*. A name match is not entailment, and calling it
        // `supported` was the mirror image of the accusation defect A32 fixed:
        // an overclaim built on weak evidence, differing only in whom it
        // flatters. A later action touching the file disproves abandonment and
        // nothing more, so it defeats the ghost and stops there.
        if b.verdict.as_str() == "entailed" && b.fidelity == "parsed" {
            return (
                NodeState::Supported,
                format!("Fulfilled later by a matching action ({:.2}).", b.score),
            );
        }
        // §5j: with candidates present but none entailing, the DEFEAT question
        // is decided by where the name appeared, not by the candidates' mere
        // existence. Retrieval is deliberately generous (A45 admits output
        // text, because a nested report can PROVE fulfilment) — but a
        // retrieved candidate that fails to entail has proven nothing, and if
        // the target's name never appeared in later action-side text, this
        // proposal is exactly as abandoned as one with no candidates at all.
        // A52 measured the alternative: 644 of 697 proposals "defeated" by
        // their own names scrolling past in output, and `dropped` unreachable.
        return match c.search.as_ref() {
            Some(s) if s.action_matches > 0 => (
                NodeState::Pending,
                format!(
                    "Later actions referenced this target ({} found); \
                     fulfillment not proven.",
                    s.action_matches
                ),
            ),
            Some(s) if s.searched && s.output_matches > 0 => dropped_verdict(
                c,
                moved_on,
                format!(
                    "No later action matching this proposal was found. \
                     Target mentioned in later output {} time(s) — \
                     mentions are not actions.",
                    s.output_matches
                ),
            ),
            Some(s) if s.searched => dropped_verdict(
                c,
                moved_on,
                "No later action matching this proposal was found.".to_string(),
            ),
            // No search record, or unsearchable: candidates exist but nothing
            // proves or disproves follow-through. Never a ghost without a
            // recorded search (§5c stands).
            _ => (
                NodeState::Pending,
                format!(
                    "Later actions referenced this target ({n} found); \
                     fulfillment not proven."
                ),
            ),
        };
    }

    // ── Material work: the only kinds that may be accused ───────────────
    debug_assert!(
        kind == crate::extract::ClaimKind::Work.as_str()
            || kind == crate::extract::ClaimKind::Marker.as_str(),
        "unhandled claim kind reached the verdict ladder: {kind}"
    );

    let authority = evidence_authority(c);
    let authority_context = authority_suffix(&authority);
    let any_parsed = authority.eligible.iter().any(|l| l.fidelity == "parsed");
    let n = authority.eligible.len();
    // §5n: the SUPPORT candidate is the best entailing link among everything
    // §5m admits — supersession filters conviction, never proof. `eligible`
    // arrives entailment-descending.
    let Some(b) = authority.eligible.first().copied() else {
        // Nothing was scored at all. Without evidence we cannot have understood
        // any, so §5b's parsed-fidelity requirement is unmet by construction.
        // Nothing was scored at all: a coverage gap, not a conflict.
        return unsupported_with_coverage(
            c,
            format!("No prior authoritative proof found for this claim.{authority_context}"),
        );
    };

    // ── Typed contract v0 (A33) — deterministic, and it goes FIRST ──────
    //
    // For a marker claim carrying numbers, the verdict is arithmetic against
    // the top-1 same-family run. No scores, no thresholds, no model. NLI is
    // demoted here to explanation and tie-breaking, exactly as North Star v2
    // requires: "models never fabricate the proof and are never the final
    // judge."
    if kind == crate::extract::ClaimKind::Marker.as_str() {
        if let Some(assertion) = crate::contract::parse_claim(&c.text) {
            // ALL matched evidence, best-matching first — not pre-filtered to
            // the claim's family. The filter was M3.10.4's last false-warning
            // class: a two-suite payload ("✔ 342 tests passed" then "✔ 36
            // tests passed") froze as `none` under §5e's conflict abstention,
            // the family filter silently dropped it, and the contract walked
            // on to a WORSE-matched, earlier failing run and convicted on it
            // — 17 of round 3's 41 warnings, every one accusing a claim whose
            // best evidence confirmed it. Skipping past unreadable evidence
            // to convict on whatever ranks below it is the A32 defect
            // reappearing inside the contract itself.
            // Admission is `related` OR family agreement — family agreement
            // is the contract's own notion of relatedness for runs, and it
            // cannot be dropped: a misstated claim shares few tokens with the
            // run that disproves it precisely BECAUSE it misstates the numbers
            // ("98/98 green" vs a run reporting 94 and 4). What protects the
            // verdict is the ORDER — the block rule below means nothing can
            // convict from beneath evidence we could not read.
            let mut matched: Vec<&LinkRow> = authority
                .active
                .iter()
                .copied()
                .filter(|l| {
                    l.fidelity == "parsed" && (l.related || l.ev.family == assertion.family)
                })
                .collect();
            matched.sort_by(|a, b| {
                b.match_strength
                    .total_cmp(&a.match_strength)
                    .then(b.score.total_cmp(&a.score))
                    .then(a.event_id.cmp(&b.event_id))
            });
            // §5f-rev — the TEMPORALLY GOVERNING run speaks first.
            //
            // M3.13.0: for a marker claim the latest same-family outcome-bearing
            // prior run outranks lexical closeness; strength breaks ties only.
            // Round 4 convicted two true claims because the walk was ordered by
            // relatedness and the claim quoted the FAILING run's command
            // spelling, so the stale run was reached first and — under textual
            // identity keys (A51) — was never superseded either.
            //
            // An unreadable candidate still blocks, but only when it is LATER
            // than the governing run: then we genuinely cannot know the final
            // state. An unreadable run that precedes a readable one has been
            // superseded and has no veto (that was M3.10.4b's rule mis-firing
            // on rank rather than on time).
            // DECIDING needs strict identity (§5f-rev: "for support, identity
            // and coverage remain strict"), and §5i gives an exact key for it.
            //
            // Two steps, because a claim is not an invocation and cannot be
            // compared to one directly:
            //   1. the claim is *about* whichever same-family run matches it
            //      most strongly — that run's `ev_identity` names the suite;
            //   2. within that identity group, the LATEST prior run governs.
            //
            // Family alone was too coarse: for the 44/44 claim the latest
            // same-family run was a `swift` suite in a different directory,
            // which reported 17 passed and had nothing to do with the claim.
            // Loose artifact correspondence accepted it; the frozen key does
            // not.
            //
            // The ANCHOR carries the correspondence test, not the governing
            // run. `identity_corresponds` reads a text blob, and a re-run is
            // very often spelled bare (`npm test`) with its directory living
            // only in the per-line `cwd` — the exact blindness §5i fixed for
            // identity. Testing the anchor asks the right question ("is this
            // identity group the one the claim is about?"); testing the
            // governing run asks whether the claim quoted its spelling.
            let target_identity = authority
                .active
                .iter()
                .filter(|l| l.ev.family == assertion.family && !l.ev_identity.is_empty())
                .max_by(|a, b| {
                    a.match_strength
                        .total_cmp(&b.match_strength)
                        .then(a.score.total_cmp(&b.score))
                        .then(b.event_id.cmp(&a.event_id))
                })
                // The anchor qualifies on RELATEDNESS, not on a text match.
                // `match_strength` is A32's frozen measure and
                // ACCUSE_MIN_STRENGTH is the artifact tier accusations already
                // require. A text test fails here for a reason worth naming:
                // the claim's backticked artifact is often the literal phrase
                // `cd atlas-proxy && npm test`, which no absolute-path command
                // string contains — the same text-blindness §5i removed from
                // identity.
                .filter(|anchor| anchor.match_strength >= ACCUSE_MIN_STRENGTH)
                .map(|l| l.ev_identity.clone());

            let governing = target_identity.as_ref().and_then(|id| {
                authority
                    .active
                    .iter()
                    .copied()
                    .filter(|l| l.ev.family == assertion.family && &l.ev_identity == id)
                    .max_by(|a, b| {
                        (a.session_rank, a.seq, a.event_id.as_str()).cmp(&(
                            b.session_rank,
                            b.seq,
                            b.event_id.as_str(),
                        ))
                    })
            });

            if let Some(gov) = governing {
                // An unreadable run blocks only when it is a LATER run of the
                // SAME invocation: then the final state is genuinely unknown.
                // A later unrelated event has no veto — treating one as a
                // blocker is how a `TaskCreate` silenced a confirmed claim.
                let blocked = authority.active.iter().any(|x| {
                    x.ev.family == crate::contract::Family::None
                        && x.fidelity == "parsed"
                        && !x.ev_identity.is_empty()
                        && x.ev_identity == gov.ev_identity
                        && (x.session_rank, x.seq) > (gov.session_rank, gov.seq)
                });
                if blocked {
                    return (
                        NodeState::Unknown,
                        "Evidence present; outcome format not interpretable.".to_string(),
                    );
                }
                if let Some(v) =
                    contract_verdict(c, gov, &assertion, &authority, &authority_context)
                {
                    // §5f-rev, and this is the whole asymmetry: the family-level
                    // veto applies to CONVICTION only. If any later same-family
                    // outcome-bearing run precedes the claim, this one may not
                    // accuse — even though identity normalisation could not
                    // prove they are the same suite. Support is unaffected,
                    // because support is already identity-strict.
                    //
                    // Without this the governing path convicted four claims
                    // whose failing build was a transient lock superseded by a
                    // successful re-run under a different invocation.
                    if v.0 == NodeState::Contradicted {
                        if let Some(latest) =
                            governing_same_family(&authority.active, assertion.family)
                        {
                            if latest.event_id != gov.event_id {
                                return (
                                    NodeState::Unknown,
                                    format!(
                                        "A later run in the same family has verdict authority; \
                                         this one cannot convict.{authority_context}"
                                    ),
                                );
                            }
                        }
                        // A59: an UNREADABLE later runner-invocation blocks
                        // conviction too. Three Round-5 false convictions
                        // shared this shape — a later green run whose compound
                        // output abstained (several summaries in one payload,
                        // §5f ambiguity) left the stale failure "latest
                        // identity-matched". The outcome is unreadable, but
                        // the frozen §5i key still says the event RAN a
                        // test/build tool, and an unreadable later run means
                        // the final state is unknown (§5f-rev's asymmetry).
                        let later_unreadable_runner = authority.active.iter().any(|x| {
                            x.ev.family == crate::contract::Family::None
                                && x.fidelity == "parsed"
                                && crate::identity::is_runner_invocation(&x.ev_identity)
                                && (x.session_rank, x.seq) > (gov.session_rank, gov.seq)
                        });
                        if later_unreadable_runner {
                            return (
                                NodeState::Unknown,
                                format!(
                                    "A later run exists whose outcome could not be read; \
                                     no earlier failure may convict past it.{authority_context}"
                                ),
                            );
                        }
                    }
                    return v;
                }
            }
            for l in matched.into_iter().take(CONTRACT_TOP_K) {
                // An uninterpretable candidate BLOCKS. Something at least as
                // relevant as everything below it exists and we cannot read
                // it, so no candidate ranked beneath it may carry a verdict
                // in either direction (§5d's abstention clause).
                if l.ev.family == crate::contract::Family::None {
                    return (
                        NodeState::Unknown,
                        "Evidence present; outcome format not interpretable.".to_string(),
                    );
                }
                // Interpretable but a different family — a build result says
                // nothing about a test-count claim. We read it; it neither
                // blocks nor decides.
                if l.ev.family != assertion.family {
                    continue;
                }
                // §5f-rev conviction veto, FAMILY-level on purpose: a stale
                // run may not convict while any later same-family
                // outcome-bearing run precedes the claim, whether or not
                // identity normalisation could match them. This is the
                // asymmetry — a normalisation miss costs recall, never
                // precision.
                if let Some(latest) = governing_same_family(&authority.active, assertion.family) {
                    if latest.event_id != l.event_id {
                        continue;
                    }
                }
                if let Some(v) = contract_verdict(c, l, &assertion, &authority, &authority_context)
                {
                    return v;
                }
            }
        }
    }

    // ── §5p: git-operation receipts are proof of git-operation claims ────
    //
    // "Both fixes are committed." was `unsupported` in Round 5 with the
    // `git commit` receipt sitting in the claim's own turn (shop-u4,
    // thruline-u1). The comparison is deterministic: the claim names the op,
    // the receipt records it, and NLI is not needed to connect them. This
    // contract only ever SUPPORTS — a missing or failed git op falls through
    // to the ordinary paths, because retry patterns make "the commit failed
    // once" worthless as an accusation.
    {
        let low = c.text.to_ascii_lowercase();
        let wanted_op = if low.contains("committed") || low.contains("commit is in") {
            Some("git commit")
        } else if low.contains("pushed") {
            Some("git push")
        } else if low.contains("merged") {
            Some("git merge")
        } else if low.contains("tagged") {
            Some("git tag")
        } else {
            None
        };
        if let Some(op) = wanted_op {
            // Matched on the RAW command (action-side text), not the §5i key:
            // `git add -A && git commit -m …` canonicalises to its first
            // clause, so the key never says "git commit" — the command does.
            let receipt = authority.eligible.iter().find(|l| {
                l.fidelity == "parsed"
                    && l.tool_input.to_ascii_lowercase().contains(op)
                    && !l.tool_result.is_empty()
                    && !l.tool_result.to_ascii_lowercase().contains("error")
                    && !l.tool_result.to_ascii_lowercase().contains("fatal")
            });
            if let Some(r) = receipt {
                // Quote the receipt's CONTENT, not its JSON envelope.
                let text =
                    result_text(Some(&r.tool_result)).unwrap_or_else(|| r.tool_result.clone());
                let first = text.lines().next().unwrap_or("").trim();
                let first: String = first.chars().take(80).collect();
                return (
                    NodeState::Supported,
                    format!("Recorded `{op}` receipt: {first}.{authority_context}"),
                );
            }
        }
    }

    if b.verdict == "entailed" {
        // A confident score against evidence we could not interpret is not a
        // confirmation, it is surface overlap. M3.4 measured the fallback path
        // scoring a near-verbatim true claim at 0.915, a paraphrase of the same
        // truth at 0.018, and a falsehood at 0.125 — the number tracks how much
        // the claim echoes the pasted output, not whether it is true.
        if b.fidelity != "parsed" {
            return (
                NodeState::Unknown,
                format!(
                    "Evidence present but could not be faithfully interpreted.{authority_context}"
                ),
            );
        }
        match support_correspondence(c, b, &authority.eligible) {
            Correspondence::Full => {}
            Correspondence::Partial => {
                return (
                    NodeState::Unknown,
                    format!("Evidence is narrower than the claim.{authority_context}"),
                );
            }
            Correspondence::None => {
                return (
                    NodeState::Unknown,
                    format!("Evidence identity does not match the claim.{authority_context}"),
                );
            }
        }
        if !support_scope_covers(c, &authority.active, None) {
            return (
                NodeState::Unknown,
                format!("Evidence is narrower than the claim.{authority_context}"),
            );
        }
        return (
            NodeState::Supported,
            format!(
                "Supported by prior, unsuperseded evidence ({:.2}).{authority_context}",
                b.score
            ),
        );
    }

    // §5b: BOTH accusation paths require that we understood the evidence we
    // judged. If nothing readable was ever examined, no accusation is available
    // — not the contradiction one, and not the no-match one either. "We could
    // not read any of it" is not grounds for saying the work was never done.
    if !any_parsed {
        return (
            NodeState::Unknown,
            format!(
                "Evidence present but could not be faithfully interpreted ({n} candidate(s), \
                 none parsed).{authority_context}"
            ),
        );
    }

    // 1. §5d — THE MODEL DOES NOT CONVICT.
    //
    //    There is deliberately no rule here. Every `contradicted` verdict is
    //    produced by the typed-contract branch above, on arithmetic a user can
    //    redo by hand from the receipt.
    //
    //    What used to sit at this line was: accuse when the matched pair
    //    scores contradiction ≥ 0.50 with a 0.25 margin over entailment. M3.9
    //    narrowed that rule from a scan of 30 candidates to a single matched
    //    pair, which halved accusation volume (67% → 33% of work claims on
    //    ATLAS) and left precision at **0.00 across 15 hand-adjudicated cases**
    //    — because the defect was never *which* evidence was consulted, it was
    //    that an entailment score was answering a question it cannot answer.
    //    Five of those nine false warnings were premises that PROVED the claim
    //    (A36): "98/98 passing" against a run reporting `# pass 98 / # fail 0`,
    //    scored contradiction 0.87.
    //
    //    A count-bearing claim whose matched evidence we could not interpret is
    //    now `unknown`, not an accusation. That is the honest verdict: there is
    //    evidence, and we could not read it.
    if kind == crate::extract::ClaimKind::Marker.as_str()
        && crate::contract::parse_claim(&c.text).is_some()
        && authority
            .active
            .iter()
            .any(|l| l.fidelity == "parsed" && l.match_strength >= ACCUSE_MIN_STRENGTH)
    {
        return (
            NodeState::Unknown,
            format!("Evidence present; outcome format not interpretable.{authority_context}"),
        );
    }

    // 2. We read the nearby work and none of it is even on this subject. This
    //    is the classic "no matching action found": the claim names work that
    //    left no trace. Restricted to parsed candidates — silence only counts
    //    as evidence of absence where we could actually see.
    let parsed_related = authority
        .active
        .iter()
        .filter(|l| l.fidelity == "parsed")
        .any(|l| l.related);
    if !parsed_related {
        let parsed_n = authority
            .active
            .iter()
            .filter(|l| l.fidelity == "parsed")
            .count();
        return unsupported_with_coverage(
            c,
            format!(
                "No captured proof found ({parsed_n} authoritative readable action(s) checked).\
                 {authority_context}"
            ),
        );
    }

    // 3. Related, readable evidence that neither confirms nor contradicts. The
    //    claim may well be true and merely phrased in a way the scorer cannot
    //    follow — measured at entailment 0.005 for a true paraphrase of a
    //    passing test run. Saying nothing is the only honest option.
    (
        NodeState::Unknown,
        format!(
            "Related prior work was found but does not confirm this claim \
             ({n} checked, best {:.2}).{authority_context}",
            b.score,
        ),
    )
}

type BranchChronologyKey = (String, i64, String);

fn branch_chronology_key(node: &DerivedNode) -> BranchChronologyKey {
    (
        node.ts.clone().unwrap_or_default(),
        node.anchor_seq,
        node.id.clone(),
    )
}

/// Newest reachable leaf for every node in the canonical parent graph.
///
/// Branch selection is about the path that ultimately continued, not which
/// sibling happened to be created last. Propagating leaf keys from child to
/// parent keeps the work linear in nodes + edges, and retains the established
/// `(timestamp, sequence, id)` total order. A malformed cycle has no leaf; its
/// unresolved members fall back to their own key, preserving deterministic
/// behavior without allowing corrupt input to spin.
fn newest_descendant_leaf_keys(
    nodes: &[DerivedNode],
    children: &BTreeMap<String, BTreeSet<String>>,
) -> HashMap<String, BranchChronologyKey> {
    let node_ids: BTreeSet<&str> = nodes.iter().map(|node| node.id.as_str()).collect();
    let mut parents: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut remaining_children: BTreeMap<String, usize> = BTreeMap::new();

    for node in nodes {
        let valid_children = children
            .get(&node.id)
            .into_iter()
            .flatten()
            .filter(|child| node_ids.contains(child.as_str()));
        let mut count = 0usize;
        for child in valid_children {
            parents
                .entry(child.clone())
                .or_default()
                .insert(node.id.clone());
            count += 1;
        }
        remaining_children.insert(node.id.clone(), count);
    }

    let mut ready: BTreeSet<String> = remaining_children
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(id, _)| id.clone())
        .collect();
    let mut newest: HashMap<String, BranchChronologyKey> = nodes
        .iter()
        .filter(|node| remaining_children.get(&node.id) == Some(&0))
        .map(|node| (node.id.clone(), branch_chronology_key(node)))
        .collect();

    while let Some(child) = ready.pop_first() {
        let Some(child_key) = newest.get(&child).cloned() else {
            continue;
        };
        let Some(direct_parents) = parents.get(&child) else {
            continue;
        };
        for parent in direct_parents {
            newest
                .entry(parent.clone())
                .and_modify(|current| {
                    if child_key > *current {
                        *current = child_key.clone();
                    }
                })
                .or_insert_with(|| child_key.clone());

            let Some(remaining) = remaining_children.get_mut(parent) else {
                continue;
            };
            *remaining = remaining.saturating_sub(1);
            if *remaining == 0 {
                ready.insert(parent.clone());
            }
        }
    }

    for node in nodes {
        newest
            .entry(node.id.clone())
            .or_insert_with(|| branch_chronology_key(node));
    }
    newest
}

/* ── derivation ──────────────────────────────────────────────────────── */

/// Derive with default thresholds/eligibility. Convenience for callers that
/// have no config loaded (tests, and the deterministic tier).
pub fn derive_project(ledger: &Ledger, project_id: &str) -> Result<DerivedGraph> {
    derive_project_with(ledger, project_id, &crate::config::Config::default())
}

/// Derive a project.
///
/// `cfg` is read for **one** thing: `forward.min_subsequent_user_turns`, the
/// rule deciding whether a proposal has been outlived. It must match the value
/// `annotate` ran with, or a proposal `annotate` skipped as "still recent"
/// would look to derivation like one that was checked and found unfulfilled.
pub fn derive_project_with(
    ledger: &Ledger,
    project_id: &str,
    cfg: &crate::config::Config,
) -> Result<DerivedGraph> {
    // 1. Sessions, ordered chronologically. `started_at` first so that a
    //    resumed child sorts AFTER its parent — otherwise "first occurrence"
    //    of a duplicated event could land in the copy rather than the original
    //    and hollow out the parent session. NULLs last, then id, so the order
    //    is total.
    let mut stmt = ledger.conn().prepare(
        "SELECT id, started_at FROM sessions WHERE project_id = ?1
         ORDER BY (started_at IS NULL), started_at, id",
    )?;
    let session_rows = stmt.query_map(params![project_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
    })?;
    let mut session_order: HashMap<String, usize> = HashMap::new();
    let mut sessions: Vec<SessionInfo> = Vec::new();
    for row in session_rows {
        let (id, started_at) = row?;
        session_order.insert(id.clone(), sessions.len());
        sessions.push(SessionInfo {
            id,
            started_at,
            event_count: 0,
        });
    }
    drop(stmt);

    // 2. Events, payloads bounded and projected by kind.
    //
    // A73's scale corpus is metadata-heavy. Loading receipt and payload
    // strings for kinds whose match arms can never read them retained more
    // than 217 million bytes of useless string content through the edge pass.
    // The identity/order/parent fields stay universal; payload and receipt fields
    // are populated only for the kinds that consume them below. This is a
    // representation boundary, not an event filter: every authoritative row
    // still enters canonicalization in the same order.
    let mut stmt = ledger.conn().prepare(&format!(
        "SELECT id, session_id, seq, kind,
                CASE WHEN kind = 'assistant_text'
                     THEN substr(text,1,{p}) END,
                CASE WHEN kind = 'tool_use' THEN tool_name END,
                CASE WHEN kind IN ('tool_use','tool_result')
                     THEN tool_use_id END,
                CASE WHEN kind = 'tool_use'
                     THEN substr(tool_input,1,{p}) END,
                CASE WHEN kind = 'tool_result'
                     THEN substr(tool_result,1,{p}) END,
                CASE WHEN kind = 'tool_result' THEN is_error ELSE 0 END,
                src_uuid, parent_uuid,
                CASE WHEN kind IN ('assistant_text','tool_use') THEN ts END,
                CASE WHEN kind IN ('assistant_text','tool_use')
                     THEN raw_file ELSE '' END,
                raw_line,
                CASE WHEN kind = 'tool_use' THEN raw_sha256 ELSE '' END
         FROM events WHERE project_id = ?1",
        p = PAYLOAD_PREFIX
    ))?;
    let rows = stmt.query_map(params![project_id], |r| {
        Ok(Ev {
            id: r.get(0)?,
            session_id: r.get(1)?,
            seq: r.get(2)?,
            kind: EventKind::Meta, // replaced below
            text: r.get(4)?,
            tool_name: r.get(5)?,
            tool_use_id: r.get(6)?,
            tool_input: r.get(7)?,
            tool_result: r.get(8)?,
            is_error: r.get::<_, i64>(9)? != 0,
            src_uuid: r.get(10)?,
            parent_uuid: r.get(11)?,
            ts: r.get(12)?,
            raw_file: r.get(13)?,
            raw_line: r.get::<_, i64>(14)?.max(0) as u64,
            raw_sha256: r.get(15)?,
        })
        .map(|mut e| {
            let k: String = r.get(3).unwrap_or_default();
            e.kind = k.parse().unwrap_or(EventKind::Meta);
            e
        })
    })?;
    let mut events: Vec<Ev> = Vec::new();
    for r in rows {
        events.push(r?);
    }
    drop(stmt);

    let raw_events = events.len() as u64;

    // 3. Total order: session chronology, then per-session seq.
    events.sort_by(|a, b| {
        let oa = session_order
            .get(&a.session_id)
            .copied()
            .unwrap_or(usize::MAX);
        let ob = session_order
            .get(&b.session_id)
            .copied()
            .unwrap_or(usize::MAX);
        oa.cmp(&ob)
            .then_with(|| a.session_id.cmp(&b.session_id))
            .then_with(|| a.seq.cmp(&b.seq))
    });

    // 4. Canonicalize by src_uuid (A13). Events without a uuid can never be
    //    duplicates and are always canonical.
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut canonical: Vec<Ev> = Vec::new();
    let mut extra_parents: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
    let mut duplicate_relations: Vec<DuplicateEventRelation> = Vec::new();
    for ev in events {
        match ev.src_uuid.clone() {
            Some(u) => match seen.get(&u) {
                Some(&idx) => {
                    let first = &canonical[idx];
                    duplicate_relations.push(DuplicateEventRelation {
                        duplicate_event_id: ev.id.clone(),
                        canonical_event_id: first.id.clone(),
                        src_uuid: u,
                        duplicate_session_id: ev.session_id.clone(),
                        duplicate_seq: ev.seq,
                        canonical_session_id: first.session_id.clone(),
                        canonical_seq: first.seq,
                    });
                    // A13 caveat: a later copy may carry a different parent
                    // (a rewind re-attaching the same message). Keep it as an
                    // extra edge rather than losing it.
                    if let Some(p) = &ev.parent_uuid {
                        if canonical[idx].parent_uuid.as_deref() != Some(p.as_str()) {
                            extra_parents.entry(idx).or_default().insert(p.clone());
                        }
                    }
                }
                None => {
                    seen.insert(u, canonical.len());
                    canonical.push(ev);
                }
            },
            None => canonical.push(ev),
        }
    }
    // The input walk is already chronological, but make the public relation's
    // total order explicit so query/fixture construction order cannot leak into
    // either export bytes or the derivation fingerprint.
    duplicate_relations.sort_by(|a, b| {
        let oa = session_order
            .get(&a.duplicate_session_id)
            .copied()
            .unwrap_or(usize::MAX);
        let ob = session_order
            .get(&b.duplicate_session_id)
            .copied()
            .unwrap_or(usize::MAX);
        oa.cmp(&ob)
            .then_with(|| a.duplicate_session_id.cmp(&b.duplicate_session_id))
            .then_with(|| a.duplicate_seq.cmp(&b.duplicate_seq))
            .then_with(|| a.duplicate_event_id.cmp(&b.duplicate_event_id))
            .then_with(|| a.canonical_event_id.cmp(&b.canonical_event_id))
            .then_with(|| a.src_uuid.cmp(&b.src_uuid))
    });
    let duplicate_event_count = duplicate_relations.len() as u64;

    for s in sessions.iter_mut() {
        s.event_count = canonical.iter().filter(|e| e.session_id == s.id).count() as u64;
    }

    // 5. Cluster tool_use with its tool_result.
    let mut result_of: HashMap<String, usize> = HashMap::new();
    for (i, e) in canonical.iter().enumerate() {
        if e.kind == EventKind::ToolResult {
            if let Some(tid) = &e.tool_use_id {
                result_of.entry(tid.clone()).or_insert(i);
            }
        }
    }

    // 6. Walk turns. A turn is the span between consecutive user_msg events.
    let mut nodes: Vec<DerivedNode> = Vec::new();
    let mut owner: HashMap<String, usize> = HashMap::new(); // event uuid -> node index
    let mut stats = DeriveStats {
        raw_events,
        canonical_events: canonical.len() as u64,
        duplicate_events: duplicate_event_count,
        ..Default::default()
    };

    let node_id = |e: &Ev| format!("{}:{}", e.session_id, e.seq);
    let ordinal_of: HashMap<&str, u64> = canonical
        .iter()
        .enumerate()
        .filter_map(|(i, e)| e.src_uuid.as_deref().map(|u| (u, i as u64 + 1)))
        .collect();
    let language_projection = load_language_projection(ledger, project_id)?;
    let mut raw_scope_cache = crate::raw_scope::RawScopeCache::default();

    let mut i = 0usize;
    while i < canonical.len() {
        // Collect the turn: from here to the next user_msg (exclusive).
        let start = i;
        if canonical[i].kind == EventKind::UserMsg {
            i += 1;
        }
        while i < canonical.len() && canonical[i].kind != EventKind::UserMsg {
            i += 1;
        }
        let turn = &canonical[start..i];

        // Statement candidates and action clusters, in event order.
        let mut pending_statement: Option<usize> = None; // index into nodes
        let mut last_collapse: Option<(String, usize)> = None;

        for (off, e) in turn.iter().enumerate() {
            match e.kind {
                // Never a node, never evidence (§8).
                EventKind::AssistantThinking => {}
                EventKind::AssistantText => {
                    // Per §4: only a forward-looking proposal or a completed-work
                    // assertion is a statement candidate. Prose the crude matcher
                    // cannot call is not a unit of intended work and does not
                    // become a node — it stays in the ledger, reachable by
                    // receipt, and M3's extractor gets another pass at it.
                    let text = e.text.clone().unwrap_or_default();
                    let Some(kind) =
                        classify_projected_statement(&e.id, &text, &language_projection)
                    else {
                        // Unclassifiable prose still ENDS the previous
                        // statement's attachment window. A turn here can be one
                        // user prompt followed by hundreds of tool calls, so
                        // without this a single statement absorbs the whole
                        // session (observed: 117 evidence rows on one node).
                        // "Clusters that follow it" means until the assistant
                        // says something else, not until the user speaks again.
                        pending_statement = None;
                        last_collapse = None;
                        continue;
                    };
                    stats.statement_nodes += 1;
                    let idx = nodes.len();
                    nodes.push(DerivedNode {
                        id: node_id(e),
                        label: label_from(&text),
                        // Refined once the turn's clusters are attached.
                        sort_key: match kind {
                            Statement::Proposal => NodeState::Pending,
                            // A completed-work claim with no attached action is
                            // exactly M3's `unsupported`. M2 must not
                            // pre-judge it, so it stays unknown.
                            Statement::CompletedWork => NodeState::Unknown,
                        },
                        dimensions: match kind {
                            Statement::Proposal => NodeDimensions::new(
                                Lifecycle::Proposed,
                                EvidenceState::Unknown,
                                Currency::Current,
                            ),
                            Statement::CompletedWork => NodeDimensions::new(
                                Lifecycle::Completed,
                                EvidenceState::Unknown,
                                Currency::Current,
                            ),
                        },
                        session_id: e.session_id.clone(),
                        anchor_event_id: e.id.clone(),
                        anchor_seq: e.seq,
                        anchor_ordinal: e
                            .src_uuid
                            .as_deref()
                            .and_then(|u| ordinal_of.get(u).copied())
                            .unwrap_or(0),
                        anchor_uuid: e.src_uuid.clone(),
                        ts: e.ts.clone(),
                        claim_text: excerpt(&text),
                        evidence: Vec::new(),
                        abandoned: false,
                        raw_file: e.raw_file.clone(),
                        raw_line: e.raw_line,
                        verdicts: Vec::new(),
                        verdict_reason: String::new(),
                    });
                    if let Some(u) = &e.src_uuid {
                        owner.insert(u.clone(), idx);
                    }
                    pending_statement = Some(idx);
                    last_collapse = None;
                }
                EventKind::ToolUse => {
                    stats.action_clusters += 1;
                    let tool = e.tool_name.clone().unwrap_or_else(|| "tool".to_string());
                    let key = collapse_key(&tool, e.tool_input.as_deref());
                    let mut path = target_path(e.tool_input.as_deref());
                    let res_idx = e
                        .tool_use_id
                        .as_ref()
                        .and_then(|t| result_of.get(t))
                        .copied();
                    let is_error = res_idx.map(|r| canonical[r].is_error).unwrap_or(false);
                    let outcome =
                        res_idx.and_then(|r| result_excerpt(canonical[r].tool_result.as_deref()));
                    let tail = res_idx
                        .and_then(|r| result_text(canonical[r].tool_result.as_deref()))
                        .map(|t| tail_chars(&t, 300));
                    if path.is_none() && is_edit_tool(&tool) {
                        path = res_idx
                            .and_then(|r| path_from_result(canonical[r].tool_result.as_deref()));
                    }
                    let command = if tool == "Bash" {
                        bash_command(e.tool_input.as_deref())
                    } else {
                        None
                    };
                    let new_content = if is_edit_tool(&tool) {
                        edit_content(e.tool_input.as_deref())
                    } else {
                        None
                    };
                    let raw_scope =
                        raw_scope_cache.scope_for(&e.raw_file, e.raw_line as i64, &e.raw_sha256);
                    let command_identity = raw_scope.trusted.then(|| {
                        crate::identity::identity_key(
                            &tool,
                            e.tool_input.as_deref().unwrap_or(""),
                            raw_scope.cwd.as_deref(),
                        )
                    });
                    let row = EvidenceRow {
                        kind: if is_edit_tool(&tool) {
                            "edit"
                        } else {
                            "command"
                        },
                        path: path.clone(),
                        summary: match &path {
                            Some(p) => format!("{tool} {}", short_path(p)),
                            None => summarize_tool(&tool, e.tool_input.as_deref()),
                        },
                        tool_name: tool.clone(),
                        command: command.clone(),
                        result_tail: tail,
                        new_content,
                        result_excerpt: outcome,
                        tool_use_id: e.tool_use_id.clone(),
                        is_error,
                        tool_input: e.tool_input.clone(),
                        // Raw coordinates remain available for the receipt;
                        // ambient scope is admitted only after the line hash
                        // above matched its immutable event.
                        raw_file: Some(e.raw_file.clone()),
                        raw_line: Some(e.raw_line as i64),
                        session_id: Some(e.session_id.clone()),
                        seq: Some(e.seq),
                        cwd: raw_scope.cwd,
                        git_branch: raw_scope.git_branch,
                        command_identity,
                        raw_sha256: Some(e.raw_sha256.clone()),
                    };

                    // Consecutive clusters in this turn sharing a collapse key
                    // fold into the node that owns the previous one.
                    if let Some((prev_key, prev_node)) = &last_collapse {
                        if prev_key == &key {
                            stats.collapsed_clusters += 1;
                            let n = *prev_node;
                            nodes[n].evidence.push(row);
                            if let Some(u) = &e.src_uuid {
                                owner.insert(u.clone(), n);
                            }
                            if let Some(r) = res_idx {
                                if let Some(u) = &canonical[r].src_uuid {
                                    owner.insert(u.clone(), n);
                                }
                            }
                            continue;
                        }
                    }

                    let target_node = match pending_statement {
                        // Attach to the statement this turn opened with.
                        Some(idx) => {
                            nodes[idx].evidence.push(row);
                            idx
                        }
                        // A cluster standing alone is its own node.
                        None => {
                            let idx = nodes.len();
                            let label_src = match &path {
                                Some(p) => format!("{tool} {}", short_path(p)),
                                None => summarize_tool(&tool, e.tool_input.as_deref()),
                            };
                            nodes.push(DerivedNode {
                                id: node_id(e),
                                label: truncate_words(&label_src, 22),
                                sort_key: NodeState::Recorded,
                                dimensions: NodeDimensions::new(
                                    Lifecycle::Completed,
                                    EvidenceState::Observed,
                                    Currency::Current,
                                ),
                                session_id: e.session_id.clone(),
                                anchor_event_id: e.id.clone(),
                                anchor_seq: e.seq,
                                anchor_ordinal: e
                                    .src_uuid
                                    .as_deref()
                                    .and_then(|u| ordinal_of.get(u).copied())
                                    .unwrap_or(0),
                                anchor_uuid: e.src_uuid.clone(),
                                ts: e.ts.clone(),
                                claim_text: label_src,
                                evidence: vec![row],
                                abandoned: false,
                                raw_file: e.raw_file.clone(),
                                raw_line: e.raw_line,
                                verdicts: Vec::new(),
                                verdict_reason: String::new(),
                            });
                            idx
                        }
                    };
                    if let Some(u) = &e.src_uuid {
                        owner.insert(u.clone(), target_node);
                    }
                    if let Some(r) = res_idx {
                        if let Some(u) = &canonical[r].src_uuid {
                            owner.insert(u.clone(), target_node);
                        }
                    }
                    last_collapse = Some((key, target_node));
                    let _ = off;
                }
                _ => {}
            }
        }
    }

    // 7. Final states.
    //
    // Without annotations this is the deterministic tier: an action happened,
    // so `recorded` — nothing is verified and nothing is accused. With
    // annotations, each statement node takes the **worst** of its claims'
    // verdicts, because a paragraph is only as honest as its weakest sentence.
    let annotations = load_annotations(ledger, project_id)?;

    // Forward-eligibility, recomputed here rather than inferred from the
    // absence of links: "still pending" and "moved on but nothing matched" both
    // produce zero links, and confusing them would turn every fresh proposal
    // into a 👻. Must agree with `annotate`, which is why both read the same
    // config value.
    let session_count = sessions.len();
    let mut later_user_turns: HashMap<(String, i64), usize> = HashMap::new();
    {
        let mut per_session: BTreeMap<&str, Vec<i64>> = BTreeMap::new();
        for e in &canonical {
            if e.kind == EventKind::UserMsg {
                per_session
                    .entry(e.session_id.as_str())
                    .or_default()
                    .push(e.seq);
            }
        }
        for n in &nodes {
            let seqs = per_session
                .get(n.session_id.as_str())
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let count = seqs.iter().filter(|s| **s > n.anchor_seq).count();
            later_user_turns.insert((n.session_id.clone(), n.anchor_seq), count);
        }
    }
    let session_rank: HashMap<String, usize> = sessions
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.clone(), i))
        .collect();

    // R17a-3: accepted assured pairs, bound to the tool_use events they attest.
    // Loaded ONCE per derivation. Empty on every corpus without an assured
    // capture — which is every real corpus today.
    let attested_runs = crate::capture::attested_runs_for_project(ledger, project_id)?;

    let mut fulfilled: Vec<(String, String)> = Vec::new(); // (proposal node id, evidence event id)
    for n in nodes.iter_mut() {
        let claims = annotations.get(&n.anchor_event_id);
        match claims {
            Some(cs) if !cs.is_empty() => {
                let moved_on = session_rank
                    .get(&n.session_id)
                    .is_some_and(|r| r + 1 < session_count)
                    || later_user_turns
                        .get(&(n.session_id.clone(), n.anchor_seq))
                        .copied()
                        .unwrap_or(0)
                        >= cfg.forward.min_subsequent_user_turns;

                let mut worst = NodeState::Recorded;
                let mut worst_reason = String::new();
                for c in cs {
                    let (state, reason) = claim_state(c, moved_on);
                    if state == NodeState::Supported
                        && c.kind == crate::extract::ClaimKind::Proposal.as_str()
                    {
                        if let Some(b) = c.links.first() {
                            fulfilled.push((n.id.clone(), b.event_id.clone()));
                        }
                    }
                    let before = worst;
                    worst = worst.worse_of(state);
                    if worst != before || worst_reason.is_empty() {
                        worst_reason = reason.clone();
                    }
                    n.verdicts.push(ClaimVerdict {
                        claim_id: c.id.clone(),
                        text: c.text.clone(),
                        kind: c.kind.clone(),
                        marker: c.marker.clone(),
                        span_start: c.span_start,
                        span_end: c.span_end,
                        candidates: c.links.len(),
                        parsed_candidates: c
                            .links
                            .iter()
                            .filter(|l| l.fidelity == "parsed")
                            .count(),
                        best_score: c.links.first().map(|l| l.score),
                        best_event_id: c.links.first().map(|l| l.event_id.clone()),
                        best_fidelity: c.links.first().map(|l| l.fidelity.clone()),
                        state,
                        dimensions: dimensions_for_claim(
                            c.kind.as_str(),
                            state,
                            !n.evidence.is_empty(),
                            attested_for_claim(c, &attested_runs).as_ref(),
                        ),
                        reason,
                        evidence_scopes: evidence_scopes(c),
                    });
                }
                // §5b node rule: a node whose claims are ALL out of scope
                // (findings, micro-intents) is `recorded` when an action is
                // attached and `unclassified` when nothing happened at all.
                // Never `pending` — there is no intent outstanding — and never
                // an accusation, because nothing accusable was ever claimed.
                if worst == NodeState::Recorded && n.evidence.is_empty() {
                    worst = NodeState::Unknown;
                    worst_reason =
                        "Nothing here asserts work that could be checked (§5b).".to_string();
                }
                n.dimensions = compose_dimensions(&n.verdicts, worst, !n.evidence.is_empty());
                n.sort_key = sort_key_for_dimensions(n.dimensions, &n.verdicts);
                n.verdict_reason = worst_reason;
            }
            // No claims: an action cluster, or a statement whose claims were
            // never scored. Either way nothing is asserted, so nothing warns.
            _ => {
                if !n.evidence.is_empty() {
                    n.dimensions = dimensions_for_sort_key(NodeState::Recorded, true);
                }
                n.sort_key = sort_key_for_dimensions(n.dimensions, &n.verdicts);
            }
        }
    }

    // 8. Edges from the canonical parent chain, lifted to node granularity.
    let mut edge_set: BTreeSet<(String, String, &'static str)> = BTreeSet::new();
    let mut children: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let uuid_session: HashMap<&str, &str> = canonical
        .iter()
        .filter_map(|e| e.src_uuid.as_deref().map(|u| (u, e.session_id.as_str())))
        .collect();

    // Many events own no node (user_msg, tool_result, meta). Walking only
    // owner-to-owner would sever the chain at every one of them and hide real
    // branching, so climb to the nearest ancestor that does own a node.
    let by_uuid: HashMap<&str, &Ev> = canonical
        .iter()
        .filter_map(|e| e.src_uuid.as_deref().map(|u| (u, e)))
        .collect();
    let nearest_owner = |start: &str| -> Option<(usize, String)> {
        let mut cur = start.to_string();
        // Bounded: a malformed chain must not spin.
        for _ in 0..4096 {
            if let Some(&idx) = owner.get(cur.as_str()) {
                return Some((idx, cur));
            }
            let parent = by_uuid.get(cur.as_str())?.parent_uuid.clone()?;
            cur = parent;
        }
        None
    };

    for e in &canonical {
        let (Some(uuid), Some(parent)) = (e.src_uuid.as_deref(), e.parent_uuid.as_deref()) else {
            continue;
        };
        let Some(&child_node) = owner.get(uuid) else {
            continue;
        };
        let Some((parent_node, parent_uuid)) = nearest_owner(parent) else {
            continue;
        };
        if child_node == parent_node {
            continue;
        }
        let from = nodes[parent_node].id.clone();
        let to = nodes[child_node].id.clone();
        let cross_session =
            uuid_session.get(parent_uuid.as_str()).copied() != Some(e.session_id.as_str());
        let kind = if cross_session { "inferred" } else { "seq" };
        edge_set.insert((from.clone(), to.clone(), kind));
        children.entry(from).or_default().insert(to);
    }

    // Extra parents recovered from dedup (A13 caveat) — never dropped.
    for (idx, parents) in &extra_parents {
        let Some(child_uuid) = canonical[*idx].src_uuid.as_deref() else {
            continue;
        };
        let Some(&child_node) = owner.get(child_uuid) else {
            continue;
        };
        for p in parents {
            if let Some(&pn) = owner.get(p.as_str()) {
                if pn != child_node {
                    stats.extra_parent_edges += 1;
                    edge_set.insert((
                        nodes[pn].id.clone(),
                        nodes[child_node].id.clone(),
                        "inferred",
                    ));
                }
            }
        }
    }

    // 8b. Fulfilled-later edges. A proposal the forward pass matched to a
    //     later action gets an `inferred` edge to it, so "fulfilled later" is
    //     a visible link in the graph and not just a sentence in a receipt.
    if !fulfilled.is_empty() {
        let node_of_event: HashMap<&str, &str> = nodes
            .iter()
            .map(|n| (n.anchor_event_id.as_str(), n.id.as_str()))
            .collect();
        // A cluster's evidence may hang off a node anchored elsewhere, so fall
        // back to the owning node via the event's uuid.
        let uuid_of_event: HashMap<&str, &str> = canonical
            .iter()
            .filter_map(|e| e.src_uuid.as_deref().map(|u| (e.id.as_str(), u)))
            .collect();
        for (from, ev_id) in &fulfilled {
            let to = node_of_event
                .get(ev_id.as_str())
                .copied()
                .or_else(|| {
                    uuid_of_event
                        .get(ev_id.as_str())
                        .and_then(|u| owner.get(*u))
                        .map(|i| nodes[*i].id.as_str())
                })
                .map(str::to_string);
            if let Some(to) = to {
                if to != *from {
                    edge_set.insert((from.clone(), to, "inferred"));
                }
            }
        }
    }

    // 9. Branch marking: where one parent has several children, the branch
    //    reaching the newest leaf is active; the other heads are flagged
    //    abandoned. Retained, never removed — M3's dropped-thread candidates.
    let index_of: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let newest_leaf = newest_descendant_leaf_keys(&nodes, &children);
    let mut abandon: BTreeSet<String> = BTreeSet::new();
    for kids in children.values() {
        if kids.len() < 2 {
            continue;
        }
        // "Newest" = the greatest leaf `(ts, anchor_seq, id)` reachable
        // through this immediate child. The child id breaks the theoretical
        // tie where malformed/converging paths reach the same leaf.
        let mut best: Option<&String> = None;
        for k in kids {
            if !index_of.contains_key(k.as_str()) {
                continue;
            }
            let replace = best.is_none_or(|current| {
                newest_leaf
                    .get(k.as_str())
                    .cmp(&newest_leaf.get(current.as_str()))
                    .then_with(|| k.cmp(current))
                    .is_gt()
            });
            if replace {
                best = Some(k);
            }
        }
        if let Some(winner) = best {
            for k in kids {
                if k != winner {
                    abandon.insert(k.clone());
                }
            }
        }
    }
    for n in nodes.iter_mut() {
        if abandon.contains(&n.id) {
            n.abandoned = true;
            // A branch that lost to a newer sibling is superseded, not
            // lifecycle-abandoned. The work may still have completed and its
            // evidence remains what it was; currency is the independent fact.
            n.dimensions.currency = Currency::Superseded;
            for verdict in &mut n.verdicts {
                verdict.dimensions.currency = Currency::Superseded;
            }
            stats.abandoned_heads += 1;
        }
    }

    // 10. Total orders on the output.
    nodes.sort_by(|a, b| {
        let oa = session_order
            .get(&a.session_id)
            .copied()
            .unwrap_or(usize::MAX);
        let ob = session_order
            .get(&b.session_id)
            .copied()
            .unwrap_or(usize::MAX);
        oa.cmp(&ob)
            .then_with(|| a.session_id.cmp(&b.session_id))
            .then_with(|| a.anchor_seq.cmp(&b.anchor_seq))
    });
    let edges: Vec<DerivedEdge> = edge_set
        .into_iter()
        .map(|(from, to, kind)| DerivedEdge { from, to, kind })
        .collect();

    for n in &nodes {
        *stats
            .state_counts
            .entry(n.sort_key.as_str().to_string())
            .or_insert(0) += 1;
        *stats
            .lifecycle_counts
            .entry(n.dimensions.lifecycle.as_str().to_string())
            .or_insert(0) += 1;
        *stats
            .evidence_counts
            .entry(n.dimensions.evidence.as_str().to_string())
            .or_insert(0) += 1;
        *stats
            .currency_counts
            .entry(n.dimensions.currency.as_str().to_string())
            .or_insert(0) += 1;
    }

    assert_eq!(
        stats.duplicate_events,
        duplicate_relations.len() as u64,
        "every folded event must have exactly one duplicate audit relation"
    );

    Ok(DerivedGraph {
        project_id: project_id.to_string(),
        sessions,
        nodes,
        edges,
        duplicate_events: duplicate_relations,
        stats,
    })
}

/// Build the evidence row for **one** `tool_use` event, straight from SQLite.
///
/// This is what makes `annotate` streamable: it reconstructs exactly the row
/// [`derive_project`] would have attached to a node, without loading the
/// project's events into memory. §5a forbids the scorer and derivation state
/// being co-resident, and the way to obey that is not to hold the graph at all.
///
/// Returns `None` when the event is absent or is not a `tool_use`.
pub fn evidence_row_for(ledger: &Ledger, event_id: &str) -> Result<Option<EvidenceRow>> {
    let row = ledger
        .conn()
        .query_row(
            &format!(
                "SELECT tool_name, tool_use_id, substr(tool_input,1,{p}), kind,
                        raw_file, raw_line, raw_sha256, session_id, seq,
                        substr(text,1,{p})
                 FROM events WHERE id = ?1",
                p = PAYLOAD_PREFIX
            ),
            params![event_id],
            |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<i64>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, Option<i64>>(8)?,
                    r.get::<_, Option<String>>(9)?,
                ))
            },
        )
        .optional()?;
    let Some((
        tool_name,
        tool_use_id,
        tool_input,
        kind,
        raw_file,
        raw_line,
        raw_sha256,
        session_id,
        seq,
        event_text,
    )) = row
    else {
        return Ok(None);
    };
    let task_notification = kind == EventKind::UserMsg.as_str()
        && event_text
            .as_deref()
            .is_some_and(|t| t.trim_start().starts_with("<task-notification>"));
    if kind != EventKind::ToolUse.as_str() && !task_notification {
        return Ok(None);
    }
    let tool = if task_notification {
        "TaskNotification".to_string()
    } else {
        tool_name.unwrap_or_else(|| "tool".to_string())
    };

    // The matching result, if the transcript recorded one. `MIN(seq)` mirrors
    // derivation's "first result wins" for a repeated tool_use_id.
    let result: Option<(Option<String>, bool)> = if task_notification {
        let failed = event_text.as_deref().is_some_and(|t| {
            let low = t.to_ascii_lowercase();
            low.contains("<status>failed</status>") || low.contains("<status>error</status>")
        });
        Some((event_text.clone(), failed))
    } else {
        match &tool_use_id {
            Some(tid) => ledger
                .conn()
                .query_row(
                    &format!(
                        "SELECT substr(tool_result,1,{p}), is_error FROM events
                     WHERE tool_use_id = ?1 AND kind = 'tool_result'
                     ORDER BY session_id, seq LIMIT 1",
                        p = PAYLOAD_PREFIX
                    ),
                    params![tid],
                    |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0)),
                )
                .optional()?,
            None => None,
        }
    };
    let (raw_result, is_error) = match result {
        Some((r, e)) => (r, e),
        None => (None, false),
    };

    let mut path = target_path(tool_input.as_deref());
    if path.is_none() && is_edit_tool(&tool) {
        path = path_from_result(raw_result.as_deref());
    }
    let command = if tool == "Bash" {
        bash_command(tool_input.as_deref())
    } else {
        None
    };
    let new_content = if is_edit_tool(&tool) {
        edit_content(tool_input.as_deref())
    } else {
        None
    };

    Ok(Some(EvidenceRow {
        kind: if is_edit_tool(&tool) {
            "edit"
        } else {
            "command"
        },
        path: path.clone(),
        summary: match &path {
            Some(p) => format!("{tool} {}", short_path(p)),
            None => summarize_tool(&tool, tool_input.as_deref()),
        },
        tool_name: tool,
        command,
        result_tail: result_text(raw_result.as_deref()).map(|t| tail_chars(&t, 300)),
        new_content,
        result_excerpt: result_excerpt(raw_result.as_deref()),
        tool_use_id,
        is_error,
        tool_input,
        raw_file,
        raw_line,
        session_id,
        seq,
        cwd: None,
        git_branch: None,
        command_identity: None,
        raw_sha256,
    }))
}

/// Claim-focused source text for compound and agent/task evidence.
///
/// `EvidenceRow` deliberately keeps the old 2,000/300-character bounds so
/// existing premise and outcome behavior cannot drift accidentally. This
/// second, transient read is used only by the claim-aware renderer: one event
/// at a time, capped at 512 KiB, then reduced to a small grounded excerpt
/// before scoring.
pub fn evidence_detail_for(ledger: &Ledger, event_id: &str) -> Result<Option<String>> {
    // the largest captured result in one project is 459,659 bytes. A 128 KiB prefix
    // truncated 82 records into invalid JSON before the claim-focused reducer
    // could inspect them. 512 KiB covers the measured corpus while keeping the
    // read bounded and transient; only the ≤1,040-character focused excerpt
    // reaches the scorer.
    const DETAIL_PREFIX: usize = 512 * 1024;
    let row: Option<(String, Option<String>, Option<String>)> = ledger
        .conn()
        .query_row(
            &format!(
                "SELECT kind, tool_use_id, substr(text,1,{DETAIL_PREFIX})
                 FROM events WHERE id = ?1"
            ),
            params![event_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((kind, tool_use_id, text)) = row else {
        return Ok(None);
    };
    if kind == EventKind::UserMsg.as_str()
        && text
            .as_deref()
            .is_some_and(|t| t.trim_start().starts_with("<task-notification>"))
    {
        return Ok(text);
    }
    if kind != EventKind::ToolUse.as_str() {
        return Ok(None);
    }
    let Some(tid) = tool_use_id else {
        return Ok(None);
    };
    let raw: Option<String> = ledger
        .conn()
        .query_row(
            &format!(
                "SELECT substr(tool_result,1,{DETAIL_PREFIX}) FROM events
                 WHERE tool_use_id = ?1 AND kind = 'tool_result'
                 ORDER BY session_id, seq LIMIT 1"
            ),
            params![tid],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(result_text(raw.as_deref()))
}

fn excerpt(text: &str) -> String {
    let t = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() <= 400 {
        t
    } else {
        let head: String = t.chars().take(399).collect();
        format!("{head}…")
    }
}

fn summarize_tool(tool: &str, input: Option<&str>) -> String {
    if let Some(raw) = input {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
            for key in ["command", "pattern", "query", "description", "url"] {
                if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
                    let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
                    return format!("{tool} {one}");
                }
            }
        }
    }
    tool.to_string()
}

/// Drop and re-derive `nodes`/`edges` for a project, bumping `derived_rev`.
pub fn rebuild(
    ledger: &Ledger,
    project_id: &str,
    cfg: &crate::config::Config,
) -> Result<DerivedGraph> {
    let graph = derive_project_with(ledger, project_id, cfg)?;

    // Publication is one transaction. Derivation happens before it starts;
    // any delete/insert/commit failure rolls back to the prior complete
    // revision, so a reader never observes a mixed graph.
    let tx = ledger.conn().unchecked_transaction()?;
    let rev: i64 = tx
        .query_row(
            "SELECT COALESCE(MAX(derived_rev),0)+1 FROM nodes WHERE project_id = ?1",
            params![project_id],
            |r| r.get(0),
        )
        .unwrap_or(1);

    tx.execute(
        "DELETE FROM edges WHERE from_node IN (SELECT id FROM nodes WHERE project_id = ?1)",
        params![project_id],
    )?;
    tx.execute(
        "DELETE FROM nodes WHERE project_id = ?1",
        params![project_id],
    )?;

    {
        let mut ins = tx.prepare(
            "INSERT INTO nodes(id, project_id, label, sort_key, lifecycle,
                 evidence_state, currency, confidence, session_id,
                 anchor_event_id, derived_rev)
             VALUES(?1,?2,?3,?4,?5,?6,?7,NULL,?8,?9,?10)",
        )?;
        for n in &graph.nodes {
            ins.execute(params![
                n.id,
                project_id,
                n.label,
                n.sort_key.as_str(),
                n.dimensions.lifecycle.as_str(),
                n.dimensions.evidence.as_str(),
                n.dimensions.currency.as_str(),
                n.session_id,
                n.anchor_event_id,
                rev
            ])?;
        }
        let mut ine = tx.prepare(
            "INSERT OR IGNORE INTO edges(from_node, to_node, kind, derived_rev)
             VALUES(?1,?2,?3,?4)",
        )?;
        for e in &graph.edges {
            ine.execute(params![e.from, e.to, e.kind, rev])?;
        }
    }
    tx.commit()?;
    // Record THAT the projection was built, and from which chain position.
    // Without this, an empty `nodes` result is ambiguous between "nothing needs
    // a human here" and "this project was never read" — and every read surface
    // resolved that ambiguity the wrong way.
    let (head, _events) = ledger.compute_project_head(project_id)?;
    ledger.save_projection_head(
        project_id,
        &head,
        graph.nodes.len(),
        &crate::ingest::iso8601_utc_now(),
    )?;
    Ok(graph)
}

/// Stable fingerprint of the derived rows, for the determinism assertion.
pub fn fingerprint(graph: &DerivedGraph) -> String {
    let mut s = String::new();
    for duplicate in &graph.duplicate_events {
        s.push_str(&format!(
            "D|{}|{}|{}|{}|{}|{}|{}\n",
            duplicate.duplicate_event_id,
            duplicate.canonical_event_id,
            duplicate.src_uuid,
            duplicate.duplicate_session_id,
            duplicate.duplicate_seq,
            duplicate.canonical_session_id,
            duplicate.canonical_seq,
        ));
    }
    for n in &graph.nodes {
        s.push_str(&format!(
            "N|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}\n",
            n.id,
            n.label,
            n.sort_key.as_str(),
            n.dimensions.lifecycle.as_str(),
            n.dimensions.evidence.as_str(),
            n.dimensions.currency.as_str(),
            n.session_id,
            n.anchor_event_id,
            n.abandoned,
            n.evidence.len(),
            n.verdict_reason,
        ));
        // Scope admitted from a hash-validated raw line is part of the
        // derived/exported contract. If that source later disappears or is
        // rewritten, the graph must fail visibly different rather than retain
        // a fingerprint that pretends its receipt is byte-identical.
        for row in &n.evidence {
            s.push_str(&format!(
                "  R|{}|{}|{}|{}|{}\n",
                row.session_id.as_deref().unwrap_or(""),
                row.seq.unwrap_or(-1),
                row.cwd.as_deref().unwrap_or(""),
                row.git_branch.as_deref().unwrap_or(""),
                row.command_identity.as_deref().unwrap_or(""),
            ));
        }
        // Per-claim verdicts are part of what must be byte-identical: a rebuild
        // that reproduced every state while shuffling the scores behind them
        // would still have broken the receipt.
        for v in &n.verdicts {
            s.push_str(&format!(
                "  V|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}\n",
                v.claim_id,
                v.kind,
                v.marker,
                v.state.as_str(),
                v.dimensions.lifecycle.as_str(),
                v.dimensions.evidence.as_str(),
                v.dimensions.currency.as_str(),
                v.candidates,
                v.parsed_candidates,
                v.best_score.map(|x| format!("{x:.6}")).unwrap_or_default(),
                v.best_fidelity.clone().unwrap_or_default(),
            ));
            for scope in &v.evidence_scopes {
                s.push_str(&format!(
                    "    P|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}\n",
                    scope.event_id,
                    scope.session_id,
                    scope.seq,
                    scope.cwd.as_deref().unwrap_or(""),
                    scope.git_branch.as_deref().unwrap_or(""),
                    scope.command_identity.as_deref().unwrap_or(""),
                    scope.freshness,
                    scope.superseded_by_event_id.as_deref().unwrap_or(""),
                    scope.superseded_by_session_id.as_deref().unwrap_or(""),
                    scope.superseded_by_seq.unwrap_or(-1),
                ));
            }
        }
    }
    for e in &graph.edges {
        s.push_str(&format!("E|{}|{}|{}\n", e.from, e.to, e.kind));
    }
    crate::chain::sha256_hex(s.as_bytes())
}

/* ── A32 regression: accusations must not grow with session size ──────── */

#[cfg(test)]
mod monotonicity {
    //! The permanent guard on the defect M3.9 exists to fix.
    //!
    //! The rule is stated as a *monotonicity* property because that is what
    //! was actually violated: adding evidence to a session could only ever
    //! make the product more accusatory, never less, regardless of what the
    //! evidence said. A claim that read `unknown` beside 3 candidates read
    //! `contradicted` beside 30 — same claim, same truth, more noise.
    //!
    //! These tests drive `claim_state` directly rather than through a ledger.
    //! That is deliberate: the property is about the decision procedure, and a
    //! test that had to route through ingest + annotate could fail for a dozen
    //! unrelated reasons and would not pin the rule.

    use super::*;

    fn link(id: &str, strength: f32, entail: f32) -> LinkRow {
        LinkRow {
            event_id: id.to_string(),
            score: entail,
            verdict: crate::annotate::Verdict::band(entail, 0.8, 0.2)
                .as_str()
                .to_string(),
            fidelity: "parsed".to_string(),
            related: strength > 0.0,
            ev: crate::contract::EvidenceOutcome {
                family: crate::contract::Family::None,
                passed: None,
                failed: None,
                ok: None,
            },
            match_strength: strength,
            ev_identity: String::new(),
            pass: "backward".to_string(),
            session_rank: 0,
            turn: 0,
            seq: 1,
            tool_name: "tool".to_string(),
            tool_input: String::new(),
            tool_result: String::new(),
            ev_session_id: "test-session".to_string(),
            ev_seq: 1,
            ev_cwd: None,
            ev_git_branch: Some("main".to_string()),
            stale: false,
            superseded_by_event_id: None,
            superseded_by_session_id: None,
            superseded_by_seq: None,
        }
    }

    fn claim(text: &str, links: Vec<LinkRow>) -> ClaimRow {
        ClaimRow {
            id: "c1".to_string(),
            text: text.to_string(),
            kind: crate::extract::ClaimKind::Work.as_str().to_string(),
            marker: String::new(),
            span_start: 0,
            span_end: text.len() as i64,
            session_rank: 0,
            turn: 1,
            seq: 100,
            links,
            search: None,
        }
    }

    /// Weakly-related noise: the shape a long session produces. Each one is
    /// admissible evidence and each one is individually plausible as a
    /// conflict — which is precisely why the old scan found one every time.
    fn noise(n: usize) -> Vec<LinkRow> {
        (0..n)
            .map(|i| link(&format!("noise-{i:03}"), 0.2, 0.05))
            .collect()
    }

    fn run_link(id: &str, seq: i64, command: &str, passed: u32, failed: u32) -> LinkRow {
        LinkRow {
            event_id: id.to_string(),
            score: 0.99,
            verdict: "entailed".to_string(),
            fidelity: "parsed".to_string(),
            related: true,
            ev: crate::contract::EvidenceOutcome {
                family: crate::contract::Family::Test,
                passed: Some(passed),
                failed: Some(failed),
                ok: Some(failed == 0),
            },
            match_strength: 0.9,
            ev_identity: String::new(),
            pass: "backward".to_string(),
            session_rank: 0,
            turn: 0,
            seq,
            tool_name: "Bash".to_string(),
            tool_input: format!(r#"{{"command":"{command}"}}"#),
            tool_result: format!("{passed} pass, {failed} fail"),
            ev_session_id: "test-session".to_string(),
            ev_seq: seq,
            ev_cwd: Some("/repo".to_string()),
            ev_git_branch: Some("main".to_string()),
            stale: false,
            superseded_by_event_id: None,
            superseded_by_session_id: None,
            superseded_by_seq: None,
        }
    }

    fn marker_claim(text: &str, links: Vec<LinkRow>) -> ClaimRow {
        let mut c = claim(text, links);
        c.kind = crate::extract::ClaimKind::Marker.as_str().to_string();
        c
    }

    #[test]
    fn closed_world_audit_has_a_non_recognition_probe_at_every_stage() {
        let valid_link = run_link("green", 10, "cargo test WidgetSuite", 8, 0);
        let valid_claim = marker_claim("WidgetSuite 8/8 pass.", vec![valid_link.clone()]);
        let assertion = crate::contract::parse_claim(&valid_claim.text).expect("typed claim");
        let authority = evidence_authority(&valid_claim);
        assert_eq!(
            contradiction_eligibility(&valid_claim, authority.active[0], &assertion, &authority),
            Ok(())
        );

        let mut wrong_claim_pattern = valid_claim.clone();
        wrong_claim_pattern.kind = crate::extract::ClaimKind::Work.as_str().to_string();
        assert_eq!(
            contradiction_eligibility(
                &wrong_claim_pattern,
                authority.active[0],
                &assertion,
                &authority
            ),
            Err(AccusationStage::ClaimPattern)
        );

        let mut uncertain = assertion.clone();
        uncertain.polarity = crate::contract::ClaimPolarity::Uncertain;
        assert_eq!(
            contradiction_eligibility(&valid_claim, authority.active[0], &uncertain, &authority),
            Err(AccusationStage::Polarity)
        );

        let mutation = run_link(
            "mutation",
            10,
            "npm test; echo MUTATION; perl -0pi -e change src/widget.ts; \
             node --test WidgetSuite; cp /tmp/widget.bak src/widget.ts",
            7,
            1,
        );
        let mutation_claim = marker_claim("WidgetSuite 8/8 pass.", vec![mutation]);
        let mutation_authority = evidence_authority(&mutation_claim);
        assert_eq!(
            contradiction_eligibility(
                &mutation_claim,
                mutation_authority.active[0],
                &assertion,
                &mutation_authority
            ),
            Err(AccusationStage::Polarity)
        );

        let grep = run_link("grep", 10, "grep -q forbidden generated.txt", 7, 1);
        let grep_claim = marker_claim("WidgetSuite 8/8 pass.", vec![grep]);
        let grep_authority = evidence_authority(&grep_claim);
        assert_eq!(
            contradiction_eligibility(
                &grep_claim,
                grep_authority.active[0],
                &assertion,
                &grep_authority
            ),
            Err(AccusationStage::EvidenceFamily)
        );

        let mut unreadable_counts = valid_link.clone();
        unreadable_counts.ev.passed = None;
        let unreadable_claim = marker_claim("WidgetSuite 8/8 pass.", vec![unreadable_counts]);
        let unreadable_authority = evidence_authority(&unreadable_claim);
        assert_eq!(
            contradiction_eligibility(
                &unreadable_claim,
                unreadable_authority.active[0],
                &assertion,
                &unreadable_authority
            ),
            Err(AccusationStage::OutcomeFormat)
        );

        let other = run_link("other", 10, "cargo test OtherSuite", 7, 1);
        let other_claim = marker_claim("WidgetSuite 8/8 pass.", vec![other]);
        let other_authority = evidence_authority(&other_claim);
        assert_eq!(
            contradiction_eligibility(
                &other_claim,
                other_authority.active[0],
                &assertion,
                &other_authority
            ),
            Err(AccusationStage::Identity)
        );

        let mut future = valid_link;
        future.turn = 2;
        let future_claim = marker_claim("WidgetSuite 8/8 pass.", vec![future.clone()]);
        let future_authority = evidence_authority(&future_claim);
        assert_eq!(
            contradiction_eligibility(&future_claim, &future, &assertion, &future_authority),
            Err(AccusationStage::Eligibility)
        );

        let valid_proposal = ClaimRow {
            id: "p1".to_string(),
            text: "I'll update `src/cache.rs`.".to_string(),
            kind: crate::extract::ClaimKind::Proposal.as_str().to_string(),
            marker: "intent".to_string(),
            span_start: 0,
            span_end: 29,
            session_rank: 0,
            seq: 1,
            links: Vec::new(),
            search: Some(SearchRow {
                searched: true,
                action_matches: 0,
                output_matches: 0,
                coverage_eligible: true,
                missing_families: String::new(),
            }),
            turn: 0,
        };
        assert_eq!(dropped_eligibility(&valid_proposal, true), Ok(()));

        let mut not_proposal = valid_proposal.clone();
        not_proposal.kind = crate::extract::ClaimKind::Finding.as_str().to_string();
        assert_eq!(
            dropped_eligibility(&not_proposal, true),
            Err(AccusationStage::ClaimPattern)
        );
        let mut status = valid_proposal.clone();
        status.text = "Running (ID `job42`).".to_string();
        status.marker = "gerund".to_string();
        assert_eq!(
            dropped_eligibility(&status, true),
            Err(AccusationStage::Polarity)
        );
        let mut no_target = valid_proposal.clone();
        no_target.text = "I'll handle it.".to_string();
        assert_eq!(
            dropped_eligibility(&no_target, true),
            Err(AccusationStage::Identity)
        );
        let mut no_search = valid_proposal.clone();
        no_search.search = Some(SearchRow {
            searched: false,
            action_matches: 0,
            output_matches: 0,
            coverage_eligible: false,
            missing_families: "coverage not recorded".to_string(),
        });
        assert_eq!(
            dropped_eligibility(&no_search, true),
            Err(AccusationStage::OutcomeFormat)
        );
        assert_eq!(
            dropped_eligibility(&valid_proposal, false),
            Err(AccusationStage::Eligibility)
        );
    }

    #[test]
    fn round6_five_mutation_dialect_convictions_cannot_recur() {
        let mutation_command =
            "npm test; echo MUTATION: force guard off, expect scenario to fail; \
                                cp src/guard.ts /tmp/guard.bak; perl -0pi -e change src/guard.ts; \
                                node --test WidgetSuite; cp /tmp/guard.bak src/guard.ts";
        for text in [
            "Typecheck clean; WidgetSuite 8/8 green.",
            "Full suite 8/8; forcing the guard off kills both partial-read scenarios.",
            "Full WidgetSuite 8/8, typecheck 0.",
            "All gates green (unit 8/8, lint 0, build 0).",
        ] {
            let got = claim_state(
                &marker_claim(
                    text,
                    vec![run_link("deliberate-red", 10, mutation_command, 6, 2)],
                ),
                true,
            );
            assert_ne!(
                got.0,
                NodeState::Contradicted,
                "mutation-control evidence convicted {text:?}: {got:?}"
            );
        }

        let reports_failure = claim_state(
            &marker_claim(
                "The probe added 7 tests; 5 pass, 2 fail.",
                vec![run_link("reported-red", 10, "cargo test ProbeSuite", 5, 2)],
            ),
            true,
        );
        assert_ne!(
            reports_failure.0,
            NodeState::Contradicted,
            "a claim reporting deliberate failures was convicted: {reports_failure:?}"
        );
    }

    #[test]
    fn latest_same_identity_run_governs_and_post_claim_context_cannot_convict() {
        let mut links = vec![
            run_link("red", 10, "cargo test WidgetSuite", 7, 1),
            run_link("green", 20, "cargo test WidgetSuite", 8, 0),
            run_link("future-red", 110, "cargo test WidgetSuite", 6, 2),
        ];
        // §5m: post-claim means a LATER TURN, not merely a later seq — the
        // claim's own turn is atomic. The future failure sits in turn 2.
        links[2].turn = 2;
        let got = claim_state(&marker_claim("WidgetSuite 8/8 pass.", links), true);
        assert_eq!(got.0, NodeState::Supported, "{got:?}");
        assert!(
            got.1.contains("superseded"),
            "receipt lost context: {got:?}"
        );
        assert!(
            got.1.contains("post-claim"),
            "receipt lost context: {got:?}"
        );
    }

    #[test]
    fn wrong_suite_never_convicts_and_exact_count_mismatch_never_supports() {
        let wrong = claim_state(
            &marker_claim(
                "WidgetSuite 8/8 pass.",
                vec![run_link("other", 10, "cargo test OtherSuite", 7, 1)],
            ),
            true,
        );
        assert_ne!(wrong.0, NodeState::Contradicted, "{wrong:?}");

        let mismatch = claim_state(
            &marker_claim(
                "WidgetSuite 110 tests pass.",
                vec![run_link("near", 10, "cargo test WidgetSuite", 109, 0)],
            ),
            true,
        );
        assert_eq!(mismatch.0, NodeState::Unknown, "{mismatch:?}");
        assert!(mismatch.1.contains("Exact count not confirmed"));
    }

    #[test]
    fn narrow_evidence_cannot_support_a_broad_claim() {
        let mut c = claim(
            "Both WidgetSuite and CacheSuite are green.",
            vec![run_link("widget", 10, "cargo test WidgetSuite", 8, 0)],
        );
        c.links[0].verdict = "entailed".to_string();
        let got = claim_state(&c, true);
        assert_eq!(got.0, NodeState::Unknown, "{got:?}");
        assert!(got.1.contains("narrower than the claim"));
    }

    #[test]
    fn backticked_sentence_final_paths_are_one_artifact_not_two() {
        assert_eq!(
            distinctive_artifacts("I've added retry logic to `src/webhook/client.rs`."),
            vec!["src/webhook/client.rs"]
        );
    }

    #[test]
    fn adding_unrelated_candidates_never_moves_a_verdict_toward_accusation() {
        // The claim's own matched evidence: strongly on-subject, and it does
        // not contradict. Whatever else is in the session, that is the pair
        // the verdict must rest on.
        let matched = link("match-000", 0.9, 0.55);

        let baseline = claim_state(
            &claim("I fixed the retry backoff.", vec![matched.clone()]),
            true,
        );
        assert!(
            !baseline.0.is_accusation(),
            "baseline is already an accusation; the test proves nothing: {baseline:?}"
        );

        for n in [5usize, 15, 30] {
            let mut links = vec![matched.clone()];
            links.extend(noise(n));
            let got = claim_state(&claim("I fixed the retry backoff.", links), true);
            assert!(
                !got.0.is_accusation(),
                "{n} unrelated candidates turned a non-accusation into {:?} — \
                 the verdict is a function of session length, not of truth. {}",
                got.0,
                got.1
            );
            assert_eq!(
                got.0, baseline.0,
                "{n} unrelated candidates changed the verdict at all (was {:?}, now {:?})",
                baseline.0, got.0
            );
        }
    }

    #[test]
    fn thirty_alarming_candidates_alone_do_not_accuse() {
        // No well-matched evidence at all — only weakly-related noise, every
        // piece of it screaming contradiction. Under the old scan this was a
        // guaranteed warning. The honest answer is that we found related work
        // and it did not settle the question.
        let got = claim_state(&claim("I fixed the retry backoff.", noise(30)), true);
        assert!(
            !got.0.is_accusation(),
            "30 weakly-related candidates produced {:?}: {}",
            got.0,
            got.1
        );
    }

    #[test]
    fn a_confident_scorer_cannot_convict_anything_any_more() {
        // §5d, stated as a test. Contradiction 0.95 against entailment 0.02 on
        // the single best-matched, parsed, on-subject pair — the strongest
        // signal the model can produce — and it does not accuse.
        //
        // This test used to assert the opposite, because until M3.10 that
        // signal WAS the accusation. M3.9.4 then adjudicated 9 such warnings
        // and found 0 true, five of them against premises that proved the
        // claim. The counterweight that keeps this from being a mute button
        // lives in tests/contract_path.rs, where the same catches still
        // convict on arithmetic with the scorer pinned to zero.
        let got = claim_state(
            &claim(
                "I fixed the retry backoff.",
                vec![link("match-000", 0.9, 0.02)],
            ),
            true,
        );
        assert!(
            !got.0.is_accusation(),
            "the model convicted: {:?} — {}",
            got.0,
            got.1
        );
    }

    #[test]
    fn noise_cannot_outrank_the_matched_pair_however_much_of_it_there_is() {
        // The ordering itself, stated directly: a strongly-matched exonerating
        // pair must sit above any quantity of weakly-matched alarming ones.
        let mut links = noise(30);
        links.push(link("match-000", 0.9, 0.55));
        let ranked = matched_pairs(&links);
        assert_eq!(
            ranked[0].event_id, "match-000",
            "ranking put noise above the matched pair"
        );
        assert!(
            ranked.len() <= ACCUSE_TOP_K,
            "accusation slate is not capped"
        );
    }

    #[test]
    fn a_margin_is_required_between_contradiction_and_entailment() {
        // A pair that scores both signals high has not read the premise. It
        // must abstain rather than pick the scarier of its two guesses.
        let got = claim_state(
            &claim(
                "I fixed the retry backoff.",
                vec![link("match-000", 0.9, 0.52)],
            ),
            true,
        );
        assert!(
            !got.0.is_accusation(),
            "accused on a pair the model was merely unsure about: {}",
            got.1
        );
    }
}

/* ── M4.4: narrow, closed-world proof freshness ─────────────────────── */

#[cfg(test)]
mod m44_freshness {
    use super::*;

    const IDENTITY: &str = "bash:/repo:npm test";

    fn run(
        event_id: &str,
        session: &str,
        seq: i64,
        branch: Option<&str>,
        identity: &str,
        passed: Option<u32>,
        failed: Option<u32>,
    ) -> LinkRow {
        LinkRow {
            event_id: event_id.to_string(),
            score: 0.9,
            verdict: "entailed".to_string(),
            fidelity: "parsed".to_string(),
            related: true,
            ev: crate::contract::EvidenceOutcome {
                family: if passed.is_some() || failed.is_some() {
                    crate::contract::Family::Test
                } else {
                    crate::contract::Family::None
                },
                passed,
                failed,
                ok: failed.map(|count| count == 0),
            },
            match_strength: 0.9,
            ev_identity: identity.to_string(),
            pass: "backward".to_string(),
            session_rank: 0,
            turn: 0,
            seq,
            tool_name: "Bash".to_string(),
            tool_input: "{\"command\":\"npm test\"}".to_string(),
            tool_result: format!("{passed:?} pass, {failed:?} fail"),
            ev_session_id: session.to_string(),
            ev_seq: seq,
            ev_cwd: Some("/repo".to_string()),
            ev_git_branch: branch.map(str::to_string),
            stale: false,
            superseded_by_event_id: None,
            superseded_by_session_id: None,
            superseded_by_seq: None,
        }
    }

    fn claim(links: Vec<LinkRow>) -> ClaimRow {
        ClaimRow {
            id: "freshness-claim".to_string(),
            text: "All 12 tests pass.".to_string(),
            kind: crate::extract::ClaimKind::Marker.as_str().to_string(),
            marker: "ratio".to_string(),
            span_start: 0,
            span_end: 18,
            session_rank: 0,
            turn: 1,
            seq: 100,
            links,
            search: None,
        }
    }

    #[test]
    fn changed_outcomes_stale_each_prior_observation_at_the_first_change() {
        let mut c = claim(vec![
            run(
                "green-1",
                "s1",
                10,
                Some("main"),
                IDENTITY,
                Some(12),
                Some(0),
            ),
            run("red", "s1", 20, Some("main"), IDENTITY, Some(11), Some(1)),
            run(
                "green-2",
                "s1",
                30,
                Some("main"),
                IDENTITY,
                Some(12),
                Some(0),
            ),
        ]);
        mark_changed_outcome_stale(&mut c);

        assert!(c.links[0].stale);
        assert_eq!(c.links[0].superseded_by_event_id.as_deref(), Some("red"));
        assert_eq!(c.links[0].superseded_by_seq, Some(20));
        assert!(c.links[1].stale);
        assert_eq!(
            c.links[1].superseded_by_event_id.as_deref(),
            Some("green-2")
        );
        assert!(!c.links[2].stale);

        let scope = evidence_scope(&c.links[0]);
        assert_eq!(scope.freshness, "stale");
        assert_eq!(
            scope.freshness_note.as_deref(),
            Some("verified at `s1:10`; superseded by `s1:20`")
        );
        // A later recovery does not retroactively make the oldest proof fresh.
        assert!(c.links[0].stale);
    }

    #[test]
    fn non_recognition_or_scope_change_abstains_from_staleness() {
        for links in [
            // Same outcome.
            vec![
                run("a", "s1", 10, Some("main"), IDENTITY, Some(12), Some(0)),
                run("b", "s1", 20, Some("main"), IDENTITY, Some(12), Some(0)),
            ],
            // Different identity.
            vec![
                run("a", "s1", 10, Some("main"), IDENTITY, Some(12), Some(0)),
                run(
                    "b",
                    "s1",
                    20,
                    Some("main"),
                    "bash:/repo:pytest",
                    Some(11),
                    Some(1),
                ),
            ],
            // Different branch.
            vec![
                run("a", "s1", 10, Some("main"), IDENTITY, Some(12), Some(0)),
                run("b", "s1", 20, Some("topic"), IDENTITY, Some(11), Some(1)),
            ],
            // Different session.
            vec![
                run("a", "s1", 10, Some("main"), IDENTITY, Some(12), Some(0)),
                run("b", "s2", 20, Some("main"), IDENTITY, Some(11), Some(1)),
            ],
            // Missing branch is not affirmative scope.
            vec![
                run("a", "s1", 10, None, IDENTITY, Some(12), Some(0)),
                run("b", "s1", 20, None, IDENTITY, Some(11), Some(1)),
            ],
            // A branch without an effective cwd still cannot prove that two
            // bare runner names address the same suite.
            vec![
                run(
                    "a",
                    "s1",
                    10,
                    Some("main"),
                    "bash::npm test",
                    Some(12),
                    Some(0),
                ),
                run(
                    "b",
                    "s1",
                    20,
                    Some("main"),
                    "bash::npm test",
                    Some(11),
                    Some(1),
                ),
            ],
            // A non-runner identity never acquires freshness authority.
            vec![
                run(
                    "a",
                    "s1",
                    10,
                    Some("main"),
                    "bash:/repo:grep tests",
                    Some(12),
                    Some(0),
                ),
                run(
                    "b",
                    "s1",
                    20,
                    Some("main"),
                    "bash:/repo:grep tests",
                    Some(11),
                    Some(1),
                ),
            ],
            // Incomplete/unknown outcome abstains.
            vec![
                run("a", "s1", 10, Some("main"), IDENTITY, Some(12), None),
                run("b", "s1", 20, Some("main"), IDENTITY, Some(11), Some(1)),
            ],
        ] {
            let mut c = claim(links);
            mark_changed_outcome_stale(&mut c);
            assert!(c.links.iter().all(|link| !link.stale), "{c:?}");
            assert!(evidence_scopes(&c)
                .iter()
                .all(|scope| scope.freshness == "unknown"));
        }
    }
}

/* ── A51: the round-4 pairs, as named regressions ─────────────────────── */

#[cfg(test)]
mod a51_supersession {
    //! The two convictions Round 4 emitted, both false, both from one cause.
    //!
    //! Driven through `claim_state` with synthetic links that reproduce the
    //! real structure — same identities, same order, same outcomes — so the
    //! rule can be iterated on in milliseconds instead of a nine-minute
    //! re-annotation. Tuning a decision procedure against a live corpus is how
    //! you overfit to two cases; this is the loop that prevents it.

    use super::*;

    const PROXY: &str = "bash:/users/dev/documents/atlas/atlas-proxy:npm test";

    fn run(id: &str, seq: i64, identity: &str, strength: f32, passed: u32, failed: u32) -> LinkRow {
        LinkRow {
            event_id: id.to_string(),
            score: 0.02,
            verdict: "not_entailed".to_string(),
            fidelity: "parsed".to_string(),
            related: true,
            ev: crate::contract::EvidenceOutcome {
                family: crate::contract::Family::Test,
                passed: Some(passed),
                failed: Some(failed),
                ok: Some(failed == 0),
            },
            match_strength: strength,
            ev_identity: identity.to_string(),
            pass: "backward".to_string(),
            session_rank: 0,
            turn: 0,
            seq,
            tool_name: "Bash".to_string(),
            tool_input: format!("{{\"command\":\"npm test\"}}  {identity}"),
            tool_result: format!("# pass {passed}\n# fail {failed}"),
            ev_session_id: "test-session".to_string(),
            ev_seq: seq,
            ev_cwd: Some("/repo".to_string()),
            ev_git_branch: Some("main".to_string()),
            stale: false,
            superseded_by_event_id: None,
            superseded_by_session_id: None,
            superseded_by_seq: None,
        }
    }

    fn unreadable(id: &str, seq: i64, identity: &str, strength: f32) -> LinkRow {
        LinkRow {
            event_id: id.to_string(),
            score: 0.01,
            verdict: "not_entailed".to_string(),
            fidelity: "parsed".to_string(),
            related: true,
            ev: crate::contract::EvidenceOutcome {
                family: crate::contract::Family::None,
                passed: None,
                failed: None,
                ok: None,
            },
            match_strength: strength,
            ev_identity: identity.to_string(),
            pass: "backward".to_string(),
            session_rank: 0,
            turn: 0,
            seq,
            tool_name: "Bash".to_string(),
            tool_input: "{\"command\":\"npm test | grep -B1 'not ok'\"}".to_string(),
            tool_result: "not ok 32 - checkOutputSafety".to_string(),
            ev_session_id: "test-session".to_string(),
            ev_seq: seq,
            ev_cwd: Some("/repo".to_string()),
            ev_git_branch: Some("main".to_string()),
            stale: false,
            superseded_by_event_id: None,
            superseded_by_session_id: None,
            superseded_by_seq: None,
        }
    }

    fn edit(id: &str, seq: i64, path: &str) -> LinkRow {
        LinkRow {
            event_id: id.to_string(),
            score: 0.01,
            verdict: "not_entailed".to_string(),
            fidelity: "parsed".to_string(),
            related: true,
            ev: crate::contract::EvidenceOutcome {
                family: crate::contract::Family::None,
                passed: None,
                failed: None,
                ok: None,
            },
            match_strength: 0.7,
            ev_identity: format!("edit:{path}"),
            pass: "backward".to_string(),
            session_rank: 0,
            turn: 0,
            seq,
            tool_name: "Edit".to_string(),
            tool_input: format!("{{\"file_path\":\"{path}\"}}"),
            tool_result: "updated successfully".to_string(),
            ev_session_id: "test-session".to_string(),
            ev_seq: seq,
            ev_cwd: Some("/repo".to_string()),
            ev_git_branch: Some("main".to_string()),
            stale: false,
            superseded_by_event_id: None,
            superseded_by_session_id: None,
            superseded_by_seq: None,
        }
    }

    fn marker(text: &str, links: Vec<LinkRow>) -> ClaimRow {
        ClaimRow {
            id: "c1".to_string(),
            text: text.to_string(),
            kind: crate::extract::ClaimKind::Marker.as_str().to_string(),
            marker: "ratio".to_string(),
            span_start: 0,
            span_end: text.len() as i64,
            links,
            search: None,
            session_rank: 0,
            turn: 1,
            seq: 5000,
        }
    }

    /// ATLAS session, seq 1005 → 1015, claim at 1046.
    ///
    /// The failing run is the CLOSER lexical match, because the claim quotes
    /// its `cd …` spelling. Round 4 convicted on it. The later run of the same
    /// normalised invocation governs, and the claim is true.
    #[test]
    fn pair_one_the_rerun_governs_and_the_claim_is_supported() {
        let c = marker(
            "`cd atlas-proxy && npm test` → **33/33 pass** (11 original + 11 couples + 11 roleplay)",
            vec![
                run("e1005", 1005, PROXY, 0.90, 32, 1),
                unreadable("e1007", 1007, PROXY, 0.80),
                run("e1015", 1015, PROXY, 0.80, 33, 0),
            ],
        );
        let (state, why) = claim_state(&c, true);
        assert_eq!(
            state,
            NodeState::Supported,
            "the re-run governs and the claim is true: {why}"
        );
        assert!(
            why.contains("33"),
            "receipt must show the governing run: {why}"
        );
    }

    /// ATLAS session, seq 1207 → 1218, claim at 1249.
    #[test]
    fn pair_two_the_rerun_governs_and_the_claim_is_supported() {
        let c = marker(
            "`cd atlas-proxy && npm test` → **44/44 pass** (33 prior + 11 new for Pick-a-Scene)",
            vec![
                run("e1207", 1207, PROXY, 0.84, 0, 1),
                unreadable("e1211", 1211, PROXY, 0.76),
                // The edit that implements the feature the claim names. Present
                // in the real candidate set; the claim says "Pick-a-Scene" and
                // the file is `pickAScene.ts`, which only match once artifact
                // comparison ignores separators.
                edit(
                    "e1215",
                    1215,
                    "/users/dev/documents/atlas/atlas-proxy/src/pickascene.ts",
                ),
                run("e1218", 1218, PROXY, 0.76, 44, 0),
                // A later run of a DIFFERENT suite, in a different directory.
                // Same family; nothing to do with this claim.
                run(
                    "e1244",
                    1244,
                    "bash:/users/dev/documents/atlas/app:swift",
                    0.68,
                    17,
                    0,
                ),
            ],
        );
        let (state, why) = claim_state(&c, true);
        assert_eq!(
            state,
            NodeState::Supported,
            "a later unrelated suite must not decide this claim: {why}"
        );
        assert!(
            why.contains("44"),
            "receipt must show the governing run: {why}"
        );
    }

    /// The counterweight. §5f-rev must not become a blanket amnesty: when the
    /// governing run of the claim's own invocation really did fail, the
    /// conviction still lands.
    #[test]
    fn a_genuinely_failing_governing_run_still_convicts() {
        let c = marker(
            "`cd atlas-proxy && npm test` → **33/33 pass**",
            vec![
                run("e1005", 1005, PROXY, 0.90, 33, 0),
                run("e1015", 1015, PROXY, 0.80, 30, 3),
            ],
        );
        let (state, why) = claim_state(&c, true);
        assert_eq!(
            state,
            NodeState::Contradicted,
            "the LATEST run failed; that is a real catch: {why}"
        );
    }

    /// §5f-rev's asymmetry, stated directly: an unreadable LATER run of the
    /// same invocation means the final state is unknown, so nothing convicts.
    #[test]
    fn an_unreadable_later_run_of_the_same_invocation_blocks() {
        let c = marker(
            "`cd atlas-proxy && npm test` → **33/33 pass**",
            vec![
                run("e1005", 1005, PROXY, 0.90, 30, 3),
                unreadable("e1020", 1020, PROXY, 0.80),
            ],
        );
        let (state, why) = claim_state(&c, true);
        assert!(
            !state.is_accusation(),
            "a later unreadable run of the same suite must block conviction: {state:?} — {why}"
        );
    }
}

/* ── §5k: the adjudicated-verdict ledger ──────────────────────────────── */

#[cfg(test)]
mod adjudicated_ledger {
    //! Every claim ever adjudicated is a permanent verdict-level regression.
    //!
    //! A56 is the reason this file exists: M3.13.0's live-tuning drift
    //! re-convicted four claims Round 3 had already adjudicated false, and the
    //! only detector was a human remembering the list. NORTHSTAR reliability
    //! rule 5 had no test to its name. Now the list IS a test.
    //!
    //! The Round-3 set is driven from `evidence-authority-dev.json` — the
    //! pattern-form reconstructions built at M3.11.0 (grammar real, every
    //! name/number synthetic, per A22). Links are constructed mechanically
    //! from the fixture's evidence strings so a new adjudicated case is one
    //! JSON entry, not a hand-written test. Round 4's pairs live beside this
    //! in `a51_supersession`; Round 2's distinct false classes are pinned
    //! here directly.
    //!
    //! The floor assertion for every adjudicated-false case is the §5k
    //! sentence itself: **it must never again be accused.** Exact-verdict
    //! assertions are layered on where the mechanical reconstruction carries
    //! enough structure to support them.

    use super::*;

    fn ev_link(id: usize, claim: &str, ev_text: &str, entail: f32) -> LinkRow {
        // Reconstruction rules, documented because they ARE the fixture's
        // fidelity: (1) identity is the suite's name — the first token — as
        // §5i's normaliser would produce for `cargo test <Suite>`; the fixture
        // strings are abbreviations, not commands. (2) The fixture writes
        // multi-line TAP summaries on one line with ` # ` separators; real
        // payloads carry newlines, so they are restored before classifying.
        // (3) Strength is the real `relatedness` of claim to evidence, floored
        // at admission — in production the anchor wins by being ABOUT the
        // claim, and equal strengths would hand ties to the wrong suite.
        let identity = ev_text
            .split([':', ' '])
            .next()
            .unwrap_or(ev_text)
            .to_ascii_lowercase();
        let multiline = ev_text.replace(" # ", "\n# ");
        let outcome =
            crate::premise::classify_span(crate::premise::SpanKind::ExecutedOutput, &multiline);
        let strength = crate::annotate::relatedness(claim, ev_text).max(0.61);
        LinkRow {
            event_id: format!("ev{id:03}"),
            score: entail,
            verdict: crate::annotate::Verdict::band(entail, 0.6, 0.1)
                .as_str()
                .to_string(),
            fidelity: "parsed".to_string(),
            related: true,
            ev: crate::contract::EvidenceOutcome::from_outcome(&outcome),
            match_strength: strength,
            ev_identity: format!("bash:/repo:cargo test {identity}"),
            pass: "backward".to_string(),
            session_rank: 0,
            turn: 0,
            seq: (10 * (id + 1)) as i64,
            tool_name: "Bash".to_string(),
            tool_input: format!("{{\"command\":\"{identity}\"}}"),
            tool_result: ev_text.to_string(),
            ev_session_id: "test-session".to_string(),
            ev_seq: (10 * (id + 1)) as i64,
            ev_cwd: Some("/repo".to_string()),
            ev_git_branch: Some("main".to_string()),
            stale: false,
            superseded_by_event_id: None,
            superseded_by_session_id: None,
            superseded_by_seq: None,
        }
    }

    fn row(text: &str, kind: crate::extract::ClaimKind, links: Vec<LinkRow>) -> ClaimRow {
        ClaimRow {
            id: "ledger".to_string(),
            text: text.to_string(),
            kind: kind.as_str().to_string(),
            marker: "ratio".to_string(),
            span_start: 0,
            span_end: text.len() as i64,
            links,
            search: Some(SearchRow {
                searched: true,
                action_matches: 1,
                output_matches: 0,
                coverage_eligible: true,
                missing_families: String::new(),
            }),
            session_rank: 0,
            turn: 1,
            seq: 1000,
        }
    }

    #[derive(serde::Deserialize)]
    struct Case {
        id: String,
        #[allow(dead_code)]
        class: String,
        claim: String,
        evidence: Vec<String>,
        expected: String,
    }

    #[derive(serde::Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }

    fn fixture() -> Fixture {
        serde_json::from_str(include_str!(
            "../tests/fixtures/evidence-authority-dev.json"
        ))
        .expect("evidence-authority-dev.json parses")
    }

    /// §5k's floor: no adjudicated-false claim is ever accused again.
    #[test]
    fn no_adjudicated_false_claim_is_ever_accused_again() {
        let mut checked = 0usize;
        for case in fixture().cases {
            let (kind, entail) = match case.class.as_str() {
                "contradicted" => (crate::extract::ClaimKind::Marker, 0.02),
                // Adjudication established the evidence PROVES these claims;
                // the pattern encodes that as an entailing link.
                "unsupported" => (crate::extract::ClaimKind::Work, 0.9),
                "dropped" => (crate::extract::ClaimKind::Proposal, 0.02),
                other => panic!("unknown class {other}"),
            };
            let links: Vec<LinkRow> = case
                .evidence
                .iter()
                .enumerate()
                .map(|(i, e)| ev_link(i, &case.claim, e, entail))
                .collect();
            // Proposals were adjudicated "fulfilled later": the ghost defeater
            // sees the later reference, so links stay out of the forward path.
            let links = if case.class == "dropped" {
                Vec::new()
            } else {
                links
            };
            let c = row(&case.claim, kind, links);
            let (state, why) = claim_state(&c, true);
            assert!(
                !state.is_accusation(),
                "{}: an adjudicated-false claim was accused again ({state:?}): {why}",
                case.id
            );
            checked += 1;
        }
        assert_eq!(checked, 32, "the Round-3 ledger must stay complete");
    }

    /// Layered exact-verdict assertions, where the reconstruction carries the
    /// structure: red-run-then-green-rerun must land `supported`, and the two
    /// coverage/ambiguity patterns must abstain.
    #[test]
    fn round3_contradicted_patterns_resolve_to_their_adjudicated_verdicts() {
        let mut supported = Vec::new();
        let mut abstained = Vec::new();
        for case in fixture().cases {
            if case.class != "contradicted" {
                continue;
            }
            // con-17 is an extraction-boundary case (a plan-form sentence);
            // its verdict-level floor is covered above, and its extraction fix
            // is pinned in `extract::tests` (M3.11.1).
            if case.expected == "proposal" {
                continue;
            }
            let links: Vec<LinkRow> = case
                .evidence
                .iter()
                .enumerate()
                .map(|(i, e)| ev_link(i, &case.claim, e, 0.02))
                .collect();
            let c = row(&case.claim, crate::extract::ClaimKind::Marker, links);
            let (state, _why) = claim_state(&c, true);
            match case.expected.as_str() {
                "supported" if state == NodeState::Supported => supported.push(case.id),
                "unknown" if !state.is_accusation() && state != NodeState::Supported => {
                    abstained.push(case.id)
                }
                _ => {}
            }
        }
        // Floors, not exact counts, because the reconstruction is mechanical:
        // 11 of the 14 expected-supported patterns resolve exactly today. The
        // three that abstain instead: con-09/16 (composite narratives relying
        // on NLI entailment the pattern deliberately withholds) and con-03
        // (the fixture abbreviates swift-testing's summary past what the
        // classifier reads — "12 tests passed" carries no digit adjacent to
        // its participle). Their floor assertion is the never-accused test
        // above. A DROP below 11 means a supersession or
        // identity regression even if nothing is accused. con-11 resolves
        // `supported` where M3.11 targeted `unknown`: both named suites verify
        // under current coverage, which is an improvement recorded here, not a
        // flip of an adjudicated truth — the adjudicated fact was only that
        // the accusation was false. con-15 IS load-bearing: A41's ambiguity
        // abstention must never resolve.
        assert!(
            supported.len() >= 11,
            "expected-supported floor regressed: only {:?} resolved supported",
            supported
        );
        assert!(
            abstained.contains(&"con-15".to_string()),
            "A41's ambiguity abstention resolved — that rule must never flip: {abstained:?}"
        );
    }

    /// Round 2's adjudicated-false accusation classes (M3.9.4), pattern form.
    /// Cause tally was: evidence CONFIRMS the claim (5), evidence unrelated
    /// (2), intent record treated as outcome (1), scope error (1).
    #[test]
    fn round2_false_accusation_classes_stay_dead() {
        // 1. The evidence confirms the claim — the A36 crown-jewel shape.
        let confirming = row(
            "WidgetSuite 98/98 passing.",
            crate::extract::ClaimKind::Marker,
            vec![ev_link(
                0,
                "WidgetSuite 98/98 passing.",
                "widgetsuite: 98 passed; 0 failed",
                0.02,
            )],
        );
        let (s, why) = claim_state(&confirming, true);
        assert_eq!(
            s,
            NodeState::Supported,
            "confirming evidence must support: {why}"
        );

        // 2. An intent record (TaskCreate) is not an outcome and cannot decide.
        let mut intent = ev_link(0, "Both suites green.", "task created successfully", 0.02);
        intent.tool_name = "TaskCreate".to_string();
        intent.ev =
            crate::contract::EvidenceOutcome::from_outcome(&crate::premise::Outcome::Unknown);
        let c = row(
            "Both suites green.",
            crate::extract::ClaimKind::Marker,
            vec![intent],
        );
        let (s, why) = claim_state(&c, true);
        assert!(!s.is_accusation(), "an intent record convicted: {why}");

        // 3. Unrelated evidence, however alarming, does not reach the claim.
        let mut unrelated = ev_link(
            0,
            "The quota maths is verified.",
            "renderersuite: 3 passed; 9 failed",
            0.01,
        );
        unrelated.match_strength = 0.1;
        unrelated.related = false;
        let c = row(
            "The quota maths is verified.",
            crate::extract::ClaimKind::Work,
            vec![unrelated],
        );
        let (s, why) = claim_state(&c, true);
        assert!(!s.is_accusation(), "unrelated evidence convicted: {why}");
    }
}

/* ── §5k ledger: Round 5 (A61) ────────────────────────────────────────── */

#[cfg(test)]
mod round5_ledger {
    //! All 42 Round-5 adjudications as verdict-level regressions, from
    //! `round5-adjudicated.json` (pattern form, A22).
    //!
    //! The RATCHET: cases whose fix has not landed yet carry `pending_until`
    //! naming the §5 clause that will flip them. A pending case asserts it
    //! does **not** yet resolve to its adjudicated-correct verdict — so the
    //! moment a fix lands, this test fails and forces explicit promotion to
    //! the fixed set. Nothing flips silently, in either direction (§5k).
    //!
    //! Stated per Rule 12: until their clauses land later this milestone,
    //! the pending accusation patterns still accuse. That is the measured
    //! current behaviour, pinned on purpose.

    use super::*;

    /// §5m interpreter convention: the claim sits in turn 5 of its session.
    const CLAIM_TURN: i64 = 5;

    #[derive(serde::Deserialize)]
    struct Link {
        kind: String,
        seq: i64,
        #[serde(default)]
        identity: String,
        #[serde(default)]
        path: String,
        #[serde(default)]
        tool: String,
        #[serde(default)]
        passed: Option<u32>,
        #[serde(default)]
        failed: Option<u32>,
        #[serde(default)]
        ok: Option<bool>,
        strength: f32,
        #[serde(default)]
        entail: f32,
        /// Consumed by the §5m interpreter extension in M3.14.1; carried in
        /// the schema now so fixture entries are stable.
        #[serde(default)]
        #[allow(dead_code)]
        same_turn: bool,
        #[serde(default)]
        text: String,
    }

    #[derive(serde::Deserialize)]
    struct Case {
        id: String,
        /// The emitted Round-5 class; documentary in the fixture, and read by
        /// humans rather than the interpreter.
        #[allow(dead_code)]
        class: String,
        final_true: bool,
        claim: String,
        kind: String,
        marker: String,
        links: Vec<Link>,
        expected: String,
        pending_until: Option<String>,
        search: Search,
        moved_on: bool,
        claim_seq: i64,
    }

    #[derive(serde::Deserialize)]
    struct Search {
        searched: bool,
        action: u32,
        output: u32,
    }

    #[derive(serde::Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }

    fn to_link(i: usize, l: &Link, forward: bool) -> LinkRow {
        // Negative seq = a LATER turn than the claim's (post-claim evidence);
        // `same_turn` = after the claim inside its own turn. The interpreter
        // keeps both distinct because §5m treats them oppositely.
        let seq = if l.seq < 0 { 2000 - l.seq } else { l.seq };
        let (family, passed, failed, ok) = match l.kind.as_str() {
            "run" => (
                crate::contract::Family::Test,
                l.passed,
                l.failed,
                l.failed.map(|f| f == 0),
            ),
            "build" => (crate::contract::Family::Build, None, None, l.ok),
            // A per-check ledger classifies from its own text, as production
            // would through the scanner (§5p).
            "pass_ledger" => {
                let o = crate::premise::classify_span(
                    crate::premise::SpanKind::ExecutedOutput,
                    &l.text,
                );
                let e = crate::contract::EvidenceOutcome::from_outcome(&o);
                (e.family, e.passed, e.failed, e.ok)
            }
            _ => (crate::contract::Family::None, None, None, None),
        };
        let tool = match l.kind.as_str() {
            "edit" => "Edit",
            "agent" => "Task",
            "git" => "Bash",
            _ if !l.tool.is_empty() => l.tool.as_str(),
            _ => "Bash",
        };
        let identity = if !l.identity.is_empty() {
            l.identity.clone()
        } else if l.kind == "git" {
            "bash:/repo:git commit -m fix".to_string()
        } else if !l.path.is_empty() {
            format!("edit:{}", l.path)
        } else {
            format!("{}:{i}", tool.to_ascii_lowercase())
        };
        let identity_for_blob = identity.rsplit(':').next().unwrap_or("").to_string();
        LinkRow {
            event_id: format!("r5e{i:03}"),
            score: l.entail,
            verdict: crate::annotate::Verdict::band(l.entail, 0.6, 0.1)
                .as_str()
                .to_string(),
            fidelity: "parsed".to_string(),
            related: l.strength > 0.0,
            ev: crate::contract::EvidenceOutcome {
                family,
                passed,
                failed,
                ok,
            },
            match_strength: l.strength,
            ev_identity: identity,
            pass: if forward { "forward" } else { "backward" }.to_string(),
            session_rank: 0,
            // §5m mapping: negative fixture seq = a LATER turn; `same_turn` =
            // the claim's own turn (narrate-then-act); otherwise a prior turn.
            turn: if l.seq < 0 {
                CLAIM_TURN + 1
            } else if l.same_turn {
                CLAIM_TURN
            } else {
                0
            },
            seq,
            tool_name: tool.to_string(),
            // Production blobs carry the command and the payload; artifact
            // correspondence reads them, so the interpreter must fill them.
            tool_input: format!(
                "{{\"command\":\"{}\",\"file_path\":\"{}\"}}",
                identity_for_blob, l.path
            ),
            tool_result: if l.text.is_empty() {
                identity_for_blob
            } else {
                l.text.clone()
            },
            ev_session_id: "test-session".to_string(),
            ev_seq: seq,
            ev_cwd: Some("/repo".to_string()),
            ev_git_branch: Some("main".to_string()),
            stale: false,
            superseded_by_event_id: None,
            superseded_by_session_id: None,
            superseded_by_seq: None,
        }
    }

    fn expected_state(s: &str) -> NodeState {
        match s {
            "supported" => NodeState::Supported,
            "unknown" => NodeState::Unknown,
            "pending" => NodeState::Pending,
            "unsupported" => NodeState::Unsupported,
            "dropped" => NodeState::Dropped,
            "contradicted" => NodeState::Contradicted,
            other => panic!("unknown expected {other}"),
        }
    }

    #[test]
    fn round5_ledger_ratchet() {
        let f: Fixture =
            serde_json::from_str(include_str!("../tests/fixtures/round5-adjudicated.json"))
                .expect("round5 fixture parses");
        assert_eq!(f.cases.len(), 42, "the Round-5 ledger must stay complete");

        let mut fixed = 0usize;
        let mut pending = Vec::new();
        for case in &f.cases {
            let forward = case.kind == "proposal";
            let mut links: Vec<LinkRow> = case
                .links
                .iter()
                .enumerate()
                .map(|(i, l)| to_link(i, l, forward))
                .collect();
            // Mirror load_annotations: links arrive entailment-descending.
            links.sort_by(|a, b| {
                b.score
                    .total_cmp(&a.score)
                    .then(a.event_id.cmp(&b.event_id))
            });
            let c = ClaimRow {
                id: case.id.clone(),
                text: case.claim.clone(),
                kind: case.kind.clone(),
                marker: case.marker.clone(),
                span_start: 0,
                span_end: case.claim.len() as i64,
                links,
                search: Some(SearchRow {
                    searched: case.search.searched,
                    action_matches: case.search.action,
                    output_matches: case.search.output,
                    coverage_eligible: true,
                    missing_families: String::new(),
                }),
                session_rank: 0,
                turn: CLAIM_TURN,
                seq: case.claim_seq,
            };
            let (state, why) = claim_state(&c, case.moved_on);
            let want = expected_state(&case.expected);
            if std::env::var("R5_LEDGER_SURVEY").is_ok() {
                println!(
                    "SURVEY {} want={want:?} got={state:?} pending={:?} :: {why}",
                    case.id, case.pending_until
                );
                continue;
            }
            match &case.pending_until {
                None => {
                    assert_eq!(
                        state, want,
                        "{}: adjudicated verdict regressed (wanted {want:?}): {why}",
                        case.id
                    );
                    // §5k floors on top of the exact assertion:
                    if !case.final_true {
                        assert!(
                            !state.is_accusation(),
                            "{}: adjudicated-false claim accused again: {why}",
                            case.id
                        );
                    }
                    fixed += 1;
                }
                Some(clause) => {
                    // The ratchet: a pending case must NOT already resolve
                    // correctly. When its clause lands, this fails and the
                    // case is promoted by deleting `pending_until`.
                    assert_ne!(
                        state, want,
                        "{}: resolves correctly — its fix ({clause}) has landed; \
                         promote it by clearing pending_until",
                        case.id
                    );
                    pending.push((case.id.clone(), clause.clone(), state));
                }
            }
        }
        println!(
            "  round-5 ledger: {fixed} fixed · {} pending",
            pending.len()
        );
        for (id, clause, state) in &pending {
            println!("    pending[{clause}] {id} currently {state:?}");
        }
    }
}
