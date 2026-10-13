//! The `thruline ui` server contract — written before the contract existed.
//!
//! Four external tracks pin fixtures against this surface, so every behaviour
//! here is asserted against the SHIPPING BINARY (`CARGO_BIN_EXE_thruline`) and
//! not against a function some in-process test reached directly. This program's
//! signature defect is a guard that exists, is tested, and is called by nothing
//! that ships; a contract test that never starts the binary would reproduce it.
//!
//! The three behaviours the contract turns on:
//!
//! 1. **A missing static directory is a loud refusal, never a blank page.** A UI
//!    that renders nothing looks like a UI with no findings, and the whole
//!    thesis of this product is that absence of a signal must not read as
//!    absence of a problem.
//! 2. **`/api/explain` and `/api/claims` are the CLI's own bytes.** Not a shape
//!    that resembles them — the same bytes, so a second representation of the
//!    same fact cannot exist to drift.
//! 3. **A non-loopback bind address is refused before anything is bound.**

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

fn thruline() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

/// A temp HOME with an execution ledger holding exactly one finished session.
struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Fixture {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("h/.thruline")).expect("home");
        std::fs::create_dir_all(dir.path().join("repo")).expect("repo");
        Fixture { dir }
    }

    fn home(&self) -> PathBuf {
        self.dir.path().join("h")
    }

    fn thruline_home(&self) -> PathBuf {
        self.home().join(".thruline")
    }

    fn repo(&self) -> PathBuf {
        self.dir.path().join("repo")
    }

    fn export_dir(&self) -> PathBuf {
        self.dir.path().join("export")
    }

    fn env(&self, cmd: &mut Command) {
        cmd.env("HOME", self.home())
            .env("THRULINE_HOME", self.thruline_home())
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .current_dir(self.repo());
    }

    /// One hook delivery, exactly as the harness makes it.
    fn hook(&self, json: &str) {
        let mut cmd = Command::new(thruline());
        cmd.args(["hook", "claude-code"]);
        self.env(&mut cmd);
        let mut child = cmd.stdin(Stdio::piped()).spawn().expect("hook");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(json.as_bytes())
            .expect("write");
        assert!(child.wait().expect("wait").success(), "hook exited nonzero");
    }

    /// A session with a completion claim in it, so the projections have
    /// something to say rather than an empty list that any shape would satisfy.
    fn record_a_session(&self) -> &Fixture {
        let s = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
        let cwd = self.repo().display().to_string();
        self.hook(&format!(
            r#"{{"hook_event_name":"SessionStart","session_id":"{s}","transcript_path":"/t","cwd":"{cwd}","source":"startup"}}"#
        ));
        self.hook(&format!(
            r#"{{"hook_event_name":"UserPromptSubmit","session_id":"{s}","transcript_path":"/t","cwd":"{cwd}","prompt":"make the change"}}"#
        ));
        self.hook(&format!(
            r#"{{"hook_event_name":"Stop","session_id":"{s}","transcript_path":"/t","cwd":"{cwd}","last_assistant_message":"Done. All tests pass. I'll add a regression test in a follow-up.","stop_hook_active":false}}"#
        ));
        // Hooks write a spool; the ledger the projections read is what `fold`
        // produces from it.
        let (code, _) = self.cli(&["fold"]);
        assert_eq!(code, Some(0), "fold");
        self
    }

    /// A recorded path in both the timeline and a completion claim.
    fn record_path_session(&self) -> &Fixture {
        let s = "cccccccc-0000-4000-8000-000000000046";
        let cwd = self.repo().display().to_string();
        self.hook(&format!(
            r#"{{"hook_event_name":"SessionStart","session_id":"{s}","transcript_path":"/t","cwd":"{cwd}","source":"startup"}}"#
        ));
        self.hook(&format!(
            r#"{{"hook_event_name":"UserPromptSubmit","session_id":"{s}","transcript_path":"/t","cwd":"{cwd}","prompt":"Please edit src/a.rs."}}"#
        ));
        self.hook(&format!(
            r#"{{"hook_event_name":"Stop","session_id":"{s}","transcript_path":"/t","cwd":"{cwd}","last_assistant_message":"All tests pass for src/a.rs.","stop_hook_active":false}}"#
        ));
        let (code, _) = self.cli(&["fold"]);
        assert_eq!(code, Some(0), "fold");
        self
    }

    /// Run a CLI verb and return (exit code, stdout).
    fn cli(&self, args: &[&str]) -> (Option<i32>, Vec<u8>) {
        let mut cmd = Command::new(thruline());
        cmd.args(args);
        self.env(&mut cmd);
        let out = cmd.output().expect("cli");
        (out.status.code(), out.stdout)
    }

    fn ui(&self, extra: &[&str]) -> Command {
        let mut cmd = Command::new(thruline());
        cmd.args(["ui", "--no-open", "--port", "0", "--dir"])
            .arg(self.export_dir());
        cmd.args(extra);
        self.env(&mut cmd);
        cmd
    }
}

/// A running `thruline ui`, killed when the test ends.
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
    /// Bounded, for the same reason `refusal` is. A mutation that stops the
    /// server printing a loopback URL — binding the wildcard, say — would
    /// otherwise block this reader forever, and a hung suite is not a witness.
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
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut seen = String::new();
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            match rx.recv_timeout(left) {
                Ok(line) => {
                    seen.push_str(&line);
                    seen.push('\n');
                    if let Some(rest) = line.trim().strip_prefix("ThruLine is at http://127.0.0.1:")
                    {
                        // Verification starts at /verification; the ordinary
                        // transcript workspace starts at the origin itself.
                        let port: u16 = rest
                            .trim()
                            .split('/')
                            .next()
                            .unwrap()
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
                    panic!(
                        "`thruline ui` never printed a loopback URL.\nstdout:\n{seen}\nstderr:\n{err}"
                    );
                }
            }
        }
    }

    fn get(&self, path: &str) -> Response {
        let mut socket = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            self.port
        );
        socket.write_all(request.as_bytes()).expect("request");
        let mut raw = Vec::new();
        socket.read_to_end(&mut raw).expect("response");
        Response::parse(&raw)
    }
}

struct Response {
    status: u16,
    head: String,
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
            head,
            body: raw[split + 4..].to_vec(),
        }
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }
}

/// Run `thruline ui` expecting it to REFUSE, and return (exit code, stdout, stderr).
///
/// Bounded on purpose. A refusal that has been deleted does not fail this
/// helper, it *serves* — and `output()` would then block until the suite is
/// killed. A mutation must produce a red test, not a hung one, so a server that
/// is still alive after the deadline is killed and reported as exit `None`,
/// which is not `Some(1)` and fails the assertion the way it should.
fn refusal(mut cmd: Command) -> (Option<i32>, String, String) {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("thruline ui");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let code = loop {
        match child.try_wait().expect("wait") {
            Some(status) => break status.code(),
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            None => std::thread::sleep(std::time::Duration::from_millis(25)),
        }
    };
    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut stdout);
    }
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut stderr);
    }
    (code, stdout, stderr)
}

// ---------------------------------------------------------------------------
// 1. The static directory
// ---------------------------------------------------------------------------

#[test]
fn a_static_directory_that_is_not_there_is_a_loud_refusal_not_a_blank_page() {
    let fx = Fixture::new();
    let missing = fx.dir.path().join("track-a-dist-that-does-not-exist");

    let (code, stdout, stderr) = refusal(fx.ui(&["--static-dir", &missing.display().to_string()]));

    assert_eq!(
        code,
        Some(1),
        "a missing static directory must exit nonzero"
    );
    let said = format!("{stdout}{stderr}");
    assert!(
        said.contains(&missing.display().to_string()),
        "the refusal must name the directory it could not find; said: {said}"
    );
    assert!(
        !stdout.contains("ThruLine is at"),
        "nothing may be served when the static directory is missing; said: {said}"
    );
}

#[test]
fn a_static_directory_without_an_index_is_refused_by_name() {
    let fx = Fixture::new();
    let empty = fx.dir.path().join("dist-empty");
    std::fs::create_dir_all(&empty).expect("dist");
    std::fs::write(empty.join("app.js"), b"console.log(1)").expect("asset");

    let (code, stdout, stderr) = refusal(fx.ui(&["--static-dir", &empty.display().to_string()]));

    assert_eq!(
        code,
        Some(1),
        "a directory with no index.html must be refused"
    );
    let said = format!("{stdout}{stderr}");
    assert!(
        said.contains("index.html"),
        "the refusal must name what was missing; said: {said}"
    );
    assert!(!stdout.contains("ThruLine is at"), "nothing may be served");
}

#[test]
fn a_static_directory_that_is_a_file_is_refused() {
    let fx = Fixture::new();
    let file = fx.dir.path().join("not-a-directory");
    std::fs::write(&file, b"i am a file").expect("file");

    let (code, stdout, stderr) = refusal(fx.ui(&["--static-dir", &file.display().to_string()]));

    assert_eq!(code, Some(1));
    let said = format!("{stdout}{stderr}");
    assert!(
        said.contains("not a directory"),
        "the refusal must say what is wrong; said: {said}"
    );
    assert!(!stdout.contains("ThruLine is at"));
}

#[test]
fn a_supplied_static_directory_is_served_instead_of_the_embedded_ui() {
    let fx = Fixture::new();
    let dist = fx.dir.path().join("dist");
    std::fs::create_dir_all(dist.join("assets")).expect("dist");
    std::fs::write(
        dist.join("index.html"),
        b"<!doctype html><title>TRACK-A</title>",
    )
    .expect("index");
    std::fs::write(dist.join("assets/app.js"), b"export const a = 1;\n").expect("app");

    let server = Server::start(fx.ui(&["--static-dir", &dist.display().to_string()]));

    let root = server.get("/");
    assert_eq!(root.status, 200);
    assert!(
        root.text().contains("TRACK-A"),
        "the supplied index.html must be served, got: {}",
        root.text()
    );

    let asset = server.get("/assets/app.js");
    assert_eq!(asset.status, 200);
    assert_eq!(asset.text(), "export const a = 1;\n");
    assert!(
        asset.head.contains("text/javascript") || asset.head.contains("application/javascript"),
        "a .js asset needs a script content type; head: {}",
        asset.head
    );

    // A client route (no file extension) is the shell, not a 404 — that is what
    // makes it a single-page app rather than a directory listing.
    let route = server.get("/projects/abc");
    assert_eq!(route.status, 200);
    assert!(route.text().contains("TRACK-A"));

    // A named file that is genuinely absent is a 404 and NOT the shell: a
    // missing asset silently answered with HTML is how a blank page happens.
    let gone = server.get("/assets/missing.js");
    assert_eq!(gone.status, 404, "a missing named file must 404");
    assert!(!gone.text().contains("TRACK-A"));
}

#[test]
fn a_static_asset_path_is_case_sensitive_on_every_platform() {
    // The machine this was built on is case-INSENSITIVE and no CI has ever run
    // here (docs/STATUS.md). Without an explicit check `/Assets/App.js` would be
    // served on macOS and 404 on a Linux runner, and a Track A fixture that
    // passed locally would fail there for a reason nothing reported. The server
    // answers the same on both because it compares the name on disk itself.
    let fx = Fixture::new();
    let dist = fx.dir.path().join("dist");
    std::fs::create_dir_all(dist.join("assets")).expect("dist");
    std::fs::write(
        dist.join("index.html"),
        b"<!doctype html><title>TRACK-A</title>",
    )
    .expect("index");
    std::fs::write(dist.join("assets/app.js"), b"export const a = 1;\n").expect("app");

    let server = Server::start(fx.ui(&["--static-dir", &dist.display().to_string()]));

    assert_eq!(server.get("/assets/app.js").status, 200, "the real name");
    for wrong in ["/Assets/app.js", "/assets/App.js", "/ASSETS/APP.JS"] {
        let response = server.get(wrong);
        assert_eq!(
            response.status,
            404,
            "{wrong} must 404 whatever the filesystem thinks; got {} / {}",
            response.status,
            response.text()
        );
    }
}

#[test]
fn a_static_path_cannot_climb_out_of_the_directory_it_was_given() {
    let fx = Fixture::new();
    let dist = fx.dir.path().join("dist");
    std::fs::create_dir_all(&dist).expect("dist");
    std::fs::write(
        dist.join("index.html"),
        b"<!doctype html><title>TRACK-A</title>",
    )
    .expect("index");
    std::fs::write(
        fx.dir.path().join("outside.txt"),
        b"SECRET-OUTSIDE-THE-ROOT",
    )
    .expect("outside");

    let server = Server::start(fx.ui(&["--static-dir", &dist.display().to_string()]));

    // 403, specifically, and not 404. An escape attempt is a different answer
    // from a missing file, and the difference is load-bearing: `..` is refused
    // by the path guard BEFORE the filesystem is consulted at all. Without that
    // guard these requests still do not leak — canonicalization catches them —
    // but they come back as 404s produced by a fall-through, which is a guard
    // that decides nothing pretending to be one that does. Asserting the code
    // is what makes the guard witnessable. `/assets/../index.html` resolves back
    // INSIDE the root and is refused anyway: the rule is about the shape of the
    // request, not about where it happens to land.
    for path in [
        "/../outside.txt",
        "/assets/../../outside.txt",
        "/%2e%2e/outside.txt",
        "/assets/../index.html",
        "/./index.html",
    ] {
        let response = server.get(path);
        assert!(
            !response.text().contains("SECRET-OUTSIDE-THE-ROOT"),
            "{path} escaped the static root: {}",
            response.text()
        );
        assert_eq!(
            response.status, 403,
            "{path} must be refused as an escape, not answered as a miss; got {}",
            response.status
        );
    }
}

// ---------------------------------------------------------------------------
// 2. The JSON API is the CLI's own bytes
// ---------------------------------------------------------------------------

#[test]
fn api_explain_and_api_claims_are_byte_for_byte_the_cli_shapes() {
    let fx = Fixture::new();
    fx.record_a_session();

    // The CLI's answer, first — the shape of record, produced by the verb the
    // contract projects.
    let (explain_code, explain_bytes) = fx.cli(&["explain", "--last", "--json"]);
    let (claims_code, claims_bytes) = fx.cli(&["claims", "--last", "--json"]);
    assert!(
        matches!(explain_code, Some(0) | Some(3)),
        "explain --json exited {explain_code:?}"
    );
    assert!(
        matches!(claims_code, Some(0) | Some(3)),
        "claims --json exited {claims_code:?}"
    );
    let explain_cli: serde_json::Value =
        serde_json::from_slice(&explain_bytes).expect("explain --json is JSON");
    assert!(
        explain_cli["claims_vs_proof"]
            .as_array()
            .is_some_and(|a| !a.is_empty()),
        "the fixture must produce at least one claim or this test proves nothing"
    );

    let server = Server::start(fx.ui(&[]));

    let explain_api = server.get("/api/explain");
    assert_eq!(explain_api.status, 200, "body: {}", explain_api.text());
    assert_eq!(
        explain_api.body, explain_bytes,
        "/api/explain must be `explain --json`'s bytes, not a shape resembling them"
    );
    assert!(
        explain_api
            .head
            .to_ascii_lowercase()
            .contains("application/json"),
        "head: {}",
        explain_api.head
    );

    let claims_api = server.get("/api/claims");
    assert_eq!(claims_api.status, 200, "body: {}", claims_api.text());
    assert_eq!(
        claims_api.body, claims_bytes,
        "/api/claims must be `claims --json`'s bytes"
    );

    // The projection names the verb it is a projection OF, so a consumer that
    // finds a difference knows which command to run to see it.
    assert!(
        explain_api
            .head
            .contains("X-ThruLine-Projection: explain --json"),
        "head: {}",
        explain_api.head
    );
    assert!(
        claims_api
            .head
            .contains("X-ThruLine-Projection: claims --json"),
        "head: {}",
        claims_api.head
    );

    // Selecting a session by id gives the same answer as the CLI's --session.
    let session = explain_cli["session"]
        .as_str()
        .expect("session id")
        .to_string();
    let (_, by_id) = fx.cli(&["explain", "--json", "--session", &session]);
    let api_by_id = server.get(&format!("/api/explain?session={session}"));
    assert_eq!(api_by_id.status, 200);
    assert_eq!(api_by_id.body, by_id);

    // The CLI's exit code is transport metadata, not a body field — §14 says 3
    // is a policy answer, not a failure, and a consumer must be able to tell
    // those apart without re-deriving the verdicts itself.
    assert!(
        explain_api.head.contains("X-ThruLine-Exit: "),
        "head: {}",
        explain_api.head
    );

    // The API adds no `schema_version` the CLI does not emit. This asserts an
    // EQUIVALENCE, not a presence: when the shapes gain one it flows through,
    // and until then nobody is handed a version that nothing stands behind.
    let api_value: serde_json::Value = serde_json::from_slice(&explain_api.body).expect("json");
    assert_eq!(
        api_value.get("schema_version").is_some(),
        explain_cli.get("schema_version").is_some(),
        "the API must neither add nor drop `schema_version`"
    );
}

#[test]
fn legacy_projections_filter_a_live_exclusion_and_restore_the_original_bytes() {
    let fx = Fixture::new();
    fx.record_path_session();
    // The capture ledger and the legacy execution store are separate. Give
    // the capture policy a real synthetic HQ project at the same root.
    std::fs::create_dir_all(fx.repo().join(".git")).expect("synthetic repository");
    let ledger = thruline_core::ledger::Ledger::open(fx.thruline_home().join("ledger.db"))
        .expect("capture ledger");
    ledger
        .upsert_project(
            &thruline_core::model::ProjectRef {
                id: "projection-project".into(),
                root_path: fx.repo().display().to_string(),
                source: thruline_core::model::Source::Codex,
                display_name: Some("Synthetic projection".into()),
            },
            "2026-10-08T00:00:00Z",
        )
        .expect("capture project");
    drop(ledger);

    let session = "cccccccc-0000-4000-8000-000000000046";
    let paths = [
        format!("/api/explain?session={session}"),
        format!("/api/claims?session={session}"),
    ];
    let server = Server::start(fx.ui(&[]));
    let before: Vec<Response> = paths.iter().map(|path| server.get(path)).collect();
    for response in &before {
        assert_eq!(response.status, 200, "{}", response.text());
        assert!(response.text().contains("src/a.rs"), "{}", response.text());
    }
    let project = fx.repo().display().to_string();
    let (code, _) = fx.cli(&["capture", "exclude", "--project", &project, "src/a.rs"]);
    assert_eq!(code, Some(0), "exclude");
    for (path, original) in paths.iter().zip(&before) {
        let response = server.get(path);
        assert_eq!(response.status, 200, "{}", response.text());
        assert!(response.head.contains("X-ThruLine-Projection: "));
        assert!(response.head.contains("X-ThruLine-Exit: "));
        let filtered: serde_json::Value = serde_json::from_slice(&response.body).expect("JSON");
        assert!(
            filtered["left_out_by_exclusion"]
                .as_u64()
                .is_some_and(|count| count > 0),
            "{path}: {filtered}"
        );
        assert!(
            response
                .text()
                .contains("left out by your exclusion (`src/a.rs`)"),
            "{path}: {}",
            response.text()
        );
        assert!(
            !response.text().contains("All tests pass for src/a.rs"),
            "the original claim must be hidden: {}",
            response.text()
        );
        assert_ne!(response.body, original.body);
    }

    let (code, _) = fx.cli(&["capture", "include", "--project", &project, "src/a.rs"]);
    assert_eq!(code, Some(0), "include");
    for (path, original) in paths.iter().zip(&before) {
        let restored = server.get(path);
        assert_eq!(restored.status, 200, "{}", restored.text());
        assert_eq!(restored.body, original.body, "{path} bytes after include");
    }
}

#[test]
fn explicit_projection_stores_work_without_home_from_a_different_launch_directory() {
    let fx = Fixture::new();
    fx.record_a_session();
    let session = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
    let (code, expected) = fx.cli(&["claims", "--session", session, "--json"]);
    assert!(matches!(code, Some(0) | Some(3)));
    let expected_value: serde_json::Value = serde_json::from_slice(&expected).unwrap();
    assert!(!expected_value["claims"].as_array().unwrap().is_empty());

    let ledger = fx.thruline_home().join("ledger.db");
    let exec_db = fx.thruline_home().join("exec.db");
    let mut command = fx.ui(&[]);
    command
        .arg("--db")
        .arg(ledger)
        .arg("--exec-db")
        .arg(exec_db)
        .arg("--verification-project")
        .arg(fx.repo())
        .env_remove("HOME")
        .current_dir(fx.dir.path());
    let server = Server::start(command);
    let response = server.get(&format!("/api/claims?session={session}"));
    assert_eq!(response.status, 200, "{}", response.text());
    assert_eq!(response.body, expected);
}

#[test]
fn api_sessions_is_byte_for_byte_the_cli_shape() {
    let fx = Fixture::new();
    fx.record_a_session();

    let (code, cli_bytes) = fx.cli(&["sessions", "--per-project", "--json"]);
    assert_eq!(code, Some(0), "sessions --json exited {code:?}");
    let cli: serde_json::Value = serde_json::from_slice(&cli_bytes).expect("sessions --json");
    assert_eq!(cli["shape"], "thruline.sessions");
    assert!(cli["schema_version"].is_u64(), "{cli}");

    let server = Server::start(fx.ui(&[]));
    let api = server.get("/api/sessions");
    assert_eq!(api.status, 200, "body: {}", api.text());
    assert_eq!(
        api.body, cli_bytes,
        "/api/sessions must be `sessions --per-project --json`'s bytes, not a shape resembling them"
    );
    assert!(
        api.head.to_ascii_lowercase().contains("application/json"),
        "head: {}",
        api.head
    );
    assert!(
        api.head.contains("X-ThruLine-Projection: sessions --json"),
        "head: {}",
        api.head
    );
    assert!(api.head.contains("X-ThruLine-Exit: "), "head: {}", api.head);
}

/// The positive control that makes the endpoint worth shipping.
///
/// Without it `/api/sessions` could answer `[]` forever and every other test
/// here would still pass. The defect being closed is that the app could open
/// exactly one session, so the list must **name** the session the projections
/// open and carry the counts a person chooses by.
#[test]
fn the_session_list_names_the_session_the_projections_open() {
    let fx = Fixture::new();
    fx.record_a_session();

    let (_, explain_bytes) = fx.cli(&["explain", "--last", "--json"]);
    let explain: serde_json::Value = serde_json::from_slice(&explain_bytes).expect("explain");
    let opened = explain["session"].as_str().expect("session id").to_string();

    let server = Server::start(fx.ui(&[]));
    let listed: serde_json::Value =
        serde_json::from_slice(&server.get("/api/sessions").body).expect("json");
    let rows = listed["sessions"].as_array().expect("sessions array");
    assert!(
        !rows.is_empty(),
        "an empty list would pass every other assertion here and close nothing"
    );
    assert_eq!(listed["total"].as_u64(), Some(rows.len() as u64));
    assert_eq!(listed["returned"].as_u64(), Some(rows.len() as u64));

    let row = rows
        .iter()
        .find(|r| r["id"].as_str() == Some(opened.as_str()))
        .unwrap_or_else(|| panic!("`{opened}` — the session `explain` opens — is not in the list"));
    // The counts a chooser needs, and the receipt handle every row carries.
    assert_eq!(row["stop_events"].as_u64(), Some(1), "{row}");
    assert_eq!(row["final_messages"].as_u64(), Some(1), "{row}");
    assert_eq!(row["user_prompts"].as_u64(), Some(1), "{row}");
    assert!(row["event_count"].as_u64().is_some_and(|n| n >= 3), "{row}");
    assert!(row["head_hash"].as_str().is_some(), "{row}");
    assert_eq!(row["harness"].as_str(), Some("claude-code"));
}

/// The ordering rule, witnessed rather than asserted in a comment.
///
/// A session with a `stop` — the only kind anything can be extracted from —
/// must outrank a session whose id sorts later and which has nothing in it.
/// That pair is exactly the defect: `explain`'s default keeps the
/// lexicographic maximum, which on the operator's store is a two-event session,
/// and the product opened on it.
#[test]
fn the_ranking_puts_a_session_with_a_stop_above_a_later_id_with_nothing_in_it() {
    let fx = Fixture::new();
    fx.record_a_session();
    // A later-sorting id (`ffff…` > `aaaa…`) that only ever started.
    let idle = "ffffffff-bbbb-4ccc-8ddd-eeeeeeeeeeee";
    let cwd = fx.repo().display().to_string();
    fx.hook(&format!(
        r#"{{"hook_event_name":"SessionStart","session_id":"{idle}","transcript_path":"/t","cwd":"{cwd}","source":"startup"}}"#
    ));
    let (code, _) = fx.cli(&["fold"]);
    assert_eq!(code, Some(0), "fold");

    // The control: the id-ordered default really does land on the idle one, so
    // the ranking below is answering a live problem and not a hypothetical.
    let (_, explain_bytes) = fx.cli(&["explain", "--last", "--json"]);
    let explain: serde_json::Value = serde_json::from_slice(&explain_bytes).expect("explain");
    assert_eq!(
        explain["session"].as_str(),
        Some(idle),
        "`explain --last` is supposed to keep the lexicographically last id; if it \
         no longer does, this test's premise has moved and so has the default this \
         change deliberately did not touch"
    );

    let server = Server::start(fx.ui(&[]));
    let listed: serde_json::Value =
        serde_json::from_slice(&server.get("/api/sessions").body).expect("json");
    let ids: Vec<&str> = listed["sessions"]
        .as_array()
        .expect("sessions")
        .iter()
        .filter_map(|r| r["id"].as_str())
        .collect();
    assert_eq!(ids.len(), 2, "both sessions must be listed: {ids:?}");
    assert_ne!(
        ids[0], idle,
        "the session with nothing in it is first — the list is ordered by id, \
         which is the defect this endpoint exists to fix"
    );
    assert_eq!(ids[1], idle, "ids: {ids:?}");
}

/// An empty store is a list of no sessions, not a 502 and not a crash.
#[test]
fn api_sessions_on_an_empty_store_is_an_empty_list() {
    let fx = Fixture::new(); // nothing recorded, nothing folded
    let server = Server::start(fx.ui(&[]));
    let response = server.get("/api/sessions");
    assert_eq!(response.status, 200, "body: {}", response.text());
    let listed: serde_json::Value = serde_json::from_slice(&response.body).expect("json");
    assert_eq!(listed["total"].as_u64(), Some(0));
    assert_eq!(listed["returned"].as_u64(), Some(0));
    assert_eq!(
        listed["sessions"].as_array().map(Vec::len),
        Some(0),
        "an empty store must answer `[]`, not an error: {listed}"
    );
}

#[test]
fn api_sessions_takes_no_query_parameters() {
    let fx = Fixture::new();
    fx.record_a_session();
    let server = Server::start(fx.ui(&[]));
    for query in ["?session=abc", "?limit=5"] {
        let response = server.get(&format!("/api/sessions{query}"));
        assert_eq!(
            response.status,
            400,
            "{query} must be refused, got {}",
            response.text()
        );
        let body: serde_json::Value = serde_json::from_slice(&response.body).expect("json");
        assert_eq!(body["error"], "unknown_query_parameter");
    }
}

#[test]
fn api_sessions_answers_only_get() {
    let fx = Fixture::new();
    fx.record_a_session();
    let server = Server::start(fx.ui(&[]));

    let mut socket = TcpStream::connect(("127.0.0.1", server.port)).expect("connect");
    let request = format!(
        "POST /api/sessions HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        server.port
    );
    socket.write_all(request.as_bytes()).expect("request");
    let mut raw = Vec::new();
    socket.read_to_end(&mut raw).expect("response");
    assert_eq!(Response::parse(&raw).status, 405);
}

#[test]
fn a_session_id_that_is_not_an_identifier_is_refused_before_any_process_runs() {
    let fx = Fixture::new();
    fx.record_a_session();
    let server = Server::start(fx.ui(&[]));

    for hostile in [
        "--exec-db",
        "a b",
        "$(touch /tmp/thruline-pwned)",
        "../../etc/passwd",
        "-",
    ] {
        let encoded: String = hostile
            .bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                    (b as char).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect();
        let response = server.get(&format!("/api/explain?session={encoded}"));
        assert_eq!(
            response.status,
            400,
            "session={hostile:?} must be refused, got {} / {}",
            response.status,
            response.text()
        );
    }
}

#[test]
fn the_projection_endpoints_answer_only_get() {
    let fx = Fixture::new();
    fx.record_a_session();
    let server = Server::start(fx.ui(&[]));

    let mut socket = TcpStream::connect(("127.0.0.1", server.port)).expect("connect");
    let request = format!(
        "POST /api/explain HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        server.port
    );
    socket.write_all(request.as_bytes()).expect("request");
    let mut raw = Vec::new();
    socket.read_to_end(&mut raw).expect("response");
    assert_eq!(Response::parse(&raw).status, 405);
}

// ---------------------------------------------------------------------------
// 3. Loopback, and the refusal to be anywhere else
// ---------------------------------------------------------------------------

#[test]
fn a_non_loopback_bind_address_is_refused_and_nothing_is_bound() {
    let fx = Fixture::new();
    for address in ["0.0.0.0", "::", "192.168.1.10", "example.com"] {
        let (code, stdout, stderr) = refusal(fx.ui(&["--address", address]));
        let said = format!("{stdout}{stderr}");
        assert_eq!(
            code,
            Some(1),
            "--address {address} must be refused; said: {said}"
        );
        assert!(
            said.contains(address),
            "the refusal must name the address it refused; said: {said}"
        );
        assert!(
            !stdout.contains("ThruLine is at"),
            "nothing may be bound when the address is refused; said: {said}"
        );
    }
}

#[test]
fn loopback_addresses_are_accepted_and_serve() {
    let fx = Fixture::new();
    for address in ["127.0.0.1", "localhost"] {
        let server = Server::start(fx.ui(&["--address", address]));
        assert_eq!(server.get("/api/state").status, 200);
    }
}

#[test]
fn the_listener_is_on_the_loopback_interface_only() {
    let fx = Fixture::new();
    let server = Server::start(fx.ui(&[]));
    // Reachable on the loopback…
    assert_eq!(server.get("/api/state").status, 200);
    // …and not on any other address this host answers to. A wildcard bind would
    // accept here; a loopback bind refuses the connection outright.
    if let Some(routable) = a_routable_local_address() {
        let attempt = TcpStream::connect_timeout(
            &std::net::SocketAddr::new(routable, server.port),
            std::time::Duration::from_millis(750),
        );
        assert!(
            attempt.is_err(),
            "the server accepted a connection on {routable} — it is not loopback-only"
        );
    } else {
        eprintln!("SKIPPED the off-loopback half: this host has no routable IPv4 address");
    }
}

/// A non-loopback IPv4 address of this host, if it has one.
fn a_routable_local_address() -> Option<std::net::IpAddr> {
    // Ask the OS which local address it would use to reach a public address; no
    // packet is sent by a UDP connect.
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    let address = socket.local_addr().ok()?.ip();
    (!address.is_loopback() && !address.is_unspecified()).then_some(address)
}

// ---------------------------------------------------------------------------
// The contract document is part of the deliverable, and it must not drift.
// ---------------------------------------------------------------------------

#[test]
fn the_contract_document_names_every_endpoint_that_ships() {
    let doc = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/UI-SERVER-CONTRACT.md");
    let text = std::fs::read_to_string(&doc)
        .unwrap_or_else(|e| panic!("{} is part of this contract: {e}", doc.display()));
    for endpoint in [
        "/api/explain",
        "/api/claims",
        "/api/sessions",
        "/api/state",
        "/api/refresh",
        "/api/lanes",
        "/fixtures/",
        "--static-dir",
        "--address",
    ] {
        assert!(
            text.contains(endpoint),
            "the contract document does not mention {endpoint}"
        );
    }
}
