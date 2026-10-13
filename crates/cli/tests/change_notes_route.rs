//! C30 R-LEDGER — `POST /api/changes/<id>/notes`, and the `note` that rides on
//! `POST …/decisions` and `…/decisions/withdraw`, through the shipping binary;
//! and `thruline change note`, the terminal door to the same function.
//!
//! The module's own tests (`crates/core/tests/change_notes.rs`) prove what a
//! note is; this proves the served routes keep the contract they are written
//! down with in CLAUDE.md (write-address row 21 and the row-1 correction):
//! same-origin, a body cap sized from the limit, `409 too_long` naming the
//! field, an unknown field refused, the note kept with its approval and not in
//! the decision file, and the notes route free while approving is the one paid
//! surface.
//!
//! A real store under a temporary directory; no real store is opened.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use rusqlite::params;
use thruline_core::capture_hold::{self, Act, Hold};
use thruline_core::ledger::Ledger;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000p001";
const CHANGE: &str = "cccccccc-0000-4000-8000-00000000c001";
const VERSION: &str = "snap_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const CANARY: &str = "ghp_TLCANARYaaZZ11bbYY22ccXX33ddWW44e";

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
    /// `paid_gate_off` uses the debug seam so an approval can be recorded; the
    /// other form is the product as it ships, with no licence installed.
    fn start(home: &Path, db: &Path, paid_gate_off: bool) -> Server {
        let mut command = Command::new(bin());
        command
            .env("HOME", home)
            .env("THRULINE_HOME", home.join(".thruline"))
            .env_remove("THRULINE_PAID_GATE_DEV")
            .env_remove("THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS")
            .env_remove("THRULINE_LICENCE_DEV_PUBLIC_KEY")
            .current_dir(home);
        if paid_gate_off {
            command.env("THRULINE_PAID_GATE_DEV", "off");
        }
        let mut child = command
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
    fn post(&self, path: &str, body: &str) -> (u16, serde_json::Value) {
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

fn notes_path() -> String {
    format!("/api/changes/{CHANGE}/notes")
}
fn decide_path() -> String {
    format!("/api/changes/{CHANGE}/decisions")
}
fn withdraw_path() -> String {
    format!("/api/changes/{CHANGE}/decisions/withdraw")
}

struct Fixture {
    _tmp: tempfile::TempDir,
    home: std::path::PathBuf,
    db: std::path::PathBuf,
    root: std::path::PathBuf,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().to_path_buf();
    let db = home.join("ledger.db");
    let root = home.join("project");
    store(&db, &root);
    Fixture {
        _tmp: tmp,
        home,
        db,
        root,
    }
}

/* ── the notes route ───────────────────────────────────────────────────── */

#[test]
fn a_note_reply_obeys_exclude_then_include_without_changing_the_kept_note() {
    let f = fixture();
    let server = Server::start(&f.home, &f.db, false);
    let body = r#"{"body":"Review src/private.rs"}"#;
    let (status, plain) = server.post(&notes_path(), body);
    assert_eq!(status, 200, "{plain}");
    assert_eq!(plain["note"]["body"], "Review src/private.rs");

    let folder = f.root.to_str().expect("UTF-8 synthetic root");
    let ledger = Ledger::open(&f.db).expect("synthetic ledger");
    let mut hold = Hold {
        project_id: PROJECT,
        project_folder: folder,
        actor: "synthetic person",
        recorded_at: "2026-10-08T00:00:01Z",
    };
    capture_hold::record_for_person(&ledger, Act::Excluded, &hold, Some("src/private.rs"))
        .expect("exclude private path");
    let (status, hidden) = server.post(&notes_path(), body);
    assert_eq!(status, 200, "{hidden}");
    assert_eq!(
        hidden["note"]["body"],
        thruline_core::capture_exclusion::marker("src/private.rs")
    );
    assert_eq!(hidden["left_out_by_exclusion"], 1);
    assert!(!hidden.to_string().contains("Review src/private.rs"));
    let kept: String = ledger
        .conn()
        .query_row(
            "SELECT body FROM change_notes ORDER BY rowid DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("kept note");
    assert_eq!(kept, "Review src/private.rs");

    hold.recorded_at = "2026-10-08T00:00:02Z";
    capture_hold::record_for_person(&ledger, Act::Included, &hold, Some("src/private.rs"))
        .expect("include private path");
    let (status, restored) = server.post(&notes_path(), body);
    assert_eq!(status, 200, "{restored}");
    assert_eq!(restored["note"]["body"], "Review src/private.rs");
    assert!(restored.get("left_out_by_exclusion").is_none());
}

#[test]
fn a_note_is_kept_served_with_who_and_when_and_masked() {
    let f = fixture();
    let server = Server::start(&f.home, &f.db, false);
    let words = format!("Checked it by hand. {CANARY}");
    let (status, answer) = server.post(
        &notes_path(),
        &serde_json::json!({ "body": words }).to_string(),
    );
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["schema_version"], 8);
    let note = &answer["note"];
    assert!(note["id"].as_str().unwrap().starts_with("cn_"), "{note}");
    assert!(
        note["decision_id"].is_null(),
        "a Brief note names no decision: {note}"
    );
    assert!(!note["actor"].as_str().unwrap().trim().is_empty());
    assert!(note["recorded_at"].as_str().unwrap().ends_with('Z'));
    assert!(
        !note["body"].as_str().unwrap().contains(CANARY),
        "the answer is masked: {note}"
    );

    // The Change serves it, masked, under notes[] at schema 7.
    let change = server.change();
    assert_eq!(change["schema_version"], 8);
    let notes = change["notes"].as_array().expect("notes[]");
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0]["id"], note["id"]);
    assert!(!notes[0]["body"].as_str().unwrap().contains(CANARY));
    // Stored as typed.
    let stored: String = Ledger::open(&f.db)
        .unwrap()
        .conn()
        .query_row("SELECT body FROM change_notes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(stored, words);
}

#[test]
fn the_notes_route_needs_the_same_origin_and_writes_nothing_without_it() {
    let f = fixture();
    let server = Server::start(&f.home, &f.db, false);
    let body = r#"{"body":"hello"}"#;
    let (status, text) = server.send("POST", &notes_path(), None, body);
    assert_eq!(status, 403, "{text}");
    assert!(text.contains("same_origin_required"), "{text}");
    let (status, _) = server.send("POST", &notes_path(), Some("http://evil.example"), body);
    assert_eq!(status, 403);
    assert_eq!(count(&f.db, "change_notes"), 0);
}

#[test]
fn the_notes_route_refuses_what_it_cannot_read_and_what_is_over_its_limit() {
    let f = fixture();
    let server = Server::start(&f.home, &f.db, false);

    // Not the shape: unknown field, not JSON, wrong type.
    for bad in [
        r#"{"body":"hi","actor":"someone else"}"#,
        r#"{"body":"hi","decision_id":"cd_1"}"#,
        "not json",
        r#"{"body":7}"#,
        r#"{}"#,
    ] {
        let (status, answer) = server.post(&notes_path(), bad);
        assert_eq!(
            (status, answer["error"].as_str()),
            (400, Some("malformed_request")),
            "{bad}"
        );
    }
    // Nothing but whitespace.
    let (status, answer) = server.post(&notes_path(), r#"{"body":"  \n "}"#);
    assert_eq!(
        (status, answer["error"].as_str()),
        (400, Some("body_required"))
    );

    // One over the limit: 409 naming the field.
    let over = "x".repeat(2001);
    let (status, answer) = server.post(
        &notes_path(),
        &serde_json::json!({ "body": over }).to_string(),
    );
    assert_eq!(status, 409, "{answer}");
    assert_eq!(answer["error"], "too_long");
    assert_eq!(answer["field"], "body");
    assert_eq!(answer["limit"], 2000);

    // A body larger than any note within the limit could make: 413.
    let huge = "x".repeat(40_000);
    let (status, _) = server.post(
        &notes_path(),
        &serde_json::json!({ "body": huge }).to_string(),
    );
    assert_eq!(
        status, 413,
        "refused before the body is read; the connection may be cut"
    );

    // Not a Change.
    let (status, answer) = server.post("/api/changes/no-such-change/notes", r#"{"body":"hi"}"#);
    assert_eq!(
        (status, answer["error"].as_str()),
        (404, Some("no_such_change"))
    );

    assert_eq!(
        count(&f.db, "change_notes"),
        0,
        "every refusal wrote nothing"
    );
}

#[test]
fn a_note_of_exactly_the_limit_in_the_widest_characters_still_fits_the_cap() {
    let f = fixture();
    let server = Server::start(&f.home, &f.db, false);
    // 2,000 characters, each written the longest way JSON can write one: a
    // surrogate pair, twelve bytes. The body cap is sized for exactly this.
    let escaped = r"😀".repeat(2000);
    let body = format!(r#"{{"body":"{escaped}"}}"#);
    assert!(
        body.len() > 512,
        "this is bigger than the old action-route cap"
    );
    let (status, answer) = server.post(&notes_path(), &body);
    assert_eq!(status, 200, "{answer}");
    let kept: String = Ledger::open(&f.db)
        .unwrap()
        .conn()
        .query_row("SELECT body FROM change_notes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(kept.chars().count(), 2000);
}

#[test]
fn only_post_reaches_the_notes_address() {
    let f = fixture();
    let server = Server::start(&f.home, &f.db, false);
    for method in ["GET", "PUT", "DELETE", "PATCH"] {
        let (status, _) = server.send(
            method,
            &notes_path(),
            Some(&server.origin()),
            r#"{"body":"hi"}"#,
        );
        assert_ne!(status, 200, "{method} must not keep a note");
    }
    assert_eq!(count(&f.db, "change_notes"), 0);
}

/* ── the note that rides on the decision routes ────────────────────────── */

#[test]
fn an_approval_and_a_withdrawal_keep_their_notes_and_the_file_does_not() {
    let f = fixture();
    let server = Server::start(&f.home, &f.db, true);
    let approve = serde_json::json!({
        "actor": "sam", "seen_version": VERSION,
        "note": format!("Reviewed the diff myself. {CANARY}"),
    })
    .to_string();
    let (status, answer) = server.post(&decide_path(), &approve);
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["acted"]["outcome"], "recorded");
    let decision = &answer["acted"]["decision"];
    let said = decision["note"]
        .as_str()
        .expect("the recorded row carries its note");
    assert!(said.starts_with("Reviewed the diff myself."), "{said}");
    assert!(!said.contains(CANARY), "masked on serve: {said}");
    assert_eq!(answer["decisions"]["standing"]["note"], decision["note"]);

    let change = server.change();
    let notes = change["notes"].as_array().unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0]["decision_id"], decision["id"]);
    assert_eq!(notes[0]["actor"], "sam");

    // The decision file is the schema-3 copy of the row and does not carry it.
    let file = root_file(&f.root);
    let text = std::fs::read_to_string(&file).expect("the decision file");
    assert!(!text.contains("Reviewed the diff"), "{text}");
    let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(doc["schema_version"], 3);

    // Withdrawing carries one too.
    let withdraw = serde_json::json!({
        "actor": "sam", "seen_version": VERSION, "note": "The agent changed the tests.",
    })
    .to_string();
    let (status, answer) = server.post(&withdraw_path(), &withdraw);
    assert_eq!(status, 200, "{answer}");
    assert_eq!(
        answer["acted"]["decision"]["note"],
        "The agent changed the tests."
    );
    assert_eq!(count(&f.db, "change_notes"), 2);
    assert_eq!(count(&f.db, "change_decisions"), 2);
}

fn root_file(root: &Path) -> std::path::PathBuf {
    thruline_core::change_decision::record_path_for(root, VERSION)
}

#[test]
fn a_decision_with_no_note_serves_no_note_and_keeps_none() {
    let f = fixture();
    let server = Server::start(&f.home, &f.db, true);
    let (status, answer) = server.post(
        &decide_path(),
        &serde_json::json!({ "actor": "sam", "seen_version": VERSION }).to_string(),
    );
    assert_eq!(status, 200, "{answer}");
    assert!(
        answer["acted"]["decision"].get("note").is_none(),
        "a derived field is absent when it does not apply: {answer}"
    );
    // A blank note is no note.
    let (status, _) = server.post(
        &withdraw_path(),
        &serde_json::json!({ "actor": "sam", "seen_version": VERSION, "note": "   " }).to_string(),
    );
    assert_eq!(status, 200);
    assert_eq!(count(&f.db, "change_notes"), 0);
}

#[test]
fn the_decision_routes_bound_the_note_and_refuse_a_field_they_do_not_name() {
    let f = fixture();
    let server = Server::start(&f.home, &f.db, true);
    let over = "n".repeat(2001);
    for path in [decide_path(), withdraw_path()] {
        let (status, answer) = server.post(
            &path,
            &serde_json::json!({ "actor": "sam", "seen_version": VERSION, "note": over })
                .to_string(),
        );
        assert_eq!(status, 409, "{path}: {answer}");
        assert_eq!(
            (answer["error"].as_str(), answer["field"].as_str()),
            (Some("too_long"), Some("note"))
        );
        // Not a field either route names.
        let (status, answer) = server.post(
            &path,
            &serde_json::json!({ "actor": "sam", "seen_version": VERSION, "decision_id": "x" })
                .to_string(),
        );
        assert_eq!(
            (status, answer["error"].as_str()),
            (400, Some("malformed_request")),
            "{path}"
        );
        // A body larger than a note within the limit could make.
        let (status, _) = server.post(
            &path,
            &serde_json::json!({ "actor": "sam", "seen_version": VERSION, "note": "x".repeat(40_000) })
                .to_string(),
        );
        assert_eq!(status, 413, "{path}");
    }
    // The name was bounded by the old 512-byte body cap; it still is.
    let (status, answer) = server.post(
        &decide_path(),
        &serde_json::json!({ "actor": "a".repeat(201), "seen_version": VERSION }).to_string(),
    );
    assert_eq!(
        (status, answer["field"].as_str()),
        (409, Some("actor")),
        "{answer}"
    );
    assert_eq!(count(&f.db, "change_decisions"), 0);
    assert_eq!(count(&f.db, "change_notes"), 0);
}

/* ── notes are free; approving is the one paid surface ─────────────────── */

#[test]
fn the_notes_route_is_free_while_approving_still_needs_a_licence() {
    let f = fixture();
    // The product as it ships: the paid gate on, no licence installed.
    let server = Server::start(&f.home, &f.db, false);
    let (status, answer) = server.post(&notes_path(), r#"{"body":"free to write"}"#);
    assert_eq!(status, 200, "{answer}");
    let (status, answer) = server.post(
        &decide_path(),
        &serde_json::json!({ "actor": "sam", "seen_version": VERSION, "note": "with a note" })
            .to_string(),
    );
    assert_eq!(status, 403, "{answer}");
    assert_eq!(answer["error"], "licence_required");
    // The refused approval kept nothing: not its row, not its note.
    assert_eq!(count(&f.db, "change_decisions"), 0);
    assert_eq!(count(&f.db, "change_notes"), 1);
}

/* ── the terminal door ─────────────────────────────────────────────────── */

fn cli(f: &Fixture, args: &[&str]) -> std::process::Output {
    Command::new(bin())
        .env("HOME", &f.home)
        .env("THRULINE_HOME", f.home.join(".thruline"))
        .env_remove("THRULINE_PAID_GATE_DEV")
        .arg("--db")
        .arg(&f.db)
        .args(["change", "note"])
        .args(args)
        .output()
        .expect("thruline change note")
}

#[test]
fn the_terminal_door_keeps_the_same_row_and_refuses_for_the_same_reasons() {
    let f = fixture();
    let kept = cli(
        &f,
        &[CHANGE, "--body", "Looks right to me.", "--actor", "alex"],
    );
    assert!(
        kept.status.success(),
        "{}",
        String::from_utf8_lossy(&kept.stdout)
    );
    let row: (String, Option<String>, String, String) = Ledger::open(&f.db)
        .unwrap()
        .conn()
        .query_row(
            "SELECT actor, decision_id, body, change_id FROM change_notes",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        row,
        (
            "alex".to_string(),
            None,
            "Looks right to me.".to_string(),
            CHANGE.to_string()
        )
    );

    for args in [
        vec![CHANGE, "--body", "   "],
        vec!["no-such-change", "--body", "hello"],
    ] {
        let refused = cli(&f, &args);
        assert_eq!(refused.status.code(), Some(1), "{args:?}");
        let said = String::from_utf8_lossy(&refused.stdout);
        assert!(said.contains("Nothing was saved"), "{said}");
    }
    let too_long = "t".repeat(2001);
    let refused = cli(&f, &[CHANGE, "--body", &too_long]);
    assert_eq!(refused.status.code(), Some(1));
    assert_eq!(
        count(&f.db, "change_notes"),
        1,
        "every refusal wrote nothing"
    );
}
