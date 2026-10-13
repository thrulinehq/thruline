//! C30 R-LEDGER — `POST /api/changes/<id>/seconds` and
//! `POST /api/changes/<id>/seconds/withdraw`, through the shipping binary; and
//! `thruline change second`, the terminal door to the same two functions.
//!
//! The module's own tests (`crates/core/tests/change_seconds.rs`) prove what a
//! second is; this proves the served routes keep the contract they are written
//! down with in CLAUDE.md (write-address rows 22 and 23): same-origin, a body
//! cap sized from the name limit, `409 too_long` naming the field, an unknown
//! field refused, the three refusals (`nothing_to_second`, `same_person`,
//! `moved`) and the one for taking back (`no_second_to_withdraw`) each appending
//! nothing, the Q3 sentence served on a refused name, the decision file never
//! rewritten, and both routes free while approving is the one paid surface.
//!
//! A real store under a temporary directory; no real store is opened. The
//! approval is made through the core function, so the server runs as it ships:
//! paid gate on, no licence installed.

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
const CANARY: &str = "ghp_TLCANARYaaZZ11bbYY22ccXX33ddWW44e";
const Q3: &str = "ThruLine records the name you type; it does not check who is at the keyboard.";

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

/// One Change, accepted, bound to a snapshot rooted at `root`, not yet approved.
fn store(db: &Path, root: &Path) {
    std::fs::create_dir_all(root).expect("root");
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
         VALUES(?1, ?2, '2026-09-22T08:00:00Z', 'Totals add up', 'They do not.', NULL,
                'sam', 'person')",
        params![CHANGE, PROJECT],
    )
    .expect("change");
    conn.execute(
        "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at, criteria_json,
                                            exclusions_json, allowed_scope_json, constraints_json,
                                            check_profile_json, content_sha256)
         VALUES(?1, 1, '2026-09-22T08:30:00Z', '[\"The totals add up\"]', '[]', '[\"checkout/\"]',
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
}

/// An approval made through the core function: the paid door is the route's
/// business, and a second is not behind it.
fn approve_as(db: &Path, who: &str) {
    let ledger = Ledger::open(db).expect("open");
    let decide = Decide {
        actor: who.to_string(),
        at: "2026-09-22T11:00:00Z".to_string(),
        seen_revision: VERSION.to_string(),
        note: None,
    };
    change_decision::approve(&ledger, CHANGE, &decide).expect("the approval");
}

fn count(db: &Path, table: &str) -> i64 {
    let ledger = Ledger::open(db).expect("open");
    ledger
        .conn()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .expect("count")
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
    /// The product as it ships: paid gate on, no licence installed.
    fn start(home: &Path, db: &Path) -> Server {
        let mut child = Command::new(bin())
            .env("HOME", home)
            .env("THRULINE_HOME", home.join(".thruline"))
            .env_remove("THRULINE_PAID_GATE_DEV")
            .env_remove("THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS")
            .env_remove("THRULINE_LICENCE_DEV_PUBLIC_KEY")
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

    /// `(status, body)` for one request. `origin` is sent as-is when given.
    fn send(&self, method: &str, path: &str, origin: Option<&str>, body: &str) -> (u16, String) {
        let mut socket = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        socket
            .set_read_timeout(Some(Duration::from_secs(30)))
            .expect("timeout");
        let origin_line = origin
            .map(|o| format!("Origin: {o}\r\n"))
            .unwrap_or_default();
        let request = if method == "POST" {
            format!(
                "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n{origin_line}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                self.port,
                body.len()
            )
        } else {
            format!(
                "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
                self.port
            )
        };
        // A refused oversized body may be cut off mid-write; the answer is
        // already on its way.
        let _ = socket.write_all(request.as_bytes());
        let mut raw = Vec::new();
        let _ = socket.read_to_end(&mut raw);
        let text = String::from_utf8_lossy(&raw).to_string();
        let status: u16 = text
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let body = text
            .split_once("\r\n\r\n")
            .map(|(_, b)| b.to_string())
            .unwrap_or_default();
        (status, body)
    }

    fn origin(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// A POST from HQ's own page.
    fn post(&self, path: &str, body: &serde_json::Value) -> (u16, serde_json::Value) {
        self.post_raw(path, &body.to_string())
    }

    fn post_raw(&self, path: &str, body: &str) -> (u16, serde_json::Value) {
        let (status, text) = self.send("POST", path, Some(&self.origin()), body);
        let json = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    fn change(&self) -> serde_json::Value {
        let (status, text) = self.send("GET", &format!("/api/changes/{CHANGE}"), None, "");
        assert_eq!(status, 200, "{text}");
        let served: serde_json::Value = serde_json::from_str(&text).expect("the Change is JSON");
        assert_eq!(
            served["schema_version"], 8,
            "the route stamps the view's version"
        );
        served["change"].clone()
    }
}

fn seconds_path() -> String {
    format!("/api/changes/{CHANGE}/seconds")
}
fn withdraw_path() -> String {
    format!("/api/changes/{CHANGE}/seconds/withdraw")
}

fn second_body(actor: &str) -> serde_json::Value {
    serde_json::json!({ "actor": actor, "seen_version": VERSION })
}

struct Fixture {
    _tmp: tempfile::TempDir,
    home: std::path::PathBuf,
    db: std::path::PathBuf,
    root: std::path::PathBuf,
}

/// A Change; `approved_by` makes `sam` (or whoever) approve it first.
fn fixture(approved_by: Option<&str>) -> Fixture {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().to_path_buf();
    let db = home.join("ledger.db");
    let root = home.join("project");
    store(&db, &root);
    if let Some(who) = approved_by {
        approve_as(&db, who);
    }
    Fixture {
        _tmp: tmp,
        home,
        db,
        root,
    }
}

/* ── the seconds route ─────────────────────────────────────────────────── */

#[test]
fn a_second_is_recorded_served_masked_and_never_touches_the_decision_file() {
    let f = fixture(Some("sam"));
    let file = change_decision::record_path_for(&f.root, VERSION);
    let file_before = std::fs::read(&file).expect("the decision file");
    let server = Server::start(&f.home, &f.db);

    let (status, answer) = server.post(&seconds_path(), &second_body(&format!("Alex {CANARY}")));
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["schema_version"], 8);
    assert_eq!(answer["acted"]["outcome"], "recorded");
    let second = &answer["acted"]["second"];
    assert!(
        second["id"].as_str().unwrap().starts_with("cds_"),
        "{second}"
    );
    assert_eq!(second["act"], "approved");
    assert_eq!(second["target_revision"], VERSION);
    assert_eq!(second["counting"], true);
    assert!(second["recorded_at"].as_str().unwrap().ends_with('Z'));
    assert!(
        !second["actor"].as_str().unwrap().contains(CANARY),
        "the answer is masked: {second}"
    );
    assert_eq!(answer["decisions"]["standing"]["seconds_standing"], 1);

    // The Change serves it, masked, and the same approval still stands.
    let change = server.change();
    let seconds = change["seconds"].as_array().expect("seconds[]");
    assert_eq!(seconds.len(), 1);
    assert_eq!(seconds[0]["id"], second["id"]);
    assert_eq!(seconds[0]["counting"], true);
    assert!(!seconds[0]["actor"].as_str().unwrap().contains(CANARY));
    assert_eq!(change["decisions"]["standing"]["seconds_standing"], 1);
    assert_eq!(change["decisions"]["standing"]["actor"], "sam");
    assert_eq!(count(&f.db, "change_decisions"), 1);

    // Stored as typed (trimmed); the decision file is byte-identical.
    let stored: String = Ledger::open(&f.db)
        .unwrap()
        .conn()
        .query_row("SELECT actor FROM change_decision_seconds", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(stored, format!("Alex {CANARY}"));
    assert_eq!(std::fs::read(&file).unwrap(), file_before);
}

#[test]
fn the_seconds_routes_need_the_same_origin_and_write_nothing_without_it() {
    let f = fixture(Some("sam"));
    let server = Server::start(&f.home, &f.db);
    let body = second_body("alex").to_string();
    for path in [seconds_path(), withdraw_path()] {
        let (status, text) = server.send("POST", &path, None, &body);
        assert_eq!(status, 403, "{path}: {text}");
        assert!(text.contains("same_origin_required"), "{text}");
        let (status, _) = server.send("POST", &path, Some("http://evil.example"), &body);
        assert_eq!(status, 403, "{path}");
    }
    assert_eq!(count(&f.db, "change_decision_seconds"), 0);
}

#[test]
fn the_seconds_route_refuses_what_it_cannot_read_and_what_is_over_its_limit() {
    let f = fixture(Some("sam"));
    let server = Server::start(&f.home, &f.db);

    // Not the shape: unknown field, not JSON, wrong type, a missing part.
    for bad in [
        r#"{"actor":"alex","seen_version":"v","note":"hi"}"#,
        r#"{"actor":"alex","seen_version":"v","decision_id":"cd_1"}"#,
        "not json",
        r#"{"actor":7,"seen_version":"v"}"#,
        r#"{"actor":"alex"}"#,
        r#"{"seen_version":"v"}"#,
        r#"{}"#,
    ] {
        let (status, answer) = server.post_raw(&seconds_path(), bad);
        assert_eq!(
            (status, answer["error"].as_str()),
            (400, Some("malformed_request")),
            "{bad}"
        );
    }
    // A name with nothing in it.
    let (status, answer) = server.post(&seconds_path(), &second_body("   "));
    assert_eq!(
        (status, answer["error"].as_str()),
        (400, Some("actor_required"))
    );

    // One over the limit: 409 naming the field.
    let (status, answer) = server.post(&seconds_path(), &second_body(&"a".repeat(201)));
    assert_eq!(status, 409, "{answer}");
    assert_eq!(
        (
            answer["error"].as_str(),
            answer["field"].as_str(),
            answer["limit"].as_i64()
        ),
        (Some("too_long"), Some("actor"), Some(200))
    );
    let (status, answer) = server.post(
        &seconds_path(),
        &serde_json::json!({ "actor": "alex", "seen_version": "v".repeat(201) }),
    );
    assert_eq!(
        (status, answer["field"].as_str()),
        (409, Some("seen_version")),
        "{answer}"
    );

    // A body larger than a name within the limit could make: 413.
    let (status, _) = server.post(
        &seconds_path(),
        &serde_json::json!({ "actor": "x".repeat(40_000), "seen_version": VERSION }),
    );
    assert_eq!(
        status, 413,
        "refused before the body is read; the connection may be cut"
    );

    // Not a Change.
    let (status, answer) = server.post("/api/changes/no-such-change/seconds", &second_body("alex"));
    assert_eq!(
        (status, answer["error"].as_str()),
        (404, Some("no_such_change"))
    );
    let (status, answer) = server.post(
        "/api/changes/no-such-change/seconds/withdraw",
        &serde_json::json!({ "actor": "alex" }),
    );
    assert_eq!(
        (status, answer["error"].as_str()),
        (404, Some("no_such_change"))
    );
    // The address with no Change in it does not reach the handler.
    let (status, _) = server.post("/api/changes/seconds", &second_body("alex"));
    assert_eq!(status, 404);

    assert_eq!(
        count(&f.db, "change_decision_seconds"),
        0,
        "every refusal wrote nothing"
    );
}

#[test]
fn a_name_in_the_widest_characters_still_fits_the_cap() {
    let f = fixture(Some("sam"));
    let server = Server::start(&f.home, &f.db);
    // 200 characters, each written the longest way JSON can write one: a
    // surrogate pair, twelve bytes. The body cap is sized for exactly this.
    let escaped = r"😀".repeat(200);
    let body = format!(r#"{{"actor":"{escaped}","seen_version":"{VERSION}"}}"#);
    assert!(
        body.len() > 512,
        "this is bigger than the old action-route cap"
    );
    let (status, answer) = server.post_raw(&seconds_path(), &body);
    assert_eq!(status, 200, "{answer}");
    let kept: String = Ledger::open(&f.db)
        .unwrap()
        .conn()
        .query_row("SELECT actor FROM change_decision_seconds", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(kept.chars().count(), 200);
}

/* ── the three refusals ────────────────────────────────────────────────── */

#[test]
fn nothing_to_second_when_no_approval_stands() {
    let f = fixture(None);
    let server = Server::start(&f.home, &f.db);
    let (status, answer) = server.post(&seconds_path(), &second_body("alex"));
    assert_eq!(status, 409, "{answer}");
    assert_eq!(answer["error"], "nothing_to_second");
    assert_eq!(
        answer["message"].as_str(),
        Some(thruline_core::change_decision_second::NOTHING_TO_SECOND)
    );
    assert_eq!(count(&f.db, "change_decision_seconds"), 0);
}

#[test]
fn the_same_person_is_refused_in_a_served_sentence_that_says_the_name_is_not_checked() {
    let f = fixture(Some("Sam"));
    let server = Server::start(&f.home, &f.db);
    // The approval's own actor, typed any way.
    for same in ["sam", "  SAM  ", "sAm"] {
        let (status, answer) = server.post(&seconds_path(), &second_body(same));
        assert_eq!(status, 409, "{same:?}: {answer}");
        assert_eq!(answer["error"], "same_person");
        assert_eq!(answer["with"], "Sam");
        let message = answer["message"].as_str().expect("a served sentence");
        assert!(message.contains("Sam"), "{message}");
        assert!(
            message.ends_with(Q3),
            "the ruling's sentence, whole, last: {message}"
        );
    }
    // A person who has already seconded.
    let (status, _) = server.post(&seconds_path(), &second_body("Alex"));
    assert_eq!(status, 200);
    let (status, answer) = server.post(&seconds_path(), &second_body(" alex "));
    assert_eq!(status, 409, "{answer}");
    assert_eq!(answer["with"], "Alex");
    assert!(answer["message"].as_str().unwrap().ends_with(Q3));
    assert_eq!(count(&f.db, "change_decision_seconds"), 1);
}

#[test]
fn another_version_than_the_approvals_is_a_200_moved_and_writes_nothing() {
    let f = fixture(Some("sam"));
    let server = Server::start(&f.home, &f.db);
    let (status, answer) = server.post(
        &seconds_path(),
        &serde_json::json!({ "actor": "alex", "seen_version": "snap_other" }),
    );
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["acted"]["outcome"], "moved");
    assert_eq!(answer["acted"]["seen"], "snap_other");
    assert_eq!(answer["acted"]["now"], VERSION);
    assert!(answer["acted"]["changed"].is_array(), "{answer}");
    assert!(
        answer["acted"]["message"]
            .as_str()
            .unwrap()
            .contains("nothing was recorded"),
        "{answer}"
    );
    assert_eq!(count(&f.db, "change_decision_seconds"), 0);
}

#[test]
fn a_moved_second_reply_hides_excluded_changed_paths_and_restores_on_include() {
    let f = fixture(Some("sam"));
    let ledger = Ledger::open(&f.db).expect("synthetic ledger");
    let original: String = ledger
        .conn()
        .query_row(
            "SELECT doc FROM workspace_snapshots WHERE id=?1",
            [VERSION],
            |row| row.get(0),
        )
        .expect("standing snapshot");
    let mut older: serde_json::Value = serde_json::from_str(&original).expect("snapshot JSON");
    older["id"] = serde_json::json!("snap_other");
    older["tree"]["deleted"] = serde_json::json!(["src/private.rs"]);
    let text = older.to_string();
    let digest = thruline_core::chain::sha256_hex(text.as_bytes());
    ledger
        .conn()
        .execute(
            "INSERT INTO workspace_snapshots(id,taken_at,root,head,branch,complete,doc,doc_sha256)
             VALUES('snap_other','2026-09-22T09:30:00Z',?1,NULL,NULL,1,?2,?3)",
            params![f.root.to_string_lossy(), text, digest],
        )
        .expect("older synthetic snapshot");
    let server = Server::start(&f.home, &f.db);
    let moved = serde_json::json!({"actor":"alex","seen_version":"snap_other"});
    let (status, plain) = server.post(&seconds_path(), &moved);
    assert_eq!(status, 200, "{plain}");
    assert_eq!(plain["acted"]["changed"][0], "src/private.rs");

    let folder = f.root.to_str().expect("UTF-8 synthetic root");
    let mut hold = Hold {
        project_id: PROJECT,
        project_folder: folder,
        actor: "synthetic person",
        recorded_at: "2026-10-08T00:00:01Z",
    };
    capture_hold::record_for_person(&ledger, Act::Excluded, &hold, Some("src/private.rs"))
        .expect("exclude private path");
    let (status, hidden) = server.post(&seconds_path(), &moved);
    assert_eq!(status, 200, "{hidden}");
    assert_eq!(
        hidden["acted"]["changed"][0],
        thruline_core::capture_exclusion::marker("src/private.rs")
    );
    assert!(!hidden.to_string().contains("src/private.rs\""));

    hold.recorded_at = "2026-10-08T00:00:02Z";
    capture_hold::record_for_person(&ledger, Act::Included, &hold, Some("src/private.rs"))
        .expect("include private path");
    let (status, restored) = server.post(&seconds_path(), &moved);
    assert_eq!(status, 200, "{restored}");
    assert_eq!(restored["acted"]["changed"][0], "src/private.rs");
    assert_eq!(count(&f.db, "change_decision_seconds"), 0);
}

/* ── taking a second back ──────────────────────────────────────────────── */

#[test]
fn a_person_takes_back_their_own_second_and_only_that() {
    let f = fixture(Some("sam"));
    let server = Server::start(&f.home, &f.db);
    assert_eq!(server.post(&seconds_path(), &second_body("alex")).0, 200);
    assert_eq!(server.post(&seconds_path(), &second_body("kim")).0, 200);

    // Someone with no second of theirs, and the approval's own actor.
    for stranger in ["pat", "sam"] {
        let (status, answer) =
            server.post(&withdraw_path(), &serde_json::json!({ "actor": stranger }));
        assert_eq!(status, 409, "{stranger}: {answer}");
        assert_eq!(answer["error"], "no_second_to_withdraw");
        assert!(
            answer["message"]
                .as_str()
                .unwrap()
                .contains("Nothing was saved"),
            "{answer}"
        );
    }
    assert_eq!(count(&f.db, "change_decision_seconds"), 2);

    let (status, answer) = server.post(&withdraw_path(), &serde_json::json!({ "actor": " ALEX " }));
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["acted"]["outcome"], "recorded");
    assert_eq!(answer["acted"]["withdrawal"]["act"], "withdrawn");
    assert_eq!(answer["decisions"]["standing"]["seconds_standing"], 1);
    let change = server.change();
    let counting: Vec<(String, bool)> = change["seconds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["actor"].as_str().unwrap().to_string(),
                s["counting"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        counting,
        [
            ("alex".to_string(), false),
            ("kim".to_string(), true),
            ("ALEX".to_string(), false)
        ]
    );
    // Nothing left to take back.
    let (status, _) = server.post(&withdraw_path(), &serde_json::json!({ "actor": "alex" }));
    assert_eq!(status, 409);
    assert_eq!(count(&f.db, "change_decision_seconds"), 3);

    // The withdraw route's own shape.
    for bad in [
        r#"{"actor":"alex","seen_version":"v"}"#,
        r#"{}"#,
        "not json",
    ] {
        let (status, answer) = server.post_raw(&withdraw_path(), bad);
        assert_eq!(
            (status, answer["error"].as_str()),
            (400, Some("malformed_request")),
            "{bad}"
        );
    }
    let (status, answer) = server.post(&withdraw_path(), &serde_json::json!({ "actor": "  " }));
    assert_eq!(
        (status, answer["error"].as_str()),
        (400, Some("actor_required"))
    );
    let (status, answer) = server.post(
        &withdraw_path(),
        &serde_json::json!({ "actor": "a".repeat(201) }),
    );
    assert_eq!((status, answer["field"].as_str()), (409, Some("actor")));
}

/* ── free, and only POST ───────────────────────────────────────────────── */

#[test]
fn both_routes_are_free_while_approving_still_needs_a_licence() {
    let f = fixture(Some("sam"));
    // The server runs as it ships: paid gate on, no licence.
    let server = Server::start(&f.home, &f.db);
    assert_eq!(server.post(&seconds_path(), &second_body("alex")).0, 200);
    assert_eq!(
        server
            .post(&withdraw_path(), &serde_json::json!({ "actor": "alex" }))
            .0,
        200
    );
    // Approving, on the same server, is the paid act.
    let (status, answer) = server.post(
        &format!("/api/changes/{CHANGE}/decisions"),
        &serde_json::json!({ "actor": "kim", "seen_version": VERSION }),
    );
    assert_eq!(status, 403, "{answer}");
    assert_eq!(answer["error"], "licence_required");
    assert_eq!(count(&f.db, "change_decision_seconds"), 2);
    assert_eq!(count(&f.db, "change_decisions"), 1);
}

#[test]
fn only_post_reaches_the_seconds_addresses() {
    let f = fixture(Some("sam"));
    let server = Server::start(&f.home, &f.db);
    for path in [seconds_path(), withdraw_path()] {
        for method in ["GET", "PUT", "DELETE", "PATCH"] {
            let (status, _) = server.send(
                method,
                &path,
                Some(&server.origin()),
                r#"{"actor":"alex","seen_version":"v"}"#,
            );
            assert_ne!(status, 200, "{method} {path} must not record a second");
        }
    }
    assert_eq!(count(&f.db, "change_decision_seconds"), 0);
}

/* ── the terminal door ─────────────────────────────────────────────────── */

fn cli(f: &Fixture, args: &[&str]) -> std::process::Output {
    Command::new(bin())
        .env("HOME", &f.home)
        .env("THRULINE_HOME", f.home.join(".thruline"))
        .env_remove("THRULINE_PAID_GATE_DEV")
        .arg("--db")
        .arg(&f.db)
        .args(["change", "second"])
        .args(args)
        .output()
        .expect("thruline change second")
}

#[test]
fn the_terminal_door_keeps_the_same_row_and_refuses_in_the_same_words() {
    let f = fixture(None);
    // Nothing is approved yet.
    let refused = cli(&f, &[CHANGE, "--actor", "alex"]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&refused.stdout)
        .contains(thruline_core::change_decision_second::NOTHING_TO_SECOND),);

    approve_as(&f.db, "sam");
    let kept = cli(&f, &[CHANGE, "--actor", "alex"]);
    assert!(
        kept.status.success(),
        "{}",
        String::from_utf8_lossy(&kept.stdout)
    );
    let said = String::from_utf8_lossy(&kept.stdout);
    assert!(said.contains(Q3), "{said}");
    let row: (String, String, String, String, Option<String>) = Ledger::open(&f.db)
        .unwrap()
        .conn()
        .query_row(
            "SELECT actor, act, target_revision, change_id, withdraws FROM change_decision_seconds",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    assert_eq!(
        row,
        (
            "alex".to_string(),
            "approved".to_string(),
            VERSION.to_string(),
            CHANGE.to_string(),
            None
        )
    );

    // The same person, the same sentence the route serves.
    let again = cli(&f, &[CHANGE, "--actor", " ALEX "]);
    assert_eq!(again.status.code(), Some(1));
    let said = String::from_utf8_lossy(&again.stdout);
    assert!(said.trim_end().ends_with(Q3), "{said}");
    let approver = cli(&f, &[CHANGE, "--actor", "SAM"]);
    assert_eq!(approver.status.code(), Some(1));

    // Another version than the approval's is "moved", and writes nothing.
    let moved = cli(
        &f,
        &[CHANGE, "--actor", "kim", "--seen-version", "snap_other"],
    );
    assert_eq!(moved.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&moved.stdout).contains("nothing was recorded"));

    // Not a Change.
    let none = cli(&f, &["no-such-change", "--actor", "kim"]);
    assert_eq!(none.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&none.stdout).contains("Nothing was saved"));
    assert_eq!(count(&f.db, "change_decision_seconds"), 1);

    // Taking it back.
    let nothing = cli(&f, &[CHANGE, "--actor", "pat", "--withdraw"]);
    assert_eq!(nothing.status.code(), Some(1));
    let back = cli(&f, &[CHANGE, "--actor", "alex", "--withdraw"]);
    assert!(
        back.status.success(),
        "{}",
        String::from_utf8_lossy(&back.stdout)
    );
    assert_eq!(count(&f.db, "change_decision_seconds"), 2);
    let again = cli(&f, &[CHANGE, "--actor", "alex", "--withdraw"]);
    assert_eq!(again.status.code(), Some(1));
    assert_eq!(count(&f.db, "change_decision_seconds"), 2);
}
