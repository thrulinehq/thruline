//! One test, alone in its own binary, because it mutates `PATH`.
//!
//! Counting git process spawns requires a shim ahead of git on `PATH`, and
//! `PATH` is process-wide. Left in the main snapshot suite, every sibling test
//! running in parallel would route its own `git` calls through the counter and
//! inflate the measurement — the same hazard that put `spec_snapshot_env.rs` in
//! its own binary.

use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use thruline_core::spec::snapshot::snapshot_tree;

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
        repo
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn git(&self, args: &[&str]) {
        let out = Command::new("git")
            .current_dir(self.path())
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn write(&self, rel: &str, contents: &str) {
        let path = self.path().join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(path, contents).expect("write");
    }
}

#[test]
fn many_gitattributes_files_cost_a_bounded_number_of_git_processes() {
    // The defect: one `git cat-file` spawned per `.gitattributes` in the tree.
    // A monorepo with hundreds turned every snapshot into hundreds of process
    // spawns, against §4.1's 50 ms p95 hook budget.
    //
    // Counting PROCESSES rather than asserting a duration: a timing threshold
    // on CI hardware is a flake generator, and the real claim is that the cost
    // is O(1) in the number of attributes files, not that it is fast today.
    // The count is obtained by putting a counting shim ahead of git on PATH.
    let repo = Repo::new();
    repo.write("keep.txt", "k\n");
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-qm", "first"]);

    let shim_dir = tempfile::tempdir().expect("tempdir");
    let counter = shim_dir.path().join("git-calls");
    let real_git = String::from_utf8_lossy(
        &Command::new("which")
            .arg("git")
            .output()
            .expect("which git")
            .stdout,
    )
    .trim()
    .to_string();
    let shim = shim_dir.path().join("git");
    std::fs::write(
        &shim,
        format!(
            "#!/bin/sh\nprintf '.' >> '{}'\nexec '{}' \"$@\"\n",
            counter.display(),
            real_git
        ),
    )
    .expect("write shim");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    let path_with_shim = format!(
        "{}:{}",
        shim_dir.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let count_calls = |n_attributes: usize| -> usize {
        for i in 0..n_attributes {
            std::fs::create_dir_all(repo.path().join(format!("m{i}"))).expect("mkdir");
            std::fs::write(
                repo.path().join(format!("m{i}/.gitattributes")),
                format!("*.g{i} filter=f{i}\n"),
            )
            .expect("write");
        }
        let _ = std::fs::remove_file(&counter);
        let out = Command::new(&shim)
            .current_dir(repo.path())
            .env("PATH", &path_with_shim)
            .args(["--version"])
            .output();
        assert!(out.is_ok(), "the shim is not executable");
        let baseline = std::fs::read(&counter).map(|b| b.len()).unwrap_or(0);

        // Drive the real engine with the shim ahead of git on PATH.
        let saved = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", &path_with_shim);
        let snap = snapshot_tree(repo.path());
        std::env::set_var("PATH", saved);

        let snap = snap.expect("snapshot");
        assert!(
            n_attributes == 0 || snap.content_filters.degrades_capture(),
            "the {n_attributes} attributes files were not detected"
        );
        assert_eq!(
            snap.content_filters.drivers.len(),
            n_attributes,
            "not every attributes file was read"
        );
        std::fs::read(&counter).map(|b| b.len()).unwrap_or(0) - baseline
    };

    let few = count_calls(2);
    let many = count_calls(40);
    assert_eq!(
        few, many,
        "reading 40 .gitattributes files costs {many} git processes where 2 cost \
         {few} — the batching regressed to one spawn per file"
    );
    assert!(
        many <= 8,
        "a snapshot spawns {many} git processes; the budget is a small constant"
    );
}
