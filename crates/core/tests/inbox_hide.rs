//! Inbox hide acts use a scratch ledger and the normal HQ index. No real store
//! or network path is opened here.

mod scratch;

use std::path::Path;

use scratch::TempTree;
use thruline_core::hq;
use thruline_core::inbox_hide::{self, Act, KeepRequest, Refusal, RowKind};
use thruline_core::ingest::ingest;
use thruline_core::Ledger;

const NOW: &str = "2026-10-07T00:00:00Z";

fn count(ledger: &Ledger) -> i64 {
    ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM inbox_hides", [], |r| r.get(0))
        .expect("hide count")
}

fn store_bytes(path: &Path) -> Vec<Option<Vec<u8>>> {
    ["", "-wal"]
        .iter()
        .map(|suffix| std::fs::read(format!("{}{}", path.display(), suffix)).ok())
        .collect()
}

#[test]
fn reading_is_byte_stable_and_only_current_row_acts_append() {
    let tree = TempTree::new();
    let db = tree.join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open scratch ledger");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hq/claude/-work-app");
    ingest(&mut ledger, &[fixture], &tree.join("mirror")).expect("ingest fixture");
    let project = hq::hq_projects(&ledger).expect("projects").remove(0);
    let index = hq::index(&ledger, &project).expect("index");
    let fp = hq::fingerprint(&ledger, &project).expect("fingerprint");
    let now = hq::parse_ms(NOW).expect("now");
    let address = index
        .inbox_addresses(now)
        .into_iter()
        .next()
        .expect("Inbox row");
    assert_eq!(
        address.row_sha256.len(),
        64,
        "the index precomputed the address"
    );
    let other_kind = if address.row_kind == RowKind::Thread {
        RowKind::Prompt
    } else {
        RowKind::Thread
    };
    assert!(
        index
            .inbox_address(&address.thread_id, other_kind, now)
            .is_none(),
        "only the displayed kind is actionable"
    );
    let row_count = index.view(now).threads.len();
    let before = store_bytes(&db);

    let empty = inbox_hide::last_acts(&ledger).expect("read acts");
    let rows = inbox_hide::current_rows(&index, now, &empty);
    assert_eq!(rows.len(), row_count);
    assert!(rows.iter().all(|r| !r.hidden));
    assert_eq!(count(&ledger), 0);
    assert_eq!(store_bytes(&db), before, "GET does not write the store");

    let hidden = inbox_hide::keep(
        &ledger,
        &index,
        KeepRequest {
            index_fp: &fp,
            thread_id: &address.thread_id,
            kind: address.row_kind,
            expected_sha256: &address.row_sha256,
            now_ms: now,
            act: Act::Hidden,
            actor: "A Person",
            at: NOW,
        },
    )
    .expect("hide write")
    .expect("current row");
    assert!(hidden.hidden);
    assert_eq!(count(&ledger), 1);
    let after_hide = store_bytes(&db);

    let again = inbox_hide::keep(
        &ledger,
        &index,
        KeepRequest {
            index_fp: &fp,
            thread_id: &address.thread_id,
            kind: address.row_kind,
            expected_sha256: &address.row_sha256,
            now_ms: now,
            act: Act::Hidden,
            actor: "A Person",
            at: NOW,
        },
    )
    .expect("duplicate outcome");
    assert_eq!(again, Err(Refusal::AlreadyHidden));
    let stale = inbox_hide::keep(
        &ledger,
        &index,
        KeepRequest {
            index_fp: &fp,
            thread_id: &address.thread_id,
            kind: address.row_kind,
            expected_sha256: &"0".repeat(64),
            now_ms: now,
            act: Act::Hidden,
            actor: "A Person",
            at: NOW,
        },
    )
    .expect("stale outcome");
    assert_eq!(stale, Err(Refusal::Changed));
    let old_index = inbox_hide::keep(
        &ledger,
        &index,
        KeepRequest {
            index_fp: "an old fingerprint",
            thread_id: &address.thread_id,
            kind: address.row_kind,
            expected_sha256: &address.row_sha256,
            now_ms: now,
            act: Act::Hidden,
            actor: "A Person",
            at: NOW,
        },
    )
    .expect("fingerprint outcome");
    assert_eq!(old_index, Err(Refusal::Changed));
    assert_eq!(count(&ledger), 1, "refusals append no acts");
    assert_eq!(store_bytes(&db), after_hide, "refusals write no bytes");

    let acts = inbox_hide::last_acts(&ledger).expect("read acts");
    assert!(inbox_hide::state_for(address.clone(), &acts).hidden);
    let mut changed = address.clone();
    changed.row_sha256 = inbox_hide::row_sha256(
        changed.row_kind,
        [address.thread_event_id.as_str(), "a newly recorded event"],
    );
    assert_ne!(changed.row_sha256, address.row_sha256);
    assert!(
        !inbox_hide::state_for(changed, &acts).hidden,
        "a changed row reappears"
    );

    let shown = inbox_hide::keep(
        &ledger,
        &index,
        KeepRequest {
            index_fp: &fp,
            thread_id: &address.thread_id,
            kind: address.row_kind,
            expected_sha256: &address.row_sha256,
            now_ms: now,
            act: Act::Shown,
            actor: "A Person",
            at: NOW,
        },
    )
    .expect("show write")
    .expect("hidden row");
    assert!(!shown.hidden);
    assert_eq!(count(&ledger), 2);
    let shows: String = ledger
        .conn()
        .query_row(
            "SELECT shows FROM inbox_hides WHERE act = 'shown'",
            [],
            |r| r.get(0),
        )
        .expect("show reference");
    assert_eq!(Some(shows), hidden.act_id);
    assert!(
        !inbox_hide::state_for(address, &inbox_hide::last_acts(&ledger).expect("last acts")).hidden
    );
}

#[test]
fn event_ids_and_kind_both_define_the_row_digest() {
    let ids = ["ab", "c"];
    assert_ne!(
        inbox_hide::row_sha256(RowKind::Thread, ids),
        inbox_hide::row_sha256(RowKind::Thread, ["a", "bc"]),
        "length framing avoids concatenation collisions",
    );
    assert_ne!(
        inbox_hide::row_sha256(RowKind::Thread, ids),
        inbox_hide::row_sha256(RowKind::Prompt, ids),
    );
    assert_ne!(
        inbox_hide::row_sha256(RowKind::Thread, ids),
        inbox_hide::row_sha256(RowKind::Thread, ["ab", "c", "d"]),
    );
    assert_ne!(
        inbox_hide::row_sha256(RowKind::Thread, ids),
        inbox_hide::row_sha256(RowKind::Thread, ["ab", "inserted earlier", "c"]),
        "a backfilled event before the last still changes the address",
    );
}
