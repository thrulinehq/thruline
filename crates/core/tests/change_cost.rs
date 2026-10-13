//! T-2-1 / `L1.1` / Rule 13 — what the store grows by, measured rather than
//! estimated.
//!
//! `docs/REUSE-MAP.md` §4.2 estimated the Change layer at ~400–600 B per
//! association and ~1.5–5 KB per Change from the per-row cost of comparable
//! existing rows. Nothing Change-shaped had ever been written when that was
//! written, so it was arithmetic. This test writes them and measures.
//!
//! The measurement is `PRAGMA page_count × PRAGMA page_size` on a fixture store
//! before and after N inserts, with the store checkpointed and vacuumed so the
//! WAL and free pages cannot flatter the number. It **fails if the cost leaves
//! the band**, in either direction: a regression that doubled the row cost and
//! an edit that made the number meaningless are both things a person should be
//! told about, and `L1.1` says the tool states what it stores in a real number.
//!
//! The printed numbers are what goes in the report and in the migration's doc
//! comment. Run with `--nocapture` to see them.

use rusqlite::params;
use thruline_core::change::{
    self, AcceptedScope, AssociationBasis, AssociationProvenance, ChangeRequest, NewAssociation,
    NewChange, RequestSource, ScopeAcceptance,
};
use thruline_core::ledger::Ledger;

/// Enough rows that one page of slack is noise rather than the answer.
const N: usize = 1_000;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000p001";
const SESSION: &str = "cccccccc-0000-4000-8000-00000000s001";

fn bytes_on_disk(ledger: &Ledger) -> u64 {
    // Fold the WAL back in and drop free pages, so the reading is the store's
    // settled size and not a transient one.
    let _ = ledger
        .conn()
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
    let _ = ledger.conn().execute_batch("VACUUM");
    let pages: i64 = ledger
        .conn()
        .query_row("PRAGMA page_count", [], |r| r.get(0))
        .unwrap_or(0);
    let size: i64 = ledger
        .conn()
        .query_row("PRAGMA page_size", [], |r| r.get(0))
        .unwrap_or(0);
    (pages.max(0) as u64) * (size.max(0) as u64)
}

/// The request prose is the measured mean `user_msg` length in the live
/// personal ledger (914 bytes, `docs/REUSE-MAP.md` §4.1), so the per-Change
/// number is not flattered by a one-line purpose.
fn purpose_of_measured_length() -> String {
    let sentence = "The upload retries once and then gives up without telling anyone. ";
    let mut text = String::new();
    while text.len() < 914 {
        text.push_str(sentence);
    }
    text.truncate(914);
    text
}

#[test]
fn the_store_grows_by_a_measured_amount_per_change_and_per_association() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES(?1, '/tmp/synthetic-project', 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
            params![PROJECT],
        )
        .expect("project");
    ledger
        .conn()
        .execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES(?1, ?2, '/tmp/synthetic-project/s.jsonl', '2026-01-01T00:00:00Z')",
            params![SESSION, PROJECT],
        )
        .expect("session");

    let empty = bytes_on_disk(&ledger);

    let purpose = purpose_of_measured_length();
    for i in 0..N {
        let id = format!("cccccccc-0000-4000-8000-{i:012}");
        change::insert_change(
            &ledger,
            &NewChange {
                id: id.clone(),
                project_id: PROJECT.to_string(),
                opened_at: "2026-01-02T09:00:00Z".to_string(),
                request: ChangeRequest {
                    title: format!("Retry failed uploads ({i})"),
                    purpose: purpose.clone(),
                    prompt_event_id: None,
                    requester: "sam".to_string(),
                    source: RequestSource::Person,
                },
            },
        )
        .expect("insert a change");
        let version = change::append_scope(
            &ledger,
            &id,
            "2026-01-02T09:05:00Z",
            &AcceptedScope {
                criteria: (0..5)
                    .map(|c| format!("Criterion {c}: the retry happens and is visible."))
                    .collect(),
                exclusions: vec!["Uploads over 2 GB.".to_string()],
                allowed_scope: vec!["src/upload/**".to_string()],
                constraints: vec!["No new dependency.".to_string()],
                check_profile: vec!["unit-tests".to_string(), "lint".to_string()],
            },
        )
        .expect("scope");
        change::accept_scope(
            &ledger,
            &id,
            version,
            &ScopeAcceptance {
                actor: "sam".to_string(),
                accepted_at: "2026-01-02T09:06:00Z".to_string(),
                rationale: "That is what I asked for.".to_string(),
            },
        )
        .expect("acceptance");
    }

    let with_changes = bytes_on_disk(&ledger);

    for i in 0..N {
        change::append_association(
            &ledger,
            &NewAssociation {
                id: format!("cccccccc-0000-4000-8000-a{i:011}"),
                change_id: format!("cccccccc-0000-4000-8000-{i:012}"),
                session_id: SESSION.to_string(),
                provenance: AssociationProvenance::Suggested,
                basis: AssociationBasis::Timing,
                actor: None,
                asserted_at: "2026-01-02T10:01:00Z".to_string(),
                reason: None,
            },
        )
        .expect("association");
    }

    let with_associations = bytes_on_disk(&ledger);

    let per_change = (with_changes - empty) as f64 / N as f64;
    let per_association = (with_associations - with_changes) as f64 / N as f64;

    println!("T-2-1 cost, measured on a fixture store, N = {N}");
    println!("  empty store:                {empty} B");
    println!("  after {N} Changes:        {with_changes} B");
    println!("  after {N} associations:   {with_associations} B");
    println!("  bytes per Change (request prose 914 B, 5 criteria, 1 acceptance): {per_change:.0}");
    println!("  bytes per association:      {per_association:.0}");

    // A Change carries its request prose, so its floor is that prose. The
    // ceiling is four times it: past that, the schema is costing more than the
    // text, which is the thing worth being told about.
    assert!(
        (914.0..4_000.0).contains(&per_change),
        "bytes per Change moved out of the measured band: {per_change:.0}"
    );
    // An association is two ids, a word, a basis and a time. The reuse map
    // estimated 400–600 B for a proposal-plus-decision pair; one append-only
    // row must not cost more than that pair was estimated at.
    assert!(
        (64.0..600.0).contains(&per_association),
        "bytes per association moved out of the measured band: {per_association:.0}"
    );
}
