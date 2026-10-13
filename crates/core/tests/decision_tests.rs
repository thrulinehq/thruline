use std::path::{Path, PathBuf};
use std::process::Command;

use thruline_core::decisions::checks::VerdictState;
use thruline_core::decisions::checks_tests::{
    check_test_present_and_runs, FeatureScope, NamedTest, NamedTestState, RecordedTestRun,
    TestRunCensus,
};
use thruline_core::decisions::params::TestPresentAndRunsParams;
use thruline_core::test_tally::TestTally;

fn fixtures(state: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/decisions/test_present_and_runs")
        .join(state)
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn commit(repo: &Path) {
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
}

fn repo_for(state: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    copy_tree(&fixtures(state), dir.path());
    git(dir.path(), &["init", "-q"]);
    commit(dir.path());
    dir
}

fn params(name_pattern: &str) -> TestPresentAndRunsParams {
    TestPresentAndRunsParams {
        name_pattern: name_pattern.into(),
        command: "cargo test approval_requires_pro".into(),
    }
}

fn census(repo: &Path, command: &str, name: &str, state: NamedTestState) -> TestRunCensus {
    let commit_oid = git(repo, &["rev-parse", "HEAD"]);
    let version_id = "hq-version-at-this-commit".to_owned();
    let tree_hash = git(repo, &["rev-parse", "HEAD^{tree}"]);
    TestRunCensus {
        version_id: version_id.clone(),
        commit_oid,
        tree_hash: tree_hash.clone(),
        complete: true,
        runs: vec![RecordedTestRun {
            run_id: "run-1".into(),
            seq: 1,
            version_id,
            tree_hash,
            command: command.into(),
            exit_code: Some(0),
            tally: Some(TestTally {
                passed: 1,
                failed: 0,
                ignored: 0,
                suites: 1,
            }),
            errors: Some(0),
            complete: true,
            named_results_complete: true,
            tests: vec![NamedTest {
                name: name.into(),
                state,
            }],
        }],
        feature_scope: None,
    }
}

#[test]
fn named_test_in_same_version_tally_holds_and_ignores_worktree_edits() {
    let repo = repo_for("holds");
    let params = params("approval_requires_pro");
    let runs = census(
        repo.path(),
        &params.command,
        "approval_requires_pro",
        NamedTestState::Passed,
    );
    let held = check_test_present_and_runs(repo.path(), "HEAD", &params, Some(&runs));
    assert_eq!(held.state, VerdictState::Holds);
    assert_eq!(held.tree_hash.as_deref(), Some(runs.tree_hash.as_str()));
    std::fs::write(
        repo.path().join("tests/gate.rs"),
        "#[test]\n#[ignore]\nfn approval_requires_pro() {}\n",
    )
    .unwrap();
    assert_eq!(
        check_test_present_and_runs(repo.path(), "HEAD", &params, Some(&runs)),
        held
    );
}

#[test]
fn test_deleted_or_unconditionally_skipped_is_broken() {
    for state in [
        "deleted",
        "ignored",
        "js_skip",
        "js_focus",
        "python_skip",
        "go_skip",
    ] {
        let repo = repo_for(state);
        let params = params("approval_requires_pro");
        let runs = census(
            repo.path(),
            &params.command,
            "approval_requires_pro",
            NamedTestState::Passed,
        );
        let result = check_test_present_and_runs(repo.path(), "HEAD", &params, Some(&runs));
        assert_eq!(
            result.state,
            VerdictState::Broken,
            "{state}: {:?}",
            result.reason
        );
        assert!(!result.findings.is_empty(), "{state}");
    }
}

#[test]
fn platform_conditional_skip_is_allowed_when_named_in_same_version_run() {
    let repo = repo_for("conditional");
    let params = params("approval_requires_pro");
    let mut runs = census(
        repo.path(),
        &params.command,
        "approval_requires_pro",
        NamedTestState::Skipped,
    );
    runs.runs[0].tally = Some(TestTally {
        passed: 0,
        failed: 0,
        ignored: 1,
        suites: 1,
    });
    assert_eq!(
        check_test_present_and_runs(repo.path(), "HEAD", &params, Some(&runs)).state,
        VerdictState::Holds
    );
}

#[test]
fn absent_or_stale_named_run_never_establishes_a_hold() {
    let repo = repo_for("holds");
    let params = params("approval_requires_pro");
    assert_eq!(
        check_test_present_and_runs(repo.path(), "HEAD", &params, None).state,
        VerdictState::Unverifiable
    );
    let mut runs = census(
        repo.path(),
        &params.command,
        "approval_requires_pro",
        NamedTestState::Passed,
    );
    runs.runs[0].version_id = "older-version".into();
    assert_eq!(
        check_test_present_and_runs(repo.path(), "HEAD", &params, Some(&runs)).state,
        VerdictState::Unverifiable
    );
    runs.runs[0].version_id = runs.version_id.clone();
    runs.runs[0].tally = None;
    assert_eq!(
        check_test_present_and_runs(repo.path(), "HEAD", &params, Some(&runs)).state,
        VerdictState::Unverifiable
    );
    runs.runs[0].tally = Some(TestTally {
        passed: 1,
        failed: 0,
        ignored: 0,
        suites: 1,
    });
    runs.runs[0].named_results_complete = false;
    assert_eq!(
        check_test_present_and_runs(repo.path(), "HEAD", &params, Some(&runs)).state,
        VerdictState::Unverifiable
    );
}

#[test]
fn a_moved_test_still_holds() {
    let repo = repo_for("moved");
    let params = params("approval_requires_pro");
    let runs = census(
        repo.path(),
        &params.command,
        "approval_requires_pro",
        NamedTestState::Passed,
    );
    assert_eq!(
        check_test_present_and_runs(repo.path(), "HEAD", &params, Some(&runs)).state,
        VerdictState::Holds
    );
}

#[test]
fn an_inline_rust_unit_test_is_found_outside_a_test_path() {
    let repo = repo_for("inline");
    let params = params("approval_requires_pro");
    let runs = census(
        repo.path(),
        &params.command,
        "crate::tests::approval_requires_pro",
        NamedTestState::Passed,
    );
    assert_eq!(
        check_test_present_and_runs(repo.path(), "HEAD", &params, Some(&runs)).state,
        VerdictState::Holds
    );
}

#[test]
fn removed_feature_with_its_tests_is_a_documented_scope_exception() {
    let repo = repo_for("holds");
    let baseline = git(repo.path(), &["rev-parse", "HEAD"]);
    std::fs::remove_file(repo.path().join("src/feature.rs")).unwrap();
    std::fs::remove_file(repo.path().join("tests/gate.rs")).unwrap();
    commit(repo.path());
    let params = params("approval_requires_pro");
    let mut runs = census(
        repo.path(),
        &params.command,
        "other_test",
        NamedTestState::Passed,
    );
    runs.feature_scope = Some(FeatureScope {
        accepted_baseline_version: baseline,
        feature_paths: vec!["src/feature.rs".into()],
        test_paths: vec!["tests/gate.rs".into()],
    });
    assert_eq!(
        check_test_present_and_runs(repo.path(), "HEAD", &params, Some(&runs)).state,
        VerdictState::Holds
    );
}

#[test]
fn removed_feature_without_its_tests_does_not_get_the_scope_exception() {
    let repo = repo_for("holds");
    let baseline = git(repo.path(), &["rev-parse", "HEAD"]);
    std::fs::remove_file(repo.path().join("src/feature.rs")).unwrap();
    commit(repo.path());
    let params = params("approval_requires_pro");
    let mut runs = census(
        repo.path(),
        &params.command,
        "other_test",
        NamedTestState::Passed,
    );
    runs.feature_scope = Some(FeatureScope {
        accepted_baseline_version: baseline,
        feature_paths: vec!["src/feature.rs".into()],
        test_paths: vec!["tests/gate.rs".into()],
    });
    assert_eq!(
        check_test_present_and_runs(repo.path(), "HEAD", &params, Some(&runs)).state,
        VerdictState::Broken
    );
}

#[test]
fn a_named_test_that_failed_is_broken() {
    let repo = repo_for("holds");
    let params = params("approval_requires_pro");
    let mut runs = census(
        repo.path(),
        &params.command,
        "approval_requires_pro",
        NamedTestState::Failed,
    );
    runs.runs[0].exit_code = Some(1);
    runs.runs[0].tally = Some(TestTally {
        passed: 0,
        failed: 1,
        ignored: 0,
        suites: 1,
    });
    assert_eq!(
        check_test_present_and_runs(repo.path(), "HEAD", &params, Some(&runs)).state,
        VerdictState::Broken
    );
}

#[test]
fn named_output_without_a_test_definition_does_not_create_one() {
    let repo = repo_for("deleted");
    let params = params("approval_requires_pro");
    let runs = census(
        repo.path(),
        &params.command,
        "approval_requires_pro",
        NamedTestState::Passed,
    );
    assert_eq!(
        check_test_present_and_runs(repo.path(), "HEAD", &params, Some(&runs)).state,
        VerdictState::Broken
    );
}
