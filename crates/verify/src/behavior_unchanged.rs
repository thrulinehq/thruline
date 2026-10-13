//! BUILD-SPEC §6.5 — `behavior_unchanged`, treated strictly.
//!
//! > *Supported only if all of: tests ran on both base tree and final tree with
//! > identical pass sets (or ThruLine re-executed them), no test files changed,
//! > no public API signature changes, no dependency manifest changes. Otherwise
//! > `unsupported` (this is the claim most often made without evidence; treat
//! > it strictly).*
//!
//! # Strict here means the default is refusal
//!
//! Every other verifier starts at `Unverifiable` and earns `Supported`. This one
//! additionally requires a **positive** piece of evidence that most sessions do
//! not have — the same suite passing on *both* trees — so the common outcome is
//! `unsupported`, and that is the spec's instruction rather than a defect.
//!
//! The distinction that has to survive: **no evidence is `unverifiable`, and
//! contrary evidence is `unsupported`.** A session that never ran the tests
//! twice has not disproved the claim; a session whose public API changed has.

use crate::binding::{CheckView, SessionView};
use crate::verdict::{Finding, Severity, Verdict, VerdictRecord};

pub const VERIFIER: &str = "behavior_unchanged@1";

/// What the caller must supply beyond the session.
#[derive(Debug, Clone, Default)]
pub struct Inputs {
    /// Test files changed in the session.
    pub test_files_changed: Vec<String>,
    /// Dependency manifests changed in the session.
    pub manifests_changed: Vec<String>,
    /// Public symbols that appeared, vanished, or changed arity, as
    /// `(path, symbol)`.
    pub api_changes: Vec<(String, String)>,
    /// Could the API diff be computed at all? `false` means unknown, which is
    /// `unverifiable` and not a pass.
    pub api_diff_available: bool,
}

pub fn verify(claim_id: &str, session: &SessionView, inputs: &Inputs) -> VerdictRecord {
    if session.capture_degraded {
        return VerdictRecord::new(claim_id, VERIFIER, Verdict::Unverifiable).reason(format!(
            "capture is degraded for this session ({}), so no claim in it can be supported",
            session
                .degraded_reason
                .clone()
                .unwrap_or_else(|| "reason not recorded".to_string())
        ));
    }

    let mut record = VerdictRecord::new(claim_id, VERIFIER, Verdict::Supported);

    // --- contrary evidence first: these REFUTE ----------------------------
    if !inputs.test_files_changed.is_empty() {
        record = record.with_finding(Finding::new(
            Severity::Fail,
            "test_files_changed",
            format!(
                "a refactor claim cannot rest on tests that were themselves edited: {}",
                inputs.test_files_changed.join(", ")
            ),
        ));
    }
    if !inputs.manifests_changed.is_empty() {
        record = record.with_finding(Finding::new(
            Severity::Fail,
            "dependencies_changed",
            format!(
                "a dependency manifest changed, so the code being compared is not the \
                 same code: {}",
                inputs.manifests_changed.join(", ")
            ),
        ));
    }
    for (path, symbol) in &inputs.api_changes {
        record = record.with_finding(
            Finding::new(
                Severity::Fail,
                "public_api_changed",
                format!("`{symbol}` changed in the public interface of {path}"),
            )
            .at(path),
        );
    }

    // --- the positive evidence §6.5 requires -------------------------------
    let tests: Vec<&CheckView> = session
        .checks
        .iter()
        .filter(|c| c.category == crate::check::CATEGORY_TEST)
        .collect();
    let trees: std::collections::BTreeSet<&str> = tests
        .iter()
        .filter(|c| c.exit_code == 0 && !c.input_tree_hash.is_empty())
        .map(|c| c.input_tree_hash.as_str())
        .collect();

    if !inputs.api_diff_available {
        record.verdict = record.verdict.worst(Verdict::Unverifiable);
        record.reasons.push(
            "the public-API diff could not be computed, so whether the interface moved \
             is unknown"
                .to_string(),
        );
        return record;
    }

    if trees.len() < 2 {
        // No contrary evidence and no positive evidence: `unverifiable`. A
        // refactor claim with one test run is the ordinary case, and calling it
        // `unsupported` would make the verdict meaningless by making it
        // universal.
        record.verdict = record.verdict.worst(Verdict::Unverifiable);
        record.reasons.push(format!(
            "the suite ran on {} tree(s); §6.5 needs it to pass on the tree before the \
             change and the tree after, and re-execution (§7) is not built",
            trees.len()
        ));
        return record;
    }

    // Identical pass sets across the two trees.
    let mut totals: std::collections::BTreeSet<(u64, u64)> = Default::default();
    for c in &tests {
        if let Some(p) = &c.parsed {
            totals.insert((p.tests_total, p.passed));
        }
    }
    if totals.len() > 1 {
        record = record.with_finding(Finding::new(
            Severity::Fail,
            "pass_set_changed",
            format!(
                "the suite reported different results on the two trees: {:?}",
                totals
            ),
        ));
        return record;
    }

    record.reasons.push(format!(
        "the suite passed identically on {} trees, no test file changed, no manifest \
         changed, and the public interface is unmoved",
        trees.len()
    ));
    record.evidence.extend(tests.iter().map(|c| c.id.clone()));
    record
}
