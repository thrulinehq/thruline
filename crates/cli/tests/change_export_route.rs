//! T-5-4 (`B10.16`, `Q18`) — `GET /api/changes/<id>/export`, through the
//! shipping binary.
//!
//! The module's own tests (`crates/core/tests/change_export.rs`) prove what it
//! answers; this proves the served route answers it: that the arm is reached
//! before the one-Change arm would swallow the suffix, that a record made
//! unreadable on disk is reported as such by the server a browser talks to,
//! and that the address takes no query, no other method, and no unknown id.
//!
//! A real store under a temporary directory; no real store is opened.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use rusqlite::params;
use thruline_core::capture_hold::{self, Act, Hold};
use thruline_core::change_decision::{self, Decide};
use thruline_core::ledger::Ledger;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000p001";
const CHANGE: &str = "cccccccc-0000-4000-8000-00000000c001";
const VERSION: &str = "snap_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

/// One Change, accepted, bound to a snapshot rooted at `root`, and approved —
/// so `.thruline/decisions/<version>.json` exists under `root`.
fn store(db: &Path, root: &Path) -> std::path::PathBuf {
    std::fs::create_dir_all(root).expect("root");
    std::fs::create_dir_all(root.join(".git")).expect("synthetic repository");
    let ledger = Ledger::open(db).expect("open");
    let conn = ledger.conn();
    conn.execute(
        "INSERT INTO projects(id, root_path, source, display_name, created_at)
         VALUES(?1, ?2, 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
        params![PROJECT, root.to_string_lossy()],
    )
    .expect("project");
    conn.execute(
        "INSERT INTO changes(id, project_id, opened_at, title, purpose, request_event_id,
                             requester, request_source)
         VALUES(?1, ?2, '2026-09-22T08:00:00Z', 'Totals add up', 'Review src/a.rs', NULL,
                'sam', 'person')",
        params![CHANGE, PROJECT],
    )
    .expect("change");
    conn.execute(
        "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at, criteria_json,
                                            exclusions_json, allowed_scope_json, constraints_json,
                                            check_profile_json, content_sha256)
         VALUES(?1, 1, '2026-09-22T08:30:00Z', '[\"Read src/a.rs\"]', '[]', '[\"checkout/\"]',
                '[]', '[]', 'deadbeef')",
        params![CHANGE],
    )
    .expect("scope");
    conn.execute(
        "INSERT INTO change_scope_acceptances(change_id, scope_version, accepted_by, accepted_at,
                                              rationale)
         VALUES(?1, 1, 'sam', '2026-09-22T09:00:00Z', '')",
        params![CHANGE],
    )
    .expect("acceptance");
    let doc = serde_json::json!({
        "id": VERSION, "taken_at": "2026-09-22T10:00:00Z", "root": root.to_string_lossy(),
        "git": null,
        "tree": { "staged": [], "unstaged": [], "untracked": [], "deleted": [], "dirty_diff_sha256": null },
        "environment": { "os": "test", "arch": "test", "git_version": null, "env_names": [], "lockfiles": {} },
        "processes": { "ps_sha256": null, "ps_lines": 0, "ports": null, "task_ids": null,
                       "log_cursors": null, "gaps": "none" },
        "complete": true, "note": null,
    });
    let text = serde_json::to_string(&doc).expect("doc");
    conn.execute(
        "INSERT INTO workspace_snapshots(id, taken_at, root, head, branch, complete, doc, doc_sha256)
         VALUES(?1, '2026-09-22T10:00:00Z', ?2, NULL, NULL, 1, ?3, ?4)",
        params![
            VERSION,
            root.to_string_lossy(),
            text,
            thruline_core::chain::sha256_hex(text.as_bytes())
        ],
    )
    .expect("snapshot");
    conn.execute(
        "INSERT INTO change_implementations(change_id, binding_version, bound_at,
                                            repository_identity, base_revision, snapshot_id,
                                            workspace_identity)
         VALUES(?1, 1, '2026-09-22T10:00:00Z', 'shop', 'base0000', ?2, NULL)",
        params![CHANGE, VERSION],
    )
    .expect("implementation");
    change_decision::approve(
        &ledger,
        CHANGE,
        &Decide {
            actor: "sam".into(),
            at: "2026-09-22T11:00:00Z".into(),
            seen_revision: VERSION.into(),
            note: None,
        },
    )
    .expect("approve");
    let file = change_decision::record_path_for(root, VERSION);
    assert!(file.is_file());
    file
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

    /// `(status, body)` for one request.
    fn send(&self, method: &str, path: &str) -> (u16, String) {
        let mut socket = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
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
        (status, body)
    }
}

#[test]
fn exclusions_cover_the_change_read_and_its_export_records() {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".thruline")).expect("home");
    let db = dir.path().join("ledger.db");
    let root = dir.path().join("worktree");
    store(&db, &root);
    let ledger = Ledger::open(&db).expect("ledger");
    let hold = Hold {
        project_id: PROJECT,
        project_folder: root.to_str().expect("root path"),
        actor: "synthetic person",
        recorded_at: "2026-09-22T12:00:00Z",
    };
    capture_hold::record_for_person(&ledger, Act::Excluded, &hold, Some(".thruline/decisions"))
        .expect("exclude decision record paths");
    let later_hold = Hold {
        recorded_at: "2026-09-22T12:00:01Z",
        ..hold
    };
    capture_hold::record_for_person(&ledger, Act::Excluded, &later_hold, Some("src/a.rs"))
        .expect("exclude a requested file");
    let server = Server::start(&home, &db);
    let (status, index) = server.send("GET", "/api/changes");
    assert_eq!(status, 200, "{index}");
    assert!(index.contains("left out by your exclusion (`src/a.rs`)"));
    assert!(!index.contains("Review src/a.rs"));
    let (status, proposals) = server.send("GET", &format!("/api/changes/{CHANGE}/proposals"));
    assert_eq!(status, 200, "{proposals}");
    assert!(
        proposals.contains("left out by your exclusion (`src/a.rs`)"),
        "{proposals}"
    );
    assert!(!proposals.contains("Read src/a.rs"), "{proposals}");
    for path in [
        format!("/api/changes/{CHANGE}"),
        format!("/api/changes/{CHANGE}/export"),
    ] {
        let (status, body) = server.send("GET", &path);
        assert_eq!(status, 200, "{path}: {body}");
        let doc: serde_json::Value = serde_json::from_str(&body).expect("JSON");
        assert!(
            doc["left_out_by_exclusion"].as_u64().is_some_and(|n| n > 0),
            "{path}: {body}"
        );
        assert!(
            body.contains("left out by your exclusion (`.thruline/decisions`)"),
            "{path}: {body}"
        );
        assert!(!body.contains("/.thruline/decisions/"), "{path}: {body}");
        assert!(!body.contains("Review src/a.rs"), "{path}: {body}");
        if path.ends_with("/export") {
            assert_eq!(doc["decisions"][0]["standing"], "readable", "{body}");
        } else {
            assert_eq!(
                doc["change"]["decisions"]["history"][0]["act"], "approved",
                "{body}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn the_served_route_reports_whether_each_record_can_be_read_now() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".thruline")).expect("home");
    let db = dir.path().join("ledger.db");
    let file = store(&db, &dir.path().join("worktree"));
    let server = Server::start(&home, &db);
    let path = format!("/api/changes/{CHANGE}/export");

    let (status, body) = server.send("GET", &path);
    assert_eq!(status, 200, "{body}");
    let doc: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(doc["schema_version"], 1);
    assert_eq!(doc["change_id"], CHANGE);
    assert_eq!(doc["decisions"][0]["standing"], "readable");
    assert_eq!(doc["totals"]["not_readable"], 0);
    assert!(
        doc["masking"].is_object(),
        "the treatment is not served beside the paths"
    );

    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    let locked = std::fs::File::open(&file).is_err();
    let (status, body) = server.send("GET", &path);
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).expect("chmod back");
    assert_eq!(status, 200);
    if locked {
        let doc: serde_json::Value = serde_json::from_str(&body).expect("json");
        assert_eq!(
            doc["decisions"][0]["standing"], "permission_denied",
            "{body}"
        );
        assert_eq!(doc["totals"]["not_readable"], 1);
    }

    std::fs::remove_file(&file).expect("remove");
    let (_, body) = server.send("GET", &path);
    let doc: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(doc["decisions"][0]["standing"], "missing", "{body}");
}

#[test]
fn the_address_takes_no_query_no_other_method_and_no_unknown_change() {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".thruline")).expect("home");
    let db = dir.path().join("ledger.db");
    store(&db, &dir.path().join("worktree"));
    let server = Server::start(&home, &db);

    let (status, body) = server.send("GET", "/api/changes/nope/export");
    assert_eq!(status, 404, "{body}");
    assert!(body.contains("no_such_change"));

    let (status, body) = server.send("GET", &format!("/api/changes/{CHANGE}/export?x=1"));
    assert_eq!(status, 400, "{body}");

    /* There is no write under this path: the file is made in the browser. */
    let (status, _) = server.send("POST", &format!("/api/changes/{CHANGE}/export"));
    assert!(
        status == 405 || status == 403,
        "a POST to the export address was answered {status}"
    );
    let rows_before: i64 = Ledger::open(&db)
        .expect("open")
        .conn()
        .query_row("SELECT COUNT(*) FROM change_decisions", [], |r| r.get(0))
        .expect("count");
    let _ = server.send("GET", &format!("/api/changes/{CHANGE}/export"));
    let rows_after: i64 = Ledger::open(&db)
        .expect("open")
        .conn()
        .query_row("SELECT COUNT(*) FROM change_decisions", [], |r| r.get(0))
        .expect("count");
    assert_eq!(rows_before, rows_after);
}

/* ── DECISION-FILE-AFTER-WITHDRAW, through the served withdraw route ────── */

/// `(status, body)` for one JSON POST, with or without an `Origin` header.
fn post_json(server: &Server, path: &str, body: &str, origin: bool) -> (u16, String) {
    let mut socket = TcpStream::connect(("127.0.0.1", server.port)).expect("connect");
    let origin = if origin {
        format!("Origin: http://127.0.0.1:{}\r\n", server.port)
    } else {
        String::new()
    };
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n{origin}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        server.port,
        body.len()
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
    (status, body)
}

/// The withdraw route replaces the decision copy under the recorded worktree
/// with a derived copy of the withdrawal row it just appended; the export
/// served next reads the approval's file as no longer that approval. A POST
/// without `Origin` is still refused and touches neither the ledger nor the
/// file.
#[test]
fn the_withdraw_route_replaces_the_copy_and_the_export_agrees() {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".thruline")).expect("home");
    let db = dir.path().join("ledger.db");
    let file = store(&db, &dir.path().join("worktree"));
    let approved_copy = std::fs::read(&file).expect("the approval copy");
    let server = Server::start(&home, &db);
    let withdraw = format!("/api/changes/{CHANGE}/decisions/withdraw");
    let body = format!("{{\"actor\":\"sam\",\"seen_version\":\"{VERSION}\"}}");

    let (status, refused) = post_json(&server, &withdraw, &body, false);
    assert_eq!(status, 403, "{refused}");
    assert_eq!(
        std::fs::read(&file).expect("copy"),
        approved_copy,
        "a refused request leaves the copy as it was"
    );

    let (status, acted) = post_json(&server, &withdraw, &body, true);
    assert_eq!(status, 200, "{acted}");
    let acted: serde_json::Value = serde_json::from_str(&acted).expect("json");
    assert_eq!(acted["acted"]["outcome"], "recorded", "{acted}");
    let row = &acted["acted"]["decision"];
    assert_eq!(row["act"], "withdrawn");

    let copy: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&file).expect("copy")).expect("json");
    assert_eq!(copy["act"], "withdrawn", "{copy}");
    assert_eq!(copy["decision_id"], row["id"], "{copy}");
    assert_eq!(copy["withdraws"], row["withdraws"], "{copy}");
    assert_eq!(copy["decided_at"], row["decided_at"], "{copy}");
    assert_eq!(copy["derived_copy"], true, "{copy}");

    let (_, export) = server.send("GET", &format!("/api/changes/{CHANGE}/export"));
    let export: serde_json::Value = serde_json::from_str(&export).expect("json");
    assert_eq!(
        export["decisions"][0]["standing"], "content_changed",
        "{export}"
    );
    assert_eq!(export["decisions"][1]["act"], "withdrawn", "{export}");
    assert_eq!(
        export["decisions"][1]["standing"],
        serde_json::Value::Null,
        "{export}"
    );
}
