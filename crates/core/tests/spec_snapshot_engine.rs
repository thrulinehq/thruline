//! Integration tests for the §4.4 snapshot engine, against real git repos.
//!
//! The engine runs `git add -A`, which is a mutating command. Everything here
//! is ultimately in service of one assertion — `the_users_real_index_is_byte_identical`
//! — and the rest establishes that the snapshot is a *correct* one and not
//! merely a harmless one.

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;
use thruline_core::spec::snapshot::{snapshot_tree, SnapshotError};

/// A throwaway repository with a deterministic identity, so nothing depends on
/// the machine's git config and no test can accidentally read the user's.
struct Repo {
    dir: TempDir,
}

impl Repo {
    fn new() -> Repo {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = Repo { dir };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.email", "test@example.invalid"]);
        repo.git(&["config", "user.name", "ThruLine Test"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .current_dir(self.path())
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn write(&self, rel: &str, contents: &str) {
        let path = self.path().join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(path, contents).expect("write");
    }

    fn index_path(&self) -> PathBuf {
        self.path().join(".git/index")
    }

    /// Everything about the real index that a mutation would disturb.
    fn index_state(&self) -> Option<(Vec<u8>, std::time::SystemTime)> {
        let path = self.index_path();
        let bytes = std::fs::read(&path).ok()?;
        let mtime = std::fs::metadata(&path).ok()?.modified().ok()?;
        Some((bytes, mtime))
    }

    /// The paths git currently has staged, as porcelain sees them.
    fn staged(&self) -> String {
        self.git(&["diff", "--cached", "--name-only"])
    }

    /// The paths in a tree object.
    fn tree_paths(&self, tree: &str) -> Vec<String> {
        let out = self.git(&["ls-tree", "-r", "--name-only", tree]);
        out.lines().map(str::to_string).collect()
    }
}

#[test]
fn the_users_real_index_is_byte_identical() {
    // THE test. `git add -A` against the real index would stage the whole
    // working tree and silently change what the user's next commit captures.
    let repo = Repo::new();
    repo.write("tracked.txt", "one\n");
    repo.git(&["add", "tracked.txt"]);
    repo.git(&["commit", "-qm", "first"]);

    // Put the index in a *distinctive* state: one path staged, one modified but
    // unstaged, one untracked. An engine that clobbered the index would have to
    // reproduce this exact state by accident to pass.
    repo.write("staged.txt", "staged\n");
    repo.git(&["add", "staged.txt"]);
    repo.write("tracked.txt", "one changed\n");
    repo.write("untracked.txt", "untracked\n");

    // Read the porcelain state FIRST and snapshot the index bytes LAST. `git
    // status` refreshes the index's stat cache and writes it back, so a probe
    // taken before it would attribute the test's own mtime change to the
    // engine. (Measured: it moved the mtime by ~29 ms, and finding out which
    // command did it is exactly why the probe is ordered this way.)
    let before_staged = repo.staged();
    let before_status = repo.git(&["status", "--porcelain"]);
    assert_eq!(
        before_staged, "staged.txt",
        "test setup did not stage anything"
    );
    let before_index = repo.index_state().expect("index exists");

    // Take several snapshots: a single clean run could pass by not having run.
    for _ in 0..3 {
        snapshot_tree(repo.path()).expect("snapshot");
    }

    let after_index = repo.index_state().expect("index still exists");
    assert_eq!(
        before_index.0, after_index.0,
        "the snapshot engine rewrote the user's index"
    );
    assert_eq!(
        before_index.1, after_index.1,
        "the snapshot engine touched the user's index file (mtime moved), which \
         is enough to make git re-stat the whole tree"
    );
    assert_eq!(
        before_staged,
        repo.staged(),
        "the snapshot engine changed what is staged"
    );
    assert_eq!(
        before_status,
        repo.git(&["status", "--porcelain"]),
        "the snapshot engine changed the working tree state"
    );
}

#[test]
fn untracked_files_are_included_and_ignored_files_are_not() {
    let repo = Repo::new();
    repo.write(".gitignore", "ignored.txt\nbuild/\n");
    repo.write("tracked.txt", "t\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-qm", "first"]);

    repo.write("untracked.txt", "u\n");
    repo.write("nested/deep/new.txt", "n\n");
    repo.write("ignored.txt", "secret\n");
    repo.write("build/artifact.bin", "junk\n");

    let snap = snapshot_tree(repo.path()).expect("snapshot");
    let paths = repo.tree_paths(&snap.tree_hash);

    for want in [
        "tracked.txt",
        "untracked.txt",
        "nested/deep/new.txt",
        ".gitignore",
    ] {
        assert!(
            paths.iter().any(|p| p == want),
            "{want} missing from the snapshot tree: {paths:?}"
        );
    }
    for unwanted in ["ignored.txt", "build/artifact.bin"] {
        assert!(
            !paths.iter().any(|p| p == unwanted),
            "{unwanted} was captured despite .gitignore: {paths:?}"
        );
    }
    assert!(snap.dirty);
    // Two new paths against HEAD.
    assert_eq!(snap.changed_paths_count, 2, "paths: {paths:?}");
    assert_eq!(
        snap.head_commit.as_deref(),
        Some(repo.git(&["rev-parse", "HEAD"]).as_str())
    );
}

#[test]
fn thrulineignore_excludes_without_discarding_the_users_global_excludes() {
    let repo = Repo::new();
    repo.write("keep.txt", "k\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-qm", "first"]);

    // A global excludes file, as a real user would have.
    let global = repo.path().join("global-excludes");
    std::fs::write(&global, "*.globalignored\n").expect("write global excludes");
    repo.git(&["config", "core.excludesFile", &global.display().to_string()]);

    repo.write("notes.thruline-scratch", "scratch\n");
    repo.write("thing.globalignored", "global\n");
    repo.write("visible.txt", "v\n");

    // Without .thrulineignore: the global exclude applies, the scratch file does not.
    let before = snapshot_tree(repo.path()).expect("snapshot");
    let paths = repo.tree_paths(&before.tree_hash);
    assert!(paths.iter().any(|p| p == "notes.thruline-scratch"));
    assert!(
        !paths.iter().any(|p| p == "thing.globalignored"),
        "the user's global excludes were not honoured: {paths:?}"
    );

    // With .thrulineignore: the scratch file drops out AND the global exclude
    // is still honoured. The second half is the one that matters — the naive
    // implementation (point core.excludesFile at .thrulineignore) passes the
    // first and fails this.
    repo.write(".thrulineignore", "*.thruline-scratch\n");
    let after = snapshot_tree(repo.path()).expect("snapshot");
    let paths = repo.tree_paths(&after.tree_hash);
    assert!(
        !paths.iter().any(|p| p == "notes.thruline-scratch"),
        ".thrulineignore was not applied: {paths:?}"
    );
    assert!(
        !paths.iter().any(|p| p == "thing.globalignored"),
        "applying .thrulineignore discarded the user's global excludes: {paths:?}"
    );
    assert!(paths.iter().any(|p| p == "visible.txt"));

    // And the user's configuration was not rewritten.
    assert_eq!(
        repo.git(&["config", "--get", "core.excludesFile"]),
        global.display().to_string(),
        "the engine wrote to the user's git config"
    );
}

#[test]
fn a_clean_tree_snapshots_to_head_and_is_not_dirty() {
    let repo = Repo::new();
    repo.write("a.txt", "a\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-qm", "first"]);

    let snap = snapshot_tree(repo.path()).expect("snapshot");
    assert_eq!(snap.tree_hash, repo.git(&["rev-parse", "HEAD^{tree}"]));
    assert!(!snap.dirty);
    assert_eq!(snap.changed_paths_count, 0);
}

#[test]
fn a_repository_with_no_commits_yet_snapshots_without_a_head() {
    let repo = Repo::new();
    repo.write("first.txt", "hello\n");
    repo.write("second.txt", "world\n");

    let snap = snapshot_tree(repo.path()).expect("snapshot");
    assert_eq!(
        snap.head_commit, None,
        "an unborn HEAD must not be an error"
    );
    assert!(snap.dirty);
    assert_eq!(snap.changed_paths_count, 2);
    assert_eq!(repo.tree_paths(&snap.tree_hash).len(), 2);
}

#[test]
fn deletions_are_captured() {
    let repo = Repo::new();
    repo.write("gone.txt", "x\n");
    repo.write("stays.txt", "y\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-qm", "first"]);

    std::fs::remove_file(repo.path().join("gone.txt")).expect("remove");
    let snap = snapshot_tree(repo.path()).expect("snapshot");
    let paths = repo.tree_paths(&snap.tree_hash);
    assert_eq!(paths, vec!["stays.txt".to_string()]);
    assert_eq!(snap.changed_paths_count, 1);
    assert!(snap.dirty);
}

#[test]
fn the_same_tree_snapshots_to_the_same_hash() {
    // Content-addressed, so this is a statement about the engine adding no
    // nondeterminism of its own (a timestamp, a temp path, an index ordering).
    let repo = Repo::new();
    repo.write("a.txt", "a\n");
    repo.write("dir/b.txt", "b\n");

    let first = snapshot_tree(repo.path()).expect("snapshot");
    let second = snapshot_tree(repo.path()).expect("snapshot");
    assert_eq!(first, second);

    repo.write("a.txt", "a changed\n");
    let third = snapshot_tree(repo.path()).expect("snapshot");
    assert_ne!(
        first.tree_hash, third.tree_hash,
        "a content change did not move the tree hash"
    );
}

#[test]
fn snapshots_work_from_a_subdirectory_and_cover_the_whole_repo() {
    let repo = Repo::new();
    repo.write("top.txt", "t\n");
    repo.write("sub/inner.txt", "i\n");

    let from_root = snapshot_tree(repo.path()).expect("snapshot");
    let from_sub = snapshot_tree(&repo.path().join("sub")).expect("snapshot");
    assert_eq!(
        from_root, from_sub,
        "a snapshot taken from a subdirectory must cover the repository, not the \
         subdirectory — a check that ran in `sub/` is still evidence about the repo"
    );
}

#[test]
fn a_path_that_is_not_a_repository_is_an_error_not_a_hash() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert!(matches!(
        snapshot_tree(dir.path()),
        Err(SnapshotError::NotARepository { .. })
    ));
    assert!(matches!(
        snapshot_tree(&dir.path().join("does-not-exist")),
        Err(SnapshotError::NotARepository { .. })
    ));
}

#[test]
fn a_nested_repository_without_a_commit_is_a_named_error_not_a_silent_skip() {
    // Reported by the independent verifier: `git add -A` exits non-zero here,
    // so there is no tree at all. The dangerous failure would be to catch that
    // and carry on without the nested directory — a tree hash that silently
    // omits part of the working tree is worse than no tree, because everything
    // downstream would bind to it as if it were complete.
    let repo = Repo::new();
    repo.write("outer.txt", "o\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-qm", "first"]);

    let inner = repo.path().join("inner");
    std::fs::create_dir_all(&inner).expect("mkdir");
    let out = Command::new("git")
        .current_dir(&inner)
        .args(["init", "-q", "-b", "main"])
        .output()
        .expect("git init");
    assert!(out.status.success());
    std::fs::write(inner.join("child.txt"), "c\n").expect("write");

    match snapshot_tree(repo.path()) {
        Err(SnapshotError::NestedRepositoryWithoutCommit { path }) => {
            assert!(
                path.contains("inner"),
                "the error must name the offending directory, got {path:?}"
            );
        }
        other => panic!("expected a named nested-repository error, got {other:?}"),
    }

    // And the workaround the error message suggests actually works, so the
    // message is advice rather than decoration.
    repo.write(".thrulineignore", "inner/\n");
    let snap = snapshot_tree(repo.path()).expect("snapshot with the nested repo excluded");
    assert!(!repo
        .tree_paths(&snap.tree_hash)
        .iter()
        .any(|p| p.starts_with("inner/")));
}

#[test]
fn a_nested_repository_with_a_commit_is_recorded_as_a_gitlink_and_is_not_an_error() {
    // The control. Only the *commitless* nested repo is fatal; a real submodule
    // records as a gitlink and must keep working, or this guard would break
    // every repository that has one.
    let repo = Repo::new();
    repo.write("outer.txt", "o\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-qm", "first"]);

    let inner = repo.path().join("inner");
    std::fs::create_dir_all(&inner).expect("mkdir");
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["config", "user.email", "test@example.invalid"],
        vec!["config", "user.name", "ThruLine Test"],
    ] {
        let out = Command::new("git")
            .current_dir(&inner)
            .args(&args)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}");
    }
    std::fs::write(inner.join("child.txt"), "c\n").expect("write");
    for args in [vec!["add", "-A"], vec!["commit", "-qm", "inner"]] {
        let out = Command::new("git")
            .current_dir(&inner)
            .args(&args)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}");
    }

    let snap = snapshot_tree(repo.path()).expect("a committed nested repo must snapshot");
    assert!(!snap.tree_hash.is_empty());
}

#[test]
fn a_gitattributes_clean_filter_defeats_tree_binding_and_the_snapshot_says_so() {
    // The verifier's most serious finding, reproduced as a standing test.
    //
    // A clean filter runs on the way INTO the object database, so the tree
    // names the filtered bytes while the working tree holds the originals. An
    // agent can therefore change what the code does and leave the tree hash
    // untouched — and §7 re-execution will materialise the honest code from
    // that tree, pass, and stamp the false claim `reexecuted: true`.
    //
    // This module cannot prevent it. It must never be silent about it.
    let repo = Repo::new();
    repo.write(".gitattributes", "*.py filter=strip-hidden\n");
    // A clean filter that deletes everything from the marker onwards.
    repo.git(&["config", "filter.strip-hidden.clean", "sed '/# HIDDEN/,$d'"]);
    repo.write("app.py", "def f(x):\n    return x + 1\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-qm", "first"]);

    let before = snapshot_tree(repo.path()).expect("snapshot");

    // Now hide special-casing below the marker: exactly BUILD-SPEC §6.8's
    // "env sniffing newly introduced in source".
    repo.write(
        "app.py",
        "def f(x):\n    return x + 1\n# HIDDEN\nimport os\nif os.environ.get('PYTEST_CURRENT_TEST'):\n    def f(x):\n        return 42\n",
    );
    let after = snapshot_tree(repo.path()).expect("snapshot");

    assert_eq!(
        before.tree_hash, after.tree_hash,
        "if this ever fails, git's clean-filter behaviour changed and the whole \
         hazard should be re-derived rather than assumed"
    );
    assert_eq!(
        after.changed_paths_count, 0,
        "the diff is blind to it too — the tree genuinely does not see the change"
    );

    // The bytes on disk did change, so the tree hash is not a faithful name for
    // the working tree.
    let on_disk = std::fs::read_to_string(repo.path().join("app.py")).expect("read");
    assert!(on_disk.contains("PYTEST_CURRENT_TEST"));

    // THE ASSERTION. The snapshot must declare it.
    assert!(
        after.content_filters.degrades_capture(),
        "a snapshot taken under a clean filter did not mark capture degraded — \
         §13 requires such claims to be `unverifiable`, never `supported`, and \
         this flag is what carries that"
    );
    assert!(after.content_filters.drivers.contains("strip-hidden"));
    assert!(after
        .content_filters
        .sources
        .iter()
        .any(|s| s == ".gitattributes"));
    assert!(
        after.content_filters.describe().contains("strip-hidden"),
        "describe(): {}",
        after.content_filters.describe()
    );
    // It was already degraded on the first snapshot, before anything was hidden.
    assert!(before.content_filters.degrades_capture());
}

#[test]
fn an_uncommitted_gitattributes_is_still_detected() {
    // The case that actually matters: an agent installing a laundering filter
    // does not commit it first. The scan reads attributes files out of the
    // snapshot tree, which includes untracked files, so this works.
    let repo = Repo::new();
    repo.write("keep.txt", "k\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-qm", "first"]);
    assert!(!snapshot_tree(repo.path())
        .expect("snapshot")
        .content_filters
        .degrades_capture());

    repo.write("sub/.gitattributes", "*.rs filter=sneaky\n");
    let snap = snapshot_tree(repo.path()).expect("snapshot");
    assert!(
        snap.content_filters.degrades_capture(),
        "an uncommitted, nested .gitattributes was not detected"
    );
    assert!(snap.content_filters.drivers.contains("sneaky"));
    assert!(snap
        .content_filters
        .sources
        .iter()
        .any(|s| s == "sub/.gitattributes"));
}

#[test]
fn line_ending_normalisation_is_recorded_but_does_not_degrade_capture() {
    // The discrimination that keeps the flag meaningful. `* text=auto` is in a
    // large fraction of real repositories; treating those as degraded would
    // make `degrades_capture` mean "you have a .gitattributes". A `text`
    // attribute cannot hide a changed statement — only a changed line
    // terminator — while `filter=` runs arbitrary code and `ident` rewrites
    // content.
    let repo = Repo::new();
    repo.write(".gitattributes", "* text=auto\n*.txt eol=lf\n");
    repo.write("a.txt", "a\n");
    let snap = snapshot_tree(repo.path()).expect("snapshot");
    assert!(!snap.content_filters.degrades_capture());
    assert!(snap.content_filters.drivers.is_empty());
    assert_eq!(
        snap.content_filters
            .other_transforms
            .iter()
            .collect::<Vec<_>>(),
        vec!["eol", "text"]
    );

    // `ident` rewrites content, so it does degrade.
    repo.write(".gitattributes", "*.c ident\n");
    assert!(snapshot_tree(repo.path())
        .expect("snapshot")
        .content_filters
        .degrades_capture());
}

#[test]
fn a_clean_repository_reports_no_filters() {
    // The negative control: without it, `degrades_capture` could be returning
    // true unconditionally and every assertion above would still pass.
    let repo = Repo::new();
    repo.write("a.txt", "a\n");
    let snap = snapshot_tree(repo.path()).expect("snapshot");
    assert!(!snap.content_filters.degrades_capture());
    assert_eq!(
        snap.content_filters.describe(),
        "no content filters in scope"
    );
    assert!(snap.content_filters.sources.is_empty());
}

#[test]
fn a_gitattributes_under_a_non_ascii_directory_is_still_detected() {
    // Git C-quotes non-ASCII paths by default, so `ls-tree --name-only` returns
    // `"caf\303\251/.gitattributes"` — WITH the surrounding quotes. A suffix
    // test for `/.gitattributes` misses it, and the file hiding a laundering
    // filter is exactly the file worth putting under such a name.
    let repo = Repo::new();
    repo.write("keep.txt", "k\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-qm", "first"]);
    repo.write("caf\u{e9}/.gitattributes", "*.rs filter=hidden\n");

    let snap = snapshot_tree(repo.path()).expect("snapshot");
    assert!(
        snap.content_filters.degrades_capture(),
        "a .gitattributes under a non-ASCII directory was not detected: {:?}",
        snap.content_filters
    );
    assert!(snap.content_filters.drivers.contains("hidden"));

    // And the raw git output really is quoted, so this case is discriminating
    // rather than incidentally passing.
    let quoted = repo.git(&["ls-tree", "-r", "--name-only", &snap.tree_hash]);
    assert!(
        quoted.contains('"'),
        "git no longer quotes non-ASCII paths here, so this test has stopped \
         discriminating: {quoted}"
    );
}

#[test]
fn snapshotting_an_unchanged_tree_writes_no_new_git_objects() {
    // BUILD-PLAN C.6 accepted that snapshots leave loose objects in the user's
    // repository, on the condition that the cost is *disclosed with a real
    // number* rather than an adjective. This is the property behind the number:
    // growth is bounded by distinct content the agent actually wrote, not by how
    // often ThruLine snapshots. Blobs and trees are content-addressed, so a
    // snapshot of an unchanged tree writes nothing at all.
    //
    // Measured on a 30-file fixture repository: 50 snapshots, each after
    // changing one file, added 151 objects / 604 KB; 20 further snapshots with
    // no change added 0. `doctor` should report the live equivalent of the first
    // figure, and this test is what keeps the second one true.
    fn object_count(repo: &Path) -> usize {
        fn walk(dir: &Path, n: &mut usize) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, n);
                } else {
                    *n += 1;
                }
            }
        }
        let mut n = 0;
        walk(&repo.join(".git/objects"), &mut n);
        n
    }

    let repo = Repo::new();
    for i in 0..10 {
        repo.write(&format!("src/f{i}.rs"), &format!("fn f{i}() {{}}\n"));
    }
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-qm", "first"]);

    // Prime: the first snapshot of a dirty tree legitimately writes objects.
    repo.write("src/f0.rs", "fn f0() { /* edited */ }\n");
    snapshot_tree(repo.path()).expect("snapshot");
    let settled = object_count(repo.path());

    for _ in 0..20 {
        snapshot_tree(repo.path()).expect("snapshot");
    }
    assert_eq!(
        object_count(repo.path()),
        settled,
        "repeated snapshots of an unchanged tree grew the object database — the \
         disclosed cost is bounded by content written, and this is what bounds it"
    );

    // And a real change does add objects, so the assertion above is not passing
    // because snapshotting has quietly stopped writing anything.
    repo.write("src/f1.rs", "fn f1() { /* new content */ }\n");
    snapshot_tree(repo.path()).expect("snapshot");
    assert!(
        object_count(repo.path()) > settled,
        "a content change wrote no objects — the tree hash cannot name content \
         git has not stored, so re-execution would have nothing to materialise"
    );
}

#[cfg(unix)]
#[test]
fn a_fifo_is_counted_rather_than_silently_dropped() {
    // Measured: `git add -A` omits a FIFO with exit 0, no warning, and no
    // diagnostic, and `git status --porcelain` does not mention it either — the
    // resulting tree is byte-identical to one taken with no FIFO present. So
    // "the tree hash names the whole working directory" had a silent exception.
    //
    // Silent omission is the failure mode this engine already rejected once,
    // for nested repositories. It is reported here rather than made fatal
    // because a pipe holds no persistent content: there is nothing a tree hash
    // could have named, so refusing to snapshot would cost more than it buys.
    use std::process::Command as Cmd;

    let repo = Repo::new();
    repo.write("a.txt", "a\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-qm", "first"]);

    let clean = snapshot_tree(repo.path()).expect("snapshot");
    assert!(clean.omitted.is_empty(), "a clean tree omits nothing");

    let fifo = repo.path().join("pipe.fifo");
    let made = Cmd::new("mkfifo").arg(&fifo).status().expect("mkfifo runs");
    assert!(made.success(), "mkfifo failed");

    let with_fifo = snapshot_tree(repo.path()).expect("snapshot");

    // The tree really is identical — this is the fact that makes the omission
    // invisible, and the reason the report has to carry it separately.
    assert_eq!(
        clean.tree_hash, with_fifo.tree_hash,
        "if git ever starts representing FIFOs this test should be re-derived"
    );
    assert_eq!(
        with_fifo.omitted,
        vec![thruline_core::spec::snapshot::OmittedEntry {
            path: "pipe.fifo".to_string(),
            kind: "fifo",
        }],
        "the FIFO was dropped without being counted"
    );
}

#[cfg(unix)]
#[test]
fn an_ignored_fifo_is_not_reported_as_an_omission() {
    // The control that keeps the report meaningful. A FIFO the user has already
    // told git to ignore is not something we failed to capture — it is
    // something we were asked to skip. Without this, any repository with a
    // socket under an ignored build directory would report a permanent
    // omission and the field would become noise.
    use std::process::Command as Cmd;

    let repo = Repo::new();
    repo.write(".gitignore", "run/\n");
    repo.write("a.txt", "a\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-qm", "first"]);
    std::fs::create_dir_all(repo.path().join("run")).expect("mkdir");
    assert!(Cmd::new("mkfifo")
        .arg(repo.path().join("run/ignored.fifo"))
        .status()
        .expect("mkfifo runs")
        .success());

    let snap = snapshot_tree(repo.path()).expect("snapshot");
    assert!(
        snap.omitted.is_empty(),
        "an ignored FIFO was reported as an omission: {:?}",
        snap.omitted
    );

    // And an unignored one alongside it still is, so the filter is not simply
    // suppressing everything.
    assert!(Cmd::new("mkfifo")
        .arg(repo.path().join("visible.fifo"))
        .status()
        .expect("mkfifo runs")
        .success());
    let snap = snapshot_tree(repo.path()).expect("snapshot");
    assert_eq!(snap.omitted.len(), 1);
    assert_eq!(snap.omitted[0].path, "visible.fifo");
}
