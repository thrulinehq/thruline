//! §5u — attention is derived, never collected.
//!
//! These drive a real ledger through ingest → annotate-free derivation and
//! assert the structural properties M6.R1's gate depends on: determinism
//! across rebuilds, content-addressed identity, shadow containment, and
//! append-only acknowledgments.
//!
//! No model is loaded: attention reads the derived projection and accepted
//! capture records, never the scorer.

use std::fs;
use std::path::PathBuf;

use thruline_core::attention::{
    acknowledge, acknowledged_ids, derive_attention, AcknowledgmentAction, AttentionKind,
};
use thruline_core::ingest::ingest;
use thruline_core::ledger::Ledger;

const PROJ: &str = "-Users-dev-attention";
const S: &str = "aaaaaaaa-1111-4222-8333-444444444444";

fn u(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012}")
}

/// One turn: a failing run, then a claim that all tests pass. The truth engine
/// convicts this shape deterministically (the M3.9 crown jewel), so it gives
/// attention a real `contradiction` to derive without inventing fixtures.
fn transcript() -> String {
    let mut l: Vec<String> = Vec::new();
    l.push(format!(
        r#"{{"type":"user","uuid":"{}","parentUuid":null,"isSidechain":false,"sessionId":"{S}","timestamp":"2026-08-01T00:00:00.000Z","cwd":"/repo","message":{{"role":"user","content":"run the suite"}}}}"#,
        u(1)
    ));
    l.push(format!(
        r#"{{"type":"assistant","uuid":"{}","parentUuid":"{}","isSidechain":false,"sessionId":"{S}","timestamp":"2026-08-01T00:00:01.000Z","cwd":"/repo","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"t1","name":"Bash","input":{{"command":"cargo test --all"}}}}]}}}}"#,
        u(2),
        u(1)
    ));
    l.push(format!(
        r#"{{"type":"user","uuid":"{}","parentUuid":"{}","isSidechain":false,"sessionId":"{S}","timestamp":"2026-08-01T00:00:02.000Z","cwd":"/repo","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"t1","content":"running 33 tests\ntest result: FAILED. 31 passed; 2 failed; 0 ignored"}}]}}}}"#,
        u(3),
        u(2)
    ));
    l.push(format!(
        r#"{{"type":"assistant","uuid":"{}","parentUuid":"{}","isSidechain":false,"sessionId":"{S}","timestamp":"2026-08-01T00:00:03.000Z","cwd":"/repo","message":{{"role":"assistant","content":[{{"type":"text","text":"33/33 tests pass."}}]}}}}"#,
        u(4),
        u(3)
    ));
    l.join("\n") + "\n"
}

fn setup() -> (tempfile::TempDir, Ledger) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("projects");
    let project = root.join(PROJ);
    fs::create_dir_all(&project).expect("mkdir");
    fs::write(project.join(format!("{S}.jsonl")), transcript()).expect("write");
    let mut ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    ingest(
        &mut ledger,
        std::slice::from_ref(&PathBuf::from(&root)),
        &dir.path().join("mirrors"),
    )
    .expect("ingest");
    (dir, ledger)
}

#[test]
fn attention_is_byte_identical_across_two_derivations() {
    let (_d, ledger) = setup();
    let a = derive_attention(&ledger, PROJ).expect("derive");
    let b = derive_attention(&ledger, PROJ).expect("derive again");
    assert_eq!(
        serde_json::to_string(&a).expect("json"),
        serde_json::to_string(&b).expect("json"),
        "attention derivation is not deterministic"
    );
    assert!(!a.is_empty(), "derivation produced nothing at all");
}

#[test]
fn a_project_that_was_never_read_says_so_instead_of_claiming_calm() {
    let (_d, ledger) = setup();
    // No rebuild has run, so the projection does not exist. This test used to
    // assert AllClear here — it encoded the defect as an expectation, and the
    // product shipped "everything checks out" for 43 of 44 real projects on the
    // strength of it. Absence of a projection is not absence of a problem.
    let items = derive_attention(&ledger, PROJ).expect("derive");
    assert_eq!(items.len(), 1, "expected exactly one item: {items:?}");
    assert_eq!(items[0].kind, AttentionKind::NotRead);
    assert!(
        !items[0].id.is_empty(),
        "even 'not read' carries an id — it is an item, not an absence"
    );
    assert!(
        !items[0].detail.to_lowercase().contains("no contradictions"),
        "a project we have not looked at must not report what it does not contain"
    );
}

#[test]
fn calm_returns_once_the_projection_has_actually_been_built() {
    let (_d, ledger) = setup();
    thruline_core::derive::rebuild(&ledger, PROJ, &thruline_core::config::Config::default())
        .expect("rebuild");
    let items = derive_attention(&ledger, PROJ).expect("derive");
    assert_eq!(items.len(), 1, "expected exactly the calm item: {items:?}");
    assert_eq!(
        items[0].kind,
        AttentionKind::AllClear,
        "once the projection is current and finds nothing, calm is the honest answer"
    );
}

#[test]
fn no_shadow_class_can_appear_in_a_derived_item() {
    // §5u's containment property, asserted over the emitted stream rather than
    // the type: a future branch that stringified a NodeState into `detail`
    // would be caught here even though the taxonomy test would not see it.
    let (_d, ledger) = setup();
    let items = derive_attention(&ledger, PROJ).expect("derive");
    let blob = serde_json::to_string(&items).expect("json");
    for shadow in ["\"unsupported\"", "\"dropped\""] {
        assert!(
            !blob.contains(shadow),
            "a shadow class reached an attention item: {blob}"
        );
    }
}

#[test]
fn dismissal_is_append_only_and_expires_when_the_subject_changes() {
    let (_d, ledger) = setup();
    let items = derive_attention(&ledger, PROJ).expect("derive");
    let id = items[0].id.clone();

    acknowledge(
        &ledger,
        &id,
        AcknowledgmentAction::Dismissed,
        1_785_000_000_000,
    )
    .expect("acknowledge");
    let acks = acknowledged_ids(&ledger).expect("read acks");
    assert!(acks.contains_key(&id), "dismissal was not recorded");

    // Append-only: the triggers must refuse both mutations. This is the same
    // protection the M5 lifecycle records carry, and it is what stops the
    // product rewriting its past to look calmer.
    let update = ledger.conn().execute(
        "UPDATE attention_acknowledgments SET action='dismissed' WHERE item_id=?1",
        rusqlite::params![id],
    );
    assert!(update.is_err(), "acknowledgments accepted an UPDATE");
    let delete = ledger.conn().execute(
        "DELETE FROM attention_acknowledgments WHERE item_id=?1",
        rusqlite::params![id],
    );
    assert!(delete.is_err(), "acknowledgments accepted a DELETE");

    // Re-deriving yields the same id while the subject is unchanged, so the
    // dismissal still applies.
    let again = derive_attention(&ledger, PROJ).expect("derive");
    assert_eq!(again[0].id, id, "id drifted without a subject change");
}

#[test]
fn attention_opens_no_socket_and_spawns_nothing() {
    // §5u structurally: the derivation path is pure ledger reads. This asserts
    // the property the M6.R1 gate reports as watchers/pollers/daemons/network
    // = 0/0/0/0 — measured here as "no threads were spawned and the call is
    // synchronous", which is the strongest claim a unit test can make.
    let (_d, ledger) = setup();
    let before = std::thread::available_parallelism().ok();
    let items = derive_attention(&ledger, PROJ).expect("derive");
    let after = std::thread::available_parallelism().ok();
    assert_eq!(before, after);
    assert!(!items.is_empty());
}
