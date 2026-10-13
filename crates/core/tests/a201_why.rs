//! A201 — `thruline why` over the truth loop's own ids: audit findings
//! (`spkf_`), left-open promises (`spkp_`), claim verdicts (`cvrd_`) and
//! claim calls (`tlc_`), each resolved by exact prefix and rendered as a
//! terminal receipt chain.
//!
//! Every fixture is pinned to fixed identities and timestamps so the insta
//! snapshots hold byte-for-byte; the only varying artifact is the tempdir
//! location, which never appears in rendered output (paths in receipts are
//! the fixture's own fixed transcript path).
//!
//! REGENERATING THE GOLDEN FILES:
//!     INSTA_UPDATE=always cargo test -p thruline-core --test a201_why
//! Read the diff before accepting it: this output is the product's answer
//! to "why should I believe you" and its vocabulary is load-bearing.

use serde_json::{json, Value};
use tempfile::tempdir;
use thruline_core::extract::{ClaimKind, Marker};
use thruline_core::speaker::{self, DigestScope, EvidenceIndex, FinalTurn, TurnClaim};
use thruline_core::verifier::{self, WhyResolution};
use thruline_core::Ledger;

const SESSION: &str = "aaaaaaaa-1111-4222-8333-444444444444";
const TRANSCRIPT: &str =
    "/home/dev/.claude/projects/-repo-app/aaaaaaaa-1111-4222-8333-444444444444.jsonl";
const CALL_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn no_proof_turn() -> FinalTurn {
    FinalTurn {
        claims: vec![TurnClaim {
            text: "All 12 tests pass.".to_string(),
            kind: ClaimKind::Marker,
            marker: Marker::Ratio,
            artifacts: Vec::new(),
            line: 2,
        }],
        boundary_line: Some(1),
        last_tool_line: None,
        tool_use_count: 0,
    }
}

fn promise_turn() -> FinalTurn {
    FinalTurn {
        claims: vec![TurnClaim {
            text: "I'll wire the new panel into `layout.rs` next.".to_string(),
            kind: ClaimKind::Proposal,
            marker: Marker::Intent,
            artifacts: vec!["layout.rs".to_string()],
            line: 2,
        }],
        boundary_line: Some(1),
        last_tool_line: None,
        tool_use_count: 0,
    }
}

fn scope(hook: &'static str) -> DigestScope<'static> {
    DigestScope {
        project_key: "-repo-app",
        session_id: SESSION,
        transcript_path: TRANSCRIPT,
        generated_at: "2026-08-31T02:02:00.000Z",
        hook,
    }
}

fn resolve(db: &std::path::Path, dir: &std::path::Path, needle: &str) -> WhyResolution {
    let ledger = Ledger::open(db).expect("ledger");
    verifier::why_truth_loop(&ledger, dir, needle)
        .expect("resolve")
        .expect("a truth-loop prefix")
}

fn rendered(resolution: WhyResolution) -> String {
    match resolution {
        WhyResolution::One { rendered, .. } => rendered,
        other => panic!("expected one match, got {other:?}"),
    }
}

/* ── findings ────────────────────────────────────────────────────────── */

#[test]
fn a_ledger_finding_answers_with_its_full_receipt_chain() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let dir = temp.path().join("digest");
    let ledger = Ledger::open(&db).expect("ledger");
    let turn = no_proof_turn();
    let digest = speaker::compose(&scope("stop"), &turn, &EvidenceIndex::build(&[]), &[]);
    let appended = speaker::record_findings(&ledger, &digest, &turn, "2026-08-31T02:02:00.000Z")
        .expect("record");
    assert_eq!(appended, 1);
    let short = digest.lines[0].why_id.as_deref().expect("why id");
    assert!(short.starts_with("spkf_"));
    drop(ledger);

    let out = rendered(resolve(&db, &dir, short));
    insta::assert_snapshot!("why_finding_from_ledger", out);
    assert!(
        out.contains("may be entirely true"),
        "the caveat is part of the answer: {out}"
    );
}

#[test]
fn a_cached_finding_answers_before_it_is_announced() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let dir = temp.path().join("digest");
    Ledger::open(&db).expect("create");
    let turn = no_proof_turn();
    let digest = speaker::compose(
        &scope("session-end"),
        &turn,
        &EvidenceIndex::build(&[]),
        &[],
    );
    speaker::write_cache(&dir, &digest).expect("cache");
    let short = digest.lines[0].why_id.as_deref().expect("why id");

    let out = rendered(resolve(&db, &dir, short));
    insta::assert_snapshot!("why_finding_from_cache", out);
    assert!(
        out.contains("cached for the next brief"),
        "names its provenance: {out}"
    );
}

/* ── promises ────────────────────────────────────────────────────────── */

#[test]
fn an_open_promise_answers_from_the_digest_sidecar() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let dir = temp.path().join("digest");
    Ledger::open(&db).expect("create");
    let turn = promise_turn();
    let digest = speaker::compose(
        &scope("session-end"),
        &turn,
        &EvidenceIndex::build(&[]),
        &[],
    );
    speaker::write_cache(&dir, &digest).expect("cache");
    let short = digest.lines[0].why_id.as_deref().expect("why id");
    assert!(short.starts_with("spkp_"));

    let out = rendered(resolve(&db, &dir, short));
    insta::assert_snapshot!("why_open_promise", out);
    assert!(
        out.contains("not a") && out.contains("judgment"),
        "a promise line judges nothing: {out}"
    );
}

/* ── verdicts and claims ─────────────────────────────────────────────── */

fn record_fixed_claim(db: &std::path::Path, args: &Value) {
    let mut ledger = Ledger::open(db).expect("ledger");
    ledger
        .record_mcp_call(&thruline_core::mcp::CallRecord {
            call_id: CALL_ID.to_string(),
            agent: "claude".to_string(),
            tool: "claim_completion".to_string(),
            scope: "project:-repo-app".to_string(),
            obligation: args
                .get("obligation")
                .and_then(Value::as_str)
                .map(str::to_owned),
            gated_chain_position: None,
            latest_chain_position: None,
            request_sha256: thruline_core::chain::sha256_hex(
                serde_json::to_string(args).expect("json").as_bytes(),
            ),
            manifest_json: args.get("manifest").map(|m| m.to_string()),
            manifest_sha256: None,
            outcome: "ready_for_verification".to_string(),
            accepted_at_unix_ms: 1_788_134_400_000,
        })
        .expect("record claim");
}

fn deliver_call(db: &std::path::Path, args: &Value) {
    let ledger = Ledger::open(db).expect("ledger");
    let payload = json!({
        "session_id": SESSION,
        "transcript_path": TRANSCRIPT,
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

#[test]
fn a_verdict_answers_with_the_manifest_checked_entry_by_entry() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let dir = temp.path().join("digest");
    Ledger::open(&db).expect("create");
    let args = json!({
        "project": "-repo-app",
        "obligation": "ship the retry logic",
        "chain_position": "0000",
        "manifest": {"tests": ["cargo test"], "release": "not checkable"},
    });
    record_fixed_claim(&db, &args);
    deliver_call(&db, &args);
    let ledger = Ledger::open(&db).expect("ledger");
    let report = verifier::verify_claims(
        &ledger,
        &thruline_core::sensor::spill_path(&db),
        "2026-08-31T04:00:00.000Z",
    )
    .expect("verify");
    let short = report[0].verdict_short_id.clone();
    drop(ledger);

    let out = rendered(resolve(&db, &dir, &short));
    insta::assert_snapshot!("why_claim_verdict", out);
}

#[test]
fn a_claim_call_answers_with_its_standing_and_history() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let dir = temp.path().join("digest");
    Ledger::open(&db).expect("create");
    let args = json!({
        "project": "-repo-app",
        "obligation": "ship the retry logic",
        "chain_position": "0000",
        "manifest": {"tests": ["cargo test"]},
    });
    record_fixed_claim(&db, &args);

    // Before any verdict: the claim names the verifier as its next step.
    let out = rendered(resolve(&db, &dir, "tlc_aaaaaaaaaaaa"));
    insta::assert_snapshot!("why_claim_awaiting", out);
    assert!(out.contains("no verdict yet"), "{out}");
}

/* ── refusals ────────────────────────────────────────────────────────── */

#[test]
fn an_unknown_id_is_not_found_and_a_bare_prefix_is_ambiguous() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let dir = temp.path().join("digest");
    let ledger = Ledger::open(&db).expect("ledger");

    // Nothing on record at all.
    assert!(matches!(
        verifier::why_truth_loop(&ledger, &dir, "spkf_000000000000").expect("resolve"),
        Some(WhyResolution::NotFound)
    ));
    // A needle with no truth-loop prefix belongs to another namespace.
    assert!(verifier::why_truth_loop(&ledger, &dir, "att_something")
        .expect("resolve")
        .is_none());
    drop(ledger);

    // Two findings; the bare prefix matches both and is refused.
    let ledger = Ledger::open(&db).expect("ledger");
    let mut turn = no_proof_turn();
    turn.claims.push(TurnClaim {
        text: "Committed the schema change on `main`.".to_string(),
        kind: ClaimKind::Work,
        marker: Marker::FirstPersonPast,
        artifacts: Vec::new(),
        line: 3,
    });
    let digest = speaker::compose(&scope("stop"), &turn, &EvidenceIndex::build(&[]), &[]);
    assert_eq!(digest.lines.len(), 2);
    speaker::record_findings(&ledger, &digest, &turn, "2026-08-31T02:02:00.000Z").expect("record");
    match verifier::why_truth_loop(&ledger, &dir, "spkf_").expect("resolve") {
        Some(WhyResolution::Ambiguous(ids)) => {
            assert_eq!(ids.len(), 2, "both candidates are listed: {ids:?}")
        }
        other => panic!("expected a refusal to guess, got {other:?}"),
    }
}
