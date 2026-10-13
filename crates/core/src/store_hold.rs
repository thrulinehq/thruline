//! The on-disk hold used while a project removal replaces the ledger file.
//!
//! Ordinary store opens use [`open_connection`]. They never inspect the hold's
//! age and never remove it. Explicit recovery can read [`HoldSnapshot`] before
//! deciding what to do with the old and new store files. These primitives do
//! not enable a removal or recovery command by themselves.

use std::ffi::OsString;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const MAX_HOLD_BYTES: u64 = 4096;

/// The only factory for connections to a live ledger. Keep the caller's flags,
/// then refuse a removal hold before the first pragma or SQL statement.
pub fn open_connection(db: &Path, flags: OpenFlags) -> Result<Connection> {
    open_sqlite(db, db, flags, None)
}

/// Preserve the importer's existing SQLite URI spelling without using the URI
/// itself as the filesystem path at which the removal hold is checked.
pub fn open_uri_connection(db: &Path, uri: &str, flags: OpenFlags) -> Result<Connection> {
    open_sqlite(db, Path::new(uri), flags, None)
}

fn open_sqlite(
    db: &Path,
    address: &Path,
    flags: OpenFlags,
    owner: Option<&StoreHold>,
) -> Result<Connection> {
    let before = before_sqlite_open(db)?;
    if let Some(owner) = owner {
        owner.authorize_current_source(db)?;
    }
    let conn = Connection::open_with_flags(address, flags)?;
    if let Some(owner) = owner {
        owner.authorize_current_source(db)?;
        check_open_identity(db, &conn, &before)?;
    } else {
        check_after_sqlite_open(db, &conn, &before)?;
    }
    Ok(conn)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HoldRecord {
    pub removal_id: String,
    pub pid: u32,
    pub phase: String,
    /// Unix seconds at hold creation; a heartbeat changes the file mtime only.
    pub started_at: u64,
}

#[derive(Clone, Debug)]
pub struct HoldSnapshot {
    pub record: HoldRecord,
    pub modified: SystemTime,
}

/// What the caller's path named immediately before SQLite opened it. An open
/// site captures this before `Connection::open*`, then calls
/// [`check_after_sqlite_open`] immediately after. Keeping the existing SQLite
/// open call preserves its flags and URI mode when no removal is active.
pub struct StoreOpenWitness {
    canonical: Option<PathBuf>,
    identity: Option<FileIdentity>,
}

pub fn before_sqlite_open(db: &Path) -> Result<StoreOpenWitness> {
    let metadata = match fs::metadata(db) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(StoreOpenWitness {
                canonical: None,
                identity: None,
            });
        }
        Err(error) => return Err(CoreError::io(db, error)),
    };
    let canonical = fs::canonicalize(db).map_err(|error| CoreError::io(db, error))?;
    Ok(StoreOpenWitness {
        canonical: Some(canonical),
        identity: Some(FileIdentity::from_metadata(&metadata)),
    })
}

/// Refuse a held live store before the caller's first pragma or SQL statement.
/// The hold is looked up beside the resolved database target, so a symlink
/// alias cannot name the same live store while missing its `.hold` file.
/// SQLite's own filename and the caller's path must still resolve to the same
/// inode, and an existing path must still name the inode seen before open.
pub fn check_after_sqlite_open(
    db: &Path,
    conn: &Connection,
    before: &StoreOpenWitness,
) -> Result<()> {
    let canonical = fs::canonicalize(db).map_err(|error| CoreError::io(db, error))?;
    if is_held(&canonical)? {
        return Err(CoreError::StoreHeld {
            path: canonical.display().to_string(),
        });
    }
    check_open_identity(db, conn, before)
}

fn check_open_identity(db: &Path, conn: &Connection, before: &StoreOpenWitness) -> Result<()> {
    let canonical = fs::canonicalize(db).map_err(|error| CoreError::io(db, error))?;
    let sqlite_path = conn.path().filter(|path| !path.is_empty()).ok_or_else(|| {
        CoreError::Integrity("SQLite did not report the opened store path".to_owned())
    })?;
    let sqlite_canonical =
        fs::canonicalize(sqlite_path).map_err(|error| CoreError::io(sqlite_path, error))?;
    let named = fs::metadata(&canonical).map_err(|error| CoreError::io(&canonical, error))?;
    let opened =
        fs::metadata(&sqlite_canonical).map_err(|error| CoreError::io(&sqlite_canonical, error))?;
    if canonical != sqlite_canonical
        || !same_file(&named, &opened)
        || before
            .canonical
            .as_ref()
            .is_some_and(|old| old != &canonical)
        || before.identity.is_some_and(|old| !old.matches(&named))
    {
        return Err(CoreError::Integrity(
            "store path changed while SQLite opened it".to_owned(),
        ));
    }
    Ok(())
}

impl HoldSnapshot {
    /// Recovery may use this signal, but ordinary opens must not.
    pub fn is_stale_at(&self, now: SystemTime, stale_after: Duration) -> bool {
        now.duration_since(self.modified)
            .is_ok_and(|age| age > stale_after)
    }
}

/// The hold's only owner. Dropping it stops its heartbeat but leaves the file
/// in place so an interrupted removal cannot silently reopen the store.
pub struct StoreHold {
    db_path: PathBuf,
    db_identity: FileIdentity,
    path: PathBuf,
    file: Arc<Mutex<File>>,
    record: HoldRecord,
    identity: FileIdentity,
    heartbeat_failed: Arc<AtomicBool>,
    stop: Sender<()>,
    heartbeat: Option<JoinHandle<()>>,
}

/// Append `.hold` to the database filename without interpreting any user text
/// as a path component. For `ledger.db`, this is `ledger.db.hold`.
pub fn hold_path(db: &Path) -> Result<PathBuf> {
    let name = db
        .file_name()
        .ok_or_else(|| CoreError::Config("store hold needs a database filename".to_owned()))?;
    let mut hold_name = OsString::from(name);
    hold_name.push(".hold");
    Ok(db.with_file_name(hold_name))
}

/// Pure read-only detection for live-store open sites. A
/// malformed or non-regular hold refuses the open rather than being ignored.
pub fn is_held(db: &Path) -> Result<bool> {
    let path = hold_path(db)?;
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(true),
        Ok(_) => Err(CoreError::Integrity(format!(
            "store hold is not a regular file: {}",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(CoreError::io(&path, error)),
    }
}

/// Read hold metadata for an explicit recovery operation. This function never
/// changes or removes the file, even when its heartbeat is stale.
pub fn read_hold(db: &Path) -> Result<Option<HoldSnapshot>> {
    let path = hold_path(db)?;
    let before = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(CoreError::io(&path, error)),
    };
    ensure_regular(&path, &before)?;
    ensure_single_link(&before, "store hold")?;
    if before.len() > MAX_HOLD_BYTES {
        return Err(CoreError::Integrity("store hold is too large".to_owned()));
    }

    let file = File::open(&path).map_err(|error| CoreError::io(&path, error))?;
    let opened = file
        .metadata()
        .map_err(|error| CoreError::io(&path, error))?;
    if !same_file(&before, &opened) {
        return Err(CoreError::Integrity(
            "store hold changed while it was opened".to_owned(),
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_HOLD_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| CoreError::io(&path, error))?;
    if bytes.len() as u64 > MAX_HOLD_BYTES {
        return Err(CoreError::Integrity("store hold is too large".to_owned()));
    }
    let record: HoldRecord = serde_json::from_slice(&bytes)?;
    validate_label(&record.removal_id, 128, "removal id")?;
    validate_label(&record.phase, 64, "removal phase")?;
    let modified = opened
        .modified()
        .map_err(|error| CoreError::io(&path, error))?;
    Ok(Some(HoldSnapshot { record, modified }))
}

impl StoreHold {
    /// Adopt the same hold inode only after its previous process has exited.
    /// Age alone is never authority, and a second adopter must lose the lock.
    pub fn resume_stale(db: &Path, id: &str, age: Duration) -> Result<Self> {
        validate_label(id, 128, "removal id")?;
        let db_metadata = fs::symlink_metadata(db).map_err(|error| CoreError::io(db, error))?;
        ensure_recovery_source(db, &db_metadata, id)?;
        let db_path = fs::canonicalize(db).map_err(|error| CoreError::io(db, error))?;
        let db_identity = FileIdentity::from_metadata(&db_metadata);
        let path = hold_path(&db_path)?;
        let metadata = fs::symlink_metadata(&path).map_err(|error| CoreError::io(&path, error))?;
        ensure_regular(&path, &metadata)?;
        ensure_single_link(&metadata, "store hold")?;
        ensure_private_owner(&metadata)?;
        let identity = FileIdentity::from_metadata(&metadata);
        let mut options = OpenOptions::new();
        options.read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(nix::libc::O_NOFOLLOW);
        }
        let mut file = options
            .open(&path)
            .map_err(|error| CoreError::io(&path, error))?;
        file.try_lock().map_err(|error| {
            CoreError::Integrity(format!("store hold already has an owner: {error}"))
        })?;
        ensure_owned_path(&path, &file, identity)?;
        let snapshot = read_hold(&db_path)?.ok_or_else(|| {
            CoreError::Integrity("store hold disappeared during recovery".to_owned())
        })?;
        if snapshot.record.removal_id != id
            || !snapshot.is_stale_at(SystemTime::now(), age.max(Duration::from_secs(120)))
        {
            return Err(CoreError::Integrity(
                "recovery needs the exact removal and a stale hold".to_owned(),
            ));
        }
        ensure_process_exited(snapshot.record.pid)?;
        ensure_owned_path(&path, &file, identity)?;
        let named =
            fs::symlink_metadata(&db_path).map_err(|error| CoreError::io(&db_path, error))?;
        ensure_recovery_source(&db_path, &named, id)?;
        if !db_identity.matches(&named) {
            return Err(CoreError::Integrity(
                "recovery store identity changed".to_owned(),
            ));
        }
        let mut record = snapshot.record;
        record.pid = std::process::id();
        let bytes = serde_json::to_vec(&record)?;
        file.seek(SeekFrom::Start(0))
            .and_then(|_| file.write_all(&bytes))
            .and_then(|()| file.set_len(bytes.len() as u64))
            .and_then(|()| file.sync_all())
            .map_err(|error| CoreError::io(&path, error))?;
        Self::start(db_path, db_identity, path, file, record)
    }

    /// Create a hold atomically with `O_CREAT | O_EXCL` and mode 0600. The
    /// caller must already have completed its read-only removal preflight.
    pub fn create(db: &Path, removal_id: &str, phase: &str) -> Result<Self> {
        validate_label(removal_id, 128, "removal id")?;
        validate_label(phase, 64, "removal phase")?;
        let db_metadata = fs::symlink_metadata(db).map_err(|error| CoreError::io(db, error))?;
        ensure_regular(db, &db_metadata)?;
        ensure_single_link(&db_metadata, "store database")?;
        let db_path = fs::canonicalize(db).map_err(|error| CoreError::io(db, error))?;
        let db_identity = FileIdentity::from_metadata(&db_metadata);
        let path = hold_path(db)?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let parent_metadata =
            fs::symlink_metadata(parent).map_err(|error| CoreError::io(parent, error))?;
        if !parent_metadata.file_type().is_dir() {
            return Err(CoreError::Integrity(
                "store hold parent is not a real directory".to_owned(),
            ));
        }

        let started_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| CoreError::Integrity("system clock predates Unix epoch".to_owned()))?
            .as_secs();
        let record = HoldRecord {
            removal_id: removal_id.to_owned(),
            pid: std::process::id(),
            phase: phase.to_owned(),
            started_at,
        };
        let bytes = serde_json::to_vec(&record)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&path)
            .map_err(|error| CoreError::io(&path, error))?;
        file.try_lock().map_err(|error| {
            CoreError::Integrity(format!("store hold already has an owner: {error}"))
        })?;
        // Once the name exists, any failure leaves the hold for explicit
        // recovery. It must not disappear while a coordinator may be active.
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| CoreError::io(&path, error))?;
        sync_parent(&path)?;
        Self::start(db_path, db_identity, path, file, record)
    }

    fn start(
        db_path: PathBuf,
        db_identity: FileIdentity,
        path: PathBuf,
        file: File,
        record: HoldRecord,
    ) -> Result<Self> {
        let metadata = file
            .metadata()
            .map_err(|error| CoreError::io(&path, error))?;
        let identity = FileIdentity::from_metadata(&metadata);
        let file = Arc::new(Mutex::new(file));
        let heartbeat_failed = Arc::new(AtomicBool::new(false));
        let (stop, receiver) = mpsc::channel();
        let heartbeat_file = Arc::clone(&file);
        let heartbeat_path = path.clone();
        let heartbeat_error = Arc::clone(&heartbeat_failed);
        let heartbeat = thread::Builder::new()
            .name("thruline-store-hold".to_owned())
            .spawn(move || loop {
                match receiver.recv_timeout(HEARTBEAT_INTERVAL) {
                    Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                    Err(RecvTimeoutError::Timeout) => {
                        let refreshed = heartbeat_file.lock().map_err(|_| ()).and_then(|file| {
                            refresh_file(&heartbeat_path, &file, identity).map_err(|_| ())
                        });
                        if refreshed.is_err() {
                            heartbeat_error.store(true, Ordering::Release);
                            break;
                        }
                    }
                }
            })
            .map_err(|error| CoreError::io(&path, error))?;
        Ok(Self {
            db_path,
            db_identity,
            path,
            file,
            record,
            identity,
            heartbeat_failed,
            stop,
            heartbeat: Some(heartbeat),
        })
    }

    pub fn record(&self) -> &HoldRecord {
        &self.record
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Only the live owner of this hold may open its source while all normal
    /// callers are refused. Both sides of the SQLite open recheck ownership.
    pub fn open_connection(&self, db: &Path, flags: OpenFlags) -> Result<Connection> {
        open_sqlite(db, db, flags, Some(self))
    }

    /// Attach only this owner's source to a writable candidate connection.
    /// An explicit URI mode and SQLite's reported mode both require read-only
    /// access; source identity and hold ownership are checked on both sides.
    pub fn attach_read_only(&self, conn: &Connection, source: &Path, schema: &str) -> Result<()> {
        if schema.is_empty()
            || !schema
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            || schema.eq_ignore_ascii_case("main")
            || schema.eq_ignore_ascii_case("temp")
        {
            return Err(CoreError::Integrity(
                "invalid removal source schema".to_owned(),
            ));
        }
        self.authorize_current_source(source)?;
        let canonical = fs::canonicalize(source).map_err(|error| CoreError::io(source, error))?;
        let mut uri = String::from("file:");
        for byte in canonical.as_os_str().as_encoded_bytes() {
            if byte.is_ascii_alphanumeric() || b"/-._~".contains(byte) {
                uri.push(char::from(*byte));
            } else {
                use std::fmt::Write as _;
                write!(uri, "%{byte:02X}")
                    .map_err(|_| CoreError::Integrity("cannot encode source URI".to_owned()))?;
            }
        }
        uri.push_str("?mode=ro");
        conn.execute(&format!("ATTACH DATABASE ?1 AS \"{schema}\""), [&uri])?;
        self.authorize_current_source(source)?;
        if !conn.is_readonly(schema)? {
            return Err(CoreError::Integrity(
                "removal source is not read-only".to_owned(),
            ));
        }
        let attached: String = conn.query_row(
            "SELECT file FROM pragma_database_list WHERE name=?1",
            [schema],
            |row| row.get(0),
        )?;
        let attached =
            fs::canonicalize(&attached).map_err(|error| CoreError::io(&attached, error))?;
        if attached != canonical {
            return Err(CoreError::Integrity(
                "attached removal source path changed".to_owned(),
            ));
        }
        self.authorize_current_source(&attached)
    }

    fn authorize_current_source(&self, db: &Path) -> Result<()> {
        let metadata = fs::symlink_metadata(db).map_err(|error| CoreError::io(db, error))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            self.authorize_source(db, metadata.dev(), metadata.ino())
        }
        #[cfg(not(unix))]
        {
            let _ = metadata;
            Err(CoreError::Integrity(
                "removal is not available on this system".to_owned(),
            ))
        }
    }

    /// After a proved candidate is renamed, bind this hold to precisely that
    /// candidate's identity, which the coordinator captured before the swap.
    pub fn adopt_swapped_source(&mut self, candidate_dev: u64, candidate_inode: u64) -> Result<()> {
        self.check_heartbeat()?;
        {
            let file = self.lock_file()?;
            ensure_owned_path(&self.path, &file, self.identity)?;
        }
        let metadata = fs::symlink_metadata(&self.db_path)
            .map_err(|error| CoreError::io(&self.db_path, error))?;
        ensure_regular(&self.db_path, &metadata)?;
        ensure_single_link(&metadata, "replacement store")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.dev() != candidate_dev || metadata.ino() != candidate_inode {
                return Err(CoreError::Integrity(
                    "replacement store identity changed".to_owned(),
                ));
            }
        }
        #[cfg(not(unix))]
        {
            let _ = (candidate_dev, candidate_inode);
            return Err(CoreError::Integrity(
                "removal is not available on this system".to_owned(),
            ));
        }
        self.db_identity = FileIdentity::from_metadata(&metadata);
        Ok(())
    }

    /// The removal owner's narrow exception for a read-only P3 source open.
    /// Ordinary open sites must continue to refuse this hold. The caller
    /// checks once before and once immediately after SQLite opens the source.
    pub(crate) fn authorize_source(&self, db: &Path, dev: u64, inode: u64) -> Result<()> {
        self.check_heartbeat()?;
        let hold_file = self.lock_file()?;
        ensure_owned_path(&self.path, &hold_file, self.identity)?;
        let canonical = fs::canonicalize(db).map_err(|error| CoreError::io(db, error))?;
        let named = fs::symlink_metadata(db).map_err(|error| CoreError::io(db, error))?;
        ensure_regular(db, &named)?;
        ensure_single_link(&named, "store database")?;
        #[cfg(unix)]
        let matches_expected = {
            use std::os::unix::fs::MetadataExt;
            named.dev() == dev && named.ino() == inode
        };
        #[cfg(not(unix))]
        let matches_expected = {
            let _ = (dev, inode);
            true
        };
        if canonical != self.db_path || !self.db_identity.matches(&named) || !matches_expected {
            return Err(CoreError::Integrity(
                "removal hold does not own this source store".to_owned(),
            ));
        }
        Ok(())
    }

    /// Refresh immediately in addition to the five-second background pulse.
    pub fn refresh(&self) -> Result<()> {
        self.check_heartbeat()?;
        let file = self.lock_file()?;
        refresh_file(&self.path, &file, self.identity)
    }

    /// Persist a new phase on the same inode, then sync it before proceeding.
    pub fn update_phase(&mut self, phase: &str) -> Result<()> {
        validate_label(phase, 64, "removal phase")?;
        self.check_heartbeat()?;
        let mut next = self.record.clone();
        next.phase = phase.to_owned();
        let bytes = serde_json::to_vec(&next)?;
        {
            let mut file = self.lock_file()?;
            ensure_owned_path(&self.path, &file, self.identity)?;
            file.seek(SeekFrom::Start(0))
                .and_then(|_| file.write_all(&bytes))
                .and_then(|()| file.set_len(bytes.len() as u64))
                .and_then(|()| file.sync_all())
                .map_err(|error| CoreError::io(&self.path, error))?;
        }
        self.record = next;
        Ok(())
    }

    /// Stop the heartbeat and remove only the inode this owner created. Any
    /// error before unlink leaves the hold for explicit recovery.
    pub fn release(mut self) -> Result<()> {
        self.stop_heartbeat();
        self.check_heartbeat()?;
        // Keep our file mutex across the final identity check, unlink and
        // directory sync. The heartbeat has stopped, and no owner method can
        // interleave its own write between the check and the unlink.
        let file = self.lock_file()?;
        ensure_owned_path(&self.path, &file, self.identity)?;
        fs::remove_file(&self.path).map_err(|error| CoreError::io(&self.path, error))?;
        sync_parent(&self.path)
    }

    fn check_heartbeat(&self) -> Result<()> {
        if self.heartbeat_failed.load(Ordering::Acquire) {
            Err(CoreError::Integrity(
                "store hold heartbeat failed; explicit recovery is required".to_owned(),
            ))
        } else {
            Ok(())
        }
    }

    fn lock_file(&self) -> Result<std::sync::MutexGuard<'_, File>> {
        self.file
            .lock()
            .map_err(|_| CoreError::Integrity("store hold file lock was poisoned".to_owned()))
    }

    fn stop_heartbeat(&mut self) {
        let _ = self.stop.send(());
        if let Some(heartbeat) = self.heartbeat.take() {
            let _ = heartbeat.join();
        }
    }
}

impl Drop for StoreHold {
    fn drop(&mut self) {
        self.stop_heartbeat();
    }
}

fn validate_label(value: &str, max_len: usize, name: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > max_len
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
    {
        return Err(CoreError::Config(format!("invalid store hold {name}")));
    }
    Ok(())
}

fn ensure_regular(path: &Path, metadata: &Metadata) -> Result<()> {
    if !metadata.file_type().is_file() {
        return Err(CoreError::Integrity(format!(
            "not a regular file: {}",
            path.display()
        )));
    }
    Ok(())
}

fn ensure_single_link(metadata: &Metadata, name: &str) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(CoreError::Integrity(format!(
                "{name} has an unexpected hard link"
            )));
        }
    }
    #[cfg(not(unix))]
    let _ = (metadata, name);
    Ok(())
}

fn ensure_private_owner(metadata: &Metadata) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() == rustix::process::getuid().as_raw() && metadata.mode() & 0o777 == 0o600
        {
            return Ok(());
        }
    }
    #[cfg(not(unix))]
    let _ = metadata;
    Err(CoreError::Integrity(
        "recovery needs a private hold owned by this account".to_owned(),
    ))
}

fn ensure_process_exited(pid: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use nix::errno::Errno;
        use nix::sys::signal::kill;
        use nix::unistd::Pid;
        let pid = i32::try_from(pid)
            .ok()
            .filter(|pid| *pid > 0)
            .ok_or_else(|| CoreError::Integrity("hold owner PID is invalid".to_owned()))?;
        if matches!(kill(Pid::from_raw(pid), None), Err(Errno::ESRCH)) {
            return Ok(());
        }
    }
    #[cfg(not(unix))]
    let _ = pid;
    Err(CoreError::Integrity(
        "hold owner is still alive or cannot be inspected".to_owned(),
    ))
}

fn ensure_recovery_source(db: &Path, metadata: &Metadata, id: &str) -> Result<()> {
    ensure_regular(db, metadata)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() == 1 {
            return Ok(());
        }
        if metadata.nlink() == 2 {
            let mut name = db
                .file_name()
                .ok_or_else(|| CoreError::Config("store has no filename".to_owned()))?
                .to_os_string();
            name.push(format!(".before-{id}"));
            let before_path = db.with_file_name(name);
            let before = fs::symlink_metadata(&before_path)
                .map_err(|error| CoreError::io(&before_path, error))?;
            ensure_regular(&before_path, &before)?;
            if before.nlink() == 2 && same_file(metadata, &before) {
                return Ok(());
            }
        }
    }
    #[cfg(not(unix))]
    let _ = id;
    Err(CoreError::Integrity(
        "recovery store has an unexpected hard link".to_owned(),
    ))
}

fn refresh_file(path: &Path, file: &File, identity: FileIdentity) -> Result<()> {
    ensure_owned_path(path, file, identity)?;
    file.set_modified(SystemTime::now())
        .and_then(|()| file.sync_all())
        .map_err(|error| CoreError::io(path, error))
}

fn ensure_owned_path(path: &Path, file: &File, identity: FileIdentity) -> Result<()> {
    let named = fs::symlink_metadata(path).map_err(|error| CoreError::io(path, error))?;
    ensure_regular(path, &named)?;
    ensure_single_link(&named, "store hold")?;
    let opened = file
        .metadata()
        .map_err(|error| CoreError::io(path, error))?;
    if !identity.matches(&named) || !identity.matches(&opened) {
        return Err(CoreError::Integrity(
            "store hold was replaced while removal was active".to_owned(),
        ));
    }
    Ok(())
}

fn sync_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let directory = File::open(parent).map_err(|error| CoreError::io(parent, error))?;
    directory
        .sync_all()
        .map_err(|error| CoreError::io(parent, error))
}

#[derive(Clone, Copy)]
struct FileIdentity {
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

impl FileIdentity {
    fn from_metadata(metadata: &Metadata) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Self {
                device: metadata.dev(),
                inode: metadata.ino(),
            }
        }
        #[cfg(not(unix))]
        {
            let _ = metadata;
            Self {}
        }
    }

    fn matches(self, metadata: &Metadata) -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            self.device == metadata.dev() && self.inode == metadata.ino()
        }
        #[cfg(not(unix))]
        {
            let _ = metadata;
            true
        }
    }
}

fn same_file(a: &Metadata, b: &Metadata) -> bool {
    FileIdentity::from_metadata(a).matches(b)
}
