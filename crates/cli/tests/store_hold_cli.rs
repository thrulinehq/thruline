use std::process::Command;

use thruline_core::ledger::Ledger;
use thruline_core::store_hold::StoreHold;

#[test]
fn ordinary_cli_read_refuses_the_hold_with_exit_three_and_one_fixed_sentence() {
    let root = tempfile::tempdir().expect("scratch home");
    let home = root.path().join("home");
    std::fs::create_dir_all(&home).expect("home");
    let db = root.path().join("ledger.db");
    drop(Ledger::open(&db).expect("scratch ledger"));
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_thruline"))
            .env("HOME", &home)
            .env("THRULINE_HOME", &home)
            .arg("--db")
            .arg(&db)
            .arg("projects")
            .output()
            .expect("projects command")
    };
    let before = run();
    assert!(before.status.success(), "{before:?}");
    let bytes = std::fs::read(&db).expect("store bytes");

    let hold = StoreHold::create(&db, "removal-01", "P1").expect("hold scratch ledger");
    let refused = run();
    assert_eq!(refused.status.code(), Some(3), "{refused:?}");
    assert!(refused.stdout.is_empty(), "{refused:?}");
    assert_eq!(
        String::from_utf8_lossy(&refused.stderr),
        "ThruLine is removing a project's records; try again when it finishes\n"
    );
    assert_eq!(std::fs::read(&db).expect("store bytes"), bytes);

    hold.release().expect("release hold");
    let after = run();
    assert!(after.status.success(), "{after:?}");
    assert_eq!(after.stdout, before.stdout);
    assert_eq!(after.stderr, before.stderr);
}

#[test]
fn held_sensor_spills_each_delivery_without_appending_sealing_or_speaking() {
    use std::io::Write;
    use std::process::Stdio;

    let root = tempfile::tempdir().expect("scratch record");
    let home = root.path().join("home");
    let mirror = root.path().join("mirror");
    std::fs::create_dir(&home).expect("scratch home");
    let db = root.path().join("ledger.db");
    drop(Ledger::open(&db).expect("scratch ledger"));
    let before = std::fs::read(&db).expect("store bytes");
    let hold = StoreHold::create(&db, "sensor-hold", "P1").expect("hold");
    let mut payloads = Vec::new();
    for event in ["SessionStart", "PreCompact", "Stop"] {
        let payload = serde_json::json!({
            "session_id": "scratch-held-session",
            "cwd": root.path(),
            "transcript_path": root.path().join("scratch.jsonl"),
            "hook_event_name": event
        })
        .to_string();
        let mut child = Command::new(env!("CARGO_BIN_EXE_thruline"))
            .env("HOME", &home)
            .env("THRULINE_HOME", &home)
            .arg("--db")
            .arg(&db)
            .args(["sensor", "append", event, "--mirror-root"])
            .arg(&mirror)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("sensor");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(payload.as_bytes())
            .expect("delivery");
        let out = child.wait_with_output().expect("sensor exit");
        assert!(out.status.success(), "{out:?}");
        assert!(out.stdout.is_empty(), "sensor must not speak into session");
        assert!(String::from_utf8_lossy(&out.stderr).contains("payload spilled"));
        payloads.push((event, payload));
    }
    assert_eq!(std::fs::read(&db).expect("held bytes"), before);
    assert!(!mirror.exists(), "held PreCompact must not seal");
    assert!(
        !db.with_file_name("sensor-live").exists(),
        "held delivery must not make liveness marker"
    );
    let spill = std::fs::read_to_string(db.with_file_name("sensor-spill.jsonl")).expect("spill");
    let rows: Vec<serde_json::Value> = spill
        .lines()
        .map(|line| serde_json::from_str(line).expect("spill JSON"))
        .collect();
    assert_eq!(
        rows.len(),
        payloads.len(),
        "each refusal is retained exactly once"
    );
    for (row, (event, payload)) in rows.iter().zip(payloads) {
        assert_eq!(row["hook_event"], event);
        assert_eq!(row["payload"], payload);
        assert_eq!(
            row["error"],
            "ThruLine is removing a project's records; try again when it finishes"
        );
    }
    hold.release().expect("release hold");
    let ledger = Ledger::open(&db).expect("unheld record");
    assert!(thruline_core::sensor::summary(&ledger)
        .expect("sensor rows")
        .is_empty());
}
