//! Family K is a catalog of proposals. Catalog entries never stand in for a
//! person's accepted, scoped decision or a computed verdict.

use std::collections::BTreeSet;

use thruline_core::decisions::kinds_k::{proposed_defaults, KCoverage, KProposalState};

#[test]
fn all_eighteen_defaults_are_distinct_advisory_proposals() {
    let defaults = proposed_defaults();
    let expected = (1..=18).map(|n| format!("K{n}")).collect::<Vec<_>>();
    assert_eq!(
        defaults.iter().map(|entry| entry.code).collect::<Vec<_>>(),
        expected.iter().map(String::as_str).collect::<Vec<_>>()
    );
    assert_eq!(
        defaults
            .iter()
            .map(|entry| entry.code)
            .collect::<BTreeSet<_>>()
            .len(),
        18
    );
    for entry in defaults {
        assert!(!entry.rule.trim().is_empty(), "{} has no rule", entry.code);
        assert!(
            !entry.producing_checks.is_empty(),
            "{} has no source checks",
            entry.code
        );
        assert_eq!(entry.state, KProposalState::ProposedAdvisory);
        assert!(
            entry.accepted_params.is_none(),
            "{} fabricated accepted scope",
            entry.code
        );
        assert!(
            entry.verdict.is_none(),
            "{} fabricated a verdict",
            entry.code
        );
    }
}

#[test]
fn checker_coverage_is_explicit_and_never_claims_a_complete_k_rule() {
    let defaults = proposed_defaults();
    assert!(defaults
        .iter()
        .all(|entry| entry.coverage != KCoverage::Complete));
    for code in [
        "K1", "K2", "K3", "K4", "K5", "K6", "K7", "K8", "K9", "K10", "K11", "K12", "K13", "K14",
        "K15", "K16", "K17",
    ] {
        let entry = defaults.iter().find(|entry| entry.code == code).unwrap();
        assert_eq!(entry.coverage, KCoverage::Partial, "{code}");
        assert!(!entry.coverage_note.trim().is_empty(), "{code}");
    }
    let entry = defaults.iter().find(|entry| entry.code == "K18").unwrap();
    assert_eq!(entry.coverage, KCoverage::Unimplemented, "K18");
    assert!(!entry.coverage_note.trim().is_empty(), "K18");
}
