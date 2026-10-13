use crate::fixture::Fixture;
use rusqlite::params;
use serde_json::{json, Value};
use std::io::Write;
use std::path::PathBuf;
use thruline_core::{
    chain::sha256_hex,
    decisions::authority::bridge,
    ledger::Ledger,
    model::{EventKind, NormalizedEvent, RawLineRef},
};

pub fn bind_native(f: &Fixture, act: &str, review: &Value, alteration: &str) -> PathBuf {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let db = f.root().join(".git/ledger.db");
    let uid = std::fs::metadata(&db).expect("db").uid();
    let dir = db.parent().expect("parent").join("decision-confirmations");
    thruline_core::private_store::create_private_dir(&dir).expect("private dir");
    let session = format!("decision-terminal-{}", ulid::Ulid::generate());
    let nonce = ulid::Ulid::generate().to_string();
    let raw = dir.join(format!("{session}.jsonl"));
    let mut tuple = review["confirmation"].clone();
    let review_sha = sha256_hex(&serde_json::to_vec(review).expect("review bytes"));
    tuple["thruline_native_review"] =
        json!({"schema_version":1,"review_sha256":review_sha,"nonce":nonce});
    let mut envelope = json!({"schema_version":1,"kind":"native_review_click","review_sha256":review_sha,"supersession":review["supersession"]});
    match alteration {
        "hash" => envelope["review_sha256"] = json!("e".repeat(64)),
        "schema" => envelope["schema_version"] = json!(2),
        "extra" => envelope["extra"] = json!(true),
        "pin" => envelope["supersession"]["successor_params_sha256"] = json!("e".repeat(64)),
        "action" => envelope["supersession"]["kind"] = json!("visual_reference_supersession"),
        "missing" => envelope
            .as_object_mut()
            .expect("object")
            .remove("review_sha256")
            .map(|_| ())
            .unwrap_or(()),
        "nonce" => tuple["thruline_native_review"]["nonce"] = json!("different"),
        _ => (),
    }
    let line = json!({"format":if alteration=="format" {"thruline_terminal_confirmation_v1"} else {"thruline_native_click_confirmation_v1"},"project_id":"p","session_id":session,"nonce":nonce,"action":"retire","act_row_id":act,"account_uid":uid,"account_name":"fixture","confirmation":tuple});
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
    writer.conn().execute("INSERT INTO decision_bridge_proofs(event_id,receipt_id,project_id,session_id,action,act_row_id,nonce,tuple_sha256,account_uid,account_name,raw_sha256,supersession_json,confirmation_method,created_at) VALUES(?1,?2,'p',?3,'retire',?4,?5,?6,?7,'fixture',?8,?9,'macos_local_authentication_tty','2026-10-07T00:07:00Z')",params![events[0].id,receipt,session,act,nonce,tuple_sha,uid,sha,envelope.to_string()]).expect("bridge");
    raw
}
