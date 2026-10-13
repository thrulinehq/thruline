//! QUAL-F2-R2 (`Q7`, brief §16 row 7) — the shipping binary, end to end: a
//! Change is opened on one commit, the work lands as another, the Change is
//! prepared on it, and only then is what counts as done accepted. The served
//! Change (`GET /api/changes/<id>`) says so in a field named `retrospective`
//! — the field the Q07 case's capability C8 looks for — and says nothing when
//! the order was the other way round.
//!
//! Every step is the product's own command (`change new`, `change prepare`,
//! `change accept`) against a real repository and a real store under a
//! temporary directory; nothing here reaches a real store. The stamps the
//! product writes are whole seconds and "before" is strict, so the steps are
//! separated by more than a second, as a person's are.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use rusqlite::params;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000p0f2";
const PURPOSE: &str = "Two retry requests for the same delivery do not enqueue duplicate work. \
    A delivery that was still waiting when the service was stopped is still exactly one \
    delivery after it starts again, and it is attempted again.";

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .expect("git must run");
    assert!(out.status.success(), "git {args:?} failed");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn pause() {
    std::thread::sleep(Duration::from_millis(1_100));
}

/// A project, a repository at its first commit, and the id of the second.
struct World {
    _dir: tempfile::TempDir,
    home: std::path::PathBuf,
    db: std::path::PathBuf,
    repo: std::path::PathBuf,
    first: String,
    second: String,
}

fn world() -> World {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".thruline")).expect("home");
    let db = dir.path().join("ledger.db");
    let repo = dir.path().join("retry-delivery");
    std::fs::create_dir_all(repo.join("src")).expect("src");
    std::fs::write(repo.join("src/lib.rs"), b"pub fn deliver() {}\n").expect("write");
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "before the retry flow"]);
    let first = git(&repo, &["rev-parse", "HEAD"]);
    std::fs::write(
        repo.join("src/lib.rs"),
        b"pub fn deliver() {}\npub fn retry() {}\n",
    )
    .expect("write");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "the retry flow"]);
    let second = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["checkout", "-q", &first]);

    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES(?1, ?2, 'claude_code', 'Retry delivery', '2026-01-01T00:00:00Z')",
            params![PROJECT, repo.display().to_string()],
        )
        .expect("project");
    World {
        _dir: dir,
        home,
        db,
        repo,
        first,
        second,
    }
}

impl World {
    fn thruline(&self, args: &[&str]) -> std::process::Output {
        let out = Command::new(bin())
            .env("HOME", &self.home)
            .env("THRULINE_HOME", self.home.join(".thruline"))
            .current_dir(&self.repo)
            .arg("--db")
            .arg(&self.db)
            .args(args)
            .output()
            .expect("running thruline");
        assert!(
            out.status.success(),
            "thruline {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        out
    }

    fn open(&self) -> String {
        let out = self.thruline(&[
            "change",
            "new",
            "--for",
            PROJECT,
            "--root",
            self.repo.to_str().expect("utf8"),
            "--title",
            "Retry a failed delivery",
            "--purpose",
            PURPOSE,
            "--requester",
            "sam",
            "--json",
        ]);
        let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
        body["change"].as_str().expect("change id").to_string()
    }

    fn land_and_prepare(&self, change: &str) {
        git(&self.repo, &["merge", "-q", "--ff-only", &self.second]);
        self.thruline(&[
            "change",
            "prepare",
            change,
            "--root",
            self.repo.to_str().expect("utf8"),
        ]);
    }

    fn accept(&self, change: &str) {
        self.thruline(&[
            "change",
            "accept",
            change,
            "--criterion",
            "1",
            "--criterion",
            "2",
            "--scope",
            "src/**",
            "--actor",
            "sam",
            "--rationale",
            "this is what I want checked",
        ]);
    }

    fn served(&self, change: &str) -> serde_json::Value {
        let server = Server::start(&self.home, &self.db);
        let (status, body) = server.get(&format!("/api/changes/{change}"));
        assert_eq!(status, 200, "{body}");
        body
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
    fn start(home: &Path, db: &Path) -> Server {
        let mut child = Command::new(bin())
            .env("HOME", home)
            .env("THRULINE_HOME", home.join(".thruline"))
            .current_dir(home)
            .arg("--db")
            .arg(db)
            .args(["ui", "--no-open", "--port", "0"])
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
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(line) => {
                    if let Some(rest) = line.trim().strip_prefix("ThruLine is at http://127.0.0.1:")
                    {
                        let port: u16 = rest
                            .trim()
                            .split('/')
                            .next()
                            .expect("port")
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
                    panic!("`thruline ui` never printed a loopback URL.\n{err}");
                }
            }
        }
    }

    fn get(&self, path: &str) -> (u16, serde_json::Value) {
        let mut socket = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            self.port
        );
        socket.write_all(request.as_bytes()).expect("request");
        let mut raw = Vec::new();
        socket.read_to_end(&mut raw).expect("response");
        let text = String::from_utf8_lossy(&raw).to_string();
        let status: u16 = text
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .expect("status");
        let body = text
            .split_once("\r\n\r\n")
            .map(|(_, b)| b.to_string())
            .unwrap_or_default();
        (status, serde_json::from_str(&body).expect("json body"))
    }
}

/// Every key name at any depth, the way the Q07 C8 probe counts "field names".
fn field_names(value: &serde_json::Value, into: &mut std::collections::BTreeSet<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                into.insert(k.clone());
                field_names(v, into);
            }
        }
        serde_json::Value::Array(items) => items.iter().for_each(|v| field_names(v, into)),
        _ => {}
    }
}

/// Q07 V1: opened on the commit before the work, the work lands, the Change is
/// prepared on it, and only then is what counts as done accepted.
#[test]
fn an_acceptance_after_the_work_landed_is_served_as_retrospective() {
    let w = world();
    let change = w.open();
    pause();
    w.land_and_prepare(&change);
    pause();
    w.accept(&change);

    let body = w.served(&change);
    let mut names = std::collections::BTreeSet::new();
    field_names(&body, &mut names);
    assert!(
        names.contains("retrospective"),
        "the served Change has no `retrospective` field: {names:?}"
    );

    let served = &body["change"];
    let r = &served["retrospective"];
    assert!(
        r.is_object(),
        "expected a retrospective acceptance; got {r}"
    );
    assert_eq!(r["opened_revision"], w.first.as_str());
    assert_eq!(r["work_revision"], w.second.as_str());
    assert_eq!(r["scope_version"], 1);
    let accepted_at = served["accepted_scope"]["acceptance"]["accepted_at"]
        .as_str()
        .expect("the acceptance's own time");
    assert_eq!(
        r["accepted_at"], accepted_at,
        "the row's own time, unchanged"
    );
    assert_eq!(
        r["accepted_on"].as_str(),
        accepted_at.get(..10),
        "the date said is the acceptance's own date"
    );
    let work_at = r["work_recorded_at"].as_str().expect("the work's time");
    assert!(
        work_at < accepted_at,
        "the work ({work_at}) is recorded before the acceptance ({accepted_at})"
    );
}

/// Q07 CTL: accepted first, then the work lands. The field is served, and it
/// says nothing.
#[test]
fn an_acceptance_before_the_work_landed_is_served_as_null() {
    let w = world();
    let change = w.open();
    pause();
    w.accept(&change);
    pause();
    w.land_and_prepare(&change);

    let body = w.served(&change);
    let served = body["change"].as_object().expect("the served Change");
    assert!(
        served.contains_key("retrospective"),
        "the field is served on every Change: {:?}",
        served.keys().collect::<Vec<_>>()
    );
    assert!(
        served["retrospective"].is_null(),
        "accepted first: nothing says after; got {}",
        served["retrospective"]
    );
}
