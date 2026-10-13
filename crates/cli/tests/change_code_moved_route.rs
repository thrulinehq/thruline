//! STEP-7-GIT-LINE (R-2026-09-28-42) — the read-time git line, through the
//! shipping binary's `GET /api/changes/<id>`.
//!
//! * The field is ONE additive field, `code_moved`, on the served Change, and
//!   the served shape's version moves 4 → 5. Every field a version-4 reader
//!   read is still there, with the same value.
//! * The route writes nothing: every table's row count and every
//!   `examination_inputs` row are identical before and after N GETs. It never
//!   re-binds the version (no new `intended` row, no applicability row).
//! * Equal heads serve `null`, exactly as a store with no git would.
//!
//! A real store and a real repository under a temporary directory; no real
//! store is opened.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use thruline_core::examined_inputs::bind_intended_at;
use thruline_core::ledger::Ledger;

const CHANGE: &str = "cccccccc-0000-4000-8000-00000000c071";

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

/// A repository with `src/lib.rs`, and a Change whose version was recorded at
/// its first commit.
fn store(db: &Path, repo: &Path) {
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/lib.rs"), b"pub fn one() {}\n").unwrap();
    git(repo, &["init", "-q", "-b", "main"]);
    git(repo, &["add", "."]);
    git(repo, &["commit", "-q", "-m", "init"]);
    let ledger = Ledger::open(db).expect("open");
    let conn = ledger.conn();
    conn.execute(
        "INSERT INTO projects(id, root_path, source, display_name, created_at)
         VALUES('p1',?1,'claude_code','Synthetic','2026-01-01T00:00:00Z')",
        [repo.to_string_lossy().to_string()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO changes(id, project_id, opened_at, title, purpose, requester, request_source)
         VALUES(?1,'p1','2026-01-02T09:00:00Z','Cart total','P','sam','person')",
        [CHANGE],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at, criteria_json,
             exclusions_json, allowed_scope_json, constraints_json, check_profile_json,
             content_sha256)
         VALUES(?1,1,'2026-01-02T09:30:00Z','[\"the total shows zero\"]','[]',
                '[\"src/**\"]','[]','[]','digest1')",
        [CHANGE],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO change_scope_acceptances(change_id, scope_version, accepted_by, accepted_at,
             rationale)
         VALUES(?1,1,'sam','2026-01-02T09:40:00Z','this is what counts')",
        [CHANGE],
    )
    .unwrap();
    let _ = conn;
    bind_intended_at(&ledger, CHANGE, "2026-01-02T09:50:00Z", Some(repo))
        .expect("bind")
        .expect("binds");
}

/// Every table's row count, and every `examination_inputs` row in full.
fn census(db: &Path) -> (Vec<(String, i64)>, Vec<String>) {
    let conn =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("read-only");
    let mut names = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap();
    let names: Vec<String> = names
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    let counts = names
        .into_iter()
        .map(|n| {
            let c: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM \"{n}\""), [], |r| r.get(0))
                .unwrap();
            (n, c)
        })
        .collect();
    let mut statement = conn
        .prepare("SELECT * FROM examination_inputs ORDER BY rowid")
        .unwrap();
    let width = statement.column_count();
    let rows = statement
        .query_map([], |r| {
            let mut cells = Vec::new();
            for i in 0..width {
                cells.push(format!("{:?}", r.get::<_, rusqlite::types::Value>(i)?));
            }
            Ok(cells.join("|"))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    (counts, rows)
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

/// The keys a version-4 reader of one served Change read.
const V4_CHANGE_KEYS: &[&str] = &[
    "schema_version",
    "id",
    "project_id",
    "opened_at",
    "request",
    "accepted_scope",
    "scope_history",
    "implementation",
    "implementation_history",
    "associations",
    "examination",
    "decision",
    "decisions",
    "reviews",
    "masking",
];

#[test]
fn after_a_commit_the_route_serves_what_moved_with_its_receipt_and_writes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".thruline")).expect("home");
    let db = dir.path().join("ledger.db");
    let repo = dir.path().join("shop");
    store(&db, &repo);
    let recorded = git(&repo, &["rev-parse", "HEAD"]);
    std::fs::write(
        repo.join("src/lib.rs"),
        b"pub fn one() {}\npub fn two() {}\n",
    )
    .unwrap();
    git(&repo, &["add", "src/lib.rs"]);
    git(&repo, &["commit", "-q", "-m", "two"]);
    let now = git(&repo, &["rev-parse", "HEAD"]);

    let server = Server::start(&home, &db);
    let path = format!("/api/changes/{CHANGE}");
    /* The first GET may be the one that pays whatever opening the store costs;
     * the census is taken after it, and every later GET must leave it alone. */
    let (status, first) = server.get(&path);
    assert_eq!(status, 200, "{first}");
    let before = census(&db);
    let mut last = first.clone();
    for _ in 0..3 {
        let (status, body) = server.get(&path);
        assert_eq!(status, 200, "{body}");
        last = body;
    }
    let after = census(&db);
    assert_eq!(before.0, after.0, "a GET changed a table's row count");
    assert_eq!(
        before.1, after.1,
        "a GET changed the examination_inputs rows"
    );
    assert_eq!(
        before
            .1
            .iter()
            .filter(|r| r.contains("Text(\"intended\")"))
            .count(),
        1,
        "the version was re-bound on read"
    );

    assert_eq!(last["schema_version"], 8, "{last}");
    let change = &last["change"];
    assert_eq!(change["schema_version"], 8);
    let moved = &change["code_moved"];
    assert!(moved.is_object(), "no code_moved field: {change}");
    assert_eq!(moved["recorded"], recorded.as_str());
    assert_eq!(moved["now"], now.as_str());
    assert_eq!(moved["changed_paths"], serde_json::json!(["src/lib.rs"]));
    assert_eq!(moved["commits"], 1);
    assert_eq!(moved["uncommitted_compared"], false);

    /* Compatibility with version 4: every key a v4 reader read is present,
     * and the only new keys are `code_moved` (version 5), `retrospective`
     * (version 6, QUAL-F2-R2), `notes` (version 7, C30 R-LEDGER) and `seconds`
     * (version 8, C30 R-LEDGER). */
    let keys: Vec<&str> = change
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    for k in V4_CHANGE_KEYS {
        assert!(keys.contains(k), "v4 key {k} is gone");
    }
    let extra: Vec<&&str> = keys
        .iter()
        .filter(|k| !V4_CHANGE_KEYS.contains(k))
        .collect();
    assert_eq!(
        extra,
        vec![&"code_moved", &"notes", &"retrospective", &"seconds"],
        "a field other than code_moved (v5), retrospective (v6), notes (v7) and seconds (v8) was added"
    );
}

#[test]
fn equal_heads_serve_null_and_the_route_still_writes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".thruline")).expect("home");
    let db = dir.path().join("ledger.db");
    let repo = dir.path().join("shop");
    store(&db, &repo);
    /* An uncommitted edit only: not compared in this slice. */
    std::fs::write(repo.join("src/lib.rs"), b"dirty\n").unwrap();

    let server = Server::start(&home, &db);
    let path = format!("/api/changes/{CHANGE}");
    let (status, _) = server.get(&path);
    assert_eq!(status, 200);
    let before = census(&db);
    let (status, body) = server.get(&path);
    assert_eq!(status, 200);
    assert_eq!(census(&db), before);
    assert!(
        body["change"]["code_moved"].is_null(),
        "equal heads must say nothing: {body}"
    );
}
