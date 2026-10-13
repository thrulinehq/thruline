//! HQ Next "What counts as done" (C30 R-ROUTES "Done-Criteria") against the
//! shipping binary: `thruline ui` on a temporary HOME and store, a synthetic
//! Claude Code recording, the socket it serves. Never the real `~/.thruline`.
//!
//! * `POST /api/changes/<id>/scope/accept` carries the person's own lines
//!   (`lines`) and what they marked out of scope (`exclusions`) into the one
//!   accepted version, refuses a part it cannot write (409, naming it) before
//!   anything is written, and refuses an unknown field rather than dropping it.
//! * `GET /api/hq/projects/<id>/done-templates` says what the project's
//!   `.thruline/policy.yml` holds for templates, and writes nothing.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

/// One checkable sentence (a proposal) and one that is not.
const PROMPT: &str =
    "When the input is empty, the parser returns an error. Fix the parser in src/a.rs.";

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
    fn repo_path(&self) -> PathBuf {
        self.dir.path().join("repo")
    }
    fn repo(&self) -> String {
        self.repo_path().display().to_string()
    }
    fn db(&self) -> PathBuf {
        self.home().join(".thruline/ledger.db")
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
            .env_remove("GITHUB_BASE_REF")
            .env_remove("GITHUB_EVENT_NAME")
            .current_dir(self.repo_path());
        cmd
    }
    /// One recording: the prompt, an edit of `src/a.rs`, its result, a reply.
    fn record(&self) {
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
                serde_json::json!({"role": "user", "content": PROMPT}),
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
                {"type": "text", "text": "Done. I updated `src/a.rs`."}]}),
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
    fn ledger_bytes(&self) -> Vec<u8> {
        let mut out = std::fs::read(self.db()).unwrap_or_default();
        out.extend(std::fs::read(self.db().with_extension("db-wal")).unwrap_or_default());
        out
    }
    fn git(&self, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(self.repo_path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .expect("git");
        assert!(output.status.success(), "git {args:?}: {output:?}");
    }
    fn commit_policy(&self, message: &str) {
        self.git(&["add", ".thruline/policy.yml"]);
        self.git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            message,
        ]);
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

    fn send(&self, head: &[u8]) -> (u16, Value) {
        let mut s = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        s.set_read_timeout(Some(Duration::from_secs(60)))
            .expect("timeout");
        // A refusal can come before the whole body is read (413).
        let _ = s.write_all(head);
        let mut raw = Vec::new();
        let _ = s.read_to_end(&mut raw);
        let text = String::from_utf8_lossy(&raw);
        let status: u16 = text[9..12].parse().expect("status");
        let body = text.split_once("\r\n\r\n").map(|x| x.1).unwrap_or("");
        (status, serde_json::from_str(body).unwrap_or(Value::Null))
    }

    fn get(&self, path: &str) -> (u16, Value) {
        self.send(
            format!(
                "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
                self.port
            )
            .as_bytes(),
        )
    }

    fn post(&self, path: &str, body: &str) -> (u16, Value) {
        self.send(
            format!(
                "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{p}\r\nOrigin: http://127.0.0.1:{p}\r\nContent-Type: application/json\r\nContent-Length: {n}\r\nConnection: close\r\n\r\n{body}",
                p = self.port,
                n = body.len()
            )
            .as_bytes(),
        )
    }

    /// The project list once the start-up warm has every row.
    fn project(&self) -> String {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let (_, list) = self.get("/api/hq/projects");
            if list["complete"] == true {
                let projects = list["projects"].as_array().expect("projects");
                assert_eq!(projects.len(), 1, "{list}");
                return projects[0]["id"].as_str().expect("id").to_string();
            }
            assert!(
                Instant::now() < deadline,
                "the list never completed: {list}"
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// "Track this work as a Change" on the one thread: the Change's id.
    fn track(&self) -> String {
        let project = self.project();
        let (_, view) = self.get(&format!("/api/hq/projects/{project}"));
        let thread = view["threads"][0]["id"].as_str().expect("a thread");
        let (code, written) = self.post(
            "/api/first-use/change",
            &format!(r#"{{"project":"{project}","thread":"{thread}"}}"#),
        );
        assert_eq!(code, 200, "{written}");
        written["change"].as_str().expect("change").to_string()
    }

    fn change(&self, id: &str) -> Value {
        let (code, doc) = self.get(&format!("/api/changes/{id}"));
        assert_eq!(code, 200, "{doc}");
        doc["change"].clone()
    }

    fn accept(&self, id: &str, body: Value) -> (u16, Value) {
        self.post(
            &format!("/api/changes/{id}/scope/accept"),
            &body.to_string(),
        )
    }
}

fn versions(change: &Value) -> usize {
    change["scope_history"].as_array().map_or(0, |h| h.len())
        + usize::from(!change["accepted_scope"].is_null())
}

#[test]
fn a_persons_line_and_what_they_marked_out_of_scope_reach_the_accepted_version() {
    let m = Machine::new();
    m.record();
    let s = Server::start(&m);
    let id = s.track();
    let (code, proposals) = s.get(&format!("/api/changes/{id}/proposals"));
    assert_eq!(code, 200, "{proposals}");
    let first = proposals["proposals"]["criteria"][0]["text"]
        .as_str()
        .expect("a proposal")
        .to_string();

    let mine = "The parser names the line it stopped at.";
    let (code, answer) = s.accept(
        &id,
        serde_json::json!({
            "criteria": [1], "lines": [mine], "exclusions": ["error messages in other languages"],
            "allowed_scope": ["src/**"], "actor": "you",
        }),
    );
    assert_eq!(code, 200, "{answer}");
    assert_eq!(answer["accepted"]["version"], 1, "{answer}");
    let attributions: Vec<&str> = answer["accepted"]["criteria"]
        .as_array()
        .expect("criteria")
        .iter()
        .map(|c| c["attribution"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(attributions, vec!["as_proposed", "your_words"], "{answer}");

    let change = s.change(&id);
    let scope = &change["accepted_scope"];
    assert_eq!(scope["version"], 1, "{change}");
    assert_eq!(
        scope["criteria"],
        serde_json::json!([first, mine]),
        "{change}"
    );
    assert_eq!(
        scope["exclusions"],
        serde_json::json!(["error messages in other languages"]),
        "{change}"
    );
    assert_eq!(scope["allowed_scope"], serde_json::json!(["src/**"]));
    assert_eq!(scope["acceptance"]["actor"], "you");
    // Accepting is not approving.
    assert!(
        change["decisions"]["history"]
            .as_array()
            .is_none_or(|h| h.is_empty()),
        "{change}"
    );
}

#[test]
fn the_persons_own_lines_alone_are_accepted() {
    let m = Machine::new();
    m.record();
    let s = Server::start(&m);
    let id = s.track();
    let (code, answer) = s.accept(
        &id,
        serde_json::json!({"lines": ["The parser keeps the input it was given."], "actor": "you"}),
    );
    assert_eq!(code, 200, "{answer}");
    let change = s.change(&id);
    assert_eq!(
        change["accepted_scope"]["criteria"],
        serde_json::json!(["The parser keeps the input it was given."])
    );
    // And with no lines and no choice, the old refusal stands.
    let (code, answer) = s.accept(&id, serde_json::json!({"actor": "you"}));
    assert_eq!(code, 400, "{answer}");
    assert_eq!(answer["error"], "say_all_or_which");
}

#[test]
fn a_part_that_cannot_be_written_is_refused_by_name_and_nothing_is_written() {
    let m = Machine::new();
    m.record();
    let s = Server::start(&m);
    let id = s.track();
    let before = m.ledger_bytes();
    let long = "x".repeat(241);
    let cases = [
        (
            serde_json::json!({"criteria": [1], "lines": [long], "actor": "you"}),
            "too_long",
            "lines",
        ),
        (
            serde_json::json!({"criteria": [1], "exclusions": ["fine", "  "], "actor": "you"}),
            "empty",
            "exclusions",
        ),
        (
            serde_json::json!({"criteria": [1], "lines": vec!["a line"; 41], "actor": "you"}),
            "too_many",
            "lines",
        ),
        (
            serde_json::json!({"criteria": [1], "exclusions": vec!["one thing"; 26], "actor": "you"}),
            "too_many",
            "exclusions",
        ),
    ];
    for (body, error, part) in cases {
        let (code, answer) = s.accept(&id, body.clone());
        assert_eq!(code, 409, "{body}: {answer}");
        assert_eq!(answer["error"], error, "{answer}");
        assert_eq!(answer["part"], part, "{answer}");
        assert!(
            answer["plain"]
                .as_str()
                .is_some_and(|p| p.contains("Nothing was saved.")),
            "{answer}"
        );
    }
    // A misspelt field is refused, never dropped.
    let (code, answer) = s.accept(
        &id,
        serde_json::json!({"criteria": [1], "exclusion": ["refunds"], "actor": "you"}),
    );
    assert_eq!(code, 400, "{answer}");
    assert_eq!(answer["error"], "malformed_request");
    // A line that repeats a chosen proposal is the core's refusal, in words.
    let (_, proposals) = s.get(&format!("/api/changes/{id}/proposals"));
    let first = proposals["proposals"]["criteria"][0]["text"]
        .as_str()
        .expect("a proposal")
        .to_lowercase();
    let (code, answer) = s.accept(
        &id,
        serde_json::json!({"criteria": [1], "lines": [first], "actor": "you"}),
    );
    assert_eq!(code, 400, "{answer}");
    assert_eq!(answer["error"], "refused");
    assert!(
        answer["detail"]
            .as_str()
            .is_some_and(|d| d.contains("twice")),
        "{answer}"
    );
    assert_eq!(versions(&s.change(&id)), 0, "a refusal wrote a version");
    assert_eq!(m.ledger_bytes(), before, "a refusal changed the ledger");
}

#[test]
fn the_largest_body_a_person_can_send_fits_and_a_larger_one_is_refused_whole() {
    let m = Machine::new();
    m.record();
    let s = Server::start(&m);
    let id = s.track();
    let lines: Vec<String> = (0..40)
        .map(|i| format!("{i:02} {}", "y".repeat(237)))
        .collect();
    let exclusions: Vec<String> = (0..25)
        .map(|i| format!("{i:02} {}", "z".repeat(237)))
        .collect();
    let body = serde_json::json!({"lines": lines, "exclusions": exclusions, "actor": "you"});
    assert!(body.to_string().len() > 512, "larger than the old cap");
    let (code, answer) = s.accept(&id, body);
    assert_eq!(code, 200, "{answer}");
    assert_eq!(
        s.change(&id)["accepted_scope"]["criteria"]
            .as_array()
            .map(|c| c.len()),
        Some(40)
    );
    // A body declared larger than the cap is refused before it is read.
    let (code, answer) = s.send(
        format!(
            "POST /api/changes/{id}/scope/accept HTTP/1.1\r\nHost: 127.0.0.1:{p}\r\nOrigin: http://127.0.0.1:{p}\r\nContent-Type: application/json\r\nContent-Length: {n}\r\nConnection: close\r\n\r\n",
            p = s.port,
            n = 400 * 1024
        )
        .as_bytes(),
    );
    assert_eq!(code, 413, "{answer}");
    assert_eq!(answer["error"], "request_too_large");
}

#[test]
fn done_templates_say_what_the_policy_file_holds_and_write_nothing() {
    let m = Machine::new();
    m.record();
    m.git(&["init", "-b", "main"]);
    std::fs::write(m.repo_path().join("README.md"), "synthetic repository\n").expect("readme");
    m.git(&["add", "README.md"]);
    m.git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "-m",
        "first",
    ]);
    let s = Server::start(&m);
    let project = s.project();
    let address = format!("/api/hq/projects/{project}/done-templates");
    let before = m.ledger_bytes();

    let (code, doc) = s.get(&address);
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["schema_version"], 1);
    assert_eq!(doc["project"]["id"], project.as_str());
    assert_eq!(doc["policy"]["path"], ".thruline/policy.yml");
    assert_eq!(doc["policy"]["state"], "absent", "{doc}");
    assert_eq!(doc["reason"], "no_policy_file");
    assert_eq!(doc["templates"], serde_json::json!([]));

    // `thruline policy --init` writes the documented example to the working
    // tree. The route keeps reading the base commit until the file is committed.
    let out = m
        .cmd(&["policy", "--init"])
        .output()
        .expect("policy --init");
    assert!(out.status.success(), "{out:?}");
    let (code, doc) = s.get(&address);
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["policy"]["state"], "absent", "{doc}");
    m.commit_policy("policy example");
    let (code, doc) = s.get(&address);
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["policy"]["state"], "read", "{doc}");
    assert!(doc["policy"]["detail"].is_null());
    assert_eq!(doc["reason"], "no_templates_section");
    assert_eq!(doc["templates"], serde_json::json!([]));

    // Named templates are served from the base commit, with the exact lines
    // the person can pick as proposals. A divergent working tree is ignored.
    let policy = m.repo_path().join(".thruline/policy.yml");
    std::fs::write(&policy, "version: 1\ntemplates:\n  bug_fix:\n    lines: [\"Handle empty input, with an error.\", \"Keep the original input.\"]\n").expect("write");
    m.commit_policy("offer done template");
    std::fs::write(&policy, "version: 1\ntemplates:\n  wrong_worktree:\n    lines: [\"This is not on the base branch.\"]\n").expect("change working tree");
    let (code, doc) = s.get(&address);
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["policy"]["state"], "read", "{doc}");
    assert!(doc["reason"].is_null(), "{doc}");
    assert_eq!(
        doc["templates"],
        serde_json::json!([{"name":"bug_fix","lines":["Handle empty input, with an error.", "Keep the original input."]}])
    );

    // A committed file that the gate cannot parse is said, with its reason.
    std::fs::write(&policy, "version: 1\ntemplates:\n  bug_fix: x\n").expect("write");
    m.commit_policy("invalid template");
    let (code, doc) = s.get(&address);
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["policy"]["state"], "unreadable", "{doc}");
    assert_eq!(doc["reason"], "policy_not_read");
    assert!(
        doc["policy"]["detail"]
            .as_str()
            .is_some_and(|d| d.contains("templates.bug_fix")),
        "{doc}"
    );

    // A link in the base tree is not followed.
    std::fs::remove_file(&policy).expect("remove");
    let elsewhere = m.dir.path().join("elsewhere.yml");
    std::fs::write(&elsewhere, "version: 1\n").expect("write");
    std::os::unix::fs::symlink(&elsewhere, &policy).expect("symlink");
    m.commit_policy("symlink policy");
    let (code, doc) = s.get(&address);
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["policy"]["state"], "not_a_regular_file", "{doc}");

    // With no identifiable base, never fall back to the checkout's symlink
    // or to a newly written working-tree policy.
    m.git(&["checkout", "-b", "feature"]);
    m.git(&["branch", "-D", "main"]);
    std::fs::remove_file(&policy).expect("remove symlink");
    std::fs::write(
        &policy,
        "version: 1\ntemplates:\n  forged:\n    lines: [\"not offered\"]\n",
    )
    .expect("working tree policy");
    let (code, doc) = s.get(&address);
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["policy"]["state"], "unreadable", "{doc}");
    assert_eq!(doc["reason"], "policy_not_read", "{doc}");
    assert_eq!(doc["templates"], serde_json::json!([]));

    // Nothing else is answered here.
    let (code, doc) = s.get(&format!("{address}?x=1"));
    assert_eq!(code, 400, "{doc}");
    let (code, doc) = s.post(&address, "{}");
    assert_eq!(code, 405, "{doc}");
    let (code, doc) = s.get("/api/hq/projects/0000000000000000/done-templates");
    assert_eq!(code, 404, "{doc}");
    assert_eq!(doc["error"], "no_such_project");
    assert_eq!(
        m.ledger_bytes(),
        before,
        "reading templates changed the ledger"
    );
}
