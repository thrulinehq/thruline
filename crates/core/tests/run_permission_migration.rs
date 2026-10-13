//! P6-5B — the run-permission table arrives as a versioned migration and
//! costs nothing until `thruline upgrade` is run deliberately (A168/A193).
//!
//! **This task changes DDL; `thruline upgrade` is required; every other verb
//! exits 3 until it runs.** These tests are that sentence, mechanically: the
//! same four every Change-layer task since T-2-1 has re-asked, asked for this
//! table against the schema at `8a1acba` (P6-5 READY; the DDL there is
//! byte-identical to `8fe2761`'s, P6-5 changed none).
//!
//! The cost is the existing full-schema path's, not a new one: creating one
//! empty table adds no pass of its own (`upgrade::estimate_phrase`).

use rusqlite::Connection;
use thruline_core::error::CoreError;
use thruline_core::ledger::Ledger;
use thruline_core::upgrade::UpgradeState;

/// The DDL digest at `8a1acba`, the commit this task was built on. Pinned so
/// the bump is a fact rather than a belief; it is the sha256 of
/// `tests/fixtures/schema-p6-5-run-route.sql`, which is that commit's own DDL.
const SCHEMA_BEFORE: &str =
    "schema_sha256_de2b05e56a94443dfecc594e763bf6fa2249e8bf030a0062fc83a90673dd7d4a";

/// The one table this task adds.
const NEW_TABLE: &str = "run_permissions";

/// The tables other tasks added on top of the schema this task was built on
/// are read from the DDL task registry at `crates/core/migrations/`, not
/// listed here (R-2026-09-24-21). This lane predates the registry; its
/// hand-written list was replaced by `110-p6-5b.toml` at the merge.
mod migration_registry;

/// This task's ID, as the registry names it.
const THIS_TASK: &str = "P6-5B";

/// Every table this tree creates beyond this task's own fixture, other than
/// the one this task adds: the tables of tasks ordered AFTER it, and the
/// tables of tasks that were built on the SAME schema (the same pinned digest)
/// but landed ahead of it — T-5-3 was built on `8fe2761a` too, so its tables
/// are absent from this fixture although it is ordered first. Both halves are
/// read from the registry; neither is a list kept here.
fn tables_added_by_other_tasks() -> Vec<String> {
    let entries = migration_registry::entries();
    let me = entries
        .iter()
        .find(|e| e.task == THIS_TASK)
        .expect("the registry must name this task");
    assert_eq!(
        me.pinned_schema_digest, SCHEMA_BEFORE,
        "the registry and this test must pin the same pre-task schema"
    );
    let mut out: Vec<String> = entries
        .iter()
        .filter(|e| e.order < me.order && e.pinned_schema_digest == SCHEMA_BEFORE)
        .flat_map(|e| e.tables.clone())
        .collect();
    out.extend(migration_registry::tables_added_after(THIS_TASK));
    out.sort();
    out
}

fn fixture_schema() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/schema-p6-5-run-route.sql"),
    )
    .expect("the pre-P6-5B schema fixture must be readable")
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
fn the_schema_version_moved_when_the_permission_table_landed() {
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
        .query_row("SELECT COUNT(*) FROM run_permissions", [], |r| r.get(0))
        .expect("query");
    assert_eq!(rows, 0, "a migration must invent no permission");
    drop(conn);

    drop(Ledger::open_upgrading(&path, &mut |_, _, _| {}).expect("a second upgrade is a no-op"));
    assert_eq!(
        Ledger::upgrade_state(&path).expect("state"),
        UpgradeState::Current
    );
}

/// The migration adds ONE table and alters none, proven against the schema at
/// `8a1acba` rather than against a memory of it.
#[test]
fn the_migration_adds_one_table_and_alters_no_existing_one() {
    let before = tempfile::tempdir().expect("tempdir");
    let old = Connection::open(before.path().join("old.db")).expect("open");
    old.execute_batch(&fixture_schema())
        .expect("apply the pre-P6-5B schema");

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
    expected.extend(tables_added_by_other_tasks());
    expected.sort();
    assert_eq!(added, expected);
}

/// An existing store upgrades with every row it had, the new table arrives
/// empty, nothing is backfilled, and its chain is vacuously whole.
#[test]
fn an_upgraded_store_keeps_its_rows_and_its_permission_chain_adds_up() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("ledger.db");
    drop(Ledger::open(&path).expect("a fresh ledger"));
    let conn = Connection::open(&path).expect("reopen");
    conn.execute(
        "INSERT INTO projects(id, root_path, source, display_name, created_at)
         VALUES('p-kept', '/p', 'claude_code', 'kept', '2026-01-01T00:00:00Z')",
        [],
    )
    .expect("a project row written before the upgrade");
    conn.execute("DELETE FROM schema_migrations", [])
        .expect("clear the fingerprint");
    drop(conn);

    assert!(Ledger::upgrade_state(&path).expect("state").is_pending());
    let ledger = Ledger::open_upgrading(&path, &mut |_, _, _| {}).expect("upgrade");
    let kept: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM projects WHERE id='p-kept'", [], |r| {
            r.get(0)
        })
        .expect("query");
    assert_eq!(kept, 1, "the upgrade lost a row it did not own");
    assert_eq!(
        thruline_core::run_permission::verify_chain(&ledger).expect("verify"),
        0
    );
}
