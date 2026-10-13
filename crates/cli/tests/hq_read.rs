//! P6-2 — `GET /api/read`, the Read: the steps of opening one piece of
//! recorded work, against the SHIPPING BINARY.
//!
//! Pinned here:
//!
//! 1. **The steps are the pipeline's own.** A fresh read serves exactly the
//!    phases `claims --json --phases` prints for the same session — same ids,
//!    same order, same counts — each `done` with a measured time. No row is
//!    added and none is dropped.
//! 2. **A cached read is a comparison, not a replay.** Read again with nothing
//!    changed, the answer is ONE step — the comparison, with its own count and
//!    measured time — and the time of the earlier read, inside 600 ms.
//! 3. **"Nothing changed since" is never said when something did.** A new
//!    recorded step, or a changed rule table, makes the next read a full one;
//!    so does a read that failed in between.
//! 4. **A failing step withholds what needs it.** A malformed rule table fails
//!    `find_claims` in the real pipeline: its reason is served, the two phases
//!    that need it are withheld with no result, and the two that did not run
//!    show no result either.
//! 5. **One session, nothing else**, and no writes.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

const SESSION: &str = "cccccccc-0000-4000-8000-0000000000a2";
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

fn phase_lines(stderr: &[u8]) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(stderr)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|v| v.get("phase").is_some())
        .collect()
}

fn read(server: &Server) -> (serde_json::Value, std::time::Duration) {
    let started = std::time::Instant::now();
    let r = server.get(&format!("/api/read?session={SESSION}"));
    let took = started.elapsed();
    assert_eq!(r.status, 200, "{}", r.text());
    let doc = r.json();
    assert_eq!(doc["contract"], "thruline.read.v1", "{doc}");
    assert_eq!(doc["session"], SESSION, "{doc}");
    // The header says which kind of answer this is, and agrees with the body.
    let kind = if doc["cached"] == true {
        "cached"
    } else {
        "fresh"
    };
    assert!(
        r.head.contains(&format!("X-ThruLine-Read: {kind}")),
        "{}",
        r.head
    );
    (doc, took)
}

fn statuses(doc: &serde_json::Value) -> Vec<(String, String)> {
    doc["phases"]
        .as_array()
        .expect("phases[]")
        .iter()
        .map(|p| {
            (
                p["phase"].as_str().unwrap_or("").to_string(),
                p["status"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect()
}

#[test]
fn a_fresh_read_serves_the_pipelines_own_steps_and_a_second_read_says_nothing_changed() {
    let fx = Fixture::new();
    fx.record_a_session();
    let (code, _, err) = fx.cli(&["claims", "--session", SESSION, "--json", "--phases"]);
    assert!(matches!(code, Some(0) | Some(3)), "claims exited {code:?}");
    let cli = phase_lines(&err);
    assert_eq!(cli.len(), 6, "{cli:?}");
    let server = Server::start(fx.ui());

    let (first, _) = read(&server);
    assert_eq!(first["cached"], false, "{first}");
    assert_eq!(first["complete"], true, "{first}");
    assert_eq!(first["compared"]["outcome"], "first", "{first}");
    let rows = first["phases"].as_array().expect("phases[]");
    assert_eq!(
        rows.len(),
        cli.len(),
        "the served steps are exactly the pipeline's: {first}"
    );
    for ((row, printed), id) in rows.iter().zip(&cli).zip(ORDER) {
        assert_eq!(row["phase"], id);
        assert_eq!(row["phase"], printed["phase"]);
        assert_eq!(row["status"], "done", "{row}");
        assert_eq!(row["count"], printed["count"], "{row} vs {printed}");
        assert_eq!(row["detail"], printed["detail"], "{row} vs {printed}");
        assert!(row["elapsed_ms"].is_u64(), "a measured time: {row}");
    }

    let (second, took) = read(&server);
    eprintln!("P6-2 cached read over HTTP: {} ms", took.as_millis());
    assert_eq!(second["cached"], true, "{second}");
    assert_eq!(
        second["read_at"], first["read_at"],
        "the earlier read's time"
    );
    assert_eq!(
        statuses(&second),
        vec![("compare_record".to_string(), "done".to_string())],
        "a cached read is one step, the comparison — nothing replayed: {second}"
    );
    assert_eq!(
        second["phases"][0]["count"], rows[0]["count"],
        "it compared the same recorded steps the first read counted"
    );
    assert!(second["phases"][0]["elapsed_ms"].is_u64(), "{second}");
    assert!(
        took < std::time::Duration::from_millis(600),
        "the cached path took {} ms",
        took.as_millis()
    );
}

#[test]
fn a_new_recorded_step_is_never_called_unchanged() {
    let fx = Fixture::new();
    fx.record_a_session();
    let server = Server::start(fx.ui());
    let (first, _) = read(&server);
    let (again, _) = read(&server);
    assert_eq!(again["cached"], true, "{again}");

    let s = SESSION;
    let cwd = fx.repo().display().to_string();
    fx.hook(&format!(
        r#"{{"hook_event_name":"UserPromptSubmit","session_id":"{s}","transcript_path":"/t","cwd":"{cwd}","prompt":"one more thing"}}"#
    ));
    let (code, _, _) = fx.cli(&["fold"]);
    assert_eq!(code, Some(0), "fold");

    let (after, _) = read(&server);
    assert_eq!(after["cached"], false, "the record grew: {after}");
    assert_eq!(after["compared"]["outcome"], "changed", "{after}");
    assert!(
        after["phases"][0]["count"].as_u64() > first["phases"][0]["count"].as_u64(),
        "{after}"
    );
}

#[test]
fn a_failing_step_withholds_what_needs_it_and_is_never_read_earlier() {
    let fx = Fixture::new();
    fx.record_a_session();
    let server = Server::start(fx.ui());
    let (first, _) = read(&server);
    assert_eq!(first["complete"], true, "{first}");

    // A rule table the pipeline cannot parse: `find_claims` fails for real.
    let table = fx.repo().join(".thruline/claims.yml");
    std::fs::create_dir_all(table.parent().expect("dir")).expect("dir");
    std::fs::write(&table, "this is not a pattern table\n").expect("table");

    let (failed, _) = read(&server);
    assert_eq!(failed["cached"], false, "a changed rule table: {failed}");
    assert_eq!(failed["complete"], false, "{failed}");
    assert_eq!(
        statuses(&failed),
        vec![
            ("read_record".into(), "done".into()),
            ("find_claims".into(), "failed".into()),
            ("match_proof".into(), "not_run".into()),
            ("workspace_since".into(), "not_run".into()),
            ("run_verifiers".into(), "withheld".into()),
            ("write_up".into(), "withheld".into()),
        ],
        "{failed}"
    );
    let rows = failed["phases"].as_array().expect("phases[]");
    assert!(
        rows[1]["reason"]
            .as_str()
            .is_some_and(|r| r.contains("claims.yml")),
        "the failure says why: {}",
        rows[1]
    );
    assert_eq!(rows[4]["withheld_by"], "find_claims");
    for row in &rows[2..] {
        for key in ["count", "detail", "elapsed_ms"] {
            assert!(row[key].is_null(), "no result for {}: {row}", row["phase"]);
        }
    }

    // A failed read is never "read earlier" …
    let (still, _) = read(&server);
    assert_eq!(still["cached"], false, "{still}");
    // … and once the table is gone again the record is read in full, not
    // called unchanged since the read before the failure.
    std::fs::remove_file(&table).expect("remove table");
    let (whole, _) = read(&server);
    assert_eq!(whole["cached"], false, "{whole}");
    assert_eq!(whole["complete"], true, "{whole}");
}

#[test]
fn the_read_address_takes_one_session_and_nothing_else() {
    let fx = Fixture::new();
    fx.record_a_session();
    let server = Server::start(fx.ui());
    let bare = server.get("/api/read");
    assert_eq!(bare.status, 400, "{}", bare.text());
    assert_eq!(bare.json()["error"], "session_required");
    let hostile = server.get("/api/read?session=--exec-db");
    assert_eq!(hostile.status, 400, "{}", hostile.text());
    let unknown = server.get(&format!("/api/read?session={SESSION}&limit=5"));
    assert_eq!(unknown.status, 400, "{}", unknown.text());
    let post = server.request("POST", &format!("/api/read?session={SESSION}"));
    assert_eq!(post.status, 405, "{}", post.text());
}

/* ── T8: facts that depend on the workspace, when git cannot answer ────── */

const T8_SESSION: &str = "cccccccc-0000-4000-8000-0000000000b8";

impl Fixture {
    fn git(&self, args: &[&str]) {
        let out = Command::new("git")
            .current_dir(self.repo())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .args(args)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}");
    }

    fn t8_hook(&self, event: &str, fields: serde_json::Value) {
        let mut delivery = serde_json::json!({
            "hook_event_name": event,
            "session_id": T8_SESSION,
            "transcript_path": "/t",
            "cwd": self.repo().display().to_string(),
        });
        for (k, v) in fields.as_object().expect("fields").clone() {
            delivery[k] = v;
        }
        self.hook(&delivery.to_string());
    }

    fn fold(&self) {
        let (code, _, err) = self.cli(&["fold"]);
        assert_eq!(code, Some(0), "fold: {}", String::from_utf8_lossy(&err));
    }

    fn last_snapshot_tree(&self) -> String {
        thruline_ledger::Ledger::open(&self.thruline_home().join("exec.db"))
            .expect("open ledger")
            .session_events(T8_SESSION)
            .expect("events")
            .iter()
            .filter(|e| e.kind().as_str().as_str() == "snapshot")
            .filter_map(|e| {
                e.payload()
                    .get("tree_hash")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            })
            .next_back()
            .expect("a snapshot")
    }

    /// A git project; a session that edits source, then a cited passing check
    /// on that tree; then a test file written through a tool that reports no
    /// path, so the final tree differs from the check's and ONLY the tree half
    /// of §6.1 rule 2 — `git diff-tree` — can say whether that change matters.
    fn record_a_check_before_the_tree_moved(&self) {
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.email", "a@e.invalid"],
            vec!["config", "user.name", "A"],
        ] {
            self.git(&args);
        }
        std::fs::create_dir_all(self.repo().join("src")).expect("src");
        std::fs::write(
            self.repo().join("src/lib.rs"),
            "pub fn rate() -> u32 { 4 }\n",
        )
        .expect("w");
        self.git(&["add", "-A"]);
        self.git(&["commit", "-qm", "baseline"]);

        self.t8_hook("SessionStart", serde_json::json!({"source": "startup"}));
        self.t8_hook(
            "UserPromptSubmit",
            serde_json::json!({"prompt": "make the rate five"}),
        );
        std::fs::write(
            self.repo().join("src/lib.rs"),
            "pub fn rate() -> u32 { 5 }\n",
        )
        .expect("w");
        self.t8_hook(
            "PostToolUse",
            serde_json::json!({"tool_name": "Edit", "tool_input": {"file_path": "src/lib.rs"},
                   "tool_response": {"success": true}, "tool_use_id": "toolu_e1"}),
        );
        self.fold();
        let tree_at_check = self.last_snapshot_tree();

        std::fs::create_dir_all(self.repo().join("tests")).expect("tests");
        std::fs::write(
            self.repo().join("tests/test_rate.py"),
            "def test_rate():\n    assert 5 == 5\n",
        )
        .expect("w");
        self.t8_hook(
            "PostToolUse",
            serde_json::json!({"tool_name": "Bash", "tool_input": {"command": "python gen.py"},
                   "tool_response": {"stdout": "", "stderr": ""}, "tool_use_id": "toolu_b1"}),
        );
        self.t8_hook(
            "Stop",
            serde_json::json!({"last_assistant_message": "Done. All tests pass.",
                   "stop_hook_active": false}),
        );
        self.fold();
        assert_ne!(
            self.last_snapshot_tree(),
            tree_at_check,
            "premise: the test file written after the check must move the tree"
        );
        let row = thruline_ledger::CheckRow {
            id: "chk_t8_fixture".to_string(),
            session_id: T8_SESSION.to_string(),
            exec_event_id: String::new(),
            category: "test".to_string(),
            runner: "pytest".to_string(),
            argv_hash: "sha256:0".to_string(),
            cwd_hash: "sha256:0".to_string(),
            env_hash: "sha256:0".to_string(),
            input_tree_hash: tree_at_check,
            exit_code: 0,
            parsed: serde_json::json!({"tests_total": 1, "passed": 1, "failed": 0,
                                        "skipped": 0, "xfail": 0})
            .to_string(),
        };
        let ledger = thruline_ledger::Ledger::open(&self.thruline_home().join("exec.db"))
            .expect("open ledger");
        assert_eq!(ledger.insert_check(&row).expect("insert"), 1);
    }
}

fn t8_read(server: &Server) -> serde_json::Value {
    let r = server.get(&format!("/api/read?session={T8_SESSION}"));
    assert_eq!(r.status, 200, "{}", r.text());
    r.json()
}

fn t8_claims(server: &Server) -> Vec<serde_json::Value> {
    let r = server.get(&format!("/api/claims?session={T8_SESSION}"));
    assert_eq!(r.status, 200, "{}", r.text());
    r.json()["claims"].as_array().expect("claims[]").clone()
}

fn phase<'a>(doc: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    doc["phases"]
        .as_array()
        .expect("phases[]")
        .iter()
        .find(|p| p["phase"] == id)
        .unwrap_or_else(|| panic!("no {id} row: {doc}"))
}

/// T8 acceptance 1, 2 and 4 against the shipping binary: with no git on the
/// PATH the Read does NOT call `workspace_since` done, it says why in a typed
/// reason, and what needs it is withheld; the claim bound to that check
/// carries the typed cause. With git back, the same record reads as computed.
#[test]
fn workspace_since_is_not_done_when_git_cannot_answer() {
    let fx = Fixture::new();
    fx.record_a_check_before_the_tree_moved();

    // PATH holding nothing: `git` cannot be found by the pipeline.
    let empty = fx.dir.path().join("no-git-here");
    std::fs::create_dir_all(&empty).expect("empty PATH dir");
    let mut ui = fx.ui();
    ui.env("PATH", &empty);
    let server = Server::start(ui);
    let doc = t8_read(&server);
    let ws = phase(&doc, "workspace_since");
    assert!(
        matches!(ws["status"].as_str(), Some("failed" | "unmeasured")),
        "git could not answer, so the workspace step is not done: {doc}"
    );
    assert_eq!(ws["reason"], "workspace_unreadable", "a typed reason: {ws}");
    for id in ["run_verifiers", "write_up"] {
        assert_eq!(phase(&doc, id)["status"], "withheld", "{id}: {doc}");
    }
    assert_eq!(
        phase(&doc, "run_verifiers")["withheld_by"],
        "workspace_since"
    );
    let claims = t8_claims(&server);
    let bound: Vec<&serde_json::Value> = claims
        .iter()
        .filter(|c| {
            c["evidence"]
                .as_array()
                .is_some_and(|e| e.iter().any(|id| id == "chk_t8_fixture"))
        })
        .collect();
    assert!(!bound.is_empty(), "a claim cites the check: {claims:?}");
    for c in &bound {
        assert_eq!(c["verdict"], "unverifiable", "{c}");
        assert_eq!(
            c["unobservable_cause"], "workspace_unreadable",
            "the cause is a typed field on the claim, not only prose: {c}"
        );
    }
    drop(server);

    // The same record, re-read with git present: the binding is computed
    // (the only change after the check is a test path), the claim carries no
    // workspace cause, and the workspace step is done. Its word is then the
    // ordinary §6.3 answer — this fixture records no command line, so
    // `narrowing_unchecked` keeps it unverifiable, which the UI calls Recorded,
    // not Fog. Fog is caused by the missing fact, not by the fixture.
    let server = Server::start(fx.ui());
    let doc = t8_read(&server);
    assert_eq!(phase(&doc, "workspace_since")["status"], "done", "{doc}");
    let claims = t8_claims(&server);
    let bound: Vec<&serde_json::Value> = claims
        .iter()
        .filter(|c| {
            c["evidence"]
                .as_array()
                .is_some_and(|e| e.iter().any(|id| id == "chk_t8_fixture"))
        })
        .collect();
    assert!(!bound.is_empty(), "{claims:?}");
    for c in bound {
        assert!(c.get("unobservable_cause").is_none(), "{c}");
        assert!(
            c["binding"]["no_relevant_change_since"].is_object(),
            "git answered, so the binding is computed: {c}"
        );
    }
}
