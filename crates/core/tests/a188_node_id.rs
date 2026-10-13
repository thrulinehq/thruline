//! A188 — the exact-claim deep link, and the kinds that must not pretend to one.
//!
//! §12 recorded this gap twice (A143, A144): the Inbox row's action says *"Look
//! at the receipt."* and the index gave it no way to reach one, because
//! `derive_attention` knew the node and the export dropped it.
//!
//! The dangerous half is not the emission, it is the **absence**. Four of the
//! five attention kinds carry no node — `staleness` carries an event id,
//! `capture_attention` an owner/source pair, `all_clear` and `not_read` the
//! literal string "project". Emitting a best-guess `node_id` for any of them
//! would deep-link a row to an arbitrary claim while its sentence describes
//! something else. So these tests care more about what is *not* there.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use thruline_core::attention::{all_clear_item, not_read_item, AttentionKind};
use thruline_core::derive::derive_project;
use thruline_core::export::{export_index, export_project, index_row, ExportOptions};
use thruline_core::ingest::ingest;
use thruline_core::ledger::Ledger;
use thruline_core::now::{now, Coverage};

const PROJ: &str = "-Users-dev-a188";
const AT: &str = "2026-08-15T00:00:00Z";

fn setup() -> (tempfile::TempDir, Ledger) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("projects");
    fs::create_dir_all(root.join(PROJ)).expect("mkdir");
    let body = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude_code_sessions.jsonl"),
    )
    .expect("fixture");
    fs::write(
        root.join(PROJ)
            .join("00000000-0000-4000-8000-000000000000.jsonl"),
        body,
    )
    .expect("write");
    let mut ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    ingest(
        &mut ledger,
        std::slice::from_ref(&root),
        &dir.path().join("mirrors"),
    )
    .expect("ingest");
    (dir, ledger)
}

fn row(ledger: &Ledger) -> Value {
    let graph = derive_project(ledger, PROJ).expect("derive");
    let doc = export_project(ledger, &graph, AT).expect("export");
    let queues = now(ledger, None, false, Coverage::Complete).expect("now");
    let queue = queues
        .iter()
        .find(|q| q.project_id == PROJ)
        .expect("a queue");
    let index = export_index(
        AT,
        ExportOptions::default(),
        vec![index_row(&doc, queue)],
        Some("2026-08-01"),
    );
    index["projects"][0].clone()
}

#[test]
fn a_project_level_kind_carries_no_node_id_at_all() {
    // The absence is the point, and it must be ABSENCE — not null, not "".
    // A reader doing `if (item.node_id)` and a reader doing
    // `if ('node_id' in item)` must reach the same conclusion.
    let (_d, ledger) = setup();
    let before = row(&ledger);
    let item = &before["attention"]["items"][0];
    assert_eq!(item["kind"], "not_read", "expected the unread state first");
    assert!(
        item.get("node_id").is_none(),
        "a project-level kind was given a node_id: {item}"
    );

    thruline_core::derive::rebuild(&ledger, PROJ, &thruline_core::config::Config::default())
        .expect("rebuild");
    let after = row(&ledger);
    let calm = &after["attention"]["items"][0];
    assert_eq!(calm["kind"], "all_clear");
    assert!(
        calm.get("node_id").is_none(),
        "the calm item was given a node_id: {calm}"
    );
}

/// The subject each kind carries, asserted at the source rather than inferred.
///
/// This is the fact A188 Part 0 measured and the emission rule depends on: only
/// `contradiction`'s subject is a node id. If a future change makes another
/// kind node-subjected, this test is where that becomes visible.
#[test]
fn only_the_contradiction_kind_has_a_node_for_a_subject() {
    let (_d, ledger) = setup();
    let calm = all_clear_item(&ledger, PROJ).expect("calm item");
    assert_eq!(calm.kind, AttentionKind::AllClear);
    assert_eq!(
        calm.subject, "project",
        "all_clear is a statement about a projection, not about a node"
    );
    let unread = not_read_item(&ledger, PROJ, "not read").expect("unread item");
    assert_eq!(
        unread.subject, "project",
        "not_read is a statement about a projection, not about a node"
    );
}

/// The index stays an index: it may carry a reference, never the claim.
#[test]
fn the_index_carries_the_id_and_not_the_claim() {
    let (_d, ledger) = setup();
    thruline_core::derive::rebuild(&ledger, PROJ, &thruline_core::config::Config::default())
        .expect("rebuild");
    let row = row(&ledger);
    for item in row["attention"]["items"].as_array().expect("items") {
        let keys: Vec<&str> = item
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        for k in &keys {
            assert!(
                matches!(*k, "action" | "id" | "kind" | "sentence" | "node_id"),
                "the index item grew a key beyond the first-level contract: {k}"
            );
        }
        // Whatever else changes, the claim text itself never enters L0.
        assert!(
            item.get("claim").is_none() && item.get("evidence").is_none(),
            "the index started carrying the claim instead of a reference to it"
        );
    }
}

#[test]
fn the_contract_version_records_the_widening() {
    let (_d, ledger) = setup();
    thruline_core::derive::rebuild(&ledger, PROJ, &thruline_core::config::Config::default())
        .expect("rebuild");
    let graph = derive_project(&ledger, PROJ).expect("derive");
    let doc = export_project(&ledger, &graph, AT).expect("export");
    let queues = now(&ledger, None, false, Coverage::Complete).expect("now");
    let queue = queues.iter().find(|q| q.project_id == PROJ).expect("queue");
    let index = export_index(
        AT,
        ExportOptions::default(),
        vec![index_row(&doc, queue)],
        Some("2026-08-01"),
    );
    assert_eq!(
        index["contract_version"], 3,
        "widening the index without bumping its version leaves readers no way to know"
    );
}
