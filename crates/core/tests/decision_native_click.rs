//! Synthetic persisted native evidence only: never calls a person constructor.
#[path = "fixtures/decision_conflict/mod.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "fixtures/decision_conflict_general/mod.rs"]
mod general;
#[path = "fixtures/decision_native_click/mod.rs"]
mod native;
#[path = "support/person_action.rs"]
mod person_action;
use fixture::*;
use general::*;
use thruline_core::decisions::{
    authority::{bridge, PersonActKind},
    storage,
};
fn bound(alteration: &str) -> (Fixture, String, String, String) {
    let mut f = Fixture::new();
    let old = f.decision("p", default("true"), true);
    let new = f.decision("p", default("false"), true);
    let act = retirement(&f, &old, &new);
    let review = reviewed(&f, &act);
    native::bind_native(&f, &act, &review, alteration);
    (f, old, new, act)
}
#[test]
fn native_exact_general_receipt_preserves_both_pins_and_lifecycle() {
    let (f, old, _, act) = bound("");
    assert!(bridge::verified(&f.ledger, PersonActKind::Retire, &act).expect("verify"));
    assert!(storage::verified_retirement_for_decision(&f.ledger, &old)
        .expect("standing")
        .is_some());
}
#[test]
fn native_changed_review_hash_is_inert() {
    let (f, _, _, act) = bound("hash");
    assert!(!bridge::verified(&f.ledger, PersonActKind::Retire, &act).expect("verify"));
}
#[test]
fn native_changed_nested_metadata_is_inert() {
    let (f, _, _, act) = bound("pin");
    assert!(!bridge::verified(&f.ledger, PersonActKind::Retire, &act).expect("verify"));
}
#[test]
fn native_wrong_action_metadata_is_inert() {
    let (f, _, _, act) = bound("action");
    assert!(!bridge::verified(&f.ledger, PersonActKind::Retire, &act).expect("verify"));
}
#[test]
fn native_wrong_schema_is_inert() {
    let (f, _, _, act) = bound("schema");
    assert!(!bridge::verified(&f.ledger, PersonActKind::Retire, &act).expect("verify"));
}
#[test]
fn native_extra_envelope_field_is_inert() {
    let (f, _, _, act) = bound("extra");
    assert!(!bridge::verified(&f.ledger, PersonActKind::Retire, &act).expect("verify"));
}
#[test]
fn native_missing_hash_is_inert() {
    let (f, _, _, act) = bound("missing");
    assert!(!bridge::verified(&f.ledger, PersonActKind::Retire, &act).expect("verify"));
}
#[test]
fn native_nonce_cannot_be_replayed() {
    let (f, _, _, act) = bound("nonce");
    assert!(!bridge::verified(&f.ledger, PersonActKind::Retire, &act).expect("verify"));
}
#[test]
fn native_cannot_claim_terminal_transport() {
    let (f, _, _, act) = bound("format");
    assert!(!bridge::verified(&f.ledger, PersonActKind::Retire, &act).expect("verify"));
}
#[test]
fn native_later_successor_revision_invalidates_retirement() {
    let (mut f, old, new, act) = bound("");
    revise(&mut f, &new, "true", true);
    assert!(!bridge::verified(&f.ledger, PersonActKind::Retire, &act).expect("verify"));
    assert!(storage::verified_retirement_for_decision(&f.ledger, &old)
        .expect("standing")
        .is_none());
}
#[test]
fn native_lost_successor_proof_invalidates_retirement() {
    let (f, old, _, act) = bound("");
    std::fs::write(f.people.last().expect("person").raw_file(), "changed")
        .expect("isolated source edit");
    assert!(!bridge::verified(&f.ledger, PersonActKind::Retire, &act).expect("verify"));
    assert!(storage::verified_retirement_for_decision(&f.ledger, &old)
        .expect("standing")
        .is_none());
}
