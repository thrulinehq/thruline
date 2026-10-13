//! A201 Task 0 — the spill guard.
//!
//! 962 real deliveries have spilled to `sensor-spill.jsonl` (3 s busy
//! timeouts against long annotate transactions) and are NOT on the sensor
//! chain. The digest and audit bind claims against chained rows only, so a
//! spilled PostToolUse could hold the very proof a no-proof line would deny —
//! a false-accusation vector. The guard is deterministic: any spilled
//! delivery naming the session under audit degrades every would-be
//! no-proof-found verdict to silence, and the digest records the reason.
//!
//! The fail-open asymmetry is deliberate and tested from both sides: a
//! missing or unreadable spill file is zero spills KNOWN (the chained scan
//! still runs; fail-open must not become fail-accuse), while a readable file
//! naming this session always silences the accusation.

use std::path::{Path, PathBuf};

use serde_json::json;
use tempfile::tempdir;
use thruline_core::sensor::{spill_path, spilled_for_session};
use thruline_core::speaker::{self, bind, compose, final_turn, Binding, EvidenceIndex};
use thruline_core::Ledger;

const SESSION: &str = "aaaaaaaa-1111-4222-8333-444444444444";
const OTHER_SESSION: &str = "dddddddd-5555-4666-8777-888888888888";
const TRANSCRIPT: &str =
    "/home/dev/.claude/projects/-repo-app/aaaaaaaa-1111-4222-8333-444444444444.jsonl";

fn user(index: usize, text: &str) -> serde_json::Value {
    json!({
        "type": "user",
        "uuid": format!("00000000-0000-4000-8000-{index:012}"),
        "isSidechain": false,
        "sessionId": SESSION,
        "timestamp": format!("2026-08-31T02:00:{index:02}.000Z"),
        "cwd": "/repo",
        "message": {"role": "user", "content": text},
    })
}

fn assistant_text(index: usize, text: &str) -> serde_json::Value {
    json!({
        "type": "assistant",
        "uuid": format!("00000000-0000-4000-8000-{index:012}"),
        "isSidechain": false,
        "sessionId": SESSION,
        "timestamp": format!("2026-08-31T02:00:{index:02}.000Z"),
        "cwd": "/repo",
        "message": {"role": "assistant", "content": [{"type": "text", "text": text}]},
    })
}

/// A transcript whose final turn claims a test run the chained record does
/// not hold — the would-be no-proof case every test here starts from.
fn unproven_claim_transcript(dir: &Path) -> PathBuf {
    let path = dir.join("session.jsonl");
    let lines = [user(1, "run them"), assistant_text(2, "All 12 tests pass.")];
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(&path, text).expect("write transcript");
    path
}

/// One spill line in the CLI writer's exact shape: the vendor payload
/// embedded verbatim as a string.
fn spill_line(session: &str) -> String {
    json!({
        "received_at": "2026-08-31T02:01:00.000Z",
        "hook_event": "PostToolUse",
        "error": "database is locked",
        "payload": json!({
            "session_id": session,
            "transcript_path": TRANSCRIPT,
            "cwd": "/repo",
            "hook_event_name": "PostToolUse",
            "tool_name": "Bash",
            "tool_input": {"command": "cargo test"},
            "tool_response": {"stdout": "test result: ok. 12 passed; 0 failed", "stderr": ""},
        }).to_string(),
    })
    .to_string()
}

fn idx_with_spills(spilled: u64) -> EvidenceIndex {
    let mut idx = EvidenceIndex::build(&[]);
    idx.spilled_deliveries = spilled;
    idx
}

/* ── counting the spill file ─────────────────────────────────────────── */

#[test]
fn the_spill_path_derives_from_the_db_path() {
    assert_eq!(
        spill_path(Path::new("/home/dev/.thruline/ledger.db")),
        PathBuf::from("/home/dev/.thruline/sensor-spill.jsonl")
    );
}

#[test]
fn spills_are_counted_per_session_and_defects_count_for_nobody() {
    let temp = tempdir().expect("tempdir");
    let spill = temp.path().join("sensor-spill.jsonl");
    let text = [
        spill_line(SESSION),
        spill_line(OTHER_SESSION),
        spill_line(SESSION),
        "not json at all".to_string(),
        json!({"received_at": "x", "hook_event": "PostToolUse", "error": "e",
               "payload": "also not json"})
        .to_string(),
        json!({"received_at": "x", "hook_event": "PostToolUse", "error": "e",
               "payload": json!({"no_session": true}).to_string()})
        .to_string(),
    ]
    .join("\n");
    std::fs::write(&spill, text).expect("write spill");
    assert_eq!(spilled_for_session(&spill, SESSION), 2);
    assert_eq!(spilled_for_session(&spill, OTHER_SESSION), 1);
    assert_eq!(
        spilled_for_session(&spill, "eeeeeeee-9999-4000-8000-000000000000"),
        0,
        "a session nothing spilled for reads clean"
    );
}

#[test]
fn a_missing_or_unreadable_spill_file_is_zero_spills_known() {
    let temp = tempdir().expect("tempdir");
    assert_eq!(
        spilled_for_session(&temp.path().join("does-not-exist.jsonl"), SESSION),
        0,
        "missing file: no spills known, the scan proceeds"
    );
    let dir_as_file = temp.path().join("a-directory");
    std::fs::create_dir_all(&dir_as_file).expect("mkdir");
    assert_eq!(
        spilled_for_session(&dir_as_file, SESSION),
        0,
        "unreadable path: no spills known, the scan proceeds"
    );
}

/* ── the guard on the binder ─────────────────────────────────────────── */

#[test]
fn a_spill_for_this_session_degrades_no_proof_to_silence_and_records_why() {
    let temp = tempdir().expect("tempdir");
    let turn = final_turn(&unproven_claim_transcript(temp.path())).expect("final turn");

    // Without the guard this exact claim is a no-proof line.
    assert!(matches!(
        bind(&turn.claims[0], &idx_with_spills(0)),
        Binding::NoProofFound { .. }
    ));

    // With one spilled delivery for the session, the verdict is silence…
    let idx = idx_with_spills(1);
    assert_eq!(bind(&turn.claims[0], &idx), Binding::Unknown);

    // …and the digest records the reason instead of a line.
    let digest = compose(
        &speaker::DigestScope {
            project_key: "-repo-app",
            session_id: SESSION,
            transcript_path: TRANSCRIPT,
            generated_at: "2026-08-31T02:02:00.000Z",
            hook: "stop",
        },
        &turn,
        &idx,
        &[],
    );
    assert!(
        digest.lines.iter().all(|l| l.kind != "no_proof"),
        "no accusation over an incomplete window: {:?}",
        digest.lines
    );
    assert_eq!(digest.spilled_deliveries, 1);
    assert_eq!(
        digest.evidence_window_note.as_deref(),
        Some("evidence window incomplete: 1 spilled delivery")
    );
}

#[test]
fn a_spill_for_a_different_session_changes_no_verdict() {
    let temp = tempdir().expect("tempdir");
    let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    let spill = temp.path().join("sensor-spill.jsonl");
    std::fs::write(&spill, spill_line(OTHER_SESSION)).expect("write spill");

    let turn = final_turn(&unproven_claim_transcript(temp.path())).expect("final turn");
    let rows = speaker::session_post_tool_rows(&ledger, SESSION).expect("rows");
    let mut idx = EvidenceIndex::build(&rows);
    idx.spilled_deliveries = spilled_for_session(&spill, SESSION);
    assert_eq!(
        idx.spilled_deliveries, 0,
        "the other session's spill is not ours"
    );
    assert!(
        matches!(bind(&turn.claims[0], &idx), Binding::NoProofFound { .. }),
        "verdicts are normal when nothing spilled for this session"
    );
}

#[test]
fn evidenced_claims_stay_evidenced_under_spills() {
    // A spilled delivery can only HIDE evidence, never create it: the guard
    // must not turn a proven claim into silence-with-a-question-mark.
    let temp = tempdir().expect("tempdir");
    let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    let payload = json!({
        "session_id": SESSION,
        "transcript_path": TRANSCRIPT,
        "cwd": "/repo",
        "hook_event_name": "PostToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": "cargo test"},
        "tool_response": {"stdout": "test result: ok. 12 passed; 0 failed", "stderr": "", "interrupted": false},
    });
    thruline_core::sensor::append(
        &ledger,
        "PostToolUse",
        payload.to_string().as_bytes(),
        "2026-08-31T02:01:00.000Z",
    )
    .expect("append");

    let turn = final_turn(&unproven_claim_transcript(temp.path())).expect("final turn");
    let rows = speaker::session_post_tool_rows(&ledger, SESSION).expect("rows");
    let mut idx = EvidenceIndex::build(&rows);
    idx.spilled_deliveries = 3;
    assert_eq!(bind(&turn.claims[0], &idx), Binding::Evidenced);
}
