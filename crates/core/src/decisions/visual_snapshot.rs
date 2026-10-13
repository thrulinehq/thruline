//! Exact-byte snapshots of an already pinned frozen record, kept internally.
//! Unsupported clone semantics abstain; this never repairs or copies a WAL.
use std::path::Path;

use super::visual_inputs::FileStamp;

#[derive(Debug)]
pub struct SnapshotReceipt {
    pub method: &'static str,
    pub sha256: String,
    pub bytes: u64,
}

/// `expected_stamp` and SHA come from the independently verified fixture.
/// The descriptor-relative clone syscall has no allocating fallback. Hashing
/// checks its deadline between reads; an individual kernel call has no promised
/// wall-clock bound.
pub fn snapshot_frozen(
    source: &Path,
    destination: &Path,
    expected_stamp: &FileStamp,
    expected_sha256: &str,
) -> Result<SnapshotReceipt, &'static str> {
    #[cfg(target_os = "macos")]
    {
        macos::snapshot(source, destination, expected_stamp, expected_sha256)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (source, destination, expected_stamp, expected_sha256);
        Err("exact internal record cloning is unavailable on this platform")
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::fs::{File, Metadata};
    use std::io::Read;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::{Component, PathBuf};
    use std::time::{Duration, Instant};

    use rustix::fs::{
        fclonefileat, fstatfs, open, renameat_with, CloneFlags, Mode, OFlags, RenameFlags,
    };
    use sha2::{Digest, Sha256};

    #[derive(Debug, PartialEq, Eq)]
    struct Identity {
        stamp: FileStamp,
        uid: u32,
        mode: u32,
        links: u64,
    }
    fn identity(meta: &Metadata) -> Identity {
        Identity {
            stamp: FileStamp {
                bytes: meta.len(),
                modified_ns: meta
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .and_then(|time| u64::try_from(time.as_nanos()).ok()),
                device: meta.dev(),
                inode: meta.ino(),
                changed_seconds: meta.ctime(),
                changed_nanoseconds: meta.ctime_nsec(),
            },
            uid: meta.uid(),
            mode: meta.mode(),
            links: meta.nlink(),
        }
    }
    fn regular(path: &Path) -> Result<(File, Identity), &'static str> {
        let before =
            std::fs::symlink_metadata(path).map_err(|_| "frozen snapshot file is unavailable")?;
        let uid = rustix::process::getuid().as_raw();
        if !before.is_file()
            || before.file_type().is_symlink()
            || before.uid() != uid
            || before.mode() & 0o077 != 0
            || before.nlink() != 1
        {
            return Err("frozen snapshot requires a private regular owned file");
        }
        let file = File::from(
            open(
                path,
                OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| "frozen snapshot file could not be opened")?,
        );
        let named = identity(&before);
        let held = identity(
            &file
                .metadata()
                .map_err(|_| "frozen snapshot descriptor is unavailable")?,
        );
        if held != named {
            return Err("frozen snapshot file changed during open");
        }
        Ok((file, held))
    }
    fn remains(path: &Path, file: &File, expected: &Identity) -> bool {
        std::fs::symlink_metadata(path).is_ok_and(|meta| identity(&meta) == *expected)
            && file
                .metadata()
                .is_ok_and(|meta| identity(&meta) == *expected)
    }
    fn wal_identity(source: &Path) -> Result<Option<Identity>, &'static str> {
        let mut name = source.as_os_str().to_os_string();
        name.push("-wal");
        let path = PathBuf::from(name);
        match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err("frozen record WAL identity is unavailable"),
            Ok(_) => {
                let (_, found) = regular(&path)?;
                if found.stamp.bytes != 0 {
                    return Err("frozen record has an unbound nonempty WAL");
                }
                Ok(Some(found))
            }
        }
    }
    fn exact_path(path: &Path) -> bool {
        path.is_absolute()
            && !path
                .components()
                .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    }
    fn directory(path: &Path, internal: u64) -> Result<File, &'static str> {
        let named =
            std::fs::symlink_metadata(path).map_err(|_| "snapshot work folder is unavailable")?;
        if !named.is_dir()
            || named.file_type().is_symlink()
            || named.uid() != rustix::process::getuid().as_raw()
            || named.mode() & 0o077 != 0
            || named.dev() != internal
        {
            return Err("snapshot requires a private owned internal work folder");
        }
        let held = File::from(
            open(
                path,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| "snapshot work folder could not be opened")?,
        );
        if !directory_matches(path, &held) {
            return Err("snapshot work folder changed during open");
        }
        Ok(held)
    }
    fn directory_matches(path: &Path, held: &File) -> bool {
        let Ok(named) = std::fs::symlink_metadata(path) else {
            return false;
        };
        let Ok(meta) = held.metadata() else {
            return false;
        };
        named.is_dir()
            && !named.file_type().is_symlink()
            && named.dev() == meta.dev()
            && named.ino() == meta.ino()
            && named.uid() == meta.uid()
            && named.mode() == meta.mode()
            && named.mode() & 0o077 == 0
            && named.uid() == rustix::process::getuid().as_raw()
    }
    fn digest(file: &mut File, bytes: u64) -> Result<String, &'static str> {
        let deadline = Instant::now() + Duration::from_secs(5 * 60);
        let mut sha = Sha256::new();
        let mut buffer = [0; 1024 * 1024];
        let mut total = 0_u64;
        loop {
            if Instant::now() >= deadline {
                return Err("snapshot verification exceeded its deadline");
            }
            let n = file
                .read(&mut buffer)
                .map_err(|_| "snapshot bytes could not be read")?;
            if n == 0 {
                break;
            }
            total = total
                .checked_add(n as u64)
                .ok_or("snapshot size overflow")?;
            if total > bytes {
                return Err("snapshot grew during verification");
            }
            sha.update(&buffer[..n]);
        }
        if total != bytes {
            return Err("snapshot bytes do not match the pinned size");
        }
        Ok(sha
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }

    pub(super) fn snapshot(
        source: &Path,
        destination: &Path,
        expected_stamp: &FileStamp,
        expected_sha256: &str,
    ) -> Result<SnapshotReceipt, &'static str> {
        if !exact_path(source)
            || !exact_path(destination)
            || destination.file_name() != Some("ledger.db".as_ref())
            || expected_sha256.len() != 64
            || !expected_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("snapshot paths or exact lowercase SHA pin are invalid");
        }
        let internal = std::fs::metadata("/System/Volumes/Data")
            .map_err(|_| "internal record volume is unavailable")?
            .dev();
        let parent = destination
            .parent()
            .ok_or("snapshot parent is unavailable")?;
        let work = directory(parent, internal)?;
        match std::fs::symlink_metadata(destination) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Err("snapshot destination must be absent"),
        }
        let (source_file, source_id) = regular(source)?;
        if source_id.stamp != *expected_stamp || source_id.stamp.device != internal {
            return Err("frozen record does not match its pinned internal identity");
        }
        let source_parent = source
            .parent()
            .ok_or("frozen record parent is unavailable")?;
        let parent_meta = std::fs::symlink_metadata(source_parent)
            .map_err(|_| "frozen record parent is unavailable")?;
        if !parent_meta.is_dir() || parent_meta.file_type().is_symlink() {
            return Err("frozen record parent must be a real folder");
        }
        let wal = wal_identity(source)?;
        let frozen = super::super::visual_inputs::open_frozen_record(source)
            .map_err(|_| "frozen record could not be opened immutably")?;
        let _: i64 = frozen
            .pragma_query_value(None, "schema_version", |row| row.get(0))
            .map_err(|_| "frozen record is not a usable SQLite file")?;
        let stage = tempfile::Builder::new()
            .prefix("record-clone-")
            .tempdir_in(parent)
            .map_err(|_| "private clone staging is unavailable")?;
        std::fs::set_permissions(stage.path(), std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "private clone staging is unavailable")?;
        let stage_dir = directory(stage.path(), internal)?;
        let staged = stage.path().join("ledger.db");
        let fs = fstatfs(&source_file).map_err(|_| "frozen record filesystem is unavailable")?;
        if !fs
            .f_fstypename
            .iter()
            .take_while(|byte| **byte != 0)
            .map(|byte| *byte as u8)
            .eq(b"apfs".iter().copied())
        {
            return Err("exact internal APFS record cloning is unavailable");
        }
        fclonefileat(
            &source_file,
            &stage_dir,
            "ledger.db",
            CloneFlags::NOFOLLOW | CloneFlags::NOOWNERCOPY,
        )
        .map_err(|_| "exact internal record clone is unavailable; no allocating fallback used")?;
        let (mut clone, _) = regular(&staged)?;
        clone
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|_| "clone privacy could not be established")?;
        let clone_id = identity(
            &clone
                .metadata()
                .map_err(|_| "clone descriptor is unavailable")?,
        );
        if clone_id.stamp.device != internal
            || clone_id.stamp.inode == source_id.stamp.inode
            || clone_id.stamp.bytes != expected_stamp.bytes
        {
            return Err("clone must be an independent exact internal file");
        }
        let sha = digest(&mut clone, expected_stamp.bytes)?;
        if sha != expected_sha256
            || !remains(&staged, &clone, &clone_id)
            || !remains(source, &source_file, &source_id)
            || wal_identity(source)? != wal
            || !directory_matches(parent, &work)
            || !directory_matches(stage.path(), &stage_dir)
        {
            return Err("frozen source or clone changed, or clone differs from its exact pin");
        }
        // Exclusive descriptor-relative publication never overwrites an existing path.
        renameat_with(
            &stage_dir,
            "ledger.db",
            &work,
            "ledger.db",
            RenameFlags::NOREPLACE,
        )
        .map_err(|_| "snapshot destination changed before publication")?;
        let published = identity(
            &clone
                .metadata()
                .map_err(|_| "snapshot descriptor is unavailable")?,
        );
        if !remains(destination, &clone, &published)
            || published.stamp.inode != clone_id.stamp.inode
            || published.stamp.device != clone_id.stamp.device
            || published.stamp.bytes != clone_id.stamp.bytes
            || published.stamp.modified_ns != clone_id.stamp.modified_ns
            || published.uid != clone_id.uid
            || published.mode != clone_id.mode
            || published.links != 1
            || published.mode & 0o777 != 0o600
            || !remains(source, &source_file, &source_id)
            || wal_identity(source)? != wal
            || !directory_matches(parent, &work)
        {
            return Err("snapshot or source changed during publication");
        }
        Ok(SnapshotReceipt {
            method: "apfs_clone",
            sha256: sha,
            bytes: expected_stamp.bytes,
        })
    }
}
