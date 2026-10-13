//! T-2-1 — the Change tables arrive as a versioned migration, and cost nothing
//! until `thruline upgrade` is run deliberately (`B11.4`, A168/A193).
//!
//! **This task changes DDL; `thruline upgrade` is required; every other verb
//! exits 3 until it runs.** These tests are that sentence, mechanically.
//!
//! What "versioned" means in this store, stated because it is not a version
//! integer: the schema version IS the digest of the DDL
//! (`Ledger::schema_fingerprint`), stamped into `schema_migrations` only after
//! the whole path completed. Adding a table therefore moves the version by
//! construction — there is no counter for a later session to forget to bump —
//! and a store carrying the old digest is refused by every reader until the
//! upgrade pays for the new one.
//!
//! **Reversible in the sense the existing migrations are**, which is: forward
//! only, and safe in one direction. The migration is idempotent (every
//! statement is `CREATE … IF NOT EXISTS`, so re-running it is a no-op), and it
//! is *compatible backwards* in the only way that matters here — it adds
//! tables and alters no existing column, so every row an older binary wrote is
//! still there and still readable. There is no down-migration, and none of the
//! thirty migrations before it has one either.

use rusqlite::Connection;
use thruline_core::error::CoreError;
use thruline_core::ledger::Ledger;
use thruline_core::upgrade::UpgradeState;

/// The DDL digest at `5cab107da5df12154cbccc49cb34f7e44f38dc65`, the tree this
/// task was built on. Pinned so the bump is a fact rather than a belief: if a
/// later edit reverts the Change tables out of the DDL, this test says so.
const SCHEMA_BEFORE_THIS_TASK: &str =
    "schema_sha256_744652141e5feb3e3a754582ab7f08899941251774aa4d032c7a0b5d96773be3";

/// The five tables the Change layer adds.
const NEW_TABLES: &[&str] = &[
    "changes",
    "change_scope_revisions",
    "change_scope_acceptances",
    "change_implementations",
    "change_session_associations",
];

/// The tables later tasks in the same layer added are read from the DDL task
/// registry at `crates/core/migrations/`, not named here (R-2026-09-24-21).
/// They are kept out of `NEW_TABLES` so this test keeps asking its own
/// question — "did T-2-1 alter anything that was already there?" — rather
/// than failing every time the layer grows; and they are read rather than
/// listed so that a task which adds a table and forgets to say so goes red in
/// `migration_registry_guard.rs` by name, instead of being noticed by
/// whichever of five hand-written arrays happened to be updated.
mod migration_registry;

/// This task's ID, as the registry names it.
const THIS_TASK: &str = "T-2-1";

/// A ledger on disk carrying a real schema with its fingerprint removed — the
/// exact state a user's store is in the moment they install a binary whose DDL
/// has changed.
fn store_with_a_pending_upgrade(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("ledger.db");
    drop(Ledger::open(&path).expect("creating a fresh ledger must still work"));
    let conn = Connection::open(&path).expect("reopen");
    conn.execute("DELETE FROM schema_migrations", [])
        .expect("clear the fingerprint");
    drop(conn);
    path
}

#[test]
fn the_schema_version_moved_when_the_change_tables_landed() {
    let now = Ledger::schema_fingerprint();
    assert_ne!(
        now, SCHEMA_BEFORE_THIS_TASK,
        "adding tables to the DDL must move the schema version"
    );
    assert!(now.starts_with("schema_sha256_"));
}

#[test]
fn an_existing_store_is_refused_until_the_upgrade_is_asked_for() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = store_with_a_pending_upgrade(dir.path());

    assert!(Ledger::upgrade_state(&path).expect("state").is_pending());
    let err = match Ledger::open(&path) {
        Ok(_) => panic!("open must refuse a pending migration"),
        Err(e) => e,
    };
    match err {
        CoreError::UpgradePending { ref estimate, .. } => assert!(!estimate.is_empty()),
        other => panic!("expected UpgradePending, got {other:?}"),
    }
    assert!(
        err.to_string().contains("thruline upgrade"),
        "the refusal must name the one command that fixes it: {err}"
    );

    // And it cost nothing: the fingerprint is still absent.
    let conn = Connection::open(&path).expect("reopen");
    let stamped: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
        .expect("count");
    assert_eq!(stamped, 0, "a refusal must not pay for any step");
}

#[test]
fn the_upgrade_adds_the_change_tables_and_is_idempotent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = store_with_a_pending_upgrade(dir.path());

    let mut steps = Vec::new();
    drop(
        Ledger::open_upgrading(&path, &mut |name, i, total| {
            steps.push(format!("{i}/{total} {name}"))
        })
        .expect("the upgrade must succeed"),
    );
    assert!(!steps.is_empty(), "the payment must be visible");
    assert_eq!(
        Ledger::upgrade_state(&path).expect("state"),
        UpgradeState::Current
    );

    let conn = Connection::open(&path).expect("reopen");
    for table in NEW_TABLES {
        let found: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |r| r.get(0),
            )
            .expect("query");
        assert_eq!(found, 1, "{table} must exist after the upgrade");
    }
    drop(conn);

    // Running it again is a no-op that still ends Current — the property the
    // existing migrations have and the only sense in which any of them is
    // "reversible": safe to re-apply, never undone.
    drop(Ledger::open_upgrading(&path, &mut |_, _, _| {}).expect("a second upgrade is a no-op"));
    assert_eq!(
        Ledger::upgrade_state(&path).expect("state"),
        UpgradeState::Current
    );
}

/// A fresh store still creates for free. Refusing that would mean no ledger
/// could ever be made, and the refusal is about migrating, not creating.
#[test]
fn a_fresh_store_still_opens_without_an_upgrade() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("new.db");
    assert_eq!(
        Ledger::upgrade_state(&path).expect("state"),
        UpgradeState::Fresh
    );
    let ledger = Ledger::open(&path).expect("a fresh ledger must open");
    let found: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='changes'",
            [],
            |r| r.get(0),
        )
        .expect("query");
    assert_eq!(found, 1);
}

/// `events` stays append-only and its triggers are untouched. The Change layer
/// adds tables beside the hash chain and never near it.
#[test]
fn the_append_only_triggers_on_events_are_untouched() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    for trigger in ["events_no_update", "events_no_delete"] {
        let sql: String = ledger
            .conn()
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='trigger' AND name=?1",
                [trigger],
                |r| r.get(0),
            )
            .expect("the trigger must still be there");
        assert!(sql.contains("events are append-only"), "{sql}");
    }
}

/// The migration adds tables and nothing else: no existing table gains or loses
/// a column, and no existing row is rewritten. Proven against the DDL at HEAD
/// by comparing the two schemas table by table.
#[test]
fn the_migration_adds_tables_and_alters_no_existing_one() {
    let head_ddl = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/schema-head.sql"),
    )
    .expect("the HEAD schema fixture must be readable");

    let before = tempfile::tempdir().expect("tempdir");
    let old = Connection::open(before.path().join("old.db")).expect("open");
    old.execute_batch(&head_ddl).expect("apply the HEAD schema");

    let after = tempfile::tempdir().expect("tempdir");
    let new = Connection::open(after.path().join("new.db")).expect("open");
    new.execute_batch(Ledger::schema_ddl())
        .expect("apply this tree's schema");

    let shape = |conn: &Connection| -> Vec<(String, String)> {
        let mut statement = conn
            .prepare(
                "SELECT name, COALESCE(sql,'') FROM sqlite_master
                  WHERE type='table' ORDER BY name",
            )
            .expect("prepare");
        let rows: Vec<(String, String)> = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("query")
            .filter_map(|r| r.ok())
            .collect();
        rows
    };

    let old_shape = shape(&old);
    let new_shape = shape(&new);
    assert!(!old_shape.is_empty(), "the HEAD schema fixture is empty");

    let mut missing = Vec::new();
    let mut altered = Vec::new();
    for (name, sql) in &old_shape {
        match new_shape.iter().find(|(n, _)| n == name) {
            None => missing.push(name.clone()),
            Some((_, new_sql)) if new_sql != sql => altered.push(name.clone()),
            Some(_) => {}
        }
    }
    assert!(missing.is_empty(), "tables disappeared: {missing:?}");
    assert!(
        altered.is_empty(),
        "existing tables were altered: {altered:?}"
    );

    let added: Vec<String> = new_shape
        .iter()
        .map(|(n, _)| n.clone())
        .filter(|n| !old_shape.iter().any(|(o, _)| o == n))
        .collect();
    let mut expected: Vec<String> = NEW_TABLES.iter().map(|t| (*t).to_string()).collect();
    expected.extend(migration_registry::tables_added_after(THIS_TASK));
    expected.sort();
    let mut added_sorted = added.clone();
    added_sorted.sort();
    assert_eq!(
        added_sorted, expected,
        "the migration must add exactly the Change tables"
    );
}
