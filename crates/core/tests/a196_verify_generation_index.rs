//! A196 — verify's per-generation accounting answers from indexes, never a
//! table scan.
//!
//! The diagnosis this pins: `ingest::verify` runs COUNT queries filtered by
//! `generation_id` against `event_generation_provenance`, whose only other
//! index is the `event_id` primary key — so each COUNT was a full scan of the
//! table (1.5 M rows / 225 MB on the live corpus), issued twice per
//! generation. At 20,354 generations that is ~40,700 scans and ~9 TB of page
//! reads: whole-ledger verify could not finish, and B-A195's ">900 s" was a
//! ceiling, not a duration. Scoped verify of a 4-generation project went
//! 71.9 s -> 57 ms once the index existed. Same disease and same fix shape as
//! A191's `idx_nodes_project`.
//!
//! The SQL below mirrors the queries in `ingest::verify` the way the T4 plan
//! test mirrors its scan (A169 lesson): plan and index are held together, so
//! if either drifts back to a scan this fails rather than verify quietly
//! becoming unbounded again.

use thruline_core::ledger::Ledger;

/// The generation-filtered accounting queries `verify` issues, and the index
/// each must answer from.
const ACCOUNTING_QUERIES: &[(&str, &str)] = &[
    (
        "SELECT COUNT(*) FROM event_generation_provenance WHERE generation_id='g'",
        "idx_generation_provenance_generation",
    ),
    (
        "SELECT COUNT(*) FROM event_generation_provenance p \
         JOIN events e ON e.id=p.event_id \
         WHERE p.generation_id='g' AND e.source_message_key IS NOT NULL",
        "idx_generation_provenance_generation",
    ),
    (
        "SELECT COUNT(*) FROM codex_message_slots WHERE generation_id='g'",
        "idx_codex_message_slots_generation",
    ),
    (
        "SELECT COUNT(*) FROM codex_reconciliation_receipts WHERE generation_id='g'",
        "idx_codex_reconciliation_generation",
    ),
];

#[test]
fn the_per_generation_accounting_answers_from_its_indexes() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    for (sql, index) in ACCOUNTING_QUERIES {
        let plan_sql = format!("EXPLAIN QUERY PLAN {sql}");
        let mut stmt = ledger.conn().prepare(&plan_sql).expect("prepare");
        let plan: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(3))
            .expect("plan rows")
            .collect::<std::result::Result<_, _>>()
            .expect("plan text");
        assert!(
            plan.iter().any(|p| p.contains(index)),
            "verify accounting query no longer answers from {index} — \
             plan: {plan:?}\nquery: {sql}"
        );
    }
}
