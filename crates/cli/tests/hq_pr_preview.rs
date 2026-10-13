//! `GET /api/changes/<id>/pr` (C30 R-PR): what the pull-request preview reads
//! from this machine — the recorded folder's branch and head, the receipts the
//! ThruLine Action would verify there, and the CLI's own check-run name and
//! badge words — through the shipping binary.
//!
//! * The passive read never verifies a receipt. The explicit token-guarded
//!   check verifies with disposable Git storage; both leave every ledger
//!   table, repository index, refs, objects and status untouched.
//! * The check-run name and the badge words are the generator's own
//!   (`thruline_attest::report`), so the page cannot draw another.
//! * A fact that could not be read is `null` with a reason, never a guess.
//!
//! A real store and a real repository under a temporary directory; no real
//! store is opened.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

use thruline_attest::report::{badge_label, Conclusion, CHECK_NAME};
use thruline_core::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use thruline_core::examined_inputs::bind_intended_at;
use thruline_core::ledger::Ledger;

const CHANGE: &str = "cccccccc-0000-4000-8000-00000000c0a1";
const OTHER: &str = "cccccccc-0000-4000-8000-00000000c0a2";

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

/// A Change whose recorded folder is `folder`, opened and accepted in `db`.
fn change(ledger: &Ledger, id: &str, folder: &Path) {
    let conn = ledger.conn();
    conn.execute(
        "INSERT INTO changes(id, project_id, opened_at, title, purpose, requester, request_source)
         VALUES(?1,'p1','2026-01-02T09:00:00Z','Cart total','P','sam','person')",
        [id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at, criteria_json,
             exclusions_json, allowed_scope_json, constraints_json, check_profile_json,
             content_sha256)
         VALUES(?1,1,'2026-01-02T09:30:00Z','[\"the total shows zero\"]','[]',
                '[\"src/**\"]','[]','[]','digest1')",
        [id],
    )
    .unwrap();
    bind_intended_at(ledger, id, "2026-01-02T09:50:00Z", Some(folder))
        .expect("bind")
        .expect("binds");
}

/// A repository on `main` with one commit, and a Change recorded in it.
fn store(db: &Path, repo: &Path) -> Ledger {
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/lib.rs"), b"pub fn one() {}\n").unwrap();
    git(repo, &["init", "-q", "-b", "main"]);
    git(repo, &["add", "."]);
    git(repo, &["commit", "-q", "-m", "init"]);
    let ledger = Ledger::open(db).expect("open");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES('p1',?1,'claude_code','Synthetic','2026-01-01T00:00:00Z')",
            [repo.to_string_lossy().to_string()],
        )
        .unwrap();
    change(&ledger, CHANGE, repo);
    ledger
}

/// Every table's row count.
fn census(db: &Path) -> Vec<(String, i64)> {
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
    names
        .into_iter()
        .map(|n| {
            let c: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM \"{n}\""), [], |r| r.get(0))
                .unwrap();
            (n, c)
        })
        .collect()
}

/// What a write to the repository would move: the index, every ref, the
/// object count and the working tree's status.
fn repo_state(repo: &Path) -> Vec<String> {
    let index = std::fs::read(repo.join(".git/index")).unwrap_or_default();
    vec![
        format!("{:?}", index),
        git(repo, &["for-each-ref"]),
        git(repo, &["count-objects", "-v"]),
        git(repo, &["status", "--porcelain", "--untracked-files=all"]),
    ]
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
        Self::start_with_environment(home, db, None, None)
    }

    fn start_with_trace(home: &Path, db: &Path, trace: Option<&Path>) -> Server {
        Self::start_with_environment(home, db, trace, None)
    }

    fn start_with_environment(
        home: &Path,
        db: &Path,
        trace: Option<&Path>,
        objects: Option<&Path>,
    ) -> Server {
        let mut command = Command::new(bin());
        command
            .env("HOME", home)
            .env("THRULINE_HOME", home.join(".thruline"))
            .current_dir(home)
            .arg("--db")
            .arg(db)
            .args(["ui", "--no-open", "--port", "0"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(trace) = trace {
            command.env("GIT_TRACE", trace);
        }
        if let Some(objects) = objects {
            command.env("GIT_OBJECT_DIRECTORY", objects);
            command.env("GIT_ALTERNATE_OBJECT_DIRECTORIES", objects);
        }
        let mut child = command.spawn().expect("thruline ui");
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

    fn request(&self, method: &str, path: &str) -> (u16, serde_json::Value) {
        let mut socket = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nOrigin: http://127.0.0.1:{}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            self.port, self.port
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
        (
            status,
            serde_json::from_str(&body).unwrap_or(serde_json::Value::Null),
        )
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

    fn checked_get(&self, path: &str, token: &str) -> (u16, serde_json::Value) {
        self.checked_get_with(
            path,
            token,
            &format!("http://127.0.0.1:{}", self.port),
            "same-origin",
        )
    }

    fn checked_get_with(
        &self,
        path: &str,
        token: &str,
        origin: &str,
        site: &str,
    ) -> (u16, serde_json::Value) {
        let mut socket = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        let origin_header = if origin.is_empty() {
            String::new()
        } else {
            format!("Origin: {origin}\r\n")
        };
        let token_header = if token.is_empty() {
            String::new()
        } else {
            format!("X-ThruLine-Run-Token: {token}\r\n")
        };
        let request = format!(
            "GET {path}?check=1 HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n{origin_header}Sec-Fetch-Site: {site}\r\n{token_header}Connection: close\r\n\r\n",
            self.port
        );
        socket.write_all(request.as_bytes()).expect("request");
        let mut raw = Vec::new();
        socket.read_to_end(&mut raw).expect("response");
        let text = String::from_utf8_lossy(&raw);
        let status: u16 = text.split_whitespace().nth(1).unwrap().parse().unwrap();
        let body = text.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
        (
            status,
            serde_json::from_str(body).unwrap_or_else(|_| serde_json::json!(body)),
        )
    }
}

fn path(id: &str) -> String {
    format!("/api/changes/{id}/pr")
}

fn machine() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".thruline")).expect("home");
    let db = dir.path().join("ledger.db");
    (dir, home, db)
}

fn signed_receipt(home: &Path, repo: &Path) -> std::path::PathBuf {
    let h = home.join(".thruline");
    let session = "cccccccc-8888-4999-8aaa-bbbbbbbbbbbb";
    for event in [
        format!(
            r#"{{"hook_event_name":"SessionStart","session_id":"{session}","transcript_path":"/t","cwd":"{}","source":"startup"}}"#,
            repo.display()
        ),
        format!(
            r#"{{"hook_event_name":"Stop","session_id":"{session}","transcript_path":"/t","cwd":"{}","last_assistant_message":"All tests pass.","stop_hook_active":false}}"#,
            repo.display()
        ),
    ] {
        let mut child = Command::new(bin())
            .args(["hook", "claude-code"])
            .env("THRULINE_HOME", &h)
            .current_dir(repo)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .expect("synthetic hook");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(event.as_bytes())
            .unwrap();
        assert!(child.wait().unwrap().success());
    }
    let folded = Command::new(bin())
        .arg("fold")
        .env("THRULINE_HOME", &h)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        folded.status.success(),
        "synthetic fold failed: {}",
        String::from_utf8_lossy(&folded.stderr)
    );
    let receipt = repo.join(".thruline/receipts/r.json");
    std::fs::create_dir_all(receipt.parent().unwrap()).unwrap();
    let issued = Command::new(bin())
        .args([
            "receipt",
            "--predicate-version",
            "0.1",
            "--session",
            session,
            "--out",
        ])
        .arg(&receipt)
        .env("THRULINE_HOME", &h)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        issued.status.success(),
        "synthetic receipt failed: {}",
        String::from_utf8_lossy(&issued.stderr)
    );
    receipt
}

fn cli_receipt(home: &Path, repo: &Path, receipt: &Path, format: &str) -> String {
    let out = Command::new(bin())
        .arg("verify-receipt")
        .arg(receipt)
        .args(["--offline", "--format", format, "--head-sha"])
        .arg(git(repo, &["rev-parse", "HEAD"]))
        .env("THRULINE_HOME", home.join(".thruline"))
        .env_remove("GITHUB_SHA")
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        matches!(out.status.code(), Some(0..=2)),
        "CLI verification failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn explicit_check_returns_the_cli_result_without_changing_repo_or_ledger() {
    let (dir, home, db) = machine();
    let repo = dir.path().join("shop");
    drop(store(&db, &repo));
    let receipt = signed_receipt(&home, &repo);
    let expected_run: serde_json::Value =
        serde_json::from_str(&cli_receipt(&home, &repo, &receipt, "check-run")).unwrap();
    let expected_badge = cli_receipt(&home, &repo, &receipt, "badge");
    let isolated = tempfile::tempdir().unwrap();
    std::fs::create_dir(isolated.path().join("objects")).unwrap();
    let isolated_out = Command::new(bin())
        .arg("verify-receipt")
        .arg(&receipt)
        .args(["--offline", "--format", "check-run", "--head-sha"])
        .arg(git(&repo, &["rev-parse", "HEAD"]))
        .arg("--repo")
        .arg(&repo)
        .env("THRULINE_HOME", home.join(".thruline"))
        .env("GIT_OBJECT_DIRECTORY", isolated.path().join("objects"))
        .env(
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            repo.join(".git/objects"),
        )
        .env("TMPDIR", isolated.path())
        .current_dir(&repo)
        .output()
        .unwrap();
    let isolated_run: serde_json::Value = serde_json::from_slice(&isolated_out.stdout).unwrap();
    assert_eq!(
        expected_run["conclusion"], isolated_run["conclusion"],
        "direct={expected_run:#}\nisolated={isolated_run:#}"
    );
    let trace = repo.join("inherited-git-trace.log");
    let server = Server::start_with_trace(&home, &db, Some(&trace));
    let token = server.get("/api/run/token").1["token"]
        .as_str()
        .unwrap()
        .to_string();
    let (passive_status, passive) = server.get(&path(CHANGE));
    assert_eq!(passive_status, 200, "{passive}");
    assert_eq!(passive["receipts"]["newest"], "r.json");
    assert!(
        passive.get("checked").is_none(),
        "the passive preview must not verify"
    );
    let before = (repo_state(&repo), census(&db));

    let (status, checked) = server.checked_get(&path(CHANGE), &token);
    assert_eq!(status, 200, "{checked}");
    assert_eq!(checked["checked"]["receipt"], "r.json");
    assert_eq!(
        checked["checked"]["head"],
        git(&repo, &["rev-parse", "HEAD"])
    );
    assert_eq!(
        checked["checked"]["fingerprint"],
        passive["receipts"]["newest_fingerprint"]
    );
    assert_eq!(
        checked["checked"]["check_run"]["name"],
        expected_run["name"]
    );
    assert_eq!(
        checked["checked"]["check_run"]["head_sha"],
        expected_run["head_sha"]
    );
    assert_eq!(
        checked["checked"]["check_run"]["conclusion"],
        expected_run["conclusion"]
    );
    assert_eq!(
        checked["checked"]["check_run"]["output"]["title"],
        expected_run["output"]["title"]
    );
    assert_eq!(checked["checked"]["badge"]["svg"], expected_badge);
    assert_eq!(
        before,
        (repo_state(&repo), census(&db)),
        "explicit check changed the repository or ledger"
    );
    assert!(!trace.exists(), "Git traced into the checked project");
}

#[test]
fn explicit_check_uses_its_own_repository_objects_despite_ambient_git_objects() {
    let (dir, home, db) = machine();
    let repo = dir.path().join("shop");
    drop(store(&db, &repo));
    let receipt = signed_receipt(&home, &repo);
    let expected: serde_json::Value =
        serde_json::from_str(&cli_receipt(&home, &repo, &receipt, "check-run")).unwrap();
    let foreign = dir.path().join("foreign-objects");
    std::fs::create_dir(&foreign).unwrap();
    let server = Server::start_with_environment(&home, &db, None, Some(&foreign));
    let token = server.get("/api/run/token").1["token"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, body) = server.checked_get(&path(CHANGE), &token);
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["checked"]["check_run"]["conclusion"],
        expected["conclusion"]
    );
    assert_eq!(
        body["checked"]["check_run"]["output"]["title"],
        expected["output"]["title"]
    );
}

#[test]
fn explicit_check_refuses_missing_token_and_foreign_page_before_reading_receipt() {
    let (dir, home, db) = machine();
    let repo = dir.path().join("shop");
    drop(store(&db, &repo));
    let server = Server::start(&home, &db);
    let token = server.get("/api/run/token").1["token"]
        .as_str()
        .unwrap()
        .to_string();
    let own = format!("http://127.0.0.1:{}", server.port);
    let before = (repo_state(&repo), census(&db));
    assert_eq!(
        server
            .checked_get_with(&path(CHANGE), "", &own, "same-origin")
            .0,
        403
    );
    assert_eq!(server.checked_get(&path(CHANGE), "wrong").0, 403);
    assert_eq!(
        server
            .checked_get_with(&path(CHANGE), &token, "http://evil.invalid", "cross-site")
            .0,
        403
    );
    assert_eq!(
        server
            .checked_get_with(&path(CHANGE), &token, "", "cross-site")
            .0,
        403
    );
    assert_eq!(before, (repo_state(&repo), census(&db)));
}

#[test]
fn explicit_check_refuses_repository_clean_filters_before_starting_verifier() {
    let (dir, home, db) = machine();
    let repo = dir.path().join("shop");
    drop(store(&db, &repo));
    let receipts = repo.join(".thruline/receipts");
    std::fs::create_dir_all(&receipts).unwrap();
    std::fs::write(receipts.join("r.json"), b"{}").unwrap();
    std::fs::write(repo.join(".gitattributes"), b"* filter=unsafe\n").unwrap();
    let marker = repo.join("filter-ran");
    git(
        &repo,
        &[
            "config",
            "filter.unsafe.clean",
            &format!("touch {}", marker.display()),
        ],
    );
    let server = Server::start(&home, &db);
    let token = server.get("/api/run/token").1["token"]
        .as_str()
        .unwrap()
        .to_string();
    let before = (git(&repo, &["count-objects", "-v"]), census(&db));
    let (status, body) = server.checked_get(&path(CHANGE), &token);
    assert_eq!(status, 503, "{body}");
    assert_eq!(body["error"], "receipt_check_unavailable");
    assert!(
        !marker.exists(),
        "a repository clean filter ran during the refused check"
    );
    assert_eq!(before, (git(&repo, &["count-objects", "-v"]), census(&db)));
}

#[test]
fn explicit_check_refuses_worktree_scoped_clean_filter() {
    let (dir, home, db) = machine();
    let repo = dir.path().join("shop");
    drop(store(&db, &repo));
    let receipts = repo.join(".thruline/receipts");
    std::fs::create_dir_all(&receipts).unwrap();
    std::fs::write(receipts.join("r.json"), b"{}").unwrap();
    git(&repo, &["config", "extensions.worktreeConfig", "true"]);
    git(
        &repo,
        &["config", "--worktree", "filter.unsafe.clean", "false"],
    );
    let server = Server::start(&home, &db);
    let token = server.get("/api/run/token").1["token"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, body) = server.checked_get(&path(CHANGE), &token);
    assert_eq!(status, 503, "{body}");
    assert_eq!(body["error"], "receipt_check_unavailable");
}

#[test]
fn it_serves_the_branch_the_head_and_no_receipt_and_writes_nothing() {
    let (dir, home, db) = machine();
    let repo = dir.path().join("shop");
    drop(store(&db, &repo));
    let head = git(&repo, &["rev-parse", "HEAD"]);
    /* an uncommitted file: a snapshot of the tree would have to write it */
    std::fs::write(repo.join("src/new.rs"), b"pub fn two() {}\n").unwrap();

    let server = Server::start(&home, &db);
    let (status, first) = server.get(&path(CHANGE));
    assert_eq!(status, 200, "{first}");
    let before = (census(&db), repo_state(&repo));
    let mut last = first;
    for _ in 0..3 {
        let (status, body) = server.get(&path(CHANGE));
        assert_eq!(status, 200, "{body}");
        last = body;
    }
    assert_eq!(
        before,
        (census(&db), repo_state(&repo)),
        "a GET wrote to the store or the repository"
    );

    assert_eq!(last["schema_version"], 1, "{last}");
    assert_eq!(last["change_id"], CHANGE);
    assert!(last["read_at"].as_str().is_some_and(|t| t.ends_with('Z')));
    assert_eq!(
        last["folder"],
        serde_json::json!({"recorded": true, "present": true, "name": "shop", "reason": null})
    );
    assert_eq!(
        last["git"],
        serde_json::json!({"branch": "main", "head": head, "detached": false, "reason": null})
    );
    assert_eq!(
        last["receipts"],
        serde_json::json!({"dir": ".thruline/receipts", "count": 0, "newest": null, "newest_fingerprint": null, "reason": null})
    );
    /* the generator's own name and words, not a copy of them */
    assert_eq!(last["check_run"]["name"], CHECK_NAME);
    let words: Vec<serde_json::Value> = [
        Conclusion::Success,
        Conclusion::Neutral,
        Conclusion::Failure,
    ]
    .into_iter()
    .map(|c| {
        let (label, colour) = badge_label(c);
        serde_json::json!({"conclusion": c.as_str(), "label": label, "colour": colour})
    })
    .collect();
    assert_eq!(last["badge"]["states"], serde_json::Value::Array(words));
    assert!(last["masking"].is_object());
    /* nothing names the folder's full path */
    assert!(
        !last.to_string().contains(&repo.display().to_string()),
        "the full folder path was served: {last}"
    );
}

#[test]
fn the_newest_regular_json_receipt_is_named_and_counted() {
    let (dir, home, db) = machine();
    let repo = dir.path().join("shop");
    drop(store(&db, &repo));
    let receipts = repo.join(".thruline/receipts");
    std::fs::create_dir_all(receipts.join("nested.json")).unwrap();
    let old = SystemTime::now() - Duration::from_secs(3600);
    for (name, age) in [
        ("a-old.json", old),
        ("b-newest.json", SystemTime::now()),
        ("c-older.json", old - Duration::from_secs(60)),
    ] {
        let f = std::fs::File::create(receipts.join(name)).unwrap();
        f.set_modified(age).unwrap();
    }
    std::fs::write(receipts.join("notes.txt"), b"not a receipt").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(receipts.join("a-old.json"), receipts.join("z-link.json")).unwrap();

    let server = Server::start(&home, &db);
    let (status, body) = server.get(&path(CHANGE));
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["receipts"]["count"], 3, "{body}");
    assert_eq!(body["receipts"]["newest"], "b-newest.json");
    assert!(body["receipts"]["reason"].is_null());
}

#[test]
fn a_detached_head_names_its_commit_and_no_branch() {
    let (dir, home, db) = machine();
    let repo = dir.path().join("shop");
    drop(store(&db, &repo));
    let head = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["checkout", "-q", "--detach"]);

    let server = Server::start(&home, &db);
    let (status, body) = server.get(&path(CHANGE));
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["git"],
        serde_json::json!({"branch": null, "head": head, "detached": true, "reason": null})
    );
}

#[test]
fn what_could_not_be_read_is_null_with_a_reason() {
    let (dir, home, db) = machine();
    let repo = dir.path().join("shop");
    let ledger = store(&db, &repo);
    /* a second Change recorded in a folder that is not a repository, which
     * is then removed from this machine */
    let plain = dir.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    change(&ledger, OTHER, &plain);
    drop(ledger);

    let server = Server::start(&home, &db);
    let (status, body) = server.get(&path(OTHER));
    assert_eq!(status, 200, "{body}");
    assert!(body["git"]["branch"].is_null() && body["git"]["head"].is_null());
    assert!(body["git"]["detached"].is_null());
    assert_eq!(body["git"]["reason"], "the folder is not a git repository");
    assert_eq!(body["receipts"]["count"], 0);

    std::fs::remove_dir_all(&plain).unwrap();
    let (status, body) = server.get(&path(OTHER));
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["folder"]["present"], false);
    assert_eq!(
        body["folder"]["reason"],
        "the folder recorded for this Change is not on this machine"
    );
    assert!(body["git"]["branch"].is_null() && body["git"]["head"].is_null());
    assert!(body["receipts"]["count"].is_null());
    assert_eq!(body["receipts"]["reason"], body["folder"]["reason"]);
}

#[test]
fn it_answers_only_a_plain_get_of_a_change_this_store_holds() {
    let (dir, home, db) = machine();
    let repo = dir.path().join("shop");
    drop(store(&db, &repo));
    let server = Server::start(&home, &db);
    let (status, body) = server.get(&path("cccccccc-0000-4000-8000-0000000000ff"));
    assert_eq!(
        (status, body),
        (404, serde_json::json!({"error": "no_such_change"}))
    );
    let (status, body) = server.get(&format!("{}?x=1", path(CHANGE)));
    assert_eq!(
        (status, body),
        (400, serde_json::json!({"error": "unknown_query_parameter"}))
    );
    let before = census(&db);
    let (status, body) = server.request("POST", &path(CHANGE));
    assert_eq!(
        (status, body),
        (405, serde_json::json!({"error": "method_not_allowed"}))
    );
    assert_eq!(census(&db), before);
    /* the one-Change route is still the one-Change route */
    let (status, body) = server.get(&format!("/api/changes/{CHANGE}"));
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["change"]["id"], CHANGE);
}

/// The same process harness exercises the separate read-only Whole Story
/// integrity route. The answer names exactly the confirmed session links and
/// leaves every ledger row unchanged across repeated reads.
#[test]
fn a_change_integrity_read_rechecks_confirmed_links_without_writing() {
    let (dir, home, db) = machine();
    // This route reads the ledger directly. Keep the unrelated legacy export
    // startup out of this focused HTTP check.
    let export = dir.path().join("export");
    std::fs::create_dir_all(&export).unwrap();
    std::fs::write(export.join("index.json"), b"{}").unwrap();
    let repo = dir.path().join("shop");
    let ledger = store(&db, &repo);
    let conn = ledger.conn();
    conn.execute(
        "INSERT INTO sessions(id, project_id, file_path, last_seq, event_count)
         VALUES('integrity-session', 'p1', '/synthetic/session.jsonl', 1, 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO change_session_associations(id, change_id, session_id, provenance,
            basis, actor, asserted_at)
         VALUES('integrity-link', ?1, 'integrity-session', 'confirmed', 'user', 'sam', 'now')",
        [CHANGE],
    )
    .unwrap();
    let raw = sha256_hex(b"one recorded line");
    let prev = genesis_prev_hash("p1");
    let head = chain_hash(&prev, &raw, "integrity-session", 1);
    conn.execute(
        "INSERT INTO events(id, project_id, session_id, seq, kind, raw_file, raw_line,
            raw_sha256, prev_hash, chain_hash)
         VALUES('integrity-event', 'p1', 'integrity-session', 1, 'user_msg',
            '/synthetic/session.jsonl', 1, ?1, ?2, ?3)",
        rusqlite::params![raw, prev, head],
    )
    .unwrap();
    conn.execute(
        "UPDATE sessions SET head_chain_hash=?1 WHERE id='integrity-session'",
        [&head],
    )
    .unwrap();
    drop(ledger);

    let server = Server::start(&home, &db);
    let path = format!("/api/changes/{CHANGE}/integrity");
    let before = census(&db);
    for _ in 0..2 {
        let (status, body) = server.get(&path);
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["state"], "INTACT", "{body}");
        assert_eq!(body["sessions_checked"], 1);
        assert_eq!(body["links_checked"], 1);
        assert_eq!(body["source_files_checked"], false);
        assert_eq!(body["saved_export_checked"], false);
    }
    assert_eq!(census(&db), before, "GET must append or change no row");
    assert_eq!(server.request("POST", &path).0, 405);
    assert_eq!(server.get(&format!("{path}?x=1")).0, 400);
    assert_eq!(server.get("/api/changes/no-such-change/integrity").0, 404);

    let ledger = Ledger::open(&db).unwrap();
    ledger
        .conn()
        .execute(
            "UPDATE sessions SET head_chain_hash='wrong' WHERE id='integrity-session'",
            [],
        )
        .unwrap();
    drop(ledger);
    let (status, body) = server.get(&path);
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["state"], "MISMATCH", "{body}");
}
