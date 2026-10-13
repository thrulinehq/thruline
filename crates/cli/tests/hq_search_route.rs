//! `GET /api/hq/projects/<id>/search` (C30 R-ROUTES "Record search", census
//! §3.08 row 4) against the shipping binary: `thruline ui` with a temporary
//! HOME, synthetic Claude Code recordings, and the socket it serves. GET only;
//! nothing here opens a real store.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

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
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .current_dir(self.dir.path().join("repo"));
        cmd
    }
    /// One session: a prompt, then tool calls (name, input, result), then
    /// the agent's last words.
    fn record(
        &self,
        session: &str,
        day: &str,
        prompt: &str,
        calls: &[(&str, Value, &str)],
        said: &str,
    ) {
        let cwd = self.repo();
        std::fs::create_dir_all(self.project_dir()).expect("dir");
        let mut n = 0u32;
        let mut line = |kind: &str, message: Value| {
            n += 1;
            json!({
                "type": kind, "uuid": format!("{session}-{n}"), "parentUuid": null,
                "isSidechain": false, "sessionId": session,
                "timestamp": format!("{day}T09:{:02}:{:02}.000Z", n / 60, n % 60),
                "cwd": cwd, "message": message,
            })
            .to_string()
        };
        let mut lines = vec![line("user", json!({"role": "user", "content": prompt}))];
        for (k, (name, input, result)) in calls.iter().enumerate() {
            let tu = format!("{session}-t{k}");
            lines.push(line(
                "assistant",
                json!({"role": "assistant", "content": [{"type": "tool_use", "id": tu, "name": name, "input": input}]}),
            ));
            lines.push(line(
                "user",
                json!({"role": "user", "content": [{"type": "tool_result", "tool_use_id": tu, "content": result, "is_error": false}]}),
            ));
        }
        lines.push(line(
            "assistant",
            json!({"role": "assistant", "content": [{"type": "text", "text": said}]}),
        ));
        std::fs::write(
            self.project_dir().join(format!("{session}.jsonl")),
            lines.join("\n") + "\n",
        )
        .expect("recording");
    }
    fn seed(&self) {
        let file = |p: &str| format!("{}/{p}", self.repo());
        self.record(
            "cccccccc-5b5b-4b5b-8b5b-000000000001",
            "2026-10-01",
            "Fix the checkout total in cart/totals.py.",
            &[
                ("Read", json!({"file_path": file("cart/totals.py")}), "def total(): ..."),
                ("Edit", json!({"file_path": file("cart/totals.py"), "old_string": "a", "new_string": "b"}), "ok"),
                ("Bash", json!({"command": "pytest -q tests/test_totals.py"}), "4 passed in 0.12s"),
                ("Bash", json!({"command": "git commit -m \"Apply the discount once\""}), "[main 3f9a1c2] Apply the discount once\n 1 file changed"),
            ],
            "Checkout total fixed. All 4 tests pass.",
        );
        self.record(
            "cccccccc-5b5b-4b5b-8b5b-000000000002",
            "2026-10-02",
            "Write the migration guide in docs/migrate.md.",
            &[
                (
                    "Read",
                    json!({"file_path": file("docs/old-guide.md")}),
                    "# Old",
                ),
                (
                    "Write",
                    json!({"file_path": file("docs/migrate.md"), "content": "# Migrating"}),
                    "File created successfully.",
                ),
            ],
            "Done. I added the migration guide to `docs/migrate.md`.",
        );
        let status = self
            .cmd(&["ingest", "--apply", "--source", "claude-code"])
            .arg(self.project_dir())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("ingest");
        assert!(status.success());
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
    fn post(&self, path: &str) -> (u16, Value) {
        self.send(&format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{p}\r\nOrigin: http://127.0.0.1:{p}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            p = self.port
        ))
    }
    /// The project id and its view, once the start-up warm has its row.
    fn project(&self) -> (String, Value) {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let (_, list) = self.get("/api/hq/projects");
            if list["complete"] == true {
                let id = list["projects"][0]["id"].as_str().expect("id").to_string();
                let (code, view) = self.get(&format!("/api/hq/projects/{id}"));
                assert_eq!(code, 200, "{view}");
                return (id, view);
            }
            assert!(
                Instant::now() < deadline,
                "the list never completed: {list}"
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    }
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

fn thread_titled(view: &Value, prefix: &str) -> String {
    view["threads"]
        .as_array()
        .and_then(|ts| {
            ts.iter()
                .find(|t| t["title"].as_str().is_some_and(|x| x.starts_with(prefix)))
        })
        .and_then(|t| t["id"].as_str())
        .unwrap_or_else(|| panic!("a thread titled {prefix}…: {view}"))
        .to_string()
}

fn hits<'a>(doc: &'a Value, kind: &str) -> Vec<&'a Value> {
    doc["results"]
        .as_array()
        .map(|r| r.iter().filter(|h| h["kind"] == kind).collect())
        .unwrap_or_default()
}

#[test]
fn a_warm_search_never_matches_an_excluded_path() {
    let m = Machine::new();
    m.seed();
    let s = Server::start(&m);
    let (id, _) = s.project();
    let search = format!("/api/hq/projects/{id}/search?q=cart%2Ftotals.py");
    let (code, before) = s.get(&search);
    assert_eq!(code, 200, "{before}");
    assert!(before["total"].as_u64().is_some_and(|n| n > 0));
    assert_eq!(s.get(&search).1, before, "warm corpus");

    let status = m
        .cmd(&["capture", "exclude", "--project", &id, "cart/totals.py"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("exclude");
    assert!(status.success());
    let (code, hidden) = s.get(&search);
    assert_eq!(code, 200, "{hidden}");
    assert_eq!(
        hidden["total"], 0,
        "excluded path cannot be searched: {hidden}"
    );
    assert!(
        hidden["left_out_by_exclusion"]
            .as_u64()
            .is_some_and(|n| n > 0),
        "the removed entries are counted: {hidden}"
    );
    assert_eq!(s.get(&search).1, hidden, "warm filtered corpus");
}

#[test]
fn a_generated_exclusion_uses_the_repositories_git_attribute_before_searching() {
    let m = Machine::new();
    let status = Command::new("git")
        .args(["-C", &m.repo(), "init", "--quiet"])
        .status()
        .expect("git init");
    assert!(status.success());
    std::fs::write(
        PathBuf::from(m.repo()).join(".gitattributes"),
        "cart/totals.py linguist-generated\nBUILDLOG linguist-generated\n",
    )
    .expect("attributes");
    m.record(
        "cccccccc-5b5b-4b5b-8b5b-000000000003",
        "2026-10-03",
        "Inspect the generated output.",
        &[("Bash", json!({"command": "cat BUILDLOG"}), "secret output")],
        "Done.",
    );
    m.seed();
    let s = Server::start(&m);
    let (id, _) = s.project();
    let search = format!("/api/hq/projects/{id}/search?q=cart%2Ftotals.py");
    assert!(s.get(&search).1["total"].as_u64().is_some_and(|n| n > 0));
    let status = m
        .cmd(&["capture", "exclude", "--project", &id, "generated"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("exclude generated");
    assert!(status.success());
    let (code, hidden) = s.get(&search);
    assert_eq!(code, 200, "{hidden}");
    assert_eq!(hidden["total"], 0, "{hidden}");
    assert!(hidden["left_out_by_exclusion"]
        .as_u64()
        .is_some_and(|n| n > 0));
    let bare = format!("/api/hq/projects/{id}/search?q=BUILDLOG");
    assert_eq!(
        s.get(&bare).1["total"],
        0,
        "generated bare command path is hidden"
    );

    // The index and exclusion keys do not move when Git attributes change.
    // A warm search must still read the current generated-file facts.
    std::fs::write(PathBuf::from(m.repo()).join(".gitattributes"), "")
        .expect("remove generated attributes");
    let (_, shown_again) = s.get(&search);
    assert!(
        shown_again["total"].as_u64().is_some_and(|n| n > 0),
        "attribute removal restores the warm search: {shown_again}"
    );
    assert!(s.get(&bare).1["total"].as_u64().is_some_and(|n| n > 0));
}

#[test]
fn reads_and_versions_are_found_across_changes_each_with_its_change() {
    let m = Machine::new();
    m.seed();
    let s = Server::start(&m);
    let (id, view) = s.project();
    let a = thread_titled(&view, "Fix the checkout total");
    let b = thread_titled(&view, "Write the migration guide");

    let (code, doc) = s.get(&format!("/api/hq/projects/{id}/search?q=totals.py"));
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["schema_version"], 1);
    assert_eq!(doc["project_id"], id.as_str());
    let read = hits(&doc, "Read");
    assert_eq!(read.len(), 1, "{doc}");
    assert_eq!(read[0]["text"], "cart/totals.py");
    assert_eq!(read[0]["thread_id"], a.as_str());
    assert!(read[0]["thread_title"]
        .as_str()
        .is_some_and(|t| t.starts_with("Fix the checkout total")));
    assert!(doc["masking"]["complete"].is_boolean(), "{doc}");
    assert_eq!(
        doc["by_kind"]
            .as_object()
            .map(|o| o.values().filter_map(Value::as_u64).sum::<u64>()),
        doc["total"].as_u64()
    );

    let (_, doc) = s.get(&format!("/api/hq/projects/{id}/search?q=old-guide"));
    assert_eq!(hits(&doc, "Read")[0]["thread_id"], b.as_str(), "{doc}");

    let (_, doc) = s.get(&format!("/api/hq/projects/{id}/search?q=3f9a1c2"));
    let v = hits(&doc, "Version");
    assert_eq!(v.len(), 1, "{doc}");
    assert_eq!(v[0]["thread_id"], a.as_str());
    assert_eq!(v[0]["version"]["message"], "Apply the discount once");
    assert_eq!(v[0]["version"]["committed"], true);

    // every hit names a thread of the project view, which addresses its Change
    let ids: Vec<&str> = view["threads"]
        .as_array()
        .expect("threads")
        .iter()
        .filter_map(|t| t["id"].as_str())
        .collect();
    let (_, doc) = s.get(&format!("/api/hq/projects/{id}/search?q=a&limit=100"));
    for h in doc["results"].as_array().expect("results") {
        assert!(ids.contains(&h["thread_id"].as_str().unwrap_or("")), "{h}");
    }
}

#[test]
fn the_open_changes_own_steps_are_left_to_its_view() {
    let m = Machine::new();
    m.seed();
    let s = Server::start(&m);
    let (id, view) = s.project();
    let a = thread_titled(&view, "Fix the checkout total");
    let (code, doc) = s.get(&format!("/api/hq/projects/{id}/search?q=totals&thread={a}"));
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["thread"], a.as_str());
    for h in doc["results"].as_array().expect("results") {
        if h["thread_id"] == a.as_str() {
            assert!(h["kind"] == "Read" || h["kind"] == "Version", "{h}");
            assert_eq!(h["open_thread"], true);
        }
    }
    assert_eq!(hits(&doc, "Read").len(), 1, "{doc}");
    let (code, body) = s.get(&format!(
        "/api/hq/projects/{id}/search?q=totals&thread=0000000000000000"
    ));
    assert_eq!(
        (code, body["error"].as_str()),
        (404, Some("no_such_thread"))
    );
}

#[test]
fn the_search_refuses_what_it_does_not_understand_and_the_other_routes_still_take_no_query() {
    let m = Machine::new();
    m.seed();
    let s = Server::start(&m);
    let (id, _) = s.project();
    let base = format!("/api/hq/projects/{id}/search");
    for (q, code, error) in [
        ("", 400, "missing_query"),
        ("?limit=5", 400, "missing_query"),
        ("?q=+", 400, "missing_query"),
        ("?q=a&q=b", 400, "malformed_request"),
        ("?q=a&limit=0", 400, "bad_limit"),
        ("?q=a&limit=101", 400, "bad_limit"),
        ("?q=a&page=2", 400, "unknown_query_parameter"),
        ("?q=%ZZ", 400, "malformed_request"),
    ] {
        let (got, body) = s.get(&format!("{base}{q}"));
        assert_eq!(
            (got, body["error"].as_str()),
            (code, Some(error)),
            "{q}: {body}"
        );
    }
    let long = "a".repeat(201);
    assert_eq!(
        s.get(&format!("{base}?q={long}")).1["error"],
        "query_too_long"
    );
    assert_eq!(s.get("/api/hq/projects/nope/search?q=a").0, 404);
    assert_eq!(s.post(&format!("{base}?q=a")).0, 405);
    assert_eq!(
        s.get(&format!("/api/hq/projects/{id}?q=a")).1["error"],
        "unknown_query_parameter"
    );
    assert_eq!(
        s.get(&format!("/api/hq/projects/{id}/statements?q=a")).0,
        400
    );
}

#[test]
fn searching_writes_nothing_and_a_second_search_answers_the_same() {
    let m = Machine::new();
    m.seed();
    let s = Server::start(&m);
    let (id, _) = s.project();
    let wal = m.db().with_extension("db-wal");
    let before = (sha256(&m.db()), sha256(&wal));
    assert!(before.0.is_some(), "no ledger at {}", m.db().display());
    let first = s.get(&format!("/api/hq/projects/{id}/search?q=guide"));
    for _ in 0..3 {
        assert_eq!(
            s.get(&format!("/api/hq/projects/{id}/search?q=guide")),
            first
        );
    }
    assert_eq!(first.0, 200);
    assert!(first.1["total"].as_u64().unwrap_or(0) > 0, "{}", first.1);
    assert_eq!((sha256(&m.db()), sha256(&wal)), before);
}
