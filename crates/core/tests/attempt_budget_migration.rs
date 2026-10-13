//! T-4-4-ATTEMPT-BUDGET (R-2026-09-28-43; founder ruling #22 (ii) a) — the
//! attempt-budget table arrives as a migration and costs nothing until
//! `thruline upgrade` is run deliberately (A168/A193).
//!
//! **This task changes DDL; `thruline upgrade` is required; every other verb
//! exits 3 until it runs.** Asked against the schema at `55b80e99` (the
//! integration this card was built on), whose DDL is
//! `tests/fixtures/schema-t-4-4-base.sql`. That DDL is unchanged since
//! `6008e9af`, so the digest below is the one QUAL-F6's unmerged branch pinned.

use rusqlite::{params, Connection};
use thruline_core::attempt_budget;
use thruline_core::error::CoreError;
use thruline_core::ledger::Ledger;
use thruline_core::upgrade::UpgradeState;

/// The DDL digest at `55b80e99`. Pinned so the bump is a fact, not a belief.
const SCHEMA_BEFORE: &str =
    "schema_sha256_653491075873d14e19f6859d7ec2b1d13381977a16393803cf1da1f40b6ab67e";

const THIS_TASK: &str = "T-4-4";

mod migration_registry;

fn fixture_schema() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/schema-t-4-4-base.sql"),
    )
    .expect("the pre-T-4-4 schema fixture must be readable")
}

#[test]
fn the_fixture_is_really_the_schema_at_the_base_commit() {
    let digest = format!(
        "schema_sha256_{}",
        thruline_core::chain::sha256_hex(fixture_schema().as_bytes())
    );
    assert_eq!(digest, SCHEMA_BEFORE);
    let me = migration_registry::entries()
        .into_iter()
        .find(|e| e.task == THIS_TASK)
        .expect("the registry must name this task");
    assert_eq!(
        me.pinned_schema_digest, SCHEMA_BEFORE,
        "the registry and this test must pin the same pre-task schema"
    );
}

#[test]
fn the_schema_version_moved_when_the_attempt_budget_table_landed() {
    assert_ne!(Ledger::schema_fingerprint(), SCHEMA_BEFORE);
}

const CARRIED: [&str; 3] = ["projects", "changes", "change_run_executions"];

/// A store exactly as the base commit would leave it: its whole DDL, its
/// fingerprint, and a Change with a finished run.
fn base_commit_store(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("ledger.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(&fixture_schema()).unwrap();
    conn.execute_batch(
        "INSERT INTO projects(id,root_path,source,display_name,created_at)
           VALUES('p-old','/old/project','claude_code','Old','2026-01-01T00:00:00Z');
         INSERT INTO changes(id,project_id,opened_at,title,purpose,requester,request_source)
           VALUES('chg-old','p-old','2026-01-01T00:00:00Z','Old','Kept','sam','person');
         INSERT INTO change_run_executions(id,run_key,change_id,name,state,pid,process_group,
             started_at,examined_inputs_id,recorded_at)
           VALUES('rx_old','run-old','chg-old','cargo test','running',1,1,
                  '2026-01-01T00:01:00Z','exi_old','2026-01-01T00:01:00Z');
         INSERT INTO change_run_executions(id,run_key,change_id,name,state,pid,process_group,
             started_at,examined_inputs_id,cleanup_verified,survivors,recorded_at)
           VALUES('rx_old2','run-old','chg-old','cargo test','completed',1,1,
                  '2026-01-01T00:01:00Z','exi_old',1,0,'2026-01-01T00:02:00Z');",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO schema_migrations(version) VALUES(?1)",
        params![SCHEMA_BEFORE],
    )
    .unwrap();
    path
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

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn an_old_store_is_refused_before_the_upgrade_and_keeps_every_row_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = base_commit_store(dir.path());
    let before = {
        let conn = Connection::open(&path).unwrap();
        assert_eq!(
            count(&conn, "SELECT COUNT(*) FROM change_run_executions"),
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
            &format!(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='{}'",
                attempt_budget::TABLE
            )
        ),
        0,
        "the refusal paid nothing"
    );
    drop(conn);

    drop(Ledger::open_upgrading(&path, &mut |_, _, _| {}).unwrap());
    assert_eq!(Ledger::upgrade_state(&path).unwrap(), UpgradeState::Current);

    let conn = Connection::open(&path).unwrap();
    assert_eq!(
        count(
            &conn,
            &format!("SELECT COUNT(*) FROM {}", attempt_budget::TABLE)
        ),
        0,
        "a migration must invent no budget"
    );
    assert_eq!(dump(&conn), before, "every earlier row is preserved");
    drop(conn);

    // After the upgrade: no budget stands for the old Change, and a budget
    // can be named against the run it already had.
    let reopened = Ledger::open(&path).unwrap();
    assert!(attempt_budget::status(&reopened, "chg-old")
        .unwrap()
        .is_none());
    let budget = attempt_budget::set(&reopened, "chg-old", 2, "sam", "2026-01-02T00:00:00Z")
        .unwrap()
        .expect("a budget against the old run");
    assert_eq!(budget.after_execution.as_deref(), Some("rx_old2"));
    assert_eq!(budget.set_on_version.as_deref(), Some("exi_old"));
    // The version the old run examined is not a new attempt.
    assert!(attempt_budget::admits(&reopened, "chg-old", "exi_old")
        .unwrap()
        .is_none());
    drop(reopened);
    drop(Ledger::open_upgrading(&path, &mut |_, _, _| {}).expect("a second upgrade is a no-op"));
}

/// The migration adds one table, one index and three triggers, and alters
/// none, proven against the base commit's schema rather than a memory of it.
#[test]
fn the_migration_adds_one_table_and_alters_no_existing_one() {
    let old = Connection::open_in_memory().unwrap();
    old.execute_batch(&fixture_schema()).unwrap();
    let new = Connection::open_in_memory().unwrap();
    new.execute_batch(Ledger::schema_ddl()).unwrap();
    let shape = |conn: &Connection| -> Vec<(String, String)> {
        let mut s = conn
            .prepare(
                "SELECT name, COALESCE(sql,'') FROM sqlite_master
                  WHERE type IN ('table','trigger','index')
                    AND name NOT LIKE 'sqlite_autoindex_%' ORDER BY name",
            )
            .unwrap();
        s.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    };
    let (old_shape, new_shape) = (shape(&old), shape(&new));
    assert!(!old_shape.is_empty(), "the schema fixture is empty");
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
        attempt_budget::TABLE,
        "idx_change_attempt_budgets_change",
        "change_attempt_budgets_no_replace",
        "change_attempt_budgets_no_update",
        "change_attempt_budgets_no_delete",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    expected.extend(migration_registry::objects_added_after(THIS_TASK));
    expected.sort();
    assert_eq!(added, expected);

    // And the registry says the same thing this tree creates.
    let me = migration_registry::entries()
        .into_iter()
        .find(|e| e.task == THIS_TASK)
        .expect("the registry must name this task");
    let mut claimed: Vec<String> = me
        .tables
        .into_iter()
        .chain(me.indexes)
        .chain(me.triggers)
        .collect();
    claimed.sort();
    let mut mine: Vec<String> = expected
        .into_iter()
        .filter(|n| !migration_registry::objects_added_after(THIS_TASK).contains(n))
        .collect();
    mine.sort();
    assert_eq!(claimed, mine);
}

/// A193: the cost is stated, not guessed. The registry note quotes
/// `upgrade::estimate_phrase` for the 2.01 M-event corpus; this pins that the
/// quote is what the function says.
#[test]
fn the_registry_states_the_upgrade_cost_estimate_phrase_gives() {
    let me = migration_registry::entries()
        .into_iter()
        .find(|e| e.task == THIS_TASK)
        .expect("the registry must name this task");
    let phrase = thruline_core::upgrade::estimate_phrase(2_010_000);
    let note = std::fs::read_to_string(migration_registry::registry_dir().join("140-t-4-4.toml"))
        .expect("the registry file is readable");
    assert!(
        note.contains(&format!("`upgrade::estimate_phrase` gives \"{phrase}\"")),
        "the note must quote {phrase:?}; task {}",
        me.task
    );
}
