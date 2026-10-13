//! Synthetic recorded user confirmations for isolated Decisions integration tests.
#![allow(dead_code)]

use std::io::Write;
use std::path::{Path, PathBuf};

use rusqlite::params;
use serde_json::{json, Value};
use thruline_core::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use thruline_core::decisions::authority::{self, bridge, PersonActKind};
use thruline_core::ledger::Ledger;
use thruline_core::model::{EventKind, NormalizedEvent, RawLineRef};

pub struct PersonRecorder {
    _temp: tempfile::TempDir,
    raw_file: PathBuf,
    project_id: String,
    session_id: String,
    source: String,
    next_seq: i64,
    prev_hash: String,
    bridge_file: Option<PathBuf>,
}

impl PersonRecorder {
    pub fn raw_file(&self) -> &Path {
        self.bridge_file.as_deref().unwrap_or(&self.raw_file)
    }

    pub fn new(ledger: &Ledger, project_id: &str) -> Self {
        Self::new_with_legacy_session(ledger, project_id, true)
    }

    pub fn bridge_only(ledger: &Ledger, project_id: &str) -> Self {
        Self::new_with_legacy_session(ledger, project_id, false)
    }

    fn new_with_legacy_session(ledger: &Ledger, project_id: &str, legacy: bool) -> Self {
        let source: String = ledger
            .conn()
            .query_row(
                "SELECT source FROM projects WHERE id=?1",
                [project_id],
                |row| row.get(0),
            )
            .expect("project source");
        assert!(matches!(source.as_str(), "codex" | "claude_code"));
        let temp = tempfile::tempdir().expect("person source directory");
        let raw_file = temp.path().join("person-session.jsonl");
        let session_id = format!("person-test-{}", ulid::Ulid::generate());
        let first_line = if source == "codex" {
            format!(
                "{}\n",
                json!({"type":"session_meta","payload":{"id":session_id,"source":"cli"}})
            )
        } else {
            String::new()
        };
        if legacy {
            std::fs::write(&raw_file, first_line).expect("person source file");
            ledger
                .conn()
                .execute(
                    "INSERT INTO sessions(id,project_id,file_path,started_at)
             VALUES(?1,?2,?3,'2026-10-07T00:00:00Z')",
                    params![session_id, project_id, raw_file.to_str()],
                )
                .expect("person session");
        }
        Self {
            _temp: temp,
            raw_file,
            project_id: project_id.into(),
            session_id,
            source,
            next_seq: 1,
            prev_hash: genesis_prev_hash(project_id),
            bridge_file: None,
        }
    }

    pub fn append(&mut self, ledger: &Ledger, action: Value) -> String {
        let text = json!({"thruline_decision_act":action}).to_string();
        self.append_text(ledger, &text)
    }

    pub fn append_text(&mut self, ledger: &Ledger, text: &str) -> String {
        let raw = match self.source.as_str() {
            "codex" => json!({"type":"response_item","payload":{"type":"message",
                "id":format!("person-item-{}",self.next_seq),"role":"user",
                "content":[{"type":"input_text","text":text}]}})
            .to_string(),
            "claude_code" => json!({"type":"user","sessionId":self.session_id,
                "message":{"role":"user","content":text}})
            .to_string(),
            _ => unreachable!(),
        };
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&self.raw_file)
            .expect("append person source");
        writeln!(file, "{raw}").expect("person source line");
        let raw_sha = sha256_hex(raw.as_bytes());
        let next_hash = chain_hash(&self.prev_hash, &raw_sha, &self.session_id, self.next_seq);
        let event_id = format!("{}-event-{}", self.session_id, self.next_seq);
        let raw_line = self.next_seq + i64::from(self.source == "codex");
        let line_session_id = (self.source == "codex").then_some(self.session_id.as_str());
        ledger
            .conn()
            .execute(
                "INSERT INTO events(id,project_id,session_id,seq,ts,kind,role,text,is_sidechain,
                raw_file,raw_line,raw_sha256,prev_hash,chain_hash,line_session_id)
             VALUES(?1,?2,?3,?4,'2026-10-07T00:02:00Z','user_msg','user',?5,0,
                ?6,?7,?8,?9,?10,?11)",
                params![
                    event_id,
                    self.project_id,
                    self.session_id,
                    self.next_seq,
                    text,
                    self.raw_file.to_str(),
                    raw_line,
                    raw_sha,
                    self.prev_hash,
                    next_hash,
                    line_session_id
                ],
            )
            .expect("person event");
        self.next_seq += 1;
        self.prev_hash = next_hash;
        event_id
    }

    pub fn bind_legacy_acceptance(&mut self, ledger: &Ledger, act_id: &str) -> String {
        let (decision_id, revision, params_json, severity, version, actor, because): (
            String,
            u32,
            String,
            String,
            String,
            String,
            String,
        ) = ledger
            .conn()
            .query_row(
                "SELECT a.decision_id,a.rev,r.params_json,a.severity,a.applies_from_version,
                    a.actor,a.rationale
             FROM decision_acceptances a JOIN decision_revisions r
               ON r.decision_id=a.decision_id AND r.rev=a.rev WHERE a.id=?1",
                [act_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .expect("acceptance tuple");
        let event_id = self.append(
            ledger,
            json!({
                "schema_version":1,"action":"accept","decision_id":decision_id,
                "revision":revision,"params_sha256":sha256_hex(params_json.as_bytes()),
                "severity":severity,"applies_from_version":version,
                "actor":actor,"because":because
            }),
        );
        authority::record(ledger, PersonActKind::Accept, act_id, &event_id)
            .expect("acceptance receipt");
        event_id
    }

    pub fn bind_legacy_retirement(&mut self, ledger: &Ledger, act_id: &str) -> String {
        let (decision_id, actor, because, superseded_by):
            (String, String, String, Option<String>) = ledger.conn().query_row(
            "SELECT decision_id,actor,reason,superseded_by FROM decision_retirements WHERE id=?1",
            [act_id],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
        ).expect("retirement tuple");
        let event_id = self.append(
            ledger,
            json!({
                "schema_version":1,"action":"retire","decision_id":decision_id,
                "actor":actor,"because":because,"superseded_by":superseded_by
            }),
        );
        authority::record(ledger, PersonActKind::Retire, act_id, &event_id)
            .expect("retirement receipt");
        event_id
    }

    pub fn bind_legacy_waiver(&mut self, ledger: &Ledger, act_id: &str) -> String {
        let (decision_id, version_id, actor, because, expires_version, expires_at): (
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
        ) = ledger
            .conn()
            .query_row(
                "SELECT decision_id,version_id,actor,reason,expires_version,expires_at
             FROM decision_waivers WHERE id=?1",
                [act_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .expect("waiver tuple");
        let event_id = self.append(
            ledger,
            json!({
                "schema_version":1,"action":"waive","decision_id":decision_id,
                "version_id":version_id,"actor":actor,"because":because,
                "expires_version":expires_version,"expires_at":expires_at
            }),
        );
        authority::record(ledger, PersonActKind::Waive, act_id, &event_id).expect("waiver receipt");
        event_id
    }

    // Active test fixtures use an isolated synthetic terminal-format receipt.
    // The legacy helpers above deliberately remain available only by their
    // explicit names for tests of pending historical chat provenance.
    pub fn bind_acceptance(&mut self, ledger: &Ledger, act_id: &str) -> String {
        self.bind_bridge_acceptance(ledger, act_id)
    }

    pub fn bind_retirement(&mut self, ledger: &Ledger, act_id: &str) -> String {
        self.bind_bridge_retirement(ledger, act_id)
    }

    pub fn bind_waiver(&mut self, ledger: &Ledger, act_id: &str) -> String {
        self.bind_bridge_waiver(ledger, act_id)
    }

    pub fn bind_bridge_acceptance(&mut self, ledger: &Ledger, act_id: &str) -> String {
        let (decision_id, revision, params_json, severity, version, actor, because): (
            String,
            u32,
            String,
            String,
            String,
            String,
            String,
        ) = ledger
            .conn()
            .query_row(
                "SELECT a.decision_id,a.rev,r.params_json,a.severity,a.applies_from_version,
                    a.actor,a.rationale FROM decision_acceptances a
             JOIN decision_revisions r ON r.decision_id=a.decision_id AND r.rev=a.rev
             WHERE a.id=?1",
                [act_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .expect("acceptance tuple");
        self.record_bridge(
            ledger,
            PersonActKind::Accept,
            act_id,
            &decision_id,
            json!({
                "schema_version":1,"action":"accept","decision_id":decision_id,
                "revision":revision,"params_sha256":sha256_hex(params_json.as_bytes()),
                "severity":severity,"applies_from_version":version,"actor":actor,"because":because,
            }),
        )
    }

    pub fn bind_bridge_retirement(&mut self, ledger: &Ledger, act_id: &str) -> String {
        let (decision_id, actor, because, superseded_by):
            (String, String, String, Option<String>) = ledger.conn().query_row(
            "SELECT decision_id,actor,reason,superseded_by FROM decision_retirements WHERE id=?1",
            [act_id],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
        ).expect("retirement tuple");
        self.record_bridge(
            ledger,
            PersonActKind::Retire,
            act_id,
            &decision_id,
            json!({
                "schema_version":1,"action":"retire","decision_id":decision_id,
                "actor":actor,"because":because,"superseded_by":superseded_by,
            }),
        )
    }

    pub fn bind_bridge_waiver(&mut self, ledger: &Ledger, act_id: &str) -> String {
        let (decision_id, version_id, actor, because, expires_version, expires_at): (
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
        ) = ledger
            .conn()
            .query_row(
                "SELECT decision_id,version_id,actor,reason,expires_version,expires_at
             FROM decision_waivers WHERE id=?1",
                [act_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .expect("waiver tuple");
        self.record_bridge(
            ledger,
            PersonActKind::Waive,
            act_id,
            &decision_id,
            json!({
                "schema_version":1,"action":"waive","decision_id":decision_id,
                "version_id":version_id,"actor":actor,"because":because,
                "expires_version":expires_version,"expires_at":expires_at,
            }),
        )
    }

    // Synthetic fixture only. The production constructor has no injectable
    // attester and requires macOS user presence plus a typed TTY challenge.
    fn record_bridge(
        &mut self,
        ledger: &Ledger,
        action: PersonActKind,
        act_id: &str,
        decision_id: &str,
        act: Value,
    ) -> String {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

        let db: String = ledger
            .conn()
            .query_row(
                "SELECT file FROM pragma_database_list WHERE name='main'",
                [],
                |row| row.get(0),
            )
            .expect("file-backed ledger");
        let db = PathBuf::from(db);
        let uid = std::fs::metadata(&db).expect("ledger metadata").uid();
        let dir = db
            .parent()
            .expect("ledger directory")
            .join("decision-confirmations");
        thruline_core::private_store::create_private_dir(&dir).expect("private bridge directory");
        let session_id = format!("decision-terminal-{}", ulid::Ulid::generate());
        let nonce = ulid::Ulid::generate().to_string();
        let raw_file = dir.join(format!("{session_id}.jsonl"));
        let tuple = json!({"thruline_decision_act":act});
        let raw = json!({
            "format":"thruline_terminal_confirmation_v1",
            "project_id":self.project_id,"session_id":session_id,"nonce":nonce,
            "action":action.as_str(),"act_row_id":act_id,"account_uid":uid,
            "account_name":"fixture","confirmation":tuple,
        });
        let bytes = serde_json::to_vec(&raw).expect("bridge bytes");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&raw_file)
            .expect("bridge raw file");
        file.write_all(&bytes).expect("bridge raw write");
        file.write_all(b"\n").expect("bridge newline");
        file.sync_all().expect("bridge sync");
        let (line_count, digest) =
            thruline_core::ingest::whole_file_digest(&raw_file).expect("bridge raw digest");
        ledger
            .conn()
            .execute(
                "INSERT INTO sessions(id,project_id,file_path,started_at,raw_line_count,raw_digest)
             VALUES(?1,?2,?3,'2026-10-07T00:02:00Z',?4,?5)",
                params![
                    session_id,
                    self.project_id,
                    raw_file.to_str(),
                    line_count as i64,
                    digest
                ],
            )
            .expect("bridge session");
        let mut event = NormalizedEvent::new(EventKind::UserMsg);
        event.role = Some("user".into());
        event.text = Some(tuple.to_string());
        event.ts = Some("2026-10-07T00:02:00Z".into());
        event.line_session_id = Some(session_id.clone());
        let raw_sha = sha256_hex(&bytes);
        let mut writer = Ledger::open(&db).expect("bridge ledger writer");
        let placed = writer
            .append_session_events(
                &self.project_id,
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
        let event_id = placed[0].id.clone();
        bridge::install_schema_for_write(&writer).expect("bridge schema");
        let receipt_id = ulid::Ulid::generate().to_string();
        let tuple_sha = sha256_hex(&serde_json::to_vec(&tuple).expect("tuple bytes"));
        writer
            .conn()
            .execute(
                "INSERT INTO decision_person_acts(id,action,act_row_id,decision_id,project_id,
                event_id,session_id,raw_file,raw_line,raw_sha256,chain_hash,tuple_sha256)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,1,?9,?10,?11)",
                params![
                    receipt_id,
                    action.as_str(),
                    act_id,
                    decision_id,
                    self.project_id,
                    event_id,
                    session_id,
                    raw_file.to_str(),
                    raw_sha,
                    placed[0].chain_hash,
                    tuple_sha
                ],
            )
            .expect("bridge person act");
        writer
            .conn()
            .execute(
                "INSERT INTO decision_bridge_proofs(event_id,receipt_id,project_id,session_id,
                action,act_row_id,nonce,tuple_sha256,account_uid,account_name,raw_sha256,
                confirmation_method,created_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'fixture',?10,
                    'macos_local_authentication_tty','2026-10-07T00:02:00Z')",
                params![
                    event_id,
                    receipt_id,
                    self.project_id,
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
        assert!(bridge::verified(&writer, action, act_id).expect("bridge verification"));
        self.bridge_file = Some(raw_file);
        event_id
    }
}
