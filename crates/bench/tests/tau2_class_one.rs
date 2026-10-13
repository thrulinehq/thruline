//! §12 class 1 against τ2-bench, and the measurement that decides whether a
//! holdout number over it means anything.
//!
//! # What this establishes
//!
//! τ2-bench **is** class 1: `sierra-research/tau2-bench` publishes runs Sierra
//! performed, each carrying the benchmark's own `reward_info`. Third-party
//! trajectories, third-party oracle. The loader here reads them, pinned to the
//! sha256 recorded at fetch.
//!
//! And then it measures the thing that has to be measured before any number is
//! quoted: **does this corpus contain the evidence ThruLine's detectors read?**
//! Every §6 verifier — `tests_pass`, `build_ok`, `lint_ok`, `typecheck_ok`,
//! `behavior_unchanged` — decides from *executed code checks bound to a git
//! tree*. A corpus without those does not exercise them, and an AUROC over it
//! would be a number about a detector that never ran.
//!
//! That is a measurement here, not an assertion.

use std::path::PathBuf;

use thruline_bench::normalize::Corpus;
use thruline_bench::{tau2, Ground, Label, Provenance};

const AIRLINE: (&str, &str) = (
    "o4-mini-2025-04-16_airline_default_gpt-4.1-2025-04-14_4trials.json",
    "6a3a354c9bce42b52905af096e9cea6f555548acbb17b567fd403c356df7b4ec",
);
const TELECOM: (&str, &str) = (
    "o4-mini-2025-04-16_telecom_no-user-op_gpt-4.1-2025-04-14_4trials.json",
    "e4f0e4827bdd36438f6daf80ceecffe7f9fd175cef80ae1f821422cb4e5743c4",
);

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/bench/tau2")
}

fn load(file: (&str, &str)) -> Corpus {
    tau2::load(&fixtures().join(file.0), file.1)
        .unwrap_or_else(|e| panic!("{e}\n\nRun `fixtures/bench/download.sh` to fetch the corpus."))
}

/// The digest is the class-1 evidence, and a mismatch is a refusal.
#[test]
#[ignore = "requires the fetched τ2-bench corpus. Run fixtures/bench/download.sh."]
fn the_artifact_digest_is_required_and_checked() {
    let ok = load(AIRLINE);
    assert_eq!(ok.provenance.class(), 1);
    assert_eq!(ok.artifact_digest.as_deref(), Some(AIRLINE.1));

    // One bit off and it is not the published corpus.
    let wrong = "0".repeat(64);
    let err = tau2::load(&fixtures().join(AIRLINE.0), &wrong).expect_err("must refuse");
    let text = err.to_string();
    assert!(text.contains("not the published corpus"), "{text}");
}

/// The oracle labels, and nothing else, decide the ground truth.
#[test]
#[ignore = "requires the fetched τ2-bench corpus."]
fn labels_come_from_the_reward_function() {
    let c = load(AIRLINE);
    assert!(c.trajectories.len() > 100, "{}", c.trajectories.len());
    let comp = c.composition();
    eprintln!("airline composition: {comp:?}");
    // Every label is one the oracle produced or is explicitly unlabelled.
    for t in &c.trajectories {
        match &t.label {
            Ground::Labelled(r) => {
                assert!(
                    matches!(r.label(), Label::HonestPass | Label::FalseSuccess),
                    "a label τ2's oracle cannot produce: {r:?}"
                );
                // The reading names the field it came from, and it is τ2's.
                assert_eq!(r.field(), "reward_info.reward");
                assert!(
                    r.raw().parse::<f64>().is_ok(),
                    "a reward that is not a number: {}",
                    r.raw()
                );
            }
            Ground::Unlabelled { reason } => {
                assert!(reason.contains("reward"), "{reason}");
            }
        }
    }
    // The `Provenance` names the field a reviewer would check.
    match &c.provenance {
        Provenance::ExternalLabelled(src) => {
            assert_eq!(src.publisher(), "sierra-research");
            assert_eq!(src.label_field(), "reward_info.reward");
        }
        other => panic!("{other:?}"),
    }
}

/// The holdout is derived from the artifact's digest, so it is reproducible and
/// was fixed by the publisher rather than chosen here.
#[test]
#[ignore = "requires the fetched τ2-bench corpus."]
fn the_holdout_is_a_function_of_the_artifact() {
    let c = load(AIRLINE);
    let a = tau2::holdout(&c, 0.5);
    let b = tau2::holdout(&c, 0.5);
    assert_eq!(a, b, "the split is not reproducible");
    assert!(!a.is_empty());
    let all: std::collections::BTreeSet<String> = c
        .trajectories
        .iter()
        .filter_map(|t| t.task_id.clone())
        .collect();
    assert!(a.len() < all.len(), "the holdout is everything");
    // A different fraction gives a different split, and the split is nested —
    // a smaller holdout is a subset of a larger one, so nobody can widen it
    // after seeing results and call it the same rule.
    let quarter = tau2::holdout(&c, 0.25);
    assert!(
        quarter.iter().all(|t| a.contains(t)),
        "the split is not nested"
    );
}

/// **The measurement that decides whether a §12 number over this corpus means
/// anything: does it contain executed code checks at all?**
#[test]
#[ignore = "requires the fetched τ2-bench corpus."]
fn tau2_carries_no_evidence_that_thrulines_verifiers_read() {
    let mut totals = (0usize, 0usize, 0usize);
    for file in [AIRLINE, TELECOM] {
        let c = load(file);
        let census = tau2::evidence_census(&c);
        eprintln!(
            "{}: {} trajectories, {} tool calls, {} distinct tools, {} code-check executions",
            file.0,
            census.trajectories,
            census.tool_calls,
            census.tools.len(),
            census.code_check_executions
        );
        let mut names: Vec<(&String, &usize)> = census.tools.iter().collect();
        names.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
        eprintln!(
            "  tools: {}",
            names
                .iter()
                .take(8)
                .map(|(n, c)| format!("{n}×{c}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        totals.0 += census.trajectories;
        totals.1 += census.tool_calls;
        totals.2 += census.code_check_executions;
        if !census.near_misses.is_empty() {
            let mut nm: Vec<(&String, &usize)> = census.near_misses.iter().collect();
            nm.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
            eprintln!(
                "  near misses (execution-sounding names that are fixed domain APIs): {}",
                nm.iter()
                    .map(|(n, c)| format!("{n}×{c}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    eprintln!(
        "TOTAL: {} trajectories, {} tool calls, {} code-check executions",
        totals.0, totals.1, totals.2
    );
    assert!(totals.1 > 1_000, "the census found almost no tool calls");
    // The finding, asserted so it is a measurement and so a future corpus that
    // *does* carry executions makes this test fail and demand attention.
    assert_eq!(
        totals.2, 0,
        "τ2-bench now contains code-check executions — §6's verifiers may apply \
         to it after all, and the conclusion recorded in REPORT.md needs redoing"
    );
}
