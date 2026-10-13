//! Full parameter proposals remain inert and retain their native source.

#[path = "../../core/tests/support/person_action.rs"]
mod person_action;

use person_action::PersonRecorder;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{json, Value};
use thruline_core::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use thruline_core::decisions::params::parse_params;
use thruline_core::decisions::storage::{self, DecisionSource, NewDecision, SourceKind};
use thruline_core::Ledger;

struct Fixture {
    temp: tempfile::TempDir,
    ledger: Ledger,
    raw: PathBuf,
    id: String,
    parent: String,
    sources: String,
    candidate: PathBuf,
    parent_revision: u32,
}

impl Fixture {
    fn new() -> Self {
        Self::with_offset(0)
    }

    fn with_offset(offset: u64) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let ledger = Ledger::open(temp.path().join("ledger.db")).unwrap();
        let raw = temp.path().join("source.jsonl");
        let text = "Remove obsolete_flag everywhere.";
        let line = json!({"type":"user","sessionId":"stage-session","message":{"role":"user","content":text}}).to_string();
        std::fs::write(&raw, format!("{line}\n")).unwrap();
        ledger.conn().execute("INSERT INTO projects(id,root_path,source,created_at) VALUES('stage-project',?1,'claude_code','2026-10-08T00:00:00Z')", [temp.path().to_str().unwrap()]).unwrap();
        ledger.conn().execute("INSERT INTO sessions(id,project_id,file_path) VALUES('stage-session','stage-project',?1)", [raw.to_str().unwrap()]).unwrap();
        let raw_sha = sha256_hex(line.as_bytes());
        let prev = genesis_prev_hash("stage-project");
        let chain = chain_hash(&prev, &raw_sha, "stage-session", 1);
        ledger.conn().execute("INSERT INTO events(id,project_id,session_id,seq,kind,role,text,is_sidechain,raw_file,raw_line,raw_sha256,prev_hash,chain_hash) VALUES('stage-event','stage-project','stage-session',1,'user_msg','user',?1,0,?2,1,?3,?4,?5)", rusqlite::params![text,raw.to_str(),raw_sha,prev,chain]).unwrap();
        let original = json!({"kind":"text_absent","terms":["obsolete_flag"],"synonyms":[],"surfaces":["code"],"allowlist":[]});
        let id = storage::create(
            &ledger,
            NewDecision {
                project_id: "stage-project".into(),
                source_anchor: "stage-event".into(),
                span: text.into(),
                sources: vec![DecisionSource {
                    source_kind: SourceKind::WorkPrompt,
                    event_id: Some("stage-event".into()),
                    file_anchor: None,
                    raw_file: raw.to_string_lossy().into_owned(),
                    raw_line: 1,
                    span_start: offset,
                    span_end: text.len() as u64,
                    span: text.into(),
                    actor: Some("user".into()),
                    recorded_at: None,
                }],
                author_actor: "fixture".into(),
                recorded_at: "2026-10-08T00:00:00Z".into(),
                params: parse_params("text_absent", 1, &original.to_string()).unwrap(),
            },
        )
        .unwrap();
        let (parent, sources): (String,String) = ledger.conn().query_row("SELECT params_json,sources_json FROM decision_revisions WHERE decision_id=?1 AND rev=1", [&id], |r| Ok((r.get(0)?,r.get(1)?))).unwrap();
        let candidate = temp.path().join("params.json");
        std::fs::write(&candidate, json!({"kind":"text_absent","terms":["obsolete_flag"],"synonyms":[],"surfaces":["code","tests"],"allowlist":[],"test_allowlist":[{"path":"tests/fixture.rs","name_pattern":"negative_fixture"}]}).to_string()).unwrap();
        Self {
            temp,
            ledger,
            raw,
            id,
            parent: sha256_hex(parent.as_bytes()),
            sources: sha256_hex(sources.as_bytes()),
            candidate,
            parent_revision: 1,
        }
    }
    fn run(&self, parent: &str, candidate: &Path, hash: &str) -> Output {
        Command::new(env!("CARGO_BIN_EXE_thruline"))
            .arg("--db")
            .arg(self.temp.path().join("ledger.db"))
            .args(["decide", "--project=stage-project", "--root"])
            .arg(self.temp.path())
            .args([
                "--json",
                "revise-params",
                &self.id,
                "--parent-revision",
                &self.parent_revision.to_string(),
                "--parent-params-sha256",
                parent,
                "--parent-sources-sha256",
                &self.sources,
                "--params-file",
            ])
            .arg(candidate)
            .args(["--params-sha256", hash])
            .env("HOME", self.temp.path())
            .env("THRULINE_HOME", self.temp.path().join("state"))
            .current_dir(self.temp.path())
            .output()
            .unwrap()
    }
    fn candidate_hash(&self) -> String {
        sha256_hex(&std::fs::read(&self.candidate).unwrap())
    }
    fn list(&self) -> Value {
        let out = Command::new(env!("CARGO_BIN_EXE_thruline"))
            .arg("--db")
            .arg(self.temp.path().join("ledger.db"))
            .args(["decide", "--project=stage-project", "--root"])
            .arg(self.temp.path())
            .args(["--json", "list"])
            .env("HOME", self.temp.path())
            .env("THRULINE_HOME", self.temp.path().join("state"))
            .current_dir(self.temp.path())
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        let doc: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(doc["shape"], "thruline.decide.list");
        assert_eq!(doc["schema_version"], 1);
        doc["decisions"][0].clone()
    }
    fn revisions(&self) -> i64 {
        self.ledger
            .conn()
            .query_row("SELECT COUNT(*) FROM decision_revisions", [], |r| r.get(0))
            .unwrap()
    }
    fn unchanged_tables(&self) -> Vec<(String, Vec<String>)> {
        let names: Vec<String> = self.ledger.conn().prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name <> 'decision_revisions' ORDER BY name").unwrap().query_map([],|r|r.get(0)).unwrap().map(Result::unwrap).collect();
        names
            .into_iter()
            .map(|name| {
                let mut q = self
                    .ledger
                    .conn()
                    .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
                    .unwrap();
                let n = q.column_count();
                let mut rows: Vec<String> = q
                    .query_map([], |row| {
                        Ok((0..n)
                            .map(|i| format!("{:?}", row.get_ref(i).unwrap()))
                            .collect::<Vec<_>>()
                            .join("|"))
                    })
                    .unwrap()
                    .map(Result::unwrap)
                    .collect();
                rows.sort();
                (name, rows)
            })
            .collect()
    }
}

#[test]
fn native_list_latest_proposal_pins_feed_the_next_complete_parameter_proposal() {
    let mut f = Fixture::new();
    let before = f.unchanged_tables();
    let first = f.list();
    assert_eq!(first["id"], f.id);
    assert_eq!(
        first["latest_proposal_pins"],
        json!({"revision":1,"params_sha256":f.parent,"sources_sha256":f.sources})
    );
    f.parent_revision = first["latest_proposal_pins"]["revision"].as_u64().unwrap() as u32;
    f.sources = first["latest_proposal_pins"]["sources_sha256"]
        .as_str()
        .unwrap()
        .into();
    let parent = first["latest_proposal_pins"]["params_sha256"]
        .as_str()
        .unwrap();
    let out = f.run(parent, &f.candidate, &f.candidate_hash());
    assert!(out.status.success(), "{out:?}");
    let second = f.list();
    let candidate = std::fs::read_to_string(&f.candidate).unwrap();
    let normalized = parse_params("text_absent", 1, &candidate).unwrap();
    assert_eq!(
        second["latest_proposal_pins"],
        json!({"revision":2,"params_sha256":sha256_hex(normalized.json().unwrap().as_bytes()),"sources_sha256":f.sources})
    );
    assert_ne!(
        first["latest_proposal_pins"]["params_sha256"],
        second["latest_proposal_pins"]["params_sha256"]
    );
    f.parent_revision = second["latest_proposal_pins"]["revision"].as_u64().unwrap() as u32;
    f.sources = second["latest_proposal_pins"]["sources_sha256"]
        .as_str()
        .unwrap()
        .into();
    let parent = second["latest_proposal_pins"]["params_sha256"]
        .as_str()
        .unwrap();
    let out = f.run(parent, &f.candidate, &f.candidate_hash());
    assert!(out.status.success(), "{out:?}");
    assert_eq!(f.revisions(), 3);
    assert_eq!(before, f.unchanged_tables());
    assert!(!f.temp.path().join(".thruline/decisions.yml").exists());
}

#[test]
fn native_list_keeps_the_accepted_revision_active_and_pins_the_latest_proposal() {
    let f = Fixture::new();
    let acceptance = storage::accept(
        &f.ledger,
        storage::AcceptanceInput {
            decision_id: f.id.clone(),
            revision: 1,
            actor: "fixture-person".into(),
            at: "2026-10-08T00:01:00Z".into(),
            because: "authored acceptance fixture".into(),
            severity: "blocks".into(),
            applies_from_version: "fixture-version".into(),
        },
    )
    .unwrap();
    PersonRecorder::bridge_only(&f.ledger, "stage-project")
        .bind_bridge_acceptance(&f.ledger, &acceptance);
    let before = f.unchanged_tables();
    let out = f.run(&f.parent, &f.candidate, &f.candidate_hash());
    assert!(out.status.success(), "{out:?}");
    let row = f.list();
    assert_eq!(row["lifecycle"], "accepted");
    assert_eq!(row["revision"], 1);
    assert_eq!(row["accepted_revision"], 1);
    assert_eq!(row["params_sha256"], f.parent);
    assert_eq!(row["params"]["surfaces"], json!(["code"]));
    assert_eq!(row["latest_proposed_revision"], 2);
    let (latest_params, latest_sources): (String, String) = f
        .ledger
        .conn()
        .query_row(
            "SELECT params_json,sources_json FROM decision_revisions WHERE decision_id=?1 AND rev=2",
            [&f.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        row["latest_proposal_pins"],
        json!({"revision":2,"params_sha256":sha256_hex(latest_params.as_bytes()),"sources_sha256":sha256_hex(latest_sources.as_bytes())})
    );
    assert_ne!(
        row["params_sha256"],
        row["latest_proposal_pins"]["params_sha256"]
    );
    assert_eq!(before, f.unchanged_tables());
}

#[test]
fn native_full_params_append_is_one_inert_revision_with_unchanged_authority_and_sources() {
    let f = Fixture::new();
    let before = f.unchanged_tables();
    let out = f.run(&f.parent, &f.candidate, &f.candidate_hash());
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(doc["shape"], "thruline.decide.revise-params");
    assert_eq!(doc["schema_version"], 1);
    assert_eq!(doc["revision"], 2);
    assert_eq!(doc["state"], "proposed");
    assert_eq!(doc["person_act_recorded"], false);
    assert_eq!(f.revisions(), 2);
    assert_eq!(before, f.unchanged_tables());
    let rows: Vec<String> = f
        .ledger
        .conn()
        .prepare("SELECT sources_json FROM decision_revisions ORDER BY rev")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(rows[0], rows[1]);
    assert!(!f.temp.path().join(".thruline/decisions.yml").exists());
    let again = f.run(&f.parent, &f.candidate, &f.candidate_hash());
    assert!(!again.status.success());
    assert_eq!(f.revisions(), 2);
}

#[test]
fn wrong_parent_or_document_digest_cannot_append() {
    let f = Fixture::new();
    let before = f.unchanged_tables();
    assert!(!f
        .run(&"0".repeat(64), &f.candidate, &f.candidate_hash())
        .status
        .success());
    assert!(!f
        .run(&f.parent, &f.candidate, &"0".repeat(64))
        .status
        .success());
    assert_eq!(f.revisions(), 1);
    assert_eq!(before, f.unchanged_tables());
}

#[test]
fn noncanonical_parent_source_bytes_cannot_be_silently_rewritten() {
    let mut f = Fixture::new();
    let raw: String = f
        .ledger
        .conn()
        .query_row(
            "SELECT sources_json FROM decision_revisions WHERE decision_id=?1 AND rev=1",
            [&f.id],
            |r| r.get(0),
        )
        .unwrap();
    let pretty =
        serde_json::to_string_pretty(&serde_json::from_str::<Value>(&raw).unwrap()).unwrap();
    // An authored legacy-format proposal, appended without changing a prior row.
    f.ledger.conn().execute("INSERT INTO decision_revisions(decision_id,rev,kind,params_schema_version,params_json,text,sources_json,author_actor,recorded_at) SELECT decision_id,2,kind,params_schema_version,params_json,text,?2,author_actor,recorded_at FROM decision_revisions WHERE decision_id=?1 AND rev=1", rusqlite::params![f.id,pretty]).unwrap();
    f.parent_revision = 2;
    f.sources = sha256_hex(pretty.as_bytes());
    let before = f.unchanged_tables();
    let out = f.run(&f.parent, &f.candidate, &f.candidate_hash());
    assert!(!out.status.success(), "{out:?}");
    assert_eq!(f.revisions(), 2);
    assert_eq!(before, f.unchanged_tables());
}

#[test]
fn changed_native_source_cannot_be_staged_as_a_person_proposal() {
    let f = Fixture::new();
    std::fs::write(&f.raw, "changed\n").unwrap();
    let out = f.run(&f.parent, &f.candidate, &f.candidate_hash());
    assert!(!out.status.success());
    assert_eq!(f.revisions(), 1);
}

#[test]
fn closed_params_and_exact_source_span_remain_required() {
    let f = Fixture::new();
    std::fs::write(&f.candidate, "{\"unknown\":true}").unwrap();
    assert!(!f
        .run(&f.parent, &f.candidate, &f.candidate_hash())
        .status
        .success());
    assert_eq!(f.revisions(), 1);
    let g = Fixture::with_offset(1);
    assert!(!g
        .run(&g.parent, &g.candidate, &g.candidate_hash())
        .status
        .success());
    assert_eq!(g.revisions(), 1);
}

#[cfg(unix)]
#[test]
fn parameter_input_refuses_symlinks_and_nonregular_files_without_hanging() {
    let f = Fixture::new();
    let link = f.temp.path().join("link.json");
    std::os::unix::fs::symlink(&f.candidate, &link).unwrap();
    assert!(!f
        .run(&f.parent, &link, &f.candidate_hash())
        .status
        .success());
    assert!(!f
        .run(&f.parent, f.temp.path(), &f.candidate_hash())
        .status
        .success());
    assert_eq!(f.revisions(), 1);
}
