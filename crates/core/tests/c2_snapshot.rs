//! C2 — the workspace snapshotter's three laws, held by test (A171).

use std::path::Path;
use std::process::Command;

use thruline_core::ledger::Ledger;
use thruline_core::snapshot::{capture, capture_with, load, store};

fn git(cwd: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

fn seeded_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    git(root, &["init", "-q", "-b", "main"]);
    std::fs::write(root.join("tracked.txt"), "committed\n").expect("write");
    std::fs::write(root.join("Cargo.lock"), "lock-bytes\n").expect("write");
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "seed"]);
    // The dirty tree the manifests must speak for:
    std::fs::write(root.join("tracked.txt"), "modified\n").expect("modify");
    std::fs::write(root.join("staged.txt"), "staged-bytes\n").expect("write");
    git(root, &["add", "staged.txt"]);
    std::fs::write(root.join("loose.txt"), "untracked-bytes\n").expect("write");
    dir
}

#[test]
fn the_manifests_speak_for_the_dirty_tree_file_by_file() {
    let dir = seeded_repo();
    let snap = capture(dir.path(), "2026-08-12T17:00:00Z").expect("capture");
    assert!(snap.complete, "a still tree captures complete");
    let git_state = snap.git.as_ref().expect("a repo has git state");
    assert_eq!(git_state.branch.as_deref(), Some("main"));
    assert!(git_state.head.as_deref().is_some_and(|h| h.len() == 40));
    let t = &snap.tree;
    assert_eq!(
        (
            t.staged.len(),
            t.unstaged.len(),
            t.untracked.len(),
            t.deleted.len()
        ),
        (1, 1, 1, 0),
        "one staged, one modified, one untracked"
    );
    // The digest is of the worktree bytes, checkable by anyone.
    let untracked = &t.untracked[0];
    assert_eq!(untracked.path, "loose.txt");
    assert_eq!(
        untracked.sha256.as_deref(),
        Some(thruline_core::chain::sha256_hex(b"untracked-bytes\n").as_str())
    );
    assert!(
        t.dirty_diff_sha256.is_some(),
        "the dirty diff carries a digest"
    );
}

#[test]
fn a_moving_tree_is_an_incomplete_snapshot_that_says_so() {
    let dir = seeded_repo();
    let root = dir.path().to_path_buf();
    let snap = capture_with(&root, "2026-08-12T17:00:00Z", || {
        std::fs::write(root.join("mid-capture.txt"), "moved\n").expect("mutate");
    })
    .expect("capture");
    assert!(!snap.complete, "a moved tree must not read as complete");
    assert!(
        snap.note
            .as_deref()
            .is_some_and(|n| n.contains("capture again")),
        "the incomplete snapshot prescribes the remedy"
    );
}

#[test]
fn the_environment_records_names_and_never_values() {
    let dir = seeded_repo();
    // The canary: a secret-shaped value that must never reach the doc.
    std::env::set_var("THRULINE_C2_CANARY", "supersecret-canary-value-1187");
    let snap = capture(dir.path(), "2026-08-12T17:00:00Z").expect("capture");
    std::env::remove_var("THRULINE_C2_CANARY");
    assert!(
        snap.environment
            .env_names
            .iter()
            .any(|n| n == "THRULINE_C2_CANARY"),
        "the name is recorded"
    );
    let doc = serde_json::to_string(&snap).expect("doc");
    assert!(
        !doc.contains("supersecret-canary-value-1187"),
        "a value crossed into the record — the names-only law is broken"
    );
    assert!(
        snap.environment.lockfiles.contains_key("Cargo.lock"),
        "the lockfile digest is recorded"
    );
}

#[test]
fn stored_snapshots_are_append_only_and_reload_digest_checked() {
    let dir = seeded_repo();
    let ledger = Ledger::open_in_memory().expect("ledger");
    let snap = capture(dir.path(), "2026-08-12T17:00:00Z").expect("capture");
    let sha = store(&ledger, &snap).expect("store");
    let (loaded, stored_sha) = load(&ledger, &snap.id).expect("load").expect("row");
    assert_eq!(stored_sha, sha);
    assert_eq!(loaded.id, snap.id);
    assert_eq!(loaded.tree.staged.len(), snap.tree.staged.len());
    let update = ledger
        .conn()
        .execute("UPDATE workspace_snapshots SET root='x'", []);
    assert!(update.is_err(), "snapshots are append-only by trigger");
    // A doctored doc is named, not served.
    ledger
        .conn()
        .execute_batch("DROP TRIGGER workspace_snapshots_no_update;")
        .expect("drop trigger");
    ledger
        .conn()
        .execute("UPDATE workspace_snapshots SET doc='{}'", [])
        .expect("forge");
    assert!(
        load(&ledger, &snap.id).is_err(),
        "a doc that no longer matches its digest must refuse to load"
    );
}

#[test]
fn a_root_outside_git_degrades_honestly() {
    let dir = tempfile::tempdir().expect("tempdir");
    let snap = capture(dir.path(), "2026-08-12T17:00:00Z").expect("capture");
    assert!(snap.git.is_none());
    assert!(
        snap.note
            .as_deref()
            .is_some_and(|n| n.contains("not a git worktree")),
        "the absence is stated, not invented around"
    );
    assert!(snap.complete);
}

#[test]
fn the_snapshotter_is_structurally_read_only() {
    // Law 1's teeth: no mutating git verb may appear in the module source.
    let src = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/snapshot.rs"),
    )
    .expect("read module");
    for verb in [
        "\"commit\"",
        "\"stash\"",
        "\"reset\"",
        "\"checkout\"",
        "\"clean\"",
        "\"push\"",
        "\"pull\"",
        "\"merge\"",
        "\"rebase\"",
        "\"restore\"",
        "\"rm\"",
        "\"mv\"",
        "\"apply\"",
    ] {
        assert!(
            !src.contains(verb),
            "snapshot.rs invokes git {verb} — the snapshotter reads, it never runs"
        );
    }
}
