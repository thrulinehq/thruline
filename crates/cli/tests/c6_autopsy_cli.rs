//! C6 — the autopsy's process-level contract (A169).
//!
//! Three properties: an empty ledger reads as an honest empty state, never a
//! wall of zeros; the privacy line is printed, not implied — "nothing leaves
//! this device" is the wedge's promise and it appears on the artifact itself;
//! and every line a person sees passes the §5v ban.

use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

fn autopsy(db: &Path) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .args(["--db", db.to_str().unwrap(), "autopsy"])
        .output()
        .expect("run autopsy");
    assert!(out.status.success(), "{out:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn r1_the_autopsy_shadow_line_carries_counts_under_a_heading_and_no_identity() {
    // §5s-amendment (R1) made this line lawful under written law rather than
    // under a defensible reading. The conditions are now testable, so they are
    // tested: an explicit ungraduated heading, and no per-claim identity.
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    {
        let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
        ledger
            .conn()
            .execute_batch(
                "INSERT INTO projects(id, root_path, source, created_at)
                   VALUES ('p1','/tmp/p1','claude_code','2026-08-13T00:00:00Z');
                 INSERT INTO sessions(id, project_id, file_path, started_at, last_seq)
                   VALUES ('s1','p1','/t/s1.jsonl','2026-05-01T00:00:00Z',0);
                 INSERT INTO source_sessions(session_id, project_id, owner_key, source, source_session_key)
                   VALUES ('s1','p1','owner','claude_code','s1');
                 INSERT INTO events(id, project_id, session_id, seq, ts, kind, text,
                                    raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
                   VALUES ('e1','p1','s1',0,'2026-05-01T00:00:00Z','meta','last-prompt',
                           '/t/s1.jsonl',1,'x','g','c1');
                 INSERT INTO nodes(id, project_id, label, sort_key, lifecycle, evidence_state,
                                   currency, session_id, anchor_event_id, derived_rev)
                   VALUES ('n9','p1','a claim nobody proved','unsupported','completed',
                           'unknown','current','s1','e1',1);",
            )
            .expect("seed");
    }
    let out = autopsy(&db);
    let shadow_line = out
        .lines()
        .find(|l| l.contains("no proof found"))
        .expect("the shadow count renders");
    assert!(
        shadow_line.contains("not yet graduated"),
        "the shadow count lost its ungraduated heading: {shadow_line}"
    );
    assert!(
        !out.contains("a claim nobody proved") && !out.contains("n9"),
        "an ungraduated claim was named on a default surface: {out}"
    );
}

#[test]
fn an_empty_ledger_reads_as_an_honest_empty_state() {
    let temp = tempdir().expect("tempdir");
    let out = autopsy(&temp.path().join("ledger.db"));
    assert!(
        out.contains("No records on this machine yet"),
        "empty state should be a sentence, not zeros: {out}"
    );
    assert!(out.contains("thruline ingest"), "the remedy is named");
    for line in out.lines() {
        assert_eq!(
            thruline_core::voice::banned_in_output(line),
            None,
            "engine lexicon reached the autopsy: {line:?}"
        );
    }
}

#[test]
fn a_seeded_ledger_prints_the_reading_with_the_privacy_line() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    {
        let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
        ledger
            .conn()
            .execute_batch(
                "INSERT INTO projects(id, root_path, source, created_at)
                   VALUES ('p1','/tmp/p1','claude_code','2026-08-12T00:00:00Z');
                 INSERT INTO sessions(id, project_id, file_path, started_at, last_seq)
                   VALUES ('s1','p1','/t/s1.jsonl','2026-05-01T00:00:00Z',1);
                 INSERT INTO source_sessions(session_id, project_id, owner_key, source, source_session_key)
                   VALUES ('s1','p1','owner','claude_code','s1');
                 INSERT INTO events(id, project_id, session_id, seq, ts, kind, text,
                                    raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
                   VALUES ('e1','p1','s1',0,'2026-05-01T00:00:00Z','user_msg','do it',
                           '/t/s1.jsonl',1,'x','g','c1'),
                          ('e2','p1','s1',1,'2026-05-01T00:01:00Z','meta','last-prompt',
                           '/t/s1.jsonl',2,'x','c1','c2');
                 INSERT INTO nodes(id, project_id, label, sort_key, lifecycle, evidence_state,
                                   currency, session_id, anchor_event_id, derived_rev)
                   VALUES ('n1','p1','a','supported','completed','supported','current','s1','e2',1),
                          ('n2','p1','b','unknown','active','unknown','current','s1','e1',1);",
            )
            .expect("seed");
    }
    let out = autopsy(&db);
    assert!(
        out.contains("Nothing leaves this device."),
        "the privacy line is part of the artifact: {out}"
    );
    assert!(out.contains("THRULINE AUTOPSY"), "the header names it");
    assert!(
        out.contains("1 session closed clean on the vendor's own marker"),
        "the vendor-close hypothesis line renders with its noun: {out}"
    );
    assert!(
        out.contains("1 supported") && out.contains("1 insufficient evidence"),
        "claim counts speak the amended vocabulary: {out}"
    );
    assert!(
        out.contains("hypotheses"),
        "the session-end section carries the shadow discipline in prose"
    );
    for line in out.lines() {
        assert_eq!(
            thruline_core::voice::banned_in_output(line),
            None,
            "engine lexicon reached the autopsy: {line:?}"
        );
    }
}
