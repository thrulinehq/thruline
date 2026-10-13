//! A208 — the §34 snapshot gate, on real repositories (MANDATE-v4 §14,
//! §32, §33 snapshot family).
//!
//! Two independent oracles decide byte preservation, and both must agree at
//! 100%:
//!
//! 1. **Stage 3's comparator.** Capture a genuinely dirty tree, destroy it,
//!    restore into a safe directory, move the restored tree into the
//!    original path, and observe under the same frozen scope — the WSID_V1
//!    identity strings must be EQUAL. The expected digest comes from an
//!    observation taken before the store ever ran.
//! 2. **A test-owned file walk.** A recursive map of every path (`.git`
//!    included) to kind, mode, and content digest, recorded before the tree
//!    was destroyed and compared after restore — built here, not in the
//!    module under test.
//!
//! Read-only capture is proven the same way: the walk before capture equals
//! the walk after, `.git/index` bytes included. Interruption is a REAL
//! injected failure through the store's seam, never a sleep (§32).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use thruline_core::chain::sha256_hex;
use thruline_core::error::CoreError;
use thruline_core::ledger::Ledger;
use thruline_core::observation::{verify_chain, ReceiptTerminal};
use thruline_core::protected_store::{
    apply_over_working_tree, capture, capture_with, plan_over_working_tree,
    record_snapshot_created, record_snapshot_verified, restore_to_new, snapshot_applies,
    verify_snapshot, CaptureOutcome, ProtectedStore, SnapshotRecordOutcome,
};
use thruline_core::workspace_identity::{observe, Observation, Scope};

fn git(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .env("GIT_AUTHOR_NAME", "oracle")
        .env("GIT_AUTHOR_EMAIL", "oracle@test")
        .env("GIT_COMMITTER_NAME", "oracle")
        .env("GIT_COMMITTER_EMAIL", "oracle@test")
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?} in {cwd:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn write(root: &Path, rel: &str, content: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("mkdir");
    }
    fs::write(path, content).expect("write");
}

/// A canonicalized repo root inside `base`, seeded with one commit.
fn seeded_repo(base: &Path) -> PathBuf {
    let root = base.join("repo");
    fs::create_dir_all(&root).expect("mkdir");
    let root = fs::canonicalize(&root).expect("canonical");
    git(&root, &["init", "-q", "-b", "main"]);
    write(&root, "base.txt", "base\n");
    write(&root, "keep.txt", "kept\n");
    write(&root, "old.txt", "renameable\n");
    write(&root, "victim.txt", "will be deleted\n");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "seed"]);
    root
}

/// Every §14 axis, present at once: staged (with a differing worktree
/// version — the duality), unstaged, untracked, a bare mode change, a
/// symlink, a deletion, and a staged rename.
fn dirty_everything(root: &Path) {
    write(root, "staged.txt", "index version\n");
    git(root, &["add", "staged.txt"]);
    write(root, "staged.txt", "worktree version\n"); // duality: both survive
    write(root, "base.txt", "unstaged edit\n");
    write(root, "loose.txt", "untracked\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.join("keep.txt"), fs::Permissions::from_mode(0o755))
            .expect("chmod");
        std::os::unix::fs::symlink("base.txt", root.join("link")).expect("symlink");
    }
    fs::remove_file(root.join("victim.txt")).expect("delete");
    git(root, &["mv", "old.txt", "new.txt"]);
}

fn fingerprint_identity(root: &Path) -> String {
    match observe(root, Scope::everything()).expect("observe") {
        Observation::Complete(fp) => fp.identity().to_string(),
        Observation::Incomplete { reason, .. } => panic!("incomplete observation: {reason}"),
    }
}

/// The test-owned second oracle: path → (kind, mode, content digest or
/// symlink target), recursive, `.git` included. Independent of the module
/// under test by construction.
fn walk_map(root: &Path) -> BTreeMap<String, (char, u32, String)> {
    use std::os::unix::fs::PermissionsExt;
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read_dir") {
            let entry = entry.expect("entry");
            let path = entry.path();
            let rel = path
                .strip_prefix(root)
                .expect("contained")
                .to_string_lossy()
                .into_owned();
            let meta = fs::symlink_metadata(&path).expect("lstat");
            if meta.file_type().is_symlink() {
                let target = fs::read_link(&path).expect("read_link");
                out.insert(rel, ('l', 0, target.to_string_lossy().into_owned()));
            } else if meta.file_type().is_dir() {
                stack.push(path);
            } else {
                let mode = meta.permissions().mode() & 0o7777;
                let sha = sha256_hex(&fs::read(&path).expect("read"));
                out.insert(rel, ('f', mode, sha));
            }
        }
    }
    out
}

fn capture_complete(store: &ProtectedStore, root: &Path) -> CaptureOutcome {
    let outcome = capture(store, root, Scope::everything()).expect("capture");
    assert!(
        outcome.manifest.is_complete(),
        "expected a complete capture: {:?}",
        outcome.manifest.incomplete_reason
    );
    outcome
}

/* ── read-only creation ─────────────────────────────────────────────── */

#[cfg(unix)]
#[test]
fn capture_leaves_the_source_tree_byte_identical_including_git() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = seeded_repo(tmp.path());
    dirty_everything(&root);
    // Settle any pending index stat-cache refresh so the porcelain pin below
    // compares like with like.
    git(&root, &["status", "--porcelain"]);

    let before = walk_map(&root);
    let index_before = fs::read(root.join(".git/index")).expect("index");
    let porcelain_before = git(&root, &["status", "--porcelain"]);

    let store = ProtectedStore::open(tmp.path().join("protected")).expect("store");
    let outcome = capture_complete(&store, &root);
    assert!(
        outcome.manifest.worktree.len() >= 6,
        "the walk saw the tree"
    );

    assert_eq!(
        walk_map(&root),
        before,
        "capture must leave every byte of the source tree — .git included — untouched"
    );
    assert_eq!(
        fs::read(root.join(".git/index")).expect("index"),
        index_before,
        "the index bytes are untouched"
    );
    assert_eq!(git(&root, &["status", "--porcelain"]), porcelain_before);
}

/* ── THE §34 gate: the byte-preservation round trip ─────────────────── */

#[cfg(unix)]
#[test]
fn the_restore_round_trip_reproduces_the_exact_workspace_identity() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = seeded_repo(tmp.path());
    dirty_everything(&root);
    git(&root, &["status", "--porcelain"]); // settle the index stat cache

    let source_identity = fingerprint_identity(&root);
    let store = ProtectedStore::open(tmp.path().join("protected")).expect("store");
    let outcome = capture_complete(&store, &root);
    assert_eq!(
        outcome.manifest.source_identity, source_identity,
        "the manifest binds to the observed identity"
    );
    let walk_before = walk_map(&root);

    // Simulate total loss of the workspace.
    fs::remove_dir_all(&root).expect("destroy the workspace");

    // Restore into a separate safe directory (the §14 default), then move
    // the restored tree into the original path — the operator's own act.
    let safe = tmp.path().join("restored");
    let report = restore_to_new(&store, &outcome.manifest, &safe).expect("restore");
    assert!(report.git_files > 0, "repository metadata was materialized");
    fs::rename(&safe, &root).expect("move the restored tree into place");

    // Oracle 1 — Stage 3's comparator: the identity strings must be EQUAL.
    assert_eq!(
        fingerprint_identity(&root),
        source_identity,
        "§34: snapshot byte preservation — the restored tree IS the captured workspace \
         identity, staged/unstaged/untracked/modes/symlinks/deletions/renames included"
    );

    // Oracle 2 — the test-owned file walk must agree byte for byte.
    assert_eq!(
        walk_map(&root),
        walk_before,
        "§34: the file walk agrees — every path, kind, mode, and content digest"
    );

    // The duality, explicitly: the index blob and the worktree blob both
    // survived, and the restored repo shows exactly the staged difference.
    let duality = outcome
        .manifest
        .staged
        .iter()
        .find(|s| s.path == "staged.txt")
        .expect("staged duality recorded");
    assert_eq!(
        store.get(&duality.index_blob_sha256).expect("index blob"),
        b"index version\n",
        "the index version survived as its own blob"
    );
    assert_eq!(
        fs::read(root.join("staged.txt")).expect("worktree"),
        b"worktree version\n",
        "the worktree version is what the file holds"
    );
    // Deletion preserved as absence; the restored index still names it.
    assert!(!root.join("victim.txt").exists());
    assert!(git(&root, &["status", "--porcelain"]).contains("victim.txt"));
}

/* ── idempotency ────────────────────────────────────────────────────── */

#[test]
fn re_protecting_an_unchanged_tree_is_idempotent() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = seeded_repo(tmp.path());
    write(&root, "base.txt", "dirty\n");
    git(&root, &["status", "--porcelain"]); // settle the index stat cache

    let store = ProtectedStore::open(tmp.path().join("protected")).expect("store");
    let first = capture_complete(&store, &root);
    let count_after_first = store.blob_count().expect("count");
    let second = capture_complete(&store, &root);

    assert_eq!(
        first.manifest_digest, second.manifest_digest,
        "an unchanged tree re-protects to the identical manifest digest"
    );
    assert_eq!(second.new_blobs, 0, "zero new blobs on re-protect");
    assert_eq!(
        store.blob_count().expect("count"),
        count_after_first,
        "the store did not grow"
    );
}

/* ── interrupted capture ────────────────────────────────────────────── */

#[test]
fn an_interrupted_capture_leaves_a_consistent_store_and_a_rerun_completes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = seeded_repo(tmp.path());
    write(&root, "loose.txt", "untracked\n");

    let store = ProtectedStore::open(tmp.path().join("protected")).expect("store");
    // A REAL injected failure mid-store (§32: no sleeps as proof): the seam
    // fires after each newly written blob; the third one dies.
    let err = capture_with(&store, &root, Scope::everything(), &mut |n| {
        if n >= 3 {
            Err(CoreError::Integrity(
                "injected mid-store failure".to_string(),
            ))
        } else {
            Ok(())
        }
    })
    .expect_err("the injected failure must surface");
    assert!(err.to_string().contains("injected"), "{err}");

    // The store remains consistent: every blob that landed is whole and
    // valid; the partial capture published no manifest and no authority.
    let partial = store.blob_count().expect("count");
    assert!(partial >= 3, "some blobs landed before the failure");
    assert_eq!(
        store.verify_all().expect("verify"),
        partial,
        "every landed blob verifies — atomic per blob, no torn writes"
    );

    // A rerun completes, deduplicating what already landed.
    let outcome = capture_complete(&store, &root);
    assert!(outcome.manifest.is_complete());
    assert_eq!(
        store.verify_all().expect("verify"),
        store.blob_count().expect("count")
    );
}

/* ── unborn repository ──────────────────────────────────────────────── */

#[cfg(unix)]
#[test]
fn an_unborn_repository_captures_and_restores_without_any_commit() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("repo");
    fs::create_dir_all(&root).expect("mkdir");
    let root = fs::canonicalize(&root).expect("canonical");
    git(&root, &["init", "-q", "-b", "main"]);
    write(&root, "staged.txt", "staged in an unborn repo\n");
    git(&root, &["add", "staged.txt"]);
    write(&root, "loose.txt", "untracked\n");

    let source_identity = fingerprint_identity(&root);
    let store = ProtectedStore::open(tmp.path().join("protected")).expect("store");
    let outcome = capture_complete(&store, &root);
    let walk_before = walk_map(&root);

    fs::remove_dir_all(&root).expect("destroy");
    let safe = tmp.path().join("restored");
    restore_to_new(&store, &outcome.manifest, &safe).expect("restore");
    fs::rename(&safe, &root).expect("move into place");

    assert_eq!(
        fingerprint_identity(&root),
        source_identity,
        "§14: recovery never depends on git commits existing"
    );
    assert_eq!(walk_map(&root), walk_before);
    assert!(git(&root, &["status", "--porcelain"]).contains("staged.txt"));
}

/* ── honest incompleteness ──────────────────────────────────────────── */

#[test]
fn a_mid_capture_workspace_write_yields_an_honest_incomplete_that_restore_refuses() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = seeded_repo(tmp.path());
    write(&root, "loose.txt", "untracked\n");

    let store = ProtectedStore::open(tmp.path().join("protected")).expect("store");
    let root_for_seam = root.clone();
    let mut fired = false;
    let outcome = capture_with(&store, &root, Scope::everything(), &mut |_| {
        if !fired {
            fired = true;
            write(&root_for_seam, "mid-capture.txt", "moved\n");
        }
        Ok(())
    })
    .expect("capture returns, honestly marked");

    assert!(
        !outcome.manifest.is_complete(),
        "the capture says incomplete"
    );
    assert!(
        outcome
            .manifest
            .incomplete_reason
            .as_deref()
            .is_some_and(|r| r.contains("changed during capture")),
        "the reason names the motion: {:?}",
        outcome.manifest.incomplete_reason
    );
    let err = restore_to_new(&store, &outcome.manifest, &tmp.path().join("t"))
        .expect_err("an incomplete capture can never satisfy a byte-preservation claim");
    assert!(err.to_string().contains("incomplete"), "{err}");
    fs::remove_file(root.join("mid-capture.txt")).expect("tidy");
}

/// The second pass's own territory: a write INSIDE `.git` mid-capture is
/// porcelain-invisible — WSID_V1's sentinel cannot see it — and the
/// re-hash pass is what catches it.
#[test]
fn a_git_internal_write_mid_capture_is_caught_by_the_second_pass() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = seeded_repo(tmp.path());
    write(&root, "loose.txt", "untracked\n");
    // Sorts before every stock .git file, so its blob lands early and the
    // seam keeps rewriting it afterwards.
    write(&root, ".git/aaa-drift-marker", "drift-0\n");

    let store = ProtectedStore::open(tmp.path().join("protected")).expect("store");
    let root_for_seam = root.clone();
    let outcome = capture_with(&store, &root, Scope::everything(), &mut |n| {
        write(
            &root_for_seam,
            ".git/aaa-drift-marker",
            &format!("drift-{n}\n"),
        );
        Ok(())
    })
    .expect("capture returns, honestly marked");

    assert!(
        !outcome.manifest.is_complete(),
        "a porcelain-invisible .git write must still be caught"
    );
    assert!(
        outcome
            .manifest
            .incomplete_reason
            .as_deref()
            .is_some_and(|r| r.contains("aaa-drift-marker") || r.contains("capture again")),
        "the reason points at the motion: {:?}",
        outcome.manifest.incomplete_reason
    );
}

/* ── verify and applicability ───────────────────────────────────────── */

#[cfg(unix)]
#[test]
fn verify_rehashes_every_referenced_blob_and_tamper_fails_it_by_name() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = seeded_repo(tmp.path());
    write(&root, "base.txt", "dirty\n");

    let store = ProtectedStore::open(tmp.path().join("protected")).expect("store");
    let outcome = capture_complete(&store, &root);
    let pref = outcome.protected_ref(None);
    let checked = verify_snapshot(&store, &pref).expect("verify");
    assert!(checked > 3, "manifest + content blobs re-hashed: {checked}");

    // Flip one byte of one referenced blob; verification must name it.
    let victim = outcome.manifest.worktree[0]
        .sha256
        .clone()
        .expect("a file entry");
    let path = store.root().join("blobs").join(&victim[..2]).join(&victim);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("unlock");
    let mut bytes = fs::read(&path).expect("read");
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    fs::write(&path, &bytes).expect("tamper");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).expect("relock");

    let err = verify_snapshot(&store, &pref).expect_err("tamper must fail verification");
    assert!(err.to_string().contains(&victim), "{err}");
}

#[test]
fn snapshot_applicability_is_identity_string_equality_and_nothing_else() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = seeded_repo(tmp.path());
    let store = ProtectedStore::open(tmp.path().join("protected")).expect("store");
    let outcome = capture_complete(&store, &root);
    assert_eq!(
        snapshot_applies(&outcome.manifest, &root).expect("applies"),
        Some(true),
        "an untouched tree still is the snapshot"
    );
    write(&root, "novel.txt", "changed\n");
    assert_eq!(
        snapshot_applies(&outcome.manifest, &root).expect("applies"),
        Some(false),
        "any identity change is relevant — the Stage 4 conservative rule"
    );
}

/* ── the over-working-tree edge, at the core layer ──────────────────── */

#[test]
fn over_work_apply_requires_the_exact_acknowledged_state_and_restores_it() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = seeded_repo(tmp.path());
    write(&root, ".gitignore", "ignored-dir/\n");
    git(&root, &["add", ".gitignore"]);
    git(&root, &["commit", "-q", "-m", "ignore"]);
    write(&root, "base.txt", "the protected state\n");

    let store = ProtectedStore::open(tmp.path().join("protected")).expect("store");
    let outcome = capture_complete(&store, &root);

    // The tree then drifts: an edit, a new untracked file, and an ignored
    // file that is OUTSIDE the snapshot's declared scope.
    write(&root, "base.txt", "drifted\n");
    write(&root, "junk.txt", "should be deleted by restore\n");
    write(
        &root,
        "ignored-dir/survivor.txt",
        "outside scope — untouched\n",
    );

    let plan = plan_over_working_tree(&outcome.manifest, &root).expect("plan");
    assert!(
        plan.writes.iter().any(|w| w.path == "base.txt"),
        "the drifted file is a named write: {:?}",
        plan.writes
    );
    assert!(
        plan.deletes.contains(&"junk.txt".to_string()),
        "the new untracked file is a named delete: {:?}",
        plan.deletes
    );
    assert!(
        !plan
            .writes
            .iter()
            .any(|w| w.path.starts_with("ignored-dir/"))
            && !plan.deletes.iter().any(|d| d.starts_with("ignored-dir/")),
        "out-of-scope files are named by neither list"
    );

    // Compare-first at the touch moment: if the tree moves after planning,
    // the acknowledged state no longer exists and apply refuses.
    write(&root, "late-drift.txt", "moved after the preview\n");
    let err = apply_over_working_tree(&store, &outcome.manifest, &root, &plan)
        .expect_err("a stale acknowledgment must refuse");
    assert!(
        err.to_string().contains("changed after the preview"),
        "{err}"
    );
    fs::remove_file(root.join("late-drift.txt")).expect("tidy");

    // Fresh plan, fresh acknowledgment: apply restores the exact identity.
    let plan = plan_over_working_tree(&outcome.manifest, &root).expect("re-plan");
    apply_over_working_tree(&store, &outcome.manifest, &root, &plan).expect("apply");
    assert_eq!(
        fingerprint_identity(&root),
        outcome.manifest.source_identity,
        "the working tree returned to the protected identity"
    );
    assert!(!root.join("junk.txt").exists(), "the named delete happened");
    assert_eq!(
        fs::read_to_string(root.join("ignored-dir/survivor.txt")).expect("survivor"),
        "outside scope — untouched\n",
        "nothing beyond the preview was touched"
    );
}

/* ── the §11 observation writers ────────────────────────────────────── */

#[test]
fn snapshot_created_and_verified_observations_carry_the_manifest_receipt() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = seeded_repo(tmp.path());
    write(&root, "base.txt", "dirty\n");
    let store = ProtectedStore::open(tmp.path().join("protected")).expect("store");
    let outcome = capture_complete(&store, &root);
    let pref = outcome.protected_ref(None);
    assert!(
        pref.repository_identity.is_some(),
        "a committed repo has one"
    );

    let ledger = Ledger::open_in_memory().expect("ledger");
    let created = record_snapshot_created(
        &ledger,
        "snap-row-1",
        &pref,
        Some(&outcome.manifest.source_identity),
        "2026-09-01T10:00:00Z",
        "cli:protect:2026-09-01T10:00:00Z",
    )
    .expect("record created");
    let SnapshotRecordOutcome::Recorded(created) = created else {
        panic!("a repo with commits records");
    };
    let rec = created.record();
    assert_eq!(rec.observation_type_raw, "SNAPSHOT_CREATED");
    assert_eq!(rec.completeness_raw, "complete");
    assert_eq!(
        rec.workspace_identity.as_deref(),
        Some(outcome.manifest.source_identity.as_str())
    );
    let receipt = rec.receipt().expect("receipt");
    match &receipt.terminal {
        ReceiptTerminal::SnapshotManifest { manifest_sha256 } => {
            assert_eq!(manifest_sha256, &pref.manifest_digest);
        }
        other => panic!("the terminal must be the snapshot manifest: {other:?}"),
    }
    assert_eq!(receipt.derived_refs[0].table, "workspace_snapshots");
    assert_eq!(receipt.derived_refs[0].id, "snap-row-1");

    let checked = verify_snapshot(&store, &pref).expect("verify");
    let verified = record_snapshot_verified(
        &ledger,
        "snap-row-1",
        &pref,
        checked,
        "2026-09-01T10:05:00Z",
        "cli:snapshot-verify:2026-09-01T10:05:00Z",
    )
    .expect("record verified");
    let SnapshotRecordOutcome::Recorded(verified) = verified else {
        panic!("recorded");
    };
    assert_eq!(verified.record().observation_type_raw, "SNAPSHOT_VERIFIED");
    assert_eq!(verify_chain(&ledger).expect("chain"), 2);

    // An unborn repo has no repository identity: the writer abstains loudly
    // instead of recording against a guess (§7: unknown stays unknown).
    let mut no_repo = pref.clone();
    no_repo.repository_identity = None;
    let out = record_snapshot_created(
        &ledger,
        "snap-row-2",
        &no_repo,
        None,
        "2026-09-01T10:10:00Z",
        "cli:protect:2026-09-01T10:10:00Z",
    )
    .expect("call succeeds");
    assert!(
        matches!(out, SnapshotRecordOutcome::NoRepositoryIdentity { .. }),
        "no placeholder identities, ever"
    );
    assert_eq!(verify_chain(&ledger).expect("chain"), 2, "nothing appended");
}
