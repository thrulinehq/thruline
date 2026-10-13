//! `thruline eval` — the M3.6 CI gate against the synthetic corpus.
//!
//! A22: the corpus is **synthetic-only, permanently**. Real projects are
//! hand-adjudicated (M3.7) and never become fixtures — committing real
//! transcript content would break the export-privacy rule and make the gate
//! unauditable by anyone but this machine.
//!
//! The run is end-to-end and uses the production path exactly: ingest into a
//! throwaway ledger, `annotate`, `derive`, then compare every node state to a
//! hand-written expectation. Nothing about the eval is special-cased, so a bug
//! that only shows up in the real pipeline cannot hide from it.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::config::Config;
use crate::derive::derive_project_with;
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::nli::EntailmentScorer;

#[derive(Debug, Deserialize)]
struct Manifest {
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    file: String,
    expect: Vec<Expectation>,
}

#[derive(Debug, Deserialize)]
struct Expectation {
    uuid: String,
    /// `None` means **no node may exist** at this event — the claim must never
    /// have been extracted.
    state: Option<String>,
    class: String,
    shape: String,
    why: String,
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub file: String,
    pub class: String,
    pub shape: String,
    pub expected: String,
    pub actual: String,
    pub passed: bool,
    pub why: String,
    /// Best entailment score behind the verdict, and whether that premise was
    /// understood. Calibration without these is guesswork.
    pub score: Option<f32>,
    pub fidelity: Option<String>,
    pub candidates: usize,
}

#[derive(Debug, Default)]
pub struct EvalReport {
    pub outcomes: Vec<Outcome>,
    pub hi: f32,
    pub lo: f32,
    pub pairs_scored: u64,
    pub fallback_premises: u64,
    pub parsed_premises: u64,
}

/// The only two verdicts that accuse (§5). `unsupported` is not one of them.
pub fn is_accusation(state: &str) -> bool {
    state == "contradicted" || state == "dropped"
}

/// Classes that must produce an ACCUSATION: evidence exists and it conflicts.
///
/// `false-completed` is deliberately NOT here. North Star v2 is explicit that
/// "no captured proof found" is a statement about our evidence, not an
/// allegation of lying — so a fabricated claim with nothing to contradict it is
/// `unsupported`, and crediting it as a catch would let the accusation gate
/// pass on cases where the product never actually accused anyone.
const FAKE_CLASSES: [&str; 1] = ["misstated-result"];

/// Classes that must produce a COVERAGE verdict — measured, but never counted
/// as a warning. Kept as its own metric so these cases cannot silently vanish
/// from the eval when they stop being accusations.
const COVERAGE_CLASSES: [&str; 2] = ["false-completed", "worst-claim-wins"];
/// Classes that are TRUE — or out of scope entirely — and must therefore never
/// be warned about. The last four were added in M3.8c from patterns that hand
/// adjudication found on real transcripts and that neither gate could see.
const TRUE_CLASSES: [&str; 9] = [
    "true-claim",
    "paraphrase",
    "unparseable-evidence",
    "proposal-fulfilled",
    "proposal-fulfilled-paraphrase",
    // §5b categories: never accusable, however confident the scorer feels.
    "analysis-prose",
    "participle-adjective",
    "finding-not-accused",
    "micro-intent",
];

impl EvalReport {
    fn count(&self, classes: &[&str], pred: impl Fn(&Outcome) -> bool) -> (usize, usize) {
        let rows: Vec<&Outcome> = self
            .outcomes
            .iter()
            .filter(|o| classes.contains(&o.class.as_str()))
            .collect();
        (rows.iter().filter(|o| pred(o)).count(), rows.len())
    }

    /// Fraction of no-proof cases correctly reported as `unsupported`.
    pub fn coverage_recall(&self) -> (usize, usize) {
        self.count(&COVERAGE_CLASSES, |o| o.actual == "unsupported")
    }

    /// Fraction of planted fakes caught as an ACCUSATION.
    ///
    /// `unsupported` deliberately does not count as a catch: a coverage gap is
    /// not the product saying the claim was false, so crediting it here would
    /// let the gate pass on abstentions (§5).
    pub fn contradicted_recall(&self) -> (usize, usize) {
        self.count(&FAKE_CLASSES, |o| is_accusation(&o.actual))
    }

    /// Fraction of TRUE claims wrongly warned about. The number that decides
    /// whether the product is trustworthy.
    pub fn false_warning_rate(&self) -> (usize, usize) {
        self.count(&TRUE_CLASSES, |o| is_accusation(&o.actual))
    }

    pub fn dropped_precision(&self) -> (usize, usize) {
        let flagged: Vec<&Outcome> = self
            .outcomes
            .iter()
            .filter(|o| o.actual == "dropped")
            .collect();
        (
            flagged.iter().filter(|o| o.expected == "dropped").count(),
            flagged.len(),
        )
    }

    pub fn dropped_recall(&self) -> (usize, usize) {
        let want: Vec<&Outcome> = self
            .outcomes
            .iter()
            .filter(|o| o.expected == "dropped")
            .collect();
        (
            want.iter().filter(|o| o.actual == "dropped").count(),
            want.len(),
        )
    }

    /// Proposals that WERE fulfilled but got called ghosts. The 👻 equivalent
    /// of a false ⚠, and just as damaging.
    pub fn paraphrase_wrongly_dropped(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|o| o.class.starts_with("proposal-fulfilled") && o.actual == "dropped")
            .count()
    }

    pub fn per_shape(&self) -> BTreeMap<String, (usize, usize)> {
        let mut m: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for o in &self.outcomes {
            let e = m.entry(o.shape.clone()).or_insert((0, 0));
            e.1 += 1;
            if o.passed {
                e.0 += 1;
            }
        }
        m
    }

    pub fn passed(&self) -> usize {
        self.outcomes.iter().filter(|o| o.passed).count()
    }

    /// The §5 gate. Returns the reasons it failed, empty when it passes.
    pub fn gate_failures(&self) -> Vec<String> {
        let mut out = Vec::new();
        let (hit, total) = self.contradicted_recall();
        let recall = if total == 0 {
            1.0
        } else {
            hit as f64 / total as f64
        };
        if recall < 0.90 {
            out.push(format!(
                "accusation-recall {recall:.2} < 0.90 ({hit}/{total} planted fakes caught)"
            ));
        }
        let (bad, t) = self.false_warning_rate();
        let rate = if t == 0 { 0.0 } else { bad as f64 / t as f64 };
        // M3.8c tightens this from ≤0.05 to ZERO. The corpus now contains the
        // real failure classes, so any false warning is a reproduction of the
        // bug A26 measured rather than an unlucky paraphrase.
        if bad > 0 {
            out.push(format!(
                "false-⚠ rate {rate:.2} ({bad}/{t}) — the gate is ZERO false warnings \
                 once the corpus contains reality's failure classes"
            ));
        }
        let (cov, covt) = self.coverage_recall();
        if covt > 0 && cov < covt {
            out.push(format!(
                "coverage-recall {cov}/{covt} — a claim with no captured proof must be \
                 reported as `unsupported`, not silently absorbed"
            ));
        }
        let ghosts = self.paraphrase_wrongly_dropped();
        if ghosts > 0 {
            out.push(format!(
                "{ghosts} fulfilled proposal(s) wrongly reported as dropped"
            ));
        }
        out
    }
}

/// Corpus project id. The corpus directory is ingested as a single project, so
/// forward-pass candidates can cross session boundaries the way they do in a
/// real project.
fn project_id_of(corpus: &Path) -> String {
    corpus
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "eval".to_string())
}

pub fn run(
    corpus: &Path,
    db: &Path,
    cfg: &Config,
    scorer: &mut dyn EntailmentScorer,
    scorer_tag: &str,
) -> Result<EvalReport> {
    let manifest_path = corpus.join("manifest.json");
    let raw =
        std::fs::read_to_string(&manifest_path).map_err(|e| CoreError::io(&manifest_path, e))?;
    let manifest: Manifest = serde_json::from_str(&raw)?;

    let mut ledger = Ledger::open(db)?;
    let eval_mirrors = db
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("eval-mirrors");
    crate::ingest::ingest(
        &mut ledger,
        std::slice::from_ref(&corpus.to_path_buf()),
        &eval_mirrors,
    )?;
    let project = project_id_of(corpus);

    let a = crate::annotate::annotate(&ledger, &project, cfg, scorer, scorer_tag)?;
    let graph = derive_project_with(&ledger, &project, cfg)?;

    // uuid → node state. A uuid with no node is the "never extracted" case.
    let state_of: BTreeMap<&str, &str> = graph
        .nodes
        .iter()
        .filter_map(|n| n.anchor_uuid.as_deref().map(|u| (u, n.sort_key.as_str())))
        .collect();
    // The worst claim's numbers — the ones that actually decided the verdict.
    let detail_of: BTreeMap<&str, (Option<f32>, Option<String>, usize)> = graph
        .nodes
        .iter()
        .filter_map(|n| {
            let u = n.anchor_uuid.as_deref()?;
            let v = n
                .verdicts
                .iter()
                .max_by_key(|v| v.state.as_str().to_string())?;
            Some((u, (v.best_score, v.best_fidelity.clone(), v.candidates)))
        })
        .collect();

    let mut report = EvalReport {
        hi: cfg.thresholds.hi,
        lo: cfg.thresholds.lo,
        pairs_scored: a.links_written,
        fallback_premises: a.fallback_premises,
        parsed_premises: a.parsed_premises,
        ..Default::default()
    };
    for case in &manifest.cases {
        for e in &case.expect {
            let actual = state_of
                .get(e.uuid.as_str())
                .copied()
                .unwrap_or("(no node)")
                .to_string();
            let expected = e.state.clone().unwrap_or_else(|| "(no node)".to_string());
            let d = detail_of.get(e.uuid.as_str()).cloned();
            report.outcomes.push(Outcome {
                file: case.file.clone(),
                class: e.class.clone(),
                shape: e.shape.clone(),
                passed: actual == expected,
                expected,
                actual,
                why: e.why.clone(),
                score: d.as_ref().and_then(|x| x.0),
                fidelity: d.as_ref().and_then(|x| x.1.clone()),
                candidates: d.map(|x| x.2).unwrap_or(0),
            });
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn o(class: &str, expected: &str, actual: &str) -> Outcome {
        Outcome {
            file: "x".into(),
            class: class.into(),
            shape: "cargo".into(),
            expected: expected.into(),
            actual: actual.into(),
            passed: expected == actual,
            why: String::new(),
            score: None,
            fidelity: None,
            candidates: 0,
        }
    }

    #[test]
    fn the_gate_fails_loudly_on_a_single_false_warning() {
        // M3.8c: the bar is ZERO, not 5%. Once the corpus contains the classes
        // that real adjudication found — analysis prose, participle
        // adjectives, micro-intents — a false warning is a reproduction of the
        // bug A26 measured, not an unlucky paraphrase, and a rate that
        // tolerates one tolerates the disease.
        let mut r = EvalReport::default();
        for _ in 0..20 {
            r.outcomes.push(o("true-claim", "supported", "supported"));
        }
        assert!(r.gate_failures().is_empty(), "{:?}", r.gate_failures());
        r.outcomes
            .push(o("analysis-prose", "recorded", "contradicted"));
        assert!(
            r.gate_failures().iter().any(|f| f.contains("false-⚠")),
            "one false warning in twenty-one must fail the gate: {:?}",
            r.gate_failures()
        );
    }

    #[test]
    fn a_missed_fake_fails_the_recall_gate() {
        let mut r = EvalReport::default();
        for _ in 0..9 {
            r.outcomes
                .push(o("misstated-result", "contradicted", "contradicted"));
        }
        r.outcomes
            .push(o("misstated-result", "contradicted", "unknown"));
        assert!(r.gate_failures().is_empty(), "9/10 is exactly 0.90");
        // A second miss, and it must be another MISSTATED case: `false-completed`
        // no longer feeds this gate at all, so using one here would have tested
        // nothing. It has its own coverage-recall gate instead.
        r.outcomes
            .push(o("misstated-result", "contradicted", "unknown"));
        assert!(r
            .gate_failures()
            .iter()
            .any(|f| f.contains("accusation-recall")));
    }

    #[test]
    fn a_fulfilled_proposal_called_a_ghost_fails_outright() {
        let mut r = EvalReport::default();
        r.outcomes
            .push(o("proposal-fulfilled", "supported", "dropped"));
        assert!(
            r.gate_failures().iter().any(|f| f.contains("wrongly")),
            "a false 👻 is as damaging as a false ⚠ and must not be tolerated"
        );
    }
}
