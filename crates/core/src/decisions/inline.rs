//! Read-only, advisory checks for a bounded inline sweep.
//!
//! This module opens SQLite in READ_ONLY mode, never constructs a writable
//! Ledger, and never appends a verdict. The caller supervises it in a child
//! process so Git and pure checkers share one hard wall-clock budget.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{CoreError, Result};

use super::assertion_adapter;
use super::authority::{bridge, PersonActKind};
use super::checks::{self, CheckOutcome, VerdictState};
use super::checks_fidelity;
use super::ci_adapter;
use super::engine::{state_name, CHECK_IMPL_VERSION};
use super::params::{parse_params, CheckParams};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineRow {
    pub decision_id: String,
    pub revision: u32,
    pub kind: String,
    pub state: String,
    pub reason: Option<String>,
    pub checked_places: u64,
    pub from_cache: bool,
    pub advisory: bool,
    pub verdict_id: Option<String>,
    pub full_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineSweep {
    pub schema_version: u32,
    pub project_id: String,
    pub version_id: String,
    pub tree_hash: Option<String>,
    pub diagnostic: bool,
    pub results: Vec<InlineRow>,
}

struct Candidate {
    id: String,
    revision: u32,
    kind: String,
    schema: u32,
    params_json: String,
    applies_from_version: Option<String>,
}

struct CachedVerdict {
    id: String,
    state: String,
    reason: Option<String>,
    checked_places: u64,
}

/// Compute ephemeral rows from a read-only connection. `diagnostic=true`
/// inspects latest proposal revisions without giving them lifecycle authority.
pub fn evaluate_read_only(
    db: &Path,
    repo: &Path,
    project_id: &str,
    version_id: &str,
    diagnostic: bool,
) -> Result<InlineSweep> {
    evaluate_read_only_mode(db, repo, project_id, version_id, diagnostic, false)
}

/// Return cached exact-tree verdicts plus explicit Unverifiable placeholders
/// for unfinished candidates. The supervised worker publishes this snapshot
/// before it starts uncached work, so a deadline never erases known evidence.
pub fn cached_read_only(
    db: &Path,
    repo: &Path,
    project_id: &str,
    version_id: &str,
    diagnostic: bool,
) -> Result<InlineSweep> {
    evaluate_read_only_mode(db, repo, project_id, version_id, diagnostic, true)
}

fn evaluate_read_only_mode(
    db: &Path,
    repo: &Path,
    project_id: &str,
    version_id: &str,
    diagnostic: bool,
    cache_only: bool,
) -> Result<InlineSweep> {
    if project_id.is_empty() || version_id.is_empty() {
        return Err(CoreError::Config(
            "inline sweep needs project and exact version".into(),
        ));
    }
    let connection = crate::store_hold::open_connection(db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    connection.busy_timeout(Duration::from_millis(5))?;
    connection.execute_batch("PRAGMA query_only=ON")?;
    let resolved = resolve_exact_commit(repo, version_id);
    let tree_hash = resolved
        .as_deref()
        .and_then(|commit| exact_tree(repo, commit));
    let version_id = resolved.as_deref().unwrap_or(version_id);
    let candidates = if diagnostic {
        proposals(&connection, project_id)?
    } else {
        accepted(&connection, project_id)?
    };
    let git_tool = git_version();
    let mut results = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let full_command = full_command(project_id, repo, version_id, &candidate.id);
        let Some(tree) = tree_hash.as_deref() else {
            results.push(unavailable(
                candidate,
                diagnostic,
                "exact version tree cannot be resolved",
                full_command,
            ));
            continue;
        };
        let parsed = match parse_params(&candidate.kind, candidate.schema, &candidate.params_json) {
            Ok(parsed) => parsed,
            Err(_) => {
                results.push(unavailable(
                    candidate,
                    diagnostic,
                    "accepted parameters cannot be parsed",
                    full_command,
                ));
                continue;
            }
        };
        // Chrome's executable identity alone cannot bind fixture-manifest and
        // stamp changes. C1 therefore remains explicit-only in the inline
        // route, without even starting Chrome --version.
        if matches!(&parsed, CheckParams::VisualMatch(_)) {
            results.push(unavailable(
                candidate,
                diagnostic,
                "C1 visual capture runs only on an explicit full check",
                full_command,
            ));
            continue;
        }
        // The ordinary engine reuses cached verdicts only for tree-only
        // checks. Recorded evidence can change without changing the tree.
        let tree_only = match &parsed {
            CheckParams::TextAbsent(text) => text.conditional_path_allowlist.is_empty(),
            CheckParams::SymbolAbsent(_)
            | CheckParams::UiCopyAbsent(_)
            | CheckParams::PathPresent(_)
            | CheckParams::FeatureDefaultPreserved(_)
            | CheckParams::TermEverywhere(_)
            | CheckParams::ForbiddenWords(_)
            | CheckParams::FileEquals(_)
            | CheckParams::AssertionFloor(_)
            | CheckParams::CiStepPresent(_) => true,
            _ => false,
        };
        if !diagnostic && tree_only {
            let tools = match &parsed {
                CheckParams::CiStepPresent(_) => json!({"git":git_tool,"ci":{
                    "adapter":"decisions.b4.actions.v1",
                    "yaml_parser":"yaml-rust2.0.13.0",
                    "baseline_version":candidate.applies_from_version,
                }}),
                _ => json!({"git":git_tool}),
            };
            let tools_json = serde_json::to_string(&tools)?;
            if let Some(previous) = cached(&connection, &candidate, version_id, tree, &tools_json)?
            {
                results.push(InlineRow {
                    decision_id: candidate.id,
                    revision: candidate.revision,
                    kind: candidate.kind,
                    state: previous.state,
                    reason: previous.reason,
                    checked_places: previous.checked_places,
                    from_cache: true,
                    advisory: false,
                    verdict_id: Some(previous.id),
                    full_command: None,
                });
                continue;
            }
        }
        if cache_only {
            let reason = match &parsed {
                CheckParams::CommandPasses(_) => {
                    "E quality checks run only on an explicit full check"
                }
                CheckParams::TestsBeforeCode(_)
                | CheckParams::ConsentRecorded(_)
                | CheckParams::OneHeavyJob(_)
                | CheckParams::RuleFilePresent(_)
                | CheckParams::TestPresentAndRuns(_)
                | CheckParams::GoldenOutput(_)
                | CheckParams::EditsWithinPaths(_)
                | CheckParams::PathsUntouched(_)
                | CheckParams::CommandForbidden(_)
                | CheckParams::DecisionSatisfied(_)
                | CheckParams::StaleClaim(_)
                | CheckParams::SummedTotals(_)
                | CheckParams::EmptyRun(_)
                | CheckParams::DelegatedRun(_)
                | CheckParams::EditDuringRun(_) => record_gap(&parsed),
                _ => "inline check pending within the 20-second budget",
            };
            results.push(unavailable(candidate, diagnostic, reason, full_command));
            continue;
        }
        let (outcome, unavailable_reason) = match &parsed {
            CheckParams::VisualMatch(_) => (None, Some("C1 visual capture runs only on an explicit full check")),
            CheckParams::CommandPasses(_) =>
                (None, Some("E quality checks run only on an explicit full check")),
            CheckParams::TextAbsent(_)
            | CheckParams::SymbolAbsent(_)
            | CheckParams::UiCopyAbsent(_)
            | CheckParams::PathPresent(_)
            | CheckParams::FeatureDefaultPreserved(_)
            | CheckParams::TermEverywhere(_)
            | CheckParams::ForbiddenWords(_) => (Some(checks::check_at(repo, version_id, &parsed)), None),
            CheckParams::FileEquals(files) => (Some(checks_fidelity::check_file_equals_at(repo, version_id, files)), None),
            CheckParams::AssertionFloor(floor) => (Some(assertion_adapter::check_at(repo, version_id, tree, floor)), None),
            CheckParams::CiStepPresent(step) => match candidate.applies_from_version.as_deref() {
                Some(baseline) => (Some(ci_adapter::check_at(repo, version_id, tree, baseline, step)), None),
                None => (None, Some("B4 needs a person's accepted baseline version before a workflow comparison")),
            },
            _ => (None, Some(record_gap(&parsed))),
        };
        if let Some(outcome) = outcome {
            if outcome.tree_hash.as_deref() != Some(tree) {
                results.push(unavailable(
                    candidate,
                    diagnostic,
                    "inline checker did not prove the exact tree",
                    full_command,
                ));
            } else {
                results.push(from_outcome(candidate, diagnostic, outcome, full_command));
            }
        } else {
            results.push(unavailable(
                candidate,
                diagnostic,
                unavailable_reason.unwrap_or("full check required"),
                full_command,
            ));
        }
    }
    Ok(InlineSweep {
        schema_version: 1,
        project_id: project_id.into(),
        version_id: version_id.into(),
        tree_hash,
        diagnostic,
        results,
    })
}

fn unavailable(candidate: Candidate, diagnostic: bool, reason: &str, command: String) -> InlineRow {
    InlineRow {
        decision_id: candidate.id,
        revision: candidate.revision,
        kind: candidate.kind,
        state: "unverifiable".into(),
        reason: Some(reason.into()),
        checked_places: 0,
        from_cache: false,
        advisory: diagnostic,
        verdict_id: None,
        full_command: Some(command),
    }
}

fn from_outcome(
    candidate: Candidate,
    diagnostic: bool,
    outcome: CheckOutcome,
    command: String,
) -> InlineRow {
    InlineRow {
        decision_id: candidate.id,
        revision: candidate.revision,
        kind: candidate.kind,
        state: state_name(outcome.state).into(),
        reason: outcome.reason,
        checked_places: outcome.checked_places,
        from_cache: false,
        advisory: diagnostic,
        verdict_id: None,
        full_command: (outcome.state == VerdictState::Unverifiable).then_some(command),
    }
}

fn record_gap(params: &CheckParams) -> &'static str {
    match params {
        CheckParams::Deferred(_) => "H1 needs a verified due-marker result and selected-version observation; run the full recorded check",
        CheckParams::TestPresentAndRuns(_) => "B2 needs a same-version recorded test tally and runnable result",
        CheckParams::GoldenOutput(_) => "C4 needs same-version recorded command output bound to the golden file",
        CheckParams::EditsWithinPaths(_) => "D1 needs complete attributed edit paths for the accepted session scope",
        CheckParams::PathsUntouched(_) => "D2 needs complete attributed edit paths for the accepted session scope",
        CheckParams::CommandForbidden(_) => "F1 needs a verified raw invocation and assured capture before absence can be checked",
        CheckParams::TestsBeforeCode(_) => "F2 lacks verified Change-specific completed edit and run order",
        CheckParams::ConsentRecorded(_) => "F3 lacks a typed person answer linked before the named action",
        CheckParams::OneHeavyJob(_) => "F4 lacks complete execution intervals across sessions",
        CheckParams::RuleFilePresent(_) => "B5 authorship needs completed edit and person-origin receipts; use the full check for an exact-tree missing-file witness",
        CheckParams::DecisionSatisfied(_) => "J1 lacks an exact claim-to-decision verdict link",
        CheckParams::StaleClaim(_) => "J2 lacks a closed relevant-edit and rerun order",
        CheckParams::SummedTotals(_) => "J3 lacks a bound single-run tally",
        CheckParams::EmptyRun(_) => "J4 lacks a complete bound run and examined-input receipt",
        CheckParams::DelegatedRun(_) => "J5 lacks the claim-to-run origin and session link",
        CheckParams::EditDuringRun(_) => "J6 lacks a verified run interval and ordered edit receipt",
        _ => "this check needs a full recorded-evidence procedure",
    }
}

fn proposals(conn: &Connection, project: &str) -> Result<Vec<Candidate>> {
    let mut statement = conn.prepare(
        "SELECT d.id,r.rev,r.kind,r.params_schema_version,r.params_json
         FROM decisions d JOIN decision_revisions r ON r.decision_id=d.id
         WHERE d.project_id=?1 AND r.rev=(SELECT max(r2.rev) FROM decision_revisions r2 WHERE r2.decision_id=d.id)
         ORDER BY d.id",
    )?;
    let rows = statement.query_map([project], |row| {
        Ok(Candidate {
            id: row.get(0)?,
            revision: row.get(1)?,
            kind: row.get(2)?,
            schema: row.get(3)?,
            params_json: row.get(4)?,
            applies_from_version: None,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn accepted(conn: &Connection, project: &str) -> Result<Vec<Candidate>> {
    let mut statement = conn.prepare(
        "SELECT d.id,a.id,a.rev,r.kind,r.params_schema_version,r.params_json,a.applies_from_version
         FROM decisions d JOIN decision_acceptances a ON a.decision_id=d.id
         JOIN decision_revisions r ON r.decision_id=d.id AND r.rev=a.rev
         WHERE d.project_id=?1 ORDER BY d.id,a.rowid DESC",
    )?;
    let rows = statement.query_map([project], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            Candidate {
                id: row.get(0)?,
                revision: row.get(2)?,
                kind: row.get(3)?,
                schema: row.get(4)?,
                params_json: row.get(5)?,
                applies_from_version: row.get(6)?,
            },
        ))
    })?;
    let mut selected = BTreeSet::new();
    let mut candidates = Vec::new();
    for row in rows {
        let (decision_id, act, candidate) = row?;
        if selected.contains(&decision_id)
            || !bridge::verified_read_only(
                conn,
                PersonActKind::Accept,
                &act,
                Duration::from_secs(2),
            )?
        {
            continue;
        }
        selected.insert(decision_id.clone());
        let mut retirements = conn.prepare(
            "SELECT id FROM decision_retirements WHERE decision_id=?1 ORDER BY rowid DESC",
        )?;
        let ids = retirements
            .query_map([&decision_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut retired = false;
        for id in ids {
            if bridge::verified_read_only(conn, PersonActKind::Retire, &id, Duration::from_secs(2))?
            {
                retired = true;
                break;
            }
        }
        if !retired {
            candidates.push(candidate);
        }
    }
    Ok(candidates)
}

fn cached(
    conn: &Connection,
    candidate: &Candidate,
    version: &str,
    tree: &str,
    tools: &str,
) -> Result<Option<CachedVerdict>> {
    let row = conn
        .query_row(
            "SELECT id,verdict,reason,basis_json FROM decision_verdicts
         WHERE decision_id=?1 AND rev=?2 AND version_id=?3 AND tree_hash=?4
           AND check_impl_version=?5 AND tool_versions_json=?6
         ORDER BY rowid DESC LIMIT 1",
            params![
                candidate.id,
                candidate.revision,
                version,
                tree,
                CHECK_IMPL_VERSION,
                tools
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((id, state, reason, basis)) = row else {
        return Ok(None);
    };
    let basis: serde_json::Value = serde_json::from_str(&basis)?;
    let Some(places) = basis
        .get("checked_places")
        .and_then(serde_json::Value::as_u64)
    else {
        return Ok(None);
    };
    Ok(Some(CachedVerdict {
        id,
        state,
        reason,
        checked_places: places,
    }))
}

fn git(repo: &Path, args: &[&str]) -> Option<String> {
    let output = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .args(args)
        .current_dir(repo)
        .output()
        .ok()?;
    output.status.success().then(|| {
        String::from_utf8(output.stdout)
            .ok()
            .map(|text| text.trim().to_owned())
    })?
}

/// Resolve HEAD or a unique 7–64-character hexadecimal abbreviation inside
/// the supervised read-only worker. Callers use the full immutable commit for
/// cache keys, tree checkers and commands.
pub fn resolve_exact_commit(repo: &Path, requested: &str) -> Option<String> {
    if requested != "HEAD"
        && (!(7..=64).contains(&requested.len())
            || !requested.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        return None;
    }
    let commit = git(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{requested}^{{commit}}"),
        ],
    )?;
    if !valid_oid(&commit)
        || (requested != "HEAD" && !commit.starts_with(&requested.to_ascii_lowercase()))
    {
        return None;
    }
    Some(commit)
}

fn exact_tree(repo: &Path, commit: &str) -> Option<String> {
    let tree = git(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{commit}^{{tree}}"),
        ],
    )?;
    valid_oid(&tree).then_some(tree)
}

fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|ch| ch.is_ascii_hexdigit())
}

fn git_version() -> String {
    crate::git_isolation::isolate_config(&mut Command::new("git"))
        .arg("--version")
        .output()
        .ok()
        .and_then(|output| output.status.success().then_some(output.stdout))
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .map_or_else(|| "unavailable".into(), |value| value.trim().into())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn full_command(project: &str, repo: &Path, version: &str, decision: &str) -> String {
    format!(
        "thruline decisions check --project={} --root={} --version={} --id={}",
        shell_quote(project),
        shell_quote(&repo.to_string_lossy()),
        shell_quote(version),
        shell_quote(decision),
    )
}

pub fn full_command_all(project: &str, repo: &Path, version: &str) -> String {
    format!(
        "thruline decisions check --project={} --root={} --version={} --all",
        shell_quote(project),
        shell_quote(&repo.to_string_lossy()),
        shell_quote(version),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decisions::checks_process::CommandForbiddenParams;
    use crate::decisions::params::{
        EditsWithinPathsParams, GoldenOutputParams, PathsUntouchedParams, TestPresentAndRunsParams,
    };

    #[test]
    fn recorded_kinds_name_the_missing_fact_in_plain_words() {
        let cases = [
            (
                CheckParams::TestPresentAndRuns(TestPresentAndRunsParams {
                    name_pattern: "test_x".into(),
                    command: "cargo test".into(),
                }),
                "same-version recorded test tally",
            ),
            (
                CheckParams::GoldenOutput(GoldenOutputParams {
                    command_id: "run-1".into(),
                    golden_path: "golden.txt".into(),
                    golden_sha256: "a".repeat(64),
                }),
                "same-version recorded command output",
            ),
            (
                CheckParams::EditsWithinPaths(EditsWithinPathsParams {
                    allowed_paths: vec!["src/**".into()],
                }),
                "complete attributed edit paths",
            ),
            (
                CheckParams::PathsUntouched(PathsUntouchedParams {
                    paths: vec!["tests/**".into()],
                }),
                "complete attributed edit paths",
            ),
            (
                CheckParams::CommandForbidden(CommandForbiddenParams {
                    patterns: vec!["--force".into()],
                    session_scope: None,
                }),
                "verified raw invocation and assured capture",
            ),
        ];
        for (params, expected) in cases {
            assert!(record_gap(&params).contains(expected), "{params:?}");
        }
    }
}
