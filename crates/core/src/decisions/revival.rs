//! Source-verified attribution of an exact held-to-broken absence transition.
use super::{
    checks::{check_at, VerdictState},
    conflict::{ancestor, blob_at, exact_version, git_output, valid_oid},
    params::{parse_params, CheckParams},
    record_source::{self, SourceEvent},
    storage,
};
use crate::{error::Result, ledger::Ledger, redact::Masked};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevivalState {
    ReAdded,
    Unverifiable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceWitness {
    pub event_id: String,
    pub raw_file: String,
    pub raw_line: u64,
    pub raw_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revival {
    pub decision_id: String,
    pub revision: u32,
    pub state: RevivalState,
    pub held_version: Option<String>,
    pub broken_version: String,
    pub introducing_version: Option<String>,
    pub path: String,
    pub line: u64,
    pub term: String,
    pub session_id: Option<String>,
    pub edit_event_id: Option<String>,
    pub result_event_id: Option<String>,
    pub version_event_id: Option<String>,
    pub witnesses: Vec<SourceWitness>,
    pub reason: Option<String>,
    pub checkable_if: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevivalReport {
    pub schema_version: u32,
    pub project_id: String,
    pub version_id: String,
    pub tree_hash: Option<String>,
    pub revivals: Vec<Revival>,
    pub missing_evidence: Vec<String>,
}

pub fn project_revivals(
    ledger: &Ledger,
    project_id: &str,
    repo: &Path,
    version_id: &str,
) -> Result<RevivalReport> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut report = RevivalReport {
        schema_version: 1,
        project_id: project_id.into(),
        version_id: version_id.into(),
        tree_hash: None,
        revivals: vec![],
        missing_evidence: vec![],
    };
    let root: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT root_path FROM projects WHERE id=?1",
            [project_id],
            |r| r.get(0),
        )
        .optional()?;
    if !root
        .as_deref()
        .and_then(|p| Path::new(p).canonicalize().ok())
        .zip(repo.canonicalize().ok())
        .is_some_and(|(a, b)| a == b)
    {
        report
            .missing_evidence
            .push("recorded repository identity is unavailable".into());
        return Ok(report);
    }
    report.tree_hash = exact_version(repo, version_id, deadline);
    let Some(tree) = report.tree_hash.clone() else {
        report
            .missing_evidence
            .push("exact target version cannot be read".into());
        return Ok(report);
    };
    let mut census = ledger
        .conn()
        .prepare("SELECT id FROM decisions WHERE project_id=?1 ORDER BY id LIMIT 257")?;
    let ids = census
        .query_map([project_id], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if ids.len() > 256 {
        report
            .missing_evidence
            .push("revival census exceeds 256 decisions".into());
    }
    for id in ids.into_iter().take(256) {
        if Instant::now() >= deadline {
            report
                .missing_evidence
                .push("revival source budget exhausted".into());
            break;
        }
        if storage::verified_retirement_for_decision_read_only(
            ledger,
            &id,
            deadline.saturating_duration_since(Instant::now()),
        )?
        .is_some()
        {
            continue;
        }
        let Some(accepted) = storage::verified_acceptance_for_decision_read_only(
            ledger,
            &id,
            deadline.saturating_duration_since(Instant::now()),
        )?
        else {
            let attempted: bool = ledger.conn().query_row(
                "SELECT EXISTS(SELECT 1 FROM decision_acceptances WHERE decision_id=?1)",
                [&id],
                |r| r.get(0),
            )?;
            if attempted {
                report.missing_evidence.push(format!(
                    "person acceptance evidence is unavailable for {id}"
                ));
            }
            continue;
        };
        let raw: String = ledger.conn().query_row(
            "SELECT params_json FROM decision_revisions WHERE decision_id=?1 AND rev=?2",
            params![accepted.id, accepted.revision],
            |r| r.get(0),
        )?;
        let check = parse_params(&accepted.kind, accepted.params_schema_version, &raw)?;
        let terms = match &check {
            CheckParams::TextAbsent(p) => p
                .terms
                .iter()
                .chain(&p.synonyms)
                .cloned()
                .collect::<Vec<_>>(),
            CheckParams::SymbolAbsent(p) => vec![p.symbol.clone()],
            CheckParams::UiCopyAbsent(p) => p.terms.clone(),
            _ => continue,
        };
        let broken:Option<(i64,String)>=ledger.conn().query_row("SELECT rowid,tree_hash FROM decision_verdicts WHERE decision_id=?1 AND rev=?2 AND version_id=?3 AND verdict='broken' ORDER BY rowid DESC LIMIT 1",params![accepted.id,accepted.revision,version_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        let Some((broken_order, stored_tree)) = broken else {
            continue;
        };
        let fresh = check_at(repo, version_id, &check);
        if fresh.state != VerdictState::Broken || fresh.findings.is_empty() {
            report.missing_evidence.push(format!(
                "recorded broken result for {} is not freshly reproduced",
                accepted.id
            ));
            continue;
        }
        let held = prior_hold(
            ledger,
            repo,
            &accepted.id,
            accepted.revision,
            broken_order,
            version_id,
            &check,
            deadline,
        )?;
        let calls = edit_ids(ledger, project_id)?;
        for finding in fresh.findings {
            let Some(term) = terms
                .iter()
                .find(|term| contains_term(&finding.span, term))
                .cloned()
                .or_else(|| terms.first().cloned())
            else {
                report
                    .missing_evidence
                    .push("accepted absence decision has no checkable term".into());
                continue;
            };
            let mut row = Revival {
                decision_id: accepted.id.clone(),
                revision: accepted.revision,
                state: RevivalState::Unverifiable,
                held_version: held.clone(),
                broken_version: version_id.into(),
                introducing_version: None,
                path: Masked::mask(&finding.path).into_text(),
                line: finding.line,
                term: Masked::mask(&term).into_text(),
                session_id: None,
                edit_event_id: None,
                result_event_id: None,
                version_event_id: None,
                witnesses: vec![],
                reason: Some(
                    "no unique source-verified introducing edit and version witness".into(),
                ),
                checkable_if: Some(
                    "provide readable edit/result sources, exact blob bytes and version ancestry"
                        .into(),
                ),
            };
            if stored_tree != tree
                || held.is_none()
                || finding.line == 0
                || fresh.reason.is_some()
                || Instant::now() >= deadline
            {
                row.reason=Some("earlier exact hold, current tree, line or accepted exclusion evidence is unavailable".into());
                report.revivals.push(row);
                continue;
            }
            let Some(held_version) = held.as_deref() else {
                report.revivals.push(row);
                continue;
            };
            let mut proofs = vec![];
            let mut rejected_native = None;
            for id in &calls {
                if Instant::now() >= deadline {
                    break;
                }
                if let Some(proof) = introducing_edit(
                    ledger,
                    project_id,
                    repo,
                    held_version,
                    version_id,
                    &finding.path,
                    finding.line,
                    &term,
                    id,
                    deadline,
                    &mut rejected_native,
                )? {
                    proofs.push(proof);
                }
            }
            if proofs.len() == 1 && Instant::now() < deadline {
                if let Some((call, result, version_call, version_result, introducing)) =
                    proofs.pop()
                {
                    row.state = RevivalState::ReAdded;
                    row.introducing_version = Some(introducing);
                    row.session_id = Some(call.session_id.clone());
                    row.edit_event_id = Some(call.id.clone());
                    row.result_event_id = Some(result.id.clone());
                    row.version_event_id = Some(version_result.id.clone());
                    row.witnesses = [call, result, version_call, version_result]
                        .iter()
                        .map(witness)
                        .collect();
                    row.reason = None;
                    row.checkable_if = None;
                }
            } else if proofs.len() > 1 {
                row.reason =
                    Some("more than one verified session matches the same introduction".into());
            } else if let Some(reason) = rejected_native {
                row.reason = Some(reason.into());
            }
            report.revivals.push(row);
        }
    }
    Ok(report)
}

#[allow(clippy::too_many_arguments)]
fn prior_hold(
    ledger: &Ledger,
    repo: &Path,
    id: &str,
    revision: u32,
    before: i64,
    target: &str,
    check: &CheckParams,
    deadline: Instant,
) -> Result<Option<String>> {
    let mut q=ledger.conn().prepare("SELECT version_id,tree_hash FROM decision_verdicts WHERE decision_id=?1 AND rev=?2 AND verdict='holds' AND rowid<?3 ORDER BY rowid DESC LIMIT 128")?;
    for row in q.query_map(params![id, revision, before], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })? {
        if Instant::now() >= deadline {
            break;
        }
        let (version, tree) = row?;
        if version != target
            && exact_version(repo, &version, deadline).as_deref() == Some(tree.as_str())
            && ancestor(repo, &version, target, deadline)
            && check_at(repo, &version, check).state == VerdictState::Holds
        {
            return Ok(Some(version));
        }
    }
    Ok(None)
}
fn edit_ids(ledger: &Ledger, project: &str) -> Result<Vec<String>> {
    let mut q=ledger.conn().prepare("SELECT id FROM events WHERE project_id=?1 AND kind='tool_use' AND (tool_name IN ('Edit','Write') OR source_tool_name IN ('Edit','Write')) ORDER BY session_id,seq LIMIT 257")?;
    let ids = q
        .query_map([project], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    // An incomplete census cannot establish unique attribution.
    Ok(if ids.len() > 256 { vec![] } else { ids })
}
type EditProof = (SourceEvent, SourceEvent, SourceEvent, SourceEvent, String);
#[allow(clippy::too_many_arguments)]
fn introducing_edit(
    ledger: &Ledger,
    project: &str,
    repo: &Path,
    held: &str,
    target: &str,
    path: &str,
    line: u64,
    term: &str,
    id: &str,
    deadline: Instant,
    rejected_native: &mut Option<&'static str>,
) -> Result<Option<EditProof>> {
    let Some((call, result)) = record_source::verified_completed_call(ledger, project, id)? else {
        return Ok(None);
    };
    if call.source != "claude_code"
        || call.role.as_deref() != Some("assistant")
        || result.is_error
        || result.is_sidechain
    {
        return Ok(None);
    }
    let Some(input) = call
        .tool_input
        .as_deref()
        .and_then(|v| serde_json::from_str::<Value>(v).ok())
    else {
        return Ok(None);
    };
    let full = repo.join(path).to_string_lossy().into_owned();
    if input.get("file_path").and_then(Value::as_str) != Some(full.as_str()) {
        return Ok(None);
    }
    let Some(envelope) = result
        .tool_result
        .as_deref()
        .and_then(|v| serde_json::from_str::<Value>(v).ok())
    else {
        return Ok(None);
    };
    let tool = call
        .tool_name
        .as_deref()
        .or(call.source_tool_name.as_deref());
    let change = match native_change(tool, &input, envelope.get("toolUseResult"), &full, term) {
        Ok(change) => change,
        Err(reason) => {
            *rejected_native = Some(reason);
            return Ok(None);
        }
    };
    let mut q=ledger.conn().prepare("SELECT id FROM events WHERE project_id=?1 AND session_id=?2 AND seq>?3 AND kind='tool_use' AND (tool_name='Bash' OR source_tool_name='Bash') ORDER BY seq LIMIT 129")?;
    let version_calls = q
        .query_map(params![project, call.session_id, result.seq], |r| {
            r.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if version_calls.len() > 128 {
        return Ok(None);
    }
    let quoted = format!("'{}'", repo.display().to_string().replace('\'', "'\\''"));
    let expected = format!("git -C {quoted} rev-parse HEAD");
    let mut matches = vec![];
    for version_id in version_calls {
        if Instant::now() >= deadline {
            return Ok(None);
        }
        let Some((observe, answer)) =
            record_source::verified_completed_call(ledger, project, &version_id)?
        else {
            continue;
        };
        if answer.is_error {
            continue;
        }
        let Some(input) = observe
            .tool_input
            .as_deref()
            .and_then(|v| serde_json::from_str::<Value>(v).ok())
        else {
            continue;
        };
        if input.get("command").and_then(Value::as_str) != Some(expected.as_str()) {
            continue;
        }
        let Some(output) = answer
            .tool_result
            .as_deref()
            .and_then(|v| serde_json::from_str::<Value>(v).ok())
        else {
            continue;
        };
        let Some(native) = output.get("toolUseResult") else {
            continue;
        };
        let completed = match native.get("exitCode") {
            Some(value) => value.as_i64() == Some(0),
            None => native.get("interrupted").and_then(Value::as_bool) == Some(false),
        };
        if !completed
            || native.get("interrupted").and_then(Value::as_bool) == Some(true)
            || native.get("stderr").and_then(Value::as_str) != Some("")
        {
            continue;
        }
        let Some(introducing) = native
            .get("stdout")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|oid| valid_oid(oid))
        else {
            continue;
        };
        if !ancestor(repo, held, introducing, deadline)
            || !ancestor(repo, introducing, target, deadline)
        {
            continue;
        }
        let Some(parents) = git_output(
            repo,
            &["rev-list", "--parents", "-n", "1", introducing],
            deadline,
        ) else {
            continue;
        };
        let Ok(parents) = std::str::from_utf8(&parents) else {
            continue;
        };
        let parents = parents.split_whitespace().collect::<Vec<_>>();
        if parents.len() != 2
            || parents[0] != introducing
            || !ancestor(repo, held, parents[1], deadline)
        {
            continue;
        }
        let parent = blob_at(repo, parents[1], path, deadline);
        let absent = change.is_creation() && path_absent(repo, parents[1], path, deadline);
        let Some(new) = change.apply(parent.as_deref(), absent, term) else {
            continue;
        };
        if blob_at(repo, introducing, path, deadline).as_deref() != Some(new.as_slice())
            || blob_at(repo, target, path, deadline).as_deref() != Some(new.as_slice())
        {
            continue;
        }
        let Some(new) = std::str::from_utf8(&new).ok() else {
            continue;
        };
        if !new
            .lines()
            .nth(line.saturating_sub(1) as usize)
            .is_some_and(|text| contains_term(text, term))
        {
            continue;
        }
        let intervening:bool=ledger.conn().query_row("SELECT EXISTS(SELECT 1 FROM events WHERE project_id=?1 AND session_id=?2 AND seq>?3 AND seq<?4 AND kind='tool_use' AND (tool_name IN ('Edit','Write') OR source_tool_name IN ('Edit','Write')))",params![project,call.session_id,result.seq,observe.seq],|r|r.get(0))?;
        if intervening {
            continue;
        }
        matches.push((
            call.clone(),
            result.clone(),
            observe,
            answer,
            introducing.into(),
        ));
    }
    Ok(if matches.len() == 1 {
        matches.pop()
    } else {
        None
    })
}

const MAX_EDIT_BLOB: usize = 2 * 1024 * 1024;

/// These are byte-bearing native echoes, not inferred tool success.
enum NativeChange {
    Edit {
        old: String,
        new: String,
        original: String,
        replace_all: bool,
    },
    Write {
        content: String,
        create: bool,
    },
}

fn native_change(
    tool: Option<&str>,
    input: &Value,
    echo: Option<&Value>,
    full: &str,
    term: &str,
) -> std::result::Result<NativeChange, &'static str> {
    match tool {
        Some("Edit") => {
            let reason = "completed native Edit echo needs exact filePath, oldString, newString and originalFile bytes";
            let echo = echo.ok_or(reason)?;
            let old = input
                .get("old_string")
                .and_then(Value::as_str)
                .ok_or(reason)?;
            let new = input
                .get("new_string")
                .and_then(Value::as_str)
                .ok_or(reason)?;
            let original = echo
                .get("originalFile")
                .and_then(Value::as_str)
                .ok_or(reason)?;
            let replace_all = match input.get("replace_all") {
                None => false,
                Some(value) => value
                    .as_bool()
                    .ok_or("native Edit replace_all must be an exact boolean")?,
            };
            if old.is_empty()
                || contains_term(old, term)
                || !contains_term(new, term)
                || echo.get("filePath").and_then(Value::as_str) != Some(full)
                || echo.get("oldString").and_then(Value::as_str) != Some(old)
                || echo.get("newString").and_then(Value::as_str) != Some(new)
                || echo
                    .get("replaceAll")
                    .is_some_and(|v| v.as_bool() != Some(replace_all))
                || original.len() > MAX_EDIT_BLOB
            {
                return Err(reason);
            }
            Ok(NativeChange::Edit {
                old: old.into(),
                new: new.into(),
                original: original.into(),
                replace_all,
            })
        }
        Some("Write") => {
            let reason = "completed native Write echo needs exact filePath/content bytes and type create or update";
            let echo = echo.ok_or(reason)?;
            let content = input.get("content").and_then(Value::as_str).ok_or(reason)?;
            let create = match echo.get("type").and_then(Value::as_str) {
                Some("create") => true,
                Some("update") => false,
                _ => return Err(reason),
            };
            if !contains_term(content, term)
                || content.len() > MAX_EDIT_BLOB
                || echo.get("filePath").and_then(Value::as_str) != Some(full)
                || echo.get("content").and_then(Value::as_str) != Some(content)
            {
                return Err(reason);
            }
            Ok(NativeChange::Write {
                content: content.into(),
                create,
            })
        }
        _ => Err("the recorded native change adapter is unsupported"),
    }
}

impl NativeChange {
    fn is_creation(&self) -> bool {
        matches!(self, Self::Write { create: true, .. })
    }
    fn apply(&self, parent: Option<&[u8]>, absent: bool, term: &str) -> Option<Vec<u8>> {
        match self {
            Self::Edit {
                old,
                new,
                original,
                replace_all,
            } => {
                let parent = std::str::from_utf8(parent?).ok()?;
                if parent.as_bytes() != original.as_bytes() || contains_term(parent, term) {
                    return None;
                }
                // Count overlapping matches too: a single-edit request must
                // not select one of multiple possible substring locations.
                let all = parent
                    .as_bytes()
                    .windows(old.len())
                    .filter(|v| *v == old.as_bytes())
                    .count();
                if all == 0 || (!replace_all && all != 1) {
                    return None;
                }
                let replacements = if *replace_all {
                    parent.match_indices(old).count()
                } else {
                    1
                };
                let size = parent
                    .len()
                    .checked_sub(old.len().checked_mul(replacements)?)?
                    .checked_add(new.len().checked_mul(replacements)?)?;
                if size > MAX_EDIT_BLOB {
                    return None;
                }
                Some(
                    if *replace_all {
                        parent.replace(old, new)
                    } else {
                        parent.replacen(old, new, 1)
                    }
                    .into_bytes(),
                )
            }
            Self::Write { content, create } => {
                if *create {
                    if parent.is_some() || !absent {
                        return None;
                    }
                } else {
                    let parent = std::str::from_utf8(parent?).ok()?;
                    if contains_term(parent, term) {
                        return None;
                    }
                }
                Some(content.as_bytes().to_vec())
            }
        }
    }
}

/// A missing regular blob alone is not absence: it may be a symlink, an
/// unreadable object or a failed Git call. Require an exact empty tree lookup.
fn path_absent(repo: &Path, version: &str, path: &str, deadline: Instant) -> bool {
    if path.is_empty()
        || Path::new(path).is_absolute()
        || path.chars().any(char::is_control)
        || Path::new(path)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return false;
    }
    git_output(
        repo,
        &["ls-tree", "-z", version, "--", &format!(":(literal){path}")],
        deadline,
    )
    .is_some_and(|bytes| bytes.is_empty())
}

fn witness(event: &SourceEvent) -> SourceWitness {
    SourceWitness {
        event_id: event.id.clone(),
        raw_file: Masked::mask(&event.raw_file).into_text(),
        raw_line: u64::try_from(event.raw_line).map_or(0, std::convert::identity),
        raw_sha256: event.raw_sha256.clone(),
    }
}
fn contains_term(text: &str, term: &str) -> bool {
    if term.is_empty() || !term.is_ascii() {
        return false;
    }
    text.char_indices().any(|(start, _)| {
        let end = start.saturating_add(term.len());
        text.get(start..end)
            .is_some_and(|v| v.eq_ignore_ascii_case(term))
            && text
                .get(..start)
                .and_then(|v| v.chars().next_back())
                .is_none_or(|c| !c.is_alphanumeric())
            && text
                .get(end..)
                .and_then(|v| v.chars().next())
                .is_none_or(|c| !c.is_alphanumeric())
    })
}
