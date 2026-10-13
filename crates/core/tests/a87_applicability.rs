//! A87: `stale` is derived from applicability, and from nothing else.
//!
//! The doctrine (NORTHSTAR-v2, "The state model" and "Proof freshness") is that
//! a proof carries three axes — evidence state, currency, applicability — and
//! that `verified` reads `stale` when, and only when, its receipt's
//! applicability has lapsed. These tests pin the "and only when", because the
//! expensive failure mode is not missing a staleness: it is manufacturing one.

use thruline_core::capture::ProofApplicability;
use thruline_core::derive::{applied_evidence_state, EvidenceState};

const STATES: [EvidenceState; 6] = [
    EvidenceState::Unknown,
    EvidenceState::Observed,
    EvidenceState::Supported,
    EvidenceState::Verified,
    EvidenceState::Contradicted,
    EvidenceState::Stale,
];

const APPLICABILITIES: [ProofApplicability; 3] = [
    ProofApplicability::Current,
    ProofApplicability::Stale,
    ProofApplicability::Unknown,
];

#[test]
fn verified_reads_stale_when_its_receipt_lapses() {
    assert_eq!(
        applied_evidence_state(EvidenceState::Verified, ProofApplicability::Stale),
        EvidenceState::Stale
    );
}

#[test]
fn exactly_one_transition_exists_in_the_whole_table() {
    let mut moved = Vec::new();
    for state in STATES {
        for applicability in APPLICABILITIES {
            let out = applied_evidence_state(state, applicability);
            if out != state {
                moved.push((state, applicability, out));
            }
        }
    }
    assert_eq!(
        moved.len(),
        1,
        "the derivation must move exactly one cell of the 6x3 table; moved: {moved:?}"
    );
    assert_eq!(
        moved[0],
        (
            EvidenceState::Verified,
            ProofApplicability::Stale,
            EvidenceState::Stale
        ),
        "the one cell that moves must be verified + lapsed receipt"
    );
}

#[test]
fn unknown_applicability_is_never_staleness() {
    // A receipt whose coverage cannot be established is exactly the case the
    // doctrine refuses to convert into a negative signal. Absence of evidence
    // is `unknown`; it is not "was proven and has decayed".
    for state in STATES {
        assert_eq!(
            applied_evidence_state(state, ProofApplicability::Unknown),
            state,
            "unknown applicability changed {state:?} — staleness is a statement \
             about coverage, not about doubt"
        );
    }
}

#[test]
fn a_lapsed_receipt_neither_softens_nor_manufactures_an_accusation() {
    assert_eq!(
        applied_evidence_state(EvidenceState::Contradicted, ProofApplicability::Stale),
        EvidenceState::Contradicted,
        "a lapsed receipt must not launder a conflicting result into staleness"
    );
    for state in [
        EvidenceState::Unknown,
        EvidenceState::Observed,
        EvidenceState::Supported,
    ] {
        assert_ne!(
            applied_evidence_state(state, ProofApplicability::Stale),
            EvidenceState::Contradicted,
            "applicability must never be able to produce an accusation from {state:?}"
        );
    }
}

#[test]
fn current_applicability_leaves_every_state_alone() {
    for state in STATES {
        assert_eq!(
            applied_evidence_state(state, ProofApplicability::Current),
            state
        );
    }
}

#[test]
fn the_derivation_is_idempotent() {
    // The read surface is a projection: applying it twice must equal applying
    // it once, or a re-derivation could drift from a rebuild.
    for state in STATES {
        for applicability in APPLICABILITIES {
            let once = applied_evidence_state(state, applicability);
            let twice = applied_evidence_state(once, applicability);
            assert_eq!(
                once, twice,
                "not idempotent at {state:?} / {applicability:?}"
            );
        }
    }
}
