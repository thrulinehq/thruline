//! Verified, operator-supplied files for the optional A18 ONNX scorer.
//!
//! This module is deliberately independent of the `ort` feature. Inspecting
//! or importing model files hashes local bytes only; it never loads a model,
//! consults a proxy, or opens a network connection.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::{CoreError, Result};

pub const MODEL_STATUS_SCHEMA: &str = "thruline-nli-model-status-v1";
pub const MODEL_IMPORT_SCHEMA: &str = "thruline-nli-model-import-v1";
pub const A18_MODEL_ID: &str = "Xenova/nli-deberta-v3-xsmall";
/// SHA-256 of the closed manifest under the framing documented by
/// `closed_manifest_digest`. This binds all three logical names, published
/// paths, exact sizes, and per-file SHA-256 receipts.
pub const A18_MODEL_RECEIPT_DIGEST: &str =
    "530faffa2fc879ab9cf6cfc0616b792a11fa89e41c240988d7bdeafcfe9061c5";
pub const A18_MODEL_RECEIPT_ID: &str = concat!(
    "Xenova/nli-deberta-v3-xsmall@sha256:",
    "530faffa2fc879ab9cf6cfc0616b792a11fa89e41c240988d7bdeafcfe9061c5"
);

/// The published A18 receipt, flattened into the directory shape `OrtNli`
/// consumes. The ONNX file is published upstream at
/// `onnx/model_quantized.onnx`; its verified local name is the flat
/// `model_quantized.onnx` below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ModelFileSpec {
    pub name: &'static str,
    pub published_path: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

pub const A18_MODEL_FILES: [ModelFileSpec; 3] = [
    ModelFileSpec {
        name: "config.json",
        published_path: "config.json",
        bytes: 1_038,
        sha256: "ec0bd14cc28640326474399cd61d38ccd52b64900228799d0f81debda8c4bc53",
    },
    ModelFileSpec {
        name: "tokenizer.json",
        published_path: "tokenizer.json",
        bytes: 8_656_551,
        sha256: "a86f883318afa11c8c10466f1bf4efaeb6ded28a52cbe57217a8fa0d0a2a87df",
    },
    ModelFileSpec {
        name: "model_quantized.onnx",
        published_path: "onnx/model_quantized.onnx",
        bytes: 87_246_587,
        sha256: "3fac2500c45c75af42c7711de0d1b93d59577456100208be0dc1f9e8811946b6",
    },
];

/// The exact verified bytes consumed by [`crate::nli::OrtNli`].
///
/// Deliberately has no `Debug` or serialization implementation: model bytes
/// are runtime input, not terminal or receipt output. The three vectors come
/// from one descriptor-anchored read and are the only bytes the tokenizer,
/// config parser, and ONNX Runtime may consume.
#[cfg(any(feature = "ort", test))]
pub(crate) struct VerifiedModelBytes {
    receipt_id: String,
    config_json: Vec<u8>,
    tokenizer_json: Vec<u8>,
    onnx_model: Vec<u8>,
}

#[cfg(any(feature = "ort", test))]
impl VerifiedModelBytes {
    #[cfg(test)]
    pub(crate) fn receipt_id(&self) -> &str {
        &self.receipt_id
    }

    pub(crate) fn into_parts(self) -> (String, Vec<u8>, Vec<u8>, Vec<u8>) {
        (
            self.receipt_id,
            self.config_json,
            self.tokenizer_json,
            self.onnx_model,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelDirectoryState {
    Missing,
    Symlink,
    NotDirectory,
    Invalid,
    Ready,
}

impl ModelDirectoryState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Symlink => "symlink",
            Self::NotDirectory => "not_directory",
            Self::Invalid => "invalid",
            Self::Ready => "ready",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelFileState {
    NotChecked,
    Missing,
    Symlink,
    NotRegular,
    SizeMismatch,
    Sha256Mismatch,
    Verified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelFileStatus {
    pub name: String,
    pub published_path: String,
    pub expected_bytes: u64,
    pub expected_sha256: String,
    pub state: ModelFileState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelDirectoryStatus {
    pub schema: &'static str,
    pub model: &'static str,
    pub path: String,
    pub state: ModelDirectoryState,
    pub ready: bool,
    /// Unexpected directory entries are counted, not echoed. A hostile or
    /// sensitive filename must not become terminal output.
    pub unexpected_entries: u64,
    pub files: Vec<ModelFileStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelImportOutcome {
    PreviewReady,
    Imported,
    RefusedSource,
    RefusedDestination,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelImportReport {
    pub schema: &'static str,
    pub model: &'static str,
    pub apply_requested: bool,
    pub outcome: ModelImportOutcome,
    pub source: ModelDirectoryStatus,
    pub destination_before: ModelDirectoryStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination_after: Option<ModelDirectoryStatus>,
}

impl ModelImportReport {
    /// Whether the requested operation was safe and complete. A preview is a
    /// success only when the exact source is ready and the destination is
    /// absent; it still writes nothing.
    pub fn successful(&self) -> bool {
        matches!(
            self.outcome,
            ModelImportOutcome::PreviewReady | ModelImportOutcome::Imported
        )
    }
}

/// Verify the exact, flat A18 directory receipt without loading the model.
pub fn verify_model_dir(dir: impl AsRef<Path>) -> Result<ModelDirectoryStatus> {
    verify_model_dir_against(dir.as_ref(), &A18_MODEL_FILES)
}

/// Open, read, and verify the complete A18 receipt without reopening a path.
///
/// On macOS and Linux the final directory is opened once without following a
/// symlink. Each expected child is then opened relative to that held
/// descriptor with `O_NOFOLLOW`, read once under its published size bound,
/// and hashed in memory. The returned bytes—not the original path—are what
/// the runtime parser and ONNX loader consume. Any symlink resolution in
/// ancestor components occurs before the directory descriptor is anchored
/// and therefore cannot change the bytes subsequently verified and loaded.
#[cfg(feature = "ort")]
pub(crate) fn load_verified_model_bytes(dir: impl AsRef<Path>) -> Result<VerifiedModelBytes> {
    #[cfg(any(target_vendor = "apple", target_os = "linux"))]
    {
        load_verified_model_bytes_against_with_hook(dir.as_ref(), &A18_MODEL_FILES, || Ok(()))
    }

    #[cfg(not(any(target_vendor = "apple", target_os = "linux")))]
    {
        let _ = dir;
        Err(CoreError::Config(
            "descriptor-anchored NLI model loading is unavailable on this platform".to_string(),
        ))
    }
}

/// Preview or apply an operator-supplied local import.
///
/// Preview is the default (`apply = false`) and performs no writes. Apply
/// stages and verifies all three files beside the destination, then renames
/// that complete directory into place. Any existing destination—valid,
/// invalid, empty, a file, or a symlink—is refused and never overwritten.
pub fn import_model_dir(
    source: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    apply: bool,
) -> Result<ModelImportReport> {
    import_model_dir_against(
        source.as_ref(),
        destination.as_ref(),
        apply,
        &A18_MODEL_FILES,
    )
}

fn unchecked_files(specs: &[ModelFileSpec], state: ModelFileState) -> Vec<ModelFileStatus> {
    specs
        .iter()
        .map(|spec| ModelFileStatus {
            name: spec.name.to_string(),
            published_path: spec.published_path.to_string(),
            expected_bytes: spec.bytes,
            expected_sha256: spec.sha256.to_string(),
            state,
            observed_bytes: None,
            observed_sha256: None,
        })
        .collect()
}

fn directory_status(
    dir: &Path,
    state: ModelDirectoryState,
    unexpected_entries: u64,
    files: Vec<ModelFileStatus>,
) -> ModelDirectoryStatus {
    ModelDirectoryStatus {
        schema: MODEL_STATUS_SCHEMA,
        model: A18_MODEL_ID,
        path: dir.display().to_string(),
        state,
        ready: state == ModelDirectoryState::Ready,
        unexpected_entries,
        files,
    }
}

fn verify_model_dir_against(dir: &Path, specs: &[ModelFileSpec]) -> Result<ModelDirectoryStatus> {
    let root_meta = match fs::symlink_metadata(dir) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(directory_status(
                dir,
                ModelDirectoryState::Missing,
                0,
                unchecked_files(specs, ModelFileState::Missing),
            ));
        }
        Err(error) => return Err(CoreError::io(dir, error)),
    };

    if root_meta.file_type().is_symlink() {
        return Ok(directory_status(
            dir,
            ModelDirectoryState::Symlink,
            0,
            unchecked_files(specs, ModelFileState::NotChecked),
        ));
    }
    if !root_meta.is_dir() {
        return Ok(directory_status(
            dir,
            ModelDirectoryState::NotDirectory,
            0,
            unchecked_files(specs, ModelFileState::NotChecked),
        ));
    }

    let expected: BTreeSet<&str> = specs.iter().map(|spec| spec.name).collect();
    let mut unexpected_entries = 0_u64;
    for entry in fs::read_dir(dir).map_err(|error| CoreError::io(dir, error))? {
        let entry = entry.map_err(|error| CoreError::io(dir, error))?;
        let expected_entry = entry
            .file_name()
            .to_str()
            .is_some_and(|name| expected.contains(name));
        if !expected_entry {
            unexpected_entries += 1;
        }
    }

    let mut files = Vec::with_capacity(specs.len());
    for spec in specs {
        let path = dir.join(spec.name);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                files.push(ModelFileStatus {
                    name: spec.name.to_string(),
                    published_path: spec.published_path.to_string(),
                    expected_bytes: spec.bytes,
                    expected_sha256: spec.sha256.to_string(),
                    state: ModelFileState::Missing,
                    observed_bytes: None,
                    observed_sha256: None,
                });
                continue;
            }
            Err(error) => return Err(CoreError::io(&path, error)),
        };

        let mut status = ModelFileStatus {
            name: spec.name.to_string(),
            published_path: spec.published_path.to_string(),
            expected_bytes: spec.bytes,
            expected_sha256: spec.sha256.to_string(),
            state: ModelFileState::NotChecked,
            observed_bytes: Some(metadata.len()),
            observed_sha256: None,
        };
        if metadata.file_type().is_symlink() {
            status.state = ModelFileState::Symlink;
        } else if !metadata.is_file() {
            status.state = ModelFileState::NotRegular;
        } else if metadata.len() != spec.bytes {
            status.state = ModelFileState::SizeMismatch;
        } else {
            let observed = sha256_file(&path)?;
            status.observed_sha256 = Some(observed.clone());
            status.state = if observed == spec.sha256 {
                ModelFileState::Verified
            } else {
                ModelFileState::Sha256Mismatch
            };
        }
        files.push(status);
    }

    let ready = unexpected_entries == 0
        && files
            .iter()
            .all(|file| file.state == ModelFileState::Verified);
    Ok(directory_status(
        dir,
        if ready {
            ModelDirectoryState::Ready
        } else {
            ModelDirectoryState::Invalid
        },
        unexpected_entries,
        files,
    ))
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(|error| CoreError::io(path, error))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| CoreError::io(path, error))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    digest_hex(&digest.finalize())
}

#[cfg(any(feature = "ort", test))]
fn sha256_bytes(bytes: &[u8]) -> Result<String> {
    digest_hex(&Sha256::digest(bytes))
}

fn digest_hex(bytes: &[u8]) -> Result<String> {
    let mut hex = String::with_capacity(64);
    for byte in bytes {
        write!(&mut hex, "{byte:02x}")
            .map_err(|error| CoreError::Config(format!("formatting SHA-256: {error}")))?;
    }
    Ok(hex)
}

/// Canonical closed-manifest framing for runtime scorer identity.
///
/// The stream is the schema plus NUL, followed in manifest order by each
/// logical name plus NUL, published path plus NUL, eight-byte big-endian
/// length, and lowercase SHA-256 receipt plus NUL. The fixed field framing and
/// published order make the identity independent of operator paths.
#[cfg(any(feature = "ort", test))]
fn closed_manifest_digest(specs: &[ModelFileSpec]) -> Result<String> {
    let mut digest = Sha256::new();
    digest.update(b"thruline-a18-model-receipt-v1\0");
    for spec in specs {
        digest.update(spec.name.as_bytes());
        digest.update([0]);
        digest.update(spec.published_path.as_bytes());
        digest.update([0]);
        digest.update(spec.bytes.to_be_bytes());
        digest.update(spec.sha256.as_bytes());
        digest.update([0]);
    }
    digest_hex(&digest.finalize())
}

#[cfg(all(
    any(feature = "ort", test),
    any(target_vendor = "apple", target_os = "linux")
))]
fn load_verified_model_bytes_against_with_hook<F>(
    dir: &Path,
    specs: &[ModelFileSpec],
    after_directory_open: F,
) -> Result<VerifiedModelBytes>
where
    F: FnOnce() -> Result<()>,
{
    use rustix::fs::{Dir, Mode, OFlags};

    let dir_fd = rustix::fs::openat(
        rustix::fs::CWD,
        dir,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| CoreError::io(dir, std::io::Error::from(error)))?;

    // Tests replace the original pathname at this exact boundary. Everything
    // below must stay relative to the held descriptor; production supplies a
    // no-op hook.
    after_directory_open()?;

    // Enumerate through the held descriptor. Unexpected names are never
    // echoed: a hostile filename must not become terminal output.
    let expected = specs
        .iter()
        .map(|spec| spec.name.as_bytes().to_vec())
        .collect::<BTreeSet<_>>();
    if expected.len() != specs.len() {
        return Err(CoreError::Config(
            "NLI model manifest contains duplicate file names".to_string(),
        ));
    }
    let mut seen = BTreeSet::new();
    let entries =
        Dir::read_from(&dir_fd).map_err(|error| CoreError::io(dir, std::io::Error::from(error)))?;
    for entry in entries {
        let entry = entry.map_err(|error| CoreError::io(dir, std::io::Error::from(error)))?;
        let name = entry.file_name().to_bytes();
        if name == b"." || name == b".." {
            continue;
        }
        if !expected.contains(name) {
            return Err(CoreError::Integrity(
                "NLI model directory contains an unexpected entry".to_string(),
            ));
        }
        if !seen.insert(name.to_vec()) {
            return Err(CoreError::Integrity(
                "NLI model directory contains a duplicate entry".to_string(),
            ));
        }
    }
    if seen != expected {
        return Err(CoreError::Integrity(
            "NLI model directory does not contain the exact closed manifest".to_string(),
        ));
    }

    let mut loaded = std::collections::BTreeMap::new();
    for spec in specs {
        let logical_path = dir.join(spec.name);
        let fd = rustix::fs::openat(
            &dir_fd,
            spec.name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| CoreError::io(&logical_path, std::io::Error::from(error)))?;
        let mut file = File::from(fd);
        let metadata = file
            .metadata()
            .map_err(|error| CoreError::io(&logical_path, error))?;
        if !metadata.is_file() {
            return Err(CoreError::Integrity(format!(
                "NLI model file {} is not a regular file",
                spec.name
            )));
        }
        if metadata.len() != spec.bytes {
            return Err(CoreError::Integrity(format!(
                "NLI model file {} has the wrong byte length",
                spec.name
            )));
        }

        let expected_len = usize::try_from(spec.bytes).map_err(|_| {
            CoreError::Config(format!(
                "NLI model file {} is too large for this platform",
                spec.name
            ))
        })?;
        let read_limit = spec.bytes.checked_add(1).ok_or_else(|| {
            CoreError::Config(format!(
                "NLI model file {} has an unsupported byte length",
                spec.name
            ))
        })?;
        let mut bytes = Vec::with_capacity(expected_len);
        (&mut file)
            .take(read_limit)
            .read_to_end(&mut bytes)
            .map_err(|error| CoreError::io(&logical_path, error))?;
        let final_metadata = file
            .metadata()
            .map_err(|error| CoreError::io(&logical_path, error))?;
        if bytes.len() != expected_len || final_metadata.len() != spec.bytes {
            return Err(CoreError::Integrity(format!(
                "NLI model file {} changed while it was being read",
                spec.name
            )));
        }
        if sha256_bytes(&bytes)? != spec.sha256 {
            return Err(CoreError::Integrity(format!(
                "NLI model file {} failed its SHA-256 receipt",
                spec.name
            )));
        }
        if loaded.insert(spec.name, bytes).is_some() {
            return Err(CoreError::Config(
                "NLI model manifest contains duplicate file names".to_string(),
            ));
        }
    }

    let mut take = |name| {
        loaded.remove(name).ok_or_else(|| {
            CoreError::Config(format!(
                "NLI model manifest is missing required file {name}"
            ))
        })
    };
    let receipt_id = format!("{A18_MODEL_ID}@sha256:{}", closed_manifest_digest(specs)?);
    let verified = VerifiedModelBytes {
        receipt_id,
        config_json: take("config.json")?,
        tokenizer_json: take("tokenizer.json")?,
        onnx_model: take("model_quantized.onnx")?,
    };
    if !loaded.is_empty() {
        return Err(CoreError::Config(
            "NLI model manifest contains unsupported required files".to_string(),
        ));
    }
    Ok(verified)
}

fn import_model_dir_against(
    source: &Path,
    destination: &Path,
    apply: bool,
    specs: &[ModelFileSpec],
) -> Result<ModelImportReport> {
    import_model_dir_against_with_hook(source, destination, apply, specs, || Ok(()))
}

fn import_model_dir_against_with_hook<F>(
    source: &Path,
    destination: &Path,
    apply: bool,
    specs: &[ModelFileSpec],
    before_publish: F,
) -> Result<ModelImportReport>
where
    F: FnOnce() -> Result<()>,
{
    let source_status = verify_model_dir_against(source, specs)?;
    let destination_status = verify_model_dir_against(destination, specs)?;

    if !source_status.ready {
        return Ok(ModelImportReport {
            schema: MODEL_IMPORT_SCHEMA,
            model: A18_MODEL_ID,
            apply_requested: apply,
            outcome: ModelImportOutcome::RefusedSource,
            source: source_status,
            destination_before: destination_status,
            destination_after: None,
        });
    }
    if destination_status.state != ModelDirectoryState::Missing {
        return Ok(ModelImportReport {
            schema: MODEL_IMPORT_SCHEMA,
            model: A18_MODEL_ID,
            apply_requested: apply,
            outcome: ModelImportOutcome::RefusedDestination,
            source: source_status,
            destination_before: destination_status,
            destination_after: None,
        });
    }

    let publish_destination = resolve_destination_outside_source(source, destination)?;

    if !apply {
        return Ok(ModelImportReport {
            schema: MODEL_IMPORT_SCHEMA,
            model: A18_MODEL_ID,
            apply_requested: false,
            outcome: ModelImportOutcome::PreviewReady,
            source: source_status,
            destination_before: destination_status,
            destination_after: None,
        });
    }

    let parent = publish_destination
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| CoreError::io(parent, error))?;
    let lock_path = parent.join(".thruline-nli-model-import.lock");
    let lock = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                CoreError::Config("another NLI model import is already active".to_string())
            } else {
                CoreError::io(&lock_path, error)
            }
        })?;
    let mut guard = ImportGuard {
        stage: None,
        lock: lock_path,
        committed: false,
    };
    // Arm cleanup immediately after `create_new`: even a lock sync failure
    // must not strand a stale importer lock.
    lock.sync_all()
        .map_err(|error| CoreError::io(&guard.lock, error))?;

    let stage = create_stage_dir(parent)?;
    guard.stage = Some(stage.clone());

    for spec in specs {
        copy_file_synced(&source.join(spec.name), &stage.join(spec.name))?;
    }
    sync_dir(&stage)?;
    let staged_status = verify_model_dir_against(&stage, specs)?;
    if !staged_status.ready {
        return Err(CoreError::Integrity(
            "staged NLI model did not match the closed A18 manifest".to_string(),
        ));
    }

    before_publish()?;
    match publish_staged_directory_noreplace(&stage, &publish_destination)? {
        PublishOutcome::Published => guard.committed = true,
        PublishOutcome::DestinationExists => {
            let current = verify_model_dir_against(&publish_destination, specs)?;
            return Ok(ModelImportReport {
                schema: MODEL_IMPORT_SCHEMA,
                model: A18_MODEL_ID,
                apply_requested: true,
                outcome: ModelImportOutcome::RefusedDestination,
                source: source_status,
                destination_before: current,
                destination_after: None,
            });
        }
    }
    sync_dir(parent)?;

    let destination_after = verify_model_dir_against(&publish_destination, specs)?;
    if !destination_after.ready {
        return Err(CoreError::Integrity(
            "imported NLI model did not match the closed A18 manifest".to_string(),
        ));
    }

    Ok(ModelImportReport {
        schema: MODEL_IMPORT_SCHEMA,
        model: A18_MODEL_ID,
        apply_requested: true,
        outcome: ModelImportOutcome::Imported,
        source: source_status,
        destination_before: destination_status,
        destination_after: Some(destination_after),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PublishOutcome {
    Published,
    DestinationExists,
}

/// Atomically publish a fully verified sibling directory without replacement.
///
/// `rustix` is the safe wrapper over `renameatx_np(RENAME_EXCL)` on Apple and
/// `renameat2(RENAME_NOREPLACE)` on Linux. If the host lacks that primitive,
/// the call fails closed and the staging guard removes only our private stage;
/// plain `rename` is never used as a fallback because it may replace an empty
/// directory created by a competitor.
#[cfg(any(target_vendor = "apple", target_os = "linux"))]
fn publish_staged_directory_noreplace(stage: &Path, destination: &Path) -> Result<PublishOutcome> {
    match rustix::fs::renameat_with(
        rustix::fs::CWD,
        stage,
        rustix::fs::CWD,
        destination,
        rustix::fs::RenameFlags::NOREPLACE,
    ) {
        Ok(()) => Ok(PublishOutcome::Published),
        Err(error) if error == rustix::io::Errno::EXIST || error == rustix::io::Errno::NOTEMPTY => {
            Ok(PublishOutcome::DestinationExists)
        }
        Err(error) => Err(CoreError::io(destination, std::io::Error::from(error))),
    }
}

#[cfg(not(any(target_vendor = "apple", target_os = "linux")))]
fn publish_staged_directory_noreplace(
    _stage: &Path,
    _destination: &Path,
) -> Result<PublishOutcome> {
    Err(CoreError::Config(
        "atomic no-replace model import is unavailable on this platform; nothing was published"
            .to_string(),
    ))
}

fn resolve_destination_outside_source(source: &Path, destination: &Path) -> Result<PathBuf> {
    let source = fs::canonicalize(source).map_err(|error| CoreError::io(source, error))?;
    let destination_absolute = lexical_absolute(destination)?;
    let mut existing = destination_absolute.clone();
    let mut missing_tail = Vec::new();
    let canonical_existing = loop {
        match fs::symlink_metadata(&existing) {
            Ok(_) => {
                break fs::canonicalize(&existing)
                    .map_err(|error| CoreError::io(&existing, error))?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = existing.file_name().ok_or_else(|| {
                    CoreError::Config(
                        "model destination has no existing filesystem ancestor".to_string(),
                    )
                })?;
                missing_tail.push(name.to_os_string());
                if !existing.pop() {
                    return Err(CoreError::Config(
                        "model destination has no existing filesystem ancestor".to_string(),
                    ));
                }
            }
            Err(error) => return Err(CoreError::io(&existing, error)),
        }
    };

    let mut resolved = canonical_existing;
    for component in missing_tail.into_iter().rev() {
        resolved.push(component);
    }

    if resolved.starts_with(&source) {
        return Err(CoreError::Config(
            "model destination must not be inside the source directory".to_string(),
        ));
    }
    Ok(resolved)
}

fn lexical_absolute(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| CoreError::io(".", error))?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(CoreError::Config(
                    "model destination must not contain parent traversal".to_string(),
                ));
            }
            Component::Normal(name) => normalized.push(name),
        }
    }
    Ok(normalized)
}

static NEXT_STAGE: AtomicU64 = AtomicU64::new(1);

fn create_stage_dir(parent: &Path) -> Result<PathBuf> {
    for _ in 0..32 {
        let id = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".thruline-nli-model-stage-{}-{id}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(CoreError::io(&path, error)),
        }
    }
    Err(CoreError::Config(
        "could not allocate a private NLI model staging directory".to_string(),
    ))
}

fn copy_file_synced(source: &Path, destination: &Path) -> Result<()> {
    let mut input = File::open(source).map_err(|error| CoreError::io(source, error))?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| CoreError::io(destination, error))?;
    std::io::copy(&mut input, &mut output).map_err(|error| CoreError::io(destination, error))?;
    output
        .flush()
        .map_err(|error| CoreError::io(destination, error))?;
    output
        .sync_all()
        .map_err(|error| CoreError::io(destination, error))?;
    Ok(())
}

fn sync_dir(dir: &Path) -> Result<()> {
    File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|error| CoreError::io(dir, error))
}

struct ImportGuard {
    stage: Option<PathBuf>,
    lock: PathBuf,
    committed: bool,
}

impl Drop for ImportGuard {
    fn drop(&mut self) {
        if !self.committed {
            if let Some(stage) = &self.stage {
                let _ = fs::remove_dir_all(stage);
            }
        }
        let _ = fs::remove_file(&self.lock);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_FILES: [ModelFileSpec; 3] = [
        ModelFileSpec {
            name: "config.json",
            published_path: "config.json",
            bytes: 3,
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        },
        ModelFileSpec {
            name: "tokenizer.json",
            published_path: "tokenizer.json",
            bytes: 4,
            sha256: "4c8a43980498636e9c1d1595fa5d115af7937c2422dfe68a2520a52b7a5fb4de",
        },
        ModelFileSpec {
            name: "model_quantized.onnx",
            published_path: "onnx/model_quantized.onnx",
            bytes: 11,
            sha256: "357e5d6fafa34d27360fec24b4326d3534905e33c6acdee60198fb078b7b79e5",
        },
    ];

    fn write_valid(dir: &Path) {
        fs::create_dir_all(dir).expect("fixture directory");
        fs::write(dir.join("config.json"), b"abc").expect("config");
        fs::write(dir.join("tokenizer.json"), b"defg").expect("tokenizer");
        fs::write(dir.join("model_quantized.onnx"), b"model-bytes").expect("model");
    }

    fn status(dir: &Path) -> ModelDirectoryStatus {
        verify_model_dir_against(dir, &TEST_FILES).expect("status")
    }

    fn import(source: &Path, destination: &Path, apply: bool) -> ModelImportReport {
        import_model_dir_against(source, destination, apply, &TEST_FILES).expect("import")
    }

    #[test]
    fn published_manifest_is_the_exact_flat_a18_receipt() {
        assert_eq!(A18_MODEL_FILES.len(), 3);
        assert_eq!(A18_MODEL_FILES[0].name, "config.json");
        assert_eq!(A18_MODEL_FILES[0].bytes, 1_038);
        assert_eq!(
            A18_MODEL_FILES[0].sha256,
            "ec0bd14cc28640326474399cd61d38ccd52b64900228799d0f81debda8c4bc53"
        );
        assert_eq!(A18_MODEL_FILES[1].name, "tokenizer.json");
        assert_eq!(A18_MODEL_FILES[1].bytes, 8_656_551);
        assert_eq!(
            A18_MODEL_FILES[1].sha256,
            "a86f883318afa11c8c10466f1bf4efaeb6ded28a52cbe57217a8fa0d0a2a87df"
        );
        assert_eq!(A18_MODEL_FILES[2].name, "model_quantized.onnx");
        assert_eq!(
            A18_MODEL_FILES[2].published_path,
            "onnx/model_quantized.onnx"
        );
        assert_eq!(A18_MODEL_FILES[2].bytes, 87_246_587);
        assert_eq!(
            A18_MODEL_FILES[2].sha256,
            "3fac2500c45c75af42c7711de0d1b93d59577456100208be0dc1f9e8811946b6"
        );
        assert_eq!(
            closed_manifest_digest(&A18_MODEL_FILES).expect("manifest digest"),
            A18_MODEL_RECEIPT_DIGEST
        );
        assert_eq!(
            A18_MODEL_RECEIPT_ID,
            format!("{A18_MODEL_ID}@sha256:{A18_MODEL_RECEIPT_DIGEST}")
        );
    }

    #[test]
    fn valid_missing_tampered_and_extra_directories_fail_loudly() {
        let temp = tempfile::tempdir().expect("tempdir");
        let model = temp.path().join("model");
        write_valid(&model);
        assert_eq!(status(&model).state, ModelDirectoryState::Ready);

        fs::remove_file(model.join("tokenizer.json")).expect("remove fixture file");
        let missing = status(&model);
        assert_eq!(missing.state, ModelDirectoryState::Invalid);
        assert_eq!(missing.files[1].state, ModelFileState::Missing);

        fs::write(model.join("tokenizer.json"), b"defg").expect("restore tokenizer");
        fs::write(model.join("config.json"), b"abd").expect("tamper config");
        let tampered = status(&model);
        assert_eq!(tampered.files[0].state, ModelFileState::Sha256Mismatch);

        fs::write(model.join("config.json"), b"abc").expect("restore config");
        fs::write(model.join("private-note.txt"), b"DO_NOT_ECHO_THIS_CONTENT").expect("extra file");
        let extra = status(&model);
        assert_eq!(extra.state, ModelDirectoryState::Invalid);
        assert_eq!(extra.unexpected_entries, 1);
        let json = serde_json::to_string(&extra).expect("json");
        assert!(!json.contains("DO_NOT_ECHO_THIS_CONTENT"));
        assert!(!json.contains("private-note.txt"));
    }

    #[cfg(unix)]
    #[test]
    fn file_and_root_symlinks_are_rejected_without_following_them() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("tempdir");
        let model = temp.path().join("model");
        write_valid(&model);
        fs::remove_file(model.join("config.json")).expect("remove config");
        let outside = temp.path().join("outside");
        fs::write(&outside, b"abc").expect("outside fixture");
        symlink(&outside, model.join("config.json")).expect("file symlink");
        assert_eq!(status(&model).files[0].state, ModelFileState::Symlink);

        let root_link = temp.path().join("model-link");
        symlink(&model, &root_link).expect("root symlink");
        assert_eq!(status(&root_link).state, ModelDirectoryState::Symlink);

        let valid_source = temp.path().join("valid-source");
        write_valid(&valid_source);
        let destination_link = temp.path().join("destination-link");
        symlink(&model, &destination_link).expect("destination symlink");
        let refused = import(&valid_source, &destination_link, true);
        assert_eq!(refused.outcome, ModelImportOutcome::RefusedDestination);
        assert!(fs::symlink_metadata(&destination_link)
            .expect("destination link")
            .file_type()
            .is_symlink());

        let alias = temp.path().join("source-alias");
        symlink(&valid_source, &alias).expect("source alias");
        let aliased_destination = alias.join("missing-parent/model");
        let error =
            import_model_dir_against(&valid_source, &aliased_destination, false, &TEST_FILES)
                .expect_err("symlink ancestor resolves inside source");
        assert!(error.to_string().contains("must not be inside"));
        assert!(!valid_source.join("missing-parent").exists());
    }

    #[cfg(any(target_vendor = "apple", target_os = "linux"))]
    #[test]
    fn verified_runtime_bytes_survive_a_path_swap_without_reopening() {
        let temp = tempfile::tempdir().expect("tempdir");
        let model = temp.path().join("model");
        let original = temp.path().join("verified-original");
        write_valid(&model);
        assert_eq!(status(&model).state, ModelDirectoryState::Ready);

        let verified = load_verified_model_bytes_against_with_hook(&model, &TEST_FILES, || {
            fs::rename(&model, &original).map_err(|error| CoreError::io(&model, error))?;
            fs::create_dir(&model).map_err(|error| CoreError::io(&model, error))?;
            // Same lengths, different hashes. An old verify-then-reopen
            // loader would consume these replacement pathname bytes.
            fs::write(model.join("config.json"), b"xyz")
                .map_err(|error| CoreError::io(model.join("config.json"), error))?;
            fs::write(model.join("tokenizer.json"), b"hijk")
                .map_err(|error| CoreError::io(model.join("tokenizer.json"), error))?;
            fs::write(model.join("model_quantized.onnx"), b"other-bytes")
                .map_err(|error| CoreError::io(model.join("model_quantized.onnx"), error))?;
            Ok(())
        })
        .expect("descriptor-backed verified bytes");

        let (_receipt_id, config, tokenizer, onnx) = verified.into_parts();
        assert_eq!(config, b"abc");
        assert_eq!(tokenizer, b"defg");
        assert_eq!(onnx, b"model-bytes");
        assert_eq!(status(&original).state, ModelDirectoryState::Ready);
        assert_eq!(status(&model).state, ModelDirectoryState::Invalid);
    }

    #[cfg(any(target_vendor = "apple", target_os = "linux"))]
    #[test]
    fn identical_receipts_in_different_directories_have_one_content_identity() {
        let temp = tempfile::tempdir().expect("tempdir");
        let first_dir = temp.path().join("operator-name-one");
        let second_dir = temp.path().join("unrelated-name-two");
        write_valid(&first_dir);
        write_valid(&second_dir);

        let first = load_verified_model_bytes_against_with_hook(&first_dir, &TEST_FILES, || Ok(()))
            .expect("first receipt");
        let second =
            load_verified_model_bytes_against_with_hook(&second_dir, &TEST_FILES, || Ok(()))
                .expect("second receipt");

        assert_eq!(first.receipt_id(), second.receipt_id());
        assert!(first.receipt_id().starts_with(A18_MODEL_ID));
        assert!(!first.receipt_id().contains("operator-name-one"));
        assert!(!first.receipt_id().contains("unrelated-name-two"));
    }

    #[cfg(any(target_vendor = "apple", target_os = "linux"))]
    #[test]
    fn runtime_snapshot_rejects_root_and_file_symlinks() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("tempdir");
        let model = temp.path().join("model");
        write_valid(&model);

        let root_link = temp.path().join("model-link");
        symlink(&model, &root_link).expect("root link");
        assert!(
            load_verified_model_bytes_against_with_hook(&root_link, &TEST_FILES, || Ok(()))
                .is_err()
        );

        let outside = temp.path().join("outside-config");
        fs::write(&outside, b"abc").expect("outside config");
        fs::remove_file(model.join("config.json")).expect("remove config");
        symlink(&outside, model.join("config.json")).expect("file link");
        assert!(
            load_verified_model_bytes_against_with_hook(&model, &TEST_FILES, || Ok(())).is_err()
        );
    }

    #[test]
    fn preview_is_write_free_and_apply_is_atomic_and_verified() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("source");
        let destination = temp.path().join("models/model");
        write_valid(&source);

        let preview = import(&source, &destination, false);
        assert_eq!(preview.outcome, ModelImportOutcome::PreviewReady);
        assert!(preview.successful());
        assert!(!destination.exists());
        assert!(!destination.parent().expect("parent").exists());

        let applied = import(&source, &destination, true);
        assert_eq!(applied.outcome, ModelImportOutcome::Imported);
        assert!(applied.successful());
        assert_eq!(
            applied.destination_after.expect("after").state,
            ModelDirectoryState::Ready
        );
        assert_eq!(
            fs::read(destination.join("config.json")).expect("read"),
            b"abc"
        );
        assert!(!destination
            .parent()
            .expect("parent")
            .join(".thruline-nli-model-import.lock")
            .exists());
    }

    #[test]
    fn invalid_source_and_every_preexisting_destination_are_refused() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        write_valid(&source);

        fs::write(source.join("config.json"), b"abd").expect("tamper source");
        let refused_source = import(&source, &destination, true);
        assert_eq!(refused_source.outcome, ModelImportOutcome::RefusedSource);
        assert!(!destination.exists());

        fs::write(source.join("config.json"), b"abc").expect("restore source");
        fs::create_dir(&destination).expect("existing destination");
        fs::write(destination.join("keep"), b"KEEP_ME").expect("sentinel");
        let refused_destination = import(&source, &destination, true);
        assert_eq!(
            refused_destination.outcome,
            ModelImportOutcome::RefusedDestination
        );
        assert_eq!(
            fs::read(destination.join("keep")).expect("sentinel remains"),
            b"KEEP_ME"
        );
    }

    #[test]
    fn a_second_apply_is_an_idempotent_refusal_not_an_overwrite() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        write_valid(&source);

        assert_eq!(
            import(&source, &destination, true).outcome,
            ModelImportOutcome::Imported
        );
        let before = status(&destination);
        let second = import(&source, &destination, true);
        let after = status(&destination);
        assert_eq!(second.outcome, ModelImportOutcome::RefusedDestination);
        assert_eq!(before, after);
        assert_eq!(after.state, ModelDirectoryState::Ready);
    }

    #[cfg(any(target_vendor = "apple", target_os = "linux"))]
    #[test]
    fn an_empty_destination_created_after_staging_is_never_replaced() {
        use std::cell::Cell;
        use std::os::unix::fs::MetadataExt;

        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        write_valid(&source);
        let competitor_identity = Cell::new(None);

        let report =
            import_model_dir_against_with_hook(&source, &destination, true, &TEST_FILES, || {
                fs::create_dir(&destination).map_err(|error| CoreError::io(&destination, error))?;
                let metadata = fs::metadata(&destination)
                    .map_err(|error| CoreError::io(&destination, error))?;
                competitor_identity.set(Some((metadata.dev(), metadata.ino())));
                Ok(())
            })
            .expect("race is a structured refusal");

        assert_eq!(report.outcome, ModelImportOutcome::RefusedDestination);
        let metadata = fs::metadata(&destination).expect("competitor destination");
        assert_eq!(
            competitor_identity.get(),
            Some((metadata.dev(), metadata.ino())),
            "the competitor's empty directory identity must survive"
        );
        assert_eq!(
            fs::read_dir(&destination)
                .expect("competitor directory")
                .count(),
            0,
            "the competitor's empty directory must remain empty"
        );
        assert!(!destination.join("config.json").exists());
        let private_residue = fs::read_dir(temp.path())
            .expect("temp entries")
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| name.starts_with(".thruline-nli-model-"))
            .collect::<Vec<_>>();
        assert!(
            private_residue.is_empty(),
            "private residue: {private_residue:?}"
        );
    }

    #[test]
    fn destination_inside_source_is_rejected_before_any_write() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("source");
        write_valid(&source);
        let destination = source.join("nested");
        let error = import_model_dir_against(&source, &destination, true, &TEST_FILES)
            .expect_err("nested destination");
        assert!(error.to_string().contains("must not be inside"));
        assert!(!destination.exists());
        assert_eq!(status(&source).state, ModelDirectoryState::Ready);

        let parent_traversal = temp.path().join("unused/../destination");
        let error = import_model_dir_against(&source, &parent_traversal, false, &TEST_FILES)
            .expect_err("parent traversal");
        assert!(error.to_string().contains("parent traversal"));
        assert!(!temp.path().join("unused").exists());
    }
}
