//! M4.3 shape-first census of the renewable temporal classification holdout.
//!
//! No transcript content is printed. The test counts the exact deterministic
//! sentence frontier that production would offer to the local language tier.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use thruline_core::chain::sha256_hex;
use thruline_core::extract::sentence_candidates;
use thruline_core::Ledger;

const FREEZE_EPOCH: i64 = 1_785_460_683;
const FREEZE_RFC3339: &str = "2026-07-31T01:18:03Z";
const GATE_SAMPLES: u64 = 25;

#[derive(Default)]
struct ProjectCount {
    sessions: BTreeSet<String>,
    events: u64,
    candidates: u64,
}

#[path = "support/live_store.rs"]
mod live_store;

fn db_path() -> PathBuf {
    let db = std::env::var_os("THRULINE_DB").map_or_else(
        || PathBuf::from(std::env::var_os("HOME").expect("HOME")).join(".thruline/ledger.db"),
        PathBuf::from,
    );
    live_store::refuse_live_store(&db);
    db
}

#[test]
#[ignore = "requires the private live ledger; emits shape counts only"]
fn temporal_language_partition_shape() {
    let ledger = Ledger::open(db_path()).expect("ledger");
    let qualifying: BTreeSet<String> = {
        let mut stmt = ledger
            .conn()
            .prepare(
                "SELECT s.id
                 FROM sessions s
                 LEFT JOIN events e ON e.session_id = s.id
                 GROUP BY s.id
                 HAVING COALESCE(MIN(julianday(e.ts)), julianday(s.started_at))
                        > julianday(?1)
                 ORDER BY s.id",
            )
            .expect("session census");
        let values = stmt
            .query_map([FREEZE_RFC3339], |row| row.get(0))
            .expect("qualifying sessions")
            .collect::<std::result::Result<_, _>>()
            .expect("session rows");
        values
    };

    let mut counts: BTreeMap<String, ProjectCount> = BTreeMap::new();
    let mut seen_uuids: HashSet<(String, String)> = HashSet::new();
    let mut stmt = ledger
        .conn()
        .prepare(
            "SELECT e.project_id, e.session_id, e.src_uuid, e.text
             FROM events e JOIN sessions s ON s.id = e.session_id
             WHERE e.kind = 'assistant_text' AND e.text IS NOT NULL
             ORDER BY e.project_id, (s.started_at IS NULL), s.started_at, s.id, e.seq, e.id",
        )
        .expect("assistant frontier");
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .expect("assistant rows");
    for row in rows {
        let (project, session, src_uuid, text) = row.expect("assistant row");
        if let Some(uuid) = src_uuid {
            if !seen_uuids.insert((project.clone(), uuid)) {
                continue;
            }
        }
        if !qualifying.contains(&session) {
            continue;
        }
        let entry = counts.entry(project).or_default();
        entry.sessions.insert(session);
        entry.events += 1;
        entry.candidates += sentence_candidates(&text).len() as u64;
    }

    let sessions: usize = counts.values().map(|count| count.sessions.len()).sum();
    let events: u64 = counts.values().map(|count| count.events).sum();
    let candidates: u64 = counts.values().map(|count| count.candidates).sum();
    let manifest = qualifying.iter().cloned().collect::<Vec<_>>().join("\n");
    let manifest_sha256 = sha256_hex(manifest.as_bytes());
    println!(
        "TEMPORAL_CENSUS freeze={FREEZE_RFC3339} freeze_epoch={FREEZE_EPOCH} projects={} sessions={sessions} assistant_events={events} candidates={candidates} gate={GATE_SAMPLES} manifest_sha256={manifest_sha256}",
        counts.len()
    );
    for (project, count) in &counts {
        println!(
            "TEMPORAL_CENSUS_PROJECT project={project} sessions={} assistant_events={} candidates={}",
            count.sessions.len(),
            count.events,
            count.candidates
        );
    }

    if candidates >= GATE_SAMPLES {
        println!("TEMPORAL_MATURITY status=MATURE samples={candidates}");
    } else if candidates == 0 {
        println!("TEMPORAL_MATURITY status=THIN samples=0 projected_epoch=unbounded");
    } else {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_secs() as i64;
        let elapsed = now.saturating_sub(FREEZE_EPOCH).max(1) as f64;
        let projected_epoch =
            FREEZE_EPOCH + ((GATE_SAMPLES as f64 / candidates as f64) * elapsed).ceil() as i64;
        println!(
            "TEMPORAL_MATURITY status=THIN samples={candidates} projected_epoch={projected_epoch}"
        );
    }
}
