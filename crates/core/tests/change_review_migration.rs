//! T-4-1 — the four review tables arrive as a versioned migration and cost
//! nothing until `thruline upgrade` is run deliberately (A168/A193).
//!
//! **This task changes DDL; `thruline upgrade` is required; every other verb
//! exits 3 until it runs.** Asked against the schema at `da9afbf`, the review
//! lane's base commit, whose DDL is `tests/fixtures/schema-review-t-4-1-base.sql`.

use rusqlite::{params, Connection};
use thruline_core::error::CoreError;
use thruline_core::ledger::{Ledger, CHANGE_REVIEW_MIGRATION, REVIEW_TABLES};
use thruline_core::upgrade::UpgradeState;

/// The DDL digest at `da9afbf`. Pinned so the bump is a fact, not a belief.
const SCHEMA_BEFORE: &str =
    "schema_sha256_de2b05e56a94443dfecc594e763bf6fa2249e8bf030a0062fc83a90673dd7d4a";

/// The DDL task registry at `crates/core/migrations/` (R-2026-09-24-21). The
/// review lane predates it; it is read here since the A2 merge.
mod migration_registry;

/// Every object (table, index, trigger) of the tasks that landed AHEAD of
/// T-4-1 while being built on the very schema T-4-1 was built on — T-5-3 and
/// P6-5B, parallel lanes whose objects are absent from this fixture although
/// they are ordered first. Read from the registry, not listed here, the way
/// `run_permission_migration.rs` reads the same fact.
fn objects_of_parallel_lanes() -> Vec<String> {
    let entries = migration_registry::entries();
    let review = entries
        .iter()
        .find(|e| e.task == "T-4-1")
        .expect("the registry must name T-4-1");
    entries
        .iter()
        .filter(|e| e.order < review.order && e.pinned_schema_digest == review.pinned_schema_digest)
        .flat_map(|e| {
            e.tables
                .iter()
                .chain(e.indexes.iter())
                .chain(e.triggers.iter())
                .cloned()
                .collect::<Vec<_>>()
        })
        .collect()
}

fn fixture_schema() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/schema-review-t-4-1-base.sql"),
    )
    .expect("the pre-T-4-1 schema fixture must be readable")
}

#[test]
fn the_fixture_is_really_the_schema_at_the_base_commit() {
    let digest = format!(
        "schema_sha256_{}",
        thruline_core::chain::sha256_hex(fixture_schema().as_bytes())
    );
    assert_eq!(digest, SCHEMA_BEFORE);
}

#[test]
fn the_schema_version_moved_when_the_review_tables_landed() {
    assert_ne!(Ledger::schema_fingerprint(), SCHEMA_BEFORE);
}

/// A store exactly as the base commit would leave it: its whole DDL, its
/// fingerprint, and real rows in the Change layer — a Change, accepted
/// criteria, the `intended` examined-inputs record that names its version,
/// and an approval of that version.
fn base_commit_store(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("ledger.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(&fixture_schema()).unwrap();
    conn.execute_batch(
        "INSERT INTO projects(id,root_path,source,display_name,created_at)
           VALUES('p-old','/old/project','claude_code','Old','2026-01-01T00:00:00Z');
         INSERT INTO changes(id,project_id,opened_at,title,purpose,requester,request_source)
           VALUES('chg-old','p-old','2026-01-01T00:00:00Z','Old change','Kept','sam','person');
         INSERT INTO change_scope_revisions(change_id,scope_version,proposed_at,criteria_json,
             exclusions_json,allowed_scope_json,constraints_json,check_profile_json,content_sha256)
           VALUES('chg-old',1,'2026-01-01T00:01:00Z','[\"adds\"]','[]','[]','[]','[]',
             'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');
         INSERT INTO change_scope_acceptances(change_id,scope_version,accepted_by,accepted_at,rationale)
           VALUES('chg-old',1,'sam','2026-01-01T00:02:00Z','ok');
         INSERT INTO examination_inputs(id,subject_kind,subject_id,recorded_at,schema_version,
             base_revision,uncommitted_state,config_files_json,tools_json,target_kind)
           VALUES('exi-old','intended','chg-old','2026-01-01T00:03:00Z',1,
             'cccccccccccccccccccccccccccccccccccccccc','clean','[]','[]','none');
         INSERT INTO change_decisions(id,change_id,act,target_revision,scope_version,actor,decided_at)
           VALUES('dec-old','chg-old','approved','cccccccccccccccccccccccccccccccccccccccc',1,'sam',
             '2026-01-01T00:04:00Z');",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO schema_migrations(version) VALUES(?1)",
        params![SCHEMA_BEFORE],
    )
    .unwrap();
    path
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn an_old_store_is_refused_before_the_upgrade_and_keeps_every_row_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = base_commit_store(dir.path());
    let before: Vec<i64> = {
        let conn = Connection::open(&path).unwrap();
        [
            "changes",
            "change_scope_acceptances",
            "examination_inputs",
            "change_decisions",
        ]
        .iter()
        .map(|t| count(&conn, &format!("SELECT COUNT(*) FROM {t}")))
        .collect()
    };
    assert_eq!(before, vec![1, 1, 1, 1], "the old store holds real rows");

    // A193: refused, and nothing paid.
    assert!(Ledger::upgrade_state(&path).unwrap().is_pending());
    match Ledger::open(&path) {
        Err(CoreError::UpgradePending { estimate, .. }) => assert!(!estimate.is_empty()),
        Err(other) => panic!("expected UpgradePending, got {other:?}"),
        Ok(_) => panic!("open must refuse a pending migration"),
    }
    let conn = Connection::open(&path).unwrap();
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM sqlite_master WHERE name='change_reviews'"
        ),
        0,
        "the refusal paid nothing"
    );
    drop(conn);

    let mut steps = Vec::new();
    drop(
        Ledger::open_upgrading(&path, &mut |name, i, total| {
            steps.push(format!("{i}/{total} {name}"))
        })
        .unwrap(),
    );
    assert!(steps.iter().any(|s| s.ends_with("change review records")));
    assert_eq!(Ledger::upgrade_state(&path).unwrap(), UpgradeState::Current);

    let conn = Connection::open(&path).unwrap();
    for table in REVIEW_TABLES {
        assert_eq!(
            count(
                &conn,
                &format!(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{table}'"
                )
            ),
            1,
            "{table} must exist"
        );
        assert_eq!(
            count(&conn, &format!("SELECT COUNT(*) FROM {table}")),
            0,
            "a migration must invent no review"
        );
    }
    assert_eq!(
        count(
            &conn,
            &format!(
                "SELECT COUNT(*) FROM schema_migrations WHERE version='{CHANGE_REVIEW_MIGRATION}'"
            )
        ),
        1,
        "the named version is stamped"
    );
    let after: Vec<i64> = [
        "changes",
        "change_scope_acceptances",
        "examination_inputs",
        "change_decisions",
    ]
    .iter()
    .map(|t| count(&conn, &format!("SELECT COUNT(*) FROM {t}")))
    .collect();
    assert_eq!(after, before, "every earlier row is preserved");
    drop(conn);

    // Restart: the fast path, and the old Change still reads, with no reviews.
    let reopened = Ledger::open(&path).unwrap();
    assert!(reopened.took_schema_fast_path());
    let view = thruline_core::change::read_change(&reopened, "chg-old")
        .unwrap()
        .unwrap();
    assert!(view.reviews.is_empty());
    assert_eq!(
        view.decisions.history.len(),
        1,
        "the old approval is still there"
    );
    drop(reopened);
    drop(Ledger::open_upgrading(&path, &mut |_, _, _| {}).expect("a second upgrade is a no-op"));
}

/// The migration adds four tables and alters none, proven against the
/// base commit's schema rather than a memory of it.
#[test]
fn the_migration_adds_four_tables_and_alters_no_existing_one() {
    let old = Connection::open_in_memory().unwrap();
    old.execute_batch(&fixture_schema()).unwrap();
    let new = Connection::open_in_memory().unwrap();
    new.execute_batch(Ledger::schema_ddl()).unwrap();
    let shape = |conn: &Connection| -> Vec<(String, String)> {
        let mut s = conn
            .prepare("SELECT name, COALESCE(sql,'') FROM sqlite_master WHERE type IN ('table','trigger','index') AND name NOT LIKE 'sqlite_autoindex_%' ORDER BY name")
            .unwrap();
        s.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    };
    let (old_shape, new_shape) = (shape(&old), shape(&new));
    for (name, sql) in &old_shape {
        let found = new_shape.iter().find(|(n, _)| n == name);
        assert!(found.is_some(), "{name} disappeared");
        assert_eq!(&found.unwrap().1, sql, "{name} was altered");
    }
    let mut added: Vec<String> = new_shape
        .iter()
        .map(|(n, _)| n.clone())
        .filter(|n| !old_shape.iter().any(|(o, _)| o == n))
        .collect();
    added.sort();
    let mut expected: Vec<String> = REVIEW_TABLES.iter().map(|t| t.to_string()).collect();
    expected.extend(
        [
            "idx_change_reviews_change",
            "change_reviews_no_update",
            "change_reviews_no_delete",
            "change_review_raw_artifacts_no_update",
            "change_review_raw_artifacts_no_delete",
            "change_review_parsed_artifacts_no_update",
            "change_review_parsed_artifacts_no_delete",
            "change_review_findings_no_update",
            "change_review_findings_no_delete",
            // T-4-2 — added on top of the review tables, by a later task with
            // its own migration test (`finding_standing_migration.rs`).
            "change_review_finding_standings",
            "idx_change_review_finding_standings_finding",
            "change_review_finding_standings_chain",
            "change_review_finding_standings_no_replace",
            "change_review_finding_standings_no_update",
            "change_review_finding_standings_no_delete",
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    expected.extend(objects_of_parallel_lanes());
    // Tasks that landed after T-4-2 (whose objects are listed above), each
    // with its own registry file and migration test.
    expected.extend(migration_registry::objects_added_after("T-4-2"));
    expected.sort();
    assert_eq!(added, expected);
}

/// A `change_reviews` table in a shape that never shipped is refused loudly,
/// not silently rewritten.
#[test]
fn an_unshipped_review_table_shape_is_refused_not_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = base_commit_store(dir.path());
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE change_reviews(id TEXT PRIMARY KEY, change_id TEXT, review_round INTEGER,
            implementation_version INTEGER, state TEXT);",
    )
    .unwrap();
    drop(conn);
    let err = Ledger::open_upgrading(&path, &mut |_, _, _| {})
        .err()
        .expect("an unshipped shape must be refused");
    assert!(err.to_string().contains("target_revision"), "{err}");
}
