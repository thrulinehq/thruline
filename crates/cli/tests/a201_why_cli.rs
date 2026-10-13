//! A201 — `thruline why <id>` end to end: a finding produced by the real
//! stop audit answers in the terminal with its receipt chain; an id nothing
//! carries is a clear refusal with a nonzero exit.

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::json;
use tempfile::tempdir;

const SESSION: &str = "bbbbbbbb-2222-4333-8444-555555555555";

fn run(db: &Path, args: &[&str], stdin: &str) -> (String, String, bool) {
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
    let out = child.wait_with_output().expect("wait");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// Stage a transcript whose final turn makes one unproven claim, audit it
/// through the real stop verb, and return the typeable id the cache carries.
fn staged_finding_id(dir: &Path, db: &Path) -> String {
    let project_dir = dir.join("projects").join("-repo-app");
    std::fs::create_dir_all(&project_dir).expect("mkdir");
    let transcript = project_dir.join(format!("{SESSION}.jsonl"));
    let lines = [
        json!({
            "type": "user", "uuid": "u1", "isSidechain": false, "sessionId": SESSION,
            "timestamp": "2026-08-31T03:00:01.000Z", "cwd": "/repo",
            "message": {"role": "user", "content": "finish it"},
        }),
        json!({
            "type": "assistant", "uuid": "a1", "isSidechain": false, "sessionId": SESSION,
            "timestamp": "2026-08-31T03:00:02.000Z", "cwd": "/repo",
            "message": {"role": "assistant", "content": [{"type": "text", "text": "All 12 tests pass."}]},
        }),
    ];
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(&transcript, text).expect("write transcript");
    let stop_payload = json!({
        "session_id": SESSION,
        "transcript_path": transcript.to_str().expect("path"),
        "cwd": "/repo",
        "hook_event_name": "Stop",
        "stop_hook_active": false,
    })
    .to_string();
    let (_, stderr, ok) = run(db, &["audit", "--hook", "stop"], &stop_payload);
    assert!(ok, "{stderr}");

    let sidecar: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("digest").join("-repo-app.json")).expect("sidecar"),
    )
    .expect("sidecar json");
    sidecar["lines"][0]["why_id"]
        .as_str()
        .expect("the brief line carries a typeable id")
        .to_string()
}

#[test]
fn why_answers_for_a_finding_the_audit_recorded() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let why_id = staged_finding_id(temp.path(), &db);
    assert!(why_id.starts_with("spkf_"), "{why_id}");

    let (stdout, stderr, ok) = run(&db, &["why", &why_id], "");
    assert!(ok, "{stderr}");
    assert!(
        stdout.contains("claimed, and no proof found in the record"),
        "the only permitted accusation shape: {stdout}"
    );
    assert!(
        stdout.contains("may be entirely true"),
        "the caveat travels with the accusation: {stdout}"
    );
    assert!(
        stdout.contains("the receipt") && stdout.contains(".jsonl:2"),
        "the chain ends at a raw file and line: {stdout}"
    );
    assert!(
        stdout.contains("chain        scope speaker"),
        "the ledger position is part of the receipt: {stdout}"
    );

    // The same id answers as JSON when asked.
    let (stdout, _, ok) = run(&db, &["why", &why_id, "--json"], "");
    assert!(ok);
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).expect("json");
    assert!(v["id"].as_str().is_some_and(|s| s.starts_with("spkf_")));
}

#[test]
fn an_id_nothing_carries_is_a_clear_refusal_with_a_nonzero_exit() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let (stdout, stderr, ok) = run(&db, &["why", "spkf_000000000000"], "");
    assert!(!ok, "an unknown id must not exit 0");
    assert!(stdout.is_empty());
    assert!(
        stderr.contains("nothing on record matches"),
        "the refusal names itself: {stderr}"
    );

    // Same contract for the other namespaces.
    for needle in ["cvrd_000000000000", "tlc_000000000000", "spkp_000000000000"] {
        let (_, stderr, ok) = run(&db, &["why", needle], "");
        assert!(!ok, "{needle} must not exit 0");
        assert!(!stderr.is_empty(), "{needle} must say why");
    }
}
