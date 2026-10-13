//! Read-only hash-chain check for the sessions confirmed for one Change.
//!
//! This checks the recorded event links and their session heads. It does not
//! re-read an agent's source file, verify a saved export, or write the usual
//! `thruline verify` metadata. Those are separate claims.

use super::*;

use rusqlite::OptionalExtension;
use thruline_core::chain::{chain_hash, genesis_prev_hash};
use thruline_core::change_propose::confirmed_sessions;

const PREFIX: &str = "/api/changes/";
const SUFFIX: &str = "/integrity";

pub(super) fn owns(path: &str) -> bool {
    path.strip_prefix(PREFIX)
        .and_then(|rest| rest.strip_suffix(SUFFIX))
        .is_some_and(|id| !id.is_empty() && !id.contains('/'))
}

fn reply(stream: &mut TcpStream, status: u16, value: serde_json::Value) -> Result<()> {
    respond(
        stream,
        status,
        "application/json; charset=utf-8",
        &serde_json::to_vec(&value)?,
    )
}

pub(super) fn handle(
    state: &Arc<State>,
    method: &str,
    path: &str,
    query: &str,
    headers: &RequestHeaders,
    stream: &mut TcpStream,
) -> Result<()> {
    if method != "GET" {
        return reply(
            stream,
            405,
            serde_json::json!({"error": "method_not_allowed"}),
        );
    }
    if !query.is_empty() {
        return reply(
            stream,
            400,
            serde_json::json!({"error": "unknown_query_parameter"}),
        );
    }
    if headers.malformed {
        return reply(
            stream,
            400,
            serde_json::json!({"error": "malformed_request"}),
        );
    }
    let Some(id) = path
        .strip_prefix(PREFIX)
        .and_then(|rest| rest.strip_suffix(SUFFIX))
        .and_then(percent_decode)
        .filter(|id| !id.is_empty() && !id.contains('/'))
    else {
        return reply(
            stream,
            400,
            serde_json::json!({"error": "malformed_request"}),
        );
    };
    let ledger = match Ledger::open_existing_read_only(&state.db) {
        Ok(ledger) => ledger,
        Err(thruline_core::error::CoreError::StoreHeld { .. }) => {
            return super::store_held(state, stream)
        }
        Err(_) => {
            return reply(
                stream,
                503,
                serde_json::json!({"error": "store_unavailable"}),
            )
        }
    };
    let project_id: Option<String> = ledger
        .conn()
        .query_row("SELECT project_id FROM changes WHERE id=?1", [&id], |row| {
            row.get(0)
        })
        .optional()?;
    let Some(project_id) = project_id else {
        return reply(stream, 404, serde_json::json!({"error": "no_such_change"}));
    };
    let checked_at = thruline_core::ingest::iso8601_utc_now();
    let result = check(&ledger, &id, &project_id);
    let (status, sessions_checked, links_checked, reason) = match result {
        Ok(facts) => (facts.state, facts.sessions, facts.links, facts.reason),
        Err(_) => (
            "UNVERIFIABLE",
            0,
            0,
            Some("The record could not be read for this check."),
        ),
    };
    reply(
        stream,
        200,
        serde_json::json!({
            "schema_version": 1,
            "change_id": id,
            "checked_at": checked_at,
            "state": status,
            "sessions_checked": sessions_checked,
            "links_checked": links_checked,
            "reason": reason,
            "scope": "confirmed_change_sessions",
            "source_files_checked": false,
            "saved_export_checked": false
        }),
    )
}

struct Facts {
    state: &'static str,
    sessions: usize,
    links: u64,
    reason: Option<&'static str>,
}

fn check(
    ledger: &Ledger,
    change_id: &str,
    project_id: &str,
) -> thruline_core::error::Result<Facts> {
    let sessions = confirmed_sessions(ledger, change_id)?;
    if sessions.is_empty() {
        return Ok(Facts {
            state: "UNVERIFIABLE",
            sessions: 0,
            links: 0,
            reason: Some("No session has been confirmed for this Change."),
        });
    }
    let mut links = 0u64;
    let mut mismatch = false;
    let mut incomplete = false;
    for session in &sessions {
        let (owner, last_seq, event_count, head): (String, i64, i64, Option<String>) = ledger
            .conn()
            .query_row(
                "SELECT project_id, last_seq, event_count, head_chain_hash FROM sessions WHERE id=?1",
                [session],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
        if owner != project_id {
            mismatch = true;
            continue;
        }
        let stored = ledger.session_chain(session)?;
        let mut prev = genesis_prev_hash(project_id);
        let mut expected_seq = 1i64;
        for link in &stored {
            links += 1;
            if link.seq != expected_seq
                || link.prev_hash != prev
                || chain_hash(&prev, &link.raw_sha256, session, link.seq) != link.chain_hash
            {
                mismatch = true;
            }
            expected_seq = link.seq.saturating_add(1);
            prev = link.chain_hash.clone();
        }
        if stored.is_empty() || head.as_deref().is_none_or(str::is_empty) {
            incomplete = true;
        } else if last_seq != expected_seq - 1
            || event_count != stored.len() as i64
            || head.as_deref() != Some(prev.as_str())
        {
            mismatch = true;
        }
    }
    let (state, reason) = if mismatch {
        (
            "MISMATCH",
            Some("A recorded hash link or session head does not add up."),
        )
    } else if incomplete {
        (
            "UNVERIFIABLE",
            Some("A confirmed session has no complete chain head to check."),
        )
    } else {
        ("INTACT", None)
    };
    Ok(Facts {
        state,
        sessions: sessions.len(),
        links,
        reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use thruline_core::chain::sha256_hex;

    #[test]
    fn confirms_only_standing_sessions_and_detects_a_changed_link() {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("ledger.db");
        let ledger = Ledger::open(&db).unwrap();
        let conn = ledger.conn();
        conn.execute("INSERT INTO projects(id, root_path, source, created_at) VALUES('p', '/synthetic', 'claude_code', 'now')", []).unwrap();
        conn.execute("INSERT INTO sessions(id, project_id, file_path) VALUES('s', 'p', '/synthetic/s.jsonl')", []).unwrap();
        conn.execute("INSERT INTO changes(id, project_id, opened_at, title, purpose, requester, request_source) VALUES('c', 'p', 'now', 'Work', '', 'person', 'person')", []).unwrap();
        conn.execute("INSERT INTO change_session_associations(id, change_id, session_id, provenance, basis, actor, asserted_at) VALUES('a', 'c', 's', 'confirmed', 'user', 'person', 'now')", []).unwrap();
        let none = check(&ledger, "c", "p").unwrap();
        assert_eq!(none.state, "UNVERIFIABLE");
        let prev = genesis_prev_hash("p");
        let raw = sha256_hex(b"one raw line");
        let head = chain_hash(&prev, &raw, "s", 1);
        conn.execute("INSERT INTO events(id, project_id, session_id, seq, kind, raw_file, raw_line, raw_sha256, prev_hash, chain_hash) VALUES('e', 'p', 's', 1, 'user_msg', '/synthetic/s.jsonl', 1, ?1, ?2, ?3)", rusqlite::params![raw, prev, head]).unwrap();
        conn.execute(
            "UPDATE sessions SET last_seq=1, event_count=1, head_chain_hash=?1 WHERE id='s'",
            [&head],
        )
        .unwrap();
        let intact = check(&ledger, "c", "p").unwrap();
        assert_eq!(intact.state, "INTACT");
        assert_eq!(intact.links, 1);
        conn.execute("UPDATE sessions SET head_chain_hash='bad' WHERE id='s'", [])
            .unwrap();
        assert_eq!(check(&ledger, "c", "p").unwrap().state, "MISMATCH");
    }
}
