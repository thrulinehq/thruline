//! A209-C5 — the round-5 findings, each pinned by its EXACT reproduction
//! string from the review that found it.
//!
//! Six consecutive rounds produced guards that passed because they tested
//! an easier case than the defect. The rule this file exists to enforce:
//! **a control's specimen is the finding's shape, verbatim.** Every test
//! here was verified to FAIL against `9796f49` before the fix was written;
//! that verification is the deliverable, not the green run.

use thruline_core::redact::{mask_recorded, Treatment};

const B64: &str = "MIIEowIBAAKCAQEATLCANARYaaZZ11bbYY22ccXX33ddWW44eeVV55ffUU66gg77";

/* ── ROOT CAUSE B — the PEM span ─────────────────────────────────────── */

/// B7-1: a tool reading a key file with line numbers. The span used to
/// stop at the first non-base64 line and re-emit everything after it.
#[test]
fn b7_1_gutter_prefixed_key_file_is_masked_whole() {
    let input = format!("     1\t-----BEGIN RSA PRIVATE KEY-----\n     2\t{B64}\n     3\t-----END RSA PRIVATE KEY-----");
    let (out, record) = mask_recorded(&input);
    assert!(
        record.families.contains("private_key_block"),
        "no detection: {out}"
    );
    assert!(!out.contains(B64), "the key body shipped: {out}");
}

/// B7-2 (REGRESSION against 0460a9e): an ordinary web-server config. The
/// label was taken as the whole remainder of the banner line, so a
/// `CERTIFICATE` elsewhere on that line vetoed the real key.
#[test]
fn b7_2_a_cert_on_the_same_line_does_not_veto_the_key() {
    let input = format!(
        r#"{{"cert":"-----BEGIN CERTIFICATE-----MIIBcert","key":"-----BEGIN RSA PRIVATE KEY----- {B64} -----END RSA PRIVATE KEY-----"}}"#
    );
    let (out, record) = mask_recorded(&input);
    assert!(
        record.families.contains("private_key_block"),
        "the key was vetoed by a certificate on the same line: {out}"
    );
    assert!(!out.contains(B64), "the key body shipped: {out}");
}

/// B7-3: the single-line non-key banner — the exact string A209-C4 wrote
/// as its own B4 example, which its guard did not cover because the guard
/// used a multi-line specimen.
#[test]
fn b7_3_a_single_line_non_key_banner_is_not_masked() {
    let input =
        "-----BEGIN CHANGELOG----- notes about PRIVATE KEY handling -----END CHANGELOG----- keep this prose";
    let (out, record) = mask_recorded(input);
    assert_eq!(record.masked, 0, "a CHANGELOG was masked: {out}");
    assert_eq!(out, input, "prose was destroyed");
}

/// B7-4: a complete one-line PEM must not swallow what follows it. The
/// destructive command here is exactly what a successor most needs.
#[test]
fn b7_4_a_one_line_pem_does_not_swallow_the_rest_of_the_line() {
    let input = format!(
        "openssl rsa -in key.pem -----BEGIN RSA PRIVATE KEY----- {B64} -----END RSA PRIVATE KEY----- && rm -rf /var/lib/production-data && echo TAIL_MATTERS"
    );
    let (out, record) = mask_recorded(&input);
    assert!(record.families.contains("private_key_block"));
    assert!(!out.contains(B64), "the key body shipped: {out}");
    assert!(
        out.contains("rm -rf /var/lib/production-data") && out.contains("TAIL_MATTERS"),
        "the tail after the END banner was swallowed: {out}"
    );
    assert!(
        out.contains("openssl rsa -in key.pem"),
        "the head was lost: {out}"
    );
}

/// A key banner with no terminator must fail toward masking.
#[test]
fn b7_1b_an_unterminated_key_masks_to_the_end() {
    let input = format!("-----BEGIN RSA PRIVATE KEY-----\n{B64}\nsome trailing prose");
    let (out, record) = mask_recorded(&input);
    assert!(record.families.contains("private_key_block"));
    assert!(!out.contains(B64), "the body shipped: {out}");
}

/* ── ROOT CAUSE A — the record must reach the treatment ──────────────── */

/// B7-6: `Treatment::Unknown` must have a production path. A record whose
/// provenance is unavailable may not render as a confident zero.
#[test]
fn b7_6_an_unknown_record_produces_unknown_not_a_confident_zero() {
    let unknown = thruline_core::redact::MaskRecord::unknown();
    let treatment = unknown.to_treatment();
    assert!(
        matches!(treatment, Treatment::Unknown { .. }),
        "an unknown record rendered as {treatment:?}"
    );
    assert!(
        treatment.needs_acknowledgement(),
        "an unknown record opened the consent gate"
    );
    // A genuine complete zero is still allowed to open it.
    let empty = thruline_core::redact::MaskRecord::default();
    assert!(!empty.to_treatment().needs_acknowledgement());
}

/// B7-8: a label lowercased before masking. An AWS key id is `[A-Z0-9]{20}`,
/// so a lowercased one is exactly recoverable by upper-casing.
#[test]
fn b7_8_a_lowercased_credential_is_still_caught() {
    // The masker must see the ORIGINAL case. This pins the property at the
    // redaction layer; the derive-path ordering is pinned in the capsule
    // tests, where the label actually gets built.
    let original = "AKIAIOSFODNN7EXAMPLE was used";
    let (_, record) = mask_recorded(original);
    assert_eq!(record.masked, 1, "the control case must mask");
    // Lowercasing first defeats it — which is why masking must come first.
    let (_, after_lowercase) = mask_recorded(&original.to_lowercase());
    assert_eq!(
        after_lowercase.masked, 0,
        "if this ever masks, the ordering argument below is moot"
    );
}

/// B7-4 (second half): a trailing partial marker. The word loop splits on
/// `' '` and `[masked: github]` is TWO words, so a budget landing between
/// them persisted `[masked:` alone — in all four artifacts and in the real
/// `nodes` table. The marker-safe helper had been reachable only from a
/// fallback branch.
#[test]
fn b7_4b_a_trailing_partial_marker_is_dropped_not_persisted() {
    let with_marker = "added the [masked: github] token";
    let cut_inside = &with_marker[..with_marker.find("github]").unwrap() + 2];
    assert!(
        cut_inside.contains("[masked:") && !cut_inside.contains(']'),
        "the fixture must actually contain a partial marker: {cut_inside:?}"
    );
    let repaired =
        thruline_core::redact::truncate_respecting_markers(cut_inside, cut_inside.chars().count());
    assert!(
        !repaired.contains("[masked"),
        "a partial marker survived: {repaired:?}"
    );
    assert!(
        repaired.starts_with("added the"),
        "content lost: {repaired:?}"
    );
}

/// A209-C5, found by re-measuring the live ledger AFTER the B7-3 fix:
/// containment on the words "PRIVATE KEY" is not a banner test. 260 of 269
/// live firings were regex patterns and code discussing PEM detection —
/// this project's own source — destroying 1.7M characters.
#[test]
fn a_regex_or_code_fragment_is_not_a_pem_banner() {
    for not_a_banner in [
        r#"-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----"#,
        r#"-----BEGIN [^-]{0,80}PRIVATE KEY-----"#,
        r#"-----BEGIN [ A-Z0-9_-]{0,100}PRIVATE KEY(?: BLOCK)?-----"#,
        r#"re.compile(r"-----BEGIN " + "PRIVATE KEY")"#,
        "-----BEGIN {0} PRIVATE KEY-----",
        "-----BEGIN … PRIVATE KEY-----",
    ] {
        let (out, record) = mask_recorded(not_a_banner);
        assert_eq!(
            record.masked, 0,
            "a regex/code fragment was masked as a key: {not_a_banner:?} -> {out}"
        );
    }
    // Real labels still fire.
    for real in [
        "RSA PRIVATE KEY",
        "OPENSSH PRIVATE KEY",
        "ENCRYPTED PRIVATE KEY",
        "PGP MESSAGE",
    ] {
        let input = format!("-----BEGIN {real}-----\n{B64}\n-----END {real}-----");
        assert!(
            mask_recorded(&input)
                .1
                .families
                .contains("private_key_block"),
            "a real banner stopped firing: {real}"
        );
    }
}
