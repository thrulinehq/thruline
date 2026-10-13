//! C6 — `thruline autopsy` aggregates (A169).
//!
//! The sixty-second wedge: what this machine's own records hold, computed
//! from stored rows, zero configuration, nothing leaving the device. This
//! module is arithmetic only — counts and sums over the ledger. The session
//! -status hypotheses come from the RB1 shadow module, composed at the CLI
//! exactly as `thruline shadow-sessions` does; nothing here types a status,
//! so the RB0 shadow gate is untouched.

use crate::error::Result;
use crate::ledger::Ledger;
use crate::session_status::CONTINUATION_PREFIX;

/// The corpus, by the numbers the header prints.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CorpusStats {
    pub events: u64,
    pub sessions: u64,
    pub projects: u64,
    /// Earliest and latest session start on record, ISO-8601.
    pub first_session: Option<String>,
    pub last_session: Option<String>,
}

pub fn corpus(ledger: &Ledger) -> Result<CorpusStats> {
    let conn = ledger.conn();
    let (events, sessions, projects) = conn.query_row(
        "SELECT (SELECT COUNT(*) FROM events),
                (SELECT COUNT(*) FROM sessions),
                (SELECT COUNT(*) FROM projects)",
        [],
        |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        },
    )?;
    let (first_session, last_session) = conn.query_row(
        "SELECT MIN(started_at), MAX(started_at) FROM sessions WHERE started_at IS NOT NULL",
        [],
        |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
            ))
        },
    )?;
    Ok(CorpusStats {
        events: events.max(0) as u64,
        sessions: sessions.max(0) as u64,
        projects: projects.max(0) as u64,
        first_session,
        last_session,
    })
}

/// Claim counts by the flat sort key — the same classes the §9 export
/// counts. The caller translates slugs to words through the voice authority;
/// no rendered string originates here.
pub fn claim_counts(ledger: &Ledger) -> Result<Vec<(String, u64)>> {
    let conn = ledger.conn();
    let mut stmt =
        conn.prepare("SELECT sort_key, COUNT(*) FROM nodes GROUP BY sort_key ORDER BY sort_key")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (slug, n) = row?;
        out.push((slug, n.max(0) as u64));
    }
    Ok(out)
}

/// The re-explanation tax: sessions that open with the vendor's continuation
/// preamble, and the bytes of preamble retyped. Uses the same inlined literal
/// as the T4 partial index, so it answers from the index's handful of rows
/// rather than a scan of every event text.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Reexplanation {
    pub sessions: u64,
    pub preamble_bytes: u64,
}

pub fn reexplanation(ledger: &Ledger) -> Result<Reexplanation> {
    let conn = ledger.conn();
    let sql = format!(
        "SELECT COUNT(DISTINCT session_id), COALESCE(SUM(LENGTH(text)), 0)
         FROM events WHERE kind='user_msg' AND text LIKE '{CONTINUATION_PREFIX}%'"
    );
    let (sessions, bytes) =
        conn.query_row(&sql, [], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
    Ok(Reexplanation {
        sessions: sessions.max(0) as u64,
        preamble_bytes: bytes.max(0) as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded() -> Ledger {
        let ledger = Ledger::open_in_memory().expect("ledger");
        ledger
            .conn()
            .execute_batch(
                "INSERT INTO projects(id, root_path, source, created_at)
                   VALUES ('p1','/tmp/p1','claude_code','2026-08-12T00:00:00Z');
                 INSERT INTO sessions(id, project_id, file_path, started_at, last_seq)
                   VALUES ('s1','p1','/t/s1.jsonl','2026-05-01T00:00:00Z',1),
                          ('s2','p1','/t/s2.jsonl','2026-08-01T00:00:00Z',0);
                 INSERT INTO events(id, project_id, session_id, seq, ts, kind, text,
                                    raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
                   VALUES ('e1','p1','s1',0,'2026-05-01T00:00:00Z','user_msg','do the thing',
                           '/t/s1.jsonl',1,'x','g','c1'),
                          ('e2','p1','s1',1,'2026-05-01T00:01:00Z','assistant_text','done',
                           '/t/s1.jsonl',2,'x','c1','c2'),
                          ('e3','p1','s2',0,'2026-08-01T00:00:00Z','user_msg',
                           'This session is being continued from a previous conversation…',
                           '/t/s2.jsonl',1,'x','g','c3');
                 INSERT INTO nodes(id, project_id, label, sort_key, lifecycle, evidence_state,
                                   currency, session_id, anchor_event_id, derived_rev)
                   VALUES ('n1','p1','a','supported','completed','supported','current','s1','e2',1),
                          ('n2','p1','b','pending','proposed','unknown','current','s1','e1',1);",
            )
            .expect("seed");
        ledger
    }

    #[test]
    fn the_aggregates_count_what_the_rows_hold() {
        let ledger = seeded();
        let c = corpus(&ledger).expect("corpus");
        assert_eq!((c.events, c.sessions, c.projects), (3, 2, 1));
        assert_eq!(c.first_session.as_deref(), Some("2026-05-01T00:00:00Z"));
        assert_eq!(c.last_session.as_deref(), Some("2026-08-01T00:00:00Z"));
        assert_eq!(
            claim_counts(&ledger).expect("claims"),
            vec![("pending".to_string(), 1), ("supported".to_string(), 1)]
        );
        let r = reexplanation(&ledger).expect("reexplanation");
        assert_eq!(r.sessions, 1);
        assert!(r.preamble_bytes > 0);
    }

    #[test]
    fn an_empty_ledger_reads_as_zeros_not_errors() {
        let ledger = Ledger::open_in_memory().expect("ledger");
        let c = corpus(&ledger).expect("corpus");
        assert_eq!((c.events, c.sessions, c.projects), (0, 0, 0));
        assert!(c.first_session.is_none());
        assert!(claim_counts(&ledger).expect("claims").is_empty());
        assert_eq!(reexplanation(&ledger).expect("reexplanation").sessions, 0);
    }
}
