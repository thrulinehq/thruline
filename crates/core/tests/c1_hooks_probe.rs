//! C1.1 — the hooks-probe pin (A166).
//!
//! Holds `docs/recon/c1-hooks-probe.json` to its own arithmetic and citation
//! discipline (RB0's shape: a counted signal must carry its evidence; an
//! uncounted one must say why), and — the probe's whole contract — that **no
//! sensor code exists in core before C1.2's shadow-gated build**. The gate
//! evolves into C1.2's containment test when the appender pack lands, exactly
//! as RB0's no-detection pin became RB1's shadow gate.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;

fn artifact() -> Value {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/recon/c1-hooks-probe.json");
    serde_json::from_str(&fs::read_to_string(path).expect("read the probe artifact"))
        .expect("the probe artifact is JSON")
}

#[test]
fn observed_plus_unobserved_covers_every_registered_event() {
    let a = artifact();
    let registered = a["registered_events"].as_u64().expect("registered_events");
    let observed = a["observed"].as_object().expect("observed").len() as u64;
    let unobserved = a["unobserved"].as_object().expect("unobserved").len() as u64;
    assert_eq!(
        observed + unobserved,
        registered,
        "an event was registered and then neither counted nor explained"
    );
}

#[test]
fn every_observed_event_carries_deliveries_and_keys_and_every_absence_a_reason() {
    let a = artifact();
    for (event, row) in a["observed"].as_object().expect("observed") {
        assert!(
            row["deliveries"].as_u64().unwrap_or(0) >= 1,
            "{event} is listed observed with no delivery"
        );
        let keys = row["keys"].as_array().expect("keys");
        assert!(!keys.is_empty(), "{event} cites no payload keys");
        // The spine every C1.2 appender builds on: if a delivery ever stops
        // carrying these, the sensor design is invalid and this fails first.
        for spine in ["session_id", "transcript_path", "hook_event_name"] {
            assert!(
                keys.iter().any(|k| k == spine),
                "{event} delivered without `{spine}` — the appender spine is broken"
            );
        }
    }
    for (event, reason) in a["unobserved"].as_object().expect("unobserved") {
        assert!(
            reason.as_str().is_some_and(|r| r.len() > 20),
            "{event} is unobserved without a real reason"
        );
    }
}

#[test]
fn the_injection_finding_separates_delivery_from_attention() {
    let a = artifact();
    let inj = &a["injection_experiment"];
    assert_eq!(inj["delivered"], Value::Bool(true));
    assert!(
        inj["evidence"]
            .as_str()
            .is_some_and(|e| e.contains("transcript")),
        "a delivered=true injection must cite where the token landed"
    );
    // Attention may not be claimed without a measurement. At A166 this held
    // the string at "unconfirmed"; at A172 the c40 probe measured it, so the
    // gate evolved rather than being deleted — it now demands that any
    // attention claim NAME the artifact that measured it and carry the
    // do-not-overclaim caveat. An unmeasured upgrade still fails here.
    let attended = inj["attended"].as_str().unwrap_or_default();
    assert!(
        attended.starts_with("unconfirmed")
            || (attended.contains("c40-injection-probe.json")
                && attended.contains("caveat_do_not_overclaim")),
        "attention was claimed without naming the probe that measured it: {attended:?}"
    );
}

#[test]
fn the_c40_probe_states_what_it_measured_and_what_it_did_not() {
    // C4.0's artifact under the same law as every probe: a measured claim
    // carries its evidence and its bound; an unmeasured surface is named,
    // not quietly assumed. `rescue` may offer only what this file measured.
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/recon/c40-injection-probe.json");
    let a: Value = serde_json::from_str(&fs::read_to_string(path).expect("read c40 artifact"))
        .expect("c40 artifact is JSON");
    let claude = &a["claude_code"];
    assert_eq!(claude["delivery"]["measured"], Value::Bool(true));
    let attention = &claude["attention"];
    assert_eq!(attention["measured"], Value::Bool(true));
    assert_eq!(
        attention["specimens"],
        Value::from(1),
        "one specimen is one specimen"
    );
    assert!(
        attention["caveat_do_not_overclaim"]
            .as_str()
            .is_some_and(|c| c.contains("does NOT prove")),
        "a measured attention claim must state what it still does not prove"
    );
    // Codex has no injection surface, and the artifact must keep saying so —
    // a rescue that silently invented one would be the exact overclaim the
    // capability matrix exists to refuse.
    assert!(
        a["codex"]["injection_surface"]
            .as_str()
            .is_some_and(|s| s.starts_with("none")),
        "codex gained an injection surface without a measurement"
    );
    assert!(
        a["unmeasured_and_named"]
            .as_array()
            .is_some_and(|u| u.len() >= 3),
        "the unmeasured list is where honesty lives; it did not survive"
    );
}

#[test]
fn sensor_code_lives_in_one_module_and_feeds_nothing_downstream() {
    // Evolved at A168, exactly as the RB0 pin became RB1's shadow gate: the
    // pre-C1.2 form banned sensor code from core outright; the appender pack
    // now exists, so the gate is containment. `sensor.rs` is the one module;
    // `ledger.rs` may name the table in schema only. Everything else in core
    // — derivation, attention, export, packet — must not read the rows:
    // observations feed nothing until C8's shadow gate.
    let a = artifact();
    // The artifact's 0 is probe-time truth and stays: A166 shipped no sensor.
    assert_eq!(a["sensor_code_written"], Value::from(0));
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut walked = 0usize;
    for entry in fs::read_dir(&src).expect("walk core src") {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("file name")
            .to_string();
        let text = fs::read_to_string(&path).expect("read source");
        // Unprobed territory: still banned everywhere in core — wiring it
        // needs its own probe-then-gate. `PreCompact` left this list at A170
        // when the c13 probe measured it (and its veto) live.
        for marker in ["idle_prompt", "hooks_sensor"] {
            assert!(
                !text.contains(marker),
                "{} contains `{marker}` — machinery before its probe",
                path.display()
            );
        }
        // Sensor vocabulary: one module, plus the schema line in ledger.rs —
        // and, since A184, `guardian.rs`. This gate's own comment named the
        // moment: observations fed nothing "until C8's shadow gate". C8 is
        // here, so the Guardian is the ONE licensed consumer, and it is itself
        // shadow-contained by `c8_guardian.rs`. Any other reader still fails.
        if name != "sensor.rs" && name != "guardian.rs" {
            for marker in ["StopFailure", "hook_event_name", "PreCompact"] {
                assert!(
                    !text.contains(marker),
                    "{} contains `{marker}` — sensor vocabulary outside sensor.rs",
                    path.display()
                );
            }
        }
        // A200 widened the license once more, by the same ritual: the A199
        // Rule 7 amendment (b) created the speaker class, whose contracted job
        // is to read ONE session's rows (indexed by session_id, never a scan)
        // and put receipt-backed words into the next session. `speaker.rs` is
        // therefore the second licensed consumer, contained by
        // `a200_speaker_digest.rs`; it derives no node, edge, status, or
        // attention item from the rows. Any other reader still fails here.
        if name != "sensor.rs"
            && name != "ledger.rs"
            && name != "guardian.rs"
            && name != "speaker.rs"
        {
            assert!(
                !text.contains("sensor_events"),
                "{} reaches the sensor rows — the Guardian and the speaker are the licensed readers",
                path.display()
            );
        }
        walked += 1;
    }
    assert!(walked > 20, "the walk read only {walked} files");
}
