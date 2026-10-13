//! T-4-2 — the finding-standing table arrives as a versioned migration and
//! costs nothing until `thruline upgrade` is run deliberately (A168/A193).
//!
//! **This task changes DDL; `thruline upgrade` is required; every other verb
//! exits 3 until it runs.** Asked against the schema at `7768962`, the commit
//! this card builds on, whose DDL is `tests/fixtures/schema-review-t-4-2-base.sql`.

use rusqlite::{params, Connection};
use thruline_core::change::{
    self, AcceptedScope, ChangeRequest, NewChange, RequestSource, ScopeAcceptance,
};
use thruline_core::change_review::{self, NewReview, ReviewFinding, ReviewState, ReviewerType};
use thruline_core::error::CoreError;
use thruline_core::finding_standing::{self, Standing};
use thruline_core::ledger::{Ledger, FINDING_STANDING_MIGRATION, FINDING_STANDING_TABLE};
use thruline_core::upgrade::UpgradeState;

/// The DDL digest at `7768962`. Pinned so the bump is a fact, not a belief.
const SCHEMA_BEFORE: &str =
    "schema_sha256_962adc5422449383aa29caeac49b43fe5341543056fa93d0f94120f1799d3af1";

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
            .join("tests/fixtures/schema-review-t-4-2-base.sql"),
    )
    .expect("the pre-T-4-2 schema fixture must be readable")
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
fn the_schema_version_moved_when_the_standing_table_landed() {
    assert_ne!(Ledger::schema_fingerprint(), SCHEMA_BEFORE);
}

const CHANGE: &str = "chg-old";

/// A review with two findings, sealed by the real writer. Written into a
/// scratch store first, then copied row for row into the old-schema store, so
/// the old store holds exactly what a real review leaves behind.
fn sealed_review_rows(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("scratch.db");
    let ledger = Ledger::open(&path).unwrap();
    ledger
        .conn()
        .execute_batch(
            "INSERT INTO projects(id,root_path,source,display_name,created_at)
               VALUES('p-old','/old/project','claude_code','Old','2026-01-01T00:00:00Z');",
        )
        .unwrap();
    change::insert_change(
        &ledger,
        &NewChange {
            id: CHANGE.into(),
            project_id: "p-old".into(),
            opened_at: "2026-01-01T00:00:00Z".into(),
            request: ChangeRequest {
                title: "Old change".into(),
                purpose: "Kept".into(),
                prompt_event_id: None,
                requester: "sam".into(),
                source: RequestSource::Person,
            },
        },
    )
    .unwrap();
    change::append_scope(
        &ledger,
        CHANGE,
        "2026-01-01T00:01:00Z",
        &AcceptedScope {
            criteria: vec!["adds".into()],
            exclusions: vec![],
            allowed_scope: vec![],
            constraints: vec![],
            check_profile: vec![],
        },
    )
    .unwrap();
    change::accept_scope(
        &ledger,
        CHANGE,
        1,
        &ScopeAcceptance {
            actor: "sam".into(),
            accepted_at: "2026-01-01T00:02:00Z".into(),
            rationale: "ok".into(),
        },
    )
    .unwrap();
    ledger
        .conn()
        .execute(
            "INSERT INTO examination_inputs(id,subject_kind,subject_id,recorded_at,schema_version,
                 base_revision,uncommitted_state,config_files_json,tools_json,target_kind)
               VALUES('exi-old','intended',?1,'2026-01-01T00:03:00Z',1,?2,'clean','[]','[]','none')",
            params![CHANGE, "c".repeat(40)],
        )
        .unwrap();
    let route = serde_json::json!({
        "endpoint": change_review::ROUTE_ENDPOINT,
        "model": "qwen3:8b",
        "model_manifest_sha256": "5".repeat(64),
        "runtime_sha256": "4".repeat(64),
        "provider_source_commit": change_review::PR_AGENT_SOURCE_COMMIT,
        "image_id": format!("sha256:{}", "9".repeat(64)),
        "plan_digest": "e".repeat(64),
    })
    .to_string();
    change_review::append_review(
        &ledger,
        &NewReview {
            id: "rev-old".into(),
            change_id: CHANGE.into(),
            review_round: 1,
            reviewer_identity: "PR-Agent 0.45.0 via qwen3:8b".into(),
            reviewer_type: ReviewerType::Model,
            review_source: "local/plain-diff".into(),
            reviewer_relationship: "not established".into(),
            relationship_provenance: "fixture".into(),
            target_revision: "c".repeat(40),
            scope_version: Some(1),
            examined_inputs_id: "exi-old".into(),
            diff_sha256: "d".repeat(64),
            route_identity_json: route,
            started_at: "2026-01-01T00:04:00Z".into(),
            ended_at: Some("2026-01-01T00:05:00Z".into()),
            state: ReviewState::Complete,
            recommendation: None,
            limitations: vec![],
            raw_bytes: b"{}".to_vec(),
            parser_id: "thruline.pr-agent-structured.v1".into(),
            parsed_json: "{}".into(),
            parse_complete: true,
            findings: vec![
                ReviewFinding {
                    kind: "SYNTHETIC bug".into(),
                    message: "subtracts".into(),
                    path: Some("calc.py".into()),
                    line: Some(2),
                },
                ReviewFinding {
                    kind: "SYNTHETIC nit".into(),
                    message: "naming".into(),
                    path: None,
                    line: None,
                },
            ],
        },
    )
    .unwrap();
    drop(ledger);
    path
}

const CARRIED: [&str; 10] = [
    "projects",
    "changes",
    "change_scope_revisions",
    "change_scope_acceptances",
    "examination_inputs",
    "change_reviews",
    "change_review_raw_artifacts",
    "change_review_parsed_artifacts",
    "change_review_findings",
    "workspace_snapshots",
];

/// A store exactly as the base commit would leave it: its whole DDL, its
/// fingerprint, and a Change holding a finished, sealed review with findings.
fn base_commit_store(dir: &std::path::Path) -> std::path::PathBuf {
    let scratch = sealed_review_rows(dir);
    let path = dir.join("ledger.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(&fixture_schema()).unwrap();
    conn.execute(
        "ATTACH DATABASE ?1 AS scratch",
        params![scratch.display().to_string()],
    )
    .unwrap();
    for table in CARRIED {
        conn.execute_batch(&format!(
            "INSERT INTO main.{table} SELECT * FROM scratch.{table};"
        ))
        .unwrap();
    }
    conn.execute_batch("DETACH DATABASE scratch;").unwrap();
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

fn dump(conn: &Connection) -> Vec<Vec<String>> {
    CARRIED
        .iter()
        .map(|t| {
            let mut s = conn
                .prepare(&format!("SELECT * FROM {t} ORDER BY rowid"))
                .unwrap();
            let n = s.column_count();
            s.query_map([], |row| {
                let mut out = Vec::new();
                for i in 0..n {
                    let v: rusqlite::types::Value = row.get(i)?;
                    out.push(format!("{v:?}"));
                }
                Ok(out.join("|"))
            })
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
        })
        .collect()
}

#[test]
fn an_old_store_is_refused_before_the_upgrade_and_keeps_every_row_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = base_commit_store(dir.path());
    let before = {
        let conn = Connection::open(&path).unwrap();
        assert_eq!(
            count(&conn, "SELECT COUNT(*) FROM change_review_findings"),
            2
        );
        dump(&conn)
    };

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
            &format!("SELECT COUNT(*) FROM sqlite_master WHERE name='{FINDING_STANDING_TABLE}'")
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
    assert!(steps
        .iter()
        .any(|s| s.ends_with("review finding standings")));
    assert_eq!(Ledger::upgrade_state(&path).unwrap(), UpgradeState::Current);

    let conn = Connection::open(&path).unwrap();
    assert_eq!(
        count(
            &conn,
            &format!("SELECT COUNT(*) FROM {FINDING_STANDING_TABLE}")
        ),
        0,
        "a migration must invent no standing"
    );
    assert_eq!(
        count(
            &conn,
            &format!(
                "SELECT COUNT(*) FROM schema_migrations WHERE version='{FINDING_STANDING_MIGRATION}'"
            )
        ),
        1,
        "the named version is stamped"
    );
    assert_eq!(
        dump(&conn),
        before,
        "every earlier row is preserved, byte for byte"
    );
    drop(conn);

    // Restart: the fast path, and the old findings read as open with no
    // decision recorded — nothing was fabricated for them.
    let reopened = Ledger::open(&path).unwrap();
    assert!(reopened.took_schema_fast_path());
    let view = finding_standing::read(&reopened, CHANGE).unwrap();
    assert_eq!(view.findings.len(), 2);
    for f in &view.findings {
        assert_eq!(f.standing, Standing::Open);
        assert!(!f.decided, "no decision is recorded for an old finding");
        assert!(f.history.is_empty());
    }
    assert_eq!(view.summary.still_wrong, 2);
    drop(reopened);
    drop(Ledger::open_upgrading(&path, &mut |_, _, _| {}).expect("a second upgrade is a no-op"));
}

/// The migration adds one table, one index and four triggers, and alters
/// none, proven against the base commit's schema rather than a memory of it.
#[test]
fn the_migration_adds_one_table_and_alters_no_existing_one() {
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
    let mut expected: Vec<String> = [
        FINDING_STANDING_TABLE,
        "idx_change_review_finding_standings_finding",
        "change_review_finding_standings_chain",
        "change_review_finding_standings_no_replace",
        "change_review_finding_standings_no_update",
        "change_review_finding_standings_no_delete",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    expected.extend(objects_of_parallel_lanes());
    // Tasks that landed after T-4-2, each with its own registry file and
    // migration test (T-4-4's `change_attempt_budgets`, …).
    expected.extend(migration_registry::objects_added_after("T-4-2"));
    expected.sort();
    assert_eq!(added, expected);
}
