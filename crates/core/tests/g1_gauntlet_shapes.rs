//! G1 — synthetic regression fixtures for the first precision gauntlet
//! (A203).
//!
//! The gauntlet replayed 29 real sessions and adjudicated every emitted line
//! twice over (plus a tie-break): 27 of 30 lines were false accusations, 1
//! ambiguous, 2 true. Each test here reproduces ONE adjudicated failure
//! shape with invented project names, invented file names, and invented
//! sentences — never the corpus text — and asserts the line is NOT emitted;
//! the two true shapes assert their lines still ARE. The mechanisms:
//!
//! * **M2 — matcher blindness**: proof present in scanned rows in shapes the
//!   recognizer does not read. The fix is not more shapes; it is the A203
//!   accounting rule — a window holding rows outside the recognized
//!   vocabulary cannot establish absence, so the verdict degrades to
//!   silence with the reason on record.
//! * **M3 — extraction misfires**: citation footers, relayed state, debt
//!   reports, admissions, past-tense events and user-directed imperatives
//!   bound as claims or promises. Killed in `speaker_gate`.
//! * **M4 — taxonomy**: claims whose honest evidence has no deterministic
//!   class were accused by construction. Now refused with
//!   "no checkable evidence class for this claim".
//!
//! The A202 shapes (resume birth, transcript deficit) are pinned by
//! `a202_window_guard.rs` and not repeated here.

use std::path::{Path, PathBuf};

use serde_json::json;
use tempfile::tempdir;
use thruline_core::extract::Marker;
use thruline_core::speaker::{
    self, bind_explained, compose, final_turn, Binding, EvidenceIndex, EvidenceRowLite, FinalTurn,
};
use thruline_core::speaker_gate::{
    admissible_promise, admit_completion, GateVerdict, CITATION_REASON, DEBT_REASON,
    NO_CLASS_REASON,
};

const SESSION: &str = "abcdabcd-1111-4222-8333-444444444444";
const TRANSCRIPT: &str =
    "/home/dev/.claude/projects/-repo-app/abcdabcd-1111-4222-8333-444444444444.jsonl";

fn envelope(index: usize, role: &str, content: serde_json::Value) -> serde_json::Value {
    json!({
        "type": role,
        "uuid": format!("00000000-0000-4000-8000-{index:012}"),
        "isSidechain": false,
        "sessionId": SESSION,
        "timestamp": format!("2026-08-31T09:00:{index:02}.000Z"),
        "cwd": "/repo",
        "message": {"role": role, "content": content},
    })
}

/// A one-turn transcript: a user prompt, then the final message under test.
fn turn_for(dir: &Path, final_message: &str) -> FinalTurn {
    let path: PathBuf = dir.join("session.jsonl");
    let lines = [
        envelope(1, "user", json!("go")),
        envelope(
            2,
            "assistant",
            json!([{"type": "text", "text": final_message}]),
        ),
    ];
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(&path, text).expect("write transcript");
    final_turn(&path).expect("final turn")
}

/// A hand-built evidence row — the binder consumes exactly this shape.
fn row(seq: i64, tool: &str, input: serde_json::Value, tail: &str) -> EvidenceRowLite {
    EvidenceRowLite {
        seq,
        chain_hash: format!("hash{seq}"),
        tool_name: tool.to_string(),
        tool_input: input.to_string(),
        response_tail: tail.to_string(),
        interrupted: false,
        cwd: Some("/repo".to_string()),
    }
}

/// An index over `rows` whose window is complete as far as A201/A202 can
/// tell — the fixtures isolate the A203 mechanisms from the earlier guards.
fn complete_index(rows: &[EvidenceRowLite]) -> EvidenceIndex {
    let mut idx = EvidenceIndex::build(rows);
    idx.transcript_tool_events = Some(0); // surplus or balance: never a gap
    idx
}

fn digest_for(turn: &FinalTurn, idx: &EvidenceIndex) -> speaker::Digest {
    compose(
        &speaker::DigestScope {
            project_key: "-repo-app",
            session_id: SESSION,
            transcript_path: TRANSCRIPT,
            generated_at: "2026-08-31T09:02:00.000Z",
            hook: "stop",
        },
        turn,
        idx,
        &[],
    )
}

/// The core assertion for a false-accusation shape: the final message, bound
/// over this window, produces NO no-proof line and NO open-promise line.
fn assert_silent(final_message: &str, rows: &[EvidenceRowLite]) {
    let temp = tempdir().expect("tempdir");
    let turn = turn_for(temp.path(), final_message);
    let digest = digest_for(&turn, &complete_index(rows));
    assert!(
        digest.lines.is_empty(),
        "this shape was adjudicated a false accusation and must stay silent: \
         {final_message:?} → {:?}",
        digest.lines
    );
}

/* ── M2: proof in scanned rows the recognizer cannot read ────────────── */

#[test]
fn g1_unrecognized_typecheck_output_blocks_the_accusation() {
    // Adjudicated shape: `npx tsc` + a web export printing "exit=0", claimed
    // as "typecheck clean, build green", accused while the proof sat four
    // rows above the claim.
    assert_silent(
        "Typecheck clean, production build green.",
        &[row(
            1,
            "Bash",
            json!({"command": "npx tsc --noEmit && npx expo export -p web"}),
            "typecheck exit=0\nExported: dist",
        )],
    );
}

#[test]
fn g1_bespoke_harness_totals_block_the_accusation() {
    // Adjudicated shape: a project harness printing "TOTAL N checks — N
    // pass" that the outcome reader does not parse.
    assert_silent(
        "48/48 pass, 0 fail, 0 skip — the full flow harness.",
        &[row(
            1,
            "Bash",
            json!({"command": "node scripts/flow-harness.mjs"}),
            "TOTAL 48 checks — 48 pass, 0 fail, 0 skip",
        )],
    );
}

#[test]
fn g1_selftest_pass_output_blocks_the_accusation() {
    assert_silent(
        "Gate tests green against the live stack.",
        &[row(
            1,
            "Bash",
            json!({"command": "python3 ops/selftest.py"}),
            "preflight: stack healthy\nselftest: PASS",
        )],
    );
}

#[test]
fn g1_deno_check_output_blocks_the_accusation() {
    assert_silent(
        "Both type checks pass.",
        &[row(
            1,
            "Bash",
            json!({"command": "deno check apps/orchestrator/main.ts"}),
            "Check file:///apps/orchestrator/main.ts",
        )],
    );
}

#[test]
fn g1_deploy_toolchain_output_blocks_the_accusation() {
    assert_silent(
        "Build green and shipped to production.",
        &[row(
            1,
            "Bash",
            json!({"command": "vercel deploy --prod"}),
            "Production: ready in 4s",
        )],
    );
}

#[test]
fn g1_push_hook_output_blocks_the_accusation() {
    // Adjudicated shape: the pre-push hook's own output WAS the gate proof.
    assert_silent(
        "The fast gate passed on push.",
        &[row(
            1,
            "Bash",
            json!({"command": "git push origin trunk"}),
            "pre-push: fast gate passed in 98s",
        )],
    );
}

#[test]
fn g1_browser_driver_rows_block_the_accusation() {
    // Adjudicated shape: dashboard work done through a browser tool; the
    // old binder demanded a file edit and accused.
    assert_silent(
        "I've updated the checkout branding in `branding.css`.",
        &[row(
            1,
            "mcp__browser__computer",
            json!({"action": "left_click"}),
            "clicked",
        )],
    );
}

#[test]
fn g1_agent_rows_block_the_accusation() {
    // Adjudicated shape: tests run inside a delegated workflow whose report
    // arrives as an agent response.
    assert_silent(
        "All 96 suite tests pass.",
        &[row(
            1,
            "Task",
            json!({"prompt": "run the suites"}),
            "workflow completed",
        )],
    );
}

#[test]
fn g1_an_illegible_recognized_run_blocks_the_accusation() {
    // A recognized invocation whose output the reader cannot classify could
    // have been the success itself.
    assert_silent(
        "All 12 tests pass.",
        &[row(
            1,
            "Bash",
            json!({"command": "cargo test"}),
            "…output elided by the harness…",
        )],
    );
}

#[test]
fn g1_the_reason_for_silence_is_on_record() {
    let temp = tempdir().expect("tempdir");
    let turn = turn_for(temp.path(), "All 12 tests pass.");
    let rows = [row(1, "Task", json!({"prompt": "run"}), "done")];
    let idx = complete_index(&rows);
    let (verdict, reason) = bind_explained(&turn.claims[0], &idx);
    assert_eq!(verdict, Binding::Unknown);
    assert_eq!(
        reason.as_deref(),
        Some(
            "1 tool event in the scanned window outside the recognized evidence vocabulary — \
             absence cannot be established"
        )
    );
    let digest = digest_for(&turn, &idx);
    assert_eq!(digest.unaccounted_rows, 1);
    assert_eq!(digest.abstentions.len(), 1);
    assert!(digest.abstentions[0].starts_with("line 2: "));
}

/* ── M3: shapes that must die at extraction ──────────────────────────── */

#[test]
fn g1_a_citation_footer_is_not_a_claim() {
    assert_silent(
        "Sources: [Harbor Council brief](https://example.invalid/brief) and \
         [Quarry payroll guide](https://example.invalid/guide).",
        &[],
    );
    assert_eq!(
        admit_completion(
            "Sources: [Harbor Council brief](https://example.invalid/brief).",
            &[]
        ),
        GateVerdict::Refused(CITATION_REASON)
    );
}

#[test]
fn g1_relayed_prior_state_is_not_a_claim() {
    // Adjudicated shape: a memory-file's account of prior-session work,
    // restated without a first-person actor, bound as a completion claim.
    assert_silent(
        "The batching twin (`orders_batch_api`) is built and shelved for when a key lands.",
        &[],
    );
}

#[test]
fn g1_a_stale_index_report_is_not_a_claim() {
    assert_silent(
        "Two notes on stale index lines: the index still says CP9 is UNCOMMITTED \
         and the fix is UNMERGED.",
        &[],
    );
    assert_eq!(
        admit_completion("the index still says CP9 is UNCOMMITTED", &[]),
        GateVerdict::Refused(DEBT_REASON)
    );
}

#[test]
fn g1_a_deferred_debt_report_is_not_a_claim() {
    assert_silent(
        "Also the deferred copy pass — `ZONE_NOTICE` still contains the banned word.",
        &[],
    );
}

#[test]
fn g1_an_admission_of_error_is_not_a_claim() {
    // Adjudicated shape: the assistant ACCEPTING a criticism, read by the
    // old extractor as a 40/40 test claim.
    assert_silent(
        "Taking the reporting criticism: dropping the room checks and reporting \
         40/40 concealed a real gap.",
        &[],
    );
}

#[test]
fn g1_a_recon_finding_about_wrongness_is_not_a_claim() {
    assert_silent(
        "A \"find any parseable index\" heuristic scores 6/6 and is confidently wrong in 2.",
        &[],
    );
}

#[test]
fn g1_a_process_narrative_is_not_a_claim() {
    assert_silent(
        "My preview pane kept wedging on scroll, so I verified the section order another way.",
        &[],
    );
}

#[test]
fn g1_a_state_description_of_styling_is_not_a_claim() {
    assert_silent(
        "Everywhere in /app: Display Serif plus Mono kickers, via the font loader.",
        &[],
    );
}

#[test]
fn g1_a_passive_new_file_description_is_not_a_claim() {
    assert_silent(
        "The hero panel is templated from computed facts in the new `lib/day-panel.ts`.",
        &[],
    );
}

/* ── M4: claims whose evidence has no deterministic class ────────────── */

#[test]
fn g1_a_read_claim_has_no_class_and_abstains_with_the_reason() {
    let temp = tempdir().expect("tempdir");
    let turn = turn_for(temp.path(), "I've read `docs/loop/PLAN-LOG.md`.");
    // The proof really is in the record — a Read row — and the old binder
    // still accused because it demanded an edit. Now: no class, silence.
    let rows = [row(
        1,
        "Read",
        json!({"file_path": "/repo/docs/loop/PLAN-LOG.md"}),
        "…file content…",
    )];
    let idx = complete_index(&rows);
    let (verdict, reason) = bind_explained(&turn.claims[0], &idx);
    assert_eq!(verdict, Binding::Unknown);
    assert_eq!(reason.as_deref(), Some(NO_CLASS_REASON));
    assert!(digest_for(&turn, &idx).lines.is_empty());
}

#[test]
fn g1_a_deploy_claim_has_no_class() {
    assert_silent("Deployed to example-app.com.", &[]);
    assert_eq!(
        admit_completion(
            "Deployed to example-app.com.",
            &["example-app.com".to_string()]
        ),
        GateVerdict::Refused(NO_CLASS_REASON)
    );
}

#[test]
fn g1_a_quoted_command_exit_code_has_no_class() {
    assert_silent("`git diff --check aaa1111 bbb2222` → exit 0.", &[]);
}

#[test]
fn g1_an_agent_workflow_report_has_no_class() {
    assert_silent("Workflow finished — 13 agents, no errors.", &[]);
}

/* ── promise shapes ──────────────────────────────────────────────────── */

#[test]
fn g1_a_now_state_description_is_not_an_open_promise() {
    assert_silent(
        "Now `reader-scope.ts` is scoped per guide — each reader sees only its own rows.",
        &[],
    );
}

#[test]
fn g1_a_past_tense_event_report_is_not_an_open_promise() {
    assert_silent(
        "Pushing this checkpoint ran the hook for real: the fast gate passed, \
         then the push went through.",
        &[],
    );
}

#[test]
fn g1_an_imperative_to_the_user_is_not_an_open_promise() {
    // The gauntlet's one AMBIGUOUS line: advice to the user, not a
    // commitment by the speaker. Structurally refused whatever marker the
    // extractor hands it.
    let text = "Test it live with the hardest questions your users actually ask — \
                that's the real evaluation.";
    assert_silent(text, &[]);
    for marker in [
        Marker::Imperative,
        Marker::Next,
        Marker::Gerund,
        Marker::Intent,
    ] {
        assert!(!admissible_promise(text, marker));
    }
}

#[test]
fn g1_a_platform_gate_description_is_not_an_open_promise() {
    assert_silent(
        "Now `platform.flag !== \"handset\"`: inline on tablet and web, \
         button-triggered on handset.",
        &[],
    );
}

/* ── the true shapes: these must STILL emit ──────────────────────────── */

#[test]
fn g1_a_dangling_conditional_abort_promise_still_emits() {
    // Adjudicated TRUE: a first-person conditional promise left dangling on
    // an auth step the user had to take — exactly what the next session
    // must hear about.
    let temp = tempdir().expect("tempdir");
    let turn = turn_for(
        temp.path(),
        "I'll abort if `is_private` is false and tell you, rather than push to a public repo.",
    );
    let digest = digest_for(&turn, &complete_index(&[]));
    assert_eq!(digest.lines.len(), 1, "{:?}", digest.lines);
    assert_eq!(digest.lines[0].kind, "open_promise");
}

#[test]
fn g1_a_dangling_preview_before_send_promise_still_emits() {
    // Adjudicated TRUE: a promise to show the user something before any
    // send, dangling on user input at session end.
    let temp = tempdir().expect("tempdir");
    let turn = turn_for(
        temp.path(),
        "I'll assemble a truthful inquiry and show you the final sender, recipient, \
         subject, and body before anything sends.",
    );
    let digest = digest_for(&turn, &complete_index(&[]));
    assert_eq!(digest.lines.len(), 1, "{:?}", digest.lines);
    assert_eq!(digest.lines[0].kind, "open_promise");
}

/* ── the mechanism stays alive ───────────────────────────────────────── */

#[test]
fn g1_a_fully_accounted_empty_window_still_accuses() {
    // The gate must quiet the false shapes without going dark: a test claim
    // over a window of purely read-only rows, complete and legible, is
    // still a no-proof line.
    let temp = tempdir().expect("tempdir");
    let turn = turn_for(temp.path(), "All 12 tests pass.");
    let rows = [
        row(1, "Bash", json!({"command": "ls"}), ""),
        row(
            2,
            "Bash",
            json!({"command": "git status && git log -3"}),
            "clean",
        ),
        row(3, "Read", json!({"file_path": "/repo/src/lib.rs"}), "…"),
    ];
    let idx = complete_index(&rows);
    assert_eq!(idx.unaccounted_rows, 0);
    let digest = digest_for(&turn, &idx);
    assert_eq!(digest.lines.len(), 1, "{:?}", digest.lines);
    assert_eq!(digest.lines[0].kind, "no_proof");
}

#[test]
fn g1_evidence_still_wins_over_unaccounted_rows() {
    // Unreadable rows can only hide proof, never unmake it: a recognized
    // passing run keeps the claim evidenced whatever else is in the window.
    let temp = tempdir().expect("tempdir");
    let turn = turn_for(temp.path(), "All 12 tests pass.");
    let rows = [
        row(
            1,
            "Bash",
            json!({"command": "cargo test"}),
            "test result: ok. 12 passed; 0 failed",
        ),
        row(2, "Task", json!({"prompt": "tidy"}), "done"),
    ];
    let idx = complete_index(&rows);
    assert!(idx.unaccounted_rows > 0);
    let (verdict, _) = bind_explained(&turn.claims[0], &idx);
    assert_eq!(verdict, Binding::Evidenced);
}
