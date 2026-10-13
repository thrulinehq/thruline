//! The first-use path, against the shipping binary (T-2-7).
//!
//! WHAT THIS FILE IS FOR. `03-USER-LENS.md` rule 8 and §4 step 1: from the
//! documented command to a true statement about the person's own project, with
//! no documentation open. The baseline's H2T-1 is the moment it removes — the
//! README quickstart never opened a browser and dead-ended at "0 transcript
//! file(s)", and the first true statement anybody reached was about demo data.
//!
//! EVERY TEST RUNS THE DOCUMENTED COMMAND. `thruline ui` is started as a
//! subprocess with a temporary `HOME` and a temporary `THRULINE_HOME`, and the
//! assertions are made against the socket it serves. Nothing here reaches into
//! a function directly: the thing being measured is the product a person meets.
//!
//! EVERY FIXTURE IS SYNTHETIC. The recordings are written by this file, under a
//! temporary HOME, in the shape the vendor writes them. No string here came off
//! a real machine, and no test in this file reads a real store.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

fn thruline() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

/// A machine: a temporary HOME, a repository, and whatever recordings a test
/// decides this machine has already made.
struct Machine {
    dir: tempfile::TempDir,
    /// The repository's folder name. A PARAMETER, because the ordinary way
    /// people name repositories — `uploads-api`, `my-app`, `web-service` — is
    /// the one this path used to get wrong, and a suite that only ever tests
    /// `repo` cannot see it.
    repo: &'static str,
}

impl Machine {
    fn new() -> Machine {
        Machine::named("repo")
    }

    fn named(repo: &'static str) -> Machine {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("h/.thruline")).expect("home");
        std::fs::create_dir_all(dir.path().join(repo).join("src")).expect("repo");
        std::fs::create_dir_all(dir.path().join(repo).join(".git")).expect("git");
        Machine { dir, repo }
    }

    fn home(&self) -> PathBuf {
        self.dir.path().join("h")
    }

    fn repo(&self) -> PathBuf {
        self.dir.path().join(self.repo)
    }

    fn env(&self, cmd: &mut Command) {
        cmd.env("HOME", self.home())
            .env("THRULINE_HOME", self.home().join(".thruline"))
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .current_dir(self.repo());
    }

    /// One Claude Code recording, in the place and the shape the vendor writes
    /// them: one directory per repository under `~/.claude/projects`, one JSONL
    /// file per session, `cwd` on the opening line.
    ///
    /// THE FOLDER NAME IS MANGLED THE WAY THE VENDOR MANGLES IT, and that is
    /// not `/` → `-` alone. Measured on this machine against the real
    /// `~/.claude/projects` on 2026-09-18 (26 folders whose recordings state a
    /// `cwd`): replacing only `/` reproduces 12 of the 26 folder names and
    /// fails on the other 14, every one of which holds a `.`, a `_` or a space
    /// in its path — the vendor turns those into `-` too. A fixture that
    /// mangles only `/` is a fixture of a machine that does not exist, and it
    /// is why the first version of this suite could not see the defect it was
    /// written to cover.
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
        let file = dir.join(format!("{session}.jsonl"));
        let lines = [
            format!(
                r#"{{"type":"user","uuid":"{session}-e1","parentUuid":null,"isSidechain":false,"sessionId":"{session}","timestamp":"{day}T09:00:00.100Z","cwd":"{cwd}","message":{{"role":"user","content":"{asked_for}"}}}}"#
            ),
            format!(
                r#"{{"type":"assistant","uuid":"{session}-e2","parentUuid":"{session}-e1","isSidechain":false,"sessionId":"{session}","timestamp":"{day}T09:01:00.100Z","cwd":"{cwd}","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"t1","name":"Edit","input":{{"file_path":"{cwd}/{touched}","old_string":"a","new_string":"b"}}}}]}}}}"#
            ),
            format!(
                r#"{{"type":"assistant","uuid":"{session}-e3","parentUuid":"{session}-e2","isSidechain":false,"sessionId":"{session}","timestamp":"{day}T09:02:00.100Z","cwd":"{cwd}","message":{{"role":"assistant","content":[{{"type":"text","text":"Done."}}]}}}}"#
            ),
        ];
        std::fs::write(&file, lines.join("\n") + "\n").expect("recording");
    }

    /// The documented command: `thruline ui`, from the repository, with the
    /// browser left alone and a port the machine chooses.
    fn ui(&self) -> Command {
        let mut cmd = Command::new(thruline());
        cmd.args(["ui", "--no-open", "--port", "0"]);
        self.env(&mut cmd);
        cmd
    }
}

/// A running `thruline ui`, killed when the test ends.
struct Server {
    child: Child,
    port: u16,
    /// The whole address it printed, hash and all.
    url: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    fn start(mut cmd: Command) -> Server {
        let mut child = cmd
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
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let mut seen = String::new();
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            match rx.recv_timeout(left) {
                Ok(line) => {
                    seen.push_str(&line);
                    seen.push('\n');
                    if let Some(rest) = line.trim().strip_prefix("ThruLine is at http://127.0.0.1:")
                    {
                        let port: u16 = rest
                            .chars()
                            .take_while(|c| c.is_ascii_digit())
                            .collect::<String>()
                            .parse()
                            .expect("port in the printed URL");
                        return Server {
                            child,
                            port,
                            url: line.trim().to_string(),
                        };
                    }
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let mut err = String::new();
                    if let Some(mut e) = child.stderr.take() {
                        let _ = e.read_to_string(&mut err);
                    }
                    panic!("`thruline ui` never printed a loopback URL.\nstdout:\n{seen}\nstderr:\n{err}");
                }
            }
        }
    }

    fn request(&self, head: &str, body: &str) -> Response {
        let mut socket = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(60)))
            .expect("read timeout");
        socket.write_all(head.as_bytes()).expect("request head");
        socket.write_all(body.as_bytes()).expect("request body");
        let mut raw = Vec::new();
        socket.read_to_end(&mut raw).expect("response");
        Response::parse(&raw)
    }

    fn get(&self, path: &str) -> Response {
        self.request(
            &format!(
                "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
                self.port
            ),
            "",
        )
    }

    /// A same-origin POST, exactly as a browser makes it. The `Origin` header
    /// is what the server requires of every write.
    fn post(&self, path: &str, body: &str) -> Response {
        self.request(
            &format!(
                "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{p}\r\nOrigin: http://127.0.0.1:{p}\r\nContent-Type: application/json\r\nContent-Length: {n}\r\nConnection: close\r\n\r\n",
                p = self.port,
                n = body.len()
            ),
            body,
        )
    }

    /// The same POST without the header a browser always sends.
    fn post_without_origin(&self, path: &str, body: &str) -> Response {
        self.request(
            &format!(
                "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{p}\r\nContent-Type: application/json\r\nContent-Length: {n}\r\nConnection: close\r\n\r\n",
                p = self.port,
                n = body.len()
            ),
            body,
        )
    }
}

struct Response {
    status: u16,
    body: Vec<u8>,
}

impl Response {
    fn parse(raw: &[u8]) -> Response {
        let split = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .expect("header terminator");
        let head = String::from_utf8_lossy(&raw[..split]).to_string();
        let status = head
            .split_whitespace()
            .nth(1)
            .and_then(|v| v.parse().ok())
            .expect("HTTP status");
        Response {
            status,
            body: raw[split + 4..].to_vec(),
        }
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }

    fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|e| panic!("not JSON ({e}): {}", self.text()))
    }
}

/// The synthetic opening ask. Two sentences, deliberately: the first names the
/// Change (it is the title), and the second is criterion-shaped — subject,
/// condition, outcome — so the acceptance branch below is actually reached. An
/// ask with nothing observable in it proposes nothing, which is correct
/// behaviour and proves nothing about accepting.
const ASKED: &str = "Fix the retry loop in the uploader. When an upload fails three times, the record shows a single failure row.";

/// One machine with two recordings for one repository, served.
fn a_machine_with_recorded_work() -> (Machine, Server) {
    recorded_work_on(Machine::new())
}

/// The same machine, with the repository named whatever the caller says.
fn recorded_work_on(machine: Machine) -> (Machine, Server) {
    machine.record(
        "cccccccc-0000-4000-8000-00000000s001",
        "Add a health check to the uploader",
        "src/health.rs",
        "2026-01-02",
    );
    machine.record(
        "cccccccc-0000-4000-8000-00000000s002",
        ASKED,
        "src/uploader.rs",
        "2026-01-09",
    );
    let server = Server::start(machine.ui());
    (machine, server)
}

/// The key of the one repository this machine recorded work for.
fn the_repository(server: &Server, machine: &Machine) -> String {
    let doc = server.get("/api/first-use").json();
    let here = machine.repo().display().to_string();
    let candidate = doc["candidates"]
        .as_array()
        .expect("candidates")
        .iter()
        .find(|c| c["label"] == here && c["recordings"].as_u64().unwrap_or(0) > 0)
        .unwrap_or_else(|| panic!("no recorded candidate for {here}: {doc}"));
    candidate["key"].as_str().expect("key").to_string()
}

// ---------------------------------------------------------------------------
// The documented command ends on the first-use screen
// ---------------------------------------------------------------------------

/// `03-USER-LENS.md` §4 step 1. The command in the README's quickstart, run in
/// a repository on a machine with nothing picked yet, prints an address that
/// opens the FIRST-USE path — not an Inbox that can only say it is empty.
#[test]
fn the_documented_command_ends_on_the_first_use_screen() {
    let (_machine, server) = a_machine_with_recorded_work();
    assert!(
        server.url.ends_with("/#/start"),
        "the address a person is given is not the first-use path: {}",
        server.url
    );
    // And the address serves the app, so the screen a person opens is real.
    let page = server.get("/");
    assert_eq!(page.status, 200, "{}", page.text());
    assert!(page.text().contains("id=\"root\""), "{}", page.text());
}

/// Once a project is picked, the same command lands where it always did. The
/// first screen is the first screen, not a screen a person cannot get past.
#[test]
fn the_landing_is_the_ordinary_one_once_a_project_is_picked() {
    let (machine, server) = a_machine_with_recorded_work();
    let key = the_repository(&server, &machine);
    let picked = server.post("/api/first-use/register", &format!(r#"{{"key":"{key}"}}"#));
    assert_eq!(picked.status, 200, "{}", picked.text());
    drop(server);

    let again = Server::start(machine.ui());
    assert!(
        !again.url.contains("#/start"),
        "a machine with a project picked was sent back to the first screen: {}",
        again.url
    );
}

// ---------------------------------------------------------------------------
// What this machine can already see
// ---------------------------------------------------------------------------

/// The candidates come from the observed capture paths, and every number
/// beside one is a typed field: how many recordings, and the instants the
/// recordings themselves carry. `B11.5`.
#[test]
fn candidate_projects_are_listed_from_the_observed_paths_with_typed_counts() {
    let (machine, server) = a_machine_with_recorded_work();
    let doc = server.get("/api/first-use").json();
    assert_eq!(doc["schema_version"], 1);
    assert_eq!(doc["registered"].as_u64(), Some(0));

    let here = machine.repo().display().to_string();
    let candidate = doc["candidates"]
        .as_array()
        .expect("candidates")
        .iter()
        .find(|c| c["label"] == here && c["recordings"].as_u64().unwrap_or(0) > 0)
        .unwrap_or_else(|| panic!("{doc}"));
    // The repository is read off the recording's own `cwd`, not guessed from
    // the munged directory name.
    assert_eq!(candidate["label"], here);
    assert_eq!(candidate["source"], "claude_code");
    assert_eq!(candidate["recordings"].as_u64(), Some(2));
    assert_eq!(candidate["not_read"].as_u64(), Some(0));
    assert_eq!(candidate["registered"], serde_json::Value::Bool(false));
    for field in ["first_recorded", "last_recorded"] {
        let at = candidate[field].as_str().unwrap_or_else(|| panic!("{doc}"));
        assert!(at.ends_with('Z') && at.len() == 20, "{field} = {at}");
    }
    // And the store says where it is and how big it is, before anything is
    // written to it (`L1.1`).
    let folder = doc["store"]["folder"].as_str().expect("store folder");
    assert!(
        Path::new(folder).starts_with(machine.home()),
        "the store is not under this machine's own home: {folder}"
    );
    assert!(doc["store"]["bytes"].is_u64(), "{doc}");
}

// ---------------------------------------------------------------------------
// Picking one
// ---------------------------------------------------------------------------

/// Picking writes the project and what is already on disk for it, and NOTHING
/// else: no Change, no scope, no acceptance, no decision.
#[test]
fn picking_a_project_writes_the_project_and_not_a_change() {
    let (machine, server) = a_machine_with_recorded_work();
    let key = the_repository(&server, &machine);
    let picked = server
        .post("/api/first-use/register", &format!(r#"{{"key":"{key}"}}"#))
        .json();
    assert_eq!(picked["read"].as_u64(), Some(2), "{picked}");
    assert_eq!(picked["recorded_work"].as_u64(), Some(2), "{picked}");
    assert!(picked["steps"].as_u64().unwrap_or(0) > 0, "{picked}");
    let project = picked["project"].as_str().expect("project");
    assert!(!project.is_empty());

    let changes = server.get("/api/changes").json();
    assert_eq!(
        changes["changes"].as_array().map(|c| c.len()),
        Some(0),
        "picking a project wrote a Change: {changes}"
    );
    // The same machine now says it has one project, so the landing can change.
    let again = server.get("/api/first-use").json();
    assert_eq!(again["registered"].as_u64(), Some(1), "{again}");
}

/// C34 pause gate (4 of 6): a repository whose folder is paused is not read by
/// the register, and the answer counts what it left out.
#[test]
fn a_paused_repository_is_not_read_by_the_register_and_the_answer_says_how_many_it_left_out() {
    let (machine, server) = a_machine_with_recorded_work();
    let key = the_repository(&server, &machine);
    let picked = server
        .post("/api/first-use/register", &format!(r#"{{"key":"{key}"}}"#))
        .json();
    assert_eq!(picked["read"].as_u64(), Some(2), "{picked}");
    assert_eq!(picked["paused_by_you"].as_u64(), Some(0), "{picked}");
    assert_eq!(picked["started_while_paused"].as_u64(), Some(0), "{picked}");
    let steps = picked["steps"].as_u64().unwrap_or(0);
    assert!(steps > 0, "{picked}");

    // The person pauses the project from a terminal; a third recording is made.
    let mut pause = Command::new(thruline());
    pause
        .args(["capture", "pause", "--project"])
        .arg(machine.repo());
    machine.env(&mut pause);
    let out = pause.output().expect("capture pause");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    machine.record(
        "cccccccc-0000-4000-8000-00000000s003",
        "Add retries to the uploader",
        "src/retry.rs",
        "2026-01-12",
    );

    let again = server
        .post("/api/first-use/register", &format!(r#"{{"key":"{key}"}}"#))
        .json();
    assert_eq!(
        again["read"].as_u64(),
        Some(0),
        "nothing under a pause is read: {again}"
    );
    assert_eq!(again["paused_by_you"].as_u64(), Some(3), "{again}");
    assert_eq!(again["steps"].as_u64(), Some(0), "{again}");
    assert_eq!(again["recorded_work"].as_u64(), Some(0), "{again}");
}

/// The browser hands over an opaque handle the server itself minted. A handle
/// the server did not mint is a refusal — never a directory to go and read.
#[test]
fn a_handle_the_server_did_not_mint_is_refused_rather_than_read_as_a_path() {
    let (_machine, server) = a_machine_with_recorded_work();
    let refused = server.post("/api/first-use/register", r#"{"key":"0000000000000000"}"#);
    assert_eq!(refused.status, 404, "{}", refused.text());
    assert_eq!(refused.json()["error"], "no_such_candidate");
}

/// Every write on this path carries the header a browser always sends and a
/// cross-origin page cannot.
#[test]
fn a_write_without_the_same_origin_header_is_refused() {
    let (_machine, server) = a_machine_with_recorded_work();
    for route in ["/api/first-use/register", "/api/first-use/change"] {
        let refused = server.post_without_origin(route, "{}");
        assert_eq!(refused.status, 403, "{route}: {}", refused.text());
        assert_eq!(refused.json()["error"], "same_origin_required", "{route}");
    }
}

// ---------------------------------------------------------------------------
// The Change the record already holds
// ---------------------------------------------------------------------------

/// The proposal is the person's own most recent ask, named in their own words,
/// and reading it writes nothing.
#[test]
fn the_proposal_is_the_most_recent_recorded_ask_and_writes_nothing() {
    let (machine, server) = a_machine_with_recorded_work();
    let key = the_repository(&server, &machine);
    let project = server
        .post("/api/first-use/register", &format!(r#"{{"key":"{key}"}}"#))
        .json()["project"]
        .as_str()
        .expect("project")
        .to_string();

    let doc = server
        .get(&format!("/api/first-use/proposal?project={project}"))
        .json();
    let proposed = &doc["proposed"];
    assert!(!proposed.is_null(), "{doc}");
    // The title is the first sentence of the ask, never a rewrite of it.
    assert_eq!(proposed["title"], "Fix the retry loop in the uploader");
    assert!(
        proposed["request"]
            .as_str()
            .expect("request")
            .starts_with("Fix the retry loop in the uploader"),
        "{doc}"
    );
    // What it would check is derived from the files the record says were
    // touched, and each check says whether this machine can run it — `B11.6`.
    let checks = proposed["profile"]["checks"].as_array().expect("checks");
    for check in checks {
        let slug = check["availability_slug"].as_str().expect("availability");
        assert!(
            [
                "check_ready",
                "check_not_available_here",
                "check_needs_a_recipe"
            ]
            .contains(&slug),
            "unknown availability {slug}"
        );
        if slug != "check_ready" {
            assert!(
                !check["availability_name"]
                    .as_str()
                    .unwrap_or_default()
                    .is_empty(),
                "an unavailable check names nothing: {check}"
            );
        }
    }

    // Reading it wrote nothing.
    let changes = server.get("/api/changes").json();
    assert_eq!(changes["changes"].as_array().map(|c| c.len()), Some(0));
}

/// The one click: the Change, with the work that was on screen CONFIRMED and
/// everything else merely suggested — and with what counts as done still
/// empty, because that is a separate act (`B5.7`, T-2-3).
#[test]
fn accepting_the_change_confirms_the_work_and_leaves_what_counts_as_done_empty() {
    let (machine, server) = a_machine_with_recorded_work();
    let key = the_repository(&server, &machine);
    let project = server
        .post("/api/first-use/register", &format!(r#"{{"key":"{key}"}}"#))
        .json()["project"]
        .as_str()
        .expect("project")
        .to_string();
    let written = server
        .post(
            "/api/first-use/change",
            &format!(r#"{{"project":"{project}"}}"#),
        )
        .json();
    let id = written["change"].as_str().expect("change id").to_string();
    assert_eq!(written["confirmed"].as_u64(), Some(1), "{written}");

    let doc = server.get(&format!("/api/changes/{id}")).json();
    let change = &doc["change"];
    assert_eq!(
        change["request"]["title"],
        "Fix the retry loop in the uploader"
    );
    assert_eq!(change["request"]["source"], "person");
    let associations = change["associations"].as_array().expect("associations");
    let confirmed: Vec<_> = associations
        .iter()
        .filter(|a| a["provenance"] == "confirmed")
        .collect();
    assert_eq!(confirmed.len(), 1, "{doc}");
    assert_eq!(confirmed[0]["actor"], "you");
    assert!(
        !confirmed[0]["asserted_at"]
            .as_str()
            .unwrap_or_default()
            .is_empty(),
        "{doc}"
    );
    // The separate act has not happened, and nothing pretended it did.
    assert!(
        change["accepted_scope"].is_null(),
        "writing the Change also wrote what counts as done: {doc}"
    );
    assert_eq!(change["decision"], "none_yet");
}

/// The second click, on its own route, is what writes what counts as done —
/// and it is the ONLY thing that does.
#[test]
fn what_counts_as_done_is_written_only_by_the_second_click() {
    let (machine, server) = a_machine_with_recorded_work();
    let key = the_repository(&server, &machine);
    let project = server
        .post("/api/first-use/register", &format!(r#"{{"key":"{key}"}}"#))
        .json()["project"]
        .as_str()
        .expect("project")
        .to_string();
    let id = server
        .post(
            "/api/first-use/change",
            &format!(r#"{{"project":"{project}"}}"#),
        )
        .json()["change"]
        .as_str()
        .expect("change id")
        .to_string();

    let proposals = server.get(&format!("/api/changes/{id}/proposals")).json();
    let criteria = proposals["proposals"]["criteria"]
        .as_array()
        .expect("criteria")
        .len();
    assert!(
        criteria > 0,
        "the fixture proposes nothing, so this test would prove nothing: {proposals}"
    );
    let accepted = server.post(
        &format!("/api/changes/{id}/scope/accept"),
        r#"{"all":true,"actor":"you"}"#,
    );
    assert_eq!(accepted.status, 200, "{}", accepted.text());
    let version = accepted.json()["accepted"]["version"].as_u64();
    assert_eq!(version, Some(1));

    let doc = server.get(&format!("/api/changes/{id}")).json();
    assert_eq!(doc["change"]["accepted_scope"]["version"].as_u64(), Some(1));
}

// ---------------------------------------------------------------------------
// Nothing recorded yet
// ---------------------------------------------------------------------------

/// A repository with nothing recorded is still a project a person may pick,
/// and what it gets is the honest absence of a proposal — never an invented
/// Change, and never a permanent "setup required" (`B11.7`).
#[test]
fn a_project_with_nothing_recorded_gets_no_proposal_rather_than_an_invented_change() {
    let machine = Machine::new();
    let server = Server::start(machine.ui());
    let doc = server.get("/api/first-use").json();
    // The server names the directory it is actually in, which on macOS is the
    // resolved one (`/private/var/...`); the test resolves the same way rather
    // than asserting against a symlink.
    let here = machine
        .repo()
        .canonicalize()
        .expect("repo")
        .display()
        .to_string();
    let candidate = doc["candidates"]
        .as_array()
        .expect("candidates")
        .iter()
        .find(|c| c["label"] == here)
        .unwrap_or_else(|| panic!("the directory the server was started in is not offered: {doc}"));
    assert_eq!(candidate["source"], "here");
    assert_eq!(candidate["recordings"].as_u64(), Some(0));
    let key = candidate["key"].as_str().expect("key").to_string();

    let picked = server
        .post("/api/first-use/register", &format!(r#"{{"key":"{key}"}}"#))
        .json();
    assert_eq!(picked["recorded_work"].as_u64(), Some(0), "{picked}");
    let project = picked["project"].as_str().expect("project").to_string();

    let proposal = server
        .get(&format!("/api/first-use/proposal?project={project}"))
        .json();
    assert!(
        proposal["proposed"].is_null(),
        "a project with nothing recorded proposed something: {proposal}"
    );
    // And the write refuses rather than inventing a Change out of nothing.
    let refused = server.post(
        "/api/first-use/change",
        &format!(r#"{{"project":"{project}"}}"#),
    );
    assert_eq!(refused.status, 409, "{}", refused.text());
    assert_eq!(refused.json()["error"], "nothing_to_propose");
    let changes = server.get("/api/changes").json();
    assert_eq!(changes["changes"].as_array().map(|c| c.len()), Some(0));
}

// ---------------------------------------------------------------------------
// The documented command is the one this file runs
// ---------------------------------------------------------------------------

/// The README's quickstart and this test suite cannot drift apart.
///
/// The baseline's H2T-1 was a documented sequence that ended nowhere: `init`
/// then `ingest`, no browser, "0 transcript file(s)". The public README's
/// `## Quick start` is five lines: `init`, the agent, `ingest --apply`, `why`,
/// `ui`. This asserts that the sequence still ends on `thruline ui` — the
/// command every test above runs — that every flag it names exists, and that
/// the output it quotes at a reader is what the commands print, read out of
/// the binary and its source rather than retyped here.
///
/// (Until the public README, the quickstart quoted two sentences from the
/// first screen's copy file and began with `cargo install`. It no longer does
/// either: install is its own section, and the words it quotes are `init`'s
/// and `why`'s. The guard follows the facts to where they now live.)
#[test]
fn the_readme_quickstart_is_the_command_this_suite_runs_and_the_words_the_screen_says() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let readme = std::fs::read_to_string(root.join("README.md")).expect("README.md");
    let start = readme
        .find("## Quick start")
        .expect("the quickstart section");
    let end = readme[start + 3..]
        .find("\n## ")
        .map(|at| start + 3 + at)
        .unwrap_or(readme.len());
    let quickstart = &readme[start..end];

    // The commands, in order, exactly as a person would run them.
    let commands: Vec<String> = quickstart
        .lines()
        .filter(|line| line.starts_with("thruline "))
        .map(|line| line.split(" #").next().unwrap_or(line).trim().to_string())
        .collect();
    let verbs: Vec<&str> = commands
        .iter()
        .map(|c| c.split_whitespace().nth(1).unwrap_or(""))
        .collect();
    assert_eq!(
        verbs,
        vec!["init", "ingest", "why", "ui"],
        "the documented sequence changed: {commands:?}"
    );
    assert!(
        commands[1].contains("--apply"),
        "the quickstart's `ingest` no longer applies, and a plain one only previews: {commands:?}"
    );
    // The sequence ends where a person can see something. The H2T-1 defect was
    // a quickstart that stopped on a command with no browser behind it.
    // `thruline ui` is what the tests above start; `--no-open --port 0` is the
    // only thing they add to it — a person's version opens the browser.
    assert_eq!(
        commands.last().map(String::as_str),
        Some("thruline ui"),
        "the documented sequence no longer ends on the command this suite runs: {quickstart}"
    );

    // Every flag and number the quickstart gives is one the binary has.
    let help = |args: &[&str]| -> String {
        let out = Command::new(thruline())
            .args(args)
            .arg("--help")
            .output()
            .expect("thruline --help");
        assert!(out.status.success(), "`thruline {args:?} --help` failed");
        String::from_utf8_lossy(&out.stdout).to_string()
    };
    assert!(
        help(&["ingest"]).contains("--apply"),
        "`thruline ingest` has no --apply, which the quickstart tells a reader to type"
    );
    let ui_line = quickstart
        .lines()
        .find(|l| l.starts_with("thruline ui"))
        .expect("the ui line");
    assert!(
        ui_line.contains("7777") && help(&["ui"]).contains("7777"),
        "the quickstart's port, or the port `thruline ui` tries first, changed: {ui_line}"
    );

    // The words it quotes at a reader are the product's own. `init`'s are
    // printed (run for real, `--yes` standing in for the Enter a person
    // presses); the one question is only asked at a terminal, so it is read
    // out of the source that prints it.
    let question = "speaker hooks    install the in-session truth loop? [Y/n]";
    let printed_by_init = [
        "  wrote          SessionStart brief, Stop audit, SessionEnd digest",
        "stop gate        not installed — add it with `thruline init --gate`",
        "undo with        thruline init --undo",
    ];
    let temp = tempfile::tempdir().expect("tempdir");
    let out = Command::new(thruline())
        .args([
            "--db",
            temp.path().join("ledger.db").to_str().expect("utf-8"),
        ])
        .args(["init", "--yes", "--settings"])
        .arg(temp.path().join("settings.json"))
        .arg("--claude-json")
        .arg(temp.path().join("claude.json"))
        .env("HOME", temp.path())
        .env("THRULINE_HOME", temp.path().join(".thruline"))
        .output()
        .expect("run thruline init");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        out.status.success(),
        "init failed: {stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source =
        std::fs::read_to_string(root.join("crates/cli/src/main.rs")).expect("the CLI's source");
    for line in std::iter::once(&question).chain(printed_by_init.iter()) {
        assert!(
            quickstart.contains(line),
            "the README no longer quotes: {line}"
        );
    }
    assert!(
        source.contains(&format!("println!(\"{question}\")")),
        "the README quotes a question `thruline init` does not ask: {question}"
    );
    for line in printed_by_init {
        assert!(
            stdout.lines().any(|l| l.trim_end() == line),
            "the README quotes a line `thruline init` does not print: {line}\n{stdout}"
        );
    }

    // `why`'s first and last lines are the vocabulary the product binds itself
    // to; they are in the one function that renders a finding.
    let verifier = std::fs::read_to_string(root.join("crates/core/src/verifier.rs"))
        .expect("the finding renderer's source");
    for line in [
        "claimed, and no proof found in the record",
        "the claim may be entirely true — the record simply does not show it",
    ] {
        assert!(
            quickstart.contains(line),
            "the README no longer quotes: {line}"
        );
        assert!(
            verifier.contains(line),
            "the README quotes a sentence `thruline why` does not say: {line}"
        );
    }
}

// ---------------------------------------------------------------------------
// Coming back to the first screen
// ---------------------------------------------------------------------------

/// The pick screen tells the truth about a repository whose name holds a `-`.
///
/// `uploads-api`, `my-app`, `web-service`: the ordinary way people name
/// repositories. "Already picked" is the only thing on that screen that says a
/// project is not new, and it was answered by comparing the live path to a
/// stored one that had been built by reading every `-` in the vendor's folder
/// name as a `/` — so for all of those it said "no" seconds after the person
/// said yes, and offered them the same project again with nothing to warn them.
#[test]
fn a_repository_whose_name_holds_a_hyphen_reads_as_already_picked_once_it_is() {
    let (machine, server) = recorded_work_on(Machine::named("uploads-api"));
    let here = machine.repo().display().to_string();
    let read = |server: &Server| -> serde_json::Value {
        let doc = server.get("/api/first-use").json();
        doc["candidates"]
            .as_array()
            .expect("candidates")
            .iter()
            .find(|c| c["label"] == here && c["recordings"].as_u64().unwrap_or(0) > 0)
            .unwrap_or_else(|| panic!("no recorded candidate for {here}: {doc}"))
            .clone()
    };

    let before = read(&server);
    assert_eq!(before["registered"], serde_json::Value::Bool(false));
    assert!(before["changes"].is_null(), "{before}");
    let key = before["key"].as_str().expect("key").to_string();

    let picked = server
        .post("/api/first-use/register", &format!(r#"{{"key":"{key}"}}"#))
        .json();
    let project = picked["project"].as_str().expect("project").to_string();

    let after = read(&server);
    assert_eq!(
        after["registered"],
        serde_json::Value::Bool(true),
        "a project registered one request ago reads as never picked: {after}"
    );
    assert_eq!(after["project"].as_str(), Some(project.as_str()), "{after}");
    // Registered and holding no Change yet are two different facts, and the
    // screen is given both.
    assert_eq!(after["changes"].as_u64(), Some(0), "{after}");

    server.post(
        "/api/first-use/change",
        &format!(r#"{{"project":"{project}"}}"#),
    );
    let tracked = read(&server);
    assert_eq!(tracked["changes"].as_u64(), Some(1), "{tracked}");
}

/// Coming back to the first screen never writes the same work down twice.
///
/// `#/start` is reachable by a bookmark, a typed address, a second tab and the
/// back button, and every arrival proposes the same most-recent piece of
/// recorded work. Writing a second Change for it put two identical items in the
/// Inbox with nothing on screen to say why there were two — the "one obvious
/// thing to do" the Inbox promises, broken by this product rather than by the
/// record.
#[test]
fn the_same_recorded_work_is_never_written_down_as_two_changes() {
    let (machine, server) = recorded_work_on(Machine::named("uploads-api"));
    let key = the_repository(&server, &machine);
    let project = server
        .post("/api/first-use/register", &format!(r#"{{"key":"{key}"}}"#))
        .json()["project"]
        .as_str()
        .expect("project")
        .to_string();
    let first = server
        .post(
            "/api/first-use/change",
            &format!(r#"{{"project":"{project}"}}"#),
        )
        .json();
    let id = first["change"].as_str().expect("change id").to_string();
    assert_eq!(first["already_tracked"], serde_json::Value::Bool(false));

    // The whole second visit, from the pick onward — the thing a bookmark does.
    let again = server
        .post("/api/first-use/register", &format!(r#"{{"key":"{key}"}}"#))
        .json();
    assert_eq!(again["project"].as_str(), Some(project.as_str()));
    let second = server.post(
        "/api/first-use/change",
        &format!(r#"{{"project":"{project}"}}"#),
    );
    assert_eq!(second.status, 200, "{}", second.text());
    let second = second.json();
    assert_eq!(
        second["already_tracked"],
        serde_json::Value::Bool(true),
        "a second visit wrote a second Change: {second}"
    );
    assert_eq!(
        second["change"].as_str(),
        Some(id.as_str()),
        "the second visit named a Change that is not the one that holds this work: {second}"
    );

    let changes = server.get("/api/changes").json();
    let written = changes["changes"].as_array().expect("changes");
    assert_eq!(
        written.len(),
        1,
        "the same recorded work is written down twice: {changes}"
    );
    assert_eq!(written[0]["id"].as_str(), Some(id.as_str()));
}

// ---------------------------------------------------------------------------
// What is kept: one number, whichever screen says it (`L1.1`)
// ---------------------------------------------------------------------------

/// Every regular file under `dir`, added up by a walk written here, which
/// shares no line with the server's own. A link is not followed.
fn bytes_on_disk(dir: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                total += meta.len();
            }
        }
    }
    total
}

/// The first screen and the Inbox say ONE number for what is kept.
///
/// The first screen's figure is `/api/first-use`'s `store.bytes`; the Inbox's
/// is `/api/storage`'s `bytes`. The T-7-1 verifier found the first measured
/// while that same request held the ledger open, so it counted the ledger's
/// shared-memory file — 32,768 bytes more, every time: 1,311,090 against
/// 1,278,322 on an empty machine, and a store of 1,328,322 bytes that read
/// "1.4 MB" on the first screen and "1.3 MB" in the Inbox. Asserted on an
/// empty machine (the screen that finding was about), in both orders, and
/// against the folder's bytes walked here once the server is idle.
#[test]
fn the_first_screen_and_the_inbox_say_one_number_for_what_is_kept() {
    let machine = Machine::new();
    let server = Server::start(machine.ui());

    let first = server.get("/api/first-use").json()["store"]["bytes"].as_u64();
    let inbox = server.get("/api/storage").json()["bytes"].as_u64();
    let first_again = server.get("/api/first-use").json()["store"]["bytes"].as_u64();
    assert!(
        inbox.unwrap_or(0) > 0,
        "an empty machine still keeps a record store: {inbox:?}"
    );
    assert_eq!(
        first, inbox,
        "the first screen and the Inbox give two numbers for one store"
    );
    assert_eq!(
        first_again, inbox,
        "a second read of the first screen moved"
    );
    assert_eq!(
        inbox,
        Some(bytes_on_disk(&machine.home().join(".thruline"))),
        "the number is not what the folder holds"
    );
}

/// The figure counts the command log wherever `THRULINE_HOME` keeps it
/// (T-7-1 verifier, `HTT-STORAGE-OMITS-EXECDB`), through the shipping binary.
///
/// `THRULINE_HOME` moves the execution store (`exec.db`) and not the ledger.
/// Set EQUAL to `HOME` — a folder that holds the records folder — the command
/// log is `~/exec.db`, and the served figure left it out: 1,094,002 bytes said
/// against 1,278,322 kept. The home directory is still never walked; the file
/// this product put there is counted and named. Set APART from `HOME`, the
/// second folder is counted whole and named — the card's own case, which the
/// earlier fix covered with a unit test only.
#[test]
fn the_figure_counts_the_command_log_wherever_thruline_home_keeps_it() {
    let command_log_in = |folder: &Path| -> u64 {
        ["exec.db", "exec.db-wal", "exec.db-shm"]
            .iter()
            .filter_map(|name| std::fs::symlink_metadata(folder.join(name)).ok())
            .filter(|meta| meta.is_file())
            .map(|meta| meta.len())
            .sum()
    };

    // THRULINE_HOME = HOME.
    let machine = Machine::new();
    let mut cmd = machine.ui();
    cmd.env("THRULINE_HOME", machine.home());
    let server = Server::start(cmd);
    let storage = server.get("/api/storage").json();
    let first = server.get("/api/first-use").json()["store"]["bytes"].as_u64();
    let exec = machine.home().join("exec.db");
    assert!(
        exec.is_file(),
        "`thruline ui` did not start its command log where THRULINE_HOME says"
    );
    let records = bytes_on_disk(&machine.home().join(".thruline"));
    let log = command_log_in(&machine.home());
    assert!(log > 0);
    assert_eq!(
        storage["bytes"].as_u64(),
        Some(records + log),
        "the command log kept in HOME is left out: {storage}"
    );
    assert_eq!(
        storage["also_command_log"],
        exec.display().to_string(),
        "{storage}"
    );
    assert!(
        storage["also"].is_null(),
        "a home directory named as a counted folder: {storage}"
    );
    assert_eq!(
        first,
        storage["bytes"].as_u64(),
        "the first screen gives another number"
    );
    drop(server);

    // THRULINE_HOME apart from HOME.
    let machine = Machine::new();
    let apart = machine.dir.path().join("thruline-home");
    std::fs::create_dir_all(&apart).expect("THRULINE_HOME");
    let mut cmd = machine.ui();
    cmd.env("THRULINE_HOME", &apart);
    let server = Server::start(cmd);
    let storage = server.get("/api/storage").json();
    let first = server.get("/api/first-use").json()["store"]["bytes"].as_u64();
    assert!(
        command_log_in(&apart) > 0,
        "no command log in the second folder"
    );
    let both = bytes_on_disk(&machine.home().join(".thruline")) + bytes_on_disk(&apart);
    assert_eq!(storage["bytes"].as_u64(), Some(both), "{storage}");
    assert_eq!(
        storage["path"],
        machine.home().join(".thruline").display().to_string(),
        "{storage}"
    );
    assert_eq!(storage["also"], apart.display().to_string(), "{storage}");
    assert!(storage["also_command_log"].is_null(), "{storage}");
    assert_eq!(
        first,
        storage["bytes"].as_u64(),
        "the first screen gives another number"
    );
}
