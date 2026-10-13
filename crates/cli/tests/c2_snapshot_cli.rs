//! C2 — the snapshot verb's process-level contract (A171).

use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

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

fn snapshot(db: &Path, root: &Path) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .args([
            "--db",
            db.to_str().unwrap(),
            "snapshot",
            "--root",
            root.to_str().unwrap(),
        ])
        .output()
        .expect("run snapshot");
    assert!(out.status.success(), "{out:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn a_dirty_repo_snapshot_prints_the_reading_and_stores_the_receipt() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("repo");
    std::fs::create_dir_all(&root).expect("mkdir");
    git(&root, &["init", "-q", "-b", "main"]);
    std::fs::write(root.join("a.txt"), "committed\n").expect("write");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "seed"]);
    std::fs::write(root.join("a.txt"), "changed\n").expect("modify");
    std::fs::write(root.join("new.txt"), "loose\n").expect("write");

    let db = temp.path().join("ledger.db");
    let out = snapshot(&db, &root);
    assert!(out.contains("WORKSPACE SNAPSHOT"), "{out}");
    assert!(out.contains("main"), "the branch is named: {out}");
    assert!(
        out.contains("1 changed") && out.contains("1 untracked"),
        "the tree counts carry nouns: {out}"
    );
    assert!(
        out.contains("values never"),
        "the names-only law is stated on the surface: {out}"
    );
    assert!(
        out.contains("complete     yes"),
        "a still tree reads complete: {out}"
    );
    assert!(
        out.contains("it never runs"),
        "the read-only stance is printed: {out}"
    );
    for line in out.lines() {
        assert_eq!(
            thruline_core::voice::banned_in_output(line),
            None,
            "engine lexicon reached the snapshot surface: {line:?}"
        );
    }
    // The receipt is a stored, digest-checked row.
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    let n: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM workspace_snapshots", [], |r| r.get(0))
        .expect("count");
    assert_eq!(n, 1);
}

#[test]
fn a_non_repo_root_reads_honestly_and_still_exits_zero() {
    let temp = tempdir().expect("tempdir");
    let root = temp.path().join("plain");
    std::fs::create_dir_all(&root).expect("mkdir");
    let db = temp.path().join("ledger.db");
    let out = snapshot(&db, &root);
    assert!(
        out.contains("not a git worktree"),
        "the absence is stated: {out}"
    );
}
