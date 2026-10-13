//! C8 — Guardian phase 1: shadow containment, conservatism, and the
//! measurement's own honesty (A184).

use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use thruline_core::guardian::{self, Class, Readiness};
use thruline_core::ledger::Ledger;

fn artifact() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/recon/c8-guardian-measurement.json");
    serde_json::from_str(&fs::read_to_string(path).expect("read the C8 artifact"))
        .expect("the C8 artifact is JSON")
}

fn seeded(now_offset_minutes: i64) -> Ledger {
    let ledger = Ledger::open_in_memory().expect("ledger");
    let _ = now_offset_minutes;
    ledger
        .conn()
        .execute_batch(
            "INSERT INTO sensor_events(id, seq, received_at, hook_event, recognized, parse_ok,
                spine_complete, session_id, payload, payload_sha256, prev_hash, chain_hash)
             VALUES
              ('a',0,'2026-08-14T09:00:00Z','SessionStart',1,1,1,'open-and-quiet','{}','x','g','c1'),
              ('b',1,'2026-08-14T09:05:00Z','PostToolUse',1,1,1,'open-and-quiet','{}','x','c1','c2'),
              ('c',2,'2026-08-14T09:00:00Z','SessionStart',1,1,1,'closed','{}','x','c2','c3'),
              ('d',3,'2026-08-14T09:30:00Z','SessionEnd',1,1,1,'closed','{}','x','c3','c4'),
              ('e',4,'2026-08-14T11:55:00Z','SessionStart',1,1,1,'still-working','{}','x','c4','c5');",
        )
        .expect("seed");
    ledger
}

#[test]
fn a_session_still_inside_its_quiet_window_is_never_a_candidate() {
    // The false alarm that would kill the product in week one: calling a
    // working session a crash. `still-working` started 5 minutes before `now`.
    let ledger = seeded(0);
    let candidates = guardian::derive(&ledger, "2026-08-14T12:00:00Z").expect("derive");
    let sessions: Vec<&str> = candidates.iter().map(|c| c.session_id.as_str()).collect();
    assert!(
        !sessions.contains(&"still-working"),
        "a session quiet for 5 minutes was called a crash candidate"
    );
    assert!(
        !sessions.contains(&"closed"),
        "a cleanly closed session was called a crash candidate"
    );
    assert_eq!(
        sessions,
        vec!["open-and-quiet"],
        "the only candidate should be the one open and quiet past the window"
    );
}

#[test]
fn every_candidate_carries_a_reason_code_evidence_and_its_assumption() {
    let ledger = seeded(0);
    let candidates = guardian::derive(&ledger, "2026-08-14T12:00:00Z").expect("derive");
    assert!(!candidates.is_empty());
    for c in &candidates {
        assert!(!c.reason_code.is_empty(), "a candidate with no reason code");
        assert!(
            c.evidence.len() >= 2,
            "a candidate standing on {} piece(s) of evidence",
            c.evidence.len()
        );
        assert!(
            c.basis.contains("assumption-grade") || c.basis.contains("measured"),
            "the basis must state what it assumes: {}",
            c.basis
        );
        // Candidate vocabulary: this is a shape, never a verdict.
        for verdict_word in ["failed", "crashed", "broken", "stalled"] {
            assert!(
                !c.basis.to_lowercase().contains(verdict_word),
                "the basis states a verdict (`{verdict_word}`) rather than a shape: {}",
                c.basis
            );
        }
    }
}

#[test]
fn the_unbuildable_classes_are_named_rather_than_silently_absent() {
    let table = guardian::readiness();
    assert_eq!(table.len(), 5, "the phase-1 class table lost a class");
    let by_class = |c: Class| {
        table
            .iter()
            .find(|(k, _)| *k == c)
            .map(|(_, r)| r.clone())
            .expect("class present")
    };
    // Two classes have no signal at all on this machine, and say so with a
    // citation. A detector list that omits what it cannot see is false calm.
    for class in [Class::ApiLimit, Class::WaitingOnYou] {
        match by_class(class) {
            Readiness::NoSignalObserved { why } => {
                assert!(
                    why.contains("A166") && why.contains('0'),
                    "{} must cite the measurement that found no rows: {why}",
                    class.as_str()
                );
            }
            other => panic!(
                "{} claims buildability it does not have: {other:?}",
                class.as_str()
            ),
        }
    }
    // And the class that has rows but cannot be separated from an innocent one.
    match by_class(Class::DetachedTool) {
        Readiness::NotSeparable { why } => assert!(
            why.contains("DENIED") || why.contains("denial"),
            "the inseparability must name what it collides with: {why}"
        ),
        other => panic!("detached_tool claims separability: {other:?}"),
    }
}

#[test]
fn the_guardian_is_shadow_contained() {
    // Phase 1 reaches NO primary surface. `export`, `now`, `packet`, `mcp`,
    // `derive`, `attention` and the capsule must not name it — the same gate
    // shape RB1's shadow module carries.
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    for surface in [
        "export.rs",
        "now.rs",
        "packet.rs",
        "mcp.rs",
        "derive.rs",
        "attention.rs",
        "capsule.rs",
        "continuity.rs",
    ] {
        let text = fs::read_to_string(src.join(surface)).unwrap_or_default();
        assert!(
            !text.contains("guardian"),
            "{surface} reaches the Guardian before any class graduates"
        );
    }
}

#[test]
fn the_measurement_refuses_to_claim_a_target_its_denominator_cannot_support() {
    // The heart of C8: the false-alarm target is NOT met, and the reason is
    // arithmetic rather than a bad rate. An artifact that reported "0% false
    // alarms" from two specimens would be the exact overclaim this product
    // exists to refuse.
    let a = artifact();
    let far = &a["false_alarm_rate"];
    assert_eq!(far["target_met"], Value::Bool(false));
    assert_eq!(far["observed"], "0 of 2");
    let why = far["why_not"].as_str().expect("a reason");
    assert!(
        why.contains("denominator"),
        "the failure must be attributed to the denominator, not the rate: {why}"
    );
    assert!(
        why.contains("upper confidence bound"),
        "statistical honesty requires the bound, not just the point estimate: {why}"
    );
    assert!(
        why.contains("0 false in 12 means little"),
        "the reliability rule this obeys must be quoted: {why}"
    );
    assert_eq!(far["graduation_bar"], Value::from(300));
    assert_eq!(far["short_by"], Value::from(298));
    // Coverage is stated as a bound on every count, not buried.
    assert_eq!(a["coverage"]["sessions_with_hook_rows"], Value::from(28));
    assert!(a["coverage"]["fraction"]
        .as_str()
        .is_some_and(|f| f.starts_with("0.0")));
    // And the adjudication names whose judgement it is.
    assert!(
        a["adjudication"]["authority"]
            .as_str()
            .is_some_and(|s| s.contains("OPERATOR")),
        "coder adjudication must name the operator as the graduation input"
    );
}

#[test]
fn nothing_graduates_in_phase_one_and_the_gap_is_arithmetic() {
    for adjudicated in [0u64, 2, 12, 299] {
        let (bar, gap) = guardian::graduation_gap(adjudicated);
        assert_eq!(bar, 300);
        assert!(gap > 0, "a class graduated on {adjudicated} adjudications");
    }
    let (_, gap) = guardian::graduation_gap(300);
    assert_eq!(gap, 0, "the bar must be reachable, or it is theatre");
}
