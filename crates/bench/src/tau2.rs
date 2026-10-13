//! §12 class 1, against τ2-bench — the first corpus that is genuinely
//! third-party trajectories with third-party oracle labels.
//!
//! `sierra-research/tau2-bench` publishes `data/tau2/results/final/`: runs
//! Sierra performed across model families, each simulation carrying the
//! benchmark's own `reward_info` — its reward function, database check, action
//! checks and natural-language assertions. Third-party trajectories, third-party
//! oracle. That is class 1 by the amendment's definition, and it needs no
//! protocol commitment because we did not run it.
//!
//! # The label mapping is mechanical, and its error direction is stated
//!
//! §12's amendment: *labels come only from the benchmark's own oracle, never
//! from us*. τ2-bench's oracle emits `reward_info.reward`, so:
//!
//! - `reward >= 1.0` → [`Label::HonestPass`]: the oracle says the work was done.
//! - `reward < 1.0` → [`Label::FalseSuccess`]: the oracle says it was not.
//! - absent → [`Ground::Unlabelled`]. Never defaulted, in either direction.
//!
//! **The residual, stated because it matters and errs against this product.**
//! §12's `false_success` means *claimed success on work that was not correct*,
//! and τ2's reward supplies only the second half. Whether a claim was made is
//! ThruLine's own extraction, so using it to build the label would be measuring
//! the extractor against itself. The consequence is that a trajectory where the
//! agent honestly reported failure is labelled positive, and ThruLine correctly
//! declining to fire counts **against** the catch rate. The measured catch rate
//! is therefore a **lower bound**, and the direction of that conservatism is
//! toward reporting this product as worse than it is.
//!
//! # `task_id` is the honest-work shape
//!
//! §12's false-positive target is per shape, and a shape is a kind of honest
//! work. In τ2-bench each `task_id` is a distinct user scenario — a different
//! job with different tools — so it is the natural unit. A domain would give
//! one shape per file and the concentration bound would refuse it, correctly.

use std::collections::BTreeMap;
use std::path::Path;

use crate::normalize::{Corpus, LoadError, Trajectory};
use crate::{ExternalSource, Ground, Label, Provenance};

/// The publisher, dataset and label field that make this class 1.
pub const PUBLISHER: &str = "sierra-research";
pub const DATASET: &str = "tau2-bench";
/// The field in the published record that carries the ground truth. Named,
/// because "it has labels" is not checkable and this is.
pub const LABEL_FIELD: &str = "reward_info.reward";

/// Read one τ2-bench results file.
///
/// `expected_digest` is the sha256 recorded **at fetch**, and it is required:
/// under the amendment, class 1's evidence is a fetched-artifact digest, so a
/// corpus whose bytes cannot be tied to an upstream file is not class 1. A
/// mismatch is a refusal, not a warning.
/// Loads and reports the digest **without pinning it**.
///
/// [`load`] is the class-1 entry point: it takes the digest recorded at fetch
/// and refuses bytes that do not match, because that digest is the evidence
/// that these are the publisher's bytes. A caller that has no fetch record —
/// `thruline bench`, pointed at whatever is on disk — cannot perform that
/// check, and the honest thing is to say so rather than to pass the file's own
/// digest in and call the identity a verification.
///
/// So this returns the digest it computed alongside the corpus, for the caller
/// to print next to `fixtures/bench/download.sh`'s pins. **A number derived
/// from an unpinned load is a number about the bytes on this disk**, and a
/// caller using it must say that.
pub fn load_unpinned(path: &Path) -> Result<(Corpus, String), LoadError> {
    let bytes = std::fs::read(path).map_err(LoadError::Io)?;
    let digest = digest_of(&bytes);
    let corpus = load(path, &digest)?;
    Ok((corpus, digest))
}

pub fn load(path: &Path, expected_digest: &str) -> Result<Corpus, LoadError> {
    let bytes = std::fs::read(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            LoadError::NotDownloaded {
                source: "tau2-bench".to_string(),
                expected: path.to_path_buf(),
            }
        } else {
            LoadError::Io(e)
        }
    })?;
    let digest = digest_of(&bytes);
    if digest != expected_digest {
        return Err(LoadError::Shape(format!(
            "{} hashes to {digest} and the fetch recorded {expected_digest}. \
             Class 1's evidence is the upstream artifact's digest; bytes that do \
             not match it are not the published corpus.",
            path.display()
        )));
    }
    let doc: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| LoadError::Shape(e.to_string()))?;
    let sims = doc
        .get("simulations")
        .and_then(|v| v.as_array())
        .ok_or_else(|| LoadError::Shape("no `simulations` array".to_string()))?;

    let mut trajectories = Vec::with_capacity(sims.len());
    for (i, sim) in sims.iter().enumerate() {
        // The id must survive: two trials of one task are two trajectories.
        let task = sim
            .get("task_id")
            .map(render_id)
            .ok_or_else(|| LoadError::Shape(format!("simulation {i} has no `task_id`")))?;
        let trial = sim.get("trial").map(render_id).unwrap_or_default();
        // **The oracle, and only the oracle.** No fallback: a simulation the
        // reward function never scored is unlabelled, which is a fact about the
        // run rather than a verdict about the agent.
        let label = match sim
            .get("reward_info")
            .and_then(|r| r.get("reward"))
            .and_then(serde_json::Value::as_f64)
        {
            Some(r) if r >= 1.0 => {
                Ground::from_oracle(LABEL_FIELD, &r.to_string(), Label::HonestPass)
            }
            Some(r) => Ground::from_oracle(LABEL_FIELD, &r.to_string(), Label::FalseSuccess),
            None => Ground::unlabelled(format!(
                "τ2-bench's reward function produced no `reward_info.reward` \
                 for this simulation (termination: {})",
                sim.get("termination_reason")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("unrecorded")
            )),
        };
        trajectories.push(Trajectory {
            id: format!("{task}#{trial}"),
            task_id: Some(task.clone()),
            label,
            shape: task,
            raw: sim.clone(),
        });
    }

    // **Publish-everything, as arithmetic rather than as a promise.** Every
    // simulation in the file becomes a trajectory or the load fails; there is
    // no path that drops one. This is the assertion that says so, and it is
    // cheap enough to keep in the loader rather than in a test that could be
    // run against a different corpus than the one shipped.
    if trajectories.len() != sims.len() {
        return Err(LoadError::Shape(format!(
            "{} simulations in the artifact and {} trajectories loaded. A \
             benchmark corpus that drops rows is a selected corpus.",
            sims.len(),
            trajectories.len()
        )));
    }

    Ok(Corpus {
        name: format!(
            "tau2-bench/{}",
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
        ),
        provenance: Provenance::ExternalLabelled(
            ExternalSource::new(PUBLISHER, DATASET, &digest, LABEL_FIELD)
                .map_err(|e| LoadError::Shape(e.to_string()))?,
        ),
        version: digest.clone(),
        artifact_digest: Some(digest),
        trajectories,
    })
}

/// The holdout, derived from the artifact's own digest.
///
/// Class 2 fixes its holdout in the committed protocol. Class 1 has no
/// protocol, so the split has to come from something equally unchosen: the
/// **digest of the file itself**, which was fixed by whoever published it.
/// Anyone with the file recomputes the same split, and it cannot be re-drawn
/// after seeing results without changing the file.
pub fn holdout(corpus: &Corpus, fraction: f64) -> Vec<String> {
    let Some(digest) = &corpus.artifact_digest else {
        return Vec::new();
    };
    let mut tasks: Vec<String> = corpus
        .trajectories
        .iter()
        .filter_map(|t| t.task_id.clone())
        .collect();
    tasks.sort();
    tasks.dedup();
    let cut = (u64::MAX as f64 * fraction.clamp(0.0, 1.0)) as u64;
    tasks
        .into_iter()
        .filter(|task| bucket(digest, task) < cut)
        .collect()
}

/// A stable 64-bit bucket for `task` under `salt`.
fn bucket(salt: &str, task: &str) -> u64 {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update([0u8]);
    h.update(task.as_bytes());
    let d = h.finalize();
    let mut b = [0u8; 8];
    b.copy_from_slice(&d[..8]);
    u64::from_be_bytes(b)
}

fn digest_of(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    let d = h.finalize();
    let mut hex = String::new();
    for byte in d {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

fn render_id(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// What ThruLine's evidence model needs, and what τ2-bench contains.
///
/// Reported rather than assumed: this is the measurement that decides whether
/// a §12 number over this corpus means anything.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvidenceCensus {
    pub trajectories: usize,
    /// Tool calls of any kind.
    pub tool_calls: usize,
    /// Distinct tool names.
    pub tools: BTreeMap<String, usize>,
    /// Calls that execute a **code check** — the evidence class every §6
    /// verifier reads.
    pub code_check_executions: usize,
    /// Which tools those were, so the count is auditable.
    pub code_check_tools: BTreeMap<String, usize>,
    /// Tools whose *name* suggests execution and which are not code checks —
    /// `run_speed_test`, `check_status_bar`. Recorded so a reader can judge the
    /// exclusion instead of trusting a keyword list they cannot see.
    pub near_misses: BTreeMap<String, usize>,
}

/// Count what evidence a τ2-bench corpus actually carries.
pub fn evidence_census(corpus: &Corpus) -> EvidenceCensus {
    // **Exact names, not substrings.** The first version matched substrings and
    // counted `run_speed_test` — a telecom device diagnostic — as a code check,
    // 53 times, because the name contains "test". That is the same class of
    // error as reading a runner off a command string instead of an argv, which
    // §13 already forbids elsewhere in this build; the census caught its own
    // author.
    //
    // What ThruLine's verifiers read is an **execution record of an arbitrary
    // command**. A fixed domain API named after a test is not one. So this is
    // the set of tool names that grant arbitrary execution, matched whole.
    const ARBITRARY_EXECUTION: &[&str] = &[
        "bash",
        "shell",
        "exec",
        "execute",
        "exec_command",
        "execute_command",
        "run_command",
        "run_shell_command",
        "terminal",
        "container_exec",
    ];
    // Named runners, also whole: a tool literally called `pytest` is a code
    // check even though it is not a shell.
    const NAMED_RUNNER: &[&str] = &[
        "pytest",
        "cargo_test",
        "go_test",
        "npm_test",
        "make_test",
        "run_tests",
        "gradle",
        "mvn",
        "dotnet_test",
        "jest",
        "vitest",
        "mocha",
    ];
    let mut c = EvidenceCensus {
        trajectories: corpus.trajectories.len(),
        ..EvidenceCensus::default()
    };
    for t in &corpus.trajectories {
        let Some(messages) = t.raw.get("messages").and_then(|m| m.as_array()) else {
            continue;
        };
        for m in messages {
            for call in m
                .get("tool_calls")
                .and_then(|t| t.as_array())
                .into_iter()
                .flatten()
            {
                let name = call
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                c.tool_calls += 1;
                let lower = name.to_ascii_lowercase();
                if ARBITRARY_EXECUTION.contains(&lower.as_str())
                    || NAMED_RUNNER.contains(&lower.as_str())
                {
                    c.code_check_executions += 1;
                    *c.code_check_tools.entry(name.clone()).or_insert(0) += 1;
                } else if lower.contains("test") || lower.contains("run") || lower.contains("exec")
                {
                    // Recorded, not counted. A reader auditing "zero code-check
                    // executions" should be able to see what was *nearly*
                    // counted and judge the exclusion, rather than trusting a
                    // keyword list they cannot see.
                    *c.near_misses.entry(name.clone()).or_insert(0) += 1;
                }
                *c.tools.entry(name).or_insert(0) += 1;
            }
        }
    }
    c
}
