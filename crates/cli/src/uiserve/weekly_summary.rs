//! C51: one observational seven-day reading for the open HQ page. The same
//! speaker finding IDs feed local notification dedup; no claim text, path or
//! project key is served. A GET opens the existing ledger read-only and adds
//! no table, migration, or record row.

use super::*;
use rusqlite::params;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) const PATH: &str = "/api/hq/weekly-summary";
pub(super) const FINDINGS_PATH: &str = "/api/hq/not-backed";
const FINDINGS_PAGE: i64 = 256;

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
    if path == PATH && !query.is_empty() {
        return reply(
            stream,
            400,
            serde_json::json!({"error": "unknown_query_parameter"}),
        );
    }
    if headers.malformed
        || !headers.content_lengths.is_empty()
        || !headers.content_types.is_empty()
        || headers.transfer_encoding
    {
        return reply(
            stream,
            400,
            serde_json::json!({"error": "malformed_request"}),
        );
    }
    let after = if path == FINDINGS_PATH && !query.is_empty() {
        match query
            .strip_prefix("after=")
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|v| *v >= -1)
        {
            Some(after) => Some(after),
            None => {
                return reply(
                    stream,
                    400,
                    serde_json::json!({"error": "unknown_query_parameter"}),
                )
            }
        }
    } else {
        None
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
    if path == FINDINGS_PATH {
        let (latest_seq, next_seq, items) = findings_after(ledger.conn(), after)?;
        return reply(
            stream,
            200,
            serde_json::json!({
                "schema_version": 1,
                "latest_seq": latest_seq,
                "next_seq": next_seq,
                "finding_items": items,
            }),
        );
    }
    let now_secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    let now = thruline_core::ingest::iso8601_utc(now_secs);
    let since = thruline_core::ingest::iso8601_utc(now_secs - 7 * 86_400);
    let summary = read(ledger.conn(), &since, &now)?;
    reply(
        stream,
        200,
        serde_json::json!({
            "schema_version": 1,
            "since": since,
            "through": now,
            "approvals": summary.approvals,
            "not_backed": summary.findings.len(),
            "rechecks": summary.rechecks,
            "projects_recorded": summary.projects_recorded,
            "finding_items": summary.findings,
        }),
    )
}

#[derive(serde::Serialize)]
struct Finding {
    id: String,
    recorded_at: String,
}

fn findings_after(
    conn: &rusqlite::Connection,
    after: Option<i64>,
) -> Result<(i64, i64, Vec<Finding>)> {
    let latest: i64 = conn.query_row(
        "SELECT COALESCE(MAX(seq), -1) FROM speaker_findings",
        [],
        |row| row.get(0),
    )?;
    // A missing cursor is the first open of this page: baseline existing
    // findings without sending a backlog. Every later read uses the unique
    // seq index and returns at most one small page.
    let Some(after) = after else {
        return Ok((latest, latest, Vec::new()));
    };
    let mut stmt = conn.prepare(
        "SELECT seq, id, recorded_at FROM speaker_findings WHERE seq>?1 ORDER BY seq LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![after, FINDINGS_PAGE], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                Finding {
                    id: row.get(1)?,
                    recorded_at: row.get(2)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let next = rows.last().map(|r| r.0).unwrap_or(latest.max(after));
    Ok((latest, next, rows.into_iter().map(|r| r.1).collect()))
}

struct Summary {
    approvals: i64,
    findings: Vec<Finding>,
    rechecks: i64,
    projects_recorded: i64,
}

fn read(conn: &rusqlite::Connection, since: &str, through: &str) -> Result<Summary> {
    // datetime() parses the timestamp offsets and fractional seconds written
    // by each capture source; the inclusive endpoints are from this GET's one
    // clock reading. One run can have multiple phase rows, so count its key.
    let approvals = conn.query_row(
        "SELECT COUNT(*) FROM change_decisions
         WHERE act='approved' AND datetime(decided_at) BETWEEN datetime(?1) AND datetime(?2)",
        params![since, through],
        |row| row.get(0),
    )?;
    let rechecks = conn.query_row(
        "SELECT COUNT(DISTINCT run_key) FROM change_run_executions
         WHERE change_id IS NOT NULL AND state='running'
           AND datetime(COALESCE(started_at, recorded_at)) BETWEEN datetime(?1) AND datetime(?2)",
        params![since, through],
        |row| row.get(0),
    )?;
    // HQ folds source-specific ledger projects under their recorded root.
    let projects_recorded = conn.query_row(
        "SELECT COUNT(DISTINCT p.root_path) FROM events e
         JOIN projects p ON p.id=e.project_id
         WHERE datetime(e.ts) BETWEEN datetime(?1) AND datetime(?2)",
        params![since, through],
        |row| row.get(0),
    )?;
    let mut stmt = conn.prepare(
        "SELECT id, recorded_at FROM speaker_findings
         WHERE datetime(recorded_at) BETWEEN datetime(?1) AND datetime(?2)
         ORDER BY recorded_at, seq",
    )?;
    let findings = stmt
        .query_map(params![since, through], |row| {
            Ok(Finding {
                id: row.get(0)?,
                recorded_at: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Summary {
        approvals,
        findings,
        rechecks,
        projects_recorded,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seven_day_read_counts_distinct_runs_and_roots_and_no_future_rows() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE change_decisions(act TEXT, decided_at TEXT);
            CREATE TABLE change_run_executions(change_id TEXT, run_key TEXT, state TEXT, started_at TEXT, recorded_at TEXT);
            CREATE TABLE projects(id TEXT, root_path TEXT);
            CREATE TABLE events(project_id TEXT, ts TEXT);
            CREATE TABLE speaker_findings(id TEXT, recorded_at TEXT, seq INTEGER);
            INSERT INTO change_decisions VALUES ('approved','2026-10-08T10:00:00Z'),('withdrawn','2026-10-08T10:01:00Z'),('approved','2026-09-30T10:00:00Z');
            INSERT INTO change_run_executions VALUES ('c1','run-1','running','2026-10-08T10:00:00Z','2026-10-08T10:00:00Z'),('c1','run-1','running','2026-10-08T10:01:00Z','2026-10-08T10:01:00Z'),('c1','run-2','blocked',NULL,'2026-10-08T10:00:00Z'),('c1','run-old','running','2026-09-30T10:00:00Z','2026-09-30T10:00:00Z');
            INSERT INTO projects VALUES ('p1','/work/a'),('p2','/work/a'),('p3','/work/b');
            INSERT INTO events VALUES ('p1','2026-10-08T10:00:00Z'),('p2','2026-10-08T10:01:00Z'),('p3','2026-09-30T10:00:00Z');
            INSERT INTO speaker_findings VALUES ('f1','2026-10-08T10:00:00Z',1),('f0','2026-09-30T10:00:00Z',0);")
            .unwrap();
        let s = read(&conn, "2026-10-02T11:00:00Z", "2026-10-09T11:00:00Z").unwrap();
        assert_eq!(s.approvals, 1);
        assert_eq!(s.rechecks, 1);
        assert_eq!(s.projects_recorded, 1);
        assert_eq!(s.findings.len(), 1);
        assert_eq!(s.findings[0].id, "f1");
        let (latest, next, items) = findings_after(&conn, None).unwrap();
        assert_eq!(
            (latest, next, items.len()),
            (1, 1, 0),
            "first open baselines"
        );
        let (_, next, items) = findings_after(&conn, Some(0)).unwrap();
        assert_eq!(next, 1);
        assert_eq!(
            items.into_iter().map(|item| item.id).collect::<Vec<_>>(),
            vec!["f1"]
        );
    }
}
