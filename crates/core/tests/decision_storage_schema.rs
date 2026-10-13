//! C24: the standing-decision ledger is append-only at the database boundary.

use rusqlite::Connection;
use thruline_core::ledger::Ledger;

const TABLES: [&str; 6] = [
    "decisions",
    "decision_revisions",
    "decision_acceptances",
    "decision_retirements",
    "decision_verdicts",
    "decision_waivers",
];

fn tables(conn: &Connection) -> Vec<String> {
    let mut statement = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .expect("prepare table census");
    statement
        .query_map([], |row| row.get(0))
        .expect("query table census")
        .map(|row| row.expect("table name"))
        .collect()
}

#[test]
fn the_six_decision_tables_exist_in_a_fresh_ledger() {
    let ledger = Ledger::open_in_memory().expect("fresh ledger");
    let present = tables(ledger.conn());
    for table in TABLES {
        assert!(present.contains(&table.to_string()), "missing {table}");
    }
}

#[test]
fn every_decision_table_refuses_update_delete_and_replace() {
    let ledger = Ledger::open_in_memory().expect("fresh ledger");
    let mut statement = ledger
        .conn()
        .prepare("SELECT name FROM sqlite_master WHERE type='trigger' ORDER BY name")
        .expect("prepare trigger census");
    let triggers: Vec<String> = statement
        .query_map([], |row| row.get(0))
        .expect("query trigger census")
        .map(|row| row.expect("trigger name"))
        .collect();
    for table in TABLES {
        for suffix in ["no_replace", "no_update", "no_delete"] {
            let name = format!("{table}_{suffix}");
            assert!(triggers.contains(&name), "missing {name}");
        }
    }
}
