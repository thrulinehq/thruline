//! Read-only person work intervals with qualified source coverage.
use super::deferred::SourceReceipt;
use crate::{error::Result, Ledger};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    ObservedPrefix,
    MissingSource,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonThread {
    pub id: String,
    pub session_id: String,
    pub prompt: String,
    pub span_start: u64,
    pub span_end: u64,
    pub source: SourceReceipt,
    pub coverage: Coverage,
    pub qualification: String,
    pub observed_through_seq: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PersonThreadReport {
    pub threads: Vec<PersonThread>,
    pub observed_edit_threads: u64,
    pub missing: Vec<String>,
}
/// A verified edit means this is no longer a "no edit followed" row. It does
/// not claim the person's requested work is complete or correct.
pub fn open_for_project(ledger: &Ledger, project_id: &str) -> Result<PersonThreadReport> {
    use super::{
        deferred_record::{project_members, source_receipt},
        record_source,
    };
    use crate::hq::prompts::{classify, names_work, PromptClass};
    let mut report = PersonThreadReport::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    for project in project_members(ledger, project_id)? {
        let mut statement=ledger.conn().prepare("SELECT id,session_id,seq,kind,tool_name FROM events WHERE project_id=?1 ORDER BY session_id,seq LIMIT 10001")?;
        let rows = statement
            .query_map([&project], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if rows.len() > 10000 {
            report
                .missing
                .push(format!("person-thread source budget reached for {project}"));
        }
        let mut pending: Option<PersonThread> = None;
        let mut completed_edits = std::collections::BTreeMap::<i64, i64>::new();
        let mut session = String::new();
        let mut after_wrapper = false;
        for (id, current, seq, kind, tool_name) in rows {
            if std::time::Instant::now() >= deadline {
                if let Some(mut row) = pending.take() {
                    row.coverage = Coverage::MissingSource;
                    row.qualification="The recorded work interval remains open; source verification reached its budget.".into();
                    report.threads.push(row);
                }
                report.missing.push(format!(
                    "person-thread source verification budget reached for {project}"
                ));
                return Ok(report);
            }
            if current != session {
                if let Some(row) = pending.take() {
                    report.threads.push(row);
                }
                session = current.clone();
                after_wrapper = false;
                completed_edits.clear();
            }
            if let Some(row) = pending.as_mut() {
                row.observed_through_seq = seq;
            }
            if kind == "user_msg" {
                let Some(event) = record_source::verified_event(ledger, &project, &id)? else {
                    report
                        .missing
                        .push(format!("source cannot be verified for person event {id}"));
                    if let Some(row) = pending.as_mut() {
                        row.coverage = Coverage::MissingSource;
                        row.qualification="No following edit can be established because part of this recorded interval has missing source.".into();
                    }
                    after_wrapper = false;
                    continue;
                };
                if event.is_sidechain || event.role.as_deref() != Some("user") {
                    continue;
                }
                let Some(text) = event.text.as_deref() else {
                    continue;
                };
                let class = classify(text, after_wrapper);
                after_wrapper = matches!(class, PromptClass::CommandWrapper);
                let PromptClass::Human { text: human } = class else {
                    continue;
                };
                // A wrapper-derived replacement is not a contiguous source
                // slice. It stays visible in HQ, without an invented receipt.
                if human != text || !names_work(&human) {
                    continue;
                }
                if let Some(row) = pending.take() {
                    report.threads.push(row);
                }
                pending=Some(PersonThread{id:format!("person-work:{}",event.id),session_id:event.session_id.clone(),prompt:crate::redact::Masked::mask(&human).text().into(),span_start:0,span_end:human.len() as u64,source:source_receipt(&event),coverage:Coverage::ObservedPrefix,qualification:"No completed edit is recorded in this observed thread interval. The record does not prove that no edit happened elsewhere.".into(),observed_through_seq:event.seq});
            } else if kind == "tool_use" {
                after_wrapper = false;
                if !matches!(tool_name.as_deref(), Some("Edit" | "Write" | "MultiEdit")) {
                    continue;
                }
                let Some(row) = pending.as_ref() else {
                    continue;
                };
                let Some((call, result)) =
                    record_source::verified_completed_call(ledger, &project, &id)?
                else {
                    report.missing.push(format!(
                        "No complete, verified edit request/result pair for event {id}."
                    ));
                    if let Some(row) = pending.as_mut() {
                        row.coverage = Coverage::MissingSource;
                        row.qualification = "The recorded work interval remains open; a following edit has no complete, verified completion receipt.".into();
                    }
                    continue;
                };
                if call.seq <= row.source.seq
                    || call.is_error
                    || result.is_error
                    || !matches!(
                        call.tool_name.as_deref(),
                        Some("Edit" | "Write" | "MultiEdit")
                    )
                {
                    continue;
                }
                if successful_edit(&call, &result) {
                    completed_edits.insert(result.seq, call.seq);
                } else if call.source == "codex"
                    && call.source_tool_name.as_deref() == Some("apply_patch")
                    && match super::deferred_record::completed_output(&result) {
                        Some((code, _)) => code == 0,
                        None => true,
                    }
                {
                    report.missing.push(format!(
                        "No closed, path-matching apply_patch completion for event {id}."
                    ));
                    if let Some(row) = pending.as_mut() {
                        row.coverage = Coverage::MissingSource;
                        row.qualification = "The recorded work interval remains open; the following patch has no complete, path-matching completion receipt.".into();
                    }
                }
            } else if kind == "tool_result" {
                after_wrapper = false;
                if let Some(call_seq) = completed_edits.remove(&seq) {
                    if pending
                        .as_ref()
                        .is_some_and(|row| call_seq > row.source.seq)
                    {
                        pending = None;
                        report.observed_edit_threads += 1;
                    }
                }
            } else if kind != "meta" {
                after_wrapper = false;
            }
        }
        if let Some(row) = pending {
            report.threads.push(row);
        }
    }
    Ok(report)
}

fn successful_edit(
    call: &super::record_source::SourceEvent,
    result: &super::record_source::SourceEvent,
) -> bool {
    use serde_json::Value;
    let Some(raw) = result.tool_result.as_deref() else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return false;
    };
    if let Some((code, output)) = super::deferred_record::completed_output(result) {
        return code == 0
            && call.source == "codex"
            && call.source_tool_name.as_deref() == Some("apply_patch")
            && completed_patch_files(call.tool_input.as_deref(), &output);
    }
    // Native Claude Edit/Write echo fields describe a completed tool result.
    // is_error=false alone, an assistant sentence, and a Bash exit do not.
    let Some(native) = value.get("toolUseResult").and_then(Value::as_object) else {
        return false;
    };
    call.source == "claude_code"
        && native
            .get("filePath")
            .or_else(|| native.get("file_path"))
            .and_then(Value::as_str)
            .is_some_and(|path| !path.is_empty())
        && match call.tool_name.as_deref() {
            Some("Edit") => {
                native.get("oldString").and_then(Value::as_str).is_some()
                    && native.get("newString").and_then(Value::as_str).is_some()
            }
            Some("Write") => native.get("content").and_then(Value::as_str).is_some(),
            Some("MultiEdit") => {
                native
                    .get("edits")
                    .and_then(Value::as_array)
                    .is_some_and(|edits| {
                        !edits.is_empty()
                            && edits.iter().all(|edit| {
                                edit.get("oldString").and_then(Value::as_str).is_some()
                                    && edit.get("newString").and_then(Value::as_str).is_some()
                            })
                    })
            }
            _ => false,
        }
}

/// This proves only that the native record describes a completed patch. It
/// does not verify the requested work, current file bytes or correctness.
/// Unrecognized patch/result shapes stay visible as missing completion evidence.
fn completed_patch_files(input: Option<&str>, output: &str) -> bool {
    if output.len() > 1024 * 1024 {
        return false;
    }
    let Some(input) = input else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(input) else {
        return false;
    };
    let Some(patch) = value.get("patch").and_then(serde_json::Value::as_str) else {
        return false;
    };
    let Some(expected) = requested_patch_files(patch) else {
        return false;
    };
    let Some(entries) = output.strip_prefix("Success. Updated the following files:\n") else {
        return false;
    };
    let mut observed = std::collections::BTreeMap::new();
    for entry in entries.lines() {
        let Some((action, path)) = entry.split_once(' ') else {
            return false;
        };
        if !matches!(action, "A" | "M" | "D")
            || !named_patch_path(path)
            || observed.len() >= 1024
            || observed.insert(path.to_owned(), action).is_some()
        {
            return false;
        }
    }
    !observed.is_empty() && observed == expected
}

fn named_patch_path(path: &str) -> bool {
    !path.is_empty() && path.trim() == path && !path.chars().any(char::is_control)
}

fn patch_section(line: &str) -> Option<(&'static str, &str)> {
    [
        ("*** Add File: ", "A"),
        ("*** Update File: ", "M"),
        ("*** Delete File: ", "D"),
    ]
    .into_iter()
    .find_map(|(prefix, action)| line.strip_prefix(prefix).map(|path| (action, path)))
}

fn requested_patch_files(patch: &str) -> Option<std::collections::BTreeMap<String, &'static str>> {
    // A closed request can name a bounded set of exact file actions. In
    // particular, moves and malformed partial sections are not guessed from
    // the success header or the display-oriented HQ patch parser.
    if patch.len() > 1024 * 1024 {
        return None;
    }
    let mut lines = patch.lines().peekable();
    if lines.next()? != "*** Begin Patch" {
        return None;
    }
    let mut files = std::collections::BTreeMap::new();
    while let Some(header) = lines.next() {
        if header == "*** End Patch" {
            return (lines.next().is_none() && !files.is_empty()).then_some(files);
        }
        let (action, path) = patch_section(header)?;
        if !named_patch_path(path)
            || files.len() >= 1024
            || files.insert(path.to_owned(), action).is_some()
        {
            return None;
        }
        let mut hunk = false;
        let mut changed = false;
        let mut end_of_file = false;
        while let Some(&line) = lines.peek() {
            if line == "*** End Patch" || patch_section(line).is_some() {
                break;
            }
            let _ = lines.next();
            if end_of_file {
                return None;
            }
            match action {
                "A" if line.starts_with('+') => {}
                "M" if line == "@@" || line.starts_with("@@ ") => hunk = true,
                "M" if hunk && (line.starts_with('+') || line.starts_with('-')) => {
                    changed = true;
                }
                "M" if hunk && line.starts_with(' ') => {}
                "M" if hunk && changed && line == "*** End of File" => end_of_file = true,
                _ => return None,
            }
        }
        if action == "M" && !(hunk && changed) {
            return None;
        }
    }
    None
}
