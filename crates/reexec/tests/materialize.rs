//! §7 materialisation, and the ruling it exists to enforce.
//!
//! BUILD-PLAN C.7/4: *"Re-execution materialises without filters. Never a plain
//! `git checkout` — write blobs directly via `git cat-file` into the target, so
//! the container holds exactly the tree bytes and smudge cannot inject."*
//!
//! The first test does not assert that ruling; it **demonstrates the
//! difference**. A `.gitattributes` smudge filter is installed, `git checkout`
//! is run as the control, and the two answers are shown to differ — so the test
//! fails if someone ever switches the implementation to a checkout, rather than
//! merely documenting that they should not.

use std::path::Path;
use std::process::Command;

use thruline_reexec::materialize::materialize;

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
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn init_repo(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.name", "t"]);
    git(dir, &["config", "user.email", "t@example.invalid"]);
}

#[test]
fn a_smudge_filter_changes_what_checkout_writes_and_not_what_materialise_writes() {
    let repo = tempfile::tempdir().expect("tempdir");
    let root = repo.path();
    init_repo(root);

    // A filter pair whose smudge half REWRITES content on the way out of the
    // object store. This is C.7 row 1, the measured attack, in its injecting
    // direction.
    std::fs::write(root.join(".gitattributes"), "shipping.py filter=inject\n")
        .expect("write attributes");
    git(root, &["config", "filter.inject.clean", "cat"]);
    git(
        root,
        &["config", "filter.inject.smudge", "sed s/HONEST/INJECTED/"],
    );
    std::fs::write(root.join("shipping.py"), "value = 'HONEST'\n").expect("write source");
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "one"]);
    let tree = git(root, &["rev-parse", "HEAD^{tree}"]);

    // The control: what a checkout produces. Removing the file and restoring it
    // runs the smudge filter, exactly as materialising by checkout would.
    std::fs::remove_file(root.join("shipping.py")).expect("remove");
    git(root, &["checkout", "--", "shipping.py"]);
    let checked_out = std::fs::read_to_string(root.join("shipping.py")).expect("read");
    assert!(
        checked_out.contains("INJECTED"),
        "the control is broken: this git did not run the smudge filter, so the test \
         below proves nothing. Got {checked_out:?}"
    );

    // What §7 actually does.
    let target = tempfile::tempdir().expect("tempdir");
    let out = materialize(root, &tree, target.path()).expect("materialise");
    let materialised =
        std::fs::read_to_string(target.path().join("shipping.py")).expect("read materialised");

    assert_eq!(
        materialised, "value = 'HONEST'\n",
        "materialisation must write the TREE bytes. A checkout writes {checked_out:?} \
         for the same tree hash, which is why its content is not a function of the tree \
         hash at all."
    );
    assert_ne!(
        materialised, checked_out,
        "the two paths agreed, so this test can no longer tell them apart"
    );
    assert!(out.complete());
}

#[test]
fn every_materialised_file_is_byte_identical_to_its_blob() {
    let repo = tempfile::tempdir().expect("tempdir");
    let root = repo.path();
    init_repo(root);
    std::fs::create_dir_all(root.join("pkg/sub")).expect("mkdir");
    std::fs::write(root.join("pkg/a.py"), "a = 1\n").expect("w");
    std::fs::write(root.join("pkg/sub/b.txt"), "b\nb\n").expect("w");
    // Binary content with a NUL and a byte sequence that is not valid UTF-8.
    std::fs::write(root.join("blob.bin"), [0u8, 159, 146, 150, 10, 0]).expect("w");
    std::fs::write(root.join("run.sh"), "#!/bin/sh\necho hi\n").expect("w");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = std::fs::metadata(root.join("run.sh"))
            .expect("meta")
            .permissions();
        p.set_mode(0o755);
        std::fs::set_permissions(root.join("run.sh"), p).expect("chmod");
        std::os::unix::fs::symlink("pkg/a.py", root.join("link.py")).expect("symlink");
    }
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "one"]);
    let tree = git(root, &["rev-parse", "HEAD^{tree}"]);

    let target = tempfile::tempdir().expect("tempdir");
    let out = materialize(root, &tree, target.path()).expect("materialise");

    for (path, oid) in &out.files {
        let blob = Command::new("git")
            .current_dir(root)
            .args(["cat-file", "blob", oid])
            .output()
            .expect("cat-file");
        let written = std::fs::read(target.path().join(path)).expect("read written");
        assert_eq!(
            written, blob.stdout,
            "{path} differs from the blob the tree names"
        );
    }
    assert!(out.files.contains_key("blob.bin"), "binary file missing");
    assert!(
        out.files.contains_key("pkg/sub/b.txt"),
        "nested file missing"
    );

    // `.git` is not in a tree, so the container never sees the repository.
    assert!(
        !target.path().join(".git").exists(),
        "the materialised tree carries a .git directory"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(target.path().join("run.sh"))
            .expect("meta")
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "the executable bit was lost");
        assert_eq!(
            out.symlinks.get("link.py").map(String::as_str),
            Some("pkg/a.py"),
            "the symlink was not reproduced as a symlink"
        );
        let meta = std::fs::symlink_metadata(target.path().join("link.py")).expect("meta");
        assert!(meta.file_type().is_symlink());
    }
}

#[test]
fn a_submodule_leaves_a_hole_that_is_reported_rather_than_filled() {
    let inner = tempfile::tempdir().expect("tempdir");
    init_repo(inner.path());
    std::fs::write(inner.path().join("dep.py"), "dep = 1\n").expect("w");
    git(inner.path(), &["add", "-A"]);
    git(inner.path(), &["commit", "-qm", "dep"]);

    let outer = tempfile::tempdir().expect("tempdir");
    init_repo(outer.path());
    std::fs::write(outer.path().join("main.py"), "main = 1\n").expect("w");
    git(outer.path(), &["add", "-A"]);
    git(outer.path(), &["commit", "-qm", "main"]);
    let added = Command::new("git")
        .current_dir(outer.path())
        .args([
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            &inner.path().to_string_lossy(),
            "vendor/dep",
        ])
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
        .output()
        .expect("submodule add");
    if !added.status.success() {
        // Some git builds refuse local submodule transports outright. Skipping
        // loudly beats a green test that exercised nothing.
        eprintln!(
            "SKIPPED: this git refuses a local submodule ({}) — the gitlink path is \
             covered by the unit test in materialize.rs",
            String::from_utf8_lossy(&added.stderr).trim()
        );
        return;
    }
    git(outer.path(), &["commit", "-qm", "vendor"]);
    let tree = git(outer.path(), &["rev-parse", "HEAD^{tree}"]);

    let target = tempfile::tempdir().expect("tempdir");
    let out = materialize(outer.path(), &tree, target.path()).expect("materialise");
    assert!(
        out.gitlinks.contains_key("vendor/dep"),
        "the submodule was not recorded: {:?}",
        out.gitlinks
    );
    assert!(
        !out.complete(),
        "a tree with a submodule has content the tree does not name, so it is not complete"
    );
    assert!(
        !target.path().join("vendor/dep/dep.py").exists(),
        "content that is not in the tree was invented"
    );
}

#[test]
fn a_tree_hash_that_does_not_exist_is_an_error_not_an_empty_directory() {
    let repo = tempfile::tempdir().expect("tempdir");
    init_repo(repo.path());
    std::fs::write(repo.path().join("a.py"), "a\n").expect("w");
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-qm", "one"]);

    let target = tempfile::tempdir().expect("tempdir");
    let err = materialize(
        repo.path(),
        "0000000000000000000000000000000000000000",
        target.path(),
    )
    .expect_err("a missing tree must be an error");
    // An empty `Ok` here would materialise nothing and then run tests against
    // an empty directory, which passes trivially in several runners.
    assert!(format!("{err}").contains("ls-tree"), "{err}");
}

/// Feed `args` on stdin and return trimmed stdout.
fn git_stdin(dir: &Path, args: &[&str], input: &[u8]) -> String {
    use std::io::Write;
    let mut child = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env_remove("GIT_INDEX_FILE")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(input)
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// The tree that writes outside its own directory.
///
/// A single git tree cannot name both `evil` (a symlink) and `evil/x` (a file
/// under it) — the names collide. It can name `Evil` and `evil/x`, which are two
/// entries to git and **one path** on a case-insensitive filesystem (macOS's
/// APFS by default, and Windows). Unicode normalisation gives the same shape on
/// APFS for an NFC/NFD pair. `git ls-tree -r` emits `Evil` first (`E` < `e`), so
/// the symlink is planted before the write that walks through it.
///
/// Returns the tree hash; `Evil` points at `escape_to`.
fn tree_that_escapes(root: &Path, escape_to: &Path) -> String {
    let link = git_stdin(
        root,
        &["hash-object", "-w", "--stdin"],
        escape_to.to_string_lossy().as_bytes(),
    );
    let payload = git_stdin(root, &["hash-object", "-w", "--stdin"], b"PWNED\n");
    let subtree = git_stdin(
        root,
        &["mktree"],
        format!("100644 blob {payload}\tx\n").as_bytes(),
    );
    git_stdin(
        root,
        &["mktree"],
        format!("120000 blob {link}\tEvil\n040000 tree {subtree}\tevil\n").as_bytes(),
    )
}

#[test]
fn a_write_can_never_land_outside_the_materialisation_directory() {
    let repo = tempfile::tempdir().expect("tempdir");
    let root = repo.path();
    init_repo(root);
    let outside = tempfile::tempdir().expect("tempdir");
    let tree = tree_that_escapes(root, outside.path());

    // The control that makes this test mean something: `git checkout-index`
    // writes the same tree without escaping — it drops the colliding symlink
    // and keeps `evil/x` inside the prefix. So the escape below is a property of
    // this materialiser, not of the tree.
    let control = tempfile::tempdir().expect("tempdir");
    let index = repo.path().join("control-index");
    let status = Command::new("git")
        .current_dir(root)
        .args(["read-tree", &tree])
        .env("GIT_INDEX_FILE", &index)
        .status()
        .expect("read-tree");
    assert!(status.success(), "read-tree failed");
    let status = Command::new("git")
        .current_dir(root)
        .args([
            "checkout-index",
            "-a",
            "-f",
            "--prefix",
            &format!("{}/", control.path().display()),
        ])
        .env("GIT_INDEX_FILE", &index)
        .status()
        .expect("checkout-index");
    assert!(status.success(), "checkout-index failed");
    assert!(
        !outside.path().join("x").exists(),
        "the control is broken: git checkout-index itself escaped, so this test cannot \
         attribute an escape to the materialiser"
    );

    // What §7 does.
    let target = tempfile::tempdir().expect("tempdir");
    let result = materialize(root, &tree, target.path());
    let landed = outside.path().join("x");
    assert!(
        !landed.exists(),
        "a byte landed OUTSIDE the materialisation directory, at {}, and materialise \
         returned {result:?}. §0.3 binds every check to a tree hash; a materialisation \
         that writes outside its own directory is not a function of the tree.",
        landed.display()
    );
    // Same filesystem split as the sibling test below: `Evil` and `evil/` are
    // one path on a case-insensitive host and two on a case-sensitive one. The
    // escape assertion above holds on both and is the property that matters;
    // only the *refusal* is case-dependent, because on a case-sensitive host
    // there is nothing to collide and so nothing to refuse.
    let probe = tempfile::tempdir().expect("tempdir");
    std::fs::write(probe.path().join("Case"), b"x").expect("probe");
    if !probe.path().join("case").exists() {
        result.expect("distinct paths must materialise");
        let meta = std::fs::symlink_metadata(target.path().join("Evil")).expect("Evil");
        assert!(
            meta.file_type().is_symlink(),
            "the symlink entry was resolved instead of written as a link"
        );
        assert!(
            target.path().join("evil").join("x").is_file(),
            "the directory entry did not receive its own file"
        );
        eprintln!("MATERIALISED (case-sensitive): distinct entries, victim untouched");
        return;
    }

    let err = result.expect_err(
        "the tree names a path that resolves outside the target and was written anyway; \
         materialisation must refuse it rather than complete",
    );
    let message = format!("{err}");
    assert!(
        message.contains("is a symlink this materialisation wrote"),
        "refused, but not for the reason that matters: {message}"
    );
    assert!(
        message.contains("\"Evil\"") || message.contains("\"evil\""),
        "the refusal does not name the component that would have been walked through: {message}"
    );
    eprintln!("REFUSED: {message}");
}

#[test]
fn a_second_entry_at_the_same_path_cannot_be_written_through_the_first() {
    // The other half of the same collision. Here the symlink is at the FINAL
    // component rather than a parent: the tree names `Evil` (a symlink to a
    // file outside) and `evil` (a regular file). `fs::write` on the second
    // opens the path with O_TRUNC and follows the link, overwriting whatever it
    // points at. `create_new` refuses.
    let repo = tempfile::tempdir().expect("tempdir");
    let root = repo.path();
    init_repo(root);
    let outside = tempfile::tempdir().expect("tempdir");
    let victim = outside.path().join("canary.txt");
    std::fs::write(&victim, "UNTOUCHED\n").expect("write victim");

    let link = git_stdin(
        root,
        &["hash-object", "-w", "--stdin"],
        victim.to_string_lossy().as_bytes(),
    );
    let payload = git_stdin(root, &["hash-object", "-w", "--stdin"], b"PWNED\n");
    let tree = git_stdin(
        root,
        &["mktree"],
        format!("120000 blob {link}\tEvil\n100644 blob {payload}\tevil\n").as_bytes(),
    );

    // `Evil` and `evil` are one path on a case-insensitive filesystem (macOS
    // default) and two on a case-sensitive one (Linux, and `ubuntu-latest` in
    // CI). Both are real deployments, so this asserts the property that holds
    // on each rather than the behaviour of whichever host ran it last — an
    // unconditional `expect_err` here passes on macOS and panics on CI, which
    // is a test that reports the filesystem instead of the code.
    let probe = tempfile::tempdir().expect("tempdir");
    std::fs::write(probe.path().join("Case"), b"x").expect("probe");
    let case_insensitive = probe.path().join("case").exists();

    let target = tempfile::tempdir().expect("tempdir");
    let result = materialize(root, &tree, target.path());

    // Invariant on BOTH filesystems, and the one that actually matters: no byte
    // outside the materialisation directory is touched, however the two entries
    // resolve.
    assert_eq!(
        std::fs::read_to_string(&victim).expect("read victim"),
        "UNTOUCHED\n",
        "a file outside the materialisation directory was overwritten through a symlink \
         the tree planted; materialise returned {result:?} (case_insensitive={case_insensitive})"
    );

    if case_insensitive {
        let message = format!("{}", result.expect_err("must refuse"));
        assert!(
            message.contains("already holds that path"),
            "refused, but not as the collision it is: {message}"
        );
        eprintln!("REFUSED (case-insensitive): {message}");
    } else {
        // Two distinct paths, so there is no collision to refuse. The tree is
        // materialised faithfully: `Evil` stays a symlink and is NOT followed,
        // and `evil` is the payload. Asserting this rather than skipping keeps
        // the case-sensitive half of the world covered.
        result.expect("distinct paths must materialise");
        let meta = std::fs::symlink_metadata(target.path().join("Evil")).expect("Evil");
        assert!(
            meta.file_type().is_symlink(),
            "the symlink entry was resolved instead of written as a link"
        );
        assert_eq!(
            std::fs::read_to_string(target.path().join("evil")).expect("evil"),
            "PWNED\n",
            "the regular entry did not receive its own blob"
        );
        eprintln!("MATERIALISED (case-sensitive): two distinct entries, victim untouched");
    }
}
