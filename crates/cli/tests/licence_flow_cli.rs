//! LICENCE-FLOW-TESTS — Free status, the Pro approval door, and paid expiry
//! through the real binary.
//!
//! Every claim below is about what the real `thruline` does as a process, on a
//! throwaway home, and none of it reaches a function directly (this
//! repository's signature defect). C20 removes the free trial in v0.1.1:
//! approving needs Pro, and a leftover v0.1.0 `trial.json` is ignored without
//! being changed or deleted.
//!
//! A licence that has to be valid comes only from a key pair generated at run
//! time, in memory, and trusted through the DEBUG-only seam;
//! no private key is committed, and no compiled test key is used. On a release
//! binary no test key can ever verify, so nothing here may depend on a seam in a
//! release build — the one positive production activation is the founder's live
//! check.
//!
//! The paid act is approving a version, so where a test says "approving is
//! allowed" it means a real approval: a Change that is accepted and bound to a
//! version, in a ledger of its own, approved through `POST
//! /api/changes/<id>/decisions`, which appends a row and writes the decision
//! file. Where it says "refused" it means `403 licence_required` with the ledger's
//! bytes unchanged.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use rand_core::OsRng;
use sha2::{Digest, Sha256};

const SEAM: &str = "THRULINE_LICENCE_DEV_PUBLIC_KEY";
const GATE: &str = "THRULINE_PAID_GATE_DEV";
const NO_BUILT_IN_KEYS: &str = "THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS";
const AFTER_RENAME: &str = "THRULINE_LICENCE_DEV_AFTER_RENAME";

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

/* ── time for a licence that expires while the UI is running ──────────── */

fn now_unix() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    i64::try_from(secs).expect("secs")
}

/* ── licences, made here, signed by a key that exists only here ────────── */

struct Issuer {
    signing: SigningKey,
}

impl Issuer {
    fn new() -> Issuer {
        Issuer {
            signing: SigningKey::generate(&mut OsRng),
        }
    }

    fn public_hex(&self) -> String {
        thruline_licence::to_hex(&self.signing.verifying_key().to_bytes())
    }

    /// A Pro licence for ThruLine, issued and expiring at the given seconds.
    fn between(&self, issued: i64, expires: i64) -> String {
        self.licence(
            &thruline_licence::timestamp(issued),
            &thruline_licence::timestamp(expires),
        )
    }

    fn licence(&self, issued_at: &str, expires_at: &str) -> String {
        let payload = format!(
            "{{\"format\":1,\"licence_id\":\"01J8ZQ4Y6T3M2N1P0R9S8T7V6W\",\"product\":\"thruline\",\"plan\":\"pro\",\"issued_at\":\"{issued_at}\",\"expires_at\":\"{expires_at}\"}}"
        );
        thruline_licence::seal(payload.as_bytes(), &self.signing).expect("seal")
    }

    /// Valid until 2099.
    fn valid(&self) -> String {
        self.licence("2026-09-01T00:00:00Z", "2099-01-01T00:00:00Z")
    }

    /// Valid until 2099, signed by this issuer, and for another product.
    fn for_product(&self, product: &str) -> String {
        let payload = format!(
            "{{\"format\":1,\"licence_id\":\"01J8ZQ4Y6T3M2N1P0R9S8T7V6W\",\"product\":\"{product}\",\"plan\":\"pro\",\"issued_at\":\"2026-09-01T00:00:00Z\",\"expires_at\":\"2099-01-01T00:00:00Z\"}}"
        );
        thruline_licence::seal(payload.as_bytes(), &self.signing).expect("seal")
    }

    /// Expired on 1 Jan 2020.
    fn expired(&self) -> String {
        self.licence("2019-01-01T00:00:00Z", "2020-01-01T00:00:00Z")
    }
}

/* ── a throwaway home, as the binary is run in it ──────────────────────── */

struct Home {
    dir: tempfile::TempDir,
}

impl Home {
    fn new() -> Home {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("h/.thruline")).expect("home");
        std::fs::create_dir_all(dir.path().join("repo")).expect("repo");
        Home { dir }
    }
    fn home(&self) -> PathBuf {
        self.dir.path().join("h")
    }
    fn thruline_home(&self) -> PathBuf {
        self.home().join(".thruline")
    }
    fn installed(&self) -> PathBuf {
        self.thruline_home().join("licence.txt")
    }
    fn trial(&self) -> PathBuf {
        self.thruline_home().join("trial.json")
    }
    fn file(&self, name: &str, text: &str) -> PathBuf {
        let p = self.dir.path().join(name);
        std::fs::write(&p, text).expect("write file");
        p
    }
    /// The environment: the binary sees this home and, when a key is given, the
    /// one test seam that lets a DEBUG build trust it. Every other seam name is
    /// removed, so the build is the one that ships plus that key.
    fn env(&self, cmd: &mut Command, key: Option<&str>) {
        cmd.env("HOME", self.home())
            .env("THRULINE_HOME", self.thruline_home())
            .env_remove(SEAM)
            .env_remove(GATE)
            .env_remove(NO_BUILT_IN_KEYS)
            .env_remove(AFTER_RENAME)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .current_dir(self.dir.path().join("repo"))
            .stdin(Stdio::null());
        if let Some(k) = key {
            cmd.env(SEAM, k);
        }
    }
    fn cli(&self, key: Option<&str>, args: &[&str]) -> Output {
        let mut cmd = Command::new(bin());
        cmd.args(args);
        self.env(&mut cmd, key);
        cmd.output().expect("run thruline")
    }
    fn status(&self, key: Option<&str>) -> serde_json::Value {
        let o = self.cli(key, &["licence", "status", "--json"]);
        assert!(o.status.success(), "status --json exited {:?}", o.status);
        serde_json::from_slice(&o.stdout).expect("status json")
    }
    fn status_line(&self, key: Option<&str>) -> String {
        let o = self.cli(key, &["licence", "status"]);
        assert!(o.status.success(), "status exited {:?}", o.status);
        stdout(&o).trim().to_string()
    }
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn sha256_of(path: &Path) -> String {
    thruline_licence::to_hex(&Sha256::digest(std::fs::read(path).expect("read")))
}

/* ── one Change a person can approve, in a ledger of its own ───────────── */

const CHANGE_ID: &str = "cccccccc-0000-4000-8000-00000000c001";
const PROJECT_ID: &str = "cccccccc-0000-4000-8000-00000000p001";
const VERSION: &str = "snap_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// A ledger OUTSIDE the ThruLine home holding one Change that is accepted, bound
/// to a version and not yet approved, with its project folder beside it. Made
/// straight in the tables, as `change_export_route.rs` does, so an approval a
/// test asks for is a real one. Outside the home, so the same record can be
/// approached from another home (an accepted limit below) and so it stays put
/// when a home is made unreadable.
struct Approvable {
    db: PathBuf,
    root: PathBuf,
}

impl Approvable {
    fn new(home: &Home) -> Approvable {
        use rusqlite::params;
        let db = home.dir.path().join("approvable.db");
        let root = home.dir.path().join("worktree");
        std::fs::create_dir_all(&root).expect("worktree");
        let ledger = thruline_core::ledger::Ledger::open(&db).expect("open the ledger");
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
        let doc = serde_json::json!({
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
        let text = serde_json::to_string(&doc).expect("doc");
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
        .expect("snapshot");
        conn.execute(
            "INSERT INTO change_implementations(change_id, binding_version, bound_at,
                 repository_identity, base_revision, snapshot_id, workspace_identity)
             VALUES(?1, 1, '2026-09-22T10:00:00Z', 'shop', 'base0000', ?2, NULL)",
            params![CHANGE_ID, VERSION],
        )
        .expect("implementation");
        Approvable { db, root }
    }

    fn decision_file(&self) -> PathBuf {
        thruline_core::change_decision::record_path_for(&self.root, VERSION)
    }

    /// How many approvals and withdrawals the ledger holds.
    fn decision_rows(&self) -> i64 {
        let conn = rusqlite::Connection::open_with_flags(
            &self.db,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("open read-only");
        conn.query_row("SELECT COUNT(*) FROM change_decisions", [], |r| r.get(0))
            .expect("count")
    }

    /// The request the Change screen's Approve sends, for the version it shows.
    fn approve(&self, server: &Server) -> (u16, String) {
        server.send(
            "POST",
            &format!("/api/changes/{CHANGE_ID}/decisions"),
            &format!(r#"{{"actor":"sam","seen_version":"{VERSION}"}}"#),
        )
    }

    /// Approving was allowed and recorded: 200, a row, and its decision file.
    fn assert_recorded(&self, server: &Server, rows_after: i64, what: &str) {
        let (code, text) = self.approve(server);
        assert_eq!(code, 200, "{what}: approving was not allowed: {text}");
        assert!(
            text.contains("\"outcome\":\"recorded\""),
            "{what}: not recorded: {text}"
        );
        assert_eq!(self.decision_rows(), rows_after, "{what}");
        assert!(self.decision_file().is_file(), "{what}: no decision file");
    }

    /// Approving was refused at the licence gate, for `status`, and appended
    /// nothing: not a row, and not a byte of the ledger.
    fn assert_refused(&self, server: &Server, status: &str, what: &str) -> String {
        let before = (self.decision_rows(), sha256_of(&self.db));
        let (code, text) = self.approve(server);
        assert_eq!(code, 403, "{what}: {text}");
        assert!(
            text.contains("\"error\":\"licence_required\""),
            "{what}: {text}"
        );
        assert!(
            text.contains(&format!("\"status\":\"{status}\"")),
            "{what}: expected status {status}: {text}"
        );
        assert_eq!(
            (self.decision_rows(), sha256_of(&self.db)),
            before,
            "{what}: a refused approval changed the ledger"
        );
        text
    }
}

/* ── `thruline ui`, and the requests a browser sends it ────────────────── */

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
    fn start(home: &Home, key: Option<&str>, db: Option<&Path>) -> Server {
        Server::spawn(home, key, db, None)
    }

    /// `thruline ui` inside a macOS sandbox profile.
    #[cfg(target_os = "macos")]
    fn start_in(home: &Home, key: Option<&str>, db: Option<&Path>, profile: &str) -> Server {
        Server::spawn(home, key, db, Some(profile))
    }

    fn spawn(home: &Home, key: Option<&str>, db: Option<&Path>, profile: Option<&str>) -> Server {
        let mut cmd = match profile {
            Some(profile) => {
                let mut c = Command::new("/usr/bin/sandbox-exec");
                c.arg("-p").arg(profile).arg(bin());
                c
            }
            None => Command::new(bin()),
        };
        if let Some(db) = db {
            cmd.arg("--db").arg(db);
        }
        cmd.args(["ui", "--no-open", "--port", "0", "--dir"])
            .arg(home.dir.path().join("export"));
        home.env(&mut cmd, key);
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("thruline ui");
        let out = child.stdout.take().expect("stdout");
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
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(line) => {
                    if let Some(rest) = line.trim().strip_prefix("ThruLine is at http://127.0.0.1:")
                    {
                        let port = rest
                            .split('/')
                            .next()
                            .and_then(|p| p.trim().parse().ok())
                            .expect("port");
                        return Server { child, port };
                    }
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("`thruline ui` never printed its address");
                }
            }
        }
    }

    /// `(status, whole response text)` for one request.
    fn send(&self, method: &str, path: &str, body: &str) -> (u16, String) {
        let mut socket = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        socket
            .set_read_timeout(Some(Duration::from_secs(120)))
            .expect("timeout");
        let request = if method == "POST" {
            format!(
                "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{p}\r\nOrigin: http://127.0.0.1:{p}\r\nContent-Type: application/json\r\nContent-Length: {n}\r\nConnection: close\r\n\r\n{body}",
                p = self.port,
                n = body.len()
            )
        } else {
            format!(
                "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
                self.port
            )
        };
        socket.write_all(request.as_bytes()).expect("request");
        let mut raw = Vec::new();
        let _ = socket.read_to_end(&mut raw);
        let text = String::from_utf8_lossy(&raw).into_owned();
        let status = text
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        (status, text)
    }

    /// `GET /api/licence`, the document the screen reads.
    fn licence(&self) -> serde_json::Value {
        let (status, text) = self.send("GET", "/api/licence", "");
        assert_eq!(status, 200, "{text}");
        let body = &text[text.find("\r\n\r\n").expect("body") + 4..];
        serde_json::from_str(body).expect("json")
    }
}

/* ══ 1. a fresh home is Free, and approving needs Pro ══════════════════ */

#[test]
fn a_fresh_home_is_free_and_approving_is_off_without_pro() {
    let home = Home::new();
    let server = Server::start(&home, None, None);
    let served = server.licence();
    assert_eq!(served["paid_gate"], true, "{served}");
    assert_eq!(served["headline"], "free", "{served}");
    assert_eq!(
        served["entitlements"]["approve_version"]["allowed"], false,
        "{served}"
    );
    assert!(served.get("trial").is_none(), "{served}");
    drop(server);

    assert!(home.status_line(None).starts_with("Free"));
    let status = home.status(None);
    assert_eq!(status["headline"], "free", "{status}");
    assert_eq!(status["entitlements"]["approve_version"]["allowed"], false);
    assert!(status.get("trial").is_none(), "{status}");
    assert!(
        !home.trial().exists(),
        "a status read wrote an obsolete trial file"
    );
}

/* ══ 2. paid approval, installation and removal ══════════════════════════ */

#[test]
fn a_pro_licence_opens_approving_and_removing_it_shuts_it_again() {
    let issuer = Issuer::new();
    let key = issuer.public_hex();
    let home = Home::new();
    let store = Approvable::new(&home);
    let server = Server::start(&home, Some(&key), Some(&store.db));
    store.assert_refused(&server, "absent", "before the licence");
    assert!(!home.trial().exists());

    let file = home.file("licence-from-the-page.txt", &issuer.valid());
    let o = home.cli(Some(&key), &["licence", "install", &file.to_string_lossy()]);
    assert!(o.status.success(), "{}", stdout(&o));
    assert_eq!(
        stdout(&o).trim(),
        "Installed. This machine has a ThruLine Pro licence, valid until 1 Jan 2099."
    );
    let doc = home.status(Some(&key));
    assert_eq!(doc["status"], "valid", "{doc}");
    assert_eq!(doc["headline"], "licence", "{doc}");
    assert_eq!(doc["entitlements"]["approve_version"]["allowed"], true);
    store.assert_recorded(&server, 1, "with the Pro licence installed");
    assert!(!home.trial().exists(), "Pro approval wrote a trial file");

    let o = home.cli(Some(&key), &["licence", "remove"]);
    assert!(o.status.success());
    assert!(!home.installed().exists());
    let doc = home.status(Some(&key));
    assert_eq!(doc["status"], "absent", "{doc}");
    assert_eq!(doc["entitlements"]["approve_version"]["allowed"], false);
    store.assert_refused(&server, "absent", "after the licence was removed");
    assert_eq!(
        store.decision_rows(),
        1,
        "removing a licence touched a record"
    );
    assert!(!home.trial().exists());
}

/* ══ 3. an installed paid licence that has expired ═══════════════════════ */

#[test]
fn an_installed_expired_licence_is_refused_and_says_expired() {
    let issuer = Issuer::new();
    let key = issuer.public_hex();
    let home = Home::new();
    let store = Approvable::new(&home);

    // Install refuses a licence that was already expired.
    let file = home.file("expired.txt", &issuer.expired());
    let o = home.cli(Some(&key), &["licence", "install", &file.to_string_lossy()]);
    assert!(!o.status.success(), "{}", stdout(&o));
    assert!(
        stdout(&o).starts_with("Not installed. This licence expired on 1 Jan 2020."),
        "{}",
        stdout(&o)
    );
    assert!(!home.installed().exists());
    // A licence that expired after installation remains as a file and is
    // reported as expired, with the paid renewal path.
    std::fs::write(home.installed(), issuer.expired()).expect("place");
    let doc = home.status(Some(&key));
    assert_eq!(doc["status"], "expired", "{doc}");
    assert_eq!(doc["sentence"], "This licence expired on 1 Jan 2020.");
    assert_eq!(
        doc["entitlements"]["approve_version"]["allowed"], false,
        "{doc}"
    );
    let server = Server::start(&home, Some(&key), Some(&store.db));
    let text = store.assert_refused(&server, "expired", "an expired licence");
    assert!(
        text.contains("This licence expired on 1 Jan 2020."),
        "{text}"
    );
    assert!(!home.trial().exists());
}

/* ══ 4. a licence that runs out while `thruline ui` is running ═══════════ */

#[test]
fn a_licence_that_expires_three_seconds_into_a_running_ui_allows_and_then_refuses() {
    let issuer = Issuer::new();
    let key = issuer.public_hex();
    let home = Home::new();
    let store = Approvable::new(&home);
    // The server is already up without a licence when it appears. Entitlement
    // is read on each request, so no restart is needed.
    let server = Server::start(&home, Some(&key), Some(&store.db));
    store.assert_refused(&server, "absent", "before the licence appears");

    let now = now_unix();
    let expires = now + 3;
    std::fs::write(home.installed(), issuer.between(now - 60, expires)).expect("place");
    assert_eq!(server.licence()["status"], "valid");
    store.assert_recorded(&server, 1, "inside the licence's last seconds");

    while now_unix() <= expires {
        std::thread::sleep(Duration::from_millis(100));
    }
    let doc = server.licence();
    assert_eq!(doc["status"], "expired", "{doc}");
    assert_eq!(
        doc["entitlements"]["approve_version"]["allowed"], false,
        "{doc}"
    );
    store.assert_refused(&server, "expired", "after the licence ran out");
    assert_eq!(store.decision_rows(), 1, "the record kept the one approval");
    assert!(!home.trial().exists());
}

/* ══ 8. licences that are cut or mangled ═══════════════════════════════════ */

/// Every install below is checked the same way: it says it did not install
/// anything, exits non-zero, and the licence that was in place is byte for byte
/// what it was, still valid.
fn install_is_refused_and_the_good_one_is_kept(
    home: &Home,
    key: &str,
    good: &str,
    name: &str,
    bad: &[u8],
    sentence: &str,
) {
    let file = home.dir.path().join(format!("{name}.txt"));
    std::fs::write(&file, bad).expect("write the bad licence");
    let o = home.cli(Some(key), &["licence", "install", &file.to_string_lossy()]);
    assert!(
        !o.status.success(),
        "{name}: a licence that should be refused was installed: {}",
        stdout(&o)
    );
    assert!(
        stdout(&o).starts_with(&format!("Not installed. {sentence}")),
        "{name}: {}",
        stdout(&o)
    );
    assert_eq!(
        std::fs::read_to_string(home.installed()).expect("still installed"),
        good,
        "{name}: a refused install touched the licence that works"
    );
    let doc = home.status(Some(key));
    assert_eq!(doc["status"], "valid", "{name}: {doc}");
}

#[test]
fn licences_cut_at_the_signature_or_mid_payload_are_refused_and_the_good_one_is_kept() {
    const NOT_A_LICENCE: &str = "This file isn't a ThruLine licence, or part of it is missing.";
    let issuer = Issuer::new();
    let key = issuer.public_hex();
    let home = Home::new();
    let good = issuer.valid();
    let file = home.file("good.txt", &good);
    let o = home.cli(Some(&key), &["licence", "install", &file.to_string_lossy()]);
    assert!(o.status.success(), "{}", stdout(&o));

    // Cut at the signature: half of it, then none of it.
    let at_signature = good.find("signature: ").expect("signature") + "signature: ".len();
    let half = at_signature + (good.trim_end().len() - at_signature) / 2;
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("cut-inside-the-signature", good.as_bytes()[..half].to_vec()),
        (
            "cut-before-the-signature",
            good.as_bytes()[..at_signature].to_vec(),
        ),
        (
            "cut-after-one-signature-digit",
            good.as_bytes()[..at_signature + 1].to_vec(),
        ),
    ];
    // Cut mid-payload: the file ends inside the JSON.
    let payload_at = good.find("payload: ").expect("payload") + "payload: ".len();
    let mid_payload = payload_at + 40;
    assert!(
        mid_payload < at_signature,
        "the payload is longer than 40 bytes"
    );
    let cases = cases.into_iter().chain([
        ("cut-mid-payload", good.as_bytes()[..mid_payload].to_vec()),
        (
            "cut-mid-payload-with-the-signature-kept",
            [
                &good.as_bytes()[..mid_payload],
                b"\n",
                &good.as_bytes()[at_signature - "signature: ".len()..],
            ]
            .concat(),
        ),
        ("empty", Vec::new()),
    ]);
    for (name, bad) in cases {
        // Without its signature a file is not a licence; with the signature but
        // not the whole payload, the signature no longer matches.
        let sentence = if name == "cut-mid-payload-with-the-signature-kept" {
            "This licence has been changed, or wasn't made by the maker it names, so it can't be trusted."
        } else {
            NOT_A_LICENCE
        };
        install_is_refused_and_the_good_one_is_kept(&home, &key, &good, name, &bad, sentence);
    }
}

#[test]
fn a_licence_given_crlf_line_endings_is_read_as_documented_and_a_second_carriage_return_is_refused()
{
    // FORMAT.md, line 7: "Lines end in `\n` (a trailing `\r` is ignored)". A
    // licence that went through a mail client or a Windows editor and came back
    // with CRLF endings is the same licence, and the binary reads it as one.
    // What is refused is a carriage return that is NOT the line ending: one more
    // of them leaves the header line as `ThruLine licence\r`, which is not the
    // header, and one inside the payload changes the bytes the signature covers.
    const NOT_A_LICENCE: &str = "This file isn't a ThruLine licence, or part of it is missing.";
    const TAMPERED: &str = "This licence has been changed, or wasn't made by the maker it names, so it can't be trusted.";
    let issuer = Issuer::new();
    let key = issuer.public_hex();
    let home = Home::new();
    let good = issuer.valid();

    let crlf = good.replace('\n', "\r\n");
    let file = home.file("crlf.txt", &crlf);
    let o = home.cli(Some(&key), &["licence", "install", &file.to_string_lossy()]);
    assert!(
        o.status.success(),
        "a CRLF copy of a good licence was refused: {}",
        stdout(&o)
    );
    assert_eq!(
        stdout(&o).trim(),
        "Installed. This machine has a ThruLine Pro licence, valid until 1 Jan 2099."
    );
    assert_eq!(
        std::fs::read_to_string(home.installed()).expect("installed"),
        crlf,
        "the bytes that verified are the bytes installed"
    );

    let doubled = good.replace('\n', "\r\r\n");
    let inside = good.replacen("\"plan\"", "\r\"plan\"", 1);
    for (name, bad, sentence) in [
        ("two-carriage-returns", doubled, NOT_A_LICENCE),
        ("a-carriage-return-in-the-payload", inside, TAMPERED),
    ] {
        install_is_refused_and_the_good_one_is_kept(
            &home,
            &key,
            &crlf,
            name,
            bad.as_bytes(),
            sentence,
        );
    }
}

/* ══ 9. `thruline ui` with no way out but loopback ═════════════════════════ */

/// Outbound network is denied — and the process killed on the attempt, so a run
/// that reaches its normal exit made none — except to this machine's own
/// loopback, which is all a browser needs to talk to `thruline ui`.
#[cfg(target_os = "macos")]
const LOOPBACK_ONLY: &str = "(version 1)(allow default)(deny network-outbound (with send-signal SIGKILL))(allow network-outbound (remote ip \"localhost:*\"))";

#[cfg(target_os = "macos")]
fn killed_by_the_denial(o: &Output) -> bool {
    use std::os::unix::process::ExitStatusExt;
    o.status.signal() == Some(9)
}

#[cfg(target_os = "macos")]
fn run_in_the_profile(program: &str, args: &[&str]) -> Output {
    Command::new("/usr/bin/sandbox-exec")
        .arg("-p")
        .arg(LOOPBACK_ONLY)
        .arg(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("sandbox-exec")
}

#[cfg(target_os = "macos")]
#[test]
fn thruline_ui_runs_the_whole_licence_path_with_no_way_out_but_loopback() {
    // Positive controls prove the profile kills outbound connections and
    // permits the loopback connection the UI needs.
    let outside = run_in_the_profile("/usr/bin/nc", &["-z", "-w", "1", "192.0.2.1", "9"]);
    assert!(
        killed_by_the_denial(&outside),
        "the profile did not stop an outbound connect: {:?}",
        outside.status
    );
    let loopback = run_in_the_profile("/usr/bin/nc", &["-z", "-w", "1", "127.0.0.1", "9"]);
    assert!(
        !killed_by_the_denial(&loopback),
        "the profile stopped loopback: {:?}",
        loopback.status
    );
    let quiet = run_in_the_profile("/bin/echo", &["quiet"]);
    assert!(quiet.status.success(), "{:?}", quiet.status);

    let issuer = Issuer::new();
    let key = issuer.public_hex();
    let home = Home::new();
    let store = Approvable::new(&home);
    let mut server = Server::start_in(&home, Some(&key), Some(&store.db), LOOPBACK_ONLY);
    let doc = server.licence();
    assert_eq!(doc["headline"], "free", "{doc}");
    assert_eq!(doc["entitlements"]["approve_version"]["allowed"], false);
    store.assert_refused(&server, "absent", "no Pro licence under network denial");
    assert!(!home.trial().exists());

    std::fs::write(home.installed(), issuer.valid()).expect("place Pro licence");
    assert_eq!(server.licence()["status"], "valid");
    store.assert_recorded(&server, 1, "Pro approval under network denial");
    assert!(
        server.child.try_wait().expect("poll").is_none(),
        "`thruline ui` was killed under the loopback-only profile"
    );
    for args in [
        &["licence", "status"][..],
        &["licence", "status", "--json"][..],
        &["licence", "fingerprint"][..],
    ] {
        let mut cmd = Command::new("/usr/bin/sandbox-exec");
        cmd.arg("-p").arg(LOOPBACK_ONLY).arg(bin()).args(args);
        home.env(&mut cmd, Some(&key));
        let o = cmd.output().expect("run thruline");
        assert!(
            !killed_by_the_denial(&o),
            "`thruline {}` tried to leave the machine",
            args.join(" ")
        );
    }
    let _ = server.child.kill();
}

/* ══ 10. fail closed ═══════════════════════════════════════════════════════ */

/// What is on disk at `trial.json`, so a test can say it was not touched.
fn shape_of(path: &Path) -> String {
    let meta = std::fs::symlink_metadata(path).expect("metadata");
    if meta.file_type().is_symlink() {
        format!(
            "symlink to {}",
            std::fs::read_link(path).expect("link").display()
        )
    } else if meta.is_dir() {
        "directory".to_string()
    } else {
        format!("file {}", sha256_of(path))
    }
}

#[cfg(unix)]
#[test]
fn legacy_trial_file_shapes_are_ignored_and_left_untouched() {
    type Place = fn(&Home);
    let variants: [(&str, Place); 4] = [
        ("garbage", |h| {
            std::fs::write(h.trial(), "{ not a trial").expect("place");
        }),
        ("a directory", |h| {
            std::fs::create_dir(h.trial()).expect("place");
        }),
        ("a symlink to garbage", |h| {
            let target = h.dir.path().join("elsewhere.json");
            std::fs::write(&target, "garbage, not a trial").expect("target");
            std::os::unix::fs::symlink(&target, h.trial()).expect("symlink");
        }),
        ("a symlink to a directory", |h| {
            let target = h.dir.path().join("a-folder");
            std::fs::create_dir(&target).expect("target");
            std::os::unix::fs::symlink(&target, h.trial()).expect("symlink");
        }),
    ];
    for (name, place) in variants {
        let home = Home::new();
        let store = Approvable::new(&home);
        place(&home);
        let shape = shape_of(&home.trial());

        assert!(home.status_line(None).starts_with("Free"), "{name}");
        let doc = home.status(None);
        assert_eq!(doc["headline"], "free", "{name}: {doc}");
        assert!(doc.get("trial").is_none(), "{name}: {doc}");
        assert_eq!(
            doc["entitlements"]["approve_version"]["allowed"], false,
            "{name}: {doc}"
        );
        let server = Server::start(&home, None, Some(&store.db));
        store.assert_refused(&server, "absent", name);
        assert!(!store.decision_file().exists(), "{name}: decision file");
        assert_eq!(shape_of(&home.trial()), shape, "{name}: old file changed");
        let stray: Vec<String> = std::fs::read_dir(home.thruline_home())
            .expect("read_dir")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("trial.json.") || n == "licence.txt")
            .collect();
        assert!(stray.is_empty(), "{name}: left beside it: {stray:?}");
    }
}

/* ══ 11. a machine-unbound paid licence works in a second home ═══════════ */

#[test]
fn a_copied_pro_licence_works_in_a_second_home() {
    let issuer = Issuer::new();
    let key = issuer.public_hex();
    let first = Home::new();
    let file = first.file("licence.txt", &issuer.valid());
    let o = first.cli(Some(&key), &["licence", "install", &file.to_string_lossy()]);
    assert!(o.status.success(), "{}", stdout(&o));

    // This paid licence names no machine. Copying its unchanged bytes to a
    // second home leaves approving available there too.
    let second = Home::new();
    std::fs::copy(first.installed(), second.installed()).expect("copy the licence");
    let store = Approvable::new(&second);
    let doc = second.status(Some(&key));
    assert_eq!(doc["status"], "valid", "{doc}");
    assert_eq!(doc["technical"]["machine_bound"], false, "{doc}");
    assert_eq!(doc["entitlements"]["approve_version"]["allowed"], true);
    let server = Server::start(&second, Some(&key), Some(&store.db));
    store.assert_recorded(&server, 1, "with a copied Pro licence in a second home");
    assert!(!second.trial().exists());
}

/* ══ 12. Pro approvals at the same moment and physical refusals ══════════ */
//
// With one valid Pro licence, simultaneous requests for the same version still
// record exactly one approval. The others get the ordinary record refusal,
// and a later refused request leaves the ledger, WAL and decision file alone.

/// What is in `dir`, by name, sorted.
fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read_dir")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// `per_server` first approvals at EACH of `servers`, every one from its own
/// thread and its own connection, all released at the same instant.
fn approve_at_once(
    store: &Approvable,
    servers: &[&Server],
    per_server: usize,
) -> Vec<(u16, String)> {
    let gate = std::sync::Barrier::new(servers.len() * per_server);
    std::thread::scope(|scope| {
        let mut sent = Vec::new();
        for &server in servers {
            for _ in 0..per_server {
                let gate = &gate;
                sent.push(scope.spawn(move || {
                    gate.wait();
                    store.approve(server)
                }));
            }
        }
        sent.into_iter()
            .map(|t| t.join().expect("an approving thread panicked"))
            .collect()
    })
}

/// How many rounds a race is run: the number the test names, or more when
/// `THRULINE_TEST_RACE_ROUNDS` asks for it (a run at scale), never fewer.
fn race_rounds(named: usize) -> usize {
    std::env::var("THRULINE_TEST_RACE_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map_or(named, |n: usize| n.max(named))
}

fn place_pro(home: &Home) -> String {
    let issuer = Issuer::new();
    let key = issuer.public_hex();
    std::fs::write(home.installed(), issuer.valid()).expect("place test Pro licence");
    key
}

/// The ordinary refusal of a second approval of one version, which is the only
/// thing a loser of these races may be told.
const ONE_AT_A_TIME: &str = "One approval stands at a time";

/// What must be true after paid approvals arrived together: exactly one was
/// recorded, and the losers were refused by the record rather than the gate.
fn assert_exactly_one_recorded(
    results: &[(u16, String)],
    store: &Approvable,
    home: &Home,
    before: &[String],
    what: &str,
) {
    let all = format!("{results:#?}");
    for (code, text) in results {
        assert!(
            !text.contains("licence_required"),
            "{what}: a paid approval was refused at the licence gate ({code}): {text}\nall: {all}"
        );
    }
    let recorded = results
        .iter()
        .filter(|(code, text)| *code == 200 && text.contains("\"outcome\":\"recorded\""))
        .count();
    assert_eq!(recorded, 1, "{what}: exactly one is recorded\nall: {all}");
    for (code, text) in results {
        let won = *code == 200 && text.contains("\"outcome\":\"recorded\"");
        assert!(
            won || (*code == 400 && text.contains(ONE_AT_A_TIME)),
            "{what}: a loser was refused for another reason ({code}): {text}"
        );
    }
    assert_eq!(store.decision_rows(), 1, "{what}: rows in the ledger");
    assert!(store.decision_file().is_file(), "{what}: no decision file");

    // Approval needs no new file in the ThruLine home. In particular, it does
    // not create or stage the v0.1.0 trial file.
    assert!(!home.trial().exists(), "{what}: approval wrote trial.json");
    assert_eq!(
        names_in(&home.thruline_home()),
        before,
        "{what}: approval changed the ThruLine home"
    );
}

/// Every attempt after the first is refused for the ordinary reason and changes
/// not one byte of the ledger or the decision file.
fn assert_later_attempts_append_nothing(store: &Approvable, server: &Server, home: &Home) {
    assert_later_attempts_append_nothing_after(store, server, home, || {});
}

/// `after_accepted` finishes any fixture work from the accepted approval
/// before this helper begins measuring later refusals.
fn assert_later_attempts_append_nothing_after(
    store: &Approvable,
    server: &Server,
    home: &Home,
    after_accepted: impl FnOnce(),
) {
    assert_later_attempts_append_nothing_with_hooks(
        store,
        server,
        home,
        after_accepted,
        || {},
        |_| {},
    );
}

/// The hooks expose the exact post-baseline and post-400 observation points to
/// physical-ledger controls. Ordinary races use no-op hooks.
fn assert_later_attempts_append_nothing_with_hooks(
    store: &Approvable,
    server: &Server,
    home: &Home,
    after_accepted: impl FnOnce(),
    after_baseline: impl FnOnce(),
    mut after_refusal: impl FnMut(usize),
) {
    after_accepted();

    // An accepted approval can still be in SQLite's WAL after its response.
    // A later connection may copy those already committed pages into the main
    // file even when its own approval is refused. Finish that earlier physical
    // write before taking the raw-byte baseline; never checkpoint after a
    // refusal, because that would conceal a write made by the refused act.
    let rows = store.decision_rows();
    let decision = sha256_of(&store.decision_file());
    assert!(!home.trial().exists(), "a Pro approval wrote trial.json");
    let checkpoint = rusqlite::Connection::open_with_flags(
        &store.db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .expect("open accepted ledger for checkpoint");
    let busy: i64 = checkpoint
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))
        .expect("checkpoint accepted approval");
    assert_eq!(busy, 0, "accepted approval still has a held WAL reader");
    drop(checkpoint);
    assert_eq!(store.decision_rows(), rows, "checkpoint changed decisions");
    assert_eq!(sha256_of(&store.decision_file()), decision);
    assert!(!home.trial().exists(), "checkpoint wrote trial.json");

    let ledger = sha256_of(&store.db);
    let ledger_wal = wal_bytes(&store.db);
    after_baseline();
    for attempt in 0..3 {
        let (code, text) = store.approve(server);
        assert_eq!(code, 400, "later attempt {attempt}: {text}");
        assert!(
            text.contains(ONE_AT_A_TIME),
            "later attempt {attempt}: {text}"
        );
        after_refusal(attempt);
        assert_eq!(
            sha256_of(&store.db),
            ledger,
            "later attempt {attempt}: a refused attempt changed the ledger"
        );
        assert_eq!(
            sha256_of(&store.decision_file()),
            decision,
            "later attempt {attempt}: the decision file changed"
        );
        assert!(
            !home.trial().exists(),
            "later attempt {attempt}: wrote trial.json"
        );
        assert_eq!(
            store.decision_rows(),
            rows,
            "later attempt {attempt}: decisions changed"
        );
        assert!(
            wal_bytes(&store.db) == ledger_wal,
            "later attempt {attempt}: ledger WAL changed during the refused interval"
        );
    }
}

/// Missing and empty WAL files are different physical states. The `-shm`
/// sidecar holds SQLite reader coordination, not persistent ledger bytes.
fn wal_bytes(db: &Path) -> Option<Vec<u8>> {
    match std::fs::read(db.with_extension("db-wal")) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => panic!("read ledger WAL: {error}"),
    }
}

struct RefusalPhysicalSnapshot {
    main: Vec<u8>,
    wal: Option<Vec<u8>>,
    decision: Vec<u8>,
    rows: i64,
}

fn refusal_physical_snapshot(store: &Approvable) -> RefusalPhysicalSnapshot {
    RefusalPhysicalSnapshot {
        main: std::fs::read(&store.db).expect("read main ledger"),
        wal: wal_bytes(&store.db),
        decision: std::fs::read(store.decision_file()).expect("read decision file"),
        rows: store.decision_rows(),
    }
}

/// Retain exact synthetic bytes for a dedicated red/green control run when its
/// external evidence directory is supplied. Normal suite runs write nothing.
fn preserve_refusal_snapshot(label: &str, snapshot: &RefusalPhysicalSnapshot) {
    let Some(dir) = std::env::var_os("THRULINE_R3_EVIDENCE_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("create R3 evidence dir");
    std::fs::write(dir.join(format!("{label}.db")), &snapshot.main).expect("save main DB");
    match &snapshot.wal {
        Some(bytes) => {
            std::fs::write(dir.join(format!("{label}.db-wal")), bytes).expect("save WAL");
        }
        None => {
            std::fs::write(dir.join(format!("{label}.db-wal.absent")), [])
                .expect("mark absent WAL");
        }
    }
    std::fs::write(dir.join(format!("{label}.decision")), &snapshot.decision)
        .expect("save decision file");
    std::fs::write(dir.join(format!("{label}.rows")), snapshot.rows.to_string())
        .expect("save row count");
}

#[test]
fn a_committed_wal_only_change_is_caught_by_the_real_refusal_witness() {
    let home = Home::new();
    let store = Approvable::new(&home);
    let key = place_pro(&home);
    let server = Server::start(&home, Some(&key), Some(&store.db));
    store.assert_recorded(&server, 1, "WAL witness control");

    let baseline = std::cell::RefCell::new(None);
    let observed = std::cell::RefCell::new(None);
    let held_reader = std::cell::RefCell::new(None);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_later_attempts_append_nothing_with_hooks(
            &store,
            &server,
            &home,
            || {},
            || {
                let snapshot = refusal_physical_snapshot(&store);
                preserve_refusal_snapshot("before", &snapshot);
                *baseline.borrow_mut() = Some(snapshot);
                let reader = rusqlite::Connection::open_with_flags(
                    &store.db,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                )
                .expect("post-baseline reader");
                reader.execute_batch("BEGIN").expect("begin held read");
                let projects: i64 = reader
                    .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
                    .expect("pin reader snapshot");
                assert_eq!(projects, 1, "reader did not see the project");
                *held_reader.borrow_mut() = Some(reader);
            },
            |attempt| {
                if attempt == 0 {
                    let writer = rusqlite::Connection::open_with_flags(
                        &store.db,
                        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
                    )
                    .expect("independent WAL writer");
                    writer
                        .execute_batch("BEGIN IMMEDIATE")
                        .expect("begin write");
                    assert_eq!(
                        writer
                            .execute(
                                "UPDATE projects SET display_name='WAL witness control' WHERE id=?1",
                                [PROJECT_ID],
                            )
                            .expect("update unrelated project"),
                        1,
                        "control did not update one project"
                    );
                    writer.execute_batch("COMMIT").expect("commit WAL write");
                    let snapshot = refusal_physical_snapshot(&store);
                    preserve_refusal_snapshot("after", &snapshot);
                    *observed.borrow_mut() = Some(snapshot);
                }
            },
        );
    }));

    let before = baseline.into_inner().expect("baseline was reached");
    let after = observed.into_inner().expect("committed write was observed");
    assert_eq!(before.main, after.main, "control changed the main DB");
    assert_ne!(before.wal, after.wal, "control did not change WAL bytes");
    assert_eq!(
        before.decision, after.decision,
        "control changed decision file"
    );
    assert_eq!(before.rows, 1, "control did not start with one decision");
    assert_eq!(after.rows, 1, "control changed decision count");
    assert!(held_reader.borrow().is_some(), "reader was not held");
    eprintln!(
        "WAL control: before={:?} after={:?}; main, decision and one row unchanged",
        before.wal.as_ref().map(Vec::len),
        after.wal.as_ref().map(Vec::len)
    );

    let panic = result.expect_err("real refusal helper accepted a committed WAL-only change");
    let message = if let Some(text) = panic.downcast_ref::<String>() {
        text.as_str()
    } else if let Some(text) = panic.downcast_ref::<&'static str>() {
        text
    } else {
        panic!("real refusal helper raised a non-text panic");
    };
    assert!(
        message.contains("later attempt 0: ledger WAL changed during the refused interval"),
        "wrong panic from refusal helper: {message}"
    );
}

#[test]
fn a_post_baseline_reader_keeps_ordinary_refusals_physically_unchanged() {
    let home = Home::new();
    let store = Approvable::new(&home);
    let key = place_pro(&home);
    let server = Server::start(&home, Some(&key), Some(&store.db));
    store.assert_recorded(&server, 1, "held-reader refusal control");

    let baseline = std::cell::RefCell::new(None);
    let observed = std::cell::RefCell::new(None);
    let held_reader = std::cell::RefCell::new(None);
    assert_later_attempts_append_nothing_with_hooks(
        &store,
        &server,
        &home,
        || {},
        || {
            let snapshot = refusal_physical_snapshot(&store);
            preserve_refusal_snapshot("reader-before", &snapshot);
            *baseline.borrow_mut() = Some(snapshot);
            let reader = rusqlite::Connection::open_with_flags(
                &store.db,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .expect("post-baseline reader");
            reader.execute_batch("BEGIN").expect("begin held read");
            let projects: i64 = reader
                .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
                .expect("pin reader snapshot");
            assert_eq!(projects, 1, "reader did not see the project");
            *held_reader.borrow_mut() = Some(reader);
        },
        |attempt| {
            if attempt == 0 {
                let snapshot = refusal_physical_snapshot(&store);
                preserve_refusal_snapshot("reader-after", &snapshot);
                *observed.borrow_mut() = Some(snapshot);
            }
        },
    );

    let before = baseline.into_inner().expect("baseline was reached");
    let after = observed.into_inner().expect("ordinary 400 was observed");
    assert_eq!(before.main, after.main, "ordinary 400 changed the main DB");
    assert_eq!(before.wal, after.wal, "ordinary 400 changed WAL bytes");
    assert_eq!(
        before.decision, after.decision,
        "ordinary 400 changed decision file"
    );
    assert_eq!(before.rows, 1, "control did not start with one decision");
    assert_eq!(after.rows, 1, "ordinary 400 changed decision count");
    assert!(held_reader.borrow().is_some(), "reader was not held");
}

#[test]
fn a_delayed_accepted_wal_checkpoint_is_outside_the_later_refusal_interval() {
    let home = Home::new();
    let store = Approvable::new(&home);
    let key = place_pro(&home);
    let server = Server::start(&home, Some(&key), Some(&store.db));
    let reader = rusqlite::Connection::open_with_flags(
        &store.db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .expect("held reader");
    reader.execute_batch("BEGIN").expect("begin held read");
    assert_eq!(
        reader
            .query_row("SELECT COUNT(*) FROM change_decisions", [], |row| row
                .get::<_, i64>(0))
            .expect("read before approval"),
        0
    );

    let before = names_in(&home.thruline_home());
    let results = approve_at_once(&store, &[&server], 2);
    assert_exactly_one_recorded(&results, &store, &home, &before, "held reader");
    let wal = store.db.with_extension("db-wal");
    assert!(
        std::fs::metadata(&wal).expect("accepted WAL").len() > 32,
        "the accepted write must still be waiting in WAL"
    );

    assert_later_attempts_append_nothing_after(&store, &server, &home, || {
        drop(reader);
        let main_before = sha256_of(&store.db);
        let checkpoint = rusqlite::Connection::open_with_flags(
            &store.db,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )
        .expect("checkpoint connection");
        let busy: i64 = checkpoint
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))
            .expect("settle the accepted WAL frames");
        assert_eq!(busy, 0, "accepted WAL frames could not be checkpointed");
        assert_ne!(
            sha256_of(&store.db),
            main_before,
            "the accepted WAL frames did not reach the main file"
        );
    });
}

#[test]
fn two_paid_approvals_at_once_to_one_server_record_one() {
    for round in 0..race_rounds(12) {
        let home = Home::new();
        let store = Approvable::new(&home);
        let key = place_pro(&home);
        let server = Server::start(&home, Some(&key), Some(&store.db));
        assert!(!home.trial().exists());
        let before = names_in(&home.thruline_home());
        let results = approve_at_once(&store, &[&server], 2);
        assert_exactly_one_recorded(&results, &store, &home, &before, &format!("round {round}"));
        assert_later_attempts_append_nothing(&store, &server, &home);
    }
}

#[test]
fn eight_paid_approvals_at_once_to_one_server_record_one() {
    for round in 0..race_rounds(6) {
        let home = Home::new();
        let store = Approvable::new(&home);
        let key = place_pro(&home);
        let server = Server::start(&home, Some(&key), Some(&store.db));
        let before = names_in(&home.thruline_home());
        let results = approve_at_once(&store, &[&server], 8);
        assert_eq!(results.len(), 8);
        assert_exactly_one_recorded(&results, &store, &home, &before, &format!("round {round}"));
        assert_later_attempts_append_nothing(&store, &server, &home);
    }
}

#[test]
fn paid_approvals_at_once_across_two_servers_on_one_home_record_one() {
    // Two `thruline ui` processes on one home and one record, two people at
    // each: the threads of one process and the processes of one home both race.
    for round in 0..race_rounds(6) {
        let home = Home::new();
        let store = Approvable::new(&home);
        let key = place_pro(&home);
        let first = Server::start(&home, Some(&key), Some(&store.db));
        let second = Server::start(&home, Some(&key), Some(&store.db));
        let before = names_in(&home.thruline_home());
        let results = approve_at_once(&store, &[&first, &second], 2);
        assert_eq!(results.len(), 4);
        assert_exactly_one_recorded(&results, &store, &home, &before, &format!("round {round}"));
        assert_later_attempts_append_nothing(&store, &second, &home);
    }
}

#[cfg(unix)]
#[test]
fn paid_approval_does_not_need_to_write_to_the_thruline_home() {
    use std::os::unix::fs::PermissionsExt;
    let home = Home::new();
    let store = Approvable::new(&home);
    let key = place_pro(&home);
    let server = Server::start(&home, Some(&key), Some(&store.db));
    let locked = |on: bool| {
        let mode = if on { 0o500 } else { 0o700 };
        std::fs::set_permissions(home.thruline_home(), std::fs::Permissions::from_mode(mode))
            .expect("chmod the ThruLine folder");
    };
    locked(true);
    let probe = home.thruline_home().join("probe");
    if std::fs::write(&probe, b"x").is_ok() {
        let _ = std::fs::remove_file(&probe);
        locked(false);
        eprintln!("NOT RUN: this user can write into a folder made read-only");
        return;
    }
    let before = names_in(&home.thruline_home());
    let results = approve_at_once(&store, &[&server], 4);
    locked(false);
    assert_exactly_one_recorded(&results, &store, &home, &before, "unwritable home");
    assert_later_attempts_append_nothing(&store, &server, &home);
}

/* ══ 13. paid refusal and obsolete files ═══════════════════════════════ */

/// The paid renewal path survives C20. A signed expiry already includes the
/// issuer's renewal grace, and an expired licence points to renewal and a new
/// file rather than opening approving.
const RENEW: &str = "Go to the licence page on ThruLine's website with your licence key, renew there if your subscription has ended, then download the new licence and install it.";

fn assert_off_for_expiry(doc: &serde_json::Value, what: &str) {
    assert_eq!(doc["status"], "expired", "{what}: {doc}");
    assert_eq!(
        doc["sentence"], "This licence expired on 1 Jan 2020.",
        "{what}: {doc}"
    );
    assert_eq!(doc["headline"], "licence", "{what}: {doc}");
    assert!(doc.get("trial").is_none(), "{what}: {doc}");
    let approve = &doc["entitlements"]["approve_version"];
    assert_eq!(approve["allowed"], false, "{what}: {doc}");
    assert_eq!(approve["status"], "expired", "{what}: {doc}");
    let reason = approve["reason"].as_str().expect("refusal reason");
    assert!(reason.contains("Pro licence"), "{what}: {reason}");
    assert!(
        reason.contains("This licence expired on 1 Jan 2020."),
        "{what}: {reason}"
    );
    assert_eq!(approve["enable"], RENEW, "{what}: {doc}");
    assert!(approve.get("by").is_none(), "{what}: {doc}");
    assert!(approve.get("sentence").is_none(), "{what}: {doc}");
}

#[test]
fn an_expired_paid_licence_refuses_with_its_renew_path_and_writes_no_trial_file() {
    let issuer = Issuer::new();
    let key = issuer.public_hex();
    let home = Home::new();
    let store = Approvable::new(&home);
    std::fs::write(home.installed(), issuer.expired()).expect("place");
    assert!(!home.trial().exists());

    assert_off_for_expiry(&home.status(Some(&key)), "the terminal");
    let server = Server::start(&home, Some(&key), Some(&store.db));
    assert_off_for_expiry(&server.licence(), "the screen's document");

    let text = store.assert_refused(&server, "expired", "an expired licence");
    assert!(
        text.contains("This licence expired on 1 Jan 2020."),
        "{text}"
    );
    assert!(text.contains(RENEW), "{text}");
    assert_eq!(store.decision_rows(), 0);
    assert!(!store.decision_file().exists());
    assert!(!home.trial().exists(), "refusal created trial.json");
    store.assert_refused(&server, "expired", "asked a second time");
    assert!(!home.trial().exists());
    assert_off_for_expiry(&home.status(Some(&key)), "after two refusals");
}

#[test]
fn an_old_running_trial_file_does_not_change_an_expired_paid_licence() {
    let issuer = Issuer::new();
    let key = issuer.public_hex();
    let home = Home::new();
    let store = Approvable::new(&home);
    std::fs::write(home.installed(), issuer.expired()).expect("place");
    let old_bytes =
        br#"{"format":1,"started_on":"2099-01-01","created_at":"2026-10-07T00:00:00Z"}"#;
    std::fs::write(home.trial(), old_bytes).expect("place old file");
    let server = Server::start(&home, Some(&key), Some(&store.db));
    assert_off_for_expiry(&home.status(Some(&key)), "expired with old trial.json");
    assert_off_for_expiry(&server.licence(), "served with old trial.json");
    let text = store.assert_refused(&server, "expired", "expired with old trial.json");
    assert!(text.contains(RENEW), "{text}");
    assert_eq!(store.decision_rows(), 0);
    assert!(!store.decision_file().exists());
    assert_eq!(std::fs::read(home.trial()).expect("old file"), old_bytes);
}

#[test]
fn a_file_that_is_not_a_usable_licence_cannot_approve() {
    let issuer = Issuer::new();
    let key = issuer.public_hex();
    for (what, bytes, slug) in [
        ("not a licence", "hello\n".to_string(), "not_a_licence"),
        (
            "for another product",
            issuer.for_product("otherthing"),
            "wrong_product",
        ),
    ] {
        let home = Home::new();
        let store = Approvable::new(&home);
        std::fs::write(home.installed(), bytes).expect("place");
        let doc = home.status(Some(&key));
        assert_eq!(doc["status"], slug, "{what}: {doc}");
        let approve = &doc["entitlements"]["approve_version"];
        assert_eq!(approve["allowed"], false, "{what}: {doc}");
        assert_eq!(approve["status"], slug, "{what}: {doc}");
        assert!(approve["reason"]
            .as_str()
            .unwrap_or("")
            .contains("Pro licence"));
        assert!(approve.get("by").is_none() && approve.get("sentence").is_none());
        assert!(doc.get("trial").is_none(), "{what}: {doc}");
        let server = Server::start(&home, Some(&key), Some(&store.db));
        store.assert_refused(&server, slug, what);
        assert!(!home.trial().exists(), "{what}: refusal wrote trial.json");
        assert_eq!(store.decision_rows(), 0, "{what}");
    }
}

#[test]
fn entitlement_documents_have_no_alternate_approval_source() {
    let issuer = Issuer::new();
    let key = issuer.public_hex();
    for (what, licence, allowed) in [
        ("Free", None, false),
        ("valid Pro", Some(issuer.valid()), true),
        ("expired Pro", Some(issuer.expired()), false),
    ] {
        let home = Home::new();
        if let Some(bytes) = licence {
            std::fs::write(home.installed(), bytes).expect("place");
        }
        let doc = home.status(Some(&key));
        let approve = &doc["entitlements"]["approve_version"];
        assert_eq!(approve["allowed"], allowed, "{what}: {doc}");
        assert!(approve.get("by").is_none(), "{what}: {doc}");
        assert!(approve.get("sentence").is_none(), "{what}: {doc}");
        assert!(doc.get("trial").is_none(), "{what}: {doc}");
    }
}
