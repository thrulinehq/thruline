//! `claims --phases` and `GET /api/phases` — the verdict pipeline's phase log.
//!
//! Two things are pinned here, against the SHIPPING BINARY:
//!
//! 1. **`--phases` changes no stdout byte.** `claims --json` with and without
//!    the flag must produce identical stdout, or `/api/claims` — which the
//!    contract test pins byte-for-byte to the CLI — would have a sibling that
//!    quietly disagrees with it.
//! 2. **The phase log is the pipeline's own numbers.** Six lines, in the
//!    documented order, each a JSON object with the four documented keys, and
//!    every count consistent with the `claims --json` document produced by the
//!    same run: claims judged equals `claims[].len()`, checks cited equals
//!    `checks`, findings equals the sum over `claims[].findings`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

const SESSION: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
const ORDER: [&str; 6] = [
    "read_record",
    "find_claims",
    "match_proof",
    "workspace_since",
    "run_verifiers",
    "write_up",
];

fn thruline() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

/// A temp HOME with an execution ledger holding exactly one finished session —
/// the same shape `ui_server_contract.rs` records.
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

    fn env(&self, cmd: &mut Command) {
        cmd.env("HOME", self.home())
            .env("THRULINE_HOME", self.thruline_home())
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .current_dir(self.repo());
    }

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

    fn record_a_session(&self) -> &Fixture {
        let s = SESSION;
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
        let (code, _, _) = self.cli(&["fold"]);
        assert_eq!(code, Some(0), "fold");
        self
    }

    /// Run a CLI verb and return (exit code, stdout, stderr).
    fn cli(&self, args: &[&str]) -> (Option<i32>, Vec<u8>, Vec<u8>) {
        let mut cmd = Command::new(thruline());
        cmd.args(args);
        self.env(&mut cmd);
        let out = cmd.output().expect("cli");
        (out.status.code(), out.stdout, out.stderr)
    }

    fn ui(&self) -> Command {
        let mut cmd = Command::new(thruline());
        cmd.args(["ui", "--no-open", "--port", "0", "--dir"])
            .arg(self.dir.path().join("export"));
        self.env(&mut cmd);
        cmd
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

    fn request(&self, method: &str, path: &str) -> Response {
        let mut socket = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            self.port
        );
        socket.write_all(request.as_bytes()).expect("request");
        let mut raw = Vec::new();
        socket.read_to_end(&mut raw).expect("response");
        Response::parse(&raw)
    }

    fn get(&self, path: &str) -> Response {
        self.request("GET", path)
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

    fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|e| panic!("body is not JSON ({e}): {}", self.text()))
    }
}

/// Parse a `--phases` stderr into its phase objects, failing on any line that
/// is not one: the CLI's stderr is the transport, so a stray line is a defect.
fn phase_lines(stderr: &[u8]) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(stderr)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .unwrap_or_else(|e| panic!("stderr line is not JSON ({e}): {line:?}"))
        })
        .collect()
}

/// The invariants every phase log must satisfy against the `claims --json`
/// document of the same run.
fn assert_phases_consistent(phases: &[serde_json::Value], claims_doc: &serde_json::Value) {
    assert_eq!(
        phases.len(),
        ORDER.len(),
        "exactly six phases, got {}: {phases:?}",
        phases.len()
    );
    for (i, (phase, expected)) in phases.iter().zip(ORDER).enumerate() {
        let object = phase
            .as_object()
            .unwrap_or_else(|| panic!("phase {i} is not an object: {phase}"));
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["count", "detail", "elapsed_ms", "phase"],
            "phase {i} has the wrong keys: {phase}"
        );
        assert_eq!(phase["phase"], expected, "phase {i} out of order: {phase}");
        assert!(phase["count"].is_u64(), "phase {i} count: {phase}");
        assert!(
            phase["elapsed_ms"].is_u64(),
            "phase {i} elapsed_ms: {phase}"
        );
        assert!(
            phase["detail"].as_str().is_some_and(|d| !d.is_empty()),
            "phase {i} detail: {phase}"
        );
    }

    let claims = claims_doc["claims"].as_array().expect("claims[]");
    let findings: u64 = claims
        .iter()
        .map(|c| c["findings"].as_array().map_or(0, |f| f.len() as u64))
        .sum();

    // read_record: the session named is the one the document is about, and a
    // recorded session has at least one event.
    assert!(phases[0]["count"].as_u64().unwrap() >= 1);
    assert!(
        phases[0]["detail"]
            .as_str()
            .unwrap()
            .contains(claims_doc["session"].as_str().unwrap()),
        "read_record must name the session: {}",
        phases[0]
    );
    // find_claims: what the extractor found; the F5 floor can only ADD a claim
    // after this point, never remove one.
    assert!(phases[1]["count"].as_u64().unwrap() <= claims.len() as u64);
    assert_eq!(
        phases[1]["count"].as_u64().unwrap() == 0,
        phases[1]["detail"] == "no completion claim in this session",
        "find_claims detail must match its count: {}",
        phases[1]
    );
    // …and it names those claims by id, never by their text: one id per claim
    // counted, each one a `claims[].id` of the same run's document.
    if phases[1]["count"].as_u64().unwrap() > 0 {
        let ids: Vec<&str> = phases[1]["detail"].as_str().unwrap().split(" · ").collect();
        assert_eq!(
            ids.len() as u64,
            phases[1]["count"].as_u64().unwrap(),
            "find_claims names one id per claim: {}",
            phases[1]
        );
        for id in ids {
            assert!(
                claims.iter().any(|c| c["id"] == id),
                "find_claims names {id:?}, which is not a claim id of the document: {}",
                phases[1]
            );
        }
    }
    // match_proof: the same `checks` the document carries.
    assert_eq!(phases[2]["count"], claims_doc["checks"], "match_proof");
    // run_verifiers: one verdict per claim, and every claim's verifier named.
    assert_eq!(
        phases[4]["count"].as_u64().unwrap(),
        claims.len() as u64,
        "run_verifiers must count the claims the document carries"
    );
    let named = phases[4]["detail"].as_str().unwrap();
    for c in claims {
        let verifier = c["verifier"].as_str().expect("verifier");
        assert!(
            named.split(" · ").any(|n| n == verifier),
            "run_verifiers detail {named:?} does not name {verifier:?}"
        );
    }
    // write_up: findings across every claim.
    assert_eq!(phases[5]["count"].as_u64().unwrap(), findings, "write_up");
    assert_eq!(phases[5]["detail"], format!("{findings} findings"));
}

#[test]
fn phases_change_no_stdout_byte_and_report_six_ordered_phases() {
    let fx = Fixture::new();
    fx.record_a_session();

    let (plain_code, plain_out, plain_err) = fx.cli(&["claims", "--last", "--json"]);
    let (phased_code, phased_out, phased_err) = fx.cli(&["claims", "--last", "--json", "--phases"]);
    assert!(matches!(plain_code, Some(0) | Some(3)), "{plain_code:?}");
    assert_eq!(
        plain_code, phased_code,
        "--phases must not change the exit code"
    );

    // (a) stdout is byte-identical.
    assert_eq!(
        plain_out, phased_out,
        "`--phases` changed the stdout of `claims --json`"
    );
    // …and without the flag nothing at all is written to stderr, so the
    // transport is opt-in rather than always-on.
    assert!(
        plain_err.is_empty(),
        "claims --json wrote to stderr without --phases: {}",
        String::from_utf8_lossy(&plain_err)
    );

    // (b) the phase log.
    let doc: serde_json::Value = serde_json::from_slice(&phased_out).expect("claims --json");
    assert!(
        !doc["claims"].as_array().expect("claims").is_empty(),
        "the fixture must yield a claim or the count checks are vacuous"
    );
    let phases = phase_lines(&phased_err);
    assert_phases_consistent(&phases, &doc);
    // The fixture's one extracted claim, named by its id — its text is not in
    // the log (the panel reads claim text from `/api/claims`).
    assert_eq!(phases[1]["count"], 1);
    assert_eq!(phases[1]["detail"], doc["claims"][0]["id"]);
    assert!(
        !phases[1]["detail"]
            .as_str()
            .unwrap()
            .contains("All tests pass"),
        "find_claims quotes claim text: {}",
        phases[1]
    );
}

#[test]
fn phases_flag_works_with_an_explicit_session_and_in_text_mode() {
    let fx = Fixture::new();
    fx.record_a_session();

    let (code, out, err) = fx.cli(&["claims", "--session", SESSION, "--json", "--phases"]);
    assert!(matches!(code, Some(0) | Some(3)), "{code:?}");
    let doc: serde_json::Value = serde_json::from_slice(&out).expect("json");
    assert_eq!(doc["session"], SESSION);
    assert_phases_consistent(&phase_lines(&err), &doc);

    // Text mode: the same six lines, still on stderr, stdout still the report.
    let (code, out, err) = fx.cli(&["claims", "--session", SESSION, "--phases"]);
    assert!(matches!(code, Some(0) | Some(3)), "{code:?}");
    assert!(
        String::from_utf8_lossy(&out).starts_with(&format!("session {SESSION}")),
        "text stdout: {}",
        String::from_utf8_lossy(&out)
    );
    let phases = phase_lines(&err);
    let names: Vec<&str> = phases
        .iter()
        .map(|p| p["phase"].as_str().unwrap())
        .collect();
    assert_eq!(names, ORDER);
}

#[test]
fn api_phases_serves_the_cli_phase_log_under_the_documented_envelope() {
    let fx = Fixture::new();
    fx.record_a_session();

    let (code, out, err) = fx.cli(&["claims", "--session", SESSION, "--json", "--phases"]);
    assert!(matches!(code, Some(0) | Some(3)), "{code:?}");
    let doc: serde_json::Value = serde_json::from_slice(&out).expect("json");
    let cli_phases = phase_lines(&err);

    let server = Server::start(fx.ui());

    for path in ["/api/phases", &format!("/api/phases?session={SESSION}")] {
        let response = server.get(path);
        assert_eq!(response.status, 200, "{path}: {}", response.text());
        assert!(
            response
                .head
                .to_ascii_lowercase()
                .contains("application/json"),
            "{path} head: {}",
            response.head
        );
        assert!(
            response
                .head
                .contains("X-ThruLine-Projection: claims --json --phases"),
            "{path} head: {}",
            response.head
        );
        assert!(
            response.head.contains("X-ThruLine-Exit: "),
            "{path} head: {}",
            response.head
        );
        let body = response.json();
        assert_eq!(body["contract"], "thruline.phases.v1", "{path}: {body}");
        // With `--last` the server reads the id from the CLI's own document;
        // by id it echoes the id it was given. Both must be THIS session.
        assert_eq!(body["session"], SESSION, "{path}: {body}");
        assert!(
            body.get("unparsed").is_none(),
            "{path}: nothing but phase lines was expected on stderr: {body}"
        );
        let served = body["phases"].as_array().expect("phases[]");
        assert_phases_consistent(served, &doc);
        // Same phases, same counts, same details as the CLI run — only the
        // measured times may differ between two runs.
        for (a, b) in served.iter().zip(&cli_phases) {
            for key in ["phase", "count", "detail"] {
                assert_eq!(
                    a[key], b[key],
                    "{path}: {key} differs from the CLI: {a} vs {b}"
                );
            }
        }
    }
}

#[test]
fn api_phases_refuses_what_api_claims_refuses() {
    let fx = Fixture::new();
    fx.record_a_session();
    let server = Server::start(fx.ui());

    let unknown = server.get("/api/phases?limit=5");
    assert_eq!(unknown.status, 400, "{}", unknown.text());
    assert_eq!(unknown.json()["error"], "unknown_query_parameter");

    let hostile = server.get("/api/phases?session=--exec-db");
    assert_eq!(hostile.status, 400, "{}", hostile.text());

    let twice = server.get(&format!("/api/phases?session={SESSION}&session={SESSION}"));
    assert_eq!(twice.status, 400, "{}", twice.text());
    assert_eq!(twice.json()["error"], "session_given_twice");

    let post = server.request("POST", "/api/phases");
    assert_eq!(post.status, 405, "{}", post.text());
}

#[test]
fn the_contract_document_names_api_phases() {
    let doc = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/UI-SERVER-CONTRACT.md");
    let text = std::fs::read_to_string(&doc)
        .unwrap_or_else(|e| panic!("{} is part of this contract: {e}", doc.display()));
    for needle in [
        "/api/phases",
        "thruline.phases.v1",
        "claims --json --phases",
    ] {
        assert!(
            text.contains(needle),
            "the contract document does not mention {needle}"
        );
    }
}
