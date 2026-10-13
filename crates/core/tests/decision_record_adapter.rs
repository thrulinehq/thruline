//! Existing captured runs are usable only when their input record binds them
//! to the exact clean commit being checked. Recorded edits have no such link.

#[path = "support/person_action.rs"]
mod person_action;

use person_action::PersonRecorder;

use std::error::Error;
use std::path::Path;
use std::process::Command;

use thruline_core::decisions::checks::VerdictState;
use thruline_core::decisions::engine;
use thruline_core::decisions::params::{
    CheckParams, CommandPassesParams, EditsWithinPathsParams, PathsUntouchedParams,
};
use thruline_core::decisions::record_adapter::check_recorded_at;
use thruline_core::decisions::storage::{
    self, AcceptanceInput, DecisionSource, NewDecision, SourceKind,
};
use thruline_core::evidence_binding::{
    self, EvidenceBinding, ExecMode, IdentityEdge, StreamCapture, WsidScopeDecl,
};
use thruline_core::examined_inputs::{self, ExaminedInputs, Input, SubjectKind, Uncommitted};
use thruline_core::ledger::Ledger;
use thruline_core::run_execution::{self, ExecutionState, Finish, Start};
use thruline_core::test_tally::{self, TestTally};

fn git(repo: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = Command::new("git").args(args).current_dir(repo).output()?;
    if !output.status.success() {
        return Err(format!("git {args:?}: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn repo() -> Result<(tempfile::TempDir, String, String), Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    git(repo.path(), &["init", "-q"])?;
    std::fs::write(repo.path().join("app.rs"), "fn app() {}\n")?;
    git(repo.path(), &["add", "-A"])?;
    git(
        repo.path(),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "first",
        ],
    )?;
    let commit = git(repo.path(), &["rev-parse", "HEAD"])?;
    let tree = git(repo.path(), &["rev-parse", "HEAD^{tree}"])?;
    Ok((repo, commit, tree))
}

fn params(command: &str, is_test: bool) -> CommandPassesParams {
    CommandPassesParams {
        command: command.into(),
        is_test,
        min_passed: u64::from(is_test),
    }
}

fn captured_run(
    ledger: &Ledger,
    repo: &Path,
    commit: &str,
    command: &str,
    exit_code: i64,
    clean: bool,
    run_key: &str,
) -> Result<String, Box<dyn Error>> {
    let repo_id = thruline_core::workspace::repo_identity_at(repo)?
        .repo_id
        .expect("fixture repository identity");
    let root = repo.canonicalize()?.to_string_lossy().into_owned();
    let wsid = format!("wsid1:{run_key}");
    let argv = command.split_whitespace().map(str::to_owned).collect();
    let stream = StreamCapture::Captured {
        sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
        bytes: 0,
    };
    let binding = EvidenceBinding {
        argv,
        mode: ExecMode::Direct,
        cwd: root.clone(),
        root: root.clone(),
        repository_identity: repo_id,
        scope: evidence_binding::classify_command(command, &root),
        wsid_scope: WsidScopeDecl::everything(),
        pre: IdentityEdge::complete(wsid.clone()),
        post: IdentityEdge::complete(wsid.clone()),
        exit_code: Some(exit_code),
        term_signal: None,
        stdout: stream.clone(),
        stderr: stream,
        artifacts: vec![],
        wrapper_defects: vec![],
        started_at: "2026-10-07T00:00:00Z".into(),
        bound_at: "2026-10-07T00:01:00Z".into(),
        source_adapter: "fixture".into(),
        source_session: run_key.into(),
    };
    let id = evidence_binding::store(ledger, &binding)?
        .stored()
        .id
        .clone();
    let mut inputs = ExaminedInputs::unknown_for(SubjectKind::CheckRun, &id);
    inputs.source_identity = Input::known(wsid);
    inputs.base_revision = Input::known(commit);
    inputs.uncommitted = if clean {
        Uncommitted::Clean
    } else {
        Uncommitted::Present {
            files: 1,
            diff_sha256: Input::known("dirty"),
        }
    };
    examined_inputs::record(ledger, &inputs, "2026-10-07T00:01:00Z")?;
    run_execution::start(
        ledger,
        &Start {
            run_key: run_key.into(),
            change_id: None,
            name: command.into(),
            pid: None,
            process_group: None,
            started_at: "2026-10-07T00:00:00Z".into(),
            examined_inputs_id: None,
        },
    )?;
    run_execution::finish(
        ledger,
        &Finish {
            run_key: run_key.into(),
            state: ExecutionState::Completed,
            reason: None,
            actor: None,
            binding_id: Some(id.clone()),
            cleanup_verified: None,
            survivors: None,
            at: "2026-10-07T00:01:00Z".into(),
        },
    )?;
    Ok(id)
}

#[test]
fn e1_uses_only_completed_runs_on_the_exact_clean_commit() -> Result<(), Box<dyn Error>> {
    let (repo, commit, tree) = repo()?;
    let ledger = Ledger::open_in_memory()?;
    let accepted = CheckParams::CommandPasses(params("cargo build", false));
    assert_eq!(
        check_recorded_at(&ledger, repo.path(), &commit, &tree, &accepted)?
            .expect("E1 dispatch")
            .state,
        VerdictState::Unverifiable
    );
    captured_run(
        &ledger,
        repo.path(),
        &commit,
        "cargo build",
        0,
        false,
        "dirty",
    )?;
    assert_eq!(
        check_recorded_at(&ledger, repo.path(), &commit, &tree, &accepted)?
            .expect("E1 dispatch")
            .state,
        VerdictState::Unverifiable,
        "a dirty run cannot establish the named commit"
    );
    captured_run(
        &ledger,
        repo.path(),
        &commit,
        "cargo build",
        0,
        true,
        "green",
    )?;
    let holds =
        check_recorded_at(&ledger, repo.path(), &commit, &tree, &accepted)?.expect("E1 dispatch");
    assert_eq!(holds.state, VerdictState::Holds);
    assert_eq!(holds.checked_places, 1);
    captured_run(&ledger, repo.path(), &commit, "cargo build", 1, true, "red")?;
    assert_eq!(
        check_recorded_at(&ledger, repo.path(), &commit, &tree, &accepted)?
            .expect("E1 dispatch")
            .state,
        VerdictState::Broken,
        "latest exact-version captured run governs"
    );
    std::fs::write(repo.path().join("app.rs"), "fn app() { changed() }\n")?;
    git(repo.path(), &["add", "-A"])?;
    git(
        repo.path(),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "second",
        ],
    )?;
    let newer = git(repo.path(), &["rev-parse", "HEAD"])?;
    let newer_tree = git(repo.path(), &["rev-parse", "HEAD^{tree}"])?;
    assert_eq!(
        check_recorded_at(&ledger, repo.path(), &newer, &newer_tree, &accepted)?
            .expect("E1 dispatch")
            .state,
        VerdictState::Unverifiable,
        "an older run cannot pass or fail a newer version"
    );
    Ok(())
}

#[test]
fn e1_test_run_without_recorded_errors_cannot_hold() -> Result<(), Box<dyn Error>> {
    let (repo, commit, tree) = repo()?;
    let ledger = Ledger::open_in_memory()?;
    let id = captured_run(
        &ledger,
        repo.path(),
        &commit,
        "cargo test",
        0,
        true,
        "tests",
    )?;
    test_tally::record(
        &ledger,
        &id,
        &TestTally {
            passed: 3,
            failed: 0,
            ignored: 0,
            suites: 1,
        },
        "2026-10-07T00:01:00Z",
    )?;
    let outcome = check_recorded_at(
        &ledger,
        repo.path(),
        &commit,
        &tree,
        &CheckParams::CommandPasses(params("cargo test", true)),
    )?
    .expect("E1 dispatch");
    assert_eq!(outcome.state, VerdictState::Unverifiable);
    assert!(outcome.reason.unwrap_or_default().contains("error count"));
    captured_run(
        &ledger,
        repo.path(),
        &commit,
        "cargo test",
        1,
        true,
        "failed-tests",
    )?;
    assert_eq!(
        check_recorded_at(
            &ledger,
            repo.path(),
            &commit,
            &tree,
            &CheckParams::CommandPasses(params("cargo test", true)),
        )?
        .expect("E1 dispatch")
        .state,
        VerdictState::Broken,
        "a captured failed test command is broken even without an error tally"
    );
    Ok(())
}

#[test]
fn d1_d2_record_dispatch_refuses_unbound_edit_events() -> Result<(), Box<dyn Error>> {
    let (repo, commit, tree) = repo()?;
    let ledger = Ledger::open_in_memory()?;
    for check in [
        CheckParams::EditsWithinPaths(EditsWithinPathsParams {
            allowed_paths: vec!["src/**".into()],
        }),
        CheckParams::PathsUntouched(PathsUntouchedParams {
            paths: vec!["tests/**".into()],
        }),
    ] {
        let outcome = check_recorded_at(&ledger, repo.path(), &commit, &tree, &check)?
            .expect("recorded kind dispatch");
        assert_eq!(outcome.state, VerdictState::Unverifiable);
        assert!(outcome.reason.unwrap_or_default().contains("edit census"));
    }
    Ok(())
}

#[test]
fn engine_dispatches_e1_and_persists_exact_version_verdict() -> Result<(), Box<dyn Error>> {
    let (repo, commit, _tree) = repo()?;
    let ledger = Ledger::open(repo.path().join(".git/decision-ledger.db"))?;
    ledger.conn().execute(
        "INSERT INTO projects(id,root_path,source,created_at) VALUES(?1,?2,'codex','2026-10-07T00:00:00Z')",
        rusqlite::params!["project-a", repo.path().to_string_lossy().as_ref()],
    )?;
    let span = "cargo build passes";
    let id = storage::create(
        &ledger,
        NewDecision {
            project_id: "project-a".into(),
            source_anchor: "source-1".into(),
            span: span.into(),
            sources: vec![DecisionSource {
                source_kind: SourceKind::RuleFile,
                event_id: None,
                file_anchor: Some("source-1".into()),
                raw_file: "notes/decision.md".into(),
                raw_line: 1,
                span_start: 0,
                span_end: span.len() as u64,
                span: span.into(),
                actor: Some("founder".into()),
                recorded_at: Some("2026-10-07T00:00:00Z".into()),
            }],
            author_actor: "founder".into(),
            recorded_at: "2026-10-07T00:00:00Z".into(),
            params: CheckParams::CommandPasses(params("cargo build", false)),
        },
    )?;
    let acceptance_id = storage::accept(
        &ledger,
        AcceptanceInput {
            decision_id: id.clone(),
            revision: 1,
            actor: "founder".into(),
            at: "2026-10-07T00:00:01Z".into(),
            because: "Standing check".into(),
            severity: "blocks".into(),
            applies_from_version: commit.clone(),
        },
    )?;
    let mut person = PersonRecorder::new(&ledger, "project-a");
    person.bind_acceptance(&ledger, &acceptance_id);
    captured_run(
        &ledger,
        repo.path(),
        &commit,
        "cargo build",
        0,
        true,
        "build",
    )?;
    let result = engine::check_decision_version(
        &ledger,
        "project-a",
        repo.path(),
        &commit,
        "2026-10-07T00:01:00Z",
        false,
        &id,
    )?;
    assert_eq!(result[0].state, VerdictState::Holds);
    assert_eq!(result[0].outcome.checked_places, 1);
    assert!(result[0].verdict_id.is_some());
    Ok(())
}
