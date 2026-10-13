//! Shared harness for the AI-REVIEW-OUT-OF-LAUNCH tests: the synthetic
//! repository and the runtime and project registries
//! (`ai_review_out_of_launch.rs`, which pins the launch build, and
//! `ai_review_feature_unchanged.rs`, which pins the `ai-review` build), the
//! compiled binary driven as a terminal and as an HQ server, and the ids the
//! rounds attack it with. Moved here unchanged from
//! `ai_review_out_of_launch.rs` (AI-REVIEW-OUT-OF-LAUNCH-R5) so the two builds
//! are driven by the same fixture and the same inputs.
//!
//! Every fixture is synthetic.
#![allow(dead_code)]

use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub struct Fixture {
    pub _temp: tempfile::TempDir,
    pub root: PathBuf,
    pub base: String,
    pub db: PathBuf,
    pub exec: PathBuf,
    pub home: PathBuf,
    pub registry: PathBuf,
    pub projects: PathBuf,
}

pub fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

impl Fixture {
    pub fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("repo");
        std::fs::create_dir(&root).expect("repo");
        let root = root.canonicalize().expect("canonical");
        let home = temp.path().join("home");
        std::fs::create_dir(&home).expect("home");
        git(&root, &["init", "-q", "-b", "main"]);
        std::fs::write(root.join("source.js"), "const value = 1;\n").expect("write");
        git(&root, &["add", "."]);
        git(&root, &["commit", "-qm", "base"]);
        let base = git(&root, &["rev-parse", "HEAD"]);
        std::fs::write(root.join("source.js"), "const value = 2;\n").expect("write");
        git(&root, &["commit", "-qam", "the change"]);

        std::fs::create_dir(temp.path().join("policies")).expect("policies");
        std::fs::write(temp.path().join("policies/no-eval.yml"), "fixture rule").expect("rule");
        let echo = thruline_core::chain::sha256_hex(&std::fs::read("/bin/echo").expect("echo"));
        let registry = temp.path().join("registry.json");
        std::fs::write(
            &registry,
            serde_json::to_vec(&json!({
                "schema": "thruline.integration-runtimes.v1",
                "tools": {
                    "ast-grep": {"version": "0.45.3", "executable": "/bin/echo", "sha256": echo, "runtime": [], "prefix": []},
                    "pr-agent": {"version": "0.45.0", "executable": "/bin/echo", "sha256": echo, "runtime": [], "prefix": []},
                },
            }))
            .expect("json"),
        )
        .expect("registry");
        let exec = temp.path().join("exec.db");
        let projects = temp.path().join("projects.json");
        std::fs::write(
            &projects,
            serde_json::to_vec(&json!({
                "schema": "thruline.review-projects.v1",
                "projects": [{"id": "t", "label": "Synthetic", "root": root, "exec_db": exec, "base_commit": base}],
            }))
            .expect("json"),
        )
        .expect("projects");
        Self {
            db: temp.path().join("core.db"),
            _temp: temp,
            root,
            base,
            exec,
            home,
            registry,
            projects,
        }
    }

    pub fn cmd(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_thruline"));
        c.env("HOME", &self.home)
            .env("THRULINE_HOME", self.home.join(".thruline"))
            .env("THRULINE_INTEGRATION_REGISTRY", &self.registry)
            .env("THRULINE_REVIEW_PROJECTS", &self.projects)
            .current_dir(&self.root)
            .arg("--db")
            .arg(&self.db);
        c
    }

    pub fn run(&self, args: &[&str]) -> (i32, String, String) {
        let out = self.cmd().args(args).output().expect("run thruline");
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    pub fn review(&self, action: &str, mode: &str, digest: &str) -> (i32, String, String) {
        let root = self.root.display().to_string();
        let exec = self.exec.display().to_string();
        self.run(&[
            "integration",
            "review",
            "--root",
            &root,
            "--exec-db",
            &exec,
            "--action",
            action,
            "--mode",
            mode,
            "--digest",
            digest,
        ])
    }
}

/* ── the HQ server ─────────────────────────────────────────────────────── */

pub struct Server {
    pub child: Child,
    pub port: u16,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    pub fn start(f: &Fixture) -> Self {
        let mut child = f
            .cmd()
            .args(["ui", "--no-open", "--port", "0"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
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
            match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(line) => {
                    if let Some(rest) = line.trim().strip_prefix("ThruLine is at http://127.0.0.1:")
                    {
                        let port = rest
                            .trim()
                            .split('/')
                            .next()
                            .unwrap_or_default()
                            .parse()
                            .expect("port");
                        return Self { child, port };
                    }
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("the server never printed its address");
                }
            }
        }
    }

    pub fn request(&self, verb: &str, path: &str, body: Option<&str>) -> (u16, String) {
        self.try_request(verb, path, body)
            .expect("header terminator")
    }

    /// As `request`, but a connection the server closes without answering
    /// (a registry it cannot read drops the connection on the listing routes)
    /// is `None` rather than a panic.
    pub fn try_request(&self, verb: &str, path: &str, body: Option<&str>) -> Option<(u16, String)> {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        let head = match body {
            Some(b) => format!(
                "{verb} {path} HTTP/1.1\r\nHost: 127.0.0.1:{p}\r\nOrigin: http://127.0.0.1:{p}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
                b.len(),
                p = self.port
            ),
            None => format!(
                "{verb} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
                self.port
            ),
        };
        stream.write_all(head.as_bytes()).expect("write");
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).expect("read");
        let split = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
        let status = String::from_utf8_lossy(&raw[..split])
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        Some((
            status,
            String::from_utf8_lossy(&raw[split + 4..]).to_string(),
        ))
    }
}

impl Fixture {
    /// Rewrite the operator's project registry so project `t` explicitly
    /// selects `recipes` (the adversary's round-2 attack 7).
    pub fn select(&self, recipes: &[&str]) {
        std::fs::write(
            &self.projects,
            serde_json::to_vec(&json!({
                "schema": "thruline.review-projects.v1",
                "projects": [{"id": "t", "label": "Synthetic", "root": self.root, "exec_db": self.exec,
                               "base_commit": self.base, "recipes": recipes}],
            }))
            .expect("json"),
        )
        .expect("projects");
    }

    /// Rewrite the registry so project `t` carries `extra` fields beside its
    /// own (a `recipes` of the wrong kind, an unknown field).
    pub fn registry_with(&self, extra: serde_json::Value) {
        let mut project = json!({"id": "t", "label": "Synthetic", "root": self.root,
                                  "exec_db": self.exec, "base_commit": self.base});
        for (k, v) in extra.as_object().expect("an object") {
            project[k] = v.clone();
        }
        std::fs::write(
            &self.projects,
            serde_json::to_vec(&json!({
                "schema": "thruline.review-projects.v1",
                "projects": [project],
            }))
            .expect("json"),
        )
        .expect("projects");
    }
}

/* ── the ids the rounds attack it with ─────────────────────────────────── */

/// Round 4's spellings of the withheld review's id: case, separator, padding,
/// an en dash, no separator, and full-width letters. (Copied from
/// `ai_review_out_of_launch.rs`, whose own `SPELLINGS` a test there holds equal
/// to this list, so the two builds are driven by one set of inputs.)
pub const ROUND_4: &[&str] = &[
    "PR-Agent",
    "pr_agent",
    "PR_AGENT",
    " pr-agent ",
    "pr\u{2013}agent",
    "pragent",
    "\u{ff30}\u{ff32}\u{ff0d}\u{ff21}gent",
];

/// Round 5: what the round-4 adversary served through the CLI preview and the
/// HQ route (launch gates/adversary-AI-REVIEW-OUT-OF-LAUNCH-R4). The forms NFKC
/// folds to the id (math bold, math sans-bold, double-struck, circled,
/// superscript), the ones it does not (small caps, Cyrillic homoglyphs, Greek
/// rho), and ASCII names that contain it or point at it.
pub const ROUND_5: &[&str] = &[
    // math bold
    "\u{1d429}\u{1d42b}-\u{1d41a}\u{1d420}\u{1d41e}\u{1d427}\u{1d42d}",
    // math sans-bold
    "\u{1d5fd}\u{1d5ff}-\u{1d5ee}\u{1d5f4}\u{1d5f2}\u{1d5fb}\u{1d601}",
    // double-struck
    "\u{1d561}\u{1d563}-\u{1d552}\u{1d558}\u{1d556}\u{1d55f}\u{1d565}",
    // circled
    "\u{24df}\u{24e1}-\u{24d0}\u{24d6}\u{24d4}\u{24dd}\u{24e3}",
    // superscript
    "\u{1d56}\u{2b3}-\u{1d43}\u{1d4d}\u{1d49}\u{207f}\u{1d57}",
    // small caps
    "\u{1d18}\u{280}-\u{1d00}\u{262}\u{1d07}\u{274}\u{1d1b}",
    // Cyrillic homoglyphs (er, a)
    "\u{440}r-\u{430}gent",
    // Greek rho
    "\u{3c1}r-agent",
    // ASCII names that contain it, or point at it
    "pr-agent-review",
    "qodo-merge/pr-agent",
    "pr-agent@0.45.0",
    "pr%2Dagent",
    "codium-pr-agent",
    "qodo-merge",
    "ai-reviewer",
];

/// Not a spelling of the withheld id at all, and not an id this build ships.
pub const NOT_A_SPELLING: &[&str] = &["made-up-tool"];

impl Fixture {
    /// How many times `needle` is in any file under the fixture's temporary
    /// directory (the repository, the stores and artifact directories the doors
    /// write, the home), with each of `fixed` taken out of a file first. The
    /// operator's own inputs are left alone: the two registries, and the
    /// `inputs/` directory a test puts its plan, cases, results and report in
    /// (AI-REVIEW-OUT-OF-LAUNCH-R6: "no id may appear in any file written").
    pub fn written_hits(&self, needle: &str, fixed: &[&str]) -> usize {
        let skip = [
            self.registry.clone(),
            self.projects.clone(),
            self._temp.path().join("inputs"),
        ];
        fn walk(dir: &Path, needle: &str, fixed: &[&str], skip: &[PathBuf]) -> usize {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return 0;
            };
            let mut hits = 0;
            for entry in entries.flatten() {
                let path = entry.path();
                if skip.contains(&path) {
                    continue;
                }
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_dir() {
                    hits += walk(&path, needle, fixed, skip);
                } else if kind.is_file() {
                    if let Ok(bytes) = std::fs::read(&path) {
                        let text = fixed
                            .iter()
                            .fold(String::from_utf8_lossy(&bytes).into_owned(), |t, s| {
                                t.replace(s, "")
                            });
                        hits += text.matches(needle).count();
                    }
                }
            }
            hits
        }
        walk(self._temp.path(), needle, fixed, &skip)
    }
}
