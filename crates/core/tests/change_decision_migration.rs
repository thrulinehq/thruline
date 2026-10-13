//! T-S-4 — the decision table arrives as a versioned migration and costs
//! nothing until `thruline upgrade` is run deliberately (A168/A193).
//!
//! **This task changes DDL; `thruline upgrade` is required; every other verb
//! exits 3 until it runs.** These tests are that sentence, mechanically: the
//! same four T-2-1 established and every Change-layer task since has re-asked,
//! asked for this table against the schema at `0d2de68c` — the sprint commit
//! this task was built on.
//!
//! The cost is the existing full-schema path's, not a new one: on the
//! 2.01 M-event corpus `upgrade::estimate_phrase` quotes about 35 seconds, and
//! creating one empty table adds nothing measurable to it.

use rusqlite::Connection;
use thruline_core::error::CoreError;
use thruline_core::ledger::Ledger;
use thruline_core::upgrade::UpgradeState;

/// The DDL digest at `0d2de68c`, the commit this task was built on. Pinned so
/// the bump is a fact rather than a belief; it is the sha256 of
/// `tests/fixtures/schema-sprint-t-s-3.sql`, which is that commit's own DDL.
const SCHEMA_BEFORE: &str =
    "schema_sha256_f10171c9b6bd1008373fa29ecb323f4b94837e05d6f2124af7ab36c4e0398947";

/// The one table this task adds.
const NEW_TABLE: &str = "change_decisions";

/// The tables tasks AFTER this one added are read from the DDL task registry
/// at `crates/core/migrations/`, not listed here (R-2026-09-24-21). They are
/// kept out of `NEW_TABLE` so this file keeps asking its own question — did
/// T-S-4 alter anything that was already there? — instead of failing every
/// time the layer grows.
mod migration_registry;

/// This task's ID, as the registry names it.
const THIS_TASK: &str = "T-S-4";

fn fixture_schema() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/schema-sprint-t-s-3.sql"),
    )
    .expect("the pre-T-S-4 schema fixture must be readable")
}

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
fn the_fixture_is_really_the_schema_at_the_commit_this_was_built_on() {
    let digest = format!(
        "schema_sha256_{}",
        thruline_core::chain::sha256_hex(fixture_schema().as_bytes())
    );
    assert_eq!(
        digest, SCHEMA_BEFORE,
        "the pinned digest and the fixture disagree, so one of them is a memory"
    );
}

#[test]
fn the_schema_version_moved_when_the_decision_table_landed() {
    let now = Ledger::schema_fingerprint();
    assert_ne!(
        now, SCHEMA_BEFORE,
        "adding a table to the DDL must move the schema version"
    );
    assert!(now.starts_with("schema_sha256_"));
}

#[test]
fn an_existing_store_is_refused_until_the_upgrade_is_asked_for() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = store_with_a_pending_upgrade(dir.path());

    assert!(Ledger::upgrade_state(&path).expect("state").is_pending());
    match Ledger::open(&path) {
        Ok(_) => panic!("open must refuse a pending migration"),
        Err(CoreError::UpgradePending { estimate, .. }) => assert!(!estimate.is_empty()),
        Err(other) => panic!("expected UpgradePending, got {other:?}"),
    }
}

#[test]
fn the_upgrade_adds_the_table_and_is_idempotent() {
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
    let found: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            [NEW_TABLE],
            |r| r.get(0),
        )
        .expect("query");
    assert_eq!(found, 1, "{NEW_TABLE} must exist after the upgrade");
    let rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM change_decisions", [], |r| r.get(0))
        .expect("query");
    assert_eq!(rows, 0, "a migration must invent no approval");
    drop(conn);

    drop(Ledger::open_upgrading(&path, &mut |_, _, _| {}).expect("a second upgrade is a no-op"));
    assert_eq!(
        Ledger::upgrade_state(&path).expect("state"),
        UpgradeState::Current
    );
}

/// The migration adds ONE table and alters none, proven against the schema at
/// `0d2de68c` rather than against a memory of it.
#[test]
fn the_migration_adds_one_table_and_alters_no_existing_one() {
    let before = tempfile::tempdir().expect("tempdir");
    let old = Connection::open(before.path().join("old.db")).expect("open");
    old.execute_batch(&fixture_schema())
        .expect("apply the pre-T-S-4 schema");

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
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("query")
            .filter_map(|r| r.ok())
            .collect()
    };

    let old_shape = shape(&old);
    let new_shape = shape(&new);
    assert!(!old_shape.is_empty(), "the schema fixture is empty");

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

    let mut added: Vec<String> = new_shape
        .iter()
        .map(|(n, _)| n.clone())
        .filter(|n| !old_shape.iter().any(|(o, _)| o == n))
        .collect();
    added.sort();
    let mut expected = vec![NEW_TABLE.to_string()];
    expected.extend(migration_registry::tables_added_after(THIS_TASK));
    expected.sort();
    assert_eq!(added, expected);
}
