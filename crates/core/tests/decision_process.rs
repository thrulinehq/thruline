//! F1–F4 use captured facts and exact version identities; absent facts never
//! produce a hold.

use thruline_core::decisions::checks::VerdictState;
use thruline_core::decisions::checks_process::{
    check_command_forbidden, check_consent_recorded, check_one_heavy_job, check_tests_before_code,
    CommandForbiddenParams, ConsentRecordedParams, OneHeavyJobParams, ProcessAction, ProcessActor,
    ProcessAnswer, ProcessCensus, ProcessCommand, ProcessEdit, ProcessRun, ProcessWindow,
    TestsBeforeCodeParams,
};

const VERSION: &str = "v2";
const TREE: &str = "tree-v2";

fn census() -> ProcessCensus {
    ProcessCensus {
        version_id: VERSION.into(),
        tree_hash: TREE.into(),
        complete: true,
        commands: Vec::new(),
        edits: Vec::new(),
        runs: Vec::new(),
        answers: Vec::new(),
        actions: Vec::new(),
    }
}

fn command(id: &str, session: &str, text: &str, start: i64, end: i64) -> ProcessCommand {
    ProcessCommand {
        id: id.into(),
        session_id: session.into(),
        actor: ProcessActor::Agent,
        seq: 1,
        version_id: VERSION.into(),
        tree_hash: TREE.into(),
        command: text.into(),
        complete: true,
        started_ms: Some(start),
        ended_ms: Some(end),
    }
}

#[test]
fn f1_matches_executed_command_tokens_and_fails_closed() {
    let params = CommandForbiddenParams {
        patterns: vec!["git push".into()],
        session_scope: None,
    };
    let mut read = census();
    read.commands
        .push(command("cmd1", "agent-a", "echo 'git push'", 1, 2));
    assert_eq!(
        check_command_forbidden(VERSION, TREE, &params, &read).state,
        VerdictState::Holds
    );
    read.commands.push(command(
        "cmd2",
        "agent-a",
        "cd repo && git push origin main",
        3,
        4,
    ));
    let broken = check_command_forbidden(VERSION, TREE, &params, &read);
    assert_eq!(broken.state, VerdictState::Broken);
    assert_eq!(broken.findings.len(), 1);
    read.commands[1].complete = false;
    read.commands.truncate(1);
    read.complete = false;
    assert_eq!(
        check_command_forbidden(VERSION, TREE, &params, &read).state,
        VerdictState::Unverifiable
    );
    read.complete = true;
    read.commands = vec![command(
        "cmd3",
        "agent-a",
        "sh -c 'git push origin main'",
        5,
        6,
    )];
    assert_eq!(
        check_command_forbidden(VERSION, TREE, &params, &read).state,
        VerdictState::Unverifiable,
        "a nested shell invocation cannot be treated as safe"
    );
    read.commands[0] = command("cmd4", "agent-a", "git push origin main", 7, 8);
    read.commands[0].actor = ProcessActor::Unknown;
    assert_eq!(
        check_command_forbidden(VERSION, TREE, &params, &read).state,
        VerdictState::Unverifiable,
        "unknown actor cannot be excluded from the agent census"
    );
}

#[test]
fn f2_requires_test_edit_red_source_edit_and_same_tree_green() {
    let params = TestsBeforeCodeParams {
        change_id: "chg1".into(),
        test_paths: vec!["tests/**".into()],
        source_paths: vec!["src/**".into()],
    };
    let mut read = census();
    read.edits = vec![
        ProcessEdit {
            change_id: "chg1".into(),
            session_id: "agent-a".into(),
            seq: 1,
            path: "tests/total.rs".into(),
            actor: ProcessActor::Agent,
        },
        ProcessEdit {
            change_id: "chg1".into(),
            session_id: "agent-a".into(),
            seq: 3,
            path: "src/total.rs".into(),
            actor: ProcessActor::Agent,
        },
    ];
    read.runs = vec![
        ProcessRun {
            change_id: "chg1".into(),
            session_id: "agent-a".into(),
            seq: 2,
            version_id: "v1".into(),
            tree_hash: "tree-v1".into(),
            exit_code: Some(1),
            passed: Some(0),
            failed: Some(1),
            complete: true,
        },
        ProcessRun {
            change_id: "chg1".into(),
            session_id: "agent-a".into(),
            seq: 4,
            version_id: VERSION.into(),
            tree_hash: TREE.into(),
            exit_code: Some(0),
            passed: Some(1),
            failed: Some(0),
            complete: true,
        },
    ];
    assert_eq!(
        check_tests_before_code(VERSION, TREE, &params, &read).state,
        VerdictState::Holds
    );
    read.edits[0].seq = 5;
    assert_eq!(
        check_tests_before_code(VERSION, TREE, &params, &read).state,
        VerdictState::Broken
    );
    read.edits[0].seq = 1;
    read.runs[1].tree_hash = "other-tree".into();
    assert_eq!(
        check_tests_before_code(VERSION, TREE, &params, &read).state,
        VerdictState::Unverifiable
    );
}

#[test]
fn f3_requires_preceding_granted_person_answer_in_same_session() {
    let params = ConsentRecordedParams {
        action_class: "install_hooks".into(),
        consent_event_kind: "permission_answer".into(),
    };
    let mut read = census();
    read.actions.push(ProcessAction {
        id: "act1".into(),
        session_id: "agent-a".into(),
        seq: 8,
        action_class: "install_hooks".into(),
    });
    assert_eq!(
        check_consent_recorded(VERSION, TREE, &params, &read).state,
        VerdictState::Broken
    );
    read.answers.push(ProcessAnswer {
        id: "ans1".into(),
        session_id: "agent-b".into(),
        seq: 7,
        event_kind: "permission_answer".into(),
        actor: ProcessActor::Person,
        granted: Some(true),
    });
    assert_eq!(
        check_consent_recorded(VERSION, TREE, &params, &read).state,
        VerdictState::Broken
    );
    read.answers[0].session_id = "agent-a".into();
    assert_eq!(
        check_consent_recorded(VERSION, TREE, &params, &read).state,
        VerdictState::Holds
    );
    read.answers[0].granted = None;
    assert_eq!(
        check_consent_recorded(VERSION, TREE, &params, &read).state,
        VerdictState::Unverifiable
    );
}

#[test]
fn f4_finds_cross_session_overlap_and_abstains_without_end_receipt() {
    let params = OneHeavyJobParams {
        command_patterns: vec!["cargo test".into()],
        window: ProcessWindow {
            start_ms: 0,
            end_ms: 100,
        },
    };
    let mut read = census();
    read.commands = vec![
        command("cmd1", "agent-a", "cargo test -p core", 10, 30),
        command("cmd2", "agent-b", "cargo test -p cli", 30, 50),
    ];
    assert_eq!(
        check_one_heavy_job(VERSION, TREE, &params, &read).state,
        VerdictState::Holds
    );
    read.commands[1].started_ms = Some(29);
    assert_eq!(
        check_one_heavy_job(VERSION, TREE, &params, &read).state,
        VerdictState::Broken
    );
    read.commands[1].started_ms = Some(40);
    read.commands[1].ended_ms = None;
    assert_eq!(
        check_one_heavy_job(VERSION, TREE, &params, &read).state,
        VerdictState::Unverifiable
    );
    read.commands = vec![command(
        "cmd3",
        "agent-a",
        "bash -c 'cargo test -p core'",
        60,
        70,
    )];
    assert_eq!(
        check_one_heavy_job(VERSION, TREE, &params, &read).state,
        VerdictState::Unverifiable,
        "nested shell work is not a proven absence of heavy jobs"
    );
}
