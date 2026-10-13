use std::path::{Path, PathBuf};
use std::process::Command;

use thruline_core::decisions::checks::{check_at, VerdictState};
use thruline_core::decisions::params::{parse_params, PARAMS_SCHEMA_VERSION};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/decisions/text_absent")
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create fixture destination");
    for entry in std::fs::read_dir(from).expect("read fixture") {
        let entry = entry.expect("fixture entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("copy fixture file");
        }
    }
}

fn repo_for(tree: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp repo");
    copy_tree(&fixtures().join(tree), dir.path());
    for args in [
        vec!["init", "-q"],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
    ] {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status()
            .expect("run git");
        assert!(status.success());
    }
    dir
}

#[test]
fn trial_removed_a1_holds_and_broken_per_surface() {
    let raw = std::fs::read_to_string(fixtures().join("params.json")).expect("params fixture");
    let params = parse_params("text_absent", PARAMS_SCHEMA_VERSION, &raw).expect("closed params");
    let holds = repo_for("holds");
    let held = check_at(holds.path(), "HEAD", &params);
    assert_eq!(held.state, VerdictState::Holds);
    assert!(held.findings.is_empty());
    let broken = repo_for("broken");
    let failed = check_at(broken.path(), "HEAD", &params);
    assert_eq!(failed.state, VerdictState::Broken);
    assert!(failed
        .findings
        .iter()
        .any(|f| f.path == "code/trial.rs" && f.line == 3));
    for surface in ["code", "ui_copy", "docs", "tests"] {
        assert!(
            failed
                .surfaces
                .iter()
                .any(|s| s.surface.as_str() == surface && s.finding_count > 0),
            "missing {surface}"
        );
    }
    assert!(!failed.findings.iter().any(|f| f.path == "CHANGELOG.md"));
}

#[test]
fn trial_removed_renamed_term_is_unverifiable_with_candidate() {
    let raw = std::fs::read_to_string(fixtures().join("params.json")).expect("params fixture");
    let params = parse_params("text_absent", PARAMS_SCHEMA_VERSION, &raw).expect("closed params");
    let repo = repo_for("renamed/before");
    let before = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo.path())
        .output()
        .expect("resolve baseline");
    assert!(before.status.success());
    let before = String::from_utf8(before.stdout).expect("baseline oid");
    std::fs::copy(
        fixtures().join("renamed/after/ui/app.jsx"),
        repo.path().join("ui/app.jsx"),
    )
    .expect("copy changed source");
    for args in [
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "renamed",
        ],
    ] {
        let status = Command::new("git")
            .args(args)
            .current_dir(repo.path())
            .status()
            .expect("commit changed version");
        assert!(status.success());
    }
    let result = thruline_core::decisions::checks::check_text_absent_transition(
        repo.path(),
        before.trim(),
        "HEAD",
        match &params {
            thruline_core::decisions::params::CheckParams::TextAbsent(p) => p,
            _ => panic!("expected absence parameters"),
        },
    );
    assert_eq!(result.state, VerdictState::Unverifiable);
    let reason = result.reason.expect("rename reason");
    assert!(reason.contains("term may have been renamed"));
    assert!(reason.contains("evaluation period"));
}

#[test]
fn trial_removed_a1_counts_a_forbidden_path_name() {
    let raw = std::fs::read_to_string(fixtures().join("params.json")).expect("params fixture");
    let params = parse_params("text_absent", PARAMS_SCHEMA_VERSION, &raw).expect("closed params");
    let repo = repo_for("path_only");
    let result = check_at(repo.path(), "HEAD", &params);
    assert_eq!(result.state, VerdictState::Broken);
    assert!(result
        .findings
        .iter()
        .any(|finding| finding.path == "code/trial.rs" && finding.line == 0));
}
