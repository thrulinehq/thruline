//! A202 — the window-completeness guard, end to end through the speaker
//! pass: `thruline digest precompute --hook session-end` must refuse to
//! accuse over a window the transcript itself testifies is incomplete, and
//! over a session whose record says it was born from a resume — the exact
//! false-accusation shape the precision gauntlet caught (5 of 24 no-proof
//! lines bound against 0-row windows in resume-born sessions).

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::json;
use tempfile::tempdir;
use thruline_core::Ledger;

const SESSION: &str = "eeeeeeee-6666-4777-8888-999999999999";

fn run_speaker(db: &Path, args: &[&str], stdin: &str) -> bool {
    let mut child = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .args(["--db", db.to_str().expect("db path")])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn thruline");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    child.wait_with_output().expect("wait").status.success()
}

/// A transcript holding one main-thread tool_use and a file claim no edit
/// backs — plus a sidechain tool_use, which must count for nothing.
fn stage(dir: &Path) -> (PathBuf, String) {
    let project_dir = dir.join("projects").join("-repo-app");
    std::fs::create_dir_all(&project_dir).expect("mkdir");
    let transcript = project_dir.join(format!("{SESSION}.jsonl"));
    let lines = [
        json!({
            "type": "user", "uuid": "u1", "isSidechain": false, "sessionId": SESSION,
            "timestamp": "2026-08-31T04:00:01.000Z", "cwd": "/repo",
            "message": {"role": "user", "content": "finish it"},
        }),
        json!({
            "type": "assistant", "uuid": "a1", "isSidechain": false, "sessionId": SESSION,
            "timestamp": "2026-08-31T04:00:02.000Z", "cwd": "/repo",
            "message": {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "cargo test"}}
            ]},
        }),
        json!({
            "type": "assistant", "uuid": "a2", "isSidechain": true, "sessionId": SESSION,
            "timestamp": "2026-08-31T04:00:03.000Z", "cwd": "/repo",
            "message": {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t2", "name": "Bash", "input": {"command": "ls"}}
            ]},
        }),
        json!({
            "type": "assistant", "uuid": "a3", "isSidechain": false, "sessionId": SESSION,
            "timestamp": "2026-08-31T04:00:04.000Z", "cwd": "/repo",
            "message": {"role": "assistant", "content": [
                {"type": "text", "text": "I've added retry logic to `client.rs`."}
            ]},
        }),
    ];
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(&transcript, text).expect("write transcript");
    let payload = json!({
        "session_id": SESSION,
        "transcript_path": transcript.to_str().expect("path"),
        "cwd": "/repo",
        "hook_event_name": "SessionEnd",
    })
    .to_string();
    (transcript, payload)
}

fn append(db: &Path, hook_event: &str, payload: serde_json::Value, at: &str) {
    let ledger = Ledger::open(db).expect("ledger");
    thruline_core::sensor::append(&ledger, hook_event, payload.to_string().as_bytes(), at)
        .expect("append");
}

#[test]
fn the_pass_degrades_gapped_windows_and_leaves_complete_ones_alone() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let (transcript, end_payload) = stage(temp.path());
    let cache_md = temp.path().join("digest").join("-repo-app.md");
    let cache_json = temp.path().join("digest").join("-repo-app.json");

    // Phase 1 — the gauntlet's SEV-1 shape: the transcript shows a
    // main-thread tool event, the session record holds none. Silence.
    assert!(run_speaker(
        &db,
        &["digest", "precompute", "--hook", "session-end"],
        &end_payload,
    ));
    assert!(
        !cache_md.exists(),
        "no accusation over a window the transcript outweighs"
    );

    // Phase 2 — the record catches up (one chained row per main-thread tool
    // event; the sidechain line must not be demanded of it): the window is
    // complete, and the unproven file claim is accused exactly as at A200.
    append(
        &db,
        "PostToolUse",
        json!({
            "session_id": SESSION,
            "transcript_path": transcript.to_str().expect("path"),
            "cwd": "/repo",
            "hook_event_name": "PostToolUse",
            "tool_name": "Bash",
            "tool_input": {"command": "cargo test"},
            "tool_response": {"stdout": "error: test failed", "stderr": "", "interrupted": false},
        }),
        "2026-08-31T04:00:02.500Z",
    );
    assert!(run_speaker(
        &db,
        &["digest", "precompute", "--hook", "session-end"],
        &end_payload,
    ));
    assert!(cache_md.exists(), "a complete window speaks");
    let sidecar: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&cache_json).expect("read"))
            .expect("sidecar json");
    assert_eq!(sidecar["transcript_tool_events"], json!(1));
    assert_eq!(sidecar["resume_started"], json!(false));
    assert!(
        sidecar["lines"]
            .as_array()
            .is_some_and(|l| l.iter().any(|line| line["kind"] == "no_proof")),
        "the A200 verdict stands over a complete window: {sidecar}"
    );

    // Phase 3 — the record then shows this session was born from a resume:
    // the prior id's evidence is unreachable, and the accusation withdraws.
    append(
        &db,
        "SessionStart",
        json!({
            "session_id": SESSION,
            "transcript_path": transcript.to_str().expect("path"),
            "cwd": "/repo",
            "hook_event_name": "SessionStart",
            "source": "resume",
        }),
        "2026-08-31T04:00:00.500Z",
    );
    assert!(run_speaker(
        &db,
        &["digest", "precompute", "--hook", "session-end"],
        &end_payload,
    ));
    assert!(
        !cache_md.exists() && !cache_json.exists(),
        "a resume-born session's window cannot establish absence"
    );
}
