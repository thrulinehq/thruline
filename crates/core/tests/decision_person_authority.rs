//! Person acts require a recorded, exact user confirmation and raw receipt.

use std::io::Write;
use std::path::PathBuf;

use serde_json::json;
use thruline_core::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use thruline_core::decisions::authority::{self, PersonActKind};
use thruline_core::decisions::params::{CheckParams, PathPresentParams};
use thruline_core::decisions::storage::{
    self, AcceptanceInput, DecisionSource, NewDecision, RetirementInput, SourceKind, WaiverInput,
};
use thruline_core::Ledger;

struct Fixture {
    _temp: tempfile::TempDir,
    ledger: Ledger,
    raw_file: PathBuf,
    decision_id: String,
    acceptance_id: String,
    params_sha256: String,
    next_seq: i64,
    prev_hash: String,
    raw_line_offset: i64,
}

impl Fixture {
    fn new() -> Self {
        Self::with_source("claude_code")
    }

    fn with_source(source: &str) -> Self {
        let temp = tempfile::tempdir().expect("scratch");
        let raw_file = temp.path().join("person-session.jsonl");
        let raw_line_offset = if source == "codex" { 1 } else { 0 };
        if source == "codex" {
            std::fs::write(
                &raw_file,
                format!(
                    "{}\n",
                    json!({"type":"session_meta","payload":{"id":"person-session","source":"cli"}})
                ),
            )
            .expect("Codex session metadata");
        } else {
            std::fs::write(&raw_file, "").expect("raw file");
        }
        let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
        ledger.conn().execute(
            "INSERT INTO projects(id,root_path,source,created_at) VALUES('authority-project','/repo',?1,'2026-10-07T00:00:00Z')",
            [source],
        ).expect("project");
        ledger.conn().execute(
            "INSERT INTO sessions(id,project_id,file_path,started_at) VALUES('person-session','authority-project',?1,'2026-10-07T00:00:00Z')",
            [raw_file.to_str().expect("file path")],
        ).expect("session");
        let decision_id = storage::create(
            &ledger,
            NewDecision {
                project_id: "authority-project".into(),
                source_anchor: "source-1".into(),
                span: "Keep the file".into(),
                sources: vec![DecisionSource {
                    source_kind: SourceKind::RuleFile,
                    event_id: None,
                    file_anchor: Some("source-1".into()),
                    raw_file: "AGENTS.md".into(),
                    raw_line: 1,
                    span_start: 0,
                    span_end: 13,
                    span: "Keep the file".into(),
                    actor: Some("person".into()),
                    recorded_at: Some("2026-10-07T00:00:00Z".into()),
                }],
                author_actor: "person".into(),
                recorded_at: "2026-10-07T00:00:00Z".into(),
                params: CheckParams::PathPresent(PathPresentParams {
                    path: "kept.txt".into(),
                    sha256: None,
                }),
            },
        )
        .expect("proposal");
        let acceptance_id = storage::accept(
            &ledger,
            AcceptanceInput {
                decision_id: decision_id.clone(),
                revision: 1,
                actor: "Founder".into(),
                at: "2026-10-07T00:01:00Z".into(),
                because: "Standing rule".into(),
                severity: "blocks".into(),
                applies_from_version: "a".repeat(40),
            },
        )
        .expect("acceptance");
        let params_json: String = ledger
            .conn()
            .query_row(
                "SELECT params_json FROM decision_revisions WHERE decision_id=?1 AND rev=1",
                [&decision_id],
                |row| row.get(0),
            )
            .expect("params");
        Self {
            _temp: temp,
            ledger,
            raw_file,
            decision_id,
            acceptance_id,
            params_sha256: sha256_hex(params_json.as_bytes()),
            next_seq: 1,
            prev_hash: genesis_prev_hash("authority-project"),
            raw_line_offset,
        }
    }

    fn accept_text(&self) -> String {
        json!({"thruline_decision_act":{
            "schema_version":1,"action":"accept","decision_id":self.decision_id,
            "revision":1,"params_sha256":self.params_sha256,
            "severity":"blocks","applies_from_version":"a".repeat(40),
            "actor":"Founder","because":"Standing rule"
        }})
        .to_string()
    }

    fn append_event(&mut self, kind: &str, role: &str, raw_type: &str, text: &str) -> String {
        let raw = match raw_type {
            "user" => {
                json!({"type":"user","sessionId":"person-session","message":{"role":role,"content":text}})
            }
            "user-subagent" => {
                json!({"type":"user","sessionId":"person-session","agentId":"agent-child",
                    "message":{"role":role,"content":text}})
            }
            "codex-user" => json!({"type":"response_item","payload":{"type":"message",
                "id":format!("person-item-{}",self.next_seq),"role":role,
                "content":[{"type":"input_text","text":text}]}}),
            "codex-mirror" => {
                json!({"type":"event_msg","payload":{"type":"user_message","message":text}})
            }
            _ => json!({"type":"assistant","sessionId":"person-session","message":{"role":role,
                "content":[{"type":"text","text":text}]}}),
        };
        let raw = raw.to_string();
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&self.raw_file)
            .expect("append raw");
        writeln!(file, "{raw}").expect("raw line");
        let event_id = format!("person-event-{}", self.next_seq);
        let raw_sha = sha256_hex(raw.as_bytes());
        let next_hash = chain_hash(&self.prev_hash, &raw_sha, "person-session", self.next_seq);
        self.ledger.conn().execute(
            "INSERT INTO events(id,project_id,session_id,seq,ts,kind,role,text,is_sidechain,raw_file,raw_line,raw_sha256,prev_hash,chain_hash,line_session_id)
             VALUES(?1,'authority-project','person-session',?2,'2026-10-07T00:02:00Z',?3,?4,?5,0,?6,?7,?8,?9,?10,?11)",
            rusqlite::params![event_id,self.next_seq,kind,role,text,self.raw_file.to_str(),
                self.next_seq + self.raw_line_offset,raw_sha,self.prev_hash,next_hash,
                (self.raw_line_offset == 1).then_some("person-session")],
        ).expect("event");
        self.next_seq += 1;
        self.prev_hash = next_hash;
        event_id
    }
}

#[test]
fn claude_code_agent_owned_user_item_cannot_authorize_person_act() {
    let mut fixture = Fixture::new();
    let text = fixture.accept_text();
    let id = fixture.append_event("user_msg", "user", "user-subagent", &text);
    assert!(!authority::verified_user_event(&fixture.ledger, &id).expect("agent-owned source"));
    assert!(authority::record(
        &fixture.ledger,
        PersonActKind::Accept,
        &fixture.acceptance_id,
        &id,
    )
    .is_err());
}

#[test]
fn accept_needs_exact_recorded_user_tuple_and_rechecks_raw_file() {
    let mut fixture = Fixture::new();
    assert!(!authority::verified(
        &fixture.ledger,
        PersonActKind::Accept,
        &fixture.acceptance_id
    )
    .expect("missing receipt"));
    let text = fixture.accept_text();
    let id = fixture.append_event("user_msg", "user", "user", &text);
    assert!(authority::verified_user_event(&fixture.ledger, &id).expect("person source"));
    let receipt_id = authority::record(
        &fixture.ledger,
        PersonActKind::Accept,
        &fixture.acceptance_id,
        &id,
    )
    .expect("record authority");
    assert_eq!(
        authority::record(
            &fixture.ledger,
            PersonActKind::Accept,
            &fixture.acceptance_id,
            &id,
        )
        .expect("same event retry"),
        receipt_id
    );
    assert!(authority::verified(
        &fixture.ledger,
        PersonActKind::Accept,
        &fixture.acceptance_id
    )
    .expect("verified"));
    assert!(fixture
        .ledger
        .conn()
        .execute("UPDATE decision_person_acts SET raw_line=2", [])
        .is_err());
    assert!(fixture
        .ledger
        .conn()
        .execute("DELETE FROM decision_person_acts", [])
        .is_err());
    std::fs::write(
        &fixture.raw_file,
        "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"tampered\"}}\n",
    )
    .expect("tamper");
    assert!(!authority::verified(
        &fixture.ledger,
        PersonActKind::Accept,
        &fixture.acceptance_id
    )
    .expect("tampered receipt"));
    assert!(!authority::verified_user_event(&fixture.ledger, &id).expect("tampered source"));
}

#[test]
fn role_or_tuple_claim_cannot_authorize_acceptance() {
    let mut fixture = Fixture::new();
    let wrong = fixture
        .accept_text()
        .replace("\"severity\":\"blocks\"", "\"severity\":\"warns\"");
    let wrong_id = fixture.append_event("user_msg", "user", "user", &wrong);
    assert!(authority::record(
        &fixture.ledger,
        PersonActKind::Accept,
        &fixture.acceptance_id,
        &wrong_id
    )
    .is_err());
    let text = fixture.accept_text();
    let assistant_id = fixture.append_event("user_msg", "user", "assistant", &text);
    assert!(
        !authority::verified_user_event(&fixture.ledger, &assistant_id)
            .expect("assistant raw source")
    );
    assert!(authority::record(
        &fixture.ledger,
        PersonActKind::Accept,
        &fixture.acceptance_id,
        &assistant_id
    )
    .is_err());
    assert!(!authority::verified(
        &fixture.ledger,
        PersonActKind::Accept,
        &fixture.acceptance_id
    )
    .expect("no authority"));
}

#[test]
fn broken_event_chain_cannot_authorize() {
    let mut fixture = Fixture::new();
    fixture.prev_hash = "0".repeat(64);
    let text = fixture.accept_text();
    let id = fixture.append_event("user_msg", "user", "user", &text);
    assert!(authority::record(
        &fixture.ledger,
        PersonActKind::Accept,
        &fixture.acceptance_id,
        &id
    )
    .is_err());
}

#[test]
fn canonical_codex_user_item_can_authorize_exact_act() {
    let mut fixture = Fixture::with_source("codex");
    let text = fixture.accept_text();
    let id = fixture.append_event("user_msg", "user", "codex-user", &text);
    assert!(authority::verified_user_event(&fixture.ledger, &id).expect("codex source"));
    authority::record(
        &fixture.ledger,
        PersonActKind::Accept,
        &fixture.acceptance_id,
        &id,
    )
    .expect("codex authority");
    assert!(authority::verified(
        &fixture.ledger,
        PersonActKind::Accept,
        &fixture.acceptance_id
    )
    .expect("codex receipt"));
}

#[test]
fn codex_successor_generation_uses_the_events_own_raw_file() {
    let mut fixture = Fixture::with_source("codex");
    let first_generation = fixture.raw_file.clone();
    let next_generation = fixture._temp.path().join("person-session-next.jsonl");
    std::fs::copy(&first_generation, &next_generation).expect("successor metadata");
    fixture.raw_file = next_generation.clone();
    let stored_session_file: String = fixture
        .ledger
        .conn()
        .query_row(
            "SELECT file_path FROM sessions WHERE id='person-session'",
            [],
            |row| row.get(0),
        )
        .expect("session path");
    assert_eq!(stored_session_file, first_generation.to_str().unwrap());
    let text = fixture.accept_text();
    let id = fixture.append_event("user_msg", "user", "codex-user", &text);
    assert!(authority::verified_user_event(&fixture.ledger, &id).expect("later generation"));
    authority::record(
        &fixture.ledger,
        PersonActKind::Accept,
        &fixture.acceptance_id,
        &id,
    )
    .expect("successor authority");
    assert!(authority::verified(
        &fixture.ledger,
        PersonActKind::Accept,
        &fixture.acceptance_id,
    )
    .expect("successor receipt"));
    assert_ne!(stored_session_file, next_generation.to_str().unwrap());
}

#[test]
fn codex_subagent_user_item_cannot_authorize_person_act() {
    let mut fixture = Fixture::with_source("codex");
    std::fs::write(
        &fixture.raw_file,
        format!(
            "{}\n",
            json!({"type":"session_meta","payload":{"id":"person-session","source":{
                "subagent":{"thread_spawn":{"parent_thread_id":"parent-session"}}
            }}})
        ),
    )
    .expect("subagent session metadata");
    let text = fixture.accept_text();
    let id = fixture.append_event("user_msg", "user", "codex-user", &text);
    assert!(!authority::verified_user_event(&fixture.ledger, &id).expect("subagent source"));
    assert!(authority::record(
        &fixture.ledger,
        PersonActKind::Accept,
        &fixture.acceptance_id,
        &id,
    )
    .is_err());
}

#[test]
fn codex_missing_or_mismatched_session_meta_cannot_authorize() {
    for meta in [
        json!({}),
        json!({"type":"session_meta","payload":{"id":"different-session","source":"cli"}}),
    ] {
        let mut fixture = Fixture::with_source("codex");
        std::fs::write(&fixture.raw_file, format!("{meta}\n")).expect("session metadata");
        let text = fixture.accept_text();
        let id = fixture.append_event("user_msg", "user", "codex-user", &text);
        assert!(!authority::verified_user_event(&fixture.ledger, &id).expect("invalid origin"));
        assert!(authority::record(
            &fixture.ledger,
            PersonActKind::Accept,
            &fixture.acceptance_id,
            &id,
        )
        .is_err());
    }
}

#[test]
fn codex_user_mirror_cannot_claim_canonical_person_authority() {
    let mut fixture = Fixture::with_source("codex");
    let text = fixture.accept_text();
    let id = fixture.append_event("user_msg", "user", "codex-mirror", &text);
    assert!(!authority::verified_user_event(&fixture.ledger, &id).expect("mirror source"));
    assert!(authority::record(
        &fixture.ledger,
        PersonActKind::Accept,
        &fixture.acceptance_id,
        &id,
    )
    .is_err());
}

#[test]
fn retire_and_waive_have_distinct_exact_person_actions() {
    let mut fixture = Fixture::new();
    let waive_id = storage::waive(
        &fixture.ledger,
        WaiverInput {
            decision_id: fixture.decision_id.clone(),
            version_id: "b".repeat(40),
            actor: "Founder".into(),
            at: "2026-10-07T00:04:00Z".into(),
            because: "One release exception".into(),
            expires_version: Some("c".repeat(40)),
            expires_at: None,
        },
    )
    .expect("waiver");
    let duplicate_waive_id = storage::waive(
        &fixture.ledger,
        WaiverInput {
            decision_id: fixture.decision_id.clone(),
            version_id: "b".repeat(40),
            actor: "Founder".into(),
            at: "2026-10-07T00:04:30Z".into(),
            because: "One release exception".into(),
            expires_version: Some("c".repeat(40)),
            expires_at: None,
        },
    )
    .expect("second waiver");
    let retire_id = storage::retire(
        &fixture.ledger,
        RetirementInput {
            decision_id: fixture.decision_id.clone(),
            actor: "Founder".into(),
            at: "2026-10-07T00:05:00Z".into(),
            because: "Changed course".into(),
            superseded_by: None,
        },
    )
    .expect("retirement");
    let retire_text = json!({"thruline_decision_act":{
        "schema_version":1,"action":"retire","decision_id":fixture.decision_id,
        "actor":"Founder","because":"Changed course","superseded_by":null
    }})
    .to_string();
    let retire_event = fixture.append_event("user_msg", "user", "user", &retire_text);
    authority::record(
        &fixture.ledger,
        PersonActKind::Retire,
        &retire_id,
        &retire_event,
    )
    .expect("retire authority");
    assert!(
        authority::verified(&fixture.ledger, PersonActKind::Retire, &retire_id)
            .expect("retirement verified")
    );
    let waive_text = json!({"thruline_decision_act":{
        "schema_version":1,"action":"waive","decision_id":fixture.decision_id,
        "version_id":"b".repeat(40),"actor":"Founder","because":"One release exception",
        "expires_version":"c".repeat(40),"expires_at":null
    }})
    .to_string();
    let waive_event = fixture.append_event("user_msg", "user", "user", &waive_text);
    authority::record(
        &fixture.ledger,
        PersonActKind::Waive,
        &waive_id,
        &waive_event,
    )
    .expect("waiver authority");
    assert!(
        authority::verified(&fixture.ledger, PersonActKind::Waive, &waive_id)
            .expect("waiver verified")
    );
    assert!(
        authority::record(
            &fixture.ledger,
            PersonActKind::Waive,
            &duplicate_waive_id,
            &waive_event
        )
        .is_err(),
        "one user event cannot authorize two acts"
    );
}
