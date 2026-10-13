//! Accepted J selectors survive an unavailable record adapter unchanged.
use serde_json::json;
use thruline_core::decisions::claim_record_adapter::project_claim_at;
use thruline_core::decisions::params::{
    CheckParams, DecisionSatisfiedParams, DelegatedRunParams, EditDuringRunParams, EmptyRunParams,
    StaleClaimParams, SummedTotalsParams,
};
use thruline_core::Ledger;

#[test]
fn all_j_selectors_are_retained_and_empty_record_cannot_be_complete() {
    let ledger = Ledger::open_in_memory().unwrap();
    let repo = tempfile::tempdir().unwrap();
    let cases = [
        CheckParams::DecisionSatisfied(DecisionSatisfiedParams {
            claim_id: "claim-a".into(),
            decision_id: "decision-b".into(),
            binding_basis: "explicit".into(),
        }),
        CheckParams::StaleClaim(StaleClaimParams {
            claim_id: "claim-a".into(),
        }),
        CheckParams::SummedTotals(SummedTotalsParams {
            claim_id: "claim-a".into(),
            run_ids: vec!["run-a".into(), "run-b".into()],
        }),
        CheckParams::EmptyRun(EmptyRunParams {
            claim_id: "claim-a".into(),
            run_id: "run-b".into(),
        }),
        CheckParams::DelegatedRun(DelegatedRunParams {
            claim_id: "claim-a".into(),
            run_source: "ci".into(),
        }),
        CheckParams::EditDuringRun(EditDuringRunParams {
            run_id: "run-b".into(),
            edit_ids: vec!["edit-a".into()],
        }),
    ];
    for selector in cases {
        let projection = project_claim_at(
            &ledger,
            repo.path(),
            "unrecorded-project",
            &"a".repeat(40),
            &"b".repeat(40),
            &selector,
        )
        .unwrap();
        assert_eq!(
            projection.selectors,
            serde_json::to_value(&selector).unwrap()
        );
        assert!(!projection.census.complete);
        assert!(projection.census.claim.version_id.is_empty());
        assert!(projection.census.claim.tree_hash.is_empty());
        assert!(projection.sources.is_empty());
        assert!(!projection.missing.is_empty());
    }
}

#[test]
fn j6_preserves_run_and_edit_ids_without_fabricating_an_assistant_claim() {
    let ledger = Ledger::open_in_memory().unwrap();
    let repo = tempfile::tempdir().unwrap();
    let params = CheckParams::EditDuringRun(EditDuringRunParams {
        run_id: "run-one".into(),
        edit_ids: vec!["edit-two".into()],
    });
    let projection = project_claim_at(
        &ledger,
        repo.path(),
        "missing",
        &"a".repeat(40),
        &"b".repeat(40),
        &params,
    )
    .unwrap();
    assert_eq!(
        projection.selectors,
        json!({"kind":"edit_during_run","run_id":"run-one","edit_ids":["edit-two"]})
    );
    assert!(projection.census.claim.id.is_empty());
    assert!(projection
        .missing
        .contains(&"J6_requires_a_run_and_edit_adapter_not_a_synthetic_claim"));
}

fn wrapper_fixture(clean: bool) -> (tempfile::TempDir, Ledger, String, String, String) {
    use std::process::Command;
    use thruline_core::evidence_binding::{
        self, EvidenceBinding, ExecMode, IdentityEdge, StreamCapture, WsidScopeDecl,
    };
    use thruline_core::examined_inputs::{self, ExaminedInputs, Input, SubjectKind, Uncommitted};
    use thruline_core::run_execution::{self, ExecutionState, Finish, Start};
    use thruline_core::test_tally::{self, TestTally};
    let repo = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo.path())
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    git(&["init", "-q"]);
    std::fs::write(repo.path().join("app.rs"), "fn app() {}\n").unwrap();
    git(&["add", "app.rs"]);
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "-qm",
        "fixture",
    ]);
    let version = git(&["rev-parse", "HEAD"]);
    let tree = git(&["rev-parse", "HEAD^{tree}"]);
    let ledger = Ledger::open_in_memory().unwrap();
    let root = repo
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let repo_id = thruline_core::workspace::repo_identity_at(repo.path())
        .unwrap()
        .repo_id
        .unwrap();
    let command = "cargo test -p fixture";
    let stream = StreamCapture::Captured {
        sha256: thruline_core::chain::sha256_hex(b""),
        bytes: 0,
    };
    let binding = EvidenceBinding {
        argv: command.split_whitespace().map(str::to_owned).collect(),
        mode: ExecMode::Direct,
        cwd: root.clone(),
        root: root.clone(),
        repository_identity: repo_id,
        scope: evidence_binding::classify_command(command, &root),
        wsid_scope: WsidScopeDecl::everything(),
        pre: IdentityEdge::complete("wsid1:recorded-run".into()),
        post: IdentityEdge::complete("wsid1:recorded-run".into()),
        exit_code: Some(0),
        term_signal: None,
        stdout: stream.clone(),
        stderr: stream,
        artifacts: Vec::new(),
        wrapper_defects: Vec::new(),
        started_at: "2026-10-07T00:00:00Z".into(),
        bound_at: "2026-10-07T00:01:00Z".into(),
        source_adapter: "fixture".into(),
        source_session: "execution-session".into(),
    };
    let id = evidence_binding::store(&ledger, &binding)
        .unwrap()
        .stored()
        .id
        .clone();
    let mut inputs = ExaminedInputs::unknown_for(SubjectKind::CheckRun, &id);
    inputs.base_revision = Input::known(version.clone());
    inputs.source_identity = Input::known("wsid1:recorded-run");
    inputs.uncommitted = if clean {
        Uncommitted::Clean
    } else {
        Uncommitted::Present {
            files: 1,
            diff_sha256: Input::known("dirty"),
        }
    };
    examined_inputs::record(&ledger, &inputs, "2026-10-07T00:01:00Z").unwrap();
    run_execution::start(
        &ledger,
        &Start {
            run_key: "execution-session".into(),
            change_id: None,
            name: command.into(),
            pid: None,
            process_group: None,
            started_at: "2026-10-07T00:00:00Z".into(),
            examined_inputs_id: None,
        },
    )
    .unwrap();
    run_execution::finish(
        &ledger,
        &Finish {
            run_key: "execution-session".into(),
            state: ExecutionState::Completed,
            reason: None,
            actor: None,
            binding_id: Some(id.clone()),
            cleanup_verified: None,
            survivors: None,
            at: "2026-10-07T00:01:00Z".into(),
        },
    )
    .unwrap();
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
    )
    .unwrap();
    (repo, ledger, version, tree, id)
}

#[test]
fn an_exact_completed_wrapper_run_keeps_its_actual_session_and_unjoined_order() {
    use thruline_core::decisions::checks_claims::RunOrigin;
    let (repo, ledger, version, tree, id) = wrapper_fixture(true);
    let params = CheckParams::EmptyRun(EmptyRunParams {
        claim_id: "missing-assistant-claim".into(),
        run_id: id,
    });
    let projection =
        project_claim_at(&ledger, repo.path(), "missing", &version, &tree, &params).unwrap();
    assert!(
        !projection.census.complete,
        "one complete execution cannot complete a missing claim census"
    );
    assert_eq!(projection.census.runs.len(), 1);
    let run = &projection.census.runs[0];
    assert_eq!(run.session_id, "execution-session");
    assert_eq!(
        run.seq, 0,
        "wrapper ordering cannot become a source session seq"
    );
    assert_eq!(run.origin, RunOrigin::OtherSession);
    assert_eq!(run.passed, Some(3));
    assert_eq!(run.failed, Some(0));
    assert!(run.complete);
    assert!(
        run.examined_paths.is_empty(),
        "a clean workspace does not identify what a runner examined"
    );
    assert_eq!(projection.sources.len(), 1);
}

#[test]
fn a_dirty_wrapper_cannot_supply_an_exact_commit_run_receipt() {
    let (repo, ledger, version, tree, id) = wrapper_fixture(false);
    let params = CheckParams::EmptyRun(EmptyRunParams {
        claim_id: "missing-assistant-claim".into(),
        run_id: id,
    });
    let projection =
        project_claim_at(&ledger, repo.path(), "missing", &version, &tree, &params).unwrap();
    assert!(!projection.census.complete);
    assert!(projection.census.runs.is_empty());
    assert!(projection.sources.is_empty());
}

fn recorded_marker_claim() -> (tempfile::TempDir, Ledger, String) {
    use rusqlite::params;
    use thruline_core::chain::{chain_hash, genesis_prev_hash, sha256_hex};
    use thruline_core::codex::{normalize_codex_line, CodexLineContext};
    use thruline_core::extract::{ClaimExtractor, MarkerExtractor, EXTRACTOR_NAME};
    let temp = tempfile::tempdir().unwrap();
    let ledger = Ledger::open_in_memory().unwrap();
    let session = thruline_core::adapter::stable_source_session_id(
        "claim-owner",
        thruline_core::model::Source::Codex,
        "native-session",
    );
    let raw_file = temp.path().join("claim.jsonl");
    let text = "All 3 tests pass.";
    let values = [
        json!({"type":"session_meta","payload":{"id":"native-session","source":"cli","cwd":temp.path()}}),
        json!({"type":"turn_context","payload":{"turn_id":"turn-one"}}),
        json!({"type":"response_item","payload":{"type":"message","role":"assistant",
            "content":[{"type":"output_text","text":text}]}}),
    ];
    let lines = values
        .iter()
        .map(serde_json::Value::to_string)
        .collect::<Vec<_>>();
    std::fs::write(&raw_file, format!("{}\n", lines.join("\n"))).unwrap();
    ledger.conn().execute("INSERT INTO projects(id,root_path,source,created_at) VALUES('claim-project',?1,'codex','now')",[temp.path().to_string_lossy().as_ref()]).unwrap();
    ledger.conn().execute("INSERT INTO sessions(id,project_id,file_path,started_at) VALUES(?1,'claim-project',?2,'now')",params![session,raw_file.to_string_lossy()]).unwrap();
    ledger
        .conn()
        .execute(
            "INSERT INTO source_sessions(session_id,project_id,owner_key,source,source_session_key)
        VALUES(?1,'claim-project','claim-owner','codex','native-session')",
            [session.as_str()],
        )
        .unwrap();
    let boundary_hash = sha256_hex(lines[1].as_bytes());
    let mut previous = genesis_prev_hash("claim-project");
    for (part, (line, value)) in lines.iter().zip(values.iter()).enumerate() {
        let raw_hash = sha256_hex(line.as_bytes());
        let context = CodexLineContext {
            owner_session_id: &session,
            source_session_id: "native-session",
            file_id: &session,
            turn_scope: if part == 0 {
                "pre-turn"
            } else {
                "turn:turn-one"
            },
            turn_boundary_line: (part > 0).then_some(2),
            turn_boundary_sha256: (part > 0).then_some(boundary_hash.as_str()),
            line: part as u64 + 1,
            raw_sha256: &raw_hash,
        };
        let mapped = normalize_codex_line(context, value).unwrap();
        assert_eq!(mapped.events.len(), 1);
        let event = &mapped.events[0].event;
        let seq = part as i64 + 1;
        let chain = chain_hash(&previous, &raw_hash, &session, seq);
        let event_id = match part {
            0 => "claim-meta",
            1 => "claim-boundary",
            _ => "claim-event",
        };
        ledger.conn().execute("INSERT INTO events(id,project_id,session_id,seq,kind,role,text,source_turn_id,line_session_id,
            source_turn_boundary_line,source_turn_boundary_sha256,raw_file,raw_line,raw_sha256,prev_hash,chain_hash)
            VALUES(?1,'claim-project',?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            params![event_id,session,seq,event.kind.as_str(),event.role,event.text,event.source_turn_id,event.line_session_id,
                event.source_turn_boundary_line.map(|line| i64::try_from(line).expect("fixture line")),event.source_turn_boundary_sha256,raw_file.to_string_lossy(),seq,raw_hash,previous,chain]).unwrap();
        previous = chain;
    }
    let claim = MarkerExtractor.extract(text).into_iter().next().unwrap();
    let id = format!(
        "c{}",
        &sha256_hex(
            format!(
                "claim-event\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
                EXTRACTOR_NAME, claim.span_start, claim.span_end, claim.text
            )
            .as_bytes()
        )[..31]
    );
    ledger
        .conn()
        .execute(
            "INSERT INTO claims(id,event_id,text,span_start,span_end,extractor,kind,marker)
        VALUES(?1,'claim-event',?2,?3,?4,?5,'marker',?6)",
            params![
                id,
                claim.text,
                claim.span_start as i64,
                claim.span_end as i64,
                EXTRACTOR_NAME,
                claim.marker.as_str()
            ],
        )
        .unwrap();
    (temp, ledger, id)
}

#[test]
fn an_existing_marker_claim_is_rederived_from_its_exact_raw_span_without_borrowing_the_selected_tree(
) {
    use thruline_core::decisions::checks_claims::ClaimKind;
    let (temp, ledger, id) = recorded_marker_claim();
    let params = CheckParams::StaleClaim(StaleClaimParams { claim_id: id });
    let projection = project_claim_at(
        &ledger,
        temp.path(),
        "claim-project",
        &"a".repeat(40),
        &"b".repeat(40),
        &params,
    )
    .unwrap();
    assert_eq!(projection.sources.len(), 1);
    assert_eq!(
        projection.census.claim.session_id,
        thruline_core::adapter::stable_source_session_id(
            "claim-owner",
            thruline_core::model::Source::Codex,
            "native-session"
        )
    );
    assert_eq!(projection.census.claim.kind, ClaimKind::TestsPass);
    assert_eq!(projection.census.claim.reported_passed, Some(3));
    assert!(projection.census.claim.version_id.is_empty());
    assert!(projection.census.claim.tree_hash.is_empty());
    assert!(!projection.census.complete);
}

#[test]
fn a_tampered_marker_source_cannot_supply_a_typed_claim_receipt() {
    let (temp, ledger, id) = recorded_marker_claim();
    let raw_file = temp.path().join("claim.jsonl");
    let raw = std::fs::read_to_string(&raw_file).unwrap();
    std::fs::write(
        &raw_file,
        raw.replace("All 3 tests pass.", "All 9 tests pass."),
    )
    .unwrap();
    let params = CheckParams::StaleClaim(StaleClaimParams { claim_id: id });
    let projection = project_claim_at(
        &ledger,
        temp.path(),
        "claim-project",
        &"a".repeat(40),
        &"b".repeat(40),
        &params,
    )
    .unwrap();
    assert!(projection.sources.is_empty());
    assert!(projection.census.claim.session_id.is_empty());
    assert!(!projection.census.complete);
}
