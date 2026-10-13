//! Local verification pack v1. Reuses accepted contracts, the append-only
//! execution ledger, dirty-tree snapshots, bounded capture and existing parsers.
//! Only this collector constructs observations; imported reports never enter it.
use crate::acceptance_exec::{self as exec, Counts, Finish, Inputs, Output};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::Read,
    path::Path,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use thruline_capture::{bounded, redaction::Redactor};
use thruline_ledger::{acceptance::Revision, Ledger, RepoId};
use thruline_policy::acceptance::{Category, Check, Document, Outcome};

pub const VERSION: &str = "local-verification-pack/3";
pub const IDS: &[&str] = &[
    "EVD-04", "EVD-06", "EVD-07", "EVD-10", "EVD-12", "FUN-01", "FUN-04", "EVD-05", "TST-01",
    "REQ-03", "DEL-01", "ARC-01", "ARC-02", "PER-02",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub id: String,
    pub description: String,
    pub input: Option<Value>,
    pub expected: Option<Value>,
    pub observed: Option<Value>,
    pub expected_exit: i32,
    pub exit: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<i32>,
    pub outcome: Outcome,
    pub complete: bool,
    pub reason: String,
    pub counts: Option<Counts>,
    pub outputs: Vec<Output>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<bounded::ResourceObservation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub version: String,
    #[serde(default)]
    pub authority: Option<String>,
    pub scope: Vec<String>,
    pub origin: String,
    pub verifier: String,
    pub limits: Value,
    pub observations: Vec<Observation>,
    pub limitations: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consistency: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub review_notes: Vec<String>,
}

pub fn limits(timeout: u32) -> Value {
    json!({"wall_seconds_total":timeout,"cpu_seconds_per_process":timeout,"bytes_per_stream":bounded::OUTPUT_LIMIT,
        "bytes_per_pack":16*1024*1024,"file_size_bytes_max":64*1024*1024,"open_files":128,"case_count_max":32,
        "parallel_checks_per_project":1,"parallel_checks_per_store":1,"core_dumps":false,"memory_hard_limit":null,
        "materialized_source_bytes_max":crate::verification_scope::MAX_BYTES,"source_files_max":crate::verification_scope::MAX_FILES,
        "retained_stream_bytes_per_pack_max":32*1024*1024,"artifact_bytes_per_pack_max":64*1024*1024,"store_admission_bytes_max":512*1024*1024,
        "boundary":"Local execution, not a sandbox. No enforced RAM or network isolation. Descendants that escape the process group are outside cancellation coverage."})
}

fn capture_json(repo: &Path, db: &Path, value: &Value) -> Result<Vec<Output>> {
    let store = exec::storage(db)?.join("outputs");
    let redactor = Redactor::from_repo(repo)?;
    let bytes = serde_json::to_vec(value)?;
    anyhow::ensure!(
        bytes.len() <= 1024 * 1024,
        "collector evaluation exceeds 1 MiB"
    );
    let mut outputs = vec![];
    for text in [&bytes[..], &[][..]] {
        let mut sink =
            thruline_capture::stream::CaptureSink::new(redactor.clone(), Some(&store), "out");
        sink.push(text);
        let captured = sink.finish();
        outputs.push(Output {
            digest: captured.hash,
            bytes: captured.bytes_redacted,
            redactions: captured.redactions_applied,
            complete: captured.store_error.is_none()
                && captured.redactions_applied == 0
                && !captured.block_cap_hit,
        });
    }
    Ok(outputs)
}

pub fn execute(
    repo: &Path,
    db: &Path,
    revision: &Revision,
    doc: &Document,
    check: &Check,
    before: &Inputs,
    cancel: &AtomicBool,
) -> Result<Finish> {
    let v = check
        .verification
        .as_ref()
        .context("missing pack declaration")?;
    let mut finish = Finish { version:1, note:"acceptance_run_finish".into(), exit_code:None, timed_out:false,
        outcome:Outcome::NotRun, counts:None, outputs:vec![], artifacts:BTreeMap::new(), inputs_after:None,
        reasons:before.limitations.clone(), pack:Some(Capture {version:VERSION.into(), authority:None, scope:v.scope.clone(),
            origin:"collector-owned local execution".into(), verifier:format!("{} / {}", VERSION, env!("CARGO_PKG_VERSION")),
            limits:limits(check.timeout_seconds), observations:vec![], consistency:None, review_notes:crate::verification_checks::test_changes(repo,revision.source_tree.as_deref(),&before.tree), limitations:vec![
                "Evidence covers the accepted command and finite examples only; passing examples do not prove all possible behavior.".into(),
                "Local developer trust. Execution uses a private captured source copy, not a hostile-code sandbox. File identity and change times detect observed edit/revert activity; a hostile process with control of the same account or filesystem is outside this trust boundary.".into(),
                "Dependency closure is declared in the accepted contract. The whole Git working tree and named external files are invalidation inputs; undeclared external inputs are outside this boundary.".into()
            ]}) };
    match exec::protected_changes(repo, revision, doc, &before.tree) {
        Ok(changes) if changes.is_empty() => {}
        _ => finish.reasons.push(
            "the accepted oracle or protected-input baseline changed or cannot be checked".into(),
        ),
    }
    if !finish.reasons.is_empty() {
        return Ok(finish);
    }
    let started = Instant::now();
    let timeout = Duration::from_secs(check.timeout_seconds.into());
    let store = exec::storage(db)?.join("outputs");
    let redactor = Redactor::from_repo(repo)?;
    let mut retained = 0;
    for folder in ["outputs", "artifacts"] {
        let path = exec::storage(db)?.join(folder);
        if path.exists() {
            for e in std::fs::read_dir(path)? {
                retained += e?.metadata()?.len();
            }
        }
    }
    if retained > 416 * 1024 * 1024 {
        finish.reasons.push("The evidence store reached its retained-output admission budget. Archive this store and explicitly choose a new store; no historical evidence was deleted.".into());
        return Ok(finish);
    }
    if matches!(
        crate::verification_checks::plan(check),
        Some(thruline_policy::acceptance::PackPlan::Completion)
    ) {
        let value = crate::verification_checks::completion_observation(repo, db, doc, check)?;
        let (outcome, reason) = crate::verification_checks::completion_result(&value);
        let outputs = capture_json(repo, db, &value)?;
        let complete = outputs.iter().all(|o| o.complete);
        if !complete {
            finish.reasons.push("The captured evaluation was redacted or incomplete. Its original interpretation cannot establish a pass.".into());
        }
        finish.outcome = if complete { outcome } else { Outcome::Unparsed };
        finish.outputs = outputs.clone();
        if let Some(p) = finish.pack.as_mut() {
            p.observations.push(Observation {
                id: check.id.clone(),
                description: "Account for every accepted product criterion".into(),
                input: None,
                expected: None,
                observed: Some(value),
                expected_exit: 0,
                exit: None,
                signal: None,
                outcome: finish.outcome,
                complete,
                reason,
                counts: None,
                outputs,
                resources: None,
                command: None,
            });
            p.consistency = Some(
                json!({"mode":"captured evidence evaluation","scope":"accepted criteria and latest admissible run references"}),
            );
        }
        finish.inputs_after = exec::inputs(repo, db, check, false).ok();
        if finish.inputs_after.as_ref() != Some(before) {
            finish.reasons.push(
                "Required inputs or referenced check attempts changed during the completion audit."
                    .into(),
            );
        }
        let cancelled = cancel.load(Ordering::SeqCst);
        if cancelled || started.elapsed() >= timeout {
            finish.timed_out = !cancelled;
            finish.outcome = if cancelled {
                Outcome::Interrupted
            } else {
                Outcome::TimedOut
            };
            finish.reasons.push("The evidence evaluation was cancelled or exceeded its time budget. Retained observations cannot establish a completed result.".into());
            if let Some(p) = finish.pack.as_mut() {
                for o in &mut p.observations {
                    o.outcome = finish.outcome;
                }
            }
        }
        return Ok(finish);
    }
    let source = match crate::verification_scope::SourceCopy::prepare(repo, before, check) {
        Ok(s) => s,
        Err(error) => {
            finish.reasons.push(format!(
                "A consistent source copy could not be established: {error}"
            ));
            return Ok(finish);
        }
    };
    let driver = tempfile::tempdir()?;
    if let Some(script) = crate::verification_structural::driver(check) {
        std::fs::write(driver.path().join("structural_driver.py"), script)?;
    }
    if matches!(
        crate::verification_checks::plan(check),
        Some(thruline_policy::acceptance::PackPlan::Suite { .. })
    ) {
        std::fs::write(
            driver.path().join("unittest_driver.py"),
            crate::verification_checks::UNITTEST,
        )?;
    }
    if let Some(p) = finish.pack.as_mut() {
        p.consistency = Some(
            json!({"mode":"private source copy","input_tree":before.tree,"source_copy":source.directory.path(),"hostile_code_isolation":false,"result_applies_only_if_inputs_unchanged":true}),
        );
    }
    let mut captured_bytes = 0;
    // The parent verifier supplies and evaluates each accepted example. A
    // target can print text, but cannot supply the expected value or a verdict.
    let cases: Vec<_> = if v.cases.is_empty() {
        vec![None]
    } else {
        v.cases.iter().map(Some).collect()
    };
    let mut observations = Vec::new();
    for case in cases {
        if started.elapsed() >= timeout
            || cancel.load(Ordering::SeqCst)
            || captured_bytes >= 16 * 1024 * 1024
        {
            break;
        }
        let mut command = source.command(repo, before, check)?;
        if let Some(plan @ thruline_policy::acceptance::PackPlan::Suite { .. }) =
            crate::verification_checks::plan(check)
        {
            command = Command::new(&before.executable);
            command
                .args(["-I", "-B"])
                .arg(driver.path().join("unittest_driver.py"))
                .arg(serde_json::to_string(plan)?)
                .current_dir(source.directory.path());
        }
        if crate::verification_structural::driver(check).is_some() {
            command = Command::new(&before.executable);
            command
                .args(["-I", "-B"])
                .arg(driver.path().join("structural_driver.py"))
                .arg(serde_json::to_string(&crate::verification_checks::plan(
                    check,
                ))?)
                .current_dir(source.directory.path());
        }
        let input = case.map(|c| serde_json::to_vec(&c.input)).transpose()?;
        let mut observation = Observation {
            id: case.map_or_else(|| check.id.clone(), |c| c.id.clone()),
            description: case.map_or_else(
                || match crate::verification_checks::plan(check) {
                    Some(thruline_policy::acceptance::PackPlan::Architecture { check, .. }) => {
                        match check {
                            thruline_policy::acceptance::ArchitectureCheck::Edges => {
                                "Check the accepted module boundaries"
                            }
                            thruline_policy::acceptance::ArchitectureCheck::Cycles => {
                                "Look for prohibited dependency cycles"
                            }
                        }
                        .into()
                    }
                    Some(thruline_policy::acceptance::PackPlan::Resource { .. }) => {
                        "Measure the scheduled memory workload".into()
                    }
                    _ => "Run the exact accepted command".into(),
                },
                |c| c.description.clone(),
            ),
            input: case.map(|c| c.input.clone()),
            expected: case.map(|c| c.expected.clone()),
            observed: None,
            expected_exit: case.map_or(0, |c| c.expected_exit),
            exit: None,
            signal: None,
            outcome: Outcome::NotRun,
            complete: false,
            reason: String::new(),
            counts: None,
            outputs: vec![],
            resources: None,
            command: Some(
                json!({"program":command.get_program().to_string_lossy(),"argv":command.get_args().map(|a|a.to_string_lossy()).collect::<Vec<_>>(),"cwd":command.get_current_dir()}),
            ),
        };
        let report = bounded::run_controlled(
            command,
            timeout.saturating_sub(started.elapsed()),
            redactor.clone(),
            &store,
            cancel,
            input,
        );
        match report {
            Err(_) => {
                observation.reason =
                    "The required process could not be started or observed.".into();
            }
            Ok(report) => {
                observation.resources = Some(report.resources.clone());
                observation.exit = report.status.code();
                #[cfg(unix)]
                {
                    use std::os::unix::process::ExitStatusExt;
                    observation.signal = report.status.signal();
                }
                let mut texts = Vec::new();
                for stream in [report.stdout, report.stderr].into_iter().flatten() {
                    let c = stream.captured;
                    captured_bytes += c.bytes_raw;
                    let complete = stream.complete
                        && !c.invalid_utf8
                        && !c.block_cap_hit
                        && c.store_error.is_none()
                        && c.redactions_applied == 0;
                    let bytes = c
                        .stored_path
                        .as_ref()
                        .and_then(|p| read_output(p, &c.hash).ok());
                    texts.push(
                        bytes
                            .as_ref()
                            .map(|b| String::from_utf8_lossy(b).into_owned())
                            .unwrap_or_default(),
                    );
                    observation.outputs.push(Output {
                        digest: c.hash,
                        bytes: c.bytes_redacted,
                        redactions: c.redactions_applied,
                        complete: complete && bytes.is_some(),
                    });
                }
                observation.complete = observation.outputs.len() == 2
                    && observation.outputs.iter().all(|o| o.complete)
                    && captured_bytes <= 16 * 1024 * 1024;
                observation.outcome = if report.cancelled {
                    Outcome::Interrupted
                } else if report.timed_out {
                    Outcome::TimedOut
                } else if !observation.complete || observation.exit.is_none() {
                    Outcome::Unparsed
                } else if observation.exit != Some(observation.expected_exit) {
                    Outcome::Failed
                } else {
                    Outcome::Passed
                };
                if case.is_some() && matches!(observation.outcome, Outcome::Passed) {
                    match texts
                        .first()
                        .filter(|s| s.len() <= 8192)
                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                    {
                        Some(value) => {
                            observation.outcome = if Some(&value) == observation.expected.as_ref() {
                                Outcome::Passed
                            } else {
                                Outcome::Failed
                            };
                            observation.observed = Some(value);
                        }
                        None => {
                            observation.outcome = Outcome::Unparsed;
                        }
                    }
                } else if case.is_none()
                    && observation.complete
                    && observation.exit.is_some()
                    && !matches!(
                        observation.outcome,
                        Outcome::TimedOut | Outcome::Interrupted
                    )
                {
                    if crate::verification_structural::driver(check).is_some() {
                        let value = texts
                            .first()
                            .and_then(|s| serde_json::from_str::<Value>(s).ok())
                            .unwrap_or(Value::Null);
                        observation.outcome = if observation.exit == Some(0) {
                            crate::verification_structural::result(check, &value).0
                        } else {
                            Outcome::Unparsed
                        };
                        observation.observed = Some(value);
                    } else if matches!(
                        crate::verification_checks::plan(check),
                        Some(thruline_policy::acceptance::PackPlan::Suite { .. })
                    ) {
                        let value = texts
                            .first()
                            .and_then(|s| serde_json::from_str::<Value>(s).ok())
                            .unwrap_or(Value::Null);
                        let (outcome, _) = crate::verification_checks::suite_result(check, &value);
                        if let Some(rows) = value["rows"].as_array() {
                            observation.counts = Some(Counts {
                                total: rows.len() as u64,
                                passed: rows.iter().filter(|r| r["state"] == "pass").count() as u64,
                                failed: rows
                                    .iter()
                                    .filter(|r| {
                                        matches!(
                                            r["state"].as_str(),
                                            Some("fail" | "error" | "unexpected_success")
                                        )
                                    })
                                    .count() as u64,
                                skipped: rows.iter().filter(|r| r["state"] == "skip").count()
                                    as u64,
                                xfail: rows
                                    .iter()
                                    .filter(|r| r["state"] == "expected_failure")
                                    .count() as u64,
                            });
                        }
                        observation.outcome = outcome;
                        observation.observed = Some(value);
                    } else if matches!(
                        crate::verification_checks::plan(check),
                        Some(thruline_policy::acceptance::PackPlan::Artifact)
                    ) {
                        observation.outcome = if observation.exit == Some(0) {
                            Outcome::Passed
                        } else {
                            Outcome::Failed
                        };
                    } else {
                        if check.category == Category::Test {
                            // Arbitrary shells printing runner-shaped summaries are
                            // reports, not evidence that the declared runner ran.
                            let base = check.argv[0].rsplit('/').next().unwrap_or("");
                            if matches!(base, "sh" | "bash" | "zsh" | "echo" | "printf") {
                                observation.outcome = Outcome::Unparsed;
                            } else {
                                match exec::parse_counts(check, &texts.join("\n")) {
                                    Ok(counts) => {
                                        // Parsed skips/TODOs remain observed counts even
                                        // when no test exercised a passing/failing case.
                                        observation.outcome = if counts.total == 0
                                            || counts.passed + counts.failed == 0
                                        {
                                            Outcome::Unparsed
                                        } else if counts.failed > 0 || observation.exit != Some(0) {
                                            Outcome::Failed
                                        } else {
                                            Outcome::Passed
                                        };
                                        observation.counts = Some(counts);
                                    }
                                    _ => {
                                        observation.outcome = Outcome::Unparsed;
                                    }
                                }
                            }
                        } else if !compiler_is_recognized(check) {
                            observation.outcome = Outcome::Unparsed;
                        }
                    }
                }
                observation.reason = match observation.outcome {
                    Outcome::Passed if case.is_some() => "The captured JSON value and process exit match this accepted example.",
                    Outcome::Passed => "The accepted runner completed successfully for this exact command and scope.",
                    Outcome::Failed if case.is_some() => "This example produced a different value or exit than the accepted requirement.",
                    Outcome::Failed => "The required runner reported a failure.",
                    Outcome::Interrupted => "Cancelled or interrupted. This attempt does not settle the requirement.",
                    Outcome::TimedOut => "The time budget expired. This is incomplete, not a product failure.",
                    _ => "The captured evidence is missing, incomplete, or not interpretable by this adapter. A success message is insufficient.",
                }.into();
                if matches!(
                    crate::verification_checks::plan(check),
                    Some(thruline_policy::acceptance::PackPlan::Suite { .. })
                ) && !matches!(
                    observation.outcome,
                    Outcome::TimedOut | Outcome::Interrupted
                ) {
                    observation.reason = crate::verification_checks::suite_result(
                        check,
                        observation.observed.as_ref().unwrap_or(&Value::Null),
                    )
                    .1;
                }
                if crate::verification_structural::driver(check).is_some()
                    && observation.exit == Some(0)
                    && !matches!(
                        observation.outcome,
                        Outcome::TimedOut | Outcome::Interrupted
                    )
                {
                    observation.reason = crate::verification_structural::result(
                        check,
                        observation.observed.as_ref().unwrap_or(&Value::Null),
                    )
                    .1;
                }
            }
        }
        let stop = !matches!(observation.outcome, Outcome::Passed | Outcome::Failed);
        observations.push(observation);
        if stop {
            break;
        }
    }
    let expected_count = v.cases.len().max(1);
    if matches!(
        crate::verification_checks::plan(check),
        Some(thruline_policy::acceptance::PackPlan::Artifact)
    ) && observations
        .first()
        .is_some_and(|o| o.outcome == Outcome::Passed)
    {
        let mut artifact_bytes = 0;
        for a in &check.artifacts {
            let path = source.directory.path().join(&a.path);
            let digest = exec::file_digest(&path, 64 * 1024 * 1024);
            match digest {
                Ok(digest) => {
                    artifact_bytes += std::fs::metadata(&path)?.len();
                    if artifact_bytes > 64 * 1024 * 1024 {
                        finish
                            .reasons
                            .push("Produced artifacts exceed the retained-artifact budget.".into());
                        break;
                    }
                    let target = exec::storage(db)?.join("artifacts");
                    std::fs::create_dir_all(&target)?;
                    std::fs::copy(&path, target.join(&digest))?;
                    finish.artifacts.insert(a.path.clone(), digest.clone());
                    if let Some(o) = observations.first_mut() {
                        if o.outcome == Outcome::Passed
                            && a.sha256.as_ref().is_some_and(|expected| {
                                expected.trim_start_matches("sha256:") != digest
                            })
                        {
                            o.outcome = Outcome::Failed;
                            o.reason=format!("The produced artifact {} has digest {}, which differs from the accepted digest. Rebuild from the declared inputs or review the expected artifact.",a.path,digest);
                        }
                    }
                }
                Err(_) => {
                    finish.reasons.push(format!("Required artifact {} was not produced or could not be fully captured. Success output alone is insufficient.",a.path));
                }
            }
        }
        if let Some(o) = observations.first_mut() {
            o.observed = Some(
                json!({"artifact_digests":finish.artifacts,"source_tree":before.tree,"outputs_absent_before_build":true}),
            );
        }
    }
    finish.outcome = aggregate(&observations, expected_count);
    finish.exit_code = observations.last().and_then(|o| o.exit);
    finish.timed_out = observations.iter().any(|o| o.outcome == Outcome::TimedOut);
    finish.counts = if v.cases.is_empty() {
        observations.first().and_then(|o| o.counts.clone())
    } else if observations
        .iter()
        .any(|o| !matches!(o.outcome, Outcome::Passed | Outcome::Failed))
    {
        None
    } else {
        Some(Counts {
            total: observations.len() as u64,
            passed: observations
                .iter()
                .filter(|o| o.outcome == Outcome::Passed)
                .count() as u64,
            failed: observations
                .iter()
                .filter(|o| o.outcome == Outcome::Failed)
                .count() as u64,
            skipped: 0,
            xfail: 0,
        })
    };
    finish.outputs = observations
        .iter()
        .flat_map(|o| o.outputs.clone())
        .collect();
    if observations.len() != expected_count {
        finish
            .reasons
            .push("not all accepted examples completed; the attempt is incomplete".into());
    }
    if observations.iter().any(|o| !o.complete) {
        finish
            .reasons
            .push("required output capture is incomplete".into());
    }
    if let Some(pack) = finish.pack.as_mut() {
        pack.observations = observations;
    }
    match source.unchanged(check){Ok(true)=>{},_=>finish.reasons.push("Source inputs changed inside the execution copy. Captured observations are retained, but no consistent-snapshot result can be admitted.".into())}
    match exec::inputs(repo, db, check, false) {
        Ok(after) => {
            if &after != before {
                finish
                    .reasons
                    .push("relevant inputs changed during execution".into());
            }
            finish.inputs_after = Some(after);
        }
        Err(_) => finish
            .reasons
            .push("inputs could not be fingerprinted after execution".into()),
    }
    Ok(finish)
}

fn compiler_is_recognized(check: &Check) -> bool {
    if !matches!(
        check.category,
        Category::Build | Category::Typecheck | Category::Lint
    ) {
        return false;
    }
    let base = check.argv[0].rsplit('/').next().unwrap_or("");
    if matches!(
        base,
        "sh" | "bash" | "zsh" | "npm" | "pnpm" | "yarn" | "npx"
    ) {
        return false;
    }
    // A bounded syntax compilation adapter; diagnostics and its target remain
    // in the receipt, and this never claims runtime behavior.
    if matches!(base, "python" | "python3")
        && check.argv.get(1).is_some_and(|s| s == "-m")
        && check.argv.get(2).is_some_and(|s| s == "py_compile")
    {
        return check.category == Category::Build && check.argv.len() > 3;
    }
    thruline_verify::check::tool_check_from_exec(&check.argv, 0).is_some_and(|c| {
        c.bypass.is_none()
            && match check.category {
                Category::Build => c.category == "build",
                Category::Typecheck => c.category == "typecheck",
                Category::Lint => c.category == "lint",
                _ => false,
            }
    })
}

fn aggregate(observations: &[Observation], expected: usize) -> Outcome {
    if observations.len() != expected || observations.is_empty() {
        Outcome::Interrupted
    } else if observations.iter().any(|o| o.outcome == Outcome::TimedOut) {
        Outcome::TimedOut
    } else if observations
        .iter()
        .any(|o| !matches!(o.outcome, Outcome::Passed | Outcome::Failed))
    {
        Outcome::Unparsed
    } else if observations.iter().any(|o| o.outcome == Outcome::Failed) {
        Outcome::Failed
    } else {
        Outcome::Passed
    }
}

pub(crate) fn read_output(path: &Path, digest: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(bounded::OUTPUT_LIMIT + 2)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() <= bounded::OUTPUT_LIMIT as usize + 1
            && format!("sha256:{}", thruline_core::chain::sha256_hex(&bytes)) == digest,
        "output integrity mismatch"
    );
    Ok(bytes)
}

/// Re-read bounded raw output and reapply the versioned rule. Stored verdict
/// words and agent text are not the deciding inputs to replay.
fn replay_observation(check: &Check, o: &Observation, db: &Path, version: &str) -> Result<Outcome> {
    if matches!(
        o.outcome,
        Outcome::TimedOut | Outcome::Interrupted | Outcome::NotRun
    ) {
        return Ok(o.outcome);
    }
    if !o.complete || o.outputs.len() != 2 {
        return Ok(Outcome::Unparsed);
    }
    if crate::verification_structural::driver(check).is_some() {
        return Ok(if o.exit == Some(0) {
            crate::verification_structural::result(
                check,
                &crate::verification_checks::stored_json(db, o)?,
            )
            .0
        } else {
            Outcome::Unparsed
        });
    }
    match crate::verification_checks::plan(check) {
        Some(thruline_policy::acceptance::PackPlan::Suite { .. }) => {
            return Ok(crate::verification_checks::suite_result(
                check,
                &crate::verification_checks::stored_json(db, o)?,
            )
            .0)
        }
        Some(thruline_policy::acceptance::PackPlan::Completion) => {
            return Ok(crate::verification_checks::completion_result(
                &crate::verification_checks::stored_json(db, o)?,
            )
            .0)
        }
        _ => {}
    }
    let mut texts = Vec::new();
    for output in &o.outputs {
        let digest = output
            .digest
            .strip_prefix("sha256:")
            .context("invalid stream digest")?;
        anyhow::ensure!(
            digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid digest"
        );
        let bytes = read_output(
            &exec::storage(db)?
                .join("outputs")
                .join(format!("{digest}.out")),
            &output.digest,
        )?;
        texts.push(String::from_utf8(bytes)?);
    }
    let Some(exit) = o.exit else {
        return Ok(Outcome::Unparsed);
    };
    if let Some(case) = check
        .verification
        .as_ref()
        .and_then(|v| v.cases.iter().find(|c| c.id == o.id))
    {
        if exit != case.expected_exit {
            return Ok(Outcome::Failed);
        }
        return Ok(
            match texts
                .first()
                .filter(|s| s.len() <= 8192)
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
            {
                Some(value) if value == case.expected => Outcome::Passed,
                Some(_) => Outcome::Failed,
                None => Outcome::Unparsed,
            },
        );
    }
    if check.category == Category::Test {
        if matches!(
            check.argv[0].rsplit('/').next(),
            Some("sh" | "bash" | "zsh" | "echo" | "printf")
        ) {
            return Ok(Outcome::Unparsed);
        }
        return Ok(match exec::parse_counts(check, &texts.join("\n")) {
            Ok(c)
                if c.total > 0
                    && (c.passed > 0
                        || matches!(
                            version,
                            "local-verification-pack/2" | "local-verification-pack/3"
                        ) && c.failed > 0) =>
            {
                if c.failed > 0 || exit != 0 {
                    Outcome::Failed
                } else {
                    Outcome::Passed
                }
            }
            _ => Outcome::Unparsed,
        });
    }
    if matches!(
        crate::verification_checks::plan(check),
        Some(thruline_policy::acceptance::PackPlan::Artifact)
    ) {
        if exit != 0 {
            return Ok(Outcome::Failed);
        }
        for a in &check.artifacts {
            let digest = o
                .observed
                .as_ref()
                .and_then(|v| v["artifact_digests"][&a.path].as_str())
                .context("artifact identity missing")?;
            anyhow::ensure!(
                digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()),
                "artifact digest malformed"
            );
            anyhow::ensure!(
                exec::file_digest(
                    &exec::storage(db)?.join("artifacts").join(digest),
                    64 * 1024 * 1024
                )? == digest,
                "artifact integrity mismatch"
            );
            if a.sha256
                .as_ref()
                .is_some_and(|expected| expected.trim_start_matches("sha256:") != digest)
            {
                return Ok(Outcome::Failed);
            }
        }
        return Ok(Outcome::Passed);
    }
    Ok(if !compiler_is_recognized(check) {
        Outcome::Unparsed
    } else if exit == 0 {
        Outcome::Passed
    } else {
        Outcome::Failed
    })
}

fn historical_state(
    check: &Check,
    finish: Option<&Finish>,
    db: &Path,
) -> (&'static str, String, bool) {
    let Some(f) = finish else {
        return ("unknown","This run started, but no finish was captured. An older pass cannot settle this attempt.".into(),false);
    };
    let Some(pack) = &f.pack else {
        return (
            "unknown",
            "This run predates the executable catalog pack.".into(),
            false,
        );
    };
    if !f.reasons.is_empty() {
        return (
            if f.outcome == Outcome::NotRun {
                "blocked"
            } else {
                "unknown"
            },
            f.reasons.join(" "),
            false,
        );
    }
    let mut replay = pack.observations.clone();
    for observation in &mut replay {
        match replay_observation(check, observation, db, &pack.version) {
            Ok(outcome) => observation.outcome = outcome,
            Err(_) => {
                return (
                    "unknown",
                    "An underlying evidence stream is unavailable or its integrity check failed."
                        .into(),
                    false,
                )
            }
        }
    }
    let expected = check
        .verification
        .as_ref()
        .map_or(1, |v| v.cases.len().max(1));
    let derived = aggregate(&replay, expected);
    if derived != f.outcome {
        return ("unknown","Replay does not agree with the stored outcome. Inspect the receipt; no verdict was admitted.".into(),false);
    }
    let state = match derived {
        Outcome::Passed => "pass",
        Outcome::Failed => "fail",
        Outcome::NotRun => "blocked",
        _ => "unknown",
    };
    let explanation=match state {
        "pass"=>"The required check met its accepted criterion for the examined inputs. This covers only the scope and examples listed here.",
        "fail"=>"The recorded result contradicts this accepted requirement. Open the receipt to see the exact mismatch.",
        "blocked"=>"The check could not start. Supply the missing input or tool, then rerun.",
        _=>"This attempt is incomplete. It does not prove success or a defect; resolve the capture or execution gap and rerun.",
    };
    let explanation = if crate::verification_structural::driver(check).is_some() {
        replay
            .first()
            .map(|o| {
                if matches!(o.outcome, Outcome::TimedOut | Outcome::Interrupted) {
                    o.reason.clone()
                } else {
                    crate::verification_structural::result(
                        check,
                        o.observed.as_ref().unwrap_or(&Value::Null),
                    )
                    .1
                }
            })
            .unwrap_or_else(|| explanation.into())
    } else {
        explanation.into()
    };
    (state, explanation, true)
}

pub fn receipt(repo: &Path, db: &Path, run: &str, include_output: bool) -> Result<Value> {
    anyhow::ensure!(
        run.starts_with("acceptance-")
            && run.len() < 128
            && run
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)),
        "invalid run identity"
    );
    let ledger = Ledger::open_read_only(db)?;
    let repo_id = RepoId::from_key(&repo.to_string_lossy());
    let belongs:bool=ledger.conn().query_row("SELECT EXISTS(SELECT 1 FROM sessions WHERE id=?1 AND repo_id=?2 AND harness='acceptance-local-v1')",[run,repo_id.as_str()],|r|r.get(0))?;
    anyhow::ensure!(belongs, "run does not belong to the configured project");
    let chain = ledger.verify_session(run)?;
    let events = ledger.session_events(run)?;
    anyhow::ensure!(
        !events.is_empty() && events.len() <= 2,
        "unsupported run history"
    );
    let mut start = events[0].payload().clone();
    let revisions = ledger.acceptance_history(&repo_id)?;
    let revision = revisions
        .iter()
        .find(|r| Some(r.hash.as_str()) == start["acceptance_hash"].as_str())
        .context("accepted requirement revision unavailable")?;
    let document = Document::parse(revision.document.as_bytes())?;
    let check = document
        .contract()
        .checks
        .iter()
        .find(|c| Some(c.id.as_str()) == start["check_id"].as_str())
        .context("accepted check unavailable")?;
    let finish = events
        .get(1)
        .map(|e| serde_json::from_value::<Finish>(e.payload().clone()))
        .transpose()?;
    let authenticated = finish
        .as_ref()
        .is_some_and(|f| verify_local_seal(db, run, &start, f));
    let (state, explanation, replayed) = if authenticated {
        historical_state(check, finish.as_ref(), db)
    } else {
        ("unknown","Local collector authority is missing or could not be verified. Imported or delegated reports cannot establish this result.".into(),false)
    };
    let recorded_state = finish.as_ref().map_or("unknown", |f| {
        if !f.reasons.is_empty() {
            if f.outcome == Outcome::NotRun {
                "blocked"
            } else {
                "unknown"
            }
        } else {
            match f.outcome {
                Outcome::Passed => "pass",
                Outcome::Failed => "fail",
                Outcome::NotRun => "blocked",
                _ => "unknown",
            }
        }
    });
    // This salted local secret-backed binding is compared internally but is
    // deliberately never exported in a receipt or an HTTP response.
    if let Some(inputs) = start["inputs"].as_object_mut() {
        inputs.remove("process_environment_private");
    }
    let mut safe_finish = serde_json::to_value(&finish)?;
    if let Some(inputs) = safe_finish["inputs_after"].as_object_mut() {
        inputs.remove("process_environment_private");
    }
    let criteria: Vec<_> = document
        .contract()
        .criteria
        .iter()
        .filter(|c| c.check_ids.contains(&check.id))
        .collect();
    let mut output = Vec::new();
    if include_output {
        if let Some(f) = &finish {
            for o in &f.outputs {
                let digest = o.digest.strip_prefix("sha256:").unwrap_or("");
                let bytes = if digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()) {
                    read_output(
                        &exec::storage(db)?
                            .join("outputs")
                            .join(format!("{digest}.out")),
                        &o.digest,
                    )
                    .ok()
                } else {
                    None
                };
                output.push(json!({"digest":o.digest,"metadata":o,"text":bytes.as_ref().map(|b|String::from_utf8_lossy(b).into_owned()),"available":bytes.is_some()}));
            }
        }
    }
    let recorded_version = finish
        .as_ref()
        .and_then(|f| f.pack.as_ref())
        .map_or("local-verification-pack/1", |p| p.version.as_str());
    let record = json!({"version":recorded_version,"run_id":run,"project":repo,"started_at":events[0].ts(),"finished_at":events.get(1).map(|e|e.ts()),
        "accepted_by":revision.accepted_by,"acceptance_hash":revision.hash,"contract_digest":document.digest(),"objective":document.contract().objective,
        "criterion":criteria,"check":check,"origin":"local collector; not an imported agent statement","trust":"local developer boundary; integrity is not proof of oracle authority",
        "start":start,"finish":safe_finish,"historical_state":recorded_state,"ledger_chain_head":chain.head_hash,
        "rule_version":recorded_version,"catalog_ids":applicable_ids(check)});
    let digest = format!(
        "sha256:{}",
        thruline_core::chain::sha256_hex(&serde_json::to_vec(&record)?)
    );
    let mut artifacts = vec![];
    if include_output {
        if let Some(f) = &finish {
            for (path, digest) in &f.artifacts {
                if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
                    continue;
                }
                let stored = exec::storage(db)?.join("artifacts").join(digest);
                let available =
                    exec::file_digest(&stored, 64 * 1024 * 1024).is_ok_and(|d| d == *digest);
                let size = std::fs::metadata(&stored).ok().map(|m| m.len());
                let mut preview = vec![];
                if available {
                    std::fs::File::open(&stored)?
                        .take(65536)
                        .read_to_end(&mut preview)?;
                }
                artifacts.push(json!({"path":path,"digest":digest,"available":available,"bytes":size,"preview":String::from_utf8(preview).ok(),"preview_truncated":size.is_some_and(|s|s>65536),"stored_path":stored}));
            }
        }
    }
    Ok(
        json!({"record":record,"receipt_digest":digest,"outputs":output,"artifacts":artifacts,"verification":{"state":state,"explanation":explanation,"replay_agrees":replayed}}),
    )
}

pub fn applicable_ids(check: &Check) -> Vec<&'static str> {
    if check.verification.is_none() {
        return vec![];
    }
    let mut ids = IDS[..5].to_vec();
    match crate::verification_checks::plan(check) {
        Some(thruline_policy::acceptance::PackPlan::Suite { .. }) => {
            ids.extend(["EVD-05", "TST-01"])
        }
        Some(thruline_policy::acceptance::PackPlan::Completion) => {
            ids.retain(|id| *id != "EVD-04");
            ids.push("REQ-03");
        }
        Some(thruline_policy::acceptance::PackPlan::Artifact) => ids.push("DEL-01"),
        Some(thruline_policy::acceptance::PackPlan::Architecture { check, .. }) => {
            ids.push(match check {
                thruline_policy::acceptance::ArchitectureCheck::Edges => "ARC-01",
                thruline_policy::acceptance::ArchitectureCheck::Cycles => "ARC-02",
            })
        }
        Some(thruline_policy::acceptance::PackPlan::Resource { .. }) => ids.push("PER-02"),
        _ => {}
    }
    if matches!(check.category, Category::Build | Category::Typecheck) {
        ids.push("FUN-01");
    }
    if check
        .verification
        .as_ref()
        .is_some_and(|v| !v.cases.is_empty())
    {
        ids.push("FUN-04");
    }
    ids
}

pub fn status(repo: &Path, db: &Path) -> Result<Value> {
    let file = repo.join(".thruline/acceptance.json");
    let mut status = crate::acceptance_cmd::status_document(repo, db, &file)?;
    status["pack_version"] = json!(VERSION);
    status["coverage"] = coverage()?;
    status["limits"] = limits(60);
    let mut history = Vec::new();
    if db.exists() {
        let ledger = Ledger::open_read_only(db)?;
        let repo_id = RepoId::from_key(&repo.to_string_lossy());
        let mut query=ledger.conn().prepare("SELECT s.id FROM sessions s JOIN events e ON e.session_id=s.id AND e.seq=1 WHERE s.repo_id=?1 AND s.harness='acceptance-local-v1' ORDER BY e.rowid DESC LIMIT 65")?;
        let mut sessions = query
            .query_map([repo_id.as_str()], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        status["history_has_more"] = json!(sessions.len() > 64);
        sessions.truncate(64);
        for run in status["runs"].as_array().into_iter().flatten() {
            if let Some(id) = run["run_id"].as_str() {
                if !sessions.iter().any(|s| s == id) {
                    sessions.push(id.into());
                }
            }
        }
        for session in sessions {
            // Preserve the earlier accepted revision's receipt across revisions.
            let r = receipt(repo, db, &session, false)?;
            if r["record"]["check"]["verification"].is_object() {
                history.push(r);
            }
        }
    }
    let runs = status["runs"].as_array().cloned().unwrap_or_default();
    let previous_invalidations = invalidations(repo, db)?;
    for run in &runs {
        if history
            .iter()
            .any(|h| h["record"]["run_id"] == run["run_id"])
            && run["current"] == false
            && run["current_inputs_identified"] == true
            && run["binding_changes"]
                .as_array()
                .is_some_and(|c| !c.is_empty())
            && !previous_invalidations
                .iter()
                .any(|i| i["record"]["run_id"] == run["run_id"])
        {
            record_invalidation(repo, db, run)?;
        }
    }
    status["invalidations"] = json!(invalidations(repo, db)?);
    let mut results = Vec::new();
    if let Some(checks) = status["contract"]["checks"].as_array() {
        for value in checks {
            let check: Check = serde_json::from_value(value.clone())?;
            if check.verification.is_none() {
                continue;
            }
            let run = runs.iter().find(|r| r["check_id"] == check.id);
            let receipt = run.and_then(|r| {
                history
                    .iter()
                    .find(|h| h["record"]["run_id"] == r["run_id"])
            });
            let mut state = receipt
                .and_then(|r| r["verification"]["state"].as_str())
                .unwrap_or("not_run");
            let mut reason = receipt
                .and_then(|r| r["verification"]["explanation"].as_str())
                .unwrap_or("Implemented and available. This accepted check has not run.")
                .to_string();
            if let Some(run) = run {
                if run["current_inputs_identified"] == false
                    && receipt.is_some_and(|r| {
                        r["record"]["start"]["inputs"]["limitations"]
                            .as_array()
                            .is_some_and(Vec::is_empty)
                    })
                {
                    state = "unknown";
                    reason="Current inputs or local observer authority cannot be identified. Resolve the missing evidence before deciding whether this result still applies.".into();
                } else if run["current"] == false
                    && receipt.is_some_and(|r| {
                        r["record"]["start"]["inputs"]["limitations"]
                            .as_array()
                            .is_some_and(Vec::is_empty)
                    })
                {
                    state = "stale";
                    reason="The code, dependency files, configuration, process environment, or verifier changed. The old receipt is preserved; rerun against the current inputs.".into();
                } else if run["admissible"] == false && matches!(state, "pass" | "fail") {
                    state = "unknown";
                    reason = run["reasons"]
                        .as_array()
                        .map(|r| {
                            r.iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                        .unwrap_or_else(|| "Evidence admission is incomplete.".into());
                }
            }
            if status["pending_draft_change"] == true {
                state = "stale";
                reason="The draft requirement changed. Review and accept the revised requirement before rerunning.".into();
            }
            let ids = applicable_ids(&check);
            let checks: Vec<_> = ids
                .iter()
                .map(|id| {
                    let status = if matches!(*id, "EVD-05" | "TST-01")
                        && matches!(state, "pass" | "fail")
                    {
                        let value = receipt
                            .and_then(|r| r["record"]["finish"]["pack"]["observations"].as_array())
                            .and_then(|a| a.first())
                            .map(|o| &o["observed"])
                            .unwrap_or(&Value::Null);
                        crate::verification_checks::suite_catalog_state(&check, value, id)
                    } else if (*id == "EVD-12"
                        && receipt.is_some_and(|r| r["verification"]["replay_agrees"] == true))
                        || (matches!(*id, "EVD-06" | "EVD-07" | "EVD-10")
                            && matches!(state, "pass" | "fail"))
                    {
                        "pass"
                    } else if *id == "EVD-04" && matches!(state, "pass" | "fail") {
                        if receipt
                            .and_then(|r| r["record"]["finish"]["pack"]["observations"].as_array())
                            .is_some_and(|os| os.iter().all(|o| o["exit"] == o["expected_exit"]))
                        {
                            "pass"
                        } else {
                            "fail"
                        }
                    } else {
                        state
                    };
                    json!({"check_id":id,"version":VERSION,"status":status})
                })
                .collect();
            let next=match state {"fail"=>"Inspect the recorded mismatch, repair the demonstrated problem, and rerun this accepted check.","stale"=>"Rerun against the current inputs; retain the previous receipt as history.","pass"=>"This pass supports only its declared scope. Check other accepted criteria before claiming completion.",_=>"Resolve the stated evidence or scope gap, then explicitly run the appropriate check."};
            results.push(json!({"id":check.id,"status":state,"explanation":reason,"next_action":next,"scope":check.verification.as_ref().map(|v|&v.scope),"scope_declaration":check.verification,"covered_inputs":receipt.map(|r|&r["record"]["start"]["inputs"]["scope_manifest"]),"review_notes":receipt.map(|r|&r["record"]["finish"]["pack"]["review_notes"]),"catalog_results":checks,"run_id":run.map(|r|&r["run_id"]),"command":check.argv,"criterion":status["contract"]["criteria"],"receipt_digest":receipt.map(|r|&r["receipt_digest"])}));
        }
    }
    status["pack_results"] = json!(results);
    status["project_coverage"] = json!({"accepted_checks":results.len(),"passed":results.iter().filter(|r|r["status"]=="pass").count(),"failed":results.iter().filter(|r|r["status"]=="fail").count(),"executed":results.iter().filter(|r|r["run_id"].is_string()).count(),"stale":results.iter().filter(|r|r["status"]=="stale").count(),"not_run":results.iter().filter(|r|r["status"]=="not_run").count(),"incomplete":results.iter().filter(|r|!["pass","fail","stale","not_run"].contains(&r["status"].as_str().unwrap_or("unknown"))).count(),"incomplete_or_not_run":results.iter().filter(|r|r["status"]!="pass" && r["status"]!="fail").count(),"broader_project_status":"unknown","explanation":"These counts cover accepted local checks only. A high pass rate here does not verify the rest of the project or the 134-check catalog."});
    status["history"] = json!(history);
    status["dependency_index"] = json!({"strategy":"conservative whole working tree + declared external files + executable + environment + accepted contract + verifier","affected_checks":results.iter().map(|r|&r["id"]).collect::<Vec<_>>(),"limitation":"Any source-tree change invalidates all checks for this project, including documentation-only edits. Precise dependency closure is deferred; no optimistic reuse."});
    Ok(status)
}

pub fn coverage() -> Result<Value> {
    let entries: Value =
        serde_json::from_str(include_str!("../../../docs/verification/coverage.json"))?;
    let rows = entries["checks"]
        .as_array()
        .context("invalid implementation registry")?;
    anyhow::ensure!(
        rows.len() == 134,
        "catalog coverage registry must contain all 134 checks"
    );
    let implemented = rows
        .iter()
        .filter(|r| r["availability"] == "implemented")
        .count();
    let validated = rows
        .iter()
        .filter(|r| r["validation_status"] == "validated")
        .count();
    Ok(
        json!({"total":rows.len(),"implemented":implemented,"validated":validated,"specification_only":rows.len()-implemented,"implementation_percent":100.0*implemented as f64/rows.len() as f64,"checks":rows}),
    )
}

pub fn run_accepted(
    repo: &Path,
    db: &Path,
    check: &str,
    digest: &str,
    cancel: &AtomicBool,
) -> Result<String> {
    let file = repo.join(".thruline/acceptance.json");
    let ledger = Ledger::open(db)?;
    let repo_id = RepoId::from_key(&repo.to_string_lossy());
    let revisions = ledger.acceptance_history(&repo_id)?;
    let revision = revisions
        .last()
        .context("accept an observable requirement first")?;
    let doc = Document::parse(revision.document.as_bytes())?;
    anyhow::ensure!(
        doc.digest() == digest,
        "accepted digest changed; review it before execution"
    );
    anyhow::ensure!(
        std::fs::read(&file)? == doc.bytes(),
        "draft differs from accepted requirement; review and accept it first"
    );
    let check = doc
        .contract()
        .checks
        .iter()
        .find(|c| c.id == check)
        .context("unknown accepted check")?;
    anyhow::ensure!(
        check.verification.is_some(),
        "this accepted check has no local verification declaration"
    );
    exec::run_controlled(repo, db, revision, &doc, check, cancel)
}

/// HMAC-SHA256, domain-separated from the process-environment binding. The key
/// is the existing 0600 local store secret and is never included in imports or
/// exported receipts. This rejects forged/imported collector-shaped notes; it
/// does not defend against an actor that controls the entire local user/store.
pub fn local_seal(db: &Path, run: &str, start: &Value, finish: &Finish) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut key = Vec::new();
    std::fs::File::open(exec::storage(db)?.join("environment.key"))?
        .take(33)
        .read_to_end(&mut key)?;
    anyhow::ensure!(key.len() == 32, "local observer authority is unavailable");
    let mut clean = finish.clone();
    if let Some(pack) = clean.pack.as_mut() {
        pack.authority = None;
    }
    let capture_version = finish
        .pack
        .as_ref()
        .map_or("local-verification-pack/1", |p| p.version.as_str());
    let message = serde_json::to_vec(&json!([
        capture_version,
        "collector-authority",
        run,
        start,
        clean
    ]))?;
    let mut inner = [0x36u8; 64];
    let mut outer = [0x5cu8; 64];
    for (i, b) in key.iter().enumerate() {
        inner[i] ^= b;
        outer[i] ^= b;
    }
    let mut hash = Sha256::new();
    hash.update(inner);
    hash.update(message);
    let inside = hash.finalize();
    let mut hash = Sha256::new();
    hash.update(outer);
    hash.update(inside);
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
fn constant_time_equal(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |n, (a, b)| n | (a ^ b)) == 0
}
pub fn verify_local_seal(db: &Path, run: &str, start: &Value, finish: &Finish) -> bool {
    finish
        .pack
        .as_ref()
        .and_then(|p| p.authority.as_ref())
        .is_some_and(|tag| {
            local_seal(db, run, start, finish)
                .is_ok_and(|expected| constant_time_equal(&expected, tag))
        })
}

/// Invalidation is a new observation in its own append-only stream. Run
/// receipts remain two-event immutable records and never acquire a new verdict.
pub fn invalidations(repo: &Path, db: &Path) -> Result<Vec<Value>> {
    if !db.exists() {
        return Ok(vec![]);
    }
    let ledger = Ledger::open_read_only(db)?;
    let id = format!(
        "verification-invalidations-{}",
        RepoId::from_key(&repo.to_string_lossy()).as_str()
    );
    if ledger.last_event(&id)?.is_none() {
        return Ok(vec![]);
    }
    ledger.verify_session(&id)?;
    let events = ledger.session_events(&id)?;
    anyhow::ensure!(
        events.len() <= 4096,
        "invalidation history exceeds reader bound"
    );
    Ok(events
        .iter()
        .map(|e| json!({"observed_at":e.ts(),"record":e.payload()}))
        .collect())
}
fn record_invalidation(repo: &Path, db: &Path, run: &Value) -> Result<()> {
    let ledger = Ledger::open(db)?;
    let repo_id = ledger.repo(&repo.to_string_lossy(), Some(&repo.to_string_lossy()))?;
    let id = format!("verification-invalidations-{}", repo_id.as_str());
    ledger.upsert_session(
        &id,
        &repo_id,
        "verification-invalidation-v1",
        None,
        false,
        None,
    )?;
    exec::append(
        &ledger,
        &repo_id,
        &id,
        json!({"note":"verification_input_changed","version":VERSION,"run_id":run["run_id"],"check_id":run["check_id"],"old_input_tree":run["input_tree"],"changes":run["binding_changes"],"origin":"local input comparison","reason":"Relevant inputs changed. This result remains invalidated even if those inputs are later restored; rerun to collect fresh evidence."}),
    )
}

pub fn verifier_identity() -> Result<String> {
    static RUNNING: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    static CACHE: std::sync::Mutex<Option<(Value, String)>> = std::sync::Mutex::new(None);
    let path = std::env::current_exe()?;
    let stamp = crate::verification_scope::stamp(&path)?;
    let mut cache = CACHE
        .lock()
        .map_err(|_| anyhow::anyhow!("verifier identity cache unavailable"))?;
    let bytes = match cache.as_ref() {
        Some((previous, bytes)) if *previous == stamp => bytes.clone(),
        _ => {
            let bytes = exec::file_digest(&path, 512 * 1024 * 1024)?;
            anyhow::ensure!(
                crate::verification_scope::stamp(&path)? == stamp,
                "verifier changed while its identity was being read"
            );
            *cache = Some((stamp, bytes.clone()));
            bytes
        }
    };
    let original = RUNNING.get_or_init(|| bytes.clone());
    anyhow::ensure!(
        *original == bytes,
        "the verifier binary changed on disk; restart the local workspace before running checks"
    );
    Ok(original.clone())
}
