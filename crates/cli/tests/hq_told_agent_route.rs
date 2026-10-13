//! C30 R-LEDGER — `POST /api/hq/projects/<id>/threads/<tid>/told-agent`
//! (write-address row 24), through the shipping binary, and the `told_agent`
//! field it adds to `GET /api/hq/projects/<id>`.
//!
//! The module's own tests (`crates/core/tests/change_agent_messages.rs`) prove
//! what an act is; this proves the served route keeps the contract it is
//! written down with in CLAUDE.md: same-origin, a body cap sized from the
//! 8,000-character limit, `409 too_long` naming `message`, an unknown field
//! (including a digest or an anchor the client might try to send) refused,
//! the anchor / version / Change read by the SERVER, every refusal appending
//! nothing, nothing sent anywhere, and the route free while approving is the
//! one paid surface.
//!
//! A real store under a temporary HOME with synthetic recordings; no real
//! store is opened.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;
use thruline_core::capture_hold::{self, Act, Hold};

const CANARY: &str = "ghp_TLCANARYaaZZ11bbYY22ccXX33ddWW44e";

struct Machine {
    dir: tempfile::TempDir,
}

impl Machine {
    fn new() -> Machine {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("h/.thruline")).expect("home");
        std::fs::create_dir_all(dir.path().join("repo/.git")).expect("repo");
        Machine { dir }
    }
    fn home(&self) -> PathBuf {
        self.dir.path().join("h")
    }
    fn db(&self) -> PathBuf {
        self.home().join(".thruline/ledger.db")
    }
    fn repo(&self) -> String {
        self.dir.path().join("repo").display().to_string()
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
            .env("USER", "robin")
            .env_remove("THRULINE_PAID_GATE_DEV")
            .env_remove("THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS")
            .env_remove("THRULINE_LICENCE_DEV_PUBLIC_KEY")
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .current_dir(self.dir.path().join("repo"));
        cmd
    }
    /// One recording that edits `file`: a prompt, an edit, a result, a claim.
    fn record(&self, session: &str, prompt: &str, day: &str, file: &str) {
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
    fn ingest(&self) {
        let status = self
            .cmd(&["ingest", "--apply", "--source", "claude-code"])
            .arg(self.project_dir())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("ingest");
        assert!(status.success());
    }
    /// Row counts of the table, read the way a person would, from outside.
    fn rows(&self) -> i64 {
        let ledger = thruline_core::ledger::Ledger::open(self.db()).expect("open");
        ledger
            .conn()
            .query_row("SELECT COUNT(*) FROM change_agent_messages", [], |r| {
                r.get(0)
            })
            .expect("count")
    }
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
                return Server { child, port };
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
        // A refused oversized body may be cut off mid-write; the answer is
        // already on its way.
        let _ = s.write_all(head.as_bytes());
        let mut raw = Vec::new();
        let _ = s.read_to_end(&mut raw);
        let text = String::from_utf8_lossy(&raw);
        let status: u16 = text.get(9..12).and_then(|s| s.parse().ok()).unwrap_or(0);
        let body = text.split_once("\r\n\r\n").map(|x| x.1).unwrap_or("");
        (status, serde_json::from_str(body).unwrap_or(Value::Null))
    }

    fn get(&self, path: &str) -> (u16, Value) {
        self.send(&format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            self.port
        ))
    }

    fn origin(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// A POST with an `Origin` line of the caller's choosing.
    fn post_from(&self, origin: Option<&str>, path: &str, body: &str) -> (u16, Value) {
        let origin_line = origin
            .map(|o| format!("Origin: {o}\r\n"))
            .unwrap_or_default();
        self.send(&format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n{origin_line}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.port,
            body.len()
        ))
    }

    /// A POST from HQ's own page.
    fn post(&self, path: &str, body: &Value) -> (u16, Value) {
        self.post_from(Some(&self.origin()), path, &body.to_string())
    }

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
}

struct Fixture {
    m: Machine,
    server: Server,
    project: String,
    /// The two-session thread, and one it is not.
    thread: Value,
    other: Value,
}

fn fixture() -> Fixture {
    let m = Machine::new();
    m.record(
        "s1",
        "Fix the parser in src/a.rs.",
        "2026-10-01",
        "src/a.rs",
    );
    m.record(
        "s2",
        "Add a parser test for empty input.",
        "2026-10-02",
        "src/a.rs",
    );
    m.record("s3", "Write the release notes.", "2026-10-05", "NOTES.md");
    m.ingest();
    let server = Server::start(&m);
    let list = server.wait_for_complete_list();
    let project = list["projects"][0]["id"].as_str().expect("id").to_string();
    let (_, view) = server.get(&format!("/api/hq/projects/{project}"));
    let threads = view["threads"].as_array().expect("threads");
    let thread = threads
        .iter()
        .find(|t| t["sessions"] == 2)
        .expect("the two-session thread")
        .clone();
    let other = threads
        .iter()
        .find(|t| t["sessions"] == 1)
        .expect("the one-session thread")
        .clone();
    Fixture {
        m,
        server,
        project,
        thread,
        other,
    }
}

impl Fixture {
    fn path(&self, thread: &Value) -> String {
        format!(
            "/api/hq/projects/{}/threads/{}/told-agent",
            self.project,
            thread["id"].as_str().expect("tid")
        )
    }
    fn thread_in_view(&self, thread: &Value) -> Value {
        let (_, view) = self
            .server
            .get(&format!("/api/hq/projects/{}", self.project));
        view["threads"]
            .as_array()
            .expect("threads")
            .iter()
            .find(|t| t["id"] == thread["id"])
            .expect("the thread")
            .clone()
    }
}

#[test]
fn a_copied_reply_obeys_exclude_then_include_without_changing_the_kept_message() {
    let f = fixture();
    let path = f.path(&f.thread);
    let message = "Read src/private.rs";
    let copied = serde_json::json!({"act":"copied","message":message});
    let (status, plain) = f.server.post(&path, &copied);
    assert_eq!(status, 200, "{plain}");
    assert_eq!(plain["act"]["message"], message);

    let folder = f.m.repo();
    let ledger = thruline_core::ledger::Ledger::open(f.m.db()).expect("synthetic ledger");
    let mut hold = Hold {
        project_id: &f.project,
        project_folder: &folder,
        actor: "synthetic person",
        recorded_at: "2026-10-08T00:00:01Z",
    };
    capture_hold::record_for_person(&ledger, Act::Excluded, &hold, Some("src/private.rs"))
        .expect("exclude private path");
    let (status, hidden) = f.server.post(&path, &copied);
    assert_eq!(status, 200, "{hidden}");
    assert_eq!(
        hidden["act"]["message"],
        thruline_core::capture_exclusion::marker("src/private.rs")
    );
    assert_eq!(hidden["left_out_by_exclusion"], 1);
    assert!(!hidden.to_string().contains("Read src/private.rs"));
    let stored: String = ledger
        .conn()
        .query_row(
            "SELECT message FROM change_agent_messages ORDER BY rowid DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("kept copy");
    assert_eq!(stored, message, "presentation must not rewrite the record");

    hold.recorded_at = "2026-10-08T00:00:02Z";
    capture_hold::record_for_person(&ledger, Act::Included, &hold, Some("src/private.rs"))
        .expect("include private path");
    let (status, restored) = f.server.post(&path, &copied);
    assert_eq!(status, 200, "{restored}");
    assert_eq!(restored["act"]["message"], message);
    assert!(restored.get("left_out_by_exclusion").is_none());
}

/* ── the contract ──────────────────────────────────────────────────────── */

#[test]
fn it_must_come_from_the_page_and_say_exactly_what_it_means() {
    let f = fixture();
    let path = f.path(&f.thread);
    let good = serde_json::json!({"act": "copied", "message": "hello"}).to_string();

    // No Origin is the route's own refusal. A foreign Origin or a lookalike
    // port never reaches the route: the server answers 403 to anything but
    // itself. Either way nothing is kept.
    let (code, body) = f.server.post_from(None, &path, &good);
    assert_eq!(code, 403, "{body}");
    assert_eq!(body["error"], "same_origin_required");
    for origin in [
        "https://example.com",
        "http://127.0.0.1:1",
        "http://localhost:1",
    ] {
        let (code, _) = f.server.post_from(Some(origin), &path, &good);
        assert_eq!(code, 403, "{origin}");
    }
    assert_eq!(f.m.rows(), 0);

    // A different method is not an act.
    let (code, body) = f.server.get(&path);
    assert_eq!(code, 405, "{body}");

    // Anything the client might try to supply about the thread, the version or
    // the digest is refused: the server reads the anchor itself.
    for extra in [
        r#"{"act":"copied","message":"hello","message_sha256":"0000000000000000000000000000000000000000000000000000000000000000"}"#,
        r#"{"act":"copied","message":"hello","thread_event_id":"e1"}"#,
        r#"{"act":"copied","message":"hello","version_id":"v1"}"#,
        r#"{"act":"copied","message":"hello","change_id":"c1"}"#,
        r#"{"act":"copied","message":"hello","actor":"someone else"}"#,
        r#"{"act":"copied","message":"hello","sent":true}"#,
    ] {
        let (code, body) = f.server.post_from(Some(&f.server.origin()), &path, extra);
        assert_eq!(code, 400, "{extra} {body}");
        assert_eq!(body["error"], "malformed_request");
    }
    // Shape: not JSON, no act, a word the table does not have, text on a mark,
    // a row named by a copy.
    for (bad, error) in [
        ("not json", "malformed_request"),
        (r#"{}"#, "malformed_request"),
        (r#"{"act":"sent","message":"hello"}"#, "unknown_act"),
        (r#"{"act":"delivered"}"#, "unknown_act"),
        (
            r#"{"act":"marked_pasted","message":"x"}"#,
            "malformed_request",
        ),
        (
            r#"{"act":"unmarked_pasted","message":"x"}"#,
            "malformed_request",
        ),
        (
            r#"{"act":"copied","message":"x","refers_to":"c"}"#,
            "malformed_request",
        ),
        (r#"{"act":"copied"}"#, "message_required"),
        (r#"{"act":"copied","message":"   \n"}"#, "message_required"),
    ] {
        let (code, body) = f.server.post_from(Some(&f.server.origin()), &path, bad);
        assert_eq!(code, 400, "{bad} {body}");
        assert_eq!(body["error"], error, "{bad}");
    }
    assert_eq!(f.m.rows(), 0, "every refusal so far appended nothing");

    // Unknown ids.
    let (code, body) = f.server.post(
        "/api/hq/projects/nothing/threads/whatever/told-agent",
        &serde_json::json!({"act": "copied", "message": "hello"}),
    );
    assert_eq!(
        (code, &body["error"]),
        (404, &Value::from("no_such_project"))
    );
    let (code, body) = f.server.post(
        &format!("/api/hq/projects/{}/threads/nothing/told-agent", f.project),
        &serde_json::json!({"act": "copied", "message": "hello"}),
    );
    assert_eq!(
        (code, &body["error"]),
        (404, &Value::from("no_such_thread"))
    );
    assert_eq!(f.m.rows(), 0);
}

#[test]
fn a_message_over_the_limit_is_refused_by_name_and_a_larger_body_by_size() {
    let f = fixture();
    let path = f.path(&f.thread);

    let (code, body) = f.server.post(
        &path,
        &serde_json::json!({"act": "copied", "message": "x".repeat(8001)}),
    );
    assert_eq!(code, 409, "{body}");
    assert_eq!(body["error"], "too_long");
    assert_eq!(body["field"], "message");
    assert_eq!(body["limit"], 8000);
    assert_eq!(f.m.rows(), 0);

    // Exactly 8,000 characters (two-byte ones: the limit is characters) is kept.
    let (code, body) = f.server.post(
        &path,
        &serde_json::json!({"act": "copied", "message": "é".repeat(8000)}),
    );
    assert_eq!(code, 200, "{body}");

    // A body bigger than 8,000 characters could make, even all written the
    // long way, is refused on size alone.
    let huge = format!(
        r#"{{"act":"copied","message":"{}"}}"#,
        "x".repeat(8000 * 12 + 5000)
    );
    let (code, body) = f.server.post_from(Some(&f.server.origin()), &path, &huge);
    assert_eq!(
        code, 413,
        "refused before the body is read; the connection may be cut: {body}"
    );
    assert_eq!(f.m.rows(), 1);
}

#[test]
fn a_copy_is_kept_as_written_with_the_anchor_read_by_the_server() {
    let f = fixture();
    let path = f.path(&f.thread);
    let words = format!("  Re-run the parser test, then show me the output. {CANARY}\n");
    let (code, body) = f.server.post(
        &path,
        &serde_json::json!({"act": "copied", "message": words}),
    );
    assert_eq!(code, 200, "{body}");

    // Nothing was sent, and the answer says so in words.
    assert_eq!(body["nothing_sent"], true);
    assert!(
        body["message"]
            .as_str()
            .unwrap_or("")
            .starts_with("Nothing was sent."),
        "{body}"
    );
    assert_eq!(body["schema_version"], 1, "the HQ routes' schema stays 1");
    let act = &body["act"];
    assert_eq!(act["act"], "copied");
    assert_eq!(act["actor"], "robin");
    assert_eq!(act["refers_to"], Value::Null);
    assert!(
        !act["message"].as_str().unwrap_or("").contains(CANARY),
        "the answer is masked: {act}"
    );

    // The row: the words exactly as written, the digest of those bytes, the
    // thread's first work prompt as its anchor, the version the thread is at,
    // and no Change (none tracks this thread).
    let ledger = thruline_core::ledger::Ledger::open(f.m.db()).expect("open");
    let row: (
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
    ) = ledger
        .conn()
        .query_row(
            "SELECT message, message_sha256, thread_id, thread_event_id, change_id, version_id,
                    actor FROM change_agent_messages",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            },
        )
        .expect("one row");
    assert_eq!(row.0, words, "kept exactly as written, secret and all");
    assert_eq!(row.1, thruline_core::chain::sha256_hex(words.as_bytes()));
    assert_eq!(row.2, f.thread["id"].as_str().unwrap());
    let (_, view) = f.server.get(&format!(
        "/api/hq/projects/{}/threads/{}",
        f.project,
        f.thread["id"].as_str().unwrap()
    ));
    let first_work = view["prompts"]
        .as_array()
        .and_then(|p| p.iter().find(|p| p["work"] == true))
        .expect("a work prompt");
    assert_eq!(row.3, first_work["event_id"].as_str().unwrap());
    assert_eq!(row.4, None);
    assert_eq!(
        row.5.as_deref(),
        f.thread["current_version_id"].as_str(),
        "the version is the thread's own"
    );
    assert_eq!(row.6, "robin");
    assert_eq!(act["thread_event_id"], row.3.as_str());
}

#[test]
fn a_thread_a_change_tracks_names_that_change_on_the_act() {
    let f = fixture();
    let path = f.path(&f.thread);
    let (code, tracked) = f.server.post(
        "/api/first-use/change",
        &serde_json::json!({"project": f.project, "thread": f.thread["id"]}),
    );
    assert_eq!(code, 200, "{tracked}");
    let change = tracked["change"].as_str().expect("change").to_string();

    let (code, body) = f.server.post(
        &path,
        &serde_json::json!({"act": "copied", "message": "go on"}),
    );
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["act"]["change_id"], change.as_str());
    // The other thread is untracked: its act names no Change, and is still kept.
    let (code, body) = f.server.post(
        &f.path(&f.other),
        &serde_json::json!({"act": "copied", "message": "go on"}),
    );
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["act"]["change_id"], Value::Null);
}

#[test]
fn a_mark_and_an_undo_follow_the_copy_and_refuse_what_they_cannot_refer_to() {
    let f = fixture();
    let path = f.path(&f.thread);
    let post = |body: Value| f.server.post(&path, &body);

    // A mark or an undo with nothing to refer to.
    let (code, body) = post(serde_json::json!({"act": "marked_pasted"}));
    assert_eq!(
        (code, &body["error"]),
        (409, &Value::from("no_copy_to_mark")),
        "{body}"
    );
    assert!(body["message"]
        .as_str()
        .unwrap()
        .contains("Nothing was saved"));
    let (code, body) = post(serde_json::json!({"act": "unmarked_pasted"}));
    assert_eq!(
        (code, &body["error"]),
        (409, &Value::from("no_mark_to_undo")),
        "{body}"
    );
    assert_eq!(f.m.rows(), 0);

    let (_, copy) = post(serde_json::json!({"act": "copied", "message": "first"}));
    let copy_id = copy["act"]["id"].as_str().unwrap().to_string();
    // The other thread's copy is not this thread's.
    let (_, foreign) = f.server.post(
        &f.path(&f.other),
        &serde_json::json!({"act": "copied", "message": "elsewhere"}),
    );
    let (code, body) =
        post(serde_json::json!({"act": "marked_pasted", "refers_to": foreign["act"]["id"]}));
    assert_eq!(
        (code, &body["error"]),
        (409, &Value::from("no_copy_to_mark")),
        "{body}"
    );

    let (code, mark) = post(serde_json::json!({"act": "marked_pasted"}));
    assert_eq!(code, 200, "{mark}");
    assert_eq!(mark["act"]["act"], "marked_pasted");
    assert_eq!(mark["act"]["refers_to"], copy_id.as_str());
    assert_eq!(mark["act"]["message"], Value::Null);
    assert_eq!(mark["told_agent"]["last_act"], "marked_pasted");
    assert_eq!(mark["nothing_sent"], true);

    // Twice is refused, naming the standing mark.
    let (code, again) = post(serde_json::json!({"act": "marked_pasted"}));
    assert_eq!(
        (code, &again["error"]),
        (409, &Value::from("already_marked")),
        "{again}"
    );
    assert_eq!(again["mark"], mark["act"]["id"]);

    let (code, undo) = post(serde_json::json!({"act": "unmarked_pasted"}));
    assert_eq!(code, 200, "{undo}");
    assert_eq!(undo["act"]["refers_to"], mark["act"]["id"]);
    assert_eq!(undo["told_agent"]["last_act"], "unmarked_pasted");
    let (code, twice) = post(serde_json::json!({"act": "unmarked_pasted"}));
    assert_eq!(
        (code, &twice["error"]),
        (409, &Value::from("no_mark_to_undo")),
        "{twice}"
    );
    assert_eq!(f.m.rows(), 4, "copy, foreign copy, mark, undo");
}

#[test]
fn the_project_view_serves_the_last_act_per_thread_and_only_there() {
    let f = fixture();
    // Before any act: no thread has the key.
    let (_, before) = f.server.get(&format!("/api/hq/projects/{}", f.project));
    assert!(
        !before.to_string().contains("told_agent"),
        "a project with no act serves exactly what it did before"
    );
    let moves = |view: &Value| {
        view["threads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| (t["id"].clone(), t["whose_move"].clone(), t["edits"].clone()))
            .collect::<Vec<_>>()
    };
    let moves_before = moves(&before);

    let path = f.path(&f.thread);
    let (_, copy) = f.server.post(
        &path,
        &serde_json::json!({"act": "copied", "message": "go on"}),
    );
    let (_, mark) = f
        .server
        .post(&path, &serde_json::json!({"act": "marked_pasted"}));

    let thread = f.thread_in_view(&f.thread);
    let told = &thread["told_agent"];
    assert_eq!(told["last_act"], "marked_pasted", "{thread}");
    assert_eq!(told["act_id"], mark["act"]["id"]);
    assert_eq!(told["at"], mark["act"]["recorded_at"]);
    assert_eq!(told["version_id"], copy["act"]["version_id"]);
    assert_eq!(told["later_work"], false, "nothing is newer than the act");
    assert_eq!(
        told.as_object().unwrap().len(),
        5,
        "last_act, at, version_id, act_id, later_work — and no text: {told}"
    );
    let other = f.thread_in_view(&f.other);
    assert!(other.get("told_agent").is_none(), "{other}");

    // The acts moved nothing the engine decides.
    let (_, after) = f.server.get(&format!("/api/hq/projects/{}", f.project));
    assert_eq!(
        moves(&after),
        moves_before,
        "whose_move and counts are the engine's"
    );
    assert_eq!(after["schema_version"], 1);
}

#[test]
fn new_work_on_the_thread_after_the_act_is_later_work() {
    let f = fixture();
    let path = f.path(&f.thread);
    let (code, _) = f.server.post(
        &path,
        &serde_json::json!({"act": "copied", "message": "go on"}),
    );
    assert_eq!(code, 200);
    assert_eq!(
        f.thread_in_view(&f.thread)["told_agent"]["later_work"],
        false
    );

    // The agent works on the same file in the same session after the act: the
    // thread gets a row newer than the act. (Dated far ahead so it is later
    // than "now" whatever the clock says.)
    let cwd = f.m.repo();
    let later = serde_json::json!({
        "type": "assistant", "uuid": "s2-later", "parentUuid": null, "isSidechain": false,
        "sessionId": "s2", "timestamp": "2099-01-01T09:05:00.000Z", "cwd": cwd,
        "message": {"role": "assistant", "content": [
            {"type": "tool_use", "id": "s2-t9", "name": "Edit",
             "input": {"file_path": format!("{cwd}/src/a.rs"), "old_string": "b", "new_string": "c"}}]},
    })
    .to_string();
    let file = f.m.project_dir().join("s2.jsonl");
    let mut text = std::fs::read_to_string(&file).expect("recording");
    text.push_str(&later);
    text.push('\n');
    std::fs::write(&file, text).expect("append");
    f.m.ingest();

    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let thread = f.thread_in_view(&f.thread);
        if thread["told_agent"]["later_work"] == true {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the new row never reached the view: {thread}"
        );
        std::thread::sleep(Duration::from_millis(300));
    }
    // The act itself is untouched by the work that followed it.
    assert_eq!(f.m.rows(), 1);
}

#[test]
fn the_route_is_free_and_the_one_paid_surface_is_untouched() {
    // The server starts as it ships — paid gate on, no licence — and the act is
    // still kept: only approving a version asks for a licence.
    let f = fixture();
    assert!(
        !f.m.home().join(".thruline/licence.json").exists(),
        "a fresh home holds no licence"
    );
    let (code, body) = f.server.post(
        &f.path(&f.thread),
        &serde_json::json!({"act": "copied", "message": "go on"}),
    );
    assert_eq!(code, 200, "{body}");
    assert_ne!(body["error"], "licence_required");
}
