//! C51: the weekly reading and indexed finding feed against the shipping HQ
//! server, on a synthetic store under a temporary HOME.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;
use sha2::{Digest, Sha256};

struct Machine(tempfile::TempDir);
impl Machine {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("h/.thruline")).unwrap();
        let export = dir.path().join("h/.thruline/export");
        std::fs::create_dir_all(&export).unwrap();
        std::fs::write(export.join("index.json"), b"{}").unwrap();
        std::fs::create_dir_all(dir.path().join("repo/.git")).unwrap();
        let me = Self(dir);
        let cwd = me.0.path().join("repo").display().to_string();
        let project = me.0.path().join("h/.claude/projects").join(
            cwd.chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect::<String>(),
        );
        std::fs::create_dir_all(&project).unwrap();
        let line = serde_json::json!({
            "type": "user", "uuid": "s1-1", "parentUuid": null, "isSidechain": false,
            "sessionId": "s1", "timestamp": thruline_core::ingest::iso8601_utc_now(), "cwd": cwd,
            "message": {"role": "user", "content": "Fix the parser."}
        });
        std::fs::write(project.join("s1.jsonl"), format!("{line}\n")).unwrap();
        assert!(me
            .command(&["ingest", "--apply", "--source", "claude-code"])
            .arg(&project)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success());
        me
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_thruline"));
        cmd.args(args)
            .env("HOME", self.0.path().join("h"))
            .env("THRULINE_HOME", self.0.path().join("h/.thruline"))
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .current_dir(self.0.path().join("repo"));
        cmd
    }
    fn record_digest(&self) -> Vec<u8> {
        let mut hash = Sha256::new();
        for name in ["ledger.db", "ledger.db-wal"] {
            hash.update(name.as_bytes());
            hash.update(
                std::fs::read(self.0.path().join("h/.thruline").join(name)).unwrap_or_default(),
            );
        }
        hash.finalize().to_vec()
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
    fn start(machine: &Machine) -> Self {
        let mut child = machine
            .command(&["ui", "--no-open", "--port", "0"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        for line in BufReader::new(stdout).lines() {
            if let Some(port) = line
                .unwrap()
                .trim()
                .strip_prefix("ThruLine is at http://127.0.0.1:")
            {
                let port = port
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .unwrap();
                let server = Self { child, port };
                let until = Instant::now() + Duration::from_secs(120);
                loop {
                    if server.get("/api/hq/projects").1["complete"] == true {
                        return server;
                    }
                    assert!(Instant::now() < until, "HQ did not complete its first read");
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("HQ did not print an address");
    }
    fn send(&self, method: &str, path: &str) -> (u16, Value) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(60)))
            .unwrap();
        stream
            .write_all(
                format!(
                    "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
                    self.port
                )
                .as_bytes(),
            )
            .unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let code = text.get(9..12).unwrap_or("0").parse().unwrap();
        let body = text.split_once("\r\n\r\n").map(|x| x.1).unwrap_or("");
        (code, serde_json::from_str(body).unwrap_or(Value::Null))
    }
    fn get(&self, path: &str) -> (u16, Value) {
        self.send("GET", path)
    }
}

#[test]
fn summary_and_finding_feed_read_a_real_store_without_changing_it() {
    let machine = Machine::new();
    let server = Server::start(&machine);
    let before = machine.record_digest();
    let (code, summary) = server.get("/api/hq/weekly-summary");
    assert_eq!(code, 200, "{summary}");
    assert_eq!(summary["schema_version"], 1);
    assert_eq!(summary["projects_recorded"], 1);
    for key in ["approvals", "not_backed", "rechecks"] {
        assert_eq!(summary[key], 0, "{summary}");
    }
    assert_eq!(server.get("/api/hq/weekly-summary?unexpected=1").0, 400);
    assert_eq!(server.send("POST", "/api/hq/weekly-summary").0, 405);
    let (code, feed) = server.get("/api/hq/not-backed");
    assert_eq!(code, 200, "{feed}");
    assert_eq!(feed["finding_items"], serde_json::json!([]));
    assert_eq!(feed["latest_seq"], -1);
    assert_eq!(server.get("/api/hq/not-backed?after=-1").0, 200);
    assert_eq!(server.get("/api/hq/not-backed?after=nope").0, 400);
    assert_eq!(machine.record_digest(), before, "a GET changed the record");
}
