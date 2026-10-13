//! Re-verify recorded due markers without executing a marker command.
use super::{
    deferred::{
        self, DeferredCondition, DeferredDecisionParams, DeferredEvaluation, DeferredProjection,
        DueState, Lifecycle, SourceReceipt,
    },
    record_source::{self, SourceEvent},
};
use crate::{error::Result, Ledger};
use rusqlite::OptionalExtension;
use serde_json::Value;
use std::{path::Path, process::Command};

pub fn check_recorded_at(
    ledger: &Ledger,
    project_id: &str,
    repo: &Path,
    version_id: &str,
    tree: &str,
    params: &DeferredDecisionParams,
) -> Result<DeferredEvaluation> {
    check_selected_at(ledger, project_id, repo, version_id, tree, params, None)
}

/// Replay selects exactly the historical marker sources. Newly ingested
/// observations cannot silently advance an earlier waiting verdict.
pub fn check_pinned_at(
    ledger: &Ledger,
    project_id: &str,
    repo: &Path,
    version_id: &str,
    tree: &str,
    params: &DeferredDecisionParams,
    projection: &DeferredProjection,
) -> Result<DeferredEvaluation> {
    let ids = projection
        .sources
        .iter()
        .map(|source| source.event_id.clone())
        .collect::<Vec<_>>();
    check_selected_at(
        ledger,
        project_id,
        repo,
        version_id,
        tree,
        params,
        Some(&ids),
    )
}

fn check_selected_at(
    ledger: &Ledger,
    project_id: &str,
    repo: &Path,
    version_id: &str,
    tree: &str,
    params: &DeferredDecisionParams,
    pinned: Option<&[String]>,
) -> Result<DeferredEvaluation> {
    params.validate()?;
    let Some(commit) = selected_commit(repo, version_id, tree) else {
        return Ok(deferred::unavailable(
            params,
            tree,
            "selected version and tree cannot be verified",
        ));
    };
    let mut projection = project_at(ledger, project_id, repo, &commit, tree, params, pinned)?;
    let action = if projection.due_state == DueState::Due {
        // Execution-dependent checks need both the existing exact-tree run
        // proof and a source link ordering that exact run after this marker.
        // Frozen producers do not provide that cross-receipt link.
        if action_needs_order(&params.action) {
            Some(super::checks::CheckOutcome::unavailable("The record cannot link this action's exact-version execution after the due marker.",Some(tree.into())))
        } else {
            match version_after_marker(ledger, repo, &commit, &projection, pinned)? {
                Some((call, result)) => {
                    projection.sources.push(source_receipt(&call));
                    projection.sources.push(source_receipt(&result));
                    Some(super::engine::evaluate_params_at(
                        ledger,
                        project_id,
                        repo,
                        &commit,
                        tree,
                        &params.action,
                        &commit,
                    )?)
                }
                None => Some(super::checks::CheckOutcome::unavailable(
                    "The record has no verified selected-version observation after the due marker.",
                    Some(tree.into()),
                )),
            }
        }
    } else {
        None
    };
    let mut evaluated = deferred::evaluate(params, tree, projection, action);
    if evaluated.projection.lifecycle == Lifecycle::Closed {
        evaluated.projection.fulfillment = Some(deferred::DeferredFulfillment {
            version_id: commit,
            tree_hash: tree.into(),
        });
    }
    Ok(evaluated)
}

pub(crate) fn selected_commit(repo: &Path, version: &str, tree: &str) -> Option<String> {
    let commit = git_oid(repo, &format!("{version}^{{commit}}"))?;
    (git_oid(repo, &format!("{commit}^{{tree}}")).as_deref() == Some(tree)).then_some(commit)
}

fn action_needs_order(params: &super::params::CheckParams) -> bool {
    use super::params::CheckParams;
    !matches!(
        params,
        CheckParams::TextAbsent(_)
            | CheckParams::SymbolAbsent(_)
            | CheckParams::UiCopyAbsent(_)
            | CheckParams::PathPresent(_)
            | CheckParams::FeatureDefaultPreserved(_)
            | CheckParams::TermEverywhere(_)
            | CheckParams::ForbiddenWords(_)
            | CheckParams::FileEquals(_)
            | CheckParams::AssertionFloor(_)
    )
}

fn version_after_marker(
    ledger: &Ledger,
    repo: &Path,
    version: &str,
    projection: &DeferredProjection,
    pinned: Option<&[String]>,
) -> Result<Option<(SourceEvent, SourceEvent)>> {
    let expected = format!(
        "git -C {} rev-parse HEAD",
        shell_quote(&repo.to_string_lossy())
    );
    for marker in &projection.sources {
        let Some(source) = record_source::verified_event(
            ledger,
            &member_for_event(ledger, &marker.event_id)?,
            &marker.event_id,
        )?
        else {
            continue;
        };
        if source.kind != "tool_result" {
            continue;
        }
        let mut statement=ledger.conn().prepare("SELECT id FROM events WHERE project_id=?1 AND session_id=?2 AND seq>?3 AND kind='tool_use' AND tool_name='Bash' AND CASE WHEN json_valid(tool_input) THEN (json_extract(tool_input,'$.command')=?4 OR json_extract(tool_input,'$.cmd')=?4) ELSE 0 END ORDER BY seq LIMIT 1001")?;
        let ids = statement
            .query_map(
                rusqlite::params![source.project_id, source.session_id, source.seq, expected],
                |row| row.get::<_, String>(0),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if ids.len() > 1000 {
            return Ok(None);
        }
        for id in ids {
            if pinned.is_some_and(|ids| !ids.contains(&id)) {
                continue;
            }
            let Some((call, result)) =
                record_source::verified_completed_call(ledger, &source.project_id, &id)?
            else {
                continue;
            };
            if command_text(&call).as_deref() != Some(&expected) {
                continue;
            }
            if completed_output(&result)
                .is_some_and(|(exit, output)| exit == 0 && output.trim() == version)
            {
                return Ok(Some((call, result)));
            }
        }
    }
    Ok(None)
}
fn member_for_event(ledger: &Ledger, id: &str) -> Result<String> {
    Ok(ledger
        .conn()
        .query_row("SELECT project_id FROM events WHERE id=?1", [id], |row| {
            row.get(0)
        })?)
}

pub fn project_at(
    ledger: &Ledger,
    project_id: &str,
    repo: &Path,
    version_id: &str,
    tree: &str,
    params: &DeferredDecisionParams,
    pinned: Option<&[String]>,
) -> Result<DeferredProjection> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut projection = DeferredProjection {
        lifecycle: Lifecycle::Open,
        due_state: DueState::Unknown,
        whose_move: None,
        waits_on: params.condition.clone(),
        sources: vec![],
        missing: vec![],
        fulfillment: None,
    };
    let Some(commit) = git_oid(repo, &format!("{version_id}^{{commit}}")) else {
        projection
            .missing
            .push("selected version is unavailable".into());
        return Ok(projection);
    };
    if git_oid(repo, &format!("{commit}^{{tree}}")).as_deref() != Some(tree) {
        projection
            .missing
            .push("selected version and tree differ".into());
        return Ok(projection);
    }
    let members = project_members(ledger, project_id)?;
    let repo_path = std::fs::canonicalize(repo).ok();
    let mut candidates = Vec::new();
    let Some(expected) = marker_command(&params.condition, repo) else {
        projection
            .missing
            .push("the condition does not name this repository".into());
        return Ok(projection);
    };
    for member in members {
        let root: Option<String> = ledger
            .conn()
            .query_row(
                "SELECT root_path FROM projects WHERE id=?1",
                [&member],
                |row| row.get(0),
            )
            .optional()?;
        if root
            .as_deref()
            .and_then(|root| std::fs::canonicalize(root).ok())
            != repo_path
            || repo_path.is_none()
        {
            continue;
        }
        let mut statement=ledger.conn().prepare("SELECT id FROM events WHERE project_id=?1 AND kind='tool_use' AND tool_name='Bash' AND CASE WHEN json_valid(tool_input) THEN (json_extract(tool_input,'$.command')=?2 OR json_extract(tool_input,'$.cmd')=?2) ELSE 0 END ORDER BY session_id,seq LIMIT 1001")?;
        let ids = statement
            .query_map(rusqlite::params![member, expected], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if ids.len() > 1000 {
            projection
                .missing
                .push(format!("command-source budget reached for {member}"));
            return Ok(projection);
        }
        for id in ids {
            if std::time::Instant::now() >= deadline {
                projection
                    .missing
                    .push("due-marker verification budget reached".into());
                return Ok(projection);
            }
            if pinned.is_some_and(|ids| !ids.contains(&id)) {
                continue;
            }
            let Some((call, result)) =
                record_source::verified_completed_call(ledger, &member, &id)?
            else {
                continue;
            };
            if result.is_error
                || call.is_error
                || call.is_sidechain
                || !matches!(call.role.as_deref(), None | Some("assistant"))
            {
                continue;
            }
            let Some(command) = command_text(&call) else {
                continue;
            };
            if command != expected {
                continue;
            }
            let Some((exit, output)) = completed_output(&result) else {
                continue;
            };
            if exit != 0 {
                continue;
            }
            let due = match &params.condition {
                DeferredCondition::Date { at_utc } => {
                    if deferred::valid_utc(output.trim()) {
                        Some(if output.trim() >= at_utc.as_str() {
                            DueState::Due
                        } else {
                            DueState::Waiting
                        })
                    } else {
                        None
                    }
                }
                DeferredCondition::Tag { repository, .. } => {
                    if std::fs::canonicalize(repository).ok() != repo_path {
                        None
                    } else {
                        let oid = output.trim();
                        if valid_oid(oid)
                            && git_oid(repo, &format!("{oid}^{{commit}}")).as_deref() == Some(oid)
                        {
                            let is_ancestor =
                                crate::git_isolation::isolate_config(&mut Command::new("git"))
                                    .args(["merge-base", "--is-ancestor", oid, &commit])
                                    .current_dir(repo)
                                    .status()
                                    .ok()
                                    .is_some_and(|s| s.success());
                            Some(if is_ancestor {
                                DueState::Due
                            } else {
                                DueState::Waiting
                            })
                        } else {
                            None
                        }
                    }
                }
                DeferredCondition::PublishedRelease { tag, .. } => release_due(&output, tag),
            };
            if let Some(due) = due {
                candidates.push((call, result, due));
            }
        }
    }
    if candidates.is_empty() {
        projection
            .missing
            .push("no complete source-verified matching marker query/result".into());
        return Ok(projection);
    }
    // There is no global order between source sessions. Agreeing observations
    // may be combined; contradictory states across sessions abstain.
    let mut latest =
        std::collections::BTreeMap::<String, (SourceEvent, SourceEvent, DueState)>::new();
    for row in candidates {
        let session = row.0.session_id.clone();
        if latest.get(&session).is_none_or(|old| row.1.seq > old.1.seq) {
            latest.insert(session, row);
        }
    }
    let mut state = None;
    for (_, (call, result, due)) in latest {
        if state.is_some_and(|state| state != due) {
            projection
                .missing
                .push("matching marker observations have no verified cross-session order".into());
            projection.due_state = DueState::Unknown;
            return Ok(projection);
        }
        state = Some(due);
        projection.sources.push(source_receipt(&call));
        projection.sources.push(source_receipt(&result));
    }
    projection.due_state = state.unwrap_or(DueState::Unknown);
    Ok(projection)
}

pub(crate) fn source_receipt(event: &SourceEvent) -> SourceReceipt {
    SourceReceipt {
        event_id: event.id.clone(),
        session_id: event.session_id.clone(),
        seq: event.seq,
        raw_file: event.raw_file.clone(),
        raw_line: event.raw_line,
        raw_sha256: event.raw_sha256.clone(),
        chain_hash: event.chain_hash.clone(),
    }
}

pub(crate) fn project_members(ledger: &Ledger, project: &str) -> Result<Vec<String>> {
    if let Some(project) = crate::hq::find_project(ledger, project)? {
        return Ok(project
            .members
            .into_iter()
            .map(|member| member.ledger_project_id)
            .collect());
    }
    Ok(vec![project.into()])
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
pub fn marker_command(condition: &DeferredCondition, repo: &Path) -> Option<String> {
    match condition {
        DeferredCondition::Date { .. } => Some("date -u +%Y-%m-%dT%H:%M:%SZ".into()),
        DeferredCondition::Tag { name, repository }
            if deferred::valid_tag(name) && Path::new(repository) == repo =>
        {
            Some(format!(
                "git -C {} rev-parse --verify refs/tags/{name}^{{commit}}",
                shell_quote(repository)
            ))
        }
        DeferredCondition::PublishedRelease { repository, tag } => Some(format!(
            "gh release view {tag} --repo {repository} --json tagName,isDraft,publishedAt"
        )),
        _ => None,
    }
}

pub(crate) fn command_text(event: &SourceEvent) -> Option<String> {
    let input: Value = serde_json::from_str(event.tool_input.as_deref()?).ok()?;
    input
        .get("command")
        .or_else(|| input.get("cmd"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Only complete native envelopes are recognized. A sentence mentioning an
/// exit code inside stdout cannot become a completion discriminator.
pub(crate) fn completed_output(event: &SourceEvent) -> Option<(i64, String)> {
    if event.is_error {
        return None;
    }
    let result: Value = serde_json::from_str(event.tool_result.as_deref()?).ok()?;
    if let Some(native) = result.get("toolUseResult").and_then(Value::as_object) {
        let code = native
            .get("exitCode")
            .or_else(|| native.get("exit_code"))
            .and_then(Value::as_i64)?;
        let stdout = native.get("stdout").and_then(Value::as_str)?;
        if native.get("interrupted").and_then(Value::as_bool) == Some(true) {
            return None;
        }
        return Some((code, stdout.into()));
    }
    let content = result.get("content")?;
    if let Some(object) = content.as_object() {
        return Some((
            object.get("exit_code")?.as_i64()?,
            object
                .get("output")
                .or_else(|| object.get("stdout"))?
                .as_str()?
                .to_owned(),
        ));
    }
    let text = content.as_str()?;
    let (headers, output) = text.split_once("\nFinal output:\n")?;
    let mut exit = None;
    for line in headers.lines() {
        if let Some(value) = line.strip_prefix("Process exited with code ") {
            if exit.is_some() {
                return None;
            }
            exit = Some(value.parse::<i64>().ok()?);
        } else if !(line.starts_with("Chunk ID: ")
            || line.starts_with("Wall time: ")
            || line.starts_with("Original token count: "))
        {
            return None;
        }
    }
    Some((exit?, output.into()))
}

fn release_due(output: &str, tag: &str) -> Option<DueState> {
    let value: Value = serde_json::from_str(output.trim()).ok()?;
    if value.get("tagName")?.as_str()? != tag {
        return None;
    }
    let draft = value.get("isDraft")?.as_bool()?;
    let published = value.get("publishedAt")?;
    if draft {
        return published.is_null().then_some(DueState::Waiting);
    }
    if published.is_null() {
        return Some(DueState::Waiting);
    }
    deferred::valid_utc(published.as_str()?).then_some(DueState::Due)
}
fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|b| b.is_ascii_hexdigit())
}
fn git_oid(repo: &Path, spec: &str) -> Option<String> {
    let out = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .args(["rev-parse", "--verify", "--end-of-options", spec])
        .current_dir(repo)
        .output()
        .ok()?;
    let text = std::str::from_utf8(&out.stdout).ok()?.trim();
    (out.status.success() && valid_oid(text)).then(|| text.into())
}
