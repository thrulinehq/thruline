//! A199 Sessions 2–3 — the speaker digest: final-turn claim extraction,
//! deterministic evidence binding, the ≤3-line renderer, and the
//! `speaker_findings` announce-to-ledger record.
//!
//! Fixtures are generated (A90): each test writes a synthetic transcript and
//! appends synthetic PostToolUse deliveries through the real sensor appender,
//! so the evidence rows carry real chain links and the binding runs against
//! exactly what production reads.
//!
//! REGENERATING THE GOLDEN FILES:
//!     INSTA_UPDATE=always cargo test -p thruline-core --test a200_speaker_digest
//! Read the diff before accepting it: a changed digest line is a changed
//! accusation surface.

use std::path::{Path, PathBuf};

use serde_json::json;
use tempfile::tempdir;
use thruline_core::speaker::{
    self, bind, compose, final_turn, render_markdown, Binding, EvidenceIndex,
};
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
    let text: String = lines
        .iter()
        .map(|l| format!("{l}\n"))
        .collect::<Vec<_>>()
        .join("");
    std::fs::write(&path, text).expect("write transcript");
    path
}

fn append_post_tool(
    ledger: &Ledger,
    index: usize,
    tool: &str,
    input: serde_json::Value,
    response: serde_json::Value,
) {
    let payload = json!({
        "session_id": SESSION,
        "transcript_path": TRANSCRIPT,
        "cwd": "/repo",
        "hook_event_name": "PostToolUse",
        "tool_name": tool,
        "tool_input": input,
        "tool_response": response,
    });
    thruline_core::sensor::append(
        ledger,
        "PostToolUse",
        payload.to_string().as_bytes(),
        &format!("2026-08-31T02:01:{index:02}.000Z"),
    )
    .expect("sensor append");
}

fn evidence(ledger: &Ledger) -> EvidenceIndex {
    let rows = speaker::session_post_tool_rows(ledger, SESSION).expect("session rows");
    EvidenceIndex::build(&rows)
}

fn digest_for(
    turn: &thruline_core::speaker::FinalTurn,
    idx: &EvidenceIndex,
    hook: &str,
) -> thruline_core::speaker::Digest {
    compose(
        &thruline_core::speaker::DigestScope {
            project_key: "-repo-app",
            session_id: SESSION,
            transcript_path: TRANSCRIPT,
            generated_at: "2026-08-31T02:02:00.000Z",
            hook,
        },
        turn,
        idx,
        &[],
    )
}

/* ── the project key ─────────────────────────────────────────────────── */

#[test]
fn project_key_prefers_the_transcript_parent_and_falls_back_to_munged_cwd() {
    assert_eq!(
        speaker::project_key(Some(TRANSCRIPT), Some("/repo")),
        Some("-repo-app".to_string()),
        "the vendor's own directory name is the key"
    );
    assert_eq!(
        speaker::project_key(None, Some("/Users/dev/thru line")),
        Some("-Users-dev-thru-line".to_string()),
        "cwd is munged the way the vendor munges it"
    );
    assert_eq!(speaker::project_key(None, None), None);
    assert_eq!(speaker::project_key(None, Some("  ")), None);
}

/* ── the final turn ──────────────────────────────────────────────────── */

#[test]
fn final_turn_reads_only_after_the_last_user_prompt() {
    let temp = tempdir().expect("tempdir");
    let path = write_transcript(
        temp.path(),
        &[
            user(1, "first ask"),
            assistant_text(2, "Fixed the retry logic in `client.rs`."),
            user(3, "second ask"),
            assistant_text(4, "All 12 tests pass."),
        ],
    );
    let turn = final_turn(&path).expect("final turn");
    assert_eq!(turn.boundary_line, Some(3));
    assert_eq!(
        turn.claims.len(),
        1,
        "the first turn's claim is out of scope"
    );
    assert_eq!(turn.claims[0].text, "All 12 tests pass.");
    assert_eq!(turn.claims[0].line, 4, "the receipt is the JSONL line");
}

#[test]
fn a_transcript_with_no_user_text_yields_no_claims() {
    let temp = tempdir().expect("tempdir");
    let path = write_transcript(temp.path(), &[assistant_text(1, "Deployed `app`.")]);
    let turn = final_turn(&path).expect("final turn");
    assert!(turn.claims.is_empty());
    assert_eq!(turn.boundary_line, None);
}

#[test]
fn a_missing_transcript_is_an_error_the_caller_fails_open_on() {
    assert!(final_turn(Path::new("/nonexistent/nope.jsonl")).is_err());
}

/* ── binding ─────────────────────────────────────────────────────────── */

#[test]
fn a_test_claim_with_a_passing_run_in_the_record_is_evidenced_and_silent() {
    let temp = tempdir().expect("tempdir");
    let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    append_post_tool(
        &ledger,
        1,
        "Bash",
        json!({"command": "cargo test"}),
        json!({"stdout": "test result: ok. 12 passed; 0 failed; 0 ignored", "stderr": "", "interrupted": false}),
    );
    let path = write_transcript(
        temp.path(),
        &[user(1, "run them"), assistant_text(2, "All 12 tests pass.")],
    );
    let turn = final_turn(&path).expect("final turn");
    let idx = evidence(&ledger);
    assert_eq!(bind(&turn.claims[0], &idx), Binding::Evidenced);
    let digest = digest_for(&turn, &idx, "session-end");
    assert!(digest.lines.is_empty(), "evidenced means silence");
}

#[test]
fn a_test_claim_with_no_run_in_the_record_is_no_proof_found() {
    let temp = tempdir().expect("tempdir");
    let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    let path = write_transcript(
        temp.path(),
        &[user(1, "run them"), assistant_text(2, "All 12 tests pass.")],
    );
    let turn = final_turn(&path).expect("final turn");
    let idx = evidence(&ledger);
    match bind(&turn.claims[0], &idx) {
        Binding::NoProofFound { expected } => {
            assert!(
                expected.contains("test"),
                "names what was looked for: {expected}"
            )
        }
        other => panic!("expected no proof found, got {other:?}"),
    }
}

#[test]
fn a_failing_run_does_not_evidence_an_all_pass_claim() {
    let temp = tempdir().expect("tempdir");
    let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    append_post_tool(
        &ledger,
        1,
        "Bash",
        json!({"command": "cargo test"}),
        json!({"stdout": "test result: FAILED. 11 passed; 1 failed", "stderr": "", "interrupted": false}),
    );
    let path = write_transcript(
        temp.path(),
        &[user(1, "run them"), assistant_text(2, "All 12 tests pass.")],
    );
    let turn = final_turn(&path).expect("final turn");
    let idx = evidence(&ledger);
    assert!(
        matches!(bind(&turn.claims[0], &idx), Binding::NoProofFound { .. }),
        "a run that reads as failure is not proof of all-pass — and the line still \
         only says no proof was found, never that the claim is false"
    );
}

#[test]
fn a_file_claim_binds_to_an_edit_touching_what_it_names() {
    let temp = tempdir().expect("tempdir");
    let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    append_post_tool(
        &ledger,
        1,
        "Edit",
        json!({"file_path": "/repo/src/client.rs", "old_string": "a", "new_string": "b"}),
        json!({"filePath": "/repo/src/client.rs"}),
    );
    let path = write_transcript(
        temp.path(),
        &[
            user(1, "fix it"),
            assistant_text(2, "I've added retry logic to `client.rs`."),
        ],
    );
    let turn = final_turn(&path).expect("final turn");
    let idx = evidence(&ledger);
    assert_eq!(bind(&turn.claims[0], &idx), Binding::Evidenced);
}

#[test]
fn a_file_claim_with_no_matching_edit_is_no_proof_found() {
    let temp = tempdir().expect("tempdir");
    let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    append_post_tool(
        &ledger,
        1,
        "Edit",
        json!({"file_path": "/repo/src/other.rs", "old_string": "a", "new_string": "b"}),
        json!({"filePath": "/repo/src/other.rs"}),
    );
    let path = write_transcript(
        temp.path(),
        &[
            user(1, "fix it"),
            assistant_text(2, "I've added retry logic to `client.rs`."),
        ],
    );
    let turn = final_turn(&path).expect("final turn");
    let idx = evidence(&ledger);
    assert!(matches!(
        bind(&turn.claims[0], &idx),
        Binding::NoProofFound { .. }
    ));
}

#[test]
fn a_commit_claim_binds_to_a_git_commit_row() {
    let temp = tempdir().expect("tempdir");
    let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    append_post_tool(
        &ledger,
        1,
        "Bash",
        json!({"command": "git commit -m 'checkpoint'"}),
        json!({"stdout": "[main abc1234] checkpoint", "stderr": "", "interrupted": false}),
    );
    let path = write_transcript(
        temp.path(),
        &[
            user(1, "ship it"),
            assistant_text(2, "Committed the parser fix on `main`."),
        ],
    );
    let turn = final_turn(&path).expect("final turn");
    let idx = evidence(&ledger);
    assert_eq!(bind(&turn.claims[0], &idx), Binding::Evidenced);
}

#[test]
fn an_unmatchable_claim_is_unknown_and_unknown_never_accuses() {
    let temp = tempdir().expect("tempdir");
    let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    let path = write_transcript(
        temp.path(),
        &[user(1, "go"), assistant_text(2, "Deployed.")],
    );
    let turn = final_turn(&path).expect("final turn");
    assert_eq!(turn.claims.len(), 1, "a bare completion verb is a claim");
    let idx = evidence(&ledger);
    assert_eq!(
        bind(&turn.claims[0], &idx),
        Binding::Unknown,
        "no deterministic class covers a bare deploy — ambiguity stays silent"
    );
    let digest = digest_for(&turn, &idx, "session-end");
    assert!(digest.lines.is_empty());
}

#[test]
fn findings_and_epistemic_prose_never_reach_the_digest() {
    let temp = tempdir().expect("tempdir");
    let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    let path = write_transcript(
        temp.path(),
        &[
            user(1, "diagnose"),
            assistant_text(
                2,
                "The root cause is the stale index in `derive.rs`. \
                 Files confirmed append-only empirically.",
            ),
        ],
    );
    let turn = final_turn(&path).expect("final turn");
    assert!(
        turn.claims.is_empty(),
        "diagnoses are findings, not completion claims: {:?}",
        turn.claims
    );
    let digest = digest_for(&turn, &evidence(&ledger), "session-end");
    assert!(digest.lines.is_empty());
}

/* ── promises ────────────────────────────────────────────────────────── */

#[test]
fn a_trailing_promise_is_left_open_and_a_followed_one_is_silent() {
    let temp = tempdir().expect("tempdir");
    let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");

    // Promise with tool activity after it: silence.
    let followed = write_transcript(
        temp.path(),
        &[
            user(1, "go"),
            assistant_text(2, "I'll wire the retry logic into `server.rs` next."),
            assistant_tool(3, "Edit", json!({"file_path": "/repo/src/server.rs"})),
        ],
    );
    let turn = final_turn(&followed).expect("final turn");
    let digest = digest_for(&turn, &evidence(&ledger), "session-end");
    assert!(
        digest.lines.is_empty(),
        "activity after the promise → fulfilment is not judgeable → silence"
    );

    // Promise as the last thing the session said: left open, as a fact.
    let trailing = write_transcript(
        temp.path(),
        &[
            user(1, "go"),
            assistant_tool(2, "Edit", json!({"file_path": "/repo/src/client.rs"})),
            assistant_text(3, "I'll wire the retry logic into `server.rs` next."),
        ],
    );
    let turn = final_turn(&trailing).expect("final turn");
    let digest = digest_for(&turn, &evidence(&ledger), "session-end");
    assert_eq!(digest.lines.len(), 1);
    assert_eq!(digest.lines[0].kind, "open_promise");
    assert_eq!(digest.lines[0].transcript_line, Some(3));
}

/* ── the renderer ────────────────────────────────────────────────────── */

#[test]
fn digest_renderer_no_proof_and_promise() {
    let temp = tempdir().expect("tempdir");
    let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    append_post_tool(
        &ledger,
        1,
        "Edit",
        json!({"file_path": "/repo/src/unrelated.rs"}),
        json!({"filePath": "/repo/src/unrelated.rs"}),
    );
    let path = write_transcript(
        temp.path(),
        &[
            user(1, "finish the release"),
            assistant_text(
                2,
                "All 54 proxy tests pass. I've added retry logic to `client.rs`. \
                 I'll wire the new panel into `layout.rs` next.",
            ),
        ],
    );
    let turn = final_turn(&path).expect("final turn");
    let digest = digest_for(&turn, &evidence(&ledger), "session-end");
    let rendered = render_markdown(&digest);
    for line in rendered.lines() {
        assert_eq!(
            thruline_core::voice::banned_in(line),
            None,
            "engine lexicon reached the brief: {line:?}"
        );
        let words: Vec<String> = line
            .to_ascii_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(str::to_owned)
            .collect();
        for forbidden in ["lie", "lied", "lies", "lying", "caught", "false"] {
            assert!(
                !words.iter().any(|w| w == forbidden),
                "forbidden vocabulary (A199): {forbidden:?} in {line:?}"
            );
        }
    }
    insta::assert_snapshot!("digest_no_proof_and_promise", rendered);
}

#[test]
fn digest_renderer_caps_at_three_lines_and_stays_inside_the_token_budget() {
    let temp = tempdir().expect("tempdir");
    let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    let path = write_transcript(
        temp.path(),
        &[
            user(1, "do everything"),
            assistant_text(
                2,
                "All 12 parser tests pass. Committed the schema change on `main`. \
                 I've added retry logic to `client.rs`. Fixed the panel wiring in `layout.rs`.",
            ),
        ],
    );
    let turn = final_turn(&path).expect("final turn");
    let digest = digest_for(&turn, &evidence(&ledger), "session-end");
    assert!(
        digest.lines.len() <= speaker::MAX_LINES,
        "at most three lines, ever"
    );
    let rendered = render_markdown(&digest);
    assert!(
        rendered.len() <= 1_600,
        "the rendered brief must sit far inside the 400-token budget: {} bytes",
        rendered.len()
    );
    insta::assert_snapshot!("digest_capped_at_three_lines", rendered);
}

#[test]
fn an_empty_digest_renders_nothing_and_clears_the_cache() {
    let temp = tempdir().expect("tempdir");
    let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    let dir = temp.path().join("digest");

    // Seed a cache, then clear it with an empty digest.
    let path = write_transcript(
        temp.path(),
        &[user(1, "run them"), assistant_text(2, "All 12 tests pass.")],
    );
    let turn = final_turn(&path).expect("final turn");
    let idx = evidence(&ledger);
    let full = digest_for(&turn, &idx, "session-end");
    speaker::write_cache(&dir, &full).expect("write");
    assert!(speaker::read_cache_markdown(&dir, "-repo-app").is_some());

    let empty = digest_for(
        &thruline_core::speaker::FinalTurn::default(),
        &idx,
        "session-end",
    );
    assert_eq!(render_markdown(&empty), "");
    speaker::write_cache(&dir, &empty).expect("clear");
    assert!(
        speaker::read_cache_markdown(&dir, "-repo-app").is_none(),
        "nothing to say means an absent cache, not filler"
    );
}

/* ── the announce-to-ledger record ───────────────────────────────────── */

#[test]
fn findings_append_idempotently_and_the_chain_verifies() {
    let temp = tempdir().expect("tempdir");
    let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    let path = write_transcript(
        temp.path(),
        &[
            user(1, "run them"),
            assistant_text(2, "All 12 tests pass. Committed the fix on `main`."),
        ],
    );
    let turn = final_turn(&path).expect("final turn");
    let idx = evidence(&ledger);
    let digest = digest_for(&turn, &idx, "stop");
    assert_eq!(digest.lines.len(), 2, "two no-proof completion claims");

    let first = speaker::record_findings(&ledger, &digest, &turn, "2026-08-31T02:03:00.000Z")
        .expect("record");
    assert_eq!(first, 2);
    let second = speaker::record_findings(&ledger, &digest, &turn, "2026-08-31T02:04:00.000Z")
        .expect("record again");
    assert_eq!(second, 0, "the same turn audited twice is one record");

    let checked = speaker::verify_findings_chain(&ledger).expect("verify");
    assert_eq!(checked, 2, "every link re-folds");

    // Append-only is enforced by the database, not by convention.
    let err = ledger
        .conn()
        .execute("UPDATE speaker_findings SET claim_text = 'rewritten'", []);
    assert!(err.is_err(), "updates must be refused by trigger");
    let err = ledger.conn().execute("DELETE FROM speaker_findings", []);
    assert!(err.is_err(), "deletes must be refused by trigger");
}
