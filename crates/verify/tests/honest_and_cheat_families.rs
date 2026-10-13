//! The families, not the one case.
//!
//! The previous control was a single honest edit — changing an expected literal
//! — and that is the **one** honest shape that trips no `fail` detector, which
//! is exactly why it passed while four others failed. Measured on the same
//! harness: a refactor that renames tests, a deliberate `skipif`, one test split
//! into two, and a flaky test marked `xfail` were **all** reported
//! `unsupported`. Three `fail` detectors fired on routine refactoring while the
//! one `warn` detector caught the realistic cheat.
//!
//! So both families are built here, end to end through the verifier — because
//! two of the severities are decided by the *run*, not by the detector, and a
//! detector-only test cannot see that.

use thruline_verify::binding::{CheckView, ParsedView, SessionView, SnapshotView};
use thruline_verify::tamper::{diff_test_file, Language};
use thruline_verify::tests_pass::{verify, Inputs};
use thruline_verify::verdict::{Severity, Verdict};

const BASE: &str = "\
def test_rate_small():
    assert rate(2) == 9.0

def test_rate_large():
    assert rate(10) == 29.0
";

/// A session whose full suite passed on the final tree, with `skipped` from the
/// run — the fact that decides `skip_marker_added`.
fn session(skipped: u64, xfail: u64) -> SessionView {
    SessionView {
        session_id: "s".into(),
        snapshots: vec![SnapshotView {
            seq: 10,
            tree_hash: "t".into(),
        }],
        edits: vec![],
        checks: vec![CheckView {
            id: "chk".into(),
            seq: Some(5),
            category: "test".into(),
            runner: "pytest".into(),
            bypass: None,
            argv: Some(vec!["pytest".into(), "-q".into()]),
            exit_code: 0,
            input_tree_hash: "t".into(),
            paths_changed_since_final: None,
            stdio: Some("pipe".into()),
            reexecution: None,
            parsed: Some(ParsedView {
                tests_total: 2,
                passed: 2 - skipped.min(2),
                failed: 0,
                skipped,
                xfail,
            }),
        }],
        head_commit: Some("c".into()),
        capture_degraded: false,
        degraded_reason: None,
    }
}

/// Run a test-file edit through the detectors and then through the verifier,
/// exactly as the CLI does.
fn verdict_for(head: &str, skipped: u64, xfail: u64) -> (Verdict, Vec<(String, Severity)>) {
    let findings = diff_test_file("test_shipping.py", BASE, head, Language::Python).expect("diff");
    let record = verify(
        "clm",
        &session(skipped, xfail),
        &Inputs {
            scope_is_all: true,
            tamper_findings: findings,
            source_changed: true,
            test_files_changed: vec!["test_shipping.py".into()],
            coverage: None,
        },
    );
    let seen = record
        .findings
        .iter()
        .map(|f| (f.detector.clone(), f.severity))
        .collect();
    (record.verdict, seen)
}

// ---------------------------------------------------------------------------
// The honest family — every one of these was `unsupported` before
// ---------------------------------------------------------------------------

#[test]
fn honest_a_refactor_that_renames_both_tests_is_supported() {
    let head = "\
def test_shipping_rate_for_a_small_parcel():
    assert rate(2) == 9.0

def test_shipping_rate_for_a_large_parcel():
    assert rate(10) == 29.0
";
    let (verdict, findings) = verdict_for(head, 0, 0);
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
}

#[test]
fn honest_a_deliberate_skipif_that_skips_nothing_here_is_supported() {
    let head = "\
import sys, pytest

@pytest.mark.skipif(sys.platform == \"win32\", reason=\"posix paths\")
def test_rate_small():
    assert rate(2) == 9.0

def test_rate_large():
    assert rate(10) == 29.0
";
    // The run skipped nothing: this is not win32.
    let (verdict, findings) = verdict_for(head, 0, 0);
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        findings
            .iter()
            .any(|(d, s)| d == "skip_marker_added" && *s == Severity::Info),
        "the marker should still be REPORTED, just not as a refutation: {findings:?}"
    );
}

#[test]
fn honest_one_test_split_into_two_is_supported() {
    let head = "\
def test_rate_small():
    assert rate(2) == 9.0

def test_rate_large_base():
    assert rate(10) == 29.0

def test_rate_large_rounding():
    assert rate(10.004) == 29.01
";
    let (verdict, findings) = verdict_for(head, 0, 0);
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
}

#[test]
fn honest_a_flaky_test_marked_xfail_is_supported_with_a_warning() {
    let head = "\
import pytest

def test_rate_small():
    assert rate(2) == 9.0

@pytest.mark.xfail(reason=\"upstream rounding is unstable\")
def test_rate_large():
    assert rate(10) == 29.0
";
    // `xfail` shows in the run as an xfail, not a skip. §6.3 already warns
    // about it; counting it in `skip_marker_added` too would refute the claim
    // twice over for one honest decision.
    let (verdict, findings) = verdict_for(head, 0, 1);
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        findings.iter().any(|(d, _)| d == "skipped_tests"),
        "the xfail was not surfaced at all: {findings:?}"
    );
}

#[test]
fn honest_a_bug_fix_that_changes_an_expected_literal_is_supported() {
    let head = "\
def test_rate_small():
    assert rate(2) == 12.5

def test_rate_large():
    assert rate(10) == 32.5
";
    let (verdict, findings) = verdict_for(head, 0, 0);
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
}

// ---------------------------------------------------------------------------
// The cheat family — two of these were `supported` before
// ---------------------------------------------------------------------------

#[test]
fn cheat_an_assertion_loosened_to_an_inequality_is_refuted() {
    // Was `supported`: it raised `assertion_weakened` twice and passed anyway.
    // An equality loosened to something that asserts nothing is a removal
    // wearing an assert's clothes.
    let head = "\
def test_rate_small():
    assert rate(2) > 0

def test_rate_large():
    assert rate(10) > 0
";
    let (verdict, findings) = verdict_for(head, 0, 0);
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(findings
        .iter()
        .any(|(d, s)| d == "assertion_weakened" && *s == Severity::Fail));
}

#[test]
fn cheat_an_assertion_moved_into_a_helper_that_always_passes_is_refuted() {
    // Was `supported`: the test still has an `assert`, so nothing was
    // "removed" — but it asserts a call that cannot fail.
    let head = "\
def _check(x):
    return True

def test_rate_small():
    assert _check(rate(2))

def test_rate_large():
    assert _check(rate(10))
";
    let (verdict, findings) = verdict_for(head, 0, 0);
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
}

#[test]
fn cheat_a_skip_marker_that_actually_skips_is_refuted() {
    let head = "\
import pytest

@pytest.mark.skip(reason=\"flaky\")
def test_rate_small():
    assert rate(2) == 9.0

def test_rate_large():
    assert rate(10) == 29.0
";
    // The run reports a skip, so the marker took effect.
    let (verdict, findings) = verdict_for(head, 1, 0);
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(findings
        .iter()
        .any(|(d, s)| d == "skip_marker_added" && *s == Severity::Fail));
}

#[test]
fn cheat_a_test_deleted_outright_is_refuted() {
    let head = "def test_rate_small():\n    assert rate(2) == 9.0\n";
    let (verdict, findings) = verdict_for(head, 0, 0);
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(findings.iter().any(|(d, _)| d == "test_deleted"));
}

#[test]
fn cheat_an_assertion_deleted_from_a_body_is_refuted() {
    let head = "\
def test_rate_small():
    assert rate(2) == 9.0

def test_rate_large():
    rate(10)
";
    let (verdict, findings) = verdict_for(head, 0, 0);
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(findings.iter().any(|(d, _)| d == "assertion_removed"));
}

// ---------------------------------------------------------------------------
// The families are the point: neither may be empty, and they must disagree
// ---------------------------------------------------------------------------

#[test]
fn the_two_families_are_separated_and_neither_is_trivially_satisfied() {
    // The guard against a verifier tuned until one family passes. If every
    // honest case were `unsupported`, or every cheat `supported`, the
    // per-case tests above could each be "fixed" one at a time without the
    // detector discriminating at all.
    let honest = [
        "def test_shipping_small():\n    assert rate(2) == 9.0\n\ndef test_shipping_large():\n    assert rate(10) == 29.0\n",
        "def test_rate_small():\n    assert rate(2) == 12.5\n\ndef test_rate_large():\n    assert rate(10) == 32.5\n",
    ];
    let cheats = [
        "def test_rate_small():\n    assert rate(2) > 0\n\ndef test_rate_large():\n    assert rate(10) > 0\n",
        "def test_rate_small():\n    assert rate(2) == 9.0\n",
    ];
    for h in honest {
        assert_eq!(verdict_for(h, 0, 0).0, Verdict::Supported, "{h}");
    }
    for c in cheats {
        assert_eq!(verdict_for(c, 0, 0).0, Verdict::Unsupported, "{c}");
    }
}
