//! A201 — the claim-loop verifier over synthetic `ready_for_verification`
//! rows.
//!
//! The fixture is built through the real pipeline: a real ingest, a real
//! packet, a real `claim_completion` accepted by the real MCP server, and
//! the claiming session identified through a real capture delivery appended
//! by the production sensor appender. The claim row is append-only and is
//! never touched; every verdict is a separate appended fact on its own
//! chain.

use std::fs;

use serde_json::{json, Value};
use thruline_core::annotate::annotate;
use thruline_core::config::Config;
use thruline_core::ingest::ingest;
use thruline_core::mcp::Server;
use thruline_core::nli::{EntailmentScorer, Scores};
use thruline_core::verifier::{self, ClaimVerdict};
use thruline_core::{Ledger, Result};

const PROJ: &str = "-Users-dev-verify";
const SESSION: &str = "bbbbbbbb-1111-4222-8333-444444444444";
const OTHER_SESSION: &str = "dddddddd-5555-4666-8777-888888888888";
const RECORDED_AT: &str = "2026-08-31T04:00:00.000Z";

struct NeverContradicts;
impl EntailmentScorer for NeverContradicts {
    fn score(&mut self, _p: &str, _h: &str) -> Result<Scores> {
        Ok(Scores {
            entailment: 0.30,
            neutral: 0.70,
            contradiction: 0.0,
        })
    }
}

fn uuid(i: usize) -> String {
    format!("00000000-0000-4000-8000-{i:012}")
}

fn transcript() -> String {
    let lines = [
        json!({
            "type": "user", "uuid": uuid(1), "parentUuid": null, "isSidechain": false,
            "sessionId": SESSION, "timestamp": "2026-08-31T03:00:01.000Z", "cwd": "/repo",
            "message": {"role": "user", "content": "ship it"},
        }),
        json!({
            "type": "assistant", "uuid": uuid(2), "parentUuid": uuid(1), "isSidechain": false,
            "sessionId": SESSION, "timestamp": "2026-08-31T03:00:02.000Z", "cwd": "/repo",
            "message": {"role": "assistant", "content": [{"type": "text", "text": "Shipping now."}]},
        }),
    ];
    lines.iter().map(|l| format!("{l}\n")).collect()
}

/// A ledger with one ingested project. Returns the guard and the db path.
fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("projects");
    fs::create_dir_all(root.join(PROJ)).expect("mkdir");
    fs::write(
        root.join(PROJ).join(format!("{SESSION}.jsonl")),
        transcript(),
    )
    .expect("write");
    let db = dir.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    ingest(
        &mut ledger,
        std::slice::from_ref(&root),
        &dir.path().join("mirrors"),
    )
    .expect("ingest");
    annotate(
        &ledger,
        PROJ,
        &Config::default(),
        &mut NeverContradicts,
        "verify-stub",
    )
    .expect("annotate");
    drop(ledger);
    (dir, db)
}

fn call(server: &Server, tool: &str, args: Value) -> Value {
    let line = json!({
        "jsonrpc":"2.0","id":1,"method":"tools/call",
        "params": {"name": tool, "arguments": args}
    })
    .to_string();
    let raw = server.handle_line(&line).expect("a reply");
    let response: Value = serde_json::from_str(&raw).expect("valid json");
    assert!(
        response.get("error").is_none(),
        "tool {tool} errored: {response}"
    );
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("text content");
    serde_json::from_str(text).expect("tool payload is json")
}

/// Submit one claim_completion through the real server; returns the exact
/// argument object used (the delivery identity) and asserts acceptance.
fn submit_claim(db: &std::path::Path, manifest: Value) -> Value {
    let server = Server::new("claude", db.to_path_buf()).expect("server");
    let packet = call(&server, "get_context_packet", json!({"project": PROJ}));
    let position = packet["chain_position"].as_str().expect("position");
    let args = json!({
        "project": PROJ,
        "obligation": "ship it",
        "chain_position": position,
        "manifest": manifest,
    });
    let accepted = call(&server, "claim_completion", args.clone());
    assert_eq!(accepted["status"], json!("ready_for_verification"));
    args
}

/// The claiming session's own capture delivery of the call — how the
/// verifier identifies which session's record to search.
fn deliver_call(db: &std::path::Path, session: &str, args: &Value) {
    let ledger = Ledger::open(db).expect("ledger");
    let payload = json!({
        "session_id": session,
        "transcript_path": format!("/home/dev/.claude/projects/{PROJ}/{session}.jsonl"),
        "cwd": "/repo",
        "hook_event_name": "PostToolUse",
        "tool_name": "mcp__thruline__claim_completion",
        "tool_input": args,
        "tool_response": {"content": [{"type": "text", "text": "accepted"}]},
    });
    thruline_core::sensor::append(
        &ledger,
        "PostToolUse",
        payload.to_string().as_bytes(),
        "2026-08-31T03:30:00.000Z",
    )
    .expect("append");
}

fn append_evidence(db: &std::path::Path, session: &str, tool: &str, input: Value, response: Value) {
    let ledger = Ledger::open(db).expect("ledger");
    let payload = json!({
        "session_id": session,
        "transcript_path": format!("/home/dev/.claude/projects/{PROJ}/{session}.jsonl"),
        "cwd": "/repo",
        "hook_event_name": "PostToolUse",
        "tool_name": tool,
        "tool_input": input,
        "tool_response": response,
    });
    thruline_core::sensor::append(
        &ledger,
        "PostToolUse",
        payload.to_string().as_bytes(),
        "2026-08-31T03:20:00.000Z",
    )
    .expect("append");
}

fn run_verifier(db: &std::path::Path) -> Vec<verifier::VerifiedClaim> {
    let ledger = Ledger::open(db).expect("ledger");
    let spill = thruline_core::sensor::spill_path(db);
    verifier::verify_claims(&ledger, &spill, RECORDED_AT).expect("verify")
}

/* ── manifest satisfied ──────────────────────────────────────────────── */

#[test]
fn a_satisfied_manifest_is_supported_and_the_rerun_appends_nothing() {
    let (dir, db) = fixture();
    let args = submit_claim(
        &db,
        json!({
            "tests": ["cargo test"],
            "files": ["client.rs"],
            "commit": "abc123",
        }),
    );
    deliver_call(&db, SESSION, &args);
    append_evidence(
        &db,
        SESSION,
        "Bash",
        json!({"command": "cargo test"}),
        json!({"stdout": "test result: ok. 12 passed; 0 failed", "stderr": "", "interrupted": false}),
    );
    append_evidence(
        &db,
        SESSION,
        "Edit",
        json!({"file_path": "/repo/src/client.rs"}),
        json!({"filePath": "/repo/src/client.rs"}),
    );
    append_evidence(
        &db,
        SESSION,
        "Bash",
        json!({"command": "git commit -m 'ship'"}),
        json!({"stdout": "[main abc123] ship", "stderr": "", "interrupted": false}),
    );

    let report = run_verifier(&db);
    assert_eq!(report.len(), 1);
    assert_eq!(report[0].verdict, ClaimVerdict::Supported);
    assert!(report[0].newly_recorded);
    assert!(report[0].verdict_short_id.starts_with("cvrd_"));

    let ledger = Ledger::open(&db).expect("ledger");
    let rows = verifier::verdict_rows(&ledger).expect("rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].session_id.as_deref(), Some(SESSION));
    assert_eq!(
        verifier::verify_verdicts_chain(&ledger).expect("chain"),
        1,
        "the verdict chain re-folds"
    );
    // Every entry carries its receipt in the stored detail.
    let detail: Vec<verifier::EntryResult> =
        serde_json::from_str(&rows[0].detail_json).expect("detail json");
    assert_eq!(detail.len(), 3);
    assert!(detail.iter().all(|e| e.outcome == "supported"));
    assert!(detail.iter().all(|e| e.basis.contains("capture seq")));
    drop(ledger);

    // The identical record state appends nothing on a re-run.
    let again = run_verifier(&db);
    assert!(!again[0].newly_recorded);
    let ledger = Ledger::open(&db).expect("ledger");
    let rows = verifier::verdict_rows(&ledger).expect("rows");
    assert_eq!(rows.len(), 1, "idempotent by verdict identity");
    drop(dir);
}

/* ── manifest unsatisfied ────────────────────────────────────────────── */

#[test]
fn an_unsatisfied_manifest_is_no_proof_found_and_the_claim_row_is_untouched() {
    let (_dir, db) = fixture();
    let args = submit_claim(&db, json!({"tests": ["cargo test"]}));
    deliver_call(&db, SESSION, &args);
    // No evidence rows at all: the session is identified, the search runs,
    // nothing matches.

    let report = run_verifier(&db);
    assert_eq!(report[0].verdict, ClaimVerdict::NoProofFound);
    assert!(
        report[0].reason.contains("no proof found in the record"),
        "the only permitted accusation shape: {}",
        report[0].reason
    );
    assert!(
        report[0].reason.contains("cargo test"),
        "names what was unproved: {}",
        report[0].reason
    );

    // The claim row is exactly as submitted — the verdict is a sibling fact.
    let ledger = Ledger::open(&db).expect("ledger");
    let outcome: String = ledger
        .conn()
        .query_row(
            "SELECT outcome FROM mcp_calls WHERE tool='claim_completion'",
            [],
            |r| r.get(0),
        )
        .expect("claim row");
    assert_eq!(outcome, "ready_for_verification");
}

#[test]
fn a_failing_run_does_not_support_a_test_entry() {
    let (_dir, db) = fixture();
    let args = submit_claim(&db, json!({"tests": ["cargo test"]}));
    deliver_call(&db, SESSION, &args);
    append_evidence(
        &db,
        SESSION,
        "Bash",
        json!({"command": "cargo test"}),
        json!({"stdout": "test result: FAILED. 11 passed; 1 failed", "stderr": "", "interrupted": false}),
    );
    let report = run_verifier(&db);
    assert_eq!(
        report[0].verdict,
        ClaimVerdict::NoProofFound,
        "a run reading as failure is not proof — and the verdict still only \
         says no proof was found, never that the claim is false"
    );
}

/* ── ambiguity never accuses ─────────────────────────────────────────── */

#[test]
fn an_unidentifiable_session_is_unknown_not_accused() {
    let (_dir, db) = fixture();
    submit_claim(&db, json!({"tests": ["cargo test"]}));
    // No capture delivery of the call: the claiming session is not on record.
    let report = run_verifier(&db);
    assert_eq!(report[0].verdict, ClaimVerdict::Unknown);
    assert!(
        report[0].reason.contains("not identified"),
        "the abstention names its cause: {}",
        report[0].reason
    );
}

#[test]
fn two_sessions_delivering_the_identical_call_is_an_abstention() {
    let (_dir, db) = fixture();
    let args = submit_claim(&db, json!({"tests": ["cargo test"]}));
    deliver_call(&db, SESSION, &args);
    deliver_call(&db, OTHER_SESSION, &args);
    let report = run_verifier(&db);
    assert_eq!(report[0].verdict, ClaimVerdict::Unknown);
    assert!(
        report[0].reason.contains("cannot be told apart"),
        "{}",
        report[0].reason
    );
}

#[test]
fn a_manifest_naming_nothing_checkable_is_unknown() {
    let (_dir, db) = fixture();
    let args = submit_claim(
        &db,
        json!({"vibes": "good", "commands_run": ["./scripts/deploy.sh"]}),
    );
    deliver_call(&db, SESSION, &args);
    let report = run_verifier(&db);
    assert_eq!(
        report[0].verdict,
        ClaimVerdict::Unknown,
        "an unrecognised command and a free-form key are not accusable"
    );
    let ledger = Ledger::open(&db).expect("ledger");
    let rows = verifier::verdict_rows(&ledger).expect("rows");
    let detail: Vec<verifier::EntryResult> =
        serde_json::from_str(&rows[0].detail_json).expect("detail");
    assert!(
        detail.iter().all(|e| e.class == "unchecked"),
        "every entry is named, none dropped: {detail:?}"
    );
}

/* ── the spill guard on the verifier ─────────────────────────────────── */

#[test]
fn a_spill_for_the_claiming_session_degrades_no_proof_to_unknown() {
    let (_dir, db) = fixture();
    let args = submit_claim(&db, json!({"tests": ["cargo test"]}));
    deliver_call(&db, SESSION, &args);
    // One spilled delivery for the claiming session: the very proof the
    // verdict would deny could be sitting in this file.
    let spill_line = json!({
        "received_at": "2026-08-31T03:25:00.000Z",
        "hook_event": "PostToolUse",
        "error": "database is locked",
        "payload": json!({
            "session_id": SESSION,
            "hook_event_name": "PostToolUse",
            "tool_name": "Bash",
            "tool_input": {"command": "cargo test"},
        }).to_string(),
    });
    fs::write(
        thruline_core::sensor::spill_path(&db),
        format!("{spill_line}\n"),
    )
    .expect("write spill");

    let report = run_verifier(&db);
    assert_eq!(report[0].verdict, ClaimVerdict::Unknown);
    assert_eq!(
        report[0].reason,
        "evidence window incomplete: 1 spilled delivery"
    );
    let ledger = Ledger::open(&db).expect("ledger");
    let rows = verifier::verdict_rows(&ledger).expect("rows");
    assert_eq!(rows[0].spilled_deliveries, 1);
}

/* ── the workspace check ─────────────────────────────────────────────── */

#[test]
fn a_file_entry_matching_the_workspace_hash_is_supported_without_a_session() {
    let (dir, db) = fixture();
    let file = dir.path().join("proof.rs");
    fs::write(&file, b"pub fn proven() {}\n").expect("write");
    let sha = thruline_core::chain::sha256_hex(b"pub fn proven() {}\n");
    submit_claim(
        &db,
        json!({"files": [{"path": file.to_str().expect("path"), "sha256": sha}]}),
    );
    // No delivery, no session — the workspace alone carries the proof.
    let report = run_verifier(&db);
    assert_eq!(report[0].verdict, ClaimVerdict::Supported);
}

/* ── the append-only contract ────────────────────────────────────────── */

#[test]
fn verdict_rows_refuse_update_and_delete() {
    let (_dir, db) = fixture();
    let args = submit_claim(&db, json!({"tests": ["cargo test"]}));
    deliver_call(&db, SESSION, &args);
    run_verifier(&db);
    let ledger = Ledger::open(&db).expect("ledger");
    assert!(
        ledger
            .conn()
            .execute("UPDATE claim_verdicts SET reason='rewritten'", [])
            .is_err(),
        "verdicts are append-only"
    );
    assert!(
        ledger
            .conn()
            .execute("DELETE FROM claim_verdicts", [])
            .is_err(),
        "verdicts are permanent"
    );
}

/* ── the loop closes: verdicts feed the next brief ───────────────────── */

/// A fixed claim row written straight through the append-only recorder, so
/// every derived id in the snapshot is content-stable (the MCP server stamps
/// wall-clock time and a nonce into real call ids; determinism here needs
/// neither).
fn record_fixed_claim(db: &std::path::Path, call_id: &str, args: &Value) -> String {
    let request_sha256 =
        thruline_core::chain::sha256_hex(serde_json::to_string(args).expect("json").as_bytes());
    let mut ledger = Ledger::open(db).expect("ledger");
    ledger
        .record_mcp_call(&thruline_core::mcp::CallRecord {
            call_id: call_id.to_string(),
            agent: "claude".to_string(),
            tool: "claim_completion".to_string(),
            scope: "project:-repo-app".to_string(),
            obligation: args
                .get("obligation")
                .and_then(Value::as_str)
                .map(str::to_owned),
            gated_chain_position: None,
            latest_chain_position: None,
            request_sha256: request_sha256.clone(),
            manifest_json: args.get("manifest").map(|m| m.to_string()),
            manifest_sha256: None,
            outcome: "ready_for_verification".to_string(),
            accepted_at_unix_ms: 1_788_134_400_000,
        })
        .expect("record claim");
    request_sha256
}

#[test]
fn an_unresolved_claim_reaches_the_next_brief_and_a_verdict_updates_it() {
    let temp = tempfile::tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    Ledger::open(&db).expect("create ledger");
    let call_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let args = json!({
        "project": "-repo-app",
        "obligation": "ship the retry logic",
        "chain_position": "0000",
        "manifest": {"tests": ["cargo test"]},
    });
    record_fixed_claim(&db, call_id, &args);

    // Before any verdict: the claim rides into the brief as still-unchecked,
    // wearing the typeable claim id.
    let ledger = Ledger::open(&db).expect("ledger");
    let lines = verifier::claim_digest_lines(&ledger, "-repo-app", 3).expect("lines");
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].kind, "claim_open");
    assert_eq!(lines[0].why_id.as_deref(), Some("tlc_aaaaaaaaaaaa"));
    drop(ledger);

    // A claims-only cache renders with the project header and the id, and
    // every line passes the voice gate.
    let dir = temp.path().join("digest");
    let ledger = Ledger::open(&db).expect("ledger");
    verifier::refresh_claim_lines(&ledger, &dir, "-repo-app", "2026-08-31T04:00:00.000Z")
        .expect("refresh");
    drop(ledger);
    let md = std::fs::read_to_string(dir.join("-repo-app.md")).expect("cache md");
    insta::assert_snapshot!("claims_only_brief_before_verdict", md);
    for line in md.lines() {
        assert_eq!(
            thruline_core::voice::banned_in(line),
            None,
            "engine lexicon reached the brief: {line:?}"
        );
    }

    // The claiming session delivers the call (identifying itself) but holds
    // no matching evidence: the verifier lands no_proof_found.
    deliver_call(&db, SESSION, &args);
    let report = run_verifier(&db);
    assert_eq!(report[0].verdict, ClaimVerdict::NoProofFound);

    // The brief now carries the verdict's own words and the verdict id.
    let ledger = Ledger::open(&db).expect("ledger");
    let lines = verifier::claim_digest_lines(&ledger, "-repo-app", 3).expect("lines");
    assert_eq!(lines[0].kind, "claim_no_proof");
    let why = lines[0].why_id.as_deref().expect("why id");
    assert!(why.starts_with("cvrd_"), "{why}");
    verifier::refresh_claim_lines(&ledger, &dir, "-repo-app", "2026-08-31T04:10:00.000Z")
        .expect("refresh");
    drop(ledger);
    let md = std::fs::read_to_string(dir.join("-repo-app.md")).expect("cache md");
    assert!(
        md.contains("claimed as finished, and no proof found in the record"),
        "{md}"
    );
    assert!(
        md.contains(why),
        "the brief line carries the verdict id: {md}"
    );

    // Evidence lands; the verifier re-runs; the standing verdict is
    // supported and the stale line is gone from the cache entirely.
    append_evidence(
        &db,
        SESSION,
        "Bash",
        json!({"command": "cargo test"}),
        json!({"stdout": "test result: ok. 12 passed; 0 failed", "stderr": "", "interrupted": false}),
    );
    let report = run_verifier(&db);
    assert_eq!(report[0].verdict, ClaimVerdict::Supported);
    let ledger = Ledger::open(&db).expect("ledger");
    verifier::refresh_claim_lines(&ledger, &dir, "-repo-app", "2026-08-31T04:20:00.000Z")
        .expect("refresh");
    assert!(
        !dir.join("-repo-app.md").exists(),
        "a supported claim leaves nothing to say — the cache is removed"
    );
    // Both verdicts stand in history; nothing was rewritten.
    let rows = verifier::verdict_rows(&ledger).expect("rows");
    assert_eq!(rows.len(), 2, "no_proof_found then supported, both kept");
    assert_eq!(verifier::verify_verdicts_chain(&ledger).expect("chain"), 2);
}
