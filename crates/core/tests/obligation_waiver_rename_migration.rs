//! R-2026-09-24-22 — `change_dispositions` becomes `change_obligation_waivers`
//! on a store that already has waivers in it, and every row survives it byte
//! for byte.
//!
//! **This task changes DDL; `thruline upgrade` is required; every other verb
//! exits 3 until it runs** (A168/A193). The questions are asked against the
//! schema at `4e486e16` — T-5-3's own DDL, pinned by digest below, because a
//! store built by the T-5-3 binary is the only store in the world that can
//! have a `change_dispositions` table with rows in it.
//!
//! The rename is **the one `ALTER TABLE` in the Change layer**, and the ruling
//! is what allows it. `RENAME TO` moves no row's bytes, which is the reason to
//! prefer it over a new table and a copy; this file is where that stops being
//! a claim. Cost (A193): nothing measurable of its own — three metadata
//! renames and four drops. The bill is the existing full-schema path's, which
//! `upgrade::estimate_phrase` quotes as about 35 seconds on the 2.01 M-event
//! corpus.

use rusqlite::Connection;
use thruline_core::error::CoreError;
use thruline_core::ledger::Ledger;
use thruline_core::upgrade::UpgradeState;

mod migration_registry;

/// The DDL digest at `4e486e16`, which is T-5-3's. Pinned so the bump is a
/// fact rather than a belief; it is the sha256 of
/// `tests/fixtures/schema-t-5-3.sql`, which is that commit's own DDL.
const SCHEMA_BEFORE: &str =
    "schema_sha256_5bb65e851f76b301222bd234f4c6ddae3120e4e40838301fc81cf78007212805";

/// The fingerprint a T-5-3 store carries, so the store under test is the thing
/// a person actually has on disk when they install this binary — not an empty
/// file and not a store with its fingerprint deleted.
fn fixture_schema() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/schema-t-5-3.sql"),
    )
    .expect("the T-5-3 schema fixture must be readable")
}

/// Two waivers, as the T-5-3 binary wrote them: one waived, one not
/// applicable, both with the actor, authority and rationale the record type
/// refuses to be without.
const WAIVERS: [(&str, &str, &str, &str, &str, &str); 2] = [
    (
        "cx_1",
        "criterion:2",
        "waived",
        "sam",
        "the release policy",
        "the card reader is not in this release",
    ),
    (
        "cx_2",
        "criterion:3",
        "not_applicable",
        "dana",
        "the release policy",
        "there is no card reader on this platform",
    ),
];

/// A store the T-5-3 binary would have left behind: its DDL, its fingerprint,
/// a project, a Change, and two waivers under the old table's name.
fn a_t_5_3_store(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("ledger.db");
    let conn = Connection::open(&path).expect("open");
    conn.execute_batch(&fixture_schema())
        .expect("the T-5-3 schema fixture must be executable SQL");
    conn.execute(
        "INSERT INTO schema_migrations(version) VALUES(?1)",
        [SCHEMA_BEFORE],
    )
    .expect("stamp T-5-3's own fingerprint");
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
    for (id, obligation, kind, actor, authority, rationale) in WAIVERS {
        conn.execute(
            "INSERT INTO change_dispositions(id, change_id, obligation_id, kind, actor,
                                             authority, rationale, disposed_at)
             VALUES(?1,'ch1',?2,?3,?4,?5,?6,'2026-09-24T03:00:00Z')",
            rusqlite::params![id, obligation, kind, actor, authority, rationale],
        )
        .expect("a waiver the T-5-3 binary could have written");
    }
    drop(conn);
    path
}

/// Every column of every waiver, in one string per row, so "byte-identical"
/// is asked of the values and not of a row count.
fn waiver_rows(conn: &Connection, table: &str, time_column: &str) -> Vec<String> {
    let sql = format!(
        "SELECT id||'|'||change_id||'|'||obligation_id||'|'||kind||'|'||actor||'|'||
                authority||'|'||rationale||'|'||{time_column}
           FROM {table} ORDER BY id"
    );
    let mut statement = conn.prepare(&sql).expect("prepare");
    statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query")
        .filter_map(|r| r.ok())
        .collect()
}

fn objects(conn: &Connection) -> Vec<(String, String)> {
    let mut statement = conn
        .prepare(
            "SELECT type, name FROM sqlite_master
              WHERE type IN ('table','index','trigger') AND name NOT LIKE 'sqlite_%'
              ORDER BY type, name",
        )
        .expect("prepare");
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("query")
        .filter_map(|r| r.ok())
        .collect()
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
    assert!(
        fixture_schema().contains("CREATE TABLE IF NOT EXISTS change_dispositions("),
        "the fixture must be the schema that HAD the old name, or this test \
         proves nothing about renaming it"
    );
}

#[test]
fn the_schema_version_moved_when_the_table_was_renamed() {
    let now = Ledger::schema_fingerprint();
    assert_ne!(
        now, SCHEMA_BEFORE,
        "renaming a table must move the schema version"
    );
    assert!(now.starts_with("schema_sha256_"));
}

/// **Every verb exits 3 until the upgrade is asked for.** A person who
/// installs this binary and runs anything at all is told, and pays nothing.
#[test]
fn a_t_5_3_store_is_refused_until_the_upgrade_is_asked_for() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = a_t_5_3_store(dir.path());

    assert!(Ledger::upgrade_state(&path).expect("state").is_pending());
    match Ledger::open(&path) {
        Ok(_) => panic!("open must refuse a pending migration"),
        Err(CoreError::UpgradePending { estimate, .. }) => assert!(!estimate.is_empty()),
        Err(other) => panic!("expected UpgradePending, got {other:?}"),
    }
    // And the store is untouched by having been refused.
    let conn = Connection::open(&path).expect("reopen");
    assert_eq!(
        waiver_rows(&conn, "change_dispositions", "disposed_at").len(),
        2
    );
}

/// The rows are the same rows: same ids, same words, same time, under the new
/// name. Nothing was copied into a new table and nothing was re-derived.
#[test]
fn the_waivers_survive_the_rename_byte_for_byte() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = a_t_5_3_store(dir.path());

    let before = {
        let conn = Connection::open(&path).expect("reopen");
        waiver_rows(&conn, "change_dispositions", "disposed_at")
    };
    assert_eq!(before.len(), 2, "the store under test must have waivers");

    drop(Ledger::open_upgrading(&path, &mut |_, _, _| {}).expect("the upgrade must succeed"));
    assert_eq!(
        Ledger::upgrade_state(&path).expect("state"),
        UpgradeState::Current
    );

    let conn = Connection::open(&path).expect("reopen");
    let after = waiver_rows(&conn, "change_obligation_waivers", "recorded_at");
    assert_eq!(
        after, before,
        "a rename carries the rows; these are not the same rows"
    );

    // The old name is gone — index and triggers with it, not only the table.
    let names: Vec<String> = objects(&conn).into_iter().map(|(_, n)| n).collect();
    for retired in [
        "change_dispositions",
        "idx_change_dispositions_change",
        "change_dispositions_no_replace",
        "change_dispositions_no_update",
        "change_dispositions_no_delete",
    ] {
        assert!(
            !names.contains(&retired.to_string()),
            "{retired} is still in the store after the rename"
        );
    }
    for arrived in [
        "change_obligation_waivers",
        "idx_change_obligation_waivers_change",
        "change_obligation_waivers_no_replace",
        "change_obligation_waivers_no_update",
        "change_obligation_waivers_no_delete",
    ] {
        assert!(
            names.contains(&arrived.to_string()),
            "{arrived} is missing after the rename"
        );
    }

    // The column that carried the retired word went with it, and the approval
    // table's foreign key now points at the new name by its new column.
    let mut statement = conn
        .prepare("PRAGMA table_info(change_decision_exceptions)")
        .expect("prepare");
    let columns: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(1))
        .expect("query")
        .filter_map(|r| r.ok())
        .collect();
    assert!(columns.contains(&"waiver_id".to_string()));
    assert!(!columns.contains(&"disposition_id".to_string()));
}

/// **An exception is still append-only after the rename.** The triggers were
/// dropped and recreated under new names; a store whose waivers could be
/// edited afterwards would be the rename quietly removing the guarantee the
/// table exists to make.
#[test]
fn the_renamed_table_still_refuses_every_edit_and_every_non_waivable_id() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = a_t_5_3_store(dir.path());
    drop(Ledger::open_upgrading(&path, &mut |_, _, _| {}).expect("the upgrade must succeed"));

    let conn = Connection::open(&path).expect("reopen");
    assert!(
        conn.execute(
            "UPDATE change_obligation_waivers SET rationale='something else' WHERE id='cx_1'",
            []
        )
        .is_err(),
        "a waiver is never edited"
    );
    assert!(
        conn.execute("DELETE FROM change_obligation_waivers WHERE id='cx_1'", [])
            .is_err(),
        "a waiver is never deleted"
    );
    assert!(
        conn.execute(
            "INSERT INTO change_obligation_waivers(id, change_id, obligation_id, kind, actor,
                                                   authority, rationale, recorded_at)
             VALUES('cx_1','ch1','criterion:9','waived','sam','p','r','2026-09-24T04:00:00Z')",
            []
        )
        .is_err(),
        "an id is written once"
    );
    for refused in [
        "candidate_identity",
        "decision_authority",
        "packet_consistency",
    ] {
        assert!(
            conn.execute(
                "INSERT INTO change_obligation_waivers(id, change_id, obligation_id, kind, actor,
                                                       authority, rationale, recorded_at)
                 VALUES(?1,'ch1',?1,'waived','sam','p','r','2026-09-24T04:00:00Z')",
                [refused]
            )
            .is_err(),
            "{refused} must still be refused by the table itself"
        );
    }
}

/// Running it twice changes nothing, and a store that never had the old table
/// is not touched by the step at all.
#[test]
fn the_rename_is_idempotent_and_a_fresh_store_never_meets_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = a_t_5_3_store(dir.path());
    drop(Ledger::open_upgrading(&path, &mut |_, _, _| {}).expect("first upgrade"));
    let once = {
        let conn = Connection::open(&path).expect("reopen");
        (
            objects(&conn),
            waiver_rows(&conn, "change_obligation_waivers", "recorded_at"),
        )
    };
    drop(Ledger::open_upgrading(&path, &mut |_, _, _| {}).expect("a second upgrade is a no-op"));
    let twice = {
        let conn = Connection::open(&path).expect("reopen");
        (
            objects(&conn),
            waiver_rows(&conn, "change_obligation_waivers", "recorded_at"),
        )
    };
    assert_eq!(once, twice, "the second upgrade changed something");

    let fresh_dir = tempfile::tempdir().expect("tempdir");
    let fresh = fresh_dir.path().join("fresh.db");
    drop(Ledger::open(&fresh).expect("a fresh ledger must still open"));
    let conn = Connection::open(&fresh).expect("reopen");
    let names: Vec<String> = objects(&conn).into_iter().map(|(_, n)| n).collect();
    assert!(names.contains(&"change_obligation_waivers".to_string()));
    assert!(!names.contains(&"change_dispositions".to_string()));
}

/// A store that somehow had both tables is refused rather than guessed at.
/// Nothing in the product can produce one; a person with a SQL prompt can, and
/// the failure mode of guessing is losing the waivers somebody recorded.
#[test]
fn a_store_with_both_tables_is_refused_by_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = a_t_5_3_store(dir.path());
    {
        let conn = Connection::open(&path).expect("reopen");
        conn.execute_batch("CREATE TABLE change_obligation_waivers(id TEXT PRIMARY KEY);")
            .expect("a hand-made second table");
    }
    match Ledger::open_upgrading(&path, &mut |_, _, _| {}) {
        Ok(_) => panic!("the upgrade must refuse a store with both tables"),
        Err(CoreError::Config(message)) => {
            assert!(
                message.contains("change_dispositions")
                    && message.contains("change_obligation_waivers"),
                "the refusal must name both tables: {message}"
            );
        }
        Err(other) => panic!("expected a refusal naming both tables, got {other:?}"),
    }
}

/// The registry file for this rename says what this migration did, and the
/// rename it records is the rename that happened.
#[test]
fn the_registry_records_this_rename() {
    let recorded = migration_registry::renames();
    assert!(
        recorded.iter().any(|r| r.kind == "table"
            && r.from == "change_dispositions"
            && r.to == "change_obligation_waivers"),
        "crates/core/migrations/100-r-2026-09-24-22.toml must record the table rename"
    );
    let entry = migration_registry::entries()
        .into_iter()
        .find(|e| e.task == "R-2026-09-24-22")
        .expect("this rename owes a registry file");
    assert_eq!(entry.pinned_schema_digest, SCHEMA_BEFORE);
    assert_eq!(
        entry.migration_test,
        "crates/core/tests/obligation_waiver_rename_migration.rs"
    );
    assert!(
        entry.tables.is_empty(),
        "a rename adds no table, and claiming one would make every later-task \
         list wrong"
    );
}
