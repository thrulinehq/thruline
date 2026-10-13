//! The world the C34 pause-gate tests run in: a temporary HOME, a real ledger
//! and mirror under it, synthetic Claude Code and Codex recordings copied from
//! `crates/core/tests/fixtures/hq`, and the shipping binary driven the way
//! Claude Code and a person drive it — hook deliveries on stdin, `ingest` on
//! the command line.
//!
//! Every process this module spawns has `HOME` and `THRULINE_HOME` inside the
//! temp dir, so no real store, mirror or settings file is ever opened.

#![allow(dead_code)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{json, Value};
use thruline_core::ledger::Ledger;

pub mod digest;

/// The fixture recordings' folder, as they state it in their `cwd`.
pub const APP: &str = "/work/app";
pub const S1: &str = "cccccccc-1111-4111-8111-111111111111";
pub const S2: &str = "cccccccc-2222-4222-8222-222222222222";

pub struct World {
    root: PathBuf,
    /// Present for an ordinary world; the temp dir goes when the world does.
    _guard: Option<tempfile::TempDir>,
}

/// Where the no-op world lives. FIXED, on purpose: a Codex session's identity
/// and a hook payload's `transcript_path` both carry the path they were found
/// at, so two worlds at two paths cannot have the same hashes, and a digest
/// committed as a fixture needs a world that is always in the same place. It is
/// removed before it is built and after it is digested.
pub const FIXED_ROOT: &str = "/tmp/thruline-capture-gates-noop-world";

impl World {
    /// An ordinary world in a fresh temp dir.
    pub fn new() -> World {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        World::build(root, Some(dir))
    }

    /// The world at [`FIXED_ROOT`], rebuilt from nothing. Callers hold
    /// [`fixed_lock`] while they use it.
    pub fn fixed() -> World {
        let root = PathBuf::from(FIXED_ROOT);
        remove_tree(&root);
        std::fs::create_dir_all(&root).expect("the fixed world's directory");
        World::build(root, None)
    }

    fn build(root: PathBuf, guard: Option<tempfile::TempDir>) -> World {
        std::fs::create_dir_all(root.join("h/.thruline")).expect("home");
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/tests/fixtures/hq");
        copy_dir(
            &fixtures.join("claude/-work-app"),
            &root.join("h/.claude/projects/-work-app"),
        );
        copy_dir(
            &fixtures.join("codex"),
            &root.join("h/.codex/sessions/2026/10/04"),
        );
        World {
            root,
            _guard: guard,
        }
    }

    /// Take the world down now (the same thing dropping it does).
    pub fn clean(self) {
        drop(self);
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn home(&self) -> PathBuf {
        self.root().join("h")
    }
    pub fn db(&self) -> PathBuf {
        self.home().join(".thruline/ledger.db")
    }
    pub fn mirror(&self) -> PathBuf {
        self.root().join("mirror")
    }
    pub fn claude_dir(&self) -> PathBuf {
        self.home().join(".claude/projects/-work-app")
    }
    pub fn transcript(&self, session: &str) -> PathBuf {
        self.claude_dir().join(format!("{session}.jsonl"))
    }
    pub fn codex_dir(&self) -> PathBuf {
        self.home().join(".codex/sessions/2026/10/04")
    }

    pub fn cmd(&self, args: &[&str]) -> Command {
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
            .current_dir(self.root());
        cmd
    }

    /// One hook delivery through the real `thruline sensor append`, payload on
    /// stdin. Returns the whole process output.
    pub fn deliver(&self, event: &str, payload: &Value) -> Output {
        let db = self.db();
        let mirror = self.mirror();
        let mut child = self
            .cmd(&[
                "--db",
                db.to_str().expect("utf8"),
                "sensor",
                "append",
                event,
                "--mirror-root",
                mirror.to_str().expect("utf8"),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn the sensor");
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(payload.to_string().as_bytes())
            .expect("payload");
        child.wait_with_output().expect("wait")
    }

    /// The payload Claude Code sends for `event` in session `S1` at `cwd`.
    pub fn payload(&self, event: &str, cwd: &str) -> Value {
        json!({
            "session_id": S1,
            "transcript_path": self.transcript(S1).to_str().expect("utf8"),
            "cwd": cwd,
            "hook_event_name": event,
        })
    }

    /// `thruline ingest --apply` of a path, as a person types it.
    pub fn ingest(&self, source: &str, path: &Path) -> Output {
        let db = self.db();
        let mirror = self.mirror();
        self.cmd(&[
            "--db",
            db.to_str().expect("utf8"),
            "ingest",
            "--apply",
            "--source",
            source,
            "--mirror-root",
            mirror.to_str().expect("utf8"),
        ])
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("ingest")
    }

    /// A `thruline capture …` verb against this world's ledger.
    pub fn capture(&self, args: &[&str]) -> Output {
        let db = self.db();
        let mut all = vec!["--db", db.to_str().expect("utf8"), "capture"];
        all.extend_from_slice(args);
        self.cmd(&all)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("capture")
    }

    /// The Stop audit (`thruline audit --hook stop`), payload on stdin.
    pub fn audit(&self, payload: &Value) -> Output {
        let db = self.db();
        let mut child = self
            .cmd(&[
                "--db",
                db.to_str().expect("utf8"),
                "audit",
                "--hook",
                "stop",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn the audit");
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(payload.to_string().as_bytes())
            .expect("payload");
        child.wait_with_output().expect("wait")
    }

    /// A session whose last word is "All 12 tests pass." with nothing in the
    /// record to back it, audited at its Stop: with no pause, exactly one
    /// finding is appended to `speaker_findings`.
    pub fn audit_a_claim(&self, cwd: &str) -> Output {
        let session = "dddddddd-1111-4111-8111-111111111111";
        let dir = self.root().join("audit/projects/-work-app");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let transcript = dir.join(format!("{session}.jsonl"));
        let lines = [
            json!({
                "type": "user", "uuid": "u1", "isSidechain": false, "sessionId": session,
                "timestamp": "2026-08-31T03:00:01.000Z", "cwd": cwd,
                "message": {"role": "user", "content": "finish it"},
            }),
            json!({
                "type": "assistant", "uuid": "a1", "isSidechain": false, "sessionId": session,
                "timestamp": "2026-08-31T03:00:02.000Z", "cwd": cwd,
                "message": {"role": "assistant", "content": [{"type": "text", "text": "All 12 tests pass."}]},
            }),
        ];
        let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
        std::fs::write(&transcript, text).expect("write the transcript");
        self.audit(&json!({
            "session_id": session,
            "transcript_path": transcript.to_str().expect("utf8"),
            "cwd": cwd,
            "hook_event_name": "Stop",
            "stop_hook_active": false,
        }))
    }

    pub fn ledger(&self) -> Ledger {
        Ledger::open(self.db()).expect("open the ledger")
    }

    pub fn count(&self, table: &str) -> i64 {
        self.ledger()
            .conn()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap_or(0)
    }
}

/// A mirror seals its generations read-only, and `TempDir`'s own destructor
/// ignores the `EACCES` that leaves behind, so a world removes itself first.
impl Drop for World {
    fn drop(&mut self) {
        remove_tree(&self.root);
    }
}

/// Remove a directory tree the mirror has made read-only (its seals and
/// payloads are immutable: files 0444, directories 0555), by making it writable
/// first. Without this a "removed" world stays behind and the next one is built
/// on top of it.
fn remove_tree(root: &Path) {
    use std::os::unix::fs::PermissionsExt;
    fn open_up(dir: &Path) {
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755));
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.filter_map(|e| e.ok()) {
                let p = e.path();
                match e.file_type() {
                    Ok(t) if t.is_dir() => open_up(&p),
                    Ok(t) if t.is_file() => {
                        let _ =
                            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644));
                    }
                    _ => {}
                }
            }
        }
    }
    if root.exists() {
        open_up(root);
        let _ = std::fs::remove_dir_all(root);
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for entry in std::fs::read_dir(from).expect("fixtures") {
        let entry = entry.expect("entry");
        std::fs::copy(entry.path(), to.join(entry.file_name())).expect("copy");
    }
}

/// The fixed sequence both the no-op test and the paused test run: seven hook
/// deliveries covering every kind the gate sees (PreCompact seals a transcript,
/// SessionEnd would remove a marker), then one `thruline ingest` of the Claude
/// Code project and one of the Codex rollout.
/// One lock for every test that uses the fixed world: they would otherwise
/// rebuild it under one another.
pub fn fixed_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn run_sequence(w: &World, cwd: &str) -> Vec<(String, Output)> {
    let mut outs = Vec::new();
    for event in [
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PreCompact",
        "Stop",
        "SessionEnd",
    ] {
        let out = w.deliver(event, &w.payload(event, cwd));
        outs.push((format!("sensor append {event}"), out));
    }
    outs.push(("audit --hook stop".to_string(), w.audit_a_claim(cwd)));
    outs.push((
        "ingest claude-code".to_string(),
        w.ingest("claude-code", &w.claude_dir()),
    ));
    outs.push((
        "ingest codex".to_string(),
        w.ingest("codex", &w.codex_dir()),
    ));
    outs
}
