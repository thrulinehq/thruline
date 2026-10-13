//! `GET /api/hq/projects/<id>/capture` (C30 R-ROUTES "Capture health")
//! against the shipping binary: `thruline ui` on a temporary HOME and store,
//! with Claude Code settings files written into that HOME and that project,
//! never the real `~/.claude` or `~/.thruline`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

struct Machine {
    dir: tempfile::TempDir,
}

impl Machine {
    /// A HOME, a store, and a project folder holding an empty `.git`: HQ
    /// lists it (a `.git` is there), and git says it is not a repository
    /// (an empty `.git` is not one), until [`Machine::git_init`].
    fn new() -> Machine {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("h/.thruline")).expect("home");
        std::fs::create_dir_all(dir.path().join("repo/.git")).expect("repo");
        Machine { dir }
    }
    fn home(&self) -> PathBuf {
        self.dir.path().join("h")
    }
    fn repo_path(&self) -> PathBuf {
        self.dir.path().join("repo")
    }
    fn repo(&self) -> String {
        self.repo_path().display().to_string()
    }
    fn db(&self) -> PathBuf {
        self.home().join(".thruline/ledger.db")
    }
    fn user_settings(&self) -> PathBuf {
        self.home().join(".claude/settings.json")
    }
    fn project_settings(&self) -> PathBuf {
        self.repo_path().join(".claude/settings.json")
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
            .env_remove("CODEX_HOME")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .current_dir(self.repo_path());
        cmd
    }
    /// One recording of four steps on 2026-10-01, 09:01 to 09:04, ingested.
    fn seed(&self) {
        let cwd = self.repo();
        std::fs::create_dir_all(self.project_dir()).expect("dir");
        let line = |n: u32, kind: &str, message: Value| {
            serde_json::json!({
                "type": kind, "uuid": format!("s1-{n}"), "parentUuid": null,
                "isSidechain": false, "sessionId": "s1",
                "timestamp": format!("2026-10-01T09:0{n}:00.000Z"), "cwd": cwd, "message": message,
            })
            .to_string()
        };
        let lines = [
            line(
                1,
                "user",
                serde_json::json!({"role": "user", "content": "Fix the parser."}),
            ),
            line(
                2,
                "assistant",
                serde_json::json!({"role": "assistant", "content": [
                {"type": "tool_use", "id": "s1-t1", "name": "Edit",
                 "input": {"file_path": format!("{cwd}/src/a.rs"), "old_string": "a", "new_string": "b"}}]}),
            ),
            line(
                3,
                "user",
                serde_json::json!({"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "s1-t1", "content": "ok", "is_error": false}]}),
            ),
            line(
                4,
                "assistant",
                serde_json::json!({"role": "assistant", "content": [
                {"type": "text", "text": "Done."}]}),
            ),
        ];
        std::fs::write(self.project_dir().join("s1.jsonl"), lines.join("\n") + "\n")
            .expect("recording");
        let status = self
            .cmd(&["ingest", "--apply", "--source", "claude-code"])
            .arg(self.project_dir())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("ingest");
        assert!(status.success());
    }
    /// One delivery through the sensor hook, as Claude Code would send it.
    fn deliver(&self, event: &str) {
        let mut child = self
            .cmd(&["sensor", "append", event])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("sensor append");
        let payload = serde_json::json!({
            "session_id": "s1", "transcript_path": self.project_dir().join("s1.jsonl"),
            "cwd": self.repo(), "hook_event_name": event,
        });
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(payload.to_string().as_bytes())
            .expect("payload");
        assert!(child.wait().expect("wait").success());
    }
    /// One delivery naming any session and folder, either of them absent.
    fn deliver_as(&self, event: &str, session: Option<&str>, cwd: Option<&str>) {
        let mut child = self
            .cmd(&["sensor", "append", event])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("sensor append");
        let mut payload = serde_json::json!({ "hook_event_name": event });
        if let Some(s) = session {
            payload["session_id"] = Value::from(s);
        }
        if let Some(c) = cwd {
            payload["cwd"] = Value::from(c);
        }
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(payload.to_string().as_bytes())
            .expect("payload");
        assert!(child.wait().expect("wait").success());
    }
    /// A second recording, session `s2`, in the same folder as `s1`.
    fn seed_second(&self) {
        let cwd = self.repo();
        let line = serde_json::json!({
            "type": "user", "uuid": "s2-1", "parentUuid": null, "isSidechain": false,
            "sessionId": "s2", "timestamp": "2026-10-02T09:00:00.000Z", "cwd": cwd,
            "message": {"role": "user", "content": "Add the test."},
        });
        std::fs::write(self.project_dir().join("s2.jsonl"), line.to_string() + "\n")
            .expect("recording");
        let status = self
            .cmd(&["ingest", "--apply", "--source", "claude-code"])
            .arg(self.project_dir())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("ingest");
        assert!(status.success());
    }
    fn write(&self, path: &PathBuf, text: &str) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        std::fs::write(path, text).expect("settings");
    }
    fn git_init(&self) {
        std::fs::remove_dir_all(self.repo_path().join(".git")).expect("empty .git");
        let ok = Command::new("git")
            .args(["init", "-q"])
            .current_dir(self.repo_path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .status()
            .expect("git init");
        assert!(ok.success());
    }
}

struct Server {
    child: Child,
    port: u16,
    stdout_drain: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(drain) = self.stdout_drain.take() {
            let _ = drain.join();
        }
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
        let mut stdout = BufReader::new(stdout);
        loop {
            let mut line = String::new();
            if stdout.read_line(&mut line).expect("line") == 0 {
                break;
            }
            if let Some(rest) = line.trim().strip_prefix("ThruLine is at http://127.0.0.1:") {
                let port = rest
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .expect("port");
                // The UI prints another line after its address. Dropping the
                // read pipe here can kill the child with a broken stdout pipe
                // just as this test opens its first request.
                let stdout_drain = std::thread::spawn(move || {
                    let _ = std::io::copy(&mut stdout, &mut std::io::sink());
                });
                return Server {
                    child,
                    port,
                    stdout_drain: Some(stdout_drain),
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

    fn post(&self, path: &str, body: &str) -> (u16, Value) {
        self.send(&format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{p}\r\nOrigin: http://127.0.0.1:{p}\r\nContent-Type: application/json\r\nContent-Length: {n}\r\nConnection: close\r\n\r\n{body}",
            p = self.port,
            n = body.len()
        ))
    }

    fn project_id(&self) -> String {
        let (code, list) = self.get("/api/hq/projects");
        assert_eq!(code, 200, "{list}");
        let projects = list["projects"].as_array().expect("projects");
        assert_eq!(projects.len(), 1, "{list}");
        projects[0]["id"].as_str().expect("id").to_string()
    }

    fn capture(&self) -> Value {
        let id = self.project_id();
        let (code, body) = self.get(&format!("/api/hq/projects/{id}/capture"));
        assert_eq!(code, 200, "{body}");
        assert_eq!(body["schema_version"], 1, "{body}");
        assert_eq!(body["project"]["id"], id.as_str(), "{body}");
        body
    }
}

fn level<'a>(doc: &'a Value, scope: &str) -> &'a Value {
    doc["claude_code_hooks"]["levels"]
        .as_array()
        .and_then(|l| l.iter().find(|l| l["scope"] == scope))
        .unwrap_or_else(|| panic!("no {scope} level: {doc}"))
}

#[test]
fn hooks_installed_by_sensor_install_are_installed_and_alive() {
    let m = Machine::new();
    m.seed();
    let out = m.cmd(&["sensor", "install"]).output().expect("install");
    assert!(out.status.success(), "{out:?}");
    let s = Server::start(&m);
    let doc = s.capture();
    let hooks = &doc["claude_code_hooks"];
    assert_eq!(hooks["installed"], true, "{doc}");
    assert_eq!(hooks["alive"], true, "{doc}");
    assert!(hooks["alive_reason"].is_null(), "{doc}");
    assert_eq!(hooks["missing_programs"], 0, "{doc}");
    let mut expected: Vec<&str> = thruline_core::sensor::install_set();
    expected.sort();
    let events: Vec<&str> = hooks["events"]
        .as_array()
        .expect("events")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(events, expected, "{doc}");
    assert_eq!(hooks["version"], env!("CARGO_PKG_VERSION"), "{doc}");
    let user = level(&doc, "user");
    assert_eq!(user["state"], "read", "{doc}");
    let commands = user["commands"].as_array().expect("commands");
    assert_eq!(commands.len(), expected.len(), "{doc}");
    for c in commands {
        assert_eq!(c["kind"], "recorder", "{c}");
        assert_eq!(c["program_exists"], true, "{c}");
        assert_eq!(c["this_program"], true, "{c}");
    }
    assert_eq!(level(&doc, "project")["state"], "absent", "{doc}");
    assert!(doc["checked_at"]
        .as_str()
        .is_some_and(|t| t.ends_with('Z') && t.len() >= 20));
}

#[test]
fn a_hook_that_names_a_missing_program_is_installed_and_not_alive() {
    let m = Machine::new();
    m.seed();
    let missing = m.dir.path().join("gone/thruline");
    m.write(
        &m.project_settings(),
        &serde_json::json!({"hooks": {"Stop": [{"hooks": [{"type": "command",
            "command": format!("{} sensor append Stop", missing.display())}]}]}})
        .to_string(),
    );
    let s = Server::start(&m);
    let doc = s.capture();
    let hooks = &doc["claude_code_hooks"];
    assert_eq!(hooks["installed"], true, "{doc}");
    assert_eq!(hooks["alive"], false, "{doc}");
    assert!(hooks["alive_reason"].is_string(), "{doc}");
    assert_eq!(hooks["missing_programs"], 1, "{doc}");
    assert_eq!(hooks["events"], serde_json::json!(["Stop"]), "{doc}");
    assert!(hooks["version"].is_null(), "{doc}");
    assert!(hooks["version_reason"].is_string(), "{doc}");
    let project = level(&doc, "project");
    assert_eq!(project["state"], "read", "{doc}");
    let c = &project["commands"][0];
    assert_eq!(c["program_exists"], false, "{doc}");
    assert_eq!(c["missing_program"], missing.display().to_string(), "{doc}");
    assert!(c["this_program"].is_null(), "{doc}");
}

#[test]
fn nothing_installed_reads_not_installed_and_no_liveness() {
    let m = Machine::new();
    m.seed();
    let s = Server::start(&m);
    let doc = s.capture();
    let hooks = &doc["claude_code_hooks"];
    assert_eq!(hooks["installed"], false, "{doc}");
    assert!(hooks["installed_reason"].is_null(), "{doc}");
    assert!(hooks["alive"].is_null(), "{doc}");
    assert!(hooks["alive_reason"].is_string(), "{doc}");
    assert_eq!(hooks["events"], serde_json::json!([]), "{doc}");
    assert!(hooks["version"].is_null(), "{doc}");
    assert_eq!(level(&doc, "user")["state"], "absent", "{doc}");
    // Not started with an observation source: said, not guessed.
    assert_eq!(doc["codex"]["configured"], false, "{doc}");
    assert!(doc["codex"]["state"].is_null(), "{doc}");
    assert!(doc["codex"]["paused"].is_null(), "{doc}");
    assert!(doc["codex"]["reason"].is_string(), "{doc}");
}

#[test]
fn an_unreadable_settings_file_reads_null_with_a_reason() {
    use std::os::unix::fs::PermissionsExt;
    let m = Machine::new();
    m.seed();
    m.write(&m.user_settings(), "{}");
    std::fs::set_permissions(m.user_settings(), std::fs::Permissions::from_mode(0o000))
        .expect("chmod");
    if std::fs::read_to_string(m.user_settings()).is_ok() {
        // Running as a user who reads through permissions (root): the case
        // cannot be built here, so say so rather than pass.
        panic!("this test cannot make a file unreadable for this user");
    }
    m.write(&m.project_settings(), "{never json");
    let s = Server::start(&m);
    let doc = s.capture();
    let hooks = &doc["claude_code_hooks"];
    assert!(hooks["installed"].is_null(), "{doc}");
    assert!(hooks["installed_reason"].is_string(), "{doc}");
    assert!(hooks["alive"].is_null(), "{doc}");
    assert!(hooks["alive_reason"].is_string(), "{doc}");
    let user = level(&doc, "user");
    assert_eq!(user["state"], "unreadable", "{doc}");
    assert!(user["reason"].is_string(), "{doc}");
    let project = level(&doc, "project");
    assert_eq!(project["state"], "not_json", "{doc}");
    assert!(project["reason"].is_string(), "{doc}");
    std::fs::set_permissions(m.user_settings(), std::fs::Permissions::from_mode(0o600))
        .expect("chmod back");
}

#[test]
fn git_is_detected_only_where_git_says_there_is_a_repository() {
    let m = Machine::new();
    m.seed();
    let s = Server::start(&m);
    let doc = s.capture();
    assert_eq!(doc["git"]["repository"], false, "{doc}");
    assert!(doc["git"]["toplevel"].is_null(), "{doc}");
    assert!(doc["git"]["reason"].is_null(), "{doc}");

    m.git_init();
    let doc = s.capture();
    assert_eq!(doc["git"]["repository"], true, "{doc}");
    let top = doc["git"]["toplevel"].as_str().expect("toplevel");
    let real = m.repo_path().canonicalize().expect("canonical");
    assert_eq!(PathBuf::from(top), real, "{doc}");
}

#[test]
fn the_first_step_is_read_from_the_record_and_hook_deliveries_are_counted() {
    let m = Machine::new();
    m.seed();
    let s = Server::start(&m);
    let doc = s.capture();
    let record = &doc["record"];
    assert_eq!(record["sessions"]["claude_code"], 1, "{doc}");
    assert_eq!(record["sessions"]["codex"], 0, "{doc}");
    assert_eq!(
        record["first_step"]["at"], "2026-10-01T09:01:00.000Z",
        "{doc}"
    );
    assert_eq!(record["first_step"]["agent"], "claude_code", "{doc}");
    assert_eq!(
        record["latest_step"]["at"], "2026-10-01T09:04:00.000Z",
        "{doc}"
    );
    assert_eq!(record["through_hooks"]["deliveries"], 0, "{doc}");
    assert!(record["through_hooks"]["latest_event"].is_null(), "{doc}");

    m.deliver("SessionStart");
    m.deliver("Stop");
    let doc = s.capture();
    let hooks = &doc["record"]["through_hooks"];
    assert_eq!(hooks["deliveries"], 2, "{doc}");
    assert_eq!(hooks["latest_event"], "Stop", "{doc}");
    assert!(hooks["first_at"].is_string(), "{doc}");
}

#[test]
fn a_project_with_no_recorded_step_reads_none() {
    let m = Machine::new();
    let s = Server::start(&m);
    let (_, overview) = s.get("/api/first-use");
    let key = overview["candidates"]
        .as_array()
        .and_then(|c| c.iter().find(|c| c["source"] == "here"))
        .and_then(|c| c["key"].as_str())
        .expect("this folder is offered")
        .to_string();
    let (code, body) = s.post("/api/first-use/register", &format!("{{\"key\":\"{key}\"}}"));
    assert_eq!(code, 200, "{body}");
    let doc = s.capture();
    let record = &doc["record"];
    assert_eq!(record["sessions"]["claude_code"], 0, "{doc}");
    assert_eq!(record["sessions"]["codex"], 0, "{doc}");
    assert!(record["first_step"].is_null(), "{doc}");
    assert!(record["latest_step"].is_null(), "{doc}");
    assert_eq!(record["through_hooks"]["deliveries"], 0, "{doc}");
    assert!(record["reason"].is_null(), "{doc}");
}

fn sha256(path: &PathBuf) -> Option<String> {
    use sha2::Digest;
    std::fs::read(path).ok().map(|b| {
        sha2::Sha256::digest(b)
            .iter()
            .map(|x| format!("{x:02x}"))
            .collect()
    })
}

#[test]
fn the_request_leaves_the_ledger_bytes_unchanged_and_answers_only_get() {
    let m = Machine::new();
    m.seed();
    m.deliver("Stop");
    let s = Server::start(&m);
    let id = s.project_id();
    // Settle whatever the server's start-up read leaves, through another route.
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let (_, list) = s.get("/api/hq/projects");
        if list["complete"] == true {
            break;
        }
        assert!(Instant::now() < deadline, "the list never completed");
        std::thread::sleep(Duration::from_millis(200));
    }
    assert_eq!(s.get(&format!("/api/hq/projects/{id}")).0, 200);
    let wal = m.db().with_extension("db-wal");
    let before = (sha256(&m.db()), sha256(&wal));
    assert!(before.0.is_some(), "no ledger at {}", m.db().display());
    for _ in 0..3 {
        assert_eq!(s.get(&format!("/api/hq/projects/{id}/capture")).0, 200);
    }
    assert_eq!((sha256(&m.db()), sha256(&wal)), before);

    let path = format!("/api/hq/projects/{id}/capture");
    assert_eq!(s.post(&path, "{}").0, 405);
    assert_eq!(s.get(&format!("{path}?x=1")).0, 400);
    let (code, body) = s.get("/api/hq/projects/nope/capture");
    assert_eq!(code, 404, "{body}");
    assert_eq!(body["error"], "no_such_project");
    assert_eq!((sha256(&m.db()), sha256(&wal)), before);
}

/// The deliveries through the hooks, computed the slow, exact way the route
/// first did (f2c719ce): every matching row read, kept by `seq`, then
/// counted. `roots` are the folders whose `cwd` counts; `members` the ledger
/// projects whose sessions count.
fn exact_through_hooks(db: &PathBuf, roots: &[String], members: &[String]) -> Value {
    use std::collections::BTreeMap;
    let conn =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("open the ledger read-only");
    let mut seen: BTreeMap<i64, (String, String)> = BTreeMap::new();
    for root in roots {
        let inside = format!("{root}/");
        let mut stmt = conn
            .prepare(
                "SELECT seq, received_at, hook_event FROM sensor_events \
                 WHERE cwd = ?1 OR substr(cwd, 1, ?2) = ?3",
            )
            .expect("prepare");
        let rows = stmt
            .query_map(
                (
                    root.as_str(),
                    inside.chars().count() as i64,
                    inside.as_str(),
                ),
                |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))),
            )
            .expect("query");
        for row in rows {
            let (seq, at_event) = row.expect("row");
            seen.insert(seq, at_event);
        }
    }
    for m in members {
        let mut stmt = conn
            .prepare(
                "SELECT seq, received_at, hook_event FROM sensor_events \
                 WHERE session_id IN (SELECT id FROM sessions WHERE project_id = ?1)",
            )
            .expect("prepare");
        let rows = stmt
            .query_map([m], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))
            .expect("query");
        for row in rows {
            let (seq, at_event) = row.expect("row");
            seen.insert(seq, at_event);
        }
    }
    serde_json::json!({
        "deliveries": seen.len(),
        "first_at": seen.values().map(|(at, _)| at.as_str()).min(),
        "latest_at": seen.values().map(|(at, _)| at.as_str()).max(),
        "latest_event": seen.values().next_back().map(|(_, e)| e.as_str()),
    })
}

/// The route keeps a tally of the deliveries per project and reads only the
/// rows appended since (the speed fix after C30 R-ROUTES). This pins its
/// answer to the exact computation above after every kind of change a tally
/// could miss: the first read, rows appended since, nothing appended, and a
/// session newly filed under the project whose deliveries were made before it
/// was.
#[test]
fn the_delivery_tally_equals_the_exact_computation_after_every_change() {
    let m = Machine::new();
    m.seed();
    let repo = m.repo();
    let inside = format!("{repo}/src/deep");
    let sibling = format!("{repo}-other");
    let elsewhere = m.dir.path().join("elsewhere").display().to_string();
    // Many deliveries, of every kind the rules tell apart.
    let batch = |round: usize| {
        for i in 0..12 {
            let event = ["PostToolUse", "Stop", "UserPromptSubmit"][(i + round) % 3];
            m.deliver_as(event, Some("s1"), Some(&repo)); // root, its session
            m.deliver_as(event, Some("y"), Some(&inside)); // inside, another session
            m.deliver_as(event, Some("y"), Some(&sibling)); // a prefix, not inside
            m.deliver_as(event, Some("s1"), Some(&elsewhere)); // its session, elsewhere
            m.deliver_as(event, Some("s2"), Some(&elsewhere)); // not filed yet
            m.deliver_as(event, Some("z"), None); // nothing that matches
            if i % 4 == 0 {
                m.deliver_as(event, Some("s1"), None); // its session, no folder
                m.deliver_as(event, None, Some(&repo)); // its folder, no session
            }
        }
    };
    batch(0);
    let s = Server::start(&m);
    let id = s.project_id();
    let members = || -> Vec<String> {
        let conn = rusqlite::Connection::open_with_flags(
            m.db(),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("open");
        let mut stmt = conn.prepare("SELECT id FROM projects").expect("prepare");
        let ids: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .expect("query")
            .map(|r| r.expect("row"))
            .collect();
        ids
    };
    let check = |label: &str| -> u64 {
        let (code, doc) = s.get(&format!("/api/hq/projects/{id}/capture"));
        assert_eq!(code, 200, "{doc}");
        let root = doc["project"]["root"].as_str().expect("root").to_string();
        assert_eq!(root, repo, "{doc}");
        let exact = exact_through_hooks(&m.db(), &[root], &members());
        assert_eq!(doc["record"]["through_hooks"], exact, "{label}: {doc}");
        exact["deliveries"].as_u64().expect("count")
    };

    let first = check("the first read");
    // 12 at the root, 12 inside, 12 by session elsewhere, 3 + 3 missing a field.
    assert_eq!(first, 42);
    assert_eq!(check("nothing appended"), first);

    batch(1);
    let second = check("rows appended since");
    assert_eq!(second, 2 * first);

    // `s2` is filed under the project only now; its earlier deliveries, made
    // from a folder that is not the project's, count from this moment.
    m.seed_second();
    let third = check("a session newly filed under the project");
    assert_eq!(third, second + 24);

    m.deliver_as("SessionEnd", Some("s2"), Some(&elsewhere));
    assert_eq!(check("one more, by the new session"), third + 1);
    assert_eq!(check("nothing appended, again"), third + 1);
}
