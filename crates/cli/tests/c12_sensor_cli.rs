//! C1.2 — the appender's process-level contract (A168).
//!
//! The two absolutes a hook entrypoint lives or dies by:
//! exit 0 no matter what (a nonzero exit surfaces inside the user's session)
//! and an empty stdout (SessionStart stdout is injected as context — a sensor
//! that speaks into the session would be an actor, not a sensor).

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use tempfile::tempdir;

fn run_append(db: &Path, event: &str, payload: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .args(["--db", db.to_str().unwrap(), "sensor", "append", event])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn appender");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(payload)
        .expect("write payload");
    child.wait_with_output().expect("wait")
}

#[test]
fn append_exits_zero_prints_nothing_and_stores_the_row() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let payload = br#"{"session_id":"s1","transcript_path":"/t/s1.jsonl","cwd":"/w","hook_event_name":"Stop"}"#;
    let out = run_append(&db, "Stop", payload);
    assert!(out.status.success(), "the appender must exit 0");
    assert!(
        out.stdout.is_empty(),
        "stdout must be empty — it is injectable context: {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    let rows = thruline_core::sensor::summary(&ledger).expect("summary");
    assert_eq!(rows, vec![("Stop".to_string(), 1)]);
}

#[test]
fn append_swallows_malformed_payloads_without_a_word_on_stdout() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let out = run_append(&db, "Stop", b"{never json");
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    assert_eq!(
        thruline_core::sensor::summary(&ledger).expect("summary"),
        vec![("Stop".to_string(), 1)],
        "malformed input is recorded, flagged in the row, never dropped"
    );
}

#[test]
fn precompact_append_seals_the_transcript_into_the_mirror() {
    // C1.3: the compaction-insurance property at process level — one hook
    // invocation, and the full transcript is a mirror generation before the
    // vendor's summarizer touches it.
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let mirror = temp.path().join("mirror");
    let project_dir = temp.path().join("-tmp-c13-cli");
    std::fs::create_dir_all(&project_dir).expect("mkdir");
    let transcript = project_dir.join("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee.jsonl");
    std::fs::write(&transcript, b"{\"type\":\"user\"}\n").expect("transcript");
    let payload = serde_json::json!({
        "session_id": "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
        "transcript_path": transcript.to_str().unwrap(),
        "cwd": "/tmp",
        "hook_event_name": "PreCompact",
        "trigger": "manual",
        "custom_instructions": ""
    })
    .to_string();
    let mut child = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .args([
            "--db",
            db.to_str().unwrap(),
            "sensor",
            "append",
            "PreCompact",
            "--mirror-root",
            mirror.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(payload.as_bytes())
        .expect("write");
    let out = child.wait_with_output().expect("wait");
    assert!(out.status.success());
    assert!(
        out.stdout.is_empty(),
        "stdout stays silent even while sealing"
    );
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    assert!(
        ledger
            .accepted_generation("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee")
            .expect("query")
            .is_some(),
        "the pre-compaction transcript is a mirror generation: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn liveness_markers_follow_the_session_and_the_doctor_reads_them() {
    // C1.4: a marker per live session, gone on close; a marker whose process
    // is gone with no close on record is the doctor's crash candidate.
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let live = temp.path().join("sensor-live");
    let start = |sid: &str, event: &str| {
        let payload = format!(
            r#"{{"session_id":"{sid}","transcript_path":"/t/{sid}.jsonl","cwd":"/w","hook_event_name":"{event}"}}"#
        );
        run_append(&db, event, payload.as_bytes())
    };
    // A start writes the marker; the close removes it.
    start("s-closes", "SessionStart");
    assert!(
        live.join("s-closes.json").exists(),
        "the marker exists while live"
    );
    start("s-closes", "SessionEnd");
    assert!(
        !live.join("s-closes.json").exists(),
        "the close removes the marker"
    );
    // A crashed session: marker present, vendor pid dead, no close on record.
    start("s-crashed", "SessionStart");
    let forged = serde_json::json!({
        "session_id": "s-crashed", "transcript_path": "/t/s-crashed.jsonl",
        "cwd": "/w", "vendor_pid": 4000000, "last_event": "PostToolUse",
        "updated_at": "2026-08-12T15:00:00Z"
    });
    std::fs::write(live.join("s-crashed.json"), forged.to_string()).expect("forge marker");
    let settings = temp.path().join("settings.json");
    let out = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .args([
            "--db",
            db.to_str().unwrap(),
            "sensor",
            "doctor",
            "--settings",
            settings.to_str().unwrap(),
        ])
        .output()
        .expect("doctor");
    assert!(out.status.success(), "the doctor exits 0");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("crash candidate") && text.contains("s-crashed"),
        "the dead marker with no close is named a candidate: {text}"
    );
    assert!(
        text.contains("read-only; it changes nothing"),
        "the doctor states its own authority"
    );
    assert!(
        live.join("s-crashed.json").exists(),
        "the doctor removed evidence — it must be read-only"
    );
    for line in text.lines() {
        assert_eq!(
            thruline_core::voice::banned_in_output(line),
            None,
            "engine lexicon reached the doctor: {line:?}"
        );
    }
}

#[test]
fn install_plan_apply_uninstall_round_trips_and_spares_foreign_entries() {
    let temp = tempdir().expect("tempdir");
    let settings = temp.path().join("settings.json");
    // A foreign hook the user already has: it must survive untouched.
    std::fs::write(
        &settings,
        r#"{"model":"opus","hooks":{"Stop":[{"hooks":[{"type":"command","command":"/usr/local/bin/somebody-else"}]}]}}"#,
    )
    .expect("seed settings");

    // `install` opens the ledger to pay migrations before any hook fires, so
    // every run pins --db to scratch — a test must never touch the real one.
    let db = temp.path().join("ledger.db");
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_thruline"))
            .args(["--db", db.to_str().unwrap()])
            .args(args)
            .output()
            .expect("run");
        assert!(out.status.success(), "{args:?}: {out:?}");
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let s = settings.to_str().unwrap();

    let plan = run(&["sensor", "plan", "--settings", s]);
    assert!(plan.contains("writes nothing"));
    assert!(
        plan.contains("StopFailure") && plan.contains("unconfirmed"),
        "the plan must state the dormant registration's status: {plan}"
    );
    // Plan wrote nothing.
    let before = std::fs::read_to_string(&settings).expect("read");
    assert!(before.contains("somebody-else") && !before.contains("sensor append"));

    run(&["sensor", "install", "--settings", s]);
    let installed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings).expect("read")).expect("json");
    assert_eq!(installed["model"], "opus", "foreign top-level keys survive");
    let stop = installed["hooks"]["Stop"].as_array().expect("Stop entries");
    assert_eq!(stop.len(), 2, "ours appends beside the foreign entry");
    for event in thruline_core::sensor::install_set() {
        assert!(
            installed["hooks"][event].is_array(),
            "{event} was not registered"
        );
    }

    // Idempotent: a second install changes nothing.
    let second = run(&["sensor", "install", "--settings", s]);
    assert!(second.contains("nothing to do"));

    run(&["sensor", "uninstall", "--settings", s]);
    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings).expect("read")).expect("json");
    assert_eq!(after["model"], "opus");
    let stop = after["hooks"]["Stop"].as_array().expect("Stop entries");
    assert_eq!(stop.len(), 1, "exactly ours was removed");
    assert!(
        stop[0]["hooks"][0]["command"]
            .as_str()
            .is_some_and(|c| c.contains("somebody-else")),
        "the foreign entry survives byte-for-byte in structure"
    );
    for event in [
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "SessionEnd",
        "StopFailure",
    ] {
        assert!(
            after["hooks"].get(event).is_none(),
            "{event} should be gone after uninstall"
        );
    }
}

#[test]
fn doctor_states_the_speaker_layer_rather_than_omitting_it() {
    // Speakers are fail-open-silent by doctrine: any error exits 0 and prints
    // nothing. So a speaker layer that has never produced a line looks exactly
    // like one that had nothing to say — everywhere except here. The doctor
    // must therefore state each absence (hook not registered, no digest cache,
    // no findings) rather than print four sensor sections and stop.
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let settings = temp.path().join("settings.json");
    // A machine with the sensors installed and no speaker entry at all.
    std::fs::write(&settings, r#"{"hooks":{}}"#).expect("seed settings");

    let out = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .args([
            "--db",
            db.to_str().unwrap(),
            "sensor",
            "doctor",
            "--settings",
            settings.to_str().unwrap(),
        ])
        .output()
        .expect("doctor");
    assert!(out.status.success(), "the doctor exits 0");
    let text = String::from_utf8_lossy(&out.stdout);

    assert!(
        text.contains("speakers"),
        "there is a speaker section: {text}"
    );
    for verb in [
        "brief --hook session-start",
        "audit --hook stop",
        "digest precompute --hook session-end",
    ] {
        let stated = text
            .lines()
            .any(|l| l.ends_with(verb) && l.contains("NOT registered"));
        assert!(
            stated,
            "an unregistered speaker is a stated absence, missing {verb}: {text}"
        );
    }
    assert!(
        text.contains("digest cache     ABSENT"),
        "a missing cache directory is stated, not swallowed: {text}"
    );
    assert!(
        text.contains("findings         0 rows"),
        "an empty findings table is stated as empty: {text}"
    );
    // Read-only, like every other section of this check.
    assert!(
        !thruline_core::speaker::digest_dir(&db).exists(),
        "the doctor created the digest directory — it must change nothing"
    );
    for line in text.lines() {
        assert_eq!(
            thruline_core::voice::banned_in_output(line),
            None,
            "engine lexicon reached the doctor: {line:?}"
        );
    }
}

#[test]
fn doctor_names_the_absent_replay_path_for_spilled_deliveries() {
    // A spilled delivery is a loss. Reporting the loss while offering no
    // disposition reads as an offer of a recovery that does not exist: there
    // is no replay verb, and the output has to say so.
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let settings = temp.path().join("settings.json");
    std::fs::write(
        temp.path().join("sensor-spill.jsonl"),
        "{\"error\":\"sqlite error: database is locked\",\"hook_event\":\"Stop\"}\n",
    )
    .expect("seed spill");

    let out = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .args([
            "--db",
            db.to_str().unwrap(),
            "sensor",
            "doctor",
            "--settings",
            settings.to_str().unwrap(),
        ])
        .output()
        .expect("doctor");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("1 spilled delivery"),
        "the spill is counted: {text}"
    );
    assert!(
        text.contains("NO replay verb exists"),
        "the absent recovery path is stated outright: {text}"
    );
    for line in text.lines() {
        assert_eq!(
            thruline_core::voice::banned_in_output(line),
            None,
            "engine lexicon reached the doctor: {line:?}"
        );
    }
}
