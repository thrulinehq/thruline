//! BUILD-SPEC §6.1 and §6.3, with a control for every way of saying no.
//!
//! A verifier that answers `unsupported` to everything satisfies M1's
//! done-when and is worthless, so every refusal below is paired with the
//! honest case it must still support.

use thruline_verify::binding::{
    bind, Binding, CheckView, EditView, ParsedView, SessionView, SnapshotView,
};
use thruline_verify::tests_pass::{narrowing_flag, verify, Inputs};
use thruline_verify::verdict::{Finding, Severity, Verdict};

fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|s| (*s).to_string()).collect()
}

fn parsed(total: u64, passed: u64, failed: u64) -> Option<ParsedView> {
    Some(ParsedView {
        tests_total: total,
        passed,
        failed,
        skipped: 0,
        xfail: 0,
    })
}

fn check(seq: u64, tree: &str, args: &[&str], exit: i64) -> CheckView {
    CheckView {
        id: format!("chk_{seq}"),
        seq: Some(seq),
        category: "test".to_string(),
        runner: "pytest".to_string(),
        bypass: None,
        argv: Some(argv(args)),
        exit_code: exit,
        input_tree_hash: tree.to_string(),
        // Rule 1 covers these fixtures; the ones that exercise rule 2 set it.
        paths_changed_since_final: None,
        stdio: Some("pipe".into()),
        reexecution: None,
        parsed: parsed(4, 4, 0),
    }
}

/// A session where one full run passed on the tree the session ended on.
fn honest_session() -> SessionView {
    SessionView {
        session_id: "s".into(),
        snapshots: vec![
            SnapshotView {
                seq: 10,
                tree_hash: "aaa".into(),
            },
            SnapshotView {
                seq: 30,
                tree_hash: "bbb".into(),
            },
        ],
        edits: vec![EditView {
            seq: 12,
            path: "src/app.py".into(),
        }],
        checks: vec![check(20, "bbb", &["pytest", "-q"], 0)],
        head_commit: Some("c0ffee".into()),
        capture_degraded: false,
        degraded_reason: None,
    }
}

fn all_scope() -> Inputs {
    Inputs {
        scope_is_all: true,
        ..Inputs::default()
    }
}

// ---------------------------------------------------------------------------
// The control: an honest run is supported
// ---------------------------------------------------------------------------

#[test]
fn a_full_passing_run_on_the_final_tree_is_supported() {
    let v = verify("clm_1", &honest_session(), &all_scope());
    assert_eq!(v.verdict, Verdict::Supported, "{v:#?}");
    assert!(v.reasons.iter().any(|r| r.contains("final tree")));
    assert!(v.reasons.iter().any(|r| r.contains("4/4 passed")));
    assert_eq!(v.evidence, vec!["chk_20".to_string()]);
}

// ---------------------------------------------------------------------------
// §6.1 binding
// ---------------------------------------------------------------------------

#[test]
fn a_check_that_ran_before_a_source_edit_is_not_evidence_for_the_final_tree() {
    // §6.3's "lucky-pass ordering": the last passing run precedes the last
    // source edit, so it is evidence about a tree that no longer exists.
    let mut s = honest_session();
    s.checks[0].input_tree_hash = "aaa".into();
    s.edits.push(EditView {
        seq: 25,
        path: "src/app.py".into(),
    });
    let v = verify("clm_1", &s, &all_scope());
    assert_eq!(v.verdict, Verdict::Unsupported, "{v:#?}");
    assert!(v
        .findings
        .iter()
        .any(|f| f.detector == "tests_ran_before_final_edit"));
}

#[test]
fn a_test_only_edit_after_the_check_does_not_break_binding_but_is_counted() {
    // §6.1 says NON-test files. A test edit is §6.7's business, and the
    // relaxation must be visible rather than invisible.
    let mut s = honest_session();
    s.checks[0].input_tree_hash = "aaa".into();
    s.edits.push(EditView {
        seq: 25,
        path: "tests/test_app.py".into(),
    });
    // The TREE half of rule 2. Both halves must agree: the edit record alone
    // is what `sed -i` walks straight through.
    s.checks[0].paths_changed_since_final = Some(vec!["tests/test_app.py".into()]);
    let binding = bind(&s, &s.checks[0]);
    assert!(
        matches!(
            binding,
            Binding::NoRelevantChangeSince {
                edits_ignored: 1,
                ..
            }
        ),
        "{binding:?}"
    );
    assert!(binding.describe().contains("confirmed against both"));
}

#[test]
fn a_session_with_no_snapshot_is_unverifiable_not_unsupported() {
    // The distinction §6 forbids collapsing. Nothing was observed; that is not
    // the same as evidence against.
    let mut s = honest_session();
    s.snapshots.clear();
    let v = verify("clm_1", &s, &all_scope());
    assert_eq!(v.verdict, Verdict::Unverifiable, "{v:#?}");
}

#[test]
fn a_check_with_no_input_tree_can_never_be_mistaken_for_a_bound_one() {
    let mut s = honest_session();
    s.checks[0].input_tree_hash = String::new();
    let binding = bind(&s, &s.checks[0]);
    assert!(
        matches!(binding, Binding::Unobservable { .. }),
        "{binding:?}"
    );
    assert!(!binding.counts());
}

// ---------------------------------------------------------------------------
// §6.3 detectors
// ---------------------------------------------------------------------------

#[test]
fn a_narrowed_run_cannot_support_a_claim_about_the_whole_suite() {
    let mut s = honest_session();
    s.checks[0].argv = Some(argv(&["pytest", "-q", "-k", "test_small"]));
    let v = verify("clm_1", &s, &all_scope());
    assert_eq!(v.verdict, Verdict::Unsupported, "{v:#?}");
    let f = v
        .findings
        .iter()
        .find(|f| f.detector == "narrowed_run")
        .expect("finding");
    assert_eq!(f.severity, Severity::Fail);
}

#[test]
fn the_same_narrowed_run_supports_a_claim_that_was_only_about_those_tests() {
    // §6.3: "Narrowed runs support only scope: named/changed, never all."
    let mut s = honest_session();
    s.checks[0].argv = Some(argv(&["pytest", "-q", "-k", "test_small"]));
    let v = verify(
        "clm_1",
        &s,
        &Inputs {
            scope_is_all: false,
            ..Inputs::default()
        },
    );
    assert_eq!(v.verdict, Verdict::Supported, "{v:#?}");
    assert!(v
        .findings
        .iter()
        .any(|f| f.detector == "narrowed_run" && f.severity == Severity::Warn));
}

#[test]
fn narrowing_is_recognised_from_argv_including_a_bare_named_file() {
    for args in [
        vec!["pytest", "-k", "x"],
        vec!["pytest", "-m", "slow"],
        vec!["pytest", "--lf"],
        vec!["pytest", "--deselect", "a::b"],
        vec!["jest", "--testNamePattern=foo"],
        vec!["pytest", "tests/test_one.py"],
    ] {
        assert!(narrowing_flag(&argv(&args)).is_some(), "{args:?}");
    }
    // And a full run is not narrowed.
    for args in [
        vec!["pytest", "-q"],
        vec!["cargo", "test"],
        vec!["pytest", "--color=yes"],
    ] {
        assert!(narrowing_flag(&argv(&args)).is_none(), "{args:?}");
    }
}

#[test]
fn a_failing_run_outranks_a_passing_one() {
    // A green run must never cover a red one. This is the product's central
    // failure mode; here it is the product's own code being held to it.
    let mut s = honest_session();
    s.checks.push(CheckView {
        id: "chk_21".into(),
        seq: Some(21),
        category: "test".into(),
        runner: "pytest".into(),
        bypass: None,
        argv: Some(argv(&["pytest", "-q", "tests/other"])),
        exit_code: 1,
        input_tree_hash: "bbb".into(),
        paths_changed_since_final: None,
        stdio: Some("pipe".into()),
        reexecution: None,
        parsed: parsed(2, 1, 1),
    });
    let v = verify("clm_1", &s, &all_scope());
    assert_eq!(v.verdict, Verdict::Unsupported, "{v:#?}");
    assert!(v.findings.iter().any(|f| f.detector == "test_run_failed"));
}

#[test]
fn a_flaky_pair_on_one_tree_is_not_evidence() {
    let mut s = honest_session();
    s.checks.push(CheckView {
        id: "chk_21".into(),
        seq: Some(21),
        category: "test".into(),
        runner: "pytest".into(),
        bypass: None,
        argv: Some(argv(&["pytest", "-q"])),
        exit_code: 0,
        input_tree_hash: "bbb".into(),
        paths_changed_since_final: None,
        stdio: Some("pipe".into()),
        reexecution: None,
        parsed: parsed(4, 3, 1),
    });
    let v = verify("clm_1", &s, &all_scope());
    assert_eq!(v.verdict, Verdict::Unsupported, "{v:#?}");
    assert!(v
        .findings
        .iter()
        .any(|f| f.detector == "nondeterministic" || f.detector == "test_run_failed"));
}

#[test]
fn skips_are_a_warning_and_are_listed_rather_than_hidden() {
    let mut s = honest_session();
    s.checks[0].parsed = Some(ParsedView {
        tests_total: 4,
        passed: 3,
        failed: 0,
        skipped: 1,
        xfail: 0,
    });
    let v = verify("clm_1", &s, &all_scope());
    assert_eq!(v.verdict, Verdict::Supported);
    assert!(v.warnings.iter().any(|w| w.contains("skipped")), "{v:#?}");
}

#[test]
fn a_run_that_could_not_be_parsed_is_unverifiable_not_clean() {
    let mut s = honest_session();
    s.checks[0].parsed = None;
    let v = verify("clm_1", &s, &all_scope());
    assert_eq!(v.verdict, Verdict::Unverifiable, "{v:#?}");
    assert!(v
        .reasons
        .iter()
        .any(|r| r.contains("not the same as passing")));
}

#[test]
fn a_run_reporting_zero_tests_proves_nothing() {
    let mut s = honest_session();
    s.checks[0].parsed = parsed(0, 0, 0);
    let v = verify("clm_1", &s, &all_scope());
    assert_eq!(v.verdict, Verdict::Unverifiable, "{v:#?}");
}

#[test]
fn no_test_check_at_all_is_unverifiable() {
    let mut s = honest_session();
    s.checks.clear();
    let v = verify("clm_1", &s, &all_scope());
    assert_eq!(v.verdict, Verdict::Unverifiable, "{v:#?}");
    assert!(v
        .reasons
        .iter()
        .any(|r| r.contains("no evidence either way")));
}

#[test]
fn degraded_capture_can_never_reach_supported() {
    // §13, and the cost ruling E11 records: for a path under a clean filter
    // ThruLine does not know what ran, and `unverifiable` is the honest answer.
    let mut s = honest_session();
    s.capture_degraded = true;
    s.degraded_reason = Some("a .gitattributes clean filter is in scope".into());
    let v = verify("clm_1", &s, &all_scope());
    assert_eq!(v.verdict, Verdict::Unverifiable, "{v:#?}");
    assert!(v.reasons[0].contains("clean filter"));
}

#[test]
fn a_tampering_finding_folds_in_and_moves_the_verdict_the_safe_way() {
    let s = honest_session();
    let v = verify(
        "clm_1",
        &s,
        &Inputs {
            scope_is_all: true,
            tamper_findings: vec![Finding::new(Severity::Fail, "assertion_removed", "x")],
            ..Inputs::default()
        },
    );
    assert_eq!(v.verdict, Verdict::Unsupported, "{v:#?}");
    // A warn-severity finding is recorded and does NOT refute.
    let v = verify(
        "clm_1",
        &s,
        &Inputs {
            scope_is_all: true,
            tamper_findings: vec![Finding::new(Severity::Warn, "assertion_weakened", "y")],
            ..Inputs::default()
        },
    );
    assert_eq!(v.verdict, Verdict::Supported);
    assert!(v
        .findings
        .iter()
        .any(|f| f.detector == "assertion_weakened"));
}

#[test]
fn a_verdict_never_moves_toward_supported() {
    assert_eq!(
        Verdict::Supported.worst(Verdict::Unsupported),
        Verdict::Unsupported
    );
    assert_eq!(
        Verdict::Unverifiable.worst(Verdict::Supported),
        Verdict::Unverifiable
    );
    assert_eq!(
        Verdict::Unsupported.worst(Verdict::Unverifiable),
        Verdict::Unsupported
    );
}

#[test]
fn test_files_changed_with_no_source_change_is_a_warning() {
    let s = honest_session();
    let v = verify(
        "clm_1",
        &s,
        &Inputs {
            scope_is_all: true,
            test_files_changed: vec!["tests/test_app.py".into()],
            source_changed: false,
            ..Inputs::default()
        },
    );
    assert!(v
        .findings
        .iter()
        .any(|f| f.detector == "test_changed_without_source_change"));
    // And with a source change to necessitate it, no finding.
    let v = verify(
        "clm_1",
        &s,
        &Inputs {
            scope_is_all: true,
            test_files_changed: vec!["tests/test_app.py".into()],
            source_changed: true,
            ..Inputs::default()
        },
    );
    assert!(!v
        .findings
        .iter()
        .any(|f| f.detector == "test_changed_without_source_change"));
}

// ---------------------------------------------------------------------------
// IR-6 (Track D) — `python3 -m pytest` is not a narrowed run
// ---------------------------------------------------------------------------

/// `-m` is pytest's marker filter and the interpreter's module flag, and the
/// scan could not tell them apart. Track D measured it against the shipped
/// binary while walking its own quickstart: the commonest way to invoke pytest
/// in a virtualenv reported `narrowed_run`, so an honest full suite could not
/// support a claim about the whole suite — §6.3 refusing honest evidence, which
/// is the direction that must never happen.
#[test]
fn ir6_the_interpreters_module_flag_is_not_pytests_marker_filter() {
    assert_eq!(
        narrowing_flag(&argv(&["python3", "-m", "pytest"])),
        None,
        "an honest full run through the interpreter was called narrowed"
    );
    assert_eq!(
        narrowing_flag(&argv(&["python3", "-m", "pytest", "-q"])),
        None,
        "the interpreter form plus a quiet flag was called narrowed"
    );
    // Every spelling of the interpreter a real project uses.
    for py in ["python", "python3", "python3.12", "/usr/bin/python3", "py"] {
        assert_eq!(
            narrowing_flag(&argv(&[py, "-m", "pytest"])),
            None,
            "{py} -m pytest was called narrowed"
        );
    }

    // THE CONTROL, and it is the whole point: a real marker filter still
    // narrows, because the scan begins at the runner and finds the second `-m`.
    assert_eq!(
        narrowing_flag(&argv(&["python3", "-m", "pytest", "-m", "slow"])),
        Some("-m".to_string()),
        "a genuine marker filter stopped being detected"
    );
    // And the direct form is untouched.
    assert_eq!(
        narrowing_flag(&argv(&["pytest", "-m", "slow"])),
        Some("-m".to_string())
    );
    // A named path after the module form still narrows — the pre-existing rule,
    // untouched. `pytest tests/` narrows too and always did; IR-6 is about the
    // FLAG, and conflating the two is how a fix gets credited with more than it
    // did.
    assert_eq!(
        narrowing_flag(&argv(&["python3", "-m", "pytest", "tests/test_one.py"])),
        Some("a named path (tests/test_one.py)".to_string())
    );
    // `-m` that is NOT the interpreter's — a runner that happens to be called
    // python is still scanned from index 1.
    assert_eq!(
        narrowing_flag(&argv(&["pytest", "-m"])),
        Some("-m".to_string())
    );
}
