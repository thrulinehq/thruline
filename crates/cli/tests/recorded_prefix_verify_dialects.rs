//! Interrupted Claude acceptance is a supported byte-digest state, not tampering.
//! Only authored temporary ledgers, captured through the public acceptance API.

use std::{fs, io::Write, path::PathBuf, process::Command};

use tempfile::TempDir;
use thruline_core::{
    chain::sha256_hex,
    ledger::{
        AcceptedGeneration, GenerationAcceptance, RawDisposition, SourceProjectIdentity,
        SourceSessionIdentity,
    },
    mirror::{MirrorIdentity, MirrorStore},
    model::{ProjectRef, SessionRef, Source},
    Ledger,
};

const PROJECT: &str = "-Users-synthetic-dialect";
const SESSION: &str = "aaaaaaaa-0000-4000-8000-000000000002";
const RECORD: &[u8] = br#"{"type":"user","message":{"role":"user","content":"accepted before parsing"},"sessionId":"aaaaaaaa-0000-4000-8000-000000000002","uuid":"dialect-user"}"#;

struct Fixture {
    _temp: TempDir,
    home: PathBuf,
    db: PathBuf,
    source: PathBuf,
}

impl Fixture {
    fn accepted_before_parse() -> Self {
        let temp = tempfile::tempdir().expect("authored fixture");
        let home = temp.path().join("home");
        fs::create_dir(&home).expect("private synthetic home");
        let source = temp.path().join(format!("{SESSION}.jsonl"));
        fs::write(&source, RECORD).expect("authored source at recorded EOF");
        let db = temp.path().join("ledger.db");
        let mut ledger = Ledger::open(&db).expect("authored ledger");
        let authority = ledger.mirror_authority_key().expect("ledger identity");
        let mirror = MirrorStore::open_namespaced(temp.path().join("mirrors"), &authority)
            .expect("private mirror");
        let identity = MirrorIdentity {
            owner_key: "legacy-local".into(),
            source_project_key: PROJECT.into(),
            source_session_key: SESSION.into(),
        };
        let lease = mirror.acquire_ingest_lease().expect("capture lease");
        let sealed = mirror
            .capture(&lease, &source, &identity)
            .expect("public capture");
        let acceptance = GenerationAcceptance {
            project: ProjectRef {
                id: PROJECT.into(),
                root_path: temp.path().display().to_string(),
                source: Source::ClaudeCode,
                display_name: None,
            },
            project_identity: SourceProjectIdentity {
                project_id: PROJECT.into(),
                owner_key: identity.owner_key.clone(),
                source: Source::ClaudeCode.as_str().into(),
                source_project_key: PROJECT.into(),
                normalized_root_hint: None,
            },
            session: SessionRef {
                id: SESSION.into(),
                project_id: PROJECT.into(),
                file_path: source.display().to_string(),
                started_at: None,
                raw_line_count: 1,
                raw_digest: Some(sealed.payload_sha256.clone()),
            },
            session_identity: SourceSessionIdentity {
                session_id: SESSION.into(),
                project_id: PROJECT.into(),
                owner_key: identity.owner_key,
                source: Source::ClaudeCode.as_str().into(),
                source_session_key: SESSION.into(),
                source_locator: Some(source.display().to_string()),
            },
            generation: AcceptedGeneration {
                id: format!("generation_{}", sealed.generation_key),
                project_id: PROJECT.into(),
                session_id: SESSION.into(),
                predecessor_id: None,
                mirror_payload_path: sealed.relative_payload,
                mirror_seal_path: sealed.relative_seal,
                payload_sha256: sealed.payload_sha256,
                payload_bytes: sealed.payload_bytes,
                seal_sha256: sealed.seal_sha256,
                inherited_prefix_bytes: 0,
                total_envelopes: 1,
                local_envelopes: 1,
                local_events: 0,
                accepted_at: "2026-10-08T00:00:00Z".into(),
            },
            expected_predecessor: None,
            dispositions: vec![RawDisposition {
                raw_line: 1,
                raw_sha256: sha256_hex(RECORD),
                byte_start: 0,
                byte_length: RECORD.len() as u64,
                envelope_family: "user".into(),
                disposition: "unknown".into(),
                reason: Some("claude_adapter_v1_no_disposition_semantics".into()),
                source_turn_id: None,
                turn_boundary_raw_line: None,
                turn_boundary_raw_sha256: None,
                emitted_event_count: 0,
            }],
            events: Vec::new(),
        };
        ledger
            .accept_source_generation(&acceptance)
            .expect("public atomic acceptance");
        assert_eq!(ledger.session_chain(SESSION).expect("chain").len(), 0);
        drop(lease);
        drop(ledger);
        Self {
            _temp: temp,
            home,
            db,
            source,
        }
    }

    fn verify(&self, expected: i32) -> serde_json::Value {
        let out = Command::new(env!("CARGO_BIN_EXE_thruline"))
            .env("HOME", &self.home)
            .env("THRULINE_HOME", self.home.join(".thruline"))
            .arg("--db")
            .arg(&self.db)
            .args(["verify", "--project", PROJECT, "--json"])
            .output()
            .expect("native verify");
        assert_eq!(out.status.code(), Some(expected), "{out:?}");
        let doc: serde_json::Value = serde_json::from_slice(&out.stdout).expect("registered JSON");
        assert_eq!(doc["shape"], "thruline.verify");
        assert_eq!(doc["exit_code"], expected);
        doc["projects"][PROJECT].clone()
    }
}

fn assert_qualified_bytes(project: &serde_json::Value, state: &str) {
    assert_eq!(project["sources"][0]["state"], state);
    assert_eq!(project["sources"][0]["sealed_copy"], "INTACT");
    assert_eq!(
        project["sources"][0]["recorded_sha256"],
        project["sources"][0]["prefix_sha256"]
    );
    assert!(project["digest_mismatches"]
        .as_array()
        .expect("mismatches")
        .is_empty());
    assert!(project["raw_mismatches"]
        .as_array()
        .expect("raw mismatches")
        .is_empty());
}

#[test]
fn interrupted_claude_acceptance_has_independently_intact_bytes() {
    let f = Fixture::accepted_before_parse();
    assert_qualified_bytes(&f.verify(0), "INTACT");
}

#[test]
fn interrupted_claude_acceptance_with_later_physical_line_growth_is_not_a_mismatch() {
    let f = Fixture::accepted_before_parse();
    let tail = b" more unrecorded bytes\n";
    fs::OpenOptions::new()
        .append(true)
        .open(&f.source)
        .expect("source")
        .write_all(tail)
        .expect("authored append");
    let project = f.verify(0);
    assert_qualified_bytes(&project, "GREW_AFTER_RECORDING");
    assert_eq!(project["sources"][0]["unrecorded_bytes"], tail.len());
}

#[test]
fn a_digest_outside_both_qualified_claude_dialects_remains_an_integrity_finding() {
    let f = Fixture::accepted_before_parse();
    let conn = rusqlite::Connection::open(&f.db).expect("authored ledger");
    conn.execute("UPDATE sessions SET raw_digest=?1", ["0".repeat(64)])
        .expect("authored corruption");
    drop(conn);
    let project = f.verify(1);
    assert_eq!(project["sources"][0]["state"], "INTACT");
    assert!(project["digest_mismatches"]
        .as_array()
        .expect("mismatches")
        .iter()
        .any(|s| s.as_str().is_some_and(|s| s.contains("raw_digest"))));
}

#[test]
fn an_interrupted_parse_marker_cannot_hide_a_real_recorded_prefix_rewrite() {
    let f = Fixture::accepted_before_parse();
    let changed = String::from_utf8(RECORD.to_vec())
        .expect("fixture")
        .replace("accepted", "rewritten");
    fs::write(&f.source, changed).expect("authored prefix rewrite");
    let project = f.verify(1);
    assert_eq!(project["sources"][0]["state"], "PREFIX_MISMATCH");
}

#[test]
fn source_gone_retains_qualified_accepted_byte_dialect_from_the_sealed_copy() {
    let f = Fixture::accepted_before_parse();
    fs::remove_file(&f.source).expect("authored source removal");
    let project = f.verify(2);
    assert_eq!(project["sources"][0]["state"], "SOURCE_GONE");
    assert_eq!(project["sources"][0]["sealed_copy"], "INTACT");
    assert!(project["digest_mismatches"]
        .as_array()
        .expect("mismatches")
        .is_empty());
}
