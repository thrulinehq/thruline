//! BUILD-PLAN C.7's S3 — the declared git environment, and its digest.
//!
//! The properties that make the field worth carrying in a receipt:
//!
//! 1. It is **stable** for an unchanged repository, or a reviewer comparing two
//!    receipts learns nothing from it.
//! 2. It **moves** for every condition C.7 enumerates — including the four rows
//!    (6, 7, 10, 11) that leave no diff at all, which is the whole reason the
//!    field exists.
//! 3. It does not move for irrelevant configuration, or every machine would
//!    produce a different digest and the field would say nothing.

use std::path::Path;
use std::process::Command;

use thruline_core::spec::git_env::git_env;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@e.invalid")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@e.invalid")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Git reads the machine's system and global configuration as well as the
/// repository's, and `git_env` records what it reads. A machine with Git LFS
/// installed has `filter.lfs.*` there, which is a content transform, so the
/// "an ordinary repository carries none" assertion would depend on the machine
/// running it (it failed on a CI runner with LFS installed). Every test here is
/// about the repository's own configuration, so they all run with neither
/// ambient file. Set once, before the first git process of this binary.
fn hermetic_git_config() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
        std::env::set_var("GIT_CONFIG_GLOBAL", "/dev/null");
    });
}

fn repo() -> tempfile::TempDir {
    hermetic_git_config();
    let dir = tempfile::tempdir().expect("tempdir");
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "user.name", "t"]);
    git(dir.path(), &["config", "user.email", "t@e.invalid"]);
    std::fs::write(dir.path().join("a.py"), "a = 1\n").expect("w");
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-qm", "one"]);
    dir
}

#[test]
fn the_digest_is_stable_for_an_unchanged_repository() {
    let dir = repo();
    let a = git_env(dir.path()).expect("env");
    let b = git_env(dir.path()).expect("env");
    assert_eq!(a.digest, b.digest);
    assert!(a.digest.starts_with("sha256:"), "{}", a.digest);
    // The digest is over the record MINUS itself, the same shape the event
    // chain uses. Two reads of one repository must agree in full.
    assert_eq!(a, b);
}

#[test]
fn every_condition_c7_enumerates_moves_the_digest() {
    // Each case is applied to a fresh repository and compared with the baseline
    // of that same repository, so nothing depends on the order they are tried.
    /// One C.7 row, applied to a fresh repository.
    type Case = (&'static str, Box<dyn Fn(&Path)>);
    let cases: Vec<Case> = vec![
        (
            "row 1 — a filter driver (attribute)",
            Box::new(|p: &Path| {
                std::fs::write(p.join(".gitattributes"), "*.py filter=hide\n").expect("w")
            }),
        ),
        (
            "row 1 — a filter driver (config, which leaves NO diff)",
            Box::new(|p: &Path| git(p, &["config", "filter.hide.clean", "sed /SECRET/d"])),
        ),
        (
            "row 2 — core.autocrlf",
            Box::new(|p: &Path| git(p, &["config", "core.autocrlf", "true"])),
        ),
        (
            "row 6 — .git/info/exclude, which leaves NO diff",
            Box::new(|p: &Path| {
                std::fs::create_dir_all(p.join(".git/info")).expect("mkdir");
                std::fs::write(p.join(".git/info/exclude"), "secret.py\n").expect("w");
            }),
        ),
        (
            "row 7 — .git/info/attributes, out of tree",
            Box::new(|p: &Path| {
                std::fs::create_dir_all(p.join(".git/info")).expect("mkdir");
                std::fs::write(p.join(".git/info/attributes"), "*.py filter=hide\n").expect("w");
            }),
        ),
        (
            "row 8 — a submodule declaration",
            Box::new(|p: &Path| {
                std::fs::write(
                    p.join(".gitmodules"),
                    "[submodule \"dep\"]\n\tpath = vendor/dep\n\turl = ../dep\n",
                )
                .expect("w")
            }),
        ),
        (
            "row 10 — sparse checkout",
            Box::new(|p: &Path| git(p, &["config", "core.sparseCheckout", "true"])),
        ),
        (
            "row 11 — a filesystem monitor",
            Box::new(|p: &Path| git(p, &["config", "core.fsmonitor", "/bin/true"])),
        ),
        (
            "core.excludesFile",
            Box::new(|p: &Path| git(p, &["config", "core.excludesFile", "/tmp/none"])),
        ),
    ];

    for (name, apply) in cases {
        let dir = repo();
        let before = git_env(dir.path()).expect("env").digest;
        apply(dir.path());
        let after = git_env(dir.path()).expect("env").digest;
        assert_ne!(before, after, "{name} did not move the digest");
    }
}

#[test]
fn editing_a_gitattributes_moves_the_digest_even_when_the_filter_set_does_not() {
    // The digest is over the CONTENT of every attribute source, not over the
    // set of filter names it mentions. Widening `filter=hide` from one path to
    // every path changes what is hidden and changes nothing about the names.
    let dir = repo();
    std::fs::write(dir.path().join(".gitattributes"), "one.py filter=hide\n").expect("w");
    let a = git_env(dir.path()).expect("env");
    std::fs::write(dir.path().join(".gitattributes"), "*.py filter=hide\n").expect("w");
    let b = git_env(dir.path()).expect("env");
    assert_ne!(a.digest, b.digest);
}

#[test]
fn irrelevant_configuration_does_not_move_the_digest() {
    // The control, and the property that makes the field readable across
    // machines: a digest that moved with the user's name would differ on every
    // machine, and a reviewer would learn nothing from comparing two.
    let dir = repo();
    let before = git_env(dir.path()).expect("env").digest;
    for args in [
        vec!["config", "user.name", "Someone Else"],
        vec!["config", "user.email", "else@e.invalid"],
        vec!["config", "color.ui", "always"],
        vec!["config", "pull.rebase", "true"],
        vec!["config", "commit.gpgsign", "false"],
    ] {
        git(dir.path(), &args);
    }
    // And an ordinary source edit, which the SNAPSHOT records and the
    // environment must not.
    std::fs::write(dir.path().join("a.py"), "a = 2\n").expect("w");
    assert_eq!(
        before,
        git_env(dir.path()).expect("env").digest,
        "irrelevant configuration moved the environment digest"
    );
}

#[test]
fn a_filter_driver_is_reported_and_an_ordinary_repository_is_not() {
    let dir = repo();
    let clean = git_env(dir.path()).expect("env");
    assert!(
        !clean.has_content_transform(),
        "an ordinary repository was reported as carrying a content transform"
    );
    git(dir.path(), &["config", "filter.hide.clean", "cat"]);
    let filtered = git_env(dir.path()).expect("env");
    assert!(filtered.has_content_transform());
    assert!(
        filtered.filters.contains_key("filter.hide.clean"),
        "{:?}",
        filtered.filters
    );
    // The VALUE is recorded, not only the name: a filter whose command changed
    // is a different filter, and the command is arbitrary code.
    assert_eq!(
        filtered.filters.get("filter.hide.clean"),
        Some(&"cat".to_string())
    );
    git(dir.path(), &["config", "filter.hide.clean", "sed /x/d"]);
    assert_ne!(
        filtered.digest,
        git_env(dir.path()).expect("env").digest,
        "changing a filter's COMMAND did not move the digest"
    );
}
