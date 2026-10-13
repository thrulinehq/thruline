//! C50 B1 / C53: qualify SQLite backup on real-process, tempfile Ledger WAL stores.
//! A held reader keeps its old snapshot until its transaction ends (C53 §1).
//! No removal implementation, journal-mode transition, or person act is used.
#![cfg(unix)]

use std::fs;
use std::io::{BufRead, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use rusqlite::backup::{Backup, StepResult};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use thruline_core::{bounded_child, chain::sha256_hex, Ledger};

const ROLE: &str = "THRULINE_C50_B1_ROLE";
const ROOT: &str = "THRULINE_C50_B1_ROOT";

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
struct View {
    projects: Vec<String>,
    integrity: String,
    journal: String,
}

fn view(conn: &Connection) -> View {
    let projects = conn
        .prepare("SELECT id FROM projects ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<String>>>()
        .unwrap();
    View {
        projects,
        integrity: conn
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .unwrap(),
        journal: conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap(),
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct ReaderViews {
    pid: u32,
    before: View,
    during: View,
    after: View,
}

/// This test owns the child. Reap it even when a qualification assertion panics.
struct Reader(Child);

impl Drop for Reader {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

fn child_command(name: &str, role: &str, root: &Path) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", name, "--nocapture"])
        .env(ROLE, role)
        .env(ROOT, root)
        .env("HOME", root.join("home"))
        .env("THRULINE_HOME", root.join("home/store"));
    command
}

fn drive(name: &str, held_transaction: bool) {
    match std::env::var(ROLE).ok().as_deref() {
        Some("coordinator") => coordinate(name, held_transaction),
        Some("reader") => read_in_child(held_transaction),
        None => {
            let scratch = tempfile::tempdir().unwrap();
            fs::create_dir_all(scratch.path().join("home/store")).unwrap();
            let mut command = child_command(name, "coordinator", scratch.path());
            // Reuse the existing bounded child runner, including group cleanup.
            let outcome =
                bounded_child::output_until(&mut command, Instant::now() + Duration::from_secs(90))
                    .unwrap();
            let bounded_child::Outcome::Finished(output) = outcome else {
                panic!("B1 coordinator exceeded its 90-second test bound");
            };
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
            assert!(
                output.status.success(),
                "B1 coordinator failed: {}",
                String::from_utf8_lossy(&output.stdout)
            );
        }
        Some(other) => panic!("unexpected B1 subprocess role: {other}"),
    }
}

fn root() -> PathBuf {
    let root = PathBuf::from(std::env::var_os(ROOT).unwrap());
    assert!(root
        .canonicalize()
        .unwrap()
        .starts_with(std::env::temp_dir().canonicalize().unwrap()));
    assert_eq!(
        PathBuf::from(std::env::var_os("HOME").unwrap()),
        root.join("home")
    );
    assert_eq!(
        PathBuf::from(std::env::var_os("THRULINE_HOME").unwrap()),
        root.join("home/store")
    );
    root
}

fn seed(path: &Path, ids: &[&str]) -> Ledger {
    let ledger = Ledger::open(path).unwrap();
    for id in ids {
        ledger.conn().execute(
            "INSERT INTO projects(id,root_path,source,created_at) VALUES(?1,?2,'codex','2026-10-09T00:00:00Z')",
            [*id, path.parent().unwrap().to_str().unwrap()],
        ).unwrap();
    }
    assert_eq!(view(ledger.conn()).journal, "wal");
    ledger
}

fn read_in_child(held_transaction: bool) {
    let root = root();
    let conn = Connection::open_with_flags(
        root.join("home/store/ledger.db"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    if held_transaction {
        conn.execute_batch("BEGIN DEFERRED").unwrap();
    }
    // SELECT establishes the WAL snapshot; BEGIN alone would not hold one.
    let before = view(&conn);
    assert_eq!(conn.is_autocommit(), !held_transaction);
    fs::write(root.join("reader-ready"), b"ready").unwrap();
    let mut signal = String::new();
    std::io::stdin().lock().read_line(&mut signal).unwrap();
    assert_eq!(signal, "after\n");
    let during = view(&conn);
    if held_transaction {
        conn.execute_batch("ROLLBACK").unwrap();
    }
    let after = view(&conn);
    let views = ReaderViews {
        pid: std::process::id(),
        before,
        during,
        after,
    };
    fs::write(
        root.join("reader-views.json"),
        serde_json::to_vec(&views).unwrap(),
    )
    .unwrap();
}

fn coordinate(name: &str, held_transaction: bool) {
    let root = root();
    let db = root.join("home/store/ledger.db");
    let wal = root.join("home/store/ledger.db-wal");
    // Keep both Ledger handles open, so last-connection checkpointing cannot
    // hide a WAL change in the before/after byte observations.
    let old = seed(&db, &["kept", "removed"]);
    let candidate = seed(&root.join("home/store/candidate.db"), &["kept"]);
    let mut destination =
        Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_WRITE).unwrap();
    destination.busy_timeout(Duration::ZERO).unwrap();
    let page_size = |conn: &Connection| {
        conn.query_row("PRAGMA page_size", [], |row| row.get::<_, i64>(0))
            .unwrap()
    };
    assert_eq!(page_size(&destination), page_size(candidate.conn()));
    let expected_old = view(old.conn());
    let expected_new = view(candidate.conn());
    assert_eq!(view(&destination).journal, "wal");
    let metadata = fs::metadata(&db).unwrap();
    let identity = (metadata.dev(), metadata.ino());
    let mut reader = Reader(
        child_command(name, "reader", &root)
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let ready_deadline = Instant::now() + Duration::from_secs(30);
    while !root.join("reader-ready").exists() {
        assert!(
            reader.0.try_wait().unwrap().is_none(),
            "reader exited before READY"
        );
        assert!(
            Instant::now() < ready_deadline,
            "reader did not become READY"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let db_before = fs::read(&db).unwrap();
    let wal_before = fs::read(&wal).unwrap();
    assert!(
        !wal_before.is_empty(),
        "qualification must include a real WAL"
    );
    let start = Instant::now();
    let result = {
        let backup = Backup::new(candidate.conn(), &mut destination).unwrap();
        backup.step(-1)
    }; // Drop/finish the backup before observing either destination view.
    let elapsed = start.elapsed();
    let db_after = fs::read(&db).unwrap();
    let wal_after = fs::read(&wal).unwrap();
    let current = view(&destination);
    let metadata = fs::metadata(&db).unwrap();
    let identity_after = (metadata.dev(), metadata.ino());

    reader
        .0
        .stdin
        .take()
        .unwrap()
        .write_all(b"after\n")
        .unwrap();
    let finish_deadline = Instant::now() + Duration::from_secs(30);
    let reader_status = loop {
        if let Some(status) = reader.0.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < finish_deadline, "reader did not finish");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(reader_status.success(), "reader failed: {reader_status}");
    let views: ReaderViews =
        serde_json::from_slice(&fs::read(root.join("reader-views.json")).unwrap()).unwrap();
    drop(reader); // Reaped before any B1 verdict assertion, including a failure.
    eprintln!(
        "B1 held_transaction={held_transaction} SQLite={} result={result:?} step_pages=-1 elapsed={elapsed:?} same_inode={} db_equal={} wal_equal={}\nDB sha {} -> {}\nWAL sha {} -> {}\nreader={views:?}\ncurrent={current:?}",
        rusqlite::version(), identity == identity_after, db_before == db_after,
        wal_before == wal_after, sha256_hex(&db_before), sha256_hex(&db_after),
        sha256_hex(&wal_before), sha256_hex(&wal_after),
    );
    assert_ne!(
        views.pid,
        std::process::id(),
        "reader must be another process"
    );
    assert_eq!(views.before, expected_old);
    assert_eq!(identity_after, identity, "live database inode changed");
    assert!(matches!(result, Ok(StepResult::Done)), "B1: {result:?}");
    assert_eq!(current, expected_new);
    assert_eq!(views.after, expected_new);
    assert_eq!(views.before.integrity, "ok");
    assert_eq!(views.during.integrity, "ok");
    assert_eq!(views.after.integrity, "ok");
    if held_transaction {
        // The copy commits while this other process still reads the complete
        // old snapshot. Ending that transaction exposes the new store.
        assert_eq!(views.during, expected_old);
    } else {
        assert_eq!(views.during, expected_new);
    }
    assert_eq!(current.integrity, "ok");
    assert_eq!(current.journal, "wal");
}

#[test]
fn b1a_existing_process_connection_sees_new_store_with_same_inode() {
    drive(
        "b1a_existing_process_connection_sees_new_store_with_same_inode",
        false,
    );
}

#[test]
fn b1b_held_reader_keeps_old_snapshot_then_sees_new_store() {
    drive(
        "b1b_held_reader_keeps_old_snapshot_then_sees_new_store",
        true,
    );
}
