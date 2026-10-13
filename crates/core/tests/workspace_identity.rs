//! R04 acceptance: identity survives moving, cloning, and worktrees.
//!
//! These are the pack's own ACCEPT lines for R04, run against real git repos
//! rather than fixtures, because the whole point of the derivation is that it
//! agrees with what git actually reports.

use std::path::Path;
use std::process::Command;

use thruline_core::workspace::{
    repo_identity_at, resolve_project_root, worktrees_at, Basis, RootResolution, RootVia,
};

/// Run git with an isolated config, so the developer's own global config (or a
/// CI runner's absence of one) cannot change the result.
fn git(cwd: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=ThruLine Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(cwd)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("git should be runnable in tests");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A repo with one commit and one remote.
fn seed_repo(dir: &Path, remote: &str) {
    std::fs::create_dir_all(dir).expect("create repo dir");
    git(dir, &["init", "-q"]);
    std::fs::write(dir.join("README.md"), "seed\n").expect("write file");
    git(dir, &["add", "README.md"]);
    git(dir, &["commit", "-q", "-m", "seed"]);
    git(dir, &["remote", "add", "origin", remote]);
}

#[test]
fn a_repo_cloned_to_two_paths_has_one_identity() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let origin = tmp.path().join("origin");
    seed_repo(&origin, "https://example.invalid/org/repo.git");

    let a = tmp.path().join("checkout-a");
    let b = tmp.path().join("somewhere/else/checkout-b");
    std::fs::create_dir_all(b.parent().expect("parent")).expect("mkdir");
    git(
        tmp.path(),
        &[
            "clone",
            "-q",
            &origin.to_string_lossy(),
            &a.to_string_lossy(),
        ],
    );
    git(
        tmp.path(),
        &[
            "clone",
            "-q",
            &origin.to_string_lossy(),
            &b.to_string_lossy(),
        ],
    );

    let ia = repo_identity_at(&a).expect("identity a");
    let ib = repo_identity_at(&b).expect("identity b");
    assert_eq!(ia.basis, Basis::Git);
    assert_eq!(
        ia.repo_id, ib.repo_id,
        "two clones of one repo at different paths must share a repo_id"
    );
    assert!(ia.repo_id.is_some());
}

#[test]
fn moving_a_repo_preserves_its_identity() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let before = tmp.path().join("before");
    seed_repo(&before, "https://example.invalid/org/moved.git");
    let id_before = repo_identity_at(&before).expect("before").repo_id;

    let after = tmp.path().join("deeper/after");
    std::fs::create_dir_all(after.parent().expect("parent")).expect("mkdir");
    std::fs::rename(&before, &after).expect("move the repo");

    let id_after = repo_identity_at(&after).expect("after").repo_id;
    assert_eq!(
        id_before, id_after,
        "moving the directory must not re-identify the repo — this is the live \
         defect A88 recorded, where a moved repo became a new project"
    );
    assert!(id_before.is_some());
}

#[test]
fn worktrees_share_a_repo_and_differ_from_each_other() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let main = tmp.path().join("main");
    seed_repo(&main, "https://example.invalid/org/wt.git");
    let feature = tmp.path().join("feature");
    git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            &feature.to_string_lossy(),
            "-b",
            "feature",
        ],
    );

    let main_id = repo_identity_at(&main).expect("main").repo_id;
    let feat_id = repo_identity_at(&feature).expect("feature").repo_id;
    assert_eq!(
        main_id, feat_id,
        "a linked worktree is the SAME repo — this is the case the A88 census \
         found split across 6 unrelated project ids"
    );

    let trees = worktrees_at(&main).expect("worktrees");
    assert_eq!(trees.len(), 2, "primary checkout plus one linked worktree");
    let ids: std::collections::BTreeSet<&str> =
        trees.iter().map(|w| w.worktree_id.as_str()).collect();
    assert_eq!(ids.len(), 2, "worktree ids must be distinct");
    for w in &trees {
        assert_eq!(
            Some(w.repo_id.clone()),
            main_id,
            "every worktree carries its repo"
        );
        assert!(w.base_commit.is_some(), "base commit is captured");
    }
    assert_eq!(
        trees.iter().filter(|w| w.is_linked).count(),
        1,
        "exactly one of the two is a linked worktree"
    );
    assert!(
        trees.iter().any(|w| w.branch.as_deref() == Some("feature")),
        "the linked worktree's branch is captured: {trees:?}"
    );
}

#[test]
fn a_repo_with_no_commits_and_no_remote_abstains() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let empty = tmp.path().join("empty");
    std::fs::create_dir_all(&empty).expect("mkdir");
    git(&empty, &["init", "-q"]);

    let id = repo_identity_at(&empty).expect("identity");
    assert_eq!(
        id.basis,
        Basis::Unresolvable,
        "an empty repo has nothing repo-specific to hash; minting an id would \
         make every empty repo on the machine identical"
    );
    assert!(id.repo_id.is_none());
}

#[test]
fn a_non_git_directory_without_a_marker_abstains() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let plain = tmp.path().join("plain");
    std::fs::create_dir_all(&plain).expect("mkdir");
    let id = repo_identity_at(&plain).expect("identity");
    assert_eq!(id.basis, Basis::Unresolvable);
    assert!(id.repo_id.is_none());
}

#[test]
fn a_marker_identifies_a_non_git_directory_and_survives_a_move() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let before = tmp.path().join("marked");
    std::fs::create_dir_all(&before).expect("mkdir");
    std::fs::write(before.join(".thruline-id"), "01JABCDEF\n").expect("write marker");

    let id_before = repo_identity_at(&before).expect("before");
    assert_eq!(id_before.basis, Basis::Marker);
    assert!(id_before.repo_id.is_some());

    let after = tmp.path().join("moved-marked");
    std::fs::rename(&before, &after).expect("move");
    let id_after = repo_identity_at(&after).expect("after");
    assert_eq!(
        id_before.repo_id, id_after.repo_id,
        "a marker is path-independent by construction"
    );
}

#[test]
fn identity_derivation_never_writes_a_marker() {
    // Deriving identity answers a question. Writing into someone's project is a
    // consent decision (M5's plan/apply discipline), and must not happen as a
    // side effect of being asked.
    let tmp = tempfile::tempdir().expect("tempdir");
    let plain = tmp.path().join("untouched");
    std::fs::create_dir_all(&plain).expect("mkdir");

    let _ = repo_identity_at(&plain).expect("identity");
    let _ = worktrees_at(&plain).expect("worktrees");

    let entries: Vec<_> = std::fs::read_dir(&plain)
        .expect("read dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name())
        .collect();
    assert!(
        entries.is_empty(),
        "identity derivation must leave the directory untouched, found {entries:?}"
    );
}

#[test]
fn recorded_cwd_wins_over_the_munged_name() {
    // The A88 rule, on real directories: the munged name demangles to a path
    // that does not exist, while the recorded cwd does.
    let tmp = tempfile::tempdir().expect("tempdir");
    let real = tmp.path().join("Telescope_project");
    std::fs::create_dir_all(&real).expect("mkdir");

    let munged = "-Users-nobody-Desktop-Telescope-project";
    let resolved = resolve_project_root(munged, Some(&real.to_string_lossy()));
    match resolved {
        RootResolution::Resolved { path, via } => {
            assert_eq!(path, real.to_string_lossy());
            assert_eq!(via, RootVia::RecordedCwd);
        }
        other => panic!("cwd must win, got {other:?}"),
    }
}

#[test]
fn reconstruction_recovers_a_separator_the_munging_destroyed() {
    // `Telescope_project` and `Telescope/project` munge identically. With no
    // recorded cwd, the filesystem is the only evidence of which was real.
    let tmp = tempfile::tempdir().expect("tempdir");
    let real = tmp.path().join("Telescope_project").join("inner");
    std::fs::create_dir_all(&real).expect("mkdir");

    let base = tmp.path().to_string_lossy().replace('/', "-");
    let munged = format!("{base}-Telescope-project-inner");
    match resolve_project_root(&munged, None) {
        RootResolution::Resolved { path, via } => {
            assert_eq!(path, real.to_string_lossy());
            assert_eq!(via, RootVia::Reconstructed);
        }
        other => panic!("expected reconstruction, got {other:?}"),
    }
}

#[test]
fn an_ambiguous_reconstruction_abstains_rather_than_picking() {
    // Both `a_b` and `a/b` exist. Guessing is exactly the failure the census
    // measured, so the only honest answer is that we cannot tell.
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(tmp.path().join("a_b")).expect("mkdir");
    std::fs::create_dir_all(tmp.path().join("a").join("b")).expect("mkdir");

    let base = tmp.path().to_string_lossy().replace('/', "-");
    let munged = format!("{base}-a-b");
    match resolve_project_root(&munged, None) {
        RootResolution::Unresolved { reason } => {
            assert!(
                reason.contains("abstaining"),
                "the reason must say it abstained: {reason}"
            );
        }
        other => panic!("ambiguity must abstain, got {other:?}"),
    }
}

/// A116 — codex records `cwd` under `payload`; Claude Code records it at the
/// top level. Reading only the top level made every codex project resolve to
/// `basis = "unresolvable"` with no `repo_id`, which silently dropped all of
/// them out of A92 repo grouping: a repo two agents both worked in grouped as
/// the Claude half alone, and the packet's repo scope quietly lost the codex
/// side. Found by the L8 walk, which cannot join its two legs without this.
#[test]
fn a_cwd_recorded_under_payload_resolves_the_same_as_a_top_level_one() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).expect("mkdir");
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "user.name", "T"]);
    git(&repo, &["config", "user.email", "t@example.invalid"]);
    std::fs::write(repo.join("a.txt"), "a\n").expect("write");
    git(&repo, &["add", "a.txt"]);
    git(&repo, &["commit", "-qm", "one"]);
    let cwd = repo.canonicalize().expect("canonicalize");
    let cwd = cwd.to_str().expect("utf8");

    let nested = tmp.path().join("codex.jsonl");
    std::fs::write(
        &nested,
        format!(r#"{{"type":"session_meta","payload":{{"id":"s","cwd":"{cwd}"}}}}"#) + "\n",
    )
    .expect("write");
    let top = tmp.path().join("claude.jsonl");
    std::fs::write(&top, format!(r#"{{"type":"user","cwd":"{cwd}"}}"#) + "\n").expect("write");

    let from_nested = thruline_core::workspace::identity_for_project(
        "p-codex",
        std::slice::from_ref(&nested.to_string_lossy().into_owned()),
        16,
        "2030-01-01T00:00:00Z",
    )
    .expect("codex identity");
    let from_top = thruline_core::workspace::identity_for_project(
        "p-claude",
        std::slice::from_ref(&top.to_string_lossy().into_owned()),
        16,
        "2030-01-01T00:00:00Z",
    )
    .expect("claude identity");

    assert_eq!(from_nested.basis, "git", "the payload shape must resolve");
    assert_eq!(
        from_nested.repo_id, from_top.repo_id,
        "and both shapes must reach the SAME repo, or cross-agent grouping is a fiction"
    );
    assert!(from_nested.repo_id.is_some(), "with a real repo id");
}
