//! §5k's permanent Round-7 adjudication ledger (A68).

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Fixture {
    note: String,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    id: String,
    set: String,
    class: String,
    final_true: bool,
    pattern: String,
    expected: String,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!("fixtures/round7-adjudicated.json"))
        .expect("Round-7 pattern ledger parses")
}

#[test]
fn round7_packet_contains_all_95_adjudications_once() {
    let f = fixture();
    assert!(f.note.contains("95 adjudications"));
    assert_eq!(f.cases.len(), 95);
    let ids: BTreeSet<&str> = f.cases.iter().map(|c| c.id.as_str()).collect();
    let patterns: BTreeSet<&str> = f.cases.iter().map(|c| c.pattern.as_str()).collect();
    assert_eq!(ids.len(), 95, "duplicate adjudication id");
    assert_eq!(patterns.len(), 95, "cases collapsed into one pattern");
}

#[test]
fn round7_distribution_and_truth_labels_match_a66() {
    let f = fixture();
    let mut by_class = BTreeMap::new();
    let mut true_by_class = BTreeMap::new();
    for case in &f.cases {
        *by_class.entry(case.class.as_str()).or_insert(0usize) += 1;
        if case.final_true {
            *true_by_class.entry(case.class.as_str()).or_insert(0usize) += 1;
        }
        assert!(!case.expected.is_empty());
    }
    assert_eq!(
        by_class,
        BTreeMap::from([("dropped", 75), ("supported", 10), ("unsupported", 10)])
    );
    assert_eq!(true_by_class, BTreeMap::from([("supported", 9)]));
    assert!(f
        .cases
        .iter()
        .all(|c| c.set == "holdout_c" || c.set == "all_projects"));
}

#[test]
fn packet_is_pattern_only_and_names_round7s_language_failures() {
    let raw = include_str!("fixtures/round7-adjudicated.json").to_ascii_lowercase();
    // Project names of the private corpora this packet was mined from are
    // deliberately not spelled out in this public copy; the shape guards below
    // (home paths, source-file extensions, the product's own name) remain.
    for forbidden in ["/users/", "thruline", ".tsx", ".swift"] {
        assert!(!raw.contains(forbidden), "source token leaked: {forbidden}");
    }
    let f = fixture();
    for family in [
        "interrogative-checklist",
        "future-procedure",
        "imperative",
        "conditional-guidance",
        "uncertain-diagnosis",
        "causal-finding",
        "immediate-action",
        "blocked-attempt",
        "past-tense-completion-status",
        "explicit-deferral",
    ] {
        assert!(
            f.cases.iter().any(|c| c.pattern.contains(family)),
            "missing named Round-7 family: {family}"
        );
    }
}
