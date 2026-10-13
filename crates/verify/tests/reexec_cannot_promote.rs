//! **A re-execution confirms or contradicts. It never promotes.**
//!
//! # The defect these tests are against, reproduced before they were written
//!
//! `thruline reexec` was a verdict-upgrading operation. Measured end to end on
//! a real session (pytest through the shim, a `printf >>` source edit
//! afterwards, `thruline fold`):
//!
//! ```text
//! before:  tests_pass -> unsupported
//!          binding: superseded_by_unobserved_change { path: "shipping.py" }
//!          finding: fail tests_ran_before_final_edit
//!
//! after `thruline reexec`:
//!          tests_pass -> supported
//!          binding: ran_on_final_tree
//!          (the fail finding is GONE)
//! ```
//!
//! The mechanism was three lines in three files agreeing with each other:
//! `reexec` inserts a `Check` row stamped with the session's **final tree
//! hash**; `bind` matched §6.1 rule 1 on that stamp and returned
//! `RanOnFinalTree`; `tests_pass` preferred `RanOnFinalTree` over everything.
//! The stamp is written by the re-executor, so rule 1's equality was not an
//! observation about where the check ran — it was a restatement of the
//! re-executor's own choice. **A guard whose input the guarded party supplies
//! is not a guard.**
//!
//! Every test below pairs the fixed behaviour with the *pre-fix* fixture
//! (`reexecution: None`, which is what the view builder produced before it read
//! the row's `reexec` block) and asserts the two answers differ. A test that
//! only asserted the good answer would still pass if the distinction were
//! deleted, because the good answer is also what an unconditional
//! `RanOnFinalTree` produces for an honest session.

use thruline_verify::binding::{
    bind, Binding, CheckView, ParsedView, Reexecution, SessionView, SnapshotView,
};
use thruline_verify::tests_pass::{verify, Inputs};
use thruline_verify::verdict::{Severity, Verdict};

const EARLY_TREE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
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

/// The agent's own run: passed, but on the tree *before* the last source edit.
fn superseded_capture() -> CheckView {
    CheckView {
        id: "chk_captured".into(),
        seq: Some(10),
        category: "test".into(),
        runner: "pytest".into(),
        bypass: None,
        argv: Some(vec!["pytest".into(), "-q".into()]),
        exit_code: 0,
        input_tree_hash: EARLY_TREE.into(),
        // The tree says a non-test source file changed after this check ran and
        // no edit event named it — §6.1's `SupersededByUnobservedChange`.
        paths_changed_since_final: Some(vec!["shipping.py".into()]),
        parsed: Some(parsed()),
        stdio: Some("pipe".into()),
        reexecution: None,
    }
}

/// A captured run that really did run on the final tree.
fn bound_capture() -> CheckView {
    CheckView {
        input_tree_hash: FINAL_TREE.into(),
        paths_changed_since_final: None,
        ..superseded_capture()
    }
}

/// What `thruline reexec` inserts: a passing row stamped with the final tree.
fn reexecuted(comparison: Option<&str>, exit_code: i64) -> CheckView {
    CheckView {
        id: "chk_reexecuted".into(),
        seq: Some(20),
        category: "test".into(),
        runner: "pytest".into(),
        bypass: None,
        argv: Some(vec!["pytest".into(), "-q".into()]),
        exit_code,
        input_tree_hash: FINAL_TREE.into(),
        paths_changed_since_final: None,
        parsed: Some(parsed()),
        stdio: Some("pipe".into()),
        reexecution: Some(Reexecution {
            of_check: Some("chk_captured".into()),
            comparison: comparison.map(str::to_string),
            image_digest: "sha256:deadbeef".into(),
        }),
    }
}

fn session(checks: Vec<CheckView>) -> SessionView {
    SessionView {
        session_id: "s".into(),
        snapshots: vec![
            SnapshotView {
                seq: 5,
                tree_hash: EARLY_TREE.into(),
            },
            SnapshotView {
                seq: 30,
                tree_hash: FINAL_TREE.into(),
            },
        ],
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

// ---------------------------------------------------------------------------
// §6.1 — the binding itself.
// ---------------------------------------------------------------------------

#[test]
fn a_reexecuted_row_does_not_bind_as_having_run_on_the_final_tree() {
    let s = session(vec![reexecuted(Some("agrees"), 0)]);
    let binding = bind(&s, &s.checks[0]);
    assert!(
        matches!(&binding, Binding::Reexecuted { underlying, .. }
            if **underlying == Binding::RanOnFinalTree),
        "a re-executed row bound as {binding:?}"
    );
    assert!(
        !binding.counts(),
        "a re-execution counts as §6.1 binding, so it can carry a claim on its own"
    );

    // **The mutation.** Strip the provenance — which is exactly the state the
    // view builder produced before it read the row's `reexec` block — and the
    // identical row binds as `RanOnFinalTree`. That difference is the whole
    // fix; if this assertion ever fails, `bind` has stopped distinguishing the
    // two and the laundering path is open again.
    let mut without = s.checks[0].clone();
    without.reexecution = None;
    assert_eq!(
        bind(&session(vec![without]), &reexecuted(Some("agrees"), 0)),
        Binding::Reexecuted {
            of_check: Some("chk_captured".into()),
            comparison: Some("agrees".into()),
            underlying: Box::new(Binding::RanOnFinalTree),
        }
    );
    let mut without = reexecuted(Some("agrees"), 0);
    without.reexecution = None;
    let s2 = session(vec![without.clone()]);
    assert_eq!(
        bind(&s2, &without),
        Binding::RanOnFinalTree,
        "the pre-fix fixture no longer reproduces the pre-fix binding, so this test \
         is no longer measuring the distinction it claims to measure"
    );
}

// ---------------------------------------------------------------------------
// §6.3 — what the verifier does with it.
// ---------------------------------------------------------------------------

#[test]
fn a_correctly_unsupported_session_stays_unsupported_after_reexec() {
    let inputs = Inputs::default();

    // Before: the agent's run is superseded, and that is the whole record.
    let before = verify("clm", &session(vec![superseded_capture()]), &inputs);
    assert_eq!(before.verdict, Verdict::Unsupported);
    assert!(has(&before, "tests_ran_before_final_edit"));

    // After `thruline reexec`: a passing, AGREEING re-execution at the final
    // tree is now in the store beside it.
    let after = verify(
        "clm",
        &session(vec![superseded_capture(), reexecuted(Some("agrees"), 0)]),
        &inputs,
    );
    assert_eq!(
        after.verdict,
        Verdict::Unsupported,
        "`thruline reexec` upgraded the verdict: {after:#?}"
    );
    assert!(
        has(&after, "tests_ran_before_final_edit"),
        "the supersession finding was dropped when the re-execution arrived — the \
         reader is told the claim is unsupported but no longer told why: {after:#?}"
    );
    assert!(
        has(&after, "reexecution_did_not_promote"),
        "the record does not say a re-execution ran and did not change the answer, so \
         the next reader will run it again and believe the better number: {after:#?}"
    );
    assert!(
        has(&after, "reexecution_confirms"),
        "the re-execution's agreement is not in the record at all: {after:#?}"
    );
    assert_eq!(
        after.binding, before.binding,
        "the binding recorded on the verdict changed when a re-execution arrived; \
         §6.1 says record WHICH RULE applied, and the rule that applied is the one \
         about the agent's own run"
    );

    // **The mutation.** With the provenance stripped from the same row — the
    // pre-fix world — the verdict flips. This is the reproduction, kept
    // executable.
    let mut laundered = reexecuted(Some("agrees"), 0);
    laundered.reexecution = None;
    let pre_fix = verify(
        "clm",
        &session(vec![superseded_capture(), laundered]),
        &inputs,
    );
    assert_eq!(
        pre_fix.verdict,
        Verdict::Supported,
        "the pre-fix fixture no longer reproduces the upgrade, so this test no longer \
         demonstrates that the provenance field is what stops it"
    );
    assert!(
        !has(&pre_fix, "tests_ran_before_final_edit"),
        "the pre-fix fixture no longer reproduces the dropped finding"
    );
}

#[test]
fn a_re_execution_alone_is_never_evidence_for_a_claim() {
    // §7 re-runs *claimed* checks. A session whose only test row is a
    // re-execution has nothing the re-execution could be about — and the
    // honest answer is `unverifiable`, which §6 forbids collapsing into
    // `supported`.
    let record = verify(
        "clm",
        &session(vec![reexecuted(Some("agrees"), 0)]),
        &Inputs::default(),
    );
    assert_eq!(record.verdict, Verdict::Unverifiable, "{record:#?}");
    assert!(
        record.evidence.contains(&"chk_reexecuted".to_string()),
        "the re-execution is not even cited: {record:#?}"
    );
}

#[test]
fn a_re_execution_may_still_contradict() {
    // The other half of "confirm or contradict". A re-execution that
    // disagreed, or that failed at the final tree, refutes the claim it was
    // re-running — and this direction must keep working, or the fix above has
    // simply made `reexec` inert.
    let disagreed = verify(
        "clm",
        &session(vec![bound_capture(), reexecuted(Some("disagrees"), 0)]),
        &Inputs::default(),
    );
    assert_eq!(disagreed.verdict, Verdict::Unsupported, "{disagreed:#?}");
    assert!(has(&disagreed, "reexecution_contradicts"), "{disagreed:#?}");

    let failed = verify(
        "clm",
        &session(vec![bound_capture(), reexecuted(Some("disagrees"), 1)]),
        &Inputs::default(),
    );
    assert_eq!(failed.verdict, Verdict::Unsupported, "{failed:#?}");
    assert!(
        has(&failed, "reexecution_failed"),
        "a re-execution that failed at the final tree is reported as a captured run \
         failing, which misplaces the fact: {failed:#?}"
    );
}

#[test]
fn a_confirming_re_execution_does_not_spoil_an_honest_session() {
    // The control that keeps the rule from being "refuse everything". A
    // captured run bound to the final tree, confirmed by a re-execution, is
    // still `supported` — and the confirmation is recorded as `info`, not as a
    // reason to doubt it. A guard that also refuses honest evidence is not a
    // guard, it is an outage.
    let record = verify(
        "clm",
        &session(vec![bound_capture(), reexecuted(Some("agrees"), 0)]),
        &Inputs::default(),
    );
    assert_eq!(record.verdict, Verdict::Supported, "{record:#?}");
    assert!(has(&record, "reexecution_confirms"), "{record:#?}");
    assert!(
        !has(&record, "reexecution_did_not_promote"),
        "an honest session is told its re-execution did not promote anything, which is \
         true and useless: {record:#?}"
    );
    assert_eq!(record.binding, Some(Binding::RanOnFinalTree));
}

#[test]
fn an_uncompared_or_incomparable_re_execution_is_not_agreement() {
    // §7's three-way distinction, carried into §6.3: `--mode full` made no
    // comparison, and `not_comparable` made one that could not be read. Neither
    // is agreement, and neither may be silent.
    let full = verify(
        "clm",
        &session(vec![bound_capture(), reexecuted(None, 0)]),
        &Inputs::default(),
    );
    assert!(has(&full, "reexecution_uncompared"), "{full:#?}");

    let nc = verify(
        "clm",
        &session(vec![bound_capture(), reexecuted(Some("not_comparable"), 0)]),
        &Inputs::default(),
    );
    assert!(has(&nc, "reexecution_not_comparable"), "{nc:#?}");
    for record in [&full, &nc] {
        assert!(
            !has(record, "reexecution_confirms"),
            "a run that was never compared is recorded as confirming: {record:#?}"
        );
        // Warn, not fail: the captured evidence stands on its own, and an
        // absent comparison is not evidence against it.
        assert!(record
            .findings
            .iter()
            .filter(|f| f.detector.starts_with("reexecution_"))
            .all(|f| f.severity == Severity::Warn));
    }
}
