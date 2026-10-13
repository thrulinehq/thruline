#![cfg(target_os = "macos")]
//! Frozen-record cloning preserves exact bytes and refuses unbound inputs.
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::PathBuf;
use thruline_core::decisions::checks_visual::visual_inputs::{file_digest, file_stamp, FileStamp};
use thruline_core::decisions::checks_visual::visual_snapshot::snapshot_frozen;

struct Fixture {
    _dir: tempfile::TempDir,
    source: PathBuf,
    work: PathBuf,
    destination: PathBuf,
    stamp: FileStamp,
    sha: String,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("frozen.db");
        let conn = rusqlite::Connection::open(&source).unwrap();
        conn.execute_batch("CREATE TABLE events(id INTEGER PRIMARY KEY, body BLOB); INSERT INTO events(body) VALUES(zeroblob(32768)); INSERT INTO events(body) VALUES(zeroblob(32768)); DELETE FROM events WHERE id=2;").unwrap();
        drop(conn);
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o600)).unwrap();
        let work = dir.path().join("capture");
        std::fs::create_dir(&work).unwrap();
        std::fs::set_permissions(&work, std::fs::Permissions::from_mode(0o700)).unwrap();
        let destination = work.join("ledger.db");
        let stamp = file_stamp(&source).unwrap();
        let sha = file_digest(&source).unwrap();
        Self {
            _dir: dir,
            source,
            work,
            destination,
            stamp,
            sha,
        }
    }
    fn run(
        &self,
    ) -> Result<
        thruline_core::decisions::checks_visual::visual_snapshot::SnapshotReceipt,
        &'static str,
    > {
        snapshot_frozen(&self.source, &self.destination, &self.stamp, &self.sha)
    }
    fn refused(&self) {
        assert!(self.run().is_err());
        assert!(!self.destination.exists());
    }
}

#[test]
fn snapshot_contract_exact_bytes_private_and_independent_clone() {
    let f = Fixture::new();
    let receipt = f.run().unwrap();
    assert_eq!(receipt.sha256, f.sha);
    assert_eq!(receipt.bytes, f.stamp.bytes);
    assert_eq!(receipt.method, "apfs_clone");
    assert_eq!(file_stamp(&f.source), Some(f.stamp.clone()));
    assert_eq!(file_digest(&f.source).as_deref(), Some(f.sha.as_str()));
    let src = std::fs::symlink_metadata(&f.source).unwrap();
    let dst = std::fs::symlink_metadata(&f.destination).unwrap();
    assert!(dst.is_file() && !dst.file_type().is_symlink());
    assert_eq!(dst.dev(), src.dev());
    assert_ne!(dst.ino(), src.ino());
    assert_eq!(dst.nlink(), 1);
    assert_eq!(dst.uid(), src.uid());
    assert_eq!(dst.permissions().mode() & 0o777, 0o600);
    assert_eq!(
        std::fs::read(&f.destination).unwrap(),
        std::fs::read(&f.source).unwrap()
    );
    std::fs::write(&f.source, b"changed synthetic source").unwrap();
    assert_eq!(file_digest(&f.destination).as_deref(), Some(f.sha.as_str()));
}

#[test]
fn snapshot_contract_wrong_pin_refuses_without_destination() {
    let f = Fixture::new();
    assert!(snapshot_frozen(&f.source, &f.destination, &f.stamp, &"0".repeat(64)).is_err());
    assert!(!f.destination.exists());
    assert_eq!(file_stamp(&f.source), Some(f.stamp));
}

#[test]
fn changed_expected_source_identity_is_refused() {
    let mut f = Fixture::new();
    f.stamp.inode += 1;
    f.refused();
}

#[test]
fn nonempty_wal_is_refused_without_source_repair() {
    let f = Fixture::new();
    let wal = PathBuf::from(format!("{}-wal", f.source.display()));
    std::fs::write(&wal, b"unbound").unwrap();
    std::fs::set_permissions(&wal, std::fs::Permissions::from_mode(0o600)).unwrap();
    let before = file_stamp(&wal).unwrap();
    f.refused();
    assert_eq!(file_stamp(&wal), Some(before));
    assert_eq!(std::fs::read(&wal).unwrap(), b"unbound");
}

#[test]
fn empty_regular_wal_remains_empty_and_unchanged() {
    let f = Fixture::new();
    let wal = PathBuf::from(format!("{}-wal", f.source.display()));
    std::fs::write(&wal, b"").unwrap();
    std::fs::set_permissions(&wal, std::fs::Permissions::from_mode(0o600)).unwrap();
    let before = file_stamp(&wal).unwrap();
    assert_eq!(f.run().unwrap().sha256, f.sha);
    assert_eq!(file_stamp(&wal), Some(before));
}

#[test]
fn symlink_wal_is_refused_even_when_target_empty() {
    let f = Fixture::new();
    let empty = f.work.join("empty");
    std::fs::write(&empty, b"").unwrap();
    symlink(&empty, format!("{}-wal", f.source.display())).unwrap();
    f.refused();
}

#[test]
fn symlink_source_is_refused() {
    let f = Fixture::new();
    let alias = f.work.join("alias.db");
    symlink(&f.source, &alias).unwrap();
    assert!(snapshot_frozen(&alias, &f.destination, &f.stamp, &f.sha).is_err());
    assert!(!f.destination.exists());
}

#[test]
fn special_source_is_refused_without_waiting_for_writer() {
    let f = Fixture::new();
    std::fs::remove_file(&f.source).unwrap();
    assert!(std::process::Command::new("/usr/bin/mkfifo")
        .arg("-m")
        .arg("600")
        .arg(&f.source)
        .status()
        .unwrap()
        .success());
    let start = std::time::Instant::now();
    f.refused();
    assert!(start.elapsed() < std::time::Duration::from_secs(2));
}

#[test]
fn directory_source_is_refused() {
    let f = Fixture::new();
    assert!(snapshot_frozen(&f.work, &f.destination, &f.stamp, &f.sha).is_err());
    assert!(!f.destination.exists());
}

#[test]
fn existing_destination_is_never_overwritten() {
    let f = Fixture::new();
    std::fs::write(&f.destination, b"keep").unwrap();
    assert!(f.run().is_err());
    assert_eq!(std::fs::read(&f.destination).unwrap(), b"keep");
}

#[test]
fn symlink_destination_is_never_followed() {
    let f = Fixture::new();
    let other = f.work.join("other");
    std::fs::write(&other, b"keep").unwrap();
    symlink(&other, &f.destination).unwrap();
    assert!(f.run().is_err());
    assert_eq!(std::fs::read(&other).unwrap(), b"keep");
}

#[test]
fn public_destination_folder_is_refused() {
    let f = Fixture::new();
    std::fs::set_permissions(&f.work, std::fs::Permissions::from_mode(0o755)).unwrap();
    f.refused();
}

#[test]
fn symlink_destination_folder_is_refused() {
    let f = Fixture::new();
    let alias = f._dir.path().join("alias");
    symlink(&f.work, &alias).unwrap();
    assert!(snapshot_frozen(&f.source, &alias.join("ledger.db"), &f.stamp, &f.sha).is_err());
    assert!(!f.destination.exists());
}

#[test]
fn noncanonical_destination_name_is_refused() {
    let f = Fixture::new();
    assert!(snapshot_frozen(&f.source, &f.work.join("other.db"), &f.stamp, &f.sha).is_err());
    assert!(!f.work.join("other.db").exists());
}

#[test]
fn broad_source_permissions_are_refused() {
    let mut f = Fixture::new();
    std::fs::set_permissions(&f.source, std::fs::Permissions::from_mode(0o644)).unwrap();
    f.stamp = file_stamp(&f.source).unwrap();
    f.refused();
}

#[test]
fn invalid_sqlite_source_is_refused_even_with_matching_pin() {
    let mut f = Fixture::new();
    std::fs::write(&f.source, b"unusable record").unwrap();
    f.stamp = file_stamp(&f.source).unwrap();
    f.sha = file_digest(&f.source).unwrap();
    f.refused();
}

#[test]
fn malformed_or_nonlowercase_pin_is_refused() {
    let f = Fixture::new();
    for pin in ["bad".to_string(), f.sha.to_ascii_uppercase()] {
        assert!(snapshot_frozen(&f.source, &f.destination, &f.stamp, &pin).is_err());
        assert!(!f.destination.exists());
    }
}

#[test]
fn parent_components_cannot_escape_snapshot_scope() {
    let f = Fixture::new();
    let escaped = f.work.join("../capture/ledger.db");
    assert!(snapshot_frozen(&f.source, &escaped, &f.stamp, &f.sha).is_err());
    assert!(!f.destination.exists());
}

#[test]
fn source_parent_components_are_not_an_exact_named_path() {
    let f = Fixture::new();
    assert!(snapshot_frozen(
        &f.work.join("../frozen.db"),
        &f.destination,
        &f.stamp,
        &f.sha
    )
    .is_err());
    assert!(!f.destination.exists());
}
