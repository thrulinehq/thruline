//! C30 R-LEDGER — `POST /api/hq/projects/<id>/capture/pause` and
//! `…/capture/exclusions` (write-address rows 26 and 27), and the `holds` they
//! add to `GET /api/hq/projects/<id>/capture`, through the shipping binary.
//!
//! The module's own tests (`crates/core/tests/capture_hold_standing.rs`,
//! `r_ledger_migration.rs`) prove what a hold is; this proves the served routes
//! keep the contract CLAUDE.md writes down for them: same-origin, bodies that
//! name nothing but the act (and the pattern), the folder taken from the
//! project and never from the browser, a body cap sized from the 256-character
//! limit, `409 too_long` naming `pattern`, every refusal appending nothing, the
//! built-in defaults served as "on by default (not recorded)" and written down
//! only by the first change, the chain still adding up under `thruline verify`
//! and failing it when a row is forged — and the point of the cards: a PAUSE is
//! honoured (C34 Q5: `ingest` and the HQ load leave the paused folder alone, the
//! hook is left out and counted), while an EXCLUSION leaves ingest intact and
//! changes the served view (C34 Q4).
//!
//! A real store under a temporary HOME with synthetic recordings; no real store
//! is opened.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

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
    fn count(&self, table: &str) -> i64 {
        let ledger = thruline_core::ledger::Ledger::open(self.db()).expect("open");
        ledger
            .conn()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .expect("count")
    }
    fn rows(&self) -> i64 {
        self.count("capture_holds")
    }
    /// `(act, project_id, project_folder, pattern, actor, ends, id)` of every
    /// row, oldest first, read the way a person would, from outside.
    #[allow(clippy::type_complexity)]
    fn table(
        &self,
    ) -> Vec<(
        String,
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        String,
    )> {
        let ledger = thruline_core::ledger::Ledger::open(self.db()).expect("open");
        let mut stmt = ledger
            .conn()
            .prepare(
                "SELECT act, project_id, project_folder, pattern, actor, ends, id
                   FROM capture_holds ORDER BY seq",
            )
            .expect("prepare");
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            })
            .expect("query")
            .collect::<Result<Vec<_>, _>>()
            .expect("rows");
        rows
    }
    fn verify(&self) -> (Option<i32>, String) {
        let out = self.cmd(&["verify"]).output().expect("verify");
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        )
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
}

fn fixture() -> Fixture {
    let m = Machine::new();
    m.record(
        "s1",
        "Fix the parser in src/a.rs.",
        "2026-10-01",
        "src/a.rs",
    );
    m.ingest();
    let server = Server::start(&m);
    let list = server.wait_for_complete_list();
    let project = list["projects"][0]["id"].as_str().expect("id").to_string();
    Fixture { m, server, project }
}

impl Fixture {
    fn pause_path(&self) -> String {
        format!("/api/hq/projects/{}/capture/pause", self.project)
    }
    fn exclusions_path(&self) -> String {
        format!("/api/hq/projects/{}/capture/exclusions", self.project)
    }
    fn pause(&self, act: &str) -> (u16, Value) {
        self.server
            .post(&self.pause_path(), &serde_json::json!({ "act": act }))
    }
    fn exclude(&self, act: &str, pattern: &str) -> (u16, Value) {
        self.server.post(
            &self.exclusions_path(),
            &serde_json::json!({ "act": act, "pattern": pattern }),
        )
    }
    fn capture(&self) -> Value {
        let (code, doc) = self
            .server
            .get(&format!("/api/hq/projects/{}/capture", self.project));
        assert_eq!(code, 200, "{doc}");
        doc
    }
    fn holds(&self) -> Value {
        self.capture()["holds"].clone()
    }
}

/// One exclusion of a served `holds`, by key.
fn entry(holds: &Value, key: &str) -> Value {
    holds["exclusions"]
        .as_array()
        .expect("exclusions")
        .iter()
        .find(|e| e["key"] == key)
        .unwrap_or_else(|| panic!("no {key:?} in {holds}"))
        .clone()
}

#[test]
fn they_must_come_from_the_page_and_say_exactly_what_they_mean() {
    let f = fixture();
    let (pause, exclusions) = (f.pause_path(), f.exclusions_path());

    // No Origin is the route's own refusal. A foreign Origin or a lookalike port
    // never reaches the route: the server answers 403 to anything but itself.
    // Either way nothing is kept.
    for (path, good) in [
        (&pause, r#"{"act":"paused"}"#),
        (&exclusions, r#"{"act":"excluded","pattern":"vendor"}"#),
    ] {
        let (code, body) = f.server.post_from(None, path, good);
        assert_eq!(code, 403, "{body}");
        assert_eq!(body["error"], "same_origin_required");
        for origin in [
            "https://example.com",
            "http://127.0.0.1:1",
            "http://localhost:1",
        ] {
            let (code, _) = f.server.post_from(Some(origin), path, good);
            assert_eq!(code, 403, "{origin}");
        }
    }
    assert_eq!(f.m.rows(), 0);

    // A different method is not an act.
    for path in [&pause, &exclusions] {
        let (code, body) = f.server.get(path);
        assert_eq!(code, 405, "{body}");
    }

    // An unknown project is named as such, on both routes.
    for (route, body) in [
        ("pause", serde_json::json!({"act": "paused"})),
        (
            "exclusions",
            serde_json::json!({"act": "excluded", "pattern": "vendor"}),
        ),
    ] {
        let (code, body) = f
            .server
            .post(&format!("/api/hq/projects/nope/capture/{route}"), &body);
        assert_eq!(code, 404, "{body}");
        assert_eq!(body["error"], "no_such_project");
    }

    // The folder, the project, the person and the time are never the browser's
    // to say: any of them is a body the route does not understand.
    for extra in [
        r#""project_folder":"/etc""#,
        r#""project_id":"x""#,
        r#""actor":"someone-else""#,
        r#""recorded_at":"2020-01-01T00:00:00Z""#,
        r#""ends":"chd_X""#,
    ] {
        for (path, base) in [
            (&pause, r#""act":"paused""#),
            (&exclusions, r#""act":"excluded","pattern":"vendor""#),
        ] {
            let (code, body) = f.server.post_from(
                Some(&f.server.origin()),
                path,
                &format!("{{{base},{extra}}}"),
            );
            assert_eq!(code, 400, "{extra}: {body}");
            assert_eq!(body["error"], "malformed_request");
        }
    }
    for (path, bad) in [
        (&pause, "not json"),
        (&pause, "{}"),
        (&pause, r#"["paused"]"#),
        (&pause, r#"{"act":7}"#),
        (&exclusions, r#"{"act":"excluded"}"#),
        (&exclusions, r#"{"pattern":"vendor"}"#),
        (&exclusions, r#"{"act":"excluded","pattern":7}"#),
    ] {
        let (code, body) = f.server.post_from(Some(&f.server.origin()), path, bad);
        assert_eq!(code, 400, "{bad}: {body}");
        assert_eq!(body["error"], "malformed_request");
    }

    // An act word the route does not own: the exclusion words are not pause
    // words and the pause words are not exclusion words.
    for (path, body) in [
        (&pause, serde_json::json!({"act": "stop"})),
        (&pause, serde_json::json!({"act": "excluded"})),
        (&pause, serde_json::json!({"act": "Paused"})),
        (
            &exclusions,
            serde_json::json!({"act": "paused", "pattern": "vendor"}),
        ),
        (
            &exclusions,
            serde_json::json!({"act": "removed", "pattern": "vendor"}),
        ),
    ] {
        let (code, answer) = f.server.post(path, &body);
        assert_eq!(code, 400, "{body}: {answer}");
        assert_eq!(answer["error"], "unknown_act");
    }
    assert_eq!(f.m.rows(), 0, "every refusal appended nothing");
}

#[test]
fn a_pause_and_a_resume_are_one_row_each_by_the_account_for_the_projects_own_folder() {
    let f = fixture();
    let root = f.capture()["project"]["root"]
        .as_str()
        .expect("root")
        .to_string();

    let (code, paused) = f.pause("paused");
    assert_eq!(code, 200, "{paused}");
    assert_eq!(paused["act"], "paused");
    assert_eq!(paused["schema_version"], 1);
    assert_eq!(paused["hold"]["act"], "paused");
    assert_eq!(
        paused["hold"]["by"], "robin",
        "the account ThruLine runs as"
    );
    assert_eq!(paused["holds"]["paused"], true);
    assert_eq!(paused["holds"]["paused_at"], paused["hold"]["at"]);
    assert_eq!(paused["holds"]["paused_by"], "robin");
    assert_eq!(
        paused["codex_observer"], "not_watching",
        "HQ was started with no observation source"
    );
    assert!(
        paused["message"].as_str().is_some_and(|m| !m.is_empty()),
        "{paused}"
    );

    // A pause and a serve-time exclusion are honoured in fields and in words, so
    // no screen has to guess (C34 Q4, Q5).
    assert_eq!(paused["holds"]["honoured"], true);
    assert_eq!(paused["holds"]["exclusions_honoured"], true);
    assert!(
        paused["holds"]["note"].as_str().is_some_and(|n| n
            .contains("While paused, ThruLine reads nothing from this folder")
            && n.contains("A path exclusion leaves matching entries out when ThruLine shows or exports a view")),
        "{paused}"
    );

    // The row: the project's own id and root, no pattern, no `ends`.
    let table = f.m.table();
    assert_eq!(table.len(), 1);
    let (act, project_id, folder, pattern, actor, ends, _) = &table[0];
    assert_eq!(act, "paused");
    assert_eq!(project_id, &f.project);
    assert_eq!(
        folder, &root,
        "the folder is the project's root, not the browser's"
    );
    assert_eq!((pattern, actor.as_str(), ends), (&None, "robin", &None));

    // Pausing a paused project changes nothing.
    let (code, again) = f.pause("paused");
    assert_eq!(code, 409, "{again}");
    assert_eq!(again["error"], "already_paused");
    assert!(again["message"]
        .as_str()
        .is_some_and(|m| m.contains("nothing was saved")));
    assert_eq!(f.m.rows(), 1);

    // The capture read says so, additively.
    let doc = f.capture();
    assert_eq!(
        doc["schema_version"], 1,
        "the capture document's schema stays 1"
    );
    assert_eq!(doc["holds"]["paused"], true);
    assert_eq!(doc["holds"]["paused_at"], paused["hold"]["at"]);
    assert_eq!(doc["holds_reason"], Value::Null);

    // Resume: one row ending the pause; again, nothing.
    let (code, resumed) = f.pause("resumed");
    assert_eq!(code, 200, "{resumed}");
    assert_eq!(resumed["holds"]["paused"], false);
    assert_eq!(resumed["holds"]["paused_at"], Value::Null);
    let (code, again) = f.pause("resumed");
    assert_eq!(code, 409, "{again}");
    assert_eq!(again["error"], "not_paused");
    let table = f.m.table();
    assert_eq!(table.len(), 2);
    assert_eq!(table[1].0, "resumed");
    assert_eq!(
        table[1].5.as_deref(),
        Some(table[0].6.as_str()),
        "it ends the pause"
    );
    assert_eq!(f.capture()["holds"]["paused"], false);

    // The chain adds up under `thruline verify`.
    let (code, text) = f.m.verify();
    assert_eq!(code, Some(0), "{text}");
    assert!(
        text.contains("capture holds  chain re-checked, 2 links add up"),
        "{text}"
    );
}

#[test]
fn the_defaults_are_served_as_on_by_default_and_are_written_down_only_by_the_first_change() {
    let f = fixture();
    // Turning on the generated-files exclusion asks Git for this project's
    // attributes. Give the synthetic project a real repository for that read.
    assert!(Command::new("git")
        .args(["init", "--quiet", &f.m.repo()])
        .status()
        .expect("initialize synthetic repository")
        .success());
    let before = f.holds();
    assert_eq!(f.m.rows(), 0, "reading the capture route wrote nothing");
    assert_eq!(before["paused"], false);
    assert_eq!(before["paused_at"], Value::Null);
    for key in ["env", "secrets"] {
        let e = entry(&before, key);
        assert_eq!(e["on"], true, "{e}");
        assert_eq!(e["source"], "default");
        assert_eq!(e["basis"], "on by default (not recorded)");
        assert_eq!(
            (e["since"].clone(), e["by"].clone()),
            (Value::Null, Value::Null)
        );
    }
    let generated = entry(&before, "generated");
    assert_eq!(generated["on"], false, "{generated}");

    // Excluding what is already on changes nothing.
    let (code, body) = f.exclude("excluded", "env");
    assert_eq!(code, 409, "{body}");
    assert_eq!(body["error"], "already_excluded");
    assert_eq!(f.m.rows(), 0);

    // Turning a default off is the first change: it appends, and appends the
    // two rows the table needs to say it.
    let (code, off) = f.exclude("included", "env");
    assert_eq!(code, 200, "{off}");
    assert_eq!(off["hold"]["act"], "included");
    assert_eq!(off["hold"]["by"], "robin");
    let e = entry(&off["holds"], "env");
    assert_eq!(
        (e["on"].clone(), e["source"].clone()),
        (false.into(), "recorded".into())
    );
    assert_eq!(e["by"], "robin");
    assert_eq!(e["basis"], Value::Null);
    assert_eq!(entry(&off["holds"], "secrets")["source"], "default");

    let table = f.m.table();
    assert_eq!(table.len(), 2);
    assert_eq!(
        (
            table[0].0.as_str(),
            table[0].3.as_deref(),
            table[0].4.as_str()
        ),
        ("excluded", Some("env"), "ThruLine default"),
        "the default is written down under ThruLine's own name"
    );
    assert_eq!(
        (
            table[1].0.as_str(),
            table[1].3.as_deref(),
            table[1].4.as_str()
        ),
        ("included", Some("env"), "robin")
    );
    assert_eq!(table[1].5.as_deref(), Some(table[0].6.as_str()));

    // And again: it is off now.
    let (code, body) = f.exclude("included", "env");
    assert_eq!(code, 409, "{body}");
    assert_eq!(body["error"], "not_excluded");
    assert_eq!(f.m.rows(), 2);

    // Back on is one ordinary row; the capture read follows the record.
    let (code, on) = f.exclude("excluded", "env");
    assert_eq!(code, 200, "{on}");
    assert_eq!(f.m.rows(), 3);
    assert_eq!(entry(&f.holds(), "env")["on"], true);

    // `generated` is off by default: turning it on is one row.
    let (code, _) = f.exclude("excluded", "generated");
    assert_eq!(code, 200);
    assert_eq!(f.m.rows(), 4);
    assert_eq!(entry(&f.holds(), "generated")["on"], true);

    let (code, text) = f.m.verify();
    assert_eq!(code, Some(0), "{text}");
    assert!(
        text.contains("capture holds  chain re-checked, 4 links add up"),
        "{text}"
    );
}

#[test]
fn a_typed_path_is_trimmed_listed_and_taken_back_and_a_bad_one_is_refused_by_name() {
    let f = fixture();
    let (code, kept) = f.exclude("excluded", "  docs/*.draft.md  ");
    assert_eq!(code, 200, "{kept}");
    assert_eq!(kept["pattern"], "docs/*.draft.md", "kept trimmed");
    assert_eq!(f.m.table()[0].3.as_deref(), Some("docs/*.draft.md"));
    let listed = entry(&f.holds(), "docs/*.draft.md");
    assert_eq!(listed["on"], true);
    assert_eq!(listed["built_in"], false);
    assert_eq!(listed["by"], "robin");

    let (code, body) = f.exclude("excluded", "docs/*.draft.md");
    assert_eq!(
        (code, body["error"].as_str()),
        (409, Some("already_excluded"))
    );
    let (code, _) = f.exclude("included", "docs/*.draft.md");
    assert_eq!(code, 200);
    assert!(
        f.holds()["exclusions"]
            .as_array()
            .expect("exclusions")
            .iter()
            .all(|e| e["key"] != "docs/*.draft.md"),
        "an included path is no longer listed"
    );
    let (code, body) = f.exclude("included", "docs/*.draft.md");
    assert_eq!((code, body["error"].as_str()), (409, Some("not_excluded")));
    let rows = f.m.rows();
    assert_eq!(rows, 2);

    // Shapes that cannot be a path inside the project: each is named, says
    // nothing was saved, and appends nothing. The leading-space forms are the
    // ones a trim would otherwise let through.
    for bad in [
        "",
        "   ",
        "/etc/passwd",
        " /etc/passwd",
        "../outside",
        "  ../outside",
        "a/../b",
        "src/?.rs",
        "a[b]",
        "a{b,c}",
        "line\nbreak",
        "nul\u{0}byte",
    ] {
        let (code, body) = f.exclude("excluded", bad);
        assert_eq!(code, 400, "{bad:?}: {body}");
        assert_eq!(body["error"], "bad_pattern", "{bad:?}");
        assert!(
            body["message"]
                .as_str()
                .is_some_and(|m| m.contains("Nothing was saved")),
            "{bad:?}: {body}"
        );
    }
    assert_eq!(f.m.rows(), rows, "a refused path appends nothing");
}

#[test]
fn a_pattern_over_the_limit_is_named_and_a_body_over_what_one_could_make_is_refused_by_size() {
    let f = fixture();
    let long = "a".repeat(257);
    let (code, body) = f.exclude("excluded", &long);
    assert_eq!(code, 409, "{body}");
    assert_eq!(body["error"], "too_long");
    assert_eq!(body["field"], "pattern");
    assert_eq!(body["limit"], 256);
    assert_eq!(f.m.rows(), 0);

    // Exactly at the limit is a path.
    let (code, ok) = f.exclude("excluded", &"a".repeat(256));
    assert_eq!(code, 200, "{ok}");
    assert_eq!(f.m.rows(), 1);

    // The cap is sized from that limit: a body larger than a 256-character
    // pattern could make, written the longest way JSON can, is refused by size.
    let huge = serde_json::json!({"act": "excluded", "pattern": "a".repeat(8_192)});
    let (code, body) = f.server.post(&f.exclusions_path(), &huge);
    assert_eq!(
        code, 413,
        "refused before the body is read; the connection may be cut: {body}"
    );
    // The pause route takes a few bytes: anything bigger is refused the same way.
    let big = serde_json::json!({"act": "paused", "pad": "x".repeat(2_000)});
    let (code, body) = f.server.post(&f.pause_path(), &big);
    assert_eq!(
        code, 413,
        "refused before the body is read; the connection may be cut: {body}"
    );
    assert_eq!(f.m.rows(), 1);
}

#[test]
fn what_a_person_typed_is_served_masked_and_kept_as_typed() {
    let f = fixture();
    let typed = format!("keys/{CANARY}");
    let (code, kept) = f.exclude("excluded", &typed);
    assert_eq!(code, 200, "{kept}");
    let served = kept.to_string();
    assert!(
        !served.contains("ghp_TLCANARY"),
        "the answer is masked: {served}"
    );
    let read = f.capture().to_string();
    assert!(!read.contains("ghp_TLCANARY"), "the read is masked: {read}");
    assert_eq!(
        f.m.table()[0].3.as_deref(),
        Some(typed.as_str()),
        "the record keeps what was typed"
    );
    // The folder never leaves in a hold's answer: the project's root is served
    // once, by the capture document, not by a row.
    assert!(kept["hold"].get("project_folder").is_none(), "{kept}");
}

#[test]
fn an_exclusion_is_recorded_only_ingest_reads_the_folder_as_it_did() {
    // An exclusion is in the record, the chain adds up, and ingest still reads
    // a recording made after it exactly as one made before (C34 Q4).
    let f = fixture();
    let events_before = f.m.count("events");
    assert!(events_before > 0);
    let (code, _) = f.exclude("excluded", "src");
    assert_eq!(code, 200);
    let (code, _) = f.exclude("included", "env");
    assert_eq!(code, 200);

    f.m.record(
        "s2",
        "Add a test for empty input.",
        "2026-10-02",
        "src/a.rs",
    );
    f.m.ingest();
    let events_after = f.m.count("events");
    assert_eq!(
        events_after,
        events_before + 4,
        "the new session was read in full while a path was marked excluded"
    );
    assert_eq!(f.m.count("sessions"), 2);

    // And what HQ serves is the record it always served.
    let (code, view) = f.server.get(&format!("/api/hq/projects/{}", f.project));
    assert_eq!(code, 200, "{view}");
    assert_eq!(view["project"]["sessions"], 2, "{view}");
    assert_eq!(view["project"]["prompts"], 2, "{view}");
}

#[test]
fn a_path_exclusion_replaces_served_edits_and_file_rows_even_after_a_warm_read() {
    let f = fixture();
    let project_path = format!("/api/hq/projects/{}", f.project);
    let (code, project) = f.server.get(&project_path);
    assert_eq!(code, 200, "{project}");
    let thread_id = project["threads"][0]["id"].as_str().expect("thread id");
    let thread_path = format!("{project_path}/threads/{thread_id}");
    let (code, before) = f.server.get(&thread_path);
    assert_eq!(code, 200, "{before}");
    assert_eq!(before["edits"][0]["file"], "src/a.rs");
    assert_eq!(before["files"][0]["path"], "src/a.rs");
    assert_eq!(f.server.get(&thread_path).1, before, "warm read");
    let ledger = thruline_core::ledger::Ledger::open(f.m.db()).expect("open ledger");
    let session_id: String = ledger
        .conn()
        .query_row("SELECT id FROM sessions LIMIT 1", [], |row| row.get(0))
        .expect("session id");
    let session_path = format!("/api/hq/sessions/{session_id}");
    let (code, session_before) = f.server.get(&session_path);
    assert_eq!(code, 200, "{session_before}");
    assert!(session_before.to_string().contains("src/a.rs"));

    let (code, _) = f.exclude("excluded", "src/a.rs");
    assert_eq!(code, 200);
    let marker = "left out by your exclusion (`src/a.rs`)";
    let (code, hidden) = f.server.get(&thread_path);
    assert_eq!(code, 200, "{hidden}");
    assert_eq!(hidden["edits"][0]["file"], marker, "{hidden}");
    assert_eq!(hidden["files"][0]["path"], marker, "{hidden}");
    assert!(
        hidden["left_out_by_exclusion"]
            .as_u64()
            .is_some_and(|n| n >= 2),
        "each replaced entry is counted: {hidden}"
    );
    assert_eq!(f.server.get(&thread_path).1, hidden, "warm hidden read");
    let (code, session_hidden) = f.server.get(&session_path);
    assert_eq!(code, 200, "{session_hidden}");
    assert!(
        session_hidden.to_string().contains(marker),
        "{session_hidden}"
    );
    assert!(
        !session_hidden.to_string().contains("src/a.rs\""),
        "{session_hidden}"
    );

    let (code, _) = f.exclude("included", "src/a.rs");
    assert_eq!(code, 200);
    let (code, restored) = f.server.get(&thread_path);
    assert_eq!(code, 200, "{restored}");
    assert_eq!(restored, before, "putting the path back restores the view");
    assert_eq!(f.server.get(&session_path).1, session_before);
}

/// One hook delivery for the project's folder, through the shipping binary.
fn deliver(m: &Machine, event: &str, cwd: &str) {
    let db = m.db();
    let mut child = m
        .cmd(&[
            "--db",
            db.to_str().expect("utf8"),
            "sensor",
            "append",
            event,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("sensor");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(
            serde_json::json!({"session_id": "s1", "cwd": cwd, "hook_event_name": event})
                .to_string()
                .as_bytes(),
        )
        .expect("payload");
    let out = child.wait_with_output().expect("wait");
    assert!(out.status.success() && out.stdout.is_empty() && out.stderr.is_empty());
}

impl Fixture {
    fn load_path(&self) -> String {
        format!("/api/hq/projects/{}/load", self.project)
    }
    /// Start a load from the page, wait for it to finish, return its last status.
    fn load(&self) -> Value {
        let (code, started) =
            self.server
                .post_from(Some(&self.server.origin()), &self.load_path(), "");
        assert_eq!(code, 202, "{started}");
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let (_, status) = self.server.get("/api/hq/load");
            if status["running"] == false {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "the load never finished: {status}"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[test]
fn a_pause_stops_the_hq_load_and_thruline_ingest_and_a_resume_lets_them_through() {
    let f = fixture();
    let events_before = f.m.count("events");
    let (code, _) = f.pause("paused");
    assert_eq!(code, 200);
    f.m.record(
        "s2",
        "Add a test for empty input.",
        "2026-10-02",
        "src/a.rs",
    );

    // `thruline ingest` of the folder: nothing is read, and it is not a failure.
    f.m.ingest();
    assert_eq!(
        f.m.count("events"),
        events_before,
        "ingest read a paused folder"
    );
    assert_eq!(f.m.count("sessions"), 1);

    // The HQ load: both recordings are under the pause, none is planned, and the
    // status counts them.
    let status = f.load();
    assert_eq!(status["paused_by_you"], 2, "{status}");
    assert_eq!(status["started_while_paused"], 0, "{status}");
    assert_eq!(status["files_total"], 0, "{status}");
    assert_eq!(
        f.m.count("events"),
        events_before,
        "the load read a paused folder"
    );

    // The hook is left out too, and the capture document counts it.
    deliver(&f.m, "Stop", &f.m.repo());
    assert_eq!(f.m.count("sensor_events"), 0);
    let capture = f.capture();
    assert_eq!(capture["paused_skips"]["hook_deliveries"], 1, "{capture}");
    assert_eq!(capture["holds"]["paused"], true, "{capture}");

    // Resumed: the load reads what the pause had held back. Both sessions began
    // outside any pause window (their first lines are dated October 1 and 2).
    let (code, _) = f.pause("resumed");
    assert_eq!(code, 200);
    let status = f.load();
    assert_eq!(status["paused_by_you"], 0, "{status}");
    assert_eq!(status["started_while_paused"], 0, "{status}");
    assert_eq!(status["files_total"], 2, "{status}");
    assert_eq!(
        f.m.count("events"),
        events_before + 4,
        "after the resume the second session is read: {status}"
    );
    // The capture document keeps saying what was left out while it was paused.
    let capture = f.capture();
    assert_eq!(capture["paused_skips"]["hook_deliveries"], 1, "{capture}");
    assert_eq!(capture["holds"]["paused"], false, "{capture}");
}

#[test]
fn a_project_nobody_paused_has_no_pause_fields_in_its_capture_document() {
    let f = fixture();
    let capture = f.capture();
    assert!(capture.get("paused_skips").is_none(), "{capture}");
    let (code, view) = f.server.get(&format!("/api/hq/projects/{}", f.project));
    assert_eq!(code, 200, "{view}");
}

#[test]
fn a_forged_row_of_the_chain_fails_verify_including_the_default_that_was_written_down() {
    let Fixture { m, server, project } = fixture();
    let (code, _) = server.post(
        &format!("/api/hq/projects/{project}/capture/exclusions"),
        &serde_json::json!({"act": "included", "pattern": "secrets"}),
    );
    assert_eq!(code, 200);
    let (code, _) = server.post(
        &format!("/api/hq/projects/{project}/capture/pause"),
        &serde_json::json!({"act": "paused"}),
    );
    assert_eq!(code, 200);
    drop(server);
    let (code, text) = m.verify();
    assert_eq!(code, Some(0), "{text}");
    assert!(
        text.contains("capture holds  chain re-checked, 3 links add up"),
        "{text}"
    );

    // In a scratch store with the trigger dropped: the written-down default
    // claims a person excluded it. Verify exits 1 and names the chain.
    {
        let conn = rusqlite::Connection::open(m.db()).expect("open");
        conn.execute_batch(
            "DROP TRIGGER capture_holds_no_update;
             UPDATE capture_holds SET actor='robin' WHERE seq=0;",
        )
        .expect("forge");
    }
    let (code, text) = m.verify();
    assert_eq!(code, Some(1), "{text}");
    assert!(
        text.contains("capture holds  CHAIN DOES NOT ADD UP"),
        "{text}"
    );
}

#[test]
fn the_routes_are_free_and_the_one_paid_surface_is_untouched() {
    // The server starts as it ships — paid gate on, no licence — and both acts
    // are still kept: only approving a version asks for a licence.
    let f = fixture();
    assert!(
        !f.m.home().join(".thruline/licence.json").exists(),
        "a fresh home holds no licence"
    );
    let (code, body) = f.pause("paused");
    assert_eq!(code, 200, "{body}");
    assert_ne!(body["error"], "licence_required");
    let (code, body) = f.exclude("excluded", "vendor");
    assert_eq!(code, 200, "{body}");
    assert_ne!(body["error"], "licence_required");
}
