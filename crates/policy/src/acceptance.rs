//! Explicit repository obligations, independent of anything an agent says.
//!
//! This module only validates the accepted contract and evaluates typed evidence.
//! It executes no commands, reads no transcripts and assigns no signer trust.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const VERSION: u32 = 1;
pub const MAX_DOCUMENT_BYTES: usize = 65_536;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid acceptance JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid acceptance contract: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    pub version: u32,
    pub id: String,
    pub revision: u32,
    pub objective: String,
    pub criteria: Vec<Criterion>,
    pub checks: Vec<Check>,
    pub protected_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Criterion {
    pub id: String,
    pub description: String,
    pub check_ids: Vec<String>,
    /// Human acceptance is separately attributed; it never becomes machine proof.
    pub human_acceptance_required: bool,
    /// What kind of observation may settle this criterion. Missing legacy
    /// declarations cannot establish broader behavioral completion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_kind: Option<EvidenceKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Behavior,
    Tests,
    Build,
    Artifact,
    Audit,
    Architecture,
    Resource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Test,
    Build,
    Lint,
    Typecheck,
    Artifact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Captured,
    Isolated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub id: String,
    pub category: Category,
    pub argv: Vec<String>,
    pub cwd: String,
    pub runner: String,
    pub mode: ExecutionMode,
    pub timeout_seconds: u32,
    pub artifacts: Vec<Artifact>,
    /// Opt-in local verification pack. The accepted document owns the oracle,
    /// scope and environment declaration; candidate output cannot change them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification: Option<Verification>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verification {
    pub scope: Vec<String>,
    /// Explicit local input files outside the Git source snapshot (for example
    /// an interpreter library or a dataset). Directories are deliberately not
    /// accepted as a claim of a closed dependency environment.
    pub environment_files: Vec<String>,
    pub environment_closed: bool,
    pub environment_description: String,
    #[serde(default)]
    pub cases: Vec<ProbeCase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<PackPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PackPlan {
    Architecture {
        root: String,
        check: ArchitectureCheck,
        policy: Option<ArchitecturePolicy>,
        /// Absent means the original Python import graph, preserving old plans.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        graph: Option<ArchitectureGraph>,
    },
    Resource {
        module: String,
        workload: String,
        size: u64,
        size_unit: String,
        warmup_cycles: u32,
        cycles: u32,
        repetitions: u32,
        sampled_peak_budget_bytes: u64,
        retained_growth_budget_bytes: u64,
    },
    Suite {
        directory: String,
        pattern: String,
        expected_tests: Vec<String>,
        exclusions: BTreeSet<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        claimed_counts: Option<SuiteCounts>,
    },
    Completion,
    Artifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchitectureCheck {
    Edges,
    Cycles,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchitectureGraph {
    PythonImports,
    RustPackages,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchitecturePolicy {
    pub id: String,
    /// Module prefixes map to named layers; the longest matching prefix wins.
    pub layers: std::collections::BTreeMap<String, String>,
    pub external_modules: BTreeSet<String>,
    pub forbidden: Vec<DependencyRule>,
    pub forbid_cycles: bool,
    pub cycle_exemptions: Vec<CycleExemption>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyRule {
    pub id: String,
    pub from: String,
    pub to: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CycleExemption {
    pub modules: BTreeSet<String>,
    pub reason: String,
}

/// An optional reported total to compare against captured execution. This is
/// the statement under examination, never an execution observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuiteCounts {
    pub passed: u64,
    pub failed: u64,
    pub skipped: u64,
    pub expected_failure: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeCase {
    pub id: String,
    pub description: String,
    pub input: serde_json::Value,
    pub expected: serde_json::Value,
    pub expected_exit: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub path: String,
    /// Absent means the criterion asks only for a file produced by this run.
    pub sha256: Option<String>,
}

pub fn document_digest(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b"thruline.acceptance.document.v1\0");
    h.update(bytes);
    hex_digest(&h.finalize())
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut out = String::from("sha256:");
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Parsed bytes and their digest cannot be replaced independently. In
/// particular, changing the objective while retaining an old digest must not
/// make old evidence satisfy a newly accepted document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    contract: Contract,
    bytes: Vec<u8>,
    digest: String,
}

impl Document {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        Ok(Self {
            contract: Contract::parse(bytes)?,
            bytes: bytes.to_vec(),
            digest: document_digest(bytes),
        })
    }

    pub fn contract(&self) -> &Contract {
        &self.contract
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }

    pub fn evaluate(&self, subject: &str, evidence: &[Evidence]) -> Result<Evaluation, Error> {
        self.contract.evaluate(&self.digest, subject, evidence)
    }
}

impl Contract {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err(Error::Invalid("document exceeds 64 KiB".into()));
        }
        let contract: Self = serde_json::from_slice(bytes)?;
        contract.validate()?;
        Ok(contract)
    }

    pub fn validate(&self) -> Result<(), Error> {
        require(self.version == VERSION, "unsupported contract version")?;
        require(self.revision > 0, "revision must be positive")?;
        identifier(&self.id)?;
        text(&self.objective, 2048, "objective")?;
        require(
            !self.criteria.is_empty(),
            "at least one criterion is required",
        )?;
        require(
            self.criteria.len() <= 64,
            "at most 64 criteria are supported",
        )?;
        require(self.checks.len() <= 128, "at most 128 checks are supported")?;
        require(
            self.protected_paths.len() <= 128,
            "too many protected paths",
        )?;
        let mut check_ids = BTreeSet::new();
        require(
            self.checks
                .iter()
                .filter(|c| {
                    matches!(
                        c.verification.as_ref().and_then(|v| v.pack.as_ref()),
                        Some(PackPlan::Completion)
                    )
                })
                .count()
                <= 1,
            "use one explicit completion audit per accepted contract",
        )?;
        for check in &self.checks {
            identifier(&check.id)?;
            require(check_ids.insert(check.id.as_str()), "duplicate check id")?;
            text(&check.runner, 128, "runner")?;
            if let Some(v) = &check.verification {
                if let Some(plan) = &v.pack {
                    require(
                        v.cases.is_empty(),
                        "pack plans and JSON examples are separate checks",
                    )?;
                    match plan {
                        PackPlan::Architecture {
                            root,
                            policy,
                            graph,
                            ..
                        } => {
                            path(root, true)?;
                            let root_manifest = if root == "." {
                                "Cargo.toml".to_string()
                            } else {
                                format!("{root}/Cargo.toml")
                            };
                            require(
                                v.scope.contains(root) || (matches!(graph, Some(ArchitectureGraph::RustPackages)) && v.scope.contains(&root_manifest)),
                                "architecture source root must be an explicit directory scope, or Rust must explicitly scope its root Cargo.toml",
                            )?;
                            let runner = if graph.is_some() {
                                "thruline-architecture-v4"
                            } else {
                                "thruline-python-architecture-v3"
                            };
                            require(check.category == Category::Lint && check.runner == runner && check.argv.len() == 1 && check.cwd == ".", "architecture plan requires its versioned owned graph adapter and repository-root cwd")?;
                            if let Some(p) = policy {
                                identifier(&p.id)?;
                                require(
                                    !p.layers.is_empty()
                                        && p.layers.len() <= 128
                                        && p.forbidden.len() <= 128
                                        && p.external_modules.len() <= 128
                                        && p.cycle_exemptions.len() <= 32,
                                    "architecture policy exceeds bounded rule limits",
                                )?;
                                for (prefix, layer) in &p.layers {
                                    text(prefix, 256, "module prefix")?;
                                    identifier(layer)?;
                                }
                                let mut ids = BTreeSet::new();
                                for rule in &p.forbidden {
                                    identifier(&rule.id)?;
                                    require(ids.insert(&rule.id), "duplicate dependency rule ID")?;
                                    require(
                                        p.layers.values().any(|v| v == &rule.from)
                                            && p.layers.values().any(|v| v == &rule.to),
                                        "dependency rule names an unmapped layer",
                                    )?;
                                }
                                for prefix in &p.external_modules {
                                    text(prefix, 256, "external module prefix")?;
                                }
                                for exemption in &p.cycle_exemptions {
                                    require(
                                        !exemption.modules.is_empty()
                                            && exemption.modules.len() <= 128,
                                        "cycle exemption must name its exact bounded component",
                                    )?;
                                    text(&exemption.reason, 1024, "cycle exemption reason")?;
                                }
                            }
                        }
                        PackPlan::Resource {
                            module,
                            workload,
                            size,
                            size_unit,
                            warmup_cycles,
                            cycles,
                            repetitions,
                            sampled_peak_budget_bytes,
                            retained_growth_budget_bytes,
                        } => {
                            path(module, false)?;
                            require(module.ends_with(".py") && check.category == Category::Lint && check.runner == "thruline-python-memory-v3" && check.argv.len() == 1 && check.cwd == ".", "resource plan requires the owned Python cycle-workload adapter and repository-root cwd")?;
                            require(
                                v.scope.contains(module),
                                "resource workload must be an explicit scoped input",
                            )?;
                            text(workload, 2048, "workload description")?;
                            text(size_unit, 128, "workload size unit")?;
                            require(*size > 0 && *sampled_peak_budget_bytes > 0 && *retained_growth_budget_bytes > 0, "resource workload and both budgets must be explicit positive values")?;
                            require((1..=10).contains(warmup_cycles) && (2..=20).contains(cycles) && (2..=5).contains(repetitions), "resource measurement requires 1–10 warmup cycles, 2–20 measured cycles and 2–5 independent process repetitions")?;
                        }
                        PackPlan::Suite {
                            directory,
                            pattern,
                            expected_tests,
                            exclusions,
                            claimed_counts,
                        } => {
                            path(directory, true)?;
                            text(pattern, 256, "test discovery pattern")?;
                            require(
                                !expected_tests.is_empty() && expected_tests.len() <= 256,
                                "accept 1–256 discovered test IDs",
                            )?;
                            let ids: BTreeSet<_> = expected_tests.iter().collect();
                            require(
                                ids.len() == expected_tests.len(),
                                "duplicate expected test ID",
                            )?;
                            for id in expected_tests {
                                text(id, 512, "test ID")?;
                            }
                            if let Some(counts) = claimed_counts {
                                require(
                                    [
                                        counts.passed,
                                        counts.failed,
                                        counts.skipped,
                                        counts.expected_failure,
                                    ]
                                    .iter()
                                    .all(|n| *n <= 256),
                                    "reported test counts exceed supported suite size",
                                )?;
                            }
                            require(
                                exclusions.iter().all(|id| expected_tests.contains(id)),
                                "exclusions must name accepted test IDs",
                            )?;
                            require(
                                check.category == Category::Test
                                    && check.runner == "thruline-unittest-v2",
                                "suite plan requires the collector-owned unittest adapter",
                            )?;
                            require(check.argv.len() == 1, "suite argv names only the accepted Python interpreter; discovery is supplied by the accepted plan")?;
                        }
                        PackPlan::Completion => require(
                            check.runner == "thruline-completion-v2"
                                && check.category == Category::Artifact,
                            "completion plan requires the internal completion evaluator",
                        )?,
                        PackPlan::Artifact => require(
                            check.category == Category::Build && !check.artifacts.is_empty(),
                            "artifact plan requires a build with named outputs",
                        )?,
                    }
                }
                require(
                    !v.scope.is_empty() && v.scope.len() <= 128,
                    "verification needs a bounded explicit scope",
                )?;
                for scope in &v.scope {
                    path(scope, true)?;
                }
                text(&v.environment_description, 2048, "environment description")?;
                require(
                    v.environment_files.len() <= 128,
                    "at most 128 external environment files",
                )?;
                for file in &v.environment_files {
                    text(file, 4096, "environment file")?;
                }
                require(v.cases.len() <= 32, "at most 32 observable cases")?;
                let mut ids = BTreeSet::new();
                for case in &v.cases {
                    identifier(&case.id)?;
                    require(ids.insert(&case.id), "duplicate observable case")?;
                    text(&case.description, 2048, "case description")?;
                    require(
                        (0..=255).contains(&case.expected_exit),
                        "expected exit must be 0–255",
                    )?;
                    require(
                        serde_json::to_vec(&case.input)?.len() <= 8192
                            && serde_json::to_vec(&case.expected)?.len() <= 8192,
                        "case input/output exceeds 8 KiB",
                    )?;
                }
                require(
                    v.cases.is_empty()
                        || (check.category == Category::Test
                            && check.runner == "thruline-json-cases-v1"),
                    "observable cases require the ThruLine JSON case runner",
                )?;
                require(
                    check.runner != "thruline-json-cases-v1" || !v.cases.is_empty(),
                    "JSON case runner requires accepted examples",
                )?;
            }
            path(&check.cwd, true)?;
            require(
                !check.argv.is_empty() && check.argv.len() <= 256,
                "argv must contain 1–256 arguments",
            )?;
            require(!check.argv[0].is_empty(), "executable cannot be empty")?;
            for argument in &check.argv {
                require(
                    argument.len() <= 8192 && !argument.contains('\0'),
                    "invalid command argument",
                )?;
            }
            require(
                (1..=3600).contains(&check.timeout_seconds),
                "timeout must be 1–3600 seconds",
            )?;
            require(check.artifacts.len() <= 64, "too many artifacts")?;
            if check.category == Category::Artifact
                && !matches!(
                    check.verification.as_ref().and_then(|v| v.pack.as_ref()),
                    Some(PackPlan::Completion)
                )
            {
                require(
                    !check.artifacts.is_empty(),
                    "artifact checks must name an artifact",
                )?;
            }
            let mut artifact_paths = BTreeSet::new();
            for artifact in &check.artifacts {
                path(&artifact.path, false)?;
                require(
                    artifact_paths.insert(&artifact.path),
                    "duplicate artifact path",
                )?;
                if let Some(hash) = &artifact.sha256 {
                    require(
                        hash.len() == 64
                            && hash
                                .bytes()
                                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                        "artifact sha256 must be 64 lowercase hex digits",
                    )?;
                }
            }
        }
        let mut criterion_ids = BTreeSet::new();
        let mut referenced = BTreeSet::new();
        for criterion in &self.criteria {
            identifier(&criterion.id)?;
            require(
                criterion_ids.insert(&criterion.id),
                "duplicate criterion id",
            )?;
            text(&criterion.description, 2048, "criterion description")?;
            require(
                !criterion.check_ids.is_empty() || criterion.human_acceptance_required,
                "criterion must require a check or human acceptance",
            )?;
            let mut unique = BTreeSet::new();
            for id in &criterion.check_ids {
                require(
                    check_ids.contains(id.as_str()),
                    "criterion references unknown check",
                )?;
                require(unique.insert(id), "duplicate check reference")?;
                referenced.insert(id.as_str());
            }
        }
        require(
            check_ids == referenced,
            "every check must belong to a criterion",
        )?;
        for protected in &self.protected_paths {
            path(protected, false)?;
        }
        Ok(())
    }

    /// Every required check gets a result even when the evidence list is empty.
    /// The caller supplies the independently determined current subject and
    /// accepted digest. Untrusted producer metadata must be validated upstream.
    fn evaluate(
        &self,
        digest: &str,
        subject: &str,
        evidence: &[Evidence],
    ) -> Result<Evaluation, Error> {
        self.validate()?;
        let mut results = Vec::new();
        for check in &self.checks {
            let matching: Vec<_> = evidence.iter().filter(|e| e.check_id == check.id).collect();
            let (state, reason, run_id) = match matching.as_slice() {
                [] => (State::Unverified, "required check has no evidence", None),
                [e] if digest.is_empty()
                    || subject.is_empty()
                    || e.contract_digest != digest
                    || e.subject != subject
                    || e.check_digest != check.digest()? =>
                {
                    (
                        State::Unverified,
                        "evidence belongs to different or unknown requirements or inputs",
                        Some(e.run_id.clone()),
                    )
                }
                [e] if e.run_id.is_empty() || !e.admissible => (
                    State::Unverified,
                    "capture, producer or artifact evidence is incomplete",
                    Some(e.run_id.clone()),
                ),
                [e] => match e.outcome {
                    Outcome::Passed => (
                        State::Verified,
                        "declared check passed for these inputs",
                        Some(e.run_id.clone()),
                    ),
                    Outcome::Failed => (
                        State::Failed,
                        "declared check failed for these inputs",
                        Some(e.run_id.clone()),
                    ),
                    Outcome::NotRun
                    | Outcome::Unparsed
                    | Outcome::TimedOut
                    | Outcome::Interrupted => (
                        State::Unverified,
                        "check did not produce an admissible deciding outcome",
                        Some(e.run_id.clone()),
                    ),
                },
                _ => (
                    State::Unverified,
                    "multiple selected results; resolve attempts explicitly",
                    None,
                ),
            };
            results.push(CheckResult {
                check_id: check.id.clone(),
                state,
                reason: reason.into(),
                run_id,
            });
        }
        let criteria: Vec<_> = self
            .criteria
            .iter()
            .map(|c| {
                let mut states: Vec<_> = results
                    .iter()
                    .filter(|r| c.check_ids.contains(&r.check_id))
                    .map(|r| r.state)
                    .collect();
                if c.human_acceptance_required {
                    // A human acceptance needs an attributed event, not a boolean
                    // supplied by a candidate. Until that event is joined, it is open.
                    states.push(State::Unverified);
                }
                CriterionResult {
                    criterion_id: c.id.clone(),
                    state: aggregate(&states),
                    human_acceptance_required: c.human_acceptance_required,
                }
            })
            .collect();
        let state = aggregate(&criteria.iter().map(|c| c.state).collect::<Vec<_>>());
        Ok(Evaluation {
            contract_digest: digest.into(),
            subject: subject.into(),
            state,
            checks: results,
            criteria,
        })
    }
}

impl Check {
    pub fn digest(&self) -> Result<String, Error> {
        let mut h = Sha256::new();
        h.update(b"thruline.acceptance.check.v1\0");
        h.update(serde_json::to_vec(self)?);
        Ok(hex_digest(&h.finalize()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Passed,
    Failed,
    NotRun,
    Unparsed,
    TimedOut,
    Interrupted,
}

/// Internal adapter input, deliberately not deserializable from candidate JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    pub check_id: String,
    pub check_digest: String,
    pub contract_digest: String,
    pub subject: String,
    pub run_id: String,
    pub outcome: Outcome,
    pub admissible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Verified,
    Failed,
    Unverified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckResult {
    pub check_id: String,
    pub state: State,
    pub reason: String,
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CriterionResult {
    pub criterion_id: String,
    pub state: State,
    pub human_acceptance_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evaluation {
    pub contract_digest: String,
    pub subject: String,
    pub state: State,
    pub checks: Vec<CheckResult>,
    pub criteria: Vec<CriterionResult>,
}

impl Evaluation {
    /// Semantic accounting for a portable assertion. This validates its shape
    /// and aggregate, not the truth or custody of its producer's evidence.
    pub fn validate_for_contract(&self, contract: &Contract) -> Result<(), Error> {
        contract.validate()?;
        require(
            self.checks.len() == contract.checks.len(),
            "required-check accounting is incomplete",
        )?;
        let mut ids = BTreeSet::new();
        for check in &self.checks {
            require(ids.insert(&check.check_id), "duplicate check result")?;
            require(
                contract.checks.iter().any(|c| c.id == check.check_id),
                "result references unknown check",
            )?;
            if check.state != State::Unverified {
                require(
                    check.run_id.as_ref().is_some_and(|id| !id.is_empty()),
                    "decided check has no producing run",
                )?;
            }
        }
        require(
            self.criteria.len() == contract.criteria.len(),
            "criterion accounting is incomplete",
        )?;
        let mut ids = BTreeSet::new();
        for result in &self.criteria {
            require(
                ids.insert(&result.criterion_id),
                "duplicate criterion result",
            )?;
            let criterion = contract
                .criteria
                .iter()
                .find(|c| c.id == result.criterion_id)
                .ok_or_else(|| Error::Invalid("result references unknown criterion".into()))?;
            let mut states: Vec<_> = self
                .checks
                .iter()
                .filter(|c| criterion.check_ids.contains(&c.check_id))
                .map(|c| c.state)
                .collect();
            if criterion.human_acceptance_required {
                states.push(State::Unverified);
            }
            require(
                result.human_acceptance_required == criterion.human_acceptance_required
                    && result.state == aggregate(&states),
                "criterion result contradicts its requirements",
            )?;
        }
        require(
            self.state == aggregate(&self.criteria.iter().map(|c| c.state).collect::<Vec<_>>()),
            "aggregate contradicts required criteria",
        )
    }
}

fn aggregate(states: &[State]) -> State {
    if states.contains(&State::Failed) {
        State::Failed
    } else if states.is_empty() || states.contains(&State::Unverified) {
        State::Unverified
    } else {
        State::Verified
    }
}

fn require(condition: bool, message: &str) -> Result<(), Error> {
    if condition {
        Ok(())
    } else {
        Err(Error::Invalid(message.into()))
    }
}

fn identifier(value: &str) -> Result<(), Error> {
    require(
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
        "ids must contain 1–128 ASCII letters, digits, dots, underscores or hyphens",
    )
}

fn text(value: &str, limit: usize, name: &str) -> Result<(), Error> {
    require(
        !value.trim().is_empty() && value.len() <= limit && !value.chars().any(char::is_control),
        &format!("{name} is empty, too long or contains control characters"),
    )
}

fn path(value: &str, allow_dot: bool) -> Result<(), Error> {
    if allow_dot && value == "." {
        return Ok(());
    }
    text(value, 4096, "path")?;
    require(
        !value.contains('\\')
            && !value.contains(':')
            && value
                .split('/')
                .all(|p| !p.is_empty() && p != "." && p != ".."),
        "paths must be normalized repository-relative paths",
    )
}

#[cfg(test)]
mod tests;
