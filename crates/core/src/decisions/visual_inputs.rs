//! Frozen record input for C1. The clock is an explicit record fact, never a
//! host clock or a Git commit timestamp. This module only reads the ledger.
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordClockWitness {
    pub event_id: String,
    pub project_id: String,
    pub session_id: String,
    pub seq: i64,
    pub raw_line: i64,
    pub event_timestamp: String,
    pub raw_sha256: String,
    pub chain_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualFixtureManifest {
    pub schema_version: u32,
    #[serde(default)]
    pub record_path: Option<PathBuf>,
    pub record_sha256: String,
    pub record_bytes: u64,
    pub capture_clock: String,
    /// `record_event_ceiling` means the UTC second at or immediately after the
    /// witness. `explicit_fixture` is an explicitly dated synthetic test record.
    pub clock_basis: String,
    pub clock_witness: Option<RecordClockWitness>,
    pub fixture_description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrozenVisualInput {
    pub manifest: VisualFixtureManifest,
    pub manifest_sha256: String,
    pub manifest_path: PathBuf,
    pub record_path: PathBuf,
    pub record_stamp: FileStamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileStamp {
    pub bytes: u64,
    pub modified_ns: Option<u64>,
    pub device: u64,
    pub inode: u64,
    pub changed_seconds: i64,
    pub changed_nanoseconds: i64,
}

pub fn file_stamp(path: &Path) -> Option<FileStamp> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return None;
    }
    #[cfg(unix)]
    let (device, inode, changed_seconds, changed_nanoseconds) = {
        use std::os::unix::fs::MetadataExt;
        (
            metadata.dev(),
            metadata.ino(),
            metadata.ctime(),
            metadata.ctime_nsec(),
        )
    };
    #[cfg(not(unix))]
    let (device, inode, changed_seconds, changed_nanoseconds) = (0, 0, 0, 0);
    Some(FileStamp {
        bytes: metadata.len(),
        modified_ns: metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|time| u64::try_from(time.as_nanos()).ok()),
        device,
        inode,
        changed_seconds,
        changed_nanoseconds,
    })
}

pub fn valid_utc_second(value: &str) -> bool {
    if value.len() != 20 || !value.is_ascii() || !value.ends_with('Z') {
        return false;
    }
    let Ok(conn) = Connection::open_in_memory() else {
        return false;
    };
    conn.query_row(
        "SELECT strftime('%Y-%m-%dT%H:%M:%SZ', julianday(?1))",
        [value],
        |row| row.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
    .as_deref()
        == Some(value)
}

pub fn manifest_path(db_path: &Path) -> PathBuf {
    std::env::var_os("THRULINE_VISUAL_FIXTURE_MANIFEST")
        .map(PathBuf::from)
        .unwrap_or_else(|| db_path.with_extension("visual-fixture.json"))
}

pub fn load_fixture_manifest(db_path: &Path) -> Result<FrozenVisualInput, &'static str> {
    load_fixture_manifest_at(db_path, &manifest_path(db_path))
}

pub fn load_fixture_manifest_at(
    db_path: &Path,
    path: &Path,
) -> Result<FrozenVisualInput, &'static str> {
    let meta = std::fs::symlink_metadata(path).map_err(|_| "record fixture manifest is missing")?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 16_384 {
        return Err("record fixture manifest is invalid");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err("record fixture manifest must be private");
        }
    }
    let body = std::fs::read(path).map_err(|_| "record fixture manifest is unavailable")?;
    let manifest: VisualFixtureManifest =
        serde_json::from_slice(&body).map_err(|_| "record fixture manifest is invalid")?;
    if manifest.schema_version != 1
        || !is_sha256(&manifest.record_sha256)
        || !valid_utc_second(&manifest.capture_clock)
        || manifest.fixture_description.trim().is_empty()
        || manifest.fixture_description.len() > 1024
    {
        return Err("record fixture clock or identity is invalid");
    }
    let record_path = manifest.record_path.as_deref().unwrap_or(db_path);
    if manifest.record_path.is_some() && !record_path.is_absolute() {
        return Err("frozen record path must be absolute");
    }
    let record_meta =
        std::fs::symlink_metadata(record_path).map_err(|_| "frozen record is missing")?;
    if !record_meta.is_file()
        || record_meta.file_type().is_symlink()
        || record_meta.len() != manifest.record_bytes
    {
        return Err("frozen record does not match its fixture manifest");
    }
    let record_stamp = file_stamp(record_path).ok_or("frozen record is unavailable")?;
    if file_digest(record_path).as_deref() != Some(&manifest.record_sha256.to_ascii_lowercase()) {
        return Err("frozen record does not match its fixture manifest");
    }
    if file_stamp(record_path).as_ref() != Some(&record_stamp) {
        return Err("frozen record changed during fixture verification");
    }
    match manifest.clock_basis.as_str() {
        "explicit_fixture" if manifest.clock_witness.is_none() => {}
        "record_event_ceiling" => verify_witness(record_path, &manifest)?,
        _ => return Err("record fixture clock basis is unavailable"),
    }
    Ok(FrozenVisualInput {
        record_stamp,
        record_path: record_path.to_path_buf(),
        manifest,
        manifest_sha256: digest(&body),
        manifest_path: path.to_path_buf(),
    })
}

fn verify_witness(db_path: &Path, manifest: &VisualFixtureManifest) -> Result<(), &'static str> {
    let witness = manifest
        .clock_witness
        .as_ref()
        .ok_or("record fixture clock witness is missing")?;
    if witness.event_id.len() > 160
        || witness.project_id.len() > 160
        || witness.session_id.len() > 160
        || !is_sha256(&witness.raw_sha256)
        || !is_sha256(&witness.chain_hash)
        || witness.seq < 0
        || witness.raw_line < 0
    {
        return Err("record fixture clock witness is invalid");
    }
    let conn = open_frozen_record(db_path).map_err(|_| "frozen record is unavailable")?;
    conn.pragma_update(None, "query_only", true)
        .map_err(|_| "frozen record is unavailable")?;
    let found = conn.query_row(
        "SELECT project_id, session_id, seq, raw_line, ts, raw_sha256, chain_hash FROM events WHERE id=?1",
        [&witness.event_id], |row| Ok(RecordClockWitness {
            event_id: witness.event_id.clone(), project_id: row.get(0)?, session_id: row.get(1)?,
            seq: row.get(2)?, raw_line: row.get(3)?, event_timestamp: row.get(4)?,
            raw_sha256: row.get(5)?, chain_hash: row.get(6)?,
        }),
    ).map_err(|_| "record fixture clock witness is not in the frozen record")?;
    if found != *witness {
        return Err("record fixture clock witness does not match the frozen record");
    }
    let timestamp = &witness.event_timestamp;
    let rounded: Option<String> = conn.query_row(
        "SELECT strftime('%Y-%m-%dT%H:%M:%SZ', ?1, CASE WHEN unixepoch(?1,'subsec') > unixepoch(?1) THEN '+1 second' ELSE '+0 seconds' END)",
        [timestamp], |row| row.get(0),
    ).map_err(|_| "record fixture clock witness is invalid")?;
    if rounded.as_deref() != Some(&manifest.capture_clock) {
        return Err("record fixture clock does not bound its recorded event");
    }
    Ok(())
}

/// Immutable SQLite URI avoids creating or changing WAL/SHM files beside the
/// preserved record. A nonempty WAL would be an unbound part of the fixture.
pub fn open_frozen_record(path: &Path) -> Result<Connection, rusqlite::Error> {
    let wal = PathBuf::from(format!("{}-wal", path.to_string_lossy()));
    if std::fs::metadata(wal).is_ok_and(|meta| meta.len() > 0) {
        return Err(rusqlite::Error::InvalidPath(path.to_path_buf()));
    }
    let mut uri = String::from("file:");
    for byte in path.as_os_str().as_encoded_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-_.:".contains(byte) {
            uri.push(*byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri.push_str("?immutable=1&mode=ro");
    Connection::open_with_flags(
        uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
}

pub fn file_digest(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let n = file.read(&mut buffer).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Some(hex(&hasher.finalize()))
}
pub fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn load_cached_fixture_manifest_at(
    db_path: &Path,
    path: &Path,
    cache: &super::visual_cache::VisualCache,
) -> Result<FrozenVisualInput, &'static str> {
    use super::visual_cache::{key, VerifiedFile};
    let meta = std::fs::symlink_metadata(path).map_err(|_| "record fixture manifest is missing")?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 16_384 {
        return Err("record fixture manifest is invalid");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err("record fixture manifest must be private");
        }
    }
    let body = std::fs::read(path).map_err(|_| "record fixture manifest is unavailable")?;
    let manifest: VisualFixtureManifest =
        serde_json::from_slice(&body).map_err(|_| "record fixture manifest is invalid")?;
    let record_path = manifest.record_path.as_deref().unwrap_or(db_path);
    let wal = PathBuf::from(format!("{}-wal", record_path.to_string_lossy()));
    if std::fs::metadata(wal).is_ok_and(|m| m.len() > 0) {
        return Err("frozen record has an unbound WAL");
    }
    let identity = key(&(path.to_path_buf(), digest(&body), record_path.to_path_buf()));
    if let Some(frozen) = cache.load::<FrozenVisualInput>("fixture", &identity) {
        return Ok(frozen);
    }
    let frozen = load_fixture_manifest_at(db_path, path)?;
    let record = VerifiedFile {
        path: frozen.record_path.clone(),
        stamp: frozen.record_stamp.clone(),
        sha256: frozen.manifest.record_sha256.clone(),
    };
    if let Some(manifest_file) = VerifiedFile::read(path) {
        let _ = cache.save_verified("fixture", &identity, &frozen, vec![record, manifest_file]);
    }
    Ok(frozen)
}
