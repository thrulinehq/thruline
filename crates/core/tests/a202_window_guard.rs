//! A202 — the window-completeness guard.
//!
//! The precision gauntlet (2026-08-31) found the false-accusation vector the
//! A201 spill guard cannot see: on resume the vendor mints a NEW session id
//! and rewrites the transcript under it, so every pre-resume PostToolUse
//! delivery lives under the ORIGINAL id — the binder's indexed scan binds a
//! completion claim against a 0-row (or truncated) window while the
//! transcript itself holds hundreds of main-thread tool_use events, and no
//! spill line exists to trip the A201 guard. 5 of 24 gauntlet no-proof lines
//! had this shape.
//!
//! The guard is deterministic and mirrors the spill guard's honesty pattern:
//!
//! * **Transcript deficit** — the transcript's own count of main-thread
//!   tool_use events exceeds the scanned sensor rows. ANY deficit marks the
//!   window incomplete (one unseen event could be the very proof a no-proof
//!   line would deny); surplus means nothing, because sidechain deliveries
//!   share the session id and can only inflate the scanned count.
//! * **Resume birth** — the session's first recorded SessionStart names
//!   `source: "resume"`. Conservative even when counts balance: sidechain
//!   deliveries can mask a main-thread deficit, and the prior id's evidence
//!   cannot be joined deterministically (stitching is future work).
//!
//! Either gap degrades NoProofFound → Unknown (silence) with the reason
//! recorded, degrades "left open" promise lines to silence (a compromised
//! record must not claim a promise was left dangling), and never touches an
//! Evidenced verdict — a gap can only hide proof, never create it.

use std::path::{Path, PathBuf};

use serde_json::json;
use tempfile::tempdir;
use thruline_core::speaker::{
    self, bind, compose, final_turn, session_born_from_resume, Binding, EvidenceIndex,
};
use thruline_core::verifier::{evaluate, ClaimVerdict, SessionBasis};
use thruline_core::Ledger;

const SESSION: &str = "aaaaaaaa-1111-4222-8333-444444444444";
const TRANSCRIPT: &str =
    "/home/dev/.claude/projects/-repo-app/aaaaaaaa-1111-4222-8333-444444444444.jsonl";

fn envelope(index: usize, role: &str, content: serde_json::Value) -> serde_json::Value {
    json!({
        "type": role,
        "uuid": format!("00000000-0000-4000-8000-{index:012}"),
        "isSidechain": false,
        "sessionId": SESSION,
        "timestamp": format!("2026-08-31T02:00:{index:02}.000Z"),
        "cwd": "/repo",
        "message": {"role": role, "content": content},
    })
}

fn user(index: usize, text: &str) -> serde_json::Value {
    envelope(index, "user", json!(text))
}

fn assistant_text(index: usize, text: &str) -> serde_json::Value {
    envelope(index, "assistant", json!([{"type": "text", "text": text}]))
}

fn assistant_tool(index: usize, name: &str, input: serde_json::Value) -> serde_json::Value {
    envelope(
        index,
        "assistant",
        json!([{"type": "tool_use", "id": format!("t{index}"), "name": name, "input": input}]),
    )
}

fn write_transcript(dir: &Path, lines: &[serde_json::Value]) -> PathBuf {
    let path = dir.join("session.jsonl");
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(&path, text).expect("write transcript");
    path
}

fn append_post_tool(ledger: &Ledger, index: usize, tool: &str, input: serde_json::Value) {
    let payload = json!({
        "session_id": SESSION,
        "transcript_path": TRANSCRIPT,
        "cwd": "/repo",
        "hook_event_name": "PostToolUse",
        "tool_name": tool,
        "tool_input": input,
        "tool_response": {"stdout": "", "stderr": "", "interrupted": false},
    });
    thruline_core::sensor::append(
        ledger,
        "PostToolUse",
        payload.to_string().as_bytes(),
        &format!("2026-08-31T02:01:{index:02}.000Z"),
    )
    .expect("sensor append");
}

fn append_session_start(ledger: &Ledger, index: usize, session: &str, source: &str) {
    let payload = json!({
        "session_id": session,
        "transcript_path": TRANSCRIPT,
        "cwd": "/repo",
        "hook_event_name": "SessionStart",
        "source": source,
    });
    thruline_core::sensor::append(
        ledger,
        "SessionStart",
        payload.to_string().as_bytes(),
        &format!("2026-08-31T02:00:{index:02}.000Z"),
    )
    .expect("sensor append");
}

/// user → two tool calls → the unproven test claim: the gauntlet's
/// false-accusation shape in miniature once the sensor rows are absent.
fn claim_transcript(dir: &Path) -> PathBuf {
    write_transcript(
        dir,
        &[
            user(1, "run them"),
            assistant_tool(2, "Bash", json!({"command": "cargo test"})),
            assistant_tool(3, "Bash", json!({"command": "cargo test"})),
            assistant_text(4, "All 12 tests pass."),
        ],
    )
}

fn digest_for(
    turn: &thruline_core::speaker::FinalTurn,
    idx: &EvidenceIndex,
) -> thruline_core::speaker::Digest {
    compose(
        &speaker::DigestScope {
            project_key: "-repo-app",
            session_id: SESSION,
            transcript_path: TRANSCRIPT,
            generated_at: "2026-08-31T02:02:00.000Z",
            hook: "stop",
        },
        turn,
        idx,
        &[],
    )
}

/* ── the transcript-deficit gap ──────────────────────────────────────── */

#[test]
fn a_zero_row_window_against_a_tool_bearing_transcript_degrades_to_silence() {
    let temp = tempdir().expect("tempdir");
    let turn = final_turn(&claim_transcript(temp.path())).expect("final turn");
    assert_eq!(turn.tool_use_count, 2, "the transcript's own testimony");

    // Without transcript testimony this exact claim is a no-proof line —
    // the pre-A202 behaviour, and the gauntlet's SEV-1 shape.
    let blind = EvidenceIndex::build(&[]);
    assert!(matches!(
        bind(&turn.claims[0], &blind),
        Binding::NoProofFound { .. }
    ));

    // With it, the 0-row window is known incomplete: silence, plus the
    // recorded reason in the digest's own words.
    let mut idx = EvidenceIndex::build(&[]);
    idx.transcript_tool_events = Some(turn.tool_use_count);
    assert_eq!(bind(&turn.claims[0], &idx), Binding::Unknown);

    let digest = digest_for(&turn, &idx);
    assert!(
        digest.lines.is_empty(),
        "no accusation over an incomplete window: {:?}",
        digest.lines
    );
    assert_eq!(
        digest.evidence_window_note.as_deref(),
        Some("evidence window incomplete: transcript holds 2 tool events, session record holds 0")
    );
    assert_eq!(digest.transcript_tool_events, Some(2));
    assert!(!digest.resume_started);
}

#[test]
fn any_deficit_is_incomplete_and_a_balanced_or_surplus_window_binds_unchanged() {
    let temp = tempdir().expect("tempdir");
    let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    // One chained row that evidences nothing the claim names.
    append_post_tool(&ledger, 1, "Bash", json!({"command": "ls"}));
    let turn = final_turn(&claim_transcript(temp.path())).expect("final turn");
    let rows = speaker::session_post_tool_rows(&ledger, SESSION).expect("rows");
    let mut idx = EvidenceIndex::build(&rows);
    assert_eq!(idx.rows_scanned, 1);

    // Deficit of one: incomplete, silence.
    idx.transcript_tool_events = Some(2);
    assert_eq!(bind(&turn.claims[0], &idx), Binding::Unknown);

    // Balanced: complete, and the A200/A201 verdict stands untouched.
    idx.transcript_tool_events = Some(1);
    assert!(matches!(
        bind(&turn.claims[0], &idx),
        Binding::NoProofFound { .. }
    ));

    // Surplus (sidechain deliveries share the session id): never a gap.
    idx.transcript_tool_events = Some(0);
    assert!(matches!(
        bind(&turn.claims[0], &idx),
        Binding::NoProofFound { .. }
    ));
    let digest = digest_for(&turn, &idx);
    assert_eq!(
        digest.evidence_window_note, None,
        "a complete window records no gap"
    );
}

/* ── the resume-birth gap ────────────────────────────────────────────── */

#[test]
fn a_resume_born_session_stays_conservative_even_when_counts_balance() {
    let temp = tempdir().expect("tempdir");
    let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    append_post_tool(&ledger, 1, "Bash", json!({"command": "ls"}));
    append_post_tool(&ledger, 2, "Bash", json!({"command": "ls"}));
    let turn = final_turn(&claim_transcript(temp.path())).expect("final turn");
    let rows = speaker::session_post_tool_rows(&ledger, SESSION).expect("rows");
    let mut idx = EvidenceIndex::build(&rows);
    idx.transcript_tool_events = Some(turn.tool_use_count);
    assert!(!idx.capture_gap(), "counts balance: no deficit");

    idx.resume_started = true;
    assert_eq!(
        bind(&turn.claims[0], &idx),
        Binding::Unknown,
        "the prior id's evidence cannot be joined, so absence cannot be established"
    );
    let digest = digest_for(&turn, &idx);
    assert!(digest.lines.is_empty());
    assert_eq!(
        digest.evidence_window_note.as_deref(),
        Some(
            "evidence window incomplete: the session began as a resume, and evidence from \
             before it is recorded under the prior session id"
        )
    );
    assert!(digest.resume_started);
}

#[test]
fn session_born_from_resume_reads_the_first_recorded_session_start_only() {
    let temp = tempdir().expect("tempdir");
    let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    let resumed = "bbbbbbbb-2222-4333-8444-555555555555";
    let startup_born = "cccccccc-3333-4444-8555-666666666666";

    // Born from a resume: the first (and only) SessionStart names it.
    append_session_start(&ledger, 1, resumed, "resume");
    // Born at startup, later resumed in place: the id's own capture covers
    // its whole life, so the resume signal must NOT fire (the gauntlet's
    // startup-born sessions with later resumes had complete windows).
    append_session_start(&ledger, 2, startup_born, "startup");
    append_session_start(&ledger, 3, startup_born, "resume");
    append_session_start(&ledger, 4, startup_born, "compact");

    assert!(session_born_from_resume(&ledger, resumed).expect("query"));
    assert!(!session_born_from_resume(&ledger, startup_born).expect("query"));
    assert!(
        !session_born_from_resume(&ledger, "dddddddd-5555-4666-8777-888888888888").expect("query"),
        "no SessionStart on record: not known resumed, the deficit check still guards"
    );
}

/* ── gaps hide proof, never create it ────────────────────────────────── */

#[test]
fn evidenced_claims_stay_evidenced_under_every_gap() {
    let temp = tempdir().expect("tempdir");
    let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    let payload = json!({
        "session_id": SESSION,
        "transcript_path": TRANSCRIPT,
        "cwd": "/repo",
        "hook_event_name": "PostToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": "cargo test"},
        "tool_response": {"stdout": "test result: ok. 12 passed; 0 failed", "stderr": "", "interrupted": false},
    });
    thruline_core::sensor::append(
        &ledger,
        "PostToolUse",
        payload.to_string().as_bytes(),
        "2026-08-31T02:01:00.000Z",
    )
    .expect("append");

    let turn = final_turn(&claim_transcript(temp.path())).expect("final turn");
    let rows = speaker::session_post_tool_rows(&ledger, SESSION).expect("rows");
    let mut idx = EvidenceIndex::build(&rows);
    idx.spilled_deliveries = 3;
    idx.transcript_tool_events = Some(500);
    idx.resume_started = true;
    assert_eq!(bind(&turn.claims[0], &idx), Binding::Evidenced);
}

/* ── promises under an incomplete window ─────────────────────────────── */

#[test]
fn promises_degrade_over_a_capture_gap_but_survive_a_spill_alone() {
    let temp = tempdir().expect("tempdir");
    // A tool call, a fresh prompt, then a trailing promise nothing follows.
    let transcript = write_transcript(
        temp.path(),
        &[
            user(1, "look around"),
            assistant_tool(2, "Bash", json!({"command": "ls"})),
            user(3, "and next?"),
            assistant_text(4, "I'll wire the new panel into `layout.rs` next."),
        ],
    );
    let turn = final_turn(&transcript).expect("final turn");
    assert_eq!(turn.tool_use_count, 1);

    let open_promises = |idx: &EvidenceIndex| -> usize {
        digest_for(&turn, idx)
            .lines
            .iter()
            .filter(|l| l.kind == "open_promise")
            .count()
    };

    // Complete window: the promise line stands (the A200 behaviour).
    let mut idx = EvidenceIndex::build(&[]);
    idx.transcript_tool_events = Some(0);
    assert_eq!(open_promises(&idx), 1);

    // A spill alone leaves it standing (pinned at A201: promises bind
    // against the transcript, which a spilled sensor delivery cannot change).
    idx.spilled_deliveries = 2;
    assert_eq!(open_promises(&idx), 1);

    // A transcript deficit compromises the record's account of the turn:
    // the promise degrades to silence with the reason on record.
    let mut idx = EvidenceIndex::build(&[]);
    idx.transcript_tool_events = Some(turn.tool_use_count);
    assert_eq!(open_promises(&idx), 0);
    assert!(digest_for(&turn, &idx)
        .evidence_window_note
        .is_some_and(|n| n.contains("transcript holds 1 tool event")));

    // So does a resume birth.
    let mut idx = EvidenceIndex::build(&[]);
    idx.transcript_tool_events = Some(0);
    idx.resume_started = true;
    assert_eq!(open_promises(&idx), 0);
}

/* ── the verifier's copy of the guard ────────────────────────────────── */

#[test]
fn the_verifier_abstains_for_a_resume_born_claiming_session() {
    let temp = tempdir().expect("tempdir");
    let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    let manifest = json!({"tests": ["cargo test"]});
    let session = || SessionBasis::Identified {
        session_id: SESSION.to_string(),
        seq: 0,
        chain_hash: "abc".to_string(),
    };

    // Not known resumed: absence over an empty-but-complete window accuses,
    // exactly as at A201.
    let mut idx = EvidenceIndex::build(&[]);
    idx.resume_started = session_born_from_resume(&ledger, SESSION).expect("query");
    assert!(!idx.resume_started);
    let before = evaluate(Some(&manifest), session(), Some(&idx), None);
    assert_eq!(before.verdict, ClaimVerdict::NoProofFound);

    // Resume-born: the same manifest degrades to unknown, and both the
    // verdict reason and the per-entry basis say why.
    append_session_start(&ledger, 1, SESSION, "resume");
    let mut idx = EvidenceIndex::build(&[]);
    idx.resume_started = session_born_from_resume(&ledger, SESSION).expect("query");
    assert!(idx.resume_started);
    let after = evaluate(Some(&manifest), session(), Some(&idx), None);
    assert_eq!(after.verdict, ClaimVerdict::Unknown);
    assert_eq!(
        after.reason,
        "evidence window incomplete: the session began as a resume, and evidence from \
         before it is recorded under the prior session id"
    );
    assert!(after
        .entries
        .iter()
        .all(|e| e.outcome == "unknown" && e.basis.contains("absence cannot be established")));
}
