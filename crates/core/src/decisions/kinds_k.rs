//! The eighteen suggested Family K standing rules.
//!
//! This catalog is a proposal surface, not an accepted decision or checker.
//! A rule requires a person's source, closed parameters and acceptance in the
//! append-only ledger before it can have an exact-version verdict. Coverage
//! says how much machinery exists today; it never converts a suggestion into
//! `holds`.

use super::checks::VerdictState;
use super::params::CheckParams;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KCoverage {
    /// No deterministic Decisions verifier for this full rule exists yet.
    Unimplemented,
    /// Related evidence/checks exist, but the whole rule is not checkable.
    Partial,
    /// Reserved for a future complete verifier with positive/negative fixtures.
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KProposalState {
    ProposedAdvisory,
}

#[derive(Debug, Clone)]
pub struct KProposal {
    pub code: &'static str,
    pub rule: &'static str,
    /// Handover's intended producing checks, whether implemented or planned.
    pub producing_checks: &'static [&'static str],
    pub coverage: KCoverage,
    pub coverage_note: &'static str,
    pub state: KProposalState,
    /// Always absent in the catalog. The person binds exact scope separately.
    pub accepted_params: Option<CheckParams>,
    /// Always absent for a proposal; only an accepted check can yield one.
    pub verdict: Option<VerdictState>,
}

#[derive(Clone, Copy)]
struct KTemplate {
    code: &'static str,
    rule: &'static str,
    producing_checks: &'static [&'static str],
    coverage: KCoverage,
    coverage_note: &'static str,
}

const TEMPLATES: [KTemplate; 18] = [
    KTemplate {
        code: "K1",
        rule: "Tests are not skipped or focused to get green.",
        producing_checks: &["B2 test_present_and_runs"],
        coverage: KCoverage::Partial,
        coverage_note: "B2 checks a person-named test and exact run; broad skipped/focused test deltas across languages are not implemented.",
    },
    KTemplate {
        code: "K2",
        rule: "Test files are not deleted while their source remains.",
        producing_checks: &["A4 inverse", "B2 feature-removal evidence"],
        coverage: KCoverage::Partial,
        coverage_note: "B2 recognizes a person-scoped feature and tests removed together; a general test-file deletion census with move netting is absent.",
    },
    KTemplate {
        code: "K3",
        rule: "Assertions are not removed or weakened.",
        producing_checks: &["B3", "assertion_weakened"],
        coverage: KCoverage::Partial,
        coverage_note: "B3 counts assertions from exact Git blobs through the shared verifier; unsupported syntax or dialects abstain, and assertion-weakening linkage remains absent.",
    },
    KTemplate {
        code: "K4",
        rule: "Assertions are not tautological.",
        producing_checks: &["assertion census"],
        coverage: KCoverage::Partial,
        coverage_note: "A scoped exact-tree Rust named-test procedure checks empty bodies and direct absolute standard assertions independently of B2 execution; helpers, control flow and ambiguous syntax abstain. The advisory delta scanner remains separate; semantic tautologies and a complete language census are not certified.",
    },
    KTemplate {
        code: "K5",
        rule: "Production code does not special-case a test environment.",
        producing_checks: &["test-environment source-pattern delta"],
        coverage: KCoverage::Partial,
        coverage_note: "An exact-commit advisory scanner signals narrow production-path test-environment branches; it cannot infer purpose or actor.",
    },
    KTemplate {
        code: "K6",
        rule: "Test inputs are not hard-coded as answer tables.",
        producing_checks: &["fixture-input lookup census"],
        coverage: KCoverage::Partial,
        coverage_note: "A bounded exact-tree Python scanner links new literal lookup tables to stable literal test assertions through one namespace import; execution, intent and other syntax remain unproved.",
    },
    KTemplate {
        code: "K7",
        rule: "Equality operators and call counters are not changed solely for tests.",
        producing_checks: &["comparison and counter delta census"],
        coverage: KCoverage::Partial,
        coverage_note: "An exact-commit advisory scanner signals a narrow constant Python equality form; call-counter coverage and intent proof are absent.",
    },
    KTemplate {
        code: "K8",
        rule: "CI is not weakened.",
        producing_checks: &["B4"],
        coverage: KCoverage::Partial,
        coverage_note: "B4 parses bounded exact-tree Actions workflows from the accepted baseline; dynamic or ambiguous syntax and other CI dialects remain unavailable.",
    },
    KTemplate {
        code: "K9",
        rule: "Coverage thresholds are not lowered.",
        producing_checks: &["B4"],
        coverage: KCoverage::Partial,
        coverage_note: "B4 preserves supported literal Actions thresholds from the accepted baseline; external configuration and dynamic thresholds remain unavailable.",
    },
    KTemplate {
        code: "K10",
        rule: "Type and lint suppressions are not added.",
        producing_checks: &["suppression delta census"],
        coverage: KCoverage::Partial,
        coverage_note: "An exact-commit advisory scanner recognizes narrow suppression additions; complete language-aware coverage is absent.",
    },
    KTemplate {
        code: "K11",
        rule: "Errors are not swallowed.",
        producing_checks: &["empty-handler and discarded-result delta census"],
        coverage: KCoverage::Partial,
        coverage_note: "An exact-commit advisory scanner recognizes narrow empty handlers; discarded-result coverage is absent.",
    },
    KTemplate {
        code: "K12",
        rule: "Placeholder implementations are not shipped as done.",
        producing_checks: &["placeholder census outside test paths"],
        coverage: KCoverage::Partial,
        coverage_note: "An exact-commit advisory scanner recognizes narrow placeholder forms; TODO text alone is not proof of a shipped stub.",
    },
    KTemplate {
        code: "K13",
        rule: "Agent sessions do not edit ThruLine rule files.",
        producing_checks: &["B5", "D2 paths_untouched"],
        coverage: KCoverage::Partial,
        coverage_note: "D2 can check accepted paths against recorded edits, but B5 actor attribution and person-edit control are absent.",
    },
    KTemplate {
        code: "K14",
        rule: "Credentials are absent from the tracked tree.",
        producing_checks: &["secrets_absent", "credential-shape masker"],
        coverage: KCoverage::Partial,
        coverage_note: "A masked secret-shape claim scanner exists; a full exact-tree Decisions verifier is absent.",
    },
    KTemplate {
        code: "K15",
        rule: "Verification bypass commands are not used.",
        producing_checks: &["F1"],
        coverage: KCoverage::Partial,
        coverage_note: "F1 proves positive literal invocation witnesses from verified raw source and assured captures; absence coverage, source-delta detection and a bound first-use rule remain absent.",
    },
    KTemplate {
        code: "K16",
        rule: "A pass claim rests on a run on the same version.",
        producing_checks: &["E1 command_passes", "J2"],
        coverage: KCoverage::Partial,
        coverage_note: "E1 checks a bound command and exact-version run; pass-claim attribution to that run is absent.",
    },
    KTemplate {
        code: "K17",
        rule: "Spec and plan files are not rewritten to fit changed code.",
        producing_checks: &["B5", "D2 paths_untouched"],
        coverage: KCoverage::Partial,
        coverage_note: "D2 can check accepted paths against recorded edits, but B5 person-edit control and fit-to-code attribution are absent.",
    },
    KTemplate {
        code: "K18",
        rule: "Accepted rule-file checks continue across sessions.",
        producing_checks: &["M1", "K13", "G2", "F1"],
        coverage: KCoverage::Unimplemented,
        coverage_note: "Rule-file ingestion and cross-session enforcement are not registered.",
    },
];

/// Suggested defaults for a first-use surface. Returning these rows performs
/// no ledger write, acceptance, check, model call or verdict inference.
pub fn proposed_defaults() -> Vec<KProposal> {
    TEMPLATES
        .iter()
        .map(|template| KProposal {
            code: template.code,
            rule: template.rule,
            producing_checks: template.producing_checks,
            coverage: template.coverage,
            coverage_note: template.coverage_note,
            state: KProposalState::ProposedAdvisory,
            accepted_params: None,
            verdict: None,
        })
        .collect()
}
