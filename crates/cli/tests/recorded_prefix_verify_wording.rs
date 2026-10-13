//! C44 §6: distinguish an agent-file rewrite from damage to the stored record.
//! Every fixture is an authored tempfile store, never a founder-store clone.

use std::os::unix::fs::PermissionsExt;
use std::{fs, path::PathBuf, process::Command};

use tempfile::TempDir;
use thruline_core::{
    chain::{chain_hash, genesis_prev_hash, sha256_hex},
    ingest::ingest,
    Ledger,
};

const PROJECT: &str = "-Users-synthetic-wording";
const SESSION: &str = "aaaaaaaa-0000-4000-8000-000000000003";
const RECORD: &str = "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"authored wording control\"},\"sessionId\":\"aaaaaaaa-0000-4000-8000-000000000003\",\"uuid\":\"wording-user\"}\n";
const CHANGED: &str = "The agent's own session file changed after ThruLine recorded it.";
const INTACT: &str = "ThruLine's record of the original is intact";

struct Fixture {
    _temp: TempDir,
    home: PathBuf,
    db: PathBuf,
    source: PathBuf,
    payload: PathBuf,
}

impl Fixture {
    fn rewritten() -> Self {
        let temp = tempfile::tempdir().expect("authored fixture");
        let home = temp.path().join("home");
        fs::create_dir(&home).expect("synthetic home");
        let project = temp.path().join("projects").join(PROJECT);
        fs::create_dir_all(&project).expect("synthetic project");
        let source = project.join(format!("{SESSION}.jsonl"));
        fs::write(&source, RECORD).expect("authored recording");
        let db = temp.path().join("ledger.db");
        let mut ledger = Ledger::open(&db).expect("authored ledger");
        let report = ingest(&mut ledger, &[project], &temp.path().join("mirrors"))
            .expect("synthetic public ingest");
        assert_eq!(report.claude_sealed, 1);
        assert!(report.new_events > 0);
        let generation = ledger
            .accepted_generation(SESSION)
            .expect("accepted receipt")
            .expect("accepted generation");
        let payload = temp
            .path()
            .join("mirrors/ledgers")
            .join(ledger.mirror_authority_key().expect("synthetic identity"))
            .join(generation.mirror_payload_path);
        drop(ledger);
        fs::write(&source, RECORD.replace("authored", "rewritten")).expect("agent-file rewrite");
        Self {
            _temp: temp,
            home,
            db,
            source,
            payload,
        }
    }

    fn output(&self, json: bool) -> Vec<u8> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_thruline"));
        command
            .env("HOME", &self.home)
            .env("THRULINE_HOME", self.home.join(".thruline"))
            .arg("--db")
            .arg(&self.db)
            .args(["verify", "--project", PROJECT]);
        if json {
            command.arg("--json");
        }
        let out = command.output().expect("native synthetic verify");
        assert_eq!(out.status.code(), Some(1), "{out:?}");
        out.stdout
    }

    fn reason(&self) -> String {
        let doc: serde_json::Value =
            serde_json::from_slice(&self.output(true)).expect("registered JSON");
        assert_eq!(doc["shape"], "thruline.verify");
        assert_eq!(doc["exit_code"], 1);
        assert_eq!(
            doc["projects"][PROJECT]["sources"][0]["state"],
            "PREFIX_MISMATCH"
        );
        doc["projects"][PROJECT]["sources"][0]["reason"]
            .as_str()
            .expect("source explanation")
            .to_owned()
    }

    fn corrupt(&self, statement: &str, trigger_name: &str) {
        let conn = rusqlite::Connection::open(&self.db).expect("authored ledger");
        let trigger: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name=?1",
                [trigger_name],
                |row| row.get(0),
            )
            .expect("immutable fixture trigger");
        conn.execute_batch(&format!("DROP TRIGGER {trigger_name}"))
            .expect("authored tamper setup");
        assert!(conn.execute(statement, []).expect("authored corruption") > 0);
        conn.execute_batch(&trigger).expect("restore exact schema");
    }
}

#[test]
fn an_agent_prefix_rewrite_names_the_intact_record_and_verified_sealed_copy() {
    let f = Fixture::rewritten();
    assert!(f.source.is_file());
    let expected = format!("{CHANGED} {INTACT}, and its sealed copy is intact.");
    assert_eq!(f.reason(), expected);
    let text = String::from_utf8(f.output(false)).expect("text output");
    assert!(text.contains(&expected), "{text}");
    assert!(text.contains("PREFIX MISMATCH"));
}

#[test]
fn an_absent_sealed_copy_is_never_described_as_intact() {
    let f = Fixture::rewritten();
    let parent = f.payload.parent().expect("authored payload directory");
    let permissions = fs::metadata(parent).expect("fixture mode").permissions();
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
        .expect("make authored directory writable");
    fs::remove_file(&f.payload).expect("remove only authored payload");
    fs::set_permissions(parent, permissions).expect("restore fixture mode");
    let reason = f.reason();
    assert_eq!(reason, CHANGED);
    assert!(!reason.contains(INTACT));
    assert!(!reason.contains("and its sealed copy"));
}

#[test]
fn a_real_chain_failure_prevents_an_intact_record_claim() {
    let f = Fixture::rewritten();
    f.corrupt(
        "UPDATE events SET chain_hash=printf('%064d',0)",
        "events_no_update",
    );
    let reason = f.reason();
    assert!(reason.starts_with(CHANGED));
    assert!(!reason.contains(INTACT), "{reason}");
    let text = String::from_utf8(f.output(false)).expect("text output");
    assert!(!text.contains(INTACT), "{text}");
}

#[test]
fn a_corrupt_sealed_original_prevents_an_intact_record_claim() {
    let f = Fixture::rewritten();
    let permissions = fs::metadata(&f.payload)
        .expect("fixture mode")
        .permissions();
    fs::set_permissions(&f.payload, fs::Permissions::from_mode(0o600))
        .expect("make authored payload writable");
    fs::write(&f.payload, b"corrupt authored seal payload\n").expect("corrupt synthetic payload");
    fs::set_permissions(&f.payload, permissions).expect("restore fixture mode");
    let reason = f.reason();
    assert!(reason.starts_with(CHANGED));
    assert!(!reason.contains(INTACT), "{reason}");
}

#[test]
fn contradictory_record_accounting_prevents_an_intact_record_claim() {
    let f = Fixture::rewritten();
    f.corrupt(
        "UPDATE session_generations SET total_envelopes=0",
        "session_generations_no_update",
    );
    let reason = f.reason();
    assert!(reason.starts_with(CHANGED));
    assert!(!reason.contains(INTACT), "{reason}");
}

#[test]
fn corrupted_session_digest_prevents_an_intact_original_claim_despite_a_valid_seal() {
    let f = Fixture::rewritten();
    let conn = rusqlite::Connection::open(&f.db).expect("authored ledger");
    assert!(
        conn.execute("UPDATE sessions SET raw_digest=printf('%064d',0)", [])
            .expect("authored stored digest corruption")
            > 0
    );
    drop(conn);
    let reason = f.reason();
    assert!(reason.starts_with(CHANGED));
    assert!(!reason.contains(INTACT), "{reason}");
}

#[test]
fn corrupted_session_line_count_prevents_an_intact_original_claim_despite_a_valid_seal() {
    let f = Fixture::rewritten();
    let conn = rusqlite::Connection::open(&f.db).expect("authored ledger");
    assert!(
        conn.execute("UPDATE sessions SET raw_line_count=0", [])
            .expect("authored stored count corruption")
            > 0
    );
    drop(conn);
    let reason = f.reason();
    assert!(reason.starts_with(CHANGED));
    assert!(!reason.contains(INTACT), "{reason}");
}

#[test]
fn a_matching_current_receipt_cannot_prove_the_different_original_record() {
    let f = Fixture::rewritten();
    let ledger = Ledger::open(&f.db).expect("authored ledger");
    let bound = ledger
        .accepted_generation(SESSION)
        .expect("accepted receipt")
        .expect("accepted generation")
        .payload_bytes as usize;
    drop(ledger);
    let current = fs::read(&f.source).expect("authored changed bytes");
    let prefix = std::str::from_utf8(&current[..bound]).expect("authored bounded row");
    let forged_raw = sha256_hex(prefix.trim_end_matches('\n').as_bytes());
    let conn = rusqlite::Connection::open(&f.db).expect("authored ledger");
    let trigger: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='events_no_update'",
            [],
            |row| row.get(0),
        )
        .expect("immutable fixture trigger");
    let links: Vec<(String, i64)> = conn
        .prepare("SELECT id,seq FROM events WHERE session_id=?1 ORDER BY seq")
        .expect("authored links")
        .query_map([SESSION], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("authored query")
        .collect::<Result<_, _>>()
        .expect("authored rows");
    assert!(!links.is_empty());
    conn.execute_batch("DROP TRIGGER events_no_update")
        .expect("authored tamper setup");
    let mut prev = genesis_prev_hash(PROJECT);
    for (id, seq) in links {
        let next = chain_hash(&prev, &forged_raw, SESSION, seq);
        assert_eq!(
            conn.execute(
                "UPDATE events SET raw_sha256=?1,prev_hash=?2,chain_hash=?3 WHERE id=?4",
                [&forged_raw, &prev, &next, &id],
            )
            .expect("authored internally consistent chain tamper"),
            1
        );
        prev = next;
    }
    conn.execute_batch(&trigger).expect("restore exact schema");
    drop(conn);
    let doc: serde_json::Value = serde_json::from_slice(&f.output(true)).expect("registered JSON");
    let project = &doc["projects"][PROJECT];
    assert_eq!(project["chain_matches"], project["events_checked"]);
    assert!(project["chain_mismatches"]
        .as_array()
        .expect("chain")
        .is_empty());
    assert!(project["raw_mismatches"]
        .as_array()
        .expect("raw")
        .is_empty());
    assert_eq!(project["sources"][0]["sealed_copy"], "INTACT");
    let reason = project["sources"][0]["reason"].as_str().expect("reason");
    assert!(reason.starts_with(CHANGED));
    assert!(!reason.contains(INTACT), "{reason}");
}
