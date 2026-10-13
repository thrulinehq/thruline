//! C30 R-LEDGER — `POST /api/hq/projects/<id>/objections` (write-address row
//! 25), through the shipping binary, and the `objections[]` it adds to a claim
//! of `GET /api/hq/projects/<id>/threads/<tid>`.
//!
//! The module's own tests (`crates/core/tests/statement_objections.rs`) prove
//! what an objection is; this proves the served route keeps the contract it is
//! written down with in CLAUDE.md: same-origin, a body cap sized from the
//! 2,000-character limit, `409 too_long` naming `reason`, an unknown field
//! (a digest, a sentence, a verdict) refused, the sentence and its digest read
//! by the SERVER, every refusal appending nothing, the verdict beside the
//! objection unchanged, a cached thread view that cannot outlive an objection,
//! no way to take one back, and the route free while approving is the one paid
//! surface.
//!
//! A real store under a temporary HOME with synthetic recordings; no real
//! store is opened.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;
use thruline_core::capture_hold::{self, Act, Hold};

const CANARY: &str = "ghp_TLCANARYaaZZ11bbYY22ccXX33ddWW44e";

struct Machine {
    dir: tempfile::TempDir,
}

impl Machine {
    fn new() -> Machine {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("h/.thruline")).expect("home");
        std::fs::create_dir_all(dir.path().join("repo/.git")).expect("repo");
        Machine { dir }
    }
    fn home(&self) -> PathBuf {
        self.dir.path().join("h")
    }
    fn db(&self) -> PathBuf {
        self.home().join(".thruline/ledger.db")
    }
    fn repo(&self) -> String {
        self.dir.path().join("repo").display().to_string()
    }
    fn project_dir(&self) -> PathBuf {
        let mangled: String = self
            .repo()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        self.home().join(".claude/projects").join(mangled)
    }
    fn cmd(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_thruline"));
        cmd.args(args)
            .env("HOME", self.home())
            .env("THRULINE_HOME", self.home().join(".thruline"))
            .env("USER", "robin")
            .env_remove("THRULINE_PAID_GATE_DEV")
            .env_remove("THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS")
            .env_remove("THRULINE_LICENCE_DEV_PUBLIC_KEY")
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .current_dir(self.dir.path().join("repo"));
        cmd
    }
    /// One recording that edits `file`: a prompt, an edit, a result, a claim.
    fn record(&self, session: &str, prompt: &str, day: &str, file: &str) {
        let cwd = self.repo();
        std::fs::create_dir_all(self.project_dir()).expect("dir");
        let line = |n: u32, kind: &str, message: Value| {
            serde_json::json!({
                "type": kind, "uuid": format!("{session}-{n}"), "parentUuid": null,
                "isSidechain": false, "sessionId": session,
                "timestamp": format!("{day}T09:0{n}:00.000Z"), "cwd": cwd, "message": message,
            })
            .to_string()
        };
        let lines = [
            line(
                1,
                "user",
                serde_json::json!({"role": "user", "content": prompt}),
            ),
            line(
                2,
                "assistant",
                serde_json::json!({"role": "assistant", "content": [
                {"type": "tool_use", "id": format!("{session}-t1"), "name": "Edit",
                 "input": {"file_path": format!("{cwd}/{file}"), "old_string": "a", "new_string": "b"}}]}),
            ),
            line(
                3,
                "user",
                serde_json::json!({"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": format!("{session}-t1"), "content": "ok", "is_error": false}]}),
            ),
            line(
                4,
                "assistant",
                serde_json::json!({"role": "assistant", "content": [
                {"type": "text", "text": "Done. I updated `src/a.rs`."}]}),
            ),
        ];
        std::fs::write(
            self.project_dir().join(format!("{session}.jsonl")),
            lines.join("\n") + "\n",
        )
        .expect("recording");
    }
    fn ingest(&self) {
        let status = self
            .cmd(&["ingest", "--apply", "--source", "claude-code"])
            .arg(self.project_dir())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("ingest");
        assert!(status.success());
    }
    /// Row counts of the table, read the way a person would, from outside.
    fn rows(&self) -> i64 {
        let ledger = thruline_core::ledger::Ledger::open(self.db()).expect("open");
        ledger
            .conn()
            .query_row("SELECT COUNT(*) FROM statement_objections", [], |r| {
                r.get(0)
            })
            .expect("count")
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
                return Server { child, port };
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("`thruline ui` printed no address");
    }

    fn send(&self, head: &str) -> (u16, Value) {
        let mut s = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        s.set_read_timeout(Some(Duration::from_secs(60)))
            .expect("timeout");
        // A refused oversized body may be cut off mid-write; the answer is
        // already on its way.
        let _ = s.write_all(head.as_bytes());
        let mut raw = Vec::new();
        let _ = s.read_to_end(&mut raw);
        let text = String::from_utf8_lossy(&raw);
        let status: u16 = text.get(9..12).and_then(|s| s.parse().ok()).unwrap_or(0);
        let body = text.split_once("\r\n\r\n").map(|x| x.1).unwrap_or("");
        (status, serde_json::from_str(body).unwrap_or(Value::Null))
    }

    fn get(&self, path: &str) -> (u16, Value) {
        self.send(&format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            self.port
        ))
    }

    fn origin(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// A POST with an `Origin` line of the caller's choosing.
    fn post_from(&self, origin: Option<&str>, path: &str, body: &str) -> (u16, Value) {
        let origin_line = origin
            .map(|o| format!("Origin: {o}\r\n"))
            .unwrap_or_default();
        self.send(&format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n{origin_line}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.port,
            body.len()
        ))
    }

    /// A POST from HQ's own page.
    fn post(&self, path: &str, body: &Value) -> (u16, Value) {
        self.post_from(Some(&self.origin()), path, &body.to_string())
    }

    fn wait_for_complete_list(&self) -> Value {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let (_, list) = self.get("/api/hq/projects");
            if list["complete"] == true {
                return list;
            }
            assert!(
                Instant::now() < deadline,
                "the list never completed: {list}"
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

struct Fixture {
    m: Machine,
    server: Server,
    project: String,
    /// The two-session thread, and one it is not.
    thread: Value,
    other: Value,
}

fn fixture() -> Fixture {
    let m = Machine::new();
    m.record(
        "s1",
        "Fix the parser in src/a.rs.",
        "2026-10-01",
        "src/a.rs",
    );
    m.record(
        "s2",
        "Add a parser test for empty input.",
        "2026-10-02",
        "src/a.rs",
    );
    m.record("s3", "Write the release notes.", "2026-10-05", "NOTES.md");
    m.ingest();
    let server = Server::start(&m);
    let list = server.wait_for_complete_list();
    let project = list["projects"][0]["id"].as_str().expect("id").to_string();
    let (_, view) = server.get(&format!("/api/hq/projects/{project}"));
    let threads = view["threads"].as_array().expect("threads");
    let thread = threads
        .iter()
        .find(|t| t["sessions"] == 2)
        .expect("the two-session thread")
        .clone();
    let other = threads
        .iter()
        .find(|t| t["sessions"] == 1)
        .expect("the one-session thread")
        .clone();
    Fixture {
        m,
        server,
        project,
        thread,
        other,
    }
}

impl Fixture {
    fn path(&self) -> String {
        format!("/api/hq/projects/{}/objections", self.project)
    }
    fn thread_path(&self, thread: &Value) -> String {
        format!(
            "/api/hq/projects/{}/threads/{}",
            self.project,
            thread["id"].as_str().expect("tid")
        )
    }
    fn thread_view(&self, thread: &Value) -> Value {
        let (code, view) = self.server.get(&self.thread_path(thread));
        assert_eq!(code, 200, "{view}");
        view
    }
    /// The first statement of a thread: its id and its sentence.
    fn first_statement(&self, thread: &Value) -> (String, String) {
        let view = self.thread_view(thread);
        let claim = &view["claims"][0];
        (
            claim["id"].as_str().expect("claim id").to_string(),
            claim["text"].as_str().expect("claim text").to_string(),
        )
    }
    fn statements(&self) -> Value {
        let (code, doc) = self
            .server
            .get(&format!("/api/hq/projects/{}/statements", self.project));
        assert_eq!(code, 200, "{doc}");
        doc
    }
    /// The one row of the table, read the way a person would, from outside.
    fn row(&self, id: &str) -> (String, String, String, String, String) {
        let ledger = thruline_core::ledger::Ledger::open(self.m.db()).expect("open");
        ledger
            .conn()
            .query_row(
                "SELECT act, statement_id, statement_sha256, actor, reason
                   FROM statement_objections WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .expect("the row")
    }
}

/// `view` with every `objections` key taken out of every claim.
fn without_objections(mut view: Value) -> Value {
    if let Some(claims) = view["claims"].as_array_mut() {
        for claim in claims {
            if let Some(map) = claim.as_object_mut() {
                map.remove("objections");
            }
        }
    }
    view
}

fn sha256_hex(text: &str) -> String {
    thruline_core::chain::sha256_hex(text.as_bytes())
}

/* ── the contract ──────────────────────────────────────────────────────── */

#[test]
fn it_must_come_from_the_page_and_say_exactly_what_it_means() {
    let f = fixture();
    let path = f.path();
    let (id, _) = f.first_statement(&f.thread);
    let good = serde_json::json!({"statement": id, "reason": "The test never ran."}).to_string();

    // No Origin is the route's own refusal. A foreign Origin or a lookalike
    // port never reaches the route: the server answers 403 to anything but
    // itself. Either way nothing is kept.
    let (code, body) = f.server.post_from(None, &path, &good);
    assert_eq!(code, 403, "{body}");
    assert_eq!(body["error"], "same_origin_required");
    for origin in [
        "https://example.com",
        "http://127.0.0.1:1",
        "http://localhost:1",
    ] {
        let (code, _) = f.server.post_from(Some(origin), &path, &good);
        assert_eq!(code, 403, "{origin}");
    }
    assert_eq!(f.m.rows(), 0);

    // A different method is not an act, and there is no way to take one back.
    let (code, body) = f.server.get(&path);
    assert_eq!(code, 405, "{body}");

    // Anything the client might try to supply about the sentence, its digest,
    // the person or the verdict is refused: the server reads the sentence
    // itself and the result is the engine's.
    for extra in [
        r#""statement_sha256":"0000000000000000000000000000000000000000000000000000000000000000""#,
        r#""sentence":"Done.""#,
        r#""verdict":"contradicted""#,
        r#""actor":"someone else""#,
        r#""act":"withdrawn""#,
        r#""withdraws":"obj_1""#,
        r#""event_id":"e1""#,
        r#""change_id":"c1""#,
    ] {
        let body = format!(r#"{{"statement":"{id}","reason":"wrong",{extra}}}"#);
        let (code, answer) = f.server.post_from(Some(&f.server.origin()), &path, &body);
        assert_eq!(code, 400, "{extra} {answer}");
        assert_eq!(answer["error"], "malformed_request");
    }
    // Shape: not JSON, nothing, a reason alone, a statement alone, wrong types.
    for (bad, error) in [
        ("not json", "malformed_request"),
        (r#"{}"#, "malformed_request"),
        (r#"{"reason":"wrong"}"#, "malformed_request"),
        (r#"{"statement":"x#0"}"#, "malformed_request"),
        (r#"{"statement":7,"reason":"wrong"}"#, "malformed_request"),
        (r#"{"statement":"x#0","reason":null}"#, "malformed_request"),
    ] {
        let (code, body) = f.server.post_from(Some(&f.server.origin()), &path, bad);
        assert_eq!(code, 400, "{bad} {body}");
        assert_eq!(body["error"], error, "{bad}");
    }
    // A reason with nothing in it.
    for reason in ["", "   ", "\n\t "] {
        let (code, body) = f.server.post(
            &path,
            &serde_json::json!({"statement": id, "reason": reason}),
        );
        assert_eq!(code, 400, "{reason:?} {body}");
        assert_eq!(body["error"], "reason_required");
    }
    assert_eq!(f.m.rows(), 0, "every refusal so far appended nothing");

    // Unknown ids: a project, and statements this project does not serve —
    // made-up, malformed, an ordinal past the last, and an event that holds no
    // statement at all.
    let (code, body) = f.server.post(
        "/api/hq/projects/nothing/objections",
        &serde_json::json!({"statement": id, "reason": "wrong"}),
    );
    assert_eq!(
        (code, &body["error"]),
        (404, &Value::from("no_such_project"))
    );
    let (event, n) = id.rsplit_once('#').expect("event#n");
    let n: usize = n.parse().expect("ordinal");
    for bad in [
        "nothing#0".to_string(),
        "nothing".to_string(),
        "#0".to_string(),
        format!("{event}#"),
        format!("{event}#-1"),
        format!("{event}#0{n}"),
        format!("{event}#{}", n + 1000),
    ] {
        let (code, body) = f.server.post(
            &path,
            &serde_json::json!({"statement": bad, "reason": "wrong"}),
        );
        assert_eq!(
            (code, &body["error"]),
            (404, &Value::from("no_such_statement")),
            "{bad}"
        );
    }
    assert_eq!(f.m.rows(), 0);
}

#[test]
fn a_reason_over_the_limit_is_refused_by_name_and_a_larger_body_by_size() {
    let f = fixture();
    let path = f.path();
    let (id, _) = f.first_statement(&f.thread);

    let (code, body) = f.server.post(
        &path,
        &serde_json::json!({"statement": id, "reason": "x".repeat(2001)}),
    );
    assert_eq!(code, 409, "{body}");
    assert_eq!(body["error"], "too_long");
    assert_eq!(body["field"], "reason");
    assert_eq!(body["limit"], 2000);
    assert_eq!(f.m.rows(), 0);

    // Exactly 2,000 characters (two-byte ones: the limit is characters) is kept.
    let (code, body) = f.server.post(
        &path,
        &serde_json::json!({"statement": id, "reason": "é".repeat(2000)}),
    );
    assert_eq!(code, 200, "{body}");

    // A body bigger than 2,000 characters could make, even all written the
    // long way, is refused on size alone.
    let huge = format!(
        r#"{{"statement":"{id}","reason":"{}"}}"#,
        "x".repeat(2000 * 12 + 5000)
    );
    let (code, body) = f.server.post_from(Some(&f.server.origin()), &path, &huge);
    assert_eq!(
        code, 413,
        "refused before the body is read; the connection may be cut: {body}"
    );
    assert_eq!(f.m.rows(), 1);
}

#[test]
fn an_objection_reply_obeys_exclude_then_include_without_rewriting_its_reason() {
    let f = fixture();
    let (statement, _) = f.first_statement(&f.thread);
    let reason = "Inspect src/private.rs";
    let body = serde_json::json!({"statement":statement,"reason":reason});
    let (code, plain) = f.server.post(&f.path(), &body);
    assert_eq!(code, 200, "{plain}");
    assert_eq!(plain["objection"]["reason"], reason);

    let folder = f.m.repo();
    let ledger = thruline_core::ledger::Ledger::open(f.m.db()).expect("synthetic ledger");
    let mut hold = Hold {
        project_id: &f.project,
        project_folder: &folder,
        actor: "synthetic person",
        recorded_at: "2026-10-08T00:00:01Z",
    };
    capture_hold::record_for_person(&ledger, Act::Excluded, &hold, Some("src/private.rs"))
        .expect("exclude private path");
    let (code, hidden) = f.server.post(&f.path(), &body);
    assert_eq!(code, 200, "{hidden}");
    assert_eq!(
        hidden["objection"]["reason"],
        thruline_core::capture_exclusion::marker("src/private.rs")
    );
    assert_eq!(hidden["left_out_by_exclusion"], 1);
    assert!(!hidden.to_string().contains(reason));
    let id = hidden["objection"]["id"].as_str().expect("objection id");
    assert_eq!(f.row(id).4, reason, "the reason remains stored as typed");

    hold.recorded_at = "2026-10-08T00:00:02Z";
    capture_hold::record_for_person(&ledger, Act::Included, &hold, Some("src/private.rs"))
        .expect("include private path");
    let (code, restored) = f.server.post(&f.path(), &body);
    assert_eq!(code, 200, "{restored}");
    assert_eq!(restored["objection"]["reason"], reason);
    assert!(restored.get("left_out_by_exclusion").is_none());
}

#[test]
fn an_objection_is_kept_as_typed_with_the_digest_the_server_computed() {
    let f = fixture();
    let (id, sentence) = f.first_statement(&f.thread);
    let reason = format!("  The test never ran.\n{CANARY}  ");
    let (code, body) = f.server.post(
        &f.path(),
        &serde_json::json!({"statement": id, "reason": reason}),
    );
    assert_eq!(code, 200, "{body}");
    assert_eq!(body["schema_version"], 1);
    assert_eq!(body["statement"], id.as_str());
    assert_eq!(
        body["message"],
        "Your objection is kept beside this statement. It does not change what ThruLine \
         says about it."
    );
    let kept = &body["objection"];
    assert_eq!(kept["actor"], "robin", "the account ThruLine runs as");
    assert_eq!(kept["matches_sentence"], true);
    assert!(kept["at"].as_str().is_some_and(|at| at.ends_with('Z')));
    assert!(
        !kept["reason"].as_str().expect("reason").contains(CANARY),
        "the answer is masked: {kept}"
    );
    let mut keys: Vec<&str> = kept
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["actor", "at", "id", "matches_sentence", "reason"]);

    // The table keeps the words as typed and the digest of the sentence the
    // server read, never one the client sent.
    let (act, statement, digest, actor, stored) = f.row(kept["id"].as_str().expect("id"));
    assert_eq!(act, "objected");
    assert_eq!(statement, id);
    assert_eq!(digest, sha256_hex(&sentence));
    assert_eq!(actor, "robin");
    assert_eq!(stored, reason, "kept exactly as typed");
    assert_eq!(f.m.rows(), 1);

    // Served masked in the thread view as well.
    let view = f.thread_view(&f.thread).to_string();
    assert!(!view.contains(CANARY), "the thread view served the secret");
}

#[test]
fn the_thread_view_shows_it_beside_the_unchanged_verdict_and_a_cached_view_never_hides_it() {
    let f = fixture();
    let (id, _) = f.first_statement(&f.thread);

    // Warm every cache the objection could be hiding behind.
    let before = f.thread_view(&f.thread);
    let again = f.thread_view(&f.thread);
    assert_eq!(before, again);
    let other_before = f.thread_view(&f.other);
    let statements_before = f.statements();
    let (_, project_before) = f.server.get(&format!("/api/hq/projects/{}", f.project));
    assert!(!before.to_string().contains("objections"));

    let (code, kept) = f.server.post(
        &f.path(),
        &serde_json::json!({"statement": id, "reason": "The test never ran."}),
    );
    assert_eq!(code, 200, "{kept}");

    let after = f.thread_view(&f.thread);
    let claim = after["claims"]
        .as_array()
        .expect("claims")
        .iter()
        .find(|c| c["id"] == id.as_str())
        .expect("the claim");
    let served = claim["objections"].as_array().expect("objections[]");
    assert_eq!(served.len(), 1, "the warmed thread view showed the new row");
    assert_eq!(served[0]["id"], kept["objection"]["id"]);
    assert_eq!(served[0]["reason"], "The test never ran.");
    assert_eq!(served[0]["actor"], "robin");
    assert_eq!(served[0]["matches_sentence"], true);

    // Nothing else moved: the verdict and its basis on every claim, the counts,
    // the other threads, the statements route and the project view.
    assert_eq!(without_objections(after.clone()), before);
    assert_eq!(f.thread_view(&f.other), other_before);
    assert_eq!(f.statements(), statements_before);
    let (_, project_after) = f.server.get(&format!("/api/hq/projects/{}", f.project));
    assert_eq!(project_after, project_before);

    // A second objection shows too, on the same warm path, oldest first.
    let (code, second) = f.server.post(
        &f.path(),
        &serde_json::json!({"statement": id, "reason": "Nor did the build."}),
    );
    assert_eq!(code, 200, "{second}");
    let later = f.thread_view(&f.thread);
    let served = later["claims"]
        .as_array()
        .expect("claims")
        .iter()
        .find(|c| c["id"] == id.as_str())
        .expect("the claim")["objections"]
        .as_array()
        .expect("objections[]")
        .clone();
    assert_eq!(served.len(), 2);
    assert_eq!(served[1]["reason"], "Nor did the build.");
    assert_eq!(without_objections(later), before);
    assert_eq!(f.m.rows(), 2);
}

#[test]
fn there_is_no_way_to_take_one_back() {
    let f = fixture();
    let (id, _) = f.first_statement(&f.thread);
    let (code, kept) = f.server.post(
        &f.path(),
        &serde_json::json!({"statement": id, "reason": "Wrong."}),
    );
    assert_eq!(code, 200, "{kept}");
    let objection = kept["objection"]["id"].as_str().expect("id").to_string();

    // C34 ruling Q16: the table can say `withdrawn`; no route does.
    for path in [
        format!("{}/withdraw", f.path()),
        format!("{}/{objection}", f.path()),
        format!("{}/{objection}/withdraw", f.path()),
    ] {
        let (code, body) = f.server.post(&path, &serde_json::json!({}));
        assert_eq!(code, 404, "{path} {body}");
        assert_eq!(body["error"], "no_such_route");
    }
    for method in ["DELETE", "PUT", "PATCH"] {
        let (code, body) = f.server.send(&format!(
            "{method} {} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nOrigin: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            f.path(),
            f.server.port,
            f.server.origin()
        ));
        assert_eq!(code, 405, "{method} {body}");
    }
    let (act, ..) = f.row(&objection);
    assert_eq!(act, "objected");
    assert_eq!(f.m.rows(), 1, "nothing was taken back or added");
}

#[test]
fn the_route_is_free_and_the_one_paid_surface_is_untouched() {
    // The server starts as it ships — paid gate on, no licence — and the act is
    // still kept: only approving a version asks for a licence.
    let f = fixture();
    assert!(
        !f.m.home().join(".thruline/licence.json").exists(),
        "a fresh home holds no licence"
    );
    let (id, _) = f.first_statement(&f.thread);
    let (code, body) = f.server.post(
        &f.path(),
        &serde_json::json!({"statement": id, "reason": "Wrong."}),
    );
    assert_eq!(code, 200, "{body}");
    assert_ne!(body["error"], "licence_required");
}
