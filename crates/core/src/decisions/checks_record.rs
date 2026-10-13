//! D1, D2, E1 and I4 checks over recorded, exact-version evidence.
//!
//! Adapters may build these typed censuses from HQ's immutable edit events and
//! execution bindings. A missing or incomplete census cannot establish a hold.

use serde::{Deserialize, Serialize};

use crate::change_link::{pattern_matches, PatternVerdict};
use crate::decisions::checks::{CheckOutcome, Finding, SurfaceResult, VerdictState};
use crate::decisions::params::{
    CommandPassesParams, EditsWithinPathsParams, PathsUntouchedParams, Surface,
};
use crate::redact::Masked;
use crate::test_tally::TestTally;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditActor {
    Person,
    Agent,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedEdit {
    pub path: String,
    pub actor: EditActor,
    pub session_id: Option<String>,
    pub edit_id: Option<String>,
}

/// Complete path census for a version. `from_version_id` is the accepted
/// baseline for I4; it is optional for D1 and D2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditCensus {
    pub from_version_id: Option<String>,
    pub version_id: String,
    pub tree_hash: String,
    pub complete: bool,
    pub edits: Vec<RecordedEdit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedCommandRun {
    pub run_id: String,
    /// Append order in the execution record. The last matching run governs.
    pub seq: u64,
    pub version_id: String,
    pub tree_hash: String,
    pub command: String,
    pub exit_code: Option<i64>,
    pub tally: Option<TestTally>,
    /// Some test runners report errors separately from failures.
    pub errors: Option<u64>,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunCensus {
    pub version_id: String,
    pub tree_hash: String,
    pub complete: bool,
    pub runs: Vec<RecordedCommandRun>,
}

pub fn check_edits_within_paths(
    version_id: &str,
    tree_hash: &str,
    params: &EditsWithinPathsParams,
    census: &EditCensus,
) -> CheckOutcome {
    if let Err(reason) = valid_edit_census(version_id, tree_hash, census) {
        return unavailable(reason, tree_hash);
    }
    if let Err(reason) = valid_patterns(&params.allowed_paths) {
        return unavailable(reason, tree_hash);
    }
    let mut findings = Vec::new();
    for edit in &census.edits {
        let matched = params
            .allowed_paths
            .iter()
            .any(|pattern| pattern_matches(pattern, &edit.path) == PatternVerdict::Matched);
        if !matched {
            findings.push(path_finding(&edit.path, "edit outside accepted paths"));
        }
    }
    let checked = census
        .edits
        .iter()
        .map(|edit| surface_of(&edit.path))
        .collect::<Vec<_>>();
    let state = if findings.is_empty() {
        VerdictState::Holds
    } else {
        VerdictState::Broken
    };
    result_with_checked(state, tree_hash, None, findings, &checked)
}

pub fn check_paths_untouched(
    version_id: &str,
    tree_hash: &str,
    params: &PathsUntouchedParams,
    census: &EditCensus,
) -> CheckOutcome {
    if let Err(reason) = valid_edit_census(version_id, tree_hash, census) {
        return unavailable(reason, tree_hash);
    }
    if let Err(reason) = valid_patterns(&params.paths) {
        return unavailable(reason, tree_hash);
    }
    let mut findings = Vec::new();
    let mut unknown_actor = false;
    for edit in &census.edits {
        let matched = params
            .paths
            .iter()
            .any(|pattern| pattern_matches(pattern, &edit.path) == PatternVerdict::Matched);
        if !matched {
            continue;
        }
        match edit.actor {
            EditActor::Agent => {
                findings.push(path_finding(&edit.path, "agent edit to protected path"))
            }
            EditActor::Unknown => unknown_actor = true,
            EditActor::Person => {}
        }
    }
    let checked = census
        .edits
        .iter()
        .map(|edit| surface_of(&edit.path))
        .collect::<Vec<_>>();
    if !findings.is_empty() {
        result_with_checked(VerdictState::Broken, tree_hash, None, findings, &checked)
    } else if unknown_actor {
        unavailable("protected path edit has unknown actor", tree_hash)
    } else {
        result_with_checked(VerdictState::Holds, tree_hash, None, findings, &checked)
    }
}

pub fn check_command_passes(
    version_id: &str,
    tree_hash: &str,
    params: &CommandPassesParams,
    census: &RunCensus,
) -> CheckOutcome {
    if version_id.trim().is_empty() || tree_hash.trim().is_empty() {
        return unavailable("exact version identity is missing", tree_hash);
    }
    if census.version_id != version_id || census.tree_hash != tree_hash || !census.complete {
        return unavailable("run census is missing for the exact version", tree_hash);
    }
    if params.command.trim().is_empty() {
        return unavailable("accepted command is empty", tree_hash);
    }
    let matching = census
        .runs
        .iter()
        .filter(|run| {
            run.version_id == version_id
                && run.tree_hash == tree_hash
                && run.command.trim() == params.command.trim()
        })
        .collect::<Vec<_>>();
    let Some(last_seq) = matching.iter().map(|run| run.seq).max() else {
        return unavailable("no run on this version", tree_hash);
    };
    let latest = matching
        .into_iter()
        .filter(|run| run.seq == last_seq)
        .collect::<Vec<_>>();
    if latest.len() != 1 {
        return unavailable("last run order is ambiguous", tree_hash);
    }
    let run = latest[0];
    if run.run_id.trim().is_empty() || run.seq == 0 {
        return unavailable("last run has no receipt identity", tree_hash);
    }
    if !run.complete {
        return unavailable("last run has incomplete capture", tree_hash);
    }
    let Some(exit_code) = run.exit_code else {
        return unavailable("last run has no exit code", tree_hash);
    };
    if exit_code != 0 {
        return failed_run(tree_hash, run, &format!("exit code {exit_code}"));
    }
    if params.is_test {
        let Some(tally) = run.tally else {
            return unavailable("test run has no tally", tree_hash);
        };
        let Some(errors) = run.errors else {
            return unavailable("test error count was not recorded", tree_hash);
        };
        if tally.suites == 0 || tally.passed == 0 {
            return failed_run(tree_hash, run, "test run has no passing tests");
        }
        if tally.failed > 0 || errors > 0 {
            return failed_run(tree_hash, run, "test run has failures or errors");
        }
        if tally.passed < params.min_passed {
            return failed_run(tree_hash, run, "test run is below accepted pass floor");
        }
    }
    result_with_checked(
        VerdictState::Holds,
        tree_hash,
        None,
        Vec::new(),
        &[Surface::Other],
    )
}

/// I4 adds the edit history to an already computed target-state check. A rule
/// that held before any new edit still holds. A broken target with no named
/// edit gives the specific "said, not done" reason only when the edit census
/// spans the acceptance version through this exact tree.
pub fn check_decision_without_implementation(
    version_id: &str,
    tree_hash: &str,
    acceptance_version: &str,
    named_paths: &[String],
    target: &CheckOutcome,
    census: &EditCensus,
) -> CheckOutcome {
    if version_id.trim().is_empty() || acceptance_version.trim().is_empty() {
        return unavailable("decision version identity is missing", tree_hash);
    }
    if target.tree_hash.as_deref() != Some(tree_hash) || tree_hash.trim().is_empty() {
        return unavailable("target check is not on this exact tree", tree_hash);
    }
    match target.state {
        VerdictState::Holds | VerdictState::NotApplicable | VerdictState::Unverifiable => {
            return target.clone();
        }
        VerdictState::Broken => {}
    }
    if let Err(reason) = valid_edit_census(version_id, tree_hash, census) {
        return unavailable(reason, tree_hash);
    }
    if census.from_version_id.as_deref() != Some(acceptance_version) {
        return unavailable("edit census does not begin at acceptance", tree_hash);
    }
    if let Err(reason) = valid_patterns(named_paths) {
        return unavailable(reason, tree_hash);
    }
    let named_edit = census.edits.iter().any(|edit| {
        named_paths
            .iter()
            .any(|pattern| pattern_matches(pattern, &edit.path) == PatternVerdict::Matched)
    });
    let mut broken = target.clone();
    broken.reason = Some(if named_edit {
        "target remains broken after edits to named paths".into()
    } else {
        "nothing changed since you decided this".into()
    });
    broken
}

fn valid_edit_census(
    version_id: &str,
    tree_hash: &str,
    census: &EditCensus,
) -> Result<(), &'static str> {
    if version_id.trim().is_empty() || tree_hash.trim().is_empty() {
        return Err("exact version identity is missing");
    }
    if census.version_id != version_id || census.tree_hash != tree_hash || !census.complete {
        return Err("edit census is missing for the exact version");
    }
    if census.edits.iter().any(|edit| !safe_path(&edit.path)) {
        return Err("recorded edit has an invalid project path");
    }
    Ok(())
}

fn valid_patterns(patterns: &[String]) -> Result<(), &'static str> {
    if patterns.is_empty() {
        return Err("accepted path set is empty");
    }
    for pattern in patterns {
        if !safe_path(pattern) && pattern != "**" {
            return Err("accepted path is not project-relative");
        }
        if pattern_matches(pattern, "__probe__") == PatternVerdict::Unreadable {
            return Err("accepted path pattern is not supported");
        }
    }
    Ok(())
}

fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with('~')
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn failed_run(tree_hash: &str, run: &RecordedCommandRun, message: &str) -> CheckOutcome {
    result_with_checked(
        VerdictState::Broken,
        tree_hash,
        None,
        vec![Finding {
            surface: Surface::Other,
            path: format!("run:{}", Masked::mask(&run.run_id).text()),
            line: 0,
            span: message.into(),
        }],
        &[Surface::Other],
    )
}

fn path_finding(path: &str, message: &str) -> Finding {
    Finding {
        surface: surface_of(path),
        path: Masked::mask(path).into_text(),
        line: 0,
        span: message.into(),
    }
}

fn surface_of(path: &str) -> Surface {
    if path.starts_with("tests/") || path.starts_with("test/") {
        Surface::Tests
    } else if path.starts_with(".github/workflows/") {
        Surface::Ci
    } else if path.starts_with("docs/") || path.ends_with(".md") {
        Surface::Docs
    } else if path.starts_with("ui/") || path.ends_with(".tsx") || path.ends_with(".jsx") {
        Surface::UiCopy
    } else if path.ends_with(".rs") || path.ends_with(".toml") || path.ends_with(".js") {
        Surface::Code
    } else {
        Surface::Other
    }
}

fn unavailable(reason: &str, tree_hash: &str) -> CheckOutcome {
    result(
        VerdictState::Unverifiable,
        tree_hash,
        Some(reason),
        Vec::new(),
    )
}

fn result(
    state: VerdictState,
    tree_hash: &str,
    reason: Option<&str>,
    findings: Vec<Finding>,
) -> CheckOutcome {
    result_with_checked(state, tree_hash, reason, findings, &[])
}

fn result_with_checked(
    state: VerdictState,
    tree_hash: &str,
    reason: Option<&str>,
    findings: Vec<Finding>,
    checked_surfaces: &[Surface],
) -> CheckOutcome {
    let mut surfaces: Vec<SurfaceResult> = Vec::new();
    for finding in &findings {
        if let Some(row) = surfaces
            .iter_mut()
            .find(|row| row.surface == finding.surface)
        {
            row.finding_count += 1;
        } else {
            surfaces.push(SurfaceResult {
                surface: finding.surface,
                checked_places: 0,
                finding_count: 1,
                state,
            });
        }
    }
    for surface in checked_surfaces {
        if let Some(row) = surfaces.iter_mut().find(|row| row.surface == *surface) {
            row.checked_places += 1;
        } else {
            surfaces.push(SurfaceResult {
                surface: *surface,
                checked_places: 1,
                finding_count: 0,
                state,
            });
        }
    }
    if surfaces.is_empty() && state == VerdictState::Holds {
        surfaces.push(SurfaceResult {
            surface: Surface::Other,
            checked_places: 0,
            finding_count: 0,
            state,
        });
    }
    surfaces.sort_by_key(|row| row.surface.as_str());
    CheckOutcome {
        state,
        tree_hash: (!tree_hash.is_empty()).then(|| tree_hash.into()),
        reason: reason.map(str::to_owned),
        checked_places: checked_surfaces.len() as u64,
        findings,
        surfaces,
        not_looked_at: Vec::new(),
        visual: Vec::new(),
    }
}
