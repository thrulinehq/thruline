#![allow(dead_code)]
use rusqlite::params;
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use thruline_core::{
    chain::{chain_hash, genesis_prev_hash, sha256_hex},
    codex::{normalize_codex_line, CodexLineContext},
    Ledger,
};

pub const PROJECT: &str = "deferred-project";
pub struct Fixture {
    pub temp: tempfile::TempDir,
    pub ledger: Ledger,
    pub repo: PathBuf,
    pub version: String,
    pub tree: String,
    pub raw: PathBuf,
    pub session: String,
}
pub fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}
impl Fixture {
    pub fn claude(items: Vec<Value>) -> Self {
        let mut fixture = Self::new(vec![]);
        fixture.session = "claude-person".into();
        fixture.raw = fixture.temp.path().join("claude-person.jsonl");
        fixture
            .ledger
            .conn()
            .execute(
                "UPDATE projects SET source='claude_code' WHERE id=?1",
                [PROJECT],
            )
            .unwrap();
        fixture
            .ledger
            .conn()
            .execute(
                "INSERT INTO sessions(id,project_id,file_path,started_at) VALUES(?1,?2,?3,'now')",
                params![fixture.session, PROJECT, fixture.raw.to_string_lossy()],
            )
            .unwrap();
        let lines: Vec<String> = items
            .into_iter()
            .map(|mut item| {
                item["sessionId"] = json!(fixture.session);
                item.to_string()
            })
            .collect();
        let bytes = format!("{}\n", lines.join("\n"));
        fs::write(&fixture.raw, &bytes).unwrap();
        let parsed = thruline_core::parse::parse_reader(
            bytes.as_bytes(),
            fixture.raw.to_string_lossy(),
            &mut thruline_core::parse::ParseStats::default(),
        )
        .unwrap();
        let mut previous = genesis_prev_hash(PROJECT);
        let mut seq = 0i64;
        for line in parsed {
            for e in line.events {
                seq += 1;
                let hash = chain_hash(&previous, &line.sha256, &fixture.session, seq);
                fixture.ledger.conn().execute("INSERT INTO events(id,project_id,session_id,seq,kind,role,text,tool_name,source_tool_name,tool_use_id,tool_input,tool_result,is_error,is_sidechain,source_turn_id,line_session_id,source_turn_boundary_line,source_turn_boundary_sha256,raw_file,raw_line,raw_sha256,prev_hash,chain_hash) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23)",params![format!("claude-event-{seq}"),PROJECT,fixture.session,seq,e.kind.as_str(),e.role,e.text,e.tool_name,e.source_tool_name,e.tool_use_id,e.tool_input,e.tool_result,i64::from(e.is_error),i64::from(e.is_sidechain),e.source_turn_id,e.line_session_id,e.source_turn_boundary_line.map(|x|x as i64),e.source_turn_boundary_sha256,fixture.raw.to_string_lossy(),line.line as i64,line.sha256,previous,hash]).unwrap();
                previous = hash;
            }
        }
        fixture
    }
    pub fn append(&self, items: Vec<Value>) {
        let existing = fs::read_to_string(&self.raw).unwrap();
        let count = existing.lines().count();
        let boundary = sha256_hex(existing.lines().nth(1).unwrap().as_bytes());
        let mut previous: String = self
            .ledger
            .conn()
            .query_row(
                "SELECT chain_hash FROM events WHERE session_id=?1 ORDER BY seq DESC LIMIT 1",
                [&self.session],
                |row| row.get(0),
            )
            .unwrap();
        let mut appended = String::new();
        for (index, mut item) in items.into_iter().enumerate() {
            if let Some(payload) = item.get_mut("payload") {
                for field in ["arguments", "output"] {
                    if let Some(value) = payload.get_mut(field) {
                        if let Some(text) = value.as_str() {
                            *value = Value::String(
                                text.replace("$REPO", &self.repo.to_string_lossy())
                                    .replace("$VERSION", &self.version),
                            );
                        }
                    }
                }
            }
            let line = item.to_string();
            let seq = (count + index + 1) as i64;
            let rawhash = sha256_hex(line.as_bytes());
            let context = CodexLineContext {
                owner_session_id: &self.session,
                source_session_id: "native-deferred",
                file_id: &self.session,
                turn_scope: "turn:turn-one",
                turn_boundary_line: Some(2),
                turn_boundary_sha256: Some(&boundary),
                line: seq as u64,
                raw_sha256: &rawhash,
            };
            let mapped = normalize_codex_line(context, &item).unwrap();
            assert_eq!(mapped.events.len(), 1);
            let e = &mapped.events[0].event;
            let hash = chain_hash(&previous, &rawhash, &self.session, seq);
            self.ledger.conn().execute("INSERT INTO events(id,project_id,session_id,seq,kind,role,text,tool_name,source_tool_name,tool_use_id,tool_input,tool_result,is_error,is_sidechain,source_turn_id,line_session_id,source_turn_boundary_line,source_turn_boundary_sha256,raw_file,raw_line,raw_sha256,prev_hash,chain_hash) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23)",params![format!("event-{seq}"),PROJECT,self.session,seq,e.kind.as_str(),e.role,e.text,e.tool_name,e.source_tool_name,e.tool_use_id,e.tool_input,e.tool_result,i64::from(e.is_error),i64::from(e.is_sidechain),e.source_turn_id,e.line_session_id,e.source_turn_boundary_line.map(|x|x as i64),e.source_turn_boundary_sha256,self.raw.to_string_lossy(),seq,rawhash,previous,hash]).unwrap();
            previous = hash;
            appended.push_str(&line);
            appended.push('\n');
        }
        fs::write(&self.raw, format!("{existing}{appended}")).unwrap();
    }
    pub fn new(items: Vec<Value>) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q"]);
        git(&repo, &["config", "user.name", "Fixture"]);
        git(&repo, &["config", "user.email", "fixture@example.invalid"]);
        fs::write(repo.join("REQUIRED"), "ready\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "fixture"]);
        let version = git(&repo, &["rev-parse", "HEAD"]);
        let tree = git(&repo, &["rev-parse", "HEAD^{tree}"]);
        let raw = temp.path().join("session.jsonl");
        let ledger = Ledger::open(temp.path().join("ledger.db")).unwrap();
        ledger
            .conn()
            .execute(
                "INSERT INTO projects(id,root_path,source,created_at) VALUES(?1,?2,'codex','now')",
                params![PROJECT, repo.to_string_lossy()],
            )
            .unwrap();
        let native = "native-deferred";
        let session = thruline_core::adapter::stable_source_session_id(
            "owner",
            thruline_core::model::Source::Codex,
            native,
        );
        ledger
            .conn()
            .execute(
                "INSERT INTO sessions(id,project_id,file_path,started_at) VALUES(?1,?2,?3,'now')",
                params![session, PROJECT, raw.to_string_lossy()],
            )
            .unwrap();
        ledger.conn().execute("INSERT INTO source_sessions(session_id,project_id,owner_key,source,source_session_key) VALUES(?1,?2,'owner','codex',?3)",params![session,PROJECT,native]).unwrap();
        let mut lines = vec![
            json!({"type":"session_meta","payload":{"id":native,"cwd":repo,"source":"cli"}}),
            json!({"type":"turn_context","payload":{"turn_id":"turn-one"}}),
        ];
        for mut item in items {
            if let Some(payload) = item.get_mut("payload") {
                if let Some(arguments) = payload.get_mut("arguments") {
                    if let Some(s) = arguments.as_str() {
                        *arguments = Value::String(
                            s.replace("$REPO", &repo.to_string_lossy())
                                .replace("$VERSION", &version),
                        );
                    }
                }
                if let Some(output) = payload.get_mut("output") {
                    if let Some(s) = output.as_str() {
                        *output = Value::String(s.replace("$VERSION", &version));
                    }
                }
            }
            lines.push(item);
        }
        let lines: Vec<String> = lines.iter().map(Value::to_string).collect();
        fs::write(&raw, format!("{}\n", lines.join("\n"))).unwrap();
        let boundary = sha256_hex(lines[1].as_bytes());
        let mut previous = genesis_prev_hash(PROJECT);
        for (i, line) in lines.iter().enumerate() {
            let rawhash = sha256_hex(line.as_bytes());
            let value = serde_json::from_str(line).unwrap();
            let context = CodexLineContext {
                owner_session_id: &session,
                source_session_id: native,
                file_id: &session,
                turn_scope: if i == 0 { "pre-turn" } else { "turn:turn-one" },
                turn_boundary_line: (i > 0).then_some(2),
                turn_boundary_sha256: (i > 0).then_some(boundary.as_str()),
                line: i as u64 + 1,
                raw_sha256: &rawhash,
            };
            let mapped = normalize_codex_line(context, &value).unwrap();
            assert_eq!(mapped.events.len(), 1);
            let e = &mapped.events[0].event;
            let seq = i as i64 + 1;
            let hash = chain_hash(&previous, &rawhash, &session, seq);
            ledger.conn().execute("INSERT INTO events(id,project_id,session_id,seq,kind,role,text,tool_name,source_tool_name,tool_use_id,tool_input,tool_result,is_error,is_sidechain,source_turn_id,line_session_id,source_turn_boundary_line,source_turn_boundary_sha256,raw_file,raw_line,raw_sha256,prev_hash,chain_hash) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23)",params![format!("event-{seq}"),PROJECT,session,seq,e.kind.as_str(),e.role,e.text,e.tool_name,e.source_tool_name,e.tool_use_id,e.tool_input,e.tool_result,i64::from(e.is_error),i64::from(e.is_sidechain),e.source_turn_id,e.line_session_id,e.source_turn_boundary_line.map(|x|x as i64),e.source_turn_boundary_sha256,raw.to_string_lossy(),seq,rawhash,previous,hash]).unwrap();
            previous = hash;
        }
        Self {
            temp,
            ledger,
            repo,
            version,
            tree,
            raw,
            session,
        }
    }
}
pub fn prompt(text: &str) -> Value {
    json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":text}]}})
}
pub fn prose(text: &str) -> Value {
    json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":text}]}})
}
pub fn call(id: &str, command: &str) -> Value {
    json!({"type":"response_item","payload":{"type":"function_call","name":"exec_command","call_id":id,"arguments":json!({"cmd":command,"workdir":"$REPO"}).to_string()}})
}
pub fn edit(id: &str) -> Value {
    json!({"type":"response_item","payload":{"type":"custom_tool_call","name":"apply_patch","call_id":id,"input":"*** Begin Patch\n*** Update File: REQUIRED\n@@\n-ready\n+done\n*** End Patch"}})
}
pub fn output(id: &str, body: &str, code: i32) -> Value {
    json!({"type":"response_item","payload":{"type":"function_call_output","call_id":id,"output":format!("Process exited with code {code}\nFinal output:\n{body}")}})
}
