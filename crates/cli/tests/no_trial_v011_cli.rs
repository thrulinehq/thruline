//! C20: the approval door needs Pro, including on a home left by v0.1.0.
//! These tests ask the real binary and keep every file in a throwaway home.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use rand_core::OsRng;
use rusqlite::params;
use sha2::{Digest, Sha256};

const CHANGE_ID: &str = "cccccccc-0000-4000-8000-00000000c001";
const PROJECT_ID: &str = "cccccccc-0000-4000-8000-00000000p001";
const VERSION: &str = "snap_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

struct Home {
    dir: tempfile::TempDir,
}

impl Home {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("h/.thruline")).expect("home");
        std::fs::create_dir_all(dir.path().join("repo")).expect("repo");
        Self { dir }
    }

    fn thruline_home(&self) -> PathBuf {
        self.dir.path().join("h/.thruline")
    }

    fn trial(&self) -> PathBuf {
        self.thruline_home().join("trial.json")
    }

    fn installed(&self) -> PathBuf {
        self.thruline_home().join("licence.txt")
    }

    fn env(&self, cmd: &mut Command, test_public_key: Option<&str>) {
        cmd.env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", self.dir.path().join("h"))
            .env("THRULINE_HOME", self.thruline_home())
            .current_dir(self.dir.path().join("repo"))
            .stdin(Stdio::null());
        if let Some(key) = test_public_key {
            // This seam exists only in a debug build. The key is made in
            // memory for this test and is never a shipped trusted key.
            cmd.env("THRULINE_LICENCE_DEV_PUBLIC_KEY", key);
        }
    }
}

struct Approvable {
    db: PathBuf,
    root: PathBuf,
}

impl Approvable {
    fn new(home: &Home) -> Self {
        let db = home.dir.path().join("approvable.db");
        let root = home.dir.path().join("worktree");
        std::fs::create_dir_all(&root).expect("worktree");
        let ledger = thruline_core::ledger::Ledger::open(&db).expect("ledger");
        let conn = ledger.conn();
        conn.execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES(?1, ?2, 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
            params![PROJECT_ID, root.to_string_lossy()],
        )
        .expect("project");
        conn.execute(
            "INSERT INTO changes(id, project_id, opened_at, title, purpose, request_event_id,
                                 requester, request_source)
             VALUES(?1, ?2, '2026-09-22T08:00:00Z', 'Totals add up', 'They do not.', NULL,
                    'sam', 'person')",
            params![CHANGE_ID, PROJECT_ID],
        )
        .expect("change");
        conn.execute(
            "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at,
                 criteria_json, exclusions_json, allowed_scope_json, constraints_json,
                 check_profile_json, content_sha256)
             VALUES(?1, 1, '2026-09-22T08:30:00Z', '[\"The totals add up\"]', '[]',
                    '[\"checkout/\"]', '[]', '[]', 'deadbeef')",
            params![CHANGE_ID],
        )
        .expect("scope");
        conn.execute(
            "INSERT INTO change_scope_acceptances(change_id, scope_version, accepted_by,
                 accepted_at, rationale)
             VALUES(?1, 1, 'sam', '2026-09-22T09:00:00Z', '')",
            params![CHANGE_ID],
        )
        .expect("acceptance");
        let snapshot = serde_json::json!({
            "id": VERSION, "taken_at": "2026-09-22T10:00:00Z", "root": root.to_string_lossy(),
            "git": null,
            "tree": { "staged": [], "unstaged": [], "untracked": [], "deleted": [],
                      "dirty_diff_sha256": null },
            "environment": { "os": "test", "arch": "test", "git_version": null,
                             "env_names": [], "lockfiles": {} },
            "processes": { "ps_sha256": null, "ps_lines": 0, "ports": null, "task_ids": null,
                           "log_cursors": null, "gaps": "none" },
            "complete": true, "note": null,
        });
        let text = serde_json::to_string(&snapshot).expect("snapshot");
        conn.execute(
            "INSERT INTO workspace_snapshots(id, taken_at, root, head, branch, complete, doc,
                                             doc_sha256)
             VALUES(?1, '2026-09-22T10:00:00Z', ?2, NULL, NULL, 1, ?3, ?4)",
            params![
                VERSION,
                root.to_string_lossy(),
                text,
                thruline_core::chain::sha256_hex(text.as_bytes())
            ],
        )
        .expect("workspace snapshot");
        conn.execute(
            "INSERT INTO change_implementations(change_id, binding_version, bound_at,
                 repository_identity, base_revision, snapshot_id, workspace_identity)
             VALUES(?1, 1, '2026-09-22T10:00:00Z', 'shop', 'base0000', ?2, NULL)",
            params![CHANGE_ID, VERSION],
        )
        .expect("implementation");
        Self { db, root }
    }

    fn approval(&self, server: &Server) -> (u16, String) {
        server.post(
            &format!("/api/changes/{CHANGE_ID}/decisions"),
            &format!(r#"{{"actor":"sam","seen_version":"{VERSION}"}}"#),
        )
    }

    fn decision_rows(&self) -> i64 {
        let conn = rusqlite::Connection::open_with_flags(
            &self.db,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("open ledger read-only");
        conn.query_row("SELECT COUNT(*) FROM change_decisions", [], |row| {
            row.get(0)
        })
        .expect("count decisions")
    }

    fn decision_file(&self) -> PathBuf {
        thruline_core::change_decision::record_path_for(&self.root, VERSION)
    }

    fn digest(&self) -> Vec<u8> {
        Sha256::digest(std::fs::read(&self.db).expect("read ledger")).to_vec()
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
    fn start(home: &Home, db: &Path, test_public_key: Option<&str>) -> Self {
        let mut cmd = Command::new(bin());
        cmd.arg("--db")
            .arg(db)
            .args(["ui", "--no-open", "--port", "0", "--dir"])
            .arg(home.dir.path().join("export"));
        home.env(&mut cmd, test_public_key);
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("thruline ui");
        let out = child.stdout.take().expect("server stdout");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines() {
                let Ok(line) = line else { return };
                if tx.send(line).is_err() {
                    return;
                }
            }
        });
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(line) => {
                    if let Some(address) =
                        line.trim().strip_prefix("ThruLine is at http://127.0.0.1:")
                    {
                        let port = address
                            .split('/')
                            .next()
                            .and_then(|p| p.trim().parse().ok())
                            .expect("server port");
                        return Self { child, port };
                    }
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("thruline ui never printed its address");
                }
            }
        }
    }

    fn post(&self, path: &str, body: &str) -> (u16, String) {
        let mut socket = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        socket
            .set_read_timeout(Some(Duration::from_secs(30)))
            .expect("read timeout");
        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{p}\r\nOrigin: http://127.0.0.1:{p}\r\nContent-Type: application/json\r\nContent-Length: {n}\r\nConnection: close\r\n\r\n{body}",
            p = self.port,
            n = body.len(),
        );
        socket.write_all(request.as_bytes()).expect("request");
        let mut raw = Vec::new();
        socket.read_to_end(&mut raw).expect("response");
        let text = String::from_utf8_lossy(&raw).into_owned();
        let status = text
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        (status, text)
    }
}

fn assert_refused_without_writes(store: &Approvable, server: &Server) -> String {
    let before = (store.decision_rows(), store.digest());
    let (code, response) = store.approval(server);
    assert_eq!(code, 403, "{response}");
    assert!(
        response.contains("\"error\":\"licence_required\""),
        "{response}"
    );
    assert!(response.contains("Pro licence"), "{response}");
    assert_eq!(
        (store.decision_rows(), store.digest()),
        before,
        "refusal changed the ledger"
    );
    assert!(
        !store.decision_file().exists(),
        "refusal wrote a decision file"
    );
    response
}

#[test]
fn approval_without_pro_is_refused_and_creates_no_trial_file() {
    let home = Home::new();
    let store = Approvable::new(&home);
    let server = Server::start(&home, &store.db, None);
    assert!(!home.trial().exists());
    let response = assert_refused_without_writes(&store, &server);
    assert!(
        !response.to_ascii_lowercase().contains("trial"),
        "{response}"
    );
    assert!(
        !home.trial().exists(),
        "a refused approval created trial.json"
    );
}

#[test]
fn a_running_v010_trial_file_is_ignored_and_left_untouched_after_refusal() {
    let home = Home::new();
    let old_bytes =
        br#"{"format":1,"started_on":"2099-01-01","created_at":"2026-10-07T00:00:00Z"}"#;
    std::fs::write(home.trial(), old_bytes).expect("place old trial");
    let store = Approvable::new(&home);
    let server = Server::start(&home, &store.db, None);
    let response = assert_refused_without_writes(&store, &server);
    assert!(
        !response.to_ascii_lowercase().contains("trial"),
        "{response}"
    );
    assert_eq!(std::fs::read(home.trial()).expect("old trial"), old_bytes);
}

#[test]
fn pro_approval_still_records_without_touching_an_old_trial_file() {
    let home = Home::new();
    let old_bytes =
        br#"{"format":1,"started_on":"2099-01-01","created_at":"2026-10-07T00:00:00Z"}"#;
    std::fs::write(home.trial(), old_bytes).expect("place old trial");
    let signing = SigningKey::generate(&mut OsRng);
    let key = thruline_licence::to_hex(&signing.verifying_key().to_bytes());
    let payload = r#"{"format":1,"licence_id":"01J8ZQ4Y6T3M2N1P0R9S8T7V6W","product":"thruline","plan":"pro","issued_at":"2026-09-01T00:00:00Z","expires_at":"2099-01-01T00:00:00Z"}"#;
    let licence = thruline_licence::seal(payload.as_bytes(), &signing).expect("seal");
    std::fs::write(home.installed(), licence).expect("install test licence");
    let store = Approvable::new(&home);
    let server = Server::start(&home, &store.db, Some(&key));
    let (code, response) = store.approval(&server);
    assert_eq!(code, 200, "{response}");
    assert!(response.contains("\"outcome\":\"recorded\""), "{response}");
    assert_eq!(store.decision_rows(), 1);
    assert!(store.decision_file().is_file());
    assert_eq!(std::fs::read(home.trial()).expect("old trial"), old_bytes);
}
