//! A208 — the protected snapshot store (MANDATE-v4 §14, §32, §33).
//!
//! The TLSNAP_V1 canonical layout is pinned by python3 hashlib vectors
//! computed before the Rust encoder existed (two independent assemblies
//! agreeing — the a205–a207 discipline); the store's discipline — content
//! addressing, encryption at rest, restrictive permissions, tamper and
//! corruption refusals, repository-destination rejection, path containment
//! on restore — is proven here against real files.

use std::fs;
use std::path::{Path, PathBuf};

use thruline_core::chain::sha256_hex;
use thruline_core::protected_store::{
    canonical_bytes, manifest_digest, restore_to_new, EntryKind, ManifestEntry, ProtectedStore,
    RenamedPair, SnapshotManifest, StagedDuality,
};

fn store_at(base: &Path) -> ProtectedStore {
    ProtectedStore::open(base.join("protected")).expect("open store")
}

fn empty_manifest(identity: &str, root: &str) -> SnapshotManifest {
    SnapshotManifest {
        source_identity: identity.to_string(),
        include_untracked: true,
        exclusions: Vec::new(),
        root: root.to_string(),
        completeness: "complete".to_string(),
        incomplete_reason: None,
        worktree: Vec::new(),
        git: Vec::new(),
        staged: Vec::new(),
        deleted: Vec::new(),
        renamed: Vec::new(),
    }
}

/* ── the pinned python3 vectors ─────────────────────────────────────── */

/// python3 hashlib (2026-09-01), two independent assemblies agreeing,
/// computed BEFORE this encoder existed. Changing any byte of the layout is
/// a migration to TLSNAP_V2, and this test is what makes that loud.
#[test]
fn the_canonical_layout_matches_the_python3_vectors() {
    // V1 — minimal complete manifest, everything empty.
    let v1 = empty_manifest(&format!("wsid1:{}", "a".repeat(64)), "/tmp/vector-repo");
    let bytes = canonical_bytes(&v1);
    assert_eq!(bytes.len(), 333, "V1 byte length");
    assert_eq!(
        manifest_digest(&v1),
        "fdcfe9ac8c5758a3b61e1c8ae976e3a19b9882a7b1ea84146ff5e1cc99e4625e",
        "V1 digest"
    );

    // V2 — every axis populated. Exclusions arrive unsorted on purpose: the
    // encoder's normalization is part of the pinned layout.
    let mut v2 = empty_manifest(&format!("wsid1:{}", "b".repeat(64)), "/w");
    v2.exclusions = vec!["b/".to_string(), "a/".to_string()];
    v2.worktree = vec![
        ManifestEntry {
            path: "bin/run".to_string(),
            kind: EntryKind::File,
            mode: 0o755,
            bytes: 10,
            sha256: Some("c".repeat(64)),
            target: None,
        },
        ManifestEntry {
            path: "link".to_string(),
            kind: EntryKind::Symlink,
            mode: 0,
            bytes: 0,
            sha256: None,
            target: Some("bin/run".to_string()),
        },
    ];
    v2.git = vec![ManifestEntry {
        path: "HEAD".to_string(),
        kind: EntryKind::File,
        mode: 0o644,
        bytes: 23,
        sha256: Some("d".repeat(64)),
        target: None,
    }];
    v2.staged = vec![StagedDuality {
        path: "bin/run".to_string(),
        index_object: "e".repeat(40),
        index_blob_sha256: "f".repeat(64),
        worktree_sha256: Some("c".repeat(64)),
    }];
    v2.deleted = vec!["gone.txt".to_string()];
    v2.renamed = vec![RenamedPair {
        new_path: "new.txt".to_string(),
        old_path: "old.txt".to_string(),
    }];
    let bytes = canonical_bytes(&v2);
    assert_eq!(bytes.len(), 872, "V2 byte length");
    assert_eq!(
        manifest_digest(&v2),
        "d64f58a828900c9c4b3d78ba248d2ed76a4352ac41a55c378ceda298b042def3",
        "V2 digest"
    );

    // V3 — an incomplete capture, untracked outside scope.
    let mut v3 = empty_manifest(&format!("wsid1:{}", "1".repeat(64)), "/x");
    v3.include_untracked = false;
    v3.completeness = "incomplete".to_string();
    v3.incomplete_reason = Some("the workspace changed during capture".to_string());
    v3.worktree = vec![ManifestEntry {
        path: "f.txt".to_string(),
        kind: EntryKind::File,
        mode: 0o644,
        bytes: 2,
        sha256: Some("2".repeat(64)),
        target: None,
    }];
    let bytes = canonical_bytes(&v3);
    assert_eq!(bytes.len(), 476, "V3 byte length");
    assert_eq!(
        manifest_digest(&v3),
        "271ad039457507bd365329c3bdf89ba998b60c13d3cafbc93c91fa15eda15598",
        "V3 digest"
    );
}

/* ── store discipline ───────────────────────────────────────────────── */

#[test]
fn blobs_are_deduplicated_content_addressed_and_verified_on_read() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = store_at(tmp.path());
    let content = b"the same bytes, twice";
    let (first, written_first) = store.put_bytes(content).expect("put");
    assert!(written_first, "the first put writes");
    assert_eq!(first.sha256, sha256_hex(content), "keyed by plaintext sha");
    let (second, written_second) = store.put_bytes(content).expect("put again");
    assert!(!written_second, "identical content is one blob");
    assert_eq!(first, second);
    assert_eq!(store.blob_count().expect("count"), 1);
    assert_eq!(store.get(&first.sha256).expect("get"), content);
    assert_eq!(store.verify_all().expect("verify"), 1);
}

#[cfg(unix)]
#[test]
fn store_permissions_are_restrictive_and_asserted() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = store_at(tmp.path());
    let (blob, _) = store.put_bytes(b"private bytes").expect("put");
    let mode = |p: &Path| fs::metadata(p).expect("meta").permissions().mode() & 0o7777;
    assert_eq!(mode(store.root()), 0o700, "store root");
    assert_eq!(mode(&store.root().join("blobs")), 0o700, "blobs dir");
    assert_eq!(mode(&store.root().join("staging")), 0o700, "staging dir");
    assert_eq!(mode(&store.root().join("store.key")), 0o600, "key file");
    let blob_path = store
        .root()
        .join("blobs")
        .join(&blob.sha256[..2])
        .join(&blob.sha256);
    assert_eq!(mode(&blob_path), 0o400, "sealed blob");
}

#[cfg(unix)]
#[test]
fn a_flipped_byte_is_detected_and_named() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = store_at(tmp.path());
    let (blob, _) = store.put_bytes(b"seal me").expect("put");
    let path = store
        .root()
        .join("blobs")
        .join(&blob.sha256[..2])
        .join(&blob.sha256);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("unlock");
    let mut bytes = fs::read(&path).expect("read sealed");
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    fs::write(&path, &bytes).expect("tamper");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).expect("relock");

    let err = store.get(&blob.sha256).expect_err("tamper must fail");
    assert!(
        err.to_string().contains(&blob.sha256),
        "the failure names the blob: {err}"
    );
    assert!(store.verify_all().is_err(), "verify_all fails too");
}

#[cfg(unix)]
#[test]
fn a_truncated_blob_refuses_to_load_and_heals_on_the_next_put() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = store_at(tmp.path());
    let content = b"will be truncated on disk";
    let (blob, _) = store.put_bytes(content).expect("put");
    let path = store
        .root()
        .join("blobs")
        .join(&blob.sha256[..2])
        .join(&blob.sha256);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("unlock");
    fs::write(&path, b"TL").expect("truncate");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).expect("relock");

    let err = store.get(&blob.sha256).expect_err("truncated must refuse");
    assert!(
        err.to_string().contains("truncated") || err.to_string().contains("decryption"),
        "the refusal says why: {err}"
    );
    // The capture path is unaffected: the same content heals in place.
    let (healed, written) = store.put_bytes(content).expect("heal");
    assert!(written, "the corrupt blob was rewritten");
    assert_eq!(healed.sha256, blob.sha256);
    assert_eq!(store.get(&blob.sha256).expect("get"), content);
}

#[test]
fn blob_files_hold_no_plaintext_at_rest_and_need_the_key() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = store_at(tmp.path());
    let canary = b"CANARY-SECRET-9f8e7d6c-do-not-appear-in-ciphertext";
    let (blob, _) = store.put_bytes(canary).expect("put");
    let path = store
        .root()
        .join("blobs")
        .join(&blob.sha256[..2])
        .join(&blob.sha256);
    let sealed = fs::read(&path).expect("read sealed");
    assert!(sealed.starts_with(b"TLPB1"), "sealed format magic");
    assert!(
        !sealed
            .windows(canary.len())
            .any(|window| window == canary.as_slice()),
        "plaintext must not appear in the sealed blob"
    );
    // A second open of the same store reads the same key and decrypts.
    let again = ProtectedStore::open(tmp.path().join("protected")).expect("reopen");
    assert_eq!(again.get(&blob.sha256).expect("get"), canary);
    // A malformed key is refused, not used.
    let key_path = store.root().join("store.key");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600)).expect("mode");
    }
    fs::write(&key_path, b"short").expect("break key");
    let err = ProtectedStore::open(tmp.path().join("protected")).expect_err("bad key refused");
    assert!(err.to_string().contains("32 bytes"), "{err}");
}

/// §14's boundary, restated as a test: the store preserves workspace bytes
/// verbatim — secrets included — because a snapshot that second-guessed its
/// bytes could not claim byte preservation. Redaction lives at Recovery
/// Capsule rendering, not here.
#[test]
fn secrets_are_preserved_verbatim_never_scanned_or_redacted() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = store_at(tmp.path());
    let secret = b"AWS_SECRET_ACCESS_KEY=AKIAIOSFODNN7EXAMPLEwJalrXUtnFEMI";
    let (blob, _) = store.put_bytes(secret).expect("put");
    assert_eq!(
        store.get(&blob.sha256).expect("get"),
        secret,
        "the bytes round-trip exactly — no masking, no rewriting"
    );
}

#[test]
fn the_store_refuses_to_live_inside_a_repository() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    fs::create_dir_all(repo.join(".git")).expect("fake repo");
    let err = ProtectedStore::open(repo.join("protected"))
        .expect_err("a store inside a repository must be refused");
    assert!(
        err.to_string().contains("outside the current repository"),
        "{err}"
    );
    assert!(!repo.join("protected").exists(), "nothing was created");
}

/* ── restore path containment ───────────────────────────────────────── */

fn file_entry(path: &str, content: &[u8], mode: u32) -> ManifestEntry {
    ManifestEntry {
        path: path.to_string(),
        kind: EntryKind::File,
        mode,
        bytes: content.len() as u64,
        sha256: Some(sha256_hex(content)),
        target: None,
    }
}

#[test]
fn restore_refuses_uncontained_manifest_paths() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = store_at(tmp.path());
    let content = b"escape attempt";
    store.put_bytes(content).expect("put");

    for evil in ["../outside.txt", "/etc/owned", "a/../../b"] {
        let mut manifest = empty_manifest(&format!("wsid1:{}", "0".repeat(64)), "/tmp/x");
        manifest.worktree = vec![file_entry(evil, content, 0o644)];
        let target = tmp.path().join(format!("target-{}", evil.len()));
        let err = restore_to_new(&store, &manifest, &target)
            .expect_err("an uncontained path must be refused");
        assert!(
            err.to_string().contains("contained relative path"),
            "{evil}: {err}"
        );
    }
    assert!(
        !tmp.path().join("outside.txt").exists(),
        "nothing escaped the target"
    );
}

#[cfg(unix)]
#[test]
fn restore_refuses_to_write_through_a_symlink_ancestor() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = store_at(tmp.path());
    let content = b"routed bytes";
    store.put_bytes(content).expect("put");
    let outside = tmp.path().join("outside");
    fs::create_dir_all(&outside).expect("outside dir");

    let mut manifest = empty_manifest(&format!("wsid1:{}", "0".repeat(64)), "/tmp/x");
    manifest.worktree = vec![
        ManifestEntry {
            path: "dir".to_string(),
            kind: EntryKind::Symlink,
            mode: 0,
            bytes: 0,
            sha256: None,
            target: Some(outside.display().to_string()),
        },
        file_entry("dir/planted.txt", content, 0o644),
    ];
    let err = restore_to_new(&store, &manifest, &tmp.path().join("target"))
        .expect_err("a write through a symlink ancestor must be refused");
    assert!(err.to_string().contains("symlink"), "{err}");
    assert!(
        !outside.join("planted.txt").exists(),
        "nothing landed outside the target"
    );
}

/// The live-ledger defect, pinned: the default restore target
/// (`<db-dir>/restores/<id>`) has intermediate directories that do not
/// exist yet — a fresh target must be creatable through them.
#[test]
fn restore_creates_missing_parent_directories_for_a_fresh_target() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = store_at(tmp.path());
    let content = b"nested target";
    store.put_bytes(content).expect("put");
    let mut manifest = empty_manifest(&format!("wsid1:{}", "0".repeat(64)), "/tmp/x");
    manifest.worktree = vec![file_entry("f.txt", content, 0o644)];
    let target = tmp.path().join("restores").join("deep").join("target");
    restore_to_new(&store, &manifest, &target).expect("restore through missing parents");
    assert_eq!(fs::read(target.join("f.txt")).expect("restored"), content);
}

#[test]
fn an_incomplete_manifest_cannot_be_restored() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store = store_at(tmp.path());
    let mut manifest = empty_manifest(&format!("wsid1:{}", "0".repeat(64)), "/tmp/x");
    manifest.completeness = "incomplete".to_string();
    manifest.incomplete_reason = Some("the tree moved".to_string());
    let err = restore_to_new(&store, &manifest, &tmp.path().join("target"))
        .expect_err("incomplete captures can never satisfy a byte-preservation claim");
    assert!(err.to_string().contains("incomplete"), "{err}");
}

/* ── structural read-only law ───────────────────────────────────────── */

/// Law 1's teeth, extended from the snapshotter and the fingerprinter to
/// the byte store: no mutating git verb, and no direct process spawn —
/// every subprocess goes through the guarded plumbing runner. The c1
/// containment vocabulary stays out of this module too.
#[test]
fn the_byte_store_is_structurally_read_only() {
    let src = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/protected_store.rs"),
    )
    .expect("read module");
    for verb in [
        "\"commit\"",
        "\"stash\"",
        "\"reset\"",
        "\"checkout\"",
        "\"clean\"",
        "\"push\"",
        "\"pull\"",
        "\"merge\"",
        "\"rebase\"",
        "\"restore\"",
        "\"rm\"",
        "\"mv\"",
        "\"apply\"",
        "\"init\"",
        "\"add\"",
        "\"tag\"",
        "\"branch\"",
        "\"switch\"",
        "\"fetch\"",
        "\"clone\"",
        "\"gc\"",
        "\"update-ref\"",
        "\"update-index\"",
        "\"read-tree\"",
        "\"write-tree\"",
        "\"checkout-index\"",
        "\"filter-branch\"",
    ] {
        assert!(
            !src.contains(verb),
            "protected_store.rs invokes git {verb} — the byte store reads, it never runs"
        );
    }
    assert!(
        !src.contains("Command::new"),
        "protected_store.rs spawns directly — subprocesses go through the guarded runner"
    );
}
