//! Developer-controlled acceptance, independent of session extraction.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Subcommand;
use serde_json::{json, Value};
use thruline_ledger::{acceptance::Revision, Ledger, RepoId};
use thruline_policy::acceptance::{Document, MAX_DOCUMENT_BYTES};

#[derive(Debug, clap::Args)]
pub struct Args {
    #[command(subcommand)]
    action: Action,
}

#[derive(Debug, clap::Args)]
pub struct Location {
    #[arg(long)]
    repo: Option<PathBuf>,
    #[arg(long)]
    exec_db: Option<PathBuf>,
    #[arg(long)]
    file: Option<PathBuf>,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Subcommand)]
pub enum Action {
    /// Write an unaccepted draft from the repository's Python or npm scripts.
    Init(Location),
    /// Accept the exact reviewed document. Does not execute its commands.
    Accept {
        #[command(flatten)]
        location: Location,
        /// Digest printed by init/status; prevents accepting changed bytes.
        #[arg(long)]
        digest: String,
        /// Previous acceptance hash printed by status, required for replacement.
        #[arg(long)]
        supersedes: Option<String>,
        /// Local attribution only; not an authenticated external identity.
        #[arg(long, default_value = "local-developer")]
        by: String,
    },
    /// Show accepted requirements and any unaccepted draft change.
    Status(Location),
    /// Execute one accepted check once, within its declared timeout.
    Run {
        #[command(flatten)]
        location: Location,
        #[arg(long)]
        check: String,
        /// Reviewed contract digest; execution never follows an unaccepted draft.
        #[arg(long)]
        digest: String,
    },
    /// Export a manual continuity packet; export does not confirm a handoff.
    Handoff(Location),
    /// Compare a reviewed handoff with this workspace and record local acknowledgement.
    Resume {
        #[command(flatten)]
        location: Location,
        #[arg(long)]
        packet: PathBuf,
        #[arg(long)]
        packet_digest: String,
        #[arg(long, default_value = "local-developer")]
        by: String,
    },
    /// Sign the current accepted-requirement result as a detached local receipt.
    Receipt {
        #[command(flatten)]
        location: Location,
        #[arg(long)]
        out: Option<PathBuf>,
        /// Open a historical local-pack receipt, including captured evidence.
        #[arg(long, conflicts_with = "out")]
        run: Option<String>,
    },
}

pub fn run(args: Args) -> Result<ExitCode> {
    match args.action {
        Action::Init(location) => init(&location),
        Action::Accept {
            location,
            digest,
            supersedes,
            by,
        } => accept(&location, &digest, supersedes.as_deref(), &by),
        Action::Status(location) => status(&location, false),
        Action::Run {
            location,
            check,
            digest,
        } => run_check(&location, &check, &digest),
        Action::Handoff(location) => status(&location, true),
        Action::Resume {
            location,
            packet,
            packet_digest,
            by,
        } => {
            let repo = root(&location)?;
            let body = crate::acceptance_handoff::acknowledge(
                &repo,
                &database_path(&location)?,
                &document_path(&location, &repo),
                &packet,
                &packet_digest,
                &by,
            )?;
            if location.json {
                crate::json_out::print(&crate::json_out::ACCEPTANCE_RESUME, body)?;
            } else {
                println!(
                    "Handoff acknowledged locally. {}",
                    body["next_action"]
                        .as_str()
                        .unwrap_or("Inspect acceptance status.")
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        Action::Receipt { location, out, run } => {
            let repo = root(&location)?;
            if let Some(run) = run {
                let receipt = crate::verification_pack::receipt(
                    &repo,
                    &database_path(&location)?,
                    &run,
                    true,
                )?;
                crate::json_out::print(&crate::json_out::ACCEPTANCE_RECEIPT, receipt)?;
                return Ok(ExitCode::SUCCESS);
            }
            let body = crate::acceptance_receipt::write(
                &repo,
                &database_path(&location)?,
                &document_path(&location, &repo),
                out.as_deref(),
            )?;
            if location.json {
                crate::json_out::print(&crate::json_out::ACCEPTANCE_RECEIPT, body)?;
            } else {
                println!(
                    "Local receipt: {}\nVerify with public key: {}",
                    body["path"].as_str().unwrap_or(""),
                    body["public_key"].as_str().unwrap_or("")
                );
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn root(location: &Location) -> Result<PathBuf> {
    let cwd = location.repo.clone().unwrap_or(std::env::current_dir()?);
    let mut command = std::process::Command::new("git");
    thruline_core::git_isolation::isolate_config(&mut command);
    let output = command
        .current_dir(&cwd)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("finding the repository root")?;
    anyhow::ensure!(
        output.status.success(),
        "acceptance needs a Git working tree"
    );
    let path = String::from_utf8(output.stdout)?;
    Ok(PathBuf::from(path.strip_suffix('\n').unwrap_or(&path)).canonicalize()?)
}

fn document_path(location: &Location, repo: &Path) -> PathBuf {
    match &location.file {
        Some(path) if path.is_absolute() => path.clone(),
        Some(path) => repo.join(path),
        None => repo.join(".thruline/acceptance.json"),
    }
}

fn database_path(location: &Location) -> Result<PathBuf> {
    let path = location
        .exec_db
        .clone()
        .map(Ok)
        .unwrap_or_else(|| Ledger::default_path().map_err(anyhow::Error::from))?;
    resolved_path(&path)
}

pub fn resolved_path(path: &Path) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut ancestor = path.as_path();
    let mut missing = Vec::new();
    while !ancestor.try_exists()? {
        missing.push(
            ancestor
                .file_name()
                .context("invalid execution store path")?
                .to_os_string(),
        );
        ancestor = ancestor
            .parent()
            .context("invalid execution store parent")?;
    }
    let mut resolved = ancestor.canonicalize()?;
    for name in missing.into_iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}

fn read_document(path: &Path) -> Result<Document> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .with_context(|| format!("reading {}", path.display()))?
        .take(MAX_DOCUMENT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    Document::parse(&bytes).with_context(|| format!("validating {}", path.display()))
}

fn history(location: &Location, repo: &Path) -> Result<Vec<Revision>> {
    let db = database_path(location)?;
    if !db.try_exists()? {
        return Ok(Vec::new());
    }
    let reader = Ledger::open_read_only(&db).context("opening acceptance history read-only")?;
    Ok(reader.acceptance_history(&RepoId::from_key(&repo.to_string_lossy()))?)
}

fn init(location: &Location) -> Result<ExitCode> {
    let repo = root(location)?;
    let path = document_path(location, &repo);
    let mut checks = Vec::new();
    let mut criteria = Vec::new();
    if repo.join("pyproject.toml").is_file()
        || repo.join("pytest.ini").is_file()
        || repo.join("setup.py").is_file()
    {
        checks.push(json!({"id":"pytest", "category":"test", "argv":["python3","-m","pytest","-q"], "cwd":".", "runner":"pytest", "mode":"captured", "timeout_seconds":120, "artifacts":[]}));
        criteria.push(json!({"id":"python-tests", "description":"The declared Python test suite passes", "check_ids":["pytest"], "human_acceptance_required":false}));
    }
    let package = repo.join("package.json");
    if package.is_file() {
        let mut bytes = Vec::new();
        std::fs::File::open(package)?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        anyhow::ensure!(
            bytes.len() <= 1024 * 1024,
            "package.json exceeds the 1 MiB discovery limit"
        );
        let package: Value = serde_json::from_slice(&bytes)?;
        for (script, category) in [
            ("test", "test"),
            ("build", "build"),
            ("lint", "lint"),
            ("typecheck", "typecheck"),
        ] {
            if let Some(command) = package["scripts"][script].as_str() {
                let parser = thruline_verify::shell::split_commands(command)
                    .ok()
                    .filter(|commands| commands.len() == 1)
                    .and_then(|commands| {
                        let argv = &commands[0].argv;
                        if argv.first().is_some_and(|arg| arg == "node")
                            && argv.iter().any(|arg| arg == "--test")
                        {
                            Some("node --test")
                        } else {
                            thruline_verify::parse::Runner::from_argv(argv)
                                .map(|runner| runner.as_str())
                        }
                    })
                    .unwrap_or("npm");
                checks.push(json!({"id":script, "category":category, "argv":["npm","run",script], "cwd":".", "runner":parser, "mode":"captured", "timeout_seconds":120, "artifacts":[]}));
                criteria.push(json!({"id":format!("npm-{script}"), "description":format!("The declared npm {script} check passes"), "check_ids":[script], "human_acceptance_required":false}));
            }
        }
    }
    anyhow::ensure!(!checks.is_empty(), "no Python or npm check configuration found; create a contract using the acceptance reference");
    let mut protected_paths = vec![".thruline/acceptance.json".to_owned()];
    for name in [
        "tests",
        "test",
        "__tests__",
        "conftest.py",
        "pyproject.toml",
        "pytest.ini",
        "setup.cfg",
        "package.json",
        "jest.config.js",
        "jest.config.ts",
        "vitest.config.js",
        "vitest.config.ts",
    ] {
        if repo.join(name).exists() {
            protected_paths.push(name.to_owned());
        }
    }
    // Common colocated root tests (including Vitest's default layout) are as
    // important as tests/ itself. Discovery remains a draft the person reviews.
    let root_entries: Vec<_> = std::fs::read_dir(&repo)?.take(4097).collect();
    anyhow::ensure!(
        root_entries.len() <= 4096,
        "root discovery exceeds 4096 entries; author and review an explicit acceptance contract"
    );
    for entry in root_entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type()?.is_file()
            && ((name.starts_with("test_") && name.ends_with(".py"))
                || name.ends_with("_test.py")
                || [
                    ".test.ts",
                    ".test.tsx",
                    ".test.js",
                    ".test.jsx",
                    ".test.mjs",
                    ".test.cjs",
                    ".spec.ts",
                    ".spec.tsx",
                    ".spec.js",
                    ".spec.jsx",
                ]
                .iter()
                .any(|suffix| name.ends_with(suffix)))
        {
            protected_paths.push(name);
        }
    }
    protected_paths.sort();
    protected_paths.dedup();
    let draft = json!({"version":1,"id":"project-checks","revision":1,
        "objective":"Verify the declared checks for this change", "criteria":criteria,"checks":checks,
        "protected_paths":protected_paths});
    let bytes = serde_json::to_vec_pretty(&draft)?;
    let document = Document::parse(&bytes)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .with_context(|| {
            format!(
                "creating {} without overwriting an existing contract",
                path.display()
            )
        })?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    let body = json!({"accepted":false,"path":path,"digest":document.digest(),"contract":document.contract(),
        "next_action":"Review the commands, scope and protected test paths, then accept the printed digest. No command has run."});
    emit(location.json, &crate::json_out::ACCEPTANCE_INIT, &body)?;
    Ok(ExitCode::SUCCESS)
}

fn accept(
    location: &Location,
    expected_digest: &str,
    supersedes: Option<&str>,
    by: &str,
) -> Result<ExitCode> {
    let repo = root(location)?;
    let db = database_path(location)?;
    let body = accept_reviewed(
        &repo,
        &db,
        &document_path(location, &repo),
        expected_digest,
        supersedes,
        by,
    )?;
    emit(location.json, &crate::json_out::ACCEPTANCE_ACCEPT, &body)?;
    Ok(ExitCode::SUCCESS)
}

/// Shared CLI/HQ acceptance. Only the exact on-disk draft can be accepted;
/// callers cannot provide replacement commands or execution inputs.
pub fn accept_reviewed(
    repo: &Path,
    db: &Path,
    draft_path: &Path,
    expected_digest: &str,
    supersedes: Option<&str>,
    by: &str,
) -> Result<Value> {
    let document = read_document(draft_path)?;
    let redactor = thruline_capture::redaction::Redactor::from_repo(repo)?;
    for check in &document.contract().checks {
        anyhow::ensure!(thruline_capture::exec::redact_argv(&redactor, &check.argv) == check.argv,
            "draft argv contains a credential; use environment configuration instead of a literal secret");
    }
    anyhow::ensure!(
        document.digest() == expected_digest,
        "draft differs from the reviewed digest; inspect it before accepting"
    );
    let previous = if db.try_exists()? {
        Ledger::open_read_only(db)?
            .acceptance_history(&RepoId::from_key(&repo.to_string_lossy()))?
    } else {
        Vec::new()
    };
    if let Some(previous) = previous.last() {
        let old = Document::parse(previous.document.as_bytes())?;
        anyhow::ensure!(
            document.contract().id == old.contract().id,
            "replacement must retain the contract id"
        );
        anyhow::ensure!(
            document.contract().revision > old.contract().revision,
            "replacement must increase the revision"
        );
        anyhow::ensure!(
            supersedes == Some(previous.hash.as_str()),
            "replacement needs --supersedes with the current acceptance hash"
        );
    } else {
        anyhow::ensure!(
            supersedes.is_none(),
            "no previous acceptance exists to supersede"
        );
    }
    // Snapshotting does not execute the contract's commands. The commit is
    // provenance, not proof that this working tree has been tested.
    let snapshot = thruline_core::spec::snapshot::snapshot_tree(repo)?;
    anyhow::ensure!(
        !db.starts_with(repo),
        "keep the execution store outside the checked repository"
    );
    let ledger = Ledger::open(db)?;
    let repo_id = ledger.repo(&repo.to_string_lossy(), Some(&repo.to_string_lossy()))?;
    let revision = ledger.accept_requirements_at(
        &repo_id,
        std::str::from_utf8(document.bytes())?,
        snapshot.head_commit.as_deref(),
        Some(&snapshot.tree_hash),
        by,
        supersedes,
    )?;
    let body = json!({"accepted":true,"digest":document.digest(),"acceptance_hash":revision.hash,
        "revision":document.contract().revision,"accepted_at":revision.accepted_at,"accepted_by":revision.accepted_by,
        "trust":"local","commands_executed":false,"next_action":"Run the declared checks; acceptance is not completion."});
    Ok(body)
}

pub fn status_document(repo: &Path, db: &Path, draft_path: &Path) -> Result<Value> {
    let revisions = if db.try_exists()? {
        Ledger::open_read_only(db)?
            .acceptance_history(&RepoId::from_key(&repo.to_string_lossy()))?
    } else {
        Vec::new()
    };
    let draft = if draft_path.try_exists()? {
        Some(read_document(draft_path)?)
    } else {
        None
    };
    let (draft_view, draft_redactions, _) = thruline_capture::redaction::Redactor::from_repo(repo)?
        .redact_json(serde_json::to_value(
            draft.as_ref().map(Document::contract),
        )?);
    let Some(revision) = revisions.last() else {
        return Ok(
            json!({"accepted":false,"state":"unverified","reason":"no accepted requirements","location":{"repo":repo,"exec_db":db},
            "draft":draft_view,"draft_review_redacted":draft_redactions > 0,"draft_digest":draft.as_ref().map(Document::digest),"next_action":"Review and accept a contract before claiming its requirements are satisfied."}),
        );
    };
    let document = Document::parse(revision.document.as_bytes())?;
    let pending_change = draft.as_ref().map(Document::digest) != Some(document.digest());
    let snapshot = thruline_core::spec::snapshot::snapshot_tree(repo)?;
    let (mut evidence, runs) = crate::acceptance_exec::evidence(repo, db, revision, &document)?;
    let protected =
        crate::acceptance_exec::protected_changes(repo, revision, &document, &snapshot.tree_hash);
    let protected_verified = protected.as_ref().is_ok_and(|changes| changes.is_empty());
    if !protected_verified {
        for item in &mut evidence {
            item.admissible = false;
        }
    }
    let evaluation = document.evaluate(&snapshot.tree_hash, &evidence)?;
    let next = match evaluation.checks.iter().find(|c| c.state != thruline_policy::acceptance::State::Verified) {
        Some(check) => format!("Inspect {}, then explicitly run that accepted check once if appropriate.", check.check_id),
        None if evaluation.state == thruline_policy::acceptance::State::Verified => "The declared checks pass for the observed local inputs. Review the scope and local evidence limits before sharing.".into(),
        None => "Human acceptance remains open; machine checks cannot accept a subjective criterion.".into(),
    };
    Ok(
        json!({"accepted":true,"location":{"repo":repo,"exec_db":db},"acceptance_hash":revision.hash,"accepted_at":revision.accepted_at,
        "accepted_by":revision.accepted_by,"contract":document.contract(),"contract_digest":document.digest(),
        "draft":draft_view,"draft_review_redacted":draft_redactions > 0,"draft_digest":draft.as_ref().map(Document::digest),"pending_draft_change":pending_change,
        "protected_inputs_verified":protected_verified,"protected_changes":protected.as_ref().ok(),
        "protected_input_error":protected.as_ref().err().map(|error| error.to_string()),
        "trust":"local","evaluation":evaluation,"state":evaluation.state,"runs":runs,
        "next_action":next}),
    )
}

fn run_check(location: &Location, check_id: &str, digest: &str) -> Result<ExitCode> {
    let repo = root(location)?;
    let previous = history(location, &repo)?;
    let revision = previous
        .last()
        .context("review and accept requirements before executing them")?;
    let document = Document::parse(revision.document.as_bytes())?;
    anyhow::ensure!(
        document.digest() == digest,
        "execution digest differs from the accepted requirements"
    );
    let draft = read_document(&document_path(location, &repo))?;
    anyhow::ensure!(
        draft.digest() == document.digest(),
        "draft changed; review it before running accepted commands"
    );
    let check = document
        .contract()
        .checks
        .iter()
        .find(|c| c.id == check_id)
        .context("no accepted check has that id")?;
    let db = database_path(location)?;
    anyhow::ensure!(
        !db.canonicalize()?.starts_with(&repo),
        "keep execution evidence outside the checked repository"
    );
    let run_id = crate::acceptance_exec::run(&repo, &db, revision, &document, check)?;
    let mut body = status_document(&repo, &db, &document_path(location, &repo))?;
    body["run_id"] = json!(run_id);
    body["attempts_executed"] = json!(1);
    emit(location.json, &crate::json_out::ACCEPTANCE_RUN, &body)?;
    Ok(ExitCode::from(match body["state"].as_str() {
        Some("verified") => 0,
        Some("failed") => 1,
        _ => 2,
    }))
}

fn status(location: &Location, handoff: bool) -> Result<ExitCode> {
    let repo = root(location)?;
    let mut body = status_document(
        &repo,
        &database_path(location)?,
        &document_path(location, &repo),
    )?;
    if !handoff && location.file.is_none() {
        body = crate::verification_pack::status(&repo, &database_path(location)?)?;
    }
    let shape = if handoff {
        if body["accepted"] == true {
            let packet = crate::acceptance_handoff::packet(&repo, &body)?;
            body["packet_digest"] = json!(crate::acceptance_handoff::digest(&packet)?);
            body["packet"] = serde_json::to_value(packet)?;
        }
        body["workspace"] = json!(RepoId::from_key(&repo.to_string_lossy()).as_str());
        body["handoff_confirmed"] = json!(false);
        body["handoff_instruction"] = json!("At resumption compare workspace, current subject and accepted contract digest. This export does not acknowledge or resume an agent.");
        &crate::json_out::ACCEPTANCE_HANDOFF
    } else {
        &crate::json_out::ACCEPTANCE_STATUS
    };
    emit(location.json, shape, &body)?;
    // Advisory inspection succeeds as a command. The result's explicit state
    // is unverified; exit zero is never an enforcement decision.
    Ok(ExitCode::SUCCESS)
}

fn emit(json_output: bool, shape: &crate::json_out::Shape, body: &Value) -> Result<()> {
    if json_output {
        return crate::json_out::print(shape, body.clone());
    }
    if let Some(digest) = body["digest"]
        .as_str()
        .or(body["contract_digest"].as_str())
        .or(body["draft_digest"].as_str())
    {
        println!("Contract {digest}");
    }
    if let Some(hash) = body["acceptance_hash"].as_str() {
        println!("Acceptance {hash}");
    }
    println!(
        "{}",
        match body["state"].as_str() {
            Some("verified") => "The declared checks pass for the observed local inputs.",
            Some("failed") => "A declared check failed for the observed local inputs.",
            _ if body["accepted"] == true =>
                "Requirements accepted locally; completion has not been established.",
            _ => "Requirements have not been accepted.",
        }
    );
    if let Some(contract) = body.get("contract") {
        let contract: thruline_policy::acceptance::Contract =
            serde_json::from_value(contract.clone())?;
        for criterion in &contract.criteria {
            let state = body["evaluation"]["criteria"]
                .as_array()
                .and_then(|items| {
                    items
                        .iter()
                        .find(|item| item["criterion_id"] == criterion.id)
                })
                .and_then(|item| item["state"].as_str())
                .unwrap_or("unverified");
            let label = match state {
                "verified" => "Proven",
                "failed" => "Not proven",
                _ => "Recorded",
            };
            println!("  [{label}] {}: {}", criterion.id, criterion.description);
        }
        for check in &contract.checks {
            println!(
                "  {}: {:?} (cwd {}, timeout {}s)",
                check.id, check.argv, check.cwd, check.timeout_seconds
            );
        }
    }
    if let Some(runs) = body["runs"].as_array() {
        for run in runs {
            if let Some(counts) = run["counts"].as_object() {
                println!(
                    "  {}: {} passed, {} failed, {} skipped ({} tests observed; local evidence)",
                    run["check_id"].as_str().unwrap_or("check"),
                    counts["passed"],
                    counts["failed"],
                    counts["skipped"],
                    counts["total"]
                );
            }
            if let Some(reasons) = run["reasons"].as_array() {
                for reason in reasons {
                    if let Some(reason) = reason.as_str() {
                        println!("    {reason}");
                    }
                }
            }
        }
    }
    if body["pending_draft_change"] == true {
        println!("The draft differs from the accepted requirements; no history was replaced.");
    }
    if let Some(next) = body["next_action"].as_str() {
        println!("Next: {next}");
    }
    if let Some(instruction) = body["handoff_instruction"].as_str() {
        println!("{instruction}");
    }
    Ok(())
}
