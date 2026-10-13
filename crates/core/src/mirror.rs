//! Private, immutable source-generation mirrors.
//!
//! A sealed mirror proves which bytes were captured. It has deliberately zero
//! ledger authority until [`crate::ledger::Ledger`] accepts the corresponding
//! generation in its atomic database transaction (A71).

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ulid::Ulid;

use crate::chain::{sha256_hex, to_hex};
use crate::error::{CoreError, Result};

const COPY_BUFFER_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorIdentity {
    pub owner_key: String,
    pub source_project_key: String,
    pub source_session_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirrorSeal {
    pub schema_version: u32,
    pub owner_key: String,
    pub source_project_key: String,
    pub source_session_key: String,
    pub payload_sha256: String,
    pub payload_bytes: u64,
    pub captured_at_unix_millis: u64,
    pub source_modified_unix_nanos: Option<u64>,
    pub source_device: Option<u64>,
    pub source_inode: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedMirror {
    pub generation_key: String,
    pub relative_payload: String,
    pub relative_seal: String,
    pub payload_sha256: String,
    pub payload_bytes: u64,
    pub seal_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineageDisposition {
    Initial,
    Identical,
    PrefixSuccessor,
    Truncated,
    Divergent,
}

#[derive(Debug, Clone)]
pub struct MirrorStore {
    root: PathBuf,
}

/// Process-scoped mirror lease backed by SQLite's operating-system lock. A
/// crashed process releases it automatically; no stale bypass token exists.
pub struct MirrorLease {
    _connection: Connection,
    root: PathBuf,
}

/// C27 — a capture permit borrowed from a held [`MirrorLease`], so worker
/// threads can capture while the one thread that owns the lease holds it.
/// The lease itself cannot cross threads (its SQLite connection is not
/// `Sync`); the permit can, and its lifetime keeps the lease alive and
/// unreleased for as long as any permit exists — so no recovery pass (which
/// runs only when a lease is acquired) can sweep a capture in flight.
#[derive(Clone, Copy)]
pub struct CapturePermit<'lease> {
    root: &'lease Path,
}

impl MirrorLease {
    pub fn permit(&self) -> CapturePermit<'_> {
        CapturePermit { root: &self.root }
    }
}

impl MirrorStore {
    /// Open the private namespace owned by one persistent ledger authority.
    /// Multiple databases may safely share the same configured base without
    /// gaining recovery authority over one another's accepted generations.
    pub fn open_namespaced(base: impl AsRef<Path>, authority_key: &str) -> Result<Self> {
        if authority_key.is_empty()
            || !authority_key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        {
            return Err(CoreError::Integrity(
                "mirror authority key is not a safe path component".to_string(),
            ));
        }
        let base = base.as_ref();
        reject_repository_destination(base)?;
        create_or_verify_private_base(base)?;
        let ledgers = base.join("ledgers");
        create_private_dir(&ledgers)?;
        Self::open(ledgers.join(authority_key))
    }

    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        reject_repository_destination(&root)?;
        create_private_dir(&root)?;
        let root = fs::canonicalize(&root).map_err(|e| CoreError::io(&root, e))?;
        reject_repository_destination(&root)?;
        for child in [
            "staging",
            "generations",
            "orphans",
            "rejected",
            "indexes",
            "spools",
        ] {
            create_private_dir(&root.join(child))?;
        }
        Ok(Self { root })
    }

    /// Open an existing store without creating directories or performing
    /// orphan recovery. Ledger verification is observational.
    pub fn open_for_verification(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        reject_repository_destination(&root)?;
        let metadata = fs::symlink_metadata(&root).map_err(|e| CoreError::io(&root, e))?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            return Err(CoreError::Integrity(
                "private mirror root is not a non-symlink directory".to_string(),
            ));
        }
        let root = fs::canonicalize(&root).map_err(|e| CoreError::io(&root, e))?;
        reject_repository_destination(&root)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn acquire_ingest_lease(&self) -> Result<MirrorLease> {
        self.verify_store_layout()?;
        let path = self.root.join("coordination.sqlite");
        create_private_regular_if_absent(&path)?;
        let connection = Connection::open(&path)?;
        set_file_mode(&path, 0o600)?;
        connection.busy_timeout(Duration::from_secs(30))?;
        connection.execute_batch(
            "PRAGMA journal_mode=DELETE;
             PRAGMA synchronous=FULL;
             CREATE TABLE IF NOT EXISTS lease_marker(singleton INTEGER PRIMARY KEY CHECK(singleton=1));
             BEGIN IMMEDIATE;
             INSERT OR IGNORE INTO lease_marker(singleton) VALUES(1);",
        )?;
        self.recover_staging()?;
        self.recover_ephemeral()?;
        Ok(MirrorLease {
            _connection: connection,
            root: self.root.clone(),
        })
    }

    pub fn resolve_payload(&self, sealed: &SealedMirror) -> Result<PathBuf> {
        self.resolve_relative(&sealed.relative_payload)
    }

    pub fn resolve_seal(&self, sealed: &SealedMirror) -> Result<PathBuf> {
        self.resolve_relative(&sealed.relative_seal)
    }

    /// Capture exact source bytes, verify a second source pass, then publish an
    /// immutable mirror. Publication is not acceptance and grants no authority.
    pub fn capture(
        &self,
        lease: &MirrorLease,
        source: &Path,
        identity: &MirrorIdentity,
    ) -> Result<SealedMirror> {
        if lease.root != self.root {
            return Err(CoreError::Integrity(
                "mirror lease belongs to another store".to_string(),
            ));
        }
        self.capture_inner(source, identity, || Ok(()))
    }

    /// [`MirrorStore::capture`] under a [`CapturePermit`] of a held lease.
    pub fn capture_permitted(
        &self,
        permit: CapturePermit<'_>,
        source: &Path,
        identity: &MirrorIdentity,
    ) -> Result<SealedMirror> {
        if permit.root != self.root {
            return Err(CoreError::Integrity(
                "mirror lease belongs to another store".to_string(),
            ));
        }
        self.capture_inner(source, identity, || Ok(()))
    }

    fn capture_inner(
        &self,
        source: &Path,
        identity: &MirrorIdentity,
        after_copy: impl FnOnce() -> Result<()>,
    ) -> Result<SealedMirror> {
        self.verify_container("staging")?;
        self.verify_container("generations")?;
        reject_source_alias(source, &self.root)?;
        let before = source_metadata(source)?;
        let capture_id = Ulid::generate().to_string();
        let staging = self.root.join("staging").join(&capture_id);
        create_private_dir(&staging)?;
        let payload_part = staging.join("payload.part");

        let (payload_bytes, payload_sha256) = copy_exact(source, &payload_part)?;
        after_copy()?;
        let after_copy = source_metadata(source)?;
        if before != after_copy {
            return Err(CoreError::Integrity(
                "source changed while its candidate generation was captured".to_string(),
            ));
        }
        let (source_bytes_again, source_sha_again) = hash_file(source)?;
        let after_verify = source_metadata(source)?;
        if after_copy != after_verify
            || source_bytes_again != payload_bytes
            || source_sha_again != payload_sha256
        {
            return Err(CoreError::Integrity(
                "source changed while its candidate generation was verified".to_string(),
            ));
        }
        let (mirror_bytes, mirror_sha) = hash_file(&payload_part)?;
        if mirror_bytes != payload_bytes || mirror_sha != payload_sha256 {
            return Err(CoreError::Integrity(
                "captured mirror failed independent size/digest verification".to_string(),
            ));
        }

        let owner_key = opaque_key("owner", &identity.owner_key);
        let project_key = opaque_key("project", &identity.source_project_key);
        let session_key = opaque_key("session", &identity.source_session_key);
        let generation_key = opaque_key(
            "generation",
            &format!("{owner_key}:{project_key}:{session_key}:{payload_sha256}"),
        );
        let seal = MirrorSeal {
            schema_version: 2,
            owner_key: owner_key.clone(),
            source_project_key: project_key.clone(),
            source_session_key: session_key.clone(),
            payload_sha256: payload_sha256.clone(),
            payload_bytes,
            captured_at_unix_millis: capture_unix_millis()?,
            source_modified_unix_nanos: before.modified_nanos,
            source_device: before.device,
            source_inode: before.inode,
        };
        let seal_bytes = serde_json::to_vec(&seal)?;
        let seal_part = staging.join("seal.part");
        write_exclusive(&seal_part, &seal_bytes)?;

        set_file_mode(&payload_part, 0o400)?;
        set_file_mode(&seal_part, 0o400)?;
        sync_dir(&staging)?;

        let relative_dir = PathBuf::from("generations")
            .join(&project_key)
            .join(&session_key)
            .join(&generation_key);
        let final_dir = self.root.join(&relative_dir);
        if let Some(parent) = final_dir.parent() {
            create_private_tree(parent, &self.root)?;
        }
        match fs::symlink_metadata(&final_dir) {
            Ok(_) => {
                verify_private_directory(&final_dir)?;
                // The exact content-addressed generation already exists. This
                // capture is an equivalent observation, not a new orphan or a
                // new authority record; remove only its private staging directory.
                fs::remove_dir_all(&staging).map_err(|e| CoreError::io(&staging, e))?;
                sync_parent(&staging)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::rename(&staging, &final_dir).map_err(|e| CoreError::io(&staging, e))?;
                set_file_mode(&final_dir, 0o500)?;
                sync_parent(&final_dir)?;
            }
            Err(error) => return Err(CoreError::io(&final_dir, error)),
        }

        let relative_payload = relative_dir.join("payload.part");
        let relative_seal = relative_dir.join("seal.part");
        let actual_seal_path = self.resolve_relative(&path_text(&relative_seal)?)?;
        let actual_seal_bytes =
            fs::read(&actual_seal_path).map_err(|e| CoreError::io(&actual_seal_path, e))?;
        let actual_seal: MirrorSeal = serde_json::from_slice(&actual_seal_bytes)?;
        if actual_seal.owner_key != owner_key
            || actual_seal.source_project_key != project_key
            || actual_seal.source_session_key != session_key
            || actual_seal.payload_sha256 != payload_sha256
            || actual_seal.payload_bytes != payload_bytes
        {
            return Err(CoreError::Integrity(
                "existing sealed generation has different capture identity".to_string(),
            ));
        }
        let sealed = SealedMirror {
            generation_key,
            relative_payload: path_text(&relative_payload)?,
            relative_seal: path_text(&relative_seal)?,
            payload_sha256: actual_seal.payload_sha256,
            payload_bytes: actual_seal.payload_bytes,
            seal_sha256: sha256_hex(&actual_seal_bytes),
        };
        self.verify(&sealed)?;
        Ok(sealed)
    }

    /// C27 — whether `source` is, by its size, modification time, device and
    /// inode, exactly the file this accepted generation's seal recorded when
    /// it was captured. Reads the seal (a few hundred bytes, checked against
    /// its accepted digest) and stats the source; it never opens the source
    /// and never reads the payload. Any doubt — a missing or unreadable seal,
    /// a seal digest that does not match, a source that cannot be stat'd, a
    /// platform without a modification time — answers `false`, so the caller
    /// takes the full capture path, which reports the problem loudly.
    pub fn source_unchanged_since(&self, source: &Path, sealed: &SealedMirror) -> bool {
        let Some(seal) = self.read_seal(sealed) else {
            return false;
        };
        let Ok(now) = source_metadata(source) else {
            return false;
        };
        seal.payload_bytes == sealed.payload_bytes
            && now.len == seal.payload_bytes
            && now.modified_nanos.is_some()
            && now.modified_nanos == seal.source_modified_unix_nanos
            && now.device.is_some()
            && now.device == seal.source_device
            && now.inode.is_some()
            && now.inode == seal.source_inode
    }

    /// C27 — whether `source` has DEFINITELY moved since this capture: the
    /// seal reads and checks, and one of the exact fields
    /// [`MirrorStore::verify_live_candidate`] compares (size, modification
    /// time, device, inode) differs from it now. `false` on any doubt, so a
    /// caller that skips work on `true` only ever skips work that check would
    /// refuse.
    pub fn source_changed_since(&self, source: &Path, sealed: &SealedMirror) -> bool {
        match self.read_seal(sealed) {
            Some(seal) => Self::source_moved_from(source, &seal),
            None => false,
        }
    }

    /// [`MirrorStore::source_changed_since`] against a seal already read:
    /// a `stat` and four comparisons, cheap enough to ask during a mapping.
    pub fn source_moved_from(source: &Path, seal: &MirrorSeal) -> bool {
        let Ok(now) = source_metadata(source) else {
            return false;
        };
        now.len != seal.payload_bytes
            || now.modified_nanos != seal.source_modified_unix_nanos
            || now.device != seal.source_device
            || now.inode != seal.source_inode
    }

    /// The seal of an accepted generation, read and checked against its
    /// accepted digest — the payload is not read. `None` on any doubt.
    pub fn read_seal(&self, sealed: &SealedMirror) -> Option<MirrorSeal> {
        let seal_path = self.resolve_seal(sealed).ok()?;
        verify_private_regular(&seal_path, 0o400).ok()?;
        let seal_bytes = fs::read(&seal_path).ok()?;
        if sha256_hex(&seal_bytes) != sealed.seal_sha256 {
            return None;
        }
        serde_json::from_slice::<MirrorSeal>(&seal_bytes).ok()
    }

    pub fn verify(&self, sealed: &SealedMirror) -> Result<MirrorSeal> {
        let payload = self.resolve_payload(sealed)?;
        let seal_path = self.resolve_seal(sealed)?;
        verify_private_regular(&payload, 0o400)?;
        verify_private_regular(&seal_path, 0o400)?;
        let seal_bytes = fs::read(&seal_path).map_err(|e| CoreError::io(&seal_path, e))?;
        if sha256_hex(&seal_bytes) != sealed.seal_sha256 {
            return Err(CoreError::Integrity(
                "mirror seal digest does not match its accepted receipt".to_string(),
            ));
        }
        let seal: MirrorSeal = serde_json::from_slice(&seal_bytes)?;
        let expected_generation_key = opaque_key(
            "generation",
            &format!(
                "{}:{}:{}:{}",
                seal.owner_key,
                seal.source_project_key,
                seal.source_session_key,
                seal.payload_sha256
            ),
        );
        let (bytes, sha) = hash_file(&payload)?;
        if seal.schema_version != 2
            || seal.captured_at_unix_millis == 0
            || expected_generation_key != sealed.generation_key
            || bytes != sealed.payload_bytes
            || sha != sealed.payload_sha256
            || bytes != seal.payload_bytes
            || sha != seal.payload_sha256
        {
            return Err(CoreError::Integrity(
                "sealed mirror payload does not match its accepted receipt".to_string(),
            ));
        }
        Ok(seal)
    }

    /// Recheck the mutable discovery source immediately before the candidate's
    /// database transaction. This never changes prior accepted authority.
    pub fn verify_live_candidate(&self, source: &Path, sealed: &SealedMirror) -> Result<()> {
        let seal = self.verify(sealed)?;
        let metadata = source_metadata(source)?;
        let (bytes, sha256) = hash_file(source)?;
        if bytes != sealed.payload_bytes
            || sha256 != sealed.payload_sha256
            || metadata.len != seal.payload_bytes
            || metadata.modified_nanos != seal.source_modified_unix_nanos
            || metadata.device != seal.source_device
            || metadata.inode != seal.source_inode
        {
            return Err(CoreError::Integrity(
                "live source changed after candidate mirror capture".to_string(),
            ));
        }
        Ok(())
    }

    pub fn classify(
        &self,
        candidate: &SealedMirror,
        accepted: Option<&SealedMirror>,
    ) -> Result<LineageDisposition> {
        self.verify(candidate)?;
        let Some(accepted) = accepted else {
            return Ok(LineageDisposition::Initial);
        };
        self.verify(accepted)?;
        if candidate.payload_bytes == accepted.payload_bytes {
            return Ok(if candidate.payload_sha256 == accepted.payload_sha256 {
                LineageDisposition::Identical
            } else {
                LineageDisposition::Divergent
            });
        }
        if candidate.payload_bytes < accepted.payload_bytes {
            return Ok(LineageDisposition::Truncated);
        }
        let candidate_path = self.resolve_payload(candidate)?;
        let prefix_sha = hash_prefix(&candidate_path, accepted.payload_bytes)?;
        Ok(if prefix_sha == accepted.payload_sha256 {
            LineageDisposition::PrefixSuccessor
        } else {
            LineageDisposition::Divergent
        })
    }

    /// Preserve a rejected sealed candidate for private diagnostics. This
    /// operation never updates ledger authority.
    pub fn retain_rejected(&self, sealed: &SealedMirror) -> Result<()> {
        self.verify_container("generations")?;
        let rejected = self.verify_container("rejected")?;
        self.verify(sealed)?;
        record_generation_observation(
            &rejected,
            &sealed.generation_key,
            "rejected_candidate_observation",
        )?;
        Ok(())
    }

    /// Record sealed generations not referenced by this observing ledger.
    ///
    /// Absence from one database is never proof that a generation is globally
    /// unaccepted: the database may be a copied/older ledger while another
    /// copy already references these exact sealed bytes. Recovery therefore
    /// appends a private marker but never moves, deletes, or mutates a sealed
    /// generation. Directory placement grants zero authority; only a committed
    /// `session_generations` row does.
    pub fn recover_unaccepted_generations(
        &self,
        lease: &MirrorLease,
        accepted_generation_keys: &BTreeSet<String>,
    ) -> Result<u64> {
        if lease.root != self.root {
            return Err(CoreError::Integrity(
                "mirror lease belongs to another store".to_string(),
            ));
        }
        let generations = self.verify_container("generations")?;
        let orphans = self.verify_container("orphans")?;
        let mut recovered = 0u64;
        for project in read_dir_paths(&generations)? {
            verify_private_directory(&project)?;
            for session in read_dir_paths(&project)? {
                verify_private_directory(&session)?;
                for generation in read_dir_paths(&session)? {
                    verify_private_directory(&generation)?;
                    let key = generation
                        .file_name()
                        .and_then(|value| value.to_str())
                        .ok_or_else(|| {
                            CoreError::Integrity(
                                "sealed generation directory has no safe key".to_string(),
                            )
                        })?;
                    if accepted_generation_keys.contains(key) {
                        continue;
                    }
                    verify_generic_generation_directory(&generation)?;
                    if record_generation_observation(
                        &orphans,
                        key,
                        "unreferenced_by_observing_ledger",
                    )? {
                        recovered = recovered.checked_add(1).ok_or_else(|| {
                            CoreError::Integrity("orphan recovery count overflow".to_string())
                        })?;
                    }
                }
            }
        }
        sync_dir(&generations)?;
        Ok(recovered)
    }

    fn resolve_relative(&self, relative: &str) -> Result<PathBuf> {
        let rel = Path::new(relative);
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err(CoreError::Integrity(
                "mirror receipt contains a non-contained path".to_string(),
            ));
        }
        let resolved = self.root.join(rel);
        verify_contained_path(&self.root, &resolved)?;
        Ok(resolved)
    }

    fn recover_staging(&self) -> Result<()> {
        let staging = self.verify_container("staging")?;
        self.verify_container("orphans")?;
        let entries = read_dir_paths(&staging)?;
        for path in entries {
            let Some(name) = path.file_name() else {
                continue;
            };
            let destination = self.root.join("orphans").join(name);
            if destination.exists() {
                return Err(CoreError::Integrity(
                    "mirror recovery found a duplicate orphan capture id".to_string(),
                ));
            }
            fs::rename(&path, &destination).map_err(|e| CoreError::io(&path, e))?;
            set_file_mode(&destination, 0o500)?;
            sync_parent(&path)?;
            sync_parent(&destination)?;
        }
        Ok(())
    }

    fn recover_ephemeral(&self) -> Result<()> {
        for name in ["indexes", "spools"] {
            let directory = self.verify_container(name)?;
            let entries = read_dir_paths(&directory)?;
            for path in entries {
                let metadata =
                    fs::symlink_metadata(&path).map_err(|error| CoreError::io(&path, error))?;
                if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                    return Err(CoreError::Integrity(
                        "mirror ephemeral recovery found a non-regular artifact".to_string(),
                    ));
                }
                fs::remove_file(&path).map_err(|error| CoreError::io(&path, error))?;
            }
            sync_dir(&directory)?;
        }
        Ok(())
    }

    fn verify_store_layout(&self) -> Result<()> {
        verify_private_directory(&self.root)?;
        for name in [
            "staging",
            "generations",
            "orphans",
            "rejected",
            "indexes",
            "spools",
        ] {
            self.verify_container(name)?;
        }
        Ok(())
    }

    fn verify_container(&self, name: &str) -> Result<PathBuf> {
        let allowed = matches!(
            name,
            "staging" | "generations" | "orphans" | "rejected" | "indexes" | "spools"
        );
        if !allowed {
            return Err(CoreError::Integrity(
                "unknown private mirror container".to_string(),
            ));
        }
        let path = self.root.join(name);
        verify_contained_path(&self.root, &path)?;
        verify_private_directory(&path)?;
        Ok(path)
    }
}

fn opaque_key(domain: &str, value: &str) -> String {
    let mut h = Sha256::new();
    for field in [domain.as_bytes(), value.as_bytes()] {
        h.update((field.len() as u64).to_le_bytes());
        h.update(field);
    }
    to_hex(&h.finalize())
}

/// Shared private-store discipline (A208): the protected snapshot store
/// reuses these exact helpers rather than copying them — one implementation
/// of "never inside a repository", "0700/0600/0400 or refuse", and
/// "fsync what you rename". Visibility is the only thing that changed.
pub(crate) fn reject_repository_destination(root: &Path) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|e| CoreError::io(".", e))?;
    let absolute = if root.is_absolute() {
        root.to_path_buf()
    } else {
        cwd.join(root)
    };
    let repository = cwd
        .ancestors()
        .find(|ancestor| ancestor.join(".git").exists());
    let destination = canonicalize_destination(&absolute)?;
    let destination_repository = destination
        .ancestors()
        .any(|ancestor| ancestor.join(".git").exists());
    let repository = repository
        .map(fs::canonicalize)
        .transpose()
        .map_err(|e| CoreError::io(&cwd, e))?;
    if destination_repository
        || repository.is_some_and(|repository| destination.starts_with(repository))
    {
        return Err(CoreError::Config(
            "private mirror root must be outside the current repository".to_string(),
        ));
    }
    Ok(())
}

fn canonicalize_destination(path: &Path) -> Result<PathBuf> {
    let mut existing = path.to_path_buf();
    let mut suffix = Vec::<OsString>::new();
    loop {
        match fs::symlink_metadata(&existing) {
            Ok(_) => {
                let mut canonical =
                    fs::canonicalize(&existing).map_err(|e| CoreError::io(&existing, e))?;
                for component in suffix.iter().rev() {
                    canonical.push(component);
                }
                return Ok(canonical);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = existing.file_name().ok_or_else(|| {
                    CoreError::Integrity(
                        "private mirror destination has no existing ancestor".to_string(),
                    )
                })?;
                suffix.push(name.to_os_string());
                if !existing.pop() {
                    return Err(CoreError::Integrity(
                        "private mirror destination escaped its filesystem root".to_string(),
                    ));
                }
            }
            Err(error) => return Err(CoreError::io(&existing, error)),
        }
    }
}

fn reject_source_alias(source: &Path, mirror_root: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(source).map_err(|e| CoreError::io(source, e))?;
    if !meta.file_type().is_file() || meta.file_type().is_symlink() {
        return Err(CoreError::Integrity(
            "source candidate is not a regular non-symlink file".to_string(),
        ));
    }
    let canonical_source = fs::canonicalize(source).map_err(|e| CoreError::io(source, e))?;
    let canonical_root =
        fs::canonicalize(mirror_root).map_err(|e| CoreError::io(mirror_root, e))?;
    if canonical_source.starts_with(&canonical_root) {
        return Err(CoreError::Integrity(
            "live source cannot be inside the private mirror store".to_string(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceMetadata {
    len: u64,
    modified_nanos: Option<u64>,
    device: Option<u64>,
    inode: Option<u64>,
}

fn source_metadata(path: &Path) -> Result<SourceMetadata> {
    let meta = fs::metadata(path).map_err(|e| CoreError::io(path, e))?;
    #[cfg(unix)]
    let (device, inode) = {
        use std::os::unix::fs::MetadataExt;
        (Some(meta.dev()), Some(meta.ino()))
    };
    #[cfg(not(unix))]
    let (device, inode) = (None, None);
    Ok(SourceMetadata {
        len: meta.len(),
        modified_nanos: meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|d| u64::try_from(d.as_nanos()).ok()),
        device,
        inode,
    })
}

fn capture_unix_millis() -> Result<u64> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CoreError::Integrity("system clock predates Unix epoch".to_string()))?
        .as_millis();
    u64::try_from(millis)
        .map_err(|_| CoreError::Integrity("capture timestamp exceeds u64".to_string()))
}

fn copy_exact(source: &Path, destination: &Path) -> Result<(u64, String)> {
    let mut input = File::open(source).map_err(|e| CoreError::io(source, e))?;
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut output = options
        .open(destination)
        .map_err(|e| CoreError::io(destination, e))?;
    set_file_mode(destination, 0o600)?;
    let mut h = Sha256::new();
    let mut total = 0u64;
    let mut buf = vec![0u8; COPY_BUFFER_BYTES];
    loop {
        let n = input.read(&mut buf).map_err(|e| CoreError::io(source, e))?;
        if n == 0 {
            break;
        }
        output
            .write_all(&buf[..n])
            .map_err(|e| CoreError::io(destination, e))?;
        h.update(&buf[..n]);
        total += n as u64;
    }
    output
        .sync_all()
        .map_err(|e| CoreError::io(destination, e))?;
    Ok((total, to_hex(&h.finalize())))
}

fn hash_file(path: &Path) -> Result<(u64, String)> {
    let mut file = File::open(path).map_err(|e| CoreError::io(path, e))?;
    let mut h = Sha256::new();
    let mut total = 0u64;
    let mut buf = vec![0u8; COPY_BUFFER_BYTES];
    loop {
        let n = file.read(&mut buf).map_err(|e| CoreError::io(path, e))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        total += n as u64;
    }
    Ok((total, to_hex(&h.finalize())))
}

fn hash_prefix(path: &Path, bytes: u64) -> Result<String> {
    let mut file = File::open(path).map_err(|e| CoreError::io(path, e))?;
    let mut h = Sha256::new();
    let mut remaining = bytes;
    let mut buf = vec![0u8; COPY_BUFFER_BYTES];
    while remaining > 0 {
        let want = usize::try_from(remaining.min(buf.len() as u64)).unwrap_or(buf.len());
        let n = file
            .read(&mut buf[..want])
            .map_err(|e| CoreError::io(path, e))?;
        if n == 0 {
            return Err(CoreError::Integrity(
                "candidate ended before the accepted prefix".to_string(),
            ));
        }
        h.update(&buf[..n]);
        remaining -= n as u64;
    }
    Ok(to_hex(&h.finalize()))
}

pub(crate) fn write_exclusive(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| CoreError::io(path, e))?;
    set_file_mode(path, 0o600)?;
    file.write_all(bytes).map_err(|e| CoreError::io(path, e))?;
    file.sync_all().map_err(|e| CoreError::io(path, e))
}

fn create_private_tree(path: &Path, boundary: &Path) -> Result<()> {
    let relative = path.strip_prefix(boundary).map_err(|_| {
        CoreError::Integrity("mirror directory escaped its private root".to_string())
    })?;
    let mut current = boundary.to_path_buf();
    for component in relative.components() {
        current.push(component);
        create_private_dir(&current)?;
    }
    Ok(())
}

fn create_or_verify_private_base(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
                return Err(CoreError::Integrity(
                    "configured mirror root is not a non-symlink directory".to_string(),
                ));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o777 != 0o700 {
                    return Err(CoreError::Integrity(
                        "configured mirror root is not already private (0700)".to_string(),
                    ));
                }
            }
            #[cfg(not(unix))]
            {
                return Err(CoreError::Config(
                    "private mirror permissions are not implemented on this platform".to_string(),
                ));
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => create_private_dir(path),
        Err(error) => Err(CoreError::io(path, error)),
    }
}

pub(crate) fn create_private_dir(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            if !meta.file_type().is_dir() || meta.file_type().is_symlink() {
                return Err(CoreError::Integrity(
                    "private mirror path is not a non-symlink directory".to_string(),
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(path) {
                Ok(()) => sync_parent(path)?,
                // C27: two captures (worker threads of one load, or two
                // processes) may create the same project or session folder at
                // once. The loser finds it made, and checks it like any other.
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    verify_private_directory(path)?;
                }
                Err(error) => return Err(CoreError::io(path, error)),
            }
        }
        Err(error) => return Err(CoreError::io(path, error)),
    }
    set_file_mode(path, 0o700)
}

pub(crate) fn verify_private_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|e| CoreError::io(path, e))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(CoreError::Integrity(
            "private mirror path is not a non-symlink directory".to_string(),
        ));
    }
    Ok(())
}

fn verify_contained_path(root: &Path, path: &Path) -> Result<()> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| CoreError::Integrity("mirror receipt escaped its private root".to_string()))?;
    verify_private_directory(root)?;
    let mut current = root.to_path_buf();
    let components: Vec<_> = relative.components().collect();
    for (index, component) in components.iter().enumerate() {
        current.push(component.as_os_str());
        let metadata = fs::symlink_metadata(&current).map_err(|e| CoreError::io(&current, e))?;
        if metadata.file_type().is_symlink()
            || (index + 1 < components.len() && !metadata.file_type().is_dir())
        {
            return Err(CoreError::Integrity(
                "mirror receipt traverses a symlink or non-directory ancestor".to_string(),
            ));
        }
    }
    let canonical = fs::canonicalize(path).map_err(|e| CoreError::io(path, e))?;
    if !canonical.starts_with(root) {
        return Err(CoreError::Integrity(
            "mirror receipt resolves outside its private root".to_string(),
        ));
    }
    Ok(())
}

fn read_dir_paths(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory).map_err(|e| CoreError::io(directory, e))? {
        paths.push(entry.map_err(|e| CoreError::io(directory, e))?.path());
    }
    paths.sort();
    Ok(paths)
}

fn verify_generic_generation_directory(directory: &Path) -> Result<()> {
    verify_private_directory(directory)?;
    verify_private_regular(&directory.join("payload.part"), 0o400)?;
    verify_private_regular(&directory.join("seal.part"), 0o400)
}

fn record_generation_observation(container: &Path, key: &str, status: &str) -> Result<bool> {
    if key.is_empty()
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err(CoreError::Integrity(
            "generation observation key is not path-safe".to_string(),
        ));
    }
    verify_private_directory(container)?;
    let marker = container.join(format!("{key}.json"));
    let bytes = serde_json::to_vec(&serde_json::json!({
        "schema_version": 1,
        "generation_key": key,
        "status": status,
    }))?;
    match fs::symlink_metadata(&marker) {
        Ok(_) => {
            verify_private_regular(&marker, 0o400)?;
            let existing = fs::read(&marker).map_err(|error| CoreError::io(&marker, error))?;
            if existing != bytes {
                return Err(CoreError::Integrity(
                    "generation observation marker conflicts with existing receipt".to_string(),
                ));
            }
            Ok(false)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            write_exclusive(&marker, &bytes)?;
            set_file_mode(&marker, 0o400)?;
            sync_parent(&marker)?;
            Ok(true)
        }
        Err(error) => Err(CoreError::io(&marker, error)),
    }
}

fn create_private_regular_if_absent(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                return Err(CoreError::Integrity(
                    "private mirror database is not a regular non-symlink file".to_string(),
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut options = OpenOptions::new();
            options.create_new(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let file = options.open(path).map_err(|e| CoreError::io(path, e))?;
            file.sync_all().map_err(|e| CoreError::io(path, e))?;
            sync_parent(path)?;
        }
        Err(error) => return Err(CoreError::io(path, error)),
    }
    set_file_mode(path, 0o600)
}

pub(crate) fn verify_private_regular(path: &Path, expected_mode: u32) -> Result<()> {
    let meta = fs::symlink_metadata(path).map_err(|e| CoreError::io(path, e))?;
    if !meta.file_type().is_file() || meta.file_type().is_symlink() {
        return Err(CoreError::Integrity(
            "mirror receipt does not resolve to a regular sealed file".to_string(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() != 1 || meta.mode() & 0o777 != expected_mode {
            return Err(CoreError::Integrity(
                "sealed mirror permissions or link count are unsafe".to_string(),
            ));
        }
    }
    Ok(())
}

pub(crate) fn set_file_mode(path: &Path, mode: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .map_err(|e| CoreError::io(path, e))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
        return Err(CoreError::Config(
            "private mirror permissions are not implemented on this platform".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn sync_parent(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let dir = File::open(parent).map_err(|e| CoreError::io(parent, e))?;
    dir.sync_all().map_err(|e| CoreError::io(parent, e))
}

pub(crate) fn sync_dir(path: &Path) -> Result<()> {
    let dir = File::open(path).map_err(|e| CoreError::io(path, e))?;
    dir.sync_all().map_err(|e| CoreError::io(path, e))
}

fn path_text(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| CoreError::Integrity("mirror path is not valid UTF-8".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn store() -> (TempDir, MirrorStore) {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path().join("mirror");
        let store = MirrorStore::open(&root).expect("open mirror");
        (tmp, store)
    }

    fn identity() -> MirrorIdentity {
        MirrorIdentity {
            owner_key: "owner-a".to_string(),
            source_project_key: "project-a".to_string(),
            source_session_key: "session-a".to_string(),
        }
    }

    #[test]
    fn capture_is_sealed_and_classifies_exact_lineage() {
        let (tmp, store) = store();
        let lease = store.acquire_ingest_lease().expect("lease");
        let source = tmp.path().join("source.jsonl");
        fs::write(&source, b"one\ntwo\n").expect("write source");
        let first = store
            .capture(&lease, &source, &identity())
            .expect("capture first");
        assert_eq!(
            store.classify(&first, None).expect("classify"),
            LineageDisposition::Initial
        );

        let same = store
            .capture(&lease, &source, &identity())
            .expect("capture same");
        assert_eq!(
            store.classify(&same, Some(&first)).expect("same"),
            LineageDisposition::Identical
        );

        fs::write(&source, b"one\ntwo\nthree\n").expect("append source");
        let successor = store
            .capture(&lease, &source, &identity())
            .expect("capture successor");
        assert_eq!(
            store.classify(&successor, Some(&first)).expect("successor"),
            LineageDisposition::PrefixSuccessor
        );

        fs::write(&source, b"one\n").expect("truncate source");
        let truncated = store
            .capture(&lease, &source, &identity())
            .expect("capture truncated");
        assert_eq!(
            store.classify(&truncated, Some(&first)).expect("truncated"),
            LineageDisposition::Truncated
        );

        fs::write(&source, b"rewritten\nlonger\n").expect("rewrite source");
        let divergent = store
            .capture(&lease, &source, &identity())
            .expect("capture divergent");
        assert_eq!(
            store.classify(&divergent, Some(&first)).expect("divergent"),
            LineageDisposition::Divergent
        );
    }

    #[test]
    fn sealed_mirror_survives_live_source_disappearance() {
        let (tmp, store) = store();
        let lease = store.acquire_ingest_lease().expect("lease");
        let source = tmp.path().join("source.jsonl");
        fs::write(&source, b"captured\n").expect("write source");
        let sealed = store
            .capture(&lease, &source, &identity())
            .expect("capture");
        fs::remove_file(&source).expect("remove live source");
        let seal = store.verify(&sealed).expect("verify sealed mirror");
        assert_eq!(seal.payload_sha256, sealed.payload_sha256);
    }

    #[test]
    fn recovery_moves_unfinished_capture_to_orphans() {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path().join("mirror");
        let store = MirrorStore::open(&root).expect("initial open");
        let partial = root.join("staging").join("partial");
        create_private_dir(&partial).expect("partial dir");
        drop(store);
        let reopened = MirrorStore::open(&root).expect("reopen");
        let _lease = reopened.acquire_ingest_lease().expect("recover");
        assert!(!partial.exists());
        assert!(root.join("orphans").join("partial").exists());
    }

    #[test]
    fn source_drift_during_capture_never_publishes_a_generation() {
        let (tmp, store) = store();
        let source = tmp.path().join("source.jsonl");
        fs::write(&source, b"before\n").expect("write source");
        let error = store
            .capture_inner(&source, &identity(), || {
                fs::write(&source, b"after\n").map_err(|error| CoreError::io(&source, error))
            })
            .expect_err("drift must reject capture");
        assert!(error.to_string().contains("source changed"));
        let generations = store.root().join("generations");
        assert!(
            fs::read_dir(generations)
                .expect("generations")
                .next()
                .is_none(),
            "no generation may publish after capture drift"
        );
    }

    #[test]
    fn precommit_live_check_is_fail_closed_without_revoking_the_mirror() {
        let (tmp, store) = store();
        let lease = store.acquire_ingest_lease().expect("lease");
        let source = tmp.path().join("source.jsonl");
        fs::write(&source, b"accepted candidate\n").expect("write source");
        let sealed = store
            .capture(&lease, &source, &identity())
            .expect("capture");
        fs::write(&source, b"changed candidate\n").expect("mutate live source");
        assert!(store.verify_live_candidate(&source, &sealed).is_err());
        assert!(
            store.verify(&sealed).is_ok(),
            "sealed history remains valid"
        );
    }

    #[test]
    #[cfg(unix)]
    fn private_artifacts_are_created_with_restrictive_modes() {
        use std::os::unix::fs::PermissionsExt;

        let (tmp, store) = store();
        let lease = store.acquire_ingest_lease().expect("lease");
        let source = tmp.path().join("source.jsonl");
        fs::write(&source, b"private\n").expect("write source");
        let sealed = store
            .capture(&lease, &source, &identity())
            .expect("capture");
        for directory in [
            store.root().to_path_buf(),
            store.root().join("staging"),
            store.root().join("generations"),
            store.root().join("indexes"),
            store.root().join("spools"),
        ] {
            assert_eq!(
                fs::metadata(&directory)
                    .expect("directory metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o700,
                "{}",
                directory.display()
            );
        }
        assert_eq!(
            fs::metadata(store.root().join("coordination.sqlite"))
                .expect("coordination metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        for file in [
            store.resolve_payload(&sealed).expect("payload"),
            store.resolve_seal(&sealed).expect("seal"),
        ] {
            assert_eq!(
                fs::metadata(&file)
                    .expect("sealed metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o400
            );
        }
    }

    #[test]
    #[cfg(unix)]
    fn namespaced_open_never_seizes_permissions_on_an_existing_broad_root() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = TempDir::new().expect("tempdir");
        let broad = tmp.path().join("shared-container");
        fs::create_dir(&broad).expect("broad container");
        fs::set_permissions(&broad, fs::Permissions::from_mode(0o755)).expect("set broad mode");
        let error = MirrorStore::open_namespaced(&broad, "ledger_synthetic")
            .expect_err("a broad existing directory must be rejected, not chmodded");
        assert!(error.to_string().contains("already private"));
        assert_eq!(
            fs::metadata(&broad)
                .expect("broad metadata")
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        assert!(!broad.join("ledgers").exists());
    }

    #[test]
    fn lease_recovery_removes_private_indexes_and_spools_before_ingest() {
        let (tmp, store) = store();
        let index = store.root().join("indexes").join("stale.sqlite");
        let spool = store.root().join("spools").join("stale.sqlite");
        fs::write(&index, b"stale").expect("index");
        fs::write(&spool, b"stale").expect("spool");
        let _lease = store.acquire_ingest_lease().expect("recover lease");
        assert!(!index.exists());
        assert!(!spool.exists());
        drop(tmp);
    }

    #[test]
    fn sealed_but_uncommitted_successor_recovers_without_touching_accepted_history() {
        let (tmp, store) = store();
        let lease = store.acquire_ingest_lease().expect("lease");
        let source = tmp.path().join("source.jsonl");
        fs::write(&source, b"g1\n").expect("write g1");
        let g1 = store
            .capture(&lease, &source, &identity())
            .expect("seal g1");
        fs::write(&source, b"g1\ng2\n").expect("write g2");
        let g2 = store
            .capture(&lease, &source, &identity())
            .expect("seal uncommitted g2");

        let accepted = BTreeSet::from([g1.generation_key.clone()]);
        assert_eq!(
            store
                .recover_unaccepted_generations(&lease, &accepted)
                .expect("recover uncommitted generation"),
            1
        );
        assert!(store.verify(&g1).is_ok(), "accepted g1 remains authority");
        assert!(
            store.verify(&g2).is_ok(),
            "recovery never mutates sealed bytes that a copied ledger may reference"
        );
        assert!(
            store
                .root()
                .join("orphans")
                .join(format!("{}.json", g2.generation_key))
                .exists(),
            "the unreferenced observation remains privately diagnosable"
        );
    }

    #[test]
    #[cfg(unix)]
    fn symlinked_ancestor_cannot_route_mirrors_into_the_repository() {
        use std::os::unix::fs::symlink;

        let cwd = std::env::current_dir().expect("current repository");
        let repository = cwd
            .ancestors()
            .find(|ancestor| ancestor.join(".git").exists())
            .expect("repository root");
        fs::create_dir_all(repository.join("target")).expect("repository target directory");
        let repository_target = tempfile::tempdir_in(repository.join("target"))
            .expect("repository-local ignored target");
        let outside = TempDir::new().expect("outside tempdir");
        let alias = outside.path().join("repository-alias");
        symlink(repository_target.path(), &alias).expect("symlink repository ancestor");
        let destination = alias.join("private-mirror");
        let error = MirrorStore::open(&destination)
            .expect_err("canonical repository containment must fail closed");
        assert!(error.to_string().contains("outside the current repository"));
        assert!(!repository_target.path().join("private-mirror").exists());
    }

    #[test]
    #[cfg(unix)]
    fn nested_generation_symlink_is_never_followed_by_receipt_resolution() {
        use std::os::unix::fs::symlink;

        let (tmp, store) = store();
        let lease = store.acquire_ingest_lease().expect("lease");
        let source = tmp.path().join("source.jsonl");
        fs::write(&source, b"sealed\n").expect("source");
        let sealed = store
            .capture(&lease, &source, &identity())
            .expect("capture");
        let payload = store.resolve_payload(&sealed).expect("payload");
        let generation = payload
            .parent()
            .expect("generation directory")
            .to_path_buf();
        let moved = tmp.path().join("moved-generation");
        set_file_mode(&generation, 0o700).expect("open generation for controlled test move");
        fs::rename(&generation, &moved).expect("move generation outside receipt path");
        symlink(&moved, &generation).expect("replace generation with symlink");

        let error = store
            .verify(&sealed)
            .expect_err("nested generation symlink must fail closed");
        assert!(error.to_string().contains("symlink"));
    }

    #[test]
    #[cfg(unix)]
    fn swapped_top_level_container_cannot_redirect_recovery() {
        use std::os::unix::fs::symlink;

        let (tmp, store) = store();
        let lease = store.acquire_ingest_lease().expect("lease before swap");
        let generations = store.root().join("generations");
        let saved = store.root().join("generations-saved");
        fs::rename(&generations, &saved).expect("move real generations container");
        let outside = tmp.path().join("outside-generations");
        fs::create_dir(&outside).expect("outside container");
        let marker = outside.join("must-remain");
        fs::write(&marker, b"private").expect("outside marker");
        symlink(&outside, &generations).expect("swap generations for symlink");

        let error = store
            .recover_unaccepted_generations(&lease, &BTreeSet::new())
            .expect_err("recovery must reject a swapped container");
        assert!(error.to_string().contains("symlink"));
        assert_eq!(
            fs::read(&marker).expect("outside marker preserved"),
            b"private"
        );
    }
}
