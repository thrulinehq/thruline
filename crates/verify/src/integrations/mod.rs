//! Versioned, deterministic external observations. Tools supply facts; this module
//! evaluates only an accepted, bounded criterion. It cannot pronounce a product correct.
//! Capturing processes and opening files belong to the CLI, never this crate.
mod clippy;
mod coverage;
mod decode;
mod dependency_graph;
mod evaluation;
mod gate;
mod infer;
mod infer_modes;
mod knip;
mod local_agent;
mod megalinter;
mod opengrep;
mod osv_modes;
mod ruff;
mod semgrep_modes;
mod sonar;
mod specialists;
mod strict_json;
mod suppressions;
use crate::Verdict;
pub use coverage::apply_capture_diagnostics;
pub use decode::decode;
pub use evaluation::evaluate;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
pub use strict_json::parse_json;
pub use suppressions::{semgrep_suppression_disabled, source_suppressions, SourceSuppressions};

pub const CONTRACT: &str = "thruline.integration-observation.v1";
pub const CATALOG: &str = include_str!("../../../../spec/integrations/catalog.json");
pub const ADAPTER_VERSION: &str = "native-adapters@3";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub schema: String,
    pub projects: Vec<Pack>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pack {
    pub id: String,
    pub feature: String,
    pub feature_name: String,
    pub repository: String,
    pub commit: String,
    pub tree: String,
    pub role: String,
    pub declared_main_license: String,
    pub contribution: String,
    pub limits: String,
    pub format: Option<String>,
    #[serde(default)]
    pub import_formats: Vec<String>,
    pub candidate_check_ids: Vec<String>,
    pub source_entry_points: Vec<String>,
    pub submodules: usize,
    pub qualification: String,
}
pub fn catalog() -> Result<Catalog, serde_json::Error> {
    serde_json::from_str(CATALOG)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Observation {
    pub rule: String,
    pub message: String,
    pub path: Option<String>,
    pub line: Option<u64>,
    /// Pointer into the digest-addressed report, not a model-generated citation.
    pub pointer: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Report {
    pub format: String,
    pub observations: Vec<Observation>,
    pub facts: BTreeMap<String, Value>,
    pub complete: bool,
    pub limitations: Vec<String>,
}
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("unrecognized report format: {0}")]
    Format(String),
    #[error("invalid report: {0}")]
    Invalid(String),
    #[error("report exceeds the 8 MiB decoder limit")]
    TooLarge,
}

/// Every axis is frozen before execution; conservative whole-workspace hashing
/// is used until a complete dependency graph can justify a narrower closure.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Inputs {
    pub workspace: String,
    pub executable: String,
    pub environment: String,
    pub rules: String,
    pub dependencies: String,
    pub dataset: String,
    pub requirement: String,
    pub policy: String,
    pub adapter: String,
}
impl Inputs {
    pub fn complete(&self) -> bool {
        self.axes().iter().all(|(_, v)| !v.is_empty())
    }
    pub fn axes(&self) -> [(&'static str, &str); 9] {
        [
            ("workspace", &self.workspace),
            ("executable", &self.executable),
            ("environment", &self.environment),
            ("rules", &self.rules),
            ("dependencies", &self.dependencies),
            ("dataset", &self.dataset),
            ("requirement", &self.requirement),
            ("policy", &self.policy),
            ("adapter", &self.adapter),
        ]
    }
    pub fn changed(&self, now: &Self) -> Vec<String> {
        self.axes()
            .iter()
            .zip(now.axes())
            .filter(|((_, a), (_, b))| *a != *b)
            .map(|((k, _), _)| k.to_string())
            .collect()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Criterion {
    pub id: String,
    pub revision: String,
    pub requirement: String,
    pub success: String,
    pub accepted_by: String,
    /// Machine condition the human accepts, not arbitrary executable code.
    pub condition: Condition,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Condition {
    /// Captured model suggestions are review material, never a passed check.
    Advisory,
    NoObservations,
    /// Selected ast-grep structural matches; captured unused-suppression Hint
    /// diagnostics remain evidence, not violations of the selected pattern.
    NoStructuralMatches,
    AllTestsPass {
        minimum: u64,
    },
    Maximum {
        fact: String,
        value: f64,
    },
    Minimum {
        fact: String,
        value: f64,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Completed,
    FailedToStart,
    TimedOut,
    OutputLimit,
    ResourceLimit,
    Cancelled,
    CaptureFailed,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Admission {
    pub accepted: bool,
    pub captured: bool,
    pub input_closure_complete: bool,
    pub exit_code: Option<i32>,
    pub run_state: RunState,
    pub before: Inputs,
    pub after: Inputs,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    Passed,
    Failed,
    Stale,
    Blocked,
    Unknown,
    Reported,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assessment {
    pub state: CheckState,
    pub verdict: Verdict,
    pub word: String,
    pub explanation: String,
    pub next: String,
    pub changed_inputs: Vec<String>,
    pub proof_level: String,
    pub advisory: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub criterion_measurement: Option<Value>,
}
fn answer(state: CheckState, explanation: &str, next: &str) -> Assessment {
    // All new external adapters are unqualified. Even a clean report cannot
    // promote a product completion claim; the bounded condition remains visible.
    let word = if state == CheckState::Failed {
        "Check condition not met"
    } else {
        "Recorded"
    };
    Assessment {
        state,
        verdict: Verdict::Unverifiable,
        word: word.into(),
        explanation: explanation.into(),
        next: next.into(),
        changed_inputs: vec![],
        proof_level: "bounded_tool_condition".into(),
        advisory: true,
        criterion_measurement: None,
    }
}
/// cargo-deny 0.20.2 returns a bitset of checks with errors, not a generic
/// finding status. Exit 1 is also its process-error status, so an exit is usable
/// only when it agrees with a decoded, complete check summary.
fn cargo_deny_exit_matches(report: &Report, exit: Option<i32>) -> bool {
    let mut expected = 0;
    let mut selected = false;
    for (bit, key) in [
        "advisories_errors",
        "bans_errors",
        "licenses_errors",
        "sources_errors",
    ]
    .iter()
    .enumerate()
    {
        if let Some(value) = report.facts.get(*key) {
            let Some(errors) = value.as_u64() else {
                return false;
            };
            selected = true;
            if errors > 0 {
                expected |= 1 << bit;
            }
        }
    }
    selected && exit == Some(expected)
}

/// Count criterion-relevant matches separately from the producer's optional
/// unused-suppression hints. Captured configured rule IDs prevent a custom rule
/// named `unused-suppression` from being silently treated as auxiliary.
fn structural_counts(report: &Report) -> Option<(u64, u64)> {
    if report.format != "ast-grep-json-v1" {
        return None;
    }
    let rules = report
        .facts
        .get("scan_entities")?
        .get("rules")?
        .as_array()?;
    if rules.is_empty() || rules.iter().any(|r| r.as_str().is_none()) {
        return None;
    }
    let total = report.observations.len() as u64;
    if report.facts.get("finding_count")?.as_u64()? != total {
        return None;
    }
    let diagnostics = report
        .facts
        .get("auxiliary_diagnostics")
        .and_then(Value::as_array);
    let auxiliary = report
        .observations
        .iter()
        .filter(|observation| {
            observation.rule == "unused-suppression"
                && !rules
                    .iter()
                    .any(|rule| rule.as_str() == Some(observation.rule.as_str()))
                && diagnostics.is_some_and(|diagnostics| {
                    diagnostics.iter().any(|diagnostic| {
                        diagnostic.get("kind").and_then(Value::as_str)
                            == Some("unused_suppression_hint")
                            && diagnostic.get("pointer").and_then(Value::as_str)
                                == Some(observation.pointer.as_str())
                    })
                })
        })
        .count() as u64;
    Some((total - auxiliary, auxiliary))
}

pub fn assess(
    criterion: &Criterion,
    admission: &Admission,
    report: &Report,
    now: &Inputs,
) -> Assessment {
    use gate::{gate, Gate, Preconditions};
    let accepted = admission.accepted
        && [
            &criterion.id,
            &criterion.revision,
            &criterion.requirement,
            &criterion.success,
            &criterion.accepted_by,
        ]
        .iter()
        .all(|s| !s.trim().is_empty());
    let changed = admission.before.changed(now);
    let status = gate(Preconditions {
        captured: admission.captured,
        accepted,
        completed: admission.run_state == RunState::Completed,
        identified: admission.before.complete()
            && now.complete()
            && admission.input_closure_complete,
        stable: admission.before == admission.after,
        current: changed.is_empty(),
        report_complete: report.complete && admission.exit_code.is_some(),
    });
    match status {
        Gate::Imported => return answer(CheckState::Reported,
            "This report was imported. ThruLine did not observe its execution.",
            "Capture a fresh run to connect the report to the exact work."),
        Gate::Unaccepted => return answer(CheckState::Blocked,
            "The success criterion has not been accepted.",
            "Agree on what should work and what evidence will count."),
        Gate::Interrupted => return answer(CheckState::Blocked,
            "The tool did not complete a usable run.",
            "Inspect the execution receipt and retry after resolving the interruption."),
        Gate::Unidentified => return answer(CheckState::Unknown,
            "Some inputs needed to judge this check have not been identified.",
            "Complete the source, rule, dependency, data and environment bindings."),
        Gate::ChangedDuringRun => return answer(CheckState::Unknown,
            "Inputs changed while the check was running.",
            "Run the check again against one stable version of the work."),
        Gate::Stale => {
            let mut a = answer(CheckState::Stale,
                "The work or a check input has changed since this run.",
                "Run this criterion again against the current inputs.");
            a.changed_inputs = changed;
            return a;
        }
        Gate::IncompleteReport => return answer(CheckState::Unknown,
            &format!("This run cannot settle the requirement. {}", report.limitations.first().map(String::as_str).unwrap_or("The output does not establish complete coverage of this check.")),
            "Review what was skipped or disabled, then capture a complete run for the accepted scope."),
        Gate::Eligible => {}
    }
    // A structured result cannot turn an unrecognized termination into a
    // completed check. Preserve its observations, but abstain on the criterion.
    let known_exit = match report.format.as_str() {
        "cargo-mutants-json-v1" => matches!(admission.exit_code, Some(0 | 2)),
        "cargo-deny-json-v1" => cargo_deny_exit_matches(report, admission.exit_code),
        "ast-grep-json-v1"
        | "dependency-cruiser-json-v1"
        | "knip-json-v1"
        | "betterleaks-json-v1"
        | "osv-json-v1"
        | "semgrep-json-v1"
        | "opengrep-json-v1"
        | "ruff-json-v1"
        | "playwright-json-v1"
        | "schemathesis-json-v1" => {
            matches!(admission.exit_code, Some(0 | 1))
        }
        _ => true,
    };
    if !known_exit {
        return answer(CheckState::Unknown,
            "The process returned an exit status this adapter has not qualified. Its observations are retained, but the execution may be incomplete.",
            "Inspect the execution diagnostics, resolve the tool failure, and capture a complete run.");
    }
    if matches!(criterion.condition, Condition::Advisory) {
        let mut assessment = answer(CheckState::Reported,
            "The local agent's actions and proposal were captured. Submission and agent test claims do not verify the accepted product behavior.",
            "Review the native actions and diff, then run the frozen independent acceptance checks before applying a proposal.");
        assessment.proof_level = "captured_advisory".into();
        return assessment;
    }
    let (satisfied, meaningful) = match &criterion.condition {
        Condition::Advisory => (false, false),
        Condition::NoObservations => (
            report.observations.is_empty(),
            report
                .facts
                .get("finding_count")
                .and_then(Value::as_u64)
                .is_some(),
        ),
        Condition::NoStructuralMatches => match structural_counts(report) {
            Some((matches, _)) => (matches == 0, true),
            None => (false, false),
        },
        Condition::AllTestsPass { minimum } => {
            let total = report.facts.get("passed").and_then(Value::as_u64);
            let failed = report.facts.get("failed").and_then(Value::as_u64);
            let skipped = report.facts.get("skipped").and_then(Value::as_u64);
            (
                total.is_some_and(|n| n >= (*minimum).max(1))
                    && failed == Some(0)
                    && skipped == Some(0),
                total.is_some() && failed.is_some() && skipped.is_some(),
            )
        }
        Condition::Maximum { fact, value } => (
            report
                .facts
                .get(fact)
                .and_then(Value::as_f64)
                .is_some_and(|n| n <= *value),
            value.is_finite() && report.facts.get(fact).and_then(Value::as_f64).is_some(),
        ),
        Condition::Minimum { fact, value } => (
            report
                .facts
                .get(fact)
                .and_then(Value::as_f64)
                .is_some_and(|n| n >= *value),
            value.is_finite() && report.facts.get(fact).and_then(Value::as_f64).is_some(),
        ),
    };
    if !meaningful {
        return answer(
            CheckState::Unknown,
            "This report does not measure the accepted success criterion.",
            "Choose a verifier that measures this requirement directly.",
        );
    }
    // A clean-looking body must never override a failing process.
    if satisfied && admission.exit_code != Some(0) {
        return answer(
            CheckState::Unknown,
            "The report looks successful, but the process did not exit successfully.",
            "Inspect the execution error before relying on this report.",
        );
    }
    let mut assessment = if satisfied {
        answer(CheckState::Passed,"This run met the agreed tool condition. It does not establish that the whole product is correct.","Review the evidence and the adapter's qualification before using it as a release gate.")
    } else {
        answer(CheckState::Failed,"This run did not meet the agreed tool condition. Review the specific evidence before treating it as a product defect.","Inspect the cited result, correct the relevant work, and capture a new run.")
    };
    if matches!(&criterion.condition, Condition::NoStructuralMatches) {
        if let Some((matches, auxiliary)) = structural_counts(report) {
            assessment.explanation = format!("Selected structural-rule matches: {matches}. Auxiliary unused-suppression hints: {auxiliary}. {}", assessment.explanation);
            assessment.criterion_measurement = Some(serde_json::json!({
                "kind":"selected_structural_rule_matches","criterion_observations":matches,
                "auxiliary_diagnostics":auxiliary,"reported_observations":report.observations.len()
            }));
        }
    }
    assessment
}
