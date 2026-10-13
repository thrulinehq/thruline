//! MANDATE-v4 §14 (A208) — protected snapshots: the byte store the C2
//! snapshotter never had, plus byte-exact restore.
//!
//! C2 (A171) records *names and digests*; §14 requires the bytes themselves,
//! so that recovery never depends on git commits existing. This module is
//! that layer: a content-addressed, encrypted-at-rest blob store under the
//! ledger's own directory, a schema-versioned `TLSNAP_V1` manifest naming
//! every preserved axis, a capture that is structurally read-only against
//! the active workspace, and a restore that materializes content, modes,
//! symlinks, deletions and untracked files byte-exactly.
//!
//! Five laws:
//!
//! 1. **Read-only capture, structurally.** The only subprocesses this module
//!    speaks are read verbs — `rev-parse`, `ls-files`, `cat-file` — and every
//!    one goes through the guarded plumbing runner in [`crate::capture`]
//!    (`GIT_OPTIONAL_LOCKS=0`, hooks and user config neutralized). A test
//!    walks this file for mutating git verbs and for any direct process
//!    spawn, and fails on either. The proof is byte-identity of the source
//!    tree — `.git` included — before and after capture.
//! 2. **Atomic or honest.** The workspace identity (WSID_V1) is observed
//!    before and after the byte walk, and every captured byte is re-read and
//!    re-hashed in a second pass (the repository metadata tree included). If
//!    anything moved, the capture is stored with what it read, marked
//!    incomplete — and an incomplete capture can never satisfy a
//!    byte-preservation claim: restore refuses it.
//! 3. **Content addressing is the identity.** Blobs are keyed by the SHA-256
//!    of their *plaintext*; identical content is one blob, so re-protecting
//!    an unchanged tree writes zero new blobs and yields the same manifest
//!    digest (the §33 idempotency property). The store is source-agnostic on
//!    purpose: any future layer with bytes to preserve (the open A207
//!    overflow mirror-ingest half) can share it.
//! 4. **Encrypted at rest, restrictive always.** Blob and manifest content
//!    is sealed with XChaCha20-Poly1305 under a per-store key held at 0600;
//!    directories are 0700, sealed blobs 0400, and every deviation is a
//!    refusal, not a warning. The key never enters the ledger. OS-keychain
//!    custody is later hardening (§27), named, not silently assumed.
//! 5. **No redaction here, ever.** Raw snapshots legitimately hold workspace
//!    bytes — secrets included (§14). Nothing in this module scans, masks,
//!    or rewrites content: a snapshot that second-guessed its bytes could
//!    not claim byte preservation. Secret masking belongs to Recovery
//!    Capsule *rendering*, before capsule persistence or egress — that
//!    boundary lives there, not here.
//!
//! ## Store layout (under `<db-dir>/protected/`, never inside a repository)
//!
//! ```text
//! protected/            0700
//!   store.key           0600   32-byte XChaCha20-Poly1305 key
//!   staging/            0700   temp files, cleaned at open
//!   blobs/<aa>/<sha256> 0400   "TLPB1" ‖ 24-byte nonce ‖ ciphertext
//! ```
//!
//! The AEAD's associated data is the blob's own plaintext sha256 hex, so a
//! sealed blob moved under a different name fails to open even with the key.
//!
//! ## The TLSNAP_V1 canonical layout
//!
//! [`crate::chain`]'s injective conventions: `lp(s)` = u64-LE byte length
//! then UTF-8 bytes; `opt` = tag byte (`0x00` absent, `0x01` then `lp`);
//! every list is a u64-LE count then items in the documented sort order.
//!
//! ```text
//! lp("TLSNAP_V1")
//! lp("identity"); lp(source_identity)                     (the wsid1: string)
//! lp("scope"); include_untracked byte; count; lp(exclusion)…  (byte-sorted, deduped)
//! lp("root"); lp(root)
//! lp("completeness"); lp("complete"|"incomplete"); opt(incomplete_reason)
//! lp("worktree"); count; per entry (path-sorted):
//!     lp(path) lp("file"|"symlink"|"dir") u32-LE mode u64-LE bytes opt(sha256) opt(target)
//! lp("git"); count; entries, same shape, paths relative to the git dir
//! lp("staged"); count; per (path-sorted):
//!     lp(path) lp(index_object) lp(index_blob_sha256) opt(worktree_sha256)
//! lp("deleted"); count; lp(path)…                         (sorted, deduped)
//! lp("renamed"); count; per (new-path-sorted): lp(new_path) lp(old_path)
//! lp("complete")
//! ```
//!
//! The manifest digest is the SHA-256 of those bytes, lowercase hex. The
//! snapshot id, creation moment, and lineage live in the C2 snapshot
//! document's [`ProtectedRef`] (hash-covered by that document's own digest),
//! *outside* this preimage — which is exactly what makes re-protecting an
//! unchanged tree reproduce the same digest. A `file` entry carries a
//! content sha and no target; a `symlink` entry carries a target, mode 0,
//! bytes 0, and no sha — the link itself is the preserved object; a `dir`
//! entry (repository metadata only) preserves an EMPTY directory with its
//! mode, because git's own layout is load-bearing even where it holds no
//! files — an unborn repository's `refs/` is nothing but empty directories,
//! and a restore that dropped them would restore a non-repository. Changing
//! any byte of this layout is a versioned migration to `TLSNAP_V2`, never a
//! refactor; python3 vectors computed before this encoder existed pin it.
//!
//! ## Declared v1 boundaries (named, not silent)
//!
//! * **Linked worktrees are refused loudly.** A linked worktree's repository
//!   metadata is split across two directories whose topology one restored
//!   directory cannot reproduce; v1 speaks for primary worktrees only.
//! * **Submodule worktree contents are not byte-captured.** The gitlink, the
//!   `.gitmodules` file, and the superproject's `.git/modules` bytes are all
//!   preserved; the checked-out submodule tree itself is outside the walk —
//!   the same boundary WSID_V1 draws ("never content-walked").
//! * **Directory permission bits are not preserved** (files, symlinks and
//!   deletions are; directories are recreated with default modes). Restore
//!   equality is proven at the workspace-identity level, which is mode-aware
//!   for files.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::capture::{git_optional, git_output, trusted_git};
use crate::chain::sha256_hex;
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::mirror::{
    create_private_dir, reject_repository_destination, set_file_mode, sync_dir, sync_parent,
    verify_private_directory, verify_private_regular, write_exclusive,
};
use crate::observation::{
    append, AppendOutcome, Completeness, DerivedRef, NewObservation, ObservationType,
    ReceiptTerminal, SourceReceipt,
};
use crate::workspace_identity::{observe, Observation, Scope, WorkspaceFingerprint};

/// The version literal inside the canonical manifest.
pub const STORE_VERSION: &str = "TLSNAP_V1";

const BLOB_MAGIC: &[u8; 5] = b"TLPB1";
const NONCE_BYTES: usize = 24;
const KEY_BYTES: usize = 32;

/* ── the content-addressed encrypted blob store ─────────────────────── */

/// One stored blob, named by the SHA-256 of its plaintext.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobRef {
    pub sha256: String,
    pub bytes: u64,
}

/// The protected snapshot store. Source-agnostic: it stores bytes under
/// their own plaintext digest and knows nothing about what they mean.
#[derive(Debug, Clone)]
pub struct ProtectedStore {
    root: PathBuf,
    key: Key,
}

impl ProtectedStore {
    /// Open (creating if absent) the store at `root` — e.g.
    /// `<db-dir>/protected`. Refuses any destination inside a repository,
    /// any pre-existing non-private layout, and any malformed key file.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        reject_repository_destination(&root)?;
        create_private_dir(&root)?;
        let root = fs::canonicalize(&root).map_err(|e| CoreError::io(&root, e))?;
        reject_repository_destination(&root)?;
        create_private_dir(&root.join("staging"))?;
        create_private_dir(&root.join("blobs"))?;
        // Staging holds only in-flight temp files; anything found at open is
        // an interrupted write with no authority — remove it (the blob it
        // was building either landed atomically or never existed).
        let staging = root.join("staging");
        for entry in fs::read_dir(&staging).map_err(|e| CoreError::io(&staging, e))? {
            let path = entry.map_err(|e| CoreError::io(&staging, e))?.path();
            let meta = fs::symlink_metadata(&path).map_err(|e| CoreError::io(&path, e))?;
            if !meta.file_type().is_file() || meta.file_type().is_symlink() {
                return Err(CoreError::Integrity(
                    "protected staging holds a non-regular artifact".to_string(),
                ));
            }
            fs::remove_file(&path).map_err(|e| CoreError::io(&path, e))?;
        }
        sync_dir(&staging)?;
        let key = load_or_create_key(&root)?;
        Ok(ProtectedStore { root, key })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Store one plaintext. Returns the blob reference and whether a new
    /// blob was actually written (`false` = deduplicated against existing,
    /// verified content). A pre-existing blob that fails verification is
    /// healed in place — the plaintext in hand *is* the content the name
    /// speaks for.
    pub fn put_bytes(&self, plaintext: &[u8]) -> Result<(BlobRef, bool)> {
        let sha = sha256_hex(plaintext);
        let blob = BlobRef {
            sha256: sha.clone(),
            bytes: plaintext.len() as u64,
        };
        let final_path = self.blob_path(&sha)?;
        if fs::symlink_metadata(&final_path).is_ok() {
            match self.get(&sha) {
                Ok(_) => return Ok((blob, false)),
                Err(_) => {
                    // Corrupt on disk; replace with a fresh sealed copy.
                    fs::remove_file(&final_path).map_err(|e| CoreError::io(&final_path, e))?;
                }
            }
        }
        let sealed = self.seal(plaintext, &sha)?;
        let staging = self
            .root
            .join("staging")
            .join(format!("{}.part", Ulid::generate()));
        write_exclusive(&staging, &sealed)?;
        if let Some(parent) = final_path.parent() {
            create_private_dir(parent)?;
        }
        fs::rename(&staging, &final_path).map_err(|e| CoreError::io(&staging, e))?;
        set_file_mode(&final_path, 0o400)?;
        sync_parent(&final_path)?;
        Ok((blob, true))
    }

    /// Load and verify one blob: seal opened under the key with the blob's
    /// own name as associated data, plaintext re-hashed against the name.
    pub fn get(&self, sha256: &str) -> Result<Vec<u8>> {
        let path = self.blob_path(sha256)?;
        verify_private_regular(&path, 0o400)?;
        let sealed = fs::read(&path).map_err(|e| CoreError::io(&path, e))?;
        let plaintext = self.open_sealed(&sealed, sha256)?;
        if sha256_hex(&plaintext) != sha256 {
            return Err(CoreError::Integrity(format!(
                "protected blob {sha256} decrypted to different content — the store is damaged"
            )));
        }
        Ok(plaintext)
    }

    pub fn has(&self, sha256: &str) -> Result<bool> {
        Ok(fs::symlink_metadata(self.blob_path(sha256)?).is_ok())
    }

    /// Count every blob in the store (dedup makes this the distinct-content
    /// count — the §33 idempotency assertions read it).
    pub fn blob_count(&self) -> Result<u64> {
        let blobs = self.root.join("blobs");
        verify_private_directory(&blobs)?;
        let mut n = 0u64;
        for shard in fs::read_dir(&blobs).map_err(|e| CoreError::io(&blobs, e))? {
            let shard = shard.map_err(|e| CoreError::io(&blobs, e))?.path();
            verify_private_directory(&shard)?;
            for entry in fs::read_dir(&shard).map_err(|e| CoreError::io(&shard, e))? {
                let _ = entry.map_err(|e| CoreError::io(&shard, e))?;
                n += 1;
            }
        }
        Ok(n)
    }

    /// Re-open and re-hash every blob in the store. Returns how many were
    /// checked; the first defective blob fails loudly, named.
    pub fn verify_all(&self) -> Result<u64> {
        let blobs = self.root.join("blobs");
        verify_private_directory(&blobs)?;
        let mut checked = 0u64;
        let mut shards: Vec<PathBuf> = Vec::new();
        for shard in fs::read_dir(&blobs).map_err(|e| CoreError::io(&blobs, e))? {
            shards.push(shard.map_err(|e| CoreError::io(&blobs, e))?.path());
        }
        shards.sort();
        for shard in shards {
            verify_private_directory(&shard)?;
            let mut names: Vec<String> = Vec::new();
            for entry in fs::read_dir(&shard).map_err(|e| CoreError::io(&shard, e))? {
                let entry = entry.map_err(|e| CoreError::io(&shard, e))?;
                names.push(entry.file_name().to_string_lossy().into_owned());
            }
            names.sort();
            for name in names {
                self.get(&name)?;
                checked += 1;
            }
        }
        Ok(checked)
    }

    fn blob_path(&self, sha256: &str) -> Result<PathBuf> {
        if sha256.len() != 64
            || !sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(CoreError::Integrity(format!(
                "{sha256:?} is not a lowercase sha256 blob name"
            )));
        }
        Ok(self.root.join("blobs").join(&sha256[..2]).join(sha256))
    }

    fn seal(&self, plaintext: &[u8], aad: &str) -> Result<Vec<u8>> {
        let cipher = XChaCha20Poly1305::new(&self.key);
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| CoreError::Integrity("sealing a protected blob failed".to_string()))?;
        let mut out = Vec::with_capacity(BLOB_MAGIC.len() + NONCE_BYTES + ciphertext.len());
        out.extend_from_slice(BLOB_MAGIC);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    fn open_sealed(&self, sealed: &[u8], aad: &str) -> Result<Vec<u8>> {
        if sealed.len() < BLOB_MAGIC.len() + NONCE_BYTES
            || sealed[..BLOB_MAGIC.len()] != BLOB_MAGIC[..]
        {
            return Err(CoreError::Integrity(format!(
                "protected blob {aad} is truncated or not a sealed blob — refusing to load it"
            )));
        }
        let nonce = XNonce::from_slice(&sealed[BLOB_MAGIC.len()..BLOB_MAGIC.len() + NONCE_BYTES]);
        let cipher = XChaCha20Poly1305::new(&self.key);
        cipher
            .decrypt(
                nonce,
                Payload {
                    msg: &sealed[BLOB_MAGIC.len() + NONCE_BYTES..],
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| {
                CoreError::Integrity(format!(
                    "protected blob {aad} failed authenticated decryption — \
                     tampered, truncated, or renamed"
                ))
            })
    }
}

fn load_or_create_key(root: &Path) -> Result<Key> {
    let path = root.join("store.key");
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            verify_private_regular(&path, 0o600)?;
            let bytes = fs::read(&path).map_err(|e| CoreError::io(&path, e))?;
            if bytes.len() != KEY_BYTES {
                return Err(CoreError::Integrity(
                    "protected store key is not 32 bytes — refusing to use it".to_string(),
                ));
            }
            Ok(*Key::from_slice(&bytes))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let key = XChaCha20Poly1305::generate_key(&mut OsRng);
            write_exclusive(&path, &key)?;
            set_file_mode(&path, 0o600)?;
            sync_parent(&path)?;
            Ok(key)
        }
        Err(e) => Err(CoreError::io(&path, e)),
    }
}

/* ── the TLSNAP_V1 manifest ─────────────────────────────────────────── */

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    File,
    Symlink,
    /// An empty directory in the repository metadata tree, preserved with
    /// its mode. Worktree entries never take this kind: git's index and
    /// porcelain cannot see empty directories, so neither does the identity
    /// oracle — but `.git`'s own empty directories are load-bearing.
    Dir,
}

impl EntryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EntryKind::File => "file",
            EntryKind::Symlink => "symlink",
            EntryKind::Dir => "dir",
        }
    }
}

/// One preserved filesystem object. A `File` carries a content sha (its
/// blob key) and mode bits; a `Symlink` carries its target and nothing else
/// — the link is preserved as a link, never followed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub path: String,
    pub kind: EntryKind,
    pub mode: u32,
    pub bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// §14's staged duality, explicit: the index version of a staged path is
/// preserved as its own blob, beside the worktree version — BOTH survive
/// when they differ, and restore reproduces the difference through the
/// preserved index.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StagedDuality {
    pub path: String,
    pub index_object: String,
    pub index_blob_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_sha256: Option<String>,
}

/// One preserved staged rename: new name, old name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RenamedPair {
    pub new_path: String,
    pub old_path: String,
}

/// The TLSNAP_V1 content manifest — every §14 axis. Deliberately free of
/// snapshot id, creation moment, and lineage: those are envelope facts in
/// the C2 document's [`ProtectedRef`], so an unchanged tree re-protects to
/// the identical manifest (and the identical stored blob).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotManifest {
    /// The source workspace identity (`wsid1:` string) this capture is
    /// coherent with — the frozen expectation, observed before the walk.
    pub source_identity: String,
    pub include_untracked: bool,
    pub exclusions: Vec<String>,
    pub root: String,
    /// `"complete"` or `"incomplete"` — an incomplete capture stores what it
    /// read and can never satisfy a byte-preservation claim.
    pub completeness: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incomplete_reason: Option<String>,
    pub worktree: Vec<ManifestEntry>,
    /// Repository metadata: every file under the (primary) git dir,
    /// path-relative to it.
    pub git: Vec<ManifestEntry>,
    pub staged: Vec<StagedDuality>,
    pub deleted: Vec<String>,
    pub renamed: Vec<RenamedPair>,
}

impl SnapshotManifest {
    pub fn is_complete(&self) -> bool {
        self.completeness == "complete"
    }
}

fn push_lp(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u64).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn push_opt(out: &mut Vec<u8>, v: Option<&str>) {
    match v {
        None => out.push(0x00),
        Some(s) => {
            out.push(0x01);
            push_lp(out, s);
        }
    }
}

fn push_count(out: &mut Vec<u8>, n: usize) {
    out.extend_from_slice(&(n as u64).to_le_bytes());
}

fn push_entry(out: &mut Vec<u8>, e: &ManifestEntry) {
    push_lp(out, &e.path);
    push_lp(out, e.kind.as_str());
    out.extend_from_slice(&e.mode.to_le_bytes());
    out.extend_from_slice(&e.bytes.to_le_bytes());
    push_opt(out, e.sha256.as_deref());
    push_opt(out, e.target.as_deref());
}

/// The documented TLSNAP_V1 canonical bytes. Lists are normalized here —
/// sorted (and exclusions/deletions deduped) — so arrival order is nuisance,
/// never identity. The python3 vectors pin this layout.
pub fn canonical_bytes(m: &SnapshotManifest) -> Vec<u8> {
    let mut out = Vec::with_capacity(4096);
    push_lp(&mut out, STORE_VERSION);
    push_lp(&mut out, "identity");
    push_lp(&mut out, &m.source_identity);
    push_lp(&mut out, "scope");
    out.push(u8::from(m.include_untracked));
    let mut exclusions = m.exclusions.clone();
    exclusions.sort();
    exclusions.dedup();
    push_count(&mut out, exclusions.len());
    for e in &exclusions {
        push_lp(&mut out, e);
    }
    push_lp(&mut out, "root");
    push_lp(&mut out, &m.root);
    push_lp(&mut out, "completeness");
    push_lp(&mut out, &m.completeness);
    push_opt(&mut out, m.incomplete_reason.as_deref());
    let mut worktree = m.worktree.clone();
    worktree.sort();
    push_lp(&mut out, "worktree");
    push_count(&mut out, worktree.len());
    for e in &worktree {
        push_entry(&mut out, e);
    }
    let mut git = m.git.clone();
    git.sort();
    push_lp(&mut out, "git");
    push_count(&mut out, git.len());
    for e in &git {
        push_entry(&mut out, e);
    }
    let mut staged = m.staged.clone();
    staged.sort();
    push_lp(&mut out, "staged");
    push_count(&mut out, staged.len());
    for s in &staged {
        push_lp(&mut out, &s.path);
        push_lp(&mut out, &s.index_object);
        push_lp(&mut out, &s.index_blob_sha256);
        push_opt(&mut out, s.worktree_sha256.as_deref());
    }
    let mut deleted = m.deleted.clone();
    deleted.sort();
    deleted.dedup();
    push_lp(&mut out, "deleted");
    push_count(&mut out, deleted.len());
    for p in &deleted {
        push_lp(&mut out, p);
    }
    let mut renamed = m.renamed.clone();
    renamed.sort();
    push_lp(&mut out, "renamed");
    push_count(&mut out, renamed.len());
    for r in &renamed {
        push_lp(&mut out, &r.new_path);
        push_lp(&mut out, &r.old_path);
    }
    push_lp(&mut out, "complete");
    out
}

/// The manifest digest — SHA-256 of [`canonical_bytes`], lowercase hex.
pub fn manifest_digest(m: &SnapshotManifest) -> String {
    sha256_hex(&canonical_bytes(m))
}

/// The envelope the C2 snapshot document carries for its byte layer:
/// versioned lineage from the digest-only record to the byte store. Rides
/// inside the content-addressed C2 doc, so it is hash-covered without any
/// schema change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtectedRef {
    pub store_version: String,
    /// The TLSNAP_V1 canonical digest — the snapshot digest.
    pub manifest_digest: String,
    /// The CAS key of the stored manifest JSON blob.
    pub manifest_blob_sha256: String,
    /// The repository identity observed at capture, for later observation
    /// writers that outlive the workspace itself. `None` is an abstention.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_identity: Option<String>,
    /// The prior protected snapshot row (same root), when one exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lineage: Option<String>,
    pub worktree_files: u64,
    pub git_files: u64,
    pub new_blobs: u64,
    pub complete: bool,
}

/* ── capture ────────────────────────────────────────────────────────── */

/// What a capture produced. The manifest is stored (even when incomplete —
/// it says what was read); `complete` gates every byte-preservation claim.
#[derive(Debug, Clone)]
pub struct CaptureOutcome {
    pub manifest: SnapshotManifest,
    pub manifest_digest: String,
    pub manifest_blob_sha256: String,
    pub repository_identity: Option<String>,
    pub new_blobs: u64,
}

impl CaptureOutcome {
    /// Assemble the C2 document's byte-layer envelope.
    pub fn protected_ref(&self, lineage: Option<String>) -> ProtectedRef {
        ProtectedRef {
            store_version: STORE_VERSION.to_string(),
            manifest_digest: self.manifest_digest.clone(),
            manifest_blob_sha256: self.manifest_blob_sha256.clone(),
            repository_identity: self.repository_identity.clone(),
            lineage,
            worktree_files: self.manifest.worktree.len() as u64,
            git_files: self.manifest.git.len() as u64,
            new_blobs: self.new_blobs,
            complete: self.manifest.is_complete(),
        }
    }
}

/// One path as read from disk: kind, permission bits, content (files only),
/// symlink target (links only).
struct ReadObject {
    kind: EntryKind,
    mode: u32,
    bytes: Option<Vec<u8>>,
    target: Option<String>,
}

fn read_entry(base: &Path, rel: &str) -> Result<ReadObject> {
    let full = base.join(rel);
    let meta = fs::symlink_metadata(&full).map_err(|e| CoreError::io(&full, e))?;
    if meta.file_type().is_symlink() {
        let target = fs::read_link(&full).map_err(|e| CoreError::io(&full, e))?;
        let target = target.to_str().map(str::to_owned).ok_or_else(|| {
            CoreError::Integrity(format!("symlink target at {rel} is not valid UTF-8"))
        })?;
        return Ok(ReadObject {
            kind: EntryKind::Symlink,
            mode: 0,
            bytes: None,
            target: Some(target),
        });
    }
    if meta.file_type().is_dir() {
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            meta.permissions().mode() & 0o7777
        };
        #[cfg(not(unix))]
        let mode = {
            return Err(CoreError::Config(
                "protected snapshots are not implemented on this platform".to_string(),
            ));
        };
        return Ok(ReadObject {
            kind: EntryKind::Dir,
            mode,
            bytes: None,
            target: None,
        });
    }
    if !meta.file_type().is_file() {
        return Err(CoreError::Integrity(format!(
            "{rel} is neither a regular file nor a symlink — this capture cannot speak for it"
        )));
    }
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o7777
    };
    #[cfg(not(unix))]
    let mode = {
        return Err(CoreError::Config(
            "protected snapshots are not implemented on this platform".to_string(),
        ));
    };
    let bytes = fs::read(&full).map_err(|e| CoreError::io(&full, e))?;
    Ok(ReadObject {
        kind: EntryKind::File,
        mode,
        bytes: Some(bytes),
        target: None,
    })
}

/// Every file and symlink under the git dir, plus every EMPTY directory —
/// git's layout is load-bearing even where it holds no files (an unborn
/// repository's `refs/` is nothing but empty directories).
fn walk_git_dir(git_dir: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel_dir) = stack.pop() {
        let full = git_dir.join(&rel_dir);
        let mut children: Vec<(String, bool)> = Vec::new();
        for entry in fs::read_dir(&full).map_err(|e| CoreError::io(&full, e))? {
            let entry = entry.map_err(|e| CoreError::io(&full, e))?;
            let name = entry
                .file_name()
                .to_str()
                .map(str::to_owned)
                .ok_or_else(|| {
                    CoreError::Integrity(
                        "a repository metadata path is not valid UTF-8".to_string(),
                    )
                })?;
            let meta =
                fs::symlink_metadata(entry.path()).map_err(|e| CoreError::io(entry.path(), e))?;
            let is_dir = meta.file_type().is_dir() && !meta.file_type().is_symlink();
            children.push((name, is_dir));
        }
        if children.is_empty() && !rel_dir.as_os_str().is_empty() {
            out.push(rel_dir.to_string_lossy().into_owned());
            continue;
        }
        children.sort();
        for (name, is_dir) in children {
            let rel = if rel_dir.as_os_str().is_empty() {
                PathBuf::from(&name)
            } else {
                rel_dir.join(&name)
            };
            if is_dir {
                stack.push(rel);
            } else {
                out.push(rel.to_string_lossy().into_owned());
            }
        }
    }
    out.sort();
    Ok(out)
}

fn parse_ls_files(bytes: &[u8]) -> Result<Vec<(String, String, String)>> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    for entry in text.split('\0').filter(|t| !t.is_empty()) {
        let Some((meta, path)) = entry.split_once('\t') else {
            return Err(CoreError::Integrity(format!(
                "unrecognized ls-files entry {entry:?}"
            )));
        };
        let fields: Vec<&str> = meta.split(' ').collect();
        let [mode, object, _stage] = fields.as_slice() else {
            return Err(CoreError::Integrity(format!(
                "unrecognized ls-files entry {entry:?}"
            )));
        };
        out.push(((*mode).to_string(), (*object).to_string(), path.to_string()));
    }
    Ok(out)
}

/// Capture the workspace at `root` into `store`, byte for byte. Read-only
/// against the active workspace; the `seam` runs after each newly written
/// blob (the §32 injected-failure seam for interrupted-capture tests — the
/// public wrapper passes a no-op).
pub fn capture_with(
    store: &ProtectedStore,
    root: &Path,
    scope: Scope,
    seam: &mut dyn FnMut(u64) -> Result<()>,
) -> Result<CaptureOutcome> {
    let git = trusted_git()?;
    let Some(toplevel) = git_optional(&git, root, &["rev-parse", "--show-toplevel"])? else {
        return Err(CoreError::Config(format!(
            "{} is not inside a git worktree — protected snapshots speak only for git workspaces",
            root.display()
        )));
    };
    let base = PathBuf::from(&toplevel);
    let base = fs::canonicalize(&base).map_err(|e| CoreError::io(&base, e))?;
    let git_dir = resolve_git_path(&git, &base, &["rev-parse", "--absolute-git-dir"])?;
    let common_dir = resolve_git_path(&git, &base, &["rev-parse", "--git-common-dir"])?;
    if git_dir != common_dir {
        return Err(CoreError::Config(
            "this is a linked worktree — protected snapshot v1 speaks for primary worktrees \
             only (a declared gap, not an oversight); protect from the primary worktree"
                .to_string(),
        ));
    }

    // The identity bracket's left edge: a complete observation, or no
    // capture at all (nothing has been read yet — there is nothing an
    // incomplete manifest could honestly preserve).
    let pre = match observe(&base, scope)? {
        Observation::Complete(fp) => *fp,
        Observation::Incomplete { reason, .. } => {
            return Err(CoreError::Integrity(format!(
                "the workspace is moving — a coherent capture could not begin ({reason})"
            )))
        }
    };
    let source_identity = pre.identity().to_string();
    let repository_identity = pre.doc().repo_id.clone();
    let scope = pre.doc().scope.clone();

    let mut new_blobs = 0u64;
    let mut incomplete: Option<String> = None;

    // The worktree set: tracked paths (gitlinks excluded — submodule
    // worktrees are outside the walk, as documented) minus worktree
    // deletions, plus the fingerprint's in-scope untracked files.
    let ls = git_output(&git, &base, &["ls-files", "-s", "-z", "--full-name"])?;
    let index_entries = parse_ls_files(&ls)?;
    let deleted: BTreeSet<&str> = pre.doc().deleted.iter().map(String::as_str).collect();
    let mut paths: BTreeSet<String> = index_entries
        .iter()
        .filter(|(mode, _, _)| mode != "160000")
        .map(|(_, _, path)| path.clone())
        .collect();
    paths.retain(|p| !deleted.contains(p.as_str()));
    for u in &pre.doc().untracked {
        paths.insert(u.path.clone());
    }

    let mut worktree: Vec<ManifestEntry> = Vec::new();
    for rel in &paths {
        match capture_one(store, &base, rel, &mut new_blobs, &mut *seam)? {
            Ok(entry) => worktree.push(entry),
            Err(reason) => {
                incomplete = Some(reason);
                break;
            }
        }
    }

    // Repository metadata: every file under the primary git dir.
    let mut git_entries: Vec<ManifestEntry> = Vec::new();
    if incomplete.is_none() {
        for rel in &walk_git_dir(&git_dir)? {
            match capture_one(store, &git_dir, rel, &mut new_blobs, &mut *seam)? {
                Ok(entry) => git_entries.push(entry),
                Err(reason) => {
                    incomplete = Some(reason);
                    break;
                }
            }
        }
    }

    // Staged duality: preserve the index version of every staged path as
    // its own blob, beside the worktree version already captured above.
    let worktree_by_path: BTreeMap<&str, &ManifestEntry> =
        worktree.iter().map(|e| (e.path.as_str(), e)).collect();
    let mut staged: Vec<StagedDuality> = Vec::new();
    if incomplete.is_none() {
        let mut staged_sources: Vec<(String, String, String)> = pre
            .doc()
            .staged
            .iter()
            .map(|s| (s.path.clone(), s.index_mode.clone(), s.index_object.clone()))
            .collect();
        for r in &pre.doc().renamed {
            staged_sources.push((
                r.new_path.clone(),
                r.index_mode.clone(),
                r.index_object.clone(),
            ));
        }
        for (path, index_mode, object) in staged_sources {
            if index_mode == "160000" || index_mode == "120000" {
                continue; // gitlinks and symlink index entries hold no blob bytes to extract
            }
            let bytes = git_output(&git, &base, &["cat-file", "blob", &object])?;
            let (blob, written) = store.put_bytes(&bytes)?;
            if written {
                new_blobs += 1;
                seam(new_blobs)?;
            }
            staged.push(StagedDuality {
                path: path.clone(),
                index_object: object,
                index_blob_sha256: blob.sha256,
                worktree_sha256: worktree_by_path
                    .get(path.as_str())
                    .and_then(|e| e.sha256.clone()),
            });
        }
    }
    drop(worktree_by_path);

    // The second pass: every captured byte is re-read and re-hashed — the
    // repository metadata tree included, where WSID_V1's porcelain sentinel
    // cannot see. Any difference is an interleaved write, said out loud.
    if incomplete.is_none() {
        for entry in &worktree {
            if let Some(reason) = reverify_entry(&base, entry)? {
                incomplete = Some(reason);
                break;
            }
        }
    }
    if incomplete.is_none() {
        let second_walk = walk_git_dir(&git_dir)?;
        let first_walk: Vec<String> = git_entries.iter().map(|e| e.path.clone()).collect();
        if second_walk != first_walk {
            incomplete = Some(
                "the repository metadata tree changed during capture — capture again".to_string(),
            );
        } else {
            for entry in &git_entries {
                if let Some(reason) = reverify_entry(&git_dir, entry)? {
                    incomplete = Some(reason);
                    break;
                }
            }
        }
    }

    // The bracket's right edge: the same identity, or an honest incomplete.
    if incomplete.is_none() {
        match observe(&base, scope.clone())? {
            Observation::Complete(post) if post.identity() == source_identity => {}
            Observation::Complete(_) => {
                incomplete = Some(
                    "the workspace changed during capture — the pre and post identities \
                     differ; capture again"
                        .to_string(),
                );
            }
            Observation::Incomplete { reason, .. } => {
                incomplete = Some(reason);
            }
        }
    }

    let manifest = SnapshotManifest {
        source_identity,
        include_untracked: scope.include_untracked,
        exclusions: scope.exclusions,
        root: canonical_text(&base),
        completeness: if incomplete.is_none() {
            "complete".to_string()
        } else {
            "incomplete".to_string()
        },
        incomplete_reason: incomplete,
        worktree,
        git: git_entries,
        staged,
        deleted: pre.doc().deleted.clone(),
        renamed: pre
            .doc()
            .renamed
            .iter()
            .map(|r| RenamedPair {
                new_path: r.new_path.clone(),
                old_path: r.old_path.clone(),
            })
            .collect(),
    };
    let digest = manifest_digest(&manifest);
    let manifest_json = serde_json::to_vec_pretty(&manifest)?;
    let (manifest_blob, written) = store.put_bytes(&manifest_json)?;
    if written {
        new_blobs += 1;
    }
    Ok(CaptureOutcome {
        manifest,
        manifest_digest: digest,
        manifest_blob_sha256: manifest_blob.sha256,
        repository_identity,
        new_blobs,
    })
}

/// The public capture: no seam.
pub fn capture(store: &ProtectedStore, root: &Path, scope: Scope) -> Result<CaptureOutcome> {
    capture_with(store, root, scope, &mut |_| Ok(()))
}

/// Capture one path into the store: read, blob, seam. The outer `Result`
/// is a store/seam failure (hard error); the inner `Err(String)` is a
/// moved-tree observation — the honest-incomplete path, not a failure.
fn capture_one(
    store: &ProtectedStore,
    base: &Path,
    rel: &str,
    new_blobs: &mut u64,
    seam: &mut dyn FnMut(u64) -> Result<()>,
) -> Result<std::result::Result<ManifestEntry, String>> {
    match read_entry(base, rel) {
        Ok(read) => {
            let (sha, size) = match read.bytes {
                Some(bytes) => {
                    let (blob, written) = store.put_bytes(&bytes)?;
                    if written {
                        *new_blobs += 1;
                        seam(*new_blobs)?;
                    }
                    (Some(blob.sha256), blob.bytes)
                }
                None => (None, 0),
            };
            Ok(Ok(ManifestEntry {
                path: rel.to_string(),
                kind: read.kind,
                mode: read.mode,
                bytes: size,
                sha256: sha,
                target: read.target,
            }))
        }
        Err(e) => Ok(Err(format!(
            "could not read {rel} mid-capture ({e}) — the tree moved or refused"
        ))),
    }
}

fn reverify_entry(base: &Path, entry: &ManifestEntry) -> Result<Option<String>> {
    match read_entry(base, &entry.path) {
        Ok(read) => {
            let sha = read.bytes.as_deref().map(sha256_hex);
            let size = read.bytes.as_ref().map_or(0, |b| b.len() as u64);
            if read.kind != entry.kind
                || read.mode != entry.mode
                || size != entry.bytes
                || sha != entry.sha256
                || read.target != entry.target
            {
                return Ok(Some(format!(
                    "{} changed during capture — capture again",
                    entry.path
                )));
            }
            Ok(None)
        }
        Err(e) => Ok(Some(format!(
            "could not re-read {} mid-capture ({e}) — the tree moved or refused",
            entry.path
        ))),
    }
}

fn resolve_git_path(git: &Path, base: &Path, args: &[&str]) -> Result<PathBuf> {
    let raw = git_optional(git, base, args)?.ok_or_else(|| {
        CoreError::Integrity(format!(
            "git {} yielded nothing at {}",
            args.join(" "),
            base.display()
        ))
    })?;
    let path = PathBuf::from(&raw);
    let path = if path.is_absolute() {
        path
    } else {
        base.join(path)
    };
    fs::canonicalize(&path).map_err(|e| CoreError::io(&path, e))
}

fn canonical_text(p: &Path) -> String {
    fs::canonicalize(p)
        .unwrap_or_else(|_| p.to_path_buf())
        .display()
        .to_string()
}

/* ── manifest load ──────────────────────────────────────────────────── */

/// Load the manifest a [`ProtectedRef`] names, with both digests
/// re-verified: the blob's own content addressing, then the canonical
/// TLSNAP_V1 digest against the envelope's record.
pub fn load_manifest(store: &ProtectedStore, pref: &ProtectedRef) -> Result<SnapshotManifest> {
    if pref.store_version != STORE_VERSION {
        return Err(CoreError::Integrity(format!(
            "snapshot store version {:?} is not {STORE_VERSION} — a newer binary wrote it",
            pref.store_version
        )));
    }
    let bytes = store.get(&pref.manifest_blob_sha256)?;
    let manifest: SnapshotManifest = serde_json::from_slice(&bytes)?;
    let digest = manifest_digest(&manifest);
    if digest != pref.manifest_digest {
        return Err(CoreError::Integrity(format!(
            "manifest canonical digest {digest} does not match the recorded {} — \
             the snapshot record is damaged",
            pref.manifest_digest
        )));
    }
    Ok(manifest)
}

/* ── verify ─────────────────────────────────────────────────────────── */

/// Re-hash every blob a snapshot references, plus its manifest. Returns the
/// number of blobs checked (manifest included); the first defect fails
/// loudly, naming the blob.
pub fn verify_snapshot(store: &ProtectedStore, pref: &ProtectedRef) -> Result<u64> {
    let manifest = load_manifest(store, pref)?;
    let mut checked = 1u64; // the manifest blob itself, verified by load_manifest
    let mut shas: BTreeSet<&str> = BTreeSet::new();
    for e in manifest.worktree.iter().chain(manifest.git.iter()) {
        if let Some(sha) = e.sha256.as_deref() {
            shas.insert(sha);
        }
    }
    for s in &manifest.staged {
        shas.insert(&s.index_blob_sha256);
    }
    for sha in shas {
        store.get(sha)?;
        checked += 1;
    }
    Ok(checked)
}

/* ── restore ────────────────────────────────────────────────────────── */

/// What a safe-directory restore materialized.
#[derive(Debug, Clone)]
pub struct RestoreReport {
    pub worktree_files: u64,
    pub git_files: u64,
    pub bytes_written: u64,
}

fn safe_rel(path: &str) -> Result<&Path> {
    let p = Path::new(path);
    if path.is_empty()
        || p.is_absolute()
        || p.components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(CoreError::Integrity(format!(
            "manifest path {path:?} is not a contained relative path — refusing to materialize it"
        )));
    }
    Ok(p)
}

/// No ancestor of `full` between `boundary` and the file itself may be a
/// symlink: a link planted earlier in the restore could otherwise route a
/// later write outside the target.
fn assert_no_symlink_ancestor(boundary: &Path, full: &Path) -> Result<()> {
    let rel = full.strip_prefix(boundary).map_err(|_| {
        CoreError::Integrity("restore path escaped its target directory".to_string())
    })?;
    let mut current = boundary.to_path_buf();
    let components: Vec<_> = rel.components().collect();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(CoreError::Integrity(format!(
                    "restore would traverse a symlink at {} — refused",
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(CoreError::io(&current, e)),
        }
    }
    Ok(())
}

fn materialize_entry(
    store: &ProtectedStore,
    boundary: &Path,
    base: &Path,
    entry: &ManifestEntry,
) -> Result<u64> {
    let rel = safe_rel(&entry.path)?;
    let full = base.join(rel);
    assert_no_symlink_ancestor(boundary, &full)?;
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
    }
    match fs::symlink_metadata(&full) {
        Ok(meta) => {
            // Over-work restore replaces in place; safe-dir restore never
            // hits this arm (the target started empty).
            if meta.file_type().is_dir() && !meta.file_type().is_symlink() {
                if entry.kind != EntryKind::Dir {
                    // Only an EMPTY directory may be replaced: anything
                    // inside it was previewed as its own delete first.
                    fs::remove_dir(&full).map_err(|e| CoreError::io(&full, e))?;
                }
            } else {
                fs::remove_file(&full).map_err(|e| CoreError::io(&full, e))?;
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(CoreError::io(&full, e)),
    }
    match entry.kind {
        EntryKind::Dir => {
            fs::create_dir_all(&full).map_err(|e| CoreError::io(&full, e))?;
            set_file_mode(&full, entry.mode)?;
            Ok(0)
        }
        EntryKind::Symlink => {
            let target = entry.target.as_deref().ok_or_else(|| {
                CoreError::Integrity(format!("symlink entry {} carries no target", entry.path))
            })?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, &full).map_err(|e| CoreError::io(&full, e))?;
            #[cfg(not(unix))]
            return Err(CoreError::Config(
                "protected snapshots are not implemented on this platform".to_string(),
            ));
            let back = fs::read_link(&full).map_err(|e| CoreError::io(&full, e))?;
            if back.to_string_lossy() != target {
                return Err(CoreError::Integrity(format!(
                    "restored symlink {} does not read back its target",
                    entry.path
                )));
            }
            Ok(0)
        }
        EntryKind::File => {
            let sha = entry.sha256.as_deref().ok_or_else(|| {
                CoreError::Integrity(format!("file entry {} carries no content sha", entry.path))
            })?;
            let bytes = store.get(sha)?;
            if bytes.len() as u64 != entry.bytes {
                return Err(CoreError::Integrity(format!(
                    "blob {sha} length does not match the manifest entry for {}",
                    entry.path
                )));
            }
            fs::write(&full, &bytes).map_err(|e| CoreError::io(&full, e))?;
            set_file_mode(&full, entry.mode)?;
            // Write-back verification: the restored bytes are re-read and
            // re-hashed — byte-exact or loud, per file.
            let back = fs::read(&full).map_err(|e| CoreError::io(&full, e))?;
            if sha256_hex(&back) != sha {
                return Err(CoreError::Integrity(format!(
                    "restored file {} does not read back byte-identical",
                    entry.path
                )));
            }
            Ok(bytes.len() as u64)
        }
    }
}

/// Materialize a snapshot into a separate safe directory — the §14 default.
/// `target` must be new or an empty directory, and must not live inside any
/// repository. Refuses incomplete manifests: bytes that straddle two states
/// cannot honestly claim to reproduce either.
pub fn restore_to_new(
    store: &ProtectedStore,
    manifest: &SnapshotManifest,
    target: &Path,
) -> Result<RestoreReport> {
    if !manifest.is_complete() {
        return Err(CoreError::Integrity(format!(
            "this capture is incomplete ({}) — its bytes describe no single moment and \
             cannot be materialized as one; capture again from a live workspace",
            manifest
                .incomplete_reason
                .as_deref()
                .unwrap_or("no reason recorded")
        )));
    }
    reject_repository_destination(target)?;
    match fs::symlink_metadata(target) {
        Ok(meta) => {
            if !meta.file_type().is_dir() || meta.file_type().is_symlink() {
                return Err(CoreError::Config(
                    "restore target exists and is not a plain directory".to_string(),
                ));
            }
            if fs::read_dir(target)
                .map_err(|e| CoreError::io(target, e))?
                .next()
                .is_some()
            {
                return Err(CoreError::Config(
                    "restore target directory is not empty — restoring over existing \
                     content requires the explicit over-working-tree gate"
                        .to_string(),
                ));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // The default target shape (`<db-dir>/restores/<id>`) may have
            // missing intermediate directories; create them, then the
            // target itself under the private-mode discipline.
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| CoreError::io(parent, e))?;
            }
            create_private_dir(target)?;
        }
        Err(e) => return Err(CoreError::io(target, e)),
    }
    let target = fs::canonicalize(target).map_err(|e| CoreError::io(target, e))?;
    let git_base = target.join(".git");
    let mut bytes_written = 0u64;
    for entry in &manifest.git {
        bytes_written += materialize_entry(store, &target, &git_base, entry)?;
    }
    for entry in &manifest.worktree {
        bytes_written += materialize_entry(store, &target, &target, entry)?;
    }
    // Deletions are preserved as absence; nothing was materialized for
    // them, and the restored index still names them — exactly the state
    // the source tree held.
    Ok(RestoreReport {
        worktree_files: manifest.worktree.len() as u64,
        git_files: manifest.git.len() as u64,
        bytes_written,
    })
}

/* ── restore over a working tree: the one destructive edge ──────────── */

/// One planned write in an over-working-tree restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedWrite {
    /// Target-relative display path (`.git/…` for repository metadata).
    pub path: String,
    pub bytes: u64,
}

/// The exact scope of an over-working-tree restore: everything it will
/// touch, and nothing else. The apply step is constructed from this plan
/// alone, so the preview *is* the operation's boundary.
#[derive(Debug, Clone)]
pub struct OverWorkPlan {
    pub writes: Vec<PlannedWrite>,
    pub deletes: Vec<String>,
    /// The target tree's identity the plan was computed against — apply
    /// re-observes and refuses if the tree moved after planning.
    pub target_identity: String,
}

fn manifest_entry_matches(base: &Path, entry: &ManifestEntry) -> Result<bool> {
    match read_entry(base, &entry.path) {
        Ok(read) => Ok(read.kind == entry.kind
            && read.mode == entry.mode
            && read.target == entry.target
            && read.bytes.as_deref().map(sha256_hex) == entry.sha256),
        Err(_) => Ok(false),
    }
}

/// Compute the exact write/delete scope for restoring `manifest` over the
/// working tree at `target_root`, and the target's current identity under
/// the manifest's own frozen scope (the compare-first token the caller must
/// have acknowledged before apply).
pub fn plan_over_working_tree(
    manifest: &SnapshotManifest,
    target_root: &Path,
) -> Result<OverWorkPlan> {
    if !manifest.is_complete() {
        return Err(CoreError::Integrity(
            "an incomplete capture cannot be restored over a working tree".to_string(),
        ));
    }
    let git = trusted_git()?;
    let Some(toplevel) = git_optional(&git, target_root, &["rev-parse", "--show-toplevel"])? else {
        return Err(CoreError::Config(format!(
            "{} is not a git worktree — over-working-tree restore compares first, which \
             needs a live worktree; use the default safe-directory restore instead",
            target_root.display()
        )));
    };
    let base = PathBuf::from(&toplevel);
    let base = fs::canonicalize(&base).map_err(|e| CoreError::io(&base, e))?;
    let git_dir = resolve_git_path(&git, &base, &["rev-parse", "--absolute-git-dir"])?;
    let common_dir = resolve_git_path(&git, &base, &["rev-parse", "--git-common-dir"])?;
    if git_dir != common_dir {
        return Err(CoreError::Config(
            "the target is a linked worktree — over-working-tree restore v1 speaks for \
             primary worktrees only"
                .to_string(),
        ));
    }
    let scope = Scope {
        include_untracked: manifest.include_untracked,
        exclusions: manifest.exclusions.clone(),
    };
    let observed = match observe(&base, scope)? {
        Observation::Complete(fp) => *fp,
        Observation::Incomplete { reason, .. } => {
            return Err(CoreError::Integrity(format!(
                "the target tree is moving ({reason}) — nothing can be acknowledged, \
                 nothing will be touched"
            )))
        }
    };

    // The target's in-scope universe: tracked files minus worktree
    // deletions, plus in-scope untracked, plus the repository metadata tree.
    let ls = git_output(&git, &base, &["ls-files", "-s", "-z", "--full-name"])?;
    let deleted: BTreeSet<&str> = observed.doc().deleted.iter().map(String::as_str).collect();
    let mut current: BTreeSet<String> = parse_ls_files(&ls)?
        .into_iter()
        .filter(|(mode, _, _)| mode != "160000")
        .map(|(_, _, path)| path)
        .filter(|p| !deleted.contains(p.as_str()))
        .collect();
    for u in &observed.doc().untracked {
        current.insert(u.path.clone());
    }
    let mut current_display: BTreeSet<String> = current;
    for rel in walk_git_dir(&git_dir)? {
        current_display.insert(format!(".git/{rel}"));
    }

    let mut manifest_paths: BTreeSet<String> = BTreeSet::new();
    let mut writes: Vec<PlannedWrite> = Vec::new();
    for entry in &manifest.git {
        safe_rel(&entry.path)?;
        let display = format!(".git/{}", entry.path);
        manifest_paths.insert(display.clone());
        if !manifest_entry_matches(&git_dir, entry)? {
            writes.push(PlannedWrite {
                path: display,
                bytes: entry.bytes,
            });
        }
    }
    for entry in &manifest.worktree {
        safe_rel(&entry.path)?;
        manifest_paths.insert(entry.path.clone());
        if !manifest_entry_matches(&base, entry)? {
            writes.push(PlannedWrite {
                path: entry.path.clone(),
                bytes: entry.bytes,
            });
        }
    }
    let deletes: Vec<String> = current_display
        .into_iter()
        .filter(|p| !manifest_paths.contains(p))
        .collect();
    Ok(OverWorkPlan {
        writes,
        deletes,
        target_identity: observed.identity().to_string(),
    })
}

/// Apply a previously previewed over-working-tree plan. Touches exactly the
/// plan's writes and deletes — nothing else — and refuses when the target
/// tree no longer matches the identity the plan was computed against.
pub fn apply_over_working_tree(
    store: &ProtectedStore,
    manifest: &SnapshotManifest,
    target_root: &Path,
    plan: &OverWorkPlan,
) -> Result<RestoreReport> {
    let git = trusted_git()?;
    let Some(toplevel) = git_optional(&git, target_root, &["rev-parse", "--show-toplevel"])? else {
        return Err(CoreError::Config(
            "the target stopped being a git worktree after planning — refused".to_string(),
        ));
    };
    let base = PathBuf::from(&toplevel);
    let base = fs::canonicalize(&base).map_err(|e| CoreError::io(&base, e))?;
    let git_dir = resolve_git_path(&git, &base, &["rev-parse", "--absolute-git-dir"])?;
    let scope = Scope {
        include_untracked: manifest.include_untracked,
        exclusions: manifest.exclusions.clone(),
    };
    match observe(&base, scope)? {
        Observation::Complete(fp) if fp.identity() == plan.target_identity => {}
        Observation::Complete(fp) => {
            return Err(CoreError::Integrity(format!(
                "the target tree changed after the preview (now {}) — the acknowledged \
                 state no longer exists; re-plan and re-acknowledge",
                fp.identity()
            )))
        }
        Observation::Incomplete { reason, .. } => {
            return Err(CoreError::Integrity(format!(
                "the target tree is moving ({reason}) — refused"
            )))
        }
    }

    let entries: BTreeMap<String, &ManifestEntry> = manifest
        .git
        .iter()
        .map(|e| (format!(".git/{}", e.path), e))
        .chain(manifest.worktree.iter().map(|e| (e.path.clone(), e)))
        .collect();

    // Deletes first, then writes: exactly the previewed set, nothing else.
    for display in &plan.deletes {
        let rel = safe_rel(display)?;
        let full = base.join(rel);
        assert_no_symlink_ancestor(&base, &full)?;
        match fs::symlink_metadata(&full) {
            Ok(meta) => {
                if meta.file_type().is_dir() && !meta.file_type().is_symlink() {
                    fs::remove_dir(&full).map_err(|e| CoreError::io(&full, e))?;
                } else {
                    fs::remove_file(&full).map_err(|e| CoreError::io(&full, e))?;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(CoreError::io(&full, e)),
        }
    }
    let mut bytes_written = 0u64;
    let mut worktree_files = 0u64;
    let mut git_files = 0u64;
    for w in &plan.writes {
        let entry = entries.get(&w.path).ok_or_else(|| {
            CoreError::Integrity(format!(
                "planned write {} names no manifest entry — the plan is not this manifest's",
                w.path
            ))
        })?;
        let base_dir = if w.path.starts_with(".git/") {
            git_files += 1;
            git_dir.clone()
        } else {
            worktree_files += 1;
            base.clone()
        };
        bytes_written += materialize_entry(store, &base, &base_dir, entry)?;
    }
    Ok(RestoreReport {
        worktree_files,
        git_files,
        bytes_written,
    })
}

/* ── the §11 observation writers ────────────────────────────────────── */

/// What recording produced. Recording abstains — loudly, with the reason —
/// when no repository identity was ever observed for this snapshot, because
/// the envelope's `repository_identity` is NOT NULL and a placeholder would
/// be a lie the chain preserves forever (§7: unknown stays unknown).
#[derive(Debug)]
pub enum SnapshotRecordOutcome {
    Recorded(Box<AppendOutcome>),
    NoRepositoryIdentity { reason: String },
}

fn snapshot_receipt(pref: &ProtectedRef, snapshot_row_id: &str) -> SourceReceipt {
    let mut receipt = SourceReceipt::new(ReceiptTerminal::SnapshotManifest {
        manifest_sha256: pref.manifest_digest.clone(),
    });
    receipt.derived_refs.push(DerivedRef {
        table: "workspace_snapshots".to_string(),
        id: snapshot_row_id.to_string(),
    });
    receipt.extra.insert(
        "manifest_blob_sha256".to_string(),
        pref.manifest_blob_sha256.clone().into(),
    );
    receipt.extra.insert(
        "store_version".to_string(),
        pref.store_version.clone().into(),
    );
    receipt
}

/// Append `SNAPSHOT_CREATED` for a stored protected snapshot. The receipt
/// terminates at the local snapshot manifest by content digest; the C2 row
/// rides along as a derived reference.
pub fn record_snapshot_created(
    ledger: &Ledger,
    snapshot_row_id: &str,
    pref: &ProtectedRef,
    workspace_identity: Option<&str>,
    observed_at: &str,
    source_session: &str,
) -> Result<SnapshotRecordOutcome> {
    let Some(repository_identity) = pref.repository_identity.as_deref() else {
        return Ok(SnapshotRecordOutcome::NoRepositoryIdentity {
            reason: "no stable repository identity was observed at capture — the snapshot \
                     stands, and the observation is not recorded rather than recorded \
                     against a guess"
                .to_string(),
        });
    };
    let mut receipt = snapshot_receipt(pref, snapshot_row_id);
    receipt
        .extra
        .insert("new_blobs".to_string(), pref.new_blobs.into());
    let outcome = append(
        ledger,
        &NewObservation {
            observation_type: ObservationType::SnapshotCreated,
            source_adapter: "thruline",
            source_session,
            repository_identity,
            workspace_identity,
            observed_at,
            completeness: if pref.complete {
                Completeness::Complete
            } else {
                Completeness::Incomplete
            },
            receipt: &receipt,
        },
    )?;
    Ok(SnapshotRecordOutcome::Recorded(Box::new(outcome)))
}

/// Append `SNAPSHOT_VERIFIED` after a verify pass in which every referenced
/// blob and the manifest were re-hashed. Only ever called on success — a
/// failed verification is a loud error, not an observation of health.
pub fn record_snapshot_verified(
    ledger: &Ledger,
    snapshot_row_id: &str,
    pref: &ProtectedRef,
    blobs_checked: u64,
    observed_at: &str,
    source_session: &str,
) -> Result<SnapshotRecordOutcome> {
    let Some(repository_identity) = pref.repository_identity.as_deref() else {
        return Ok(SnapshotRecordOutcome::NoRepositoryIdentity {
            reason: "no stable repository identity was observed at capture — the \
                     verification stands on its own printed result"
                .to_string(),
        });
    };
    let mut receipt = snapshot_receipt(pref, snapshot_row_id);
    receipt
        .extra
        .insert("blobs_checked".to_string(), blobs_checked.into());
    let outcome = append(
        ledger,
        &NewObservation {
            observation_type: ObservationType::SnapshotVerified,
            source_adapter: "thruline",
            source_session,
            repository_identity,
            workspace_identity: None,
            observed_at,
            completeness: Completeness::Complete,
            receipt: &receipt,
        },
    )?;
    Ok(SnapshotRecordOutcome::Recorded(Box::new(outcome)))
}

/// The applicability question, answered the Stage-4 way: observe the live
/// workspace under the manifest's own frozen scope and compare identity
/// strings — no second comparator exists. `None` means the observation was
/// incomplete (the tree is moving; nothing can be claimed).
pub fn snapshot_applies(manifest: &SnapshotManifest, root: &Path) -> Result<Option<bool>> {
    let scope = Scope {
        include_untracked: manifest.include_untracked,
        exclusions: manifest.exclusions.clone(),
    };
    Ok(match observe(root, scope)? {
        Observation::Complete(fp) => Some(fp.identity() == manifest.source_identity),
        Observation::Incomplete { .. } => None,
    })
}

/// Re-export for callers that hold a fingerprint and want its identity
/// string without re-observing (the CLI's protect path).
pub fn identity_of_fingerprint(fp: &WorkspaceFingerprint) -> &str {
    fp.identity()
}
