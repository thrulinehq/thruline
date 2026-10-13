use crate::{fixture::Fixture, person_action::PersonRecorder};
use rusqlite::params;
use serde_json::{json, Value};
use std::io::Write;
use std::path::PathBuf;
use thruline_core::{
    chain::sha256_hex,
    decisions::authority::{bridge, PersonActKind},
    ledger::Ledger,
    model::{EventKind, NormalizedEvent, RawLineRef},
};

pub fn retirement(f: &Fixture, old: &str, new: &str) -> String {
    thruline_core::decisions::storage::retire(
        &f.ledger,
        thruline_core::decisions::storage::RetirementInput {
            decision_id: old.into(),
            actor: "person".into(),
            at: "2026-10-07T00:04:00Z".into(),
            because: "replace exact predecessor".into(),
            superseded_by: Some(new.into()),
        },
    )
    .expect("pending retirement")
}
pub fn acceptance_metadata(f: &Fixture, id: &str, prefix: &str) -> Value {
    let (act,rev,kind,schema,bytes,version):(String,u32,String,u32,String,String)=f.ledger.conn().query_row(
        "SELECT a.id,a.rev,r.kind,r.params_schema_version,r.params_json,a.applies_from_version FROM decision_acceptances a JOIN decision_revisions r ON r.decision_id=a.decision_id AND r.rev=a.rev WHERE a.decision_id=?1 ORDER BY a.rowid DESC LIMIT 1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).expect("acceptance fields");
    let mut result = serde_json::Map::new();
    for (key, value) in [
        ("decision_id", json!(id)),
        ("acceptance_id", json!(act)),
        ("revision", json!(rev)),
        ("kind", json!(kind)),
        ("params_schema_version", json!(schema)),
        ("params_sha256", json!(sha256_hex(bytes.as_bytes()))),
        ("applies_from_version", json!(version)),
    ] {
        result.insert(format!("{prefix}_{key}"), value);
    }
    Value::Object(result)
}
pub fn metadata(f: &Fixture, old: &str, new: &str) -> Value {
    let mut result =
        json!({"schema_version":1,"kind":"general_decision_supersession","project_id":"p"});
    for prefix in ["predecessor", "successor"] {
        let side = acceptance_metadata(f, if prefix == "predecessor" { old } else { new }, prefix);
        result
            .as_object_mut()
            .expect("object")
            .extend(side.as_object().expect("side").clone());
    }
    result
}
pub fn revise(f: &mut Fixture, id: &str, value: &str, verified: bool) -> String {
    use thruline_core::decisions::storage;
    let (span, sources): (String, String) = f
        .ledger
        .conn()
        .query_row(
            "SELECT text,sources_json FROM decision_revisions WHERE decision_id=?1 AND rev=1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("source");
    let revision = storage::append_revision(
        &f.ledger,
        storage::RevisionInput {
            decision_id: id.into(),
            span,
            sources: serde_json::from_str(&sources).expect("sources"),
            author_actor: "person".into(),
            recorded_at: "2026-10-07T00:06:00Z".into(),
            params: crate::fixture::default(value),
        },
    )
    .expect("revision");
    let act = storage::accept(
        &f.ledger,
        storage::AcceptanceInput {
            decision_id: id.into(),
            revision,
            actor: "person".into(),
            at: "2026-10-07T00:06:00Z".into(),
            because: "exact successor revision".into(),
            severity: "warns".into(),
            applies_from_version: f.version.clone(),
        },
    )
    .expect("pending acceptance");
    if verified {
        let mut person = PersonRecorder::bridge_only(&f.ledger, "p");
        person.bind_acceptance(&f.ledger, &act);
        f.people.push(person);
    }
    act
}
// Authored terminal-format fixture only. It never calls a person constructor,
// OS presence or a confirm command and has no production escape hatch.
pub fn bind_review(f: &Fixture, act: &str, review: &Value) -> PathBuf {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let db = f.root().join(".git/ledger.db");
    let uid = std::fs::metadata(&db).expect("db").uid();
    let dir = db.parent().expect("parent").join("decision-confirmations");
    thruline_core::private_store::create_private_dir(&dir).expect("private dir");
    let session = format!("decision-terminal-{}", ulid::Ulid::generate());
    let nonce = ulid::Ulid::generate().to_string();
    let raw = dir.join(format!("{session}.jsonl"));
    let tuple = review["confirmation"].clone();
    let line = json!({"format":"thruline_terminal_confirmation_v1","project_id":"p","session_id":session,"nonce":nonce,"action":"retire","act_row_id":act,"account_uid":uid,"account_name":"fixture","confirmation":tuple});
    let bytes = serde_json::to_vec(&line).expect("line");
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&raw)
        .expect("raw");
    file.write_all(&bytes).expect("write");
    file.write_all(b"\n").expect("newline");
    file.sync_all().expect("sync");
    let (count, digest) = thruline_core::ingest::whole_file_digest(&raw).expect("digest");
    f.ledger.conn().execute("INSERT INTO sessions(id,project_id,file_path,started_at,raw_line_count,raw_digest) VALUES(?1,'p',?2,'2026-10-07T00:07:00Z',?3,?4)",params![session,raw.to_str(),count as i64,digest]).expect("session");
    let mut writer = Ledger::open(&db).expect("writer");
    let mut event = NormalizedEvent::new(EventKind::UserMsg);
    event.role = Some("user".into());
    event.text = Some(tuple.to_string());
    event.line_session_id = Some(session.clone());
    let sha = sha256_hex(&bytes);
    let events = writer
        .append_session_events(
            "p",
            &session,
            &[(
                RawLineRef {
                    file: raw.display().to_string(),
                    line: 1,
                    sha256: sha.clone(),
                },
                event,
            )],
        )
        .expect("events");
    bridge::install_schema_for_write(&writer).expect("schema");
    let receipt = ulid::Ulid::generate().to_string();
    let tuple_sha = sha256_hex(&serde_json::to_vec(&tuple).expect("tuple"));
    let decision = tuple["thruline_decision_act"]["decision_id"]
        .as_str()
        .expect("decision");
    writer.conn().execute("INSERT INTO decision_person_acts(id,action,act_row_id,decision_id,project_id,event_id,session_id,raw_file,raw_line,raw_sha256,chain_hash,tuple_sha256) VALUES(?1,'retire',?2,?3,'p',?4,?5,?6,1,?7,?8,?9)",params![receipt,act,decision,events[0].id,session,raw.to_str(),sha,events[0].chain_hash,tuple_sha]).expect("receipt");
    writer.conn().execute("INSERT INTO decision_bridge_proofs(event_id,receipt_id,project_id,session_id,action,act_row_id,nonce,tuple_sha256,account_uid,account_name,raw_sha256,supersession_json,confirmation_method,created_at) VALUES(?1,?2,'p',?3,'retire',?4,?5,?6,?7,'fixture',?8,?9,'macos_local_authentication_tty','2026-10-07T00:07:00Z')",params![events[0].id,receipt,session,act,nonce,tuple_sha,uid,sha,tuple["thruline_general_supersession"].to_string()]).expect("bridge");
    raw
}
pub fn reviewed(f: &Fixture, act: &str) -> Value {
    bridge::review_terminal_act(&f.ledger, PersonActKind::Retire, act, None).expect("review")
}
pub fn assert_valid(f: &Fixture, act: &str) {
    assert!(bridge::verified(&f.ledger, PersonActKind::Retire, act).expect("verify"));
}
