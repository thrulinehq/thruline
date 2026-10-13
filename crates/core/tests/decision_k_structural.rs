//! Structural K candidates are advisory observations, never accepted verdicts.

use thruline_core::decisions::checks_k_candidates::KCandidateState;
use thruline_core::decisions::checks_preservation::{AssertionCensus, AssertionFile, CiStepCensus};

use thruline_core::decisions::checks_k_structural::{
    scan_k_structural, scan_k_structural_candidates, AssertionDetectorFinding, AssertionEvidence,
    CiEvidence, KStructuralInput, SourceCensus, SourceDelta, TestFileCensus, TestSourceLink,
};

fn source(path: &str, before: &str, after: &str) -> KStructuralInput {
    KStructuralInput {
        source: Some(SourceCensus {
            complete: true,
            changes: vec![SourceDelta {
                path: path.into(),
                baseline: Some(before.into()),
                target: Some(after.into()),
            }],
        }),
        ..KStructuralInput::default()
    }
}

fn files(
    baseline_tests: &[&str],
    target_tests: &[&str],
    target_source: &[&str],
    links: Vec<TestSourceLink>,
) -> KStructuralInput {
    KStructuralInput {
        files: Some(TestFileCensus {
            complete: true,
            baseline_test_paths: baseline_tests.iter().map(|s| (*s).into()).collect(),
            target_test_paths: target_tests.iter().map(|s| (*s).into()).collect(),
            target_source_paths: target_source.iter().map(|s| (*s).into()).collect(),
            links,
        }),
        ..KStructuralInput::default()
    }
}

fn link(test_path: &str, source_path: &str, moved_to: Option<&str>) -> TestSourceLink {
    TestSourceLink {
        test_path: test_path.into(),
        source_paths: vec![source_path.into()],
        moved_to: moved_to.map(str::to_owned),
    }
}

fn assertion_file(base: u64, target: u64, moved_to: Option<&str>) -> AssertionFile {
    AssertionFile {
        baseline_path: Some("tests/old.rs".into()),
        target_path: Some(moved_to.unwrap_or("tests/old.rs").into()),
        baseline_blob: Some("a".repeat(40)),
        target_blob: Some("b".repeat(40)),
        baseline_count: base,
        target_count: target,
        moved_to: moved_to.map(str::to_owned),
    }
}

fn assertions(
    base: u64,
    target: u64,
    moved_to: Option<&str>,
    findings: Vec<AssertionDetectorFinding>,
) -> KStructuralInput {
    KStructuralInput {
        assertions: Some(AssertionEvidence {
            census: AssertionCensus {
                baseline_version: "a".repeat(40),
                baseline_tree_hash: "b".repeat(40),
                target_version: "c".repeat(40),
                target_tree_hash: "d".repeat(40),
                complete: true,
                files: vec![assertion_file(base, target, moved_to)],
            },
            detector_complete: true,
            detector_findings: findings,
        }),
        ..KStructuralInput::default()
    }
}

fn ci_step() -> CiStepCensus {
    CiStepCensus {
        baseline_version: "a".repeat(40),
        baseline_tree_hash: "b".repeat(40),
        target_version: "c".repeat(40),
        target_tree_hash: "d".repeat(40),
        workflow: ".github/workflows/ci.yml".into(),
        step: "test".into(),
        baseline_blob: "e".repeat(40),
        target_blob: "f".repeat(40),
        complete: true,
        baseline_step_present: true,
        target_step_present: true,
        baseline_threshold_pct: Some(80),
        target_threshold_pct: Some(80),
        baseline_bypasses: Vec::new(),
        target_bypasses: Vec::new(),
    }
}

fn ci(steps: Vec<CiStepCensus>) -> KStructuralInput {
    KStructuralInput {
        ci: Some(CiEvidence {
            complete: true,
            steps,
        }),
        ..KStructuralInput::default()
    }
}

#[test]
fn k1_new_skip_or_focus_markers_are_candidates_across_thirteen_languages() {
    for (path, marker) in [
        ("tests/app.test.js", "test.only('works', () => {});"),
        ("tests/app.test.ts", "test.skip('works', () => {});"),
        ("tests/test_app.py", "@pytest.mark.skip"),
        ("tests/app_test.go", "t.Skip(\"later\")"),
        ("src/lib.rs", "#[ignore]"),
        ("src/AppTest.java", "@Disabled"),
        ("src/AppTest.kt", "@Ignore"),
        ("tests/AppTests.cs", "[Fact(Skip = \"later\")]"),
        ("spec/app_spec.rb", "skip 'later'"),
        ("tests/AppTest.php", "$this->markTestSkipped('later');"),
        ("tests/AppTests.swift", "throw XCTSkip(\"later\")"),
        ("test/app_test.dart", "test('works', () {}, skip: true);"),
        ("test/app_test.exs", "@tag :skip"),
    ] {
        let input = source(path, "test body\n", &format!("test body\n{marker}\n"));
        let outcome = scan_k_structural("K1", &input);
        assert_eq!(
            outcome.state,
            KCandidateState::Signal,
            "{path}: {outcome:?}"
        );
        assert_eq!(outcome.observations[0].line, 2);
    }
}

#[test]
fn k1_comments_quoted_text_platform_skip_and_moved_marker_are_controls() {
    for (path, before, after) in [
        ("tests/app.test.ts", "test('works', () => {});\n", "// test.skip('works', () => {});\n"),
        ("tests/app.test.ts", "test('works', () => {});\n", "const note = \"test.only('works')\";\n"),
        ("tests/test_app.py", "def test_app(): pass\n", "@pytest.mark.skipif(sys.platform == 'win32', reason='platform')\ndef test_app(): pass\n"),
        ("tests/app.test.ts", "test('works', () => {});\n", "if (process.platform === 'win32') {\n  test.skip('platform', () => {});\n}\n"),
        ("src/lib.rs", "#[test]\n", "#[cfg_attr(target_os = \"windows\", ignore)]\n#[test]\n"),
        ("tests/app_test.go", "func TestApp(t *testing.T) {}\n", "if runtime.GOOS == \"windows\" { t.Skip(\"platform\") }\n"),
    ] {
        let outcome = scan_k_structural("K1", &source(path, before, after));
        assert_eq!(outcome.state, KCandidateState::NoSignal, "{path}: {outcome:?}");
    }
    let moved = KStructuralInput {
        source: Some(SourceCensus {
            complete: true,
            changes: vec![
                SourceDelta {
                    path: "tests/old.test.ts".into(),
                    baseline: Some("test.skip('same', () => {});\n".into()),
                    target: None,
                },
                SourceDelta {
                    path: "tests/new.test.ts".into(),
                    baseline: None,
                    target: Some("test.skip('same', () => {});\n".into()),
                },
            ],
        }),
        ..KStructuralInput::default()
    };
    assert_eq!(
        scan_k_structural("K1", &moved).state,
        KCandidateState::NoSignal
    );
}

#[test]
fn k1_missing_or_partial_source_census_is_unverifiable() {
    let absent = scan_k_structural("K1", &KStructuralInput::default());
    assert_eq!(absent.state, KCandidateState::Unverifiable);
    assert!(absent.reason.is_some());
    let mut partial = source("tests/app.test.ts", "", "test.only('works', () => {});");
    partial.source.as_mut().expect("source").complete = false;
    let outcome = scan_k_structural("K1", &partial);
    assert_eq!(outcome.state, KCandidateState::Unverifiable);
    assert!(outcome.reason.is_some());
}

#[test]
fn k2_deleted_test_with_linked_source_still_present_is_a_candidate() {
    let input = files(
        &["tests/auth_test.rs"],
        &[],
        &["src/auth.rs"],
        vec![link("tests/auth_test.rs", "src/auth.rs", None)],
    );
    let outcome = scan_k_structural("K2", &input);
    assert_eq!(outcome.state, KCandidateState::Signal);
    assert_eq!(outcome.observations[0].path, "tests/auth_test.rs");
}

#[test]
fn k2_move_and_whole_feature_removal_are_netted() {
    let moved = files(
        &["tests/old.rs"],
        &["tests/new.rs"],
        &["src/auth.rs"],
        vec![link("tests/old.rs", "src/auth.rs", Some("tests/new.rs"))],
    );
    assert_eq!(
        scan_k_structural("K2", &moved).state,
        KCandidateState::NoSignal
    );
    let removed = files(
        &["tests/old.rs"],
        &[],
        &[],
        vec![link("tests/old.rs", "src/auth.rs", None)],
    );
    assert_eq!(
        scan_k_structural("K2", &removed).state,
        KCandidateState::NoSignal
    );
}

#[test]
fn k2_missing_link_or_dangling_move_is_unverifiable() {
    let missing = files(&["tests/old.rs"], &[], &["src/auth.rs"], Vec::new());
    let outcome = scan_k_structural("K2", &missing);
    assert_eq!(outcome.state, KCandidateState::Unverifiable);
    assert!(outcome.reason.is_some());
    let dangling = files(
        &["tests/old.rs"],
        &[],
        &["src/auth.rs"],
        vec![link(
            "tests/old.rs",
            "src/auth.rs",
            Some("tests/missing.rs"),
        )],
    );
    assert_eq!(
        scan_k_structural("K2", &dangling).state,
        KCandidateState::Unverifiable
    );
}

#[test]
fn k3_removed_assertion_and_weakened_assertion_are_separate_signals() {
    let removed = scan_k_structural("K3", &assertions(2, 1, None, Vec::new()));
    assert_eq!(removed.state, KCandidateState::Signal);
    assert_eq!(removed.observations[0].marker, "assertion_count_decreased");
    let weakened = scan_k_structural(
        "K3",
        &assertions(
            1,
            1,
            None,
            vec![AssertionDetectorFinding {
                detector: "assertion_weakened".into(),
                path: "tests/old.rs".into(),
                line: 7,
            }],
        ),
    );
    assert_eq!(weakened.state, KCandidateState::Signal);
    assert_eq!(weakened.observations[0].line, 7);
}

#[test]
fn k3_move_nets_counts_and_missing_strength_census_abstains() {
    let moved = assertions(2, 2, Some("tests/new.rs"), Vec::new());
    assert_eq!(
        scan_k_structural("K3", &moved).state,
        KCandidateState::NoSignal
    );
    let mut missing = moved;
    missing
        .assertions
        .as_mut()
        .expect("assertions")
        .detector_complete = false;
    let outcome = scan_k_structural("K3", &missing);
    assert_eq!(outcome.state, KCandidateState::Unverifiable);
    assert!(outcome.reason.is_some());
}

#[test]
fn k8_removed_ci_step_or_new_bypass_is_a_candidate() {
    let mut removed = ci_step();
    removed.target_step_present = false;
    let outcome = scan_k_structural("K8", &ci(vec![removed]));
    assert_eq!(outcome.state, KCandidateState::Signal);
    assert_eq!(outcome.observations[0].marker, "ci_step_removed");
    let mut bypass = ci_step();
    bypass.target_bypasses = vec!["continue-on-error".into()];
    let outcome = scan_k_structural("K8", &ci(vec![bypass]));
    assert_eq!(outcome.state, KCandidateState::Signal);
    assert_eq!(outcome.observations[0].marker, "ci_bypass_added");
    assert_eq!(
        scan_k_structural("K8", &ci(vec![ci_step()])).state,
        KCandidateState::NoSignal
    );
}

#[test]
fn k9_lowered_or_removed_threshold_is_a_candidate() {
    let mut lowered = ci_step();
    lowered.target_threshold_pct = Some(70);
    let outcome = scan_k_structural("K9", &ci(vec![lowered]));
    assert_eq!(outcome.state, KCandidateState::Signal);
    assert_eq!(outcome.observations[0].marker, "coverage_threshold_lowered");
    let mut removed = ci_step();
    removed.target_threshold_pct = None;
    assert_eq!(
        scan_k_structural("K9", &ci(vec![removed])).state,
        KCandidateState::Signal
    );
    assert_eq!(
        scan_k_structural("K9", &ci(vec![ci_step()])).state,
        KCandidateState::NoSignal
    );
}

#[test]
fn k8_k9_incomplete_or_mixed_version_ci_census_is_unverifiable() {
    let mut partial = ci(vec![ci_step()]);
    partial.ci.as_mut().expect("ci").complete = false;
    for code in ["K8", "K9"] {
        let outcome = scan_k_structural(code, &partial);
        assert_eq!(outcome.state, KCandidateState::Unverifiable);
        assert!(outcome.reason.is_some());
    }
    let mut other = ci_step();
    other.target_version = "f".repeat(40);
    let mixed = ci(vec![ci_step(), other]);
    assert_eq!(
        scan_k_structural("K8", &mixed).state,
        KCandidateState::Unverifiable
    );
    assert_eq!(
        scan_k_structural("K9", &mixed).state,
        KCandidateState::Unverifiable
    );
}

#[test]
fn batch_scanner_has_five_stable_k_codes_and_never_a_verdict() {
    let outcomes = scan_k_structural_candidates(&KStructuralInput::default());
    assert_eq!(
        outcomes
            .iter()
            .map(|row| row.code.as_str())
            .collect::<Vec<_>>(),
        ["K1", "K2", "K3", "K8", "K9"]
    );
    assert!(outcomes
        .iter()
        .all(|row| row.state == KCandidateState::Unverifiable && row.reason.is_some()));
}
