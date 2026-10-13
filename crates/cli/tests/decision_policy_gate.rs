//! A PR head cannot waive its base policy, and a receipt cannot invent Holds.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use thruline_core::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use thruline_core::decisions::authority::{self, bridge, PersonActKind};
use thruline_core::decisions::params::{CheckParams, PathPresentParams};
use thruline_core::decisions::storage::{
    self, AcceptanceInput, DecisionSource, NewDecision, RetirementInput, RevisionInput, SourceKind,
    WaiverInput,
};
use thruline_core::ledger::Ledger;
use thruline_core::model::{EventKind, NormalizedEvent, RawLineRef};
use ulid::Ulid;

const BIN: &str = env!("CARGO_BIN_EXE_thruline");

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git launched");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git text")
        .trim()
        .to_string()
}

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    db: PathBuf,
    base: String,
    id: String,
    raw_file: PathBuf,
    next_seq: i64,
    prev_hash: String,
}

impl Fixture {
    fn new() -> Self {
        Self::new_with_authority(true)
    }

    fn new_unverified() -> Self {
        Self::new_with_authority(false)
    }

    fn new_with_authority(verified: bool) -> Self {
        let temp = tempfile::tempdir().expect("scratch");
        let root = temp.path().join("repo");
        std::fs::create_dir_all(root.join(".thruline")).expect("policy dir");
        git(&root, &["init", "-q"]);
        std::fs::write(root.join("kept.txt"), "present\n").expect("required file");
        std::fs::write(root.join("guard.txt"), "present\n").expect("guard file");
        std::fs::write(
            root.join(".thruline/policy.yml"),
            "version: 1\ndecisions:\n  require_hold: blocking\n",
        )
        .expect("base policy");
        git(
            &root,
            &["add", "kept.txt", "guard.txt", ".thruline/policy.yml"],
        );
        git(
            &root,
            &[
                "-c",
                "user.name=Person",
                "-c",
                "user.email=person@example.invalid",
                "commit",
                "-qm",
                "base",
            ],
        );
        let base = git(&root, &["rev-parse", "HEAD"]);
        let db = temp.path().join("ledger.db");
        let raw_file = temp.path().join("person-session.jsonl");
        std::fs::write(
            &raw_file,
            format!(
                "{}\n",
                serde_json::json!({
                    "type":"session_meta","payload":{"id":"pr-person-session","source":"cli"}
                })
            ),
        )
        .expect("raw session");
        let ledger = Ledger::open(&db).expect("scratch ledger");
        ledger
            .conn()
            .execute(
                "INSERT INTO projects(id,root_path,source,created_at) VALUES(?1,?2,'codex','2026-10-07T00:00:00Z')",
                ("project-pr", root.to_str().expect("root text")),
            )
            .expect("project");
        ledger.conn().execute(
            "INSERT INTO sessions(id,project_id,file_path,started_at) VALUES('pr-person-session','project-pr',?1,'2026-10-07T00:00:00Z')",
            [raw_file.to_str().expect("session path")],
        ).expect("session");
        let span = "Keep kept.txt";
        let id = storage::create(
            &ledger,
            NewDecision {
                project_id: "project-pr".into(),
                source_anchor: "person-rule:1".into(),
                span: span.into(),
                sources: vec![DecisionSource {
                    source_kind: SourceKind::RuleFile,
                    event_id: None,
                    file_anchor: Some("person-rule:1".into()),
                    raw_file: "AGENTS.md".into(),
                    raw_line: 1,
                    span_start: 0,
                    span_end: span.len() as u64,
                    span: span.into(),
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
        storage::accept(
            &ledger,
            AcceptanceInput {
                decision_id: id.clone(),
                revision: 1,
                actor: "person".into(),
                at: "2026-10-07T00:01:00Z".into(),
                because: "standing rule".into(),
                severity: "blocks".into(),
                applies_from_version: base.clone(),
            },
        )
        .expect("accepted");
        drop(ledger);
        let mut fixture = Self {
            _temp: temp,
            root,
            db,
            base,
            id,
            raw_file,
            next_seq: 1,
            prev_hash: genesis_prev_hash("project-pr"),
        };
        if verified {
            let id = fixture.id.clone();
            fixture.verify_acceptance(&id);
        }
        let ledger = Ledger::open(&fixture.db).expect("ledger");
        storage::write_derived_file(&ledger, "project-pr", &fixture.root).expect("derived copy");
        fixture
    }

    fn append_person_confirmation(&mut self, action: Value) -> String {
        let text = serde_json::json!({"thruline_decision_act":action}).to_string();
        let raw = serde_json::json!({"type":"response_item","payload":{"type":"message",
            "id":format!("pr-person-item-{}",self.next_seq),"role":"user",
            "content":[{"type":"input_text","text":text}]}})
        .to_string();
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&self.raw_file)
            .expect("append raw session");
        writeln!(file, "{raw}").expect("raw message");
        let event_id = format!("pr-person-event-{}", self.next_seq);
        let raw_sha = sha256_hex(raw.as_bytes());
        let next_hash = chain_hash(
            &self.prev_hash,
            &raw_sha,
            "pr-person-session",
            self.next_seq,
        );
        let ledger = Ledger::open(&self.db).expect("ledger");
        ledger.conn().execute(
            "INSERT INTO events(id,project_id,session_id,seq,ts,kind,role,text,is_sidechain,
             raw_file,raw_line,raw_sha256,prev_hash,chain_hash,line_session_id)
             VALUES(?1,'project-pr','pr-person-session',?2,'2026-10-07T00:02:00Z','user_msg','user',?3,0,?4,?5,?6,?7,?8,'pr-person-session')",
            rusqlite::params![event_id,self.next_seq,text,self.raw_file.to_str(),self.next_seq+1,
                raw_sha,self.prev_hash,next_hash],
        ).expect("person event");
        self.next_seq += 1;
        self.prev_hash = next_hash;
        event_id
    }

    fn verify_legacy_acceptance(&mut self, id: &str) {
        let ledger = Ledger::open(&self.db).expect("ledger");
        let (act_id, revision, params_json, severity, version, actor, because):
            (String, u32, String, String, String, String, String) = ledger.conn().query_row(
            "SELECT a.id,a.rev,r.params_json,a.severity,a.applies_from_version,a.actor,a.rationale
             FROM decision_acceptances a JOIN decision_revisions r
               ON r.decision_id=a.decision_id AND r.rev=a.rev
             WHERE a.decision_id=?1 ORDER BY a.rowid DESC LIMIT 1",
            [id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,
                row.get(4)?,row.get(5)?,row.get(6)?)),
        ).expect("acceptance row");
        if authority::verified(&ledger, PersonActKind::Accept, &act_id).expect("existing authority")
        {
            return;
        }
        let event = self.append_person_confirmation(serde_json::json!({
            "schema_version":1,"action":"accept","decision_id":id,"revision":revision,
            "params_sha256":sha256_hex(params_json.as_bytes()),"severity":severity,
            "applies_from_version":version,"actor":actor,"because":because,
        }));
        authority::record(&ledger, PersonActKind::Accept, &act_id, &event)
            .expect("acceptance authority");
        assert!(authority::verified(&ledger, PersonActKind::Accept, &act_id)
            .expect("verify acceptance"));
        storage::write_derived_file(&ledger, "project-pr", &self.root)
            .expect("verified derived copy");
    }

    fn verify_acceptance(&mut self, id: &str) {
        let ledger = Ledger::open(&self.db).expect("ledger");
        let (act_id, revision, params_json, severity, version, actor, because):
            (String, u32, String, String, String, String, String) = ledger.conn().query_row(
            "SELECT a.id,a.rev,r.params_json,a.severity,a.applies_from_version,a.actor,a.rationale
             FROM decision_acceptances a JOIN decision_revisions r
               ON r.decision_id=a.decision_id AND r.rev=a.rev
             WHERE a.decision_id=?1 ORDER BY a.rowid DESC LIMIT 1",
            [id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,
                row.get(4)?,row.get(5)?,row.get(6)?)),
        ).expect("acceptance row");
        if bridge::verified(&ledger, PersonActKind::Accept, &act_id).expect("existing bridge") {
            return;
        }
        let act = serde_json::json!({
            "schema_version":1,"action":"accept","decision_id":id,"revision":revision,
            "params_sha256":sha256_hex(params_json.as_bytes()),"severity":severity,
            "applies_from_version":version,"actor":actor,"because":because,
        });
        drop(ledger);
        self.bridge_record(PersonActKind::Accept, &act_id, id, act);
        let ledger = Ledger::open(&self.db).expect("ledger");
        storage::write_derived_file(&ledger, "project-pr", &self.root)
            .expect("bridge derived copy");
    }

    // Synthetic local record for this disposable test only. Production has no
    // injected attester; it requires the OS callback and a typed TTY answer.
    fn bridge_record(&self, action: PersonActKind, act_id: &str, decision_id: &str, act: Value) {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        let mut ledger = Ledger::open(&self.db).expect("ledger");
        let nonce = Ulid::generate().to_string();
        let session_id = format!("decision-terminal-{}", Ulid::generate());
        let dir = self._temp.path().join("decision-confirmations");
        thruline_core::private_store::create_private_dir(&dir).expect("private raw dir");
        let raw_file = dir.join(format!("{session_id}.jsonl"));
        let uid = self.db.metadata().expect("ledger metadata").uid();
        let tuple = serde_json::json!({"thruline_decision_act":act});
        let raw = serde_json::json!({
            "format":"thruline_terminal_confirmation_v1","project_id":"project-pr",
            "session_id":session_id,"nonce":nonce,"action":action.as_str(),
            "act_row_id":act_id,"account_uid":uid,"account_name":"fixture",
            "confirmation":tuple,
        });
        let bytes = serde_json::to_vec(&raw).expect("raw bytes");
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&raw_file)
            .expect("raw file");
        output.write_all(&bytes).expect("raw write");
        output.write_all(b"\n").expect("raw newline");
        output.sync_all().expect("raw sync");
        let (lines, digest) = thruline_core::ingest::whole_file_digest(&raw_file).expect("digest");
        ledger
            .conn()
            .execute(
                "INSERT INTO sessions(id,project_id,file_path,started_at,raw_line_count,raw_digest)
             VALUES(?1,'project-pr',?2,'2026-10-07T00:02:00Z',?3,?4)",
                rusqlite::params![session_id, raw_file.to_str(), lines as i64, digest],
            )
            .expect("bridge session");
        let mut event = NormalizedEvent::new(EventKind::UserMsg);
        event.role = Some("user".into());
        event.text = Some(tuple.to_string());
        event.ts = Some("2026-10-07T00:02:00Z".into());
        event.line_session_id = Some(session_id.clone());
        let raw_sha = sha256_hex(&bytes);
        let placed = ledger
            .append_session_events(
                "project-pr",
                &session_id,
                &[(
                    RawLineRef {
                        file: raw_file.to_string_lossy().into_owned(),
                        line: 1,
                        sha256: raw_sha.clone(),
                    },
                    event,
                )],
            )
            .expect("bridge event");
        let event_id = &placed[0].id;
        bridge::install_schema_for_write(&ledger).expect("bridge schema");
        let receipt_id = Ulid::generate().to_string();
        let tuple_sha = sha256_hex(&serde_json::to_vec(&tuple).expect("tuple bytes"));
        ledger
            .conn()
            .execute(
                "INSERT INTO decision_person_acts(id,action,act_row_id,decision_id,project_id,
             event_id,session_id,raw_file,raw_line,raw_sha256,chain_hash,tuple_sha256)
             VALUES(?1,?2,?3,?4,'project-pr',?5,?6,?7,1,?8,?9,?10)",
                rusqlite::params![
                    receipt_id,
                    action.as_str(),
                    act_id,
                    decision_id,
                    event_id,
                    session_id,
                    raw_file.to_str(),
                    raw_sha,
                    placed[0].chain_hash,
                    tuple_sha
                ],
            )
            .expect("bridge act");
        ledger
            .conn()
            .execute(
                "INSERT INTO decision_bridge_proofs(event_id,receipt_id,project_id,session_id,
             action,act_row_id,nonce,tuple_sha256,account_uid,account_name,raw_sha256,
             confirmation_method,created_at)
             VALUES(?1,?2,'project-pr',?3,?4,?5,?6,?7,?8,'fixture',?9,
                    'macos_local_authentication_tty','2026-10-07T00:02:00Z')",
                rusqlite::params![
                    event_id,
                    receipt_id,
                    session_id,
                    action.as_str(),
                    act_id,
                    nonce,
                    tuple_sha,
                    uid,
                    raw_sha
                ],
            )
            .expect("bridge proof");
        assert!(bridge::verified(&ledger, action, act_id).expect("bridge verify"));
    }

    fn add_guard_decision(&self) -> String {
        let ledger = Ledger::open(&self.db).expect("ledger");
        let span = "Keep guard.txt";
        let id = storage::create(
            &ledger,
            NewDecision {
                project_id: "project-pr".into(),
                source_anchor: "person-rule:2".into(),
                span: span.into(),
                sources: vec![DecisionSource {
                    source_kind: SourceKind::RuleFile,
                    event_id: None,
                    file_anchor: Some("person-rule:2".into()),
                    raw_file: "AGENTS.md".into(),
                    raw_line: 2,
                    span_start: 0,
                    span_end: span.len() as u64,
                    span: span.into(),
                    actor: Some("person".into()),
                    recorded_at: Some("2026-10-07T00:00:00Z".into()),
                }],
                author_actor: "person".into(),
                recorded_at: "2026-10-07T00:00:00Z".into(),
                params: CheckParams::PathPresent(PathPresentParams {
                    path: "guard.txt".into(),
                    sha256: None,
                }),
            },
        )
        .expect("guard proposal");
        storage::accept(
            &ledger,
            AcceptanceInput {
                decision_id: id.clone(),
                revision: 1,
                actor: "person".into(),
                at: "2026-10-07T00:01:00Z".into(),
                because: "standing rule".into(),
                severity: "blocks".into(),
                applies_from_version: self.base.clone(),
            },
        )
        .expect("guard acceptance");
        storage::write_derived_file(&ledger, "project-pr", &self.root).expect("derived copy");
        id
    }

    fn retire_first(&mut self, verified: bool) {
        let ledger = Ledger::open(&self.db).expect("ledger");
        let act_id = storage::retire(
            &ledger,
            RetirementInput {
                decision_id: self.id.clone(),
                actor: "person".into(),
                at: "2026-10-07T00:04:00Z".into(),
                because: "Changed course".into(),
                superseded_by: None,
            },
        )
        .expect("retirement");
        if verified {
            let decision_id = self.id.clone();
            self.bridge_record(
                PersonActKind::Retire,
                &act_id,
                &decision_id,
                serde_json::json!({
                    "schema_version":1,"action":"retire","decision_id":decision_id,
                    "actor":"person","because":"Changed course","superseded_by":null,
                }),
            );
        }
        storage::write_derived_file(&ledger, "project-pr", &self.root)
            .expect("retired derived copy");
    }

    fn waive_first(&mut self, verified: bool) {
        let ledger = Ledger::open(&self.db).expect("ledger");
        let version = git(&self.root, &["rev-parse", "HEAD"]);
        let act_id = storage::waive(
            &ledger,
            WaiverInput {
                decision_id: self.id.clone(),
                version_id: version.clone(),
                actor: "person".into(),
                at: "2026-10-07T00:04:00Z".into(),
                because: "Named exception".into(),
                expires_version: None,
                expires_at: None,
            },
        )
        .expect("waiver");
        if verified {
            let decision_id = self.id.clone();
            self.bridge_record(
                PersonActKind::Waive,
                &act_id,
                &decision_id,
                serde_json::json!({
                    "schema_version":1,"action":"waive","decision_id":decision_id,
                    "version_id":version,"actor":"person","because":"Named exception",
                    "expires_version":null,"expires_at":null,
                }),
            );
        }
    }

    fn command(&self, args: &[&str]) -> Output {
        Command::new(BIN)
            .arg("--db")
            .arg(&self.db)
            .arg("decisions")
            .args(args)
            .arg("--project")
            .arg("project-pr")
            .arg("--root")
            .arg(&self.root)
            .arg("--json")
            .output()
            .expect("decisions command")
    }

    fn receipt_path(&self) -> PathBuf {
        self.root.join(".thruline/decisions-receipt.json")
    }

    fn check_receipt(&self) -> Value {
        let path = self.receipt_path();
        let output = self.command(&[
            "check",
            "--all",
            "--receipt-out",
            path.to_str().expect("path text"),
        ]);
        assert_eq!(
            output.status.code(),
            Some(3),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&std::fs::read(path).expect("exact receipt")).expect("receipt JSON")
    }

    fn check_hold_receipt(&self) {
        let path = self.receipt_path();
        let output = self.command(&[
            "check",
            "--all",
            "--receipt-out",
            path.to_str().expect("receipt path"),
        ]);
        if output.status.code() == Some(0) {
            let receipt: Value =
                serde_json::from_slice(&std::fs::read(path).expect("hold receipt"))
                    .expect("receipt JSON");
            assert!(receipt["verdicts"]
                .as_array()
                .is_some_and(|rows| rows.iter().all(|row| row["state"] == "holds")));
        }
    }

    fn break_head_and_weaken_policy(&self) {
        std::fs::remove_file(self.root.join("kept.txt")).expect("remove required file");
        std::fs::write(
            self.root.join(".thruline/policy.yml"),
            "version: 1\ndecisions:\n  require_hold: none\n",
        )
        .expect("head policy");
        git(&self.root, &["add", "-A"]);
        git(
            &self.root,
            &[
                "-c",
                "user.name=Agent",
                "-c",
                "user.email=agent@example.invalid",
                "commit",
                "-qm",
                "weaken head",
            ],
        );
    }
}

#[test]
fn pr_gate_uses_base_policy_and_exact_broken_receipt() {
    let fixture = Fixture::new();
    fixture.break_head_and_weaken_policy();
    let receipt = fixture.check_receipt();
    assert_eq!(receipt["schema_version"], 1);
    assert_eq!(receipt["verdicts"][0]["decision_id"], fixture.id);
    assert_eq!(receipt["verdicts"][0]["state"], "broken");
    let base = fixture.command(&["gate", "--base", &fixture.base]);
    assert_eq!(
        base.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&base.stderr)
    );
    let body: Value = serde_json::from_slice(&base.stdout).expect("gate JSON");
    assert_eq!(body["shape"], "thruline.decide.gate");
    assert_eq!(body["conclusion"], "failure");
    assert_eq!(body["reasons"][0]["rule"], "decisions.require_hold");
    let head = fixture.command(&["gate"]);
    assert_eq!(
        head.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&head.stderr)
    );
}

#[test]
fn forged_holds_and_unresolvable_base_never_pass() {
    let fixture = Fixture::new();
    fixture.break_head_and_weaken_policy();
    let mut receipt = fixture.check_receipt();
    receipt["verdicts"][0]["state"] = Value::String("holds".into());
    std::fs::write(fixture.receipt_path(), receipt.to_string()).expect("tampered receipt");
    let forged = fixture.command(&["gate", "--base", &fixture.base]);
    assert_eq!(
        forged.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&forged.stderr)
    );
    let missing_base = fixture.command(&["gate", "--base", "refs/heads/not-present"]);
    assert_ne!(missing_base.status.code(), Some(0));
}

#[test]
fn pr_gate_stays_inactive_before_bridge_acceptance() {
    let fixture = Fixture::new_unverified();
    fixture.check_hold_receipt();
    let gate = fixture.command(&["gate", "--base", &fixture.base]);
    assert_eq!(
        gate.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&gate.stderr)
    );
    let body: Value = serde_json::from_slice(&gate.stdout).expect("pending gate JSON");
    assert!(body["reason"]
        .as_str()
        .is_some_and(|reason| reason.contains("bridge is pending")));
}

#[test]
fn pr_gate_ignores_historical_chat_only_receipt() {
    let mut fixture = Fixture::new_unverified();
    let id = fixture.id.clone();
    fixture.verify_legacy_acceptance(&id);
    let gate = fixture.command(&["gate", "--base", &fixture.base]);
    assert_eq!(
        gate.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&gate.stderr)
    );
    let body: Value = serde_json::from_slice(&gate.stdout).expect("legacy gate JSON");
    assert!(body["reason"]
        .as_str()
        .is_some_and(|reason| reason.contains("bridge is pending")));
}

#[test]
fn pending_revision_cannot_switch_off_a_confirmed_pr_gate() {
    let fixture = Fixture::new();
    let ledger = Ledger::open(&fixture.db).expect("ledger");
    let span = "Keep kept.txt after proposed amendment";
    let revision = storage::append_revision(
        &ledger,
        RevisionInput {
            decision_id: fixture.id.clone(),
            span: span.into(),
            sources: vec![DecisionSource {
                source_kind: SourceKind::RuleFile,
                event_id: None,
                file_anchor: Some("person-rule:pending".into()),
                raw_file: "AGENTS.md".into(),
                raw_line: 2,
                span_start: 0,
                span_end: span.len() as u64,
                span: span.into(),
                actor: Some("person".into()),
                recorded_at: Some("2026-10-07T00:03:00Z".into()),
            }],
            author_actor: "person".into(),
            recorded_at: "2026-10-07T00:03:00Z".into(),
            params: CheckParams::PathPresent(PathPresentParams {
                path: "kept.txt".into(),
                sha256: None,
            }),
        },
    )
    .expect("proposed revision");
    storage::accept(
        &ledger,
        AcceptanceInput {
            decision_id: fixture.id.clone(),
            revision,
            actor: "person".into(),
            at: "2026-10-07T00:04:00Z".into(),
            because: "Proposed amendment".into(),
            severity: "blocks".into(),
            applies_from_version: fixture.base.clone(),
        },
    )
    .expect("pending acceptance");
    storage::write_derived_file(&ledger, "project-pr", &fixture.root)
        .expect("confirmed predecessor remains derived");
    assert_eq!(
        storage::verified_acceptance_for_decision(&ledger, &fixture.id)
            .expect("active selection")
            .expect("confirmed predecessor")
            .revision,
        1
    );
    fixture.check_hold_receipt();
    let gate = fixture.command(&["gate", "--base", &fixture.base]);
    assert_eq!(
        gate.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&gate.stderr)
    );
    let body: Value = serde_json::from_slice(&gate.stdout).expect("gate result");
    assert!(body["reason"]
        .as_str()
        .is_none_or(|reason| !reason.contains("pending")));
}

#[test]
fn pr_gate_allows_exact_recorded_acceptance_and_hold() {
    let mut fixture = Fixture::new();
    let id = fixture.id.clone();
    fixture.verify_acceptance(&id);
    fixture.check_hold_receipt();
    let gate = fixture.command(&["gate", "--base", &fixture.base]);
    assert_eq!(
        gate.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&gate.stderr)
    );
}

#[test]
fn pr_gate_refuses_unverified_waiver_of_verified_broken_rule() {
    let mut fixture = Fixture::new();
    let id = fixture.id.clone();
    fixture.verify_acceptance(&id);
    fixture.break_head_and_weaken_policy();
    fixture.check_receipt();
    fixture.waive_first(false);
    let gate = fixture.command(&["gate", "--base", &fixture.base]);
    assert_eq!(
        gate.status.code(),
        Some(3),
        "unverified waiver authorized PR gate: {}",
        String::from_utf8_lossy(&gate.stdout)
    );
}

#[test]
fn pr_gate_allows_exact_recorded_waiver_of_verified_broken_rule() {
    let mut fixture = Fixture::new();
    let id = fixture.id.clone();
    fixture.verify_acceptance(&id);
    fixture.break_head_and_weaken_policy();
    fixture.check_receipt();
    fixture.waive_first(true);
    let gate = fixture.command(&["gate", "--base", &fixture.base]);
    assert_eq!(
        gate.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&gate.stderr)
    );
}

#[test]
fn pr_gate_refuses_unverified_retirement_of_verified_broken_rule() {
    let mut fixture = Fixture::new();
    let id = fixture.id.clone();
    fixture.verify_acceptance(&id);
    let guard = fixture.add_guard_decision();
    fixture.verify_acceptance(&guard);
    fixture.break_head_and_weaken_policy();
    fixture.retire_first(false);
    fixture.check_hold_receipt();
    let gate = fixture.command(&["gate", "--base", &fixture.base]);
    assert_eq!(
        gate.status.code(),
        Some(3),
        "unverified retirement authorized PR gate: {}",
        String::from_utf8_lossy(&gate.stdout)
    );
}

#[test]
fn pr_gate_allows_exact_recorded_retirement_with_remaining_hold() {
    let mut fixture = Fixture::new();
    let id = fixture.id.clone();
    fixture.verify_acceptance(&id);
    let guard = fixture.add_guard_decision();
    fixture.verify_acceptance(&guard);
    fixture.break_head_and_weaken_policy();
    fixture.retire_first(true);
    fixture.check_hold_receipt();
    let gate = fixture.command(&["gate", "--base", &fixture.base]);
    assert_eq!(
        gate.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&gate.stderr)
    );
}
