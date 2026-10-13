//! A captured tool request is useful evidence, but it is not a complete
//! exact-version process or protected-file census.

use std::fs;
use std::path::Path;
use std::process::Command;

use thruline_core::decisions::checks::VerdictState;
use thruline_core::decisions::checks_preservation::{
    check_rule_file_present, RuleFilePresentParams,
};
use thruline_core::decisions::checks_process::{
    check_command_forbidden, check_consent_recorded, check_one_heavy_job, check_tests_before_code,
    CommandForbiddenParams, ConsentRecordedParams, OneHeavyJobParams, ProcessActor, ProcessWindow,
    TestsBeforeCodeParams,
};
use thruline_core::decisions::record_adapter_phase2::project_phase2_at;
use thruline_core::Ledger;

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git available");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git utf8")
        .trim()
        .into()
}

fn fixture() -> (tempfile::TempDir, Ledger, String, String) {
    let temp = tempfile::tempdir().expect("scratch");
    let repo = temp.path();
    git(repo, &["init", "-q"]);
    git(repo, &["config", "user.name", "Fixture"]);
    git(repo, &["config", "user.email", "fixture@example.invalid"]);
    fs::write(repo.join("CLAUDE.md"), "Keep this rule.\n").expect("rule file");
    git(repo, &["add", "."]);
    git(repo, &["commit", "-qm", "baseline"]);
    fs::create_dir(repo.join("src")).expect("source dir");
    fs::write(repo.join("src/main.rs"), "fn main() {}\n").expect("source");
    git(repo, &["add", "."]);
    git(repo, &["commit", "-qm", "target"]);
    let version = git(repo, &["rev-parse", "HEAD"]);
    let tree = git(repo, &["rev-parse", "HEAD^{tree}"]);
    let ledger = Ledger::open_in_memory().expect("ledger");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id,root_path,source,created_at) VALUES('record-project',?1,'codex','2026-10-07T00:00:00Z')",
            [repo.to_string_lossy().as_ref()],
        )
        .expect("project");
    ledger
        .conn()
        .execute(
            "INSERT INTO sessions(id,project_id,file_path,started_at) VALUES('agent-session','record-project','/scratch/agent.jsonl','2026-10-07T00:00:00Z')",
            [],
        )
        .expect("session");
    ledger
        .conn()
        .execute(
            "INSERT INTO events(id,project_id,session_id,seq,ts,kind,role,tool_name,tool_input,raw_file,raw_line,raw_sha256,prev_hash,chain_hash)
             VALUES('cmd-1','record-project','agent-session',1,'2026-10-07T00:00:01Z','tool_use','assistant','Bash',?1,'/scratch/agent.jsonl',1,'raw-1','prev-1','hash-1')",
            [r#"{"command":"git push origin main"}"#],
        )
        .expect("command event");
    ledger
        .conn()
        .execute(
            "INSERT INTO events(id,project_id,session_id,seq,ts,kind,role,tool_name,tool_input,raw_file,raw_line,raw_sha256,prev_hash,chain_hash)
             VALUES('edit-2','record-project','agent-session',2,'2026-10-07T00:00:02Z','tool_use','assistant','Edit',?1,'/scratch/agent.jsonl',2,'raw-2','hash-1','hash-2')",
            [r#"{"file_path":"CLAUDE.md","old_string":"Keep this rule.","new_string":"Change this rule."}"#],
        )
        .expect("edit event");
    (temp, ledger, version, tree)
}

#[test]
fn recorded_tool_requests_project_as_partial_and_all_f_b5_checks_abstain() {
    let (temp, ledger, version, tree) = fixture();
    let projection = project_phase2_at(&ledger, temp.path(), "record-project", &version, &tree)
        .expect("projection");
    assert!(projection.target_verified);
    assert!(!projection.process.complete);
    assert!(!projection.rule_edits.complete);
    assert_eq!(projection.process.commands.len(), 1);
    assert_eq!(projection.process.commands[0].id, "cmd-1");
    assert_eq!(projection.process.commands[0].actor, ProcessActor::Agent);
    assert!(!projection.process.commands[0].complete);
    assert_eq!(projection.process.commands[0].seq, 0);
    assert!(projection.process.commands[0].version_id.is_empty());
    assert_eq!(projection.rule_edits.edits.len(), 1);
    assert_eq!(projection.rule_edits.edits[0].path, "CLAUDE.md");
    assert!(projection.missing.contains(&"exact_event_version_tree"));
    assert!(projection.missing.contains(&"person_edit_provenance"));

    let process = &projection.process;
    assert_eq!(
        check_command_forbidden(
            &version,
            &tree,
            &CommandForbiddenParams {
                patterns: vec!["git push".into()],
                session_scope: None,
            },
            process,
        )
        .state,
        VerdictState::Unverifiable
    );
    assert_eq!(
        check_tests_before_code(
            &version,
            &tree,
            &TestsBeforeCodeParams {
                change_id: "chg-1".into(),
                test_paths: vec!["tests/**".into()],
                source_paths: vec!["src/**".into()],
            },
            process,
        )
        .state,
        VerdictState::Unverifiable
    );
    assert_eq!(
        check_consent_recorded(
            &version,
            &tree,
            &ConsentRecordedParams {
                action_class: "install_hooks".into(),
                consent_event_kind: "permission_answer".into(),
            },
            process,
        )
        .state,
        VerdictState::Unverifiable
    );
    assert_eq!(
        check_one_heavy_job(
            &version,
            &tree,
            &OneHeavyJobParams {
                command_patterns: vec!["cargo test".into()],
                window: ProcessWindow {
                    start_ms: 1,
                    end_ms: 2,
                },
            },
            process,
        )
        .state,
        VerdictState::Unverifiable
    );
    assert_eq!(
        check_rule_file_present(
            temp.path(),
            &version,
            &RuleFilePresentParams {
                paths: vec!["CLAUDE.md".into()],
                person_edit_basis: "same_version".into(),
            },
            &projection.rule_edits,
        )
        .state,
        VerdictState::Unverifiable
    );
}

#[test]
fn wrong_tree_and_other_project_cannot_borrow_recorded_tools() {
    let (temp, ledger, version, tree) = fixture();
    let wrong = project_phase2_at(
        &ledger,
        temp.path(),
        "record-project",
        &version,
        &"0".repeat(40),
    )
    .expect("wrong tree projection");
    assert!(!wrong.target_verified);
    assert!(wrong.process.commands.is_empty());
    assert!(wrong.rule_edits.edits.is_empty());
    assert!(wrong.missing.contains(&"exact_commit_tree_mismatch"));

    let other = project_phase2_at(&ledger, temp.path(), "other-project", &version, &tree)
        .expect("other project projection");
    assert!(other.target_verified);
    assert!(other.process.commands.is_empty());
    assert!(other.rule_edits.edits.is_empty());
    assert!(other.missing.contains(&"ledger_project_missing"));
}
