//! §5k's permanent Round-6 adjudication ledger (A63).
//!
//! This first layer pins the packet itself before M3.15 changes behavior:
//! every real adjudication is present once, the missing requested sample is
//! explicit, and no holdout transcript content entered the repository.
//! Checkpoint-specific behavioral interpreters promote the `pending_until`
//! cases as §5q–§5r land.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Fixture {
    note: String,
    unfilled_sample_slots: Vec<Unfilled>,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Unfilled {
    set: String,
    class: String,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct Case {
    id: String,
    set: String,
    class: String,
    final_true: bool,
    pattern: String,
    expected: String,
    pending_until: Option<String>,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!("fixtures/round6-adjudicated.json"))
        .expect("Round-6 pattern ledger parses")
}

#[test]
fn round6_packet_is_54_real_cases_plus_one_unfilled_slot() {
    let f = fixture();
    assert!(f.note.contains("54 adjudications"));
    assert_eq!(f.cases.len(), 54, "never invent the requested 55th case");
    assert_eq!(f.unfilled_sample_slots.len(), 1);
    let missing = &f.unfilled_sample_slots[0];
    assert_eq!(
        (missing.set.as_str(), missing.class.as_str()),
        ("prior", "supported")
    );
    assert!(missing.reason.contains("nine"));

    let ids: BTreeSet<&str> = f.cases.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids.len(), f.cases.len(), "duplicate ledger id");
    let patterns: BTreeSet<&str> = f.cases.iter().map(|c| c.pattern.as_str()).collect();
    assert_eq!(
        patterns.len(),
        f.cases.len(),
        "two cases collapsed into one pattern"
    );
}

#[test]
fn round6_distribution_and_truth_labels_match_the_measured_packet() {
    let f = fixture();
    let mut distribution: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    let mut true_by_class: BTreeMap<&str, usize> = BTreeMap::new();
    for c in &f.cases {
        *distribution.entry((&c.set, &c.class)).or_default() += 1;
        if c.final_true {
            *true_by_class.entry(&c.class).or_default() += 1;
        }
    }
    assert_eq!(
        distribution,
        BTreeMap::from([
            (("holdout_b", "contradicted"), 7),
            (("holdout_b", "dropped"), 7),
            (("holdout_b", "supported"), 10),
            (("holdout_b", "unsupported"), 10),
            (("prior", "contradicted"), 1),
            (("prior", "supported"), 9),
            (("prior", "unsupported"), 10),
        ])
    );
    assert_eq!(
        true_by_class,
        BTreeMap::from([("supported", 18), ("unsupported", 2)])
    );
}

#[test]
fn fixture_is_pattern_only_and_names_the_must_not_regress_families() {
    let raw = include_str!("fixtures/round6-adjudicated.json").to_ascii_lowercase();
    // Project names of the private corpora this ledger was mined from are
    // deliberately not spelled out in this public copy; the shape guards below
    // (home paths, source-file extensions, the product's own name) remain.
    for forbidden in ["thruline", "/users/", ".tsx", ".swift"] {
        assert!(
            !raw.contains(forbidden),
            "holdout/transcript token leaked into pattern ledger: {forbidden}"
        );
    }

    let f = fixture();
    let mutation = f
        .cases
        .iter()
        .filter(|c| {
            c.class == "contradicted"
                && (c.pattern.contains("mutation")
                    || c.pattern.contains("deliberate-check-failures"))
        })
        .count();
    assert_eq!(mutation, 4, "four mutation-specific shapes must stay named");
    assert!(
        f.cases
            .iter()
            .any(|c| c.pattern == "claim-reports-deliberate-check-failures"),
        "the fifth failure-reporting dialect case is missing"
    );
    assert!(
        f.cases
            .iter()
            .filter(|c| c.class == "dropped")
            .any(|c| c.pattern.contains("status")),
        "status-report ghosts are not named"
    );

    for c in &f.cases {
        assert!(!c.expected.is_empty());
    }

    let unresolved_accusations: BTreeSet<&str> = f
        .cases
        .iter()
        .filter(|c| {
            (c.class == "contradicted" || c.class == "dropped") && c.pending_until.is_some()
        })
        .map(|c| c.id.as_str())
        .collect();
    assert!(
        unresolved_accusations.is_empty(),
        "M3.15.2 must promote the two deferred same-turn/agent-record shapes: \
         {unresolved_accusations:?}"
    );
    for c in f.cases.iter().filter(|c| c.class == "contradicted") {
        assert!(
            c.pending_until.is_none(),
            "{} was not promoted with the §5q checkpoint",
            c.id
        );
    }
}
