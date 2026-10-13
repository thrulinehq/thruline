//! F1 production adapter: a genuine captured forbidden invocation can break
//! the rule despite incomplete coverage; lack of a witness never becomes Holds.
use std::fs;
use std::path::Path;
use std::process::Command;

use rusqlite::params;
use serde_json::json;
use thruline_core::capture::{
    accept_capture_pair, record_source_assurance, CaptureAcceptance, CaptureRequest, Collector,
    SourceAssuranceMeasurement, SourceDelivery,
};
use thruline_core::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use thruline_core::codex::{normalize_codex_line, CodexLineContext};
use thruline_core::decisions::attested_command_adapter::check_forbidden_at;
use thruline_core::decisions::checks::VerdictState;
use thruline_core::decisions::checks_process::CommandForbiddenParams;
use thruline_core::Ledger;

const PROJECT: &str = "record-project";
fn recorded_session_id() -> String {
    thruline_core::adapter::stable_source_session_id(
        "owner",
        thruline_core::model::Source::Codex,
        NATIVE_SESSION,
    )
}
fn recorded_turn_id() -> String {
    thruline_core::codex::source_turn_identity(&recorded_session_id(), "turn:turn-one").unwrap()
}
const NATIVE_SESSION: &str = "record-session";
const COMMAND: &str = "git status --porcelain";

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

struct Fixture {
    temp: tempfile::TempDir,
    ledger: Ledger,
    version: String,
    tree: String,
    raw_file: std::path::PathBuf,
    raw_lines: Vec<String>,
    recorded_turn: String,
    command: String,
}

impl Fixture {
    fn new(actor_label_override: bool, broken_chain: bool) -> Self {
        Self::adversarial(actor_label_override, broken_chain, "none")
    }
    fn adversarial(actor_label_override: bool, broken_chain: bool, mode: &str) -> Self {
        let session = recorded_session_id();
        let recorded_turn = if mode == "turn" {
            thruline_core::codex::source_turn_identity(&session, "turn:different-turn").unwrap()
        } else {
            recorded_turn_id()
        };
        let command = match mode {
            "comment" => "# ; git status --porcelain",
            "quoted" => "echo 'git status --porcelain'",
            "flag" => "echo '--no-verify'",
            "conditional" => "false && git status --porcelain",
            "tilde" => "git status --porcelain ~",
            "quoted-tilde" => "git status --porcelain '~'",
            _ => COMMAND,
        }
        .to_owned();
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q"]);
        git(&repo, &["config", "user.name", "Decisions Fixture"]);
        git(&repo, &["config", "user.email", "fixture@example.invalid"]);
        fs::write(repo.join("tracked.txt"), "one\n").unwrap();
        git(&repo, &["add", "tracked.txt"]);
        git(&repo, &["commit", "-qm", "fixture"]);
        let version = git(&repo, &["rev-parse", "HEAD"]);
        let tree = git(&repo, &["rev-parse", "HEAD^{tree}"]);
        let ledger = Ledger::open(temp.path().join("ledger.db")).unwrap();
        ledger
            .conn()
            .execute(
                "INSERT INTO projects(id,root_path,source,created_at) VALUES(?1,?2,'codex','now')",
                params![PROJECT, repo.to_string_lossy()],
            )
            .unwrap();
        ledger.conn().execute(
            "INSERT INTO source_project_identities(project_id,owner_key,source,source_project_key)
             VALUES(?1,'owner','codex','native-project')",[PROJECT],
        ).unwrap();
        let raw_file = temp.path().join("source.jsonl");
        ledger
            .conn()
            .execute(
                "INSERT INTO sessions(id,project_id,file_path,started_at) VALUES(?1,?2,?3,'now')",
                params![session, PROJECT, raw_file.to_string_lossy()],
            )
            .unwrap();
        ledger.conn().execute(
            "INSERT INTO source_sessions(session_id,project_id,owner_key,source,source_session_key)
             VALUES(?1,?2,'owner','codex',?3)",params![session,PROJECT,NATIVE_SESSION],
        ).unwrap();
        let raw_lines = vec![
            json!({"type":"session_meta","payload":{"id":NATIVE_SESSION,"source":"cli","cwd":repo}}).to_string(),
            json!({"type":"turn_context","payload":{"turn_id":"turn-one"}}).to_string(),
            json!({"type":"response_item","payload":{"type":"function_call","name":"exec_command",
                "call_id":"call-one","arguments":json!({"cmd":command}).to_string()}}).to_string(),
            json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"call-one",
                "output":"Process exited with code 0"}}).to_string(),
        ];
        fs::write(&raw_file, format!("{}\n", raw_lines.join("\n"))).unwrap();
        let boundary_hash = sha256_hex(raw_lines[1].as_bytes());
        let mut previous = genesis_prev_hash(PROJECT);
        for (part, line) in raw_lines.iter().enumerate() {
            let raw_hash = sha256_hex(line.as_bytes());
            let value = serde_json::from_str(line).unwrap();
            let context = CodexLineContext {
                owner_session_id: &session,
                source_session_id: NATIVE_SESSION,
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
            let mapped = normalize_codex_line(context, &value).unwrap();
            assert_eq!(mapped.events.len(), 1);
            let event = &mapped.events[0].event;
            let seq = part as i64 + 1;
            let hash = chain_hash(&previous, &raw_hash, &session, seq);
            let id = match part {
                0 => "meta-event",
                1 => "boundary-event",
                2 => "call-event",
                _ => "result-event",
            };
            let role = if actor_label_override && part == 2 {
                Some("assistant")
            } else {
                event.role.as_deref()
            };
            let recorded_previous = if broken_chain && part == 2 {
                "invalid-prefix"
            } else {
                &previous
            };
            let turn = if mode == "turn" && part >= 2 {
                Some(recorded_turn.as_str())
            } else {
                event.source_turn_id.as_deref()
            };
            let forged_call = thruline_core::codex::source_tool_identity(&session, "other-call");
            let tool_id = if mode == "tool" && part >= 2 {
                Some(forged_call.as_str())
            } else {
                event.tool_use_id.as_deref()
            };
            let native = if mode == "native" && part == 2 {
                Some("other-native-session")
            } else {
                event.line_session_id.as_deref()
            };
            ledger.conn().execute(
                "INSERT INTO events(id,project_id,session_id,seq,kind,role,text,tool_name,source_tool_name,
                    tool_use_id,tool_input,tool_result,source_turn_id,line_session_id,
                    source_turn_boundary_line,source_turn_boundary_sha256,
                    raw_file,raw_line,raw_sha256,prev_hash,chain_hash)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)",
                params![id,PROJECT,session,seq,event.kind.as_str(),role,event.text,event.tool_name,
                    event.source_tool_name,tool_id,event.tool_input,event.tool_result,turn,native,
                    event.source_turn_boundary_line.map(|line| i64::try_from(line).unwrap()),event.source_turn_boundary_sha256,
                    raw_file.to_string_lossy(),part as i64 + 1,raw_hash,recorded_previous,hash],
            ).unwrap();
            previous = hash;
        }
        let fixture = Self {
            temp,
            ledger,
            version,
            tree,
            raw_file,
            raw_lines,
            recorded_turn,
            command,
        };
        if mode == "dirty" || mode == "staged" {
            fs::write(fixture.repo().join("tracked.txt"), "uncommitted change\n").unwrap();
            if mode == "staged" {
                git(&fixture.repo(), &["add", "tracked.txt"]);
            }
        }
        if mode != "no-capture" {
            fixture.capture("nonce-one");
        }
        fixture
    }

    fn repo(&self) -> std::path::PathBuf {
        self.temp.path().join("repo")
    }

    fn capture(&self, nonce: &str) {
        // Only a new disposable fixture collector is created; no user key is read.
        let collector =
            Collector::open(self.temp.path().join("capture"), "owner", "codex").unwrap();
        let config = sha256_hex(b"fixture-config");
        record_source_assurance(
            &self.ledger,
            &collector,
            &SourceAssuranceMeasurement {
                source: "codex".into(),
                source_version: "fixture-v1".into(),
                config_sha256: config.clone(),
                matcher: "exec_command".into(),
                pre_blocks: true,
                rewrite_exact: true,
                stable_tool_identity: true,
                completion_pairing: true,
                measured_pre_delay_ms: 400,
                child_started_after_pre: true,
                wrapper_exit_code: 0,
                source_exit_code: 0,
            },
        )
        .unwrap();
        let outcome = collector
            .execute(&CaptureRequest {
                owner_key: "owner".into(),
                source: "codex".into(),
                source_project_key: "native-project".into(),
                source_session_key: NATIVE_SESSION.into(),
                source_turn_id: Some(self.recorded_turn.clone()),
                source_tool_id: "call-one".into(),
                wrapper_nonce: nonce.into(),
                command: self.command.clone(),
                cwd: self.repo(),
                source_config_sha256: config,
                source_version: "fixture-v1".into(),
                assurance_enabled: true,
            })
            .unwrap();
        let delivery = SourceDelivery {
            schema_version: 1,
            attempt_id: outcome.attempt_id.clone(),
            owner_key: "owner".into(),
            source: "codex".into(),
            source_project_key: "native-project".into(),
            source_session_key: NATIVE_SESSION.into(),
            source_turn_id: Some(self.recorded_turn.clone()),
            source_tool_id: "call-one".into(),
            wrapper_nonce: nonce.into(),
            source_event_id: "result-event".into(),
            source_exit_code: Some(outcome.exit_code),
            source_term_signal: None,
            observed_at_unix_ms: 1,
        };
        let pre = outcome.pre.as_ref().unwrap();
        let post = outcome.post.as_ref().unwrap();
        accept_capture_pair(
            &self.ledger,
            &collector,
            &CaptureAcceptance {
                outcome: &outcome,
                delivery: &delivery,
                pre_private_relative_ref: format!(
                    "sealed/{}/pre.{}.json",
                    outcome.attempt_id, pre.capsule_id
                ),
                post_private_relative_ref: format!(
                    "sealed/{}/post.{}.json",
                    outcome.attempt_id, post.capsule_id
                ),
            },
        )
        .unwrap();
    }

    fn check(
        &self,
        pattern: &str,
        scope: Option<&str>,
    ) -> thruline_core::decisions::checks::CheckOutcome {
        check_forbidden_at(
            &self.ledger,
            &self.repo(),
            PROJECT,
            &self.version,
            &self.tree,
            &CommandForbiddenParams {
                patterns: vec![pattern.into()],
                session_scope: scope.map(str::to_owned),
            },
        )
        .unwrap()
    }
}

#[test]
fn f1_one_genuine_exact_tree_witness_breaks_without_a_complete_census() {
    let fixture = Fixture::new(false, false);
    let outcome = fixture.check("git status", None);
    assert_eq!(outcome.state, VerdictState::Broken);
    assert_eq!(outcome.findings.len(), 1);
    assert_eq!(outcome.findings[0].path, fixture.raw_file.to_string_lossy());
    assert_eq!(outcome.findings[0].line, 3);
    assert!(outcome.findings[0].span.contains("call-event"));
    assert!(outcome.findings[0].span.contains("raw_sha256"));
    assert!(outcome.findings[0].span.contains("attempt"));
    assert!(outcome.reason.unwrap().contains("incomplete"));
}

#[test]
fn f1_raw_source_tampering_removes_the_witness() {
    let mut fixture = Fixture::new(false, false);
    fixture.raw_lines[2] = fixture.raw_lines[2].replace("git status", "git log");
    fs::write(
        &fixture.raw_file,
        format!("{}\n", fixture.raw_lines.join("\n")),
    )
    .unwrap();
    assert_eq!(
        fixture.check("git status", None).state,
        VerdictState::Unverifiable
    );
}

#[test]
fn f1_forged_ledger_actor_label_cannot_override_raw_normalization() {
    let fixture = Fixture::new(true, false);
    assert_eq!(
        fixture.check("git status", None).state,
        VerdictState::Unverifiable
    );
}

#[test]
fn f1_broken_session_chain_cannot_supply_a_witness() {
    let fixture = Fixture::new(false, true);
    assert_eq!(
        fixture.check("git status", None).state,
        VerdictState::Unverifiable
    );
}

#[test]
fn f1_no_matching_witness_and_other_session_scope_never_hold() {
    let fixture = Fixture::new(false, false);
    assert_eq!(
        fixture.check("git push", None).state,
        VerdictState::Unverifiable
    );
    assert_eq!(
        fixture.check("git status", Some("different-session")).state,
        VerdictState::Unverifiable
    );
}

#[test]
fn f1_two_identical_capture_attempts_cannot_choose_an_attribution() {
    let fixture = Fixture::new(false, false);
    fixture.capture("nonce-two");
    assert_eq!(
        fixture.check("git status", None).state,
        VerdictState::Unverifiable
    );
}

#[test]
fn f1_wrong_tree_cannot_borrow_a_recorded_command() {
    let fixture = Fixture::new(false, false);
    let outcome = check_forbidden_at(
        &fixture.ledger,
        &fixture.repo(),
        PROJECT,
        &fixture.version,
        &"0".repeat(40),
        &CommandForbiddenParams {
            patterns: vec!["git status".into()],
            session_scope: None,
        },
    )
    .unwrap();
    assert_eq!(outcome.state, VerdictState::Unverifiable);
}

#[test]
fn f1_stored_turn_label_cannot_retarget_an_identical_command_to_another_capture_turn() {
    let fixture = Fixture::adversarial(false, false, "turn");
    assert_eq!(
        fixture.check("git status", None).state,
        VerdictState::Unverifiable
    );
}

#[test]
fn f1_stored_tool_call_identity_must_rederive_from_the_raw_call_id() {
    let fixture = Fixture::adversarial(false, false, "tool");
    assert_eq!(
        fixture.check("git status", None).state,
        VerdictState::Unverifiable
    );
}

#[test]
fn f1_stored_native_session_label_must_match_the_anchored_session_meta() {
    let fixture = Fixture::adversarial(false, false, "native");
    assert_eq!(
        fixture.check("git status", None).state,
        VerdictState::Unverifiable
    );
}

#[test]
fn f1_comment_only_forbidden_text_is_not_an_invocation() {
    let fixture = Fixture::adversarial(false, false, "comment");
    assert_eq!(
        fixture.check("git status", None).state,
        VerdictState::Unverifiable
    );
}

#[test]
fn f1_quoted_echo_arguments_do_not_become_programs_or_flags() {
    for (mode, pattern) in [("quoted", "git status"), ("flag", "--no-verify")] {
        let fixture = Fixture::adversarial(false, false, mode);
        assert_eq!(
            fixture.check(pattern, None).state,
            VerdictState::Unverifiable
        );
    }
}

#[test]
fn f1_unreached_conditional_branch_does_not_supply_an_invocation() {
    let fixture = Fixture::adversarial(false, false, "conditional");
    assert_eq!(
        fixture.check("git status", None).state,
        VerdictState::Unverifiable
    );
}

#[test]
fn f1_tilde_expansion_is_outside_the_supported_invocation_grammar() {
    for mode in ["tilde", "quoted-tilde"] {
        let fixture = Fixture::adversarial(false, false, mode);
        assert_eq!(
            fixture.check("git status", None).state,
            VerdictState::Unverifiable
        );
    }
}

#[test]
fn f1_stable_dirty_and_staged_captures_do_not_describe_the_committed_tree() {
    for mode in ["dirty", "staged"] {
        let fixture = Fixture::adversarial(false, false, mode);
        // Restore the present checkout. Only captured content, not today's
        // git status, can decide whether the historical run examined HEAD.
        git(&fixture.repo(), &["reset", "--hard", "HEAD"]);
        assert_eq!(
            fixture.check("git status", None).state,
            VerdictState::Unverifiable
        );
    }
}

#[path = "support/person_action.rs"]
mod person_action;

#[test]
fn f1_engine_rechecks_new_witnesses_and_tampered_source_on_the_same_tree() {
    use thruline_core::decisions::params::CheckParams;
    use thruline_core::decisions::storage::{
        AcceptanceInput, DecisionSource, NewDecision, SourceKind,
    };
    use thruline_core::decisions::{engine, storage};
    let mut fixture = Fixture::adversarial(false, false, "no-capture");
    let mut person = person_action::PersonRecorder::new(&fixture.ledger, PROJECT);
    let span = "Never invoke git status";
    let source_id = person.append_text(&fixture.ledger, span);
    let decision_id = storage::create(
        &fixture.ledger,
        NewDecision {
            project_id: PROJECT.into(),
            source_anchor: source_id.clone(),
            span: span.into(),
            sources: vec![DecisionSource {
                source_kind: SourceKind::WorkPrompt,
                event_id: Some(source_id),
                file_anchor: None,
                raw_file: person.raw_file().to_string_lossy().into_owned(),
                raw_line: 2,
                span_start: 0,
                span_end: span.len() as u64,
                span: span.into(),
                actor: Some("founder".into()),
                recorded_at: Some("2026-10-07T00:00:00Z".into()),
            }],
            author_actor: "founder".into(),
            recorded_at: "2026-10-07T00:00:00Z".into(),
            params: CheckParams::CommandForbidden(CommandForbiddenParams {
                patterns: vec!["git status".into()],
                session_scope: None,
            }),
        },
    )
    .unwrap();
    let acceptance = storage::accept(
        &fixture.ledger,
        AcceptanceInput {
            decision_id: decision_id.clone(),
            revision: 1,
            actor: "founder".into(),
            at: "2026-10-07T00:01:00Z".into(),
            because: "fixture proof requirement".into(),
            severity: "warns".into(),
            applies_from_version: fixture.version.clone(),
        },
    )
    .unwrap();
    person.bind_acceptance(&fixture.ledger, &acceptance);
    let check = |fixture: &Fixture, at: &str| {
        engine::check_decision_version(
            &fixture.ledger,
            PROJECT,
            &fixture.repo(),
            &fixture.version,
            at,
            false,
            &decision_id,
        )
        .unwrap()
    };
    let first = check(&fixture, "2026-10-07T00:02:00Z");
    assert_eq!(first[0].state, VerdictState::Unverifiable);
    fixture.capture("nonce-one");
    let second = check(&fixture, "2026-10-07T00:03:00Z");
    assert_eq!(
        second[0].state,
        VerdictState::Broken,
        "new recorded evidence must defeat an earlier cached U"
    );
    assert!(!second[0].from_cache);
    fixture.raw_lines[2] = fixture.raw_lines[2].replace("git status", "git log");
    fs::write(
        &fixture.raw_file,
        format!("{}\n", fixture.raw_lines.join("\n")),
    )
    .unwrap();
    let third = check(&fixture, "2026-10-07T00:04:00Z");
    assert_eq!(
        third[0].state,
        VerdictState::Unverifiable,
        "tampered raw proof must defeat a cached Broken"
    );
    assert!(!third[0].from_cache);
}
