//! AGENT-LOG-LOCATOR-BOUNDED — the Change route never waits on a log path that
//! is not an ordinary file.
//!
//! `GET /api/changes/<id>` says, for each linked conversation, where its own
//! log holds the agent's final claim. It used to open whatever the ledger's
//! recorded path named, for reading, on every request: with that path replaced
//! by a named pipe, the request was never answered and the local server was
//! wedged for as long as the pipe had no writer. Here the real binary serves a
//! Change two of whose conversations are linked, one whose log path has become
//! something that is not an ordinary file (a named pipe, a socket, a directory,
//! a device) and one whose log is fine. The route answers within a bound, says
//! for the first that there is no place and why, and still places the second.
//!
//! Every id is synthetic (`cccccccc-` prefix) and every line of every log is
//! invented.

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

const CHANGE: &str = "cccccccc-0000-4000-8000-00000000c0e1";
/// The conversation whose log path is replaced.
const SPECIAL: &str = "cccccccc-0000-4000-8000-00000000f0a1";
/// The conversation whose log stays an ordinary file.
const ORDINARY: &str = "cccccccc-0000-4000-8000-00000000f0b1";
const WORDS: &str = "Done. The cart total is added and the tests pass.";

/// How long the route may take before a silent server is called wedged. The
/// request takes milliseconds; nothing short of a wait that never ends crosses
/// this.
const ANSWER_BOUND: Duration = Duration::from_secs(30);

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

struct Store {
    dir: tempfile::TempDir,
    db: PathBuf,
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

fn line(session: &str, n: u32, kind: &str, role: &str, content: serde_json::Value) -> String {
    serde_json::json!({
        "type": kind,
        "uuid": format!("cccccccc-0000-4000-8000-{}{n:02}", &session[session.len() - 10..]),
        "isSidechain": false,
        "sessionId": session,
        "cwd": "/synthetic/shop",
        "timestamp": format!("2026-09-23T09:00:{n:02}.000Z"),
        "message": { "role": role, "content": content },
    })
    .to_string()
}

/// The walk's conversation: the final message is on line 4.
fn walk(session: &str) -> Vec<String> {
    vec![
        line(
            session,
            1,
            "user",
            "user",
            serde_json::json!("Add a cart total function."),
        ),
        line(
            session,
            2,
            "assistant",
            "assistant",
            serde_json::json!([{ "type": "tool_use", "id": "toolu_e1", "name": "Edit",
                "input": { "file_path": "/synthetic/shop/src/lib.rs", "old_string": "x", "new_string": "y" } }]),
        ),
        line(
            session,
            3,
            "user",
            "user",
            serde_json::json!([{ "type": "tool_result", "tool_use_id": "toolu_e1",
                "content": "The file has been updated." }]),
        ),
        line(
            session,
            4,
            "assistant",
            "assistant",
            serde_json::json!([{ "type": "text", "text": WORDS }]),
        ),
    ]
}

/// Two conversations, ingested for real and confirmed into one Change.
fn store() -> Store {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".thruline")).expect("home");
    let db = dir.path().join("ledger.db");
    let project = dir.path().join("projects/-synthetic-shop");
    std::fs::create_dir_all(&project).expect("mkdir");
    for session in [SPECIAL, ORDINARY] {
        std::fs::write(
            project.join(format!("{session}.jsonl")),
            walk(session).join("\n") + "\n",
        )
        .expect("log");
    }
    let mut ledger = Ledger::open(&db).expect("open");
    thruline_core::ingest::ingest(
        &mut ledger,
        &[dir.path().join("projects")],
        &dir.path().join("mirrors"),
    )
    .expect("ingest");
    let project_id: String = ledger
        .conn()
        .query_row(
            "SELECT project_id FROM events WHERE session_id=?1 LIMIT 1",
            params![SPECIAL],
            |r| r.get(0),
        )
        .expect("the conversation was ingested");
    ledger
        .conn()
        .execute(
            "INSERT INTO changes(id, project_id, opened_at, title, purpose, request_event_id,
                                 requester, request_source)
             VALUES(?1, ?2, '2026-09-22T08:00:00Z', 'Totals add up', 'They do not.', NULL,
                    'sam', 'person')",
            params![CHANGE, project_id],
        )
        .expect("change");
    for (n, session) in [SPECIAL, ORDINARY].into_iter().enumerate() {
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
    Store { dir, db }
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
        let mut child = Command::new(bin())
            .env_clear()
            .env("HOME", store.home())
            .env("THRULINE_HOME", store.home().join(".thruline"))
            .env("PATH", "/usr/bin:/bin")
            .env("USER", "walker")
            .env("LOGNAME", "walker")
            .current_dir(store.home())
            .arg("--db")
            .arg(&store.db)
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

    /// One GET, answered within [`ANSWER_BOUND`] or not at all.
    fn get(&self, path: &str) -> (u16, String) {
        let mut socket = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        socket
            .set_read_timeout(Some(ANSWER_BOUND))
            .expect("read timeout");
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            self.port
        );
        socket.write_all(request.as_bytes()).expect("request");
        let started = Instant::now();
        let mut raw = Vec::new();
        if let Err(e) = socket.read_to_end(&mut raw) {
            panic!(
                "GET {path} was not answered within {ANSWER_BOUND:?} ({e}; {} bytes arrived after \
                 {:?}): the route is waiting on something the request should never wait on",
                raw.len(),
                started.elapsed()
            );
        }
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

/// Serve the Change with `SPECIAL`'s log replaced by whatever `replace` makes
/// of its path. The route must answer, say that conversation has no place and
/// why, never serve a word of anything, and still place the other one.
fn assert_route_answers(replace: impl FnOnce(&Path)) {
    let store = store();
    let server = Server::start(&store);
    // Replaced once the server is up, the way a log goes away under a running
    // HQ. (Start-up itself reads the recorded transcripts to build the project
    // graph, which is not this route and not this card.)
    let log = store.log(SPECIAL);
    std::fs::remove_file(&log).expect("remove the log");
    replace(&log);

    let started = Instant::now();
    let (status, body) = server.get(&format!("/api/changes/{CHANGE}"));
    assert_eq!(status, 200, "{body}");
    assert!(
        started.elapsed() < ANSWER_BOUND,
        "the route took {:?}",
        started.elapsed()
    );
    let body: serde_json::Value = serde_json::from_str(&body).expect("json");
    let sessions = body["agent_log"]["sessions"]
        .as_array()
        .expect("agent_log.sessions");
    assert_eq!(sessions.len(), 2, "{body}");
    let special = sessions
        .iter()
        .find(|e| e["session_id"] == SPECIAL)
        .expect("an entry for the replaced log");
    assert!(special["receipt"].is_null(), "{special}");
    assert_eq!(special["not_located"], "log_unreadable", "{special}");
    let ordinary = sessions
        .iter()
        .find(|e| e["session_id"] == ORDINARY)
        .expect("an entry for the ordinary log");
    assert_eq!(
        ordinary["receipt"]["file"],
        store.log(ORDINARY).to_string_lossy().as_ref(),
        "{ordinary}"
    );
    assert_eq!(ordinary["receipt"]["line"], 4, "{ordinary}");
    assert!(ordinary["not_located"].is_null(), "{ordinary}");

    // And it keeps answering: a second request is the same answer.
    let (again_status, again) = server.get(&format!("/api/changes/{CHANGE}"));
    assert_eq!(again_status, 200);
    let again: serde_json::Value = serde_json::from_str(&again).expect("json");
    assert_eq!(again["agent_log"], body["agent_log"]);
}

#[test]
fn the_route_answers_when_a_log_path_has_become_a_named_pipe() {
    assert_route_answers(|log| {
        let made = Command::new("mkfifo").arg(log).status().expect("mkfifo");
        assert!(made.success(), "mkfifo failed");
    });
}

#[test]
fn the_route_answers_when_a_log_path_has_become_a_socket() {
    // A socket's name has to be short and the log's is not: bind it under a
    // short name and move it into place. The listener lives as long as the
    // request does.
    let short = tempfile::Builder::new()
        .prefix("s")
        .tempdir()
        .expect("short dir");
    let at = short.path().join("s");
    let listener = std::os::unix::net::UnixListener::bind(&at).unwrap_or_else(|e| {
        panic!(
            "binding a socket at {}: {e} (is TMPDIR too long for a socket's name?)",
            at.display()
        )
    });
    assert_route_answers(|log| std::fs::rename(&at, log).expect("move the socket into place"));
    drop(listener);
}

#[test]
fn the_route_answers_when_a_log_path_has_become_a_directory() {
    assert_route_answers(|log| std::fs::create_dir(log).expect("mkdir"));
}

#[test]
fn the_route_answers_when_a_log_path_has_become_a_device() {
    // `/dev/null` is a character device and is always empty.
    assert_route_answers(|log| std::os::unix::fs::symlink("/dev/null", log).expect("symlink"));
}
