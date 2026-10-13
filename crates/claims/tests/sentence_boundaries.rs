//! The sentence splitter's token and markup rules.
//!
//! Each string here is taken verbatim from `thruline claims` output on a real
//! ledger, where the splitter broke on `.` inside filenames and code spans and
//! orphaned closing markup onto the following piece. Measured before the guard
//! was widened: 20.0% of emitted promise strings were malformed. The promise
//! text is a first-level string — the thing a person reads first — so a
//! fragment starting mid-word is not cosmetic, it is the product mis-quoting
//! the agent it is reporting on.

use thruline_claims::rules::sentences;

fn pieces(text: &str) -> Vec<&str> {
    sentences(text).into_iter().map(|(_, s)| s).collect()
}

#[test]
fn a_dot_inside_a_token_does_not_end_a_sentence() {
    // Reported before as the fragment: "md to stay in the core checkout — say
    // so and I'll move it back;"
    let got = pieces("I want FINDINGS.md to stay in the core checkout.");
    assert_eq!(got.len(), 1, "one sentence, not two: {got:?}");
    assert!(got[0].contains("FINDINGS.md"), "{got:?}");
}

#[test]
fn the_version_case_the_old_guard_covered_still_holds() {
    assert_eq!(pieces("Bumped to 1.2.3 today.").len(), 1);
}

#[test]
fn real_terminal_punctuation_still_splits() {
    let got = pieces("First thing. Second thing. Third thing.");
    assert_eq!(got.len(), 3, "{got:?}");
}

#[test]
fn closing_markup_belongs_to_the_sentence_it_closes() {
    // Reported before as: "** Its one remaining open ignore, a C# deletion..."
    let got = pieces("**That is done.** Its one remaining ignore is a decoy.");
    assert_eq!(got.len(), 2, "{got:?}");
    assert!(
        got[0].trim_end().ends_with("**"),
        "the closing ** stays with the sentence it closes: {got:?}"
    );
    assert!(
        !got[1].trim_start().starts_with("**"),
        "and is not orphaned onto the next: {got:?}"
    );
}

#[test]
fn a_dot_inside_a_code_span_does_not_truncate_it() {
    // Reported before as: "The remaining `thruline."
    let got = pieces("The remaining `thruline.perf` call is a Phase 2 deliverable.");
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(
        got[0].matches('`').count() % 2,
        0,
        "backticks stay balanced: {got:?}"
    );
}

#[test]
fn an_opening_bold_on_the_next_line_is_not_swallowed() {
    // The guard only attaches a markup run after REAL terminal punctuation and
    // only when whitespace follows, so a newline break cannot eat the `**`
    // that opens the next line.
    let got = pieces("Done here.\n**Next section** follows.");
    assert!(got.len() >= 2, "{got:?}");
    assert!(
        got[1].trim_start().starts_with("**"),
        "the opening ** stays with the line it opens: {got:?}"
    );
}
