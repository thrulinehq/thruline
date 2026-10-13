//! Partial F1–F4 and B5 projection from the existing append-only event record.
//!
//! A tool request is evidence that an agent asked to run a command or edit a
//! path. Current HQ version buckets and ledger events do not prove a complete
//! exact-tree census across sessions, global order, command intervals, Change
//! links, consent answers, or person-authored file edits. This adapter keeps
//! those observations available while explicitly refusing a complete census.

use std::path::Path;
use std::process::Command;

use rusqlite::OptionalExtension;
use serde_json::Value;

use crate::error::Result;
use crate::hq::{commands, edits, parse_ms};
use crate::ledger::Ledger;
use crate::redact::Masked;

use super::checks_process::{ProcessActor, ProcessCensus, ProcessCommand, ProcessEdit};
use super::checks_record::{EditActor, EditCensus, RecordedEdit};

/// Only captured requests are projected. Both censuses stay incomplete until
/// the missing receipts listed here exist and are verified.
#[derive(Debug, Clone)]
pub struct Phase2RecordProjection {
    pub target_verified: bool,
    pub process: ProcessCensus,
    pub rule_edits: EditCensus,
    pub missing: Vec<&'static str>,
}

/// Project one ledger project's tool requests toward a named immutable commit.
///
/// The target check prevents observations from being borrowed when a caller
/// supplied the wrong tree. It does not attribute any event to that commit:
/// every projected row carries an empty version/tree and incomplete receipt.
pub fn project_phase2_at(
    ledger: &Ledger,
    repo: &Path,
    ledger_project_id: &str,
    version_id: &str,
    tree_hash: &str,
) -> Result<Phase2RecordProjection> {
    let mut projection = Phase2RecordProjection {
        target_verified: false,
        process: ProcessCensus {
            version_id: version_id.into(),
            tree_hash: tree_hash.into(),
            complete: false,
            commands: Vec::new(),
            edits: Vec::new(),
            runs: Vec::new(),
            answers: Vec::new(),
            actions: Vec::new(),
        },
        rule_edits: EditCensus {
            from_version_id: None,
            version_id: version_id.into(),
            tree_hash: tree_hash.into(),
            complete: false,
            edits: Vec::new(),
        },
        missing: Vec::new(),
    };
    if !exact_commit_tree(repo, version_id, tree_hash) {
        projection.missing.push("exact_commit_tree_mismatch");
        return Ok(projection);
    }
    projection.target_verified = true;
    let project_exists = ledger
        .conn()
        .query_row(
            "SELECT 1 FROM projects WHERE id=?1",
            [ledger_project_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some();
    if !project_exists {
        projection.missing.push("ledger_project_missing");
        return Ok(projection);
    }
    projection.missing.extend([
        "hq_project_membership",
        "exact_event_version_tree",
        "cross_stream_event_order",
        "complete_edit_command_census",
        "command_end_time",
        "event_specific_change_and_run_order",
        "typed_person_consent",
        "person_edit_provenance",
        "confirmed_edit_result",
    ]);

    let mut stmt = ledger.conn().prepare(
        "SELECT id,session_id,ts,role,tool_name,source_tool_name,tool_input
         FROM events WHERE project_id=?1 AND kind='tool_use'
         ORDER BY session_id,seq",
    )?;
    let rows = stmt.query_map([ledger_project_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    })?;
    for row in rows {
        let (id, session_id, ts, role, tool_name, source_tool_name, input) = row?;
        let Some(input) = input.and_then(|raw| serde_json::from_str::<Value>(&raw).ok()) else {
            push_missing(&mut projection.missing, "tool_input_unreadable");
            continue;
        };
        let name = tool_name.as_deref().unwrap_or("");
        let source_name = source_tool_name.as_deref().unwrap_or("");
        let actor = if role.as_deref() == Some("assistant") {
            ProcessActor::Agent
        } else {
            ProcessActor::Unknown
        };
        if name == "Bash" || matches!(source_name, "exec_command" | "shell_command") {
            if let Some(command) = commands::command_of(&input) {
                projection.process.commands.push(ProcessCommand {
                    id: id.clone(),
                    session_id: session_id.clone(),
                    actor,
                    // The stored seq is local to its session; there is no
                    // verified global sequence for F2/F3.
                    seq: 0,
                    version_id: String::new(),
                    tree_hash: String::new(),
                    command: Masked::mask(&command).into_text(),
                    complete: false,
                    started_ms: ts.as_deref().and_then(parse_ms),
                    ended_ms: None,
                });
            } else {
                push_missing(&mut projection.missing, "command_input_unreadable");
            }
        }
        if edits::EDIT_TOOLS.contains(&name) {
            let observed = edits::edits_of(name, &input, None);
            if observed.is_empty() {
                push_missing(&mut projection.missing, "edit_path_unreadable");
            }
            for (part, edit) in observed.into_iter().enumerate() {
                let path = Masked::mask(&edit.path).into_text();
                projection.process.edits.push(ProcessEdit {
                    change_id: String::new(),
                    session_id: session_id.clone(),
                    seq: 0,
                    path: path.clone(),
                    actor,
                });
                projection.rule_edits.edits.push(RecordedEdit {
                    path,
                    actor: match actor {
                        ProcessActor::Agent => EditActor::Agent,
                        ProcessActor::Person => EditActor::Person,
                        ProcessActor::Unknown => EditActor::Unknown,
                    },
                    session_id: Some(session_id.clone()),
                    edit_id: Some(format!("{id}#{part}")),
                });
            }
        }
    }
    Ok(projection)
}

fn push_missing(missing: &mut Vec<&'static str>, code: &'static str) {
    if !missing.contains(&code) {
        missing.push(code);
    }
}

fn exact_commit_tree(repo: &Path, version_id: &str, tree_hash: &str) -> bool {
    fn oid(repo: &Path, spec: &str) -> Option<String> {
        let output = crate::git_isolation::isolate_config(&mut Command::new("git"))
            .args(["rev-parse", "--verify", "--end-of-options", spec])
            .current_dir(repo)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let oid = std::str::from_utf8(&output.stdout).ok()?.trim();
        (matches!(oid.len(), 40 | 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .then(|| oid.to_owned())
    }
    if !matches!(version_id.len(), 40 | 64)
        || !version_id.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !matches!(tree_hash.len(), 40 | 64)
        || !tree_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return false;
    }
    let Some(commit) = oid(repo, &format!("{version_id}^{{commit}}")) else {
        return false;
    };
    commit == version_id && oid(repo, &format!("{commit}^{{tree}}")).as_deref() == Some(tree_hash)
}
