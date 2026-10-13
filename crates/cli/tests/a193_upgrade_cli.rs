//! A193 — the process-level contract of the pending-upgrade refusal.
//!
//! Three properties, and they are not the same property:
//!
//! 1. An ordinary command refuses in one line, exits **non-zero**, and pays
//!    nothing.
//! 2. The **sensor never blocks**. Exit 0, stdout silent, and the payload it was
//!    handed is preserved rather than dropped — a hook that loses a user's
//!    session data because a schema moved is worse than a hook that is slow.
//! 3. `thruline upgrade` pays it, says so, and leaves everything working.

use std::path::Path;
use std::process::{Command, Stdio};

use tempfile::tempdir;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

/// A ledger with a real schema whose fingerprint has been removed — what a
/// user's ledger looks like the moment they install a binary with a new DDL.
fn pending_ledger(dir: &Path) -> std::path::PathBuf {
    let db = dir.join("ledger.db");
    let out = Command::new(bin())
        .args(["--db", db.to_str().unwrap(), "projects"])
        .output()
        .expect("creating the ledger");
    assert!(out.status.success(), "{out:?}");

    let conn = rusqlite::Connection::open(&db).expect("open");
    conn.execute("DELETE FROM schema_migrations", [])
        .expect("clearing the fingerprint");
    drop(conn);
    db
}

fn fingerprints(db: &Path) -> i64 {
    let conn = rusqlite::Connection::open(db).expect("open");
    conn.query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
        .expect("count")
}

#[test]
fn an_ordinary_command_refuses_in_one_line_and_pays_nothing() {
    let dir = tempdir().expect("tempdir");
    let db = pending_ledger(dir.path());

    let out = Command::new(bin())
        .args(["--db", db.to_str().unwrap(), "now"])
        .output()
        .expect("running");

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert_eq!(
        out.status.code(),
        Some(3),
        "the refusal has its own exit code, distinct from a real failure. \
         stderr: {stderr}"
    );
    assert!(
        stderr.contains("schema upgrade pending") && stderr.contains("thruline upgrade"),
        "stderr must name the state and the one command that fixes it: {stderr}"
    );
    assert_eq!(
        stderr.lines().filter(|l| !l.trim().is_empty()).count(),
        1,
        "the refusal is ONE line, not an error report: {stderr}"
    );
    assert!(
        stdout.trim().is_empty(),
        "a refused command wrote to stdout: {stdout}"
    );
    assert_eq!(
        fingerprints(&db),
        0,
        "the refused command paid for a migration it refused to do"
    );
}

/// The sensor absolute. A Claude Code hook runs on a timeout, its stdout is
/// consumed by the vendor, and a non-zero exit is a visible failure in someone
/// else's tool. None of that may change because ThruLine's schema moved.
#[test]
fn the_sensor_never_blocks_and_never_loses_the_payload() {
    let dir = tempdir().expect("tempdir");
    let db = pending_ledger(dir.path());

    let payload = r#"{"session_id":"a193-probe","hook_event_name":"SessionStart"}"#;
    let mut child = Command::new(bin())
        .args([
            "--db",
            db.to_str().unwrap(),
            "sensor",
            "append",
            "SessionStart",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawning the sensor");
    {
        use std::io::Write;
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(payload.as_bytes())
            .expect("writing the payload");
    }
    let out = child.wait_with_output().expect("sensor exits");

    assert_eq!(
        out.status.code(),
        Some(0),
        "the sensor must exit 0 whatever the ledger's state — it is someone \
         else's hook. stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).trim().is_empty(),
        "the sensor wrote to stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert_eq!(
        fingerprints(&db),
        0,
        "the sensor migrated the ledger — the one thing a hook must never do"
    );

    // And the payload survived. This is the half that makes the refusal safe:
    // the events are not dropped, they are spilled, and the upgrade the user
    // eventually runs does not cost them a session.
    let spill = db.with_file_name("sensor-spill.jsonl");
    let spilled = std::fs::read_to_string(&spill).unwrap_or_default();
    assert!(
        spilled.contains("a193-probe"),
        "the refused payload was not spilled to {}: {spilled:?}",
        spill.display()
    );
}

#[test]
fn the_upgrade_verb_pays_it_visibly_and_everything_works_after() {
    let dir = tempdir().expect("tempdir");
    let db = pending_ledger(dir.path());

    let dry = Command::new(bin())
        .args(["--db", db.to_str().unwrap(), "upgrade", "--dry-run"])
        .output()
        .expect("dry run");
    assert!(dry.status.success());
    let dry_out = String::from_utf8_lossy(&dry.stdout);
    assert!(dry_out.contains("Schema upgrade pending"), "{dry_out}");
    assert!(
        dry_out.contains("estimate"),
        "the dry run must say how long it will take: {dry_out}"
    );
    assert!(
        dry_out.contains("disk"),
        "the dry run must report the disk preflight: {dry_out}"
    );
    assert_eq!(
        fingerprints(&db),
        0,
        "--dry-run performed the upgrade it was asked to describe"
    );

    let out = Command::new(bin())
        .args(["--db", db.to_str().unwrap(), "upgrade"])
        .output()
        .expect("upgrade");
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("[ 1/") && text.contains("Done in"),
        "the payment must be visible step by step: {text}"
    );
    assert!(
        fingerprints(&db) > 0,
        "the upgrade did not stamp the schema it claims to have applied"
    );

    // The whole point: afterwards, ordinary commands work again.
    let after = Command::new(bin())
        .args(["--db", db.to_str().unwrap(), "now"])
        .output()
        .expect("now");
    assert!(
        after.status.success(),
        "a command still refused after the upgrade: {}",
        String::from_utf8_lossy(&after.stderr)
    );

    // And running it twice is safe and silent about work it did not do.
    let again = Command::new(bin())
        .args(["--db", db.to_str().unwrap(), "upgrade"])
        .output()
        .expect("upgrade again");
    assert!(again.status.success());
    assert!(
        String::from_utf8_lossy(&again.stdout).contains("already up to date"),
        "{:?}",
        String::from_utf8_lossy(&again.stdout)
    );
}
