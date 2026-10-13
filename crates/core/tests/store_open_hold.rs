use std::fs;

use rusqlite::{Connection, OpenFlags};
use tempfile::tempdir;
use thruline_core::error::CoreError;
use thruline_core::ledger::Ledger;
use thruline_core::store_hold::{before_sqlite_open, check_after_sqlite_open, StoreHold};
use thruline_core::upgrade::{row_census, UpgradeState};

#[test]
fn no_hold_keeps_the_live_store_bytes_and_open_modes() {
    let root = tempdir().expect("scratch directory");
    let db = root.path().join("ledger.db");
    drop(Ledger::open(&db).expect("create scratch ledger"));
    let before = fs::read(&db).expect("store bytes");

    assert!(Ledger::open(&db)
        .expect("ordinary open")
        .took_schema_fast_path());
    assert_eq!(
        Ledger::upgrade_state(&db).expect("state"),
        UpgradeState::Current
    );
    assert!(Ledger::open_upgrading(&db, &mut |_, _, _| {})
        .expect("upgrading open")
        .took_schema_fast_path());
    let read_only = Ledger::open_existing_read_only(&db).expect("read-only open");
    assert!(read_only.took_schema_fast_path());
    assert!(read_only
        .conn()
        .is_readonly(rusqlite::MAIN_DB)
        .expect("mode"));
    drop(read_only);
    assert!(!row_census(&db).expect("census").is_empty());
    assert_eq!(fs::read(&db).expect("store bytes"), before);
}

#[test]
fn observational_open_refuses_an_unstamped_schema_without_writing_it() {
    let root = tempdir().expect("scratch directory");
    let db = root.path().join("old.db");
    let conn = Connection::open(&db).expect("scratch SQLite file");
    conn.execute_batch("CREATE TABLE old_record(id INTEGER PRIMARY KEY);")
        .expect("unstamped schema");
    drop(conn);
    let before = fs::read(&db).expect("store bytes");
    assert!(matches!(
        Ledger::open_existing_read_only(&db),
        Err(CoreError::UpgradePending { .. })
    ));
    assert_eq!(fs::read(&db).expect("store bytes"), before);
}

#[test]
fn held_store_refuses_all_core_open_sites_before_schema_work() {
    let root = tempdir().expect("scratch directory");
    let db = root.path().join("ledger.db");
    drop(Ledger::open(&db).expect("create scratch ledger"));
    let bytes = fs::read(&db).expect("store bytes");
    let hold = StoreHold::create(&db, "removal-01", "P1").expect("hold");

    assert!(matches!(
        Ledger::open(&db),
        Err(CoreError::StoreHeld { .. })
    ));
    assert!(matches!(
        Ledger::upgrade_state(&db),
        Err(CoreError::StoreHeld { .. })
    ));
    assert!(matches!(
        Ledger::open_upgrading(&db, &mut |_, _, _| {}),
        Err(CoreError::StoreHeld { .. })
    ));
    assert!(matches!(
        Ledger::open_existing_read_only(&db),
        Err(CoreError::StoreHeld { .. })
    ));
    assert!(matches!(row_census(&db), Err(CoreError::StoreHeld { .. })));
    assert_eq!(fs::read(&db).expect("store bytes"), bytes);
    hold.release().expect("release");
    assert!(Ledger::open(&db).is_ok());
}

#[cfg(unix)]
#[test]
fn symlink_alias_cannot_bypass_a_target_store_hold() {
    let root = tempdir().expect("scratch directory");
    let db = root.path().join("ledger.db");
    let alias = root.path().join("alias.db");
    drop(Ledger::open(&db).expect("create scratch ledger"));
    std::os::unix::fs::symlink(&db, &alias).expect("alias");
    let hold = StoreHold::create(&db, "removal-01", "P1").expect("hold");
    assert!(matches!(
        Ledger::open(&alias),
        Err(CoreError::StoreHeld { .. })
    ));
    assert!(matches!(
        row_census(&alias),
        Err(CoreError::StoreHeld { .. })
    ));
    hold.release().expect("release");
    assert!(Ledger::open(&alias).is_ok());
}

#[test]
fn a_hold_created_after_sqlite_open_is_seen_before_first_statement() {
    let root = tempdir().expect("scratch directory");
    let db = root.path().join("ledger.db");
    drop(Connection::open(&db).expect("create scratch SQLite file"));
    let before = before_sqlite_open(&db).expect("witness");
    let conn = Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("read-only connection");
    let hold = StoreHold::create(&db, "removal-01", "P1").expect("hold");
    assert!(matches!(
        check_after_sqlite_open(&db, &conn, &before),
        Err(CoreError::StoreHeld { .. })
    ));
    drop(conn);
    hold.release().expect("release");
}

#[test]
fn a_changed_path_or_a_different_sqlite_connection_is_refused() {
    let root = tempdir().expect("scratch directory");
    let db = root.path().join("ledger.db");
    let other = root.path().join("other.db");
    drop(Connection::open(&db).expect("first SQLite file"));
    drop(Connection::open(&other).expect("second SQLite file"));
    let before = before_sqlite_open(&db).expect("witness");
    let wrong = Connection::open_with_flags(&other, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("other connection");
    assert!(matches!(
        check_after_sqlite_open(&db, &wrong, &before),
        Err(CoreError::Integrity(_))
    ));
    drop(wrong);

    fs::rename(&other, &db).expect("replace the named file");
    let changed = Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("replacement connection");
    assert!(matches!(
        check_after_sqlite_open(&db, &changed, &before),
        Err(CoreError::Integrity(_))
    ));
}
