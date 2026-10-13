//! A33: the crown-jewel catch is arithmetic, and this proves it.
//!
//! The strongest available assertion is not "the contradiction fires" — the NLI
//! path could produce that by luck. It is that the contradiction **survives a
//! scorer that never contradicts anything**. If the verdict still lands, no
//! part of it came from the model.
//!
//! Runs in the default suite: the stub scorer needs no model download.

use std::fs;
use std::path::PathBuf;

use thruline_core::annotate::annotate;
use thruline_core::config::Config;
use thruline_core::derive::{derive_project_with, NodeState};
use thruline_core::ingest::ingest;
use thruline_core::ledger::Ledger;
use thruline_core::nli::{EntailmentScorer, Scores};
use thruline_core::Result;

const PROJ: &str = "-Users-dev-contract";
const S: &str = "cccccccc-1111-4222-8333-444444444444";

/// A scorer that is incapable of alleging a conflict. Entailment mid-band,
/// contradiction pinned to zero. Anything this test still catches was caught
/// by arithmetic.
struct NeverContradicts;

impl EntailmentScorer for NeverContradicts {
    fn score(&mut self, _premise: &str, _hypothesis: &str) -> Result<Scores> {
        Ok(Scores {
            entailment: 0.30,
            neutral: 0.70,
            contradiction: 0.0,
        })
    }
}

fn u(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012}")
}

/// One turn: a command, its recorded result, then a claim about it.
fn transcript(command: &str, result: &str, claim: &str) -> String {
    let mut l: Vec<String> = Vec::new();
    l.push(format!(
        r#"{{"type":"user","uuid":"{}","parentUuid":null,"isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-29T00:00:00.000Z","message":{{"role":"user","content":"run it"}}}}"#,
        u(1)
    ));
    l.push(format!(
        r#"{{"type":"assistant","uuid":"{}","parentUuid":"{}","isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-29T00:00:01.000Z","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"t1","name":"Bash","input":{{"command":"{command}"}}}}]}}}}"#,
        u(2),
        u(1)
    ));
    l.push(format!(
        r#"{{"type":"user","uuid":"{}","parentUuid":"{}","isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-29T00:00:02.000Z","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"t1","content":"{result}"}}]}}}}"#,
        u(3),
        u(2)
    ));
    l.push(format!(
        r#"{{"type":"assistant","uuid":"{}","parentUuid":"{}","isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-29T00:00:03.000Z","message":{{"role":"assistant","content":[{{"type":"text","text":"{claim}"}}]}}}}"#,
        u(4),
        u(3)
    ));
    l.join("\n") + "\n"
}

fn verdict_for(command: &str, result: &str, claim: &str) -> (NodeState, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("projects");
    let project = root.join(PROJ);
    fs::create_dir_all(&project).expect("mkdir");
    fs::write(
        project.join(format!("{S}.jsonl")),
        transcript(command, result, claim),
    )
    .expect("write");
    let mut ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    ingest(
        &mut ledger,
        std::slice::from_ref(&PathBuf::from(&root)),
        &dir.path().join("mirrors"),
    )
    .expect("ingest");

    let cfg = Config::default();
    annotate(&ledger, PROJ, &cfg, &mut NeverContradicts, "never-v1").expect("annotate");
    let g = derive_project_with(&ledger, PROJ, &cfg).expect("derive");

    let n = g
        .nodes
        .iter()
        .find(|n| !n.verdicts.is_empty())
        .expect("a node carrying claims");
    let v = n
        .verdicts
        .iter()
        .max_by_key(|v| v.state.as_str().to_string())
        .expect("a verdict");
    (v.state, v.reason.clone())
}

#[test]
fn the_misstated_result_catch_survives_a_scorer_that_never_contradicts() {
    let (state, reason) = verdict_for(
        "cargo test --all",
        "running 33 tests\\ntest result: FAILED. 31 passed; 2 failed; 0 ignored",
        "33/33 tests pass.",
    );
    assert_eq!(
        state,
        NodeState::Contradicted,
        "the crown jewel must be arithmetic — it was not caught: {reason}"
    );
    assert!(
        reason.contains("Checked against the latest identity-matched prior run"),
        "caught, but not by the contract: {reason}"
    );
    assert!(
        reason.contains("31") && reason.contains('2'),
        "the receipt must show BOTH numbers: {reason}"
    );
}

#[test]
fn a_failed_build_contradicts_a_success_claim_without_the_model() {
    let (state, reason) = verdict_for(
        "cargo build --release",
        "   Compiling thruline-core v0.1.0\\nerror: could not compile `thruline-core`",
        "Build succeeded, clippy clean.",
    );
    assert_eq!(state, NodeState::Contradicted, "{reason}");
    assert!(
        reason.contains("Checked against the latest identity-matched prior run"),
        "{reason}"
    );
}

#[test]
fn a_truthful_marker_is_supported_without_the_model() {
    // The same deterministic path must confirm as readily as it accuses,
    // otherwise the contract is just a differently-shaped accusation machine.
    let (state, reason) = verdict_for(
        "cargo test --all",
        "running 12 tests\\ntest result: ok. 12 passed; 0 failed",
        "All 12 tests pass.",
    );
    assert_eq!(state, NodeState::Supported, "{reason}");
    assert!(
        reason.contains("Checked against the latest identity-matched prior run"),
        "{reason}"
    );
}

#[test]
fn an_aggregate_count_is_never_accused() {
    // The measured M3.8 false alarm: a total that spans several runs, matched
    // against one of them. Arithmetic must decline, not accuse.
    let (state, reason) = verdict_for(
        "swift test",
        "Executed 43 tests, with 0 failures",
        "306 tests green (303 + 3 new).",
    );
    assert_ne!(
        state,
        NodeState::Contradicted,
        "an aggregate count was accused on a partial run: {reason}"
    );
}

/* ── M3.10.3: §5d — the model does not convict ─────────────────────────── */

#[test]
fn case_001_the_measured_false_accusation_now_reads_supported() {
    // The exact shape M3.9.4 adjudicated as a false accusation, and the reason
    // A36 exists. Claim: "98/98 passing". Evidence: a run whose TAP summary
    // says `# pass 98 / # fail 0` — it PROVES the claim. NLI scored that pair
    // at contradiction 0.87 and the product accused the user.
    //
    // Under §5d it is arithmetic, and it comes out the only way it can.
    let (state, reason) = verdict_for(
        "npm test",
        "1..98\\n# tests 98\\n# suites 0\\n# pass 98\\n# fail 0\\n# skipped 0\\n# todo 0",
        "**98/98 passing** (was 94, added 4 new tests).",
    );
    assert_eq!(
        state,
        NodeState::Supported,
        "case-001 must flip to supported; still {state:?}: {reason}"
    );
    assert!(
        reason.contains("Checked against the latest identity-matched prior run"),
        "case-001 was decided by something other than the contract: {reason}"
    );
}

#[test]
fn node_tap_failures_still_convict() {
    // The counterweight: the same format, a real misstatement. If §5d only
    // ever exonerated, it would be a mute button rather than a truth layer.
    let (state, reason) = verdict_for(
        "npm test",
        "1..98\\n# tests 98\\n# pass 94\\n# fail 4\\n# skipped 0",
        "98/98 passing, all green.",
    );
    assert_eq!(
        state,
        NodeState::Contradicted,
        "a real misstatement in node TAP was not caught: {reason}"
    );
    assert!(
        reason.contains("Checked against the latest identity-matched prior run"),
        "{reason}"
    );
}

#[test]
fn an_uninterpretable_outcome_is_unknown_never_an_accusation() {
    // §5d's abstention clause. The evidence is on-subject and readable as
    // prose, but its result is not a format we can interpret — so there is no
    // arithmetic to do, and the honest verdict says exactly that.
    let (state, reason) = verdict_for(
        "./scripts/harness.sh --suite rooms",
        "harness finished; see artifacts/rooms-report.html for details",
        "104/104 rooms tests pass.",
    );
    assert!(
        !state.is_accusation(),
        "accused on evidence whose outcome we cannot read: {state:?} — {reason}"
    );
}

#[test]
fn unreadable_best_evidence_blocks_conviction_on_worse_evidence() {
    // The last false-warning class of round 3, reproduced end to end. The
    // turn contains TWO runs: first an old failing one, then the two-suite
    // payload the claim is actually about — which §5e's conflict rule
    // abstains on (342 vs 36 is two suites, not a disagreement). The claim's
    // best evidence is therefore uninterpretable, and convicting on the
    // worse-matched failing run above it is the A32 defect inside the
    // contract. The only honest verdict is `unknown`, with the §5d receipt.
    let (state, reason) = verdict_for(
        "./scripts/full-gate.sh",
        "\\u2718 Test run with 341 tests in 36 suites failed after 13.6 seconds with 1 issue.\\n\\u2714 Test run with 342 tests in 36 suites passed after 14.3 seconds.\\n\\u2714 Test run with 36 tests in 6 suites passed after 0.2 seconds.",
        "MarketKit 342, MarketCore 36 tests green.",
    );
    assert!(
        !state.is_accusation(),
        "convicted past uninterpretable best evidence: {state:?} — {reason}"
    );
    assert_eq!(
        reason, "Evidence present; outcome format not interpretable.",
        "the \u{a7}5d receipt must say exactly why we abstained: {reason}"
    );
}
