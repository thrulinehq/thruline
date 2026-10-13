//! C30 R-LEDGER, at the process boundary (C34 rulings Q14 and Q17).
//!
//! * Q14 — `thruline upgrade` counts the rows of every table before and after
//!   it migrates, and says "rows before = rows after for every existing table
//!   · new tables N, all empty" when that is what it found. When it finds
//!   anything else it names the table and both counts, and a table that holds
//!   FEWER rows afterwards makes the command exit 1.
//! * Q17 — `thruline verify` re-folds the `capture_hold` chain beside the
//!   observation and permission chains, prints its line, and exits 1 when a
//!   row has been tampered with.
//!
//! Every store here is a scratch store in a temporary folder; none is the
//! user's.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::tempdir;
use thruline_core::capture_hold::{self, Act, Hold, Recorded};
use thruline_core::ledger::Ledger;

/// The DDL digest of the commit before the R-LEDGER tables (`74b06079`), as
/// `crates/core/tests/r_ledger_migration.rs` pins it.
const SCHEMA_BEFORE: &str =
    "schema_sha256_c4a93038ac02f0152719db97c72c73d394677c7fe6596d3e3739fe54c0b2ab22";

const TABLES: [&str; 6] = [
    "change_notes",
    "statement_objections",
    "change_decision_seconds",
    "change_agent_messages",
    "capture_holds",
    "inbox_hides",
];

fn run(db: &Path, args: &[&str]) -> Output {
    let mut all = vec!["--db", db.to_str().expect("db path")];
    all.extend_from_slice(args);
    Command::new(env!("CARGO_BIN_EXE_thruline"))
        .args(&all)
        .output()
        .expect("run thruline")
}

/// A store as the base commit would leave it: everything this binary creates,
/// less the thirty R-LEDGER objects, stamped with the base commit's own
/// fingerprint, with something in it.
fn a_store_from_before_the_tables(dir: &Path) -> PathBuf {
    let db = dir.join("ledger.db");
    let made = run(&db, &["projects"]);
    assert!(made.status.success(), "{made:?}");
    let conn = rusqlite::Connection::open(&db).expect("open");
    for table in TABLES {
        // A dropped table takes its index and its triggers with it.
        conn.execute_batch(&format!("DROP TABLE {table};"))
            .expect("drop table");
    }
    let named = TABLES.map(|t| format!("'{t}'")).join(",");
    let left: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM sqlite_master WHERE tbl_name IN ({named})"),
            [],
            |r| r.get(0),
        )
        .expect("object census");
    assert_eq!(left, 0, "dropping a table drops its index and triggers");
    conn.execute_batch(&format!(
        "DELETE FROM schema_migrations WHERE version='{}';
         INSERT INTO schema_migrations(version) VALUES('{SCHEMA_BEFORE}');
         INSERT INTO projects(id, root_path, source, display_name, created_at)
           VALUES('cccccccc-0000-4000-8000-00000000p160', '/tmp/synthetic-project',
                  'claude_code', 'Synthetic', '2026-01-01T00:00:00Z');
         -- The source-identity mapping every earlier upgrade backfilled, so the
         -- store is what the base commit leaves.
         INSERT INTO source_project_identities(project_id, owner_key, source,
                                               source_project_key, normalized_root_hint)
           VALUES('cccccccc-0000-4000-8000-00000000p160', 'legacy-local', 'claude_code',
                  'cccccccc-0000-4000-8000-00000000p160', '/tmp/synthetic-project');",
        Ledger::schema_fingerprint()
    ))
    .expect("restamp as the base commit");
    drop(conn);
    db
}

fn rows_of(db: &Path, table: &str) -> i64 {
    let conn = rusqlite::Connection::open(db).expect("open");
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .expect("count")
}

#[test]
fn upgrade_counts_every_table_and_finds_the_six_new_ones_empty() {
    let dir = tempdir().expect("tempdir");
    let db = a_store_from_before_the_tables(dir.path());
    let stamps_before = rows_of(&db, "schema_migrations");

    let refused = run(&db, &["projects"]);
    assert_eq!(
        refused.status.code(),
        Some(3),
        "a store without the six tables is pending, and an ordinary verb pays nothing"
    );

    let out = run(&db, &["upgrade"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{out:?}");
    assert!(
        text.contains("counting    every table before the upgrade ... "),
        "the count is taken, and announced, before anything is paid: {text}"
    );
    assert!(
        text.contains("Row census, every table counted before and after:"),
        "{text}"
    );
    assert!(
        text.contains("  rows before = rows after for every existing table ("),
        "the census line: {text}"
    );
    assert!(
        text.contains(") · new tables 6, all empty"),
        "the six new tables are named by count and found empty: {text}"
    );
    assert!(
        text.contains(&format!(
            "schema_migrations is the upgrade's own stamp: {stamps_before} before, {} after",
            stamps_before + 1
        )),
        "the one table an upgrade writes to is named, with both counts: {text}"
    );
    // The count comes before the first step; the census after the last step and
    // before the summary.
    let counted = text.find("counting    every table").expect("count line");
    let first_step = text.find("[ 1/").expect("first step");
    let last_step = text
        .find("stamping the schema fingerprint")
        .expect("last step");
    let census = text.find("Row census").expect("census");
    let done = text.find("Done in").expect("summary");
    assert!(
        counted < first_step && last_step < census && census < done,
        "{text}"
    );

    for table in TABLES {
        assert_eq!(rows_of(&db, table), 0, "{table}");
    }

    // A store already current is not counted: there is nothing to compare.
    let again = run(&db, &["upgrade"]);
    let again_text = String::from_utf8_lossy(&again.stdout);
    assert!(again_text.contains("already up to date"), "{again_text}");
    assert!(!again_text.contains("Row census"), "{again_text}");
}

#[test]
fn the_dry_run_counts_nothing_and_changes_nothing() {
    let dir = tempdir().expect("tempdir");
    let db = a_store_from_before_the_tables(dir.path());
    let dry = run(&db, &["upgrade", "--dry-run"]);
    let text = String::from_utf8_lossy(&dry.stdout);
    assert!(dry.status.success(), "{dry:?}");
    assert!(
        text.contains(
            "rows        every table is counted before and after, and the two counts compared"
        ),
        "the dry run says the census is part of the upgrade: {text}"
    );
    assert!(!text.contains("counting    every table"), "{text}");
    assert!(!text.contains("Row census"), "{text}");
    assert!(text.contains("--dry-run: nothing was changed."), "{text}");
    let conn = rusqlite::Connection::open(&db).expect("open");
    let made: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='capture_holds'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(made, 0, "--dry-run performed the upgrade it described");
}

/// The census is loud when rows are missing afterwards. The one way the schema
/// path legitimately removes rows is the derived `nodes`/`edges` pair, dropped
/// and re-derived when their stored shape predates the dimensional domain; a
/// scratch store with a stale `nodes` table and one row in it makes the
/// upgrade do exactly that, and the census must say so and exit 1.
#[test]
fn a_table_that_holds_fewer_rows_afterwards_is_named_and_the_exit_is_1() {
    let dir = tempdir().expect("tempdir");
    let db = dir.path().join("ledger.db");
    let made = run(&db, &["projects"]);
    assert!(made.status.success(), "{made:?}");
    {
        let conn = rusqlite::Connection::open(&db).expect("open");
        let sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='nodes'",
                [],
                |r| r.get(0),
            )
            .expect("nodes DDL");
        let stale = sql.replace("sort_key TEXT", "sort_key  TEXT");
        assert_ne!(
            stale, sql,
            "the stale shape must differ from the current one"
        );
        // The derived row's parents, so the schema path's foreign-key check
        // (which runs before the derived refresh) finds nothing wrong.
        conn.execute_batch(
            "INSERT INTO projects(id,root_path,source,display_name,created_at)
               VALUES('p-old','/old/project','claude_code','Old','2026-01-01T00:00:00Z');
             INSERT INTO source_project_identities(project_id,owner_key,source,
                                                   source_project_key,normalized_root_hint)
               VALUES('p-old','legacy-local','claude_code','p-old','/old/project');
             INSERT INTO sessions(id,project_id,file_path,started_at,last_seq,raw_line_count,
                                  event_count)
               VALUES('s-old','p-old','/old/project/s-old.jsonl','2026-01-01T00:00:00Z',0,1,1);
             INSERT INTO source_sessions(session_id,project_id,owner_key,source,
                                         source_session_key,source_locator)
               VALUES('s-old','p-old','legacy-local','claude_code','s-old',
                      '/old/project/s-old.jsonl');
             INSERT INTO events(id,project_id,session_id,seq,ts,kind,role,text,raw_file,raw_line,
                                raw_sha256,prev_hash,chain_hash)
               VALUES('e-old','p-old','s-old',0,'2026-01-01T00:00:00Z','user_msg','user','x',
                      '/old/project/s-old.jsonl',1,
                      'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                      'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                      'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc');",
        )
        .expect("parents");
        conn.execute_batch("DROP TABLE edges; DROP TABLE nodes;")
            .expect("drop derived");
        conn.execute_batch(&stale).expect("stale nodes");
        conn.execute(
            "INSERT INTO nodes(id, project_id, label, sort_key, lifecycle, evidence_state,
                               currency, confidence, session_id, anchor_event_id, derived_rev)
             VALUES('n1','p-old','a derived node','recorded','proposed','unknown','current',
                    NULL,'s-old','e-old',1)",
            [],
        )
        .expect("a derived row");
        conn.execute("DELETE FROM schema_migrations", [])
            .expect("make the store pending");
    }

    let out = run(&db, &["upgrade"]);
    let text = String::from_utf8_lossy(&out.stdout);
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{text}\n{err}");
    assert!(
        text.contains("  FEWER ROWS  nodes: 1 before, 0 after"),
        "the table and both counts are named: {text}"
    );
    assert!(
        text.contains("  rows differ for 1 of "),
        "the headline does not claim rows before = rows after: {text}"
    );
    assert!(
        !text.contains("rows before = rows after"),
        "the equal-rows sentence must not be printed over a loss: {text}"
    );
    assert!(text.contains("Done in"), "the schema is upgraded: {text}");
    assert!(err.contains("fewer rows"), "{err}");
    // The upgrade itself did finish: ordinary commands work again.
    assert!(run(&db, &["projects"]).status.success());
}

fn hold(folder: &str) -> Hold<'_> {
    Hold {
        project_id: "hq-project",
        project_folder: folder,
        actor: "sam",
        recorded_at: "2026-10-07T12:00:00Z",
    }
}

fn appended(recorded: Recorded) -> capture_hold::HoldRow {
    match recorded {
        Recorded::Appended(row) => *row,
        Recorded::Refused(why) => panic!("expected an appended row, got {why:?}"),
    }
}

#[test]
fn verify_re_folds_the_capture_hold_chain_and_a_tampered_row_fails_it() {
    let dir = tempdir().expect("tempdir");
    let db = dir.path().join("ledger.db");
    drop(Ledger::open(&db).expect("create"));

    // An empty chain adds up vacuously, and the line is printed in the column
    // the observation and permission lines use.
    let empty = run(&db, &["verify"]);
    let text = String::from_utf8_lossy(&empty.stdout);
    assert_eq!(empty.status.code(), Some(0), "{text}");
    assert!(
        text.contains("capture holds  chain re-checked, 0 links add up"),
        "{text}"
    );
    let lines: Vec<&str> = text.lines().collect();
    let at = |prefix: &str| {
        lines
            .iter()
            .position(|l| l.starts_with(prefix))
            .unwrap_or_else(|| panic!("no {prefix:?} line: {text}"))
    };
    assert_eq!(
        at("observations   ") + 1,
        at("permissions    "),
        "the permission line follows the observation line: {text}"
    );
    assert_eq!(
        at("permissions    ") + 1,
        at("capture holds  "),
        "the capture-hold line follows the permission line: {text}"
    );

    {
        let ledger = Ledger::open(&db).expect("open");
        appended(
            capture_hold::record(&ledger, Act::Paused, &hold("/work/app"), None).expect("pause"),
        );
        appended(
            capture_hold::record(&ledger, Act::Excluded, &hold("/work/app"), Some("env"))
                .expect("exclude"),
        );
    }
    let intact = run(&db, &["verify"]);
    let text = String::from_utf8_lossy(&intact.stdout);
    assert_eq!(intact.status.code(), Some(0), "{text}");
    assert!(
        text.contains("capture holds  chain re-checked, 2 links add up"),
        "{text}"
    );

    // Tampered, in a scratch store with the trigger dropped: verify exits 1
    // and names the chain.
    {
        let conn = rusqlite::Connection::open(&db).expect("open");
        conn.execute_batch(
            "DROP TRIGGER capture_holds_no_update;
             UPDATE capture_holds SET actor='mallory' WHERE seq=0;",
        )
        .expect("forge");
    }
    let broken = run(&db, &["verify"]);
    let text = String::from_utf8_lossy(&broken.stdout);
    assert_eq!(broken.status.code(), Some(1), "{text}");
    assert!(
        text.contains("capture holds  CHAIN DOES NOT ADD UP"),
        "the failure is named: {text}"
    );
}
