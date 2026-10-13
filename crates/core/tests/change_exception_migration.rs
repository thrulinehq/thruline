//! T-5-3 — the two exception tables arrive as a versioned migration and cost
//! nothing until `thruline upgrade` is run deliberately (A168/A193).
//!
//! **This task changes DDL; `thruline upgrade` is required; every other verb
//! exits 3 until it runs.** These tests are that sentence, mechanically, asked
//! against the schema at `8fe2761a` — the commit this task was built on, which
//! is T-5-2's.
//!
//! The table this task added shipped as `change_dispositions` and carries the
//! name `change_obligation_waivers` here because R-2026-09-24-22 renamed it
//! hours later, with its own migration and its own test
//! (`obligation_waiver_rename_migration.rs`). This file asks its question of
//! the tree as it is now, which is why the new name appears in a T-5-3 test.
//!
//! The cost is the existing full-schema path's, not a new one: on the
//! 2.01 M-event corpus `upgrade::estimate_phrase` quotes about 35 seconds, and
//! creating two empty tables adds nothing measurable to it.

use rusqlite::Connection;
use thruline_core::error::CoreError;
use thruline_core::ledger::Ledger;
use thruline_core::upgrade::UpgradeState;

/// The DDL digest at `8fe2761a`. Pinned so the bump is a fact rather than a
/// belief; it is the sha256 of `tests/fixtures/schema-sprint-t-5-2.sql`, which
/// is that commit's own DDL.
const SCHEMA_BEFORE: &str =
    "schema_sha256_de2b05e56a94443dfecc594e763bf6fa2249e8bf030a0062fc83a90673dd7d4a";

/// The two tables this task adds, and there is no third.
///
/// `change_obligation_waivers` holds an exception — one person setting one
/// obligation aside. `change_decision_exceptions` holds what one approval was
/// given under, copied rather than joined, so an approval cannot acquire an
/// exception written after it (`B10.9`).
const NEW_TABLES: &[&str] = &["change_decision_exceptions", "change_obligation_waivers"];

/// The tables tasks AFTER this one added are read from the DDL task registry
/// at `crates/core/migrations/`, not listed here (R-2026-09-24-21), so this
/// file keeps asking its own question — did T-5-3 alter anything that was
/// already there? — instead of failing every time the layer grows.
mod migration_registry;

/// This task's ID, as the registry names it.
const THIS_TASK: &str = "T-5-3";

fn fixture_schema() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/schema-sprint-t-5-2.sql"),
    )
    .expect("the pre-T-5-3 schema fixture must be readable")
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
fn the_schema_version_moved_when_the_exception_tables_landed() {
    let now = Ledger::schema_fingerprint();
    assert_ne!(
        now, SCHEMA_BEFORE,
        "adding tables to the DDL must move the schema version"
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
fn the_upgrade_adds_the_tables_and_is_idempotent() {
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
        let rows: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .expect("query");
        assert_eq!(rows, 0, "a migration must invent no exception");
    }
    drop(conn);

    drop(Ledger::open_upgrading(&path, &mut |_, _, _| {}).expect("a second upgrade is a no-op"));
    assert_eq!(
        Ledger::upgrade_state(&path).expect("state"),
        UpgradeState::Current
    );
}

/// The migration adds exactly these two tables and alters none, proven against
/// the schema at `8fe2761a` rather than against a memory of it.
#[test]
fn the_migration_adds_two_tables_and_alters_no_existing_one() {
    let before = tempfile::tempdir().expect("tempdir");
    let old = Connection::open(before.path().join("old.db")).expect("open");
    old.execute_batch(&fixture_schema())
        .expect("apply the pre-T-5-3 schema");

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
    let mut expected: Vec<String> = NEW_TABLES.iter().map(|t| (*t).to_string()).collect();
    expected.extend(migration_registry::tables_added_after(THIS_TASK));
    expected.sort();
    assert_eq!(added, expected);
}

/// **An exception is append-only, and the three prerequisites are refused by
/// the table itself** — not only by `ObligationWaiver::validate`, so a caller
/// that bypassed the validator still could not write one (`B7.12`).
#[test]
fn the_store_refuses_a_non_waivable_id_and_every_edit() {
    let dir = tempfile::tempdir().expect("tempdir");
    let conn = Connection::open(dir.path().join("schema.db")).expect("open");
    conn.execute_batch(Ledger::schema_ddl()).expect("schema");
    /* The foreign key is real, so the Change this is about has to exist —
     * which is itself part of what the table refuses. */
    conn.execute("PRAGMA foreign_keys=ON", []).ok();
    conn.execute(
        "INSERT INTO projects(id, root_path, source, display_name, created_at)
         VALUES('p1','/tmp/p1','claude_code','t','2026-01-01T00:00:00Z')",
        [],
    )
    .expect("a project");
    conn.execute(
        "INSERT INTO changes(id, project_id, opened_at, title, purpose, requester, request_source)
         VALUES('ch1','p1','2026-01-01T00:00:00Z','t','p','sam','person')",
        [],
    )
    .expect("a change");

    let insert = |obligation: &str, id: &str| {
        conn.execute(
            "INSERT INTO change_obligation_waivers(id, change_id, obligation_id, kind, actor,
                                             authority, rationale, recorded_at)
             VALUES(?1,'ch1',?2,'waived','sam','the release policy','a reason','2026-01-01T00:00:00Z')",
            rusqlite::params![id, obligation],
        )
    };
    for refused in [
        "candidate_identity",
        "decision_authority",
        "packet_consistency",
    ] {
        assert!(
            insert(refused, refused).is_err(),
            "{refused} must be refused by the table, not only by the validator"
        );
    }
    insert("criterion:1", "cx1").expect("an ordinary obligation may be set aside");
    assert!(
        conn.execute(
            "UPDATE change_obligation_waivers SET rationale='something else' WHERE id='cx1'",
            []
        )
        .is_err(),
        "an exception is never edited"
    );
    assert!(
        conn.execute("DELETE FROM change_obligation_waivers WHERE id='cx1'", [])
            .is_err(),
        "an exception is never deleted"
    );
}
