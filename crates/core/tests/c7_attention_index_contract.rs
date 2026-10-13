//! C7 Part 0 pin — the §9 index carries the calm/never-looked distinction.
//!
//! The Inbox (C7 second half) renders its empty state from `p.attention.items`,
//! and the empty state is the whole risk: "Everything checks out" is a claim,
//! and a product that says it because it found nothing to look at — rather than
//! because it looked and found nothing — is lying calmly. That defect has
//! already shipped once here (A-note in `attention_derivation.rs`: the product
//! reported calm for 43 of 44 real projects on the strength of a test that
//! encoded the defect as an expectation).
//!
//! `attention_derivation.rs` pins the distinction at the *derivation*. This
//! pins it one layer out, at the **index the UI actually reads**, because a
//! derivation that separates two states and an export that flattens them is a
//! product that lies anyway. Measured shape recorded in
//! `docs/recon/c7-attention-probe.json`.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use thruline_core::derive::derive_project;
use thruline_core::export::{export_index, export_project, index_row, ExportOptions};
use thruline_core::ingest::ingest;
use thruline_core::ledger::Ledger;
use thruline_core::now::{now, Coverage};

const PROJ: &str = "-Users-dev-c7-inbox";
const S: &str = "bbbbbbbb-1111-4222-8333-555555555555";
const AT: &str = "2026-08-15T00:00:00Z";

fn u(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012}")
}

fn transcript() -> String {
    let mut l: Vec<String> = Vec::new();
    l.push(format!(
        r#"{{"type":"user","uuid":"{}","parentUuid":null,"isSidechain":false,"sessionId":"{S}","timestamp":"2026-08-01T00:00:00.000Z","cwd":"/repo","message":{{"role":"user","content":"add the inbox"}}}}"#,
        u(1)
    ));
    l.push(format!(
        r#"{{"type":"assistant","uuid":"{}","parentUuid":"{}","isSidechain":false,"sessionId":"{S}","timestamp":"2026-08-01T00:00:01.000Z","cwd":"/repo","message":{{"role":"assistant","content":[{{"type":"text","text":"I will add the inbox tab."}}]}}}}"#,
        u(2),
        u(1)
    ));
    l.join("\n") + "\n"
}

fn setup() -> (tempfile::TempDir, Ledger) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("projects");
    fs::create_dir_all(root.join(PROJ)).expect("mkdir");
    fs::write(root.join(PROJ).join(format!("{S}.jsonl")), transcript()).expect("write");
    let mut ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    ingest(
        &mut ledger,
        std::slice::from_ref(&PathBuf::from(&root)),
        &dir.path().join("mirrors"),
    )
    .expect("ingest");
    (dir, ledger)
}

/// Build the one index row the UI would read for this project.
fn row(ledger: &Ledger) -> Value {
    let graph = derive_project(ledger, PROJ).expect("derive");
    let doc = export_project(ledger, &graph, AT).expect("export project");
    let queues = now(ledger, None, false, Coverage::Complete).expect("now");
    let queue = queues
        .iter()
        .find(|q| q.project_id == PROJ)
        .expect("a queue for the project");
    let index = export_index(
        AT,
        ExportOptions::default(),
        vec![index_row(&doc, queue)],
        Some("2026-08-01"),
    );
    index["projects"][0].clone()
}

fn kinds(row: &Value) -> Vec<String> {
    row["attention"]["items"]
        .as_array()
        .expect("items array")
        .iter()
        .map(|i| i["kind"].as_str().expect("kind").to_string())
        .collect()
}

#[test]
fn the_index_separates_evaluated_calm_from_never_evaluated() {
    let (_d, ledger) = setup();

    // Nothing has been rebuilt, so no projection exists at the current chain
    // position. The index must NOT say everything checks out.
    let before = row(&ledger);
    assert_eq!(
        kinds(&before),
        vec!["not_read"],
        "a project whose projection was never built must say so in the index"
    );

    thruline_core::derive::rebuild(&ledger, PROJ, &thruline_core::config::Config::default())
        .expect("rebuild");

    // Now the projection is current and found nothing. Calm is earned.
    let after = row(&ledger);
    assert_eq!(
        kinds(&after),
        vec!["all_clear"],
        "once the projection is current and finds nothing, the index may say calm"
    );

    assert_ne!(
        before["attention"]["items"][0]["sentence"], after["attention"]["items"][0]["sentence"],
        "the two states must not share a sentence — that is the flattening this pins against"
    );
}

/// The Inbox reads exactly these four keys per item and nothing deeper.
///
/// First level only: no `detail`, no anchor event id, no session id. Receipts
/// live one disclosure behind, and the index is the L0 contract (§9).
#[test]
fn an_index_item_carries_only_first_level_keys() {
    let (_d, ledger) = setup();
    thruline_core::derive::rebuild(&ledger, PROJ, &thruline_core::config::Config::default())
        .expect("rebuild");
    let row = row(&ledger);
    let item = &row["attention"]["items"][0];
    let mut keys: Vec<&str> = item
        .as_object()
        .expect("item object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["action", "id", "kind", "sentence"],
        "the index item shape changed; the Inbox and the probe artifact both read it"
    );
}

/// `needing_attention` counts what needs a human. Calm does not; "we have not
/// looked" does — not looking is itself a thing a person can act on.
#[test]
fn calm_does_not_count_as_needing_attention_but_never_looked_does() {
    let (_d, ledger) = setup();
    let before = row(&ledger);
    assert_eq!(
        before["attention"]["needing_attention"], 1,
        "an unread project is not a calm project"
    );
    thruline_core::derive::rebuild(&ledger, PROJ, &thruline_core::config::Config::default())
        .expect("rebuild");
    let after = row(&ledger);
    assert_eq!(
        after["attention"]["needing_attention"], 0,
        "calm must not inflate the queue count"
    );
}
