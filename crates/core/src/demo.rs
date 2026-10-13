//! The L8 walk, as a shipped capability rather than a test fixture.
//!
//! `thruline demo` runs this. So does `crates/core/tests/l8_handoff_demo.rs`.
//! **One code path**, deliberately: a demo that drifted from the test that pins
//! it would be a demo of nothing, and this product does not get to ship a claim
//! its own suite is not checking.
//!
//! The walk, end to end, with **zero seeded events**:
//!
//!   Claude-side work (real JSONL ingest)
//!     → both agents' projects resolve to ONE repo (A92 identity)
//!     → packet #1 compiled and ISSUED to the Claude-side agent
//!     → [`crate::packet::handoff`] freezes that exact issue into a document
//!     → codex-side continuation (real rollout ingest)
//!     → a REAL wrapper subprocess, accepted as an assured pair
//!     → packet #2 issued as the SUCCESSOR of packet #1
//!     → and #2's VERIFIED section renders the claim line that #1 could not.
//!
//! Everything is created inside the directory the caller hands in. Nothing
//! reads or writes the user's real ledger, and no network call is made.

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::capture::{
    accept_capture_pair, opaque_source_project_key, record_source_assurance, CaptureAcceptance,
    CaptureRequest, CaptureStrength, Collector, SourceAssuranceMeasurement, SourceDelivery,
};
use crate::chain::sha256_hex;
use crate::config::Config;
use crate::error::CoreError;
use crate::ingest::{ingest, ingest_codex, CodexIngestOptions};
use crate::packet::{compile_request, handoff, repo_scope, Budget};
use crate::workspace::identity_for_project;
use crate::{Ledger, Result};

const OWNER: &str = "wedge-owner";
const COMMAND: &str = "git status --porcelain";
const CLAIM: &str = "The build succeeded.";

/// What the walk produced. Every field is something the caller can print or
/// assert on — the CLI does the former, the test does the latter.
#[derive(Debug, Clone)]
pub struct DemoReport {
    pub repo_id: String,
    pub project_ids: Vec<String>,
    /// The issue id packet #1 was retained under.
    pub first_issue: String,
    /// The issue id packet #2 was retained under, successor of the first.
    pub second_issue: String,
    /// Packet #1's VERIFIED section — a measured zero.
    pub first_verified_section: String,
    /// Packet #2's VERIFIED section — the payoff line.
    pub second_verified_section: String,
    /// The frozen handoff document for packet #1.
    pub frozen_handoff: String,
    /// The frozen handoff document for packet #2, naming its predecessor.
    pub frozen_successor: String,
    /// Rows in `events` with no session — must be zero, or the walk proves
    /// nothing about a real pipeline.
    pub seeded_events: i64,
}

/// The annotation layer needs an entailment scorer, and the demo must stay
/// offline and deterministic. **This is not a model and never pretends to be
/// one** — it is a fixed response so the walk exercises the claim pipeline
/// without reaching for a provider. It is reachable only from this module.
struct DemoScorer;

impl crate::nli::EntailmentScorer for DemoScorer {
    fn score(&mut self, _premise: &str, _hypothesis: &str) -> Result<crate::nli::Scores> {
        Ok(crate::nli::Scores {
            entailment: 0.95,
            neutral: 0.05,
            contradiction: 0.0,
        })
    }
}

fn git(cwd: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("/usr/bin/git")
        .args(args)
        .current_dir(cwd)
        .status()
        .map_err(|e| CoreError::Integrity(format!("the demo needs git on PATH: {e}")))?;
    if !status.success() {
        return Err(CoreError::Integrity(format!("git {args:?} failed")));
    }
    Ok(())
}

/// A real git repository — the thing both agents work in, and the thing that
/// makes them one project in the read surface.
fn repository(path: &Path) -> Result<()> {
    git(path, &["init", "-q"])?;
    git(path, &["config", "user.name", "Wedge"])?;
    git(path, &["config", "user.email", "wedge@example.invalid"])?;
    fs::write(path.join("tracked.txt"), "one\n")
        .map_err(|e| CoreError::Integrity(format!("demo write: {e}")))?;
    git(path, &["add", "tracked.txt"])?;
    git(path, &["commit", "-qm", "fixture"])?;
    Ok(())
}

/// A Claude Code session: the agent states an intention. No tool result backs
/// it, so nothing here can reach `verified` — which is the point of leg one.
fn claude_transcript(cwd: &str) -> String {
    format!(
        concat!(
            r#"{{"type":"user","uuid":"00000000-0000-4000-8000-00000000c001","sessionId":"cccccccc-0000-4000-8000-000000000001","timestamp":"2030-01-01T00:00:00.000Z","parentUuid":null,"isSidechain":false,"cwd":"{cwd}","message":{{"role":"user","content":"Get the build green."}}}}"#,
            "\n",
            r#"{{"type":"assistant","uuid":"00000000-0000-4000-8000-00000000c002","sessionId":"cccccccc-0000-4000-8000-000000000001","timestamp":"2030-01-01T00:00:01.000Z","parentUuid":"00000000-0000-4000-8000-00000000c001","isSidechain":false,"cwd":"{cwd}","message":{{"role":"assistant","content":[{{"type":"text","text":"I will fix the retry guard and verify the build."}}]}}}}"#,
            "\n"
        ),
        cwd = cwd
    )
}

/// A codex rollout carrying the claim, the command that backs it, and the
/// command's output — the continuation of the same work in the same repo.
fn codex_transcript(cwd: &str) -> String {
    format!(
        concat!(
            r#"{{"timestamp":"2030-01-01T01:00:00Z","type":"session_meta","payload":{{"id":"wedge-session","cwd":"{cwd}"}}}}"#,
            "\n",
            r#"{{"timestamp":"2030-01-01T01:00:01Z","type":"turn_context","payload":{{"turn_id":"wedge-turn","cwd":"{cwd}","branch":"main"}}}}"#,
            "\n",
            r#"{{"timestamp":"2030-01-01T01:00:02Z","type":"response_item","payload":{{"type":"message","id":"m1","role":"assistant","content":[{{"type":"output_text","text":"{claim}"}}]}}}}"#,
            "\n",
            r#"{{"timestamp":"2030-01-01T01:00:03Z","type":"response_item","payload":{{"type":"function_call","id":"fc1","call_id":"call-a","name":"exec_command","arguments":{{"cmd":"{cmd}"}}}}}}"#,
            "\n",
            r#"{{"timestamp":"2030-01-01T01:00:04Z","type":"response_item","payload":{{"type":"function_call_output","id":"fo1","call_id":"call-a","output":"build succeeded"}}}}"#,
            "\n"
        ),
        cwd = cwd,
        claim = CLAIM,
        cmd = COMMAND
    )
}

fn verified_section(rendered: &str) -> String {
    rendered
        .split("## VERIFIED AND CURRENT")
        .nth(1)
        .and_then(|s| s.split("\n## ").next())
        .unwrap_or("")
        .to_string()
}

fn one_row(ledger: &Ledger, sql: &str, param: &[&dyn rusqlite::ToSql]) -> Result<String> {
    Ok(ledger.conn().query_row(sql, param, |r| r.get(0))?)
}

/// Run the whole walk inside `dir`. The caller owns the directory — the CLI
/// hands in a temp dir it will drop, the test hands in its own.
pub fn walk(dir: &Path) -> Result<DemoReport> {
    let repo = dir.join("repo");
    fs::create_dir_all(&repo).map_err(|e| CoreError::Integrity(format!("demo mkdir: {e}")))?;
    repository(&repo)?;
    // The canonical path both transcripts record as their cwd. This is the ONLY
    // thing tying the two agents together — no shared id is passed between
    // them, because in the real world there is none to pass.
    let cwd = repo
        .canonicalize()
        .map_err(|e| CoreError::Integrity(format!("demo canonicalize: {e}")))?;
    let cwd_str = cwd.to_string_lossy().into_owned();

    let mut ledger = Ledger::open(dir.join("ledger.db"))?;

    // ---- LEG ONE: Claude-side work, ingested from real JSONL ---------------
    let claude_root = dir.join("claude-projects");
    let claude_dir = claude_root.join("-wedge-repo");
    fs::create_dir_all(&claude_dir)
        .map_err(|e| CoreError::Integrity(format!("demo mkdir: {e}")))?;
    let claude_file = claude_dir.join("cccccccc-0000-4000-8000-000000000001.jsonl");
    fs::write(&claude_file, claude_transcript(&cwd_str))
        .map_err(|e| CoreError::Integrity(format!("demo write: {e}")))?;
    let demo_mirrors = dir.join("mirrors");
    ingest(
        &mut ledger,
        std::slice::from_ref(&claude_root),
        &demo_mirrors,
    )?;

    // ---- LEG TWO: the codex continuation, from a real rollout --------------
    let codex_file = dir.join("source").join("rollout-wedge.jsonl");
    fs::create_dir_all(
        codex_file
            .parent()
            .ok_or_else(|| CoreError::Integrity("demo path".to_string()))?,
    )
    .map_err(|e| CoreError::Integrity(format!("demo mkdir: {e}")))?;
    fs::write(&codex_file, codex_transcript(&cwd_str))
        .map_err(|e| CoreError::Integrity(format!("demo write: {e}")))?;
    ingest_codex(
        &mut ledger,
        std::slice::from_ref(&codex_file),
        &CodexIngestOptions {
            mirror_root: dir.join("private-mirror"),
            owner_key: Some(OWNER.to_string()),
        },
    )?;

    // ---- THE JOIN: two agents, two project ids, ONE repo -------------------
    let project_ids: Vec<String> = {
        let mut stmt = ledger
            .conn()
            .prepare("SELECT id FROM projects ORDER BY id")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        out
    };
    let session_files = [
        claude_file.to_string_lossy().into_owned(),
        codex_file.to_string_lossy().into_owned(),
    ];
    let mut repo_ids = Vec::new();
    for (project_id, file) in project_ids.iter().zip(session_files.iter()) {
        let row = identity_for_project(
            project_id,
            std::slice::from_ref(file),
            64,
            "2030-01-01T00:00:00Z",
        )?;
        ledger.record_workspace_identity(&row)?;
        repo_ids.push(row.repo_id.clone().ok_or_else(|| {
            CoreError::Integrity(format!("demo: no repo id derived for {project_id}"))
        })?);
    }
    if repo_ids.iter().any(|r| r != &repo_ids[0]) {
        return Err(CoreError::Integrity(
            "demo: the two agents did not resolve to one repo".to_string(),
        ));
    }
    let scope = repo_scope(&ledger, &repo_ids[0])?;

    // Both agents' work goes through the annotation layer, once, BEFORE packet
    // #1 — so the only thing that changes between the two packets is the run.
    for project_id in &project_ids {
        crate::annotate::annotate(
            &ledger,
            project_id,
            &Config::default(),
            &mut DemoScorer,
            "demo-fixed-scorer",
        )?;
    }

    // ---- PACKET #1: issued to the Claude-side agent ------------------------
    let first = compile_request(
        &ledger,
        &scope,
        Some("claude"),
        &Budget::default(),
        &Config::default(),
    )?;
    let first_verified_section = verified_section(&first.render());
    let first_issue = first.retain(&mut ledger, Some("claude"), 1_000)?;
    let frozen_handoff = handoff(&ledger, &first_issue)?
        .ok_or_else(|| CoreError::Integrity("demo: packet #1 was not retained".to_string()))?;

    // ---- THE REAL RUN: a subprocess, on this machine, right now ------------
    let (codex_project_id, source_project_key): (String, String) = ledger.conn().query_row(
        "SELECT project_id, source_project_key FROM source_project_identities \
         WHERE source='codex' LIMIT 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let source_session_key = one_row(
        &ledger,
        "SELECT source_session_key FROM source_sessions WHERE source='codex' LIMIT 1",
        &[],
    )?;
    // The DERIVED turn id, read back — never the raw "wedge-turn" the
    // transcript carries. A103 cost a milestone to that distinction.
    let derived_turn_id = one_row(
        &ledger,
        "SELECT source_turn_id FROM events WHERE kind='tool_use' AND project_id=?1 LIMIT 1",
        &[&codex_project_id],
    )?;
    let tool_result_event_id = one_row(
        &ledger,
        "SELECT id FROM events WHERE kind='tool_result' AND project_id=?1 LIMIT 1",
        &[&codex_project_id],
    )?;

    let collector = Collector::open(dir.join("capture"), OWNER, "codex")?;
    let config = sha256_hex(b"wedge-config");
    record_source_assurance(
        &ledger,
        &collector,
        &SourceAssuranceMeasurement {
            source: "codex".to_string(),
            source_version: "wedge-v1".to_string(),
            config_sha256: config.clone(),
            matcher: "exec_command".to_string(),
            pre_blocks: true,
            rewrite_exact: true,
            stable_tool_identity: true,
            completion_pairing: true,
            measured_pre_delay_ms: 400,
            child_started_after_pre: true,
            wrapper_exit_code: 0,
            source_exit_code: 0,
        },
    )?;

    let opaque = opaque_source_project_key(OWNER, "codex", &source_project_key);
    let outcome = collector.execute(&CaptureRequest {
        owner_key: OWNER.to_string(),
        source: "codex".to_string(),
        source_project_key: opaque.clone(),
        source_session_key: source_session_key.clone(),
        source_turn_id: Some(derived_turn_id.clone()),
        source_tool_id: "call-a".to_string(),
        wrapper_nonce: "nonce-wedge".to_string(),
        command: COMMAND.to_string(),
        cwd: repo.clone(),
        source_config_sha256: config.clone(),
        source_version: "wedge-v1".to_string(),
        assurance_enabled: true,
    })?;
    if outcome.receipt.strength != CaptureStrength::Assured {
        return Err(CoreError::Integrity(
            "demo: the wrapper run was not assured, so nothing may claim verified".to_string(),
        ));
    }
    let pre = outcome
        .pre
        .as_ref()
        .ok_or_else(|| CoreError::Integrity("demo: no pre capsule".to_string()))?;
    let post = outcome
        .post
        .as_ref()
        .ok_or_else(|| CoreError::Integrity("demo: no post capsule".to_string()))?;
    accept_capture_pair(
        &ledger,
        &collector,
        &CaptureAcceptance {
            outcome: &outcome,
            delivery: &SourceDelivery {
                schema_version: 1,
                attempt_id: outcome.attempt_id.clone(),
                owner_key: OWNER.to_string(),
                source: "codex".to_string(),
                source_project_key: opaque.clone(),
                source_session_key: source_session_key.clone(),
                source_turn_id: Some(derived_turn_id.clone()),
                source_tool_id: "call-a".to_string(),
                wrapper_nonce: "nonce-wedge".to_string(),
                source_event_id: tool_result_event_id.clone(),
                source_exit_code: Some(outcome.exit_code),
                source_term_signal: None,
                observed_at_unix_ms: 2,
            },
            pre_private_relative_ref: format!(
                "sealed/{}/pre.{}.json",
                outcome.attempt_id, pre.capsule_id
            ),
            post_private_relative_ref: format!(
                "sealed/{}/post.{}.json",
                outcome.attempt_id, post.capsule_id
            ),
        },
    )?;

    // ---- PACKET #2: the successor, and the payoff --------------------------
    let second = compile_request(
        &ledger,
        &scope,
        Some("codex"),
        &Budget::default(),
        &Config::default(),
    )?;
    let second_verified_section = verified_section(&second.render());
    let second_issue =
        second.retain_after(&mut ledger, Some("codex"), 2_000, Some(&first_issue))?;
    let frozen_successor = handoff(&ledger, &second_issue)?
        .ok_or_else(|| CoreError::Integrity("demo: packet #2 was not retained".to_string()))?;

    let seeded_events: i64 = ledger.conn().query_row(
        "SELECT COUNT(*) FROM events WHERE session_id IS NULL OR session_id = ''",
        [],
        |r| r.get(0),
    )?;

    Ok(DemoReport {
        repo_id: repo_ids[0].clone(),
        project_ids,
        first_issue,
        second_issue,
        first_verified_section,
        second_verified_section,
        frozen_handoff,
        frozen_successor,
        seeded_events,
    })
}
