//! Interactive, local confirmation of a Decisions person act.
//!
//! The macOS user-presence callback and a typed answer happen when the act is
//! created. A later read can prove that the stored tuple, terminal log, event,
//! session chain, and immutable receipt still agree. LocalAuthentication does
//! not issue a portable signed identity proof: a same-UID process with direct
//! write access to both SQLite and the raw log is outside this persisted
//! consistency guarantee.

use std::fs;
#[cfg(target_os = "macos")]
use std::fs::OpenOptions;
#[cfg(target_os = "macos")]
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::process::{Command, Output, Stdio};
#[cfg(target_os = "macos")]
use std::thread;
use std::time::{Duration, Instant};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
#[cfg(target_os = "macos")]
use ulid::Ulid;

use crate::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
#[cfg(target_os = "macos")]
use crate::model::{EventKind, NormalizedEvent, RawLineRef};
use crate::redact::{resembles_known_credential, Masked};

use super::super::params::{parse_params, CheckParams};
use super::{target, PersonActKind};

#[path = "general_supersession.rs"]
mod general_supersession;

#[path = "bridge_raw.rs"]
mod raw_receipt;

#[path = "native_click.rs"]
mod native_click;
pub use native_click::{confirm_native_click, native_click_availability, NativeClickAvailability};

const FORMAT: &str = "thruline_terminal_confirmation_v1";

const DDL: &str = r#"
CREATE TABLE IF NOT EXISTS decision_bridge_proofs(
  event_id TEXT PRIMARY KEY REFERENCES events(id),
  receipt_id TEXT NOT NULL UNIQUE REFERENCES decision_person_acts(id),
  project_id TEXT NOT NULL REFERENCES projects(id),
  session_id TEXT NOT NULL REFERENCES sessions(id),
  action TEXT NOT NULL CHECK(action IN ('accept','retire','waive','supersede')),
  act_row_id TEXT NOT NULL,
  nonce TEXT NOT NULL UNIQUE,
  tuple_sha256 TEXT NOT NULL,
  account_uid INTEGER NOT NULL,
  account_name TEXT NOT NULL,
  raw_sha256 TEXT NOT NULL,
  supersession_json TEXT,
  confirmation_method TEXT NOT NULL CHECK(confirmation_method='macos_local_authentication_tty'),
  created_at TEXT NOT NULL);
CREATE TRIGGER IF NOT EXISTS decision_bridge_proofs_no_replace
  BEFORE INSERT ON decision_bridge_proofs
  WHEN EXISTS(SELECT 1 FROM decision_bridge_proofs old WHERE old.event_id=NEW.event_id)
  BEGIN SELECT RAISE(ABORT,'decision bridge proofs are append-only'); END;
CREATE TRIGGER IF NOT EXISTS decision_bridge_proofs_no_update
  BEFORE UPDATE ON decision_bridge_proofs
  BEGIN SELECT RAISE(ABORT,'decision bridge proofs are append-only'); END;
CREATE TRIGGER IF NOT EXISTS decision_bridge_proofs_no_delete
  BEFORE DELETE ON decision_bridge_proofs
  BEGIN SELECT RAISE(ABORT,'decision bridge proofs are append-only'); END;
"#;

/// The bridge table is installed only by an explicit Decisions write or
/// upgrade operation. A read or gate never creates it.
pub fn install_schema_for_write(ledger: &Ledger) -> Result<()> {
    ledger.conn().execute_batch(DDL)?;
    let mut statement = ledger
        .conn()
        .prepare("PRAGMA table_info(decision_bridge_proofs)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if !columns.iter().any(|column| column == "supersession_json") {
        ledger.conn().execute_batch(
            "ALTER TABLE decision_bridge_proofs ADD COLUMN supersession_json TEXT",
        )?;
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct SupersessionInput {
    pub predecessor_revision: u32,
    pub screen: String,
}

fn table_exists(conn: &Connection) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='decision_bridge_proofs'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

/// A project gate activates only when every standing blocking decision has a
/// fresh interactive receipt. A later pending revision cannot switch off a
/// previously confirmed block. A wholly unconfirmed block keeps the gate off.
pub fn gates_active(ledger: &Ledger, project_id: &str) -> Result<bool> {
    let mut statement = ledger.conn().prepare(
        "SELECT d.id,a.severity FROM decisions d JOIN decision_acceptances a ON a.decision_id=d.id
         WHERE d.project_id=?1
           AND a.rowid=(SELECT max(a2.rowid) FROM decision_acceptances a2
                        WHERE a2.decision_id=d.id) ORDER BY d.id",
    )?;
    let rows = statement
        .query_map([project_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if rows.is_empty() {
        return Ok(false);
    }
    let mut standing = 0usize;
    for (decision_id, newest_severity) in rows {
        if super::super::storage::verified_retirement_for_decision(ledger, &decision_id)?.is_some()
        {
            continue;
        }
        let confirmed =
            super::super::storage::verified_acceptance_for_decision(ledger, &decision_id)?;
        match confirmed {
            Some(accepted) if accepted.severity == "blocks" => standing += 1,
            None if newest_severity == "blocks" => return Ok(false),
            _ => {}
        }
    }
    Ok(standing > 0)
}

struct BridgeRow {
    receipt_id: String,
    event_id: String,
    project_id: String,
    session_id: String,
    action: String,
    act_row_id: String,
    nonce: String,
    tuple_sha256: String,
    account_uid: u32,
    account_name: String,
    raw_sha256: String,
    supersession_json: Option<String>,
    receipt_decision_id: String,
    receipt_project_id: String,
    receipt_session_id: String,
    receipt_raw_file: String,
    receipt_raw_line: i64,
    receipt_raw_sha256: String,
    receipt_chain_hash: String,
    receipt_tuple_sha256: String,
}

struct EventEvidence {
    project: String,
    session: String,
    seq: i64,
    kind: String,
    text: Option<String>,
    sidechain: i64,
    raw_sha: String,
    prev: String,
    chain: String,
    raw_file: String,
    line_session: Option<String>,
}

struct SessionEvidence {
    raw_lines: i64,
    raw_digest: Option<String>,
    last_seq: i64,
    event_count: i64,
    head: Option<String>,
}

struct TerminalLineFields<'a> {
    project_id: &'a str,
    session_id: &'a str,
    nonce: &'a str,
    action: &'a str,
    act_row_id: &'a str,
    account_uid: u32,
    account_name: &'a str,
    confirmation: &'a Value,
}

/// Re-derive a terminal receipt without changing the schema or local files.
/// Corrupt, absent, or legacy-only evidence is inert.
pub fn verified(ledger: &Ledger, action: PersonActKind, act_row_id: &str) -> Result<bool> {
    verified_on_connection(ledger.conn(), action, act_row_id, None)
}

pub fn verified_read_only(
    conn: &Connection,
    action: PersonActKind,
    act_row_id: &str,
    budget: Duration,
) -> Result<bool> {
    let deadline = Instant::now() + budget;
    let valid = verified_on_connection(conn, action, act_row_id, Some(deadline))?;
    Ok(valid && Instant::now() < deadline)
}

fn verified_on_connection(
    conn: &Connection,
    action: PersonActKind,
    act_row_id: &str,
    deadline: Option<Instant>,
) -> Result<bool> {
    if expired(deadline) {
        return Ok(false);
    }
    if !table_exists(conn)? {
        return Ok(false);
    }
    let target = target(conn, action, act_row_id)?;
    let row: Option<BridgeRow> = conn
        .query_row(
            "SELECT b.receipt_id,b.event_id,b.project_id,b.session_id,b.action,b.act_row_id,
                    b.nonce,b.tuple_sha256,b.account_uid,b.account_name,b.raw_sha256,
                    b.supersession_json,
                    p.decision_id,p.project_id,p.session_id,p.raw_file,p.raw_line,
                    p.raw_sha256,p.chain_hash,p.tuple_sha256
             FROM decision_bridge_proofs b
             JOIN decision_person_acts p ON p.id=b.receipt_id AND p.event_id=b.event_id
             WHERE p.action=?1 AND p.act_row_id=?2
               AND b.confirmation_method='macos_local_authentication_tty'",
            params![action.as_str(), act_row_id],
            |r| {
                Ok(BridgeRow {
                    receipt_id: r.get(0)?,
                    event_id: r.get(1)?,
                    project_id: r.get(2)?,
                    session_id: r.get(3)?,
                    action: r.get(4)?,
                    act_row_id: r.get(5)?,
                    nonce: r.get(6)?,
                    tuple_sha256: r.get(7)?,
                    account_uid: r.get(8)?,
                    account_name: r.get(9)?,
                    raw_sha256: r.get(10)?,
                    supersession_json: r.get(11)?,
                    receipt_decision_id: r.get(12)?,
                    receipt_project_id: r.get(13)?,
                    receipt_session_id: r.get(14)?,
                    receipt_raw_file: r.get(15)?,
                    receipt_raw_line: r.get(16)?,
                    receipt_raw_sha256: r.get(17)?,
                    receipt_chain_hash: r.get(18)?,
                    receipt_tuple_sha256: r.get(19)?,
                })
            },
        )
        .optional()?;
    let Some(row) = row else { return Ok(false) };
    if expired(deadline) {
        return Ok(false);
    }
    let mut confirmation = target.confirmation.clone();
    // Native review wraps (and does not replace) the existing supersession pins.
    let native = match row.supersession_json.as_deref() {
        Some(stored) => match native_click::parse_envelope(stored) {
            Ok(value) => value,
            Err(_) => return Ok(false),
        },
        None => None,
    };
    let nested = native
        .as_ref()
        .and_then(|n| n.supersession.as_ref())
        .map(serde_json::to_string)
        .transpose()?;
    let metadata = if native.is_some() {
        nested.as_deref()
    } else {
        row.supersession_json.as_deref()
    };
    let bridge_action = match (action, row.action.as_str(), metadata) {
        (PersonActKind::Accept, "supersede", Some(stored)) => {
            let Ok(metadata) = serde_json::from_str::<Value>(stored) else {
                return Ok(false);
            };
            let Some(revision) = metadata
                .get("predecessor_revision")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
            else {
                return Ok(false);
            };
            let Some(screen) = metadata.get("screen").and_then(Value::as_str) else {
                return Ok(false);
            };
            let Ok(derived) =
                supersession_data(conn, &target.decision_id, act_row_id, revision, screen)
            else {
                return Ok(false);
            };
            if metadata != derived {
                return Ok(false);
            }
            confirmation
                .as_object_mut()
                .expect("act tuple object")
                .insert("thruline_supersession".into(), derived);
            "supersede"
        }
        (PersonActKind::Retire, "retire", Some(stored)) => {
            let Ok(metadata) = serde_json::from_str::<Value>(stored) else {
                return Ok(false);
            };
            let Ok(Some(derived)) = general_supersession::derive(conn, act_row_id, deadline) else {
                return Ok(false);
            };
            if metadata != derived {
                return Ok(false);
            }
            let Some(object) = confirmation.as_object_mut() else {
                return Ok(false);
            };
            object.insert("thruline_general_supersession".into(), derived);
            "retire"
        }
        (_, found, None) if found == action.as_str() => found,
        _ => return Ok(false),
    };
    if let Some(native) = &native {
        native_click::bind_confirmation(&mut confirmation, &native.review_sha256, &row.nonce)?;
    }
    let tuple_sha256 = sha256_hex(&serde_json::to_vec(&confirmation)?);
    if row.receipt_id.is_empty()
        || row.project_id != target.project_id
        || row.receipt_project_id != target.project_id
        || row.receipt_decision_id != target.decision_id
        || row.session_id != row.receipt_session_id
        || row.act_row_id != act_row_id
        || row.action != bridge_action
        || row.tuple_sha256 != tuple_sha256
        || row.receipt_tuple_sha256 != tuple_sha256
        || row.raw_sha256 != row.receipt_raw_sha256
        || row.receipt_raw_line != 1
        || row.account_name.is_empty()
    {
        return Ok(false);
    }
    let event: Option<EventEvidence> = conn
        .query_row(
            "SELECT e.project_id,e.session_id,e.seq,e.kind,e.text,e.is_sidechain,
                    e.raw_sha256,e.prev_hash,e.chain_hash,e.raw_file,e.line_session_id
             FROM events e JOIN sessions s ON s.id=e.session_id
             WHERE e.id=?1 AND e.role='user' AND s.project_id=e.project_id
               AND s.file_path=e.raw_file AND e.raw_line=1",
            [&row.event_id],
            |r| {
                Ok(EventEvidence {
                    project: r.get(0)?,
                    session: r.get(1)?,
                    seq: r.get(2)?,
                    kind: r.get(3)?,
                    text: r.get(4)?,
                    sidechain: r.get(5)?,
                    raw_sha: r.get(6)?,
                    prev: r.get(7)?,
                    chain: r.get(8)?,
                    raw_file: r.get(9)?,
                    line_session: r.get(10)?,
                })
            },
        )
        .optional()?;
    let Some(EventEvidence {
        project,
        session,
        seq,
        kind,
        text,
        sidechain,
        raw_sha,
        prev,
        chain,
        raw_file: event_file,
        line_session,
    }) = event
    else {
        return Ok(false);
    };
    if project != target.project_id
        || session != row.session_id
        || seq != 1
        || kind != "user_msg"
        || text.as_deref() != Some(confirmation.to_string().as_str())
        || sidechain != 0
        || raw_sha != row.raw_sha256
        || event_file != row.receipt_raw_file
        || line_session.as_deref() != Some(session.as_str())
        || prev != genesis_prev_hash(&project)
        || chain != chain_hash(&prev, &raw_sha, &session, 1)
        || chain != row.receipt_chain_hash
    {
        return Ok(false);
    }
    let vendor_source: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM source_sessions WHERE session_id=?1)",
        [&session],
        |r| r.get(0),
    )?;
    if vendor_source {
        return Ok(false);
    }
    let normalized: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM events WHERE id=?1
         AND tool_name IS NULL AND source_tool_name IS NULL AND tool_use_id IS NULL
         AND tool_input IS NULL AND tool_result IS NULL AND is_error=0
         AND src_uuid IS NULL AND parent_uuid IS NULL AND source_turn_id IS NULL
         AND source_turn_boundary_line IS NULL AND source_turn_boundary_sha256 IS NULL
         AND source_message_key IS NULL)",
        [&row.event_id],
        |r| r.get(0),
    )?;
    if !normalized || expired(deadline) {
        return Ok(false);
    }
    let path = Path::new(&row.receipt_raw_file);
    let Some(db_path) = database_path(conn)? else {
        return Ok(false);
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let Ok(db_meta) = fs::symlink_metadata(&db_path) else {
            return Ok(false);
        };
        if db_meta.file_type().is_symlink()
            || db_meta.uid() != row.account_uid
            || row.account_uid == 0
        {
            return Ok(false);
        }
    }
    let expected_dir = db_path
        .parent()
        .unwrap_or(Path::new("/"))
        .join("decision-confirmations");
    let canonical_parent = path.parent().and_then(|parent| parent.canonicalize().ok());
    let canonical_expected = expected_dir.canonicalize().ok();
    if canonical_parent.is_none()
        || canonical_parent != canonical_expected
        || path.file_name().and_then(|name| name.to_str())
            != Some(format!("{}.jsonl", row.session_id).as_str())
        || !private_path(
            path.parent().unwrap_or(Path::new("/")),
            row.account_uid,
            true,
        )
        || !private_path(path, row.account_uid, false)
    {
        return Ok(false);
    }
    let Some(raw) = raw_receipt::read_private(path, row.account_uid, deadline) else {
        return Ok(false);
    };
    let Some(line) = raw.strip_suffix(b"\n") else {
        return Ok(false);
    };
    if line.contains(&b'\n') || sha256_hex(line) != row.raw_sha256 {
        return Ok(false);
    }
    let raw_value: Value = match serde_json::from_slice(line) {
        Ok(value) => value,
        Err(_) => return Ok(false),
    };
    let mut expected = terminal_line(&TerminalLineFields {
        project_id: &target.project_id,
        session_id: &session,
        nonce: &row.nonce,
        action: bridge_action,
        act_row_id,
        account_uid: row.account_uid,
        account_name: &row.account_name,
        confirmation: &confirmation,
    });
    if native.is_some() {
        expected["format"] = json!(native_click::FORMAT);
    }
    if raw_value != expected {
        return Ok(false);
    }
    let mut digest = Sha256::new();
    digest.update((line.len() as u64).to_le_bytes());
    digest.update(line);
    let digest = crate::chain::to_hex(&digest.finalize());
    if expired(deadline) {
        return Ok(false);
    }
    let stored: Option<SessionEvidence> = conn
        .query_row(
            "SELECT raw_line_count,raw_digest,last_seq,event_count,head_chain_hash
             FROM sessions WHERE id=?1 AND project_id=?2",
            params![session, project],
            |r| {
                Ok(SessionEvidence {
                    raw_lines: r.get(0)?,
                    raw_digest: r.get(1)?,
                    last_seq: r.get(2)?,
                    event_count: r.get(3)?,
                    head: r.get(4)?,
                })
            },
        )
        .optional()?;
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM events WHERE session_id=?1",
        [&session],
        |r| r.get(0),
    )?;
    Ok(!expired(deadline)
        && count == 1
        && stored.is_some_and(|state| {
            state.raw_lines == 1
                && state.raw_digest.as_deref() == Some(&digest)
                && state.last_seq == 1
                && state.event_count == 1
                && state.head.as_deref() == Some(chain.as_str())
        }))
}

fn expired(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|limit| Instant::now() >= limit)
}

fn terminal_line(fields: &TerminalLineFields<'_>) -> Value {
    json!({
        "format": FORMAT, "project_id": fields.project_id, "session_id": fields.session_id,
        "nonce": fields.nonce, "action": fields.action, "act_row_id": fields.act_row_id,
        "account_uid": fields.account_uid, "account_name": fields.account_name,
        "confirmation": fields.confirmation,
    })
}

/// One visual reference replacement within the same decision. The exact old
/// and new revision contents are re-read on every verification. Other screen
/// and tolerance changes require their own revision acceptance.
fn supersession_data(
    conn: &Connection,
    decision_id: &str,
    acceptance_id: &str,
    predecessor_revision: u32,
    screen: &str,
) -> Result<Value> {
    let successor: (u32, String, u32, String) = conn.query_row(
        "SELECT a.rev,r.kind,r.params_schema_version,r.params_json
         FROM decision_acceptances a JOIN decision_revisions r
           ON r.decision_id=a.decision_id AND r.rev=a.rev
         WHERE a.id=?1 AND a.decision_id=?2",
        params![acceptance_id, decision_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    if predecessor_revision == 0 || predecessor_revision >= successor.0 || screen.trim().is_empty()
    {
        return Err(CoreError::Config(
            "supersession needs an earlier revision and named screen".into(),
        ));
    }
    let previous: (String, u32, String) = conn.query_row(
        "SELECT kind,params_schema_version,params_json FROM decision_revisions
         WHERE decision_id=?1 AND rev=?2",
        params![decision_id, predecessor_revision],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let old = match parse_params(&previous.0, previous.1, &previous.2)? {
        CheckParams::VisualMatch(params) => params,
        _ => {
            return Err(CoreError::Config(
                "supersession requires visual_match revisions".into(),
            ))
        }
    };
    let mut new = match parse_params(&successor.1, successor.2, &successor.3)? {
        CheckParams::VisualMatch(params) => params,
        _ => {
            return Err(CoreError::Config(
                "supersession requires visual_match revisions".into(),
            ))
        }
    };
    let old_screen = old
        .screens
        .iter()
        .filter(|item| item.screen == screen)
        .collect::<Vec<_>>();
    let found = new
        .screens
        .iter()
        .filter(|item| item.screen == screen)
        .count();
    if old_screen.len() != 1 || found != 1 {
        return Err(CoreError::Config(
            "supersession screen must occur once in both revisions".into(),
        ));
    }
    let old_screen = old_screen[0];
    let new_screen = new
        .screens
        .iter_mut()
        .find(|item| item.screen == screen)
        .expect("count checked");
    let new_path = new_screen.reference_path.clone();
    let new_sha = new_screen.reference_sha256.clone();
    if old_screen.reference_path == new_path || old_screen.reference_sha256 == new_sha {
        return Err(CoreError::Config(
            "supersession must replace the named reference path and digest".into(),
        ));
    }
    new_screen
        .reference_path
        .clone_from(&old_screen.reference_path);
    new_screen
        .reference_sha256
        .clone_from(&old_screen.reference_sha256);
    if new != old {
        return Err(CoreError::Config(
            "supersession changed fields beyond the named reference".into(),
        ));
    }
    Ok(json!({
        "predecessor_decision_id": decision_id,
        "predecessor_revision": predecessor_revision,
        "predecessor_params_sha256": sha256_hex(previous.2.as_bytes()),
        "successor_decision_id": decision_id,
        "successor_revision": successor.0,
        "successor_params_sha256": sha256_hex(successor.3.as_bytes()),
        "screen": screen,
        "old_reference_path": old_screen.reference_path,
        "old_reference_sha256": old_screen.reference_sha256,
        "new_reference_path": new_path,
        "new_reference_sha256": new_sha,
    }))
}

/// Show the actual stored revision behind a pending act before asking for OS
/// presence. This is a read-only review, not a person receipt. For accept, the
/// displayed parameter digest must be the digest in the canonical act tuple.
pub fn review_terminal_act(
    ledger: &Ledger,
    action: PersonActKind,
    act_row_id: &str,
    supersession: Option<&SupersessionInput>,
) -> Result<Value> {
    let target = target(ledger.conn(), action, act_row_id)?;
    let mut confirmation = target.confirmation.clone();
    let mut metadata = None;
    let bridge_action = if let Some(selection) = supersession {
        if action != PersonActKind::Accept {
            return Err(CoreError::Config(
                "supersession must accept a successor revision".into(),
            ));
        }
        let binding = supersession_data(
            ledger.conn(),
            &target.decision_id,
            act_row_id,
            selection.predecessor_revision,
            &selection.screen,
        )?;
        confirmation
            .as_object_mut()
            .expect("act tuple object")
            .insert("thruline_supersession".into(), binding.clone());
        metadata = Some(binding);
        "supersede"
    } else if action == PersonActKind::Retire {
        if let Some(binding) = general_supersession::derive(ledger.conn(), act_row_id, None)? {
            confirmation
                .as_object_mut()
                .ok_or_else(|| CoreError::Integrity("act tuple is not an object".into()))?
                .insert("thruline_general_supersession".into(), binding.clone());
            metadata = Some(binding);
        }
        action.as_str()
    } else {
        action.as_str()
    };
    let act_tuple = &confirmation["thruline_decision_act"];
    let (revision, revision_basis, acceptance_version) = if action == PersonActKind::Accept {
        let revision = act_tuple["revision"]
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| CoreError::Integrity("acceptance has no revision".into()))?;
        (
            revision,
            "act_revision",
            act_tuple["applies_from_version"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        )
    } else if let Some(accepted) =
        super::super::storage::verified_acceptance_for_decision(ledger, &target.decision_id)?
    {
        (
            accepted.revision,
            "bridge_verified_acceptance",
            accepted.applies_from_version,
        )
    } else {
        let (revision, version): (u32, String) = ledger.conn().query_row(
            "SELECT a.rev,a.applies_from_version FROM decision_acceptances a WHERE a.decision_id=?1
             ORDER BY a.rowid DESC LIMIT 1",
            [&target.decision_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        (
            revision,
            "latest_recorded_acceptance_pending_bridge",
            version,
        )
    };
    let (kind, schema, params_json, text): (String, u32, String, String) =
        ledger.conn().query_row(
            "SELECT kind,params_schema_version,params_json,text FROM decision_revisions
         WHERE decision_id=?1 AND rev=?2",
            params![target.decision_id, revision],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
    let parsed_params = parse_params(&kind, schema, &params_json)?;
    let params_sha256 = sha256_hex(params_json.as_bytes());
    if action == PersonActKind::Accept
        && act_tuple["params_sha256"].as_str() != Some(params_sha256.as_str())
    {
        return Err(CoreError::Integrity(
            "displayed revision parameters differ from the pending act".into(),
        ));
    }
    let safe =
        |text: &str| !resembles_known_credential(text) && Masked::mask(text).into_text() == text;
    if !super::super::review_params::safe_for_terminal(
        ledger,
        &target.project_id,
        &parsed_params,
        &params_json,
        &acceptance_version,
    )? || !safe(&text)
        || !safe(&confirmation.to_string())
        || metadata
            .as_ref()
            .is_some_and(|value| !safe(&value.to_string()))
    {
        return Err(CoreError::Config(
            "decision review contains masked values and cannot be shown at the terminal".into(),
        ));
    }
    let summary = if let Some(binding) = &metadata {
        if binding["kind"].as_str() == Some("general_decision_supersession") {
            format!(
                "Retire decision {} revision {} (params SHA-256 {}) in favour of decision {} revision {} (params SHA-256 {}).",
                binding["predecessor_decision_id"], binding["predecessor_revision"],
                binding["predecessor_params_sha256"], binding["successor_decision_id"],
                binding["successor_revision"], binding["successor_params_sha256"],
            )
        } else {
            format!(
            "Replace {} reference {} (SHA-256 {}) in revision {} with {} (SHA-256 {}) in revision {}.",
            binding["screen"].as_str().unwrap_or("selected screen"),
            binding["old_reference_path"].as_str().unwrap_or("?"),
            binding["old_reference_sha256"].as_str().unwrap_or("?"),
            binding["predecessor_revision"],
            binding["new_reference_path"].as_str().unwrap_or("?"),
            binding["new_reference_sha256"].as_str().unwrap_or("?"),
            binding["successor_revision"],
        )
        }
    } else {
        format!("Review the {bridge_action} action and the displayed decision revision before confirming.")
    };
    let general_details = metadata
        .as_ref()
        .filter(|m| m["kind"].as_str() == Some("general_decision_supersession"))
        .map(|m| general_supersession::review_details(ledger, m))
        .transpose()?;
    let mut review = json!({
        "schema_version": 1,
        "project_id": target.project_id,
        "decision_id": target.decision_id,
        "action": bridge_action,
        "act_row_id": act_row_id,
        "decision": {
            "revision": revision,
            "revision_basis": revision_basis,
            "kind": kind,
            "text": text,
            "params": serde_json::from_str::<Value>(&params_json)?,
            "params_sha256": params_sha256,
        },
        "confirmation": confirmation,
        "supersession": metadata,
        "summary": summary,
    });
    if let Some(details) = general_details {
        review["supersession_decisions"] = details;
    }
    Ok(review)
}

/// Kind lookup only, used after full receipt verification by lifecycle reads.
pub(crate) fn has_exact_general_supersession(conn: &Connection, act_row_id: &str) -> Result<bool> {
    let json: Option<String> = conn.query_row(
        "SELECT supersession_json FROM decision_bridge_proofs WHERE action='retire' AND act_row_id=?1",
        [act_row_id], |r| r.get(0),
    ).optional()?.flatten();
    Ok(json
        .and_then(|s| match native_click::parse_envelope(&s) {
            Ok(Some(native)) => native.supersession,
            Ok(None) => serde_json::from_str::<Value>(&s).ok(),
            Err(_) => None,
        })
        .is_some_and(|m| m["kind"].as_str() == Some("general_decision_supersession")))
}

fn database_path(conn: &Connection) -> Result<Option<PathBuf>> {
    let path: String = conn.query_row(
        "SELECT file FROM pragma_database_list WHERE name='main'",
        [],
        |r| r.get(0),
    )?;
    if path.is_empty() {
        return Ok(None);
    }
    Ok(Some(PathBuf::from(path)))
}

#[cfg(unix)]
fn private_path(path: &Path, uid: u32, directory: bool) -> bool {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    !meta.file_type().is_symlink()
        && if directory {
            meta.is_dir()
        } else {
            meta.is_file()
        }
        && meta.uid() == uid
        && meta.permissions().mode() & 0o077 == 0
}

#[cfg(not(unix))]
fn private_path(_path: &Path, _uid: u32, _directory: bool) -> bool {
    false
}

/// The only production constructor of a C28 terminal receipt. No caller can
/// supply an OS-presence boolean or an attestation token.
pub fn confirm_terminal(
    ledger: &mut Ledger,
    action: PersonActKind,
    act_row_id: &str,
) -> Result<String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (ledger, action, act_row_id);
        return Err(CoreError::Config(
            "terminal confirmation requires macOS LocalAuthentication".into(),
        ));
    }
    #[cfg(target_os = "macos")]
    {
        confirm_terminal_macos(ledger, action, act_row_id, None)
    }
}

/// Confirm a single successor acceptance as a visual reference supersession.
pub fn confirm_terminal_supersession(
    ledger: &mut Ledger,
    acceptance_id: &str,
    selection: SupersessionInput,
) -> Result<String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (ledger, acceptance_id, selection);
        return Err(CoreError::Config(
            "terminal confirmation requires macOS LocalAuthentication".into(),
        ));
    }
    #[cfg(target_os = "macos")]
    {
        confirm_terminal_macos(
            ledger,
            PersonActKind::Accept,
            acceptance_id,
            Some(selection),
        )
    }
}

/// Bind stdin and stdout to the account's physical foreground controlling
/// terminal. `/dev/tty` is an alias on macOS; its metadata is not an identity.
#[cfg(target_os = "macos")]
fn terminal_account_uid() -> Result<u32> {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(CoreError::Config(
            "person confirmation requires an interactive terminal".into(),
        ));
    }
    let uid = rustix::process::getuid().as_raw();
    if uid == 0 || rustix::process::geteuid().as_raw() != uid {
        return Err(CoreError::Config(
            "terminal and process account must match the ledger owner".into(),
        ));
    }
    let input = rustix::fs::fstat(std::io::stdin())
        .map_err(|_| CoreError::Config("stdin terminal identity is unavailable".into()))?;
    let output = rustix::fs::fstat(std::io::stdout())
        .map_err(|_| CoreError::Config("stdout terminal identity is unavailable".into()))?;
    if input.st_rdev != output.st_rdev {
        return Err(CoreError::Config(
            "stdin and stdout must use the same terminal device".into(),
        ));
    }
    let session = rustix::process::getsid(None)
        .map_err(|_| CoreError::Config("process session identity is unavailable".into()))?;
    let terminal_session = rustix::termios::tcgetsid(std::io::stdin())
        .map_err(|_| CoreError::Config("stdin is not this session controlling terminal".into()))?;
    if terminal_session != session {
        return Err(CoreError::Config(
            "stdin is not this session controlling terminal".into(),
        ));
    }
    let foreground = rustix::termios::tcgetpgrp(std::io::stdin()).map_err(|_| {
        CoreError::Config("terminal foreground process group is unavailable".into())
    })?;
    if foreground != rustix::process::getpgrp() {
        return Err(CoreError::Config(
            "person confirmation requires the terminal foreground process group".into(),
        ));
    }
    let name = rustix::termios::ttyname(std::io::stdin(), Vec::new())
        .map_err(|_| CoreError::Config("stdin physical terminal path is unavailable".into()))?;
    let path = Path::new(std::ffi::OsStr::from_bytes(name.as_bytes()));
    let named = fs::symlink_metadata(path).map_err(|e| CoreError::io(path, e))?;
    if named.file_type().is_symlink() || !named.file_type().is_char_device() {
        return Err(CoreError::Config(
            "stdin physical terminal must be a device without a symlink".into(),
        ));
    }
    // Opening a physical tty must never acquire it as a new controlling tty;
    // NOFOLLOW closes the path lookup race after the lstat above.
    let descriptor = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NOCTTY
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map_err(|_| CoreError::Config("stdin physical terminal cannot be opened safely".into()))?;
    let physical = rustix::fs::fstat(&descriptor)
        .map_err(|_| CoreError::Config("stdin physical terminal identity is unavailable".into()))?;
    if named.dev() != input.st_dev as u64
        || named.ino() != input.st_ino
        || named.rdev() != input.st_rdev as u64
        || physical.st_dev != input.st_dev
        || physical.st_ino != input.st_ino
        || physical.st_rdev != input.st_rdev
        || !rustix::fs::FileType::from_raw_mode(physical.st_mode).is_char_device()
    {
        return Err(CoreError::Config(
            "stdin terminal path and descriptor identities differ".into(),
        ));
    }
    let owner = named.uid();
    #[cfg(test)]
    let owner = terminal_pty_tests::device_uid_override().unwrap_or(owner);
    if owner != uid || input.st_uid != uid || physical.st_uid != uid {
        return Err(CoreError::Config(
            "terminal and process account must match the ledger owner".into(),
        ));
    }
    Ok(uid)
}

#[cfg(target_os = "macos")]
fn confirm_terminal_macos(
    ledger: &mut Ledger,
    action: PersonActKind,
    act_row_id: &str,
    supersession: Option<SupersessionInput>,
) -> Result<String> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(CoreError::Config(
            "person confirmation requires an interactive terminal".into(),
        ));
    }
    if agent_context() {
        return Err(CoreError::Config(
            "an agent or automated context cannot create a person confirmation".into(),
        ));
    }
    let uid = terminal_account_uid()?;
    let mut tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(|_| CoreError::Config("person confirmation requires /dev/tty".into()))?;
    let Some(db_path) = database_path(ledger.conn())? else {
        return Err(CoreError::Config(
            "terminal confirmation needs a file-backed ledger".into(),
        ));
    };
    let db_meta = fs::symlink_metadata(&db_path).map_err(|e| CoreError::io(&db_path, e))?;
    if db_meta.file_type().is_symlink() || !db_meta.is_file() || db_meta.uid() != uid {
        return Err(CoreError::Config(
            "ledger and terminal account owners differ".into(),
        ));
    }
    let account = os_account_name()?;
    let target = target(ledger.conn(), action, act_row_id)?;
    let already_recorded: bool = ledger.conn().query_row(
        "SELECT EXISTS(SELECT 1 FROM decision_person_acts WHERE action=?1 AND act_row_id=?2)",
        params![action.as_str(), act_row_id],
        |row| row.get(0),
    )?;
    if already_recorded {
        return Err(CoreError::Config(
            "this act already has a historical receipt; prepare a new revision or act for bridge confirmation".into(),
        ));
    }
    let mut confirmation = target.confirmation.clone();
    let (bridge_action, supersession_json) = if let Some(selection) = &supersession {
        if action != PersonActKind::Accept {
            return Err(CoreError::Config(
                "supersession must accept a successor revision".into(),
            ));
        }
        let metadata = supersession_data(
            ledger.conn(),
            &target.decision_id,
            act_row_id,
            selection.predecessor_revision,
            &selection.screen,
        )?;
        confirmation
            .as_object_mut()
            .expect("act tuple object")
            .insert("thruline_supersession".into(), metadata.clone());
        ("supersede", Some(serde_json::to_string(&metadata)?))
    } else if action == PersonActKind::Retire {
        if let Some(metadata) = general_supersession::derive(ledger.conn(), act_row_id, None)? {
            confirmation
                .as_object_mut()
                .ok_or_else(|| CoreError::Integrity("act tuple is not an object".into()))?
                .insert("thruline_general_supersession".into(), metadata.clone());
            (action.as_str(), Some(serde_json::to_string(&metadata)?))
        } else {
            (action.as_str(), None)
        }
    } else {
        (action.as_str(), None)
    };
    let review = review_terminal_act(ledger, action, act_row_id, supersession.as_ref())?;
    if review["confirmation"] != confirmation {
        return Err(CoreError::Integrity(
            "reviewed decision changed before confirmation".into(),
        ));
    }
    let tuple_sha = sha256_hex(&serde_json::to_vec(&confirmation)?);
    let nonce = Ulid::generate().to_string();
    let session_id = format!("decision-terminal-{}", Ulid::generate());
    let challenge = format!("CONFIRM {} {}", nonce, &tuple_sha[..12]);
    let deadline = Instant::now() + Duration::from_secs(120);
    writeln!(
        tty,
        "\nThruLine decision {} for project {}:\nOS account: {} ({})\n{}\n",
        bridge_action,
        target.project_id,
        account,
        uid,
        serde_json::to_string_pretty(&review)?
    )
    .map_err(|e| CoreError::io(Path::new("/dev/tty"), e))?;
    let reason = format!(
        "Confirm ThruLine decision {} nonce {} tuple SHA-256 {}",
        bridge_action, nonce, tuple_sha
    );
    os_user_presence(&reason)?;
    if expired(Some(deadline)) {
        return Err(CoreError::Config("decision challenge expired".into()));
    }
    writeln!(tty, "Type exactly: {challenge}")
        .map_err(|e| CoreError::io(Path::new("/dev/tty"), e))?;
    let answer = read_tty_answer(&mut tty, deadline)?;
    if answer.trim_end_matches(['\r', '\n']) != challenge {
        return Err(CoreError::Config(
            "typed confirmation did not match the exact decision challenge".into(),
        ));
    }
    if agent_context() {
        return Err(CoreError::Config(
            "agent context appeared during confirmation".into(),
        ));
    }
    if expired(Some(deadline)) {
        return Err(CoreError::Config("decision challenge expired".into()));
    }
    if review_terminal_act(ledger, action, act_row_id, supersession.as_ref())? != review {
        return Err(CoreError::Integrity(
            "reviewed decision changed during confirmation".into(),
        ));
    }
    // Presence and typing can take time. Refuse a path replacement before
    // creating the first confirmation file or appending any authority rows.
    let current_db = fs::symlink_metadata(&db_path)
        .map_err(|_| CoreError::Config("ledger changed during terminal confirmation".into()))?;
    if current_db.file_type().is_symlink()
        || !current_db.is_file()
        || current_db.uid() != uid
        || current_db.uid() != db_meta.uid()
        || current_db.dev() != db_meta.dev()
        || current_db.ino() != db_meta.ino()
    {
        return Err(CoreError::Config(
            "ledger changed during terminal confirmation".into(),
        ));
    }
    if terminal_account_uid()? != uid {
        return Err(CoreError::Config(
            "terminal account changed during confirmation".into(),
        ));
    }
    let dir = db_path
        .parent()
        .unwrap_or(Path::new("/"))
        .join("decision-confirmations");
    crate::private_store::create_private_dir(&dir).map_err(|e| CoreError::io(&dir, e))?;
    if !private_path(&dir, uid, true) {
        return Err(CoreError::Config(
            "confirmation directory is not private to the account".into(),
        ));
    }
    let raw_path = dir.join(format!("{session_id}.jsonl"));
    let line = terminal_line(&TerminalLineFields {
        project_id: &target.project_id,
        session_id: &session_id,
        nonce: &nonce,
        action: bridge_action,
        act_row_id,
        account_uid: uid,
        account_name: &account,
        confirmation: &confirmation,
    });
    let bytes = serde_json::to_vec(&line)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&raw_path)
        .map_err(|e| CoreError::io(&raw_path, e))?;
    file.write_all(&bytes)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|e| CoreError::io(&raw_path, e))?;
    if !private_path(&raw_path, uid, false) {
        return Err(CoreError::Config(
            "confirmation log is not private to the account".into(),
        ));
    }
    let (line_count, raw_digest) = crate::ingest::whole_file_digest(&raw_path)?;
    if expired(Some(deadline)) {
        return Err(CoreError::Config("decision challenge expired".into()));
    }
    let now = crate::ingest::iso8601_utc_now();
    ledger.conn().execute(
        "INSERT INTO sessions(id,project_id,file_path,started_at,raw_line_count,raw_digest)
         VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            session_id,
            target.project_id,
            raw_path.to_string_lossy(),
            now,
            line_count as i64,
            raw_digest
        ],
    )?;
    let mut event = NormalizedEvent::new(EventKind::UserMsg);
    event.role = Some("user".into());
    event.text = Some(confirmation.to_string());
    event.ts = Some(now.clone());
    event.line_session_id = Some(session_id.clone());
    let raw_sha = sha256_hex(&bytes);
    let placed = ledger.append_session_events(
        &target.project_id,
        &session_id,
        &[(
            RawLineRef {
                file: raw_path.to_string_lossy().into_owned(),
                line: 1,
                sha256: raw_sha.clone(),
            },
            event,
        )],
    )?;
    let event_id = placed
        .first()
        .ok_or_else(|| CoreError::Integrity("terminal event was not appended".into()))?
        .id
        .clone();
    install_schema_for_write(ledger)?;
    let receipt_id = Ulid::generate().to_string();
    let tx = ledger.conn().unchecked_transaction()?;
    tx.execute(
        "INSERT INTO decision_person_acts(id,action,act_row_id,decision_id,project_id,event_id,
         session_id,raw_file,raw_line,raw_sha256,chain_hash,tuple_sha256)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,1,?9,?10,?11)",
        params![
            receipt_id,
            action.as_str(),
            act_row_id,
            target.decision_id,
            target.project_id,
            event_id,
            session_id,
            raw_path.to_string_lossy(),
            raw_sha,
            placed[0].chain_hash,
            tuple_sha
        ],
    )?;
    tx.execute(
        "INSERT INTO decision_bridge_proofs(event_id,receipt_id,project_id,session_id,action,
         act_row_id,nonce,tuple_sha256,account_uid,account_name,raw_sha256,supersession_json,
         confirmation_method,created_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'macos_local_authentication_tty',?13)",
        params![
            event_id,
            receipt_id,
            target.project_id,
            session_id,
            bridge_action,
            act_row_id,
            nonce,
            tuple_sha,
            uid,
            account,
            raw_sha,
            supersession_json,
            now
        ],
    )?;
    tx.commit()?;
    if !verified(ledger, action, act_row_id)? {
        return Err(CoreError::Integrity(
            "new terminal confirmation did not verify".into(),
        ));
    }
    Ok(receipt_id)
}

#[cfg(target_os = "macos")]
fn os_account_name() -> Result<String> {
    let result = bounded_output(
        Command::new("/usr/bin/id").arg("-un"),
        Duration::from_secs(2),
    )?;
    if !result.status.success() {
        return Err(CoreError::Config("cannot identify terminal account".into()));
    }
    let name = String::from_utf8(result.stdout)
        .map_err(|_| CoreError::Config("terminal account name is invalid".into()))?;
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err(CoreError::Config("terminal account name is empty".into()));
    }
    Ok(name)
}

#[cfg(target_os = "macos")]
fn os_user_presence(reason: &str) -> Result<()> {
    #[cfg(test)]
    if terminal_pty_tests::presence_stub_enabled() {
        terminal_pty_tests::presence_callback();
        return Ok(());
    }
    let status = bounded_status(
        Command::new("/usr/bin/swift")
            .arg("-e")
            .arg(include_str!("terminal_presence.swift"))
            .env("THRULINE_DECISION_REASON", reason)
            .stdin(Stdio::inherit())
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
        Duration::from_secs(100),
    )?;
    if status.success() {
        Ok(())
    } else {
        Err(CoreError::Config(
            "macOS user-presence confirmation was not completed".into(),
        ))
    }
}

#[cfg(target_os = "macos")]
fn agent_context() -> bool {
    const MARKERS: &[&str] = &[
        "CODEX_THREAD_ID",
        "CODEX_RUN_ID",
        "CODEX_SANDBOX",
        "CLAUDECODE",
        "CLAUDE_CODE_ENTRYPOINT",
        "OPENAI_AGENT",
        "THRULINE_AGENT_CONTEXT",
        "THRULINE_MODEL_CONTEXT",
        "GITHUB_ACTIONS",
        "CI",
    ];
    if MARKERS.iter().any(|name| std::env::var_os(name).is_some()) {
        return true;
    }
    let mut pid = std::process::id();
    for _ in 0..12 {
        let output = bounded_output(
            Command::new("/bin/ps").args(["-p", &pid.to_string(), "-o", "ppid=", "-o", "comm="]),
            Duration::from_secs(2),
        );
        let Ok(output) = output else { return true };
        if !output.status.success() {
            return true;
        }
        let row = String::from_utf8_lossy(&output.stdout);
        let mut parts = row.split_whitespace();
        let Some(parent) = parts.next().and_then(|part| part.parse::<u32>().ok()) else {
            return true;
        };
        let command = parts.collect::<Vec<_>>().join(" ").to_ascii_lowercase();
        if ["codex", "claude", "mcp", "agent", "chatgpt"]
            .iter()
            .any(|name| command.contains(name))
        {
            return true;
        }
        if parent == 0 || parent == pid {
            break;
        }
        pid = parent;
    }
    false
}

#[cfg(target_os = "macos")]
fn read_tty_answer(tty: &mut fs::File, deadline: Instant) -> Result<String> {
    let flags = rustix::fs::fcntl_getfl(&*tty)
        .map_err(|error| CoreError::Config(format!("terminal flags unavailable: {error}")))?;
    struct RestoreFlags {
        tty: fs::File,
        original: rustix::fs::OFlags,
    }
    impl Drop for RestoreFlags {
        fn drop(&mut self) {
            let _ = rustix::fs::fcntl_setfl(&self.tty, self.original);
        }
    }
    // A duplicate descriptor shares the terminal's file status flags. It lets
    // every return path restore the original mode while `tty` is read mutably.
    let _restore = RestoreFlags {
        tty: tty
            .try_clone()
            .map_err(|error| CoreError::io(Path::new("/dev/tty"), error))?,
        original: flags,
    };
    rustix::fs::fcntl_setfl(&*tty, flags | rustix::fs::OFlags::NONBLOCK).map_err(|error| {
        CoreError::Config(format!("terminal nonblocking mode unavailable: {error}"))
    })?;
    let mut bytes = Vec::new();
    loop {
        if Instant::now() >= deadline {
            return Err(CoreError::Config("decision challenge expired".into()));
        }
        let mut one = [0u8; 1];
        match tty.read(&mut one) {
            Ok(1) if one[0] == b'\n' => break,
            Ok(1) if bytes.len() < 255 => bytes.push(one[0]),
            Ok(1) => return Err(CoreError::Config("terminal answer is too long".into())),
            Ok(_) => return Err(CoreError::Config("terminal answer ended early".into())),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20));
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error) => return Err(CoreError::io(Path::new("/dev/tty"), error)),
        }
    }
    String::from_utf8(bytes).map_err(|_| CoreError::Config("terminal answer is not UTF-8".into()))
}

#[cfg(all(test, target_os = "macos"))]
mod terminal_tests {
    use super::*;

    #[test]
    fn terminal_flags_are_restored_when_an_answer_fails() {
        let mut file = tempfile::tempfile().expect("temporary handle");
        let before = rustix::fs::fcntl_getfl(&file).expect("original flags");
        assert!(read_tty_answer(&mut file, Instant::now() + Duration::from_secs(1)).is_err());
        let after = rustix::fs::fcntl_getfl(&file).expect("restored flags");
        assert_eq!(after, before);
    }
}

#[cfg(target_os = "macos")]
fn bounded_status(command: &mut Command, timeout: Duration) -> Result<std::process::ExitStatus> {
    let mut child = command
        .spawn()
        .map_err(|error| CoreError::Config(format!("OS confirmation could not start: {error}")))?;
    let deadline = Instant::now() + timeout;
    loop {
        match child
            .try_wait()
            .map_err(|error| CoreError::Config(format!("OS confirmation wait failed: {error}")))?
        {
            Some(status) => return Ok(status),
            None if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(CoreError::Config("OS confirmation timed out".into()));
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn bounded_output(command: &mut Command, timeout: Duration) -> Result<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|error| CoreError::Config(format!("OS account check could not start: {error}")))?;
    let deadline = Instant::now() + timeout;
    loop {
        match child
            .try_wait()
            .map_err(|error| CoreError::Config(format!("OS account check wait failed: {error}")))?
        {
            Some(_) => {
                return child.wait_with_output().map_err(|error| {
                    CoreError::Config(format!("OS account check output failed: {error}"))
                })
            }
            None if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(CoreError::Config("OS account check timed out".into()));
            }
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
#[path = "bridge_terminal_tests.rs"]
mod terminal_pty_tests;
