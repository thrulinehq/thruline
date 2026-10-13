//! §9's PR comment: the table a reviewer reads, for people who have never
//! installed ThruLine.
//!
//! # The line glyphs, and why every one of them carries a word
//!
//! UX-DOCTRINE: *"Every appearance carries glyph + word beside it, so the state
//! survives grayscale and a screen reader alike."* This surface is the one the
//! doctrine was written for — §15 calls the PR comment and the badge "the viral
//! surface", which is another way of saying it is the ThruLine most people will
//! ever see, in a medium that strips colour and is read aloud.
//!
//! So the three line glyphs are fixed, and each is followed by its word in the
//! same cell:
//!
//! | glyph | word | means |
//! |---|---|---|
//! | ✅ | `supported` / `ok` | the record proves it |
//! | ◽ | `unproved` / `skipped` | nothing contradicts it and nothing proves it |
//! | ❌ | `unsupported` / `FAIL` | the record contradicts it |
//!
//! `◽` rather than a yellow circle on purpose: the neutral state must not read
//! as a dimmer green, and an outlined glyph survives the greyscale test that a
//! coloured one fails.
//!
//! # Every cell that could carry attacker text is inert
//!
//! A claim's text is written by the agent, a check's detail can quote a path,
//! and both land in a markdown comment on a public pull request. [`inert`]
//! fences them so no cell can close the table, start a heading, or open a link.

use thruline_policy::{Conclusion, Decision};

use crate::receipt_source::Source;
use crate::ruling::Ruling;
use crate::verify_json::VerifyJson;

/// The hidden marker that lets the App find and edit its own comment instead of
/// adding one per push. A reviewer sees nothing; the App sees exactly one.
pub const MARKER: &str = "<!-- thruline-app-comment -->";

/// Glyph and word for a claim verdict.
pub fn verdict_glyph(verdict: &str) -> (&'static str, &'static str) {
    match verdict {
        "supported" => ("✅", "supported"),
        "unsupported" => ("❌", "unsupported"),
        "unverifiable" => ("◽", "unverifiable"),
        _ => ("◽", "unrecognised"),
    }
}

/// Glyph and word for an integrity check outcome.
pub fn check_glyph(outcome: &str) -> (&'static str, &'static str) {
    match outcome {
        "pass" => ("✅", "ok"),
        "fail" => ("❌", "FAIL"),
        "skipped" => ("◽", "skipped"),
        _ => ("◽", "not stated"),
    }
}

/// Glyph and word for a conclusion.
pub fn conclusion_glyph(c: Conclusion) -> (&'static str, &'static str) {
    match c {
        Conclusion::Success => ("✅", "success"),
        Conclusion::Neutral => ("◽", "neutral"),
        Conclusion::Failure => ("❌", "failure"),
    }
}

/// Makes a cell inert: no pipe can close the table, no backtick can escape the
/// code span, and control characters cannot smuggle a newline into a row.
///
/// Truncated at 200 characters because a check detail is a sentence and a
/// 40 KB one is an attempt to make the comment unreadable.
pub fn inert(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    out.push('`');
    for (i, c) in s.chars().enumerate() {
        if i >= 200 {
            out.push('…');
            break;
        }
        match c {
            '`' => out.push('\''),
            '|' => out.push('\u{2502}'),
            '\n' | '\r' | '\t' => out.push(' '),
            c if (c as u32) < 0x20 => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('`');
    out
}

/// The whole comment.
///
/// `receipt` is `None` when none was found — the comment still renders, because
/// "no receipt on this pull request" is a finding and a silent App is one a
/// repository stops noticing has broken.
pub fn render(
    ruling: &Ruling,
    source: Option<Source>,
    receipt: Option<&VerifyJson>,
    policy: Option<&Decision>,
) -> String {
    let (glyph, word) = conclusion_glyph(ruling.conclusion);
    let mut out = String::new();
    out.push_str(MARKER);
    out.push('\n');
    out.push_str(&format!(
        "### {glyph} ThruLine — {word}\n\n**{}**\n\n",
        ruling.ground.title()
    ));

    match source {
        Some(s) => {
            out.push_str(&format!("Receipt found via **{}**.\n", s.as_str()));
            if let Some(c) = s.caveat() {
                out.push_str(&format!("> {c}\n"));
            }
        }
        None => {
            out.push_str(
                "No receipt was found on this pull request. ThruLine looked in the \
                 artifact attestations, in `.thruline/receipts/` on the head commit, \
                 and in the comments on this thread.\n",
            );
        }
    }
    out.push('\n');

    if let Some(r) = receipt {
        // The verdict table. **The clause "neutral with the verdict table" is
        // this block** — a neutral conclusion with no table is "ThruLine looked
        // and won't say", which is the second-worst thing this surface can do.
        out.push_str("| | claim | verdict |\n|---|---|---|\n");
        if r.claims.total == 0 {
            out.push_str("| ◽ | *(this receipt makes no claim)* | none |\n");
        }
        for id in &r.claims.unsupported_claim_ids {
            let (g, w) = verdict_glyph("unsupported");
            out.push_str(&format!("| {g} | {} | {w} |\n", inert(id)));
        }
        for id in &r.claims.unverifiable_claim_ids {
            let (g, w) = verdict_glyph("unverifiable");
            out.push_str(&format!("| {g} | {} | {w} |\n", inert(id)));
        }
        // **A count without ids still gets a row.** The verdict turns on the
        // COUNTS — `anything_unproved` is three `> 0` comparisons — while this
        // table is built from the ID LISTS. When the verifier reports
        // `unsupported: 2` and itemises none, every unproved claim drops out of
        // the table and the only row left is the green one: one ✅ under a
        // `neutral` conclusion, which reads as "mostly fine".
        //
        // The summary line below still says "1 of 3 claim(s) supported", so the
        // fact is not lost — but the table is what gets scanned, and the block
        // above exists because "a neutral conclusion with no table is the
        // second-worst thing this surface can do". A table that shows only the
        // good news is not better than no table.
        //
        // Nothing here changes a verdict. It stops the table disagreeing with
        // the sentence under it.
        let unnamed = |n: u64, listed: usize| n.saturating_sub(listed as u64);
        let unnamed_unsupported =
            unnamed(r.claims.unsupported, r.claims.unsupported_claim_ids.len());
        if unnamed_unsupported > 0 {
            let (g, w) = verdict_glyph("unsupported");
            out.push_str(&format!(
                "| {g} | *{unnamed_unsupported} claim(s), not itemised by the verifier* | {w} |\n"
            ));
        }
        let unnamed_unverifiable =
            unnamed(r.claims.unverifiable, r.claims.unverifiable_claim_ids.len());
        if unnamed_unverifiable > 0 {
            let (g, w) = verdict_glyph("unverifiable");
            out.push_str(&format!(
                "| {g} | *{unnamed_unverifiable} claim(s), not itemised by the verifier* | {w} |\n"
            ));
        }
        if r.claims.supported > 0 {
            let (g, w) = verdict_glyph("supported");
            out.push_str(&format!(
                "| {g} | *{} other claim(s)* | {w} |\n",
                r.claims.supported
            ));
        }
        if r.claims.other > 0 {
            out.push_str(&format!(
                "| ◽ | *{} verdict(s) this build does not recognise* | unrecognised |\n",
                r.claims.other
            ));
        }
        out.push('\n');

        out.push_str(&format!(
            "Receipt integrity: **{}**. {} of {} claim(s) supported.\n\n",
            r.integrity, r.claims.supported, r.claims.total
        ));

        if !r.checks.is_empty() {
            out.push_str(
                "<details><summary>What was checked against this repository</summary>\n\n",
            );
            out.push_str("| | check | detail |\n|---|---|---|\n");
            for c in &r.checks {
                let (g, w) = check_glyph(&c.outcome);
                out.push_str(&format!(
                    "| {g} | {} {w} | {} |\n",
                    inert(&c.name),
                    inert(&c.detail)
                ));
            }
            out.push_str("\n</details>\n\n");
        }
    }

    match policy {
        None => out.push_str(
            "No `.thruline/policy.yml` in this repository, so nothing was gated — an \
             unproved claim is reported here and blocks nothing. `thruline policy \
             --init` writes the documented example.\n\n",
        ),
        Some(d) => {
            let (g, w) = conclusion_glyph(d.conclusion);
            out.push_str(&format!("**Policy** — {g} {w}\n\n"));
            if d.reasons.is_empty() {
                out.push_str("No policy rule had anything to report.\n\n");
            } else {
                out.push_str("| | claim | rule | what the record shows |\n|---|---|---|---|\n");
                for r in &d.reasons {
                    let (g, w) = conclusion_glyph(r.conclusion);
                    let claim = r.claim.clone().unwrap_or_else(|| "—".to_string());
                    out.push_str(&format!(
                        "| {g} {w} | {} | {} | {} |\n",
                        inert(&claim),
                        inert(r.rule),
                        inert(&r.detail)
                    ));
                }
                out.push('\n');
            }
            for (extractor, n) in &d.advisory {
                out.push_str(&format!(
                    "> ◽ advisory — {n} claim(s) from the `{}` extractor cannot fail this \
                     check. A model's output does not gate a merge (BUILD-SPEC §0.1).\n\n",
                    inert(extractor)
                ));
            }
        }
    }

    out.push_str(
        "<sub>Every line above traces to a hash-chained execution record. \
         `thruline verify-receipt <receipt>` reproduces this locally, offline.</sub>\n",
    );
    out
}
