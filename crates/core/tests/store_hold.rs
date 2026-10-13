use std::fs::{self, File};
use std::time::{Duration, SystemTime};

use rusqlite::{Connection, OpenFlags};
use tempfile::TempDir;
use thruline_core::store_hold::{hold_path, is_held, read_hold, StoreHold};

fn scratch_db() -> (TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().expect("scratch directory");
    let db = root.path().join("ledger.db");
    fs::write(&db, b"scratch ledger bytes").expect("scratch db");
    (root, db)
}

fn scratch_wal_db() -> (TempDir, std::path::PathBuf, Connection) {
    let root = tempfile::tempdir().expect("scratch directory");
    let db = root.path().join("ledger.db");
    let writer = Connection::open(&db).expect("scratch sqlite db");
    writer
        .busy_timeout(Duration::from_millis(100))
        .expect("short busy timeout");
    let mode: String = writer
        .query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))
        .expect("enter WAL mode");
    assert!(mode.eq_ignore_ascii_case("wal"));
    writer
        .execute_batch(
            "CREATE TABLE probe (id INTEGER PRIMARY KEY); INSERT INTO probe (id) VALUES (1);",
        )
        .expect("seed scratch db");
    (root, db, writer)
}

fn checkpoint_then_leave_wal(writer: &Connection) -> ((i64, i64, i64), rusqlite::Result<String>) {
    let checkpoint = writer
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .expect("checkpoint result");
    let journal_mode = writer.query_row("PRAGMA journal_mode=DELETE", [], |row| row.get(0));
    (checkpoint, journal_mode)
}

#[test]
fn no_hold_detection_is_read_only() {
    let (_root, db) = scratch_db();
    let before = fs::read(&db).expect("db bytes");
    let hold = hold_path(&db).expect("hold path");
    assert_eq!(
        hold.file_name().and_then(|name| name.to_str()),
        Some("ledger.db.hold")
    );
    assert!(!is_held(&db).expect("hold detection"));
    assert!(read_hold(&db).expect("hold read").is_none());
    assert!(!hold.exists());
    assert_eq!(fs::read(&db).expect("db bytes"), before);
}

#[test]
fn exclusive_create_phase_refresh_and_release() {
    let (_root, db) = scratch_db();
    let mut owner = StoreHold::create(&db, "removal-01", "P1").expect("create hold");
    let path = owner.path().to_path_buf();
    assert!(is_held(&db).expect("hold detection"));
    assert!(StoreHold::create(&db, "removal-02", "P1").is_err());
    let snapshot = read_hold(&db).expect("read hold").expect("present hold");
    assert_eq!(snapshot.record.removal_id, "removal-01");
    assert_eq!(snapshot.record.phase, "P1");
    assert_eq!(snapshot.record.pid, std::process::id());

    let file = File::options().write(true).open(&path).expect("open hold");
    let old = SystemTime::now() - Duration::from_secs(240);
    file.set_modified(old).expect("age hold");
    owner.refresh().expect("refresh hold");
    let refreshed = read_hold(&db).expect("read hold").expect("present hold");
    assert!(refreshed.modified > old);
    assert!(!refreshed.is_stale_at(SystemTime::now(), Duration::from_secs(120)));
    owner.update_phase("swapped").expect("update phase");
    assert_eq!(
        read_hold(&db)
            .expect("read hold")
            .expect("present hold")
            .record
            .phase,
        "swapped"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path)
            .expect("hold metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
    owner.release().expect("release hold");
    assert!(!is_held(&db).expect("hold detection"));
}

#[test]
fn dropping_owner_does_not_clear_an_interrupted_hold() {
    let (_root, db) = scratch_db();
    let owner = StoreHold::create(&db, "removal-01", "P3").expect("create hold");
    drop(owner);
    assert!(is_held(&db).expect("hold detection"));
    assert_eq!(
        read_hold(&db)
            .expect("read hold")
            .expect("present hold")
            .record
            .phase,
        "P3"
    );
}

#[test]
fn replacing_a_hold_cannot_change_or_remove_the_replacement() {
    let (_root, db) = scratch_db();
    let mut owner = StoreHold::create(&db, "removal-01", "P1").expect("create hold");
    let hold = hold_path(&db).expect("hold path");
    let displaced = db.with_file_name("displaced.hold");
    fs::rename(&hold, &displaced).expect("displace hold");
    fs::write(&hold, b"replacement").expect("replace hold");
    assert!(owner.update_phase("P2").is_err());
    assert!(owner.release().is_err());
    assert_eq!(fs::read(&hold).expect("replacement bytes"), b"replacement");
}

#[cfg(unix)]
#[test]
fn a_hard_link_prevents_owner_writes_and_unlink() {
    let (_root, db) = scratch_db();
    let mut owner = StoreHold::create(&db, "removal-01", "P1").expect("create hold");
    let hold = hold_path(&db).expect("hold path");
    let other_link = db.with_file_name("other.hold");
    fs::hard_link(&hold, &other_link).expect("hard link hold");
    assert!(owner.update_phase("P2").is_err());
    assert!(owner.release().is_err());
    assert!(hold.exists());
    assert!(other_link.exists());
}

#[cfg(unix)]
#[test]
fn a_hardlinked_database_cannot_acquire_a_hold() {
    let (root, db) = scratch_db();
    let alias = root.path().join("alias.db");
    fs::hard_link(&db, &alias).expect("hardlink scratch database");
    let original = fs::read(&db).expect("scratch database bytes");
    let problem = StoreHold::create(&db, "removal-01", "P1")
        .err()
        .expect("hardlinked database must refuse hold");
    assert!(problem.to_string().contains("store database"), "{problem}");
    assert!(!hold_path(&db).expect("hold path").exists());
    assert_eq!(fs::read(&alias).expect("alias bytes"), original);
}

#[test]
fn a_nonregular_hold_refuses_detection() {
    let (_root, db) = scratch_db();
    let hold = hold_path(&db).expect("hold path");
    fs::create_dir(&hold).expect("directory at hold path");
    assert!(is_held(&db).is_err());
    assert!(read_hold(&db).is_err());
}

#[cfg(unix)]
#[test]
fn symlinked_hold_and_database_are_refused() {
    use std::os::unix::fs::symlink;

    let (_root, db) = scratch_db();
    let hold = hold_path(&db).expect("hold path");
    symlink(&db, &hold).expect("symlinked hold");
    assert!(is_held(&db).is_err());
    assert!(read_hold(&db).is_err());
    fs::remove_file(&hold).expect("remove symlink");

    let link_db = db.with_file_name("linked.db");
    symlink(&db, &link_db).expect("symlinked db");
    assert!(StoreHold::create(&link_db, "removal-01", "P1").is_err());
    assert!(!hold_path(&link_db).expect("hold path").exists());
}

fn abandoned_hold(db: &std::path::Path, id: &str) -> std::path::PathBuf {
    let owner = StoreHold::create(db, id, "P3").expect("hold");
    let path = owner.path().to_path_buf();
    drop(owner);
    // A real, reaped child supplies a PID the OS confirms has exited.
    let mut child = std::process::Command::new("/usr/bin/true")
        .spawn()
        .expect("scratch child");
    let pid = child.id();
    assert!(child.wait().expect("reap child").success());
    let mut record = read_hold(db).expect("read").expect("hold").record;
    record.pid = pid;
    fs::write(&path, serde_json::to_vec(&record).expect("record")).expect("abandon");
    File::options()
        .write(true)
        .open(&path)
        .expect("hold file")
        .set_modified(SystemTime::now() - Duration::from_secs(240))
        .expect("age hold");
    path
}

#[test]
fn recovery_adopts_only_an_old_hold_whose_process_has_exited() {
    let (_root, db) = scratch_db();
    let path = abandoned_hold(&db, "removal-01");
    let before = fs::read(&db).expect("store bytes");
    let recovered = StoreHold::resume_stale(&db, "removal-01", Duration::from_secs(120))
        .expect("adopt stale hold");
    assert_eq!(recovered.record().pid, std::process::id());
    assert_eq!(recovered.record().phase, "P3");
    assert!(!read_hold(&db)
        .expect("read")
        .expect("hold")
        .is_stale_at(SystemTime::now(), Duration::from_secs(120)));
    assert!(StoreHold::resume_stale(&db, "removal-01", Duration::ZERO).is_err());
    recovered.release().expect("release adopted hold");
    assert!(!path.exists());
    assert_eq!(fs::read(&db).expect("store bytes"), before);
}

#[test]
fn recovery_refuses_a_live_pid_a_fresh_hold_and_a_different_removal() {
    let (_root, db) = scratch_db();
    let owner = StoreHold::create(&db, "removal-01", "P3").expect("hold");
    let path = owner.path().to_path_buf();
    drop(owner);
    File::options()
        .write(true)
        .open(&path)
        .expect("hold file")
        .set_modified(SystemTime::now() - Duration::from_secs(240))
        .expect("age hold");
    let held = fs::read(&path).expect("hold bytes");
    assert!(StoreHold::resume_stale(&db, "removal-01", Duration::from_secs(120)).is_err());
    assert_eq!(fs::read(&path).expect("hold bytes"), held);
    fs::remove_file(&path).expect("test-owned abandoned hold");
    let path = abandoned_hold(&db, "removal-02");
    assert!(StoreHold::resume_stale(&db, "removal-01", Duration::from_secs(120)).is_err());
    File::options()
        .write(true)
        .open(&path)
        .expect("hold file")
        .set_modified(SystemTime::now())
        .expect("fresh hold");
    assert!(StoreHold::resume_stale(&db, "removal-02", Duration::from_secs(120)).is_err());
}

#[cfg(unix)]
#[test]
fn recovery_allows_only_the_exact_pre_swap_hard_link_pair() {
    let (root, db) = scratch_db();
    abandoned_hold(&db, "removal-01");
    let unknown = root.path().join("unknown.db");
    fs::hard_link(&db, &unknown).expect("unknown hard link");
    assert!(StoreHold::resume_stale(&db, "removal-01", Duration::from_secs(120)).is_err());
    fs::rename(&unknown, root.path().join("ledger.db.before-removal-01")).expect("expected pair");
    let owner = StoreHold::resume_stale(&db, "removal-01", Duration::from_secs(120))
        .expect("exact pair may recover");
    // Recovery must reconcile this link before any SQLite connection is opened.
    assert!(owner
        .open_connection(&db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .is_err());
    owner.release().expect("release hold");
}

#[test]
fn held_source_attachment_is_read_only_and_candidate_is_writable() {
    let root = tempfile::tempdir().expect("temp store");
    let db = root.path().join("source ?#%.db");
    let source = Connection::open(&db).expect("source");
    source
        .execute_batch("CREATE TABLE probe(id INTEGER); INSERT INTO probe VALUES (7)")
        .expect("seed");
    drop(source);
    let before = fs::read(&db).expect("source bytes");
    let owner = StoreHold::create(&db, "attachment-test", "P3").expect("hold");
    let candidate = thruline_core::store_hold::open_connection(
        &root.path().join("candidate.db"),
        OpenFlags::default(),
    )
    .expect("candidate");
    owner
        .attach_read_only(&candidate, &db, "src")
        .expect("attach held source");
    assert!(candidate.is_readonly("src").expect("source mode"));
    candidate
        .execute_batch("CREATE TABLE main.probe AS SELECT * FROM src.probe")
        .expect("write candidate from source");
    assert_eq!(
        candidate
            .query_row("SELECT id FROM main.probe", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        7
    );
    assert!(candidate
        .execute("INSERT INTO src.probe VALUES (8)", [])
        .is_err());
    drop(candidate);
    owner.release().expect("release hold");
    assert_eq!(fs::read(&db).expect("source bytes"), before);
}

#[test]
fn source_attachment_refuses_another_store_or_an_invalid_schema() {
    let root = tempfile::tempdir().expect("temp store");
    let db = root.path().join("source.db");
    let other = root.path().join("other.db");
    for path in [&db, &other] {
        Connection::open(path)
            .expect("source")
            .execute_batch("CREATE TABLE probe(id INTEGER)")
            .expect("seed");
    }
    let before = fs::read(&db).expect("source bytes");
    let owner = StoreHold::create(&db, "attachment-test", "P3").expect("hold");
    let candidate = thruline_core::store_hold::open_connection(
        &root.path().join("candidate.db"),
        OpenFlags::default(),
    )
    .expect("candidate");
    assert!(owner.attach_read_only(&candidate, &other, "src").is_err());
    assert!(owner
        .attach_read_only(&candidate, &db, "src; SELECT 1")
        .is_err());
    assert!(owner.attach_read_only(&candidate, &db, "main").is_err());
    let databases: i64 = candidate
        .query_row("SELECT count(*) FROM pragma_database_list", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(databases, 1);
    drop(candidate);
    owner.release().expect("release hold");
    assert_eq!(fs::read(&db).expect("source bytes"), before);
}

/// Historical C47 observation: an initialized reader obstructs this transition.
/// It does not establish ownership; C48 requires the OS census as well.
#[test]
fn t10_idle_second_connection_blocks_the_swap() {
    let (_root, db, writer) = scratch_wal_db();
    let idle_reader = Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("second read-only connection");
    let _: i64 = idle_reader
        .query_row("SELECT COUNT(*) FROM probe", [], |row| row.get(0))
        .expect("idle reader initialized");

    let (checkpoint, mode) = checkpoint_then_leave_wal(&writer);
    let transition_succeeded = mode
        .as_ref()
        .is_ok_and(|value| value.eq_ignore_ascii_case("delete"));
    assert!(
        checkpoint.0 != 0 || !transition_succeeded,
        "C47 A1 disproved: idle second connection allowed checkpoint {checkpoint:?} and journal_mode {mode:?}"
    );
}

/// A reader with an older WAL snapshot must also block the proposed swap.
#[test]
fn t10_active_reader_blocks_the_swap() {
    let (_root, db, writer) = scratch_wal_db();
    let reader = Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("second read-only connection");
    reader.execute_batch("BEGIN DEFERRED").expect("begin read");
    let _: i64 = reader
        .query_row("SELECT COUNT(*) FROM probe", [], |row| row.get(0))
        .expect("establish read snapshot");
    writer
        .execute("INSERT INTO probe (id) VALUES (2)", [])
        .expect("advance WAL beyond reader snapshot");

    let (checkpoint, mode) = checkpoint_then_leave_wal(&writer);
    let transition_succeeded = mode
        .as_ref()
        .is_ok_and(|value| value.eq_ignore_ascii_case("delete"));
    assert!(
        checkpoint.0 != 0 || !transition_succeeded,
        "C47 A1 disproved: active second connection allowed checkpoint {checkpoint:?} and journal_mode {mode:?}"
    );
    reader
        .execute_batch("ROLLBACK")
        .expect("close read snapshot");
}

/// Retain the experiment that disproved C47 A1. An open-only reader can survive
/// a successful checkpoint and journal transition; neither proves zero holders.
#[test]
fn journal_transition_can_succeed_while_an_open_only_connection_remains() {
    let (_root, db, writer) = scratch_wal_db();
    let _opened = Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("second connection, with no statement yet");

    let wal = db.with_file_name("ledger.db-wal");
    let shm = db.with_file_name("ledger.db-shm");
    let before = (wal.exists(), shm.exists());
    let (checkpoint, mode) = checkpoint_then_leave_wal(&writer);
    let after = (wal.exists(), shm.exists());
    let transition_succeeded = mode
        .as_ref()
        .is_ok_and(|value| value.eq_ignore_ascii_case("delete"));
    eprintln!("C48 retained A1 experiment: checkpoint={checkpoint:?}, mode={mode:?}, WAL/SHM before={before:?}, after={after:?}; open-only reader is still alive");
    assert_eq!(checkpoint, (0, 0, 0));
    assert!(transition_succeeded, "{mode:?}");
    assert_eq!(before, (true, true));
    assert_eq!(after, (false, false));
}
