//! C2 and C4 compare fixed evidence on an exact Git tree. Neither runs a
//! command or reads a moving worktree to make a verdict.

use std::error::Error;
use std::path::Path;
use std::process::Command;

use thruline_core::chain::sha256_hex;
use thruline_core::decisions::checks::VerdictState;
use thruline_core::decisions::checks_fidelity::{check_file_equals_at, check_golden_output_at};
use thruline_core::decisions::params::{
    ByteSpan, FileEqualsParams, FileEqualsSpan, GoldenOutputParams,
};
use thruline_core::evidence_binding::{
    self, EvidenceBinding, ExecMode, IdentityEdge, StreamCapture, WsidScopeDecl,
};
use thruline_core::examined_inputs::{self, ExaminedInputs, Input, SubjectKind, Uncommitted};
use thruline_core::ledger::Ledger;
use thruline_core::run_execution::{self, ExecutionState, Finish, Start};

const C2_HOLD_LEFT: &[u8] = include_bytes!("fixtures/decisions/file_equals/holds/left.txt");
const C2_HOLD_RIGHT: &[u8] = include_bytes!("fixtures/decisions/file_equals/holds/right.txt");
const C2_BREAK_LEFT: &[u8] = include_bytes!("fixtures/decisions/file_equals/broken/left.txt");
const C2_BREAK_RIGHT: &[u8] = include_bytes!("fixtures/decisions/file_equals/broken/right.txt");
const C4_HOLD: &[u8] = include_bytes!("fixtures/decisions/golden_output/holds/golden.txt");
const C4_BREAK: &[u8] = include_bytes!("fixtures/decisions/golden_output/broken/golden.txt");

fn git(repo: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = Command::new("git").args(args).current_dir(repo).output()?;
    if !output.status.success() {
        return Err(format!("git {args:?}: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn fixture(files: &[(&str, &[u8])]) -> Result<(tempfile::TempDir, String), Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    git(repo.path(), &["init", "-q"])?;
    for (path, contents) in files {
        let full = repo.path().join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(full, contents)?;
    }
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
            "fixture",
        ],
    )?;
    let commit = git(repo.path(), &["rev-parse", "HEAD"])?;
    Ok((repo, commit))
}

fn c2(left: &str, right: &str) -> FileEqualsParams {
    FileEqualsParams {
        left: left.into(),
        right: right.into(),
        span: None,
    }
}

#[test]
fn c2_hold_and_broken_fixtures_use_bytes_from_the_named_tree() -> Result<(), Box<dyn Error>> {
    let (held, held_commit) = fixture(&[
        ("docs/left.txt", C2_HOLD_LEFT),
        ("docs/right.txt", C2_HOLD_RIGHT),
    ])?;
    let params = c2("docs/left.txt", "docs/right.txt");
    let first = check_file_equals_at(held.path(), &held_commit, &params);
    assert_eq!(first.state, VerdictState::Holds);
    assert_eq!(first.checked_places, 2);
    std::fs::write(
        held.path().join("docs/right.txt"),
        b"tampered working tree\n",
    )?;
    assert_eq!(
        check_file_equals_at(held.path(), &held_commit, &params),
        first
    );

    let (broken, broken_commit) = fixture(&[
        ("docs/left.txt", C2_BREAK_LEFT),
        ("docs/right.txt", C2_BREAK_RIGHT),
    ])?;
    let result = check_file_equals_at(broken.path(), &broken_commit, &params);
    assert_eq!(result.state, VerdictState::Broken);
    assert_eq!(result.checked_places, 2);
    assert_eq!(result.findings.len(), 1);
    assert!(result.findings[0].span.contains("differ"));
    assert!(result.findings[0].line >= 2);
    Ok(())
}

#[test]
fn c2_compares_accepted_byte_spans_and_reports_missing_file() -> Result<(), Box<dyn Error>> {
    let (repo, commit) = fixture(&[
        ("docs/with-wrapper.txt", b"before\nalpha\nbeta\nafter\n"),
        ("docs/bare.txt", b"alpha\nbeta\n"),
    ])?;
    let mut params = c2("docs/with-wrapper.txt", "docs/bare.txt");
    params.span = Some(FileEqualsSpan {
        left: Some(ByteSpan { start: 7, end: 18 }),
        right: None,
    });
    assert_eq!(
        check_file_equals_at(repo.path(), &commit, &params).state,
        VerdictState::Holds
    );
    params.span = Some(FileEqualsSpan {
        left: Some(ByteSpan { start: 7, end: 999 }),
        right: None,
    });
    assert_eq!(
        check_file_equals_at(repo.path(), &commit, &params).state,
        VerdictState::Broken
    );
    assert_eq!(
        check_file_equals_at(
            repo.path(),
            &commit,
            &c2("docs/missing.txt", "docs/bare.txt")
        )
        .state,
        VerdictState::Broken
    );
    assert_eq!(
        check_file_equals_at(
            repo.path(),
            &commit,
            &c2("../website/README.md", "docs/bare.txt")
        )
        .state,
        VerdictState::Unverifiable,
        "a second project cannot be smuggled in through a relative path"
    );
    Ok(())
}

fn recorded_output(
    ledger: &Ledger,
    repo: &Path,
    commit: &str,
    output: &[u8],
) -> Result<String, Box<dyn Error>> {
    let repo_id = thruline_core::workspace::repo_identity_at(repo)?
        .repo_id
        .expect("fixture repo identity");
    let root = repo.canonicalize()?.to_string_lossy().into_owned();
    let wsid = "wsid1:golden-fixture";
    let capture = StreamCapture::Captured {
        sha256: sha256_hex(output),
        bytes: output.len() as u64,
    };
    let binding = EvidenceBinding {
        argv: vec!["does-not-exist-and-must-not-run".into()],
        mode: ExecMode::Direct,
        cwd: root.clone(),
        root: root.clone(),
        repository_identity: repo_id,
        scope: evidence_binding::classify_command("does-not-exist-and-must-not-run", &root),
        wsid_scope: WsidScopeDecl::everything(),
        pre: IdentityEdge::complete(wsid.into()),
        post: IdentityEdge::complete(wsid.into()),
        exit_code: Some(0),
        term_signal: None,
        stdout: capture.clone(),
        stderr: StreamCapture::Captured {
            sha256: sha256_hex(b""),
            bytes: 0,
        },
        artifacts: vec![],
        wrapper_defects: vec![],
        started_at: "2026-10-07T00:00:00Z".into(),
        bound_at: "2026-10-07T00:01:00Z".into(),
        source_adapter: "fixture".into(),
        source_session: "golden-run".into(),
    };
    let id = evidence_binding::store(ledger, &binding)?
        .stored()
        .id
        .clone();
    let mut inputs = ExaminedInputs::unknown_for(SubjectKind::CheckRun, &id);
    inputs.source_identity = Input::known(wsid);
    inputs.base_revision = Input::known(commit);
    inputs.uncommitted = Uncommitted::Clean;
    examined_inputs::record(ledger, &inputs, "2026-10-07T00:01:00Z")?;
    run_execution::start(
        ledger,
        &Start {
            run_key: "golden-run".into(),
            change_id: None,
            name: "does-not-exist-and-must-not-run".into(),
            pid: None,
            process_group: None,
            started_at: "2026-10-07T00:00:00Z".into(),
            examined_inputs_id: None,
        },
    )?;
    run_execution::finish(
        ledger,
        &Finish {
            run_key: "golden-run".into(),
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
fn c4_compares_pinned_golden_with_same_version_recorded_stdout() -> Result<(), Box<dyn Error>> {
    let (repo, commit) = fixture(&[("golden.txt", C4_HOLD)])?;
    let ledger = Ledger::open_in_memory()?;
    let id = recorded_output(&ledger, repo.path(), &commit, C4_HOLD)?;
    let params = GoldenOutputParams {
        command_id: id,
        golden_path: "golden.txt".into(),
        golden_sha256: sha256_hex(C4_HOLD),
    };
    let held = check_golden_output_at(&ledger, repo.path(), &commit, &params)?;
    assert_eq!(held.state, VerdictState::Holds);
    assert_eq!(held.checked_places, 2);

    std::fs::write(repo.path().join("golden.txt"), C4_BREAK)?;
    assert_eq!(
        check_golden_output_at(&ledger, repo.path(), &commit, &params)?.state,
        VerdictState::Holds,
        "working-tree edits cannot change the named version"
    );
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
            "golden changed",
        ],
    )?;
    let newer = git(repo.path(), &["rev-parse", "HEAD"])?;
    assert_eq!(
        check_golden_output_at(&ledger, repo.path(), &newer, &params)?.state,
        VerdictState::Broken,
        "the pinned golden file changed in the newer tree"
    );
    Ok(())
}

#[test]
fn c4_output_difference_breaks_and_unbound_command_is_unverifiable() -> Result<(), Box<dyn Error>> {
    let (repo, commit) = fixture(&[("golden.txt", C4_BREAK)])?;
    let ledger = Ledger::open_in_memory()?;
    let id = recorded_output(&ledger, repo.path(), &commit, C4_HOLD)?;
    let mut params = GoldenOutputParams {
        command_id: id,
        golden_path: "golden.txt".into(),
        golden_sha256: sha256_hex(C4_BREAK),
    };
    let broken = check_golden_output_at(&ledger, repo.path(), &commit, &params)?;
    assert_eq!(broken.state, VerdictState::Broken);
    assert_eq!(broken.findings.len(), 1);
    params.command_id = "0".repeat(64);
    assert_eq!(
        check_golden_output_at(&ledger, repo.path(), &commit, &params)?.state,
        VerdictState::Unverifiable
    );
    Ok(())
}
