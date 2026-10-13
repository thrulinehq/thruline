//! A209-C12 — key TYPES, and the generator round 12 missed.
//!
//! Round 12 closed wrap width by requiring a repeating wrap: some run
//! width >= 16 shared by >= 3 candidate lines. That bar is met by every
//! RSA-2048 key, which has 26 body lines at wrap 64 — and RSA-2048 was the
//! entire corpus. It is met by NO key whose body is short.
//!
//! A PKCS#8 Ed25519 or X25519 private key is **one line of 64 characters**.
//! Re-wrapped at any width from 24 to 63 it yields at most two lines of
//! equal width and no run of 64, so neither arm of the rule fires and the
//! block is emitted verbatim. Ed25519 is `ssh-keygen`'s default algorithm.
//! EC P-256 leaks the same way at 55-63 — including width 60, which
//! A209-C11 names among the widths it fixed.
//!
//! The second miss is in the census itself: only the LONGEST run on each
//! line was counted, so any constant per-line token with a longer run
//! erased the payload from the census. `/` is a base64 character, so a
//! filesystem path is one long run — `rg` output hides the key it prints.
//!
//! The fix is structural, not another heuristic: a BEGIN banner with a
//! matching END banner is a self-identifying block, and the format is what
//! says so. Heuristics remain only for UNTERMINATED blocks. That makes the
//! label allowlist load-bearing, so certificates, public keys and
//! `authorized_keys` get their own must-not-mask controls.
//!
//! Each control was verified FAILING against `823f02a` first.

mod keymat;

use keymat::{
    body_of, leak_reason_for, must_not_mask, private_key_specimens, rewrap_pem,
    surviving_fragment_of,
};
use thruline_core::redact::mask_recorded;

/// Every private-key type, at every wrap width a tool or a terminal can
/// produce. `fold -w 40` emits these; so does an 80-column pane.
#[test]
fn every_private_key_type_is_masked_at_every_wrap_width() {
    let mut failures = Vec::new();
    let mut checked = 0usize;
    for (name, pem) in private_key_specimens() {
        for w in 24..=76usize {
            let doc = rewrap_pem(pem, w);
            checked += 1;
            if let Some(why) = leak_reason_for(&doc, pem) {
                failures.push(format!("  {name} @ wrap {w}: {why}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {checked} (key type x wrap width) pairs leaked a real private key:\n{}",
        failures.len(),
        failures
            .iter()
            .take(24)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The unwrapped originals, exactly as `openssl` and `ssh-keygen` wrote
/// them. A single-line 64-character body has no wrap to repeat at all.
#[test]
fn every_private_key_type_is_masked_as_emitted() {
    let mut failures = Vec::new();
    for (name, pem) in private_key_specimens() {
        if let Some(why) = leak_reason_for(pem, pem) {
            failures.push(format!("  {name}: {why}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} key types leaked as their tool emitted them:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// A constant per-line token whose base64 run is LONGER than the wrap
/// erases the payload from a census that keeps only the longest run per
/// line. `/` is a base64 character, so a path is one run.
#[test]
fn a_path_prefix_does_not_hide_the_payload_it_precedes() {
    let path = "/home/ci/deploy/secrets/prod/keydir/id_rsa";
    let mut failures = Vec::new();
    for (name, pem) in private_key_specimens() {
        for w in [32usize, 40] {
            let doc = rewrap_pem(pem, w)
                .lines()
                .enumerate()
                .map(|(i, l)| format!("{path}:{}:{l}", i + 1))
                .collect::<Vec<_>>()
                .join("\n");
            if let Some(why) = leak_reason_for(&doc, pem) {
                failures.push(format!("  {name} @ wrap {w}: {why}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} path-prefixed outputs hid the key they printed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Once block STRUCTURE decides, the label allowlist in `is_key_banner`
/// is the only thing standing between a certificate chain and
/// destruction. None of these may be treated as a private key, and their
/// key material must survive whole.
///
/// Scoped to `private_key_block` deliberately. A209-C12 corrects the
/// verdict's claim that `authorized_keys` "did not fire at HEAD": it did,
/// and still does, under the **email** family — `ssh-keygen` appends a
/// `user@host` comment, which is email-shaped and is masked by a detector
/// this stage never touched. That masking is pre-existing, identical at
/// `823f02a`, and correct: it removes the comment and leaves the public
/// key blob intact, which is the property asserted here.
#[test]
fn terminated_blocks_that_are_not_private_keys_are_never_masked() {
    let mut failures = Vec::new();
    for (name, doc) in must_not_mask() {
        let (out, rec) = mask_recorded(&doc);
        if rec.families.contains("private_key_block") {
            failures.push(format!("  {name}: treated as a private key"));
        }
        // The key material itself must survive whole.
        for payload in doc.lines().filter(|l| !l.trim_start().starts_with("-----")) {
            let material = payload
                .split_whitespace()
                .find(|w| w.len() >= 24)
                .unwrap_or(payload);
            if !material.is_empty() && !out.contains(material) {
                failures.push(format!("  {name}: material was destroyed"));
                break;
            }
        }
    }
    assert!(
        failures.is_empty(),
        "a non-private-key block was destroyed:\n{}",
        failures.join("\n")
    );
}

/// A certificate sitting directly beside a real private key: the key must
/// go, the certificate must stay. This is the pair that a structural rule
/// gets wrong if the label allowlist slips.
#[test]
fn a_certificate_beside_a_key_survives_while_the_key_is_masked() {
    let (_, key) = private_key_specimens()[0];
    let cert = keymat::CERTIFICATE;
    let doc = format!("{cert}\n{key}\n{cert}");
    let (out, rec) = mask_recorded(&doc);
    assert!(
        rec.families.contains("private_key_block"),
        "the key was not detected beside certificates: {out}"
    );
    assert!(
        surviving_fragment_of(&out, &body_of(key)).is_none(),
        "the key survived beside certificates: {out}"
    );
    let cert_line = cert.lines().nth(1).expect("certificate payload");
    assert_eq!(
        out.matches(cert_line).count(),
        2,
        "both certificates must survive whole: {out}"
    );
}
