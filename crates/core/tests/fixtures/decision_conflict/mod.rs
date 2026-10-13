#![allow(dead_code)]
use crate::person_action::PersonRecorder;
use serde_json::{json, Value};
use std::{path::Path, process::Command};
use thruline_core::{
    decisions::{
        params::*,
        storage::{self, *},
    },
    ledger::Ledger,
};

pub fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("fixture git");
    assert!(output.status.success(), "git {args:?}: {:?}", output.stderr);
    String::from_utf8(output.stdout)
        .expect("fixture git UTF8")
        .trim()
        .into()
}
pub fn commit(repo: &Path) -> String {
    git(repo, &["add", "-A"]);
    git(
        repo,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
    );
    git(repo, &["rev-parse", "HEAD"])
}
pub struct Fixture {
    pub temp: tempfile::TempDir,
    pub ledger: Ledger,
    pub version: String,
    pub people: Vec<PersonRecorder>,
}
impl Fixture {
    pub fn new() -> Self {
        let temp = tempfile::tempdir().expect("fixture temp");
        git(temp.path(), &["init", "-q"]);
        std::fs::write(temp.path().join("app.rs"), "const ENABLED: bool = true;\n")
            .expect("fixture source");
        let version = commit(temp.path());
        let ledger = Ledger::open(temp.path().join(".git/ledger.db")).expect("fixture ledger");
        for project in ["p", "other"] {
            ledger.conn().execute("INSERT INTO projects(id,root_path,source,created_at) VALUES(?1,?2,'claude_code','2026-10-07T00:00:00Z')",rusqlite::params![project,temp.path().display().to_string()]).expect("fixture project");
        }
        Self {
            temp,
            ledger,
            version,
            people: vec![],
        }
    }
    pub fn decision(&mut self, project: &str, params: CheckParams, accepted: bool) -> String {
        let anchor = ulid::Ulid::generate().to_string();
        let span = "Keep this exact rule";
        let id = storage::create(
            &self.ledger,
            NewDecision {
                project_id: project.into(),
                source_anchor: anchor.clone(),
                span: span.into(),
                sources: vec![DecisionSource {
                    source_kind: SourceKind::WorkPrompt,
                    event_id: Some(anchor),
                    file_anchor: None,
                    raw_file: "fixture.jsonl".into(),
                    raw_line: 1,
                    span_start: 0,
                    span_end: span.len() as u64,
                    span: span.into(),
                    actor: Some("person".into()),
                    recorded_at: Some("2026-10-07T00:00:00Z".into()),
                }],
                author_actor: "person".into(),
                recorded_at: "2026-10-07T00:00:00Z".into(),
                params,
            },
        )
        .expect("fixture decision");
        let act = storage::accept(
            &self.ledger,
            AcceptanceInput {
                decision_id: id.clone(),
                revision: 1,
                actor: "person".into(),
                at: "2026-10-07T00:01:00Z".into(),
                because: "exact fixture".into(),
                severity: "warns".into(),
                applies_from_version: self.version.clone(),
            },
        )
        .expect("fixture acceptance");
        if accepted {
            let mut recorder = PersonRecorder::bridge_only(&self.ledger, project);
            recorder.bind_acceptance(&self.ledger, &act);
            self.people.push(recorder);
        }
        id
    }
    pub fn root(&self) -> &Path {
        self.temp.path()
    }
    pub fn captured(&mut self, rows: Vec<Value>) -> Vec<String> {
        use thruline_core::{
            chain::sha256_hex,
            model::RawLineRef,
            parse::{parse_reader, ParseStats},
        };
        let path = self.root().join(".git/agent.jsonl");
        let bytes = rows
            .iter()
            .map(|row| format!("{row}\n"))
            .collect::<String>();
        std::fs::write(&path, &bytes).expect("fixture raw");
        self.ledger.conn().execute("INSERT INTO sessions(id,project_id,file_path,started_at) VALUES('agent','p',?1,'2026-10-07T00:03:00Z')",[path.display().to_string()]).expect("fixture session");
        let mut stats = ParseStats::default();
        let parsed = parse_reader(bytes.as_bytes(), path.display().to_string(), &mut stats)
            .expect("fixture parse");
        let mut normalized = vec![];
        for (index, line) in parsed.iter().enumerate() {
            for event in &line.events {
                normalized.push((
                    RawLineRef {
                        file: path.display().to_string(),
                        line: index as u64 + 1,
                        sha256: sha256_hex(rows[index].to_string().as_bytes()),
                    },
                    event.clone(),
                ));
            }
        }
        self.ledger
            .append_session_events("p", "agent", &normalized)
            .expect("fixture events")
            .into_iter()
            .map(|event| event.id)
            .collect()
    }
}
pub fn default(value: &str) -> CheckParams {
    CheckParams::FeatureDefaultPreserved(FeatureDefaultPreservedParams {
        path: "app.rs".into(),
        symbol: "ENABLED".into(),
        expected_value: value.into(),
    })
}
pub fn absence() -> CheckParams {
    CheckParams::TextAbsent(TextAbsentParams {
        terms: vec!["obsolete".into()],
        synonyms: vec![],
        surfaces: vec![Surface::Code],
        allowlist: vec![],
        test_allowlist: vec![],
        json_field_allowlist: vec![],
        conditional_path_allowlist: vec![],
        span_allowlist: vec![],
    })
}
pub fn edit_rows(
    before: &str,
    after: &str,
    version: &str,
    root: &Path,
    failed: bool,
    echo: bool,
) -> Vec<Value> {
    let quoted_root = format!("'{}'", root.display().to_string().replace('\'', "'\\''"));
    let observe = format!("git -C {quoted_root} rev-parse HEAD");
    vec![
        json!({"type":"assistant","sessionId":"agent","message":{"role":"assistant","content":[{"type":"tool_use","id":"edit","name":"Edit","input":{"file_path":root.join("app.rs"),"old_string":before,"new_string":after,"replace_all":false}}]}}),
        json!({"type":"user","sessionId":"agent","toolUseResult":if echo { json!({"filePath":root.join("app.rs"),"oldString":before,"newString":after,"originalFile":before}) } else { Value::Null },"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"edit","is_error":failed,"content":"updated"}]}}),
        json!({"type":"assistant","sessionId":"agent","message":{"role":"assistant","content":[{"type":"tool_use","id":"head","name":"Bash","input":{"command":observe}}]}}),
        json!({"type":"user","sessionId":"agent","toolUseResult":{"stdout":version,"stderr":"","exitCode":0},"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"head","is_error":false,"content":version}]}}),
    ]
}
