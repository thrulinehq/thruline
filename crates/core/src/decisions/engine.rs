//! Accepted decision checks, exact-tree cache, and deterministic replay.
//!
//! This module runs only closed procedures. A proposal never enters the check
//! query until a separate person acceptance names its exact revision.

use std::path::Path;
use std::process::Command;

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use crate::chain::sha256_hex;
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;

use super::assertion_adapter;
use super::checks::{check_at, CheckOutcome, VerdictState};
use super::checks_claims;
use super::checks_fidelity;
use super::checks_preservation;
use super::checks_process;
use super::checks_visual;
use super::ci_adapter;
use super::params::{parse_params, CheckParams};
use super::record_adapter;
use super::record_adapter_phase2;
use super::storage::{self, VerdictInput};
use super::{attested_command_adapter, claim_record_adapter};

#[path = "receipt_limits.rs"]
mod receipt_limits;

pub const CHECK_IMPL_VERSION: &str = "decisions.0.2.c35.w2.1";

struct CheckedEvaluation {
    outcome: CheckOutcome,
    record_projection: Option<Value>,
    checkable_if: Option<String>,
}

impl From<CheckOutcome> for CheckedEvaluation {
    fn from(outcome: CheckOutcome) -> Self {
        Self {
            outcome,
            record_projection: None,
            checkable_if: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunResult {
    pub decision_id: String,
    pub revision: u32,
    pub state: VerdictState,
    pub tree_hash: Option<String>,
    pub from_cache: bool,
    pub verdict_id: Option<String>,
    pub outcome: CheckOutcome,
}

struct AcceptedCheck {
    id: String,
    revision: u32,
    kind: String,
    params_schema_version: u32,
    params_json: String,
    applies_from_version: String,
}

/// Check every active, accepted decision for one named version. A repeated
/// exact-tree computation uses its recorded row. Replay always recomputes and
/// refuses a changed result, leaving the historical row intact.
pub fn check_project_version(
    ledger: &Ledger,
    project_id: &str,
    repo: &Path,
    version_id: &str,
    computed_at: &str,
    replay: bool,
) -> Result<Vec<RunResult>> {
    check_project_version_selected(
        ledger,
        project_id,
        repo,
        version_id,
        computed_at,
        replay,
        None,
    )
}

/// Run one accepted decision without computing or appending rows for its peers.
pub fn check_decision_version(
    ledger: &Ledger,
    project_id: &str,
    repo: &Path,
    version_id: &str,
    computed_at: &str,
    replay: bool,
    decision_id: &str,
) -> Result<Vec<RunResult>> {
    check_project_version_selected(
        ledger,
        project_id,
        repo,
        version_id,
        computed_at,
        replay,
        Some(decision_id),
    )
}

fn check_project_version_selected(
    ledger: &Ledger,
    project_id: &str,
    repo: &Path,
    version_id: &str,
    computed_at: &str,
    replay: bool,
    selected_id: Option<&str>,
) -> Result<Vec<RunResult>> {
    if project_id.is_empty() || version_id.is_empty() || computed_at.is_empty() {
        return Err(CoreError::Config(
            "project, version and computation time are required".into(),
        ));
    }
    let mut accepted = accepted_checks(ledger, project_id)?;
    if let Some(id) = selected_id {
        accepted.retain(|decision| decision.id == id);
        if accepted.is_empty() {
            return Err(CoreError::Config(format!(
                "no accepted decision {id} in project {project_id}"
            )));
        }
    }
    let git_tool = git_version();
    let mut results = Vec::with_capacity(accepted.len());
    for decision in accepted {
        let Some(tree_hash) = resolve_tree(repo, version_id) else {
            let outcome = CheckOutcome::unavailable("version tree cannot be resolved", None);
            results.push(RunResult {
                decision_id: decision.id,
                revision: decision.revision,
                state: outcome.state,
                tree_hash: None,
                from_cache: false,
                verdict_id: None,
                outcome,
            });
            continue;
        };
        let params = parse_params(
            &decision.kind,
            decision.params_schema_version,
            &decision.params_json,
        )?;
        let tools = match &params {
            CheckParams::VisualMatch(visual) => json!({
                "git": git_tool,
                "visual": checks_visual::visual_tool_identity(visual),
            }),
            CheckParams::CiStepPresent(_) => json!({
                "git": git_tool,
                "ci": {
                    "adapter": "decisions.b4.actions.v1",
                    "yaml_parser": "yaml-rust2.0.13.0",
                    "baseline_version": decision.applies_from_version,
                },
            }),
            _ => json!({"git": git_tool}),
        };
        let tools_json = serde_json::to_string(&tools)?;
        // Recorded inputs can arrive or lose verifiable source bytes without
        // a tree change. Until their immutable fingerprint joins the key,
        // ordinary checks re-evaluate that evidence. Replay still compares
        // against the stored outcome after re-verifying its sources.
        let tree_only = match &params {
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
        let cached = if !replay && !tree_only {
            None
        } else {
            cached_outcome(
                ledger,
                &decision.id,
                decision.revision,
                version_id,
                &tree_hash,
                &tools_json,
            )?
        };
        if let Some((id, previous)) = cached {
            if !replay {
                results.push(RunResult {
                    decision_id: decision.id,
                    revision: decision.revision,
                    state: previous.state,
                    tree_hash: Some(tree_hash),
                    from_cache: true,
                    verdict_id: Some(id),
                    outcome: previous,
                });
                continue;
            }
            let evaluated = if let CheckParams::Deferred(deferred) = &params {
                let basis: String = ledger.conn().query_row(
                    "SELECT basis_json FROM decision_verdicts WHERE id=?1",
                    [&id],
                    |row| row.get(0),
                )?;
                let basis: Value = serde_json::from_str(&basis)?;
                let Some(stored) = basis.get("record_projection") else {
                    return Err(CoreError::Integrity(
                        "deferred replay lacks its marker source projection".into(),
                    ));
                };
                let pinned: super::deferred::DeferredProjection =
                    serde_json::from_value(stored.clone())?;
                let replayed = replay_deferred(
                    ledger, project_id, repo, version_id, &tree_hash, deferred, &pinned,
                )?;
                if serde_json::to_value(&replayed.projection)? != *stored {
                    return Err(CoreError::Integrity(
                        "deferred replay marker source projection differs".into(),
                    ));
                }
                CheckedEvaluation {
                    outcome: replayed.outcome,
                    record_projection: Some(stored.clone()),
                    checkable_if: None,
                }
            } else {
                checked_outcome(
                    ledger, project_id, repo, version_id, &params, &tree_hash, &decision,
                )?
            };
            let outcome = evaluated.outcome;
            if super::replay_basis::stable_verdict_basis(&previous)
                != super::replay_basis::stable_verdict_basis(&outcome)
            {
                return Err(CoreError::Integrity(format!(
                    "decision replay differs for {} revision {} on {}",
                    decision.id, decision.revision, version_id
                )));
            }
            results.push(RunResult {
                decision_id: decision.id,
                revision: decision.revision,
                state: previous.state,
                tree_hash: Some(tree_hash),
                from_cache: false,
                verdict_id: Some(id),
                outcome,
            });
            continue;
        }
        if replay {
            let outcome = CheckOutcome::unavailable(
                "no stored decision verdict to replay on this version",
                Some(tree_hash.clone()),
            );
            results.push(RunResult {
                decision_id: decision.id,
                revision: decision.revision,
                state: outcome.state,
                tree_hash: Some(tree_hash),
                from_cache: false,
                verdict_id: None,
                outcome,
            });
            continue;
        }
        let evaluated = checked_outcome(
            ledger, project_id, repo, version_id, &params, &tree_hash, &decision,
        )?;
        let outcome = evaluated.outcome;
        let verdict_id = storage::record_verdict(
            ledger,
            VerdictInput {
                decision_id: decision.id.clone(),
                revision: decision.revision,
                version_id: version_id.into(),
                tree_hash: tree_hash.clone(),
                verdict: state_name(outcome.state).into(),
                reason: outcome.reason.clone(),
                checkable_if: evaluated.checkable_if.or_else(|| {
                    (outcome.state == VerdictState::Unverifiable).then(|| {
                        "Provide the named version and required recorded evidence, then check again"
                            .into()
                    })
                }),
                findings: serde_json::to_value(&outcome.findings)?,
                surfaces: serde_json::to_value(&outcome.surfaces)?,
                check_impl_version: CHECK_IMPL_VERSION.into(),
                tool_versions: tools.clone(),
                computed_at: computed_at.into(),
                basis: json!({
                    "decision_id": decision.id,
                    "revision": decision.revision,
                    "version_id": version_id,
                    "tree_hash": tree_hash,
                    "params_sha256": sha256_hex(decision.params_json.as_bytes()),
                    "checked_places": outcome.checked_places,
                    "visual": &outcome.visual,
                    "not_looked_at": &outcome.not_looked_at,
                    "record_projection": evaluated.record_projection,
                }),
            },
        )?;
        results.push(RunResult {
            decision_id: decision.id,
            revision: decision.revision,
            state: outcome.state,
            tree_hash: Some(tree_hash),
            from_cache: false,
            verdict_id: Some(verdict_id),
            outcome,
        });
    }
    Ok(results)
}

fn checked_outcome(
    ledger: &Ledger,
    project_id: &str,
    repo: &Path,
    version_id: &str,
    params: &super::params::CheckParams,
    tree_hash: &str,
    decision: &AcceptedCheck,
) -> Result<CheckedEvaluation> {
    let mut record_projection = None;
    let mut receipt_missing = None;
    let mut outcome = match params {
        CheckParams::Deferred(deferred) => {
            let evaluated = if let Some(previous) = prior_deferred_fulfillment(
                ledger, project_id, repo, version_id, tree_hash, deferred, decision,
            )? {
                previous
            } else {
                super::deferred_record::check_recorded_at(
                    ledger, project_id, repo, version_id, tree_hash, deferred,
                )?
            };
            record_projection = Some(serde_json::to_value(&evaluated.projection)?);
            evaluated.outcome
        }
        CheckParams::VisualMatch(visual) => {
            let Some(db_path) = ledger.conn().path().filter(|path| !path.is_empty()) else {
                return Ok(CheckOutcome::unavailable(
                    "visual capture needs a file-backed ledger",
                    Some(tree_hash.into()),
                )
                .into());
            };
            let Ok(db_path) = std::fs::canonicalize(db_path) else {
                return Ok(CheckOutcome::unavailable(
                    "visual ledger path cannot be resolved",
                    Some(tree_hash.into()),
                )
                .into());
            };
            checks_visual::check_visual_at_for_decision(
                repo,
                version_id,
                visual,
                &db_path,
                &decision.id,
                i64::from(decision.revision),
            )
        }
        CheckParams::FileEquals(files) => {
            checks_fidelity::check_file_equals_at(repo, version_id, files)
        }
        CheckParams::GoldenOutput(golden) => {
            checks_fidelity::check_golden_output_at(ledger, repo, version_id, golden)?
        }
        CheckParams::AssertionFloor(floor) => {
            assertion_adapter::check_at(repo, version_id, tree_hash, floor)
        }
        CheckParams::CiStepPresent(step) => ci_adapter::check_at(
            repo,
            version_id,
            tree_hash,
            &decision.applies_from_version,
            step,
        ),
        CheckParams::RuleFilePresent(rule) => {
            let record = record_adapter_phase2::project_phase2_at(
                ledger, repo, project_id, version_id, tree_hash,
            )?;
            receipt_missing = Some(record.missing.clone());
            record_projection = Some(json!({
                "selectors": params,
                "target_verified": record.target_verified,
                "observed": {"edits": record.rule_edits.edits.len()},
                "missing": record.missing,
            }));
            checks_preservation::check_rule_file_present(repo, version_id, rule, &record.rule_edits)
        }
        CheckParams::CommandForbidden(rule) => attested_command_adapter::check_forbidden_at(
            ledger, repo, project_id, version_id, tree_hash, rule,
        )?,
        CheckParams::TestsBeforeCode(rule) => {
            let record = record_adapter_phase2::project_phase2_at(
                ledger, repo, project_id, version_id, tree_hash,
            )?;
            receipt_missing = Some(record.missing.clone());
            record_projection = Some(json!({
                "selectors": params,
                "target_verified": record.target_verified,
                "observed": {"commands": record.process.commands.len(), "edits": record.process.edits.len(), "runs": record.process.runs.len()},
                "missing": record.missing,
            }));
            checks_process::check_tests_before_code(version_id, tree_hash, rule, &record.process)
        }
        CheckParams::ConsentRecorded(rule) => {
            let record = record_adapter_phase2::project_phase2_at(
                ledger, repo, project_id, version_id, tree_hash,
            )?;
            receipt_missing = Some(record.missing.clone());
            record_projection = Some(json!({
                "selectors": params,
                "target_verified": record.target_verified,
                "observed": {"answers": record.process.answers.len(), "actions": record.process.actions.len()},
                "missing": record.missing,
            }));
            checks_process::check_consent_recorded(version_id, tree_hash, rule, &record.process)
        }
        CheckParams::OneHeavyJob(rule) => {
            let record = record_adapter_phase2::project_phase2_at(
                ledger, repo, project_id, version_id, tree_hash,
            )?;
            receipt_missing = Some(record.missing.clone());
            record_projection = Some(json!({
                "selectors": params,
                "target_verified": record.target_verified,
                "observed": {"commands": record.process.commands.len()},
                "missing": record.missing,
            }));
            checks_process::check_one_heavy_job(version_id, tree_hash, rule, &record.process)
        }
        CheckParams::DecisionSatisfied(_)
        | CheckParams::StaleClaim(_)
        | CheckParams::SummedTotals(_)
        | CheckParams::EmptyRun(_)
        | CheckParams::DelegatedRun(_)
        | CheckParams::EditDuringRun(_) => {
            let projection = claim_record_adapter::project_claim_at(
                ledger, repo, project_id, version_id, tree_hash, params,
            )?;
            receipt_missing = Some(projection.missing.clone());
            record_projection = Some(json!({
                "selectors": projection.selectors,
                "sources": projection.sources,
                "missing": projection.missing,
            }));
            match params {
                CheckParams::DecisionSatisfied(_) => {
                    checks_claims::check_decision_satisfied(&projection.census)
                }
                CheckParams::StaleClaim(_) => checks_claims::check_stale_claim(&projection.census),
                CheckParams::SummedTotals(_) => {
                    checks_claims::check_summed_totals(&projection.census)
                }
                CheckParams::EmptyRun(_) => checks_claims::check_empty_run(&projection.census),
                CheckParams::DelegatedRun(_) => {
                    checks_claims::check_delegated_run(&projection.census)
                }
                CheckParams::EditDuringRun(_) => {
                    checks_claims::check_edit_during_run(&projection.census)
                }
                _ => unreachable!("J selector group"),
            }
        }
        _ => {
            match record_adapter::check_recorded_at(ledger, repo, version_id, tree_hash, params)? {
                Some(outcome) => outcome,
                None => check_at(repo, version_id, params),
            }
        }
    };
    if outcome.tree_hash.is_none() {
        outcome.tree_hash = Some(tree_hash.into());
    } else if outcome.tree_hash.as_deref() != Some(tree_hash) {
        return Ok(CheckOutcome::unavailable(
            "version tree changed during check",
            Some(tree_hash.into()),
        )
        .into());
    }
    let checkable_if = if outcome.state == VerdictState::Unverifiable {
        receipt_missing
            .as_deref()
            .and_then(|missing| {
                receipt_limits::for_incomplete(params, missing, outcome.reason.as_deref())
            })
            .map(|limit| {
                outcome.reason = Some(limit.reason.into());
                limit.checkable_if.into()
            })
    } else {
        None
    };
    Ok(CheckedEvaluation {
        outcome,
        record_projection,
        checkable_if,
    })
}

fn replay_deferred(
    ledger: &Ledger,
    project: &str,
    repo: &Path,
    version: &str,
    tree: &str,
    params: &super::deferred::DeferredDecisionParams,
    pinned: &super::deferred::DeferredProjection,
) -> Result<super::deferred::DeferredEvaluation> {
    let Some(selected) = super::deferred_record::selected_commit(repo, version, tree) else {
        return Err(CoreError::Integrity(
            "deferred replay selected version and tree differ".into(),
        ));
    };
    let target = pinned.fulfillment.as_ref();
    let replay_version = target.map_or(selected.as_str(), |target| target.version_id.as_str());
    let replay_tree = target.map_or(tree, |target| target.tree_hash.as_str());
    let mut evaluated = super::deferred_record::check_pinned_at(
        ledger,
        project,
        repo,
        replay_version,
        replay_tree,
        params,
        pinned,
    )?;
    if replay_version != selected {
        if !is_ancestor(repo, replay_version, &selected)
            || evaluated.outcome.state != VerdictState::Holds
        {
            return Err(CoreError::Integrity(
                "deferred fulfillment cannot be verified on an ancestor version".into(),
            ));
        }
        evaluated.outcome.tree_hash = Some(tree.into());
        evaluated.outcome.reason = Some(format!(
            "Deferred action was satisfied on version {replay_version}."
        ));
    }
    Ok(evaluated)
}

/// One verified fulfillment closes a deferred action. Later descendants retain
/// that history, without retiring the acceptance or reusing an unverified row.
fn prior_deferred_fulfillment(
    ledger: &Ledger,
    project: &str,
    repo: &Path,
    version: &str,
    tree: &str,
    params: &super::deferred::DeferredDecisionParams,
    decision: &AcceptedCheck,
) -> Result<Option<super::deferred::DeferredEvaluation>> {
    let Some(selected) = super::deferred_record::selected_commit(repo, version, tree) else {
        return Ok(None);
    };
    let mut statement=ledger.conn().prepare("SELECT basis_json FROM decision_verdicts WHERE decision_id=?1 AND rev=?2 AND verdict='holds' AND check_impl_version=?3 ORDER BY rowid LIMIT 129")?;
    let rows = statement
        .query_map(
            params![decision.id, decision.revision, CHECK_IMPL_VERSION],
            |row| row.get::<_, String>(0),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() > 128 {
        return Ok(None);
    }
    for row in rows {
        let Ok(basis) = serde_json::from_str::<Value>(&row) else {
            continue;
        };
        if basis.get("params_sha256").and_then(Value::as_str)
            != Some(sha256_hex(decision.params_json.as_bytes()).as_str())
        {
            continue;
        }
        let Some(value) = basis.get("record_projection") else {
            continue;
        };
        let Ok(projection) =
            serde_json::from_value::<super::deferred::DeferredProjection>(value.clone())
        else {
            continue;
        };
        let Some(fulfillment) = &projection.fulfillment else {
            continue;
        };
        if projection.lifecycle != super::deferred::Lifecycle::Closed
            || !is_ancestor(repo, &fulfillment.version_id, &selected)
        {
            continue;
        }
        let evaluated = replay_deferred(ledger, project, repo, version, tree, params, &projection);
        if let Ok(evaluated) = evaluated {
            if evaluated.outcome.state == VerdictState::Holds
                && serde_json::to_value(&evaluated.projection)? == *value
            {
                return Ok(Some(evaluated));
            }
        }
    }
    Ok(None)
}

fn is_ancestor(repo: &Path, old: &str, new: &str) -> bool {
    crate::git_isolation::isolate_config(&mut Command::new("git"))
        .args(["merge-base", "--is-ancestor", old, new])
        .current_dir(repo)
        .output()
        .ok()
        .is_some_and(|out| out.status.success())
}

/// Reuse the closed dispatcher for a deferred typed action. The wrapper
/// validates recursion and capture-only actions before invoking this reader.
pub fn evaluate_params_at(
    ledger: &Ledger,
    project_id: &str,
    repo: &Path,
    version_id: &str,
    tree_hash: &str,
    params: &CheckParams,
    baseline_version: &str,
) -> Result<CheckOutcome> {
    let decision = AcceptedCheck {
        id: "deferred-action".into(),
        revision: 0,
        kind: params.kind().into(),
        params_schema_version: super::params::PARAMS_SCHEMA_VERSION,
        params_json: params.json()?,
        applies_from_version: baseline_version.into(),
    };
    Ok(checked_outcome(
        ledger, project_id, repo, version_id, params, tree_hash, &decision,
    )?
    .outcome)
}

fn resolve_tree(repo: &Path, version_id: &str) -> Option<String> {
    let commit = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .args([
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{version_id}^{{commit}}"),
        ])
        .current_dir(repo)
        .output()
        .ok()?;
    if !commit.status.success() {
        return None;
    }
    let commit = std::str::from_utf8(&commit.stdout).ok()?.trim();
    if commit.len() < 40 || !commit.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    let tree = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .args([
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{commit}^{{tree}}"),
        ])
        .current_dir(repo)
        .output()
        .ok()?;
    if !tree.status.success() {
        return None;
    }
    let tree = std::str::from_utf8(&tree.stdout).ok()?.trim();
    (tree.len() >= 40 && tree.chars().all(|ch| ch.is_ascii_hexdigit())).then(|| tree.into())
}

fn accepted_checks(ledger: &Ledger, project_id: &str) -> Result<Vec<AcceptedCheck>> {
    storage::accepted_for_project(ledger, project_id)?
        .into_iter()
        .map(|decision| {
            Ok(AcceptedCheck {
                id: decision.id,
                revision: decision.revision,
                kind: decision.kind,
                params_schema_version: decision.params_schema_version,
                params_json: serde_json::to_string(&decision.params)?,
                applies_from_version: decision.applies_from_version,
            })
        })
        .collect()
}

fn cached_outcome(
    ledger: &Ledger,
    decision_id: &str,
    revision: u32,
    version_id: &str,
    tree_hash: &str,
    tools_json: &str,
) -> Result<Option<(String, CheckOutcome)>> {
    let row = ledger
        .conn()
        .query_row(
            "SELECT id,verdict,reason,findings_json,surfaces_json,basis_json
             FROM decision_verdicts
             WHERE decision_id=?1 AND rev=?2 AND version_id=?3 AND tree_hash=?4
               AND check_impl_version=?5 AND tool_versions_json=?6
             ORDER BY rowid DESC LIMIT 1",
            params![
                decision_id,
                revision,
                version_id,
                tree_hash,
                CHECK_IMPL_VERSION,
                tools_json
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?;
    let Some((id, verdict, reason, findings, surfaces, basis)) = row else {
        return Ok(None);
    };
    let Ok(basis) = serde_json::from_str::<Value>(&basis) else {
        return Ok(None);
    };
    let Some(checked_places) = basis.get("checked_places").and_then(Value::as_u64) else {
        return Ok(None);
    };
    let visual = basis
        .get("visual")
        .cloned()
        .map(serde_json::from_value)
        .transpose()?
        .unwrap_or_default();
    let Ok(surfaces) = serde_json::from_str(&surfaces) else {
        return Ok(None);
    };
    let state: VerdictState = serde_json::from_value(Value::String(verdict))?;
    let outcome = CheckOutcome {
        state,
        tree_hash: Some(tree_hash.into()),
        reason,
        checked_places,
        findings: serde_json::from_str(&findings)?,
        surfaces,
        not_looked_at: basis
            .get("not_looked_at")
            .cloned()
            .map(serde_json::from_value)
            .transpose()?
            .unwrap_or_default(),
        visual,
    };
    Ok(Some((id, outcome)))
}

pub const fn state_name(state: VerdictState) -> &'static str {
    match state {
        VerdictState::Holds => "holds",
        VerdictState::Broken => "broken",
        VerdictState::Unverifiable => "unverifiable",
        VerdictState::NotApplicable => "not_applicable",
    }
}

fn git_version() -> String {
    let output = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .arg("--version")
        .output();
    match output {
        Ok(output) if output.status.success() => std::str::from_utf8(&output.stdout)
            .map_or_else(|_| "unavailable".into(), |text| text.trim().to_owned()),
        _ => "unavailable".into(),
    }
}
