//! STEP-7-GIT-LINE (R-2026-09-28-42, founder ruling #21 (a): "The read-time
//! git line, cautious direction only, with its receipt shown.")
//!
//! One read-only engine function compares the git HEAD recorded on a Change's
//! standing `intended` row with the HEAD of the recorded folder NOW. It says
//! something only when the two differ AND git answers within a bound; every
//! other case — equal heads, a folder that is gone, a folder that is not a git
//! repository, a git that does not answer — is silence, never "unchanged".
//!
//! Every repository and store here lives in a tempdir; git runs with no system
//! or global configuration.

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use thruline_core::change_code_moved::{code_moved, code_moved_with, GitReader};
use thruline_core::examined_inputs::bind_intended_at;
use thruline_core::ledger::Ledger;

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("git must run");
    assert!(out.status.success(), "git {args:?} failed");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A repository with `src/lib.rs` committed, and a Change whose `intended`
/// row was recorded at that commit.
fn fixture(repo: &Path, store: &Path) -> Ledger {
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/lib.rs"), b"pub fn one() {}\n").unwrap();
    std::fs::write(repo.join("README.md"), b"shop\n").unwrap();
    git(repo, &["init", "-q", "-b", "main"]);
    git(repo, &["add", "."]);
    git(repo, &["commit", "-q", "-m", "init"]);

    let ledger = Ledger::open(store.join("ledger.db")).expect("open");
    let conn = ledger.conn();
    conn.execute(
        "INSERT INTO projects(id, root_path, source, display_name, created_at)
         VALUES('p1',?1,'claude_code','Synthetic','2026-01-01T00:00:00Z')",
        [repo.to_string_lossy().to_string()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO changes(id, project_id, opened_at, title, purpose, requester, request_source)
         VALUES('c1','p1','2026-01-02T09:00:00Z','Cart total','P','sam','person')",
        [],
    )
    .unwrap();
    let _ = conn;
    bind_intended_at(&ledger, "c1", "2026-01-02T09:20:00Z", Some(repo))
        .expect("bind")
        .expect("a change with an established root binds");
    ledger
}

fn commit_lib_change(repo: &Path) -> String {
    std::fs::write(
        repo.join("src/lib.rs"),
        b"pub fn one() {}\npub fn two() {}\n",
    )
    .unwrap();
    git(repo, &["add", "src/lib.rs"]);
    git(repo, &["commit", "-q", "-m", "two"]);
    git(repo, &["rev-parse", "HEAD"])
}

#[test]
fn a_commit_touching_src_lib_rs_is_said_to_have_moved_the_code_naming_the_path() {
    let repo = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    let ledger = fixture(repo.path(), store.path());
    let recorded = git(repo.path(), &["rev-parse", "HEAD"]);
    let now = commit_lib_change(repo.path());

    let moved = code_moved(&ledger, "c1")
        .expect("reading")
        .expect("the heads differ and git answered, so the code moved");
    assert_eq!(
        moved.recorded, recorded,
        "the receipt names the recorded commit"
    );
    assert_eq!(moved.now, now, "the receipt names the current commit");
    assert_eq!(moved.changed_paths, vec!["src/lib.rs".to_string()]);
    assert_eq!(moved.commits, 1);
    assert!(
        !moved.uncommitted_compared,
        "uncommitted edits are not compared, and the field says so"
    );
}

#[test]
fn equal_heads_say_nothing() {
    let repo = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    let ledger = fixture(repo.path(), store.path());
    /* An uncommitted edit is NOT compared: equal HEADs are silence, and
     * silence is never "unchanged". */
    std::fs::write(repo.path().join("src/lib.rs"), b"dirty\n").unwrap();
    assert_eq!(code_moved(&ledger, "c1").expect("reading"), None);
}

#[test]
fn a_recorded_folder_that_is_gone_says_nothing() {
    let parent = tempfile::tempdir().unwrap();
    let repo = parent.path().join("shop");
    std::fs::create_dir_all(&repo).unwrap();
    let store = tempfile::tempdir().unwrap();
    let ledger = fixture(&repo, store.path());
    commit_lib_change(&repo);
    std::fs::rename(&repo, parent.path().join("moved-away")).unwrap();
    assert_eq!(code_moved(&ledger, "c1").expect("reading"), None);
}

#[test]
fn a_recorded_folder_that_is_no_longer_a_repository_says_nothing() {
    let repo = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    let ledger = fixture(repo.path(), store.path());
    commit_lib_change(repo.path());
    std::fs::rename(repo.path().join(".git"), repo.path().join("not-git")).unwrap();
    assert_eq!(code_moved(&ledger, "c1").expect("reading"), None);
}

#[test]
fn a_change_with_nothing_recorded_says_nothing() {
    let store = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(store.path().join("ledger.db")).expect("open");
    assert_eq!(
        code_moved(&ledger, "no-such-change").expect("reading"),
        None
    );
}

#[cfg(unix)]
#[test]
fn a_git_that_does_not_answer_says_nothing_within_the_bound() {
    use std::os::unix::fs::PermissionsExt;
    let repo = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    let ledger = fixture(repo.path(), store.path());
    commit_lib_change(repo.path());

    /* A stand-in `git` that stalls on every call, holding a child of its own
     * so only a group kill ends it. It lives in the test's own tempdir. */
    let tools = tempfile::tempdir().unwrap();
    let stand_in = tools.path().join("git");
    std::fs::write(&stand_in, "#!/bin/sh\nsleep 60 & wait\n").unwrap();
    std::fs::set_permissions(&stand_in, std::fs::Permissions::from_mode(0o755)).unwrap();

    let reader = GitReader {
        program: stand_in.into_os_string(),
        limit: Duration::from_millis(800),
    };
    let started = Instant::now();
    let answer = code_moved_with(&ledger, "c1", &reader).expect("reading");
    let took = started.elapsed();
    assert_eq!(answer, None, "a git that does not answer is silence");
    assert!(
        took < Duration::from_secs(10),
        "the read was not bounded: {took:?}"
    );
}
