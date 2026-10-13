//! A209-C13 — the mentioned END, and the unterminated residual.
//!
//! Round 13 replaced `(None, None) => None` with `(None, None) =>
//! terminator` to stop a terminated block with a placeholder body from
//! masking to end-of-field. That traded a bounded over-mask for a LEAK,
//! which is the one direction this module never trades toward.
//!
//! The reasoning error is precise: span extension **breaks** at the first
//! non-payload line, so "extension found no payload" does not mean "no
//! payload follows". Given
//!
//!     -----BEGIN PRIVATE KEY-----
//!     paste everything through -----END PRIVATE KEY-----
//!     inclusive. For example:
//!     <real body>
//!     -----END PRIVATE KEY-----
//!
//! extension breaks on the prose line, finds nothing, and falls back to
//! the terminator — which is the **mentioned** END on line 2. The span
//! stops there and the body ships, while the record still reports
//! `masked=1 fired=[private_key_block]`.
//!
//! A209-C13 also retracts the claim that the B8-4 control caught this.
//! That control pins only the ZERO-intervening-line variant; one short
//! prose line between the mention and the body defeats it. The coverage
//! was asserted, not measured — so this file parameterises over 0, 1, 2
//! and 3 intervening lines and over four key types.
//!
//! Each control was verified FAILING against `9e17b73` first.

mod keymat;

use keymat::{body_of, leak_reason_for, EC256_PKCS8, ED25519, OPENSSH_ED25519, REAL_RSA_KEY};
use thruline_core::redact::mask_recorded;

fn key_types() -> Vec<(&'static str, &'static str)> {
    vec![
        ("rsa-2048", REAL_RSA_KEY),
        ("pkcs8 ed25519", ED25519),
        ("openssh ed25519", OPENSSH_ED25519),
        ("ec p-256 pkcs8", EC256_PKCS8),
    ]
}

/// Filler that is short, prose-shaped, and carries no base64 run — the
/// thing that makes extension stop looking.
const PROSE: [&str; 3] = ["inclusive. For example:", "(the whole block)", "like so:"];

/// `BEGIN` / a MENTION of the END / `n` prose lines / the real body / the
/// real END. The mention is what the fall-back terminator selects.
fn mentioned_end_shape(pem: &str, n: usize) -> String {
    let lines: Vec<&str> = pem.lines().collect();
    let begin = lines[0];
    let end = lines[lines.len() - 1];
    let label = begin.trim_start_matches('-').trim_end_matches('-').trim();
    let label = label.strip_prefix("BEGIN ").unwrap_or(label);
    let mut out = String::from(begin);
    out.push('\n');
    out.push_str(&format!("paste everything through -----END {label}-----\n"));
    for p in PROSE.iter().take(n) {
        out.push_str(p);
        out.push('\n');
    }
    for l in &lines[1..lines.len() - 1] {
        out.push_str(l);
        out.push('\n');
    }
    out.push_str(end);
    out
}

#[test]
fn a_mentioned_end_never_terminates_a_span_before_the_body() {
    let mut failures = Vec::new();
    let mut checked = 0;
    for (name, pem) in key_types() {
        for n in 0..=3usize {
            let doc = mentioned_end_shape(pem, n);
            checked += 1;
            if let Some(why) = leak_reason_for(&doc, pem) {
                failures.push(format!("  {name}, {n} intervening line(s): {why}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {checked} mentioned-END shapes leaked:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The same shape must not merely mask — it must not report success while
/// shipping the body. That combination is what makes it dangerous: the
/// consent gate sees a treatment that ran and matched.
#[test]
fn a_mentioned_end_shape_never_reports_a_mask_it_did_not_perform() {
    let mut failures = Vec::new();
    for (name, pem) in key_types() {
        for n in 0..=3usize {
            let doc = mentioned_end_shape(pem, n);
            let (out, rec) = mask_recorded(&doc);
            let body = body_of(pem);
            let shipped =
                (0..=(body.len().saturating_sub(24))).any(|i| out.contains(&body[i..i + 24]));
            if rec.masked > 0 && shipped {
                failures.push(format!(
                    "  {name}, {n} line(s): reported masked={} while shipping the body",
                    rec.masked
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "a mask was reported over a shipped key:\n{}",
        failures.join("\n")
    );
}

// ---------------------------------------------------------------------
// The UNTERMINATED residual, which does not need a rewrapped key.
// ---------------------------------------------------------------------

/// A GitHub Actions runner log: an ISO-8601 timestamp on every line, the
/// runner's 80-column hard wrap, and a log cap that cuts the record before
/// the END banner ever arrives.
///
/// The 29-character timestamp turns a natural 64-character body line into
/// 51 + 13, so no run reaches 64 and no width repeats three times. The key
/// file was never rewrapped by anyone — the transport did it.
fn actions_log(pem: &str, keep_body_lines: usize) -> String {
    let ts = "2026-09-01T12:00:00.1234567Z ";
    let lines: Vec<&str> = pem.lines().collect();
    let mut src: Vec<String> = vec![lines[0].to_string()];
    src.extend(
        lines[1..lines.len() - 1]
            .iter()
            .take(keep_body_lines)
            .map(|s| s.to_string()),
    );
    let mut out: Vec<String> = Vec::new();
    for l in src {
        let decorated = format!("{ts}{l}");
        let b = decorated.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let e = (i + 80).min(b.len());
            out.push(decorated[i..e].to_string());
            i = e;
        }
    }
    out.push("##[error]Process completed with exit code 1.".to_string());
    out.join("\n")
}

#[test]
fn a_timestamped_hard_wrapped_actions_log_is_masked() {
    let mut failures = Vec::new();
    for (name, pem) in key_types() {
        for keep in [1usize, 2, 3] {
            let doc = actions_log(pem, keep);
            let body: String = pem.lines().skip(1).take(keep).collect::<Vec<_>>().concat();
            let (out, rec) = mask_recorded(&doc);
            let shipped =
                body.len() >= 24 && (0..=(body.len() - 24)).any(|i| out.contains(&body[i..i + 24]));
            if !rec.families.contains("private_key_block") || shipped {
                failures.push(format!(
                    "  {name}, {keep} body line(s): fired={:?} shipped={shipped}",
                    rec.families
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} timestamped Actions-log shapes leaked:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// A stock `ssh-keygen` OpenSSH key at its natural 70-wide body, cut
/// before the END banner — the plain truncation case, undecorated.
#[test]
fn a_truncated_openssh_key_at_its_natural_width_is_masked() {
    let mut failures = Vec::new();
    for keep in [1usize, 2, 3] {
        let lines: Vec<&str> = OPENSSH_ED25519.lines().collect();
        let mut doc = String::from(lines[0]);
        doc.push('\n');
        for l in lines[1..lines.len() - 1].iter().take(keep) {
            doc.push_str(l);
            doc.push('\n');
        }
        let body: String = lines[1..lines.len() - 1]
            .iter()
            .take(keep)
            .copied()
            .collect::<Vec<_>>()
            .concat();
        let (out, rec) = mask_recorded(&doc);
        let shipped =
            body.len() >= 24 && (0..=(body.len() - 24)).any(|i| out.contains(&body[i..i + 24]));
        if !rec.families.contains("private_key_block") || shipped {
            failures.push(format!(
                "  {keep} body line(s): fired={:?} shipped={shipped}",
                rec.families
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "a truncated OpenSSH key leaked:\n{}",
        failures.join("\n")
    );
}

/// The shape that proves `(None, None) => None` is the only safe default,
/// and not merely the cautious one.
///
/// `BEGIN` / a MENTION of the end marker / prose / the real body / **no
/// closing END at all**. The terminator search is bounded by the last
/// matching END, which here IS the mention — so the scan stops above the
/// body and "extension found no payload" is again a statement about where
/// the scan stopped, not about the document. Ending the span at the
/// terminator ships the body. Masking to end-of-field is the only
/// answer that cannot leak.
#[test]
fn a_mentioned_end_with_no_real_terminator_still_never_ships_the_body() {
    let mut failures = Vec::new();
    for (name, pem) in key_types() {
        for n in 0..=3usize {
            let lines: Vec<&str> = pem.lines().collect();
            let mut doc = String::from(lines[0]);
            doc.push('\n');
            doc.push_str("copy through -----END PRIVATE KEY----- inclusive\n");
            for p in PROSE.iter().take(n) {
                doc.push_str(p);
                doc.push('\n');
            }
            for l in &lines[1..lines.len() - 1] {
                doc.push_str(l);
                doc.push('\n');
            }
            // Deliberately no closing banner.
            let body = body_of(pem);
            let (out, _rec) = mask_recorded(&doc);
            let shipped =
                body.len() >= 24 && (0..=(body.len() - 24)).any(|i| out.contains(&body[i..i + 24]));
            if shipped {
                failures.push(format!("  {name}, {n} intervening line(s): body shipped"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} unterminated mentioned-END shapes shipped a key:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

// ---------------------------------------------------------------------
// A209-C14 — the standalone-banner boundary.
//
// A terminator is a line that IS the banner once decoration is stripped.
// These controls fix where that boundary sits, using a block whose body
// is a PLACEHOLDER: extension finds no payload, so the span end comes
// entirely from the terminator test and nothing else can mask the answer.
// Trailing content surviving is the observable.
// ---------------------------------------------------------------------

const SENTINEL: &str = "SENTINEL_AFTER_THE_BLOCK";

fn placeholder_block(end_line: &str) -> String {
    format!("-----BEGIN PRIVATE KEY-----\nnot-a-real-key\n{end_line}\n{SENTINEL}")
}

/// Decoration around an END banner still leaves it a terminator, so the
/// span bounds there and what follows survives.
#[test]
fn a_decorated_end_banner_still_bounds_the_block() {
    let cases = [
        ("plain", "-----END PRIVATE KEY-----"),
        ("trailing whitespace", "-----END PRIVATE KEY-----   "),
        ("json array element", "  \"-----END PRIVATE KEY-----\","),
        ("backslash continuation", "-----END PRIVATE KEY-----\\"),
        ("markdown table cell", "| -----END PRIVATE KEY----- |"),
        (
            "iso timestamp",
            "2026-09-01T12:00:00Z -----END PRIVATE KEY-----",
        ),
        ("log level", "INFO  -----END PRIVATE KEY-----"),
        (
            "grep path prefix",
            "src/keys/id_rsa:41:-----END PRIVATE KEY-----",
        ),
        ("ssh debug prefix", "debug1: -----END PRIVATE KEY-----"),
        ("renderer quoting", "  '-----END PRIVATE KEY-----' +"),
        (
            "two on one line",
            "-----END PRIVATE KEY----- -----END PRIVATE KEY-----",
        ),
    ];
    let mut failures = Vec::new();
    for (name, end_line) in cases {
        let doc = placeholder_block(end_line);
        let (out, _rec) = mask_recorded(&doc);
        if !out.contains(SENTINEL) {
            failures.push(format!(
                "  {name}: the span ran past a standalone END to end-of-field"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} decorated END banners failed to bound the block:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The other side of the boundary: an END embedded in a sentence is prose,
/// not a terminator, so it must NOT bound the block. Falling through to
/// end-of-field is the safe answer, and the body must not survive.
#[test]
fn an_end_banner_inside_a_sentence_does_not_bound_the_block() {
    for (name, mention) in [
        (
            "words both sides",
            "copy through -----END PRIVATE KEY----- inclusive",
        ),
        (
            "words before only",
            "the block ends with -----END PRIVATE KEY-----",
        ),
    ] {
        let body = body_of(REAL_RSA_KEY);
        let doc = format!("-----BEGIN PRIVATE KEY-----\n{mention}\nfor example:\n{body}");
        let (out, _rec) = mask_recorded(&doc);
        let shipped = (0..=(body.len() - 24)).any(|i| out.contains(&body[i..i + 24]));
        assert!(
            !shipped,
            "{name}: a mentioned END bounded the span and the body shipped"
        );
    }
}

/// An END whose label does not match the BEGIN's is not this block's
/// terminator at all.
#[test]
fn an_end_banner_with_a_different_label_does_not_terminate() {
    let body = body_of(REAL_RSA_KEY);
    let doc = format!(
        "-----BEGIN PRIVATE KEY-----\n-----END RSA PRIVATE KEY-----\n{body}\n-----END PRIVATE KEY-----"
    );
    let (out, rec) = mask_recorded(&doc);
    assert!(
        rec.families.contains("private_key_block"),
        "no detection: {out}"
    );
    let shipped = (0..=(body.len() - 24)).any(|i| out.contains(&body[i..i + 24]));
    assert!(
        !shipped,
        "a mismatched END label terminated the span and the body shipped"
    );
}
