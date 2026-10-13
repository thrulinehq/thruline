//! C1.2 — the appender pack, held to the probe's key-for-key contract (A168).
//!
//! Fixtures below are shaped exactly like the A166 deliveries: same spine,
//! same per-event keys. If the vendor's payloads drift, the probe is re-run
//! and these fixtures move with the artifact — never silently.

mod scratch;

use scratch::TempTree;

use serde_json::json;
use thruline_core::ledger::Ledger;
use thruline_core::sensor::{
    append, install_set, summary, verify_chain, DORMANT_EVENTS, PINNED_EVENTS,
};

fn probe_shaped(event: &str) -> String {
    let mut v = json!({
        "session_id": "4f5d8b47-ec14-45b7-9a6a-206a18f43417",
        "transcript_path": "/Users/x/.claude/projects/-tmp-probe/4f5d8b47.jsonl",
        "cwd": "/tmp/probe",
        "hook_event_name": event,
    });
    match event {
        "UserPromptSubmit" => v["prompt"] = json!("run the thing"),
        "PreToolUse" => {
            v["tool_name"] = json!("Bash");
            v["tool_input"] = json!({"command": "echo probe-ok"});
            v["tool_use_id"] = json!("toolu_01");
        }
        "PostToolUse" => {
            v["tool_name"] = json!("Bash");
            v["tool_response"] = json!({"stdout": "probe-ok"});
            v["duration_ms"] = json!(12);
        }
        "Stop" => {
            v["last_assistant_message"] = json!("done");
            v["background_tasks"] = json!([]);
            v["stop_hook_active"] = json!(false);
        }
        "SessionEnd" => v["reason"] = json!("other"),
        "PreCompact" => {
            v["trigger"] = json!("manual");
            v["custom_instructions"] = json!("None");
        }
        _ => {}
    }
    v.to_string()
}

#[test]
fn every_pinned_event_appends_with_its_spine_and_verbatim_payload() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    for (i, event) in PINNED_EVENTS.iter().enumerate() {
        let payload = probe_shaped(event);
        let rec =
            append(&ledger, event, payload.as_bytes(), "2026-08-12T14:00:00Z").expect("append");
        assert_eq!(rec.seq, i as i64, "seq is dense and ordered");
        assert!(rec.recognized && rec.parse_ok && rec.spine_complete);
        assert_eq!(
            rec.session_id.as_deref(),
            Some("4f5d8b47-ec14-45b7-9a6a-206a18f43417")
        );
        let stored: String = ledger
            .conn()
            .query_row(
                "SELECT payload FROM sensor_events WHERE id = ?1",
                [&rec.id],
                |r| r.get(0),
            )
            .expect("stored row");
        assert_eq!(stored, payload, "the payload is verbatim, byte for byte");
    }
    assert_eq!(
        verify_chain(&ledger).expect("verify"),
        PINNED_EVENTS.len() as u64,
        "the chain re-folds link by link"
    );
}

#[test]
fn malformed_spineless_and_unknown_deliveries_are_recorded_not_dropped() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    // Malformed JSON: recorded verbatim, flagged, never an error (Rule 12 —
    // the session must never pay for our parse).
    let broken = append(&ledger, "Stop", b"{not json", "2026-08-12T14:00:01Z").expect("append");
    assert!(!broken.parse_ok && !broken.spine_complete && broken.recognized);
    // Valid JSON without the spine: a fact about the delivery, in the row.
    let spineless = append(&ledger, "Stop", br#"{"x":1}"#, "2026-08-12T14:00:02Z").expect("append");
    assert!(spineless.parse_ok && !spineless.spine_complete);
    // An event name the probe never measured: recorded and marked strange,
    // because a vendor adding events must surface, not vanish.
    let strange = append(
        &ledger,
        "SomeFutureEvent",
        probe_shaped("SomeFutureEvent").as_bytes(),
        "2026-08-12T14:00:03Z",
    )
    .expect("append");
    assert!(!strange.recognized && strange.parse_ok);
    assert_eq!(verify_chain(&ledger).expect("verify"), 3);
    let counts = summary(&ledger).expect("summary");
    assert_eq!(
        counts.iter().map(|(_, n)| n).sum::<u64>(),
        3,
        "every delivery is a row — nothing was dropped"
    );
}

#[test]
fn sensor_rows_are_append_only_by_trigger() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    append(
        &ledger,
        "Stop",
        probe_shaped("Stop").as_bytes(),
        "2026-08-12T14:00:00Z",
    )
    .expect("append");
    let update = ledger
        .conn()
        .execute("UPDATE sensor_events SET hook_event = 'x'", []);
    assert!(update.is_err(), "update must be refused by trigger");
    let delete = ledger.conn().execute("DELETE FROM sensor_events", []);
    assert!(delete.is_err(), "delete must be refused by trigger");
}

#[test]
fn the_install_set_is_the_probes_plus_the_dormant_registration() {
    // The set the installer writes is exactly what the probes license: six
    // events measured at A166, PreCompact measured at A170 (the c13 probe),
    // plus StopFailure wired dormant with its unconfirmed status stated.
    // Growing this list requires a new probe, not a hunch.
    assert_eq!(
        PINNED_EVENTS,
        &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "Stop",
            "SessionEnd",
            "PreCompact"
        ]
    );
    assert_eq!(DORMANT_EVENTS, &["StopFailure"]);
    assert_eq!(install_set().len(), 8);
    assert!(
        thruline_core::sensor::DORMANT_NOTE.contains("unconfirmed"),
        "the dormant registration must state its unconfirmed status"
    );
}

#[test]
fn the_c13_probe_artifact_holds_its_own_claims() {
    // The pin discipline of every probe: a measured claim must carry its
    // evidence, an unmeasured one must say so, and neither may flip without
    // a new measurement.
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/recon/c13-precompact-probe.json");
    let a: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("read c13 artifact"))
            .expect("c13 artifact is JSON");
    let keys: Vec<&str> = a["precompact_keys"]
        .as_array()
        .expect("keys")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    for spine in ["session_id", "transcript_path", "hook_event_name"] {
        assert!(
            keys.contains(&spine),
            "the pre-compaction delivery lost `{spine}`"
        );
    }
    assert!(
        keys.contains(&"trigger"),
        "the trigger field is the payload's whole point"
    );
    assert_eq!(a["veto"]["measured"], serde_json::Value::Bool(true));
    assert!(
        a["veto"]["observed"]
            .as_str()
            .is_some_and(|o| o.contains("blocked")),
        "a measured veto must cite what was observed"
    );
    assert_eq!(a["trigger_measured"], serde_json::json!(["manual"]));
    assert!(
        a["trigger_unmeasured"]
            .as_str()
            .is_some_and(|s| s.starts_with("auto")),
        "auto stays unmeasured until a measurement says otherwise"
    );
}

#[test]
fn sealing_a_transcript_is_idempotent_and_survives_the_sweep() {
    // C1.3's property: after the pre-compaction seal, the vendor deleting or
    // rewriting the original costs nothing — the generation holds the bytes.
    let dir = TempTree::new();
    let project_dir = dir.path().join("-tmp-c13-project");
    std::fs::create_dir_all(&project_dir).expect("mkdir");
    let transcript = project_dir.join("11111111-2222-4333-8444-555555555555.jsonl");
    std::fs::write(
        &transcript,
        b"{\"type\":\"user\"}\n{\"type\":\"assistant\"}\n",
    )
    .expect("write transcript");
    let mirror_root = dir.path().join("mirror");
    let db = dir.path().join("ledger.db");
    let mut ledger = thruline_core::ledger::Ledger::open(&db).expect("ledger");
    let first = thruline_core::ingest::seal_transcript(&mut ledger, &mirror_root, &transcript)
        .expect("seal");
    assert_eq!(first.claude_sealed, 1, "first seal accepts a generation");
    assert!(first.claude_seal_failures.is_empty());
    let second = thruline_core::ingest::seal_transcript(&mut ledger, &mirror_root, &transcript)
        .expect("re-seal");
    assert_eq!(
        (second.claude_sealed, second.claude_seal_identical),
        (0, 1),
        "an identical file is an identical observation, not a new generation"
    );
    let accepted = ledger
        .accepted_generation("11111111-2222-4333-8444-555555555555")
        .expect("query")
        .expect("a generation is on record");
    assert!(accepted.payload_bytes > 0);
    // The sweep: the original leaves the disk; the generation holds.
    std::fs::remove_file(&transcript).expect("sweep");
    assert!(
        ledger
            .accepted_generation("11111111-2222-4333-8444-555555555555")
            .expect("query")
            .is_some(),
        "the sealed generation outlives its source"
    );
}

#[test]
fn a_tampered_row_is_named_by_verify() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    append(
        &ledger,
        "Stop",
        probe_shaped("Stop").as_bytes(),
        "2026-08-12T14:00:00Z",
    )
    .expect("append");
    // Bypass the trigger the way an attacker with the file would: drop it.
    ledger
        .conn()
        .execute_batch(
            "DROP TRIGGER sensor_events_no_update;
             UPDATE sensor_events SET payload = '{\"forged\":true}' WHERE seq = 0;",
        )
        .expect("forge");
    assert!(
        verify_chain(&ledger).is_err(),
        "a rewritten payload must break the fold"
    );
}
