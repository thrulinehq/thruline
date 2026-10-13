//! RB6a — the eval harness, over fixed specimens.
//!
//! Two rules shape it, and they come straight from the item:
//!
//! 1. **The deterministic skeleton is ground truth where it can be.** RB7's
//!    Layer-2 candidates already say, deterministically, whether a session
//!    contains a decision at all. Where the skeleton can decide, it decides, and
//!    the model is scored against it. That is the only scoring in this project
//!    that does not need a human.
//!
//! 2. **Undecidable cases append to an adjudication file. They never halt.** A
//!    harness that stops at the first case it cannot score is a harness that
//!    measures nothing on a real corpus. An undecidable specimen is written out
//!    for a human to rule on later, and the run continues — the same shape as
//!    `thruline adjudicate`, which already exists for exactly this.
//!
//! What this harness measures on the machine it was written on is **the
//! plumbing, the labeling, and the abstention paths** — because the provider it
//! runs against is the mock tier and no local model is installed. It does not
//! measure model quality, and saying so is not a hedge: a number produced by
//! scoring a deterministic echo against a deterministic skeleton would be a
//! number about nothing.

use serde::{Deserialize, Serialize};

use crate::mind::{run, Abstention, MindJob, MindOutput, MindProvider, MindResult};

/// One fixed specimen: a record to hand the model, and what the deterministic
/// skeleton says about it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Specimen {
    pub id: String,
    pub job: MindJob,
    /// The deterministic record handed to the model.
    pub record: String,
    /// What the skeleton establishes, where it can.
    pub truth: GroundTruth,
    /// Why this specimen is in the set — read by humans, not by code.
    #[serde(default)]
    pub note: Option<String>,
}

/// What the deterministic layer can say about a specimen without a model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum GroundTruth {
    /// The skeleton establishes there IS something here to propose. A model
    /// that abstains is wrong.
    ProposalExpected,
    /// The skeleton establishes there is NOT. A model that proposes is wrong —
    /// this is the direction that matters, because a confident proposal about
    /// an empty session is the failure mode Layer 3 exists to contain.
    AbstentionExpected,
    /// The skeleton cannot decide. Scored by nobody; written out for a human.
    Undecidable { question: String },
}

/// One specimen's outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Verdict {
    /// Scored against the skeleton and agreed with it.
    Agreed,
    /// Scored against the skeleton and disagreed.
    Disagreed { expected: String, got: String },
    /// Not scorable. Appended to the adjudication file.
    Adjudicate,
}

#[derive(Debug, Clone, Serialize)]
pub struct Scored {
    pub specimen: String,
    pub job: MindJob,
    pub verdict: Verdict,
    pub output: MindOutput,
}

/// What a run produced. Counts, not a rate — a percentage over eleven specimens
/// is a number with more decimal places than evidence.
#[derive(Debug, Clone, Default, Serialize)]
pub struct EvalReport {
    pub specimens: usize,
    pub agreed: usize,
    pub disagreed: usize,
    pub adjudicate: usize,
    /// Abstentions, by reason. On a machine with no model this is the whole
    /// report, and it should be: every specimen abstaining with `no_model` is
    /// the mandatory fallback working.
    pub abstentions: Vec<(String, usize)>,
    pub scored: Vec<Scored>,
}

impl EvalReport {
    /// One line, for a human reading test output.
    pub fn summary(&self) -> String {
        format!(
            "{} specimens · {} agreed · {} disagreed · {} for adjudication",
            self.specimens, self.agreed, self.disagreed, self.adjudicate
        )
    }
}

/// One line of the adjudication file — the same shape as everything else this
/// project asks a human to rule on: the question, the evidence, and no default
/// answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdjudicationRow {
    pub specimen: String,
    pub job: String,
    pub question: String,
    pub record: String,
    pub model_said: String,
    pub provenance: String,
    /// Always absent when written. A pre-filled verdict is not a verdict.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
}

/// Run every specimen. Never returns early, whatever any one of them does.
///
/// `adjudication` receives one JSON line per undecidable specimen. It is a sink
/// rather than a path so a test can hold it in memory and a real run can append
/// to a file, without this function knowing which.
pub fn evaluate(
    provider: &dyn MindProvider,
    specimens: &[Specimen],
    adjudication: &mut dyn FnMut(AdjudicationRow),
) -> EvalReport {
    let mut report = EvalReport {
        specimens: specimens.len(),
        ..Default::default()
    };
    let mut abstentions: std::collections::BTreeMap<String, usize> = Default::default();

    for specimen in specimens {
        let output = run(provider, specimen.job, &specimen.record);
        if let MindResult::Abstained { reason, .. } = &output.result {
            *abstentions.entry(reason.as_str().to_string()).or_default() += 1;
        }

        let verdict = match &specimen.truth {
            GroundTruth::Undecidable { question } => {
                adjudication(AdjudicationRow {
                    specimen: specimen.id.clone(),
                    job: specimen.job.as_str().to_string(),
                    question: question.clone(),
                    record: specimen.record.clone(),
                    model_said: describe(&output.result),
                    provenance: output.label(),
                    verdict: None,
                });
                report.adjudicate += 1;
                Verdict::Adjudicate
            }
            GroundTruth::ProposalExpected if output.result.is_proposal() => {
                report.agreed += 1;
                Verdict::Agreed
            }
            GroundTruth::ProposalExpected => {
                report.disagreed += 1;
                Verdict::Disagreed {
                    expected: "a proposal".to_string(),
                    got: describe(&output.result),
                }
            }
            GroundTruth::AbstentionExpected if !output.result.is_proposal() => {
                report.agreed += 1;
                Verdict::Agreed
            }
            GroundTruth::AbstentionExpected => {
                report.disagreed += 1;
                Verdict::Disagreed {
                    expected: "an abstention".to_string(),
                    got: describe(&output.result),
                }
            }
        };

        report.scored.push(Scored {
            specimen: specimen.id.clone(),
            job: specimen.job,
            verdict,
            output,
        });
    }

    report.abstentions = abstentions.into_iter().collect();
    report
}

fn describe(result: &MindResult) -> String {
    match result {
        MindResult::Proposal { text, .. } => text.clone(),
        MindResult::Abstained { reason, detail } => {
            format!("abstained ({}): {detail}", reason.as_str())
        }
    }
}

/// The committed specimen set.
pub const SPECIMENS_JSON: &str = include_str!("../tests/fixtures/rb6a/specimens.json");

/// Parse the committed specimens, failing loudly rather than returning an empty
/// set — an eval harness that silently runs zero specimens reports a clean pass.
pub fn committed_specimens() -> Result<Vec<Specimen>, String> {
    let specimens: Vec<Specimen> =
        serde_json::from_str(SPECIMENS_JSON).map_err(|e| format!("rb6a specimens: {e}"))?;
    if specimens.is_empty() {
        return Err("rb6a specimens: the set is empty".to_string());
    }
    Ok(specimens)
}

/// A count of abstentions by reason, for callers that only want the shape.
pub fn abstention_count(report: &EvalReport, reason: Abstention) -> usize {
    report
        .abstentions
        .iter()
        .find(|(r, _)| r == reason.as_str())
        .map(|(_, n)| *n)
        .unwrap_or(0)
}
