//! Bounded re-verification of one recorded source event.
//! The ledger labels are accepted only when raw bytes re-normalize to them.

use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::time::{Duration, Instant};

use rusqlite::{params, OptionalExtension};
use serde_json::Value;

use crate::adapter::stable_source_session_id;
use crate::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use crate::codex::{normalize_codex_line, CodexEmissionRole, CodexLineContext};
use crate::error::Result;
use crate::ledger::Ledger;
use crate::model::NormalizedEvent;
use crate::model::Source;
use crate::parse::{parse_reader, ParseStats};

const MAX_RAW_BYTES: usize = 1_000_000;
const MAX_READ_BYTES: usize = 64 * 1024 * 1024;
const MAX_CHAIN_ROWS: usize = 100_000;

#[derive(Debug, Clone)]
pub(crate) struct SourceEvent {
    pub id: String,
    pub project_id: String,
    pub session_id: String,
    pub seq: i64,
    pub kind: String,
    pub role: Option<String>,
    pub text: Option<String>,
    pub tool_name: Option<String>,
    pub source_tool_name: Option<String>,
    pub tool_input: Option<String>,
    pub tool_result: Option<String>,
    pub is_error: bool,
    pub is_sidechain: bool,
    pub tool_use_id: Option<String>,
    pub source_turn_boundary_line: Option<i64>,
    pub source_turn_boundary_sha256: Option<String>,
    pub raw_file: String,
    pub raw_line: i64,
    pub raw_sha256: String,
    pub chain_hash: String,
    pub source: String,
    pub source_turn_id: Option<String>,
    line_session_id: Option<String>,
}

/// Verify source bytes, normalized content, and the exact session-chain prefix.
/// A bound or read failure is absence of verified evidence.
pub(crate) fn verified_event(
    ledger: &Ledger,
    project_id: &str,
    event_id: &str,
) -> Result<Option<SourceEvent>> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let event = ledger
        .conn()
        .query_row(
            "SELECT e.id,e.project_id,e.session_id,e.seq,e.kind,e.role,e.text,
                e.tool_name,e.source_tool_name,e.tool_input,e.raw_file,e.raw_line,
                e.raw_sha256,e.chain_hash,p.source,e.source_turn_id,e.line_session_id,
                e.tool_use_id,e.source_turn_boundary_line,e.source_turn_boundary_sha256,
                e.tool_result,e.is_error,e.is_sidechain
         FROM events e JOIN sessions s ON s.id=e.session_id AND s.project_id=e.project_id
         JOIN projects p ON p.id=e.project_id WHERE e.id=?1 AND e.project_id=?2",
            params![event_id, project_id],
            |row| {
                Ok(SourceEvent {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    session_id: row.get(2)?,
                    seq: row.get(3)?,
                    kind: row.get(4)?,
                    role: row.get(5)?,
                    text: row.get(6)?,
                    tool_name: row.get(7)?,
                    source_tool_name: row.get(8)?,
                    tool_input: row.get(9)?,
                    raw_file: row.get(10)?,
                    raw_line: row.get(11)?,
                    raw_sha256: row.get(12)?,
                    chain_hash: row.get(13)?,
                    source: row.get(14)?,
                    source_turn_id: row.get(15)?,
                    line_session_id: row.get(16)?,
                    tool_use_id: row.get(17)?,
                    source_turn_boundary_line: row.get(18)?,
                    source_turn_boundary_sha256: row.get(19)?,
                    tool_result: row.get(20)?,
                    is_error: row.get(21)?,
                    is_sidechain: row.get(22)?,
                })
            },
        )
        .optional()?;
    let Some(event) = event else {
        return Ok(None);
    };
    if event.project_id != project_id
        || event.seq <= 0
        || event.raw_line <= 0
        || event
            .source_turn_boundary_line
            .is_some_and(|line| line <= 0)
        || event.seq as usize > MAX_CHAIN_ROWS
    {
        return Ok(None);
    }
    let Some(prefix) =
        read_raw_prefix(Path::new(&event.raw_file), event.raw_line as u64, deadline)?
    else {
        return Ok(None);
    };
    let Some(raw) = prefix.last() else {
        return Ok(None);
    };
    if sha256_hex(raw) != event.raw_sha256 || !normalizes_to(ledger, &event, &prefix)? {
        return Ok(None);
    }
    let mut statement = ledger.conn().prepare(
        "SELECT seq,raw_sha256,prev_hash,chain_hash FROM events
         WHERE session_id=?1 AND project_id=?2 AND seq<=?3 ORDER BY seq",
    )?;
    let rows = statement.query_map(params![event.session_id, project_id, event.seq], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    let mut previous = genesis_prev_hash(project_id);
    let mut found = false;
    for (expected, row) in (1i64..).zip(rows) {
        if Instant::now() >= deadline {
            return Ok(None);
        }
        let (seq, raw_hash, prev_hash, stored_hash) = row?;
        if seq != expected
            || prev_hash != previous
            || stored_hash != chain_hash(&previous, &raw_hash, &event.session_id, seq)
        {
            return Ok(None);
        }
        if seq == event.seq {
            found = raw_hash == event.raw_sha256 && stored_hash == event.chain_hash;
        }
        previous = stored_hash;
    }
    Ok(found.then_some(event))
}

fn same_json(left: Option<&str>, right: Option<&str>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => match (
            serde_json::from_str::<Value>(left),
            serde_json::from_str::<Value>(right),
        ) {
            (Ok(left), Ok(right)) => left == right,
            _ => false,
        },
        _ => false,
    }
}

fn normalized_matches(event: &SourceEvent, candidate: &NormalizedEvent) -> bool {
    candidate.kind.as_str() == event.kind
        && candidate.role == event.role
        && candidate.text == event.text
        && candidate.tool_name == event.tool_name
        && candidate.source_tool_name == event.source_tool_name
        && same_json(candidate.tool_input.as_deref(), event.tool_input.as_deref())
        && same_json(
            candidate.tool_result.as_deref(),
            event.tool_result.as_deref(),
        )
        && candidate.is_error == event.is_error
        && candidate.is_sidechain == event.is_sidechain
        && candidate.tool_use_id == event.tool_use_id
        && candidate.source_turn_id == event.source_turn_id
        && candidate
            .source_turn_boundary_line
            .and_then(|line| i64::try_from(line).ok())
            == event.source_turn_boundary_line
        && candidate.source_turn_boundary_sha256 == event.source_turn_boundary_sha256
        && candidate.line_session_id == event.line_session_id
}

/// A completed native call is a unique request/result pair in one source
/// session. This proves the result's origin and order, not its success.
pub(crate) fn verified_completed_call(
    ledger: &Ledger,
    project_id: &str,
    event_id: &str,
) -> Result<Option<(SourceEvent, SourceEvent)>> {
    let Some(call) = verified_event(ledger, project_id, event_id)? else {
        return Ok(None);
    };
    let Some(tool_id) = call.tool_use_id.as_deref().filter(|id| !id.is_empty()) else {
        return Ok(None);
    };
    if call.kind != "tool_use" || call.is_sidechain {
        return Ok(None);
    }
    let mut statement = ledger.conn().prepare(
        "SELECT id,kind FROM events WHERE project_id=?1 AND session_id=?2
         AND tool_use_id=?3 AND kind IN ('tool_use','tool_result') ORDER BY seq LIMIT 4",
    )?;
    let rows = statement
        .query_map(params![project_id, call.session_id, tool_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() != 2
        || rows[0] != (call.id.clone(), "tool_use".into())
        || rows[1].1 != "tool_result"
    {
        return Ok(None);
    }
    let Some(result) = verified_event(ledger, project_id, &rows[1].0)? else {
        return Ok(None);
    };
    if result.seq <= call.seq
        || result.is_sidechain
        || result.tool_result.is_none()
        || result.source_turn_id != call.source_turn_id
        || result.raw_file != call.raw_file
        || result.raw_line <= call.raw_line
    {
        return Ok(None);
    }
    Ok(Some((call, result)))
}

fn normalizes_to(ledger: &Ledger, event: &SourceEvent, prefix: &[Vec<u8>]) -> Result<bool> {
    let Some(raw) = prefix.last() else {
        return Ok(false);
    };
    match event.source.as_str() {
        "claude_code" => {
            let mut stats = ParseStats::default();
            let Ok(lines) = parse_reader(raw.as_slice(), event.raw_file.clone(), &mut stats) else {
                return Ok(false);
            };
            Ok(lines.len() == 1
                && lines[0].sha256 == event.raw_sha256
                && lines[0].session_id.as_deref()
                    == event
                        .line_session_id
                        .as_deref()
                        .or(Some(event.session_id.as_str()))
                && lines[0]
                    .events
                    .iter()
                    .filter(|candidate| normalized_matches(event, candidate))
                    .count()
                    == 1)
        }
        "codex" => {
            let owner: Option<(String, String)> = ledger
                .conn()
                .query_row(
                    "SELECT owner_key,source_session_key FROM source_sessions
                 WHERE session_id=?1 AND project_id=?2 AND source='codex'",
                    params![event.session_id, event.project_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((owner_key, source_session_key)) = owner else {
                return Ok(false);
            };
            if stable_source_session_id(&owner_key, Source::Codex, &source_session_key)
                != event.session_id
                || event.line_session_id.as_deref() != Some(source_session_key.as_str())
            {
                return Ok(false);
            }
            let mut first_meta: Option<(u64, String)> = None;
            let mut turn_scope = "pre-turn".to_owned();
            let mut turn_boundary_line = None;
            let mut turn_boundary_sha256: Option<String> = None;
            for (part, bytes) in prefix.iter().enumerate() {
                let raw_line = part as u64 + 1;
                let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
                    continue;
                };
                match value.get("type").and_then(Value::as_str) {
                    Some("session_meta") if first_meta.is_none() => {
                        if value
                            .get("payload")
                            .and_then(|payload| payload.get("id"))
                            .and_then(Value::as_str)
                            != Some(source_session_key.as_str())
                        {
                            return Ok(false);
                        }
                        first_meta = Some((raw_line, sha256_hex(bytes)));
                    }
                    Some("turn_context") => {
                        let recognized = value
                            .get("payload")
                            .and_then(|payload| payload.get("turn_id"))
                            .and_then(Value::as_str)
                            .filter(|turn| !turn.trim().is_empty());
                        turn_scope = recognized.map_or_else(
                            || format!("unidentified-turn-at-{raw_line}"),
                            |turn| format!("turn:{turn}"),
                        );
                        turn_boundary_line = Some(raw_line);
                        turn_boundary_sha256 = Some(sha256_hex(bytes));
                    }
                    _ => {}
                }
            }
            let Some((meta_line, meta_hash)) = first_meta else {
                return Ok(false);
            };
            if !frozen_context_line(ledger, event, meta_line, &meta_hash)? {
                return Ok(false);
            }
            if let (Some(line), Some(hash)) = (turn_boundary_line, turn_boundary_sha256.as_deref())
            {
                if !frozen_context_line(ledger, event, line, hash)? {
                    return Ok(false);
                }
            }
            let Ok(value) = serde_json::from_slice::<Value>(raw) else {
                return Ok(false);
            };
            let context = CodexLineContext {
                owner_session_id: &event.session_id,
                source_session_id: &source_session_key,
                file_id: &event.session_id,
                turn_scope: &turn_scope,
                turn_boundary_line,
                turn_boundary_sha256: turn_boundary_sha256.as_deref(),
                line: event.raw_line as u64,
                raw_sha256: &event.raw_sha256,
            };
            let Ok(mapped) = normalize_codex_line(context, &value) else {
                return Ok(false);
            };
            Ok(mapped.envelope_type == "response_item"
                && mapped
                    .events
                    .iter()
                    .filter(|candidate| {
                        matches!(&candidate.emission, CodexEmissionRole::CanonicalItem { .. })
                            && normalized_matches(event, &candidate.event)
                    })
                    .count()
                    == 1)
        }
        _ => Ok(false),
    }
}

/// Source context must itself have an immutable hash anchor. A preceding
/// accepted event or the target's accepted generation disposition supplies it.
fn frozen_context_line(
    ledger: &Ledger,
    event: &SourceEvent,
    line: u64,
    hash: &str,
) -> Result<bool> {
    let Ok(line) = i64::try_from(line) else {
        return Ok(false);
    };
    let present: bool = ledger.conn().query_row(
        "SELECT EXISTS(SELECT 1 FROM events WHERE session_id=?1 AND project_id=?2
                       AND seq<=?3 AND raw_line=?4 AND raw_sha256=?5)
             OR EXISTS(SELECT 1 FROM event_generation_provenance p
                       JOIN generation_raw_dispositions d ON d.generation_id=p.generation_id
                       JOIN session_generations g ON g.id=p.generation_id
                       WHERE p.event_id=?6 AND g.session_id=?1 AND g.project_id=?2
                         AND d.raw_line=?4 AND d.raw_sha256=?5)",
        params![
            event.session_id,
            event.project_id,
            event.seq,
            line,
            hash,
            event.id
        ],
        |row| row.get(0),
    )?;
    Ok(present)
}

fn read_raw_prefix(path: &Path, want: u64, deadline: Instant) -> Result<Option<Vec<Vec<u8>>>> {
    if Instant::now() >= deadline {
        return Ok(None);
    }
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return Ok(None);
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Ok(None);
    }
    #[cfg(all(test, unix))]
    read_tests::before_open();
    #[cfg(unix)]
    let file: File = {
        let Ok(descriptor) = rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        ) else {
            return Ok(None);
        };
        let Ok(stat) = rustix::fs::fstat(&descriptor) else {
            return Ok(None);
        };
        if !rustix::fs::FileType::from_raw_mode(stat.st_mode).is_file() {
            return Ok(None);
        }
        descriptor.into()
    };
    #[cfg(not(unix))]
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return Ok(None),
    };
    let Ok(opened) = file.metadata() else {
        return Ok(None);
    };
    if !same_source_metadata(&metadata, &opened) {
        return Ok(None);
    }
    #[cfg(all(test, unix))]
    read_tests::after_open();
    // Bound the descriptor's bytes to its pinned initial size, preserving
    // prefixes of large stable sessions without following concurrent growth.
    let read_limit = opened.len().min(MAX_READ_BYTES as u64 + 1);
    let mut reader = BufReader::new(file.take(read_limit));
    let mut seen_bytes = 0usize;
    let mut current_line = 1u64;
    let mut current = Vec::new();
    let mut prefix = Vec::new();
    loop {
        if Instant::now() >= deadline {
            return Ok(None);
        }
        let Ok(buffer) = reader.fill_buf() else {
            return Ok(None);
        };
        if buffer.is_empty() {
            if current_line == want && !current.is_empty() {
                prefix.push(current);
                return Ok(pinned_source_prefix(
                    path,
                    reader.get_ref().get_ref(),
                    &opened,
                    deadline,
                    prefix,
                ));
            }
            return Ok(None);
        }
        let end = buffer
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|at| at + 1)
            .unwrap_or(buffer.len());
        seen_bytes = seen_bytes.saturating_add(end);
        if seen_bytes > MAX_READ_BYTES || current.len().saturating_add(end) > MAX_RAW_BYTES + 2 {
            return Ok(None);
        }
        let line_ended = buffer[end - 1] == b'\n';
        current.extend_from_slice(&buffer[..end]);
        reader.consume(end);
        if line_ended {
            current.pop();
            if current.ends_with(b"\r") {
                current.pop();
            }
            prefix.push(std::mem::take(&mut current));
            if current_line == want {
                return Ok(pinned_source_prefix(
                    path,
                    reader.get_ref().get_ref(),
                    &opened,
                    deadline,
                    prefix,
                ));
            }
            current_line += 1;
        }
    }
}

fn same_source_metadata(first: &std::fs::Metadata, second: &std::fs::Metadata) -> bool {
    if !first.is_file()
        || !second.is_file()
        || first.file_type().is_symlink()
        || second.file_type().is_symlink()
        || first.len() != second.len()
        || first.modified().ok() != second.modified().ok()
    {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        first.dev() == second.dev()
            && first.ino() == second.ino()
            && first.ctime() == second.ctime()
            && first.ctime_nsec() == second.ctime_nsec()
    }
    #[cfg(not(unix))]
    {
        first.created().ok() == second.created().ok()
    }
}

fn pinned_source_prefix(
    path: &Path,
    file: &File,
    opened: &std::fs::Metadata,
    deadline: Instant,
    prefix: Vec<Vec<u8>>,
) -> Option<Vec<Vec<u8>>> {
    let after = file.metadata().ok()?;
    let named = std::fs::symlink_metadata(path).ok()?;
    if Instant::now() >= deadline
        || !same_source_metadata(opened, &after)
        || !same_source_metadata(opened, &named)
    {
        return None;
    }
    Some(prefix)
}

#[cfg(all(test, unix))]
#[path = "record_source_read_tests.rs"]
mod read_tests;
