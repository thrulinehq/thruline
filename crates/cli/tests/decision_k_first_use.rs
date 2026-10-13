//! Family K suggestions must be reachable on first use without accepting a rule.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use serde_json::Value;
use thruline_core::ledger::Ledger;

const DECISION_TABLES: [&str; 7] = [
    "decisions",
    "decision_revisions",
    "decision_acceptances",
    "decision_retirements",
    "decision_verdicts",
    "decision_waivers",
    "decision_person_acts",
];

struct Fixture {
    temp: tempfile::TempDir,
    hq_id: String,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("scratch directory");
        let root = temp.path().join("repo");
        let store = temp.path().join("home/.thruline");
        std::fs::create_dir_all(&root).expect("scratch project");
        std::fs::create_dir_all(&store).expect("scratch store");
        assert!(Command::new("git")
            .args(["init", "-q"])
            .current_dir(&root)
            .status()
            .expect("initialize scratch Git repository")
            .success());
        let ledger = Ledger::open(store.join("ledger.db")).expect("scratch ledger");
        ledger
            .conn()
            .execute(
                "INSERT INTO projects(id,root_path,source,created_at) VALUES(?1,?2,'codex','2026-10-07T00:00:00Z')",
                ("first-use-project", root.to_str().expect("root path")),
            )
            .expect("scratch project row");
        let hq_id = thruline_core::hq::hq_projects(&ledger)
            .expect("HQ projects")
            .into_iter()
            .next()
            .expect("one HQ project")
            .id;
        Self { temp, hq_id }
    }

    fn root(&self) -> PathBuf {
        self.temp.path().join("repo")
    }

    fn db(&self) -> PathBuf {
        self.temp.path().join("home/.thruline/ledger.db")
    }

    fn home(&self) -> PathBuf {
        self.temp.path().join("home")
    }

    fn init_git(&self) {
        let root = self.root();
        let init = Command::new("git")
            .args(["init", "-q"])
            .current_dir(&root)
            .status()
            .expect("initialize scratch Git repository");
        assert!(init.success(), "scratch Git initialization failed");
        let commit = Command::new("git")
            .args([
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "First-use fixture",
            ])
            .current_dir(root)
            .status()
            .expect("commit scratch Git repository");
        assert!(commit.success(), "scratch Git commit failed");
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
    fn start(fixture: &Fixture) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_thruline"))
            .args(["ui", "--no-open", "--port", "0"])
            .env("HOME", fixture.home())
            .env("THRULINE_HOME", fixture.home().join(".thruline"))
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .current_dir(fixture.root())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("UI server starts");
        for line in BufReader::new(child.stdout.take().expect("server stdout")).lines() {
            let Ok(line) = line else { break };
            if let Some(rest) = line.trim().strip_prefix("ThruLine is at http://127.0.0.1:") {
                if let Ok(port) = rest
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                {
                    return Self { child, port };
                }
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("UI server did not report a port");
    }

    fn get(&self, path: &str) -> (u16, Value) {
        let mut socket = TcpStream::connect(("127.0.0.1", self.port)).expect("connect to UI");
        write!(
            socket,
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            self.port
        )
        .expect("send GET");
        let mut bytes = Vec::new();
        socket.read_to_end(&mut bytes).expect("read response");
        let text = String::from_utf8(bytes).expect("UTF-8 response");
        let status = text[9..12].parse().expect("HTTP status");
        let body = text.split_once("\r\n\r\n").expect("HTTP body").1;
        (status, serde_json::from_str(body).expect("JSON body"))
    }
}

fn decision_rows(ledger: &Ledger) -> BTreeMap<&'static str, i64> {
    DECISION_TABLES
        .into_iter()
        .map(|table| {
            let count = ledger
                .conn()
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .expect("decision row count");
            (table, count)
        })
        .collect()
}

fn data_version(ledger: &Ledger) -> i64 {
    ledger
        .conn()
        .query_row("PRAGMA data_version", [], |row| row.get(0))
        .expect("SQLite data version")
}

fn assert_defaults(document: &Value) {
    assert_eq!(document["schema_version"], 1);
    let defaults = document["defaults"].as_array().expect("defaults array");
    assert_eq!(defaults.len(), 18, "all suggested standing rules");
    let codes = defaults
        .iter()
        .map(|row| row["code"].as_str().expect("K code"))
        .collect::<Vec<_>>();
    let expected = (1..=18).map(|n| format!("K{n}")).collect::<Vec<_>>();
    assert_eq!(
        codes,
        expected.iter().map(String::as_str).collect::<Vec<_>>()
    );
    for row in defaults {
        let code = row["code"].as_str().expect("K code");
        assert!(!row["rule"].as_str().expect("rule").trim().is_empty());
        assert!(
            !row["producing_checks"]
                .as_array()
                .expect("producing checks")
                .is_empty(),
            "{code} needs named producing checks"
        );
        assert_eq!(row["state"], "proposed_advisory", "{code}");
        assert!(
            row["accepted_params"].is_null(),
            "{code} has no accepted scope"
        );
        assert!(row["verdict"].is_null(), "{code} has no verdict");
        assert!(
            !row["coverage_note"]
                .as_str()
                .expect("coverage note")
                .trim()
                .is_empty(),
            "{code} needs a coverage explanation"
        );
        let coverage = row["coverage"].as_str().expect("coverage state");
        let expected_coverage = if code == "K18" {
            "unimplemented"
        } else {
            "partial"
        };
        assert_eq!(coverage, expected_coverage, "{code}");
    }
}

#[test]
fn first_use_defaults_are_available_from_decisions_cli_without_a_git_version_or_ledger_write() {
    let fixture = Fixture::new();
    let ledger = Ledger::open(fixture.db()).expect("observe ledger");
    let before_rows = decision_rows(&ledger);
    let before_version = data_version(&ledger);
    let output = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .arg("--db")
        .arg(fixture.db())
        .args(["decisions", "defaults", "--project", "first-use-project"])
        .arg("--root")
        .arg(fixture.root())
        .arg("--json")
        .output()
        .expect("decisions defaults command");
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let document: Value = serde_json::from_slice(&output.stdout).expect("CLI JSON");
    assert_eq!(document["shape"], "thruline.decide.defaults");
    assert_defaults(&document);
    assert_eq!(document["project_id"], "first-use-project");
    assert_eq!(
        decision_rows(&ledger),
        before_rows,
        "CLI appended a decision row"
    );
    assert_eq!(data_version(&ledger), before_version, "CLI wrote to ledger");
}

#[test]
fn first_use_defaults_are_available_from_read_only_decisions_http_without_ledger_write() {
    let fixture = Fixture::new();
    // HQ's UI startup can resolve a project version; the CLI test above keeps
    // its scratch root without Git to prove defaults need no version at all.
    fixture.init_git();
    let server = Server::start(&fixture);
    let ledger = Ledger::open(fixture.db()).expect("observe ledger");
    let before_rows = decision_rows(&ledger);
    let before_version = data_version(&ledger);
    let path = format!("/api/hq/projects/{}/decisions/defaults", fixture.hq_id);
    let (status, document) = server.get(&path);
    assert_eq!(
        status, 200,
        "GET should expose suggested defaults: {document}"
    );
    assert_defaults(&document);
    assert_eq!(document["project_id"], fixture.hq_id);
    assert_eq!(
        decision_rows(&ledger),
        before_rows,
        "GET appended a decision row"
    );
    assert_eq!(data_version(&ledger), before_version, "GET wrote to ledger");
}
