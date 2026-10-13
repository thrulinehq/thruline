//! B3–B5 bind preservation evidence to exact Git objects and attributed edits.

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use thruline_core::decisions::checks::VerdictState;
use thruline_core::decisions::checks_preservation::{
    check_assertion_floor, check_ci_step_present, check_rule_file_present, AssertionCensus,
    AssertionFile, AssertionFloorParams, CiStepCensus, CiStepPresentParams, RuleFilePresentParams,
};
use thruline_core::decisions::checks_record::{EditActor, EditCensus, RecordedEdit};

struct Fixture {
    dir: TempDir,
    baseline: String,
    moved: String,
    reduced: String,
    baseline_tree: String,
    moved_tree: String,
    reduced_tree: String,
    old_blob: String,
    moved_blob: String,
    reduced_blob: String,
    ci_baseline_blob: String,
    ci_reduced_blob: String,
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
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git(repo, &["init", "-q"]);
    git(repo, &["config", "user.name", "Decisions Fixture"]);
    git(repo, &["config", "user.email", "fixture@example.invalid"]);
    fs::create_dir_all(repo.join("tests")).unwrap();
    fs::create_dir_all(repo.join(".github/workflows")).unwrap();
    fs::write(
        repo.join("tests/old.rs"),
        "#[test] fn proof() { assert!(true); assert_eq!(1, 1); }\n",
    )
    .unwrap();
    fs::write(
        repo.join(".github/workflows/ci.yml"),
        "jobs:\n  build:\n    steps:\n      - name: test\n        run: cargo test\n# coverage 80\n",
    )
    .unwrap();
    fs::write(repo.join("CLAUDE.md"), "Always review the evidence.\n").unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-qm", "baseline"]);
    let baseline = git(repo, &["rev-parse", "HEAD"]);
    let baseline_tree = git(repo, &["rev-parse", "HEAD^{tree}"]);
    let old_blob = git(repo, &["rev-parse", "HEAD:tests/old.rs"]);
    let ci_baseline_blob = git(repo, &["rev-parse", "HEAD:.github/workflows/ci.yml"]);
    git(repo, &["mv", "tests/old.rs", "tests/new.rs"]);
    git(repo, &["commit", "-qm", "move test"]);
    let moved = git(repo, &["rev-parse", "HEAD"]);
    let moved_tree = git(repo, &["rev-parse", "HEAD^{tree}"]);
    let moved_blob = git(repo, &["rev-parse", "HEAD:tests/new.rs"]);
    fs::write(
        repo.join("tests/new.rs"),
        "#[test] fn proof() { assert!(true); }\n",
    )
    .unwrap();
    fs::write(repo.join(".github/workflows/ci.yml"), "jobs:\n  build:\n    steps:\n      - name: test\n        run: cargo test || true\n# coverage 70\n").unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-qm", "weaken preserved proof"]);
    let reduced = git(repo, &["rev-parse", "HEAD"]);
    let reduced_tree = git(repo, &["rev-parse", "HEAD^{tree}"]);
    let reduced_blob = git(repo, &["rev-parse", "HEAD:tests/new.rs"]);
    let ci_reduced_blob = git(repo, &["rev-parse", "HEAD:.github/workflows/ci.yml"]);
    Fixture {
        dir,
        baseline,
        moved,
        reduced,
        baseline_tree,
        moved_tree,
        reduced_tree,
        old_blob,
        moved_blob,
        reduced_blob,
        ci_baseline_blob,
        ci_reduced_blob,
    }
}

fn assertion_file(f: &Fixture, target_blob: &str, count: u64) -> AssertionFile {
    AssertionFile {
        baseline_path: Some("tests/old.rs".into()),
        target_path: Some("tests/new.rs".into()),
        baseline_blob: Some(f.old_blob.clone()),
        target_blob: Some(target_blob.into()),
        baseline_count: 2,
        target_count: count,
        moved_to: Some("tests/new.rs".into()),
    }
}

#[test]
fn b3_nets_recorded_move_but_breaks_on_assertion_floor_loss() {
    let f = fixture();
    let params = AssertionFloorParams {
        paths: vec!["tests/**".into()],
        minimum: 2,
        baseline_version: f.baseline.clone(),
    };
    let mut evidence = AssertionCensus {
        baseline_version: f.baseline.clone(),
        baseline_tree_hash: f.baseline_tree.clone(),
        target_version: f.moved.clone(),
        target_tree_hash: f.moved_tree.clone(),
        complete: true,
        files: vec![assertion_file(&f, &f.moved_blob, 2)],
    };
    assert_eq!(
        check_assertion_floor(f.dir.path(), &f.moved, &params, &evidence).state,
        VerdictState::Holds
    );
    evidence.target_version = f.reduced.clone();
    evidence.target_tree_hash = f.reduced_tree.clone();
    evidence.files[0] = assertion_file(&f, &f.reduced_blob, 1);
    assert_eq!(
        check_assertion_floor(f.dir.path(), &f.reduced, &params, &evidence).state,
        VerdictState::Broken
    );
    evidence.files[0].target_blob = Some(f.moved_blob.clone());
    assert_eq!(
        check_assertion_floor(f.dir.path(), &f.reduced, &params, &evidence).state,
        VerdictState::Unverifiable
    );
}

#[test]
fn b4_detects_missing_step_lowered_threshold_and_new_bypass() {
    let f = fixture();
    let params = CiStepPresentParams {
        workflow: ".github/workflows/ci.yml".into(),
        step: "test".into(),
        threshold_pct: Some(80),
    };
    let mut evidence = CiStepCensus {
        baseline_version: f.baseline.clone(),
        baseline_tree_hash: f.baseline_tree.clone(),
        target_version: f.moved.clone(),
        target_tree_hash: f.moved_tree.clone(),
        workflow: params.workflow.clone(),
        step: params.step.clone(),
        baseline_blob: f.ci_baseline_blob.clone(),
        target_blob: f.ci_baseline_blob.clone(),
        complete: true,
        baseline_step_present: true,
        target_step_present: true,
        baseline_threshold_pct: Some(80),
        target_threshold_pct: Some(80),
        baseline_bypasses: Vec::new(),
        target_bypasses: Vec::new(),
    };
    assert_eq!(
        check_ci_step_present(f.dir.path(), &f.moved, &params, &evidence).state,
        VerdictState::Holds
    );
    evidence.target_version = f.reduced.clone();
    evidence.target_tree_hash = f.reduced_tree.clone();
    evidence.target_blob = f.ci_reduced_blob.clone();
    evidence.target_threshold_pct = Some(70);
    evidence.target_bypasses.push("|| true".into());
    assert_eq!(
        check_ci_step_present(f.dir.path(), &f.reduced, &params, &evidence).state,
        VerdictState::Broken
    );
    evidence.complete = false;
    assert_eq!(
        check_ci_step_present(f.dir.path(), &f.reduced, &params, &evidence).state,
        VerdictState::Unverifiable
    );
}

#[test]
fn b5_requires_person_edit_for_agent_tamper_on_same_exact_version() {
    let f = fixture();
    let params = RuleFilePresentParams {
        paths: vec!["CLAUDE.md".into()],
        person_edit_basis: "same_version".into(),
    };
    let mut evidence = EditCensus {
        from_version_id: Some(f.moved.clone()),
        version_id: f.reduced.clone(),
        tree_hash: f.reduced_tree.clone(),
        complete: true,
        edits: vec![RecordedEdit {
            path: "CLAUDE.md".into(),
            actor: EditActor::Agent,
            session_id: Some("agent-a".into()),
            edit_id: Some("edit1".into()),
        }],
    };
    assert_eq!(
        check_rule_file_present(f.dir.path(), &f.reduced, &params, &evidence).state,
        VerdictState::Broken
    );
    evidence.edits.push(RecordedEdit {
        path: "CLAUDE.md".into(),
        actor: EditActor::Person,
        session_id: Some("person".into()),
        edit_id: Some("edit2".into()),
    });
    assert_eq!(
        check_rule_file_present(f.dir.path(), &f.reduced, &params, &evidence).state,
        VerdictState::Holds
    );
    evidence.edits[0].actor = EditActor::Unknown;
    assert_eq!(
        check_rule_file_present(f.dir.path(), &f.reduced, &params, &evidence).state,
        VerdictState::Unverifiable
    );
}
