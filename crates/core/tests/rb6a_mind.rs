//! RB6a — the local mind's contract, in executable form.
//!
//! The four things that must be true of a model that "narrates and proposes,
//! never decides":
//!
//! 1. It can do **exactly three jobs**, and there is no fourth.
//! 2. Every output is **labeled** language-tier, shadow-mode, not truth-bearing
//!    — and the label cannot be omitted, because it is stamped rather than set.
//! 3. **No model is a working state**, not an error state. It abstains.
//! 4. **Nothing truth-bearing reads it.** The shadow gate.

use thruline_core::language::select_model;
use thruline_core::mind::{
    is_loopback, run, Abstention, MindJob, MindProvider, MindResult, MockProvider, NoModelProvider,
    OllamaProvider, Provenance, MODE, TIER,
};
use thruline_core::mind_eval::{
    abstention_count, committed_specimens, evaluate, AdjudicationRow, GroundTruth,
};

#[test]
fn there_are_exactly_three_jobs() {
    assert_eq!(
        MindJob::ALL.len(),
        3,
        "RB6a is cut to three jobs. A fourth is a scope change, not a feature: \
         a local model that may be asked anything becomes one that is asked to decide."
    );
    let mut slugs: Vec<&str> = MindJob::ALL.iter().map(|j| j.as_str()).collect();
    slugs.sort_unstable();
    assert_eq!(
        slugs,
        [
            "capsule_compaction",
            "decision_proposal",
            "session_narration"
        ]
    );
    // Every job has a distinct Layer-3 slot and a non-empty instruction.
    let mut slots: Vec<&str> = MindJob::ALL.iter().map(|j| j.slot()).collect();
    slots.sort_unstable();
    slots.dedup();
    assert_eq!(slots.len(), 3, "two jobs share a slot");
    for job in MindJob::ALL {
        assert!(
            job.instruction().contains("INSUFFICIENT"),
            "{job} gives the model no way to decline, so it will invent one"
        );
    }
}

/// The label is stamped, not passed. Whatever the provider says and whatever
/// the job is, the three fields come out the same.
#[test]
fn every_output_is_labeled_shadow_and_not_truth_bearing() {
    let providers: Vec<(&str, Box<dyn MindProvider>)> = vec![
        ("no-model", Box::new(NoModelProvider)),
        ("mock", Box::new(MockProvider::default())),
    ];
    for (name, provider) in &providers {
        for job in MindJob::ALL {
            for record in ["a real looking record", "<<thin>>", ""] {
                let out = run(provider.as_ref(), *job, record);
                assert_eq!(out.tier, TIER, "{name}/{job}");
                assert_eq!(out.mode, MODE, "{name}/{job}");
                assert!(
                    !out.truth_bearing,
                    "{name}/{job} produced a truth-bearing output"
                );
                assert!(
                    out.label().starts_with("model-proposed-shadow"),
                    "{name}/{job} label: {}",
                    out.label()
                );
                assert!(
                    out.label().contains(&out.provenance.provider),
                    "the label must name what produced it: {}",
                    out.label()
                );
            }
        }
    }
}

/// The mandatory fallback. On a machine with no local model — which is the
/// machine RB6a was written on — every job abstains, names `no_model`, and
/// never produces a proposal.
#[test]
fn with_no_model_every_job_abstains_and_none_invents() {
    for job in MindJob::ALL {
        let out = run(&NoModelProvider, *job, "a record with plenty in it");
        match &out.result {
            MindResult::Abstained { reason, .. } => {
                assert_eq!(*reason, Abstention::NoModel, "{job}");
            }
            MindResult::Proposal { text, .. } => {
                panic!("{job} invented {text:?} with no model installed")
            }
        }
        assert_eq!(out.provenance.model, "none");
        assert_eq!(out.provenance.provider, "no-model");
    }
}

/// A model that takes the instruction's escape hatch has ANSWERED. It has not
/// failed, and its answer must not be re-read as content.
#[test]
fn a_model_saying_insufficient_is_an_abstention_not_a_proposal() {
    struct Declining;
    impl MindProvider for Declining {
        fn provenance(&self) -> Provenance {
            Provenance::none("test")
        }
        fn complete(&self, _job: MindJob, _record: &str) -> Result<String, String> {
            Ok("  INSUFFICIENT.  ".to_string())
        }
    }
    let out = run(&Declining, MindJob::DecisionProposal, "anything");
    match out.result {
        MindResult::Abstained { reason, .. } => {
            assert_eq!(reason, Abstention::ModelSaidInsufficient)
        }
        other => panic!("a declining model was read as content: {other:?}"),
    }
}

#[test]
fn an_empty_answer_is_unusable_rather_than_an_empty_proposal() {
    struct Silent;
    impl MindProvider for Silent {
        fn provenance(&self) -> Provenance {
            Provenance::none("test")
        }
        fn complete(&self, _job: MindJob, _record: &str) -> Result<String, String> {
            Ok("   \n  ".to_string())
        }
    }
    let out = run(&Silent, MindJob::SessionNarration, "anything");
    match out.result {
        MindResult::Abstained { reason, .. } => assert_eq!(reason, Abstention::Unusable),
        other => panic!("an empty answer became a proposal: {other:?}"),
    }
}

/// The core's single permitted egress is a model server on this machine. A base
/// URL pointing anywhere else would turn a shadow-mode narration into an
/// exfiltration channel for transcript text.
#[test]
fn the_ollama_provider_refuses_a_non_loopback_endpoint() {
    let selection = select_model(24 * 1024 * 1024 * 1024, None).expect("selection");
    assert!(OllamaProvider::new("http://127.0.0.1:11434/v1", selection.clone()).is_ok());
    for hostile in [
        "http://10.0.0.5:11434/v1",
        "https://models.example.com/v1",
        "http://localhost.evil.example.com/v1",
    ] {
        assert!(
            OllamaProvider::new(hostile, selection.clone()).is_err(),
            "accepted a non-loopback endpoint: {hostile}"
        );
        assert!(!is_loopback(hostile));
    }
}

// --- the eval harness ------------------------------------------------------

#[test]
fn the_committed_specimen_set_is_real_and_covers_all_three_jobs() {
    let specimens = committed_specimens().expect("specimens parse");
    assert!(specimens.len() >= 10, "{} specimens", specimens.len());

    let mut jobs: Vec<&str> = specimens.iter().map(|s| s.job.as_str()).collect();
    jobs.sort_unstable();
    jobs.dedup();
    assert_eq!(jobs.len(), 3, "the set does not exercise all three jobs");

    // Both scorable directions, and at least one honest undecidable. A set with
    // no undecidables would mean the ground truth was invented to be complete.
    let expects_proposal = specimens
        .iter()
        .filter(|s| matches!(s.truth, GroundTruth::ProposalExpected))
        .count();
    let expects_abstention = specimens
        .iter()
        .filter(|s| matches!(s.truth, GroundTruth::AbstentionExpected))
        .count();
    let undecidable = specimens
        .iter()
        .filter(|s| matches!(s.truth, GroundTruth::Undecidable { .. }))
        .count();
    assert!(expects_proposal > 0 && expects_abstention > 0 && undecidable > 0);

    // Every undecidable states its question. "Undecidable" with no question is
    // a specimen nobody can ever rule on.
    for specimen in &specimens {
        if let GroundTruth::Undecidable { question } = &specimen.truth {
            assert!(
                question.len() > 40,
                "{} is undecidable with no real question",
                specimen.id
            );
        }
    }
}

/// The harness never halts. Undecidable specimens are written out and the run
/// continues — a harness that stops at the first case it cannot score measures
/// nothing on a real corpus.
#[test]
fn undecidable_specimens_are_adjudicated_not_fatal() {
    let specimens = committed_specimens().expect("specimens");
    let mut rows: Vec<AdjudicationRow> = Vec::new();
    let report = evaluate(&MockProvider::default(), &specimens, &mut |row| {
        rows.push(row)
    });

    assert_eq!(
        report.specimens,
        specimens.len(),
        "the harness stopped early: {}",
        report.summary()
    );
    assert_eq!(
        report.agreed + report.disagreed + report.adjudicate,
        report.specimens
    );
    assert!(report.adjudicate > 0, "no specimen reached adjudication");
    assert_eq!(rows.len(), report.adjudicate);

    for row in &rows {
        assert!(
            row.verdict.is_none(),
            "the harness pre-filled a verdict for {}, which is not a verdict",
            row.specimen
        );
        assert!(!row.question.is_empty());
        assert!(
            row.provenance.starts_with("model-proposed-shadow"),
            "an adjudication row lost its provenance: {}",
            row.provenance
        );
    }
}

/// What the mock tier actually establishes: the plumbing, the labeling, and
/// both abstention directions. Not model quality — and the test says so, so
/// nobody quotes this as a score.
#[test]
fn the_mock_tier_exercises_both_scorable_directions() {
    let specimens = committed_specimens().expect("specimens");
    let report = evaluate(&MockProvider::default(), &specimens, &mut |_| {});
    assert!(
        report.disagreed == 0,
        "the mock tier disagreed with the skeleton, which means the plumbing is \
         wrong rather than the model: {:?}",
        report
            .scored
            .iter()
            .filter(|s| !matches!(s.verdict, thruline_core::mind_eval::Verdict::Agreed))
            .map(|s| &s.specimen)
            .collect::<Vec<_>>()
    );
    assert!(report.agreed >= 6, "{}", report.summary());
}

/// The whole report on a machine with no model, which is the machine this was
/// built on: every specimen abstains with `no_model`, nothing is proposed, and
/// nothing crashes. That is the mandatory fallback working end to end.
#[test]
fn with_no_model_the_harness_still_runs_and_reports_the_fallback() {
    let specimens = committed_specimens().expect("specimens");
    let mut rows = 0;
    let report = evaluate(&NoModelProvider, &specimens, &mut |_| rows += 1);

    assert_eq!(report.specimens, specimens.len());
    assert_eq!(
        abstention_count(&report, Abstention::NoModel),
        specimens.len(),
        "every specimen must abstain with no_model when no model exists"
    );
    assert!(
        report.scored.iter().all(|s| !s.output.result.is_proposal()),
        "something was proposed with no model installed"
    );
    // And the specimens whose ground truth expects an abstention are AGREED
    // with — the fallback is not merely safe, it is correct on those.
    assert!(report.agreed > 0, "{}", report.summary());
}

/// The shadow gate. Nothing that decides anything may read the local mind.
///
/// Enumerating the modules that must NOT reference it, rather than asserting a
/// count somewhere, is the A187 lesson: a containment check that can be
/// satisfied by a substring gets satisfied by accident.
#[test]
fn no_truth_bearing_module_reads_the_local_mind() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // Every module whose output is a fact the product stands behind.
    let truth_bearing = [
        "derive.rs",
        "candidates.rs",
        "packet.rs",
        "export.rs",
        "attention.rs",
        "now.rs",
        "chain.rs",
        "ledger.rs",
        "premise.rs",
        "continuity.rs",
        "capsule.rs",
        "session_status.rs",
        "guardian.rs",
    ];
    for module in truth_bearing {
        let path = root.join("crates/core/src").join(module);
        let text =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for forbidden in [
            "crate::mind",
            "mind::",
            "MindOutput",
            "MindProvider",
            "mind_grounded",
            "FactCatalog",
            "GroundedAnalysis",
        ] {
            assert!(
                !text.contains(forbidden),
                "{module} references {forbidden}. RB6a is shadow-first: a truth-bearing \
                 module that reads a model proposal has stopped being truth-bearing."
            );
        }
    }
}

/// Produce the real adjudication file, on demand.
///
/// `#[ignore]` on purpose, following `head_fold_bench`'s precedent: a test that
/// writes into the repository on every `cargo test` is a test that makes the
/// tree dirty for everyone. Run it when the specimen set changes:
///
/// ```bash
/// cargo test --test rb6a_mind -- --ignored --nocapture
/// ```
///
/// It appends rather than truncates, because an adjudication file is a queue a
/// human is working through and a re-run must not silently discard the entries
/// they have not reached yet.
#[test]
#[ignore]
fn write_the_adjudication_file() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = root.join("docs/recon/rb6a-adjudication.jsonl");
    let specimens = committed_specimens().expect("specimens");

    let existing = std::fs::read_to_string(&out).unwrap_or_default();
    let mut appended = 0;
    let mut lines = String::new();
    let report = evaluate(&MockProvider::default(), &specimens, &mut |row| {
        // Idempotent by specimen id: re-running must not duplicate a question
        // someone is already looking at.
        if existing.contains(&format!("\"specimen\":\"{}\"", row.specimen)) {
            return;
        }
        lines.push_str(&serde_json::to_string(&row).expect("row"));
        lines.push('\n');
        appended += 1;
    });

    if !lines.is_empty() {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&out)
            .expect("opening the adjudication file");
        f.write_all(lines.as_bytes()).expect("appending");
    }
    println!("{}", report.summary());
    println!("appended {appended} row(s) to {}", out.display());
}
