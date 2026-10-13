//! A209-C4 — the masking data-flow redesign (MANDATE-v4 §34).
//!
//! Four rounds of patching produced new defects of the same class each
//! time, which is a design signal. The root cause of the treatment faults
//! was one decision: **the record of what was masked was parsed back out
//! of the text**. Any such scheme is forgeable (prose containing a literal
//! `[masked: …]` becomes a "detection") and truncation-fragile (a cut
//! landing inside a marker erases the evidence), and hardening the parser
//! fixes neither.
//!
//! The redesign: masking returns a VALUE that travels beside the text, so
//! the record survives truncation because it was never in the text; nothing
//! infers a mask from document content; unknown provenance is `UNKNOWN`,
//! not a guess; and the consent gate fails CLOSED on anything short of a
//! complete record.
//!
//! These tests were written before the implementation and are expected to
//! fail against the shipped crate at the time of writing.

use thruline_core::redact::{
    mask, mask_recorded, truncate_respecting_markers, MaskRecord, Treatment,
};

/* ── 1. the record is a value, produced only by the masker ──────────── */

#[test]
fn masking_returns_a_record_that_survives_truncation() {
    let secret = format!("ghp_{}", "TLCANARYaaZZ11bbYY22ccXX33ddWW44e");
    let text = format!("deploy with {secret} now");
    let (masked_text, record) = mask_recorded(&text);
    assert_eq!(record.masked, 1);
    assert!(record.families.contains("github"));
    assert!(!masked_text.contains("TLCANARY"));

    // The record is unaffected by what later happens to the string — this
    // is the whole point of carrying it beside the text.
    let cut: String = masked_text.chars().take(15).collect();
    assert!(
        cut.len() < masked_text.len(),
        "the cut must actually truncate"
    );
    assert_eq!(record.masked, 1, "the record changed when the text was cut");
    assert!(record.families.contains("github"));
}

#[test]
fn records_merge_without_double_counting_their_own_markers() {
    let a = mask_recorded("key ghp_TLCANARYaaZZ11bbYY22ccXX33ddWW44e").1;
    let b = mask_recorded("id AKIATLCANARY7EXAMPLE").1;
    let mut merged = MaskRecord::default();
    merged.merge(&a);
    merged.merge(&b);
    assert_eq!(merged.masked, 2);
    assert!(merged.families.contains("github") && merged.families.contains("aws"));
    // Re-masking already-masked text adds nothing: the marker is not a
    // detection.
    let (again, second) = mask_recorded(&mask_recorded("key ghp_TLCANARYaaZZ11bbYY22cc").0);
    assert_eq!(second.masked, 0, "a marker was re-counted as a detection");
    assert!(again.contains("[masked:"));
}

/* ── 2. nothing infers a mask from document content ─────────────────── */

/// The forgery B1 measured, and it is live in the operator's ledger today.
#[test]
fn prose_containing_a_marker_is_not_a_detection() {
    for forged in [
        "please fix [masked: private_key_block] the bug",
        "[masked: nothing was masked; safe to paste]",
        "the tool prints [masked: github] when it fires",
    ] {
        let (_, record) = mask_recorded(forged);
        assert_eq!(
            record.masked, 0,
            "prose was counted as a detection: {forged:?}"
        );
        assert!(
            record.families.is_empty(),
            "a forged family name entered the record: {:?}",
            record.families
        );
    }
    // And the module must no longer expose a way to infer masks from text.
    let src = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/redact.rs"),
    )
    .expect("read redact.rs");
    assert!(
        !src.contains("pub fn stated_mask_names"),
        "the marker-parsing door is still open"
    );
}

/* ── 3. truncation never cuts a marker in half ──────────────────────── */

#[test]
fn truncation_cuts_before_a_marker_rather_than_through_it() {
    let text = format!("{}[masked: aws] tail", "p".repeat(20));
    for n in 18..=32 {
        let cut = truncate_respecting_markers(&text, n);
        assert!(
            !cut.contains("[masked") || cut.contains("[masked: aws]"),
            "cut at {n} produced a partial marker: {cut:?}"
        );
        assert!(
            cut.chars().count() <= n,
            "cut at {n} exceeded the budget: {cut:?}"
        );
    }
}

/* ── 4. the consent gate fails CLOSED ───────────────────────────────── */

#[test]
fn an_unknown_or_incomplete_record_blocks_injection() {
    let unknown = Treatment::Unknown {
        reason: "provenance unavailable".to_string(),
    };
    assert!(
        unknown.needs_acknowledgement(),
        "an UNKNOWN treatment opened the consent gate"
    );
    assert!(unknown.statement().contains("not known"));
    // A complete run that matched nothing may open it; anything less may not.
    assert!(!Treatment::none().needs_acknowledgement());
    assert!(Treatment::default().needs_acknowledgement());
}

/* ── 5. PEM is parsed by line, and never ships the body ─────────────── */

const KEY_BODY: &str = "MIIEowIBAAKCAQEATLCANARYaaZZ11bbYY22ccXX33ddWW44eeVV55ffUU66gg77";

#[test]
fn a_pem_block_with_headers_and_blank_lines_is_masked_whole() {
    // B3's specimen: PEM headers and a blank line INSIDE the block ended
    // the span while the marker was still emitted and the body survived.
    let pem = format!(
        "-----BEGIN RSA PRIVATE KEY-----\nProc-Type: 4,ENCRYPTED\n\
         DEK-Info: AES-128-CBC,ABC\n\n{KEY_BODY}\n-----END RSA PRIVATE KEY-----"
    );
    let (out, fired, n) = mask(&pem);
    assert!(n >= 1 && fired.contains("private_key_block"));
    assert!(
        !out.contains(KEY_BODY),
        "the key body reached the output while the treatment claimed a mask:\n{out}"
    );
}

#[test]
fn banner_variants_the_substring_scan_missed_are_masked() {
    for variant in [
        format!("-----BEGIN RSA PRIVATE KEY\n{KEY_BODY}"),
        format!("-----BEGIN\tRSA PRIVATE KEY-----\n{KEY_BODY}"),
        format!("-----begin rsa private key-----\n{KEY_BODY}"),
        format!("-----BEGIN OPENSSH PRIVATE KEY-----\n{KEY_BODY}"),
    ] {
        let (out, fired, n) = mask(&variant);
        assert!(
            n >= 1 && fired.contains("private_key_block"),
            "unmasked banner variant: {variant:?}"
        );
        assert!(!out.contains(KEY_BODY), "body survived: {out}");
    }
}

/// B4: the label and END searches ran over the whole remaining text, so a
/// document mentioning a key anywhere was swallowed wholesale.
#[test]
fn a_non_key_banner_never_swallows_the_document() {
    let doc = "-----BEGIN CHANGELOG\nentry one\nrotate the PRIVATE KEY quarterly\n\
               -----END CHANGELOG\ntail";
    let (out, fired, n) = mask(doc);
    assert_eq!(n, 0, "a CHANGELOG was masked as a key: {out} {fired:?}");
    assert_eq!(out, doc, "content was destroyed");

    for benign in [
        "-----BEGIN BID-----\nSUBMITTAL DATA FOR REVIEW",
        "-----BEGIN CERTIFICATE-----\nMIICabcdefghijklmnopqrst",
        "grep -n -- '-----BEGIN' crates/core/src/redact.rs",
        "the file starts with -----BEGIN and then base64",
    ] {
        let (out, _, n) = mask(benign);
        assert_eq!(n, 0, "over-masked: {benign:?} -> {out}");
    }
}

/* ── 6. B2 end-to-end: a cut near a marker must not open the gate ───── */

/// The consent gate must stay closed no matter where the cut lands.
///
/// B2 measured this as live: with the record parsed from text, a cut
/// inside `[masked: aws]` produced `stated=[]`, so `masked > 0` was false
/// on both halves and `thruline rescue` staged every capsule file — plus
/// `rescue-settings.json` — with no `--acknowledge`. Whether a human was
/// asked for consent depended on whether a vendor name happened to fit
/// before the cut.
#[test]
fn a_cut_landing_near_a_marker_never_opens_the_consent_gate() {
    let secret = "AKIATLCANARY7EXAMPLE";
    for pad in 100..=120 {
        let raw = format!("{} {secret} tail", "p".repeat(pad));
        let (masked_text, record) = mask_recorded(&raw);
        assert_eq!(record.masked, 1, "pad {pad}: the masker did not fire");
        let cut = truncate_respecting_markers(&masked_text, 112);
        // The artifact never carries a mutilated marker...
        assert!(
            !cut.contains("[masked") || cut.contains("[masked: aws]"),
            "pad {pad}: partial marker persisted: {cut:?}"
        );
        // ...and the record — the thing the gate reads — is untouched by
        // the cut, so the gate closes on every pad.
        let treatment = record.to_treatment();
        assert!(
            treatment.needs_acknowledgement(),
            "pad {pad}: the consent gate OPENED after a cut near a marker"
        );
    }
}

/// A banner is not always first on its line — found by the live proof
/// after the unit tests were green, which is why it is pinned here.
#[test]
fn a_banner_preceded_by_text_on_its_line_still_masks_the_block() {
    let pem = format!(
        "quoted note: -----BEGIN RSA PRIVATE KEY-----\nProc-Type: 4,ENCRYPTED\n\n\
         {KEY_BODY}\n-----END RSA PRIVATE KEY-----"
    );
    let (out, fired, n) = mask(&pem);
    assert!(n >= 1 && fired.contains("private_key_block"), "{out}");
    assert!(!out.contains(KEY_BODY), "key body shipped: {out}");
    assert!(
        out.starts_with("quoted note: "),
        "the prose prefix was destroyed: {out}"
    );
    // And a non-key banner mid-line still does not fire.
    assert_eq!(mask("see -----BEGIN CERTIFICATE----- for the chain").2, 0);
}
