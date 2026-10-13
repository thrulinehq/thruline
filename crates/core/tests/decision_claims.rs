//! J1–J6 read typed claim and run receipts, never agent prose or a model.

use thruline_core::decisions::checks::VerdictState;
use thruline_core::decisions::checks_claims::{
    check_decision_satisfied, check_delegated_run, check_edit_during_run, check_empty_run,
    check_stale_claim, check_summed_totals, ClaimCensus, ClaimEdit, ClaimKind, ClaimReceipt,
    ClaimRun, DecisionClaimBinding, RunOrigin,
};

const VERSION: &str = "v2";
const TREE: &str = "tree-v2";

fn base() -> ClaimCensus {
    ClaimCensus {
        complete: true,
        final_seq: 9,
        claim: ClaimReceipt {
            id: "claim1".into(),
            session_id: "agent-a".into(),
            seq: 3,
            version_id: VERSION.into(),
            tree_hash: TREE.into(),
            kind: ClaimKind::TestsPass,
            reported_passed: Some(3),
            bound_run_id: Some("run1".into()),
        },
        decision: None,
        runs: vec![ClaimRun {
            id: "run1".into(),
            session_id: "agent-a".into(),
            seq: 2,
            version_id: VERSION.into(),
            tree_hash: TREE.into(),
            origin: RunOrigin::SameSession,
            passed: Some(3),
            failed: Some(0),
            exit_code: Some(0),
            started_ms: Some(10),
            ended_ms: Some(20),
            examined_paths: vec!["src/total.rs".into()],
            complete: true,
        }],
        edits: Vec::new(),
    }
}

#[test]
fn j1_compares_only_explicit_accepted_binding_on_claim_tree() {
    let mut read = base();
    read.claim.kind = ClaimKind::DecisionSatisfied;
    read.decision = Some(DecisionClaimBinding {
        decision_id: "decision1".into(),
        revision: 1,
        accepted: true,
        binding_basis: "accepted_term_overlap".into(),
        version_id: VERSION.into(),
        tree_hash: TREE.into(),
        verdict: Some(VerdictState::Holds),
    });
    assert_eq!(check_decision_satisfied(&read).state, VerdictState::Holds);
    read.decision.as_mut().unwrap().verdict = Some(VerdictState::Broken);
    let mismatch = check_decision_satisfied(&read);
    assert_eq!(mismatch.state, VerdictState::Broken);
    assert!(mismatch.reason.unwrap().contains("no proof found"));
    read.decision.as_mut().unwrap().tree_hash = "other-tree".into();
    assert_eq!(
        check_decision_satisfied(&read).state,
        VerdictState::Unverifiable
    );
}

#[test]
fn j2_later_relevant_edit_without_rerun_makes_claim_stale() {
    let mut read = base();
    assert_eq!(check_stale_claim(&read).state, VerdictState::Holds);
    read.edits.push(ClaimEdit {
        id: "edit1".into(),
        session_id: "agent-a".into(),
        seq: 5,
        version_id: "v3".into(),
        tree_hash: "tree-v3".into(),
        path: "src/total.rs".into(),
        at_ms: Some(25),
        relevant: true,
    });
    assert_eq!(check_stale_claim(&read).state, VerdictState::Unverifiable);
    read.runs.push(ClaimRun {
        id: "run2".into(),
        session_id: "agent-a".into(),
        seq: 6,
        version_id: "v3".into(),
        tree_hash: "tree-v3".into(),
        origin: RunOrigin::SameSession,
        passed: Some(3),
        failed: Some(0),
        exit_code: Some(0),
        started_ms: Some(30),
        ended_ms: Some(40),
        examined_paths: vec!["src/total.rs".into()],
        complete: true,
    });
    assert_eq!(check_stale_claim(&read).state, VerdictState::Holds);
    read.complete = false;
    assert_eq!(check_stale_claim(&read).state, VerdictState::Unverifiable);
}

#[test]
fn j3_rejects_a_sum_that_no_single_run_reports() {
    let mut read = base();
    read.claim.reported_passed = Some(6);
    read.runs.push(ClaimRun {
        id: "run2".into(),
        session_id: "agent-a".into(),
        seq: 4,
        passed: Some(3),
        ..read.runs[0].clone()
    });
    let summed = check_summed_totals(&read);
    assert_eq!(summed.state, VerdictState::Unverifiable);
    assert!(summed.reason.unwrap().contains("no single run"));
    read.runs[1].passed = Some(6);
    assert_eq!(check_summed_totals(&read).state, VerdictState::Holds);
}

#[test]
fn j4_empty_or_failing_bound_run_cannot_prove_tests_pass() {
    let mut read = base();
    assert_eq!(check_empty_run(&read).state, VerdictState::Holds);
    read.runs[0].passed = Some(0);
    assert_eq!(check_empty_run(&read).state, VerdictState::Broken);
    read.runs[0].passed = None;
    assert_eq!(check_empty_run(&read).state, VerdictState::Unverifiable);
}

#[test]
fn j5_delegated_run_is_only_a_pointer_outside_this_record() {
    let mut read = base();
    assert_eq!(check_delegated_run(&read).state, VerdictState::Holds);
    read.runs[0].origin = RunOrigin::Ci;
    assert_eq!(check_delegated_run(&read).state, VerdictState::Unverifiable);
    read.runs[0].origin = RunOrigin::Unknown;
    assert_eq!(check_delegated_run(&read).state, VerdictState::Unverifiable);
}

#[test]
fn j6_edit_inside_run_interval_invalidates_that_file_evidence() {
    let mut read = base();
    assert_eq!(check_edit_during_run(&read).state, VerdictState::Holds);
    read.edits.push(ClaimEdit {
        id: "edit1".into(),
        session_id: "agent-a".into(),
        seq: 4,
        version_id: VERSION.into(),
        tree_hash: TREE.into(),
        path: "src/total.rs".into(),
        at_ms: Some(15),
        relevant: true,
    });
    assert_eq!(
        check_edit_during_run(&read).state,
        VerdictState::Unverifiable
    );
    read.edits[0].at_ms = Some(20);
    assert_eq!(check_edit_during_run(&read).state, VerdictState::Holds);
    read.runs[0].ended_ms = None;
    assert_eq!(
        check_edit_during_run(&read).state,
        VerdictState::Unverifiable
    );
}
