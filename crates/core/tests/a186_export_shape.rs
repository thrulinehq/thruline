//! A186 — the §9 project document carries each structure once.
//!
//! RB7's Layer-2 candidates are a **project-wide** structure. A158 emitted them
//! at document level (correct) *and* inside the per-receipt loop, which was
//! quadratic in both axes at once: space O(receipts × candidates), because one
//! structure was stored once per receipt, and time O(receipts × derivation),
//! because `candidates::derive` is a full derivation over the project's events
//! and was re-run for every receipt.
//!
//! What that cost on the real corpus, measured at A186 before the fix:
//! 1209 receipts × 350,845 bytes = 424 MB of duplication inside a single 479 MB
//! document — receipts were 99.72% of the file with the receipt *count*
//! unchanged. Across 90 projects the export went 324 MB → 4,794 MB, the largest
//! single document reached **2.73 GB**, and the cold export stopped completing:
//! it died at project 28 of 91 after 28 minutes.
//!
//! The size was the lesser problem. Both shells fetch `projects/<id>.json`
//! whole, so a 2.73 GB document is one the product's own front door cannot open.
//!
//! This pins the shape rather than a byte count, because a byte count would
//! drift with the corpus while the defect being guarded is structural.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use thruline_core::derive::derive_project;
use thruline_core::export::export_project;
use thruline_core::ingest::ingest;
use thruline_core::ledger::Ledger;

const PROJ: &str = "-Users-dev-a186";
const AT: &str = "2026-08-15T00:00:00Z";

fn doc() -> Value {
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
    let graph = derive_project(&ledger, PROJ).expect("derive");
    let out = export_project(&ledger, &graph, AT).expect("export");
    std::mem::forget(dir);
    out
}

#[test]
fn candidates_live_once_at_document_level_and_never_inside_a_receipt() {
    let doc = doc();
    assert!(
        doc.get("candidates").is_some(),
        "the document-level candidates block is what the UI reads (fixture.candidates)"
    );

    let receipts = doc["receipts"].as_object().expect("receipts object");
    assert!(
        !receipts.is_empty(),
        "the fixture produced no receipts to check"
    );
    for (id, receipt) in receipts {
        assert!(
            receipt.get("candidates").is_none(),
            "receipt {id} carries a copy of the project-wide candidates block — \
             this is the A186 quadratic duplication, back again"
        );
    }
}

/// The receipt's own key set, pinned. Anything project-wide added here is
/// multiplied by the receipt count, so the set is small on purpose.
#[test]
fn a_receipt_carries_only_its_own_four_facts() {
    let doc = doc();
    let receipts = doc["receipts"].as_object().expect("receipts object");
    for (id, receipt) in receipts {
        let mut keys: Vec<&str> = receipt
            .as_object()
            .expect("receipt object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["claim", "evidence", "ledger", "verdict"],
            "receipt {id} changed shape; a per-receipt field is paid for once per receipt"
        );
    }
}
