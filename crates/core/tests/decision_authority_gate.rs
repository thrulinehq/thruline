//! A local actor label cannot authorize a standing decision at Approve.

use std::io::Write;
use std::path::PathBuf;

use rusqlite::params;
use serde_json::json;
use thruline_core::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use thruline_core::change_decision::{self, Decide, Outcome};
use thruline_core::decisions::authority::{self, bridge, PersonActKind};
use thruline_core::decisions::engine::{self, CHECK_IMPL_VERSION};
use thruline_core::decisions::params::{
    CheckParams, Surface, TextAbsentParams, VisualMatchParams, VisualScreen,
};
use thruline_core::decisions::storage::{
    self, AcceptanceInput, DecisionSource, NewDecision, RetirementInput, RevisionInput, SourceKind,
    VerdictInput, WaiverInput,
};
use thruline_core::ledger::Ledger;
use thruline_core::model::{EventKind, NormalizedEvent, RawLineRef};
use ulid::Ulid;

const PROJECT: &str = "authority-gate-project";
const CHANGE: &str = "authority-gate-change";
const VERSION: &str = "snap_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const ACCEPT_AT: &str = "2026-10-07T10:10:00Z";
const ACT_AT: &str = "2026-10-07T12:00:00Z";

struct Fixture {
    _temp: tempfile::TempDir,
    ledger: Ledger,
    raw_file: PathBuf,
    decision_id: String,
    acceptance_id: String,
    params_sha256: String,
    next_seq: i64,
    prev_hash: String,
}

impl Fixture {
    fn new(verdict: &str) -> Self {
        let temp = tempfile::tempdir().expect("scratch");
        std::fs::create_dir(temp.path().join(".thruline")).expect("policy directory");
        std::fs::write(
            temp.path().join(".thruline/policy.yml"),
            "version: 1\ndecisions:\n  require_hold: blocking\n  on_unverifiable: block\n",
        )
        .expect("decision policy");
        let raw_file = temp.path().join("person-session.jsonl");
        std::fs::write(
            &raw_file,
            format!(
                "{}\n",
                json!({
                    "type":"session_meta","payload":{"id":"authority-session","source":"cli"}
                })
            ),
        )
        .expect("raw source");
        let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
        let root = temp.path().to_string_lossy().into_owned();
        ledger.conn().execute(
            "INSERT INTO projects(id,root_path,source,created_at) VALUES(?1,?2,'codex','2026-10-07T10:00:00Z')",
            params![PROJECT, root],
        ).expect("project");
        ledger.conn().execute(
            "INSERT INTO sessions(id,project_id,file_path,started_at) VALUES('authority-session',?1,?2,'2026-10-07T10:00:00Z')",
            params![PROJECT, raw_file.to_str()],
        ).expect("session");
        ledger.conn().execute(
            "INSERT INTO changes(id,project_id,opened_at,title,purpose,requester,request_source)
             VALUES(?1,?2,'2026-10-07T10:00:00Z','Gate fixture','Authority','sam','person')",
            params![CHANGE, PROJECT],
        ).expect("change");
        ledger.conn().execute(
            "INSERT INTO change_scope_revisions(change_id,scope_version,proposed_at,criteria_json,
               exclusions_json,allowed_scope_json,constraints_json,check_profile_json,content_sha256)
             VALUES(?1,1,'2026-10-07T10:01:00Z','[\"Keep the rule\"]','[]','[]','[]','[]','deadbeef')",
            [CHANGE],
        ).expect("scope");
        ledger.conn().execute(
            "INSERT INTO change_scope_acceptances(change_id,scope_version,accepted_by,accepted_at,rationale)
             VALUES(?1,1,'sam','2026-10-07T10:02:00Z','Agreed')",
            [CHANGE],
        ).expect("scope acceptance");
        let doc = json!({
            "id": VERSION, "taken_at": "2026-10-07T10:30:00Z", "root": root,
            "git": null,
            "tree": {"staged": [], "unstaged": [], "untracked": [], "deleted": [], "dirty_diff_sha256": null},
            "environment": {"os": "test", "arch": "test", "git_version": null, "env_names": [], "lockfiles": {}},
            "processes": {"ps_sha256": null, "ps_lines": 0, "ports": null, "task_ids": null,
                          "log_cursors": null, "gaps": "none"},
            "complete": true, "note": null
        }).to_string();
        ledger.conn().execute(
            "INSERT INTO workspace_snapshots(id,taken_at,root,head,branch,complete,doc,doc_sha256)
             VALUES(?1,'2026-10-07T10:30:00Z',?2,NULL,NULL,1,?3,?4)",
            params![VERSION, root, doc, sha256_hex(doc.as_bytes())],
        ).expect("snapshot");
        ledger
            .conn()
            .execute(
                "INSERT INTO change_implementations(change_id,binding_version,bound_at,
              repository_identity,base_revision,snapshot_id,workspace_identity)
             VALUES(?1,1,'2026-10-07T10:30:00Z','fixture','base',?2,NULL)",
                params![CHANGE, VERSION],
            )
            .expect("implementation");
        let span = "No obsolete copy anywhere";
        let decision_id = storage::create(
            &ledger,
            NewDecision {
                project_id: PROJECT.into(),
                source_anchor: "rule-file:1".into(),
                span: span.into(),
                sources: vec![DecisionSource {
                    source_kind: SourceKind::RuleFile,
                    event_id: None,
                    file_anchor: Some("rule-file:1".into()),
                    raw_file: "AGENTS.md".into(),
                    raw_line: 1,
                    span_start: 0,
                    span_end: span.len() as u64,
                    span: span.into(),
                    actor: Some("sam".into()),
                    recorded_at: Some("2026-10-07T10:00:00Z".into()),
                }],
                author_actor: "sam".into(),
                recorded_at: "2026-10-07T10:00:00Z".into(),
                params: CheckParams::TextAbsent(TextAbsentParams {
                    test_allowlist: Vec::new(),
                    json_field_allowlist: Vec::new(),
                    conditional_path_allowlist: Vec::new(),
                    span_allowlist: Vec::new(),
                    terms: vec!["obsolete".into()],
                    synonyms: vec![],
                    surfaces: vec![Surface::UiCopy],
                    allowlist: vec![],
                }),
            },
        )
        .expect("proposal");
        let acceptance_id = storage::accept(
            &ledger,
            AcceptanceInput {
                decision_id: decision_id.clone(),
                revision: 1,
                actor: "sam".into(),
                at: ACCEPT_AT.into(),
                because: "Standing rule".into(),
                severity: "blocks".into(),
                applies_from_version: VERSION.into(),
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
        let historical = VerdictInput {
            decision_id: decision_id.clone(),
            revision: 1,
            version_id: VERSION.into(),
            tree_hash: "fixture-tree".into(),
            verdict: verdict.into(),
            reason: None,
            checkable_if: None,
            findings: json!([]),
            surfaces: json!([]),
            check_impl_version: CHECK_IMPL_VERSION.into(),
            tool_versions: json!({}),
            computed_at: "2026-10-07T11:00:00Z".into(),
            basis: json!({"tree_hash":"fixture-tree"}),
        };
        // This preexisting row represents a verdict written before authority
        // receipts were enforced. It must not activate an unverified act.
        ledger
            .conn()
            .execute(
                "INSERT INTO decision_verdicts(id,decision_id,rev,version_id,tree_hash,verdict,
                 reason,checkable_if,findings_json,surfaces_json,check_impl_version,
                 tool_versions_json,computed_at,basis_json)
             VALUES('historical-verdict',?1,?2,?3,?4,?5,NULL,NULL,?6,?7,?8,?9,?10,?11)",
                params![
                    historical.decision_id,
                    historical.revision,
                    historical.version_id,
                    historical.tree_hash,
                    historical.verdict,
                    historical.findings.to_string(),
                    historical.surfaces.to_string(),
                    historical.check_impl_version,
                    historical.tool_versions.to_string(),
                    historical.computed_at,
                    historical.basis.to_string()
                ],
            )
            .expect("historical verdict");
        Self {
            _temp: temp,
            ledger,
            raw_file,
            decision_id,
            acceptance_id,
            params_sha256: sha256_hex(params_json.as_bytes()),
            next_seq: 1,
            prev_hash: genesis_prev_hash(PROJECT),
        }
    }

    fn append_person_confirmation(&mut self, action: serde_json::Value) -> String {
        let text = json!({"thruline_decision_act":action}).to_string();
        let raw = json!({"type":"response_item","payload":{"type":"message",
            "id":format!("person-item-{}",self.next_seq),"role":"user",
            "content":[{"type":"input_text","text":text}]}})
        .to_string();
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&self.raw_file)
            .expect("raw append");
        writeln!(file, "{raw}").expect("write raw line");
        let event_id = format!("authority-gate-event-{}", self.next_seq);
        let raw_sha = sha256_hex(raw.as_bytes());
        let next_hash = chain_hash(
            &self.prev_hash,
            &raw_sha,
            "authority-session",
            self.next_seq,
        );
        self.ledger.conn().execute(
            "INSERT INTO events(id,project_id,session_id,seq,ts,kind,role,text,is_sidechain,
             raw_file,raw_line,raw_sha256,prev_hash,chain_hash,line_session_id)
             VALUES(?1,?2,'authority-session',?3,'2026-10-07T11:15:00Z','user_msg','user',?4,0,?5,?6,?7,?8,?9,'authority-session')",
            params![event_id,PROJECT,self.next_seq,text,self.raw_file.to_str(),self.next_seq+1,
                raw_sha,self.prev_hash,next_hash],
        ).expect("event");
        self.next_seq += 1;
        self.prev_hash = next_hash;
        event_id
    }

    fn verify_legacy_acceptance(&mut self) {
        let event = self.append_person_confirmation(json!({
            "schema_version":1,"action":"accept","decision_id":self.decision_id,
            "revision":1,"params_sha256":self.params_sha256,"severity":"blocks",
            "applies_from_version":VERSION,"actor":"sam","because":"Standing rule"
        }));
        authority::record(
            &self.ledger,
            PersonActKind::Accept,
            &self.acceptance_id,
            &event,
        )
        .expect("authority receipt");
        assert!(
            authority::verified(&self.ledger, PersonActKind::Accept, &self.acceptance_id)
                .expect("verify acceptance")
        );
    }

    fn verify_acceptance(&mut self) {
        self.record_bridge(
            PersonActKind::Accept,
            &self.acceptance_id.clone(),
            json!({
                "schema_version":1,"action":"accept","decision_id":self.decision_id,
                "revision":1,"params_sha256":self.params_sha256,"severity":"blocks",
                "applies_from_version":VERSION,"actor":"sam","because":"Standing rule"
            }),
        );
    }

    // A synthetic on-disk terminal receipt is used only in this disposable
    // fixture. It does not invoke or bypass the production OS-presence door.
    fn record_bridge(&mut self, action: PersonActKind, act_row_id: &str, act: serde_json::Value) {
        self.record_bridge_with_supersession(action, act_row_id, act, None);
    }

    fn record_bridge_with_supersession(
        &mut self,
        action: PersonActKind,
        act_row_id: &str,
        act: serde_json::Value,
        supersession: Option<serde_json::Value>,
    ) {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        let nonce = Ulid::generate().to_string();
        let session_id = format!("decision-terminal-{}", Ulid::generate());
        let dir = self._temp.path().join("decision-confirmations");
        thruline_core::private_store::create_private_dir(&dir).expect("bridge directory");
        let file = dir.join(format!("{session_id}.jsonl"));
        let uid = self
            ._temp
            .path()
            .join("ledger.db")
            .metadata()
            .expect("ledger metadata")
            .uid();
        let mut tuple = json!({"thruline_decision_act":act});
        let bridge_action = if let Some(metadata) = &supersession {
            tuple["thruline_supersession"] = metadata.clone();
            "supersede"
        } else {
            action.as_str()
        };
        let raw = json!({
            "format":"thruline_terminal_confirmation_v1", "project_id":PROJECT,
            "session_id":session_id,"nonce":nonce,"action":bridge_action,
            "act_row_id":act_row_id,"account_uid":uid,"account_name":"fixture",
            "confirmation":tuple,
        });
        let bytes = serde_json::to_vec(&raw).expect("raw bytes");
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&file)
            .expect("bridge raw file");
        output.write_all(&bytes).expect("bridge raw write");
        output.write_all(b"\n").expect("bridge newline");
        output.sync_all().expect("bridge fsync");
        let (lines, digest) = thruline_core::ingest::whole_file_digest(&file).expect("raw digest");
        self.ledger
            .conn()
            .execute(
                "INSERT INTO sessions(id,project_id,file_path,started_at,raw_line_count,raw_digest)
             VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    session_id,
                    PROJECT,
                    file.to_str(),
                    ACT_AT,
                    lines as i64,
                    digest
                ],
            )
            .expect("bridge session");
        let mut event = NormalizedEvent::new(EventKind::UserMsg);
        event.role = Some("user".into());
        event.text = Some(tuple.to_string());
        event.ts = Some(ACT_AT.into());
        event.line_session_id = Some(session_id.clone());
        let raw_sha = sha256_hex(&bytes);
        let placed = self
            .ledger
            .append_session_events(
                PROJECT,
                &session_id,
                &[(
                    RawLineRef {
                        file: file.to_string_lossy().into_owned(),
                        line: 1,
                        sha256: raw_sha.clone(),
                    },
                    event,
                )],
            )
            .expect("bridge event");
        let event_id = &placed[0].id;
        let decision_id = self.decision_id.clone();
        let receipt_id = Ulid::generate().to_string();
        let tuple_sha = sha256_hex(&serde_json::to_vec(&tuple).expect("tuple bytes"));
        bridge::install_schema_for_write(&self.ledger).expect("bridge schema");
        self.ledger
            .conn()
            .execute(
                "INSERT INTO decision_person_acts(id,action,act_row_id,decision_id,project_id,
             event_id,session_id,raw_file,raw_line,raw_sha256,chain_hash,tuple_sha256)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,1,?9,?10,?11)",
                params![
                    receipt_id,
                    action.as_str(),
                    act_row_id,
                    decision_id,
                    PROJECT,
                    event_id,
                    session_id,
                    file.to_str(),
                    raw_sha,
                    placed[0].chain_hash,
                    tuple_sha
                ],
            )
            .expect("bridge person act");
        self.ledger
            .conn()
            .execute(
                "INSERT INTO decision_bridge_proofs(event_id,receipt_id,project_id,session_id,
             action,act_row_id,nonce,tuple_sha256,account_uid,account_name,raw_sha256,
             supersession_json,confirmation_method,created_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'fixture',?10,?11,
                    'macos_local_authentication_tty',?12)",
                params![
                    event_id,
                    receipt_id,
                    PROJECT,
                    session_id,
                    bridge_action,
                    act_row_id,
                    nonce,
                    tuple_sha,
                    uid,
                    raw_sha,
                    supersession.as_ref().map(serde_json::Value::to_string),
                    ACT_AT
                ],
            )
            .expect("bridge proof");
        assert!(bridge::verified(&self.ledger, action, act_row_id).expect("bridge verification"));
    }

    fn retire(&mut self, verified: bool) {
        let act_id = storage::retire(
            &self.ledger,
            RetirementInput {
                decision_id: self.decision_id.clone(),
                actor: "sam".into(),
                at: "2026-10-07T11:30:00Z".into(),
                because: "Changed course".into(),
                superseded_by: None,
            },
        )
        .expect("retirement");
        if verified {
            self.record_bridge(
                PersonActKind::Retire,
                &act_id,
                json!({
                    "schema_version":1,"action":"retire","decision_id":self.decision_id,
                    "actor":"sam","because":"Changed course","superseded_by":null
                }),
            );
        }
    }

    fn waive(&mut self, verified: bool) {
        let act_id = storage::waive(
            &self.ledger,
            WaiverInput {
                decision_id: self.decision_id.clone(),
                version_id: VERSION.into(),
                actor: "sam".into(),
                at: "2026-10-07T11:30:00Z".into(),
                because: "Named exception".into(),
                expires_version: None,
                expires_at: None,
            },
        )
        .expect("waiver");
        if verified {
            self.record_bridge(
                PersonActKind::Waive,
                &act_id,
                json!({
                    "schema_version":1,"action":"waive","decision_id":self.decision_id,
                    "version_id":VERSION,"actor":"sam","because":"Named exception",
                    "expires_version":null,"expires_at":null
                }),
            );
        }
    }

    fn approve(&self) -> thruline_core::error::Result<Outcome> {
        change_decision::approve(
            &self.ledger,
            CHANGE,
            &Decide {
                actor: "sam".into(),
                at: ACT_AT.into(),
                seen_revision: VERSION.into(),
                note: None,
            },
        )
    }

    fn policy(&self, body: &str) {
        let dir = self._temp.path().join(".thruline");
        std::fs::create_dir_all(&dir).expect("policy folder");
        std::fs::write(dir.join("policy.yml"), body).expect("project policy");
    }
}

#[test]
fn unverified_acceptance_keeps_approve_gate_inactive() {
    let fixture = Fixture::new("holds");
    assert!(matches!(
        fixture.approve().expect("bridge pending"),
        Outcome::Recorded { .. }
    ));
}

#[test]
fn recorded_event_without_linked_proof_keeps_approve_gate_inactive() {
    let mut fixture = Fixture::new("broken");
    let _event = fixture.append_person_confirmation(json!({
        "schema_version":1,"action":"accept","decision_id":fixture.decision_id,
        "revision":1,"params_sha256":fixture.params_sha256,"severity":"blocks",
        "applies_from_version":VERSION,"actor":"sam","because":"Standing rule"
    }));
    assert!(!bridge::gates_active(&fixture.ledger, PROJECT).expect("gate readiness"));
    assert!(matches!(
        fixture.approve().expect("incomplete bridge stays inert"),
        Outcome::Recorded { .. }
    ));
}

#[test]
fn forged_bridge_row_cannot_promote_a_chat_confirmation() {
    use std::os::unix::fs::MetadataExt;
    let mut fixture = Fixture::new("broken");
    fixture.verify_legacy_acceptance();
    bridge::install_schema_for_write(&fixture.ledger).expect("bridge schema");
    let (receipt,event,session,raw_sha,tuple_sha): (String,String,String,String,String) = fixture.ledger.conn()
        .query_row("SELECT id,event_id,session_id,raw_sha256,tuple_sha256 FROM decision_person_acts WHERE action='accept' AND act_row_id=?1",
            [&fixture.acceptance_id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)))
        .expect("historical receipt");
    let uid = fixture
        ._temp
        .path()
        .join("ledger.db")
        .metadata()
        .expect("ledger file")
        .uid();
    fixture
        .ledger
        .conn()
        .execute(
            "INSERT INTO decision_bridge_proofs(event_id,receipt_id,project_id,session_id,action,
         act_row_id,nonce,tuple_sha256,account_uid,account_name,raw_sha256,
         confirmation_method,created_at)
         VALUES(?1,?2,?3,?4,'accept',?5,'forged-nonce',?6,?7,'fixture',?8,
                'macos_local_authentication_tty',?9)",
            params![
                event,
                receipt,
                PROJECT,
                session,
                fixture.acceptance_id,
                tuple_sha,
                uid,
                raw_sha,
                ACT_AT
            ],
        )
        .expect("manual forged row");
    assert!(!bridge::verified(
        &fixture.ledger,
        PersonActKind::Accept,
        &fixture.acceptance_id
    )
    .expect("verification"));
    assert!(matches!(
        fixture.approve().expect("forgery cannot activate gate"),
        Outcome::Recorded { .. }
    ));
}

#[test]
fn changed_or_symlinked_terminal_log_is_inert() {
    let mut changed = Fixture::new("broken");
    changed.verify_acceptance();
    let dir = changed._temp.path().join("decision-confirmations");
    let raw = std::fs::read_dir(&dir)
        .expect("raw listing")
        .next()
        .expect("raw file")
        .expect("entry")
        .path();
    std::fs::write(&raw, b"{}\n").expect("tamper raw");
    assert!(!bridge::verified(
        &changed.ledger,
        PersonActKind::Accept,
        &changed.acceptance_id
    )
    .expect("tamper check"));
    assert!(matches!(
        changed.approve().expect("tampered proof stays inert"),
        Outcome::Recorded { .. }
    ));

    let mut linked = Fixture::new("broken");
    linked.verify_acceptance();
    let dir = linked._temp.path().join("decision-confirmations");
    let raw = std::fs::read_dir(&dir)
        .expect("raw listing")
        .next()
        .expect("raw file")
        .expect("entry")
        .path();
    let moved = raw.with_extension("saved");
    std::fs::rename(&raw, &moved).expect("move raw");
    std::os::unix::fs::symlink(&moved, &raw).expect("symlink attack");
    assert!(
        !bridge::verified(&linked.ledger, PersonActKind::Accept, &linked.acceptance_id)
            .expect("symlink check")
    );
}

#[test]
fn one_supersession_receipt_binds_both_visual_revisions_and_references() {
    let mut fixture = Fixture::new("holds");
    let span = "Approve-Sheet matches the accepted reference";
    let source = |anchor: &str| DecisionSource {
        source_kind: SourceKind::RuleFile,
        event_id: None,
        file_anchor: Some(anchor.into()),
        raw_file: "DESIGN.md".into(),
        raw_line: 1,
        span_start: 0,
        span_end: span.len() as u64,
        span: span.into(),
        actor: Some("sam".into()),
        recorded_at: Some(ACCEPT_AT.into()),
    };
    let old_screen = VisualScreen {
        screen: "Approve-Sheet".into(),
        route: "/approve".into(),
        scenario: "design".into(),
        legacy_route: None,
        legacy_scenario: None,
        reference_path: "refs/Approve-Sheet.png".into(),
        reference_sha256: "a".repeat(64),
        viewport_width: 1512,
        viewport_height: 2708,
        full_page: true,
        mask_rectangles: vec![],
    };
    let old_params = CheckParams::VisualMatch(VisualMatchParams {
        screens: vec![old_screen.clone()],
        tolerance_pct: 0.0,
    });
    let visual_id = storage::create(
        &fixture.ledger,
        NewDecision {
            project_id: PROJECT.into(),
            source_anchor: "visual-rule:1".into(),
            span: span.into(),
            sources: vec![source("visual-rule:1")],
            author_actor: "sam".into(),
            recorded_at: ACCEPT_AT.into(),
            params: old_params,
        },
    )
    .expect("old visual revision");
    let mut new_screen = old_screen.clone();
    new_screen.reference_path = "refs/Approve-Sheet.c20.png".into();
    new_screen.reference_sha256 = "b".repeat(64);
    let next = storage::append_revision(
        &fixture.ledger,
        RevisionInput {
            decision_id: visual_id.clone(),
            span: span.into(),
            sources: vec![source("visual-rule:2")],
            author_actor: "sam".into(),
            recorded_at: ACT_AT.into(),
            params: CheckParams::VisualMatch(VisualMatchParams {
                screens: vec![new_screen.clone()],
                tolerance_pct: 0.0,
            }),
        },
    )
    .expect("successor revision");
    assert_eq!(next, 2);
    let acceptance_id = storage::accept(
        &fixture.ledger,
        AcceptanceInput {
            decision_id: visual_id.clone(),
            revision: 2,
            actor: "sam".into(),
            at: ACT_AT.into(),
            because: "C20 replaces the trial-era reference".into(),
            severity: "blocks".into(),
            applies_from_version: VERSION.into(),
        },
    )
    .expect("pending successor acceptance");
    let params = |rev: u32| -> String {
        fixture
            .ledger
            .conn()
            .query_row(
                "SELECT params_json FROM decision_revisions WHERE decision_id=?1 AND rev=?2",
                params![visual_id, rev],
                |row| row.get(0),
            )
            .expect("revision params")
    };
    let previous_params = params(1);
    let successor_params = params(2);
    let metadata = json!({
        "predecessor_decision_id":visual_id,
        "predecessor_revision":1,
        "predecessor_params_sha256":sha256_hex(previous_params.as_bytes()),
        "successor_decision_id":visual_id,
        "successor_revision":2,
        "successor_params_sha256":sha256_hex(successor_params.as_bytes()),
        "screen":"Approve-Sheet",
        "old_reference_path":old_screen.reference_path,
        "old_reference_sha256":old_screen.reference_sha256,
        "new_reference_path":new_screen.reference_path,
        "new_reference_sha256":new_screen.reference_sha256,
    });
    fixture.decision_id = visual_id.clone();
    fixture.record_bridge_with_supersession(
        PersonActKind::Accept,
        &acceptance_id,
        json!({"schema_version":1,"action":"accept","decision_id":visual_id,
            "revision":2,"params_sha256":sha256_hex(successor_params.as_bytes()),
            "severity":"blocks","applies_from_version":VERSION,
            "actor":"sam","because":"C20 replaces the trial-era reference"}),
        Some(metadata),
    );
    assert_eq!(
        storage::verified_acceptance_for_decision(&fixture.ledger, &visual_id)
            .expect("selected successor")
            .expect("accepted successor")
            .revision,
        2
    );
    assert!(
        fixture
            .ledger
            .conn()
            .execute(
                "UPDATE decision_bridge_proofs SET supersession_json='{}' WHERE act_row_id=?1",
                [&acceptance_id],
            )
            .is_err(),
        "supersession proof is append-only"
    );
}

#[test]
fn c28_pending_prebridge_block_does_not_refuse_approve() {
    let fixture = Fixture::new("broken");
    assert!(matches!(
        fixture
            .approve()
            .expect("gate stays off before bridge acceptance"),
        Outcome::Recorded { .. }
    ));
}

#[test]
fn dormant_bridge_does_not_parse_or_enforce_decision_policy() {
    let fixture = Fixture::new("broken");
    fixture.policy("decisions: [\n");
    assert!(matches!(
        fixture
            .approve()
            .expect("dormant bridge ignores decisions policy"),
        Outcome::Recorded { .. }
    ));
}

#[test]
fn c28_recorded_chat_receipt_alone_does_not_activate_block() {
    let mut fixture = Fixture::new("broken");
    fixture.verify_legacy_acceptance();
    assert!(matches!(
        fixture
            .approve()
            .expect("chat receipt is not an interactive bridge act"),
        Outcome::Recorded { .. }
    ));
}

#[test]
fn pending_revisions_cannot_switch_off_an_existing_confirmed_block() {
    for (verdict, severity) in [
        ("broken", "blocks"),
        ("broken", "warns"),
        ("holds", "blocks"),
        ("holds", "warns"),
    ] {
        let mut fixture = Fixture::new(verdict);
        fixture.verify_acceptance();
        let span = "No obsolete copy after an unconfirmed edit";
        let revision = storage::append_revision(
            &fixture.ledger,
            RevisionInput {
                decision_id: fixture.decision_id.clone(),
                span: span.into(),
                sources: vec![DecisionSource {
                    source_kind: SourceKind::RuleFile,
                    event_id: None,
                    file_anchor: Some("rule-file:pending".into()),
                    raw_file: "AGENTS.md".into(),
                    raw_line: 2,
                    span_start: 0,
                    span_end: span.len() as u64,
                    span: span.into(),
                    actor: Some("sam".into()),
                    recorded_at: Some("2026-10-07T11:35:00Z".into()),
                }],
                author_actor: "sam".into(),
                recorded_at: "2026-10-07T11:35:00Z".into(),
                params: CheckParams::TextAbsent(TextAbsentParams {
                    test_allowlist: Vec::new(),
                    json_field_allowlist: Vec::new(),
                    conditional_path_allowlist: Vec::new(),
                    span_allowlist: Vec::new(),
                    terms: vec!["obsolete".into()],
                    synonyms: vec![],
                    surfaces: vec![Surface::UiCopy],
                    allowlist: vec![],
                }),
            },
        )
        .expect("pending revision");
        storage::accept(
            &fixture.ledger,
            AcceptanceInput {
                decision_id: fixture.decision_id.clone(),
                revision,
                actor: "sam".into(),
                at: "2026-10-07T11:40:00Z".into(),
                because: "Proposed amendment".into(),
                severity: severity.into(),
                applies_from_version: VERSION.into(),
            },
        )
        .expect("pending acceptance");
        assert_eq!(
            storage::verified_acceptance_for_decision(&fixture.ledger, &fixture.decision_id)
                .expect("active selection")
                .expect("confirmed predecessor")
                .revision,
            1,
            "pending {severity} revision replaced the confirmed predecessor"
        );
        assert!(
            bridge::gates_active(&fixture.ledger, PROJECT).expect("gate readiness"),
            "pending {severity} revision switched off an established block"
        );
        assert_eq!(
            fixture.approve().is_err(),
            verdict == "broken",
            "pending {severity} revision changed the established {verdict} outcome"
        );
    }
}

#[test]
fn unverified_retirement_cannot_remove_a_verified_block() {
    let mut fixture = Fixture::new("broken");
    fixture.verify_acceptance();
    fixture.retire(false);
    assert!(
        fixture.approve().is_err(),
        "unverified retirement erased a block"
    );
}

#[test]
fn unverified_waiver_cannot_override_a_verified_broken_verdict() {
    let mut fixture = Fixture::new("broken");
    fixture.verify_acceptance();
    fixture.waive(false);
    assert!(
        fixture.approve().is_err(),
        "unverified waiver cleared a block"
    );
}

#[test]
fn exact_person_acceptance_receipt_can_clear_hold() {
    let mut fixture = Fixture::new("holds");
    fixture.verify_acceptance();
    assert!(matches!(
        fixture.approve().expect("Approve"),
        Outcome::Recorded { .. }
    ));
}

#[test]
fn exact_person_retirement_and_waiver_receipts_can_clear_broken() {
    let mut retired = Fixture::new("broken");
    retired.verify_acceptance();
    retired.retire(true);
    assert!(matches!(
        retired.approve().expect("verified retirement"),
        Outcome::Recorded { .. }
    ));

    let mut waived = Fixture::new("broken");
    waived.verify_acceptance();
    waived.waive(true);
    assert!(matches!(
        waived.approve().expect("verified waiver"),
        Outcome::Recorded { .. }
    ));
}

#[test]
fn only_recorded_person_acts_enter_the_active_selection() {
    let mut fixture = Fixture::new("holds");
    assert!(storage::accepted_for_project(&fixture.ledger, PROJECT)
        .expect("pending acceptance")
        .is_empty());
    assert!(engine::check_project_version(
        &fixture.ledger,
        PROJECT,
        fixture._temp.path(),
        VERSION,
        ACT_AT,
        false,
    )
    .expect("pending check selection")
    .is_empty());
    fixture.verify_acceptance();
    assert_eq!(
        storage::accepted_for_project(&fixture.ledger, PROJECT)
            .expect("verified acceptance")
            .len(),
        1
    );
    fixture.retire(false);
    assert_eq!(
        storage::accepted_for_project(&fixture.ledger, PROJECT)
            .expect("unverified retirement")
            .len(),
        1
    );
    fixture.retire(true);
    assert!(storage::accepted_for_project(&fixture.ledger, PROJECT)
        .expect("verified retirement")
        .is_empty());
}

#[test]
fn new_verdict_requires_a_recorded_person_acceptance() {
    let mut fixture = Fixture::new("holds");
    let input = VerdictInput {
        decision_id: fixture.decision_id.clone(),
        revision: 1,
        version_id: VERSION.into(),
        tree_hash: "fixture-tree".into(),
        verdict: "holds".into(),
        reason: None,
        checkable_if: None,
        findings: json!([]),
        surfaces: json!([]),
        check_impl_version: CHECK_IMPL_VERSION.into(),
        tool_versions: json!({}),
        computed_at: ACT_AT.into(),
        basis: json!({"tree_hash":"fixture-tree"}),
    };
    assert!(storage::record_verdict(&fixture.ledger, input.clone()).is_err());
    fixture.verify_acceptance();
    storage::record_verdict(&fixture.ledger, input).expect("verified verdict");
}

#[test]
fn approve_respects_project_on_unverifiable_warn_for_verified_block() {
    let mut warning = Fixture::new("unverifiable");
    warning.verify_acceptance();
    warning.policy("version: 1\ndecisions:\n  on_unverifiable: warn\n");
    assert!(matches!(
        warning
            .approve()
            .expect("policy permits uncertain evidence"),
        Outcome::Recorded { .. }
    ));

    let mut blocking = Fixture::new("unverifiable");
    blocking.verify_acceptance();
    blocking.policy("version: 1\ndecisions:\n  on_unverifiable: block\n");
    assert!(blocking.approve().is_err(), "explicit block must refuse");

    let mut default = Fixture::new("unverifiable");
    default.verify_acceptance();
    assert!(
        default.approve().is_err(),
        "missing policy defaults to block"
    );
}

#[test]
fn pending_retirement_does_not_prevent_accepting_a_later_revision() {
    let mut fixture = Fixture::new("holds");
    fixture.verify_acceptance();
    fixture.retire(false);
    let span = "No obsolete copy in the current UI";
    let revision = storage::append_revision(
        &fixture.ledger,
        RevisionInput {
            decision_id: fixture.decision_id.clone(),
            span: span.into(),
            sources: vec![DecisionSource {
                source_kind: SourceKind::RuleFile,
                event_id: None,
                file_anchor: Some("rule-file:2".into()),
                raw_file: "AGENTS.md".into(),
                raw_line: 2,
                span_start: 0,
                span_end: span.len() as u64,
                span: span.into(),
                actor: Some("sam".into()),
                recorded_at: Some("2026-10-07T11:35:00Z".into()),
            }],
            author_actor: "sam".into(),
            recorded_at: "2026-10-07T11:35:00Z".into(),
            params: CheckParams::TextAbsent(TextAbsentParams {
                test_allowlist: Vec::new(),
                json_field_allowlist: Vec::new(),
                conditional_path_allowlist: Vec::new(),
                span_allowlist: Vec::new(),
                terms: vec!["obsolete".into()],
                synonyms: vec![],
                surfaces: vec![Surface::UiCopy],
                allowlist: vec![],
            }),
        },
    )
    .expect("revision");
    assert_eq!(revision, 2);
    storage::accept(
        &fixture.ledger,
        AcceptanceInput {
            decision_id: fixture.decision_id.clone(),
            revision,
            actor: "sam".into(),
            at: "2026-10-07T11:40:00Z".into(),
            because: "Amended standing rule".into(),
            severity: "blocks".into(),
            applies_from_version: VERSION.into(),
        },
    )
    .expect("pending retirement must remain inert");
}
