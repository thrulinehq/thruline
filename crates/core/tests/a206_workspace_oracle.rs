//! A206 — THE workspace-oracle harness (MANDATE-v4 §12, §32, §33).
//!
//! Real temporary git repositories, real commands, and expectations owned by
//! the oracle, not by the canonicalizer under test: every negative case's
//! expected CLASS is derived from the manipulation performed on the repo,
//! and the positive digest vectors are pinned from python3 hashlib runs over
//! the documented canonical layout (git blob ids are content-deterministic,
//! so a real repository's index digest is hand-computable outside Rust).
//!
//! Every §12 hard negative case is here: wrong repository, wrong worktree,
//! wrong branch, different HEAD, detached-HEAD mismatch, staged mismatch,
//! dirty-tracked mismatch, relevant-untracked mismatch, deleted-file
//! mismatch, rename mismatch, file-mode mismatch, symlink mismatch,
//! submodule mismatch, concurrent mutation, incomplete fingerprint — plus
//! merge conflicts, determinism across independent captures, nuisance
//! metamorphics, live read-only proof, and the foreground writer end to end.
//! Mode-bit and symlink cases are `cfg(unix)`: they run on the macOS dev
//! host and on Linux CI; only a Windows host would skip them, and Windows is
//! not a supported dev host for this repository.
//!
//! Each mismatch case asserts a positive control first (same state ⇒
//! `Match`), so the rejection provably comes from the manipulation.

use std::path::Path;
use std::process::Command;

use thruline_core::ledger::Ledger;
use thruline_core::observation::verify_chain;
use thruline_core::workspace_identity::{
    compare, compare_with, observe, observe_with, record_observation, Axis, Comparison,
    Observation, RecordOutcome, Scope, WorkspaceFingerprint,
};

fn git(cwd: &Path, args: &[&str]) -> String {
    let out = git_raw(cwd, args);
    assert!(out.status.success(), "git {args:?} in {cwd:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn git_allow_fail(cwd: &Path, args: &[&str]) {
    let _ = git_raw(cwd, args);
}

fn git_raw(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .env("GIT_AUTHOR_NAME", "oracle")
        .env("GIT_AUTHOR_EMAIL", "oracle@test")
        .env("GIT_COMMITTER_NAME", "oracle")
        .env("GIT_COMMITTER_EMAIL", "oracle@test")
        .output()
        .expect("git runs")
}

fn write(root: &Path, rel: &str, content: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(path, content).expect("write");
}

/// A repository with one committed base file, on branch `main`.
fn seeded_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    seed_at(dir.path());
    dir
}

fn seed_at(root: &Path) {
    git(root, &["init", "-q", "-b", "main"]);
    write(root, "base.txt", "base\n");
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "seed"]);
}

/// Observe and require completeness — the expected side of every case.
fn fingerprint(root: &Path) -> WorkspaceFingerprint {
    match observe(root, Scope::everything()).expect("observe") {
        Observation::Complete(fp) => *fp,
        Observation::Incomplete { reason, .. } => {
            panic!("expected a complete observation, got incomplete: {reason}")
        }
    }
}

fn axes_of(c: &Comparison) -> Vec<Axis> {
    match c {
        Comparison::Mismatch { axes } => axes.clone(),
        other => panic!("expected a mismatch, got {other:?}"),
    }
}

/// The positive control: the same state must match before the manipulation,
/// so the rejection afterwards is attributable to the manipulation alone.
fn assert_match(expected: &WorkspaceFingerprint, root: &Path) {
    assert_eq!(
        compare(expected, root).expect("compare"),
        Comparison::Match,
        "positive control: the unmanipulated state must match"
    );
}

/* ── the §12 hard negative cases ────────────────────────────────────── */

#[test]
fn wrong_repository_is_rejected() {
    let a = seeded_repo();
    let b = tempfile::tempdir().expect("tempdir");
    git(b.path(), &["init", "-q", "-b", "main"]);
    write(b.path(), "other.txt", "different history\n");
    git(b.path(), &["add", "."]);
    git(b.path(), &["commit", "-q", "-m", "other"]);

    let expected = fingerprint(a.path());
    assert_match(&expected, a.path());
    let axes = axes_of(&compare(&expected, b.path()).expect("compare"));
    assert!(
        axes.contains(&Axis::RepositoryIdentity),
        "a different repository must be rejected on repository identity: {axes:?}"
    );
}

#[test]
fn wrong_worktree_of_the_same_repository_is_rejected() {
    let base = tempfile::tempdir().expect("tempdir");
    let repo = base.path().join("repo");
    std::fs::create_dir_all(&repo).expect("mkdir");
    seed_at(&repo);
    git(&repo, &["branch", "wt2"]);
    let wt = base.path().join("wt");
    git(
        &repo,
        &["worktree", "add", wt.to_str().expect("utf8"), "wt2"],
    );

    let expected = fingerprint(&repo);
    assert_match(&expected, &repo);
    let axes = axes_of(&compare(&expected, &wt).expect("compare"));
    assert!(
        axes.contains(&Axis::WorktreeIdentity),
        "a sibling worktree must be rejected on worktree identity: {axes:?}"
    );
    assert!(axes.contains(&Axis::Root), "its root differs too: {axes:?}");
    assert!(
        !axes.contains(&Axis::RepositoryIdentity),
        "same repository — the rejection must be the worktree, not the repo: {axes:?}"
    );
}

#[test]
fn wrong_branch_at_the_same_commit_is_rejected() {
    let dir = seeded_repo();
    let expected = fingerprint(dir.path());
    assert_match(&expected, dir.path());
    git(dir.path(), &["checkout", "-q", "-b", "other"]);
    let axes = axes_of(&compare(&expected, dir.path()).expect("compare"));
    assert!(
        axes.contains(&Axis::BranchState),
        "a different branch must be rejected on branch state: {axes:?}"
    );
    assert!(
        !axes.contains(&Axis::Head),
        "the commit did not move — HEAD must not be blamed: {axes:?}"
    );
}

#[test]
fn different_head_is_rejected() {
    let dir = seeded_repo();
    let expected = fingerprint(dir.path());
    assert_match(&expected, dir.path());
    write(dir.path(), "base.txt", "advanced\n");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "advance"]);
    let axes = axes_of(&compare(&expected, dir.path()).expect("compare"));
    assert!(
        axes.contains(&Axis::Head),
        "a different HEAD commit must be rejected: {axes:?}"
    );
}

#[test]
fn detached_head_mismatch_is_rejected() {
    let dir = seeded_repo();
    let expected = fingerprint(dir.path());
    assert_match(&expected, dir.path());
    git(dir.path(), &["checkout", "-q", "--detach"]);
    let axes = axes_of(&compare(&expected, dir.path()).expect("compare"));
    assert!(
        axes.contains(&Axis::BranchState),
        "detaching HEAD must be rejected on branch state: {axes:?}"
    );
    assert!(
        !axes.contains(&Axis::Head),
        "the commit is the same one — HEAD must not be blamed: {axes:?}"
    );
}

#[test]
fn staged_state_mismatch_is_rejected() {
    let dir = seeded_repo();
    let expected = fingerprint(dir.path());
    assert_match(&expected, dir.path());
    write(dir.path(), "base.txt", "staged edit\n");
    git(dir.path(), &["add", "base.txt"]);
    let axes = axes_of(&compare(&expected, dir.path()).expect("compare"));
    assert!(
        axes.contains(&Axis::Staged),
        "a staged change must be rejected on the staged axis: {axes:?}"
    );
    assert!(
        axes.contains(&Axis::Index),
        "the index moved with it: {axes:?}"
    );
}

#[test]
fn dirty_tracked_content_mismatch_is_rejected() {
    let dir = seeded_repo();
    let expected = fingerprint(dir.path());
    assert_match(&expected, dir.path());
    write(dir.path(), "base.txt", "unstaged edit\n");
    let axes = axes_of(&compare(&expected, dir.path()).expect("compare"));
    assert!(
        axes.contains(&Axis::DirtyTracked),
        "an unstaged tracked edit must be rejected: {axes:?}"
    );
    assert!(
        !axes.contains(&Axis::Index),
        "nothing was staged — the index did not move: {axes:?}"
    );
}

#[test]
fn relevant_untracked_file_mismatch_is_rejected() {
    let dir = seeded_repo();
    let expected = fingerprint(dir.path());
    assert_match(&expected, dir.path());
    write(dir.path(), "novel.txt", "untracked\n");
    let axes = axes_of(&compare(&expected, dir.path()).expect("compare"));
    assert!(
        axes.contains(&Axis::Untracked),
        "a relevant untracked file must be rejected: {axes:?}"
    );
}

#[test]
fn deleted_file_mismatch_is_rejected() {
    let dir = seeded_repo();
    let expected = fingerprint(dir.path());
    assert_match(&expected, dir.path());
    std::fs::remove_file(dir.path().join("base.txt")).expect("delete");
    let axes = axes_of(&compare(&expected, dir.path()).expect("compare"));
    assert!(
        axes.contains(&Axis::Deleted),
        "a deleted tracked file must be rejected: {axes:?}"
    );
}

#[test]
fn rename_mismatch_is_rejected() {
    let dir = seeded_repo();
    let expected = fingerprint(dir.path());
    assert_match(&expected, dir.path());
    git(dir.path(), &["mv", "base.txt", "renamed.txt"]);
    let axes = axes_of(&compare(&expected, dir.path()).expect("compare"));
    assert!(
        axes.contains(&Axis::Renamed),
        "a staged rename must be rejected on the rename axis: {axes:?}"
    );
}

#[cfg(unix)]
#[test]
fn file_mode_mismatch_is_rejected() {
    use std::os::unix::fs::PermissionsExt;
    let dir = seeded_repo();
    let expected = fingerprint(dir.path());
    assert_match(&expected, dir.path());
    let path = dir.path().join("base.txt");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let axes = axes_of(&compare(&expected, dir.path()).expect("compare"));
    assert!(
        axes.contains(&Axis::Modes),
        "a bare executable-bit change must be rejected on the modes axis: {axes:?}"
    );
}

#[cfg(unix)]
#[test]
fn symlink_target_mismatch_is_rejected() {
    let dir = seeded_repo();
    write(dir.path(), "b.txt", "second\n");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "second"]);
    std::os::unix::fs::symlink("base.txt", dir.path().join("link")).expect("symlink");

    let expected = fingerprint(dir.path());
    assert!(
        expected
            .doc()
            .symlinks
            .iter()
            .any(|s| s.path == "link" && s.target == "base.txt"),
        "the expected side records the symlink target"
    );
    assert_match(&expected, dir.path());

    std::fs::remove_file(dir.path().join("link")).expect("unlink");
    std::os::unix::fs::symlink("b.txt", dir.path().join("link")).expect("relink");
    let axes = axes_of(&compare(&expected, dir.path()).expect("compare"));
    assert!(
        axes.contains(&Axis::Symlinks),
        "a retargeted symlink must be rejected on the symlink axis: {axes:?}"
    );
}

#[test]
fn submodule_state_mismatch_is_rejected() {
    let base = tempfile::tempdir().expect("tempdir");
    let sub = base.path().join("sub");
    std::fs::create_dir_all(&sub).expect("mkdir");
    seed_at(&sub);
    let host = base.path().join("host");
    std::fs::create_dir_all(&host).expect("mkdir");
    seed_at(&host);
    git(
        &host,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "../sub",
            "vendor/dep",
        ],
    );
    git(&host, &["commit", "-q", "-m", "vendored"]);

    let expected = fingerprint(&host);
    assert!(
        expected
            .doc()
            .submodules
            .iter()
            .any(|s| s.path == "vendor/dep"),
        "the expected side records the submodule"
    );
    assert_match(&expected, &host);

    // Advance the submodule's checked-out HEAD inside the host worktree.
    git(
        &host.join("vendor/dep"),
        &["commit", "-q", "--allow-empty", "-m", "drift"],
    );
    let axes = axes_of(&compare(&expected, &host).expect("compare"));
    assert!(
        axes.contains(&Axis::Submodules),
        "a drifted submodule must be rejected on the submodule axis: {axes:?}"
    );
    assert!(
        !axes.contains(&Axis::DirtyTracked),
        "submodule drift is not file dirt — content is never walked: {axes:?}"
    );
}

#[test]
fn merge_conflict_mismatch_is_rejected() {
    let dir = seeded_repo();
    git(dir.path(), &["checkout", "-q", "-b", "feature"]);
    write(dir.path(), "clash.txt", "feature side\n");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "feature"]);
    git(dir.path(), &["checkout", "-q", "main"]);

    let expected = fingerprint(dir.path());
    assert_match(&expected, dir.path());

    write(dir.path(), "clash.txt", "main side\n");
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "main side"]);
    // Both sides added clash.txt — the merge conflicts and leaves stages.
    git_allow_fail(dir.path(), &["merge", "-q", "feature"]);

    let comparison = compare(&expected, dir.path()).expect("compare");
    let axes = axes_of(&comparison);
    assert!(
        axes.contains(&Axis::Conflicts),
        "unmerged paths must be rejected on the conflicts axis: {axes:?}"
    );
}

#[test]
fn concurrent_mutation_is_incomplete_not_a_result() {
    let dir = seeded_repo();
    let root = dir.path().to_path_buf();

    // Directly at the observation layer: a REAL interleaved write between
    // the two porcelain reads, through the same seam the snapshotter pins —
    // no timing, no sleeps (§32).
    let moved = observe_with(&root, Scope::everything(), || {
        write(&root, "mid-capture.txt", "moved\n");
    })
    .expect("observe");
    let Observation::Incomplete { reason, .. } = moved else {
        panic!("a tree that moved mid-observation must be incomplete: {moved:?}");
    };
    assert!(
        reason.contains("changed during observation"),
        "the reason names the motion: {reason}"
    );

    // And at the comparison layer: the mutation surfaces as Incomplete,
    // never as Match or Mismatch.
    std::fs::remove_file(root.join("mid-capture.txt")).expect("tidy");
    let expected = fingerprint(&root);
    assert_match(&expected, &root);
    let out = compare_with(&expected, &root, || {
        write(&root, "mid-capture-2.txt", "moved again\n");
    })
    .expect("compare");
    assert!(
        matches!(out, Comparison::Incomplete { .. }),
        "a comparison over a moving tree must be incomplete: {out:?}"
    );
}

/// §12's sharpest sentence — "never claim a match from an incomplete
/// observation" — pinned where it is most tempting to fail it: the
/// mid-observation mutation CONVERGES the tree onto the expected state, so
/// the final tree genuinely equals the expectation, and the only honest
/// answer is still Incomplete, because the observation itself straddled two
/// states. The retry — a fresh, complete observation — then matches.
#[test]
fn an_incomplete_observation_never_matches_even_when_the_tree_converges() {
    let dir = seeded_repo();
    let root = dir.path().to_path_buf();
    let expected = fingerprint(&root); // clean tree

    // Dirty the tree, then let the mid-observation write revert it to the
    // exact expected bytes.
    write(&root, "base.txt", "storm\n");
    let out = compare_with(&expected, &root, || {
        write(&root, "base.txt", "base\n");
    })
    .expect("compare");
    assert_ne!(
        out,
        Comparison::Match,
        "the tree converged onto the expectation, but the observation was \
         incomplete — a match here would be §12's forbidden claim"
    );
    assert!(
        matches!(out, Comparison::Incomplete { .. }),
        "convergent mid-observation mutation is Incomplete: {out:?}"
    );

    // The §12 retry path: a fresh observation of the now-still tree matches.
    assert_eq!(
        compare(&expected, &root).expect("retry"),
        Comparison::Match,
        "after the tree settles, a complete re-observation matches"
    );
}

/* ── determinism, nuisance, scope, and the pinned real-repo vector ──── */

#[test]
fn same_state_matches_and_is_deterministic_across_independent_captures() {
    let dir = seeded_repo();
    let root = dir.path();
    // A tree with every kind of state: staged, dirty, untracked.
    write(root, "staged.txt", "staged\n");
    git(root, &["add", "staged.txt"]);
    write(root, "base.txt", "dirty\n");
    write(root, "loose.txt", "untracked\n");

    let one = fingerprint(root);
    let two = fingerprint(root);
    assert_eq!(
        one.identity(),
        two.identity(),
        "two independent captures of the same tree state must agree"
    );
    assert_eq!(compare(&one, root).expect("compare"), Comparison::Match);
}

#[test]
fn nuisance_changes_do_not_move_the_digest_and_material_ones_do() {
    let dir = seeded_repo();
    let root = dir.path();
    write(root, "base.txt", "dirty\n");
    let before = fingerprint(root);

    // Nuisance: rewrite the same bytes — mtime moves, content does not.
    write(root, "base.txt", "dirty\n");
    assert_eq!(
        fingerprint(root).identity(),
        before.identity(),
        "an mtime-only touch is nuisance, not identity"
    );

    // Nuisance: reordered scope exclusions normalize to one digest.
    let scoped = |exclusions: Vec<String>| {
        let scope = Scope {
            include_untracked: true,
            exclusions,
        };
        match observe(root, scope).expect("observe") {
            Observation::Complete(fp) => fp.identity().to_string(),
            other => panic!("complete expected: {other:?}"),
        }
    };
    assert_eq!(
        scoped(vec!["b/".into(), "a/".into(), "a/".into()]),
        scoped(vec!["a/".into(), "b/".into()]),
        "exclusion order and duplication are nuisance"
    );

    // Material: one changed byte moves the digest.
    write(root, "base.txt", "dirty!\n");
    assert_ne!(
        fingerprint(root).identity(),
        before.identity(),
        "a content change is identity"
    );
}

#[test]
fn declared_exclusions_narrow_untracked_relevance_and_are_hash_covered() {
    let dir = seeded_repo();
    let root = dir.path();
    write(root, "scratch/tmp.txt", "temp\n");

    let scope = Scope {
        include_untracked: true,
        exclusions: vec!["scratch/".to_string()],
    };
    let expected = match observe(root, scope).expect("observe") {
        Observation::Complete(fp) => *fp,
        other => panic!("complete expected: {other:?}"),
    };
    assert!(
        expected.doc().untracked.is_empty(),
        "the excluded untracked file is outside the declared scope"
    );
    assert_match(&expected, root);

    // More churn inside the exclusion stays outside the identity — the
    // frozen scope drives the comparison, not a fresh judgment call.
    write(root, "scratch/more.txt", "more temp\n");
    assert_eq!(
        compare(&expected, root).expect("compare"),
        Comparison::Match
    );

    // A file inside scope still mismatches.
    write(root, "seen.txt", "relevant\n");
    let axes = axes_of(&compare(&expected, root).expect("compare"));
    assert!(axes.contains(&Axis::Untracked), "{axes:?}");
    std::fs::remove_file(root.join("seen.txt")).expect("tidy");

    // And the scope itself is hash-covered: the same tree under a different
    // declared scope is a different identity, so scope cannot be swapped
    // without minting a visibly different expectation.
    let everything = fingerprint(root);
    assert_ne!(
        everything.identity(),
        expected.identity(),
        "scope is part of the digest, not a comparison-time parameter"
    );
}

/// The pinned real-repository vector: git blob ids are deterministic from
/// content, so the whole index digest is hand-computable outside Rust.
/// python3 (2026-08-31): blob("alpha\n") = 4a58007052a65fbc2fc3f910f2855f45a4058e74,
/// blob("#!/bin/sh\n") = 1a2485251c33a70432394c93fb89330ef214bfc9, and the
/// documented lp-encoding over the two `ls-files -s` entries digests to the
/// value below.
#[cfg(unix)]
#[test]
fn the_index_digest_matches_the_hand_computed_real_repo_vector() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    git(root, &["init", "-q", "-b", "main"]);
    write(root, "a.txt", "alpha\n");
    write(root, "bin/run", "#!/bin/sh\n");
    std::fs::set_permissions(root.join("bin/run"), std::fs::Permissions::from_mode(0o755))
        .expect("chmod");
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "pinned"]);

    let fp = fingerprint(root);
    assert_eq!(
        fp.doc().index_digest,
        "dcc67ad2703bc4cbc3eebac62530eb974d5a89aa1057d00415bb7e6c5b36ee4d",
        "the index digest over a real repository matches the python3 vector"
    );

    // The untracked content digest is equally checkable by anyone.
    write(root, "notes.md", "hello\n");
    let fp = fingerprint(root);
    let entry = fp
        .doc()
        .untracked
        .iter()
        .find(|u| u.path == "notes.md")
        .expect("recorded");
    assert_eq!(
        entry.sha256,
        "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03"
    );
}

/* ── read-only proof and the foreground writer ──────────────────────── */

#[test]
fn observation_leaves_the_workspace_byte_identical() {
    let dir = seeded_repo();
    let root = dir.path();
    write(root, "base.txt", "dirty\n");
    write(root, "loose.txt", "untracked\n");
    git(root, &["add", "loose.txt"]);

    let index_before = std::fs::read(root.join(".git/index")).expect("index");
    let porcelain_before = git(root, &["status", "--porcelain"]);
    let head_before = git(root, &["rev-parse", "HEAD"]);

    let _ = fingerprint(root);

    assert_eq!(
        std::fs::read(root.join(".git/index")).expect("index"),
        index_before,
        "the observation rewrote the index — it must not touch the repo"
    );
    assert_eq!(git(root, &["status", "--porcelain"]), porcelain_before);
    assert_eq!(git(root, &["rev-parse", "HEAD"]), head_before);
}

#[test]
fn a_complete_observation_records_with_receipt_and_identity() {
    let dir = seeded_repo();
    let root = dir.path();
    write(root, "loose.txt", "untracked\n");

    let obs = observe(root, Scope::everything()).expect("observe");
    let Observation::Complete(ref fp) = obs else {
        panic!("complete expected");
    };

    let ledger = Ledger::open_in_memory().expect("ledger");
    let out = record_observation(
        &ledger,
        &obs,
        "2026-08-31T18:00:00Z",
        "cli:oracle:2026-08-31T18:00:00Z",
        Some("snap-row-1"),
    )
    .expect("record");
    let RecordOutcome::Recorded {
        outcome,
        workspace_identity,
    } = out
    else {
        panic!("a git repository with commits has a repository identity");
    };
    assert_eq!(workspace_identity.as_deref(), Some(fp.identity()));

    let rec = outcome.record();
    assert_eq!(rec.observation_type_raw, "WORKSPACE_OBSERVED");
    assert_eq!(rec.completeness_raw, "complete");
    assert_eq!(rec.source_adapter, "thruline");
    assert_eq!(
        Some(rec.repository_identity.as_str()),
        fp.doc().repo_id.as_deref(),
        "the envelope's repository identity is the workspace layer's, reused"
    );
    assert_eq!(rec.workspace_identity.as_deref(), Some(fp.identity()));
    assert!(
        rec.workspace_identity
            .as_deref()
            .is_some_and(|w| w.starts_with("wsid1:")),
        "the identity string is version-tagged"
    );

    let receipt = rec.receipt().expect("receipt parses");
    match &receipt.terminal {
        thruline_core::observation::ReceiptTerminal::Filesystem { path, sha256 } => {
            assert_eq!(path, &fp.doc().root, "the receipt terminates at the root");
            assert_eq!(
                sha256.as_deref(),
                Some(fp.digest_hex()),
                "and carries the canonical digest of what was observed there"
            );
        }
        other => panic!("the terminal must be filesystem state: {other:?}"),
    }
    assert_eq!(receipt.derived_refs.len(), 1);
    assert_eq!(receipt.derived_refs[0].table, "workspace_snapshots");
    assert_eq!(receipt.derived_refs[0].id, "snap-row-1");

    assert_eq!(verify_chain(&ledger).expect("verify"), 1);
}

/// A worktree-relative sanity pin: observing from a subdirectory of the
/// repository yields the same identity as observing from the root — the
/// canonical root, not the caller's cwd, is the identity.
#[test]
fn observation_is_rooted_at_the_repository_not_the_caller() {
    let dir = seeded_repo();
    let root = dir.path();
    write(root, "nested/inner.txt", "nested\n");
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "nested"]);
    write(root, "nested/dirty.txt", "untracked\n");

    let from_root = fingerprint(root);
    let from_subdir = fingerprint(&root.join("nested"));
    assert_eq!(
        from_root.identity(),
        from_subdir.identity(),
        "the identity belongs to the workspace, not the entry point"
    );
}
