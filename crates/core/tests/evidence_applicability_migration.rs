//! T-S-2 — the evidence-applicability table arrives as a versioned migration
//! and costs nothing until `thruline upgrade` is run deliberately (A168/A193).
//!
//! Same four questions every migration test in this crate asks:
//!
//! * the schema version moved, because the version IS the digest of the DDL;
//! * an existing store is refused until the upgrade is asked for;
//! * the upgrade adds exactly one table, alters none, and is idempotent;
//! * every row an older binary wrote is still there and still readable —
//!   byte-identical, asserted on a fixture store carrying rows in every table
//!   the Change layer had at the commit this task was built on (HEAD
//!   `9f294bab`, sprint 2026-09-22).

use rusqlite::Connection;
use thruline_core::error::CoreError;
use thruline_core::ledger::Ledger;
use thruline_core::upgrade::UpgradeState;

/// The DDL digest at `9f294bab`, the commit this task was built on. Pinned so
/// the bump is a fact rather than a belief; it is the sha256 of
/// `tests/fixtures/schema-t-s-2-base.sql`, which is that commit's own DDL.
const SCHEMA_BEFORE: &str =
    "schema_sha256_a11ca26e743fd3d4cc68f7cfce465ae9db718b10de7cb0cc7c887087e950f4e5";

/// The one table this task adds.
const NEW_TABLE: &str = "change_criterion_freshness";

fn fixture_schema() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/schema-t-s-2-base.sql"),
    )
    .expect("the base schema fixture must be readable")
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
fn the_schema_version_moved_when_the_applicability_table_landed() {
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
    drop(conn);

    drop(Ledger::open_upgrading(&path, &mut |_, _, _| {}).expect("a second upgrade is a no-op"));
    assert_eq!(
        Ledger::upgrade_state(&path).expect("state"),
        UpgradeState::Current
    );
}

/// The migration adds ONE table and alters none, proven against the schema at
/// `9f294bab` rather than against a memory of it.
#[test]
fn the_migration_adds_one_table_and_alters_no_existing_one() {
    let before = tempfile::tempdir().expect("tempdir");
    let old = Connection::open(before.path().join("old.db")).expect("open");
    old.execute_batch(&fixture_schema())
        .expect("apply the pre-T-S-2 schema");

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

    let added: Vec<String> = new_shape
        .iter()
        .map(|(n, _)| n.clone())
        .filter(|n| !old_shape.iter().any(|(o, _)| o == n))
        .collect();
    // Tables later tasks added on top of this one, each with its own entry in
    // the layer's migration tests: T-S-4's `change_decisions`, T-3-4's
    // `change_run_executions` (what became of a check's execution), and
    // T-3-6-F1's `check_run_tallies` (how many tests one run counted).
    let mut expected = vec![
        NEW_TABLE.to_string(),
        "change_decisions".to_string(),
        "change_run_executions".to_string(),
        "check_run_tallies".to_string(),
        "check_run_records".to_string(),
        // T-5-3 — the two exception tables.
        "change_obligation_waivers".to_string(),
        "change_decision_exceptions".to_string(),
        // P6-5B — `run_permission_migration.rs` is its own compatibility test.
        "run_permissions".to_string(),
        // T-4-1 — the four review tables (`change_review_migration.rs`).
        "change_reviews".to_string(),
        "change_review_raw_artifacts".to_string(),
        "change_review_parsed_artifacts".to_string(),
        "change_review_findings".to_string(),
        // T-4-2 — where each finding stands (`finding_standing_migration.rs`).
        "change_review_finding_standings".to_string(),
        // T-4-4 — a person's attempt budget (`attempt_budget_migration.rs`).
        "change_attempt_budgets".to_string(),
        // C24 — append-only standing Decisions and person authority (orders 150–151).
        "decisions".to_string(),
        "decision_revisions".to_string(),
        "decision_acceptances".to_string(),
        "decision_retirements".to_string(),
        "decision_verdicts".to_string(),
        "decision_waivers".to_string(),
        "decision_person_acts".to_string(),
        // C30 R-LEDGER — what a person did (order 160; `r_ledger_migration.rs`).
        "change_notes".to_string(),
        "statement_objections".to_string(),
        "change_decision_seconds".to_string(),
        "change_agent_messages".to_string(),
        "capture_holds".to_string(),
        "inbox_hides".to_string(),
        // C47 — named per-project removal receipt (order 170).
        "project_removals".to_string(),
    ];
    expected.sort();
    assert_eq!(added, expected);
}

/// Every row an older binary wrote is still there, byte for byte, after the
/// upgrade.
#[test]
fn every_row_written_before_the_upgrade_survives_it_byte_identically() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("ledger.db");

    let conn = Connection::open(&path).expect("open");
    conn.execute_batch(&fixture_schema())
        .expect("apply the pre-T-S-2 schema");
    seed(&conn);
    let before = census(&conn);
    assert!(
        before.iter().all(|(_, rows)| !rows.is_empty()),
        "the fixture must actually carry rows: {before:?}"
    );
    drop(conn);

    assert!(Ledger::upgrade_state(&path).expect("state").is_pending());
    drop(Ledger::open_upgrading(&path, &mut |_, _, _| {}).expect("upgrade"));

    let conn = Connection::open(&path).expect("reopen");
    let after = census(&conn);
    assert_eq!(before, after, "the upgrade rewrote stored rows");
    let new_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM change_criterion_freshness", [], |r| {
            r.get(0)
        })
        .expect("the new table must be readable");
    assert_eq!(
        new_rows, 0,
        "a migration invents no applicability record of its own"
    );
}

fn census(conn: &Connection) -> Vec<(String, Vec<String>)> {
    let tables = [
        (
            "changes",
            "SELECT id||'|'||title||'|'||requester FROM changes ORDER BY id",
        ),
        (
            "change_criterion_evidence",
            "SELECT id||'|'||criterion_index||'|'||evidence_kind||'|'||basis||'|'||COALESCE(detail,'')
               FROM change_criterion_evidence ORDER BY id",
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
    conn.execute(
        "INSERT INTO change_criterion_evidence(id, change_id, scope_version, criterion_index,
             evidence_kind, evidence_id, session_id, basis, detail, actor, asserted_at, reason)
         VALUES('cce1','c1',1,1,'check_run','eb1','s1','declared','cargo-test',NULL,
                '2026-01-03T09:10:00Z',NULL)",
        [],
    )
    .expect("criterion link");
}
