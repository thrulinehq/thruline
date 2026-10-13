//! C4.4 — the synthetic kill-9 preparation demo (A181, corrected by P1
//! Packet 1).
//!
//! `thruline demo --recovery` runs this. So does
//! `crates/core/tests/c44_recovery_demo.rs`. **One code path** — a demo that
//! drifted from the test pinning it would be a demo of nothing, and this
//! product does not ship a claim its own suite is not checking (the A115
//! precedent, reused deliberately).
//!
//! The walk exercises local preparation and persistence on real OS and Git
//! machinery, with synthetic records and a synthetic worker process:
//!
//!   a real child process is started as a bounded synthetic worker in a real
//!     throwaway Git repo
//!     → `protect` describes it: workspace snapshot + Recovery Capsule
//!     → the harness sends SIGKILL **to its own child and to nothing else**
//!     → the liveness marker outlives the process with no close on record,
//!       which is the crash CANDIDATE shape the sensor doctor names
//!     → `rescue` prepares the successor and launches nothing
//!     → the legacy/local-only handshake runs and records its non-success
//!
//! No Claude or Codex successor is launched or interrogated. The walk cannot
//! prove recovery and must end `insufficient` with
//! `successor_evidence_missing` visible in its check details.
//!
//! **The one destructive act in this repository is here, and it is bounded by
//! construction**: [`kill_own_child`] takes a [`std::process::Child`] we
//! spawned and can kill nothing else. No pid arrives from a row, a payload, or
//! a user. A test asserts the demo module names no other kill path.

use std::path::Path;
use std::process::{Child, Command, Stdio};

use crate::error::{CoreError, Result};
use crate::ledger::Ledger;

/// What the walk produced. Every field is something the caller can print or
/// assert on — the CLI does the former, the test does the latter.
#[derive(Debug, Clone)]
pub struct RecoveryReport {
    pub project_id: String,
    /// The child's pid, killed with SIGKILL by the harness.
    pub killed_pid: u32,
    /// True when the child was gone after the kill.
    pub child_died: bool,
    /// The liveness marker left behind, with no close recorded — the crash
    /// candidate shape.
    pub crash_candidate: bool,
    pub snapshot_id: String,
    /// Dirty file entries described by the read-only snapshot. The snapshot
    /// records paths/digests, not restorable file content.
    pub snapshot_file_entries: usize,
    pub capsule_sections_with_lines: usize,
    /// The rescue preview's staged artifacts.
    pub staged: Vec<String>,
    pub handshake_coverage: String,
    pub handshake_outcome: String,
    pub handshake_reason: Option<String>,
    /// P1 metric: protocol-valid successor handshakes only.
    pub verified_continuation_rate: (u64, u64),
    /// Historical local-only rows, visible for audit and excluded from P1.
    pub legacy_local_continuity_rate: (u64, u64),
}

/// SIGKILL a child THIS process spawned. The only destructive act in the
/// codebase, and it cannot reach anything else: the argument is a handle to
/// our own child, not a pid from anywhere.
fn kill_own_child(child: &mut Child) -> Result<(u32, bool)> {
    let pid = child.id();
    child
        .kill()
        .map_err(|e| CoreError::Config(format!("could not kill the demo's own child: {e}")))?;
    let died = child.wait().map(|s| !s.success()).unwrap_or(true);
    Ok((pid, died))
}

fn git(root: &Path, args: &[&str]) -> Result<()> {
    let out = Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_AUTHOR_NAME", "demo")
        .env("GIT_AUTHOR_EMAIL", "demo@example.invalid")
        .env("GIT_COMMITTER_NAME", "demo")
        .env("GIT_COMMITTER_EMAIL", "demo@example.invalid")
        .output()
        .map_err(|e| CoreError::io(root, e))?;
    if !out.status.success() {
        return Err(CoreError::Config(format!(
            "the demo's own git setup failed: git {args:?}"
        )));
    }
    Ok(())
}

/// Run the whole walk inside `dir`. The caller owns the directory — the CLI
/// hands in a temp dir it will drop, the test hands in its own. Nothing reads
/// or writes the user's real ledger, and no network call is made.
pub fn walk(dir: &Path) -> Result<RecoveryReport> {
    let repo = dir.join("work");
    std::fs::create_dir_all(&repo).map_err(|e| CoreError::io(&repo, e))?;
    git(&repo, &["init", "-q", "-b", "main"])?;
    std::fs::write(repo.join("feature.txt"), "the committed part\n")
        .map_err(|e| CoreError::io(&repo, e))?;
    git(&repo, &["add", "."])?;
    git(&repo, &["commit", "-q", "-m", "the work so far"])?;
    // The uncommitted work a crash would otherwise strand.
    std::fs::write(
        repo.join("feature.txt"),
        "the committed part\nthe part in flight\n",
    )
    .map_err(|e| CoreError::io(&repo, e))?;
    std::fs::write(repo.join("scratch.txt"), "never committed\n")
        .map_err(|e| CoreError::io(&repo, e))?;

    let ledger = Ledger::open(dir.join("ledger.db"))?;
    let project_id = "-demo-recovery".to_string();
    let now = crate::ingest::iso8601_utc_now();
    ledger.conn().execute(
        "INSERT INTO projects(id, root_path, source, display_name, created_at)
         VALUES (?1, ?2, 'claude_code', 'recovery demo', ?3)",
        rusqlite::params![project_id, repo.display().to_string(), now],
    )?;
    ledger.conn().execute(
        "INSERT INTO sessions(id, project_id, file_path, started_at, last_seq)
         VALUES ('demo-s1', ?1, ?2, ?3, 1)",
        rusqlite::params![project_id, repo.join("s1.jsonl").display().to_string(), now],
    )?;
    ledger.conn().execute_batch(
        "INSERT INTO events(id, project_id, session_id, seq, ts, kind, text, tool_name,
                            tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
           VALUES ('demo-e1','-demo-recovery','demo-s1',0,'2026-08-14T09:00:00Z','user_msg',
                   'finish the in-flight feature and push it',NULL,NULL,'/demo/s1.jsonl',1,
                   'x','g','h1'),
                  ('demo-e2','-demo-recovery','demo-s1',1,'2026-08-14T09:01:00Z','tool_use',NULL,
                   'Bash','{\"command\":\"git push origin main\"}','/demo/s1.jsonl',2,'x','h1','h2');",
    )?;

    // ── the agent is working ──────────────────────────────────────────────
    let mut child = Command::new("sleep")
        .arg("300")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| CoreError::io(dir, e))?;

    // The sensor's liveness marker for that session, exactly as the appender
    // writes one.
    let live_dir = dir.join("sensor-live");
    std::fs::create_dir_all(&live_dir).map_err(|e| CoreError::io(&live_dir, e))?;
    let marker = live_dir.join("demo-s1.json");
    std::fs::write(
        &marker,
        serde_json::json!({
            "session_id": "demo-s1",
            "transcript_path": repo.join("s1.jsonl").display().to_string(),
            "cwd": repo.display().to_string(),
            "vendor_pid": child.id(),
            "last_event": "PostToolUse",
            "updated_at": now,
        })
        .to_string(),
    )
    .map_err(|e| CoreError::io(&marker, e))?;

    // ── protect ───────────────────────────────────────────────────────────
    let snapshot = crate::snapshot::capture(&repo, &now)?;
    crate::snapshot::store(&ledger, &snapshot)?;
    let capsule = crate::capsule::compile(&ledger, &project_id, Some(&snapshot), &now)?;
    let snapshot_file_entries =
        snapshot.tree.unstaged.len() + snapshot.tree.untracked.len() + snapshot.tree.staged.len();

    // ── the crash ─────────────────────────────────────────────────────────
    let (killed_pid, child_died) = kill_own_child(&mut child)?;

    // The marker outlives the process and no SessionEnd was ever recorded:
    // that is the crash CANDIDATE shape, in the doctor's own vocabulary.
    let close_recorded = crate::sensor::has_close(&ledger, "demo-s1")?;
    let crash_candidate = marker.exists() && !close_recorded && child_died;

    // ── rescue (prepares; launches nothing) ───────────────────────────────
    let staged_dir = dir.join("rescue");
    std::fs::create_dir_all(&staged_dir).map_err(|e| CoreError::io(&staged_dir, e))?;
    let capsule_json = staged_dir.join("capsule.json");
    let capsule_md = staged_dir.join("capsule.md");
    std::fs::write(&capsule_json, serde_json::to_string_pretty(&capsule)?)
        .map_err(|e| CoreError::io(&capsule_json, e))?;
    std::fs::write(&capsule_md, crate::capsule::render_markdown(&capsule))
        .map_err(|e| CoreError::io(&capsule_md, e))?;

    // ── local-only comparison; no successor exists in this walk ──────────
    let fresh = crate::snapshot::capture(&repo, &now)?;
    let handshake = crate::continuity::handshake(&ledger, &capsule, Some(&fresh), &now, true)?;
    let handshake_reason = handshake
        .checks
        .iter()
        .find(|check| check.name == "successor evidence")
        .map(|check| check.detail.clone());
    crate::continuity::record(&ledger, &handshake)?;
    let rate = crate::continuity::verified_continuation_rate(&ledger)?;
    let legacy_rate = crate::continuity::legacy_local_continuity_rate(&ledger)?;

    Ok(RecoveryReport {
        project_id,
        killed_pid,
        child_died,
        crash_candidate,
        snapshot_id: snapshot.id.clone(),
        snapshot_file_entries,
        capsule_sections_with_lines: capsule
            .sections
            .iter()
            .filter(|s| !s.lines.is_empty())
            .count(),
        staged: vec![
            capsule_md.display().to_string(),
            capsule_json.display().to_string(),
        ],
        handshake_coverage: handshake.coverage(),
        handshake_outcome: handshake.outcome.as_str().to_string(),
        handshake_reason,
        verified_continuation_rate: rate,
        legacy_local_continuity_rate: legacy_rate,
    })
}
