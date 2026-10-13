//! C47: a removal receipt arrives as an append-only schema addition. A person
//! cannot use the removal route until the one founder upgrade includes it.

use rusqlite::{params, Connection};
use thruline_core::error::CoreError;
use thruline_core::ledger::Ledger;

mod migration_registry;

const TASK: &str = "C47-REMOVE-RECORDS";
const BUILT_ON: &str = "0b463bc02f85da5eaf0489e38e58188958d0b4b7";
const SCHEMA_BEFORE: &str =
    "schema_sha256_cd760d4aee258c1a22deb71541ba85f67b53199bf8dc4cfbbbbccd8b39dc96e0";
const MARKER: &str = "-- C47 / Remove records per project (migration registry order 170).";

fn previous_ddl() -> &'static str {
    Ledger::schema_ddl()
        .split_once(MARKER)
        .expect("C47 must append its DDL after every existing object")
        .0
        .strip_suffix('\n')
        .expect("C47 starts after one separating newline")
}

#[test]
fn receipt_ddl_is_a_pure_append_and_claimed_by_order_170() {
    let before = previous_ddl();
    assert_eq!(
        format!(
            "schema_sha256_{}",
            thruline_core::chain::sha256_hex(before.as_bytes())
        ),
        SCHEMA_BEFORE,
        "earlier DDL must stay byte for byte unchanged"
    );
    let entry = migration_registry::entries()
        .into_iter()
        .find(|entry| entry.task == TASK)
        .expect("C47 must have a registry entry");
    assert_eq!(entry.order, 170);
    assert_eq!(entry.built_on_commit, BUILT_ON);
    assert_eq!(entry.pinned_schema_digest, SCHEMA_BEFORE);
    assert_eq!(
        entry.migration_test,
        "crates/core/tests/remove_records_migration.rs"
    );
    assert_eq!(entry.tables, ["project_removals"]);
    assert_eq!(entry.indexes, ["idx_project_removals_project"]);
    assert_eq!(
        entry.triggers,
        [
            "project_removals_no_replace",
            "project_removals_no_update",
            "project_removals_no_delete",
        ]
    );

    let appended = &Ledger::schema_ddl()[before.len()..];
    let statements: Vec<&str> = appended
        .lines()
        .map(str::trim_start)
        .filter(|line| !line.is_empty() && !line.starts_with("--"))
        .collect();
    for forbidden in ["ALTER ", "DROP ", "DELETE FROM", "UPDATE ", "RENAME "] {
        assert!(
            !statements.iter().any(|line| line.starts_with(forbidden)),
            "C47 DDL cannot {forbidden} an existing row or object"
        );
    }
    assert_ne!(Ledger::schema_fingerprint(), SCHEMA_BEFORE);
}

#[test]
fn the_receipt_adds_exactly_its_table_index_and_three_triggers() {
    let old = Connection::open_in_memory().expect("prior schema connection");
    old.execute_batch(previous_ddl()).expect("prior schema");
    let new = Connection::open_in_memory().expect("current schema connection");
    new.execute_batch(Ledger::schema_ddl())
        .expect("current schema");
    let objects = |conn: &Connection| -> Vec<(String, String, String)> {
        let mut statement = conn
            .prepare(
                "SELECT type, name, COALESCE(sql, '') FROM sqlite_master
                 WHERE type IN ('table', 'index', 'trigger')
                   AND name NOT LIKE 'sqlite_%'
                 ORDER BY type, name",
            )
            .expect("schema object query");
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .expect("schema objects")
            .collect::<std::result::Result<_, _>>()
            .expect("schema object row")
    };
    let before = objects(&old);
    let after = objects(&new);
    for object in &before {
        assert!(
            after.contains(object),
            "migration 170 changed or removed an existing schema object: {object:?}"
        );
    }
    let mut added: Vec<(String, String)> = after
        .iter()
        .filter(|object| !before.contains(object))
        .map(|(kind, name, _)| (kind.clone(), name.clone()))
        .collect();
    added.sort();
    let mut expected = vec![
        ("table".to_string(), "project_removals".to_string()),
        (
            "index".to_string(),
            "idx_project_removals_project".to_string(),
        ),
        (
            "trigger".to_string(),
            "project_removals_no_replace".to_string(),
        ),
        (
            "trigger".to_string(),
            "project_removals_no_update".to_string(),
        ),
        (
            "trigger".to_string(),
            "project_removals_no_delete".to_string(),
        ),
    ];
    expected.sort();
    assert_eq!(added, expected);
}

#[test]
fn an_existing_store_needs_upgrade_and_keeps_all_existing_rows() {
    let dir = tempfile::tempdir().expect("scratch store");
    let path = dir.path().join("ledger.db");
    let conn = Connection::open(&path).expect("scratch connection");
    conn.execute_batch(previous_ddl()).expect("prior schema");
    conn.execute(
        "INSERT INTO schema_migrations(version) VALUES(?1)",
        [SCHEMA_BEFORE],
    )
    .expect("prior fingerprint");
    conn.execute(
        "INSERT INTO projects(id,root_path,source,display_name,created_at)
         VALUES('kept','/scratch/kept','codex','Kept','2026-10-08T00:00:00Z')",
        [],
    )
    .expect("old project");
    drop(conn);

    assert!(Ledger::upgrade_state(&path).unwrap().is_pending());
    assert!(matches!(
        Ledger::open(&path),
        Err(CoreError::UpgradePending { .. })
    ));
    drop(Ledger::open_upgrading(&path, &mut |_, _, _| {}).expect("upgrade"));
    let conn = Connection::open(&path).expect("reopen upgraded store");
    let kept: (i64, String, String) = conn
        .query_row(
            "SELECT rowid,id,display_name FROM projects WHERE id='kept'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("kept project");
    assert_eq!(kept, (1, "kept".into(), "Kept".into()));
    let receipts: i64 = conn
        .query_row("SELECT COUNT(*) FROM project_removals", [], |row| {
            row.get(0)
        })
        .expect("empty receipt table");
    assert_eq!(receipts, 0, "an upgrade invents no removal");
    let old_stamp: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version=?1",
            [SCHEMA_BEFORE],
            |row| row.get(0),
        )
        .expect("old fingerprint retained");
    assert_eq!(old_stamp, 1);
}

#[test]
fn a_receipt_cannot_be_changed_or_deleted() {
    let dir = tempfile::tempdir().expect("scratch store");
    let path = dir.path().join("ledger.db");
    drop(Ledger::open(&path).expect("fresh store"));
    let conn = Connection::open(&path).expect("reopen");
    conn.execute(
        "INSERT INTO project_removals(
           id,actor,removed_at,hq_project_id,ledger_project_ids,roots,
           per_table_removed,session_heads_sha256,plan_sha256,
           store_bytes_before,files_removed)
         VALUES(?1,'person','2026-10-08T00:00:00Z','hq',
                '[\"p\"]','[\"/scratch/p\"]','{\"events\":2}',?2,?2,1024,'[]')",
        params!["removal-1", "a".repeat(64)],
    )
    .expect("one receipt");
    assert!(
        conn.execute(
            "INSERT INTO project_removals(
               id,actor,removed_at,hq_project_id,ledger_project_ids,roots,
               per_table_removed,session_heads_sha256,plan_sha256,
               store_bytes_before,files_removed)
             SELECT NULL,actor,removed_at,hq_project_id,ledger_project_ids,roots,
                    per_table_removed,session_heads_sha256,plan_sha256,
                    store_bytes_before,files_removed
             FROM project_removals WHERE id='removal-1'",
            [],
        )
        .is_err(),
        "a receipt without an id must be refused"
    );
    assert!(
        conn.execute(
            "INSERT OR REPLACE INTO project_removals
             SELECT * FROM project_removals WHERE id='removal-1'",
            [],
        )
        .is_err(),
        "replace must not bypass append-only receipts"
    );
    assert!(conn
        .execute(
            "UPDATE project_removals SET actor='other' WHERE id='removal-1'",
            [],
        )
        .is_err());
    assert!(conn
        .execute("DELETE FROM project_removals WHERE id='removal-1'", [])
        .is_err());
    let actor: String = conn
        .query_row(
            "SELECT actor FROM project_removals WHERE id='removal-1'",
            [],
            |row| row.get(0),
        )
        .expect("receipt remains");
    assert_eq!(actor, "person");
}
