//! C30 R-PREFS — `GET`/`POST /api/preferences` against the shipping binary:
//! `thruline ui` with a temporary HOME, so the records folder (and the
//! `preferences.json` beside the ledger) is a temp dir, never `~/.thruline`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;
use sha2::{Digest, Sha256};

struct Machine {
    dir: tempfile::TempDir,
}

impl Machine {
    fn new() -> Machine {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("h/.thruline")).expect("home");
        std::fs::create_dir_all(dir.path().join("repo/.git")).expect("repo");
        let m = Machine { dir };
        m.seed();
        m
    }
    fn home(&self) -> PathBuf {
        self.dir.path().join("h")
    }
    fn records(&self) -> PathBuf {
        self.home().join(".thruline")
    }
    fn prefs(&self) -> PathBuf {
        self.records().join("preferences.json")
    }
    fn cmd(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_thruline"));
        cmd.args(args)
            .env("HOME", self.home())
            .env("THRULINE_HOME", self.records())
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .current_dir(self.dir.path().join("repo"));
        cmd
    }
    /// One recording, ingested, so the server opens a real ledger.
    fn seed(&self) {
        let cwd = self.dir.path().join("repo").display().to_string();
        let mangled: String = cwd
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let project = self.home().join(".claude/projects").join(mangled);
        std::fs::create_dir_all(&project).expect("dir");
        let line = serde_json::json!({
            "type": "user", "uuid": "s1-1", "parentUuid": null, "isSidechain": false,
            "sessionId": "s1", "timestamp": "2026-10-01T09:01:00.000Z", "cwd": cwd,
            "message": {"role": "user", "content": "Fix the parser."},
        });
        std::fs::write(project.join("s1.jsonl"), format!("{line}\n")).expect("recording");
        let status = self
            .cmd(&["ingest", "--apply", "--source", "claude-code"])
            .arg(&project)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("ingest");
        assert!(status.success());
        assert!(self.records().join("ledger.db").is_file(), "no ledger");
    }
    /// The ledger's bytes: the database and its write-ahead log (the shared
    /// memory index is excluded; readers touch it by design).
    fn ledger_digest(&self) -> String {
        let mut h = Sha256::new();
        for name in ["ledger.db", "ledger.db-wal"] {
            h.update(name.as_bytes());
            h.update(std::fs::read(self.records().join(name)).unwrap_or_default());
        }
        let mut s = String::with_capacity(64);
        for b in h.finalize() {
            use std::fmt::Write as _;
            let _ = write!(s, "{b:02x}");
        }
        s
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
    fn start(m: &Machine) -> Server {
        let mut child = m
            .cmd(&["ui", "--no-open", "--port", "0"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("thruline ui");
        let stdout = child.stdout.take().expect("stdout");
        for line in BufReader::new(stdout).lines() {
            let line = line.expect("line");
            if let Some(rest) = line.trim().strip_prefix("ThruLine is at http://127.0.0.1:") {
                let port = rest
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .expect("port");
                let s = Server { child, port };
                s.settle();
                return s;
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("`thruline ui` printed no address");
    }

    /// Wait for the start-up warm to finish, so the ledger's bytes are still
    /// from here on unless a request changes them.
    fn settle(&self) {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let (_, list) = self.get("/api/hq/projects");
            if list["complete"] == true {
                return;
            }
            assert!(Instant::now() < deadline, "the list never completed");
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn origin(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn send(&self, raw: &[u8]) -> (u16, Value) {
        let mut s = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        s.set_read_timeout(Some(Duration::from_secs(60)))
            .expect("timeout");
        // A refused oversized body may be cut off mid-write; the answer is
        // what is checked.
        let _ = s.write_all(raw);
        let mut out = Vec::new();
        let _ = s.read_to_end(&mut out);
        let text = String::from_utf8_lossy(&out);
        let status: u16 = text.get(9..12).unwrap_or("0").parse().unwrap_or(0);
        let body = text.split_once("\r\n\r\n").map(|x| x.1).unwrap_or("");
        (status, serde_json::from_str(body).unwrap_or(Value::Null))
    }

    fn get(&self, path: &str) -> (u16, Value) {
        self.send(
            format!(
                "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
                self.port
            )
            .as_bytes(),
        )
    }

    /// A POST with `origin` (none when `None`) and a JSON body.
    fn post_as(&self, origin: Option<&str>, content_type: &str, body: &str) -> (u16, Value) {
        let o = origin
            .map(|o| format!("Origin: {o}\r\n"))
            .unwrap_or_default();
        self.send(
            format!(
                "POST /api/preferences HTTP/1.1\r\nHost: 127.0.0.1:{p}\r\n{o}Content-Type: {content_type}\r\nContent-Length: {n}\r\nConnection: close\r\n\r\n{body}",
                p = self.port,
                n = body.len()
            )
            .as_bytes(),
        )
    }

    fn post(&self, body: &str) -> (u16, Value) {
        self.post_as(Some(&self.origin()), "application/json", body)
    }
}

fn defaults() -> Value {
    serde_json::json!({
        "face": "plain",
        "density": "comfortable",
        "motion": "system",
        "colour_vision": "standard",
        "keys": "on",
        "masked_copy": true,
        "notifications": {"ready": false, "not_backed": false, "agent_waiting": false, "recording_stopped": false},
        "weekly_summary": false,
    })
}

#[test]
fn get_serves_every_key_at_its_default_and_writes_nothing() {
    let m = Machine::new();
    let s = Server::start(&m);
    let before = m.ledger_digest();
    let (code, doc) = s.get("/api/preferences");
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["schema_version"], 1);
    assert_eq!(doc["preferences"], defaults());
    assert_eq!(doc["defaults"], defaults());
    assert_eq!(doc["file"], "absent");
    assert_eq!(doc["problems"], serde_json::json!([]));
    assert_eq!(doc["unknown_keys"], serde_json::json!([]));
    assert_eq!(doc["path"], m.prefs().display().to_string());
    assert!(!m.prefs().exists(), "a read wrote the file");
    // A query, or another method, is refused.
    assert_eq!(s.get("/api/preferences?x=1").0, 400);
    let (code, _) = s.send(
        format!(
            "DELETE /api/preferences HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nOrigin: {}\r\nConnection: close\r\n\r\n",
            s.port,
            s.origin()
        )
        .as_bytes(),
    );
    assert_eq!(code, 405);
    assert_eq!(m.ledger_digest(), before, "a read changed the ledger");
}

#[test]
fn post_round_trips_and_keeps_unknown_keys_without_serving_them() {
    let m = Machine::new();
    let s = Server::start(&m);
    let before = m.ledger_digest();

    let (code, doc) =
        s.post(r#"{"face":"technical","keys":"mod","notifications":{"not_backed":true}}"#);
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["preferences"]["face"], "technical");
    assert_eq!(doc["preferences"]["keys"], "mod");
    assert_eq!(
        doc["preferences"]["notifications"],
        serde_json::json!({"ready": false, "not_backed": true, "agent_waiting": false, "recording_stopped": false})
    );
    assert_eq!(doc["preferences"]["density"], "comfortable");
    assert_eq!(doc["file"], "read");

    // A second POST changes only what it names.
    let (code, doc) = s.post(
        r#"{"density":"compact","motion":"reduce","colour_vision":"proven_teal","masked_copy":false,"weekly_summary":true}"#,
    );
    assert_eq!(code, 200, "{doc}");
    let (code, read) = s.get("/api/preferences");
    assert_eq!(code, 200);
    assert_eq!(read["preferences"], doc["preferences"]);
    assert_eq!(
        read["preferences"],
        serde_json::json!({
            "face": "technical", "density": "compact", "motion": "reduce",
            "colour_vision": "proven_teal", "keys": "mod", "masked_copy": false,
            "notifications": {"ready": false, "not_backed": true, "agent_waiting": false, "recording_stopped": false},
            "weekly_summary": true,
        })
    );

    // The file on disk is the whole document, schema-stamped.
    let on_disk: Value =
        serde_json::from_slice(&std::fs::read(m.prefs()).expect("file")).expect("json");
    assert_eq!(on_disk["schema_version"], 1);
    assert_eq!(on_disk["preferences"], read["preferences"]);

    // A key from a newer version is reported, not served, and kept.
    let mut with_future = on_disk.clone();
    with_future["preferences"]["sidebar_width"] = serde_json::json!(320);
    std::fs::write(m.prefs(), with_future.to_string()).expect("write");
    let (_, read) = s.get("/api/preferences");
    assert_eq!(read["unknown_keys"], serde_json::json!(["sidebar_width"]));
    assert!(read["preferences"].get("sidebar_width").is_none());
    assert_eq!(s.post(r#"{"face":"plain"}"#).0, 200);
    let on_disk: Value =
        serde_json::from_slice(&std::fs::read(m.prefs()).expect("file")).expect("json");
    assert_eq!(on_disk["preferences"]["sidebar_width"], 320);
    assert_eq!(on_disk["preferences"]["face"], "plain");

    // No temporary file is left behind.
    let leftovers: Vec<_> = std::fs::read_dir(m.records())
        .expect("dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    assert_eq!(
        m.ledger_digest(),
        before,
        "a preference write touched the ledger"
    );
}

#[test]
fn bad_keys_values_origins_and_bodies_are_refused_and_write_nothing() {
    let m = Machine::new();
    let s = Server::start(&m);
    let before = m.ledger_digest();
    // A file is there first, so "writes nothing" is a byte comparison.
    assert_eq!(s.post(r#"{"face":"technical"}"#).0, 200);
    let kept = std::fs::read(m.prefs()).expect("file");

    let cases: &[(&str, &str)] = &[
        (r#"{"theme":"dark"}"#, "theme"),
        (r#"{"face":"technical","theme":"dark"}"#, "theme"),
        (r#"{"face":"loud"}"#, "face"),
        (r#"{"density":1}"#, "density"),
        (r#"{"motion":"full"}"#, "motion"),
        (r#"{"keys":"alt"}"#, "keys"),
        (r#"{"colour_vision":true}"#, "colour_vision"),
        (r#"{"masked_copy":"yes"}"#, "masked_copy"),
        (r#"{"weekly_summary":null}"#, "weekly_summary"),
        (r#"{"notifications":true}"#, "notifications"),
        (r#"{"notifications":{"perm":true}}"#, "notifications.perm"),
        (
            r#"{"notifications":{"not_backed":"no"}}"#,
            "notifications.not_backed",
        ),
    ];
    for (body, key) in cases {
        let (code, doc) = s.post(body);
        assert_eq!(code, 400, "{body}: {doc}");
        assert_eq!(doc["key"], *key, "{body}: {doc}");
        assert_eq!(
            std::fs::read(m.prefs()).expect("file"),
            kept,
            "{body} wrote"
        );
    }
    for body in ["{}", "[]", "not json", r#""face""#] {
        let (code, doc) = s.post(body);
        assert_eq!(code, 400, "{body}: {doc}");
    }
    // Not JSON by content type.
    assert_eq!(
        s.post_as(Some(&s.origin()), "text/plain", r#"{"face":"plain"}"#)
            .0,
        400
    );

    // No Origin: refused by the route. A foreign Origin: refused before it.
    let (code, doc) = s.post_as(None, "application/json", r#"{"face":"plain"}"#);
    assert_eq!(code, 403, "{doc}");
    assert_eq!(doc["error"], "same_origin_required");
    let (code, _) = s.post_as(
        Some("http://evil.example"),
        "application/json",
        r#"{"face":"plain"}"#,
    );
    assert_eq!(code, 403);
    let (code, _) = s.post_as(
        Some("http://127.0.0.1:1"),
        "application/json",
        r#"{"face":"plain"}"#,
    );
    assert_eq!(code, 403);

    // Too large.
    let big = format!(r#"{{"face":"plain","x":"{}"}}"#, "a".repeat(4096));
    let (code, doc) = s.post(&big);
    assert_eq!(code, 413, "{doc}");

    assert_eq!(std::fs::read(m.prefs()).expect("file"), kept);
    assert_eq!(m.ledger_digest(), before);
}

#[cfg(unix)]
#[test]
fn a_symlinked_preferences_file_is_never_written_through() {
    let m = Machine::new();
    let s = Server::start(&m);
    let target = m.dir.path().join("elsewhere.json");
    let original = br#"{"schema_version":1,"preferences":{"face":"technical"}}"#;
    std::fs::write(&target, original).expect("target");
    std::os::unix::fs::symlink(&target, m.prefs()).expect("symlink");

    let (code, doc) = s.get("/api/preferences");
    assert_eq!(code, 200);
    assert_eq!(doc["file"], "not_a_regular_file");
    assert_eq!(doc["preferences"], defaults(), "the link was followed");
    assert_eq!(doc["problems"][0]["problem"], "not_a_regular_file");

    let (code, doc) = s.post(r#"{"face":"plain"}"#);
    // What is on disk first, so a write through the link is named as that.
    assert_eq!(
        std::fs::read(&target).expect("target"),
        original,
        "written through the link"
    );
    assert!(
        std::fs::symlink_metadata(m.prefs())
            .expect("link")
            .file_type()
            .is_symlink(),
        "the link was replaced"
    );
    assert_eq!(code, 409, "{doc}");
    assert_eq!(doc["error"], "preferences_file_not_regular");
}

#[test]
fn a_corrupted_file_reads_as_defaults_with_the_problem_named() {
    let m = Machine::new();
    let s = Server::start(&m);

    std::fs::write(m.prefs(), b"{ this is not json").expect("write");
    let (code, doc) = s.get("/api/preferences");
    assert_eq!(code, 200);
    assert_eq!(doc["preferences"], defaults());
    assert_eq!(doc["file"], "not_json");
    assert_eq!(doc["problems"][0]["problem"], "not_json");

    // One bad value costs only its own key.
    std::fs::write(
        m.prefs(),
        br#"{"schema_version":1,"preferences":{"face":"technical","density":"huge","notifications":{"not_backed":true,"agent_waiting":7}}}"#,
    )
    .expect("write");
    let (_, doc) = s.get("/api/preferences");
    assert_eq!(doc["file"], "read");
    assert_eq!(doc["preferences"]["face"], "technical");
    assert_eq!(doc["preferences"]["density"], "comfortable");
    assert_eq!(
        doc["preferences"]["notifications"],
        serde_json::json!({"ready": false, "not_backed": true, "agent_waiting": false, "recording_stopped": false})
    );
    let problems: Vec<&str> = doc["problems"]
        .as_array()
        .expect("problems")
        .iter()
        .filter_map(|p| p["key"].as_str())
        .collect();
    assert!(problems.contains(&"density"), "{doc}");
    assert!(problems.contains(&"notifications.agent_waiting"), "{doc}");

    // A newer version's file is served as defaults and left alone.
    let newer = br#"{"schema_version":2,"preferences":{"face":"technical"}}"#;
    std::fs::write(m.prefs(), newer).expect("write");
    let (_, doc) = s.get("/api/preferences");
    assert_eq!(doc["file"], "newer_schema");
    assert_eq!(doc["preferences"], defaults());
    assert_eq!(s.post(r#"{"face":"plain"}"#).0, 409);
    assert_eq!(std::fs::read(m.prefs()).expect("file"), newer);

    // A corrupt file is replaced by the next change.
    std::fs::write(m.prefs(), b"garbage").expect("write");
    let (code, doc) = s.post(r#"{"density":"compact"}"#);
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["file"], "read");
    assert_eq!(doc["preferences"]["density"], "compact");
    assert_eq!(doc["preferences"]["face"], "plain");
}
