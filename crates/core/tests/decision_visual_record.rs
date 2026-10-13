//! Exact native schema preparation is limited to disposable C1 snapshots.
use std::path::PathBuf;
use std::time::{Duration, Instant};
use thruline_core::decisions::checks_visual::visual_record::{
    prepare_exact_snapshot, prepare_exact_snapshot_with_timeout,
};

fn private_work_dir() -> tempfile::TempDir {
    let mut builder = tempfile::Builder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o700));
    }
    builder.tempdir().unwrap()
}

struct Fixture {
    dir: tempfile::TempDir,
    db: PathBuf,
    server: PathBuf,
}
impl Fixture {
    fn new(help: &str, upgrade: &str) -> Self {
        let dir = private_work_dir();
        let db = dir.path().join("ledger.db");
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch("CREATE TABLE events(id TEXT PRIMARY KEY,ts TEXT,seq INTEGER,raw_line INTEGER,raw_sha256 TEXT,chain_hash TEXT); INSERT INTO events VALUES('clock','2026-10-06T23:42:16.900Z',1,1,'raw','chain'); CREATE TABLE ledger_meta(project_id TEXT,head_hash TEXT,event_count INTEGER); INSERT INTO ledger_meta VALUES('project','head',1);").unwrap();
        drop(conn);
        let server = dir.path().join("native-cli");
        std::fs::write(&server, format!("#!/usr/bin/env python3\nimport sys,sqlite3,time\nfrom pathlib import Path\nif '--help' in sys.argv:\n{}\nelse:\n{}\n", indent(help), indent(upgrade))).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&server, std::fs::Permissions::from_mode(0o700)).unwrap();
            std::fs::set_permissions(&db, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        Self { dir, db, server }
    }
    fn prepare(&self) -> Result<(), &'static str> {
        prepare_exact_snapshot(&self.server, &self.db, self.dir.path(), Some("clock"))
    }
}
fn indent(text: &str) -> String {
    text.lines().map(|line| format!("    {line}\n")).collect()
}

#[test]
fn native_upgrade_runs_in_scratch_and_preserves_record_facts() {
    let f = Fixture::new(
        "print('Usage: thruline upgrade')",
        "Path(sys.argv[2]).with_suffix('.upgraded').write_text('done')",
    );
    assert_eq!(f.prepare(), Ok(()));
    assert!(f.db.with_extension("upgraded").exists());
}

#[test]
fn explicitly_unsupported_legacy_upgrade_passes_through() {
    let f = Fixture::new(
        "print(\"error: unrecognized subcommand 'upgrade'\",file=sys.stderr);sys.exit(2)",
        "sys.exit(9)",
    );
    assert_eq!(f.prepare(), Ok(()));
}

#[test]
fn legacy_help_cannot_change_record_facts_before_pass_through() {
    let f=Fixture::new("c=sqlite3.connect(sys.argv[2]);c.execute('DELETE FROM events');c.commit();print(\"error: unrecognized subcommand 'upgrade'\",file=sys.stderr);sys.exit(2)", "pass");
    assert!(f.prepare().is_err());
}

#[test]
fn supported_help_mutation_is_refused_before_upgrade_runs() {
    let f=Fixture::new("c=sqlite3.connect(sys.argv[2]);c.execute('DELETE FROM events');c.commit();print('Usage: thruline upgrade')", "Path(sys.argv[2]).with_suffix('.upgraded').write_text('done')");
    assert!(f.prepare().is_err());
    assert!(!f.db.with_extension("upgraded").exists());
}

#[test]
fn help_cannot_change_schema_while_record_counts_stay_equal() {
    let f=Fixture::new("c=sqlite3.connect(sys.argv[2]);c.execute('CREATE TABLE unrelated(value TEXT)');c.commit();print('Usage: thruline upgrade')", "pass");
    assert!(f.prepare().is_err());
}

#[test]
fn arbitrary_probe_failure_is_not_legacy_support() {
    let f = Fixture::new(
        "print('store unavailable',file=sys.stderr);sys.exit(5)",
        "pass",
    );
    assert!(f.prepare().is_err());
}

#[test]
fn upgrade_failure_preserves_a_private_bounded_diagnostic() {
    let f = Fixture::new(
        "print('Usage: thruline upgrade')",
        "print('native upgrade failed',file=sys.stderr);sys.exit(3)",
    );
    assert!(f.prepare().is_err());
    let log = f.dir.path().join("native-upgrade.stderr.log");
    assert!(std::fs::read_to_string(&log)
        .unwrap()
        .contains("native upgrade failed"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(log).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn changed_event_count_head_or_clock_witness_is_unverifiable() {
    for sql in [
        "DELETE FROM events",
        "UPDATE ledger_meta SET head_hash='different'",
        "UPDATE events SET raw_sha256='different'",
    ] {
        let body = format!("c=sqlite3.connect(sys.argv[2]);c.execute({sql:?});c.commit()");
        let f = Fixture::new("print('Usage: thruline upgrade')", &body);
        assert!(f.prepare().is_err(), "changed fact: {sql}");
    }
}

#[test]
fn snapshot_outside_work_is_refused_before_starting_native_cli() {
    let f = Fixture::new("sys.exit(0)", "pass");
    let other = private_work_dir();
    assert!(prepare_exact_snapshot(&f.server, &f.db, other.path(), Some("clock")).is_err());
}

#[test]
fn native_upgrade_timeout_kills_and_reaps_its_child() {
    let f = Fixture::new("print('Usage: thruline upgrade')", "time.sleep(2)");
    let start = Instant::now();
    assert!(prepare_exact_snapshot_with_timeout(
        &f.server,
        &f.db,
        f.dir.path(),
        Some("clock"),
        Duration::from_millis(200)
    )
    .is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn oversized_native_output_fails_with_a_bounded_saved_log() {
    let f = Fixture::new(
        "print('Usage: thruline upgrade')",
        "sys.stderr.write('x'*(2<<20));sys.stderr.flush()",
    );
    assert!(f.prepare().is_err());
    assert!(
        std::fs::metadata(f.dir.path().join("native-upgrade.stderr.log"))
            .unwrap()
            .len()
            <= 65536
    );
}

#[test]
fn normally_exited_native_cli_cannot_leave_a_snapshot_writer_alive() {
    let f = Fixture::new(
        "print('Usage: thruline upgrade')",
        "import os\nif os.fork() == 0:\n    time.sleep(0.3)\n    c=sqlite3.connect(sys.argv[2]);c.execute('DELETE FROM events');c.commit()\n    os._exit(0)",
    );
    assert_eq!(f.prepare(), Ok(()));
    std::thread::sleep(Duration::from_millis(500));
    let conn = rusqlite::Connection::open(&f.db).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        count, 1,
        "a surviving native descendant changed the snapshot after verification"
    );
}

#[cfg(unix)]
#[test]
fn a_hard_link_to_a_source_record_is_refused_before_native_upgrade() {
    let f = Fixture::new(
        "print('Usage: thruline upgrade')",
        "Path(sys.argv[2]).with_suffix('.upgraded').write_text('done')",
    );
    let work = private_work_dir();
    let linked = work.path().join("ledger.db");
    std::fs::hard_link(&f.db, &linked).unwrap();
    assert!(prepare_exact_snapshot(&f.server, &linked, work.path(), Some("clock")).is_err());
    assert!(!linked.with_extension("upgraded").exists());
}
