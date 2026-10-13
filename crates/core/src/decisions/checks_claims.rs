//! J1–J6: bounded checks over typed claim, decision, run and edit receipts.
//!
//! No claim sentence, model output or inferred intent enters these functions.
//! The caller projects a verified claim and complete session census; a missing
//! link remains `unverifiable` and never becomes a reported pass.

use serde::{Deserialize, Serialize};

use crate::change_link::{pattern_matches, PatternVerdict};
use crate::redact::Masked;

use super::checks::{CheckOutcome, Finding, SurfaceResult, VerdictState};
use super::params::Surface;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    DecisionSatisfied,
    TestsPass,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOrigin {
    SameSession,
    OtherSession,
    Ci,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimReceipt {
    pub id: String,
    pub session_id: String,
    pub seq: u64,
    pub version_id: String,
    pub tree_hash: String,
    pub kind: ClaimKind,
    pub reported_passed: Option<u64>,
    pub bound_run_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimRun {
    pub id: String,
    pub session_id: String,
    pub seq: u64,
    pub version_id: String,
    pub tree_hash: String,
    pub origin: RunOrigin,
    pub passed: Option<u64>,
    pub failed: Option<u64>,
    pub exit_code: Option<i64>,
    pub started_ms: Option<i64>,
    pub ended_ms: Option<i64>,
    pub examined_paths: Vec<String>,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimEdit {
    pub id: String,
    pub session_id: String,
    pub seq: u64,
    pub version_id: String,
    pub tree_hash: String,
    pub path: String,
    pub at_ms: Option<i64>,
    /// A source-backed adapter identifies paths relevant to the claim. It
    /// cannot derive relevance from the claim's prose in this verdict path.
    pub relevant: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionClaimBinding {
    pub decision_id: String,
    pub revision: u32,
    pub accepted: bool,
    /// `accepted_term_overlap` or `explicit`, recorded by the binding adapter.
    pub binding_basis: String,
    pub version_id: String,
    pub tree_hash: String,
    pub verdict: Option<VerdictState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimCensus {
    pub complete: bool,
    pub final_seq: u64,
    pub claim: ClaimReceipt,
    pub decision: Option<DecisionClaimBinding>,
    pub runs: Vec<ClaimRun>,
    pub edits: Vec<ClaimEdit>,
}

/// J1: one accepted decision's verdict on the claim's exact version.
pub fn check_decision_satisfied(census: &ClaimCensus) -> CheckOutcome {
    if let Err(reason) = valid(census) {
        return unavailable(reason, census);
    }
    if census.claim.kind != ClaimKind::DecisionSatisfied {
        return not_applicable("claim is not a decision-satisfied claim", census);
    }
    let Some(binding) = &census.decision else {
        return unavailable("claim has no accepted decision binding", census);
    };
    if binding.decision_id.is_empty()
        || binding.revision == 0
        || !binding.accepted
        || !matches!(
            binding.binding_basis.as_str(),
            "accepted_term_overlap" | "explicit"
        )
    {
        return unavailable("decision binding is not accepted or source-backed", census);
    }
    if binding.version_id != census.claim.version_id || binding.tree_hash != census.claim.tree_hash
    {
        return unavailable("decision verdict is not on the claim's exact tree", census);
    }
    match binding.verdict {
        Some(VerdictState::Holds) => outcome(VerdictState::Holds, census, None, Vec::new(), 1),
        Some(VerdictState::Broken) => outcome(
            VerdictState::Broken,
            census,
            Some("claimed, and no proof found in the record"),
            vec![fact(
                "accepted decision is broken on claim version",
                census.claim.seq,
            )],
            1,
        ),
        _ => unavailable(
            "accepted decision has no holding verdict on claim version",
            census,
        ),
    }
}

/// J2: later relevant edits make an earlier claim stale until a complete run
/// on the edited version follows before the session's final message.
pub fn check_stale_claim(census: &ClaimCensus) -> CheckOutcome {
    if let Err(reason) = valid(census) {
        return unavailable(reason, census);
    }
    let latest = census
        .edits
        .iter()
        .filter(|edit| {
            edit.relevant
                && edit.session_id == census.claim.session_id
                && edit.seq > census.claim.seq
                && edit.seq < census.final_seq
        })
        .max_by_key(|edit| edit.seq);
    let Some(edit) = latest else {
        return outcome(VerdictState::Holds, census, None, Vec::new(), 0);
    };
    if edit.id.is_empty()
        || edit.path.is_empty()
        || edit.version_id.is_empty()
        || edit.tree_hash.is_empty()
    {
        return unavailable("later edit receipt is incomplete", census);
    }
    let rerun = census
        .runs
        .iter()
        .filter(|run| {
            run.session_id == census.claim.session_id
                && run.seq > edit.seq
                && run.seq < census.final_seq
                && run.version_id == edit.version_id
                && run.tree_hash == edit.tree_hash
                && run.origin == RunOrigin::SameSession
        })
        .max_by_key(|run| run.seq);
    if let Some(run) = rerun {
        if !run.complete || run.exit_code.is_none() || run.passed.is_none() || run.failed.is_none()
        {
            return unavailable("rerun after edit has incomplete result", census);
        }
        if run.exit_code == Some(0)
            && run.passed.is_some_and(|passed| passed > 0)
            && run.failed == Some(0)
        {
            return outcome(VerdictState::Holds, census, None, Vec::new(), 1);
        }
    }
    outcome(
        VerdictState::Unverifiable,
        census,
        Some("claim became stale after a relevant edit without a passing rerun"),
        vec![edit_fact(edit)],
        1,
    )
}

/// J3: a stated tally needs one run that reported it. A sum across runs is not
/// a run receipt and is explicitly uncheckable as the stated total.
pub fn check_summed_totals(census: &ClaimCensus) -> CheckOutcome {
    if let Err(reason) = valid(census) {
        return unavailable(reason, census);
    }
    let Some(reported) = census.claim.reported_passed else {
        return unavailable("claim has no typed reported test tally", census);
    };
    let runs = census
        .runs
        .iter()
        .filter(|run| {
            run.session_id == census.claim.session_id
                && run.seq <= census.final_seq
                && run.version_id == census.claim.version_id
                && run.tree_hash == census.claim.tree_hash
        })
        .collect::<Vec<_>>();
    if runs.iter().any(|run| !run.complete || run.passed.is_none()) {
        return unavailable("recorded run tallies are incomplete", census);
    }
    if runs.iter().any(|run| run.passed == Some(reported)) {
        return outcome(
            VerdictState::Holds,
            census,
            None,
            Vec::new(),
            runs.len() as u64,
        );
    }
    let sum = runs
        .iter()
        .filter_map(|run| run.passed)
        .try_fold(0u64, u64::checked_add);
    let reason = if runs.len() > 1 && sum == Some(reported) {
        "no single run reports this number; it equals a sum across runs"
    } else {
        "no single run reports this number"
    };
    outcome(
        VerdictState::Unverifiable,
        census,
        Some(reason),
        vec![fact(
            "claimed tally lacks one run receipt",
            census.claim.seq,
        )],
        runs.len() as u64,
    )
}

/// J4: an all-tests-pass claim resting on zero passing tests is contradicted
/// by the run's own count, even if its exit status was zero.
pub fn check_empty_run(census: &ClaimCensus) -> CheckOutcome {
    if let Err(reason) = valid(census) {
        return unavailable(reason, census);
    }
    if census.claim.kind != ClaimKind::TestsPass {
        return not_applicable("claim is not a tests-pass claim", census);
    }
    let Some(run) = bound_run(census) else {
        return unavailable("claim has no unique bound run receipt", census);
    };
    if !same_record_run(census, run) {
        return unavailable("bound test run is outside the claim's exact record", census);
    }
    let (Some(passed), Some(failed), Some(exit)) = (run.passed, run.failed, run.exit_code) else {
        return unavailable("bound test run has no complete tally or exit", census);
    };
    if passed == 0 {
        return outcome(
            VerdictState::Broken,
            census,
            Some("all-tests-pass claim rests on an empty run"),
            vec![fact("zero passing tests", run.seq)],
            1,
        );
    }
    if failed > 0 || exit != 0 {
        return outcome(
            VerdictState::Broken,
            census,
            Some("bound test run did not pass"),
            vec![fact("run failed", run.seq)],
            1,
        );
    }
    outcome(VerdictState::Holds, census, None, Vec::new(), 1)
}

/// J5: a CI or other-session run is a pointer, not local run evidence.
pub fn check_delegated_run(census: &ClaimCensus) -> CheckOutcome {
    if let Err(reason) = valid(census) {
        return unavailable(reason, census);
    }
    let Some(run) = bound_run(census) else {
        return unavailable("claim has no unique bound run receipt", census);
    };
    if !run.complete || run.id.is_empty() {
        return unavailable("bound run receipt is incomplete", census);
    }
    if same_record_run(census, run) {
        return outcome(VerdictState::Holds, census, None, Vec::new(), 1);
    }
    let reason = match run.origin {
        RunOrigin::Ci | RunOrigin::OtherSession => {
            "run not in this record; only a delegated pointer is available"
        }
        _ => "run origin cannot be tied to this claim's session and version",
    };
    outcome(
        VerdictState::Unverifiable,
        census,
        Some(reason),
        vec![fact("bound run is outside this recorded session", run.seq)],
        1,
    )
}

/// J6: an edit inside the bound run's half-open interval invalidates that
/// run's evidence for the edited file. The product state is not inferred.
pub fn check_edit_during_run(census: &ClaimCensus) -> CheckOutcome {
    if let Err(reason) = valid(census) {
        return unavailable(reason, census);
    }
    let Some(run) = bound_run(census) else {
        return unavailable("claim has no unique bound run receipt", census);
    };
    if !same_record_run(census, run) {
        return unavailable("bound run is outside this exact record", census);
    }
    let (Some(start), Some(end)) = (run.started_ms, run.ended_ms) else {
        return unavailable("bound run interval is incomplete", census);
    };
    if start >= end || run.examined_paths.is_empty() {
        return unavailable("bound run interval or examined paths are invalid", census);
    }
    let mut findings = Vec::new();
    for edit in census
        .edits
        .iter()
        .filter(|edit| edit.relevant && edit.session_id == run.session_id)
    {
        if edit.id.is_empty() || edit.path.is_empty() {
            return unavailable("edit receipt is incomplete", census);
        }
        let relevant_path = run.examined_paths.iter().try_fold(false, |hit, pattern| {
            match pattern_matches(pattern, &edit.path) {
                PatternVerdict::Matched => Some(true),
                PatternVerdict::DidNotMatch => Some(hit),
                PatternVerdict::Unreadable => None,
            }
        });
        let Some(relevant_path) = relevant_path else {
            return unavailable("run path scope cannot be read", census);
        };
        if !relevant_path {
            continue;
        }
        let Some(at) = edit.at_ms else {
            return unavailable("relevant edit timestamp is missing", census);
        };
        if start <= at && at < end {
            findings.push(edit_fact(edit));
        }
    }
    if findings.is_empty() {
        outcome(
            VerdictState::Holds,
            census,
            None,
            Vec::new(),
            run.examined_paths.len() as u64,
        )
    } else {
        outcome(
            VerdictState::Unverifiable,
            census,
            Some("edit occurred during the bound run; that file's run evidence cannot count"),
            findings,
            run.examined_paths.len() as u64,
        )
    }
}

fn valid(census: &ClaimCensus) -> Result<(), &'static str> {
    let claim = &census.claim;
    if !census.complete
        || claim.id.is_empty()
        || claim.session_id.is_empty()
        || claim.version_id.is_empty()
        || claim.tree_hash.is_empty()
        || claim.seq == 0
        || census.final_seq <= claim.seq
    {
        return Err("claim census is incomplete or not bound to an exact version");
    }
    Ok(())
}

fn bound_run(census: &ClaimCensus) -> Option<&ClaimRun> {
    let id = census.claim.bound_run_id.as_deref()?;
    let mut matching = census.runs.iter().filter(|run| run.id == id);
    let first = matching.next()?;
    matching.next().is_none().then_some(first)
}

fn same_record_run(census: &ClaimCensus, run: &ClaimRun) -> bool {
    run.complete
        && run.origin == RunOrigin::SameSession
        && run.session_id == census.claim.session_id
        && run.version_id == census.claim.version_id
        && run.tree_hash == census.claim.tree_hash
}

fn edit_fact(edit: &ClaimEdit) -> Finding {
    Finding {
        surface: Surface::Code,
        path: Masked::mask(&edit.path).into_text(),
        line: edit.seq,
        span: "recorded relevant edit".into(),
    }
}

fn fact(statement: &str, seq: u64) -> Finding {
    Finding {
        surface: Surface::Other,
        path: String::new(),
        line: seq,
        span: statement.into(),
    }
}

fn unavailable(reason: &'static str, census: &ClaimCensus) -> CheckOutcome {
    CheckOutcome::unavailable(
        reason,
        (!census.claim.tree_hash.is_empty()).then(|| census.claim.tree_hash.clone()),
    )
}

fn not_applicable(reason: &'static str, census: &ClaimCensus) -> CheckOutcome {
    outcome(
        VerdictState::NotApplicable,
        census,
        Some(reason),
        Vec::new(),
        0,
    )
}

fn outcome(
    state: VerdictState,
    census: &ClaimCensus,
    reason: Option<&str>,
    findings: Vec<Finding>,
    checked_places: u64,
) -> CheckOutcome {
    CheckOutcome {
        state,
        tree_hash: Some(census.claim.tree_hash.clone()),
        reason: reason.map(str::to_owned),
        checked_places,
        surfaces: vec![SurfaceResult {
            surface: Surface::Other,
            checked_places,
            finding_count: findings.len() as u64,
            state,
        }],
        findings,
        not_looked_at: Vec::new(),
        visual: Vec::new(),
    }
}
