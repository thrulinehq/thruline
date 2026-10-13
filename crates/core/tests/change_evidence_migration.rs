//! T-3-1 — the criterion ↔ evidence table arrives as a versioned migration and
//! costs nothing until `thruline upgrade` is run deliberately (A168/A193).
//!
//! **This task changes DDL; `thruline upgrade` is required; every other verb
//! exits 3 until it runs.** These tests are that sentence, mechanically, and
//! they are the same four T-2-1 established, re-asked for this table:
//!
//! * the schema version moved, because the version IS the digest of the DDL;
//! * an existing store is refused until the upgrade is asked for;
//! * the upgrade adds exactly one table, alters none, and is idempotent;
//! * every row an older binary wrote is still there and still readable —
//!   byte-identical, asserted on a fixture store carrying rows in every table
//!   the Change layer already had.

use rusqlite::Connection;
use thruline_core::error::CoreError;
use thruline_core::ledger::Ledger;
use thruline_core::upgrade::UpgradeState;

/// The DDL digest at `ee6be52d`, the Phase 2 exit this task was built on.
/// Pinned so the bump is a fact rather than a belief.
const SCHEMA_AT_PHASE_2_EXIT: &str =
    "schema_sha256_10a22ce71f7b6b0fbe1ca6d9524377fa931fda7240ad2549e5fb54a661d8d34e";

/// The one table this task adds.
const NEW_TABLE: &str = "change_criterion_evidence";

/// The tables later tasks added on top of this one are read from the DDL task
/// registry at `crates/core/migrations/`, not listed here (R-2026-09-24-21).
/// The point of the comparison below is unchanged: the set of added tables is
/// EXACTLY known, and a table nobody claimed is the failure this test and
/// `migration_registry_guard.rs` exist to catch.
mod migration_registry;

/// This task's ID, as the registry names it.
const THIS_TASK: &str = "T-3-1";

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
fn the_schema_version_moved_when_the_criterion_evidence_table_landed() {
    let now = Ledger::schema_fingerprint();
    assert_ne!(
        now, SCHEMA_AT_PHASE_2_EXIT,
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
    drop(conn);

    drop(Ledger::open_upgrading(&path, &mut |_, _, _| {}).expect("a second upgrade is a no-op"));
    assert_eq!(
        Ledger::upgrade_state(&path).expect("state"),
        UpgradeState::Current
    );
}

/// The migration adds ONE table and alters none, proven against the schema at
/// the Phase 2 exit rather than against a memory of it.
#[test]
fn the_migration_adds_one_table_and_alters_no_existing_one() {
    let phase2 = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/schema-phase-2.sql"),
    )
    .expect("the Phase 2 schema fixture must be readable");

    let before = tempfile::tempdir().expect("tempdir");
    let old = Connection::open(before.path().join("old.db")).expect("open");
    old.execute_batch(&phase2)
        .expect("apply the Phase 2 schema");

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
    assert!(!old_shape.is_empty(), "the Phase 2 schema fixture is empty");

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
    let mut expected = vec![NEW_TABLE.to_string()];
    expected.extend(migration_registry::tables_added_after(THIS_TASK));
    expected.sort();
    let mut added = added;
    added.sort();
    assert_eq!(added, expected);
}

/// Every row an older binary wrote is still there, byte for byte, after the
/// upgrade — the compatibility test T-2-1 established, re-run over a store that
/// carries rows in every Change table.
#[test]
fn every_row_written_before_the_upgrade_survives_it_byte_identically() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("ledger.db");

    // A store on the Phase 2 schema, with content.
    let phase2 = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/schema-phase-2.sql"),
    )
    .expect("fixture");
    let conn = Connection::open(&path).expect("open");
    conn.execute_batch(&phase2).expect("apply Phase 2 schema");
    seed(&conn);
    let before = census(&conn);
    assert!(
        before.iter().all(|(_, rows)| !rows.is_empty()),
        "the fixture must actually carry rows: {before:?}"
    );
    drop(conn);

    // It is refused until the upgrade is asked for, and then it is paid once.
    assert!(Ledger::upgrade_state(&path).expect("state").is_pending());
    drop(Ledger::open_upgrading(&path, &mut |_, _, _| {}).expect("upgrade"));

    let conn = Connection::open(&path).expect("reopen");
    let after = census(&conn);
    assert_eq!(before, after, "the upgrade rewrote stored rows");
    let new_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM change_criterion_evidence", [], |r| {
            r.get(0)
        })
        .expect("the new table must be readable");
    assert_eq!(new_rows, 0, "a migration invents no links");
}

/// Rows in every table the Change layer had before this task, as a text census
/// that a rewrite would change.
fn census(conn: &Connection) -> Vec<(String, Vec<String>)> {
    let tables = [
        ("projects", "SELECT id||'|'||root_path FROM projects ORDER BY id"),
        ("sessions", "SELECT id||'|'||file_path FROM sessions ORDER BY id"),
        (
            "events",
            "SELECT id||'|'||kind||'|'||chain_hash FROM events ORDER BY id",
        ),
        (
            "changes",
            "SELECT id||'|'||title||'|'||requester FROM changes ORDER BY id",
        ),
        (
            "change_scope_revisions",
            "SELECT change_id||'|'||scope_version||'|'||criteria_json||'|'||content_sha256
               FROM change_scope_revisions ORDER BY change_id, scope_version",
        ),
        (
            "change_scope_acceptances",
            "SELECT change_id||'|'||scope_version||'|'||accepted_by||'|'||rationale
               FROM change_scope_acceptances ORDER BY change_id, scope_version",
        ),
        (
            "change_session_associations",
            "SELECT id||'|'||provenance||'|'||basis||'|'||COALESCE(actor,'')||'|'||COALESCE(reason,'')
               FROM change_session_associations ORDER BY id",
        ),
        (
            "evidence_bindings",
            "SELECT id||'|'||scope_class||'|'||scope_key||'|'||doc_sha256
               FROM evidence_bindings ORDER BY id",
        ),
    ];
    tables
        .iter()
        .map(|(name, sql)| {
            let mut statement = conn.prepare(sql).expect("prepare");
            let rows: Vec<String> = statement
                .query_map([], |row| row.get::<_, String>(0))
                .expect("query")
                .filter_map(|r| r.ok())
                .collect();
            ((*name).to_string(), rows)
        })
        .collect()
}

fn seed(conn: &Connection) {
    conn.execute(
        "INSERT INTO projects(id, root_path, source, display_name, created_at)
         VALUES('p1','/tmp/synthetic','claude_code','Synthetic','2026-01-01T00:00:00Z')",
        [],
    )
    .expect("project");
    conn.execute(
        "INSERT INTO sessions(id, project_id, file_path, started_at)
         VALUES('s1','p1','/tmp/synthetic/s1.jsonl','2026-01-01T00:00:00Z')",
        [],
    )
    .expect("session");
    conn.execute(
        "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name, tool_input,
             raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
         VALUES('e1','p1','s1',1,'2026-01-01T00:10:00Z','tool_use','Edit',
                '{\"file_path\":\"/tmp/synthetic/crates/core/src/importer.rs\"}',
                '/tmp/synthetic/s1.jsonl',1,'aa','','bb')",
        [],
    )
    .expect("event");
    conn.execute(
        "INSERT INTO changes(id, project_id, opened_at, title, purpose, requester, request_source)
         VALUES('c1','p1','2026-01-02T09:00:00Z','T','P','sam','person')",
        [],
    )
    .expect("change");
    conn.execute(
        "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at, criteria_json,
             exclusions_json, allowed_scope_json, constraints_json, check_profile_json,
             content_sha256)
         VALUES('c1',1,'2026-01-02T09:30:00Z','[\"one thing\"]','[]','[\"crates/**\"]','[]',
                '[\"cargo-test\"]','digest')",
        [],
    )
    .expect("scope");
    conn.execute(
        "INSERT INTO change_scope_acceptances(change_id, scope_version, accepted_by, accepted_at,
             rationale)
         VALUES('c1',1,'sam','2026-01-02T09:40:00Z','this is what counts')",
        [],
    )
    .expect("acceptance");
    conn.execute(
        "INSERT INTO change_session_associations(id, change_id, session_id, provenance, basis,
             actor, asserted_at, reason)
         VALUES('a1','c1','s1','confirmed','user','sam','2026-01-02T10:00:00Z','mine')",
        [],
    )
    .expect("association");
    conn.execute(
        "INSERT INTO evidence_bindings(id, seq, recorded_at, schema_version, scope_class,
             scope_key, root, repository_identity, pre_identity, post_identity, exit_code,
             term_signal, bound_at, source_session, doc, doc_sha256)
         VALUES('eb1',0,'2026-01-03T09:00:00Z',1,'build','bash:/tmp/synthetic:cargo clippy',
                '/tmp/synthetic','repo','pre','post',0,NULL,'2026-01-03T09:00:00Z','s1',
                '{\"stdout\":{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":1},
                  \"stderr\":{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":1}}','ddd')",
        [],
    )
    .expect("binding");
}
