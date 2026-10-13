use crate::fixture::{absence, commit, Fixture};
use serde_json::{json, Value};
use thruline_core::{
    chain::sha256_hex,
    decisions::{
        engine,
        revival::{self, RevivalReport},
    },
    model::RawLineRef,
    parse::{parse_reader, ParseStats},
};

pub const BEFORE: &str = "// café\nconst ACTIVE: bool = false;\n";
pub const AFTER: &str = "// café\nconst OBSOLETE: bool = false;\n";

pub fn edit(
    before: &str,
    old: &str,
    new: &str,
    replace_all: bool,
    path: &std::path::Path,
) -> (Value, Value) {
    (
        json!({"file_path":path,"old_string":old,"new_string":new,"replace_all":replace_all}),
        json!({"filePath":path,"oldString":old,"newString":new,"originalFile":before}),
    )
}
pub fn write(content: &str, kind: &str, path: &std::path::Path) -> (Value, Value) {
    (
        json!({"file_path":path,"content":content}),
        json!({"type":kind,"filePath":path,"content":content}),
    )
}
pub fn setup(
    before: Option<&str>,
    after: &str,
    tool: &str,
    make: impl FnOnce(&std::path::Path) -> (Value, Value),
    mutate: impl FnOnce(&mut Vec<Value>),
) -> Fixture {
    setup_with_parent(before, after, tool, make, mutate, |_| {})
}

pub fn setup_with_parent(
    before: Option<&str>,
    after: &str,
    tool: &str,
    make: impl FnOnce(&std::path::Path) -> (Value, Value),
    mutate: impl FnOnce(&mut Vec<Value>),
    parent: impl FnOnce(&Fixture),
) -> Fixture {
    let mut f = Fixture::new();
    std::fs::write(
        f.root().join("retained.rs"),
        "const RETAINED: bool = true;\n",
    )
    .unwrap();
    match before {
        Some(text) => std::fs::write(f.root().join("app.rs"), text).unwrap(),
        None => std::fs::remove_file(f.root().join("app.rs")).unwrap(),
    }
    parent(&f);
    f.version = commit(f.root());
    f.decision("p", absence(), true);
    engine::check_project_version(
        &f.ledger,
        "p",
        f.root(),
        &f.version,
        "2026-10-08T00:01:00Z",
        false,
    )
    .unwrap();
    let app = f.root().join("app.rs");
    if std::fs::symlink_metadata(&app).is_ok_and(|m| m.file_type().is_symlink()) {
        std::fs::remove_file(&app).unwrap();
    }
    std::fs::write(&app, after).unwrap();
    f.version = commit(f.root());
    let path = f.root().join("app.rs");
    let (input, echo) = make(&path);
    let root = format!(
        "'{}'",
        f.root().display().to_string().replace('\'', "'\\''")
    );
    let mut rows = vec![
        json!({"type":"assistant","sessionId":"agent","message":{"role":"assistant","content":[{"type":"tool_use","id":"change","name":tool,"input":input}]}}),
        json!({"type":"user","sessionId":"agent","toolUseResult":echo,"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"change","is_error":false,"content":"completed"}]}}),
        json!({"type":"assistant","sessionId":"agent","message":{"role":"assistant","content":[{"type":"tool_use","id":"head","name":"Bash","input":{"command":format!("git -C {root} rev-parse HEAD")}}]}}),
        json!({"type":"user","sessionId":"agent","toolUseResult":{"stdout":f.version,"stderr":"","exitCode":0},"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"head","is_error":false,"content":f.version}]}}),
    ];
    mutate(&mut rows);
    capture(&mut f, "agent", rows);
    engine::check_project_version(
        &f.ledger,
        "p",
        f.root(),
        &f.version,
        "2026-10-08T00:03:00Z",
        false,
    )
    .unwrap();
    f
}
pub fn capture(f: &mut Fixture, session: &str, mut rows: Vec<Value>) {
    for row in &mut rows {
        row["sessionId"] = json!(session);
    }
    let path = f.root().join(".git").join(format!("{session}.jsonl"));
    let bytes = rows
        .iter()
        .map(|row| format!("{row}\n"))
        .collect::<String>();
    std::fs::write(&path, &bytes).unwrap();
    f.ledger.conn().execute("INSERT INTO sessions(id,project_id,file_path,started_at) VALUES(?1,'p',?2,'2026-10-08T00:02:00Z')",rusqlite::params![session,path.to_string_lossy()]).unwrap();
    let parsed = parse_reader(
        bytes.as_bytes(),
        path.display().to_string(),
        &mut ParseStats::default(),
    )
    .unwrap();
    let normalized = parsed
        .into_iter()
        .enumerate()
        .flat_map(|(i, line)| {
            line.events.into_iter().map({
                let path = &path;
                let rows = &rows;
                move |event| {
                    (
                        RawLineRef {
                            file: path.to_string_lossy().into_owned(),
                            line: i as u64 + 1,
                            sha256: sha256_hex(rows[i].to_string().as_bytes()),
                        },
                        event,
                    )
                }
            })
        })
        .collect::<Vec<_>>();
    f.ledger
        .append_session_events("p", session, &normalized)
        .unwrap();
}
pub fn report(f: &Fixture) -> RevivalReport {
    let before = counts(f);
    let result = revival::project_revivals(&f.ledger, "p", f.root(), &f.version).unwrap();
    assert_eq!(before, counts(f), "attribution is read-only");
    assert!(
        !result.revivals.is_empty(),
        "the current finding must remain visible"
    );
    result
}
fn counts(f: &Fixture) -> (i64, i64, i64) {
    f.ledger.conn().query_row("SELECT (SELECT count(*) FROM decision_person_acts),(SELECT count(*) FROM decision_bridge_proofs),(SELECT count(*) FROM decision_verdicts)",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap()
}
