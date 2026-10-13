//! `GET` / `POST /api/hq/projects/<id>/install` (C30 R-ROUTES "Install")
//! against the shipping binary: `thruline ui` on a temporary HOME, a
//! temporary ThruLine home and a temporary git repository as the project.
//! Nothing here reads or writes the real `~/.claude`, `~/.claude.json`,
//! `~/.thruline`, a shell profile or any real project: every path the route
//! resolves (HOME, THRULINE_HOME, PATH, the project's own folder) is inside
//! one `tempfile::TempDir`, and `every_file_the_route_writes_is_one_it_named`
//! walks that whole folder to prove nothing else changed.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::Value;

const HOOKS_FILE: &str = ".claude/settings.local.json";
const MCP_FILE: &str = ".mcp.json";

struct Machine {
    dir: tempfile::TempDir,
}

impl Machine {
    /// A HOME, a ThruLine home, a search path holding one program the shim
    /// can wrap, and a git repository with one ingested recording.
    fn new() -> Machine {
        let dir = tempfile::tempdir().expect("tempdir");
        let m = Machine { dir };
        std::fs::create_dir_all(m.home().join(".thruline")).expect("home");
        std::fs::create_dir_all(m.repo_path()).expect("repo");
        std::fs::create_dir_all(m.bin()).expect("bin");
        // One program on the search path, so the shim writes at least one
        // wrapper whatever the host has installed.
        let pytest = m.bin().join("pytest");
        std::fs::write(&pytest, "#!/bin/sh\nexit 0\n").expect("pytest");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&pytest, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let ok = Command::new("git")
            .args(["init", "-q"])
            .current_dir(m.repo_path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .status()
            .expect("git init");
        assert!(ok.success());
        m.seed();
        m
    }
    fn home(&self) -> PathBuf {
        self.dir.path().join("h")
    }
    fn thruline_home(&self) -> PathBuf {
        self.home().join(".thruline")
    }
    fn bin(&self) -> PathBuf {
        self.dir.path().join("bin")
    }
    fn repo_path(&self) -> PathBuf {
        self.dir.path().join("repo")
    }
    fn repo(&self) -> String {
        self.repo_path().display().to_string()
    }
    fn db(&self) -> PathBuf {
        self.thruline_home().join("ledger.db")
    }
    fn hooks_file(&self) -> PathBuf {
        self.repo_path().join(HOOKS_FILE)
    }
    fn mcp_file(&self) -> PathBuf {
        self.repo_path().join(MCP_FILE)
    }
    fn shim_dir(&self) -> PathBuf {
        self.thruline_home().join("shim")
    }
    fn user_settings(&self) -> PathBuf {
        self.home().join(".claude/settings.json")
    }
    fn search_path(&self) -> String {
        format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", self.bin().display())
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
            .env("THRULINE_HOME", self.thruline_home())
            .env("PATH", self.search_path())
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .env_remove("THRULINE_HOOK_SPOOL")
            .env_remove("THRULINE_SHIM_DIR")
            .env_remove("CODEX_HOME")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .current_dir(self.repo_path());
        cmd
    }
    /// One recording of two steps in the repository, ingested, so HQ lists
    /// the repository as a project.
    fn seed(&self) {
        let cwd = self.repo();
        std::fs::create_dir_all(self.project_dir()).expect("dir");
        let line = |n: u32, kind: &str, message: Value| {
            serde_json::json!({
                "type": kind, "uuid": format!("s1-{n}"), "parentUuid": null,
                "isSidechain": false, "sessionId": "s1",
                "timestamp": format!("2026-10-01T09:0{n}:00.000Z"), "cwd": cwd, "message": message,
            })
            .to_string()
        };
        let lines = [
            line(
                1,
                "user",
                serde_json::json!({"role": "user", "content": "Fix the parser."}),
            ),
            line(
                2,
                "assistant",
                serde_json::json!({"role": "assistant", "content": [
                {"type": "text", "text": "Done."}]}),
            ),
        ];
        std::fs::write(self.project_dir().join("s1.jsonl"), lines.join("\n") + "\n")
            .expect("recording");
        let status = self
            .cmd(&["ingest", "--apply", "--source", "claude-code"])
            .arg(self.project_dir())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("ingest");
        assert!(status.success());
    }
    /// Every file under the whole temporary folder, by its path relative to
    /// that folder, with its bytes' digest (a symlink by its target). The
    /// store's own files and HQ's reading of the projects are left out: the
    /// server writes them whether or not anything is installed.
    fn files(&self) -> BTreeMap<String, String> {
        let root = self.dir.path().to_path_buf();
        let mut out = BTreeMap::new();
        for entry in walkdir::WalkDir::new(&root).follow_links(false) {
            let entry = entry.expect("walk");
            let rel = entry
                .path()
                .strip_prefix(&root)
                .expect("inside")
                .display()
                .to_string();
            let name = entry.file_name().to_string_lossy();
            if name.starts_with("ledger.db")
                || rel.starts_with("h/.thruline/export")
                || rel.starts_with("repo/.git/")
            {
                continue;
            }
            let ft = entry.file_type();
            if ft.is_symlink() {
                let to = std::fs::read_link(entry.path()).expect("link");
                out.insert(rel, format!("-> {}", to.display()));
            } else if ft.is_file() {
                out.insert(rel, sha256(entry.path()).expect("read"));
            }
        }
        out
    }
}

fn sha256(path: &Path) -> Option<String> {
    use sha2::Digest;
    std::fs::read(path).ok().map(|b| {
        sha2::Sha256::digest(b)
            .iter()
            .map(|x| format!("{x:02x}"))
            .collect()
    })
}

/// The paths whose digest differs between two walks, or that exist in one only.
fn changed(before: &BTreeMap<String, String>, after: &BTreeMap<String, String>) -> Vec<String> {
    let mut keys: Vec<&String> = before.keys().chain(after.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter(|k| before.get(*k) != after.get(*k))
        .cloned()
        .collect()
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
        Server::start_with(m, &[])
    }

    fn start_with(m: &Machine, before_ui: &[&str]) -> Server {
        let mut args: Vec<&str> = before_ui.to_vec();
        args.extend(["ui", "--no-open", "--port", "0"]);
        let mut child = m
            .cmd(&args)
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

    fn send(&self, head: &str) -> (u16, Value, String) {
        let mut s = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        s.set_read_timeout(Some(Duration::from_secs(60)))
            .expect("timeout");
        s.write_all(head.as_bytes()).expect("write");
        let mut raw = Vec::new();
        s.read_to_end(&mut raw).expect("read");
        let text = String::from_utf8_lossy(&raw).into_owned();
        let status: u16 = text[9..12].parse().expect("status");
        let body = text.split_once("\r\n\r\n").map(|x| x.1).unwrap_or("");
        (
            status,
            serde_json::from_str(body).unwrap_or(Value::Null),
            text.clone(),
        )
    }

    fn get(&self, path: &str) -> (u16, Value) {
        let (s, v, _) = self.send(&format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            self.port
        ));
        (s, v)
    }

    fn token(&self) -> String {
        let (code, body) = self.get("/api/run/token");
        assert_eq!(code, 200, "{body}");
        body["token"].as_str().expect("token").to_string()
    }

    /// A POST with exactly the headers the page's own fetch sends.
    fn post_as(
        &self,
        path: &str,
        body: &str,
        origin: Option<&str>,
        token: Option<&str>,
    ) -> (u16, Value, String) {
        let mut head = format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{p}\r\nContent-Type: application/json\r\nContent-Length: {n}\r\nConnection: close\r\n",
            p = self.port,
            n = body.len()
        );
        if let Some(o) = origin {
            head.push_str(&format!("Origin: {o}\r\n"));
        }
        if let Some(t) = token {
            head.push_str(&format!("X-ThruLine-Run-Token: {t}\r\n"));
        }
        head.push_str("\r\n");
        head.push_str(body);
        self.send(&head)
    }

    fn origin(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn project_id(&self) -> String {
        let (code, list) = self.get("/api/hq/projects");
        assert_eq!(code, 200, "{list}");
        let projects = list["projects"].as_array().expect("projects");
        assert_eq!(projects.len(), 1, "{list}");
        projects[0]["id"].as_str().expect("id").to_string()
    }

    fn address(&self) -> String {
        format!("/api/hq/projects/{}/install", self.project_id())
    }

    fn plan(&self) -> Value {
        let (code, doc) = self.get(&self.address());
        assert_eq!(code, 200, "{doc}");
        assert_eq!(doc["schema_version"], 1, "{doc}");
        doc
    }

    /// Install exactly these choices against the plan read just now.
    fn install(&self, mcp: bool, speaker: bool, run_capture: bool) -> (u16, Value) {
        let plan = self.plan();
        let body = serde_json::json!({
            "hooks": true, "mcp": mcp, "speaker": speaker, "run_capture": run_capture,
            "plan": plan["plan"],
        })
        .to_string();
        let token = self.token();
        let (code, doc, _) =
            self.post_as(&self.address(), &body, Some(&self.origin()), Some(&token));
        (code, doc)
    }
}

fn json_of(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).expect("read")).expect("json")
}

/// Every hook command under one event of a settings document.
fn commands(doc: &Value, event: &str) -> Vec<String> {
    doc["hooks"][event]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|e| e["hooks"].as_array().cloned().unwrap_or_default())
        .filter_map(|h| h["command"].as_str().map(str::to_string))
        .collect()
}

fn exe() -> String {
    PathBuf::from(env!("CARGO_BIN_EXE_thruline"))
        .display()
        .to_string()
}

#[test]
fn the_plan_names_each_file_and_writes_nothing() {
    let m = Machine::new();
    let s = Server::start(&m);
    let before = m.files();
    let plan = s.plan();
    let parts = &plan["parts"];
    assert_eq!(parts["hooks"]["state"], "will_add", "{plan}");
    assert_eq!(parts["hooks"]["file"], HOOKS_FILE, "{plan}");
    assert_eq!(parts["hooks"]["level"], "project-local", "{plan}");
    let mut events = thruline_core::sensor::install_set();
    events.sort();
    let mut adds: Vec<&str> = parts["hooks"]["adds"]
        .as_array()
        .expect("adds")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    adds.sort();
    assert_eq!(adds, events, "{plan}");
    assert_eq!(parts["speaker"]["state"], "will_add", "{plan}");
    assert_eq!(parts["speaker"]["file"], HOOKS_FILE, "{plan}");
    assert_eq!(
        parts["speaker"]["adds"].as_array().map(Vec::len),
        Some(3),
        "{plan}"
    );
    assert_eq!(parts["mcp"]["state"], "will_add", "{plan}");
    assert_eq!(parts["mcp"]["file"], MCP_FILE, "{plan}");
    assert_eq!(parts["mcp"]["level"], "project", "{plan}");
    assert_eq!(
        parts["mcp"]["adds"],
        serde_json::json!(["mcpServers.thruline"]),
        "{plan}"
    );
    assert_eq!(parts["run_capture"]["state"], "will_add", "{plan}");
    assert_eq!(parts["run_capture"]["level"], "thruline-home", "{plan}");
    assert_eq!(
        parts["run_capture"]["path_line"], "export PATH=\"$HOME/.thruline/shim:$PATH\"",
        "{plan}"
    );
    let id = plan["plan"].as_str().expect("plan id");
    assert_eq!(id.len(), 64, "{plan}");
    // Reading it again is the same plan, and nothing was written by either.
    assert_eq!(s.plan()["plan"], plan["plan"]);
    assert_eq!(changed(&before, &m.files()), Vec::<String>::new());
    assert!(!m.hooks_file().exists());
    assert!(!m.mcp_file().exists());
    assert!(!m.shim_dir().exists());
}

#[test]
fn hooks_alone_write_exactly_the_sensor_entries_and_nothing_else() {
    let m = Machine::new();
    let s = Server::start(&m);
    let before = m.files();
    let (code, doc) = s.install(false, false, false);
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["changed"], true, "{doc}");
    let hooks = &doc["parts"]["hooks"];
    assert_eq!(hooks["outcome"], "wrote", "{doc}");
    assert_eq!(hooks["file"], HOOKS_FILE, "{doc}");
    for part in ["mcp", "speaker", "run_capture"] {
        assert_eq!(doc["parts"][part]["outcome"], "not_chosen", "{part}: {doc}");
    }
    assert_eq!(
        changed(&before, &m.files()),
        vec![format!("repo/{HOOKS_FILE}")],
        "the hooks wrote something besides their one file"
    );
    let written = json_of(&m.hooks_file());
    let events = thruline_core::sensor::install_set();
    let keys: Vec<&String> = written["hooks"]
        .as_object()
        .expect("hooks")
        .keys()
        .collect();
    assert_eq!(keys.len(), events.len(), "{written}");
    for event in &events {
        assert_eq!(
            commands(&written, event),
            vec![format!("{} sensor append {event}", exe())],
            "{written}"
        );
    }
    // The file holds the hooks and nothing else.
    assert_eq!(
        written
            .as_object()
            .map(|o| o.keys().cloned().collect::<Vec<_>>()),
        Some(vec!["hooks".to_string()])
    );
}

#[test]
fn each_optional_part_writes_exactly_its_own_entries() {
    let m = Machine::new();
    let s = Server::start(&m);
    assert_eq!(s.install(false, false, false).0, 200);
    let hooks_only = std::fs::read(m.hooks_file()).expect("hooks");

    // Speaker hooks: three entries, beside the sensor pack, in the same file.
    let before = m.files();
    let (code, doc) = s.install(false, true, false);
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["parts"]["hooks"]["outcome"], "already", "{doc}");
    assert_eq!(doc["parts"]["speaker"]["outcome"], "wrote", "{doc}");
    assert_eq!(
        changed(&before, &m.files()),
        vec![format!("repo/{HOOKS_FILE}")]
    );
    let written = json_of(&m.hooks_file());
    for (event, verb) in [
        ("SessionStart", "brief --hook session-start"),
        ("Stop", "audit --hook stop"),
        ("SessionEnd", "digest precompute --hook session-end"),
    ] {
        let cmds = commands(&written, event);
        assert!(
            cmds.contains(&format!("{} {verb}", exe())),
            "{event}: {written}"
        );
        assert!(
            cmds.contains(&format!("{} sensor append {event}", exe())),
            "{event}: {written}"
        );
        assert_eq!(cmds.len(), 2, "{event}: {written}");
    }
    assert_ne!(std::fs::read(m.hooks_file()).expect("hooks"), hooks_only);

    // The MCP server: one key, in the project's .mcp.json, reading this store.
    let before = m.files();
    let (code, doc) = s.install(true, false, false);
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["parts"]["mcp"]["outcome"], "wrote", "{doc}");
    assert_eq!(doc["parts"]["mcp"]["file"], MCP_FILE, "{doc}");
    assert_eq!(
        changed(&before, &m.files()),
        vec![format!("repo/{MCP_FILE}")]
    );
    let mcp = json_of(&m.mcp_file());
    assert_eq!(
        mcp,
        serde_json::json!({"mcpServers": {"thruline": {
            "type": "stdio", "command": exe(),
            "args": ["mcp", "--agent", "claude", "--db", m.db().display().to_string()],
            "env": {}
        }}})
    );

    // Run capture: the shim folder in ThruLine's home, and nothing else.
    let before = m.files();
    let (code, doc) = s.install(false, false, true);
    assert_eq!(code, 200, "{doc}");
    let run = &doc["parts"]["run_capture"];
    assert_eq!(run["outcome"], "wrote", "{doc}");
    assert!(
        run["entries"]
            .as_array()
            .is_some_and(|w| w.iter().any(|x| x == "pytest")),
        "{doc}"
    );
    let moved = changed(&before, &m.files());
    assert!(!moved.is_empty());
    for path in &moved {
        assert!(
            path.starts_with("h/.thruline/shim/"),
            "run capture wrote outside the shim folder: {path}"
        );
    }
    assert!(m.shim_dir().join("thruline-shim.json").is_file());
    assert!(m.shim_dir().join("pytest").is_file());
    // No shell profile was touched: the line is for the person to add.
    for profile in [
        ".zshrc",
        ".bashrc",
        ".profile",
        ".bash_profile",
        ".zprofile",
    ] {
        assert!(!m.home().join(profile).exists(), "{profile} was written");
    }
    assert_eq!(
        run["path_line"], "export PATH=\"$HOME/.thruline/shim:$PATH\"",
        "{doc}"
    );
    // The user's own Claude Code settings and config were never written.
    assert!(!m.user_settings().exists());
    assert!(!m.home().join(".claude.json").exists());
}

#[test]
fn a_second_click_changes_nothing_and_says_so() {
    let m = Machine::new();
    let s = Server::start(&m);
    let (code, first) = s.install(true, true, true);
    assert_eq!(code, 200, "{first}");
    assert_eq!(first["changed"], true, "{first}");
    let before = m.files();
    let (code, second) = s.install(true, true, true);
    assert_eq!(code, 200, "{second}");
    assert_eq!(second["changed"], false, "{second}");
    for part in ["hooks", "mcp", "speaker", "run_capture"] {
        assert_eq!(
            second["parts"][part]["outcome"], "already",
            "{part}: {second}"
        );
    }
    assert!(
        second["plain"]
            .as_str()
            .is_some_and(|p| p.contains("Nothing changed")),
        "{second}"
    );
    assert_eq!(changed(&before, &m.files()), Vec::<String>::new());
    // The plan read afterwards says so too.
    let plan = s.plan();
    for part in ["hooks", "mcp", "speaker", "run_capture"] {
        assert_eq!(plan["parts"][part]["state"], "installed", "{part}: {plan}");
    }
}

#[test]
fn a_foreign_origin_a_missing_or_wrong_token_is_refused_and_writes_nothing() {
    let m = Machine::new();
    let s = Server::start(&m);
    let plan = s.plan();
    let body = serde_json::json!({
        "hooks": true, "mcp": true, "speaker": true, "run_capture": true, "plan": plan["plan"],
    })
    .to_string();
    let token = s.token();
    let addr = s.address();
    let before = m.files();
    // No token.
    let (code, doc, _) = s.post_as(&addr, &body, Some(&s.origin()), None);
    assert_eq!(
        (code, doc["error"].as_str()),
        (403, Some("run_token_required"))
    );
    // A wrong token.
    let wrong = "0".repeat(token.len());
    let (code, doc, _) = s.post_as(&addr, &body, Some(&s.origin()), Some(&wrong));
    assert_eq!(
        (code, doc["error"].as_str()),
        (403, Some("run_token_required"))
    );
    // Two tokens.
    let two = format!(
        "POST {addr} HTTP/1.1\r\nHost: 127.0.0.1:{p}\r\nOrigin: {o}\r\nX-ThruLine-Run-Token: {token}\r\nX-ThruLine-Run-Token: {token}\r\nContent-Type: application/json\r\nContent-Length: {n}\r\nConnection: close\r\n\r\n{body}",
        p = s.port,
        o = s.origin(),
        n = body.len()
    );
    let (code, doc, _) = s.send(&two);
    assert_eq!(
        (code, doc["error"].as_str()),
        (403, Some("run_token_required"))
    );
    // No Origin at all.
    let (code, doc, _) = s.post_as(&addr, &body, None, Some(&token));
    assert_eq!(
        (code, doc["error"].as_str()),
        (403, Some("same_origin_required"))
    );
    // A foreign Origin, token or not.
    let (code, _, raw) = s.post_as(&addr, &body, Some("http://evil.example"), Some(&token));
    assert_eq!(code, 403, "{raw}");
    let (code, _, raw) = s.post_as(
        &addr,
        &body,
        Some(&format!("http://127.0.0.1:{}", s.port + 1)),
        Some(&token),
    );
    assert_eq!(code, 403, "{raw}");
    assert_eq!(changed(&before, &m.files()), Vec::<String>::new());
    assert!(!m.hooks_file().exists());
    assert!(!m.mcp_file().exists());
    assert!(!m.shim_dir().exists());
}

#[test]
fn the_body_is_exactly_the_choices() {
    let m = Machine::new();
    let s = Server::start(&m);
    let plan = s.plan();
    let token = s.token();
    let addr = s.address();
    let before = m.files();
    let id = plan["plan"].as_str().expect("plan").to_string();
    for (body, code, error) in [
        (
            serde_json::json!({"hooks": true, "mcp": true, "speaker": false, "run_capture": false, "plan": id, "shell": true}),
            400,
            "invalid_install_request",
        ),
        (
            serde_json::json!({"hooks": true, "mcp": true, "speaker": false, "plan": id}),
            400,
            "invalid_install_request",
        ),
        (
            serde_json::json!({"hooks": false, "mcp": true, "speaker": false, "run_capture": false, "plan": id}),
            400,
            "hooks_required",
        ),
        (
            serde_json::json!({"hooks": true, "mcp": "yes", "speaker": false, "run_capture": false, "plan": id}),
            400,
            "invalid_install_request",
        ),
    ] {
        let (got, doc, _) = s.post_as(&addr, &body.to_string(), Some(&s.origin()), Some(&token));
        assert_eq!((got, doc["error"].as_str()), (code, Some(error)), "{body}");
    }
    let big = format!("{{\"hooks\":true,\"plan\":\"{}\"}}", "a".repeat(4096));
    let (got, _, _) = s.post_as(&addr, &big, Some(&s.origin()), Some(&token));
    assert_eq!(got, 413);
    assert_eq!(s.get(&format!("{addr}?x=1")).0, 400);
    let (got, doc) = s.get("/api/hq/projects/nope/install");
    assert_eq!((got, doc["error"].as_str()), (404, Some("no_such_project")));
    let (got, _, _) = s.send(&format!(
        "PUT {addr} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
        s.port
    ));
    assert_eq!(got, 405);
    assert_eq!(changed(&before, &m.files()), Vec::<String>::new());
}

#[test]
fn a_stale_plan_is_refused_and_writes_nothing() {
    let m = Machine::new();
    let s = Server::start(&m);
    let plan = s.plan();
    // The project's settings change after the plan was read.
    std::fs::create_dir_all(m.hooks_file().parent().expect("parent")).expect("dir");
    std::fs::write(m.hooks_file(), "{\"permissions\": {}}\n").expect("settings");
    let before = m.files();
    let body = serde_json::json!({
        "hooks": true, "mcp": false, "speaker": false, "run_capture": false, "plan": plan["plan"],
    })
    .to_string();
    let token = s.token();
    let (code, doc, _) = s.post_as(&s.address(), &body, Some(&s.origin()), Some(&token));
    assert_eq!(code, 409, "{doc}");
    assert_eq!(doc["error"], "plan_changed", "{doc}");
    assert_ne!(doc["plan"]["plan"], plan["plan"], "{doc}");
    assert_eq!(changed(&before, &m.files()), Vec::<String>::new());
    // Installing against the fresh plan keeps what the person had there.
    let (code, doc) = s.install(false, false, false);
    assert_eq!(code, 200, "{doc}");
    let written = json_of(&m.hooks_file());
    assert_eq!(written["permissions"], serde_json::json!({}), "{written}");
    assert!(written["hooks"].is_object(), "{written}");
}

#[test]
fn a_symlinked_settings_file_or_folder_is_refused_and_its_target_untouched() {
    let m = Machine::new();
    let elsewhere = m.dir.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).expect("elsewhere");
    let target = elsewhere.join("settings.json");
    std::fs::write(&target, "{}\n").expect("target");
    std::fs::create_dir_all(m.repo_path().join(".claude")).expect(".claude");
    std::os::unix::fs::symlink(&target, m.hooks_file()).expect("symlink");
    let mcp_target = elsewhere.join("mcp.json");
    std::fs::write(&mcp_target, "{}\n").expect("mcp target");
    std::os::unix::fs::symlink(&mcp_target, m.mcp_file()).expect("symlink");
    let s = Server::start(&m);
    let plan = s.plan();
    for part in ["hooks", "speaker", "mcp"] {
        assert_eq!(plan["parts"][part]["state"], "refused", "{part}: {plan}");
        assert_eq!(
            plan["parts"][part]["refusal"]["code"], "symlink",
            "{part}: {plan}"
        );
    }
    let before = m.files();
    let (code, doc) = s.install(true, true, false);
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["changed"], false, "{doc}");
    for part in ["hooks", "speaker", "mcp"] {
        assert_eq!(doc["parts"][part]["outcome"], "refused", "{part}: {doc}");
    }
    assert_eq!(changed(&before, &m.files()), Vec::<String>::new());
    assert_eq!(std::fs::read_to_string(&target).expect("target"), "{}\n");
    assert_eq!(
        std::fs::read_to_string(&mcp_target).expect("target"),
        "{}\n"
    );

    // A `.claude` folder that is itself a link is refused the same way.
    let m = Machine::new();
    let away = m.dir.path().join("away");
    std::fs::create_dir_all(&away).expect("away");
    std::os::unix::fs::symlink(&away, m.repo_path().join(".claude")).expect("symlink");
    let s = Server::start(&m);
    let plan = s.plan();
    assert_eq!(
        plan["parts"]["hooks"]["refusal"]["code"], "symlink",
        "{plan}"
    );
    let (code, doc) = s.install(false, false, false);
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["parts"]["hooks"]["outcome"], "refused", "{doc}");
    assert_eq!(std::fs::read_dir(&away).expect("away").count(), 0);
}

#[test]
fn hooks_already_installed_for_every_folder_are_left_alone() {
    let m = Machine::new();
    let out = m.cmd(&["sensor", "install"]).output().expect("install");
    assert!(out.status.success(), "{out:?}");
    let user_before = std::fs::read(m.user_settings()).expect("user settings");
    let s = Server::start(&m);
    let plan = s.plan();
    assert_eq!(plan["parts"]["hooks"]["state"], "installed", "{plan}");
    assert_eq!(
        plan["parts"]["hooks"]["where"],
        serde_json::json!(["user"]),
        "{plan}"
    );
    let before = m.files();
    let (code, doc) = s.install(false, false, false);
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["parts"]["hooks"]["outcome"], "already", "{doc}");
    assert_eq!(doc["changed"], false, "{doc}");
    assert_eq!(changed(&before, &m.files()), Vec::<String>::new());
    assert_eq!(
        std::fs::read(m.user_settings()).expect("user settings"),
        user_before
    );
}

#[test]
fn another_recorder_refuses_the_hooks_and_writes_nothing() {
    let m = Machine::new();
    let out = m
        .cmd(&["hook-install", "--scope", "user"])
        .output()
        .expect("hook-install");
    assert!(out.status.success(), "{out:?}");
    let s = Server::start(&m);
    let plan = s.plan();
    assert_eq!(plan["parts"]["hooks"]["state"], "refused", "{plan}");
    assert_eq!(
        plan["parts"]["hooks"]["refusal"]["code"], "another_recorder",
        "{plan}"
    );
    let before = m.files();
    let (code, doc) = s.install(false, false, false);
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["parts"]["hooks"]["outcome"], "refused", "{doc}");
    assert_eq!(changed(&before, &m.files()), Vec::<String>::new());
}

#[test]
fn after_the_install_the_capture_route_reads_installed() {
    let m = Machine::new();
    let s = Server::start(&m);
    let id = s.project_id();
    let (_, doc) = s.get(&format!("/api/hq/projects/{id}/capture"));
    assert_eq!(doc["claude_code_hooks"]["installed"], false, "{doc}");
    assert_eq!(s.install(false, false, false).0, 200);
    let (code, doc) = s.get(&format!("/api/hq/projects/{id}/capture"));
    assert_eq!(code, 200, "{doc}");
    let hooks = &doc["claude_code_hooks"];
    assert_eq!(hooks["installed"], true, "{doc}");
    assert_eq!(hooks["alive"], true, "{doc}");
    let local = hooks["levels"]
        .as_array()
        .and_then(|l| l.iter().find(|l| l["scope"] == "project-local"))
        .expect("project-local level");
    assert_eq!(local["state"], "read", "{doc}");
    assert_eq!(
        local["commands"].as_array().map(Vec::len),
        Some(thruline_core::sensor::install_set().len()),
        "{doc}"
    );
}

#[test]
fn a_store_other_than_the_one_the_hooks_write_refuses_hooks_and_speakers() {
    let m = Machine::new();
    let other = m.dir.path().join("other/ledger.db");
    std::fs::create_dir_all(other.parent().expect("parent")).expect("dir");
    std::fs::copy(m.db(), &other).expect("copy store");
    let wal = m.db().with_extension("db-wal");
    if wal.exists() {
        std::fs::copy(&wal, other.with_extension("db-wal")).expect("copy wal");
    }
    let s = Server::start_with(&m, &["--db", &other.display().to_string()]);
    let plan = s.plan();
    for part in ["hooks", "speaker"] {
        assert_eq!(plan["parts"][part]["state"], "refused", "{part}: {plan}");
        assert_eq!(
            plan["parts"][part]["refusal"]["code"], "other_store",
            "{part}: {plan}"
        );
    }
    assert_eq!(plan["parts"]["mcp"]["state"], "will_add", "{plan}");
    let before = m.files();
    let (code, doc) = s.install(true, true, false);
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["parts"]["hooks"]["outcome"], "refused", "{doc}");
    assert_eq!(doc["parts"]["speaker"]["outcome"], "refused", "{doc}");
    assert_eq!(doc["parts"]["mcp"]["outcome"], "wrote", "{doc}");
    let moved = changed(&before, &m.files());
    assert_eq!(moved, vec![format!("repo/{MCP_FILE}")], "{moved:?}");
    // The MCP server reads the store this HQ reads.
    let mcp = json_of(&m.mcp_file());
    assert_eq!(
        mcp["mcpServers"]["thruline"]["args"][4],
        other.display().to_string()
    );
}

#[test]
fn the_ledger_is_not_written_by_a_plan_or_an_install() {
    let m = Machine::new();
    let s = Server::start(&m);
    s.plan();
    let wal = m.db().with_extension("db-wal");
    let before = (sha256(&m.db()), sha256(&wal));
    assert!(before.0.is_some());
    s.plan();
    assert_eq!(s.install(true, true, true).0, 200);
    s.plan();
    assert_eq!((sha256(&m.db()), sha256(&wal)), before);
}

#[test]
fn the_terminal_undo_it_names_removes_exactly_what_it_wrote() {
    let m = Machine::new();
    std::fs::create_dir_all(m.repo_path().join(".claude")).expect(".claude");
    std::fs::write(
        m.hooks_file(),
        "{\"permissions\": {\"allow\": [\"Bash(ls)\"]}}\n",
    )
    .expect("settings");
    std::fs::write(
        m.mcp_file(),
        "{\"mcpServers\": {\"other\": {\"command\": \"x\"}}}\n",
    )
    .expect("mcp");
    let s = Server::start(&m);
    let (code, doc) = s.install(true, true, false);
    assert_eq!(code, 200, "{doc}");
    let undo = s.plan()["undo"]["project"]
        .as_str()
        .expect("an undo for the project's files")
        .to_string();
    assert_eq!(
        undo,
        "thruline init --undo --settings .claude/settings.local.json --claude-json .mcp.json"
    );
    let args: Vec<&str> = undo.split(' ').skip(1).collect();
    let out = m.cmd(&args).output().expect("undo");
    assert!(out.status.success(), "{out:?}");
    let hooks = json_of(&m.hooks_file());
    assert_eq!(hooks["permissions"]["allow"][0], "Bash(ls)", "{hooks}");
    assert!(
        !hooks.to_string().contains("thruline"),
        "a ThruLine hook survived the undo: {hooks}"
    );
    let mcp = json_of(&m.mcp_file());
    assert_eq!(
        mcp,
        serde_json::json!({"mcpServers": {"other": {"command": "x"}}})
    );
}

/* ───────── C46 §1.2(2): Settings' speaker switch, Off (write-address row 30) ───────── */

impl Server {
    fn uninstall_address(&self) -> String {
        format!("/api/hq/projects/{}/uninstall", self.project_id())
    }

    /// Take the speaker hooks back against the plan read just now.
    fn take_speaker_back(&self) -> (u16, Value) {
        let plan = self.plan();
        let body = serde_json::json!({"part": "speaker", "plan": plan["plan"]}).to_string();
        let token = self.token();
        let (code, doc, _) = self.post_as(
            &self.uninstall_address(),
            &body,
            Some(&self.origin()),
            Some(&token),
        );
        (code, doc)
    }
}

const SPEAKER_ENTRIES: [&str; 3] = [
    "SessionStart brief --hook session-start",
    "Stop audit --hook stop",
    "SessionEnd digest precompute --hook session-end",
];

#[test]
fn speaker_off_takes_out_exactly_the_three_speaker_hooks_and_nothing_else() {
    let m = Machine::new();
    std::fs::create_dir_all(m.repo_path().join(".claude")).expect(".claude");
    std::fs::write(
        m.hooks_file(),
        "{\"permissions\": {\"allow\": [\"Bash(ls)\"]}}\n",
    )
    .expect("settings");
    let s = Server::start(&m);
    assert_eq!(s.install(false, false, false).0, 200);
    let hooks_only = json_of(&m.hooks_file());
    let (code, doc) = s.install(false, true, false);
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["parts"]["speaker"]["outcome"], "wrote", "{doc}");
    assert_eq!(s.plan()["parts"]["speaker"]["state"], "installed");

    let wal = m.db().with_extension("db-wal");
    let ledger = (sha256(&m.db()), sha256(&wal));
    let before = m.files();
    let (code, doc) = s.take_speaker_back();
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["changed"], true, "{doc}");
    assert_eq!(doc["part"], "speaker", "{doc}");
    assert_eq!(doc["file"], HOOKS_FILE, "{doc}");
    assert_eq!(doc["removed"], serde_json::json!(SPEAKER_ENTRIES), "{doc}");
    // exactly one file changed, and it is what it was before the speakers went in
    assert_eq!(
        changed(&before, &m.files()),
        vec![format!("repo/{HOOKS_FILE}")]
    );
    assert_eq!(
        json_of(&m.hooks_file()),
        hooks_only,
        "only the speakers came out"
    );
    assert_eq!(
        doc["after"]["parts"]["speaker"]["state"], "will_add",
        "{doc}"
    );
    assert_eq!(
        doc["after"]["parts"]["hooks"]["state"], "installed",
        "{doc}"
    );
    // the record is not written, and the user's own settings never were
    assert_eq!((sha256(&m.db()), sha256(&wal)), ledger);
    assert!(!m.user_settings().exists());

    // a second press: there is nothing left to take back, and nothing changes
    let before = m.files();
    let (code, doc) = s.take_speaker_back();
    assert_eq!(
        (code, doc["error"].as_str()),
        (409, Some("not_installed")),
        "{doc}"
    );
    assert!(
        doc["plain"]
            .as_str()
            .is_some_and(|p| p.ends_with("Nothing changed.")),
        "{doc}"
    );
    assert_eq!(changed(&before, &m.files()), Vec::<String>::new());
    // and On again puts back the same three
    let (code, doc) = s.install(false, true, false);
    assert_eq!(code, 200, "{doc}");
    assert_eq!(doc["parts"]["speaker"]["outcome"], "wrote", "{doc}");
}

#[test]
fn speaker_off_needs_this_page_its_token_the_plan_and_exactly_its_body() {
    let m = Machine::new();
    let s = Server::start(&m);
    let (code, doc) = s.install(false, true, false);
    assert_eq!(code, 200, "{doc}");
    let plan = s.plan();
    let id = plan["plan"].as_str().expect("plan").to_string();
    let token = s.token();
    let addr = s.uninstall_address();
    let body = serde_json::json!({"part": "speaker", "plan": id}).to_string();
    let before = m.files();
    let (code, doc, _) = s.post_as(&addr, &body, Some(&s.origin()), None);
    assert_eq!(
        (code, doc["error"].as_str()),
        (403, Some("run_token_required"))
    );
    let (code, doc, _) = s.post_as(&addr, &body, None, Some(&token));
    assert_eq!(
        (code, doc["error"].as_str()),
        (403, Some("same_origin_required"))
    );
    let (code, _, raw) = s.post_as(&addr, &body, Some("http://evil.example"), Some(&token));
    assert_eq!(code, 403, "{raw}");
    for (b, code, error) in [
        (
            serde_json::json!({"part": "speaker", "plan": "0"}),
            409,
            "plan_changed",
        ),
        (
            serde_json::json!({"part": "speaker", "plan": id, "file": "x"}),
            400,
            "invalid_uninstall_request",
        ),
        (
            serde_json::json!({"part": "speaker"}),
            400,
            "invalid_uninstall_request",
        ),
        (
            serde_json::json!({"part": "hooks", "plan": id}),
            400,
            "unknown_part",
        ),
        (
            serde_json::json!({"part": "mcp", "plan": id}),
            400,
            "unknown_part",
        ),
    ] {
        let (got, doc, _) = s.post_as(&addr, &b.to_string(), Some(&s.origin()), Some(&token));
        assert_eq!(
            (got, doc["error"].as_str()),
            (code, Some(error)),
            "{b}: {doc}"
        );
    }
    assert_eq!(s.get(&addr).0, 405);
    assert_eq!(s.get(&format!("{addr}?x=1")).0, 400);
    let (got, doc, _) = s.post_as(
        "/api/hq/projects/nope/uninstall",
        &body,
        Some(&s.origin()),
        Some(&token),
    );
    assert_eq!((got, doc["error"].as_str()), (404, Some("no_such_project")));
    assert_eq!(changed(&before, &m.files()), Vec::<String>::new());
    assert_eq!(s.plan()["parts"]["speaker"]["state"], "installed");
}

#[test]
fn speaker_hooks_turned_on_at_another_level_are_never_touched() {
    let m = Machine::new();
    // ThruLine's three speaker hooks in the user's own settings, as `thruline init` writes them there
    let exe = exe();
    let entry = |verb: &str| serde_json::json!([{"hooks": [{"type": "command", "command": format!("{exe} {verb}"), "timeout": 5}]}]);
    let user = serde_json::json!({"hooks": {
        "SessionStart": entry("brief --hook session-start"),
        "Stop": entry("audit --hook stop"),
        "SessionEnd": entry("digest precompute --hook session-end"),
    }});
    std::fs::create_dir_all(m.user_settings().parent().expect("parent")).expect("dir");
    std::fs::write(
        m.user_settings(),
        serde_json::to_string_pretty(&user).expect("json"),
    )
    .expect("user settings");
    let user_before = std::fs::read(m.user_settings()).expect("user settings");
    let s = Server::start(&m);
    let plan = s.plan();
    assert_eq!(plan["parts"]["speaker"]["state"], "installed", "{plan}");
    assert_eq!(
        plan["parts"]["speaker"]["where"],
        serde_json::json!(["user"]),
        "{plan}"
    );
    let before = m.files();
    let (code, doc) = s.take_speaker_back();
    assert_eq!(
        (code, doc["error"].as_str()),
        (409, Some("elsewhere")),
        "{doc}"
    );
    assert_eq!(doc["where"], serde_json::json!(["user"]), "{doc}");
    assert!(
        doc["plain"].as_str().is_some_and(
            |p| p.contains("user Claude Code settings") && p.ends_with("Nothing changed here.")
        ),
        "{doc}"
    );
    assert_eq!(changed(&before, &m.files()), Vec::<String>::new());
    assert_eq!(
        std::fs::read(m.user_settings()).expect("user settings"),
        user_before
    );
}

#[test]
fn speaker_off_refuses_when_another_settings_level_cannot_be_read() {
    let m = Machine::new();
    let s = Server::start(&m);
    assert_eq!(s.install(false, true, false).0, 200);
    std::fs::create_dir_all(m.user_settings().parent().expect("parent")).expect("dir");
    std::fs::write(m.user_settings(), "not JSON").expect("user settings");
    let before = m.files();

    // The project's copy is on, but an unreadable user level might also hold
    // a speaker. The switch cannot promise Off until every level is known.
    let (code, doc) = s.take_speaker_back();
    assert_eq!(
        (code, doc["error"].as_str()),
        (409, Some("refused")),
        "{doc}"
    );
    assert!(
        doc["plain"].as_str().is_some_and(
            |p| p.contains("could not be read") && p.ends_with("Nothing changed here.")
        ),
        "{doc}"
    );
    assert_eq!(changed(&before, &m.files()), Vec::<String>::new());
    assert_eq!(s.plan()["parts"]["speaker"]["state"], "installed");
}
