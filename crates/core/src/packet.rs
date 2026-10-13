//! M6.R3 — the deterministic Context Packet compiler.
//!
//! **Zero model calls.** Selection is priority rules over derived objects; the
//! only judgement in this file is which object a line came from.
//!
//! Three things make it honest rather than merely structured:
//!
//! 1. **Every line carries its origin object id, its axis states, and why it
//!    was included.** The renderer cannot emit a fact without them, because
//!    [`Line`] has no constructor that omits them. This is the mechanism behind
//!    "unverified claims never render as bare fact": a `supported` claim reads
//!    as supported *in the same line as its text*, not in a legend elsewhere.
//! 2. **An empty section says why it is empty, and the three reasons are not
//!    interchangeable.** [`Empty::NoSourceObject`] means the model has nothing
//!    that could fill it; [`Empty::SourcePresentButEmpty`] means it was queried
//!    and measured zero; [`Empty::TrimmedToBudget`] means it was dropped for
//!    space. Collapsing a measured zero into a structural gap — or the reverse
//!    — is the same costume-swap this product exists to refuse.
//! 3. **The packet binds to a chain position** and recompiles byte-identically
//!    from the same position.
//!
//! **It does not read the attention queue, and that is deliberate.**
//! `attention::contradictions` selects `FROM nodes`, and the `nodes` table
//! holds whichever project was last derived — one of forty-four on the live
//! ledger. A compiler that trusted it would report "nothing needs you" for
//! forty-three projects out of forty-four. Every set here is computed from the
//! in-memory [`DerivedGraph`] the caller passes in.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::capture::ProofApplicability;
use crate::chain::sha256_hex;
use crate::derive::{Currency, DerivedGraph, EvidenceState, Lifecycle};
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;

/// The fourteen sections, in the order they are always emitted.
///
/// The order is the contract: a consumer may rely on position, and a section
/// that has nothing to say still appears, carrying its reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Section {
    Task,
    NonGoals,
    CurrentState,
    WorkspaceState,
    Ownership,
    OtherActiveWork,
    VerifiedCurrent,
    SupportedNotVerified,
    StaleOrConflicting,
    FailedApproaches,
    OpenHumanDecisions,
    RequiredCompletionEvidence,
    Resume,
    SnapshotBinding,
}

pub const SECTION_ORDER: [Section; 14] = [
    Section::Task,
    Section::NonGoals,
    Section::CurrentState,
    Section::WorkspaceState,
    Section::Ownership,
    Section::OtherActiveWork,
    Section::VerifiedCurrent,
    Section::SupportedNotVerified,
    Section::StaleOrConflicting,
    Section::FailedApproaches,
    Section::OpenHumanDecisions,
    Section::RequiredCompletionEvidence,
    Section::Resume,
    Section::SnapshotBinding,
];

impl Section {
    pub fn title(self) -> &'static str {
        match self {
            Section::Task => "TASK",
            Section::NonGoals => "NON-GOALS",
            Section::CurrentState => "CURRENT STATE",
            Section::WorkspaceState => "WORKSPACE STATE",
            Section::Ownership => "OWNERSHIP",
            Section::OtherActiveWork => "OTHER ACTIVE WORK",
            Section::VerifiedCurrent => "VERIFIED AND CURRENT",
            Section::SupportedNotVerified => "SUPPORTED, NOT VERIFIED",
            Section::StaleOrConflicting => "STALE OR CONFLICTING",
            Section::FailedApproaches => "FAILED APPROACHES",
            Section::OpenHumanDecisions => "OPEN HUMAN DECISIONS",
            Section::RequiredCompletionEvidence => "REQUIRED COMPLETION EVIDENCE",
            Section::Resume => "RESUME",
            Section::SnapshotBinding => "SNAPSHOT BINDING",
        }
    }

    /// Sections that survive any budget. The task is what the packet is *for*,
    /// and the binding is what makes every other line checkable.
    pub fn is_protected(self) -> bool {
        matches!(self, Section::Task | Section::SnapshotBinding)
    }
}

/// What a line came from. Every line has one; there is no "no origin" variant.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    /// Supplied by the caller, not derived. Rendered as such, never as a fact
    /// the engine established.
    Caller,
    Project(String),
    Repo(String),
    Node(String),
    Claim(String),
    Decision(String),
}

impl Origin {
    pub fn as_str(&self) -> String {
        match self {
            Origin::Caller => "caller".to_string(),
            Origin::Project(id) => format!("project:{id}"),
            Origin::Repo(id) => format!("repo:{id}"),
            Origin::Node(id) => format!("node:{id}"),
            Origin::Claim(id) => format!("claim:{id}"),
            Origin::Decision(id) => format!("decision:{id}"),
        }
    }
}

/// The three proof axes, carried on every line (NORTHSTAR-v2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Axes {
    pub evidence: Option<EvidenceState>,
    pub currency: Option<Currency>,
    pub applicability: ProofApplicability,
}

impl Axes {
    /// For structural lines (counts, bindings) that assert no claim.
    pub fn structural() -> Self {
        Axes {
            evidence: None,
            currency: None,
            applicability: ProofApplicability::Unknown,
        }
    }

    fn render(&self) -> String {
        format!(
            "evidence={} · currency={} · applicability={}",
            self.evidence.map(|e| e.as_str()).unwrap_or("n/a"),
            self.currency.map(|c| c.as_str()).unwrap_or("n/a"),
            match self.applicability {
                ProofApplicability::Current => "current",
                ProofApplicability::Stale => "stale",
                ProofApplicability::Unknown => "unknown",
            }
        )
    }
}

/// One emitted line. Constructing it requires all four facts, which is what
/// stops a claim reaching the page without its axes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub section: Section,
    pub text: String,
    pub origin: Origin,
    pub axes: Axes,
    /// Why this line is in the packet at all.
    pub reason: String,
}

impl Line {
    pub fn render(&self) -> String {
        format!(
            "- {}\n  [{} · {} · because: {}]",
            self.text,
            self.origin.as_str(),
            self.axes.render(),
            self.reason
        )
    }

    /// Token estimate, per the R09 rule: ceil(chars / 4).
    pub fn tokens(&self) -> usize {
        self.render().chars().count().div_ceil(4)
    }
}

/// Why a section carries nothing. The three are never interchangeable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Empty {
    /// The model has no object that could fill this section.
    NoSourceObject { what: String },
    /// A source exists and was queried; the measured answer is zero.
    SourcePresentButEmpty { measured: String },
    /// Dropped for space. Recorded per line in the exclusions report.
    TrimmedToBudget { lines: usize, tokens: usize },
}

impl Empty {
    fn render(&self) -> String {
        match self {
            Empty::NoSourceObject { what } => {
                format!("(nothing — no source object exists: {what})")
            }
            Empty::SourcePresentButEmpty { measured } => {
                format!("(nothing — the source was queried and is empty: {measured})")
            }
            Empty::TrimmedToBudget { lines, tokens } => {
                format!("({lines} line(s), {tokens} tokens, trimmed to fit the budget)")
            }
        }
    }
}

/// What the packet scopes to. A repo may span many project ids — the A92
/// grouping — and each keeps its own chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Project(String),
    Repo {
        repo_id: String,
        project_ids: Vec<String>,
    },
}

impl Scope {
    pub fn project_ids(&self) -> Vec<String> {
        match self {
            Scope::Project(id) => vec![id.clone()],
            Scope::Repo { project_ids, .. } => project_ids.clone(),
        }
    }

    fn origin(&self) -> Origin {
        match self {
            Scope::Project(id) => Origin::Project(id.clone()),
            Scope::Repo { repo_id, .. } => Origin::Repo(repo_id.clone()),
        }
    }
}

/// The chain position the packet is bound to.
///
/// One `(project_id, head_hash, event_count)` per project in scope, plus a
/// digest folding them in sorted order. Recompiling at the same position must
/// reproduce the same packet byte for byte; a changed digest means the record
/// moved underneath it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub heads: Vec<(String, String, u64)>,
    pub digest: String,
}

impl Binding {
    /// Build from heads already folded this request. Pure — no second pass
    /// over the events table.
    pub fn from_heads(mut heads: Vec<(String, String, u64)>) -> Binding {
        heads.sort();
        heads.dedup();
        let mut buf = String::from("thruline.packet.binding.v1\n");
        for (pid, head, count) in &heads {
            buf.push_str(&format!(
                "{}:{pid}\n{}:{head}\n{count}\n",
                pid.len(),
                head.len()
            ));
        }
        Binding {
            digest: sha256_hex(buf.as_bytes()),
            heads,
        }
    }

    pub fn compute(ledger: &Ledger, project_ids: &[String]) -> Result<Binding> {
        let mut heads = Vec::new();
        let mut sorted = project_ids.to_vec();
        sorted.sort();
        sorted.dedup();
        for pid in &sorted {
            let (head, count) = ledger.compute_project_head(pid)?;
            heads.push((pid.clone(), head, count));
        }
        let decisions = crate::decisions::context::fingerprint(ledger, &sorted)?;
        Ok(Self::from_heads(heads).with_decisions(decisions.as_deref()))
    }

    fn with_decisions(mut self, fingerprint: Option<&str>) -> Self {
        if let Some(fingerprint) = fingerprint {
            self.digest = sha256_hex(
                format!(
                    "thruline.packet.binding.decisions.v1\n{}\n{fingerprint}\n",
                    self.digest
                )
                .as_bytes(),
            );
        }
        self
    }
}

/// A line that did not make it, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Excluded {
    pub section: Section,
    pub origin: Origin,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub tokens: usize,
}

impl Default for Budget {
    fn default() -> Self {
        // Generous by default: trimming is a deliberate act, not an ambient one.
        Budget { tokens: 100_000 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub scope: Scope,
    /// How each project's view was obtained. Exposed so a caller can never
    /// report a fallback compile as a fast-path one.
    pub served_by: Vec<(String, ServedBy)>,
    pub lines: Vec<Line>,
    pub empties: BTreeMap<Section, Empty>,
    pub exclusions: Vec<Excluded>,
    pub binding: Binding,
    pub tokens: usize,
}

impl Packet {
    /// The markdown form. Deterministic: no clock, no map iteration order, no
    /// randomness reaches this function.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for section in SECTION_ORDER {
            out.push_str(&format!("## {}\n\n", section.title()));
            let lines: Vec<&Line> = self.lines.iter().filter(|l| l.section == section).collect();
            if lines.is_empty() {
                let note = self
                    .empties
                    .get(&section)
                    .map(Empty::render)
                    .unwrap_or_else(|| "(nothing)".to_string());
                out.push_str(&note);
                out.push_str("\n\n");
            } else {
                for line in lines {
                    out.push_str(&line.render());
                    out.push('\n');
                }
                out.push('\n');
            }
        }
        out.push_str("## EXCLUSIONS\n\n");
        if self.exclusions.is_empty() {
            out.push_str("(nothing was excluded)\n");
        } else {
            for ex in &self.exclusions {
                out.push_str(&format!(
                    "- {} · {} · {}\n",
                    ex.section.title(),
                    ex.origin.as_str(),
                    ex.reason
                ));
            }
        }
        out
    }
}

fn push(
    lines: &mut Vec<Line>,
    section: Section,
    text: String,
    origin: Origin,
    axes: Axes,
    reason: &str,
) {
    lines.push(Line {
        section,
        text,
        origin,
        axes,
        reason: reason.to_string(),
    });
}

/// One node as stored in the persisted projection, before its axes are parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedNode {
    pub id: String,
    pub label: String,
    pub lifecycle: String,
    pub evidence: String,
    pub currency: String,
}

/// Which path produced a project's view.
///
/// Carried into the packet and printed, because a fallback that silently posed
/// as the fast path would make every latency claim unfalsifiable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServedBy {
    /// Read from `nodes` at a projection head matching the current chain
    /// position.
    PersistedProjection,
    /// Derived in memory, because the projection was absent or behind.
    InMemoryDerivation,
}

impl ServedBy {
    pub fn as_str(self) -> &'static str {
        match self {
            ServedBy::PersistedProjection => "persisted projection",
            ServedBy::InMemoryDerivation => "in-memory derivation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewNode {
    pub id: String,
    pub label: String,
    pub dimensions: crate::derive::NodeDimensions,
}

/// What the compiler reads: one project's nodes, and how they were obtained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionView {
    pub project_id: String,
    pub nodes: Vec<ViewNode>,
    pub served_by: ServedBy,
    /// Claim-level verdicts, when the view can supply them.
    ///
    /// `None` is not "no verdicts" — it is "this view cannot answer
    /// claim-level questions". The persisted projection stores node dimensions
    /// only, so a fast-path view returns `None`, and a section that treated
    /// that as zero would repeat A94a's mistake of reading absence of a
    /// projection as absence of a finding.
    pub claim_verdicts: Option<Vec<ClaimLine>>,
}

/// One claim's verdict, as the packet needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimLine {
    pub node_id: String,
    pub text: String,
    pub evidence: EvidenceState,
    pub currency: Currency,
}

impl From<&DerivedGraph> for ProjectionView {
    fn from(graph: &DerivedGraph) -> ProjectionView {
        ProjectionView {
            project_id: graph.project_id.clone(),
            nodes: graph
                .nodes
                .iter()
                .map(|n| ViewNode {
                    id: n.id.clone(),
                    label: n.label.clone(),
                    dimensions: n.dimensions,
                })
                .collect(),
            served_by: ServedBy::InMemoryDerivation,
            claim_verdicts: Some(
                graph
                    .nodes
                    .iter()
                    .flat_map(|n| {
                        n.verdicts.iter().map(|v| ClaimLine {
                            node_id: n.id.clone(),
                            text: v.text.clone(),
                            evidence: v.dimensions.evidence,
                            currency: v.dimensions.currency,
                        })
                    })
                    .collect(),
            ),
        }
    }
}

/// Obtain a project's view, preferring the persisted projection.
///
/// Falls back to deriving when the projection has never been built or is behind
/// the record. The fallback is CORRECT, not merely acceptable: a projection
/// older than the ledger would answer about a world that has moved. Which path
/// ran is recorded on the view.
pub fn view_for(
    ledger: &Ledger,
    project_id: &str,
    cfg: &crate::config::Config,
) -> Result<ProjectionView> {
    let (head, _) = ledger.compute_project_head(project_id)?;
    view_for_with_head(ledger, project_id, &head, cfg)
}

/// As [`view_for`], but reusing a chain head already folded this request.
///
/// A93b prelude: `view_for` and `Binding::compute` each folded
/// `compute_project_head` over every event, so one packet paid the fold twice.
/// On a 473k-event project that was the dominant cost, and it was not
/// derivation — it was the same query run again.
pub fn view_for_with_head(
    ledger: &Ledger,
    project_id: &str,
    current_head: &str,
    cfg: &crate::config::Config,
) -> Result<ProjectionView> {
    let built = ledger.projection_head(project_id)?;
    if built.is_some_and(|(h, _)| h == current_head) {
        let stored = ledger.projection_nodes(project_id)?;
        let mut nodes = Vec::with_capacity(stored.len());
        for row in stored {
            // A stored value outside the schema's CHECK domain is corruption.
            // Failing loud beats substituting a default, which would put a
            // confidently wrong axis into a packet.
            let dimensions = crate::derive::NodeDimensions {
                lifecycle: crate::derive::Lifecycle::parse(&row.lifecycle).ok_or_else(|| {
                    crate::error::CoreError::Integrity(format!(
                        "projection node {} carries lifecycle {:?}, outside the schema domain",
                        row.id, row.lifecycle
                    ))
                })?,
                evidence: crate::derive::EvidenceState::parse(&row.evidence).ok_or_else(|| {
                    crate::error::CoreError::Integrity(format!(
                        "projection node {} carries evidence {:?}, outside the schema domain",
                        row.id, row.evidence
                    ))
                })?,
                currency: crate::derive::Currency::parse(&row.currency).ok_or_else(|| {
                    crate::error::CoreError::Integrity(format!(
                        "projection node {} carries currency {:?}, outside the schema domain",
                        row.id, row.currency
                    ))
                })?,
            };
            nodes.push(ViewNode {
                id: row.id,
                label: row.label,
                dimensions,
            });
        }
        return Ok(ProjectionView {
            project_id: project_id.to_string(),
            nodes,
            served_by: ServedBy::PersistedProjection,
            // The projection stores node dimensions only.
            claim_verdicts: None,
        });
    }
    let graph = crate::derive::derive_project_with(ledger, project_id, cfg)?;
    Ok(ProjectionView::from(&graph))
}

/// Compile a packet for a scope, folding each project's chain head EXACTLY
/// ONCE and sharing it between the projection check and the snapshot binding.
pub fn compile_request(
    ledger: &Ledger,
    scope: &Scope,
    task: Option<&str>,
    budget: &Budget,
    cfg: &crate::config::Config,
) -> Result<Packet> {
    let mut heads = Vec::new();
    let mut views = Vec::new();
    for pid in scope.project_ids() {
        let (head, count) = ledger.compute_project_head(&pid)?;
        views.push(view_for_with_head(ledger, &pid, &head, cfg)?);
        heads.push((pid, head, count));
    }
    compile_inner(
        ledger,
        &views,
        scope,
        task,
        budget,
        Some(Binding::from_heads(heads)),
    )
}

/// Compile a packet. Deterministic in its inputs: the same graphs at the same
/// chain position produce the same bytes.
///
/// `graphs` are derived in memory by the caller (`derive::derive_project_with`),
/// never read from the `nodes` table — see the module docs.
pub fn compile(
    ledger: &Ledger,
    graphs: &[DerivedGraph],
    scope: &Scope,
    task: Option<&str>,
    budget: &Budget,
) -> Result<Packet> {
    let views: Vec<ProjectionView> = graphs.iter().map(ProjectionView::from).collect();
    compile_views(ledger, &views, scope, task, budget)
}

/// The compiler proper. Takes views so a packet can be served from the
/// persisted projection without materialising a full graph.
pub fn compile_views(
    ledger: &Ledger,
    graphs: &[ProjectionView],
    scope: &Scope,
    task: Option<&str>,
    budget: &Budget,
) -> Result<Packet> {
    compile_inner(ledger, graphs, scope, task, budget, None)
}

fn compile_inner(
    ledger: &Ledger,
    graphs: &[ProjectionView],
    scope: &Scope,
    task: Option<&str>,
    budget: &Budget,
    prebuilt: Option<Binding>,
) -> Result<Packet> {
    let mut lines: Vec<Line> = Vec::new();
    let mut empties: BTreeMap<Section, Empty> = BTreeMap::new();
    let decisions = crate::decisions::context::read(ledger, &scope.project_ids())?;

    // 1. TASK — echoed, never derived. The model has no task object, so a task
    //    can only come from whoever asked for the packet, and it is labelled
    //    `caller` so no reader mistakes it for something the engine established.
    match task {
        Some(t) if !t.trim().is_empty() => push(
            &mut lines,
            Section::Task,
            t.trim().to_string(),
            Origin::Caller,
            Axes::structural(),
            "supplied by the caller; the model has no task object to derive one from",
        ),
        _ => {
            empties.insert(
                Section::Task,
                Empty::NoSourceObject {
                    what: "no task/obligation object exists in the model, and no task was supplied"
                        .to_string(),
                },
            );
        }
    }

    // 2. NON-GOALS — the axis arm exists (`Currency::OutOfScope`) with no
    //    producer anywhere in the engine, so there is nothing to select.
    empties.insert(
        Section::NonGoals,
        Empty::NoSourceObject {
            what: "`Currency::OutOfScope` exists as an axis value but no code path produces it"
                .to_string(),
        },
    );

    // 3. CURRENT STATE — composed counts, never a single badge.
    let mut total_nodes = 0usize;
    for graph in graphs {
        let mut lifecycle: BTreeMap<&str, usize> = BTreeMap::new();
        let mut evidence: BTreeMap<&str, usize> = BTreeMap::new();
        let mut currency: BTreeMap<&str, usize> = BTreeMap::new();
        for node in &graph.nodes {
            *lifecycle
                .entry(node.dimensions.lifecycle.as_str())
                .or_default() += 1;
            *evidence
                .entry(node.dimensions.evidence.as_str())
                .or_default() += 1;
            *currency
                .entry(node.dimensions.currency.as_str())
                .or_default() += 1;
        }
        total_nodes += graph.nodes.len();
        let fmt = |m: &BTreeMap<&str, usize>| {
            m.iter()
                .map(|(k, v)| format!("{v} {k}"))
                .collect::<Vec<_>>()
                .join(" · ")
        };
        push(
            &mut lines,
            Section::CurrentState,
            format!(
                "{}: {} node(s) — lifecycle {} — evidence {} — currency {}",
                graph.project_id,
                graph.nodes.len(),
                fmt(&lifecycle),
                fmt(&evidence),
                fmt(&currency)
            ),
            Origin::Project(graph.project_id.clone()),
            Axes::structural(),
            "composed counts across all three axes; never collapsed to one badge",
        );
        push(
            &mut lines,
            Section::CurrentState,
            format!(
                "{}: served from the {}",
                graph.project_id,
                graph.served_by.as_str()
            ),
            Origin::Project(graph.project_id.clone()),
            Axes::structural(),
            "which path produced this view; a fallback must never pose as the fast path",
        );
    }

    // 4. WORKSPACE STATE — A92 identity, abstention included.
    let identities = ledger.workspace_identities()?;
    let in_scope: BTreeSet<String> = scope.project_ids().into_iter().collect();
    let mut any_workspace = false;
    for row in identities
        .iter()
        .filter(|r| in_scope.contains(&r.project_id))
    {
        any_workspace = true;
        let repo = row
            .repo_id
            .as_deref()
            .map(|r| format!("repo {}", &r[..r.len().min(16)]))
            .unwrap_or_else(|| "no repo identity (abstained)".to_string());
        push(
            &mut lines,
            Section::WorkspaceState,
            format!(
                "{} — {} · working root {} · basis {} · resolved via {} · {} session(s) sampled",
                row.project_id,
                repo,
                row.resolved_root.as_deref().unwrap_or("(unresolved)"),
                row.basis,
                row.resolved_via.as_deref().unwrap_or("-"),
                row.sessions_sampled
            ),
            Origin::Project(row.project_id.clone()),
            Axes::structural(),
            "A92 repo identity; an unresolved root is recorded as abstention, never guessed",
        );
    }
    if !any_workspace {
        empties.insert(
            Section::WorkspaceState,
            Empty::SourcePresentButEmpty {
                measured: "no workspace_identities row for any project in scope; run `thruline identity backfill`"
                    .to_string(),
            },
        );
    }

    // 5. OWNERSHIP — one degenerate owner key, so there is nothing to say that
    //    would distinguish one lane from another.
    empties.insert(
        Section::Ownership,
        Empty::NoSourceObject {
            what: "no lease or per-agent ownership object exists; owner_key is a single constant"
                .to_string(),
        },
    );

    // 6. OTHER ACTIVE WORK — leases are M8.
    empties.insert(
        Section::OtherActiveWork,
        Empty::NoSourceObject {
            what: "no lease object exists; parallel-work coordination is M8".to_string(),
        },
    );

    // 7-9. The three verdict sections, split so that nothing is laundered.
    let mut supported = Vec::new();
    let mut conflicting = Vec::new();
    for graph in graphs {
        for node in &graph.nodes {
            let d = node.dimensions;
            match d.evidence {
                EvidenceState::Supported if d.currency == Currency::Current => {
                    supported.push((graph.project_id.clone(), node));
                }
                EvidenceState::Contradicted | EvidenceState::Stale => {
                    conflicting.push((graph.project_id.clone(), node));
                }
                _ => {}
            }
        }
    }

    // 7. VERIFIED AND CURRENT — read at CLAIM level.
    //
    // NORTHSTAR-v2: "Verdicts live at claim level." The node composition is
    // capped at `supported` by RULING 4 and must stay capped, so reading node
    // dimensions here could never surface a verified claim — it reported an
    // empty section while a verified claim sat underneath it.
    let unanswerable: Vec<&str> = graphs
        .iter()
        .filter(|g| g.claim_verdicts.is_none())
        .map(|g| g.project_id.as_str())
        .collect();
    let verified_claims: Vec<(&str, &ClaimLine)> = graphs
        .iter()
        .filter_map(|g| {
            g.claim_verdicts
                .as_ref()
                .map(|c| (g.project_id.as_str(), c))
        })
        .flat_map(|(pid, claims)| claims.iter().map(move |c| (pid, c)))
        .filter(|(_, c)| c.evidence == EvidenceState::Verified && c.currency == Currency::Current)
        .collect();
    let total_claims: usize = graphs
        .iter()
        .filter_map(|g| g.claim_verdicts.as_ref())
        .map(Vec::len)
        .sum();

    if !unanswerable.is_empty() {
        // Distinct from a measured zero: the view cannot answer, which is a
        // fact about the view and not about the project.
        empties.insert(
            Section::VerifiedCurrent,
            Empty::SourcePresentButEmpty {
                measured: format!(
                    "cannot be answered from the persisted projection, which stores node \
                     dimensions only and retains no claim verdicts ({} project(s): {}). \
                     Recompile from derivation to read this section",
                    unanswerable.len(),
                    unanswerable.join(", ")
                ),
            },
        );
    } else if verified_claims.is_empty() {
        empties.insert(
            Section::VerifiedCurrent,
            Empty::SourcePresentButEmpty {
                measured: format!(
                    "0 of {total_claims} claim(s) are verified and current. This section is \
                     NOT filled from `supported`"
                ),
            },
        );
    } else {
        for (pid, claim) in &verified_claims {
            push(
                &mut lines,
                Section::VerifiedCurrent,
                claim.text.clone(),
                Origin::Claim(format!("{pid}/{}", claim.node_id)),
                Axes {
                    evidence: Some(claim.evidence),
                    currency: Some(claim.currency),
                    applicability: ProofApplicability::Unknown,
                },
                "a typed contract checked real project state against an attested run. \
                 SHADOW: `verified` has not graduated and does not render on a public \
                 surface (§5s)",
            );
        }
    }

    // 8. SUPPORTED, NOT VERIFIED — the honest middle. Rendered with the axes
    //    inline so the weaker standing travels with the sentence.
    if supported.is_empty() {
        empties.insert(
            Section::SupportedNotVerified,
            Empty::SourcePresentButEmpty {
                measured: format!(
                    "0 of {total_nodes} node(s) carry evidence=supported + currency=current"
                ),
            },
        );
    } else {
        for (pid, node) in &supported {
            push(
                &mut lines,
                Section::SupportedNotVerified,
                node.label.clone(),
                Origin::Node(format!("{pid}/{}", node.id)),
                Axes {
                    evidence: Some(node.dimensions.evidence),
                    currency: Some(node.dimensions.currency),
                    applicability: ProofApplicability::Unknown,
                },
                "entailed by captured evidence; NOT verified against present project state",
            );
        }
    }

    // 9. STALE OR CONFLICTING.
    if conflicting.is_empty() {
        empties.insert(
            Section::StaleOrConflicting,
            Empty::SourcePresentButEmpty {
                measured: format!(
                    "0 of {total_nodes} node(s) are contradicted or stale. Staleness additionally \
                     requires a lapsed receipt, and applicability is `unknown` for every object \
                     while the capture substrate is empty"
                ),
            },
        );
    } else {
        for (pid, node) in &conflicting {
            push(
                &mut lines,
                Section::StaleOrConflicting,
                node.label.clone(),
                Origin::Node(format!("{pid}/{}", node.id)),
                Axes {
                    evidence: Some(node.dimensions.evidence),
                    currency: Some(node.dimensions.currency),
                    applicability: ProofApplicability::Unknown,
                },
                "conflicting or lapsed evidence; do not act on this without re-checking",
            );
        }
    }

    // 10. FAILED APPROACHES. `abandoned` is explicitly disclaimed as failure by
    //     the code that sets it, so using it here would invent a verdict.
    empties.insert(
        Section::FailedApproaches,
        Empty::NoSourceObject {
            what: "no failed-approach object exists; `abandoned` is set with supersession and \
                   explicitly does not mean the work failed"
                .to_string(),
        },
    );

    // 11. OPEN HUMAN DECISIONS — conflicts minus what a human already dismissed.
    let acknowledged = crate::attention::acknowledged_ids(ledger)?;
    let open: Vec<_> = conflicting
        .iter()
        .filter(|(pid, node)| !acknowledged.contains_key(&format!("{pid}/{}", node.id)))
        .collect();
    if open.is_empty() {
        empties.insert(
            Section::OpenHumanDecisions,
            Empty::SourcePresentButEmpty {
                measured: format!(
                    "no unacknowledged conflict in scope ({} acknowledgment(s) on record)",
                    acknowledged.len()
                ),
            },
        );
    } else {
        for (pid, node) in open {
            push(
                &mut lines,
                Section::OpenHumanDecisions,
                format!("needs a human ruling: {}", node.label),
                Origin::Node(format!("{pid}/{}", node.id)),
                Axes {
                    evidence: Some(node.dimensions.evidence),
                    currency: Some(node.dimensions.currency),
                    applicability: ProofApplicability::Unknown,
                },
                "conflicting evidence with no acknowledgment on record",
            );
        }
    }

    // 12. REQUIRED COMPLETION EVIDENCE — what the search could not reach.
    //     A coverage statement about records not read, NOT a pre-declared
    //     acceptance criterion, and it says so.
    let missing = missing_evidence_families(ledger, &scope.project_ids())?;
    for decision in &decisions.decisions {
        push(
            &mut lines,
            Section::RequiredCompletionEvidence,
            format!(
                "accepted standing check {} revision {} ({}; from {}): {}; this acceptance is not proof the check holds",
                decision.kind, decision.revision, decision.severity,
                decision.applies_from_version, decision.params
            ),
            Origin::Decision(format!("{}/{}", decision.project_id, decision.decision_id)),
            Axes::structural(),
            &format!(
                "verified bridge receipt {}; event {}; acceptance {}; typed parameters SHA-256 {}; tuple SHA-256 {}; raw receipt SHA-256 {}",
                decision.receipt_id, decision.event_id, decision.acceptance_id,
                decision.params_sha256, decision.tuple_sha256, decision.raw_sha256
            ),
        );
    }
    if missing.is_empty() && decisions.decisions.is_empty() {
        empties.insert(
            Section::RequiredCompletionEvidence,
            Empty::SourcePresentButEmpty {
                measured: "no claim in scope reports an unreached evidence family".to_string(),
            },
        );
    } else {
        for (claim_id, families) in &missing {
            push(
                &mut lines,
                Section::RequiredCompletionEvidence,
                format!("proof may live in records not yet read: {families}"),
                Origin::Claim(claim_id.clone()),
                Axes::structural(),
                "coverage statement about records not read — not a pre-declared acceptance criterion",
            );
        }
    }

    // 13. RESUME — the lineage table exists and is empty.
    empties.insert(
        Section::Resume,
        Empty::SourcePresentButEmpty {
            measured: "no session-generation lineage recorded for any project in scope".to_string(),
        },
    );

    // 14. SNAPSHOT BINDING.
    let binding = match prebuilt {
        Some(b) => b.with_decisions(decisions.fingerprint.as_deref()),
        None => Binding::compute(ledger, &scope.project_ids())?,
    };
    if decisions.fingerprint
        != crate::decisions::context::fingerprint(ledger, &scope.project_ids())?
    {
        return Err(crate::error::CoreError::Integrity(
            "Decisions changed while the packet was compiled; request it again".into(),
        ));
    }
    push(
        &mut lines,
        Section::SnapshotBinding,
        format!(
            "chain position {} across {} project(s)",
            &binding.digest[..16],
            binding.heads.len()
        ),
        scope.origin(),
        Axes::structural(),
        if decisions.fingerprint.is_some() {
            "chain position and exact Decision content bind this packet; retained bytes are available through handoff"
        } else {
            "recompiling at this chain position reproduces this packet byte for byte"
        },
    );
    for (pid, head, count) in &binding.heads {
        push(
            &mut lines,
            Section::SnapshotBinding,
            format!("{pid} @ {} ({count} events)", &head[..16]),
            Origin::Project(pid.clone()),
            Axes::structural(),
            "per-project chain head; `thruline verify` re-checks it against the raw records",
        );
    }

    let (lines, exclusions, trimmed) = trim_to_budget(lines, budget);
    for (section, (count, tokens)) in trimmed {
        empties.entry(section).or_insert(Empty::TrimmedToBudget {
            lines: count,
            tokens,
        });
    }

    let tokens = lines.iter().map(Line::tokens).sum();
    Ok(Packet {
        scope: scope.clone(),
        served_by: graphs
            .iter()
            .map(|g| (g.project_id.clone(), g.served_by))
            .collect(),
        lines,
        empties,
        exclusions,
        binding,
        tokens,
    })
}

/// Claims whose evidence search reports families it could not reach.
fn missing_evidence_families(
    ledger: &Ledger,
    project_ids: &[String],
) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for pid in project_ids {
        // Driven from `claim_searches`, NOT from `events`.
        //
        // The join written the obvious way made SQLite choose
        // `SEARCH e USING idx_events_src_uuid (project_id=?)` — it walked every
        // event in the project and probed for a claim on each. On the 473k-event
        // project that was **1.0–1.3 s**, and it was the entire cost of a
        // packet compile: head 0.04 ms + view 2.35 ms + this.
        //
        // Only 116 rows in the whole live ledger carry a non-empty
        // `missing_families`, so the selective side is `cs`. The EXISTS keeps
        // SQLite on that side and reduces the event touch to one primary-key
        // probe per surviving row. Same rows, proven: 116 out of 116 both ways,
        // zero in either difference. **1.0 s → 2 ms warm.**
        let mut stmt = ledger.conn().prepare(
            "SELECT cs.claim_id, cs.missing_families
             FROM claim_searches cs
             WHERE cs.missing_families IS NOT NULL AND cs.missing_families <> ''
               AND EXISTS (SELECT 1 FROM claims c
                            JOIN events e ON e.id = c.event_id
                            WHERE c.id = cs.claim_id AND e.project_id = ?1)
             ORDER BY cs.claim_id",
        )?;
        let rows = stmt.query_map([pid], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            out.push(row?);
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// Whole-line trimming, lowest priority first. Never splits a line, and never
/// touches a protected section.
///
/// Priority is the section's position in [`SECTION_ORDER`]: later sections are
/// dropped first, because the packet is ordered most-load-bearing first.
type TrimOutcome = (Vec<Line>, Vec<Excluded>, BTreeMap<Section, (usize, usize)>);

fn trim_to_budget(lines: Vec<Line>, budget: &Budget) -> TrimOutcome {
    let total: usize = lines.iter().map(Line::tokens).sum();
    if total <= budget.tokens {
        return (lines, Vec::new(), BTreeMap::new());
    }
    // Drop order: latest section first, and within a section the last line
    // first, so trimming is deterministic.
    let mut order: Vec<usize> = (0..lines.len()).collect();
    order.sort_by_key(|&i| {
        let pos = SECTION_ORDER
            .iter()
            .position(|s| *s == lines[i].section)
            .unwrap_or(usize::MAX);
        std::cmp::Reverse((pos, i))
    });

    let mut running = total;
    let mut dropped: BTreeSet<usize> = BTreeSet::new();
    for i in order {
        if running <= budget.tokens {
            break;
        }
        if lines[i].section.is_protected() {
            continue;
        }
        running -= lines[i].tokens();
        dropped.insert(i);
    }

    let mut kept = Vec::new();
    let mut exclusions = Vec::new();
    let mut summary: BTreeMap<Section, (usize, usize)> = BTreeMap::new();
    for (i, line) in lines.into_iter().enumerate() {
        if dropped.contains(&i) {
            let entry = summary.entry(line.section).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += line.tokens();
            exclusions.push(Excluded {
                section: line.section,
                origin: line.origin.clone(),
                reason: format!("trimmed to fit the {}-token budget", budget.tokens),
            });
        } else {
            kept.push(line);
        }
    }
    (kept, exclusions, summary)
}

// ---------------------------------------------------------------------------
// R13 — retention and deltas.
// ---------------------------------------------------------------------------

/// A packet line in its retained form: strings only, so retention does not
/// depend on the in-memory enum layout and an old row stays readable after the
/// enums grow.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RetainedLine {
    pub section: String,
    pub text: String,
    pub origin: String,
    pub evidence: String,
    pub currency: String,
    pub applicability: String,
    pub reason: String,
}

fn axis_strings(axes: &Axes) -> (String, String, String) {
    (
        axes.evidence
            .map(|e| e.as_str())
            .unwrap_or("n/a")
            .to_string(),
        axes.currency
            .map(|c| c.as_str())
            .unwrap_or("n/a")
            .to_string(),
        match axes.applicability {
            ProofApplicability::Current => "current",
            ProofApplicability::Stale => "stale",
            ProofApplicability::Unknown => "unknown",
        }
        .to_string(),
    )
}

impl Packet {
    pub fn retained_lines(&self) -> Vec<RetainedLine> {
        self.lines
            .iter()
            .map(|l| {
                let (evidence, currency, applicability) = axis_strings(&l.axes);
                RetainedLine {
                    section: l.section.title().to_string(),
                    text: l.text.clone(),
                    origin: l.origin.as_str(),
                    evidence,
                    currency,
                    applicability,
                    reason: l.reason.clone(),
                }
            })
            .collect()
    }

    /// Canonical retained form. Content-addressing keys on these bytes, so two
    /// compiles at one position must serialise identically — which they do,
    /// because the packet itself is byte-stable at a position.
    pub fn retained_json(&self) -> String {
        serde_json::to_string(&self.retained_lines()).unwrap_or_else(|_| "[]".to_string())
    }

    fn served_by_json(&self) -> String {
        let pairs: Vec<(String, String)> = self
            .served_by
            .iter()
            .map(|(p, s)| (p.clone(), s.as_str().to_string()))
            .collect();
        serde_json::to_string(&pairs).unwrap_or_else(|_| "[]".to_string())
    }

    /// Retain this packet as served. Called at the two serve points — the MCP
    /// tool and the CLI — never from an internal compile, because retaining a
    /// packet nobody received would make the issue log a record of our own
    /// activity rather than of what an agent was handed.
    pub fn retain(&self, ledger: &mut Ledger, agent: Option<&str>, at_ms: i64) -> Result<String> {
        self.retain_after(ledger, agent, at_ms, None)
    }

    /// Retain as the successor of `predecessor` — the handoff link.
    pub fn retain_after(
        &self,
        ledger: &mut Ledger,
        agent: Option<&str>,
        at_ms: i64,
        predecessor: Option<&str>,
    ) -> Result<String> {
        let lines = self.retained_json();
        let count = self.lines.len();
        ledger.record_packet_issue(&IssueRecord {
            chain_position: &self.binding.digest,
            scope: &scope_label(self),
            agent,
            served_by_json: &self.served_by_json(),
            lines_json: &lines,
            line_count: count,
            issued_at_unix_ms: at_ms,
            predecessor_issue_id: predecessor,
        })
    }
}

fn scope_label(packet: &Packet) -> String {
    match &packet.scope {
        Scope::Project(p) => format!("project:{p}"),
        Scope::Repo { repo_id, .. } => format!("repo:{repo_id}"),
    }
}

/// The fields of one packet issue. A struct rather than eight positional
/// arguments, so a future field cannot be transposed with its neighbour.
#[derive(Debug, Clone, Copy)]
pub struct IssueRecord<'a> {
    pub chain_position: &'a str,
    pub scope: &'a str,
    pub agent: Option<&'a str>,
    pub served_by_json: &'a str,
    pub lines_json: &'a str,
    pub line_count: usize,
    pub issued_at_unix_ms: i64,
    /// The issue this packet continues from, if it is a continuation.
    pub predecessor_issue_id: Option<&'a str>,
}

/// How one line differs between the retained packet and the current one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeltaKind {
    Added,
    Removed,
    AxisChanged,
}

impl DeltaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DeltaKind::Added => "added",
            DeltaKind::Removed => "removed",
            DeltaKind::AxisChanged => "axis-changed",
        }
    }
}

/// One changed line. Carries the same discipline as a packet line — origin,
/// axes, reason — because a delta is a claim about the project too, and a bare
/// "this changed" is exactly the unreceipted fact the packet format refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaLine {
    pub kind: DeltaKind,
    pub section: String,
    pub text: String,
    pub origin: String,
    pub evidence: String,
    pub currency: String,
    pub applicability: String,
    /// For `AxisChanged`, the axes as they stood in the retained packet.
    pub was: Option<(String, String, String)>,
    pub reason: String,
}

impl DeltaLine {
    pub fn render(&self) -> String {
        let axes = match &self.was {
            Some((e, c, a)) => format!(
                "evidence={} (was {e}) · currency={} (was {c}) · applicability={} (was {a})",
                self.evidence, self.currency, self.applicability
            ),
            None => format!(
                "evidence={} · currency={} · applicability={}",
                self.evidence, self.currency, self.applicability
            ),
        };
        format!(
            "- [{}] {}\n  [{} · {} · because: {}]",
            self.kind.as_str(),
            self.text,
            self.origin,
            axes,
            self.reason
        )
    }
}

/// The answer to "is my position still current?".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextAnswer {
    Current {
        position: String,
    },
    Changed {
        latest: String,
        lines: Vec<DeltaLine>,
        served_by: Vec<(String, ServedBy)>,
    },
    /// We never served a packet at that position, so there is nothing to diff
    /// against. Distinct from "nothing changed" on purpose.
    UnknownPosition {
        latest: String,
    },
}

fn key(section: &str, origin: &str, text: &str) -> String {
    format!("{section}\u{1}{origin}\u{1}{text}")
}

/// Compare a retained packet against a fresh compile.
///
/// Grouped by section in the packet's own fixed order, so a reader walks the
/// changes in the same shape as the document they already hold.
pub fn diff_lines(retained: &[RetainedLine], current: &[RetainedLine]) -> Vec<DeltaLine> {
    let mut before: BTreeMap<String, &RetainedLine> = BTreeMap::new();
    for l in retained {
        before.insert(key(&l.section, &l.origin, &l.text), l);
    }
    let mut after: BTreeMap<String, &RetainedLine> = BTreeMap::new();
    for l in current {
        after.insert(key(&l.section, &l.origin, &l.text), l);
    }

    let mut out = Vec::new();
    for (k, now) in &after {
        match before.get(k) {
            None => out.push(DeltaLine {
                kind: DeltaKind::Added,
                section: now.section.clone(),
                text: now.text.clone(),
                origin: now.origin.clone(),
                evidence: now.evidence.clone(),
                currency: now.currency.clone(),
                applicability: now.applicability.clone(),
                was: None,
                reason: now.reason.clone(),
            }),
            Some(then)
                if then.evidence != now.evidence
                    || then.currency != now.currency
                    || then.applicability != now.applicability =>
            {
                out.push(DeltaLine {
                    kind: DeltaKind::AxisChanged,
                    section: now.section.clone(),
                    text: now.text.clone(),
                    origin: now.origin.clone(),
                    evidence: now.evidence.clone(),
                    currency: now.currency.clone(),
                    applicability: now.applicability.clone(),
                    was: Some((
                        then.evidence.clone(),
                        then.currency.clone(),
                        then.applicability.clone(),
                    )),
                    reason: now.reason.clone(),
                })
            }
            Some(_) => {}
        }
    }
    for (k, then) in &before {
        if !after.contains_key(k) {
            out.push(DeltaLine {
                kind: DeltaKind::Removed,
                section: then.section.clone(),
                text: then.text.clone(),
                origin: then.origin.clone(),
                evidence: then.evidence.clone(),
                currency: then.currency.clone(),
                applicability: then.applicability.clone(),
                was: None,
                reason: then.reason.clone(),
            });
        }
    }
    // Group by the packet's own section order, then stably within a section.
    let order = |name: &str| {
        SECTION_ORDER
            .iter()
            .position(|s| s.title() == name)
            .unwrap_or(usize::MAX)
    };
    out.sort_by(|a, b| {
        order(&a.section)
            .cmp(&order(&b.section))
            .then(a.kind.as_str().cmp(b.kind.as_str()))
            .then(a.origin.cmp(&b.origin))
            .then(a.text.cmp(&b.text))
    });
    out
}

/// The R13 answer for a claimed chain position.
pub fn context_since(
    ledger: &Ledger,
    scope: &Scope,
    position: &str,
    cfg: &crate::config::Config,
) -> Result<ContextAnswer> {
    let latest = Binding::compute(ledger, &scope.project_ids())?.digest;
    if latest == position {
        return Ok(ContextAnswer::Current { position: latest });
    }
    let Some((lines_json, _served)) = ledger.retained_packet(position)? else {
        return Ok(ContextAnswer::UnknownPosition { latest });
    };
    let retained: Vec<RetainedLine> = serde_json::from_str(&lines_json).map_err(|e| {
        crate::error::CoreError::Integrity(format!("retained packet is unreadable: {e}"))
    })?;
    // Recompile with the SAME task the retained packet carried. The task is
    // caller-supplied, so compiling without it would report the TASK line as
    // removed on every delta — a difference in the question asked, dressed up
    // as a change in the project. Recovered from the retained lines rather than
    // stored twice.
    let task = retained
        .iter()
        .find(|l| l.section == Section::Task.title())
        .map(|l| l.text.clone());
    let fresh = compile_request(ledger, scope, task.as_deref(), &Budget::default(), cfg)?;
    Ok(ContextAnswer::Changed {
        latest,
        lines: diff_lines(&retained, &fresh.retained_lines()),
        served_by: fresh.served_by.clone(),
    })
}

/// Render a retained issue as the HANDOFF artifact.
///
/// Reads the frozen content and re-renders it. It does **not** recompile: the
/// `packet_issues` row is the freeze, and a handoff that recompiled would hand
/// the successor a different document than the predecessor was given — which is
/// the one thing a handoff must not do.
pub fn handoff(ledger: &Ledger, issue_id: &str) -> Result<Option<String>> {
    let Some((lines_json, predecessor)) = ledger.retained_issue(issue_id)? else {
        return Ok(None);
    };
    let lines: Vec<RetainedLine> = serde_json::from_str(&lines_json).map_err(|e| {
        crate::error::CoreError::Integrity(format!("retained packet is unreadable: {e}"))
    })?;
    let mut out = String::new();
    out.push_str(&format!("# HANDOFF — frozen issue {issue_id}\n\n"));
    match &predecessor {
        Some(p) => out.push_str(&format!("continues from issue {p}\n\n")),
        None => out.push_str("this is the start of the chain\n\n"),
    }
    let mut section = String::new();
    for line in &lines {
        if line.section != section {
            section = line.section.clone();
            out.push_str(&format!("## {section}\n\n"));
        }
        out.push_str(&format!(
            "- {}\n  [{} · evidence={} · currency={} · applicability={} · because: {}]\n",
            line.text, line.origin, line.evidence, line.currency, line.applicability, line.reason
        ));
    }
    Ok(Some(out))
}

/// Resolve a repo scope from A92 identity: one repo, many project ids.
///
/// The argument arrives in one of two shapes, and conflating them was the A129
/// defect: a 64-hex `repo_id` (what `workspace_identities` stores) or a
/// filesystem path (what every caller actually types). A path matched zero rows
/// and returned an *empty scope*, which compiled to a packet whose every
/// section read "the source was queried and is empty" — a fail-open in the one
/// file whose stated contract is that a measured zero and a structural gap are
/// never interchangeable.
///
/// Three shapes, three distinct answers:
///
/// - **A path** resolves through [`crate::workspace::repo_identity_at`] to a
///   repo id, then scopes on it.
/// - **An unknown repo id** is an error. Nothing on this ledger can name it, so
///   an empty packet would assert a measurement we never made.
/// - **A resolvable repo carrying no projects yet** is a legitimate empty
///   scope: the question was asked and the answer really is zero.
pub fn repo_scope(ledger: &Ledger, repo: &str) -> Result<Scope> {
    let rows = ledger.workspace_identities()?;
    let scope_on = |repo_id: &str| -> Scope {
        let mut project_ids: Vec<String> = rows
            .iter()
            .filter(|r| r.repo_id.as_deref() == Some(repo_id))
            .map(|r| r.project_id.clone())
            .collect();
        project_ids.sort();
        project_ids.dedup();
        Scope::Repo {
            repo_id: repo_id.to_string(),
            project_ids,
        }
    };

    // The hash shape. A repo id that no identity row carries cannot be
    // measured, and answering "zero" would be the fail-open we are removing.
    if is_repo_id(repo) {
        let scope = scope_on(repo);
        if matches!(&scope, Scope::Repo { project_ids, .. } if project_ids.is_empty()) {
            return Err(CoreError::Config(format!(
                "unknown repo id {repo}: no workspace identity on this ledger carries it. \
                 Pass the repo path instead, or run `thruline identity backfill` first."
            )));
        }
        return Ok(scope);
    }

    // The path shape. Abstention from identity derivation is an error here, not
    // an empty answer: we could not determine *which* repo was asked about.
    let identity = crate::workspace::repo_identity_at(Path::new(repo))?;
    let Some(repo_id) = identity.repo_id else {
        return Err(CoreError::Config(format!(
            "{repo} is not a resolvable repo: no git root commit, no remote, and no \
             .thruline-id marker. Pass a repo id, or a path inside a git repository."
        )));
    };
    Ok(scope_on(&repo_id))
}

/// A `repo_id` is [`sha256_hex`] output — exactly 64 lowercase hex characters.
/// Anything else is treated as a path, including a relative one.
fn is_repo_id(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Lifecycle is a work-item axis and never appears on a proof line; exposed so
/// callers can compose their own counts without re-deriving the mapping.
pub fn lifecycle_label(l: Lifecycle) -> &'static str {
    l.as_str()
}
