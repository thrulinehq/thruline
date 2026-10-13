//! **Appendix, never headline.** τ2-bench scoped to the claim extractor.
//!
//! # Why this corpus is here at all, and what it may be used for
//!
//! τ2-bench is genuine class 1 — third-party trajectories, third-party labels —
//! and it contains **zero executed code checks**, which is measured in
//! `tau2_class_one.rs`. §6's verifiers therefore cannot be measured on it, and
//! its coverage for them is zero. That finding stands and stays recorded.
//!
//! But it has 656 real agent trajectories and 4,993 real tool calls, and §5's
//! **claim extractor** reads exactly that: agent prose. So the corpus measures
//! one component of this system honestly, and this file is that measurement,
//! scoped in the same breath as its result.
//!
//! # What can and cannot be computed here, stated before the numbers
//!
//! Precision and recall of the extractor need labels for **whether a completion
//! claim was made**. Nobody publishes those; τ2 labels whether the *task*
//! succeeded. Reporting a precision figure against the wrong ground truth would
//! be the same error as reporting an AUROC over a corpus with no executed
//! checks — a number about something other than what it names.
//!
//! What the corpus does support is an **association**: if the extractor works,
//! completion claims should concentrate on oracle-passed trajectories and
//! honest-failure markers on oracle-failed ones. If it were noise the two
//! distributions would be identical. That is falsifiable, needs no hand labels,
//! and is reported as what it is.
//!
//! Run with `--ignored` after `fixtures/bench/download.sh`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use thruline_bench::{tau2, Label};
use thruline_claims::extract::{extract, Source};
use thruline_claims::rules::Table;

const AIRLINE: (&str, &str) = (
    "o4-mini-2025-04-16_airline_default_gpt-4.1-2025-04-14_4trials.json",
    "6a3a354c9bce42b52905af096e9cea6f555548acbb17b567fd403c356df7b4ec",
);

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/bench/tau2")
}

/// The last assistant message of a simulation — where a completion claim lives,
/// the same place `claims_cmd` reads it from in a real session.
fn final_assistant_message(raw: &serde_json::Value) -> Option<String> {
    let msgs = raw.get("messages")?.as_array()?;
    msgs.iter()
        .rev()
        .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("assistant"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .map(str::to_string)
}

#[test]
#[ignore = "requires the fetched τ2-bench corpus."]
fn the_claim_extractor_over_real_agent_prose() {
    let corpus = tau2::load(&fixtures().join(AIRLINE.0), AIRLINE.1)
        .unwrap_or_else(|e| panic!("{e}\n\nRun `fixtures/bench/download.sh`."));
    let table = Table::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .expect("the claim pattern table");

    // rows: oracle label -> (with a completion claim, without, no final message)
    let mut cross: BTreeMap<&'static str, [usize; 3]> = BTreeMap::new();
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut negated = 0usize;
    let mut promises = 0usize;

    for t in &corpus.trajectories {
        let key = match t.label.label() {
            Some(Label::HonestPass) => "oracle: passed",
            Some(Label::FalseSuccess) => "oracle: failed",
            _ => "oracle: unlabelled",
        };
        let row = cross.entry(key).or_insert([0, 0, 0]);
        let Some(text) = final_assistant_message(&t.raw) else {
            row[2] += 1;
            continue;
        };
        let e = extract(
            &[Source {
                event_id: t.id.clone(),
                session_id: t.id.clone(),
                text,
            }],
            &table,
        );
        negated += e.negated.len();
        promises += e.promises.len();
        if e.claims.is_empty() {
            row[1] += 1;
        } else {
            row[0] += 1;
            for c in &e.claims {
                *kinds.entry(c.kind.as_str().to_string()).or_insert(0) += 1;
            }
        }
    }

    eprintln!("\nτ2-bench airline — CLAIM EXTRACTOR ONLY (appendix; not a §12 headline)");
    eprintln!("  oracle label          claim    no claim   no final message");
    for (k, [with, without, none]) in &cross {
        eprintln!("  {k:<20} {with:>6} {without:>10} {none:>18}");
    }
    eprintln!("  claim kinds extracted: {kinds:?}");
    eprintln!("  negated (matched then refused): {negated}");
    eprintln!("  promises extracted: {promises}");

    let passed = cross.get("oracle: passed").copied().unwrap_or([0; 3]);
    let failed = cross.get("oracle: failed").copied().unwrap_or([0; 3]);
    let rate = |r: [usize; 3]| {
        let seen = r[0] + r[1];
        if seen == 0 {
            None
        } else {
            Some(r[0] as f64 / seen as f64)
        }
    };
    match (rate(passed), rate(failed)) {
        (Some(p), Some(f)) => {
            eprintln!("  completion-claim rate on oracle-passed: {:.3}", p);
            eprintln!("  completion-claim rate on oracle-failed: {:.3}", f);
            eprintln!(
                "  difference: {:+.3}  (the association; zero is the null)",
                p - f
            );
        }
        _ => eprintln!("  one of the two classes had no readable final message"),
    }

    // **The positive control, and it is load-bearing.** A rate of zero could
    // mean the extractor is silent on this corpus, or it could mean the pattern
    // table never loaded. The same table, over software prose, must fire.
    let control = extract(
        &[Source {
            event_id: "control".into(),
            session_id: "control".into(),
            text: "Added the handling fee to the shipping rate. All tests pass.".into(),
        }],
        &table,
    );
    assert!(
        !control.claims.is_empty(),
        "the pattern table extracted nothing from `All tests pass`, so a zero on \
         this corpus would say nothing about the corpus"
    );

    // How much of this corpus is the harness's own boilerplate rather than an
    // agent's report? An assertion about coverage is a number, not a sentence.
    let boilerplate = corpus
        .trajectories
        .iter()
        .filter_map(|t| final_assistant_message(&t.raw))
        .filter(|m| m.contains("TRANSFERRED TO A HUMAN AGENT"))
        .count();
    eprintln!(
        "  final messages that are the harness's transfer boilerplate: {boilerplate} \
         of {}",
        corpus.trajectories.len()
    );

    // **The assertions are about scope, not about the result.** A number is
    // reported whatever it is; what must hold is that this measurement stays
    // what it says it is.
    assert!(
        corpus.trajectories.len() > 100,
        "{}",
        corpus.trajectories.len()
    );
    let total: usize = cross.values().map(|r| r[0] + r[1] + r[2]).sum();
    assert_eq!(
        total,
        corpus.trajectories.len(),
        "the cross-tabulation dropped trajectories; every one is counted or the \
         rates are over an unstated subset"
    );
    // Only the extractor ran. Nothing here reached a §6 verifier, and the
    // corpus has nothing for one to read.
    let census = tau2::evidence_census(&corpus);
    assert_eq!(
        census.code_check_executions, 0,
        "τ2-bench gained executed code checks; if that is real, this corpus is no \
         longer appendix-only and REPORT.md must change with it"
    );
}

/// **The ruling, made structural rather than editorial.**
///
/// The operator's instruction is that τ2-bench never appears in the headline.
/// An instruction is not a control: the corpus is genuine class 1, it has 200
/// task ids to serve as honest-work shapes, and both classes are present — so
/// nothing about the *statistics* stops it producing an AUROC. What stops it is
/// that not one of its trajectories carries the evidence §6's verifiers read.
///
/// This test builds the most headline-shaped `Measurement` the corpus permits
/// and asserts it is refused.
#[test]
#[ignore = "requires the fetched τ2-bench corpus."]
fn tau2_cannot_produce_a_headline_however_it_is_arranged() {
    use thruline_bench::{Measurement, NotReportable, Outcome, Verdict};

    let corpus = tau2::load(&fixtures().join(AIRLINE.0), AIRLINE.1)
        .unwrap_or_else(|e| panic!("{e}\n\nRun `fixtures/bench/download.sh`."));

    // Arranged as favourably as the corpus allows: every trajectory reaches a
    // verdict, the verdicts agree with the labels perfectly, and each task id is
    // its own honest-work shape. This is the best case, and it must still be
    // refused.
    let outcomes: Vec<Outcome> = corpus
        .trajectories
        .iter()
        .map(|t| Outcome {
            id: t.id.clone(),
            task_id: t.task_id.clone(),
            label: t.label.clone(),
            verdict: Some(match t.label.label() {
                Some(l) if l.is_positive() => Verdict::Unsupported,
                _ => Verdict::Supported,
            }),
            shape: t.shape.clone(),
            duration_ms: 1,
            detectors: Vec::new(),
            // The measured fact: zero executed code checks bound to a git tree.
            executed_checks: 0,
            protocol_hash: None,
        })
        .collect();

    let m = Measurement::of(
        "tau2-bench/airline",
        corpus.provenance.clone(),
        None,
        outcomes,
    );
    assert_eq!(m.evidence_coverage(), 0.0);
    match m.overclaim_index() {
        Err(NotReportable::NoVerifiableEvidence { trajectories, .. }) => {
            assert_eq!(trajectories, corpus.trajectories.len());
        }
        Ok(h) => panic!(
            "τ2-bench produced a §12 headline (AUROC {:.3}); the ruling that it must \
             not is editorial again",
            h.auroc
        ),
        Err(other) => panic!(
            "refused, but for the wrong reason — a corpus with better statistics \
             would slip through: {other}"
        ),
    }
}
