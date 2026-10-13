//! T-2-2 — the association table's amended shape, and the refusal that stops
//! a half-migrated store from serving wrong answers quietly.
//!
//! **This task changes DDL; `thruline upgrade` is required; every other verb
//! exits 3 until it runs.** T-2-1's `change_migration.rs` proves that sentence
//! for the whole Change layer and it still holds; this file is only about the
//! one table T-2-2 amends.
//!
//! What was amended, and why there is no ALTER for it: `provenance` gained
//! `rejected` and the table gained `reason`. The table has never shipped —
//! `5cab107d…` has no Change tables at all and T-2-1 is uncommitted — so no
//! database outside a developer's tree carries the earlier shape. Rather than
//! write a data-rewriting migration for a case that cannot exist, the ledger
//! **refuses to open** such a store and names the table and the missing word.
//! Failing loud beats migrating an imaginary population.

use rusqlite::Connection;
use thruline_core::ledger::Ledger;

/// Ledgers created by this binary carry the amended table.
#[test]
fn a_fresh_store_carries_the_amended_association_table() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    let sql: String = ledger
        .conn()
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table'
              AND name='change_session_associations'",
            [],
            |row| row.get(0),
        )
        .expect("the table exists");
    assert!(sql.contains("'rejected'"), "{sql}");
    assert!(sql.contains("reason"), "{sql}");
}

/// A rejection is held to the confirmation's bar: a named person, on a `user`
/// basis. The database says so, not a caller.
#[test]
fn a_rejection_needs_a_person_exactly_as_a_confirmation_does() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    seed(&ledger);
    for (basis, actor) in [
        ("user", None),
        ("timing", Some("sam")),
        ("filename", Some("sam")),
        ("agent-declared", Some("sam")),
        ("user", Some("   ")),
    ] {
        let refused = ledger.conn().execute(
            "INSERT INTO change_session_associations(id, change_id, session_id, provenance,
                 basis, actor, asserted_at, reason)
             VALUES('a1','c1','s1','rejected',?1,?2,'2026-01-03T00:00:00Z','no')",
            rusqlite::params![basis, actor],
        );
        assert!(
            refused.is_err(),
            "a rejection with basis {basis} and actor {actor:?} must be refused"
        );
    }
    ledger
        .conn()
        .execute(
            "INSERT INTO change_session_associations(id, change_id, session_id, provenance,
                 basis, actor, asserted_at, reason)
             VALUES('a1','c1','s1','rejected','user','sam','2026-01-03T00:00:00Z','no')",
            [],
        )
        .expect("a named person on a user basis may reject");
}

/// The three words are the whole vocabulary. A fourth is refused at INSERT.
#[test]
fn provenance_is_exactly_three_words() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    seed(&ledger);
    for word in ["maybe", "probably", "auto-confirmed", ""] {
        assert!(
            ledger
                .conn()
                .execute(
                    "INSERT INTO change_session_associations(id, change_id, session_id,
                         provenance, basis, actor, asserted_at, reason)
                     VALUES('a-'||?1,'c1','s1',?1,'user','sam','2026-01-03T00:00:00Z','x')",
                    rusqlite::params![word],
                )
                .is_err(),
            "{word:?} is not a provenance"
        );
    }
}

/// A store carrying the pre-T-2-2 table is refused, by name, rather than
/// opened and silently unable to record a rejection.
#[test]
fn a_store_with_the_earlier_table_shape_is_refused_by_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("ledger.db");
    drop(Ledger::open(&path).expect("create"));

    // Replace the table with the shape T-2-1 wrote, and clear the fingerprint
    // so the full schema path runs — exactly what a developer tree built from
    // the intermediate working copy would look like.
    let conn = Connection::open(&path).expect("reopen");
    conn.execute_batch(
        "PRAGMA writable_schema=ON;
         DROP TABLE change_session_associations;
         CREATE TABLE change_session_associations(
           id TEXT PRIMARY KEY,
           change_id TEXT NOT NULL REFERENCES changes(id),
           session_id TEXT NOT NULL REFERENCES sessions(id),
           provenance TEXT NOT NULL CHECK(provenance IN ('suggested','confirmed')),
           basis TEXT NOT NULL CHECK(basis IN ('user','timing','filename','agent-declared')),
           actor TEXT,
           asserted_at TEXT NOT NULL);
         PRAGMA writable_schema=OFF;
         DELETE FROM schema_migrations;",
    )
    .expect("install the earlier shape");
    drop(conn);

    let err = match Ledger::open_upgrading(&path, &mut |_, _, _| {}) {
        Ok(_) => panic!("a store with the earlier table shape must be refused"),
        Err(e) => e.to_string(),
    };
    assert!(
        err.contains("change_session_associations"),
        "the refusal must name the table: {err}"
    );
    assert!(
        err.contains("'rejected'"),
        "the refusal must name what is missing: {err}"
    );
}

fn seed(ledger: &Ledger) {
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES('p1','/tmp/synthetic','claude_code','Synthetic','2026-01-01T00:00:00Z')",
            [],
        )
        .expect("project");
    ledger
        .conn()
        .execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES('s1','p1','/tmp/synthetic/s.jsonl','2026-01-01T00:00:00Z')",
            [],
        )
        .expect("session");
    ledger
        .conn()
        .execute(
            "INSERT INTO changes(id, project_id, opened_at, title, purpose, requester,
                 request_source)
             VALUES('c1','p1','2026-01-02T09:00:00Z','T','P','sam','person')",
            [],
        )
        .expect("change");
}
