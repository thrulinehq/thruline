//! Exact Git history adapter for the advisory K checks. The production
//! adapter lives in a new file and is included directly until the parent
//! registers it in the Decisions module.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use thruline_core::decisions::checks_k_candidates;

mod decisions {
    pub use thruline_core::decisions::{checks_k_candidates, checks_k_structural};
}

mod redact {
    pub use thruline_core::redact::*;
}

#[path = "../src/decisions/checks_k_history.rs"]
mod checks_k_history;

use checks_k_candidates::KCandidateState;
use checks_k_history::{scan_k_history_first_parent, scan_k_history_pair};

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git executes");
    assert!(
        output.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git UTF-8 output")
        .trim()
        .to_owned()
}

fn repo() -> TempDir {
    let temp = tempfile::tempdir().expect("temp repo");
    git(temp.path(), &["init", "-q"]);
    temp
}

fn commit(repo: &Path, message: &str) -> String {
    git(repo, &["add", "-A"]);
    git(
        repo,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-q",
            "-m",
            message,
        ],
    );
    git(repo, &["rev-parse", "HEAD"])
}

fn write(repo: &Path, relative: &str, content: &str) {
    let path = repo.join(relative);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    std::fs::write(path, content).expect("write fixture");
}

fn state(rows: &[checks_k_candidates::KCandidateOutcome], code: &str) -> KCandidateState {
    rows.iter()
        .find(|row| row.code == code)
        .expect("kind row")
        .state
}

#[test]
fn k1_added_skip_is_a_source_backed_advisory_at_target_line() {
    let temp = repo();
    write(
        temp.path(),
        "ui/src/widget.test.js",
        "test('works', () => {});\n",
    );
    let base = commit(temp.path(), "base");
    write(
        temp.path(),
        "ui/src/widget.test.js",
        "test('works', () => {});\ntest.skip('deferred', () => {});\n",
    );
    let target = commit(temp.path(), "skip");
    let rows = scan_k_history_pair(temp.path(), &base, &target);
    let k1 = rows.iter().find(|row| row.code == "K1").expect("K1");
    assert_eq!(k1.state, KCandidateState::Signal);
    assert_eq!(k1.observations.len(), 1);
    assert_eq!(k1.observations[0].path, "ui/src/widget.test.js");
    assert_eq!(k1.observations[0].line, 2);
    assert_eq!(k1.observations[0].marker, "test_skipped");
    assert_eq!(state(&rows, "K2"), KCandidateState::NoSignal);
}

#[test]
fn k1_identical_test_move_nets_added_lines() {
    let temp = repo();
    write(
        temp.path(),
        "ui/src/old.test.js",
        "test.skip('old', () => {});\n",
    );
    let base = commit(temp.path(), "base");
    std::fs::rename(
        temp.path().join("ui/src/old.test.js"),
        temp.path().join("ui/src/new.test.js"),
    )
    .expect("move test");
    let target = commit(temp.path(), "move");
    let rows = scan_k_history_pair(temp.path(), &base, &target);
    assert_eq!(state(&rows, "K1"), KCandidateState::NoSignal);
    // A Git move is not an accepted test-to-source link; K2 abstains.
    assert_eq!(state(&rows, "K2"), KCandidateState::Unverifiable);
}

#[test]
fn k2_deletion_without_source_link_abstains_even_when_source_remains() {
    let temp = repo();
    write(temp.path(), "src/calc.rs", "pub fn calc() -> i32 { 1 }\n");
    write(
        temp.path(),
        "tests/calc.rs",
        "#[test] fn calc() { assert_eq!(1, 1); }\n",
    );
    let base = commit(temp.path(), "base");
    std::fs::remove_file(temp.path().join("tests/calc.rs")).expect("delete test");
    let target = commit(temp.path(), "delete test");
    let rows = scan_k_history_pair(temp.path(), &base, &target);
    let k2 = rows.iter().find(|row| row.code == "K2").expect("K2");
    assert_eq!(k2.state, KCandidateState::Unverifiable);
    assert!(k2.reason.is_some());
}

#[test]
fn exact_first_parent_identity_and_bounded_history_are_required() {
    let temp = repo();
    write(temp.path(), "a.txt", "a\n");
    let first = commit(temp.path(), "first");
    write(temp.path(), "b.txt", "b\n");
    let second = commit(temp.path(), "second");
    write(temp.path(), "c.txt", "c\n");
    let third = commit(temp.path(), "third");

    let short = scan_k_history_pair(temp.path(), &first[..8], &second);
    assert_eq!(short.len(), 10);
    assert!(short
        .iter()
        .all(|row| row.state == KCandidateState::Unverifiable));
    let wrong_parent = scan_k_history_pair(temp.path(), &first, &third);
    assert!(wrong_parent
        .iter()
        .all(|row| row.state == KCandidateState::Unverifiable));

    let pairs = scan_k_history_first_parent(temp.path(), &third, 2).expect("two pairs");
    assert_eq!(pairs.len(), 2);
    assert_eq!((&pairs[0].baseline, &pairs[0].target), (&second, &third));
    assert_eq!((&pairs[1].baseline, &pairs[1].target), (&first, &second));
    assert_eq!(pairs[0].outcomes.len(), 10);
    assert!(scan_k_history_first_parent(temp.path(), &third, 201).is_err());
    assert!(scan_k_history_first_parent(temp.path(), &third[..8], 2).is_err());
}

#[test]
fn unsupported_record_links_never_become_no_signal_or_holds() {
    let temp = repo();
    write(temp.path(), "CLAUDE.md", "Rule.\n");
    write(temp.path(), ".github/workflows/ci.yml", "jobs: {}\n");
    let base = commit(temp.path(), "base");
    write(temp.path(), "CLAUDE.md", "Rule changed.\n");
    write(
        temp.path(),
        ".github/workflows/ci.yml",
        "jobs:\n  test: {}\n",
    );
    write(temp.path(), "docs/PLAN.md", "New plan.\n");
    let target = commit(temp.path(), "record gaps");
    let rows = scan_k_history_pair(temp.path(), &base, &target);
    for code in ["K3", "K8", "K9", "K13", "K14", "K15", "K16", "K17"] {
        let row = rows.iter().find(|row| row.code == code).expect("kind row");
        assert_eq!(row.state, KCandidateState::Unverifiable, "{code}");
        assert!(row.reason.is_some(), "{code}");
    }
    let k13 = rows.iter().find(|row| row.code == "K13").expect("K13");
    assert_eq!(k13.observations[0].path, "CLAUDE.md");
    assert_eq!(
        k13.observations[0].marker,
        "rule_file_changed_actor_unverified"
    );
    let k17 = rows.iter().find(|row| row.code == "K17").expect("K17");
    assert_eq!(k17.observations[0].path, "docs/PLAN.md");
    assert_eq!(
        k17.observations[0].marker,
        "spec_plan_changed_actor_unverified"
    );
}

#[test]
fn unreadable_changed_test_blob_abstains_on_k1() {
    let temp = repo();
    write(temp.path(), "src/a.test.js", "test('ok', () => {});\n");
    let base = commit(temp.path(), "base");
    let large = "x".repeat(2 * 1024 * 1024 + 1);
    write(temp.path(), "src/a.test.js", &large);
    let target = commit(temp.path(), "too large");
    let rows = scan_k_history_pair(temp.path(), &base, &target);
    assert_eq!(state(&rows, "K1"), KCandidateState::Unverifiable);
}
