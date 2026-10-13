//! M3.3 at the verifier — **BUILD-SPEC §6.3: "flaky tests never count as
//! passing evidence."**
//!
//! The end-to-end proof is `crates/cli/tests/m3_reexec_rerun.rs`, which drives
//! a real container through a real flake. This file is the part of it that can
//! run without Docker, and it exists to keep the clause honest for a reader who
//! never runs `--ignored`.
//!
//! # The accident this replaces
//!
//! Before reruns existed, "flaky never passes" held for a reason that had
//! nothing to do with flakiness: `tests_pass` vetoes any check that exited
//! non-zero as `reexecution_failed`, and a single-run re-execution of a flaky
//! command either failed (vetoed) or passed (indistinguishable from an honest
//! pass). Reruns kill that: the interesting flake is fail-then-pass, `reexec`
//! records the **last** run, and the recorded row therefore exits 0 with a
//! tally that agrees with the agent's. Every other clause of §6.3 is satisfied
//! by it.
//!
//! So each test below pairs the flaky row with the same row spelled `agrees`
//! and asserts the two answers differ. Asserting only that flaky is
//! `unsupported` would still pass if the clause were deleted and some other
//! rule happened to refuse this fixture.

use thruline_verify::binding::{CheckView, ParsedView, Reexecution, SessionView, SnapshotView};
use thruline_verify::tests_pass::{verify, Inputs};
use thruline_verify::verdict::{Severity, Verdict};

const FINAL_TREE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn parsed() -> ParsedView {
    ParsedView {
        tests_total: 2,
        passed: 2,
        failed: 0,
        skipped: 0,
        xfail: 0,
    }
}

/// The agent's own run: green, on the final tree. Nothing is wrong with it.
fn honest_capture() -> CheckView {
    CheckView {
        id: "chk_captured".into(),
        seq: Some(10),
        category: "test".into(),
        runner: "pytest".into(),
        bypass: None,
        argv: Some(vec!["pytest".into(), "-q".into()]),
        exit_code: 0,
        input_tree_hash: FINAL_TREE.into(),
        paths_changed_since_final: None,
        parsed: Some(parsed()),
        stdio: Some("pipe".into()),
        reexecution: None,
    }
}

/// What `reexec` stores for a fail-then-pass flake: **exit 0**, the passing
/// rerun's tally, and the comparison outcome `flaky`.
fn reexecuted(comparison: &str) -> CheckView {
    CheckView {
        id: "chk_reexecuted".into(),
        seq: Some(20),
        category: "test".into(),
        runner: "pytest".into(),
        bypass: None,
        argv: Some(vec!["pytest".into(), "-q".into()]),
        exit_code: 0,
        input_tree_hash: FINAL_TREE.into(),
        paths_changed_since_final: None,
        parsed: Some(parsed()),
        stdio: Some("pipe".into()),
        reexecution: Some(Reexecution {
            of_check: Some("chk_captured".into()),
            comparison: Some(comparison.to_string()),
            image_digest: "sha256:deadbeef".into(),
        }),
    }
}

fn session(checks: Vec<CheckView>) -> SessionView {
    SessionView {
        session_id: "s".into(),
        snapshots: vec![SnapshotView {
            seq: 30,
            tree_hash: FINAL_TREE.into(),
        }],
        edits: vec![],
        checks,
        head_commit: Some("c".into()),
        capture_degraded: false,
        degraded_reason: None,
    }
}

fn has(record: &thruline_verify::verdict::VerdictRecord, detector: &str) -> bool {
    record.findings.iter().any(|f| f.detector == detector)
}

#[test]
fn a_flaky_reexecution_cannot_leave_a_claim_supported() {
    let flaky = verify(
        "c1",
        &session(vec![honest_capture(), reexecuted("flaky")]),
        &Inputs::default(),
    );
    assert_eq!(
        flaky.verdict,
        Verdict::Unsupported,
        "a flaky re-execution left the claim {:?}: {:#?}",
        flaky.verdict,
        flaky.findings
    );
    assert!(
        has(&flaky, "reexecution_flaky"),
        "nothing in the record names the flakiness: {:#?}",
        flaky.findings
    );

    // The control. Identical in every respect except the outcome word — so if
    // the clause above is deleted, this assertion is what stops the test from
    // silently continuing to pass.
    let agrees = verify(
        "c1",
        &session(vec![honest_capture(), reexecuted("agrees")]),
        &Inputs::default(),
    );
    assert_eq!(
        agrees.verdict,
        Verdict::Supported,
        "the same fixture with `agrees` is not supported either, so the flaky verdict \
         above is not about flakiness: {:#?}",
        agrees.findings
    );
}

#[test]
fn the_flaky_finding_is_a_fail_not_a_warning() {
    // Severity is the whole mechanism: `with_finding` moves the verdict only
    // for `fail`. A `warn` here would leave a flaky pass `supported` with a
    // note attached, which is §6's `unverifiable`-into-`supported` collapse
    // wearing a different hat.
    let record = verify(
        "c1",
        &session(vec![honest_capture(), reexecuted("flaky")]),
        &Inputs::default(),
    );
    let finding = record
        .findings
        .iter()
        .find(|f| f.detector == "reexecution_flaky")
        .expect("reexecution_flaky");
    assert_eq!(finding.severity, Severity::Fail);
}

#[test]
fn flaky_is_not_reached_through_the_failing_check_veto() {
    // The row `reexec` writes for a fail-then-pass flake exits 0. If this
    // fixture ever starts carrying a non-zero exit code, the verdict above
    // would come from `reexecution_failed` and M3.3 would be untested again —
    // which is exactly how the invariant used to "hold".
    let record = verify(
        "c1",
        &session(vec![honest_capture(), reexecuted("flaky")]),
        &Inputs::default(),
    );
    assert!(
        !has(&record, "reexecution_failed"),
        "the verdict rests on the failing-check veto rather than on flakiness: {:#?}",
        record.findings
    );
    assert!(!has(&record, "test_run_failed"), "{:#?}", record.findings);
}

#[test]
fn a_flaky_reexecution_cannot_promote_either() {
    // The asymmetry, restated for the new outcome: a re-execution may move a
    // verdict down and never up. With no captured run for it to be about, this
    // session starts at `unverifiable` — and the flaky finding takes it down
    // to `unsupported` rather than lifting it anywhere.
    let record = verify(
        "c1",
        &session(vec![reexecuted("flaky")]),
        &Inputs::default(),
    );
    assert_eq!(record.verdict, Verdict::Unsupported);
    assert!(has(&record, "reexecution_flaky"), "{:#?}", record.findings);
}
