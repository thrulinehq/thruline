//! The flight recorder's model — every decision it makes, without a terminal.

use super::*;

fn row(seq: u64, kind: &str, text: &str) -> Row {
    Row {
        seq,
        kind: kind.into(),
        text: text.into(),
        detail: vec![("kind".into(), kind.into())],
        risk: None,
    }
}

fn flight() -> Flight {
    Flight {
        session: "s".into(),
        rows: vec![
            row(0, "session_start", "session began"),
            row(4, "user_prompt", "add a handling fee"),
            row(8, "exec", "pytest -q → exit 0"),
            row(12, "fs_change", "shipping.py modified"),
            row(16, "stop", "All tests pass."),
        ],
        claims: vec![ClaimRow {
            kind: "tests_pass".into(),
            text: "All tests pass.".into(),
            verdict: "unsupported".into(),
            reasons: vec!["ran on the final tree".into()],
            findings: vec!["narrowed_run".into()],
        }],
        promises: vec![PromiseRow {
            text: "I'll add integration tests in a follow-up".into(),
            status: "open".into(),
        }],
        stop: StopSummary {
            reason: "agent_finished".into(),
            detail: "the agent finished normally".into(),
            seq: Some(16),
        },
        ..Flight::default()
    }
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

#[test]
fn an_empty_query_shows_everything_and_not_nothing() {
    let f = flight();
    assert_eq!(f.visible().len(), f.rows.len());
    assert_eq!(f.current().map(|r| r.seq), Some(0));
}

#[test]
fn a_query_filters_on_the_kind_and_the_text_case_insensitively() {
    let mut f = flight();
    f.set_query("PYTEST".into());
    assert_eq!(f.visible(), vec![2]);
    f.set_query("fs_change".into());
    assert_eq!(f.visible(), vec![3]);
    f.set_query("nothing matches this".into());
    assert!(f.visible().is_empty());
    // ...and a cursor over an empty list does not index into nothing.
    assert_eq!(f.current(), None);
    f.move_by(5);
    assert_eq!(f.current(), None);
}

#[test]
fn changing_the_query_puts_the_cursor_at_the_top_of_the_new_list() {
    let mut f = flight();
    f.move_by(3);
    assert_eq!(f.current().map(|r| r.seq), Some(12));
    // The index is into the FILTERED list; keeping it across a filter change
    // would land the cursor on an unrelated row.
    f.set_query("stop".into());
    assert_eq!(f.selected(), 0);
    assert_eq!(f.current().map(|r| r.seq), Some(16));
}

// ---------------------------------------------------------------------------
// The cursor
// ---------------------------------------------------------------------------

#[test]
fn the_cursor_is_clamped_rather_than_wrapped() {
    let mut f = flight();
    f.move_by(-1);
    assert_eq!(f.current().map(|r| r.seq), Some(0), "no wrap to the end");
    f.move_by(100);
    assert_eq!(f.current().map(|r| r.seq), Some(16), "clamped at the end");
    f.move_by(1);
    assert_eq!(f.current().map(|r| r.seq), Some(16), "and stays there");
}

// ---------------------------------------------------------------------------
// Jump-to-stop (§10)
// ---------------------------------------------------------------------------

#[test]
fn jumping_to_the_stop_clears_the_filter_that_would_hide_it() {
    let mut f = flight();
    f.set_query("pytest".into());
    assert_eq!(f.visible(), vec![2], "the stop is filtered out");
    assert!(f.jump_to_stop());
    assert!(
        f.query.is_empty(),
        "a jump into a hidden row reads as a no-op"
    );
    assert_eq!(f.current().map(|r| r.seq), Some(16));
}

#[test]
fn a_session_with_no_stop_event_reports_that_rather_than_jumping_somewhere() {
    let mut f = flight();
    f.stop.seq = None;
    assert!(!f.jump_to_stop());
    assert_eq!(
        f.current().map(|r| r.seq),
        Some(0),
        "the cursor did not move"
    );

    // ...and a stop whose seq names no row is also a refusal, not a guess at
    // the nearest one.
    let mut f = flight();
    f.stop.seq = Some(999);
    assert!(!f.jump_to_stop());
}

// ---------------------------------------------------------------------------
// The header, in the product's own vocabulary
// ---------------------------------------------------------------------------

#[test]
fn the_headline_never_says_lie_and_never_accuses_on_a_number_alone() {
    let f = flight();
    let line = f.headline();
    assert!(
        line.contains("claimed, and no proof found in the record"),
        "{line}"
    );
    for forbidden in ["lie", "lied", "lying", "dishonest", "cheat"] {
        assert!(
            !line.to_lowercase().contains(forbidden),
            "the vocabulary is binding: {line}"
        );
    }
}

#[test]
fn an_unverifiable_claim_is_not_reported_as_an_unsupported_one() {
    let mut f = flight();
    f.claims[0].verdict = "unverifiable".into();
    let line = f.headline();
    assert!(line.contains("could not be checked"), "{line}");
    assert!(
        !line.contains("no proof found"),
        "§6: `unverifiable` must never read as an accusation: {line}"
    );
}

#[test]
fn a_session_with_no_claims_says_so_rather_than_reporting_all_clear() {
    let mut f = flight();
    f.claims.clear();
    assert_eq!(
        f.headline(),
        "no completion claim was extracted from this session"
    );
    assert!(!f.headline().contains("supported"));
}

#[test]
fn an_honest_session_is_reported_as_one() {
    let mut f = flight();
    f.claims[0].verdict = "supported".into();
    assert_eq!(f.headline(), "1 claim(s), all supported by the record");
}

#[test]
fn the_verdict_counts_are_a_tally_and_not_a_judgement() {
    let mut f = flight();
    f.claims.push(ClaimRow {
        kind: "build_ok".into(),
        text: "it builds".into(),
        verdict: "unsupported".into(),
        reasons: vec![],
        findings: vec![],
    });
    assert_eq!(f.verdict_counts(), vec![("unsupported".to_string(), 2)]);
}

// ---------------------------------------------------------------------------
// Panes
// ---------------------------------------------------------------------------

#[test]
fn the_panes_cycle_and_every_one_has_a_name() {
    let mut pane = Pane::Timeline;
    let mut seen = Vec::new();
    for _ in 0..3 {
        seen.push(pane.title());
        pane = pane.next();
    }
    assert_eq!(seen, vec!["timeline", "claims", "promises"]);
    assert_eq!(pane, Pane::Timeline, "the cycle closes");
}

/// The model carries verdicts; it does not compute them.
///
/// A census, because the failure it guards is a TUI whose claims pane disagrees
/// with `thruline claims` — the product contradicting its own evidence — and
/// that happens the moment this crate can reach a verifier.
#[test]
fn this_crate_cannot_reach_a_verifier() {
    // **Comments stripped first.** The manifest explains why these are absent,
    // so a search over the raw text finds the explanation and calls it a
    // dependency — a guard that reads prose where it means to read a
    // declaration. The same lesson as the keyless document guard, one file
    // along: assert the relationship, not the co-occurrence.
    let manifest: String = include_str!("../Cargo.toml")
        .lines()
        .map(|l| l.split('#').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in ["thruline-verify", "thruline-claims", "thruline-ledger"] {
        assert!(
            !manifest.contains(forbidden),
            "the flight recorder must be handed its verdicts, not compute them: \
             {forbidden} is a dependency"
        );
    }
    // The control: the strip did not remove everything, which would make the
    // assertions above vacuous.
    assert!(
        manifest.contains("ratatui"),
        "the comment strip ate the dependency table"
    );
    let source = include_str!("lib.rs");
    assert!(
        !source.contains("Verdict::") && !source.contains("fn verify"),
        "a verdict is decided in crates/verify and nowhere else"
    );
}
