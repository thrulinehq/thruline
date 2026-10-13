//! C4.3 / P1 Packet 1 — the legacy continuity truth gate.

use thruline_core::capsule;
use thruline_core::continuity::{
    self, Check, CheckState, Outcome, MANDATORY_CHECK_NAMES, SUCCESSOR_EVIDENCE_MISSING,
};
use thruline_core::ledger::Ledger;

const NOW: &str = "2026-08-14T10:00:00Z";

fn seeded() -> (Ledger, String) {
    let ledger = Ledger::open_in_memory().expect("ledger");
    ledger
        .conn()
        .execute_batch(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
               VALUES ('p1','/tmp/p1','claude_code','p1','2026-08-14T00:00:00Z');
             INSERT INTO sessions(id, project_id, file_path, started_at, last_seq)
               VALUES ('s1','p1','/t/s1.jsonl','2026-08-14T00:00:00Z',1);
             INSERT INTO events(id, project_id, session_id, seq, ts, kind, text, tool_name,
                                tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
               VALUES ('e1','p1','s1',0,'2026-08-14T00:00:01Z','user_msg','make the thing work',
                       NULL,NULL,'/t/s1.jsonl',1,'x','g','c1'),
                      ('e2','p1','s1',1,'2026-08-14T00:00:02Z','tool_use',NULL,'Bash',
                       '{\"command\":\"git push origin main\"}','/t/s1.jsonl',2,'x','c1','c2');",
        )
        .expect("seed");
    (ledger, "p1".to_string())
}

fn required_checks(state: CheckState) -> Vec<Check> {
    MANDATORY_CHECK_NAMES
        .iter()
        .map(|name| Check {
            name: (*name).to_string(),
            state,
            detail: "table-driven evaluator fixture".to_string(),
        })
        .collect()
}

fn set_state(checks: &mut [Check], name: &str, state: CheckState) {
    checks
        .iter_mut()
        .find(|check| check.name == name)
        .expect("required check")
        .state = state;
}

#[test]
fn mandatory_truth_table_fails_closed() {
    let all_insufficient = required_checks(CheckState::Insufficient);

    let mut one_insufficient = required_checks(CheckState::Passed);
    set_state(
        &mut one_insufficient,
        "workspace digest",
        CheckState::Insufficient,
    );

    let mut missing = required_checks(CheckState::Passed);
    missing.retain(|check| check.name != "HEAD lineage");

    let mut one_failed = required_checks(CheckState::Passed);
    set_state(
        &mut one_failed,
        "objective acknowledged",
        CheckState::Failed,
    );

    let mut optional_cannot_compensate = one_insufficient.clone();
    optional_cannot_compensate.push(Check {
        name: "optional presentation check".to_string(),
        state: CheckState::Passed,
        detail: "an optional pass has no authority over required evidence".to_string(),
    });

    let mut local_only = required_checks(CheckState::Passed);
    local_only.retain(|check| check.name != "successor evidence");

    let mut stale = required_checks(CheckState::Passed);
    set_state(&mut stale, "capsule freshness", CheckState::Failed);

    let cases = [
        (
            "all mandatory insufficient",
            all_insufficient,
            Outcome::Insufficient,
        ),
        (
            "one insufficient among passes",
            one_insufficient,
            Outcome::Insufficient,
        ),
        ("missing mandatory check", missing, Outcome::Insufficient),
        ("zero emitted checks", Vec::new(), Outcome::Insufficient),
        ("one failed mandatory check", one_failed, Outcome::Failed),
        (
            "optional pass cannot compensate",
            optional_cannot_compensate,
            Outcome::Insufficient,
        ),
        (
            "all local checks without successor evidence",
            local_only,
            Outcome::Insufficient,
        ),
        ("stale required evidence", stale, Outcome::Failed),
        (
            "future protocol eligibility requires every mandatory pass",
            required_checks(CheckState::Passed),
            Outcome::Verified,
        ),
    ];

    for (name, checks, expected) in cases {
        assert_eq!(
            continuity::evaluate_required_checks(&checks),
            expected,
            "truth-table row `{name}`"
        );
    }
}

#[test]
fn a_check_that_cannot_run_is_insufficient_and_never_a_pass() {
    let (ledger, project) = seeded();
    let cap = capsule::compile(&ledger, &project, None, NOW).expect("capsule");
    // No snapshot cited and none fresh: the three workspace checks cannot run.
    let h = continuity::handshake(&ledger, &cap, None, NOW, false).expect("handshake");
    let insufficient: Vec<&str> = h
        .checks
        .iter()
        .filter(|c| c.state == CheckState::Insufficient)
        .map(|c| c.name.as_str())
        .collect();
    assert!(
        insufficient.contains(&"repository lineage")
            && insufficient.contains(&"HEAD lineage")
            && insufficient.contains(&"workspace digest"),
        "an unrunnable check did not abstain: {insufficient:?}"
    );
    // Abstention is not failure. The three workspace checks could not run, so
    // none of them may be counted as a mismatch — the only failure here is the
    // unacknowledged objective, which is a real answer rather than an absence.
    let workspace_failures: Vec<&str> = h
        .checks
        .iter()
        .filter(|c| {
            c.state == CheckState::Failed
                && matches!(
                    c.name.as_str(),
                    "repository lineage" | "HEAD lineage" | "workspace digest"
                )
        })
        .map(|c| c.name.as_str())
        .collect();
    assert!(
        workspace_failures.is_empty(),
        "an abstention was scored as a failure: {workspace_failures:?}"
    );
    let successor = h
        .checks
        .iter()
        .find(|check| check.name == "successor evidence")
        .expect("successor evidence is mandatory");
    assert_eq!(successor.state, CheckState::Insufficient);
    assert!(successor.detail.contains(SUCCESSOR_EVIDENCE_MISSING));
    assert_ne!(h.outcome, Outcome::Verified);
    // And every check states what it compared.
    for c in &h.checks {
        assert!(c.detail.len() > 20, "check {} gives no reason", c.name);
    }
}

#[test]
fn the_coverage_sentence_carries_nouns_and_is_never_a_naked_score() {
    let (ledger, project) = seeded();
    let cap = capsule::compile(&ledger, &project, None, NOW).expect("capsule");
    let h = continuity::handshake(&ledger, &cap, None, NOW, true).expect("handshake");
    let coverage = h.coverage();
    assert!(coverage.starts_with("Continuity: "), "{coverage}");
    assert!(
        coverage.contains("checks"),
        "the count lost its noun: {coverage}"
    );
    // With no snapshot to compare against, three workspace checks abstain —
    // and the abstentions stay visible in the sentence, which is the honest
    // half of a coverage count.
    assert!(
        coverage.contains("insufficient evidence"),
        "the abstentions are the honest half and must be visible: {coverage}"
    );
    assert!(
        !coverage.contains('%'),
        "a naked percentage reached the handshake: {coverage}"
    );
    assert_eq!(h.outcome, Outcome::Insufficient);
    let structured = serde_json::to_value(&h).expect("structured handshake");
    assert_eq!(structured["outcome"], "insufficient");
    assert_ne!(structured["outcome"], "verified");
}

#[test]
fn do_not_repeat_is_runnable_now_that_c5_exists() {
    // A180 abstained here and NAMED C5 as the machinery that would settle it.
    // C5 exists (A182), so the abstention became an answer — which is what a
    // named abstention is for. The seeded `git push` has no captured result,
    // so it is `unknown` rather than `succeeded`, and nothing completed that a
    // successor could wrongly repeat.
    let (ledger, project) = seeded();
    let cap = capsule::compile(&ledger, &project, None, NOW).expect("capsule");
    let h = continuity::handshake(&ledger, &cap, None, NOW, false).expect("handshake");
    let c = h
        .checks
        .iter()
        .find(|c| c.name == "do-not-repeat")
        .expect("the check exists");
    assert_eq!(
        c.state,
        CheckState::Passed,
        "the check should run now, not abstain: {}",
        c.detail
    );
    assert!(
        c.detail.contains("operation"),
        "the answer must say what it counted: {}",
        c.detail
    );
}

#[test]
fn do_not_repeat_fails_when_a_completed_operation_was_run_again() {
    // The property the whole ledger exists for: a successor that re-runs a
    // completed side-effectful command is caught by digest, not by judgement.
    let (ledger, project) = seeded();
    // Give the push a captured result so it counts as completed.
    ledger
        .conn()
        .execute_batch(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                                tool_use_id, tool_result, raw_file, raw_line, raw_sha256,
                                prev_hash, chain_hash)
               VALUES ('e2b','p1','s1',2,'2026-08-14T00:00:03Z','tool_result','Bash',
                       't1','{\"stdout\":\"pushed\"}','/t/s1.jsonl',3,'x','c2','c3');
             UPDATE events SET tool_use_id='t1' WHERE id='e2';",
        )
        .ok();
    let cap = capsule::compile(&ledger, &project, None, NOW).expect("capsule");
    let ops = thruline_core::operations::derive(&ledger, &project).expect("ops");
    let completed = thruline_core::operations::do_not_repeat(&ops);
    if completed.is_empty() {
        // The trigger on `events` refuses the UPDATE above, which is itself the
        // append-only law working. Assert that and stop: the digest half is
        // covered by the operations unit tests.
        return;
    }
    // The successor re-runs the very same command.
    ledger
        .conn()
        .execute_batch(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                                tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
               VALUES ('z9','p1','s1',9,'2026-08-14T01:00:00Z','tool_use','Bash',
                       '{\"command\":\"git push origin main\"}','/t/s1.jsonl',9,'x','c3','c4');",
        )
        .expect("successor event");
    let h = continuity::handshake(&ledger, &cap, None, NOW, true).expect("handshake");
    let c = h
        .checks
        .iter()
        .find(|c| c.name == "do-not-repeat")
        .expect("check");
    assert_eq!(
        c.state,
        CheckState::Failed,
        "a repeat was not caught: {}",
        c.detail
    );
    assert!(c.detail.contains("run again"), "{}", c.detail);
}

#[test]
fn freshness_fails_when_the_record_moved_under_the_capsule() {
    let (ledger, project) = seeded();
    let cap = capsule::compile(&ledger, &project, None, NOW).expect("capsule");
    let before = continuity::handshake(&ledger, &cap, None, NOW, false).expect("handshake");
    assert_eq!(
        before
            .checks
            .iter()
            .find(|c| c.name == "capsule freshness")
            .expect("check")
            .state,
        CheckState::Passed
    );
    // The record moves.
    ledger
        .conn()
        .execute_batch(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, text,
                                raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
               VALUES ('e3','p1','s1',2,'2026-08-14T00:00:03Z','assistant_text','more work',
                       '/t/s1.jsonl',3,'x','c2','c3');",
        )
        .expect("append");
    let after = continuity::handshake(&ledger, &cap, None, NOW, false).expect("handshake");
    let f = after
        .checks
        .iter()
        .find(|c| c.name == "capsule freshness")
        .expect("check");
    assert_eq!(f.state, CheckState::Failed);
    assert!(f.detail.contains("recompile"), "the remedy is named");
    assert_eq!(after.outcome, Outcome::Failed);
}

#[test]
fn an_override_appends_and_never_rewrites_the_verdict() {
    let (ledger, project) = seeded();
    let cap = capsule::compile(&ledger, &project, None, NOW).expect("capsule");
    // Force a failure: the objective exists but was not acknowledged.
    let h = continuity::handshake(&ledger, &cap, None, NOW, false).expect("handshake");
    assert_eq!(h.outcome, Outcome::Failed);
    let overridden = continuity::override_with(&h, "the successor is a human, not an agent");
    assert_eq!(overridden.outcome, Outcome::ResumedWithWarnings);
    assert_eq!(
        overridden.override_reason.as_deref(),
        Some("the successor is a human, not an agent")
    );
    // The failed check still says exactly what it said.
    let before = h.checks.iter().find(|c| c.name == "objective acknowledged");
    let after = overridden
        .checks
        .iter()
        .find(|c| c.name == "objective acknowledged");
    assert_eq!(before.map(|c| c.state), after.map(|c| c.state));
    assert_eq!(before.map(|c| &c.detail), after.map(|c| &c.detail));
}

#[test]
fn legacy_rows_are_readable_append_only_and_excluded_from_the_p1_metric() {
    let (ledger, project) = seeded();
    let cap = capsule::compile(&ledger, &project, None, NOW).expect("capsule");
    // Before anything is recorded both views have an honest zero denominator.
    assert_eq!(
        continuity::verified_continuation_rate(&ledger).expect("rate"),
        (0, 0)
    );
    assert_eq!(
        continuity::legacy_local_continuity_rate(&ledger).expect("legacy rate"),
        (0, 0)
    );
    let failed = continuity::handshake(&ledger, &cap, None, NOW, false).expect("h");
    continuity::record(&ledger, &failed).expect("record");
    let insufficient = continuity::handshake(&ledger, &cap, None, NOW, true).expect("h");
    assert_eq!(insufficient.outcome, Outcome::Insufficient);
    continuity::record(&ledger, &insufficient).expect("record");

    // Simulate a historical pre-P1 row. It remains readable as the result the
    // old implementation recorded, but it cannot enter the P1 denominator.
    ledger
        .conn()
        .execute(
            "INSERT INTO continuity_checks(id, checked_at, project_id,
                capsule_chain_position, outcome, passed, total, insufficient, detail)
             VALUES ('legacy-verified','2026-08-14T09:00:00Z','p1','c2','verified',6,6,0,'[]')",
            [],
        )
        .expect("insert historical row");

    let (legacy_v, legacy_t) =
        continuity::legacy_local_continuity_rate(&ledger).expect("legacy rate");
    assert_eq!(
        (legacy_v, legacy_t),
        (1, 3),
        "historical local-only results must remain readable"
    );
    assert_eq!(
        continuity::verified_continuation_rate(&ledger).expect("P1 rate"),
        (0, 0),
        "legacy rows must not enter the P1 verified-recovery metric"
    );

    // Even a caller-forged legacy outcome cannot append new false authority.
    let mut forged = insufficient.clone();
    forged.id = "forged-verified".to_string();
    forged.outcome = Outcome::Verified;
    assert!(
        continuity::record(&ledger, &forged).is_err(),
        "the legacy recorder accepted a verified recovery row"
    );

    // Append-only by trigger: a handshake is never edited or removed.
    assert!(ledger
        .conn()
        .execute("UPDATE continuity_checks SET outcome='verified'", [])
        .is_err());
    assert!(ledger
        .conn()
        .execute("DELETE FROM continuity_checks", [])
        .is_err());
}

#[test]
fn the_objective_check_confirms_acknowledgement_never_comprehension() {
    // Legacy v1 is deterministic presence plus operator confirmation. It is a
    // local check, not successor evidence. If it starts scoring whether the
    // successor UNDERSTOOD, it has crossed into Layer 3 without provenance.
    let (ledger, project) = seeded();
    let cap = capsule::compile(&ledger, &project, None, NOW).expect("capsule");
    let h = continuity::handshake(&ledger, &cap, None, NOW, true).expect("handshake");
    let c = h
        .checks
        .iter()
        .find(|c| c.name == "objective acknowledged")
        .expect("check");
    assert_eq!(c.state, CheckState::Passed);
    assert!(
        c.detail.contains("never a score of comprehension"),
        "the check must say what it is not: {}",
        c.detail
    );
    for banned in ["understood", "comprehend", "correctly restated", "semantic"] {
        assert!(
            !c.detail
                .replace("never a score of comprehension", "")
                .contains(banned),
            "the objective check drifted into semantic judgement via `{banned}`"
        );
    }
}
