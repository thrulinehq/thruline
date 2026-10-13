//! C34 R-LEDGER (7), ruling Q5 — **a recorded pause is honoured by the capture
//! callers**, through the shipping binary in a temporary HOME.
//!
//! The no-op proof (`capture_gates_noop.rs`) says the gates do nothing while no
//! one has paused anything. This is the other half: with a pause in the record,
//! the very same deliveries and the very same `thruline ingest` write NOTHING
//! for that folder, the counts say so, and everything outside the folder is
//! recorded as it always was.
//!
//! The session-start rule (design §6.2): while paused nothing under the folder
//! is read; after the pause is lifted, a recording whose session STARTED inside
//! it is never read; a session that started outside it is read in full.
//!
//! No real store is opened: every process has HOME and THRULINE_HOME inside the
//! temp dir.

mod capture_gates_support;

use std::process::Output;

use capture_gates_support::digest::digest;
use capture_gates_support::{World, APP, S1};
use serde_json::json;

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A world whose Claude Code project is already in the ledger, so the folder
/// `/work/app` is a project a person can pause.
fn world_with_a_project() -> World {
    let w = World::new();
    let out = w.ingest("claude-code", &w.claude_dir());
    assert!(out.status.success(), "{}", text(&out));
    assert!(w.count("events") > 0);
    w
}

fn act(w: &World, verb: &str) {
    let out = w.capture(&[verb, "--project", APP]);
    assert!(out.status.success(), "capture {verb}: {}", text(&out));
}

fn spill(w: &World) -> std::path::PathBuf {
    w.home().join(".thruline/sensor-spill.jsonl")
}

#[test]
fn a_paused_folder_records_no_hook_delivery_and_says_how_many_it_left_out() {
    let w = world_with_a_project();
    assert_eq!(w.count("sensor_events"), 0);
    act(&w, "pause");
    let mirror_before = digest(&w).summary["mirror"].clone();

    // Seven deliveries, PreCompact among them (which would seal the transcript)
    // and SessionEnd (which would touch a liveness marker).
    for event in [
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PreCompact",
        "Stop",
        "SessionEnd",
    ] {
        let out = w.deliver(event, &w.payload(event, APP));
        assert!(
            out.status.success(),
            "{event}: a hook exits 0 whatever happens"
        );
        assert!(out.stdout.is_empty(), "{event}: a hook prints nothing");
        assert!(
            out.stderr.is_empty(),
            "{event}: a paused delivery is not a failure and says nothing: {}",
            text(&out)
        );
    }
    assert_eq!(w.count("sensor_events"), 0, "nothing was appended");
    assert!(
        !spill(&w).exists(),
        "a pause is not a failure: nothing is spilled"
    );
    assert!(
        !w.home().join(".thruline/sensor-live").exists(),
        "no liveness marker was written"
    );
    assert_eq!(
        digest(&w).summary["mirror"],
        mirror_before,
        "the pre-compaction seal did not happen"
    );

    // A folder INSIDE the paused one is paused too.
    let inside = w.deliver("Stop", &w.payload("Stop", "/work/app/crates/core"));
    assert!(inside.status.success() && inside.stdout.is_empty());
    assert_eq!(w.count("sensor_events"), 0);

    // Whole path components, never a string prefix: `/work/appx` is another
    // folder, and so is anything elsewhere. Both are recorded.
    for other in ["/work/appx", "/elsewhere"] {
        let out = w.deliver("Stop", &w.payload("Stop", other));
        assert!(out.status.success() && out.stdout.is_empty());
    }
    assert_eq!(
        w.count("sensor_events"),
        2,
        "`/work/appx` and `/elsewhere` are not under `/work/app`"
    );

    // Every skip is counted, and `thruline capture holds` reports it.
    let holds = w.capture(&["holds", "--project", APP]);
    let said = text(&holds);
    assert!(
        said.contains("8 hook delivery"),
        "7 deliveries + 1 inside the folder were left out: {said}"
    );
    assert!(said.contains("not recorded while paused"), "{said}");
    assert!(said.contains("reads nothing from this folder"), "{said}");

    // Resumed, the folder is recorded again (this session started outside the
    // pause: the fixture's first line is dated 2026-10-01).
    act(&w, "resume");
    let out = w.deliver("Stop", &w.payload("Stop", APP));
    assert!(out.status.success() && out.stdout.is_empty());
    assert_eq!(w.count("sensor_events"), 3);
}

#[test]
fn a_paused_folder_is_not_audited_into_the_record() {
    let w = world_with_a_project();
    act(&w, "pause");
    let out = w.audit_a_claim(APP);
    assert!(out.status.success());
    assert!(out.stdout.is_empty(), "{}", text(&out));
    assert_eq!(
        w.count("speaker_findings"),
        0,
        "no completion claim was appended while the folder was paused"
    );
    let said = text(&w.capture(&["holds", "--project", APP]));
    assert!(said.contains("1 Stop audit"), "the skip is counted: {said}");

    act(&w, "resume");
    let out = w.audit_a_claim(APP);
    assert!(out.status.success());
    assert_eq!(
        w.count("speaker_findings"),
        1,
        "after the pause the same audit appends its one finding"
    );
}

/// A transcript whose session starts at `started`, in the Claude Code project.
fn start_a_session(w: &World, session: &str, started: &str) {
    let line = json!({
        "parentUuid": null, "isSidechain": false,
        "uuid": format!("{session}-u1"), "timestamp": started, "cwd": APP,
        "sessionId": session, "version": "2.1.0", "gitBranch": "main",
        "userType": "external", "type": "user",
        "message": {"role": "user", "content": "Add a README section."},
    });
    std::fs::write(
        w.claude_dir().join(format!("{session}.jsonl")),
        format!("{line}\n"),
    )
    .expect("transcript");
}

fn append_a_line(w: &World, session: &str, at: &str, text: &str) {
    use std::io::Write;
    let line = json!({
        "parentUuid": null, "isSidechain": false,
        "uuid": format!("{session}-late"), "timestamp": at, "cwd": APP,
        "sessionId": session, "version": "2.1.0", "gitBranch": "main",
        "userType": "external", "type": "user",
        "message": {"role": "user", "content": text},
    });
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(w.transcript(session))
        .expect("open");
    writeln!(f, "{line}").expect("append");
}

#[test]
fn thruline_ingest_leaves_a_paused_folder_alone_and_after_resume_skips_only_what_started_in_the_pause(
) {
    let w = world_with_a_project();
    let events_at_start = w.count("events");
    let sessions_at_start = w.count("sessions");
    act(&w, "pause");

    // A session starts while recording is paused.
    let started = thruline_core::ingest::iso8601_utc_now();
    let paused_session = "eeeeeeee-3333-4333-8333-333333333333";
    start_a_session(&w, paused_session, &started);

    // `thruline ingest` of the folder, paused: nothing is read, one line names
    // the folder, the report counts what was left out, and it is not a failure.
    let out = w.ingest("claude-code", &w.claude_dir());
    let said = text(&out);
    assert!(out.status.success(), "{said}");
    assert_eq!(
        said.lines()
            .filter(|l| l.starts_with("paused by you:"))
            .count(),
        1,
        "exactly one line per paused folder: {said}"
    );
    assert!(said.contains("/work/app"), "{said}");
    assert!(
        said.contains("3 recording(s) not read: 3 because the folder is paused, 0 because"),
        "S1, S2 and the new session were all under the pause: {said}"
    );
    assert!(said.contains("nothing was read"), "{said}");
    assert_eq!(
        w.count("events"),
        events_at_start,
        "nothing under the folder was read"
    );
    assert_eq!(w.count("sessions"), sessions_at_start);

    // The pause is lifted a second later.
    std::thread::sleep(std::time::Duration::from_millis(1_100));
    act(&w, "resume");

    // S1 started on 2026-10-01, outside the pause, and Claude Code kept writing
    // it while recording was paused: it is read in full, new line included.
    append_a_line(&w, S1, &started, "One more thing: add a changelog entry.");
    let out = w.ingest("claude-code", &w.claude_dir());
    let said = text(&out);
    assert!(out.status.success(), "{said}");
    assert!(
        said.contains("1 recording(s) not read: 0 because the folder is paused, 1 because the session started while recording was paused"),
        "the session that started in the pause is counted, and only it: {said}"
    );
    assert_eq!(
        w.count("events"),
        events_at_start + 1,
        "the line a session that started outside the pause wrote inside it is read"
    );
    let ledger = w.ledger();
    let stored: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM events WHERE session_id = ?1",
            [paused_session],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(
        stored, 0,
        "the session that started in the pause is never read"
    );
    assert_eq!(w.count("sessions"), sessions_at_start);
}

#[test]
fn a_hook_for_a_session_that_started_in_the_pause_is_left_out_after_resume() {
    let w = world_with_a_project();
    act(&w, "pause");
    let started = thruline_core::ingest::iso8601_utc_now();
    let paused_session = "eeeeeeee-3333-4333-8333-333333333333";
    start_a_session(&w, paused_session, &started);
    std::thread::sleep(std::time::Duration::from_millis(1_100));
    act(&w, "resume");

    let payload = json!({
        "session_id": paused_session,
        "transcript_path": w.transcript(paused_session).to_str().expect("utf8"),
        "cwd": APP,
        "hook_event_name": "Stop",
    });
    let out = w.deliver("Stop", &payload);
    assert!(out.status.success() && out.stdout.is_empty() && out.stderr.is_empty());
    assert_eq!(
        w.count("sensor_events"),
        0,
        "a session that started while recording was paused is not recorded after it"
    );
    // The same folder, another session (started on 2026-10-01): recorded.
    let out = w.deliver("Stop", &w.payload("Stop", APP));
    assert!(out.status.success());
    assert_eq!(w.count("sensor_events"), 1);
}

#[test]
fn a_hook_whose_payload_is_not_json_or_names_no_folder_is_recorded_as_before() {
    let w = world_with_a_project();
    act(&w, "pause");
    // Nothing here says which folder it belongs to, so nothing says it is paused.
    let db = w.db();
    let mut child = w
        .cmd(&[
            "--db",
            db.to_str().expect("utf8"),
            "sensor",
            "append",
            "Stop",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    {
        use std::io::Write;
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(b"{never json")
            .expect("payload");
    }
    let out = child.wait_with_output().expect("wait");
    assert!(out.status.success() && out.stdout.is_empty());
    assert_eq!(
        w.count("sensor_events"),
        1,
        "a delivery that names no folder is recorded"
    );

    let out = w.deliver(
        "Stop",
        &json!({"session_id": "x", "hook_event_name": "Stop"}),
    );
    assert!(out.status.success());
    assert_eq!(w.count("sensor_events"), 2);
}
