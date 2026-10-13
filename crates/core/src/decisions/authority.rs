//! Recorded person authority for accepting, retiring, and waiving decisions.
//!
//! A caller-supplied actor label is never evidence. An authority receipt binds
//! one immutable act row to an exact confirmation in a recorded user message,
//! its raw transcript line, and its session chain. Until an external host can
//! attest user presence, this proves consistency with the local record, not a
//! cryptographic identity of the person at the keyboard.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::{Duration, Instant};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use ulid::Ulid;

use crate::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use crate::codex::{normalize_codex_line, CodexEmissionRole, CodexLineContext};
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::model::EventKind;
use crate::parse::{parse_reader, ParseStats};

#[path = "bridge.rs"]
pub mod bridge;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersonActKind {
    Accept,
    Retire,
    Waive,
}

impl PersonActKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Retire => "retire",
            Self::Waive => "waive",
        }
    }
}

struct Target {
    project_id: String,
    decision_id: String,
    confirmation: Value,
}

struct EventProof {
    event_id: String,
    session_id: String,
    raw_file: String,
    raw_line: i64,
    raw_sha256: String,
    chain_hash: String,
}

struct RawMessage<'a> {
    raw_file: &'a str,
    raw_line: u64,
    raw_sha256: &'a str,
    session_id: &'a str,
    text: &'a str,
}

struct ReceiptRow {
    decision_id: String,
    project_id: String,
    event_id: String,
    session_id: String,
    raw_file: String,
    raw_line: i64,
    raw_sha256: String,
    chain_hash: String,
    tuple_sha256: String,
}

struct AcceptanceRow {
    project_id: String,
    decision_id: String,
    revision: u32,
    params_json: String,
    severity: String,
    version: String,
    actor: String,
    because: String,
}

struct WaiverRow {
    project_id: String,
    decision_id: String,
    version_id: String,
    actor: String,
    because: String,
    expires_version: Option<String>,
    expires_at: Option<String>,
}

struct RecordedEvent {
    project_id: String,
    session_id: String,
    line_session_id: Option<String>,
    source: String,
    seq: i64,
    kind: String,
    text: Option<String>,
    raw_file: String,
    raw_line: i64,
    raw_sha256: String,
    prev_hash: String,
    chain_hash: String,
    is_sidechain: i64,
}

/// Bind an already appended act to one exact, recorded user confirmation.
/// The act remains inert for gates until this receipt verifies on read.
pub fn record(
    ledger: &Ledger,
    action: PersonActKind,
    act_row_id: &str,
    event_id: &str,
) -> Result<String> {
    let target = target(ledger.conn(), action, act_row_id)?;
    let existing: Option<(String, String)> = ledger
        .conn()
        .query_row(
            "SELECT id,event_id FROM decision_person_acts WHERE action=?1 AND act_row_id=?2",
            params![action.as_str(), act_row_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((id, recorded_event_id)) = existing {
        if recorded_event_id == event_id && verified(ledger, action, act_row_id)? {
            return Ok(id);
        }
        return Err(CoreError::Config(
            "person action already has a different or invalid confirmation".into(),
        ));
    }
    let proof = event_proof(
        ledger.conn(),
        &target.project_id,
        event_id,
        Some(&target.confirmation),
        None,
    )?
    .ok_or_else(|| CoreError::Config("person action lacks an exact recorded user source".into()))?;
    let tuple_sha256 = sha256_hex(&serde_json::to_vec(&target.confirmation)?);
    let id = Ulid::generate().to_string();
    ledger.conn().execute(
        "INSERT INTO decision_person_acts(
            id,action,act_row_id,decision_id,project_id,event_id,session_id,
            raw_file,raw_line,raw_sha256,chain_hash,tuple_sha256)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        params![
            id,
            action.as_str(),
            act_row_id,
            target.decision_id,
            target.project_id,
            proof.event_id,
            proof.session_id,
            proof.raw_file,
            proof.raw_line,
            proof.raw_sha256,
            proof.chain_hash,
            tuple_sha256
        ],
    )?;
    Ok(id)
}

/// A gate must call this on the exact latest act row it proposes to honor.
/// Missing, changed, or unreadable evidence returns false.
pub fn verified(ledger: &Ledger, action: PersonActKind, act_row_id: &str) -> Result<bool> {
    verified_on_connection(ledger.conn(), action, act_row_id, None)
}

/// Read-only verification with a strict time budget for advisory cache reads.
/// A budget overrun is silence, never a presumed valid receipt.
pub fn verified_read_only(
    conn: &Connection,
    action: PersonActKind,
    act_row_id: &str,
    budget: Duration,
) -> Result<bool> {
    verified_on_connection(conn, action, act_row_id, Some(Instant::now() + budget))
}

fn verified_on_connection(
    conn: &Connection,
    action: PersonActKind,
    act_row_id: &str,
    deadline: Option<Instant>,
) -> Result<bool> {
    let target = target(conn, action, act_row_id)?;
    let receipt: Option<ReceiptRow> = conn
        .query_row(
            "SELECT decision_id,project_id,event_id,session_id,raw_file,raw_line,
                    raw_sha256,chain_hash,tuple_sha256
             FROM decision_person_acts WHERE action=?1 AND act_row_id=?2",
            params![action.as_str(), act_row_id],
            |row| {
                Ok(ReceiptRow {
                    decision_id: row.get(0)?,
                    project_id: row.get(1)?,
                    event_id: row.get(2)?,
                    session_id: row.get(3)?,
                    raw_file: row.get(4)?,
                    raw_line: row.get(5)?,
                    raw_sha256: row.get(6)?,
                    chain_hash: row.get(7)?,
                    tuple_sha256: row.get(8)?,
                })
            },
        )
        .optional()?;
    let Some(receipt) = receipt else {
        return Ok(false);
    };
    if receipt.decision_id != target.decision_id
        || receipt.project_id != target.project_id
        || receipt.tuple_sha256 != sha256_hex(&serde_json::to_vec(&target.confirmation)?)
    {
        return Ok(false);
    }
    let Some(proof) = event_proof(
        conn,
        &receipt.project_id,
        &receipt.event_id,
        Some(&target.confirmation),
        deadline,
    )?
    else {
        return Ok(false);
    };
    Ok(proof.session_id == receipt.session_id
        && proof.raw_file == receipt.raw_file
        && proof.raw_line == receipt.raw_line
        && proof.raw_sha256 == receipt.raw_sha256
        && proof.chain_hash == receipt.chain_hash)
}

/// Verify the source of a proposal without requiring an action confirmation.
/// Only a normal, recorded user message with matching raw bytes and chain
/// qualifies as a person's source.
pub fn verified_user_event(ledger: &Ledger, event_id: &str) -> Result<bool> {
    let project_id: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT project_id FROM events WHERE id=?1",
            [event_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(project_id) = project_id else {
        return Ok(false);
    };
    Ok(event_proof(ledger.conn(), &project_id, event_id, None, None)?.is_some())
}

fn target(conn: &Connection, action: PersonActKind, row_id: &str) -> Result<Target> {
    match action {
        PersonActKind::Accept => {
            let found: Option<AcceptanceRow> = conn
                .query_row(
                    "SELECT d.project_id,a.decision_id,a.rev,r.params_json,a.severity,
                            a.applies_from_version,a.actor,a.rationale
                     FROM decision_acceptances a
                     JOIN decisions d ON d.id=a.decision_id
                     JOIN decision_revisions r ON r.decision_id=a.decision_id AND r.rev=a.rev
                     WHERE a.id=?1",
                    [row_id],
                    |row| {
                        Ok(AcceptanceRow {
                            project_id: row.get(0)?,
                            decision_id: row.get(1)?,
                            revision: row.get(2)?,
                            params_json: row.get(3)?,
                            severity: row.get(4)?,
                            version: row.get(5)?,
                            actor: row.get(6)?,
                            because: row.get(7)?,
                        })
                    },
                )
                .optional()?;
            let Some(found) = found else {
                return Err(CoreError::Config(
                    "decision acceptance does not exist".into(),
                ));
            };
            Ok(Target {
                project_id: found.project_id,
                decision_id: found.decision_id.clone(),
                confirmation: json!({"thruline_decision_act":{
                    "schema_version":1,"action":"accept","decision_id":found.decision_id,
                    "revision":found.revision,"params_sha256":sha256_hex(found.params_json.as_bytes()),
                    "severity":found.severity,"applies_from_version":found.version,
                    "actor":found.actor,"because":found.because,
                }}),
            })
        }
        PersonActKind::Retire => {
            let found: Option<(String, String, String, String, Option<String>)> = conn
                .query_row(
                    "SELECT d.project_id,r.decision_id,r.actor,r.reason,r.superseded_by
                     FROM decision_retirements r JOIN decisions d ON d.id=r.decision_id
                     WHERE r.id=?1",
                    [row_id],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    },
                )
                .optional()?;
            let Some((project_id, decision_id, actor, because, superseded_by)) = found else {
                return Err(CoreError::Config(
                    "decision retirement does not exist".into(),
                ));
            };
            Ok(Target {
                project_id,
                decision_id: decision_id.clone(),
                confirmation: json!({"thruline_decision_act":{
                    "schema_version":1,"action":"retire","decision_id":decision_id,
                    "actor":actor,"because":because,"superseded_by":superseded_by,
                }}),
            })
        }
        PersonActKind::Waive => {
            let found: Option<WaiverRow> = conn
                .query_row(
                    "SELECT d.project_id,w.decision_id,w.version_id,w.actor,w.reason,
                            w.expires_version,w.expires_at
                     FROM decision_waivers w JOIN decisions d ON d.id=w.decision_id
                     WHERE w.id=?1",
                    [row_id],
                    |row| {
                        Ok(WaiverRow {
                            project_id: row.get(0)?,
                            decision_id: row.get(1)?,
                            version_id: row.get(2)?,
                            actor: row.get(3)?,
                            because: row.get(4)?,
                            expires_version: row.get(5)?,
                            expires_at: row.get(6)?,
                        })
                    },
                )
                .optional()?;
            let Some(found) = found else {
                return Err(CoreError::Config("decision waiver does not exist".into()));
            };
            Ok(Target {
                project_id: found.project_id,
                decision_id: found.decision_id.clone(),
                confirmation: json!({"thruline_decision_act":{
                    "schema_version":1,"action":"waive","decision_id":found.decision_id,
                    "version_id":found.version_id,"actor":found.actor,"because":found.because,
                    "expires_version":found.expires_version,"expires_at":found.expires_at,
                }}),
            })
        }
    }
}

fn event_proof(
    conn: &Connection,
    project_id: &str,
    event_id: &str,
    expected: Option<&Value>,
    deadline: Option<Instant>,
) -> Result<Option<EventProof>> {
    if deadline.is_some_and(|limit| Instant::now() >= limit) {
        return Ok(None);
    }
    let event: Option<RecordedEvent> = conn
        .query_row(
            "SELECT e.project_id,e.session_id,e.seq,e.kind,e.text,e.raw_file,e.raw_line,
                    e.raw_sha256,e.prev_hash,e.chain_hash,e.is_sidechain,
                    e.line_session_id,p.source
             FROM events e JOIN sessions s ON s.id=e.session_id AND s.project_id=e.project_id
             JOIN projects p ON p.id=e.project_id
             WHERE e.id=?1 AND e.role='user'",
            [event_id],
            |row| {
                Ok(RecordedEvent {
                    project_id: row.get(0)?,
                    session_id: row.get(1)?,
                    seq: row.get(2)?,
                    kind: row.get(3)?,
                    text: row.get(4)?,
                    raw_file: row.get(5)?,
                    raw_line: row.get(6)?,
                    raw_sha256: row.get(7)?,
                    prev_hash: row.get(8)?,
                    chain_hash: row.get(9)?,
                    is_sidechain: row.get(10)?,
                    line_session_id: row.get(11)?,
                    source: row.get(12)?,
                })
            },
        )
        .optional()?;
    let Some(event) = event else {
        return Ok(None);
    };
    if event.project_id != project_id
        || event.kind != "user_msg"
        || event.is_sidechain != 0
        || event.raw_line < 1
    {
        return Ok(None);
    }
    if event.source == "codex" {
        // A subagent's parent instruction is also a canonical `user_msg` in
        // its own rollout. HQ identifies its origin from the first line;
        // the ledger's `session_id` is owner-scoped, so compare the raw
        // session_meta id with the event's source session key instead.
        let Some(source_session_id) = event.line_session_id.as_deref() else {
            return Ok(None);
        };
        // A later accepted generation can have a new sealed payload path;
        // read metadata from the very same immutable file as this event.
        let raw_file = Path::new(&event.raw_file);
        let Some(meta_line) = raw_line_bytes(raw_file, 1, deadline)? else {
            return Ok(None);
        };
        let Some(meta) = serde_json::from_slice::<Value>(&meta_line).ok() else {
            return Ok(None);
        };
        if meta.get("type").and_then(Value::as_str) != Some("session_meta") {
            return Ok(None);
        }
        let Some(origin) = crate::hq::codex_origin(raw_file) else {
            return Ok(None);
        };
        if origin.subagent || origin.key.as_deref() != Some(source_session_id) {
            return Ok(None);
        }
    }
    let Some(text) = event.text.as_deref() else {
        return Ok(None);
    };
    if let Some(expected) = expected {
        if serde_json::from_str::<Value>(text).ok().as_ref() != Some(expected) {
            return Ok(None);
        }
    }
    let Some(raw) = raw_line_bytes(Path::new(&event.raw_file), event.raw_line as u64, deadline)?
    else {
        return Ok(None);
    };
    if sha256_hex(&raw) != event.raw_sha256 {
        return Ok(None);
    }
    if !raw_is_user_message(
        Some(&event.source),
        &raw,
        &RawMessage {
            raw_file: &event.raw_file,
            raw_line: event.raw_line as u64,
            raw_sha256: &event.raw_sha256,
            session_id: &event.session_id,
            text,
        },
    ) {
        return Ok(None);
    }
    let mut statement = conn.prepare(
        "SELECT seq,raw_file,raw_line,raw_sha256,prev_hash,chain_hash
         FROM events WHERE session_id=?1 AND seq<=?2 ORDER BY seq",
    )?;
    let rows = statement.query_map(params![event.session_id, event.seq], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;
    let mut prev = genesis_prev_hash(project_id);
    let mut found = false;
    for (next_seq, row) in (1i64..).zip(rows) {
        if deadline.is_some_and(|limit| Instant::now() >= limit) {
            return Ok(None);
        }
        let (seq, raw_file, raw_line, raw_sha256, prev_hash, chain_hash_found) = row?;
        if seq != next_seq
            || prev_hash != prev
            || chain_hash_found != chain_hash(&prev, &raw_sha256, &event.session_id, seq)
        {
            return Ok(None);
        }
        if seq == event.seq {
            found = raw_file == event.raw_file
                && raw_line == event.raw_line
                && raw_sha256 == event.raw_sha256
                && prev_hash == event.prev_hash
                && chain_hash_found == event.chain_hash;
        }
        prev = chain_hash_found;
    }
    if !found || deadline.is_some_and(|limit| Instant::now() >= limit) {
        return Ok(None);
    }
    Ok(Some(EventProof {
        event_id: event_id.into(),
        session_id: event.session_id,
        raw_file: event.raw_file,
        raw_line: event.raw_line,
        raw_sha256: event.raw_sha256,
        chain_hash: event.chain_hash,
    }))
}

fn raw_is_user_message(source: Option<&str>, raw: &[u8], message: &RawMessage<'_>) -> bool {
    match source {
        Some("claude_code") => {
            // Claude Code may put agent-owned input in the user channel.
            // `isSidechain` is checked above; an explicit agent owner is also
            // disqualifying even when that flag was absent in the raw line.
            if serde_json::from_slice::<Value>(raw)
                .ok()
                .is_some_and(|value| value.get("agentId").is_some_and(|id| !id.is_null()))
            {
                return false;
            }
            let mut stats = ParseStats::default();
            let Ok(parsed) = parse_reader(raw, message.raw_file.to_owned(), &mut stats) else {
                return false;
            };
            parsed.len() == 1
                && parsed[0].raw_type == "user"
                && parsed[0].session_id.as_deref() == Some(message.session_id)
                && parsed[0].sha256 == message.raw_sha256
                && parsed[0].events.iter().any(|part| {
                    part.kind == EventKind::UserMsg
                        && part.role.as_deref() == Some("user")
                        && part.text.as_deref() == Some(message.text)
                        && !part.is_sidechain
                })
        }
        Some("codex") => {
            let Ok(value) = serde_json::from_slice::<Value>(raw) else {
                return false;
            };
            let context = CodexLineContext {
                owner_session_id: message.session_id,
                source_session_id: message.session_id,
                file_id: "person-authority",
                turn_scope: "person-authority",
                turn_boundary_line: None,
                turn_boundary_sha256: None,
                line: message.raw_line,
                raw_sha256: message.raw_sha256,
            };
            let Ok(mapped) = normalize_codex_line(context, &value) else {
                return false;
            };
            mapped.envelope_type == "response_item"
                && mapped.events.iter().any(|part| {
                    matches!(&part.emission, CodexEmissionRole::CanonicalItem { .. })
                        && part.event.kind == EventKind::UserMsg
                        && part.event.role.as_deref() == Some("user")
                        && part.event.text.as_deref() == Some(message.text)
                })
        }
        _ => false,
    }
}

fn raw_line_bytes(path: &Path, want: u64, deadline: Option<Instant>) -> Result<Option<Vec<u8>>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return Ok(None),
    };
    let mut reader = BufReader::new(file);
    let mut line = 0u64;
    loop {
        if deadline.is_some_and(|limit| Instant::now() >= limit) {
            return Ok(None);
        }
        let mut bytes = Vec::new();
        let n = reader
            .read_until(b'\n', &mut bytes)
            .map_err(|e| CoreError::io(path, e))?;
        if n == 0 {
            return Ok(None);
        }
        if bytes.len() > 1_000_000 {
            return Ok(None);
        }
        line += 1;
        if line != want {
            continue;
        }
        if bytes.ends_with(b"\n") {
            bytes.pop();
            if bytes.ends_with(b"\r") {
                bytes.pop();
            }
        }
        return Ok(Some(bytes));
    }
}
