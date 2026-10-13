//! P6-5B — permission for checks to run in a project is a row in the record,
//! append-only and hash-chained (guard 5B-3), and a revocation is its own row
//! pointing at the grant it takes back.
//!
//! Synthetic ids only (`cccccccc-` prefix); every store is a temporary ledger.

use rusqlite::{params, Connection};
use thruline_core::chain::{chain_hash, genesis_prev_hash};
use thruline_core::ledger::Ledger;
use thruline_core::run_permission::{self as rp, Act, Consent, Standing};

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000p5b1";
const CHANGE: &str = "cccccccc-0000-4000-8000-00000000c5b1";
const FOLDER: &str = "/synthetic/shop";

fn store() -> (tempfile::TempDir, Ledger) {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open a fresh ledger");
    let conn = ledger.conn();
    conn.execute(
        "INSERT INTO projects(id, root_path, source, display_name, created_at)
         VALUES(?1, '/synthetic/shop', 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
        params![PROJECT],
    )
    .expect("insert project");
    conn.execute(
        "INSERT INTO changes(id, project_id, opened_at, title, purpose, request_event_id,
                             requester, request_source)
         VALUES(?1, ?2, '2026-09-24T08:00:00Z', 'Cart total', 'Add it.', NULL, 'sam', 'person')",
        params![CHANGE, PROJECT],
    )
    .expect("insert change");
    (dir, ledger)
}

fn act<'a>(folder: &'a str, at: &'a str) -> Consent<'a> {
    Consent {
        project_id: PROJECT,
        project_folder: folder,
        actor: "sam",
        recorded_at: at,
    }
}

#[test]
fn a_grant_and_its_revocation_are_two_rows_and_the_revocation_names_the_grant() {
    let (_dir, ledger) = store();
    assert_eq!(
        rp::standing(&ledger, FOLDER).expect("standing"),
        Standing::Never
    );

    let grant = rp::grant(&ledger, &act(FOLDER, "2026-09-24T09:00:00Z")).expect("grant");
    assert_eq!(grant.act, Act::Granted);
    assert_eq!(grant.actor, "sam");
    assert_eq!(grant.project_id, PROJECT);
    assert_eq!(
        rp::standing(&ledger, FOLDER).expect("standing"),
        Standing::Granted(grant.clone())
    );

    let revocation = rp::revoke(&ledger, &act(FOLDER, "2026-09-24T09:05:00Z"))
        .expect("revoke")
        .expect("a standing grant to take back");
    assert_eq!(revocation.act, Act::Revoked);
    assert_eq!(revocation.revokes.as_deref(), Some(grant.id.as_str()));
    assert_eq!(
        rp::standing(&ledger, FOLDER).expect("standing"),
        Standing::Revoked(revocation.clone())
    );

    // Taking back what is already taken back appends nothing.
    assert!(rp::revoke(&ledger, &act(FOLDER, "2026-09-24T09:06:00Z"))
        .expect("revoke")
        .is_none());
    let history = rp::history(&ledger, FOLDER).expect("history");
    assert_eq!(history, vec![grant, revocation], "both acts, oldest first");

    // Another folder has its own standing.
    assert_eq!(
        rp::standing(&ledger, "/synthetic/elsewhere").expect("standing"),
        Standing::Never
    );
    assert!(rp::revoke(
        &ledger,
        &act("/synthetic/elsewhere", "2026-09-24T09:07:00Z")
    )
    .expect("revoke")
    .is_none());
}

/// **Guard 5B-3.** The rows are append-only: the table's triggers refuse an
/// UPDATE, a DELETE and a second row under an existing id — through a raw
/// connection, the way anything outside the product would try it.
#[test]
fn guard_5b_3_the_permission_record_is_append_only() {
    let (dir, ledger) = store();
    let grant = rp::grant(&ledger, &act(FOLDER, "2026-09-24T09:00:00Z")).expect("grant");
    drop(ledger);
    let raw = Connection::open(dir.path().join("ledger.db")).expect("raw");

    for (what, sql) in [
        (
            "an UPDATE",
            "UPDATE run_permissions SET actor='an agent' WHERE id=?1",
        ),
        ("a DELETE", "DELETE FROM run_permissions WHERE id=?1"),
    ] {
        let err = raw
            .execute(sql, params![grant.id])
            .expect_err(&format!("{what} of a permission row went through"));
        assert!(
            err.to_string().contains("append-only"),
            "{what} was refused for the wrong reason: {err}"
        );
    }
    let err = raw
        .execute(
            "INSERT OR REPLACE INTO run_permissions(id, seq, act, project_id, project_folder,
                 actor, recorded_at, revokes, content_sha256, prev_hash, chain_hash)
             VALUES(?1, 99, 'revoked', ?2, ?3, 'an agent', 'now', ?1, 'x', 'y', 'z')",
            params![grant.id, PROJECT, FOLDER],
        )
        .expect_err("a row was replaced under an existing id");
    assert!(err.to_string().contains("append-only"), "{err}");

    let actor: String = raw
        .query_row(
            "SELECT actor FROM run_permissions WHERE id=?1",
            params![grant.id],
            |r| r.get(0),
        )
        .expect("the row is still there");
    assert_eq!(actor, "sam");
}

/// **Guard 5B-3.** The rows are hash-chained: each links to the one before
/// from genesis, `verify_chain` re-folds them from their own columns, and a
/// row altered behind the triggers' backs is found.
#[test]
fn guard_5b_3_the_permission_record_is_hash_chained_and_verify_finds_a_changed_row() {
    let (dir, ledger) = store();
    let g1 = rp::grant(&ledger, &act(FOLDER, "2026-09-24T09:00:00Z")).expect("grant");
    let r1 = rp::revoke(&ledger, &act(FOLDER, "2026-09-24T09:05:00Z"))
        .expect("revoke")
        .expect("revocation");
    let g2 = rp::grant(&ledger, &act(FOLDER, "2026-09-24T09:10:00Z")).expect("grant again");

    assert_eq!((g1.seq, r1.seq, g2.seq), (0, 1, 2));
    assert_eq!(g1.prev_hash, genesis_prev_hash(rp::CHAIN_SCOPE));
    assert_eq!(r1.prev_hash, g1.chain_hash);
    assert_eq!(g2.prev_hash, r1.chain_hash);
    for row in [&g1, &r1, &g2] {
        assert_eq!(
            row.chain_hash,
            chain_hash(
                &row.prev_hash,
                &row.content_sha256,
                rp::CHAIN_SCOPE,
                row.seq
            )
        );
    }
    assert_eq!(rp::verify_chain(&ledger).expect("the chain adds up"), 3);
    drop(ledger);

    // Somebody with the file and sqlite3 drops the trigger and rewrites who
    // allowed it. The chain no longer adds up, and verify says where.
    let raw = Connection::open(dir.path().join("ledger.db")).expect("raw");
    raw.execute_batch("DROP TRIGGER run_permissions_no_update")
        .expect("drop the trigger");
    raw.execute(
        "UPDATE run_permissions SET actor='an agent' WHERE id=?1",
        params![g1.id],
    )
    .expect("tamper");
    drop(raw);
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("reopen");
    let err = rp::verify_chain(&ledger).expect_err("a rewritten row verified");
    assert!(err.to_string().contains("seq 0"), "{err}");
}

/// The table's own shape: a revocation must name a grant, a grant must not.
#[test]
fn neither_act_can_be_written_wearing_the_others_shape() {
    let (dir, ledger) = store();
    drop(ledger);
    let raw = Connection::open(dir.path().join("ledger.db")).expect("raw");
    for (act, revokes) in [("revoked", None::<&str>), ("granted", Some("rpm_x"))] {
        let result = raw.execute(
            "INSERT INTO run_permissions(id, seq, act, project_id, project_folder,
                 actor, recorded_at, revokes, content_sha256, prev_hash, chain_hash)
             VALUES('rpm_shape', 0, ?1, ?2, ?3, 'sam', 'now', ?4, 'x', 'y', 'z')",
            params![act, PROJECT, FOLDER, revokes],
        );
        assert!(
            result.is_err(),
            "a {act} row with revokes={revokes:?} was stored"
        );
    }
}

/// **ADV-run-ledger-4.** A grant is appended only over the standing the
/// caller's decision rested on, read inside the append's own transaction.
/// A second "Allow" that saw "never" after the first was recorded appends
/// nothing and is told where the record stands; a grant over a standing grant
/// goes only when that grant is what the person was shown (its copy in the
/// folder had gone, so they were asked again); and a grant decided before a
/// revocation it never saw appends nothing.
#[test]
fn a_grant_over_a_standing_that_moved_appends_nothing() {
    let (_dir, ledger) = store();
    let g1 =
        match rp::grant_over(&ledger, &act(FOLDER, "2026-09-24T09:00:00Z"), None).expect("grant") {
            rp::Granting::Appended(row) => row,
            moved => panic!("the first grant was not appended: {moved:?}"),
        };
    assert_eq!(
        rp::grant_over(&ledger, &act(FOLDER, "2026-09-24T09:00:01Z"), None).expect("grant"),
        rp::Granting::Moved(Standing::Granted(g1.clone())),
        "a second press that saw 'never' appended a grant"
    );
    assert_eq!(rp::history(&ledger, FOLDER).expect("history").len(), 1);

    // Asked again over the grant it was shown: one new grant.
    let g2 = match rp::grant_over(
        &ledger,
        &act(FOLDER, "2026-09-24T09:01:00Z"),
        Some(g1.id.as_str()),
    )
    .expect("grant")
    {
        rp::Granting::Appended(row) => row,
        moved => panic!("a grant over the grant the person saw was refused: {moved:?}"),
    };
    let r = rp::revoke(&ledger, &act(FOLDER, "2026-09-24T09:02:00Z"))
        .expect("revoke")
        .expect("a standing grant to take back");
    assert_eq!(r.revokes.as_deref(), Some(g2.id.as_str()));
    // Decided while g2 stood; the revocation came first.
    assert_eq!(
        rp::grant_over(
            &ledger,
            &act(FOLDER, "2026-09-24T09:02:01Z"),
            Some(g2.id.as_str())
        )
        .expect("grant"),
        rp::Granting::Moved(Standing::Revoked(r.clone()))
    );
    assert!(matches!(
        rp::grant_over(
            &ledger,
            &act(FOLDER, "2026-09-24T09:03:00Z"),
            Some(r.id.as_str())
        )
        .expect("grant"),
        rp::Granting::Appended(_)
    ));
    assert_eq!(rp::history(&ledger, FOLDER).expect("history").len(), 4);
    assert_eq!(rp::verify_chain(&ledger).expect("the chain adds up"), 4);
}

/// Settings may show one grant while another process acts. Taking back must
/// compare the row the person saw inside the append transaction, so a stale
/// press never revokes a newer grant for the same folder.
#[test]
fn a_revoke_over_a_standing_that_moved_appends_nothing() {
    let (_dir, ledger) = store();
    let first = rp::grant(&ledger, &act(FOLDER, "2026-09-24T09:00:00Z")).expect("first grant");
    let old = rp::revoke(&ledger, &act(FOLDER, "2026-09-24T09:01:00Z"))
        .expect("first revoke")
        .expect("standing grant");
    let current = rp::grant(&ledger, &act(FOLDER, "2026-09-24T09:02:00Z")).expect("new grant");

    assert_eq!(
        rp::revoke_over(
            &ledger,
            &act(FOLDER, "2026-09-24T09:03:00Z"),
            Some(first.id.as_str()),
        )
        .expect("stale revoke"),
        rp::Revoking::Moved(Standing::Granted(current.clone())),
    );
    assert_eq!(rp::history(&ledger, FOLDER).expect("history").len(), 3);
    assert_eq!(
        rp::revoke_over(
            &ledger,
            &act(FOLDER, "2026-09-24T09:04:00Z"),
            Some(current.id.as_str()),
        )
        .expect("current revoke"),
        rp::Revoking::Appended(rp::history(&ledger, FOLDER).expect("history")[3].clone()),
    );
    let history = rp::history(&ledger, FOLDER).expect("history");
    assert_eq!(history[3].revokes.as_deref(), Some(current.id.as_str()));
    assert_eq!(old.revokes.as_deref(), Some(first.id.as_str()));
    assert_eq!(rp::verify_chain(&ledger).expect("chain"), 4);
}

/// **ADV-run-ledger-4.** Racing writers on one store, each on its own
/// connection the way two requests are: ten grants that all saw "never"
/// append exactly one row, and `grant` over a grant that already stands
/// appends nothing and answers with the standing grant.
#[test]
fn racing_grants_on_separate_connections_append_exactly_one_row() {
    let (dir, ledger) = store();
    let db = dir.path().join("ledger.db");
    let answers: Vec<rp::Granting> = std::thread::scope(|s| {
        let racers: Vec<_> = (0..10)
            .map(|_| {
                let db = db.clone();
                s.spawn(move || {
                    let mine = Ledger::open(&db).expect("open");
                    rp::grant_over(&mine, &act(FOLDER, "2026-09-24T09:00:00Z"), None)
                        .expect("grant")
                })
            })
            .collect();
        racers
            .into_iter()
            .map(|r| r.join().expect("a racer"))
            .collect()
    });
    let appended = answers
        .iter()
        .filter(|a| matches!(a, rp::Granting::Appended(_)))
        .count();
    assert_eq!(appended, 1, "{answers:#?}");
    let history = rp::history(&ledger, FOLDER).expect("history");
    assert_eq!(history.len(), 1, "{history:#?}");
    for answer in &answers {
        if let rp::Granting::Moved(now) = answer {
            assert_eq!(now, &Standing::Granted(history[0].clone()));
        }
    }

    let again = rp::grant(&ledger, &act(FOLDER, "2026-09-24T09:05:00Z")).expect("grant");
    assert_eq!(
        again, history[0],
        "a grant over a standing grant appended a row"
    );
    assert_eq!(rp::history(&ledger, FOLDER).expect("history").len(), 1);
    assert_eq!(rp::verify_chain(&ledger).expect("the chain adds up"), 1);
}
