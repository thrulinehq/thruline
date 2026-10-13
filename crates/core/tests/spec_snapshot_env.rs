//! One test, alone in its own binary, because it mutates process environment.
//!
//! `GIT_INDEX_FILE` is read from the process environment by every `git`
//! invocation. Setting it inside a test binary that runs other git-using tests
//! in parallel threads would leak into their `git add`, so this lives apart.

use std::path::Path;
use std::process::Command;

use thruline_core::spec::snapshot::snapshot_tree;

fn git(cwd: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_git_index_file_inherited_from_the_environment_is_ignored() {
    // ThruLine runs inside git hooks, and git exports GIT_INDEX_FILE to some of
    // them. An engine that inherited it would read the wrong index for its
    // read-only commands and, far worse, `git add -A` into whatever index the
    // hook was operating on.
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path();
    git(repo, &["init", "-q", "-b", "main"]);
    git(repo, &["config", "user.email", "test@example.invalid"]);
    git(repo, &["config", "user.name", "ThruLine Test"]);
    std::fs::write(repo.join("a.txt"), "a\n").expect("write");
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-qm", "first"]);
    std::fs::write(repo.join("b.txt"), "b\n").expect("write");

    let decoy = repo.join("decoy.index");
    std::env::set_var("GIT_INDEX_FILE", &decoy);
    let taken = snapshot_tree(repo);
    std::env::remove_var("GIT_INDEX_FILE");

    let snap = taken.expect("snapshot");
    assert_eq!(
        snap.changed_paths_count, 1,
        "the diff was computed against a foreign index"
    );
    assert!(
        !decoy.exists(),
        "the engine wrote to the GIT_INDEX_FILE it inherited from its environment"
    );
}
