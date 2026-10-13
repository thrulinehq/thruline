//! UNRECORDED-WORK-NO-QUOTE — a Change never shows the agent's own words from
//! a log ThruLine was not recording, on any surface a person or a program
//! reads. It says where in that log the agent said it (a file and a line) and
//! nothing of what it said.
//!
//! One store, made with the real ingest: two conversations linked to one
//! Change, neither recorded by the execution store. Each log holds a plain
//! canary sentence and a credential split into pieces that an earlier
//! word-by-word gate admitted; a third piece of the credential sits in an
//! earlier turn. The whole thing runs through the shipping binary, the served
//! routes, the command line and the MCP server.
//!
//! Not reached here, on purpose: `/fixtures/…` and `thruline export`, the
//! project graph's own reading. Its claim receipts carry the claim text by
//! design, they are not the Change, and they are a different card.
//!
//! Every id is synthetic (`cccccccc-` prefix) and every key piece is an
//! invented string that opens nothing.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use rusqlite::params;
use thruline_core::change::{
    append_association, AssociationBasis, AssociationProvenance, NewAssociation,
};
use thruline_core::ledger::Ledger;

const CHANGE: &str = "cccccccc-0000-4000-8000-00000000c0d1";
const FIRST: &str = "cccccccc-0000-4000-8000-00000000a0d1";
const SECOND: &str = "cccccccc-0000-4000-8000-00000000a0d2";

/// What the agent's final message says in the first log: the canary, and a
/// credential split into backtick-wrapped pieces.
const FIRST_FINAL: &str = "Done. The tests pass. The signing key is `zqvx` `krmt` `wbnj` `hcsf` `plmg`. The quokka cart total is added and the tests pass.";
/// The second log's final message: another canary, another split.
const SECOND_FINAL: &str = "Done. The tests pass. The signing key is `zqvxa`, `krmta`, `wbnja`, `hcsfa`. The wallaby cart total is added and the tests pass.";
/// An earlier turn of the first log, in the grouped-key layout.
const EARLIER_TURN: &str = "Rotated the fixture key QX72-M9ZP-W4LK-R8NV for the demo.";

/// Everything only the logs hold. None of it may appear on any surface.
const WORDS_OF_THE_LOGS: [&str; 16] = [
    "quokka",
    "wallaby",
    "cart total is added",
    "zqvx",
    "krmt",
    "wbnj",
    "hcsf",
    "plmg",
    "zqvxa",
    "krmta",
    "wbnja",
    "hcsfa",
    "QX72",
    "M9ZP",
    "W4LK",
    "R8NV",
];

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

struct Store {
    dir: tempfile::TempDir,
    db: PathBuf,
    project: String,
}

impl Store {
    fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }

    fn log(&self, session: &str) -> PathBuf {
        self.dir
            .path()
            .join("projects")
            .join("-synthetic-shop")
            .join(format!("{session}.jsonl"))
    }
}

fn line(
    session: &str,
    n: u32,
    kind: &str,
    role: &str,
    content: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "type": kind,
        "uuid": format!("cccccccc-0000-4000-8000-{}{n:02}", &session[session.len() - 10..]),
        "isSidechain": false,
        "sessionId": session,
        "cwd": "/synthetic/shop",
        "timestamp": format!("2026-09-23T09:00:{n:02}.000Z"),
        "message": { "role": role, "content": content },
    })
}

fn text(session: &str, n: u32, words: &str) -> serde_json::Value {
    line(
        session,
        n,
        "assistant",
        "assistant",
        serde_json::json!([{ "type": "text", "text": words }]),
    )
}

fn user(session: &str, n: u32, words: &str) -> serde_json::Value {
    line(session, n, "user", "user", serde_json::json!(words))
}

fn edit(session: &str, n: u32) -> serde_json::Value {
    line(
        session,
        n,
        "assistant",
        "assistant",
        serde_json::json!([{
            "type": "tool_use", "id": "toolu_d1", "name": "Edit",
            "input": { "file_path": "/synthetic/shop/src/lib.rs", "old_string": "x", "new_string": "y" }
        }]),
    )
}

fn edited(session: &str, n: u32) -> serde_json::Value {
    line(
        session,
        n,
        "user",
        "user",
        serde_json::json!([{
            "type": "tool_result", "tool_use_id": "toolu_d1", "content": "The file has been updated."
        }]),
    )
}

fn write_log(path: &Path, lines: &[serde_json::Value]) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    let body: Vec<String> = lines.iter().map(ToString::to_string).collect();
    std::fs::write(path, body.join("\n") + "\n").expect("log");
}

/// Two conversations, ingested for real, linked to one Change whose scope is
/// accepted. Nothing was ever recorded by the execution store.
fn store() -> Store {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".thruline")).expect("home");
    let db = dir.path().join("ledger.db");

    // FIRST: the final message is on line 6, after an earlier turn.
    write_log(
        &dir.path()
            .join("projects/-synthetic-shop")
            .join(format!("{FIRST}.jsonl")),
        &[
            user(FIRST, 1, "Add a cart total function."),
            text(FIRST, 2, EARLIER_TURN),
            user(FIRST, 3, "Now finish it."),
            edit(FIRST, 4),
            edited(FIRST, 5),
            text(FIRST, 6, FIRST_FINAL),
        ],
    );
    // SECOND: the walk's four-line shape, the final message on line 4.
    write_log(
        &dir.path()
            .join("projects/-synthetic-shop")
            .join(format!("{SECOND}.jsonl")),
        &[
            user(SECOND, 1, "Show the cart total."),
            edit(SECOND, 2),
            edited(SECOND, 3),
            text(SECOND, 4, SECOND_FINAL),
        ],
    );

    let mut ledger = Ledger::open(&db).expect("open");
    thruline_core::ingest::ingest(
        &mut ledger,
        &[dir.path().join("projects")],
        &dir.path().join("mirrors"),
    )
    .expect("ingest");
    let project: String = ledger
        .conn()
        .query_row(
            "SELECT project_id FROM events WHERE session_id=?1 LIMIT 1",
            params![FIRST],
            |r| r.get(0),
        )
        .expect("the first conversation was ingested");
    ledger
        .conn()
        .execute(
            "INSERT INTO changes(id, project_id, opened_at, title, purpose, request_event_id,
                                 requester, request_source)
             VALUES(?1, ?2, '2026-09-22T08:00:00Z', 'Totals add up', 'They do not.', NULL,
                    'sam', 'person')",
            params![CHANGE, project],
        )
        .expect("change");
    ledger
        .conn()
        .execute(
            "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at, criteria_json,
                                                exclusions_json, allowed_scope_json, constraints_json,
                                                check_profile_json, content_sha256)
             VALUES(?1, 1, '2026-09-22T08:30:00Z', '[\"The totals add up\"]', '[]', '[\"checkout/\"]',
                    '[]', '[]', 'deadbeef')",
            params![CHANGE],
        )
        .expect("scope");
    ledger
        .conn()
        .execute(
            "INSERT INTO change_scope_acceptances(change_id, scope_version, accepted_by, accepted_at,
                                                  rationale)
             VALUES(?1, 1, 'sam', '2026-09-22T09:00:00Z', '')",
            params![CHANGE],
        )
        .expect("acceptance");
    for (n, session) in [FIRST, SECOND].into_iter().enumerate() {
        append_association(
            &ledger,
            &NewAssociation {
                id: format!("cccccccc-0000-4000-8000-00000000a5{n:02}"),
                change_id: CHANGE.to_string(),
                session_id: session.to_string(),
                provenance: AssociationProvenance::Confirmed,
                basis: AssociationBasis::User,
                actor: Some("sam".to_string()),
                asserted_at: "2026-09-23T10:00:00Z".to_string(),
                reason: None,
            },
        )
        .expect("confirm");
    }
    drop(ledger);
    Store { dir, db, project }
}

/// The shipping binary with nothing inherited but what it needs.
fn thruline(store: &Store) -> Command {
    let mut command = Command::new(bin());
    command
        .env_clear()
        .env("HOME", store.home())
        .env("THRULINE_HOME", store.home().join(".thruline"))
        .env("PATH", "/usr/bin:/bin")
        .env("USER", "walker")
        .env("LOGNAME", "walker")
        .current_dir(store.home())
        .arg("--db")
        .arg(&store.db);
    command
}

struct Server {
    child: Child,
    port: u16,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    fn start(store: &Store) -> Server {
        let mut child = thruline(store)
            .args(["ui", "--no-open", "--port", "0"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("thruline ui");
        let stdout = child.stdout.take().expect("stdout");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { return };
                if tx.send(line).is_err() {
                    return;
                }
            }
        });
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(line) => {
                    if let Some(rest) = line.trim().strip_prefix("ThruLine is at http://127.0.0.1:")
                    {
                        let port: u16 = rest
                            .trim()
                            .split('/')
                            .next()
                            .expect("port")
                            .parse()
                            .expect("port in the printed URL");
                        return Server { child, port };
                    }
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let mut err = String::new();
                    if let Some(mut e) = child.stderr.take() {
                        let _ = e.read_to_string(&mut err);
                    }
                    panic!("`thruline ui` never printed a loopback URL.\n{err}");
                }
            }
        }
    }

    fn get(&self, path: &str) -> (u16, String) {
        let mut socket = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            self.port
        );
        socket.write_all(request.as_bytes()).expect("request");
        let mut raw = Vec::new();
        socket.read_to_end(&mut raw).expect("response");
        let text = String::from_utf8_lossy(&raw).to_string();
        let status: u16 = text
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .expect("status");
        let body = text
            .split_once("\r\n\r\n")
            .map(|(_, b)| b.to_string())
            .unwrap_or_default();
        (status, body)
    }
}

/// Which words of the logs `surface` carries.
fn leaks(surface: &str) -> Vec<&'static str> {
    WORDS_OF_THE_LOGS
        .iter()
        .copied()
        .filter(|w| surface.contains(w))
        .collect()
}

fn assert_none_of_the_logs(label: &str, surface: &str) {
    let found = leaks(surface);
    assert!(
        found.is_empty(),
        "{label} carries words of the agent's log: {found:?}\n{surface}"
    );
}

/// Every read route that can name the Change or either conversation.
fn routes() -> Vec<String> {
    let mut all = vec![
        "/api/changes".to_string(),
        format!("/api/changes/{CHANGE}"),
        format!("/api/changes/{CHANGE}/proposals"),
        format!("/api/changes/{CHANGE}/coverage"),
        format!("/api/changes/{CHANGE}/applicability"),
        format!("/api/changes/{CHANGE}/readiness"),
        format!("/api/changes/{CHANGE}/findings"),
        format!("/api/changes/{CHANGE}/inputs"),
        format!("/api/changes/{CHANGE}/export"),
        "/api/sessions".to_string(),
        "/api/state".to_string(),
    ];
    for session in [FIRST, SECOND] {
        all.push(format!("/api/sessions/{session}/changes"));
        all.push(format!("/api/explain?session={session}"));
        all.push(format!("/api/claims?session={session}"));
        all.push(format!("/api/phases?session={session}"));
        all.push(format!("/api/read?session={session}"));
    }
    all
}

#[test]
fn no_served_route_carries_a_word_of_the_agents_log() {
    let store = store();
    let server = Server::start(&store);
    let mut change_view = String::new();
    for route in routes() {
        let (status, body) = server.get(&route);
        assert_eq!(status, 200, "{route}: {body}");
        assert_none_of_the_logs(&route, &body);
        if route == format!("/api/changes/{CHANGE}") {
            change_view = body;
        }
    }

    // What the Change route serves instead: where each conversation's final
    // message is in its own log, and nothing of what it says.
    let body: serde_json::Value = serde_json::from_str(&change_view).expect("json");
    assert!(
        body.get("agent_said").is_none(),
        "the route still serves a field named for the agent's words: {body}"
    );
    let sessions = body["agent_log"]["sessions"]
        .as_array()
        .expect("agent_log.sessions");
    assert_eq!(sessions.len(), 2, "{body}");
    for (session, line) in [(FIRST, 6), (SECOND, 4)] {
        let entry = sessions
            .iter()
            .find(|e| e["session_id"] == session)
            .unwrap_or_else(|| panic!("no entry for {session}: {body}"));
        assert_eq!(
            entry["receipt"]["file"],
            store.log(session).to_string_lossy().as_ref(),
            "{entry}"
        );
        assert_eq!(entry["receipt"]["line"], line, "{entry}");
        assert!(entry["not_located"].is_null(), "{entry}");
        let keys: Vec<&String> = entry["receipt"]
            .as_object()
            .expect("object")
            .keys()
            .collect();
        assert_eq!(
            keys,
            ["file", "line"],
            "a receipt is a file and a line, and no more"
        );
    }
}

#[test]
fn neither_the_command_line_nor_the_mcp_server_carries_a_word_of_the_agents_log() {
    let store = store();
    for (label, args) in [
        (
            "change results",
            vec!["change", "results", CHANGE, "--json"],
        ),
        (
            "change links (first)",
            vec!["change", "links", "--session", FIRST, "--json"],
        ),
        (
            "change links (second)",
            vec!["change", "links", "--session", SECOND, "--json"],
        ),
        (
            "explain (first)",
            vec!["explain", "--session", FIRST, "--json"],
        ),
        (
            "explain (second)",
            vec!["explain", "--session", SECOND, "--json"],
        ),
        (
            "claims (first)",
            vec!["claims", "--session", FIRST, "--json"],
        ),
        (
            "claims (second)",
            vec!["claims", "--session", SECOND, "--json"],
        ),
    ] {
        let out = thruline(&store).args(&args).output().expect("thruline");
        let all = format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_none_of_the_logs(label, &all);
    }

    let mut child = thruline(&store)
        .args(["mcp", "--agent", "probe"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("thruline mcp");
    let calls = [
        serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
            "name": "get_context_packet", "arguments": { "project": store.project }
        }}),
    ];
    {
        let mut stdin = child.stdin.take().expect("stdin");
        for call in &calls {
            writeln!(stdin, "{call}").expect("call");
        }
    }
    let out = child.wait_with_output().expect("mcp output");
    let all = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        all.contains("chain_position"),
        "the packet call did not answer: {all}"
    );
    assert_none_of_the_logs("mcp", &all);
}
