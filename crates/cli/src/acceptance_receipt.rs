//! Version-specific accepted-requirement receipts. V0.1 remains a separate
//! session-claim protocol; it does not acquire these stronger accounting fields.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thruline_attest::{Envelope, PAYLOAD_TYPE};
use thruline_policy::acceptance::{Category, Contract, Evaluation, Outcome, State};

pub const PREDICATE_TYPE: &str = thruline_attest::session_v2::PREDICATE_TYPE;
const LIMIT: u64 = 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Statement {
    #[serde(rename = "_type")]
    type_: String,
    subject: Vec<Subject>,
    #[serde(rename = "predicateType")]
    predicate_type: String,
    pub predicate: Record,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Subject {
    name: String,
    digest: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Origin {
    LocalCapture,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    version: u32,
    kind: String,
    workspace: String,
    contract: Contract,
    contract_digest: String,
    acceptance_hash: String,
    accepted_at: String,
    evaluation: Evaluation,
    state: State,
    origin: Origin,
    producer_version: String,
    producer_binary_sha256: String,
    protected_inputs_verified: bool,
    protected_changes: Option<Vec<String>>,
    protected_input_error: Option<String>,
    runs: Vec<Run>,
    next_action: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Run {
    check_id: String,
    check_digest: String,
    run_id: String,
    chain_head: String,
    input_tree: String,
    input_commit: Option<String>,
    executable_digest: String,
    current: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    current_inputs_identified: Option<bool>,
    admissible: bool,
    outcome: Outcome,
    trust: String,
    environment_boundary: String,
    runner_version: String,
    counts: Option<crate::acceptance_exec::Counts>,
    exit_code: Option<i32>,
    outputs: Option<Vec<crate::acceptance_exec::Output>>,
    artifacts: Option<BTreeMap<String, String>>,
    reasons: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    binding_changes: Vec<Value>,
}

fn digest(value: &str, prefix: bool) -> bool {
    let value = if prefix {
        value.strip_prefix("sha256:").unwrap_or("")
    } else {
        value
    };
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn tree(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Record {
    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.version == 2 && self.kind == "accepted_requirements",
            "unknown required result version or kind"
        );
        self.evaluation.validate_for_contract(&self.contract)?;
        anyhow::ensure!(
            !self.producer_version.is_empty() && digest(&self.producer_binary_sha256, false),
            "producing verifier identity is unavailable"
        );
        anyhow::ensure!(
            digest(&self.workspace, false)
                && digest(&self.contract_digest, true)
                && digest(&self.acceptance_hash, false),
            "invalid result identity digest"
        );
        anyhow::ensure!(
            self.contract_digest == self.evaluation.contract_digest
                && self.state == self.evaluation.state,
            "result identity or aggregate contradicts its evaluation"
        );
        anyhow::ensure!(tree(&self.evaluation.subject), "invalid result subject");
        if self.state == State::Verified {
            anyhow::ensure!(
                self.protected_inputs_verified
                    && self.protected_input_error.is_none()
                    && self.protected_changes.as_ref().is_some_and(Vec::is_empty),
                "satisfied requirements contradict the protected-input check"
            );
        }
        let mut ids = BTreeSet::new();
        let mut checks = BTreeSet::new();
        for run in &self.runs {
            anyhow::ensure!(
                ids.insert(&run.run_id) && checks.insert(&run.check_id),
                "duplicate run or check binding"
            );
            let check = self
                .contract
                .checks
                .iter()
                .find(|c| c.id == run.check_id)
                .context("run references unknown required check")?;
            let result = self
                .evaluation
                .checks
                .iter()
                .find(|r| r.check_id == run.check_id)
                .context("run has no required result")?;
            anyhow::ensure!(
                result.run_id.as_ref() == Some(&run.run_id)
                    && run.check_digest == check.digest()?,
                "run identity differs from the required result"
            );
            anyhow::ensure!(
                digest(&run.chain_head, true)
                    && digest(&run.executable_digest, false)
                    && tree(&run.input_tree)
                    && run.trust == "local",
                "unsupported producing evidence identity or trust"
            );
            if let Some(outputs) = &run.outputs {
                anyhow::ensure!(
                    outputs.len() <= 2 && outputs.iter().all(|o| digest(&o.digest, true)),
                    "invalid output digest or stream count"
                );
            }
            if let Some(counts) = &run.counts {
                let sum = [counts.passed, counts.failed, counts.skipped, counts.xfail]
                    .into_iter()
                    .try_fold(0u64, |n, v| n.checked_add(v));
                anyhow::ensure!(
                    sum == Some(counts.total),
                    "test counts do not sum to the recorded total"
                );
            }
            if result.state != State::Unverified {
                anyhow::ensure!(
                    run.current
                        && run.admissible
                        && run.input_tree == self.evaluation.subject
                        && run.reasons.is_empty()
                        && run
                            .outputs
                            .as_ref()
                            .is_some_and(|outputs| outputs.len() == 2
                                && outputs.iter().all(|o| o.complete && o.redactions == 0)),
                    "decided result has incomplete or inapplicable evidence"
                );
            }
            if result.state == State::Verified {
                anyhow::ensure!(
                    run.outcome == Outcome::Passed && run.exit_code == Some(0),
                    "passed result contradicts the child outcome"
                );
                for artifact in &check.artifacts {
                    let observed = run.artifacts.as_ref().and_then(|a| a.get(&artifact.path));
                    anyhow::ensure!(
                        observed.is_some_and(|value| digest(value, false)
                            && artifact
                                .sha256
                                .as_ref()
                                .is_none_or(|expected| expected == value)),
                        "passed result lacks a required artifact digest"
                    );
                }
                if check.category == Category::Test {
                    anyhow::ensure!(
                        run.counts
                            .as_ref()
                            .is_some_and(|c| c.total > 0 && c.passed > 0 && c.failed == 0),
                        "test pass has no exercised passing tests"
                    );
                }
            }
            if result.state == State::Failed {
                anyhow::ensure!(
                    run.outcome == Outcome::Failed,
                    "failed result contradicts its evidence outcome"
                );
            }
        }
        for result in &self.evaluation.checks {
            if let Some(id) = &result.run_id {
                anyhow::ensure!(ids.contains(id), "required result references a missing run");
            }
        }
        Ok(())
    }
}

impl Statement {
    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.type_ == thruline_attest::statement::STATEMENT_TYPE
                && self.predicate_type == PREDICATE_TYPE,
            "unknown attestation type"
        );
        self.predicate.validate()?;
        anyhow::ensure!(
            self.subject.len() == 1
                && self.subject[0].name == self.predicate.workspace
                && self.subject[0].digest.len() == 1
                && self.subject[0].digest.get("gitTree")
                    == Some(&self.predicate.evaluation.subject),
            "attestation subject differs from its requirement result"
        );
        Ok(())
    }
}

/// One portable projection, used both when signing and when comparing a local
/// ledger with a received result. Free-form objectives and actor labels stay local.
pub fn record(repo: &Path, db: &Path, file: &Path) -> Result<Record> {
    let body = crate::acceptance_cmd::status_document(repo, db, file)?;
    anyhow::ensure!(
        body["accepted"] == true,
        "no accepted requirements to attest"
    );
    let mut contract: Contract = serde_json::from_value(body["contract"].clone())?;
    contract.objective = "Verify the explicitly declared checks".into();
    for criterion in &mut contract.criteria {
        criterion.description = format!("Declared criterion {}", criterion.id);
    }
    let mut result = Record {
        version: 2,
        kind: "accepted_requirements".into(),
        workspace: thruline_ledger::RepoId::from_key(&repo.to_string_lossy())
            .as_str()
            .into(),
        contract,
        contract_digest: body["contract_digest"]
            .as_str()
            .context("missing contract digest")?
            .into(),
        acceptance_hash: body["acceptance_hash"]
            .as_str()
            .context("missing acceptance identity")?
            .into(),
        accepted_at: body["accepted_at"]
            .as_str()
            .context("missing acceptance time")?
            .into(),
        evaluation: serde_json::from_value(body["evaluation"].clone())?,
        state: serde_json::from_value(body["state"].clone())?,
        origin: Origin::LocalCapture,
        producer_version: env!("CARGO_PKG_VERSION").into(),
        producer_binary_sha256: crate::acceptance_exec::file_digest(
            &std::env::current_exe()?,
            256 * 1024 * 1024,
        )?,
        protected_inputs_verified: body["protected_inputs_verified"] == true,
        protected_changes: serde_json::from_value(body["protected_changes"].clone())?,
        protected_input_error: serde_json::from_value(body["protected_input_error"].clone())?,
        runs: serde_json::from_value(body["runs"].clone())?,
        next_action: body["next_action"]
            .as_str()
            .context("missing next action")?
            .into(),
    };
    // Never silently alter an identity-bearing argv while still signing its
    // original digest. Reject sensitive command metadata instead.
    let redactor = thruline_capture::redaction::Redactor::from_repo(repo)?;
    for check in &result.contract.checks {
        anyhow::ensure!(
            thruline_capture::exec::redact_argv(&redactor, &check.argv) == check.argv,
            "sensitive command metadata cannot be shared"
        );
        if let Some(verification) = &check.verification {
            let raw = serde_json::to_string(verification)?;
            anyhow::ensure!(
                redactor.redact(&raw).into_text() == raw,
                "sensitive accepted example or scope metadata cannot be shared"
            );
        }
    }
    for run in &mut result.runs {
        run.reasons = run
            .reasons
            .iter()
            .map(|reason| redactor.redact(reason).into_text())
            .collect();
    }
    result.validate()?;
    Ok(result)
}

pub fn write(repo: &Path, db: &Path, file: &Path, destination: Option<&Path>) -> Result<Value> {
    let result = record(repo, db, file)?;
    let home = db
        .parent()
        .context("execution store has no parent")?
        .join("receipts");
    let key_dir = db
        .parent()
        .context("execution store has no parent")?
        .join(format!(
            "{}.acceptance",
            db.file_name()
                .context("execution store has no filename")?
                .to_string_lossy()
        ));
    let private = key_dir.join("signing.key");
    let public = key_dir.join("signing.pub");
    let signing = if private.try_exists()? {
        thruline_attest::key::load_signing(&private)?
    } else {
        thruline_attest::key::generate(&private, &public)?
    };
    let subject = Subject {
        name: result.workspace.clone(),
        digest: BTreeMap::from([("gitTree".into(), result.evaluation.subject.clone())]),
    };
    let statement = Statement {
        type_: thruline_attest::statement::STATEMENT_TYPE.into(),
        subject: vec![subject],
        predicate_type: PREDICATE_TYPE.into(),
        predicate: result,
    };
    statement.validate()?;
    let bytes = thruline_core::spec::canonical::canonical_bytes(&statement)?;
    let envelope = thruline_attest::sign(
        &bytes,
        PAYLOAD_TYPE,
        &signing,
        Some(thruline_attest::key_id(&signing.verifying_key())),
    );
    let envelope_bytes = serde_json::to_vec_pretty(&envelope)?;
    let hash = thruline_core::chain::sha256_hex(&envelope_bytes);
    let path = destination
        .map(Path::to_path_buf)
        .unwrap_or_else(|| home.join(format!("{hash}.json")));
    let absolute = crate::acceptance_cmd::resolved_path(&path)?;
    // A detached file is required; creating it in the source would invalidate
    // the very current-subject assertion being signed.
    let parent = absolute
        .parent()
        .context("receipt needs a parent directory")?;
    anyhow::ensure!(
        !absolute.starts_with(repo),
        "write the receipt outside the checked source tree"
    );
    std::fs::create_dir_all(parent)?;
    anyhow::ensure!(
        !parent.canonicalize()?.starts_with(repo),
        "write the receipt outside the checked source tree"
    );
    let opened = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&absolute);
    match opened {
        Ok(mut output) => {
            output.write_all(&envelope_bytes)?;
            output.sync_all()?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = std::fs::read(&absolute)?;
            anyhow::ensure!(
                existing == envelope_bytes,
                "receipt destination already contains different bytes"
            );
        }
        Err(error) => return Err(error).context("creating a detached receipt without overwriting"),
    }
    Ok(
        json!({"path":absolute,"sha256":hash,"public_key":public,"predicate_type":PREDICATE_TYPE,
        "state":statement.predicate.state,"trust":"local","next_action":"Verify with the explicit public key; signature validity does not establish independent custody."}),
    )
}

pub fn is_v2(envelope: &Envelope) -> bool {
    envelope
        .payload_bytes()
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .is_some_and(|v| {
            v["predicateType"] == PREDICATE_TYPE
                && v["predicate"]["kind"] == "accepted_requirements"
        })
}

pub fn verify(args: crate::receipt_cmd::VerifyArgs, envelope: Envelope) -> Result<ExitCode> {
    anyhow::ensure!(
        envelope.payload_type == PAYLOAD_TYPE,
        "receipt payload type is not in-toto"
    );
    let bytes = envelope.payload_bytes()?;
    anyhow::ensure!(
        bytes.len() <= LIMIT as usize,
        "receipt payload exceeds 1 MiB"
    );
    let statement: Statement = serde_json::from_slice(&bytes)?;
    statement.validate()?;
    let key_path = args.key.as_ref().context("v0.2 receipt verification needs an explicit --key; a producer does not choose its own trusted identity")?;
    let key = thruline_attest::key::load_verifying(key_path)?;
    envelope.verify(&[key])?;
    let repo = args
        .repo
        .unwrap_or(std::env::current_dir()?)
        .canonicalize()?;
    let db = args
        .exec_db
        .unwrap_or(thruline_ledger::Ledger::default_path()?);
    let local = record(&repo, &db, &repo.join(".thruline/acceptance.json"));
    let matches = match local {
        Ok(local) => serde_json::to_value(local)? == serde_json::to_value(&statement.predicate)?,
        Err(_) => false,
    };
    let reason = if matches {
        "The signed result reproduces from this local acceptance history and current evidence."
    } else {
        "The signature is valid, but this machine could not reproduce the same result from its current acceptance history and evidence."
    };
    let state = if matches {
        statement.predicate.state
    } else {
        State::Unverified
    };
    let body = json!({"signature_valid":true,"state":state,"trust":if matches {"local"} else {"untrusted"},
        "current_evidence_matches":matches,"reason":reason,"signed_result":statement.predicate});
    let format = if args.json {
        "json"
    } else {
        args.format.as_str()
    };
    match format {
        "json" => crate::json_out::print(&crate::json_out::VERIFY_ACCEPTANCE_RECEIPT, body)?,
        "text" | "md" => println!("Signature valid. {reason}\nRequirements: {state:?}. Evidence trust: {}.", if matches { "local" } else { "untrusted" }),
        _ => anyhow::bail!("this v0.2 acceptance reader supports text, md and json; hosted enforcement formats remain unqualified"),
    }
    Ok(ExitCode::from(match state {
        State::Verified => 0,
        State::Failed => 1,
        State::Unverified => 2,
    }))
}

pub fn read(path: &Path) -> Result<Envelope> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(16 * LIMIT + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() <= 16 * LIMIT as usize, "receipt exceeds 16 MiB");
    Ok(serde_json::from_slice(&bytes)?)
}
