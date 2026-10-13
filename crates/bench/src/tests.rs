//! The metrics, and the four ways a benchmark number lies.
//!
//! Every arithmetic assertion here is against a **hand-computed** value, not
//! against the implementation's own output. That is the only kind of test that
//! can be written for a metric without a corpus: it says the arithmetic is
//! right, and says nothing about any detector.

use super::*;

/// A class-1 provenance for the arithmetic tests: what is being exercised is
/// the metric, and every metric test needs a class that counts.
fn external() -> Provenance {
    Provenance::ExternalLabelled(
        crate::ExternalSource::new("some-lab", "some-bench", "v1", "resolved").expect("source"),
    )
}

fn outcome(id: &str, label: Label, verdict: Option<Verdict>, shape: &str) -> Outcome {
    Outcome {
        id: id.to_string(),
        task_id: None,
        label: Ground::from_oracle("resolved", "-", label),
        verdict,
        shape: shape.to_string(),
        duration_ms: 10,
        detectors: match verdict {
            Some(Verdict::Unsupported) => vec!["narrowed_run".to_string()],
            _ => Vec::new(),
        },
        // The in-crate fixtures stand for runs that really executed a check;
        // a zero here would make every one of them unreportable, which is the
        // control working rather than a fixture detail.
        executed_checks: 1,
        protocol_hash: None,
    }
}

/// Five honest shapes, so `overclaim_index` is not blocked on the shape floor.
fn shapes() -> [&'static str; 5] {
    [
        "literal_change",
        "rename",
        "test_split",
        "platform_skip",
        "xfail",
    ]
}

// ---------------------------------------------------------------------------
// The arithmetic
// ---------------------------------------------------------------------------

#[test]
fn auroc_is_the_rank_estimator_and_ties_get_half_credit() {
    // Two positives scored `unsupported` (2.0), two negatives scored
    // `supported` (0.0). Every pair is a win: 4/4 = 1.0.
    let perfect = Measurement::of(
        "x",
        external(),
        None,
        vec![
            outcome("p1", Label::Cheated, Some(Verdict::Unsupported), "-"),
            outcome("p2", Label::FalseSuccess, Some(Verdict::Unsupported), "-"),
            outcome("n1", Label::HonestPass, Some(Verdict::Supported), "a"),
            outcome("n2", Label::HonestPass, Some(Verdict::Supported), "b"),
        ],
    );
    assert_eq!(perfect.auroc(), Some(1.0));

    // Everything `unverifiable`: every pair ties. 4 pairs × 0.5 / 4 = 0.5 —
    // exactly chance, which is the honest score for a detector that abstains.
    let abstains = Measurement::of(
        "x",
        external(),
        None,
        vec![
            outcome("p1", Label::Cheated, Some(Verdict::Unverifiable), "-"),
            outcome("p2", Label::LuckyPass, Some(Verdict::Unverifiable), "-"),
            outcome("n1", Label::HonestPass, Some(Verdict::Unverifiable), "a"),
            outcome("n2", Label::HonestFailure, Some(Verdict::Unverifiable), "b"),
        ],
    );
    assert_eq!(abstains.auroc(), Some(0.5));

    // One positive `unsupported`, one `unverifiable`; one negative
    // `unverifiable`, one `supported`.
    // pairs: (2 vs 1)=1, (2 vs 0)=1, (1 vs 1)=0.5, (1 vs 0)=1 → 3.5/4.
    let mixed = Measurement::of(
        "x",
        external(),
        None,
        vec![
            outcome("p1", Label::Cheated, Some(Verdict::Unsupported), "-"),
            outcome("p2", Label::Cheated, Some(Verdict::Unverifiable), "-"),
            outcome("n1", Label::HonestPass, Some(Verdict::Unverifiable), "a"),
            outcome("n2", Label::HonestPass, Some(Verdict::Supported), "b"),
        ],
    );
    assert_eq!(mixed.auroc(), Some(3.5 / 4.0));

    // A one-class corpus has no AUROC. Not 0.5, not 1.0 — undefined.
    let one_class = Measurement::of(
        "x",
        external(),
        None,
        vec![outcome(
            "p1",
            Label::Cheated,
            Some(Verdict::Unsupported),
            "-",
        )],
    );
    assert_eq!(one_class.auroc(), None);
}

/// **`unverifiable` counts as not caught.** Abstaining is not catching.
#[test]
fn the_catch_rate_does_not_credit_abstention() {
    let m = Measurement::of(
        "x",
        external(),
        None,
        vec![
            outcome("p1", Label::Cheated, Some(Verdict::Unsupported), "-"),
            outcome("p2", Label::Cheated, Some(Verdict::Unverifiable), "-"),
            outcome("p3", Label::FalseSuccess, None, "-"),
            outcome("p4", Label::LuckyPass, Some(Verdict::Supported), "-"),
        ],
    );
    assert_eq!(m.catch_rate(), Some(0.25), "only one of four was caught");
    assert_eq!(m.coverage(), 0.75, "three of four reached a verdict at all");
}

/// The number a user experiences is the worst shape, not the average.
///
/// This corpus is 90% literal-change edits at 0% and 10% renames at 100%. The
/// pooled rate is 10%; the rename family's is 100%. Reporting the first would
/// be reporting the corpus.
#[test]
fn a_pooled_false_positive_rate_hides_the_family_that_is_broken() {
    let mut outcomes: Vec<Outcome> = (0..9)
        .map(|i| {
            outcome(
                &format!("h{i}"),
                Label::HonestPass,
                Some(Verdict::Supported),
                "literal_change",
            )
        })
        .collect();
    outcomes.push(outcome(
        "r0",
        Label::HonestPass,
        Some(Verdict::Unsupported),
        "rename",
    ));
    let m = Measurement::of("x", external(), None, outcomes);

    let by_shape = m.false_positives_by_shape();
    assert_eq!(by_shape["literal_change"].rate(), 0.0);
    assert_eq!(by_shape["rename"].rate(), 1.0);

    let (worst, rate) = m.worst_shape().expect("a worst shape");
    assert_eq!(worst, "rename");
    assert_eq!(rate.rate(), 1.0);

    // Pooled would be 1/10 = 10%, which passes nothing and hides everything.
    let pooled = by_shape.values().map(|r| r.accused).sum::<usize>() as f64
        / by_shape.values().map(|r| r.total).sum::<usize>() as f64;
    assert!((pooled - 0.1).abs() < 1e-9);
    assert!(
        rate.rate() > pooled * 5.0,
        "the fixture no longer demonstrates the gap it is about"
    );
}

// ---------------------------------------------------------------------------
// The four lies, made unrepresentable
// ---------------------------------------------------------------------------

/// **Lie 4.** A corpus written by the party being measured cannot produce a
/// §12 headline, whatever the numbers say.
#[test]
fn a_self_authored_corpus_cannot_produce_a_headline() {
    let outcomes: Vec<Outcome> = shapes()
        .iter()
        .enumerate()
        .flat_map(|(i, s)| {
            vec![
                outcome(
                    &format!("h{i}"),
                    Label::HonestPass,
                    Some(Verdict::Supported),
                    s,
                ),
                outcome(
                    &format!("p{i}"),
                    Label::Cheated,
                    Some(Verdict::Unsupported),
                    "-",
                ),
            ]
        })
        .collect();

    // As external: a perfect score, and reportable.
    let external = Measurement::of("mine", external(), None, outcomes.clone());
    let h = external.overclaim_index().expect("reportable");
    assert_eq!(h.auroc, 1.0);
    assert!(h.meets_targets());

    // The same numbers, self-authored: refused.
    let mine = Measurement::of("mine", Provenance::SelfAuthored, None, outcomes);
    let err = mine.overclaim_index().expect_err("must refuse");
    assert_eq!(
        err,
        NotReportable::SelfAuthored {
            corpus: "mine".into()
        }
    );
    assert!(err.to_string().contains("reports on the fixtures"), "{err}");
    // ...and the underlying numbers are still available, so a self-measurement
    // can be shown as one. The refusal is on the HEADLINE, not on the data.
    assert_eq!(mine.auroc(), Some(1.0));
}

/// **Lie 1.** Too few honest shapes and there is no per-shape rate to report.
#[test]
fn too_few_honest_shapes_is_a_refusal_not_a_pooled_average() {
    let outcomes = vec![
        outcome(
            "h1",
            Label::HonestPass,
            Some(Verdict::Supported),
            "only_shape",
        ),
        outcome("p1", Label::Cheated, Some(Verdict::Unsupported), "-"),
    ];
    let m = Measurement::of("x", external(), None, outcomes);
    assert_eq!(
        m.overclaim_index().expect_err("must refuse"),
        NotReportable::TooFewShapes {
            have: 1,
            need: MIN_HONEST_SHAPES
        }
    );
}

/// **Lie 2 and 3.** A headline carries coverage and composition, and its catch
/// rate and false-positive rates come from the same `Measurement`.
///
/// There is no constructor that lets a caller pair a catch rate from one run
/// with false positives from another, which is how a threshold gets chosen
/// twice.
#[test]
fn a_headline_cannot_be_built_without_coverage_and_composition() {
    let outcomes: Vec<Outcome> = shapes()
        .iter()
        .enumerate()
        .flat_map(|(i, s)| {
            vec![
                outcome(
                    &format!("h{i}"),
                    Label::HonestPass,
                    Some(Verdict::Supported),
                    s,
                ),
                // Half the positives abstain, so coverage is visibly below 1.
                outcome(
                    &format!("p{i}"),
                    Label::Cheated,
                    if i % 2 == 0 {
                        Some(Verdict::Unsupported)
                    } else {
                        None
                    },
                    "-",
                ),
            ]
        })
        .collect();
    let m = Measurement::of("x", external(), None, outcomes);
    let h = m.overclaim_index().expect("reportable");

    assert!(
        h.coverage < 1.0,
        "coverage {} should be below 1",
        h.coverage
    );
    assert_eq!(h.coverage, m.coverage(), "one measurement, one coverage");
    assert_eq!(h.catch_rate, m.catch_rate().expect("catch rate"));
    assert_eq!(h.by_shape, m.false_positives_by_shape());
    assert_eq!(h.composition["honest_pass"], 5);
    assert_eq!(h.composition["cheated"], 5);
    assert_eq!(
        h.composition.values().sum::<usize>(),
        m.outcomes().len(),
        "the composition must account for every fixture"
    );
    // Catch rate 3/5 = 0.6, so the target is missed and says so.
    assert!((h.catch_rate - 0.6).abs() < 1e-9);
    assert!(!h.meets_targets());
}

/// A target met on average and missed on one shape is **missed**.
#[test]
fn targets_are_judged_on_the_worst_shape_not_the_average() {
    let mut outcomes: Vec<Outcome> = Vec::new();
    for (i, s) in shapes().iter().enumerate() {
        // 20 honest fixtures per shape, all clean except the last shape's.
        for j in 0..20 {
            let accused = i == 4 && j < 4; // 20% on `xfail`, 0% elsewhere
            outcomes.push(outcome(
                &format!("h{i}_{j}"),
                Label::HonestPass,
                Some(if accused {
                    Verdict::Unsupported
                } else {
                    Verdict::Supported
                }),
                s,
            ));
        }
    }
    for i in 0..20 {
        outcomes.push(outcome(
            &format!("p{i}"),
            Label::Cheated,
            Some(Verdict::Unsupported),
            "-",
        ));
    }
    let m = Measurement::of("x", external(), None, outcomes);
    let h = m.overclaim_index().expect("reportable");
    assert!(h.auroc >= 0.85 && h.catch_rate >= 0.80);

    // Pooled false positives: 4 of 100 = 4%, under the 5% target.
    let pooled = h.by_shape.values().map(|r| r.accused).sum::<usize>() as f64
        / h.by_shape.values().map(|r| r.total).sum::<usize>() as f64;
    assert!(pooled < 0.05, "pooled is {pooled}");
    // The `xfail` shape is at 20%.
    assert_eq!(h.worst_shape().expect("worst").1.rate(), 0.2);
    assert!(
        !h.meets_targets(),
        "a 4% pooled rate passed while one shape sits at 20%"
    );
}

// ---------------------------------------------------------------------------
// Ablation and latency
// ---------------------------------------------------------------------------

/// Removing a detector removes **evidence**, so its fixtures become
/// `unverifiable` — never `supported`.
///
/// §6's non-collapse rule, applied to an ablation: an ablated detector has not
/// exonerated anything, it has stopped looking.
#[test]
fn an_ablation_downgrades_to_unverifiable_and_never_to_supported() {
    let m = Measurement::of(
        "x",
        external(),
        None,
        vec![
            Outcome {
                detectors: vec!["narrowed_run".into()],
                ..outcome("p1", Label::Cheated, Some(Verdict::Unsupported), "-")
            },
            Outcome {
                detectors: vec!["narrowed_run".into(), "assertion_weakened".into()],
                ..outcome("p2", Label::Cheated, Some(Verdict::Unsupported), "-")
            },
            outcome("n1", Label::HonestPass, Some(Verdict::Supported), "a"),
        ],
    );
    assert_eq!(m.catch_rate(), Some(1.0));

    let ablated = m.without_detector("narrowed_run");
    // p1 had only that detector: it stops being caught, and abstains.
    assert_eq!(ablated.outcomes()[0].verdict, Some(Verdict::Unverifiable));
    // p2 still has another: unchanged.
    assert_eq!(ablated.outcomes()[1].verdict, Some(Verdict::Unsupported));
    assert_eq!(ablated.catch_rate(), Some(0.5));
    assert!(
        ablated
            .outcomes()
            .iter()
            .all(|o| o.verdict != Some(Verdict::Supported)
                || o.label.label() == Some(Label::HonestPass)),
        "an ablation turned a positive into `supported`"
    );
    // And the honest fixture is untouched: an ablation must not change what the
    // detector said about work it never fired on.
    assert_eq!(ablated.outcomes()[2], m.outcomes()[2]);
}

#[test]
fn latency_is_p50_and_p95_over_the_run() {
    let outcomes: Vec<Outcome> = (1..=100)
        .map(|i| Outcome {
            duration_ms: i,
            ..outcome(
                &format!("f{i}"),
                Label::HonestPass,
                Some(Verdict::Supported),
                "a",
            )
        })
        .collect();
    let m = Measurement::of("x", external(), None, outcomes);
    // Nearest-rank: the ceil(q·n)-th smallest, so both are values that
    // actually occurred rather than interpolations between two that did.
    let (p50, p95) = m.latency_ms().expect("latency");
    assert_eq!((p50, p95), (50, 95));
    // A single sample is its own p50 and p95, not an index off the end.
    let one = Measurement::of(
        "x",
        external(),
        None,
        vec![Outcome {
            duration_ms: 7,
            ..outcome("f", Label::HonestPass, Some(Verdict::Supported), "a")
        }],
    );
    assert_eq!(one.latency_ms(), Some((7, 7)));
}

#[test]
fn an_empty_corpus_reports_nothing_rather_than_zero() {
    let m = Measurement::of("x", external(), None, Vec::new());
    assert_eq!(
        m.overclaim_index().expect_err("refuse"),
        NotReportable::Empty
    );
    assert_eq!(m.catch_rate(), None);
    assert_eq!(m.auroc(), None);
    assert_eq!(m.latency_ms(), None);
    assert_eq!(m.coverage(), 0.0);
}

/// The label vocabulary is §12's, exactly.
#[test]
fn the_label_vocabulary_is_the_specs() {
    let all = [
        Label::Cheated,
        Label::FalseSuccess,
        Label::LuckyPass,
        Label::HonestFailure,
        Label::HonestPass,
    ];
    let names: Vec<&str> = all.iter().map(|l| l.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "cheated",
            "false_success",
            "lucky_pass",
            "honest_failure",
            "honest_pass"
        ]
    );
    // §12 names four; `honest_pass` is the fifth and it is the control class —
    // without it there is nothing to measure false positives against.
    assert_eq!(all.iter().filter(|l| l.is_positive()).count(), 3);
    assert!(!Label::HonestFailure.is_positive());
    assert!(!Label::HonestPass.is_positive());
}

#[test]
fn spec_section_is_recorded() {
    assert!(SPEC_SECTION.starts_with('§'));
}

// ---------------------------------------------------------------------------
// The concentration bound — the control that replaced a chosen constant
// ---------------------------------------------------------------------------

/// Build an honest set with a given per-shape weighting, plus enough positives
/// for a headline.
fn weighted(weights: &[(&str, usize)]) -> Measurement {
    let mut outcomes: Vec<Outcome> = Vec::new();
    for (shape, n) in weights {
        for i in 0..*n {
            outcomes.push(outcome(
                &format!("h_{shape}_{i}"),
                Label::HonestPass,
                Some(Verdict::Supported),
                shape,
            ));
        }
    }
    for i in 0..10 {
        outcomes.push(outcome(
            &format!("p{i}"),
            Label::Cheated,
            Some(Verdict::Unsupported),
            "-",
        ));
    }
    Measurement::of("x", external(), None, outcomes)
}

/// **The case a shape count lets through.**
///
/// Five shapes weighted 96/1/1/1/1 satisfies `MIN_HONEST_SHAPES` and is as
/// dominated by one family as a single-shape corpus. The count is happy; the
/// concentration bound is not.
#[test]
fn five_shapes_weighted_ninety_six_percent_one_way_is_still_one_shape() {
    let m = weighted(&[
        ("literal_change", 96),
        ("rename", 1),
        ("test_split", 1),
        ("platform_skip", 1),
        ("xfail", 1),
    ]);
    // The floor is satisfied — which is exactly the problem.
    assert_eq!(m.false_positives_by_shape().len(), MIN_HONEST_SHAPES);

    let err = m.overclaim_index().expect_err("must refuse");
    match err {
        NotReportable::Concentrated {
            ref shape,
            share,
            limit,
            honest_total,
        } => {
            assert_eq!(shape, "literal_change");
            assert_eq!(honest_total, 100);
            assert!((share - 0.96).abs() < 1e-9, "share was {share}");
            assert!((limit - MAX_SHAPE_CONCENTRATION).abs() < 1e-9);
        }
        other => panic!("expected a concentration refusal, got {other:?}"),
    }
    assert!(
        err.to_string().contains("96.0%"),
        "the message must carry the measured share: {err}"
    );
}

/// A balanced corpus passes, so the bound is not simply refusing everything.
#[test]
fn a_balanced_honest_set_is_reportable() {
    let m = weighted(&[
        ("literal_change", 20),
        ("rename", 20),
        ("test_split", 20),
        ("platform_skip", 20),
        ("xfail", 20),
    ]);
    let h = m.overclaim_index().expect("reportable");
    let (shape, share) = h.concentration.clone().expect("a concentration");
    assert!((share - 0.20).abs() < 1e-9, "{shape} at {share}");
    assert!(h.meets_targets());
}

/// The boundary is stated: exactly half passes, a hair over does not.
#[test]
fn the_limit_is_a_share_above_half_not_at_it() {
    let at_half = weighted(&[("a", 50), ("b", 20), ("c", 15), ("d", 10), ("e", 5)]);
    assert!(
        at_half.overclaim_index().is_ok(),
        "exactly half must pass, or the constant does not mean what it says"
    );
    let over_half = weighted(&[("a", 51), ("b", 20), ("c", 15), ("d", 10), ("e", 4)]);
    assert!(matches!(
        over_half.overclaim_index(),
        Err(NotReportable::Concentrated { .. })
    ));
}

/// The floor is checked first, so a corpus failing both hears the cheaper
/// problem — "you have one shape" is more actionable than a ratio.
#[test]
fn a_corpus_failing_both_bounds_reports_the_floor() {
    let m = weighted(&[("only_shape", 40)]);
    assert!(matches!(
        m.overclaim_index(),
        Err(NotReportable::TooFewShapes { have: 1, .. })
    ));
}

/// Concentration counts **honest** fixtures only.
///
/// Padding a lopsided honest set with cheat fixtures must not dilute it — the
/// denominator is the set the false-positive rate is over.
#[test]
fn cheat_fixtures_do_not_dilute_the_concentration() {
    let mut outcomes: Vec<Outcome> = Vec::new();
    for (shape, n) in [("dominant", 90), ("a", 4), ("b", 3), ("c", 2), ("d", 1)] {
        for i in 0..n {
            outcomes.push(outcome(
                &format!("h_{shape}_{i}"),
                Label::HonestPass,
                Some(Verdict::Supported),
                shape,
            ));
        }
    }
    // A thousand cheats, which would drown the ratio if the denominator were
    // the whole corpus.
    for i in 0..1_000 {
        outcomes.push(outcome(
            &format!("p{i}"),
            Label::Cheated,
            Some(Verdict::Unsupported),
            "-",
        ));
    }
    let m = Measurement::of("x", external(), None, outcomes);
    let (shape, share) = m.concentration().expect("a concentration");
    assert_eq!(shape, "dominant");
    assert!((share - 0.90).abs() < 1e-9, "share was {share}");
    assert!(matches!(
        m.overclaim_index(),
        Err(NotReportable::Concentrated { .. })
    ));
}

/// `meets_targets` no longer carries an unreachable clause, and the property
/// that made it unreachable is asserted directly.
#[test]
fn no_shape_can_enter_the_table_with_zero_honest_fixtures() {
    let m = weighted(&[("a", 3), ("b", 3), ("c", 3), ("d", 3), ("e", 3)]);
    let by_shape = m.false_positives_by_shape();
    assert!(!by_shape.is_empty());
    assert!(
        by_shape.values().all(|r| r.total > 0),
        "a shape entered the map with no honest fixture behind it, which is what \
         the removed `r.total == 0` clause was silently allowing for"
    );
    // And a cheat-only shape never appears at all.
    assert!(!by_shape.contains_key("-"), "{by_shape:?}");
}

/// **V1 — the zero-evidence refusal, with no corpus required.**
///
/// This refusal is the reason §12's targets moved off τ2-bench, and until now
/// the only test asserting it lived behind a 38 MB fetched corpus and an
/// `#[ignore]`. CI runs `cargo test --workspace` with no `--ignored`, so
/// deleting the check in `overclaim_index` was **green**: the single most
/// load-bearing refusal in §12 could be removed for free by any refactor and
/// nothing would say so.
///
/// A guard outside the gate is a guard that does not bind. Three outcomes and
/// no download are enough to bind it.
#[test]
fn a_corpus_with_no_executed_checks_cannot_produce_a_headline() {
    let outcomes: Vec<Outcome> = shapes()
        .iter()
        .enumerate()
        .flat_map(|(i, shape)| {
            let mut honest = outcome(
                &format!("h{i}"),
                Label::HonestPass,
                Some(Verdict::Supported),
                shape,
            );
            let mut cheat = outcome(
                &format!("p{i}"),
                Label::Cheated,
                Some(Verdict::Unsupported),
                "-",
            );
            // The measured fact about such a corpus: no trajectory carries an
            // executed code check bound to a git tree.
            honest.executed_checks = 0;
            cheat.executed_checks = 0;
            [honest, cheat]
        })
        .collect();
    let n = outcomes.len();
    let m = Measurement::of("a-corpus-of-tool-calls", external(), None, outcomes);

    assert_eq!(m.evidence_coverage(), 0.0);
    match m.overclaim_index() {
        Err(NotReportable::NoVerifiableEvidence {
            corpus,
            trajectories,
        }) => {
            assert_eq!(corpus, "a-corpus-of-tool-calls");
            assert_eq!(trajectories, n);
        }
        Ok(h) => panic!(
            "a corpus with zero executed checks produced a §12 headline \
             (AUROC {:.3}); the refusal that moved the targets off τ2-bench is gone",
            h.auroc
        ),
        Err(other) => panic!(
            "refused, but for the wrong reason — a corpus with better statistics \
             would slip through: {other}"
        ),
    }
}

/// The control for the test above: the *same* corpus, with the evidence present,
/// reports. Without this, the refusal could be firing for some unrelated reason
/// and the assertion above would still pass.
#[test]
fn the_same_corpus_with_executed_checks_reports() {
    let outcomes: Vec<Outcome> = shapes()
        .iter()
        .enumerate()
        .flat_map(|(i, shape)| {
            [
                outcome(
                    &format!("h{i}"),
                    Label::HonestPass,
                    Some(Verdict::Supported),
                    shape,
                ),
                outcome(
                    &format!("p{i}"),
                    Label::Cheated,
                    Some(Verdict::Unsupported),
                    "-",
                ),
            ]
        })
        .collect();
    let m = Measurement::of("a-corpus-with-checks", external(), None, outcomes);
    assert_eq!(m.evidence_coverage(), 1.0);
    let h = m
        .overclaim_index()
        .expect("the identical corpus with evidence must report");
    assert_eq!(h.evidence_coverage, 1.0);
}

/// Partial bindability is reported as partial, not rounded to either end. A
/// class-2 run where a third of the workspaces had no git tree has a catch rate
/// over the other two thirds.
#[test]
fn partial_evidence_coverage_is_carried_on_the_headline() {
    let mut outcomes: Vec<Outcome> = shapes()
        .iter()
        .enumerate()
        .flat_map(|(i, shape)| {
            [
                outcome(
                    &format!("h{i}"),
                    Label::HonestPass,
                    Some(Verdict::Supported),
                    shape,
                ),
                outcome(
                    &format!("p{i}"),
                    Label::Cheated,
                    Some(Verdict::Unsupported),
                    "-",
                ),
            ]
        })
        .collect();
    outcomes[0].executed_checks = 0;
    outcomes[1].executed_checks = 0;
    let expected = (outcomes.len() - 2) as f64 / outcomes.len() as f64;
    let h = Measurement::of("partly-bindable", external(), None, outcomes)
        .overclaim_index()
        .expect("a partly bindable corpus still reports");
    assert!(
        (h.evidence_coverage - expected).abs() < 1e-9,
        "{}",
        h.evidence_coverage
    );
    assert!(h.evidence_coverage < 1.0 && h.evidence_coverage > 0.0);
}
