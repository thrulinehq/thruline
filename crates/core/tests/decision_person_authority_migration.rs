//! C24 Decisions Phase 2: person-action receipts are a distinct DDL upgrade.

use rusqlite::{params, Connection};
use thruline_core::chain::sha256_hex;
use thruline_core::error::CoreError;
use thruline_core::ledger::Ledger;
use thruline_core::upgrade::UpgradeState;

mod migration_registry;

const THIS_TASK: &str = "C24-PERSON-AUTHORITY";
const SCHEMA_BEFORE: &str =
    "schema_sha256_bedaa8ab11a700afb23d6b574e2a675fe571a3fbe8e2a6823a16706a47cf05ce";

fn fixture_schema() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/schema-c24-authority-base.sql"),
    )
    .expect("pre-authority Decisions schema fixture")
}

#[test]
fn registry_pins_the_exact_phase_one_schema() {
    let digest = format!("schema_sha256_{}", sha256_hex(fixture_schema().as_bytes()));
    assert_eq!(digest, SCHEMA_BEFORE);
    assert_ne!(Ledger::schema_fingerprint(), SCHEMA_BEFORE);
    let entry = migration_registry::entries()
        .into_iter()
        .find(|entry| entry.task == THIS_TASK)
        .expect("separate Phase 2 registry entry");
    assert_eq!(entry.pinned_schema_digest, SCHEMA_BEFORE);
    assert_eq!(entry.tables, vec!["decision_person_acts"]);
    assert_eq!(entry.indexes, vec!["idx_decision_person_acts_decision"]);
    assert_eq!(entry.triggers.len(), 3);
}

#[test]
fn populated_phase_one_store_refuses_open_until_upgrade_and_preserves_acts() {
    let dir = tempfile::tempdir().expect("scratch");
    let path = dir.path().join("old-ledger.db");
    let conn = Connection::open(&path).expect("old store");
    conn.execute_batch(&fixture_schema()).expect("old DDL");
    conn.execute_batch(
        "INSERT INTO projects(id,root_path,source,created_at)
           VALUES('old-project','/old/repo','codex','2026-10-07T00:00:00Z');
         INSERT INTO decisions(id,project_id,created_at)
           VALUES('old-decision','old-project','2026-10-07T00:00:00Z');
         INSERT INTO decision_revisions(decision_id,rev,kind,params_schema_version,
             params_json,text,sources_json,author_actor,recorded_at)
           VALUES('old-decision',1,'path_present',1,'{}','Keep it','[]','person',
                  '2026-10-07T00:00:00Z');
         INSERT INTO decision_acceptances(id,decision_id,rev,actor,accepted_at,rationale,
             severity,applies_from_version)
           VALUES('old-acceptance','old-decision',1,'person','2026-10-07T00:01:00Z',
                  'Earlier simulation','blocks','old-version');",
    )
    .expect("historical act");
    conn.execute(
        "INSERT INTO schema_migrations(version) VALUES(?1)",
        params![SCHEMA_BEFORE],
    )
    .expect("old schema fingerprint");
    drop(conn);

    assert!(Ledger::upgrade_state(&path).expect("state").is_pending());
    assert!(matches!(
        Ledger::open(&path),
        Err(CoreError::UpgradePending { .. })
    ));
    let conn = Connection::open(&path).expect("read old store");
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='decision_person_acts'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .expect("old table census"),
        0,
        "refusal must not run the migration"
    );
    drop(conn);

    drop(Ledger::open_upgrading(&path, &mut |_, _, _| {}).expect("explicit upgrade"));
    assert_eq!(
        Ledger::upgrade_state(&path).expect("upgraded state"),
        UpgradeState::Current
    );
    let conn = Connection::open(&path).expect("read upgraded store");
    assert_eq!(
        conn.query_row(
            "SELECT actor || ':' || rationale FROM decision_acceptances
             WHERE id='old-acceptance'",
            [],
            |row| row.get::<_, String>(0),
        )
        .expect("historical acceptance"),
        "person:Earlier simulation"
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM decision_person_acts", [], |row| {
            row.get::<_, i64>(0)
        })
        .expect("person receipts"),
        0,
        "an upgrade must not invent confirmation for historical acts"
    );
    drop(conn);
    let reopened = Ledger::open(&path).expect("open upgraded store");
    assert!(
        thruline_core::decisions::storage::accepted_for_project(&reopened, "old-project")
            .expect("active Decisions")
            .is_empty(),
        "the historical acceptance remains inert after migration"
    );
}

#[test]
fn authority_ddl_adds_one_table_one_index_three_triggers_without_altering_old_objects() {
    let old = Connection::open_in_memory().expect("old schema");
    old.execute_batch(&fixture_schema()).expect("old DDL");
    let new = Connection::open_in_memory().expect("new schema");
    new.execute_batch(Ledger::schema_ddl()).expect("new DDL");
    let shape = |conn: &Connection| -> Vec<(String, String)> {
        let mut statement = conn
            .prepare(
                "SELECT name,COALESCE(sql,'') FROM sqlite_master
                 WHERE type IN ('table','index','trigger')
                   AND name NOT LIKE 'sqlite_autoindex_%' ORDER BY name",
            )
            .expect("shape query");
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("shape rows")
            .map(|row| row.expect("schema object"))
            .collect()
    };
    let old_shape = shape(&old);
    let new_shape = shape(&new);
    for object in &old_shape {
        assert!(
            new_shape.contains(object),
            "existing object changed: {}",
            object.0
        );
    }
    let mut added: Vec<String> = new_shape
        .iter()
        .filter(|object| !old_shape.contains(object))
        .map(|object| object.0.clone())
        .collect();
    added.sort();
    let mut expected = [
        "decision_person_acts",
        "idx_decision_person_acts_decision",
        "decision_person_acts_no_replace",
        "decision_person_acts_no_update",
        "decision_person_acts_no_delete",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    // Tables later tasks added on top of this one are read from the registry
    // (R-2026-09-24-21), so a task after this one cannot turn this test red.
    expected.extend(migration_registry::objects_added_after(THIS_TASK));
    expected.sort();
    assert_eq!(added, expected);
}
