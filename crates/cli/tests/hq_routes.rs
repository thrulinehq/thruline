//! HQ Next's `/api/hq/` routes against the shipping binary: `thruline ui` with
//! a temporary HOME, synthetic Claude Code recordings, and the socket it serves.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

struct Machine {
    dir: tempfile::TempDir,
    /// The repository folder's name under the temporary folder.
    repo_name: &'static str,
}

impl Machine {
    fn new() -> Machine {
        Machine::named("repo")
    }
    fn named(repo_name: &'static str) -> Machine {
        // Under the OS temp folder, but a git repository: real work, kept.
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("h/.thruline")).expect("home");
        std::fs::create_dir_all(dir.path().join(repo_name).join(".git")).expect("repo");
        Machine { dir, repo_name }
    }
    fn home(&self) -> PathBuf {
        self.dir.path().join("h")
    }
    fn repo(&self) -> String {
        self.dir.path().join(self.repo_name).display().to_string()
    }
    fn project_dir(&self) -> PathBuf {
        let mangled: String = self
            .repo()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        self.home().join(".claude/projects").join(mangled)
    }
    fn cmd(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_thruline"));
        cmd.args(args)
            .env("HOME", self.home())
            .env("THRULINE_HOME", self.home().join(".thruline"))
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .current_dir(self.dir.path().join(self.repo_name));
        cmd
    }
    /// One recording: a prompt, an edit, a test run, a claim.
    fn record(&self, session: &str, prompt: &str, day: &str) {
        self.record_file(session, prompt, day, "src/a.rs");
    }
    /// One recording that edits `file`.
    fn record_file(&self, session: &str, prompt: &str, day: &str, file: &str) {
        let cwd = self.repo();
        std::fs::create_dir_all(self.project_dir()).expect("dir");
        let line = |n: u32, kind: &str, message: Value| {
            serde_json::json!({
                "type": kind, "uuid": format!("{session}-{n}"), "parentUuid": null,
                "isSidechain": false, "sessionId": session,
                "timestamp": format!("{day}T09:0{n}:00.000Z"), "cwd": cwd, "message": message,
            })
            .to_string()
        };
        let lines = [
            line(
                1,
                "user",
                serde_json::json!({"role": "user", "content": prompt}),
            ),
            line(
                2,
                "assistant",
                serde_json::json!({"role": "assistant", "content": [
                {"type": "tool_use", "id": format!("{session}-t1"), "name": "Edit",
                 "input": {"file_path": format!("{cwd}/{file}"), "old_string": "a", "new_string": "b"}}]}),
            ),
            line(
                3,
                "user",
                serde_json::json!({"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": format!("{session}-t1"), "content": "ok", "is_error": false}]}),
            ),
            line(
                4,
                "assistant",
                serde_json::json!({"role": "assistant", "content": [
                {"type": "text", "text": "Done. I updated `src/a.rs`."}]}),
            ),
        ];
        std::fs::write(
            self.project_dir().join(format!("{session}.jsonl")),
            lines.join("\n") + "\n",
        )
        .expect("recording");
    }
}

struct Server {
    child: Child,
    port: u16,
    /// The whole address `thruline ui` printed, fragment included.
    url: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    fn start(m: &Machine) -> Server {
        let mut child = m
            .cmd(&["ui", "--no-open", "--port", "0"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("thruline ui");
        let stdout = child.stdout.take().expect("stdout");
        for line in BufReader::new(stdout).lines() {
            let line = line.expect("line");
            if let Some(rest) = line.trim().strip_prefix("ThruLine is at http://127.0.0.1:") {
                let port = rest
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .expect("port");
                return Server {
                    child,
                    port,
                    url: line.trim().to_string(),
                };
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("`thruline ui` printed no address");
    }

    fn send(&self, head: &str) -> (u16, Value) {
        let mut s = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        s.set_read_timeout(Some(Duration::from_secs(60)))
            .expect("timeout");
        s.write_all(head.as_bytes()).expect("write");
        let mut raw = Vec::new();
        s.read_to_end(&mut raw).expect("read");
        let text = String::from_utf8_lossy(&raw);
        let status: u16 = text[9..12].parse().expect("status");
        let body = text.split_once("\r\n\r\n").map(|x| x.1).unwrap_or("");
        (status, serde_json::from_str(body).unwrap_or(Value::Null))
    }

    fn get(&self, path: &str) -> (u16, Value) {
        self.send(&format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            self.port
        ))
    }

    fn post(&self, path: &str, origin: bool) -> (u16, Value) {
        let o = if origin {
            format!("Origin: http://127.0.0.1:{}\r\n", self.port)
        } else {
            String::new()
        };
        self.send(&format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{p}\r\n{o}Content-Length: 0\r\nConnection: close\r\n\r\n",
            p = self.port
        ))
    }

    /// The project list once the start-up warm has every row.
    fn wait_for_complete_list(&self) -> Value {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let (_, list) = self.get("/api/hq/projects");
            if list["complete"] == true {
                return list;
            }
            assert!(
                Instant::now() < deadline,
                "the list never completed: {list}"
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn wait_for_load(&self) -> Value {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let (_, s) = self.get("/api/hq/load");
            if s["running"] == false && s["finished_at"].is_string() {
                return s;
            }
            assert!(Instant::now() < deadline, "load never finished: {s}");
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

fn seed(m: &Machine) {
    m.record("s1", "Fix the parser in src/a.rs.", "2026-10-01");
    let status = m
        .cmd(&["ingest", "--apply", "--source", "claude-code"])
        .arg(m.project_dir())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("ingest");
    assert!(status.success());
}

#[test]
fn every_read_route_answers_from_the_store() {
    let m = Machine::new();
    seed(&m);
    let s = Server::start(&m);
    let (code, list) = s.get("/api/hq/projects");
    assert_eq!(code, 200, "{list}");
    assert_eq!(list["schema_version"], 1);
    let projects = list["projects"].as_array().expect("projects");
    assert_eq!(projects.len(), 1);
    let id = projects[0]["id"].as_str().expect("id").to_string();
    // Served at once: either the warm row, or a quick row whose counts are
    // absent rather than zero.
    if projects[0]["complete"] == false {
        assert!(projects[0]["prompts"].is_null(), "{list}");
        assert_eq!(projects[0]["sessions"], 1, "{list}");
        assert!(projects[0]["last_activity"].is_string(), "{list}");
    }
    let list = s.wait_for_complete_list();
    assert_eq!(list["projects"][0]["prompts"], 1, "{list}");

    let (code, project) = s.get(&format!("/api/hq/projects/{id}"));
    assert_eq!(code, 200);
    let thread = &project["threads"][0];
    assert_eq!(thread["title"], "Fix the parser in src/a.rs.");
    let tid = thread["id"].as_str().expect("tid");

    let (code, view) = s.get(&format!("/api/hq/projects/{id}/threads/{tid}"));
    assert_eq!(code, 200, "{view}");
    assert_eq!(view["prompts"][0]["text"], "Fix the parser in src/a.rs.");
    assert_eq!(view["edits"][0]["file"], "src/a.rs");
    assert_eq!(view["edits"][0]["source"], "request");

    let (code, statements) = s.get(&format!("/api/hq/projects/{id}/statements"));
    assert_eq!(code, 200, "{statements}");
    assert_eq!(statements["schema_version"], 1);
    let claims = statements["claims"].as_array().expect("claims");
    assert_eq!(claims.len(), 1, "{statements}");
    assert_eq!(claims[0]["thread_id"], tid);
    assert_eq!(claims[0]["agent"], "claude_code");
    assert_eq!(claims[0]["verdict"], "recorded");
    assert!(claims[0].get("text").is_none());
    // Beside the verdict: when the run that proves it was recorded, whether a run
    // came before it, and whether the proof it rested on was moved.
    assert!(claims[0]["proven_at"].is_null());
    assert_eq!(claims[0]["run_before"], false);
    assert_eq!(claims[0]["proof_moved"], false);
    assert_eq!(statements["sessions"][0]["source"], "claude_code");
    assert_eq!(
        s.post(&format!("/api/hq/projects/{id}/statements"), true).0,
        405
    );

    let (code, timeline) = s.get("/api/hq/sessions/s1");
    assert_eq!(code, 200);
    assert_eq!(timeline["items"][0]["kind"], "prompt");

    assert_eq!(s.get("/api/hq/projects/nope").0, 404);
    assert_eq!(s.get(&format!("/api/hq/projects/{id}/threads/nope")).0, 404);
    assert_eq!(s.get("/api/hq/sessions/nope").0, 404);
    assert_eq!(s.get("/api/hq/projects?x=1").0, 400);
    assert_eq!(s.post("/api/hq/projects", true).0, 405);
    assert_eq!(s.get(&format!("/api/hq/projects/{id}/load")).0, 405);
}

#[test]
fn inbox_hide_requires_same_origin_and_rereads_one_append_only_act() {
    let m = Machine::new();
    seed(&m);
    // This route does not need the unrelated legacy export. Keep startup fast
    // while exercising the shipping server and its real scratch ledger.
    let export = m.home().join(".thruline/export");
    std::fs::create_dir_all(&export).expect("export dir");
    std::fs::write(export.join("index.json"), b"{}").expect("existing export sentinel");
    let s = Server::start(&m);
    let list = s.wait_for_complete_list();
    let id = list["projects"][0]["id"].as_str().expect("project id");
    let path = format!("/api/hq/projects/{id}/inbox-hides");
    let db = m.home().join(".thruline/ledger.db");
    let conn =
        rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("read-only scratch ledger");
    let schema = || -> i64 {
        conn.query_row("PRAGMA schema_version", [], |r| r.get(0))
            .expect("schema version")
    };
    let acts = || -> i64 {
        conn.query_row("SELECT COUNT(*) FROM inbox_hides", [], |r| r.get(0))
            .expect("hide count")
    };
    let before_schema = schema();
    let (code, before) = s.get(&path);
    assert_eq!(code, 200, "{before}");
    assert_eq!(before["rows"].as_array().map(Vec::len), Some(1), "{before}");
    assert_eq!(before["rows"][0]["hidden"], false);
    assert_eq!(
        s.get(&path),
        (200, before.clone()),
        "a no-op read is stable"
    );
    assert_eq!(acts(), 0);

    let row = &before["rows"][0];
    let body = serde_json::json!({
        "act": "hidden",
        "row_kind": row["row_kind"],
        "thread_id": row["thread_id"],
        "expected_sha256": row["row_sha256"],
    })
    .to_string();
    let (code, denied) = s.post(&path, false);
    assert_eq!(code, 403, "{denied}");
    assert_eq!(denied["error"], "same_origin_required");
    assert_eq!(acts(), 0);

    let (code, kept) = s.post_json(&path, &body);
    assert_eq!(code, 200, "{kept}");
    assert_eq!(kept["row"]["hidden"], true);
    assert_eq!(acts(), 1);
    let (code, after) = s.get(&path);
    assert_eq!(code, 200, "{after}");
    assert_eq!(after["rows"][0]["hidden"], true);
    assert_eq!(after["rows"][0]["row_sha256"], row["row_sha256"]);
    assert_eq!(after["rows"][0]["act_id"], kept["row"]["act_id"]);
    let (code, duplicate) = s.post_json(&path, &body);
    assert_eq!(code, 409, "{duplicate}");
    assert_eq!(duplicate["error"], "already_hidden");
    assert_eq!(acts(), 1, "a duplicate POST adds no act");
    assert_eq!(schema(), before_schema, "the route adds no schema");
}

#[test]
fn the_load_reads_every_recording_once_and_needs_the_same_origin() {
    let m = Machine::new();
    seed(&m);
    // Recorded after the store was seeded: only the load can bring it in.
    m.record("s2", "Add a second parser test.", "2026-10-02");
    let s = Server::start(&m);
    let (_, list) = s.get("/api/hq/projects");
    let id = list["projects"][0]["id"].as_str().expect("id").to_string();

    let (code, body) = s.post(&format!("/api/hq/projects/{id}/load"), false);
    assert_eq!(code, 403, "{body}");
    assert_eq!(body["error"], "same_origin_required");
    assert_eq!(s.post("/api/hq/projects/nope/load", true).0, 404);

    let (code, body) = s.post(&format!("/api/hq/projects/{id}/load"), true);
    assert_eq!(code, 202, "{body}");
    assert_eq!(body["files_total"], 2);
    let done = s.wait_for_load();
    assert_eq!(done["files_done"], 2);
    assert!(done["events_added"].as_u64().unwrap_or(0) > 0, "{done}");
    assert_eq!(done["errors"].as_array().map(Vec::len), Some(0), "{done}");
    let (_, project) = s.get(&format!("/api/hq/projects/{id}"));
    assert_eq!(project["project"]["sessions"], 2, "{project}");

    // Again: nothing new was recorded, so nothing is added.
    assert_eq!(s.post(&format!("/api/hq/projects/{id}/load"), true).0, 202);
    let again = s.wait_for_load();
    assert_eq!(again["events_added"], 0, "{again}");
}

/* ── C21-d step 4.1: a write that meets another writer waits, then says so ── */

impl Server {
    fn post_json(&self, path: &str, body: &str) -> (u16, Value) {
        self.send(&format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{p}\r\nOrigin: http://127.0.0.1:{p}\r\nContent-Type: application/json\r\nContent-Length: {n}\r\nConnection: close\r\n\r\n{body}",
            p = self.port,
            n = body.len()
        ))
    }
}

/// Another process's write transaction on the store, as a hook's `sensor
/// append` holds one, released when dropped or by `release`.
struct Writer(
    Option<std::sync::mpsc::Sender<()>>,
    Option<std::thread::JoinHandle<()>>,
);

impl Writer {
    fn hold(db: PathBuf) -> Writer {
        let (held_tx, held_rx) = std::sync::mpsc::channel();
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let handle = std::thread::spawn(move || {
            let conn = rusqlite::Connection::open(db).expect("second connection");
            conn.execute_batch("BEGIN IMMEDIATE")
                .expect("take the write lock");
            held_tx.send(()).expect("held");
            let _ = rx.recv();
            let _ = conn.execute_batch("ROLLBACK");
        });
        held_rx.recv().expect("lock taken");
        Writer(Some(tx), Some(handle))
    }
    fn release(mut self) {
        self.0.take();
        if let Some(h) = self.1.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.0.take();
        if let Some(h) = self.1.take() {
            let _ = h.join();
        }
    }
}

fn first_use_key(s: &Server) -> String {
    let (code, overview) = s.get("/api/first-use");
    assert_eq!(code, 200, "{overview}");
    overview["candidates"]
        .as_array()
        .and_then(|c| c.iter().find(|c| c["source"] == "claude_code"))
        .and_then(|c| c["key"].as_str())
        .expect("a candidate")
        .to_string()
}

#[test]
fn use_this_project_waits_for_another_writer_then_succeeds() {
    let m = Machine::new();
    m.record("s1", "Fix the parser in src/a.rs.", "2026-10-01");
    let s = Server::start(&m);
    let key = first_use_key(&s);
    let writer = Writer::hold(m.home().join(".thruline/ledger.db"));
    // Longer than rusqlite's own default wait (5 s), which is all a
    // connection opened by `Ledger::open` gets.
    let released = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(8));
        writer.release();
    });
    let started = Instant::now();
    let (code, body) = s.post_json("/api/first-use/register", &format!("{{\"key\":\"{key}\"}}"));
    released.join().expect("release");
    assert_eq!(code, 200, "{body}");
    assert!(
        started.elapsed() >= Duration::from_millis(7500),
        "it waited for the writer"
    );
    assert!(body["steps"].as_u64().unwrap_or(0) > 0, "{body}");
}

#[test]
fn use_this_project_says_who_held_the_record_when_the_wait_runs_out() {
    let m = Machine::new();
    m.record("s1", "Fix the parser in src/a.rs.", "2026-10-01");
    let s = Server::start(&m);
    let key = first_use_key(&s);
    let _writer = Writer::hold(m.home().join(".thruline/ledger.db"));
    let (code, body) = s.post_json("/api/first-use/register", &format!("{{\"key\":\"{key}\"}}"));
    assert_eq!(code, 503, "{body}");
    assert_eq!(body["error"], "store_busy");
    assert!(body["waited_ms"].as_u64().unwrap_or(0) >= 29_000, "{body}");
    assert_eq!(
        body["message"],
        "Another ThruLine process was writing to the record (a Claude Code hook or a terminal command), so nothing was changed. Try again in a moment."
    );
    assert!(!body.to_string().contains("database is locked"), "{body}");
}

#[test]
fn the_load_waits_for_another_writer_then_reads_everything() {
    let m = Machine::new();
    seed(&m);
    m.record("s2", "Add a second parser test.", "2026-10-02");
    let s = Server::start(&m);
    let (_, list) = s.get("/api/hq/projects");
    let id = list["projects"][0]["id"].as_str().expect("id").to_string();
    let writer = Writer::hold(m.home().join(".thruline/ledger.db"));
    assert_eq!(s.post(&format!("/api/hq/projects/{id}/load"), true).0, 202);
    std::thread::sleep(Duration::from_secs(8));
    writer.release();
    let done = s.wait_for_load();
    assert_eq!(done["errors"].as_array().map(Vec::len), Some(0), "{done}");
    assert_eq!(done["store_busy"], 0, "{done}");
    assert!(done["events_added"].as_u64().unwrap_or(0) > 0, "{done}");
}

#[test]
fn scratch_folders_are_not_offered_and_the_count_is_said() {
    let m = Machine::new();
    m.record("s1", "Fix the parser in src/a.rs.", "2026-10-01");
    // A session Claude Code ran inside a scratch folder.
    let scratch = m
        .home()
        .join(".claude/projects/-private-tmp-claude-501-x-scratchpad");
    std::fs::create_dir_all(&scratch).expect("dir");
    let line = serde_json::json!({"type":"user","uuid":"x1","parentUuid":null,"isSidechain":false,
        "sessionId":"x","timestamp":"2026-10-02T09:00:00Z","cwd":"/private/tmp/claude-501/x/scratchpad",
        "message":{"role":"user","content":"try this"}});
    std::fs::write(scratch.join("x.jsonl"), format!("{line}\n")).expect("write");
    let s = Server::start(&m);
    let (code, overview) = s.get("/api/first-use");
    assert_eq!(code, 200);
    let labels: Vec<String> = overview["candidates"]
        .as_array()
        .expect("candidates")
        .iter()
        .map(|c| c.to_string())
        .collect();
    // The fixture's real Git repository can itself live under /private/tmp
    // when the gate uses the internal APFS scratch volume. Only the recorded
    // non-repository scratch folder must be absent.
    assert!(
        labels
            .iter()
            .all(|c| !c.contains("/private/tmp/claude-501/x/scratchpad")),
        "{labels:?}"
    );
    assert_eq!(overview["excluded_temp_projects"], 1, "{overview}");
    let d = overview["diagnostics"]
        .as_array()
        .and_then(|d| d.iter().find(|d| d["code"] == "temp_projects_excluded"))
        .expect("diagnostic");
    assert_eq!(d["count"], 1);
    assert_eq!(
        d["sentence"],
        "1 folder recorded in a temporary folder is not listed: scratch work is not a project."
    );
    // The machine field is still there, unchanged in form.
    assert!(overview["notes"]
        .as_array()
        .is_some_and(|n| n.iter().any(|n| n == "temp_projects_excluded=1")));

    // And HQ's own list says the same.
    let status = m
        .cmd(&["ingest", "--apply", "--source", "claude-code"])
        .arg(&scratch)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("ingest");
    assert!(status.success());
    let (_, list) = s.get("/api/hq/projects");
    assert_eq!(list["excluded_temp_projects"], 1, "{list}");
    assert_eq!(list["projects"].as_array().map(Vec::len), Some(0), "{list}");
}

/* ── C27 item 4: First run only when nothing is recorded ── */

#[test]
fn a_store_with_a_project_but_no_records_still_opens_on_first_run() {
    let m = Machine::new();
    let s = Server::start(&m);
    assert!(s.url.ends_with("/#/start"), "{}", s.url);
    // Pick this folder: a project row with nothing recorded in it.
    let (_, overview) = s.get("/api/first-use");
    let key = overview["candidates"]
        .as_array()
        .and_then(|c| c.iter().find(|c| c["source"] == "here"))
        .and_then(|c| c["key"].as_str())
        .expect("this folder is offered")
        .to_string();
    assert_eq!(
        s.post_json("/api/first-use/register", &format!("{{\"key\":\"{key}\"}}"))
            .0,
        200
    );
    drop(s);
    let again = Server::start(&m);
    assert!(
        again.url.ends_with("/#/start"),
        "a store with a project row but no records left First run: {}",
        again.url
    );
}

/// CTO 2026-10-12: ThruLine must not show a guessed path. A folder picked on
/// First run before anything was recorded in it is listed under the path that
/// was picked, not under its folder name with every `-` read as `/` (the
/// folder this test picks is `my-app`, which that reading calls `my/app`).
#[test]
fn a_picked_folder_with_nothing_recorded_is_listed_under_the_path_that_was_picked() {
    let m = Machine::named("my-app");
    let s = Server::start(&m);
    let (_, overview) = s.get("/api/first-use");
    let key = overview["candidates"]
        .as_array()
        .and_then(|c| c.iter().find(|c| c["source"] == "here"))
        .and_then(|c| c["key"].as_str())
        .expect("this folder is offered")
        .to_string();
    assert_eq!(
        s.post_json("/api/first-use/register", &format!("{{\"key\":\"{key}\"}}"))
            .0,
        200
    );
    let (code, list) = s.get("/api/hq/projects");
    assert_eq!(code, 200, "{list}");
    let projects = list["projects"].as_array().expect("projects");
    assert_eq!(projects.len(), 1, "{list}");
    let picked = std::fs::canonicalize(m.repo())
        .expect("the folder exists")
        .display()
        .to_string();
    assert_eq!(projects[0]["root"], picked, "{list}");
    assert_eq!(projects[0]["path_known"], true, "{list}");
    assert_eq!(
        projects[0]["members"][0]["root_basis"], "stored_root",
        "{list}"
    );
    assert_eq!(projects[0]["sessions"], 0, "{list}");
}

#[test]
fn a_store_with_records_opens_on_the_inbox() {
    let m = Machine::new();
    seed(&m);
    let s = Server::start(&m);
    assert!(
        !s.url.contains("#/start"),
        "a store with records was sent to First run: {}",
        s.url
    );
}

/* ── C26 ruling (b): track one THREAD as a Change ── */

fn ingest_all(m: &Machine) {
    let status = m
        .cmd(&["ingest", "--apply", "--source", "claude-code"])
        .arg(m.project_dir())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("ingest");
    assert!(status.success());
}

#[test]
fn tracking_a_thread_confirms_every_session_of_that_thread_and_nothing_newer() {
    let m = Machine::new();
    // One thread over two sessions (same file, a day apart) …
    m.record_file(
        "s1",
        "Fix the parser in src/a.rs.",
        "2026-10-01",
        "src/a.rs",
    );
    m.record_file(
        "s2",
        "Add a parser test for empty input.",
        "2026-10-02",
        "src/a.rs",
    );
    // … and newer, unrelated work: what the old path would have confirmed.
    m.record_file("s3", "Write the release notes.", "2026-10-05", "NOTES.md");
    ingest_all(&m);
    let s = Server::start(&m);
    let list = s.wait_for_complete_list();
    let id = list["projects"][0]["id"].as_str().expect("id").to_string();
    let (_, project) = s.get(&format!("/api/hq/projects/{id}"));
    let thread = project["threads"]
        .as_array()
        .and_then(|t| t.iter().find(|t| t["sessions"] == 2))
        .expect("the two-session thread")
        .clone();
    let tid = thread["id"].as_str().expect("tid");

    let body = format!(r#"{{"project":"{id}","thread":"{tid}"}}"#);
    let (code, written) = s.post_json("/api/first-use/change", &body);
    assert_eq!(code, 200, "{written}");
    assert_eq!(written["already_tracked"], false, "{written}");
    assert_eq!(written["thread"], tid);
    assert_eq!(written["title"], thread["title"]);
    assert_eq!(written["confirmed"], 2, "{written}");
    assert_eq!(written["suggested"], 0, "{written}");
    let change_id = written["change"].as_str().expect("change").to_string();

    let (_, doc) = s.get(&format!("/api/changes/{change_id}"));
    let change = &doc["change"];
    assert_eq!(change["request"]["title"], thread["title"], "{doc}");
    // The request points at the thread's first work prompt — its identity.
    let (_, view) = s.get(&format!("/api/hq/projects/{id}/threads/{tid}"));
    let first_work = view["prompts"]
        .as_array()
        .and_then(|p| p.iter().find(|p| p["work"] == true))
        .expect("a work prompt");
    assert_eq!(
        change["request"]["prompt_event_id"], first_work["event_id"],
        "{doc}"
    );
    let mut confirmed: Vec<String> = change["associations"]
        .as_array()
        .expect("associations")
        .iter()
        .filter(|a| a["provenance"] == "confirmed")
        .map(|a| a["session_id"].as_str().unwrap_or("").to_string())
        .collect();
    confirmed.sort();
    assert_eq!(confirmed, vec!["s1", "s2"], "{doc}");
    // No project-wide suggester ran: nothing is merely suggested.
    assert!(
        change["associations"]
            .as_array()
            .expect("associations")
            .iter()
            .all(|a| a["provenance"] != "suggested"),
        "{doc}"
    );
    // Nothing else was appended: no scope, no approval.
    assert!(change["accepted_scope"].is_null(), "{doc}");

    // A second click names the same Change and writes nothing.
    let (code, again) = s.post_json("/api/first-use/change", &body);
    assert_eq!(code, 200, "{again}");
    assert_eq!(again["already_tracked"], true, "{again}");
    assert_eq!(again["change"], change_id.as_str());
}

#[test]
fn an_unknown_or_foreign_thread_is_refused() {
    let m = Machine::new();
    m.record("s1", "Fix the parser in src/a.rs.", "2026-10-01");
    ingest_all(&m);
    let s = Server::start(&m);
    let list = s.wait_for_complete_list();
    let id = list["projects"][0]["id"].as_str().expect("id").to_string();
    let (code, body) = s.post_json(
        "/api/first-use/change",
        &format!(r#"{{"project":"{id}","thread":"0000000000000000"}}"#),
    );
    assert_eq!(code, 404, "{body}");
    assert_eq!(body["error"], "no_such_thread");
    let (code, body) = s.post_json(
        "/api/first-use/change",
        r#"{"project":"0000000000000000","thread":"0000000000000000"}"#,
    );
    assert_eq!(code, 404, "{body}");
    assert_eq!(body["error"], "no_such_project");
}

/// C27 (c): a thread read is served from the server's cache of serialized
/// views when nothing has moved, and never once the record has: a new line in
/// the thread's session changes the project's heads, and the next read shows
/// it. A cached read equals what a freshly started server serves.
#[test]
fn a_cached_thread_view_is_the_fresh_one_and_never_outlives_a_change() {
    let m = Machine::new();
    seed(&m);
    let s = Server::start(&m);
    let list = s.wait_for_complete_list();
    let id = list["projects"][0]["id"].as_str().expect("id").to_string();
    let (_, project) = s.get(&format!("/api/hq/projects/{id}"));
    let tid = project["threads"][0]["id"]
        .as_str()
        .expect("tid")
        .to_string();
    let path = format!("/api/hq/projects/{id}/threads/{tid}");
    let (code, first) = s.get(&path);
    assert_eq!(code, 200, "{first}");
    let (_, warm) = s.get(&path);
    assert_eq!(first, warm, "a warm read serves the same view");

    // One more statement by the agent in the same session, ingested.
    let file = m.project_dir().join("s1.jsonl");
    let mut text = std::fs::read_to_string(&file).expect("read");
    text.push_str(
        &(serde_json::json!({
            "type": "assistant", "uuid": "s1-5", "parentUuid": null, "isSidechain": false,
            "sessionId": "s1", "timestamp": "2026-10-01T09:05:00.000Z", "cwd": m.repo(),
            "message": {"role": "assistant", "content": [
                {"type": "text", "text": "Also renamed the helper."}]}
        })
        .to_string()
            + "\n"),
    );
    std::fs::write(&file, text).expect("append");
    let status = m
        .cmd(&["ingest", "--apply", "--source", "claude-code"])
        .arg(m.project_dir())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("ingest");
    assert!(status.success());

    let (code, after) = s.get(&path);
    assert_eq!(code, 200, "{after}");
    assert_ne!(
        after, first,
        "the cached view outlived a change to the record"
    );
    drop(s);
    let fresh = Server::start(&m);
    fresh.wait_for_complete_list();
    let (_, from_fresh) = fresh.get(&path);
    assert_eq!(after, from_fresh, "a cached read equals a fresh server's");
}

#[test]
fn a_cached_project_view_is_the_fresh_one_and_never_outlives_a_change() {
    let m = Machine::new();
    seed(&m);
    let s = Server::start(&m);
    let list = s.wait_for_complete_list();
    let id = list["projects"][0]["id"].as_str().expect("id").to_string();
    let path = format!("/api/hq/projects/{id}");
    let (code, first) = s.get(&path);
    assert_eq!(code, 200, "{first}");
    let (_, warm) = s.get(&path);
    assert_eq!(first, warm, "a warm read serves the same view");

    // A second session, recorded and ingested: a new thread in the project.
    m.record_file(
        "s2",
        "Add a CLI flag in src/b.rs.",
        "2026-10-02",
        "src/b.rs",
    );
    let status = m
        .cmd(&["ingest", "--apply", "--source", "claude-code"])
        .arg(m.project_dir())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("ingest");
    assert!(status.success());

    let (code, after) = s.get(&path);
    assert_eq!(code, 200, "{after}");
    assert_ne!(
        after, first,
        "the cached project view outlived a change to the record"
    );
    assert_eq!(
        after["threads"].as_array().map(Vec::len),
        Some(2),
        "the new session's thread is in the view: {after}"
    );
    drop(s);
    let fresh = Server::start(&m);
    fresh.wait_for_complete_list();
    let (_, from_fresh) = fresh.get(&path);
    assert_eq!(after, from_fresh, "a cached read equals a fresh server's");
}

/* ── C29 R2: a load that is still writing never blocks a screen ── */

/// How many recordings the R2 load reads, and how many turns each holds:
/// enough that, in the debug build this suite runs, the load is still
/// writing after the screen reads below have been made several times over.
const R2_FILES: u32 = 240;
const R2_TURNS: u32 = 40;

/// The bound one screen read must answer within while the load writes. A warm
/// read of this one-project store answers in milliseconds; every file the
/// load commits moves the store's fingerprint, so a read during the load may
/// rebuild the project's index, which on this store takes well under a second
/// even in a debug build on a busy machine. 2 s is the point past which a
/// person sees a screen hang, and the load is asserted to run for longer than
/// twice that, so a read that waited for the load could not answer inside it.
const R2_BOUND: Duration = Duration::from_secs(2);

impl Machine {
    /// One long recording: `turns` prompts, each with an edit and its result.
    fn record_long(&self, session: &str, turns: u32) {
        let cwd = self.repo();
        std::fs::create_dir_all(self.project_dir()).expect("dir");
        let mut lines = Vec::new();
        for t in 0..turns {
            let line = |k: u32, kind: &str, message: Value| {
                serde_json::json!({
                    "type": kind, "uuid": format!("{session}-{t}-{k}"), "parentUuid": null,
                    "isSidechain": false, "sessionId": session,
                    "timestamp": format!("2026-10-03T{:02}:{:02}:{:02}.000Z", t / 60, t % 60, k),
                    "cwd": cwd, "message": message,
                })
                .to_string()
            };
            lines.push(line(
                1,
                "user",
                serde_json::json!({"role": "user",
                    "content": format!("Step {t} of {session}: tidy src/r{t}.rs.")}),
            ));
            lines.push(line(
                2,
                "assistant",
                serde_json::json!({"role": "assistant", "content": [
                {"type": "tool_use", "id": format!("{session}-t{t}"), "name": "Edit",
                 "input": {"file_path": format!("{cwd}/src/r{t}.rs"), "old_string": "a", "new_string": "b"}}]}),
            ));
            lines.push(line(
                3,
                "user",
                serde_json::json!({"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": format!("{session}-t{t}"), "content": "ok", "is_error": false}]}),
            ));
        }
        std::fs::write(
            self.project_dir().join(format!("{session}.jsonl")),
            lines.join("\n") + "\n",
        )
        .expect("recording");
    }
}

/// C29 R2: "reload while agents write" has no time budget of its own; its
/// budget is that it never blocks any screen. While a load is running and has
/// already appended rows, the three reads a screen makes (the project list,
/// the project, a thread) each answer 200 within [`R2_BOUND`], and the load is
/// still running once they have answered.
#[test]
fn a_load_that_is_still_writing_never_blocks_a_screen() {
    let m = Machine::new();
    seed(&m);
    let s = Server::start(&m);
    let list = s.wait_for_complete_list();
    let id = list["projects"][0]["id"].as_str().expect("id").to_string();
    let (_, project) = s.get(&format!("/api/hq/projects/{id}"));
    let tid = project["threads"][0]["id"]
        .as_str()
        .expect("tid")
        .to_string();
    let reads = [
        "/api/hq/projects".to_string(),
        format!("/api/hq/projects/{id}"),
        format!("/api/hq/projects/{id}/threads/{tid}"),
    ];
    for path in &reads {
        assert_eq!(s.get(path).0, 200, "{path} before the load");
    }

    // Recorded after the store was seeded: only the load brings them in.
    for n in 0..R2_FILES {
        m.record_long(&format!("cccccccc-0000-4000-8000-{n:012}"), R2_TURNS);
    }
    let started = Instant::now();
    let (code, body) = s.post(&format!("/api/hq/projects/{id}/load"), true);
    assert_eq!(code, 202, "{body}");

    // Rounds of screen reads made while the load had written and was still
    // writing: running with rows added before the reads, running after them.
    let mut during = 0u32;
    let mut added_seen = Vec::new();
    let mut slowest = Duration::ZERO;
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        let (_, before) = s.get("/api/hq/load");
        if before["running"] != true {
            break;
        }
        let added = before["events_added"].as_u64().unwrap_or(0);
        for path in &reads {
            let t = Instant::now();
            let (code, body) = s.get(path);
            let took = t.elapsed();
            slowest = slowest.max(took);
            assert_eq!(code, 200, "{path} during the load: {body}");
            assert!(
                took < R2_BOUND,
                "{path} took {took:?} while the load was writing (bound {R2_BOUND:?}): \
                 a screen waited on the load"
            );
        }
        let (_, after) = s.get("/api/hq/load");
        if added > 0 && after["running"] == true {
            during += 1;
            added_seen.push(added);
        }
        assert!(Instant::now() < deadline, "the load never finished");
        std::thread::sleep(Duration::from_millis(100));
    }
    let ran = started.elapsed();
    let done = s.wait_for_load();
    eprintln!(
        "R2: load ran {ran:?}; {during} rounds of reads inside it; slowest read {slowest:?}; \
         rows added seen {added_seen:?}; {} rows in all",
        done["events_added"]
    );
    assert_eq!(done["errors"].as_array().map(Vec::len), Some(0), "{done}");
    assert_eq!(done["files_done"], R2_FILES + 1, "{done}");
    assert!(
        during >= 3,
        "only {during} rounds of reads fell inside the load ({ran:?}); it must run \
         long enough to be read from while it writes: raise R2_FILES"
    );
    assert!(
        ran > R2_BOUND * 2,
        "the load ran {ran:?}, too short to tell a blocked read from a quick one"
    );
    assert!(
        added_seen.first() < added_seen.last(),
        "the load status showed rows being added while the screens read: {added_seen:?}"
    );
}
