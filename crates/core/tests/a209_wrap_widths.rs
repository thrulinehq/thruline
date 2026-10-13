//! A209-C11 — the SECOND generator: wrap width.
//!
//! Round 10 named the first generator (any per-line decoration defeats a
//! whole-line base64 test) and cured it by testing for a base64 RUN inside
//! the line. That cure carried an unexamined assumption: that payload runs
//! are 64 characters, because RFC 7468 wraps at 64. The detection bar was
//! set to 64 and `qualifying >= 2` was left in place above it — where it
//! is unreachable, since any input with two runs of 64 already has one.
//!
//! Wrap width is not a constant. `openssl` and `gpg` emit 64, OpenSSH 70,
//! MIME 76; an editor, a log pane, `fold -w`, or a terminal capture emits
//! whatever it likes. A key wrapped at 40 has no run of 64 anywhere and
//! ships in full. Worse, a *hard wrap* applied on top of a decoration
//! breaks the width into an alternation — a timestamped 64-wrapped body
//! folded at 80 columns yields runs of 49, 15, 49, 15, … — so neither a
//! length bar nor a run of equal consecutive widths sees it.
//!
//! Each control here was verified FAILING against `8d93d94` before any
//! production change, and asserts the ENTIRE body absent (every 24-char
//! window), not a sampled first line.

mod keymat;

use keymat::{decorate, hard_wrap, leak_reason, rewrap, REAL_RSA_KEY};
use thruline_core::redact::mask_recorded;

/// Widths a real pipeline produces. 64 is RFC 7468, 76 is MIME; the rest
/// are what a wrapper, an editor or a narrow pane leaves behind.
const WIDTHS: [usize; 7] = [40, 44, 48, 56, 60, 64, 76];

#[test]
fn every_wrap_width_is_masked_undecorated() {
    let mut failures = Vec::new();
    for w in WIDTHS {
        if let Some(why) = leak_reason(&rewrap(w)) {
            failures.push(format!("  wrap {w}: {why}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} wrap widths leaked a real RSA-2048 key:\n{}",
        failures.len(),
        WIDTHS.len(),
        failures.join("\n")
    );
}

#[test]
fn every_wrap_width_is_masked_decorated() {
    // One decoration per family that round 10 measured as leaking, so a
    // width fix cannot be verified only on undecorated input.
    let decorations = [
        ("iso8601 timestamp", "2026-09-01T12:00:00Z "),
        ("ssh debug", "debug1: "),
        ("pytest assertion", "E       "),
        ("table pipe", "| "),
    ];
    let mut failures = Vec::new();
    for w in WIDTHS {
        for (name, prefix) in decorations {
            let doc = decorate(&rewrap(w), prefix);
            if let Some(why) = leak_reason(&doc) {
                failures.push(format!("  wrap {w} + {name}: {why}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} (width x decoration) pairs leaked:\n{}",
        failures.len(),
        WIDTHS.len() * decorations.len(),
        failures.join("\n")
    );
}

/// The shape that defeats equal-consecutive-width reasoning: a decorated
/// key hard-wrapped by a fixed-width pane. Run lengths alternate long,
/// short, long, short — no two ADJACENT candidate lines share a width, and
/// none reaches 64.
#[test]
fn hard_wrapped_logs_are_masked() {
    let cases = [
        (
            "ts+64 folded at 80",
            "2024-01-01T00:00:00.000000000Z ",
            64usize,
            80usize,
        ),
        ("pytest+76 folded at 80", "E       ", 76, 80),
        ("ssh debug+70 folded at 72", "debug1: ", 70, 72),
        ("bare 64 folded at 50", "", 64, 50),
    ];
    let mut failures = Vec::new();
    for (name, prefix, wrap, cols) in cases {
        let doc = hard_wrap(&decorate(&rewrap(wrap), prefix), cols);
        if let Some(why) = leak_reason(&doc) {
            failures.push(format!("  {name}: {why}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} hard-wrapped logs leaked:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

/// The k6.pem regression, verbatim in shape: a final wrap short enough
/// that span extension's bar rejects it, and — at width 24 — one that
/// BEGINS with `+`. `body_candidate` trimmed `+` as a decoration, so the
/// 16-character final line `+zB+Rj4/f3UQilGJ` measured 15 and the span
/// stopped one line early, shipping the tail of the key.
#[test]
fn short_final_wraps_are_masked() {
    // width -> final wrap length, for this 1,624-character body.
    let cases = [(54usize, 4usize), (16, 8), (62, 12), (24, 16)];
    let mut failures = Vec::new();
    for (w, tail) in cases {
        let doc = rewrap(w);
        let last = doc.lines().nth_back(1).unwrap_or_default().to_string();
        assert_eq!(last.len(), tail, "fixture drifted: wrap {w} final line");
        if let Some(why) = leak_reason(&doc) {
            failures.push(format!("  wrap {w} (final {tail}, `{last}`): {why}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} short final wraps leaked:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

/// The final wrap at width 24 begins with `+`, which is a base64
/// character and must never be trimmed as decoration.
#[test]
fn final_wrap_beginning_with_plus_is_payload() {
    let doc = rewrap(24);
    let last = doc.lines().nth_back(1).unwrap_or_default();
    assert!(
        last.starts_with('+') && last.len() == 16,
        "fixture drifted: expected a 16-char final wrap starting with `+`, got `{last}`"
    );
    assert!(
        leak_reason(&doc).is_none(),
        "a 16-character final wrap beginning with `+` shipped: {:?}",
        leak_reason(&doc)
    );
}

/// The raw fast-path guard reads the ORIGINAL text for `-----BEGIN`, so a
/// banner whose `B` is percent-encoded returns before the decoder — which
/// would have produced `-----BEGIN` — ever runs. The guard must consume
/// the decoded view, like every other reader in this module.
#[test]
fn percent_encoded_banner_is_not_a_bypass() {
    let doc = REAL_RSA_KEY.replacen("-----BEGIN", "-----%42EGIN", 1);
    assert!(
        !doc.to_ascii_uppercase().contains("-----BEGIN"),
        "fixture drifted: the raw text must not contain a literal banner"
    );
    assert!(
        leak_reason(&doc).is_none(),
        "a percent-encoded banner bypassed masking entirely: {:?}",
        leak_reason(&doc)
    );
}

/// Encoded end-to-end: the same bypass through the transport the ledger
/// actually stores, where the whole record is one JSON-escaped line.
#[test]
fn percent_encoded_banner_survives_json_transport() {
    let escaped = REAL_RSA_KEY
        .replacen("-----BEGIN", "-----%42EGIN", 1)
        .replace('\n', "\\n");
    assert!(
        leak_reason(&escaped).is_none(),
        "a percent-encoded banner in JSON transport leaked: {:?}",
        leak_reason(&escaped)
    );
}

/// The over-mask direction the width rule opens, and the reason equal
/// width alone is not enough: `rg` and `grep` print the SAME long path on
/// every match line, so six output lines share one 60-character run.
///
/// Measured on the operator's own ledger, a width rule without the
/// distinctness clause fired on 13 rows of 2,046,353 — every one a
/// credential-scanning command, a detector's own regex, or a test fixture,
/// and every one with at most two DISTINCT runs at its repeating width.
/// A wrapped key's payload lines are distinct by construction.
#[test]
fn grep_output_repeating_one_path_near_a_banner_is_not_masked() {
    let path = "aG9tZS9vcGVyYXRvci9zcmMvY3JlZGVudGlhbHMvc2Nhbm5lci9maXh0dXJl";
    assert_eq!(
        path.len(),
        60,
        "fixture drifted: the repeated run must be wide"
    );
    let doc = format!(
        "rg -n -- '-----BEGIN [A-Z ]*PRIVATE KEY-----' .\n\
         {path}.swift:9:        -----BEGIN RSA PRIVATE KEY-----\n\
         {path}.swift:15:        openai key goes here\n\
         {path}.swift:16:        anthropic key goes here\n\
         {path}.swift:17:        aws key goes here\n\
         {path}.swift:19:        authorization header\n"
    );
    let (out, rec) = mask_recorded(&doc);
    assert_eq!(
        rec.masked, 0,
        "grep output repeating one path was masked as key material: {out}"
    );
}

/// The same shape with DISTINCT runs is payload and must mask — the
/// control that stops the clause above from being satisfied by simply
/// never firing.
#[test]
fn distinct_runs_of_one_width_near_a_banner_are_still_masked() {
    assert!(
        leak_reason(&rewrap(60)).is_none(),
        "a key wrapped at 60 with distinct lines must still mask"
    );
}
