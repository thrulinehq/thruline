//! Explicit local runs attached to an acceptance before the command starts.
//! Events, output capture and runner parsing reuse the existing engine.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use thruline_capture::{bounded, redaction::Redactor};
use thruline_core::spec::event::{Event, EventId, Payload, Source};
use thruline_ledger::{acceptance::Revision, EventRow, Ledger, RepoId};
use thruline_policy::acceptance::{Category, Check, Document, Evidence, ExecutionMode, Outcome};
use thruline_verify::parse::{self, Runner, RunnerOutcome};

const HARNESS: &str = "acceptance-local-v1";

pub fn protected_changes(
    repo: &Path,
    revision: &Revision,
    document: &Document,
    tree: &str,
) -> Result<Vec<String>> {
    let baseline = revision
        .source_tree
        .as_deref()
        .context("accepted revision has no protected-input baseline; review and supersede it")?;
    let mut command = Command::new("git");
    thruline_core::git_isolation::isolate_config(&mut command);
    let out = command
        .current_dir(repo)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .args([
            "diff-tree",
            "-r",
            "--name-only",
            "-z",
            "--no-commit-id",
            "--no-textconv",
            baseline,
            tree,
            "--",
        ])
        .output()?;
    anyhow::ensure!(
        out.status.success(),
        "accepted tree history is unavailable; protected inputs cannot be compared"
    );
    Ok(String::from_utf8(out.stdout)?
        .split('\0')
        .filter(|p| !p.is_empty())
        .filter(|path| {
            document
                .contract()
                .protected_paths
                .iter()
                .any(|protected| *path == protected || path.starts_with(&format!("{protected}/")))
        })
        .map(str::to_string)
        .collect())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inputs {
    pub tree: String,
    pub commit: Option<String>,
    pub git_environment: String,
    // Local-only, salted with an unexported random secret. Never export this
    // field or use an unsalted digest as a privacy transform for credentials.
    pub process_environment_private: String,
    pub executable: String,
    pub executable_digest: String,
    pub cwd: String,
    pub limitations: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub environment_files: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verifier_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_manifest: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_dependencies: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Start {
    version: u32,
    note: String,
    acceptance_hash: String,
    contract_digest: String,
    check_id: String,
    check_digest: String,
    inputs: Inputs,
    artifacts_before: BTreeMap<String, Option<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    pub total: u64,
    pub passed: u64,
    pub failed: u64,
    pub skipped: u64,
    pub xfail: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Output {
    pub digest: String,
    pub bytes: u64,
    pub redactions: u64,
    pub complete: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finish {
    pub version: u32,
    pub note: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub outcome: Outcome,
    pub counts: Option<Counts>,
    pub outputs: Vec<Output>,
    pub artifacts: BTreeMap<String, String>,
    pub inputs_after: Option<Inputs>,
    pub reasons: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<crate::verification_pack::Capture>,
}

pub fn storage(db: &Path) -> Result<PathBuf> {
    let parent = db
        .parent()
        .context("execution store needs a parent directory")?;
    // File-specific namespace avoids cross-store environment tokens and blobs.
    let name = db
        .file_name()
        .context("execution store needs a filename")?
        .to_string_lossy();
    Ok(parent.join(format!("{name}.acceptance")))
}

fn environment_token(db: &Path, create: bool) -> Result<String> {
    let directory = storage(db)?;
    let key_path = directory.join("environment.key");
    if create && !key_path.try_exists()? {
        std::fs::create_dir_all(&directory)?;
        let mut key = [0u8; 32];
        std::fs::File::open("/dev/urandom")?.read_exact(&mut key)?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&key_path) {
            Ok(mut file) => {
                file.write_all(&key)?;
                file.sync_all()?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    let mut key = Vec::new();
    std::fs::File::open(&key_path)?
        .take(33)
        .read_to_end(&mut key)?;
    anyhow::ensure!(key.len() == 32, "environment binding key is invalid");
    let mut h = Sha256::new();
    h.update(b"thruline.local.environment.v1\0");
    h.update(&key);
    let env: BTreeMap<_, _> = std::env::vars_os()
        .filter(|(k, _)| k != "_" && k != "SHLVL")
        .collect();
    for (key, value) in env {
        for value in [key, value] {
            let bytes = value.as_encoded_bytes();
            h.update((bytes.len() as u64).to_le_bytes());
            h.update(bytes);
        }
    }
    h.update(&key);
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

pub fn file_digest(path: &Path, limit: u64) -> Result<String> {
    let metadata = std::fs::metadata(path)?;
    anyhow::ensure!(
        metadata.is_file() && metadata.len() <= limit,
        "file is not regular or exceeds the digest limit"
    );
    let mut file = std::fs::File::open(path)?.take(limit + 1);
    let mut h = Sha256::new();
    let mut bytes = [0; 64 * 1024];
    let mut total = 0;
    loop {
        let n = file.read(&mut bytes)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        anyhow::ensure!(total <= limit, "file grew beyond the digest limit");
        h.update(&bytes[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

pub fn inputs(repo: &Path, db: &Path, check: &Check, create: bool) -> Result<Inputs> {
    let cwd = repo.join(&check.cwd).canonicalize()?;
    anyhow::ensure!(
        cwd.starts_with(repo),
        "check working directory escapes the repository"
    );
    let program = if matches!(
        check.verification.as_ref().and_then(|v| v.pack.as_ref()),
        Some(thruline_policy::acceptance::PackPlan::Completion)
    ) {
        std::env::current_exe().ok()
    } else if check.argv[0].contains('/') {
        cwd.join(&check.argv[0]).canonicalize().ok()
    } else {
        thruline_capture::exec::resolve_program(&check.argv[0], None)
            .and_then(|p| p.canonicalize().ok())
    };
    let snapshot = thruline_core::spec::snapshot::snapshot_tree(repo)?;
    let git_env = thruline_core::spec::git_env::git_env(repo)?;
    let mut limitations = Vec::new();
    if snapshot.content_filters.degrades_capture()
        || !snapshot.omitted.is_empty()
        || git_env.sparse
        || !git_env.submodules.is_empty()
        || !git_env.unreadable.is_empty()
    {
        limitations.push("working tree or Git environment cannot be fully represented".into());
    }
    let mut environment_files = BTreeMap::new();
    if let Some(v) = &check.verification {
        for scope in &v.scope {
            match repo.join(scope).canonicalize() {
                Ok(path) if path.starts_with(repo) => {
                    // Git intentionally respects ignore rules. An explicitly
                    // scoped file is still an input even when Git excludes it.
                    // Bind it independently in the supplemental file manifest.
                    if path.is_file() {
                        match file_digest(&path, 16 * 1024 * 1024) {
                            Ok(digest) => {
                                environment_files
                                    .insert(path.to_string_lossy().into_owned(), digest);
                            }
                            Err(_) => limitations.push(format!(
                                "required scope file cannot be fingerprinted: {scope}"
                            )),
                        }
                    } else if !path.is_dir() {
                        limitations.push(format!(
                            "required scope is not a regular file or directory: {scope}"
                        ));
                    }
                }
                _ => limitations.push(format!(
                    "required scope is missing or outside the project: {scope}"
                )),
            }
        }
        if !v.environment_closed {
            limitations.push("the accepted dependency/environment boundary is incomplete".into());
        }
        for name in &v.environment_files {
            let path = repo.join(name);
            match file_digest(&path, 16 * 1024 * 1024) {
                Ok(digest) => {
                    environment_files.insert(name.clone(), digest);
                }
                Err(_) => limitations.push(format!(
                    "required environment file cannot be fingerprinted: {name}"
                )),
            }
        }
    }
    let executable_digest = match program.as_ref().map(|p| {
        if std::env::current_exe().as_ref().ok() == Some(p) {
            crate::verification_pack::verifier_identity()
        } else {
            file_digest(p, 256 * 1024 * 1024)
        }
    }) {
        Some(Ok(digest)) => digest,
        _ => {
            limitations.push("declared executable is missing or cannot be fingerprinted".into());
            String::new()
        }
    };
    let scope_manifest = if check.verification.is_some() {
        match crate::verification_scope::manifest(repo, &snapshot.tree_hash, check) {
            Ok(m) => Some(m),
            Err(error) => {
                limitations.push(format!("input scope could not be established: {error}"));
                None
            }
        }
    } else {
        None
    };
    Ok(Inputs {
        scope_manifest,
        evidence_dependencies: crate::verification_scope::completion_dependencies(repo, db, check)?,
        environment_files,
        verifier_digest: if check.verification.is_some() {
            Some(crate::verification_pack::verifier_identity()?)
        } else {
            None
        },
        tree: snapshot.tree_hash,
        commit: snapshot.head_commit,
        git_environment: git_env.digest,
        process_environment_private: environment_token(db, create)?,
        executable: program
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        executable_digest,
        cwd: cwd.to_string_lossy().into_owned(),
        limitations,
    })
}

fn artifact(repo: &Path, name: &str) -> Result<Option<String>> {
    let path = repo.join(name);
    if !path.try_exists()? {
        return Ok(None);
    }
    let resolved = path.canonicalize()?;
    anyhow::ensure!(
        resolved.starts_with(repo) && !std::fs::symlink_metadata(&path)?.file_type().is_symlink(),
        "artifact must be a regular file inside the repository"
    );
    Ok(Some(file_digest(&resolved, 64 * 1024 * 1024)?))
}

pub fn append(ledger: &Ledger, repo: &RepoId, session: &str, value: Value) -> Result<()> {
    let last = ledger.last_event(session)?;
    let seq = last.as_ref().map_or(1, |(seq, _)| seq + 1);
    let previous = last.map_or_else(
        || thruline_core::spec::chain::genesis_prev_hash(session),
        |(_, hash)| hash,
    );
    let body = serde_json::from_value(value)?;
    let event = Event::new(
        session,
        seq,
        thruline_core::ingest::iso8601_utc_now(),
        Source::Thruline,
        Payload::Note(body),
        previous,
    )?
    .seal()?;
    anyhow::ensure!(
        ledger.append_events(repo, &[EventRow::bare(event)])? == 1,
        "run event was not appended"
    );
    Ok(())
}

/// The caller has already required the exact reviewed digest. No retries.
pub fn run(
    repo: &Path,
    db: &Path,
    revision: &Revision,
    document: &Document,
    check: &Check,
) -> Result<String> {
    run_controlled(repo, db, revision, document, check, &AtomicBool::new(false))
}

pub fn run_controlled(
    repo: &Path,
    db: &Path,
    revision: &Revision,
    document: &Document,
    check: &Check,
    cancel: &AtomicBool,
) -> Result<String> {
    anyhow::ensure!(check.mode == ExecutionMode::Captured, "isolated accepted checks are not implemented in this build; leave this requirement open or explicitly review a revised captured check");
    let redactor = Redactor::from_repo(repo)?;
    anyhow::ensure!(thruline_capture::exec::redact_argv(&redactor, &check.argv) == check.argv,
        "accepted argv contains a credential; use environment configuration and accept a revised command");
    std::fs::create_dir_all(storage(db)?)?;
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(storage(db)?.join("verification.run.lock"))?;
    lock.try_lock()
        .context("another check is running for this project and evidence store")?;
    let before = match inputs(repo, db, check, true) {
        Ok(inputs) => inputs,
        Err(_) => Inputs {
            tree: String::new(),
            commit: None,
            git_environment: String::new(),
            process_environment_private: environment_token(db, true)?,
            executable: String::new(),
            executable_digest: String::new(),
            cwd: repo.join(&check.cwd).to_string_lossy().into_owned(),
            limitations: vec![
                "required project, working directory, or input fingerprint could not be read"
                    .into(),
            ],
            environment_files: BTreeMap::new(),
            verifier_digest: None,
            scope_manifest: None,
            evidence_dependencies: None,
        },
    };
    let mut artifacts_before = BTreeMap::new();
    for item in &check.artifacts {
        artifacts_before.insert(item.path.clone(), artifact(repo, &item.path)?);
    }
    let ledger = Ledger::open(db)?;
    let repo_id = ledger.repo(&repo.to_string_lossy(), Some(&repo.to_string_lossy()))?;
    anyhow::ensure!(
        ledger.acceptance_history(&repo_id)?.last().map(|r| &r.hash) == Some(&revision.hash),
        "acceptance changed before execution; review the current contract"
    );
    let session = format!("acceptance-{}", EventId::generate());
    ledger.upsert_session(&session, &repo_id, HARNESS, None, false, None)?;
    let start = Start {
        version: 1,
        note: "acceptance_run_begin".into(),
        acceptance_hash: revision.hash.clone(),
        contract_digest: document.digest().into(),
        check_id: check.id.clone(),
        check_digest: check.digest()?,
        inputs: before.clone(),
        artifacts_before,
    };
    append(&ledger, &repo_id, &session, serde_json::to_value(&start)?)?;
    let store = storage(db)?.join("outputs");
    let mut command = Command::new(&before.executable);
    command.args(&check.argv[1..]).current_dir(&before.cwd);
    if check.verification.is_some() {
        let mut finish = crate::verification_pack::execute(
            repo, db, revision, document, check, &before, cancel,
        )?;
        let seal = crate::verification_pack::local_seal(
            db,
            &session,
            &serde_json::to_value(&start)?,
            &finish,
        )?;
        if let Some(pack) = finish.pack.as_mut() {
            pack.authority = Some(seal);
        }
        append(&ledger, &repo_id, &session, serde_json::to_value(finish)?)?;
        return Ok(session);
    }
    let report = bounded::run_controlled(
        command,
        Duration::from_secs(check.timeout_seconds.into()),
        redactor,
        &store,
        cancel,
        None,
    );
    let mut finish = Finish {
        version: 1,
        note: "acceptance_run_finish".into(),
        exit_code: None,
        timed_out: false,
        outcome: Outcome::NotRun,
        counts: None,
        outputs: Vec::new(),
        artifacts: BTreeMap::new(),
        inputs_after: None,
        reasons: before.limitations.clone(),
        pack: None,
    };
    match protected_changes(repo, revision, document, &before.tree) {
        Ok(changes) if changes.is_empty() => {}
        Ok(_) => finish
            .reasons
            .push("protected requirements or tests differ from their accepted baseline".into()),
        Err(_) => finish
            .reasons
            .push("the protected-input baseline could not be verified".into()),
    }
    match report {
        Err(_) => finish
            .reasons
            .push("the child process could not be launched or observed".into()),
        Ok(report) => {
            finish.exit_code = report.status.code();
            finish.timed_out = report.timed_out;
            let mut text = String::new();
            for stream in [report.stdout, report.stderr] {
                if let Some(stream) = stream {
                    let captured = stream.captured;
                    let complete = stream.complete
                        && !captured.invalid_utf8
                        && !captured.block_cap_hit
                        && captured.store_error.is_none();
                    if !complete || captured.redactions_applied > 0 {
                        finish.reasons.push("output is incomplete or redacted; deciding evidence may be unavailable".into());
                    }
                    if let Some(path) = captured.stored_path {
                        let mut bytes = Vec::new();
                        std::fs::File::open(path)?
                            .take(bounded::OUTPUT_LIMIT + 2)
                            .read_to_end(&mut bytes)?;
                        anyhow::ensure!(
                            bytes.len() <= bounded::OUTPUT_LIMIT as usize + 1,
                            "captured output exceeded its bound"
                        );
                        let digest = format!("sha256:{}", thruline_core::chain::sha256_hex(&bytes));
                        if digest == captured.hash {
                            text.push_str(&String::from_utf8(bytes)?);
                            text.push('\n');
                        } else {
                            finish.reasons.push(
                                "stored output differs from the captured stream digest".into(),
                            );
                        }
                    }
                    finish.outputs.push(Output {
                        digest: captured.hash,
                        bytes: captured.bytes_redacted,
                        redactions: captured.redactions_applied,
                        complete,
                    });
                } else {
                    finish
                        .reasons
                        .push("output pipe did not close within the capture deadline".into());
                }
            }
            finish.outcome = if report.cancelled {
                Outcome::Interrupted
            } else if report.timed_out {
                Outcome::TimedOut
            } else if report.status.code().is_none() {
                Outcome::Interrupted
            } else if !report.status.success() {
                Outcome::Failed
            } else if check.category != Category::Test {
                Outcome::Passed
            } else {
                Outcome::Unparsed
            };
            if check.category == Category::Test {
                match parse_counts(check, &text) {
                    Ok(parsed) => {
                        // Even a zero exit with only skips is not evidence of
                        // an exercised suite. Counts remain visible as observed.
                        if report.status.success() && !report.timed_out {
                            finish.outcome = if parsed.failed > 0 {
                                Outcome::Failed
                            } else if parsed.total > 0 && parsed.passed > 0 {
                                Outcome::Passed
                            } else {
                                Outcome::Unparsed
                            };
                        }
                        finish.counts = Some(parsed);
                    }
                    Err(reason) => {
                        if report.status.success() {
                            finish.reasons.push(reason);
                        }
                    }
                }
            }
        }
    }
    for item in &check.artifacts {
        match artifact(repo, &item.path) {
            Ok(Some(hash)) => {
                if start.artifacts_before[&item.path].is_some() {
                    finish.reasons.push(format!(
                        "artifact {} already existed before this run; its producer is unproved",
                        item.path
                    ));
                }
                if item
                    .sha256
                    .as_ref()
                    .is_some_and(|expected| *expected != hash)
                {
                    finish.reasons.push(format!(
                        "artifact {} differs from the accepted digest",
                        item.path
                    ));
                }
                finish.artifacts.insert(item.path.clone(), hash);
            }
            _ => finish.reasons.push(format!(
                "required artifact {} is absent or unreadable",
                item.path
            )),
        }
    }
    match inputs(repo, db, check, false) {
        Ok(after) => {
            if after != before {
                finish
                    .reasons
                    .push("inputs changed during execution".into());
            }
            finish.inputs_after = Some(after);
        }
        Err(_) => finish
            .reasons
            .push("inputs could not be measured after execution".into()),
    }
    append(&ledger, &repo_id, &session, serde_json::to_value(finish)?)?;
    Ok(session)
}

pub fn parse_counts(check: &Check, output: &str) -> std::result::Result<Counts, String> {
    let program = check.argv[0].rsplit('/').next().unwrap_or("");
    if check.runner == "node --test"
        && (matches!(program, "npm" | "pnpm" | "yarn")
            || (program == "node" && check.argv.iter().any(|arg| arg == "--test")))
    {
        let p = parse::parse_node_test(output).map_err(str::to_string)?;
        return Ok(Counts {
            total: p.tests_total,
            passed: p.passed,
            failed: p.failed,
            skipped: p.skipped,
            xfail: p.todo,
        });
    }
    match parse::parse(declared_runner(check), output) {
        RunnerOutcome::Parsed(p) => Ok(Counts {
            total: p.tests_total,
            passed: p.passed,
            failed: p.failed,
            skipped: p.skipped,
            xfail: p.xfail,
        }),
        RunnerOutcome::Unverifiable { reason, .. } => Err(reason),
    }
}

fn declared_runner(check: &Check) -> Option<Runner> {
    let direct = Runner::from_argv(&check.argv);
    if let Some(runner) = direct {
        return (runner.as_str() == check.runner).then_some(runner);
    }
    // A reviewed npm script is the boundary. Its script definition is part of
    // the input tree; the developer names its actual parser rather than treating
    // npm's own exit as a count of executed tests.
    if !matches!(
        check.argv[0].rsplit('/').next(),
        Some("npm" | "pnpm" | "yarn")
    ) {
        return None;
    }
    match check.runner.as_str() {
        "jest" | "vitest" => Some(Runner::Jest),
        "mocha" => Some(Runner::Mocha),
        _ => None,
    }
}

/// Verify history before selecting the latest explicit attempt per check. An
/// unfinished newer attempt must not fall back to an older green one.
pub fn evidence(
    repo: &Path,
    db: &Path,
    revision: &Revision,
    document: &Document,
) -> Result<(Vec<Evidence>, Vec<Value>)> {
    let ledger = Ledger::open_read_only(db)?;
    let repo_id = RepoId::from_key(&repo.to_string_lossy());
    let mut query = ledger.conn().prepare("SELECT s.id FROM sessions s JOIN events e ON e.session_id=s.id AND e.seq=1 WHERE s.repo_id=?1 AND s.harness=?2 ORDER BY e.rowid DESC LIMIT 4097")?;
    let sessions = query
        .query_map([repo_id.as_str(), HARNESS], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    anyhow::ensure!(
        sessions.len() <= 4096,
        "acceptance history exceeds this reader's bound; no evidence was promoted"
    );
    let mut selected = BTreeMap::new();
    for session in sessions {
        ledger.verify_session(&session)?;
        let events = ledger.session_events(&session)?;
        anyhow::ensure!(
            !events.is_empty() && events.len() <= 2,
            "unexpected acceptance run event count"
        );
        let start: Start = serde_json::from_value(events[0].payload().clone())?;
        anyhow::ensure!(
            start.version == 1 && start.note == "acceptance_run_begin",
            "unsupported acceptance run version"
        );
        let finish: Option<Finish> = events
            .get(1)
            .map(|e| serde_json::from_value(e.payload().clone()))
            .transpose()?;
        if let Some(finish) = &finish {
            anyhow::ensure!(
                finish.version == 1 && finish.note == "acceptance_run_finish",
                "unsupported run finish"
            );
        }
        if start.acceptance_hash == revision.hash && start.contract_digest == document.digest() {
            selected
                .entry(start.check_id.clone())
                .or_insert((session, start, finish));
        }
    }
    let mut evidence = Vec::new();
    let mut details = Vec::new();
    let invalidations = crate::verification_pack::invalidations(repo, db)?;
    for check in &document.contract().checks {
        let Some((session, start, finish)) = selected.remove(&check.id) else {
            continue;
        };
        let mut reasons = finish.as_ref().map_or_else(
            || vec!["run began but no finish was recorded".into()],
            |f| f.reasons.clone(),
        );
        let current = inputs(repo, db, check, false);
        let already_invalidated = invalidations
            .iter()
            .any(|i| i["record"]["run_id"] == session);
        let current_matches = !already_invalidated
            && current
                .as_ref()
                .is_ok_and(|current| *current == start.inputs);
        let mut binding_changes = Vec::<Value>::new();
        if let Ok(now) = &current {
            let old = serde_json::to_value(&start.inputs)?;
            let new = serde_json::to_value(now)?;
            for key in [
                "tree",
                "git_environment",
                "executable_digest",
                "verifier_digest",
                "environment_files",
                "scope_manifest",
                "evidence_dependencies",
                "cwd",
            ] {
                if old[key] != new[key] {
                    binding_changes
                        .push(json!({"input":key,"previous":old[key],"current":new[key]}));
                }
            }
            if now.process_environment_private != start.inputs.process_environment_private {
                binding_changes.push(json!({"input":"process environment","previous":"privately fingerprinted","current":"changed; credential values are not exported"}));
            }
        }
        if already_invalidated {
            reasons.push(
                "this run was invalidated by an observed input change; a fresh run is required"
                    .into(),
            );
        }
        if !current_matches {
            reasons.push(
                "current inputs or executable/environment differ, or could not be measured".into(),
            );
        }
        let mut admissible =
            current_matches && reasons.is_empty() && start.check_digest == check.digest()?;
        if let Some(finish) = &finish {
            if finish.pack.is_some()
                && !crate::verification_pack::verify_local_seal(
                    db,
                    &session,
                    &serde_json::to_value(&start)?,
                    finish,
                )
            {
                admissible = false;
                reasons.push("local collector authority could not be verified; imported reports are not observations".into());
            }
            admissible &= finish.inputs_after.as_ref() == Some(&start.inputs)
                && finish.outputs.len()
                    == finish.pack.as_ref().map_or(2, |p| p.observations.len() * 2);
            for output in &finish.outputs {
                let expected = output.digest.strip_prefix("sha256:").unwrap_or("");
                if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
                    anyhow::bail!("invalid output digest in run evidence");
                }
                let path = storage(db)?.join("outputs").join(format!("{expected}.out"));
                if file_digest(&path, bounded::OUTPUT_LIMIT + 1)
                    .ok()
                    .as_deref()
                    != Some(expected)
                    || !output.complete
                {
                    admissible = false;
                    reasons.push("recorded output is missing, changed or incomplete".into());
                }
            }
            for artifact_spec in &check.artifacts {
                let expected = finish.artifacts.get(&artifact_spec.path);
                let observed = if finish.pack.as_ref().is_some_and(|p| {
                    matches!(
                        p.version.as_str(),
                        "local-verification-pack/2" | "local-verification-pack/3"
                    )
                }) {
                    expected.and_then(|digest| {
                        file_digest(
                            &storage(db).ok()?.join("artifacts").join(digest),
                            64 * 1024 * 1024,
                        )
                        .ok()
                    })
                } else {
                    artifact(repo, &artifact_spec.path).ok().flatten()
                };
                if expected.is_none() || observed.as_ref() != expected {
                    admissible = false;
                    reasons.push(format!(
                        "artifact {} is missing or changed",
                        artifact_spec.path
                    ));
                }
            }
        } else {
            admissible = false;
        }
        evidence.push(Evidence {
            check_id: check.id.clone(),
            check_digest: start.check_digest,
            contract_digest: start.contract_digest,
            subject: start.inputs.tree.clone(),
            run_id: session.clone(),
            outcome: finish.as_ref().map_or(Outcome::Interrupted, |f| f.outcome),
            admissible,
        });
        details.push(json!({"check_id":check.id,"run_id":session,"input_tree":start.inputs.tree,
            "check_digest":check.digest()?,"chain_head":ledger.verify_session(&session)?.head_hash,
            "executable_digest":start.inputs.executable_digest,"outcome":finish.as_ref().map_or(Outcome::Interrupted, |f| f.outcome),
            "admissible":admissible,
            "input_commit":start.inputs.commit,"current":current_matches,"current_inputs_identified":current.as_ref().is_ok_and(|i|i.limitations.is_empty()),"trust":"local",
            "environment_boundary":"observed process environment, executable and Git tree; external services and installed dependencies are not a closed environment",
            "runner_version":"not separately measured; resolved executable bytes are bound",
            "counts":finish.as_ref().and_then(|f| f.counts.as_ref()),"exit_code":finish.as_ref().and_then(|f| f.exit_code),
            "outputs":finish.as_ref().map(|f| &f.outputs),"artifacts":finish.as_ref().map(|f| &f.artifacts),
            "binding_changes":binding_changes,"reasons":reasons}));
    }
    Ok((evidence, details))
}
