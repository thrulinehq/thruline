//! A200 — the speaker verbs end to end: `digest precompute --hook
//! session-end`, `brief --hook session-start`, `audit --hook stop`.
//!
//! The contract under test: speakers read a hook payload from stdin, put
//! receipt-backed words where the loop needs them (the digest cache, the
//! SessionStart additionalContext, the `speaker_findings` chain), and are
//! fail-open in every other respect — any error or missing datum exits 0
//! having printed nothing, because a speaker must never cost a session its
//! hook budget or its attention.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::json;
use tempfile::tempdir;
use thruline_core::Ledger;

const SESSION: &str = "bbbbbbbb-2222-4333-8444-555555555555";

/// Run the binary with a stdin payload; return (stdout, stderr, success).
fn run_speaker(db: &Path, args: &[&str], stdin: &str) -> (String, String, bool) {
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

/// A project directory shaped like the vendor's, one transcript inside.
fn stage(dir: &Path, assistant_final: &str) -> (PathBuf, String) {
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
            "message": {"role": "assistant", "content": [{"type": "text", "text": assistant_final}]},
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

/// A PostToolUse row the recognizer CANNOT account for, of a class that cannot
/// hide a test run: an edit-family row whose payload names no path.
fn seed_pathless_edit_row(db: &Path, transcript: &Path) {
    let ledger = Ledger::open(db).expect("ledger");
    let payload = json!({
        "session_id": SESSION,
        "transcript_path": transcript.to_str().expect("path"),
        "cwd": "/repo",
        "hook_event_name": "PostToolUse",
        "tool_name": "Edit",
        // No file_path/notebook_path/path anywhere: unaccountable by design.
        "tool_input": {"old_string": "a", "new_string": "b"},
        "tool_response": {},
    });
    thruline_core::sensor::append(
        &ledger,
        "PostToolUse",
        payload.to_string().as_bytes(),
        "2026-08-31T03:00:01.400Z",
    )
    .expect("append");
}

fn seed_edit_row(db: &Path, transcript: &Path) {
    let ledger = Ledger::open(db).expect("ledger");
    let payload = json!({
        "session_id": SESSION,
        "transcript_path": transcript.to_str().expect("path"),
        "cwd": "/repo",
        "hook_event_name": "PostToolUse",
        "tool_name": "Edit",
        "tool_input": {"file_path": "/repo/src/client.rs"},
        "tool_response": {"filePath": "/repo/src/client.rs"},
    });
    thruline_core::sensor::append(
        &ledger,
        "PostToolUse",
        payload.to_string().as_bytes(),
        "2026-08-31T03:00:01.500Z",
    )
    .expect("append");
}

#[test]
fn precompute_writes_the_cache_and_brief_injects_it_with_receipts() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let (transcript, end_payload) = stage(
        temp.path(),
        "All 12 tests pass. I'll wire the panel into `layout.rs` next.",
    );
    seed_edit_row(&db, &transcript);

    // SessionEnd: silent, exit 0, cache written.
    let (stdout, stderr, ok) = run_speaker(
        &db,
        &["digest", "precompute", "--hook", "session-end"],
        &end_payload,
    );
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "", "the SessionEnd speaker prints nothing");
    let cache_md = temp.path().join("digest").join("-repo-app.md");
    let cache_json = temp.path().join("digest").join("-repo-app.json");
    assert!(cache_md.exists(), "the rendered cache exists");
    assert!(cache_json.exists(), "the receipts sidecar exists");
    let sidecar: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&cache_json).expect("read"))
            .expect("sidecar json");
    assert_eq!(sidecar["schema"], "thruline_digest_v0");
    assert_eq!(sidecar["session_id"], SESSION);
    assert!(
        sidecar["lines"]
            .as_array()
            .is_some_and(|l| !l.is_empty() && l.len() <= 3),
        "at most three lines: {sidecar}"
    );
    for line in sidecar["lines"].as_array().expect("lines") {
        assert!(
            line["transcript_line"].as_u64().is_some(),
            "every line carries its receipt: {line}"
        );
    }

    // SessionStart: valid hookSpecificOutput JSON with the cached lines.
    let start_payload = json!({
        "session_id": "cccccccc-3333-4444-8555-666666666666",
        "transcript_path": transcript
            .with_file_name("cccccccc-3333-4444-8555-666666666666.jsonl")
            .to_str()
            .expect("path"),
        "cwd": "/repo",
        "hook_event_name": "SessionStart",
        "source": "startup",
    })
    .to_string();
    let (stdout, stderr, ok) =
        run_speaker(&db, &["brief", "--hook", "session-start"], &start_payload);
    assert!(ok, "{stderr}");
    let out: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid JSON");
    assert_eq!(out["hookSpecificOutput"]["hookEventName"], "SessionStart");
    let context = out["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("additionalContext");
    assert!(
        context.contains("claimed, and no proof found in the record"),
        "the honest form, verbatim: {context}"
    );
    assert!(
        context.contains("receipt") && context.contains(".jsonl:"),
        "every line traces to a file and line: {context}"
    );
    assert!(
        context.len() <= 1_600,
        "hard token budget: {} bytes",
        context.len()
    );
    for line in context.lines() {
        assert_eq!(
            thruline_core::voice::banned_in_output(line),
            None,
            "engine lexicon reached the brief: {line:?}"
        );
    }
}

#[test]
fn brief_with_no_cache_prints_nothing_and_exits_zero() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let payload = json!({
        "session_id": SESSION,
        "transcript_path": "/nonexistent/projects/-repo-app/x.jsonl",
        "cwd": "/repo",
        "hook_event_name": "SessionStart",
    })
    .to_string();
    let (stdout, stderr, ok) = run_speaker(&db, &["brief", "--hook", "session-start"], &payload);
    assert!(ok);
    assert_eq!(stdout, "", "healthy runs are invisible");
    assert_eq!(stderr, "", "and silent");
}

#[test]
fn a_clean_session_end_clears_the_cache_and_the_next_brief_is_silent() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");

    // First: a no-proof end leaves a cache.
    let (_transcript, end_payload) = stage(temp.path(), "All 12 tests pass.");
    let (_, _, ok) = run_speaker(
        &db,
        &["digest", "precompute", "--hook", "session-end"],
        &end_payload,
    );
    assert!(ok);
    assert!(temp.path().join("digest").join("-repo-app.md").exists());

    // Then: the session's record now backs the claim — cache cleared.
    let (transcript2, end_payload2) = stage(temp.path(), "All 12 tests pass.");
    let ledger = Ledger::open(&db).expect("ledger");
    let payload = json!({
        "session_id": SESSION,
        "transcript_path": transcript2.to_str().expect("path"),
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
        "2026-08-31T03:10:00.000Z",
    )
    .expect("append");
    drop(ledger);
    let (_, _, ok) = run_speaker(
        &db,
        &["digest", "precompute", "--hook", "session-end"],
        &end_payload2,
    );
    assert!(ok);
    assert!(
        !temp.path().join("digest").join("-repo-app.md").exists(),
        "nothing to say means an absent cache"
    );
}

#[test]
fn audit_announces_to_the_ledger_idempotently_and_never_speaks() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let (_transcript, end_payload) = stage(temp.path(), "All 12 tests pass.");
    let stop_payload = {
        let mut v: serde_json::Value = serde_json::from_str(&end_payload).expect("payload");
        v["hook_event_name"] = json!("Stop");
        v["stop_hook_active"] = json!(false);
        v.to_string()
    };

    let (stdout, stderr, ok) = run_speaker(&db, &["audit", "--hook", "stop"], &stop_payload);
    assert!(ok, "{stderr}");
    assert_eq!(
        stdout, "",
        "v0 announces to the ledger, never to the session"
    );
    assert_eq!(stderr, "");

    let ledger = Ledger::open(&db).expect("ledger");
    let count: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM speaker_findings", [], |r| r.get(0))
        .expect("count");
    assert_eq!(count, 1, "one no-proof completion claim, one row");
    let (kind, hook): (String, String) = ledger
        .conn()
        .query_row(
            "SELECT claim_kind, hook FROM speaker_findings LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("row");
    assert_eq!(hook, "stop");
    assert!(kind == "work" || kind == "marker", "only accusable kinds");
    assert_eq!(
        thruline_core::speaker::verify_findings_chain(&ledger).expect("verify"),
        1,
        "the chain re-folds"
    );
    drop(ledger);

    // The same turn audited again appends nothing.
    let (_, _, ok) = run_speaker(&db, &["audit", "--hook", "stop"], &stop_payload);
    assert!(ok);
    let ledger = Ledger::open(&db).expect("ledger");
    let count: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM speaker_findings", [], |r| r.get(0))
        .expect("count");
    assert_eq!(count, 1, "idempotent by finding identity");

    // And the audit fed the cache, so a crash before SessionEnd still briefs.
    assert!(temp.path().join("digest").join("-repo-app.md").exists());
}

#[test]
fn speakers_fail_open_on_every_defect() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");

    // Malformed stdin.
    let (stdout, stderr, ok) = run_speaker(
        &db,
        &["digest", "precompute", "--hook", "session-end"],
        "not json at all",
    );
    assert!(
        ok && stdout.is_empty() && stderr.is_empty(),
        "{stdout}{stderr}"
    );

    // A transcript that does not exist.
    let ghost = json!({
        "session_id": SESSION,
        "transcript_path": "/nonexistent/projects/-repo-app/gone.jsonl",
        "cwd": "/repo",
        "hook_event_name": "SessionEnd",
    })
    .to_string();
    let (stdout, stderr, ok) = run_speaker(
        &db,
        &["digest", "precompute", "--hook", "session-end"],
        &ghost,
    );
    assert!(
        ok && stdout.is_empty() && stderr.is_empty(),
        "{stdout}{stderr}"
    );

    // A database path that cannot be opened (it is a directory).
    let dir_db = temp.path().join("not-a-db");
    std::fs::create_dir_all(&dir_db).expect("mkdir");
    let (_transcript, end_payload) = stage(temp.path(), "All 12 tests pass.");
    let (stdout, stderr, ok) = run_speaker(
        &dir_db,
        &["digest", "precompute", "--hook", "session-end"],
        &end_payload,
    );
    assert!(
        ok && stdout.is_empty() && stderr.is_empty(),
        "{stdout}{stderr}"
    );

    // The wrong hook value speaks to nobody.
    let (stdout, stderr, ok) =
        run_speaker(&db, &["audit", "--hook", "session-start"], &end_payload);
    assert!(
        ok && stdout.is_empty() && stderr.is_empty(),
        "{stdout}{stderr}"
    );

    // A Stop delivered while a stop hook is already active is left alone.
    let mut v: serde_json::Value = serde_json::from_str(&end_payload).expect("payload");
    v["stop_hook_active"] = json!(true);
    let (stdout, _, ok) = run_speaker(&db, &["audit", "--hook", "stop"], &v.to_string());
    assert!(ok && stdout.is_empty());
    let ledger = Ledger::open(&db).expect("ledger");
    let count: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM speaker_findings", [], |r| r.get(0))
        .expect("count");
    assert_eq!(count, 0, "an active stop hook records nothing");
}

#[test]
fn an_unreadable_row_of_another_class_does_not_silence_a_test_claim() {
    // A203 was session-global: ANY row the recognizer could not read silenced
    // EVERY claim in the session. Measured on the author's machine, that
    // counter was above zero in 24 of 24 real sessions (1 … 1968), so
    // `NoProofFound` was unreachable and `speaker_findings` had never held a
    // single row in weeks of use — a dead feature that shipped green because
    // the only fixture exercising it had no tool events at all, the one shape
    // in which the counter is zero.
    //
    // Here the claim engages the TEST class and the session holds one
    // unreadable row that can only ever hide an EDIT. It cannot hide a test
    // run, so the absence of a test run is still establishable and the finding
    // must still be written.
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let (transcript, end_payload) = stage(temp.path(), "All 12 tests pass.");
    let stop_payload = {
        let mut v: serde_json::Value = serde_json::from_str(&end_payload).expect("payload");
        v["hook_event_name"] = json!("Stop");
        v["stop_hook_active"] = json!(false);
        v.to_string()
    };
    seed_pathless_edit_row(&db, &transcript);

    let (stdout, stderr, ok) = run_speaker(&db, &["audit", "--hook", "stop"], &stop_payload);
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "", "a speaker never speaks into the session");

    let ledger = Ledger::open(&db).expect("ledger");
    let count: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM speaker_findings", [], |r| r.get(0))
        .expect("count");
    assert_eq!(
        count, 1,
        "an unreadable EDIT row cannot hide a TEST run, so the test claim's \
         absence of proof is still establishable"
    );
}

#[test]
fn an_unreadable_shell_row_still_silences_a_test_claim() {
    // The other half of the same rule, and the reason the guard exists at all:
    // a shell command could have run anything, so it taints every class. The
    // class-scoping must not become a licence to accuse over a window that
    // genuinely could hold the proof.
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let (transcript, end_payload) = stage(temp.path(), "All 12 tests pass.");
    let stop_payload = {
        let mut v: serde_json::Value = serde_json::from_str(&end_payload).expect("payload");
        v["hook_event_name"] = json!("Stop");
        v["stop_hook_active"] = json!(false);
        v.to_string()
    };
    {
        let ledger = Ledger::open(&db).expect("ledger");
        let payload = json!({
            "session_id": SESSION,
            "transcript_path": transcript.to_str().expect("path"),
            "cwd": "/repo",
            "hook_event_name": "PostToolUse",
            "tool_name": "Bash",
            "tool_input": {"command": "./run-everything.sh"},
            "tool_response": {},
        });
        thruline_core::sensor::append(
            &ledger,
            "PostToolUse",
            payload.to_string().as_bytes(),
            "2026-08-31T03:00:01.300Z",
        )
        .expect("append");
    }

    let (_stdout, stderr, ok) = run_speaker(&db, &["audit", "--hook", "stop"], &stop_payload);
    assert!(ok, "{stderr}");

    let ledger = Ledger::open(&db).expect("ledger");
    let count: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM speaker_findings", [], |r| r.get(0))
        .expect("count");
    assert_eq!(
        count, 0,
        "an unreadable shell command could have run the tests — no accusation"
    );
}
