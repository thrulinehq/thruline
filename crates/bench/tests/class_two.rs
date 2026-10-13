//! §12 class 2 end to end: the pre-registration ladder, rung by rung.
//!
//! Every refusal on the class-2 path was written and none was exercised. That
//! is the shape of a guard that quietly stops working — an unreachable success
//! path is a future loosening, and an unreachable *refusal* is one that nobody
//! notices has been removed. So this walks the whole ladder: each rung is
//! reached by taking a corpus that reports and breaking exactly one thing, and
//! the last test is the one where it all holds and a headline comes out.

use std::collections::BTreeMap;

use thruline_bench::{
    Ground, HoldoutLedger, Label, Measurement, NotReportable, Outcome, Protocol, Provenance,
    RunManifest, Verdict,
};

/// The shared temp-root guard. See tests/common/mod.rs.
mod common;

const SHAPES: [&str; 5] = ["rename", "refactor", "docs", "config", "xfail"];

fn protocol() -> Protocol {
    let mut tasks: Vec<String> = Vec::new();
    for i in 0..10 {
        tasks.push(format!("h{i}"));
    }
    for i in 0..4 {
        tasks.push(format!("p{i}"));
    }
    Protocol {
        harness: "terminal-bench".into(),
        harness_version: "1.0.0".into(),
        model: "some-model@2026-01".into(),
        holdout: tasks.clone(),
        tasks,
        parameters: BTreeMap::from([("seed".to_string(), "7".to_string())]),
        detectors: BTreeMap::from([("narrowed_run".to_string(), "enabled".to_string())]),
        preconditions: Default::default(),
    }
}

/// Ten honest trajectories across five shapes, four positives, all bound to
/// `hash`. This reports; every test below breaks one thing about it.
fn outcomes(hash: &str) -> Vec<Outcome> {
    let mut out = Vec::new();
    for i in 0..10 {
        out.push(Outcome {
            id: format!("h{i}"),
            task_id: Some(format!("h{i}")),
            label: Ground::from_oracle("reward", "1", Label::HonestPass),
            verdict: Some(Verdict::Supported),
            shape: SHAPES[i % SHAPES.len()].to_string(),
            duration_ms: 5,
            detectors: Vec::new(),
            executed_checks: 1,
            protocol_hash: Some(hash.to_string()),
        });
    }
    for i in 0..4 {
        out.push(Outcome {
            id: format!("p{i}"),
            task_id: Some(format!("p{i}")),
            label: Ground::from_oracle("reward", "0", Label::Cheated),
            verdict: Some(Verdict::Unsupported),
            shape: "-".to_string(),
            duration_ms: 7,
            detectors: vec!["narrowed_run".into()],
            executed_checks: 1,
            protocol_hash: Some(hash.to_string()),
        });
    }
    out
}

fn measurement(run: Option<RunManifest>, outcomes: Vec<Outcome>) -> Measurement {
    let commitment = protocol().commit().expect("commit");
    Measurement::of(
        "generated",
        Provenance::GeneratedUnderProtocol(commitment),
        run,
        outcomes,
    )
}

/// A run manifest for a genuinely first run.
///
/// **It goes through the ledger**, because that is now the only way to get one.
/// `RunManifest::first()` used to exist and was a one-token bypass of the whole
/// holdout discipline; these tests took it, which is exactly how a bypass stays
/// invisible. Each call gets its own temp ledger, so "first" is a fact about a
/// fresh record rather than an assertion.
fn first_run() -> RunManifest {
    ledger_in(&fresh_dir("first"))
        .spend(&protocol().hash(), "run-1")
        .expect("spend")
}

/// A temp directory unique to this call, so tests never share a ledger.
///
/// Returns a GUARD, not a path: the caller must hold it for as long as it uses
/// the directory, and the directory goes when it drops — including while
/// unwinding from a failed assertion, which is the run that used to leak most.
/// This returned a bare `PathBuf` and leaked 1,583 directories on the
/// development machine before Track C found it (DECISIONS #158).
fn fresh_dir(tag: &str) -> common::TempRoot {
    common::TempRoot::new(&format!("c2-{tag}"))
}

fn ledger_in(dir: &common::TempRoot) -> HoldoutLedger {
    HoldoutLedger::open(&dir.join("holdout.tsv")).expect("open")
}

/// The rung that must hold, or none of the others matter: with the protocol
/// committed, the run declared, every trajectory bound to it and every task
/// accounted for, a class-2 corpus **reports**.
#[test]
fn a_pre_registered_run_reports() {
    let hash = protocol().hash();
    let h = measurement(Some(first_run()), outcomes(&hash))
        .overclaim_index()
        .expect("a correctly pre-registered class-2 run must report");
    assert_eq!(h.class, 2);
    assert_eq!(h.protocol_hash.as_deref(), Some(hash.as_str()));
    assert_eq!(h.oracle_coverage, 1.0);
}

#[test]
fn a_class_two_corpus_without_a_run_manifest_is_refused() {
    let hash = protocol().hash();
    let err = measurement(None, outcomes(&hash))
        .overclaim_index()
        .expect_err("class 2 without a manifest is refused");
    assert!(matches!(err, NotReportable::NoRunManifest), "{err:?}");
}

#[test]
fn a_run_under_a_different_protocol_is_refused() {
    let hash = protocol().hash();
    let mut tuned = protocol();
    tuned
        .detectors
        .insert("narrowed_run".into(), "disabled".into());
    let err = measurement(
        Some(
            ledger_in(&fresh_dir("tuned"))
                .spend(&tuned.hash(), "run-1")
                .expect("spend"),
        ),
        outcomes(&hash),
    )
    .overclaim_index()
    .expect_err("a run under a tuned protocol must be refused");
    assert!(
        matches!(err, NotReportable::ProtocolMismatch { .. }),
        "{err:?}"
    );
}

/// **The attack a corpus-level manifest does not stop.** Every trajectory here
/// is real and every one is accounted for; one of them was produced under a
/// different protocol. One manifest for the corpus cannot see that.
#[test]
fn a_trajectory_from_another_run_is_refused_even_when_the_corpus_manifest_matches() {
    let hash = protocol().hash();
    let mut mixed = outcomes(&hash);
    let mut tuned = protocol();
    tuned.parameters.insert("seed".into(), "8".into());
    mixed[3].protocol_hash = Some(tuned.hash());
    let err = measurement(Some(first_run()), mixed)
        .overclaim_index()
        .expect_err("a trajectory from a second run must be refused");
    match err {
        NotReportable::TrajectoryUnbound { id, found, .. } => {
            assert_eq!(id, "h3");
            assert_eq!(found, Some(tuned.hash()));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_trajectory_bound_to_nothing_is_refused() {
    let hash = protocol().hash();
    let mut unbound = outcomes(&hash);
    unbound[0].protocol_hash = None;
    let err = measurement(Some(first_run()), unbound)
        .overclaim_index()
        .expect_err("an unbound trajectory must be refused");
    assert!(
        matches!(err, NotReportable::TrajectoryUnbound { found: None, .. }),
        "{err:?}"
    );
}

/// A holdout consumed twice is a validation set with a holdout's name, and the
/// second run knows it because the ledger told it.
#[test]
fn a_spent_holdout_is_refused_and_the_ledger_is_what_notices() {
    let dir = common::TempRoot::new("holdout");
    let path = dir.join("holdout-ledger.tsv");
    let hash = protocol().hash();

    let mut ledger = thruline_bench::HoldoutLedger::open(&path).expect("open");
    let first = ledger.spend(&hash, "run-1").expect("spend");
    assert!(first.prior_runs().is_empty());
    measurement(Some(first), outcomes(&hash))
        .overclaim_index()
        .expect("the first run may report");

    // A second process opening the same ledger sees the first run.
    let mut ledger = thruline_bench::HoldoutLedger::open(&path).expect("reopen");
    let second = ledger.spend(&hash, "run-2").expect("spend");
    assert_eq!(second.prior_runs(), ["run-1"]);
    let err = measurement(Some(second), outcomes(&hash))
        .overclaim_index()
        .expect_err("a second run over the same holdout must be refused");
    match err {
        NotReportable::HoldoutSpent { prior, .. } => assert_eq!(prior, ["run-1"]),
        other => panic!("{other:?}"),
    }
    let _ = std::fs::remove_file(&path);
}

/// A run that crashed on three tasks and published the other eleven is a
/// selected corpus. The refusal names them.
#[test]
fn a_task_the_protocol_declared_and_the_corpus_lacks_is_refused_by_name() {
    let hash = protocol().hash();
    let mut dropped = outcomes(&hash);
    dropped.retain(|o| o.id != "p2" && o.id != "h7");
    let err = measurement(Some(first_run()), dropped)
        .overclaim_index()
        .expect_err("a dropped trajectory must be refused");
    match err {
        NotReportable::TrajectoriesUnaccounted { missing, declared } => {
            assert_eq!(declared, 14);
            assert_eq!(missing, ["h7", "p2"]);
        }
        other => panic!("{other:?}"),
    }
}

/// The same run, with the crashed trajectories **recorded as unlabelled**
/// rather than omitted, reports — and says so in `oracle_coverage`. This is the
/// rung that keeps the previous one from being a reason to hide failures.
#[test]
fn a_crashed_trajectory_recorded_as_unlabelled_still_reports() {
    let hash = protocol().hash();
    let mut crashed = outcomes(&hash);
    for o in crashed.iter_mut().filter(|o| o.id == "h7") {
        o.label = Ground::unlabelled("the harness was killed at 600 s");
        o.verdict = None;
    }
    let h = measurement(Some(first_run()), crashed)
        .overclaim_index()
        .expect("an unlabelled trajectory is recorded, not fatal");
    assert!(h.oracle_coverage < 1.0, "{}", h.oracle_coverage);
    assert_eq!(h.composition.get("unlabelled"), Some(&1));
}

/// The holdout is the set the targets are computed over. An empty one would
/// make every rate a rate over nothing.
#[test]
fn an_empty_holdout_is_refused() {
    let mut p = protocol();
    p.holdout = vec!["h0".into()];
    let commitment = p.commit().expect("commit");
    let hash = p.hash();
    let mut outs = outcomes(&hash);
    // The one holdout task is present in the corpus but carries no task_id, so
    // the holdout selects nothing.
    for o in outs.iter_mut().filter(|o| o.id == "h0") {
        o.task_id = None;
    }
    let m = Measurement::of(
        "generated",
        Provenance::GeneratedUnderProtocol(commitment),
        Some(
            ledger_in(&fresh_dir("empty-holdout"))
                .spend(&hash, "run-1")
                .expect("spend"),
        ),
        outs,
    );
    let err = m
        .overclaim_index()
        .expect_err("an empty holdout is refused");
    // `unaccounted` is checked first and h0 is now unaccounted for, which is
    // the same finding one rung earlier. Either refusal is correct; silence is
    // not.
    assert!(
        matches!(
            err,
            NotReportable::HoldoutEmpty { .. } | NotReportable::TrajectoriesUnaccounted { .. }
        ),
        "{err:?}"
    );
}

// ---------------------------------------------------------------------------
// V2: the ledger is the only way in.
// ---------------------------------------------------------------------------

/// **A spent holdout cannot be laundered, because there is no second door.**
///
/// The measured bypass: spend a holdout three times through `HoldoutLedger`,
/// then build a manifest with `RunManifest::first()` and get a clean headline
/// over a thrice-spent holdout — AUROC 1.000. "The ledger enforces it across
/// processes" was true only if the caller chose the ledger, and with a public
/// constructor named `first` the caller was a human making a judgement call.
///
/// This test asserts the judgement is gone: every path to a `RunManifest` runs
/// through `spend`, so obtaining one **records** the spend. The compile-time
/// half of the guard is that `RunManifest::first` no longer exists; this is the
/// behavioural half.
#[test]
fn a_holdout_spent_three_times_cannot_be_re_declared_as_a_first_run() {
    let dir = fresh_dir("laundering");
    let hash = protocol().hash();
    let commitment = protocol().commit().expect("commit");

    for run in ["run-1", "run-2", "run-3"] {
        let _ = ledger_in(&dir).spend(&hash, run).expect("spend");
    }

    // A fourth run, however it is obtained, carries the first three.
    let fourth = ledger_in(&dir).spend(&hash, "run-4").expect("spend");
    assert_eq!(fourth.prior_runs(), ["run-1", "run-2", "run-3"]);

    let m = Measurement::of(
        "generated",
        Provenance::GeneratedUnderProtocol(commitment),
        Some(fourth),
        outcomes(&hash),
    );
    match m.overclaim_index() {
        Err(NotReportable::HoldoutSpent { prior, .. }) => assert_eq!(prior.len(), 3),
        Ok(h) => panic!(
            "a thrice-spent holdout produced a headline (AUROC {:.3})",
            h.auroc
        ),
        Err(other) => panic!("{other}"),
    }
}

/// A ledger handle opened **before** another run spent still sees that spend,
/// because `spend` re-reads the file it is about to append to.
///
/// The residual, stated: this is not a lock. Two writers appending at the same
/// instant can still both see an empty history. It closes the realistic case —
/// one run finishing while another holds a handle opened earlier — and the
/// unrealistic one is named rather than implied away.
#[test]
fn a_ledger_handle_opened_early_still_sees_a_spend_that_landed_first() {
    let dir = fresh_dir("stale-handle");
    let hash = protocol().hash();

    // Opened first, spent second.
    let mut early = ledger_in(&dir);
    let _ = ledger_in(&dir)
        .spend(&hash, "the-other-run")
        .expect("spend");

    let late = early.spend(&hash, "my-run").expect("spend");
    assert_eq!(
        late.prior_runs(),
        ["the-other-run"],
        "a handle opened before the other spend reported an unspent holdout"
    );
}
