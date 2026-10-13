//! B3–B5: preservation checks bound to exact Git objects and captured edits.
//!
//! Assertion counts and CI observations come from complete, deterministic
//! adapters. B3 is designed for the existing `thruline-verify` parser, whose
//! findings are supplied as typed counts rather than duplicated here. Every
//! supplied file identity is checked against the named baseline/target Git
//! object before it can support a hold.

use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::change_link::{pattern_matches, PatternVerdict};
use crate::redact::Masked;

use super::checks::{CheckOutcome, Finding, SurfaceResult, VerdictState};
use super::checks_presence;
use super::checks_record::{EditActor, EditCensus};
use super::params::{PathPresentParams, Surface};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssertionFloorParams {
    pub paths: Vec<String>,
    pub minimum: u64,
    pub baseline_version: String,
}

/// One file's parsed assertion counts and exact blob receipts. `moved_to`
/// explicitly records a rename; a new target path is not silently inferred to
/// be the same test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssertionFile {
    pub baseline_path: Option<String>,
    pub target_path: Option<String>,
    pub baseline_blob: Option<String>,
    pub target_blob: Option<String>,
    pub baseline_count: u64,
    pub target_count: u64,
    pub moved_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssertionCensus {
    pub baseline_version: String,
    pub baseline_tree_hash: String,
    pub target_version: String,
    pub target_tree_hash: String,
    pub complete: bool,
    pub files: Vec<AssertionFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CiStepPresentParams {
    pub workflow: String,
    pub step: String,
    pub threshold_pct: Option<u32>,
}

/// Observations from a complete parser of the two workflow blobs. The checker
/// confirms both blob receipts in Git; if a parser cannot resolve a step or
/// threshold, it must set `complete=false`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CiStepCensus {
    pub baseline_version: String,
    pub baseline_tree_hash: String,
    pub target_version: String,
    pub target_tree_hash: String,
    pub workflow: String,
    pub step: String,
    pub baseline_blob: String,
    pub target_blob: String,
    pub complete: bool,
    pub baseline_step_present: bool,
    pub target_step_present: bool,
    pub baseline_threshold_pct: Option<u32>,
    pub target_threshold_pct: Option<u32>,
    pub baseline_bypasses: Vec<String>,
    pub target_bypasses: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleFilePresentParams {
    pub paths: Vec<String>,
    /// The supported person-edit exception is explicit, never inferred.
    pub person_edit_basis: String,
}

/// B3: the baseline's person-pinned assertion count remains after file moves.
pub fn check_assertion_floor(
    repo: &Path,
    version: &str,
    params: &AssertionFloorParams,
    census: &AssertionCensus,
) -> CheckOutcome {
    let Some((target_commit, target_tree)) = exact_version(repo, version) else {
        return unavailable("target version cannot be resolved", None);
    };
    if !census.complete
        || census.target_version != version
        || census.target_tree_hash != target_tree
        || census.baseline_version != params.baseline_version
        || census.files.is_empty()
    {
        return unavailable(
            "assertion census is incomplete or not on the named versions",
            Some(target_tree),
        );
    }
    let Some((base_commit, base_tree)) = exact_version(repo, &params.baseline_version) else {
        return unavailable(
            "assertion baseline version cannot be resolved",
            Some(target_tree),
        );
    };
    if census.baseline_tree_hash != base_tree || !valid_paths(&params.paths) {
        return unavailable(
            "assertion baseline or accepted paths are invalid",
            Some(target_tree),
        );
    }
    let mut baseline = 0u64;
    let mut target = 0u64;
    let mut target_path = String::new();
    for file in &census.files {
        let base_in_scope = file
            .baseline_path
            .as_deref()
            .is_some_and(|path| matches_any(&params.paths, path) == Some(true));
        let target_in_scope = file
            .target_path
            .as_deref()
            .is_some_and(|path| matches_any(&params.paths, path) == Some(true));
        if !base_in_scope && !target_in_scope {
            return unavailable(
                "assertion census includes a file outside accepted scope",
                Some(target_tree),
            );
        }
        if file.baseline_path != file.target_path {
            if let (Some(old), Some(new)) = (&file.baseline_path, &file.target_path) {
                if old != new && file.moved_to.as_deref() != Some(new.as_str()) {
                    return unavailable(
                        "test move has no explicit moved_to receipt",
                        Some(target_tree),
                    );
                }
            }
        }
        if let Some(path) = &file.baseline_path {
            if !safe_path(path)
                || file.baseline_blob.as_deref() != git_blob(repo, &base_commit, path).as_deref()
            {
                return unavailable(
                    "assertion baseline blob receipt differs from Git",
                    Some(target_tree),
                );
            }
            if base_in_scope {
                baseline = baseline.saturating_add(file.baseline_count);
            }
        } else if file.baseline_blob.is_some() || file.baseline_count != 0 {
            return unavailable(
                "new assertion file has a fabricated baseline",
                Some(target_tree),
            );
        }
        if let Some(path) = &file.target_path {
            if !safe_path(path)
                || file.target_blob.as_deref() != git_blob(repo, &target_commit, path).as_deref()
            {
                return unavailable(
                    "assertion target blob receipt differs from Git",
                    Some(target_tree),
                );
            }
            target = target.saturating_add(file.target_count);
            target_path = path.clone();
        } else if file.target_blob.is_some() || file.target_count != 0 {
            return unavailable(
                "removed assertion file has a fabricated target",
                Some(target_tree),
            );
        }
    }
    if baseline != params.minimum {
        return unavailable(
            "accepted assertion floor differs from baseline census",
            Some(target_tree),
        );
    }
    if target < params.minimum {
        let finding = Finding {
            surface: Surface::Tests,
            path: Masked::mask(&target_path).into_text(),
            line: 0,
            span: format!("assertion count fell from {} to {}", baseline, target),
        };
        return outcome(
            VerdictState::Broken,
            &target_tree,
            Some("assertion count is below accepted floor"),
            vec![finding],
            census.files.len() as u64,
            Surface::Tests,
        );
    }
    outcome(
        VerdictState::Holds,
        &target_tree,
        None,
        Vec::new(),
        census.files.len() as u64,
        Surface::Tests,
    )
}

/// B4: a named CI step and optional threshold survive; bypass settings cannot
/// be newly introduced. Parsing is supplied by an exact-blob adapter.
pub fn check_ci_step_present(
    repo: &Path,
    version: &str,
    params: &CiStepPresentParams,
    census: &CiStepCensus,
) -> CheckOutcome {
    let Some((target_commit, target_tree)) = exact_version(repo, version) else {
        return unavailable("target version cannot be resolved", None);
    };
    if !safe_path(&params.workflow)
        || params.step.trim().is_empty()
        || params.threshold_pct.is_some_and(|value| value > 100)
    {
        return unavailable("accepted CI parameters are invalid", Some(target_tree));
    }
    if !census.complete
        || census.workflow != params.workflow
        || census.step != params.step
        || census.target_version != version
        || census.target_tree_hash != target_tree
    {
        return unavailable(
            "CI census is incomplete or not on the target tree",
            Some(target_tree),
        );
    }
    let Some((baseline_commit, baseline_tree)) = exact_version(repo, &census.baseline_version)
    else {
        return unavailable("CI baseline cannot be resolved", Some(target_tree));
    };
    if census.baseline_tree_hash != baseline_tree
        || Some(census.baseline_blob.as_str())
            != git_blob(repo, &baseline_commit, &params.workflow).as_deref()
        || Some(census.target_blob.as_str())
            != git_blob(repo, &target_commit, &params.workflow).as_deref()
    {
        return unavailable(
            "CI workflow blob receipts differ from Git",
            Some(target_tree),
        );
    }
    if !census.baseline_step_present
        || (params.threshold_pct.is_some() && census.baseline_threshold_pct != params.threshold_pct)
    {
        return unavailable(
            "accepted CI baseline does not contain the named step or threshold",
            Some(target_tree),
        );
    }
    if census
        .baseline_bypasses
        .iter()
        .chain(&census.target_bypasses)
        .any(|entry| {
            !matches!(
                entry.as_str(),
                "|| true" | "continue-on-error" | "allow_failure"
            )
        })
    {
        return unavailable(
            "CI bypass observation is not a closed kind",
            Some(target_tree),
        );
    }
    let mut findings = Vec::new();
    if !census.target_step_present {
        findings.push(ci_finding(&params.workflow, "accepted CI step is absent"));
    }
    if let Some(minimum) = params.threshold_pct {
        if census
            .target_threshold_pct
            .is_none_or(|value| value < minimum)
        {
            findings.push(ci_finding(
                &params.workflow,
                "accepted coverage threshold was removed or lowered",
            ));
        }
    }
    for bypass in &census.target_bypasses {
        if !census.baseline_bypasses.contains(bypass) {
            findings.push(ci_finding(
                &params.workflow,
                "CI bypass setting was introduced",
            ));
        }
    }
    let state = if findings.is_empty() {
        VerdictState::Holds
    } else {
        VerdictState::Broken
    };
    outcome(state, &target_tree, None, findings, 1, Surface::Ci)
}

/// B5: every protected rule file exists and an agent edit has a person edit
/// in the same exact-version edit census. Unknown authorship is uncheckable.
pub fn check_rule_file_present(
    repo: &Path,
    version: &str,
    params: &RuleFilePresentParams,
    census: &EditCensus,
) -> CheckOutcome {
    let Some((commit, tree)) = exact_version(repo, version) else {
        return unavailable("target version cannot be resolved", None);
    };
    if params.person_edit_basis != "same_version" || !valid_paths(&params.paths) {
        return unavailable("accepted rule-file scope is invalid", Some(tree));
    }
    // Absence is a complete fact about the named immutable tree. Authorship
    // still needs a complete edit census, but missing file evidence does not.
    let mut findings = Vec::new();
    for path in &params.paths {
        if path.ends_with("/**") || path == "**" {
            return unavailable("rule-file presence needs named files", Some(tree));
        }
        let presence = checks_presence::check_path_present(
            repo,
            &commit,
            &PathPresentParams {
                path: path.clone(),
                sha256: None,
            },
        );
        if presence.tree_hash.as_deref() != Some(tree.as_str()) {
            return unavailable("rule-file Git tree changed during check", Some(tree));
        }
        match presence.state {
            VerdictState::Holds => {}
            VerdictState::Broken => {
                let Some(finding) = presence.findings.first() else {
                    return unavailable("rule-file path result has no finding", Some(tree));
                };
                findings.push(rule_finding(path, &finding.span));
            }
            VerdictState::Unverifiable | VerdictState::NotApplicable => {
                return unavailable("rule-file Git path cannot be read", Some(tree));
            }
        }
    }
    if !findings.is_empty() {
        return outcome(
            VerdictState::Broken,
            &tree,
            None,
            findings,
            params.paths.len() as u64,
            Surface::Docs,
        );
    }
    if !census.complete
        || census.version_id != version
        || census.tree_hash != tree
        || census.from_version_id.as_deref().is_none_or(str::is_empty)
    {
        return unavailable(
            "rule-file edit census is incomplete or not on this version",
            Some(tree),
        );
    }
    for edit in &census.edits {
        let Some(in_scope) = matches_any(&params.paths, &edit.path) else {
            return unavailable("rule-file path pattern cannot be read", Some(tree));
        };
        if !in_scope {
            continue;
        }
        if edit.edit_id.as_deref().is_none_or(str::is_empty)
            || edit.session_id.as_deref().is_none_or(str::is_empty)
        {
            return unavailable("protected-path edit receipt is incomplete", Some(tree));
        }
        if edit.actor == EditActor::Unknown {
            return unavailable("protected-path edit actor is unknown", Some(tree));
        }
        if edit.actor == EditActor::Agent
            && !census
                .edits
                .iter()
                .any(|other| other.path == edit.path && other.actor == EditActor::Person)
        {
            findings.push(rule_finding(
                &edit.path,
                "agent edit has no person edit in this version",
            ));
        }
    }
    let state = if findings.is_empty() {
        VerdictState::Holds
    } else {
        VerdictState::Broken
    };
    outcome(
        state,
        &tree,
        None,
        findings,
        params.paths.len() as u64,
        Surface::Docs,
    )
}

fn exact_version(repo: &Path, version: &str) -> Option<(String, String)> {
    if version.is_empty() {
        return None;
    }
    let commit = git_oid(repo, &format!("{version}^{{commit}}"))?;
    let tree = git_oid(repo, &format!("{commit}^{{tree}}"))?;
    Some((commit, tree))
}

fn git_blob(repo: &Path, commit: &str, path: &str) -> Option<String> {
    if !safe_path(path) {
        return None;
    }
    git_oid(repo, &format!("{commit}:{path}"))
}

fn git_oid(repo: &Path, spec: &str) -> Option<String> {
    let output = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .args(["rev-parse", "--verify", "--end-of-options", spec])
        .current_dir(repo)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let oid = std::str::from_utf8(&output.stdout).ok()?.trim();
    (oid.len() >= 40 && oid.bytes().all(|byte| byte.is_ascii_hexdigit())).then(|| oid.into())
}

fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
}

fn valid_paths(paths: &[String]) -> bool {
    !paths.is_empty()
        && paths.iter().all(|path| {
            safe_path(path)
                && pattern_matches(path, "__scope_probe__") != PatternVerdict::Unreadable
        })
}

fn matches_any(patterns: &[String], path: &str) -> Option<bool> {
    let mut hit = false;
    for pattern in patterns {
        match pattern_matches(pattern, path) {
            PatternVerdict::Matched => hit = true,
            PatternVerdict::DidNotMatch => {}
            PatternVerdict::Unreadable => return None,
        }
    }
    Some(hit)
}

fn ci_finding(path: &str, statement: &str) -> Finding {
    Finding {
        surface: Surface::Ci,
        path: Masked::mask(path).into_text(),
        line: 0,
        span: statement.into(),
    }
}

fn rule_finding(path: &str, statement: &str) -> Finding {
    Finding {
        surface: Surface::Docs,
        path: Masked::mask(path).into_text(),
        line: 0,
        span: statement.into(),
    }
}

fn unavailable(reason: &'static str, tree: Option<String>) -> CheckOutcome {
    CheckOutcome::unavailable(reason, tree)
}

fn outcome(
    state: VerdictState,
    tree: &str,
    reason: Option<&str>,
    findings: Vec<Finding>,
    checked_places: u64,
    surface: Surface,
) -> CheckOutcome {
    CheckOutcome {
        state,
        tree_hash: Some(tree.into()),
        reason: reason.map(str::to_owned),
        checked_places,
        surfaces: vec![SurfaceResult {
            surface,
            checked_places,
            finding_count: findings.len() as u64,
            state,
        }],
        findings,
        not_looked_at: Vec::new(),
        visual: Vec::new(),
    }
}
