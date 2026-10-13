//! C27: a Codex recording that is still being written, and has moved since
//! its capture, is refused BEFORE its mapping — with exactly the refusal the
//! final pre-commit check records after the mapping, and nothing else
//! different. The seam appends a line to the file right after its capture, as
//! a live writer would; one store runs with the precheck, one without.

mod scratch;

use std::io::Write as _;
use std::path::{Path, PathBuf};

use scratch::TempTree;
use thruline_core::ingest::{ingest_codex_with_live_seam, verify, CodexIngestOptions};
use thruline_core::Ledger;

fn line(i: usize) -> String {
    serde_json::json!({
        "timestamp": format!("2026-10-04T08:00:{:02}.000Z", i % 60), "type": "response_item",
        "payload": {"type": "message", "id": format!("m{i}"), "role": if i.is_multiple_of(2) { "user" } else { "assistant" },
                    "content": [{"type": if i.is_multiple_of(2) { "input_text" } else { "output_text" },
                                 "text": format!("Message {i}.")}]}
    })
    .to_string()
        + "\n"
}

fn rollout(dir: &Path) -> PathBuf {
    let path = dir.join("src/rollout-2026-10-04T08-00-00-live.jsonl");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
    let mut body = serde_json::json!({
        "timestamp": "2026-10-04T08:00:00.000Z", "type": "session_meta",
        "payload": {"id": "codex-live-1", "cwd": "/work/app", "source": "vscode", "originator": "Codex Desktop"}
    })
    .to_string()
        + "\n";
    // More than 1,024 lines, so a mapping is asked at least once whether
    // its source has moved.
    body.extend((0..1500).map(line));
    std::fs::write(&path, body).expect("write");
    path
}

/// Every deterministic row a refusal or an acceptance can leave: refusals
/// (without their wall-clock instant), generations, sessions and events.
fn rows(db: &Path) -> Vec<String> {
    let ledger = Ledger::open(db).expect("open");
    let conn = ledger.conn();
    let mut out = Vec::new();
    for sql in [
        "SELECT id || '|' || status || '|' || reason || '|' || COALESCE(candidate_sha256,'') || '|' || COALESCE(raw_envelopes,'') || '|' || COALESCE(predecessor_generation_id,'') FROM candidate_assessments ORDER BY id",
        "SELECT payload_sha256 || '|' || payload_bytes || '|' || total_envelopes FROM session_generations ORDER BY id",
        "SELECT id || '|' || raw_line_count || '|' || COALESCE(raw_digest,'') || '|' || event_count FROM sessions ORDER BY id",
        "SELECT session_id || '|' || seq || '|' || raw_sha256 || '|' || chain_hash FROM events ORDER BY session_id, seq",
    ] {
        let mut stmt = conn.prepare(sql).expect("prepare");
        let found: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .expect("query")
            .map(|r| r.expect("row"))
            .collect();
        out.extend(found);
    }
    out
}

fn run(before: bool, during: bool) -> (thruline_core::ingest::IngestReport, Vec<String>) {
    let tmp = TempTree::new();
    let file = rollout(tmp.path());
    let db = tmp.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    let options = CodexIngestOptions {
        mirror_root: tmp.path().join("mirrors"),
        owner_key: Some("live-test-owner".to_string()),
    };
    // A live writer: one more line lands after the capture.
    let append = |p: &Path| {
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(p)
            .expect("open");
        f.write_all(line(1500).as_bytes()).expect("append");
    };
    let report =
        ingest_codex_with_live_seam(&mut ledger, &[file], &options, &append, before, during)
            .expect("ingest");
    let v = verify(&ledger, None).expect("verify");
    assert!(v.ok(), "verify after a refused live candidate: {v:?}");
    drop(ledger);
    (report, rows(&db))
}

fn assert_same(
    (with, with_rows): &(thruline_core::ingest::IngestReport, Vec<String>),
    (without, without_rows): &(thruline_core::ingest::IngestReport, Vec<String>),
) {
    assert_eq!(with.live_source_prechecks, 1, "{with:?}");
    assert_eq!(without.live_source_prechecks, 0);
    assert_eq!(
        with.candidate_statuses, without.candidate_statuses,
        "the same refusal"
    );
    assert_eq!(
        with.candidate_statuses.get("capture_failed"),
        Some(&1),
        "{with:?}"
    );
    assert_eq!(
        (with.new_events, with.lines_read, with.quarantined_envelopes),
        (
            without.new_events,
            without.lines_read,
            without.quarantined_envelopes
        )
    );
    assert!(
        with_rows.iter().any(|r| r.contains("capture_failed")),
        "{with_rows:?}"
    );
    assert_eq!(with_rows, without_rows, "the same rows");
}

#[test]
fn a_live_source_that_moved_is_refused_before_mapping_with_the_same_refusal() {
    assert_same(&run(true, false), &run(false, false));
}

#[test]
fn a_live_source_that_moves_during_mapping_stops_it_with_the_same_refusal() {
    // No check before the mapping: only the watch inside it can see the move.
    assert_same(&run(false, true), &run(false, false));
}
