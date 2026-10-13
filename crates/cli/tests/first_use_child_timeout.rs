//! SNAPSHOT-CHILD-TIMEOUT (D-051): the first-use Change write, when `git` or
//! `ps` does not answer.
//!
//! WHAT THIS FILE IS FOR. `POST /api/first-use/change` holds the browser's
//! request open while it takes a workspace snapshot, and the snapshot runs
//! `git` and `ps`. Before this card, a child that stalled stalled the request:
//! four `first_use_cli` tests met a client read timeout in one gate
//! (ADV-t71-1, `first_use_cli.rs:192`). Each test here puts a stand-in for one
//! program first on the server's `PATH`, inside the test's own temporary
//! directory, and asserts three things against the shipping binary:
//!
//! 1. the route answers within the snapshot's time limit;
//! 2. what was not read is recorded as absent, never as a value; and
//! 3. the stand-in and the process it started are gone afterwards.
//!
//! A third test runs with no stand-in and pins that a normal run stores
//! exactly the shape it stored before.
//!
//! EVERY FIXTURE IS SYNTHETIC, as in `first_use_cli.rs`, whose helpers this
//! file repeats in the smallest form it needs.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn thruline() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

/// The snapshot's time limit (`thruline_core::bounded_child::TIME_LIMIT`) is
/// 20 s. The route is allowed that plus a margin for the rest of its work; the
/// client waits longer than both, so a stall shows as a slow answer measured
/// here rather than as a socket error.
const ROUTE_MUST_ANSWER_WITHIN: Duration = Duration::from_secs(40);
const CLIENT_WAITS: Duration = Duration::from_secs(50);

/// How long each stand-in stalls. Longer than the client waits, so a server
/// that waits for the child is caught.
const STALL_SECS: u32 = 90;

struct Machine {
    dir: tempfile::TempDir,
}

impl Machine {
    fn new() -> Machine {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("h/.thruline")).expect("home");
        std::fs::create_dir_all(dir.path().join("repo/src")).expect("repo");
        std::fs::create_dir_all(dir.path().join("repo/.git")).expect("git");
        std::fs::create_dir_all(dir.path().join("bin")).expect("bin");
        Machine { dir }
    }

    fn home(&self) -> PathBuf {
        self.dir.path().join("h")
    }

    fn repo(&self) -> PathBuf {
        self.dir.path().join("repo")
    }

    fn bin(&self) -> PathBuf {
        self.dir.path().join("bin")
    }

    fn pids(&self, program: &str) -> PathBuf {
        self.dir.path().join(format!("{program}.pids"))
    }

    fn ledger(&self) -> PathBuf {
        self.home().join(".thruline/ledger.db")
    }

    /// A stand-in for `program`, first on the server's `PATH`. When its
    /// arguments contain `stall_on`, it writes its own pid and the pid of a
    /// child it starts, and waits on that child for [`STALL_SECS`]. Any other
    /// call is handed to the real program unchanged.
    fn stand_in(&self, program: &str, real: &Path, stall_on: &str) {
        let script = format!(
            "#!/bin/sh\ncase \" $* \" in\n  *\" {stall_on} \"*)\n    echo $$ >> '{pids}'\n    sleep {STALL_SECS} &\n    echo $! >> '{pids}'\n    wait\n    ;;\nesac\nexec '{real}' \"$@\"\n",
            pids = self.pids(program).display(),
            real = real.display(),
        );
        let path = self.bin().join(program);
        std::fs::write(&path, script).expect("stand-in");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    fn record(&self, session: &str, asked_for: &str, touched: &str, day: &str) {
        let cwd = self.repo().display().to_string();
        let mangled: String = cwd
            .chars()
            .map(|c| match c {
                '/' | '.' | '_' | ' ' => '-',
                other => other,
            })
            .collect();
        let dir = self.home().join(".claude/projects").join(mangled);
        std::fs::create_dir_all(&dir).expect("project dir");
        let lines = [
            format!(
                r#"{{"type":"user","uuid":"{session}-e1","parentUuid":null,"isSidechain":false,"sessionId":"{session}","timestamp":"{day}T09:00:00.100Z","cwd":"{cwd}","message":{{"role":"user","content":"{asked_for}"}}}}"#
            ),
            format!(
                r#"{{"type":"assistant","uuid":"{session}-e2","parentUuid":"{session}-e1","isSidechain":false,"sessionId":"{session}","timestamp":"{day}T09:01:00.100Z","cwd":"{cwd}","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"t1","name":"Edit","input":{{"file_path":"{cwd}/{touched}","old_string":"a","new_string":"b"}}}}]}}}}"#
            ),
        ];
        std::fs::write(
            dir.join(format!("{session}.jsonl")),
            lines.join("\n") + "\n",
        )
        .expect("recording");
    }

    fn ui(&self) -> Command {
        let path = format!(
            "{}:{}",
            self.bin().display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut cmd = Command::new(thruline());
        cmd.args(["ui", "--no-open", "--port", "0"])
            .env("HOME", self.home())
            .env("THRULINE_HOME", self.home().join(".thruline"))
            .env("PATH", path)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .current_dir(self.repo());
        cmd
    }
}

/// The real program, found on this process's own `PATH`.
fn real(program: &str) -> PathBuf {
    let path = std::env::var_os("PATH").expect("PATH");
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|p| p.is_file())
        .unwrap_or_else(|| panic!("{program} is not on PATH"))
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
    fn start(mut cmd: Command) -> Server {
        // Owned by a `Server` from the moment it exists, so a panic below
        // still kills and reaps it (`Drop`).
        let mut server = Server {
            child: cmd
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .expect("thruline ui"),
            port: 0,
        };
        let stdout = server.child.stdout.take().expect("stdout");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { return };
                if tx.send(line).is_err() {
                    return;
                }
            }
        });
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = rx
                .recv_timeout(left)
                .expect("`thruline ui` printed an address");
            if let Some(rest) = line.trim().strip_prefix("ThruLine is at http://127.0.0.1:") {
                server.port = rest
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .expect("port");
                return server;
            }
        }
    }

    fn request(&self, head: &str, body: &str) -> (u16, serde_json::Value) {
        let mut socket = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        socket
            .set_read_timeout(Some(CLIENT_WAITS))
            .expect("timeout");
        socket.write_all(head.as_bytes()).expect("head");
        socket.write_all(body.as_bytes()).expect("body");
        let mut raw = Vec::new();
        socket.read_to_end(&mut raw).expect("response");
        let split = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .expect("header terminator");
        let status = String::from_utf8_lossy(&raw[..split])
            .split_whitespace()
            .nth(1)
            .and_then(|v| v.parse().ok())
            .expect("status");
        let json = serde_json::from_slice(&raw[split + 4..]).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    fn get(&self, path: &str) -> serde_json::Value {
        let head = format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            self.port
        );
        self.request(&head, "").1
    }

    fn post(&self, path: &str, body: &str) -> (u16, serde_json::Value) {
        let head = format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{p}\r\nOrigin: http://127.0.0.1:{p}\r\nContent-Type: application/json\r\nContent-Length: {n}\r\nConnection: close\r\n\r\n",
            p = self.port,
            n = body.len()
        );
        self.request(&head, body)
    }
}

/// Pick the machine's one project, then write the Change, timing that write.
fn write_the_change(machine: &Machine, server: &Server) -> (String, Duration) {
    let doc = server.get("/api/first-use");
    let here = machine.repo().display().to_string();
    let key = doc["candidates"]
        .as_array()
        .expect("candidates")
        .iter()
        .find(|c| c["label"] == here)
        .and_then(|c| c["key"].as_str())
        .unwrap_or_else(|| panic!("no candidate for {here}: {doc}"))
        .to_string();
    let (status, picked) = server.post("/api/first-use/register", &format!(r#"{{"key":"{key}"}}"#));
    assert_eq!(status, 200, "{picked}");
    let project = picked["project"].as_str().expect("project").to_string();
    let started = Instant::now();
    let (status, written) = server.post(
        "/api/first-use/change",
        &format!(r#"{{"project":"{project}"}}"#),
    );
    let took = started.elapsed();
    assert_eq!(status, 200, "{written}");
    let id = written["change"].as_str().expect("change id").to_string();
    (id, took)
}

fn machine_with_recorded_work() -> Machine {
    let machine = Machine::new();
    machine.record(
        "cccccccc-0000-4000-8000-00000000t001",
        "Fix the retry loop in the uploader. When an upload fails three times, the record shows a single failure row.",
        "src/uploader.rs",
        "2026-01-09",
    );
    machine
}

/// The stored snapshot docs, oldest first.
fn snapshot_docs(machine: &Machine) -> Vec<serde_json::Value> {
    let db = rusqlite::Connection::open_with_flags(
        machine.ledger(),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("ledger");
    let mut stmt = db
        .prepare("SELECT doc FROM workspace_snapshots ORDER BY rowid")
        .expect("select");
    let docs = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .expect("rows")
        .map(|d| serde_json::from_str(&d.expect("doc")).expect("json doc"))
        .collect();
    docs
}

/// The snapshot id the Change's `intended` record cites, `None` for SQL NULL.
fn intended_snapshot(machine: &Machine, change: &str) -> Option<String> {
    let db = rusqlite::Connection::open_with_flags(
        machine.ledger(),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("ledger");
    db.query_row(
        "SELECT snapshot_id FROM examination_inputs
          WHERE subject_kind='intended' AND subject_id=?1
          ORDER BY rowid DESC LIMIT 1",
        [change],
        |r| r.get::<_, Option<String>>(0),
    )
    .expect("the Change's intended record")
}

/// Every pid a stand-in wrote is gone: the stand-in and the child it started.
/// A grandchild that outlived its parent is reaped by the system, so the look
/// is repeated for a short while before it fails.
fn assert_gone(machine: &Machine, program: &str) {
    let text = std::fs::read_to_string(machine.pids(program))
        .unwrap_or_else(|_| panic!("the {program} stand-in was never asked to stall"));
    let pids: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(pids.len(), 2, "{program} stand-in pids: {text:?}");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let alive: Vec<&str> = pids
            .iter()
            .copied()
            .filter(|pid| {
                Command::new("kill")
                    .args(["-0", pid])
                    .stderr(Stdio::null())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
            })
            .collect();
        if alive.is_empty() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "left behind by the {program} stand-in: {alive:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// `ps` does not answer. The Change is written within the limit, the snapshot
/// is still stored, and its process list says it was not read — no digest and
/// no line count — naming the read that timed out.
#[test]
fn a_slow_ps_leaves_the_change_answered_in_time_with_the_process_list_recorded_as_not_read() {
    let machine = machine_with_recorded_work();
    machine.stand_in("ps", &real("ps"), "-axo");
    let server = Server::start(machine.ui());
    let (change, took) = write_the_change(&machine, &server);
    assert!(
        took < ROUTE_MUST_ANSWER_WITHIN,
        "the Change took {took:?} to write while ps stalled"
    );

    let docs = snapshot_docs(&machine);
    assert_eq!(docs.len(), 1, "{docs:?}");
    let doc = &docs[0];
    assert!(doc["processes"]["ps_sha256"].is_null(), "{doc}");
    assert!(
        doc["processes"]["ps_lines"].is_null(),
        "a ps that did not finish was recorded as a count: {doc}"
    );
    assert_eq!(
        doc["timed_out"],
        serde_json::json!(["ps -axo pid,ppid,stat,etime,command"]),
        "{doc}"
    );
    assert_eq!(
        intended_snapshot(&machine, &change).as_deref(),
        doc["id"].as_str(),
        "the Change's record does not cite the snapshot that was taken"
    );
    assert_gone(&machine, "ps");
}

/// `git` does not answer whether this is a worktree. The Change is written
/// within the limit, and no snapshot is stored: an empty tree or "not a git
/// worktree" would be a value standing for what was not read. The Change's
/// record says the snapshot is unknown.
#[test]
fn a_slow_git_leaves_the_change_answered_in_time_with_no_snapshot_rather_than_a_guessed_one() {
    let machine = machine_with_recorded_work();
    machine.stand_in("git", &real("git"), "--is-inside-work-tree");
    let server = Server::start(machine.ui());
    let (change, took) = write_the_change(&machine, &server);
    assert!(
        took < ROUTE_MUST_ANSWER_WITHIN,
        "the Change took {took:?} to write while git stalled"
    );

    let docs = snapshot_docs(&machine);
    assert!(
        docs.is_empty(),
        "a snapshot was stored although git never said what the folder is: {docs:?}"
    );
    assert_eq!(
        intended_snapshot(&machine, &change),
        None,
        "the Change's record names a snapshot that was never taken"
    );
    assert_gone(&machine, "git");
}

/// No stand-in. The snapshot is the one this route always stored: a process
/// list with a digest and a count, and no `timed_out` key at all.
#[test]
fn a_normal_run_stores_the_snapshot_it_always_did() {
    let machine = machine_with_recorded_work();
    let server = Server::start(machine.ui());
    let (change, _) = write_the_change(&machine, &server);
    let docs = snapshot_docs(&machine);
    assert_eq!(docs.len(), 1, "{docs:?}");
    let doc = &docs[0];
    assert!(doc["processes"]["ps_sha256"].is_string(), "{doc}");
    assert!(doc["processes"]["ps_lines"].is_u64(), "{doc}");
    assert!(
        doc.get("timed_out").is_none(),
        "a normal capture carries a timed_out key: {doc}"
    );
    assert_eq!(
        intended_snapshot(&machine, &change).as_deref(),
        doc["id"].as_str()
    );
}
