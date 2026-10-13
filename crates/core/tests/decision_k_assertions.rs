//! C28 F1: independently prove execution and assertion-content verdicts.
//! The named run census comes from a real standalone rustc/libtest execution.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use thruline_core::decisions::checks::VerdictState;
use thruline_core::decisions::checks_k_assertions::{check_at, RustNamedTestParams};
use thruline_core::decisions::checks_tests::{
    check_test_present_and_runs, NamedTest, NamedTestState, RecordedTestRun, TestRunCensus,
};
use thruline_core::decisions::params::TestPresentAndRunsParams;
use thruline_core::test_tally::parse_result_line;

const NAME: &str = "approval_requires_pro";

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn commit_fixture(repo: &Path) -> (String, String) {
    git(repo, &["add", "-A"]);
    git(
        repo,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
    );
    (
        git(repo, &["rev-parse", "HEAD"]),
        git(repo, &["rev-parse", "HEAD^{tree}"]),
    )
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let destination = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &destination);
        } else {
            std::fs::copy(entry.path(), destination).unwrap();
        }
    }
}

fn fixture(state: &str) -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    let from = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/decisions/test_present_and_runs")
        .join(state);
    copy_tree(&from, repo.path());
    git(repo.path(), &["init", "-q"]);
    commit_fixture(repo.path());
    repo
}

fn source_repo(source: &str) -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    std::fs::create_dir(repo.path().join("tests")).unwrap();
    std::fs::write(repo.path().join("tests/gate.rs"), source).unwrap();
    git(repo.path(), &["init", "-q"]);
    commit_fixture(repo.path());
    repo
}

fn params() -> RustNamedTestParams {
    RustNamedTestParams {
        path: "tests/gate.rs".into(),
        test_name: NAME.into(),
    }
}

fn checked(repo: &Path) -> thruline_core::decisions::checks::CheckOutcome {
    check_at(
        repo,
        &git(repo, &["rev-parse", "HEAD"]),
        &git(repo, &["rev-parse", "HEAD^{tree}"]),
        &params(),
    )
}

fn run(repo: &Path, binary: &Path) -> Output {
    let compile = Command::new("rustc")
        .args(["--test", "tests/gate.rs", "-o"])
        .arg(binary)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "rustc: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    Command::new(binary)
        .args([NAME, "--exact", "--nocapture"])
        .current_dir(repo)
        .output()
        .unwrap()
}

fn executed_census(repo: &Path) -> (TestPresentAndRunsParams, TestRunCensus) {
    // The compiler inputs are precisely the committed fixture, not a dirty
    // worktree silently attributed to that commit.
    git(repo, &["diff", "--exit-code", "HEAD", "--"]);
    let compiled = tempfile::tempdir().unwrap();
    let binary = compiled.path().join("fixture-test");
    let output = run(repo, &binary);
    assert!(
        output.status.success(),
        "libtest: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(output.stderr.is_empty(), "unexpected libtest stderr");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let named = format!("test {NAME} ... ok");
    assert_eq!(
        stdout.lines().filter(|line| *line == named).count(),
        1,
        "{stdout}"
    );
    let tallies = stdout
        .lines()
        .filter_map(parse_result_line)
        .collect::<Vec<_>>();
    assert_eq!(tallies.len(), 1, "{stdout}");
    let tally = tallies[0];
    assert_eq!(
        (tally.passed, tally.failed, tally.ignored, tally.suites),
        (1, 0, 0, 1)
    );
    let commit = git(repo, &["rev-parse", "HEAD"]);
    let tree = git(repo, &["rev-parse", "HEAD^{tree}"]);
    let command = format!("{} {NAME} --exact --nocapture", binary.display());
    let params = TestPresentAndRunsParams {
        name_pattern: NAME.into(),
        command: command.clone(),
    };
    let census = TestRunCensus {
        version_id: commit.clone(),
        commit_oid: commit.clone(),
        tree_hash: tree.clone(),
        complete: true,
        runs: vec![RecordedTestRun {
            run_id: "actual-rustc-libtest-run".into(),
            seq: 1,
            version_id: commit,
            tree_hash: tree,
            command,
            exit_code: output.status.code().map(i64::from),
            tally: Some(tally),
            errors: Some(0),
            complete: true,
            named_results_complete: true,
            tests: vec![NamedTest {
                name: NAME.into(),
                state: NamedTestState::Passed,
            }],
        }],
        feature_scope: None,
    };
    (params, census)
}

#[test]
fn holds_fixture_executes_real_gate_behavior() {
    let repo = fixture("holds");
    let (b2, census) = executed_census(repo.path());
    assert_eq!(
        check_test_present_and_runs(repo.path(), &census.commit_oid, &b2, Some(&census)).state,
        VerdictState::Holds
    );
    assert_eq!(checked(repo.path()).state, VerdictState::Holds);
    // A predicate that incorrectly permits the free case must fail this test.
    std::fs::write(
        repo.path().join("src/feature.rs"),
        "pub fn gate_open(_has_pro: bool) -> bool { true }\n",
    )
    .unwrap();
    let compiled = tempfile::tempdir().unwrap();
    let output = run(repo.path(), &compiled.path().join("mutant-test"));
    assert!(
        !output.status.success(),
        "the real gate assertion did not detect a bad predicate"
    );
    let tally = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .find_map(parse_result_line)
        .unwrap();
    assert_eq!(tally.failed, 1);
}

#[test]
fn assertion_free_fixture_runs_b2_holds_while_k4_breaks() {
    let repo = fixture("assertion_free");
    let (b2, census) = executed_census(repo.path());
    assert_eq!(
        check_test_present_and_runs(repo.path(), &census.commit_oid, &b2, Some(&census)).state,
        VerdictState::Holds
    );
    let result = checked(repo.path());
    assert_eq!(result.state, VerdictState::Broken);
    assert_eq!(result.tree_hash.as_deref(), Some(census.tree_hash.as_str()));
    assert_eq!(result.findings.len(), 1);
    assert_eq!(result.findings[0].path, "tests/gate.rs");
    assert!(result.reason.unwrap().contains("no assertions"));
}

#[test]
fn tautology_is_a_separate_executed_broken_control() {
    let repo = source_repo("#[test]\nfn approval_requires_pro() {\n    ::std::assert!(true);\n}\n");
    let (b2, census) = executed_census(repo.path());
    assert_eq!(
        check_test_present_and_runs(repo.path(), &census.commit_oid, &b2, Some(&census)).state,
        VerdictState::Holds
    );
    let result = checked(repo.path());
    assert_eq!(result.state, VerdictState::Broken);
    assert_eq!(result.findings[0].line, 3);
    assert!(result.reason.unwrap().contains("tautological"));
}

#[test]
fn helper_closure_and_conditional_assertions_abstain() {
    for source in [
        "fn helper() { ::std::assert!(true); }\n#[test]\nfn approval_requires_pro() { helper(); }",
        "#[test]\nfn approval_requires_pro() { let _check = || ::std::assert!(true); }",
        "#[test]\nfn approval_requires_pro() { if false { ::std::assert!(true); } }",
        "#[test]\nfn approval_requires_pro() { return; ::std::assert!(true); }",
    ] {
        let repo = source_repo(source);
        assert_eq!(
            checked(repo.path()).state,
            VerdictState::Unverifiable,
            "{source}"
        );
    }
}

#[test]
fn malformed_duplicate_or_extra_attribute_tests_abstain() {
    for source in [
        "#[test]\nfn approval_requires_pro() { ::std::assert!(; }",
        "#[test]\nfn approval_requires_pro() {}\nmod nested { #[test] fn approval_requires_pro() {} }",
        "#[test]\n#[ignore]\nfn approval_requires_pro() {}",
        "#[test]\n#[should_panic]\nfn approval_requires_pro() {}",
        "#[test]\nfn approval_requires_pro<T>() {}",
    ] {
        let repo = source_repo(source);
        assert_eq!(checked(repo.path()).state, VerdictState::Unverifiable, "{source}");
    }
}

#[test]
fn comments_do_not_supply_assertions_and_macro_shadowing_abstains() {
    let repo = source_repo("#[test]\nfn approval_requires_pro() { /* ::std::assert!(true); */ }");
    assert_eq!(checked(repo.path()).state, VerdictState::Broken);
    for source in [
        "macro_rules! assert { ($x:expr) => {} }\n#[test]\nfn approval_requires_pro() { assert!(false); }",
        "#[test]\nfn approval_requires_pro() { assert!(some_predicate()); }",
        "#[test]\nfn approval_requires_pro() { let _ = \"assert!(true)\"; }",
        "mod std {}\n#[test]\nfn approval_requires_pro() { std::assert!(true); }",
    ] {
        let repo = source_repo(source);
        assert_eq!(checked(repo.path()).state, VerdictState::Unverifiable, "{source}");
    }
}

#[test]
fn literal_true_comparisons_break_without_using_advisory_signals() {
    for assertion in [
        "::std::assert!((true));",
        "::std::assert!(1 == 1);",
        "::std::assert_eq!(1, 1);",
        "::std::assert_ne!(1, 2);",
    ] {
        let repo = source_repo(&format!("#[test]\nfn {NAME}() {{ {assertion} }}"));
        assert_eq!(
            checked(repo.path()).state,
            VerdictState::Broken,
            "{assertion}"
        );
    }
}

#[test]
fn direct_call_assertions_hold_only_for_supported_structural_scope() {
    for assertion in [
        "::std::assert!(predicate());",
        "::std::assert!(!predicate());",
        "::std::assert_eq!(predicate(), false);",
        "::std::assert_ne!(predicate(), true);",
    ] {
        let repo = source_repo(&format!(
            "fn predicate() -> bool {{ false }}\n#[test]\nfn {NAME}() {{ {assertion} }}"
        ));
        assert_eq!(
            checked(repo.path()).state,
            VerdictState::Holds,
            "{assertion}"
        );
    }
}

#[test]
fn immutable_tree_and_claimed_tree_are_checked() {
    let repo = fixture("holds");
    let commit = git(repo.path(), &["rev-parse", "HEAD"]);
    let tree = git(repo.path(), &["rev-parse", "HEAD^{tree}"]);
    let before = check_at(repo.path(), &commit, &tree, &params());
    assert_eq!(before.state, VerdictState::Holds);
    std::fs::write(
        repo.path().join("tests/gate.rs"),
        "#[test]\nfn approval_requires_pro() {}\n",
    )
    .unwrap();
    assert_eq!(check_at(repo.path(), &commit, &tree, &params()), before);
    assert_eq!(
        check_at(repo.path(), &commit, &"0".repeat(40), &params()).state,
        VerdictState::Unverifiable
    );
    assert_eq!(
        check_at(repo.path(), "HEAD", &tree, &params()).state,
        VerdictState::Unverifiable
    );
}

#[test]
fn invalid_missing_or_symlink_scope_abstains() {
    let repo = source_repo("#[test]\nfn approval_requires_pro() {}");
    let commit = git(repo.path(), &["rev-parse", "HEAD"]);
    let tree = git(repo.path(), &["rev-parse", "HEAD^{tree}"]);
    for path in [
        "../tests/gate.rs",
        "/tests/gate.rs",
        "tests/gate.py",
        "tests/missing.rs",
        "tests/./gate.rs",
    ] {
        let p = RustNamedTestParams {
            path: path.into(),
            test_name: NAME.into(),
        };
        assert_eq!(
            check_at(repo.path(), &commit, &tree, &p).state,
            VerdictState::Unverifiable,
            "{path}"
        );
    }
    let missing = RustNamedTestParams {
        path: "tests/gate.rs".into(),
        test_name: "absent_test".into(),
    };
    assert_eq!(
        check_at(repo.path(), &commit, &tree, &missing).state,
        VerdictState::Unverifiable
    );
    #[cfg(unix)]
    {
        std::fs::remove_file(repo.path().join("tests/gate.rs")).unwrap();
        std::os::unix::fs::symlink("missing", repo.path().join("tests/gate.rs")).unwrap();
        let (commit, tree) = commit_fixture(repo.path());
        assert_eq!(
            check_at(repo.path(), &commit, &tree, &params()).state,
            VerdictState::Unverifiable
        );
    }
}

#[test]
fn source_and_ast_budgets_abstain() {
    let huge = format!("//{}\n#[test]\nfn {NAME}() {{}}", "x".repeat(256 * 1024));
    let repo = source_repo(&huge);
    assert_eq!(checked(repo.path()).state, VerdictState::Unverifiable);
    let mut many_nodes = format!("#[test]\nfn {NAME}() {{}}\n");
    for index in 0..8000 {
        many_nodes.push_str(&format!("fn helper_{index}() {{}}\n"));
    }
    let repo = source_repo(&many_nodes);
    assert_eq!(checked(repo.path()).state, VerdictState::Unverifiable);
}
