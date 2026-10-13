//! A209-C10 — the GENERATOR: per-line decoration.
//!
//! Ten rounds enumerated members of the transport set — line numbers, diff
//! gutters, `> ` quoting, renderer quotes — and each round found its next
//! member by construction rather than by anticipation. The generator went
//! unnamed for all ten: **any per-line prefix defeats body detection**,
//! because the payload test required the whole trimmed line to be base64.
//! Prefixes are unbounded, so enumerating them could never terminate.
//!
//! These controls span distinct decoration FAMILIES rather than distinct
//! members, and the fix they force — test for a base64 RUN inside the
//! line — closes the family instead of one more member. Each was verified
//! FAILING against `b1543bf` first.

mod keymat;

use keymat::{leak_reason, REAL_RSA_KEY as RSA};
use thruline_core::redact::mask_recorded;

/// Apply a per-line decoration to every line of a document.
fn decorate(doc: &str, prefix: &str) -> String {
    doc.lines()
        .map(|l| format!("{prefix}{l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The distinct decoration families. Six of the fourteen the verifier
/// enumerated by SOURCE rather than by shape.
fn families() -> Vec<(&'static str, &'static str)> {
    vec![
        ("iso8601 timestamp", "2026-09-01T12:00:00Z "),
        ("bracketed timestamp", "[2026-09-01 12:00:00] "),
        ("bare clock", "12:04:05 "),
        ("ssh debug", "debug1: "),
        ("log level", "INFO  "),
        ("pytest assertion", "E       "),
        ("table pipe", "| "),
        ("psql continuation", "    | "),
        ("shell comment", "# "),
        ("c++ comment", "// "),
        ("c block comment", "* "),
    ]
}

/// A209-C11: this used to sample the FIRST payload line and assert only
/// that one fragment was gone. Two regressions — a shipped final wrap and
/// a shipped narrow wrap — were invisible under it for eleven rounds,
/// because detection and span extension disagree at the body's END, which
/// is exactly where a first-line check never looks. It now asserts that no
/// 24-character window of the body survives anywhere in the output.
fn leaks(doc: &str) -> bool {
    leak_reason(doc).is_some()
}

#[test]
fn every_per_line_decoration_family_is_masked() {
    let mut failures = Vec::new();
    for (name, prefix) in families() {
        if leaks(&decorate(RSA, prefix)) {
            failures.push(name);
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} decoration families leaked a real RSA-2048 key: {:?}",
        failures.len(),
        families().len(),
        failures
    );
}

/// The verifier's scenario, verbatim in shape: a developer pastes a CI log
/// — `docker logs -t`, `kubectl logs --timestamps`, a GitHub Actions raw
/// log — and asks why the parser rejects the key. The material lands in
/// "What was asked", one of the two sections this module's own header
/// calls the first a fresh session reads.
#[test]
fn a_pasted_ci_log_containing_a_key_is_masked() {
    for (name, prefix) in [
        ("docker logs -t", "2026-09-01T12:00:00.123456789Z "),
        ("kubectl --timestamps", "2026-09-01T12:00:00Z "),
        ("github actions raw", "2026-09-01T12:00:00.1234567Z "),
    ] {
        let log = format!(
            "{}\n{}\n{}",
            format_args!("{prefix}Starting deploy"),
            decorate(RSA, prefix),
            format_args!("{prefix}error: parser rejected key")
        );
        assert!(
            !leaks(&log),
            "{name}: a pasted CI log shipped a real RSA-2048 key verbatim"
        );
    }
}

/// The unterminated span ran to end-of-field, so trailing content in the
/// SAME field was destroyed. A209-C9's "object structure intact" held only
/// because those artifacts happened to be in separate fields.
#[test]
fn trailing_content_in_the_same_field_survives_an_unterminated_key() {
    let head = RSA.lines().take(3).collect::<Vec<_>>().join("\n");
    let doc = format!(
        "{head}\nSENTINEL_ONE\nif rg -n -- '-----BEGIN PRIVATE KEY-----' . ; then echo SCAN_SURVIVES; fi\nSENTINEL_TWO"
    );
    let (out, rec) = mask_recorded(&doc);
    assert!(
        rec.families.contains("private_key_block"),
        "no detection: {out}"
    );
    // Every payload line the document actually contains, not the first two.
    for frag in head.lines().filter(|l| l.len() >= 60) {
        assert!(!out.contains(frag), "key material shipped: {out}");
    }
    for sentinel in ["SENTINEL_ONE", "SCAN_SURVIVES", "SENTINEL_TWO"] {
        assert!(
            out.contains(sentinel),
            "{sentinel} was destroyed by an unterminated span running to end-of-field: {out}"
        );
    }
}

/// The over-mask direction the run-based test opens: content near a banner
/// mention that is base64-alphabet but is not key material.
#[test]
fn base64_shaped_prose_near_a_banner_is_not_masked() {
    for (name, doc) in [
        (
            "git shas near a mention",
            "see -----BEGIN PRIVATE KEY----- handling\n             da39a3ee5e6b4b0d3255bfef95601890afd80709\n             e3b0c44298fc1c149afbf4c8996fb92427ae41e4\n",
        ),
        (
            "hash table near a mention",
            "// -----BEGIN PRIVATE KEY----- is the banner\n             sha256 e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934c\n             sha256 da39a3ee5e6b4b0d3255bfef95601890afd80709aabbccdd\n",
        ),
    ] {
        let (out, rec) = mask_recorded(doc);
        assert_eq!(
            rec.masked, 0,
            "{name}: base64-shaped prose near a banner mention was masked: {out}"
        );
    }
}
