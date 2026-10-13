//! The polarity boundary: what a redactor masks versus what an accuser may act
//! on.
//!
//! **Over-masking is fail-safe for a redactor and fail-dangerous for an
//! accuser.** `mask_pem_blocks` fires on a terminated private-key banner
//! whatever follows it — A209-C12 disclosed the residual that buys, and those
//! rows were an accepted over-mask. §6.6's `secrets_absent` then exempted
//! structural families from its second conjunct and moved them across the
//! boundary, turning an accepted over-mask into a false accusation.
//!
//! Every shape below is checked in **both** directions: the masker's behaviour
//! is unchanged, and the accuser's is stricter.

use thruline_core::redact::{contains_accusable_key_block, mask_recorded};

fn masked_families(text: &str) -> Vec<String> {
    mask_recorded(text).1.families.into_iter().collect()
}

// ---------------------------------------------------------------------------
// Must accuse: real key material
// ---------------------------------------------------------------------------

const PKCS8: &str = "-----BEGIN PRIVATE KEY-----\n\
                     MC4CAQAwBQYDK2VwBCIEIL9k1sQFCJTZ1cVMcVQmpTfbUvC7Y0f5wY6wZ3vJqK9M\n\
                     -----END PRIVATE KEY-----\n";
const RSA: &str = "-----BEGIN RSA PRIVATE KEY-----\n\
                   MIIEowIBAAKCAQEA7Q9Xn2mKfWvGqRt3sYbLpZcNdEhJkMnOpQrStUvWxYzAbCd\n\
                   MIIEowIBAAKCAQEA7Q9Xn2mKfWvGqRt3sYbLpZcNdEhJkMnOpQrStUvWxYzAbCe\n\
                   -----END RSA PRIVATE KEY-----\n";
const OPENSSH: &str = "-----BEGIN OPENSSH PRIVATE KEY-----\n\
                       b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW\n\
                       -----END OPENSSH PRIVATE KEY-----\n";

#[test]
fn real_key_material_is_masked_and_accuses() {
    for (name, text) in [("pkcs8", PKCS8), ("rsa", RSA), ("openssh", OPENSSH)] {
        assert!(
            masked_families(text).contains(&"private_key_block".to_string()),
            "{name} was not masked"
        );
        assert!(
            contains_accusable_key_block(text),
            "{name} is real key material and did not accuse"
        );
    }
}

#[test]
fn a_truncated_real_key_still_accuses_because_the_bytes_are_still_a_key() {
    let truncated = "-----BEGIN RSA PRIVATE KEY-----\n\
                     MIIEowIBAAKCAQEA7Q9Xn2mKfWvGqRt3sYbLpZcNdEhJkMnOpQrStUvWxYzAbCd\n";
    assert!(contains_accusable_key_block(truncated));
}

// ---------------------------------------------------------------------------
// Must NOT accuse: A209-C12's disclosed over-mask, and EC PARAMETERS
// ---------------------------------------------------------------------------

#[test]
fn a_genuine_label_with_a_placeholder_body_is_masked_and_does_not_accuse() {
    // A209-C12's own disclosed residual, verbatim in shape: "terminated
    // private-key blocks with genuine labels and placeholder bodies, which a
    // redactor cannot distinguish from a key without reading the payload."
    // Fourteen such rows were an accepted OVER-MASK. Promoting them into
    // accusations is the polarity error.
    for body in [
        "not-a-real-key",
        "REPLACE ME",
        "<your-private-key-here>",
        "TODO: paste the deploy key",
        "REPLACEMEREPLACEMEREPLACEMEREPLACEME",
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
    ] {
        let text = format!("-----BEGIN PRIVATE KEY-----\n{body}\n-----END PRIVATE KEY-----\n");
        // The masker still masks. That is the accepted over-mask and it stays.
        assert!(
            masked_families(&text).contains(&"private_key_block".to_string()),
            "the masker stopped masking {body:?} — this fix must not weaken the redactor"
        );
        // The accuser does not act on it.
        assert!(
            !contains_accusable_key_block(&text),
            "{body:?} would falsely accuse a developer of committing a secret"
        );
    }
}

#[test]
fn ec_parameters_carries_a_curve_oid_and_never_accuses() {
    // On A209-C12's key-material label allowlist — correctly, because
    // over-masking is free for a redactor. It carries no private material.
    let text = "-----BEGIN EC PARAMETERS-----\nBggqhkjOPQMBBw==\n-----END EC PARAMETERS-----\n";
    assert!(
        !contains_accusable_key_block(text),
        "EC PARAMETERS accused; it is a curve OID"
    );
    // Even with a long, key-shaped body, the LABEL disqualifies it.
    let long = "-----BEGIN EC PARAMETERS-----\n\
                MC4CAQAwBQYDK2VwBCIEIL9k1sQFCJTZ1cVMcVQmpTfbUvC7Y0f5wY6wZ3vJqK9M\n\
                -----END EC PARAMETERS-----\n";
    assert!(!contains_accusable_key_block(long));
}

// ---------------------------------------------------------------------------
// The twelve shapes that already produced no family stay silent on both sides
// ---------------------------------------------------------------------------

#[test]
fn public_material_and_look_alikes_neither_mask_nor_accuse() {
    for (name, text) in [
        ("certificate", "-----BEGIN CERTIFICATE-----\nMIIDdzCCAl+gAwIBAgIEAgAAuTANBgkqhkiG9w0BAQUFADBaMQsw\n-----END CERTIFICATE-----\n"),
        ("pgp public", "-----BEGIN PGP PUBLIC KEY BLOCK-----\nmQENBF8xAAEBCADQ1nQ0v\n-----END PGP PUBLIC KEY BLOCK-----\n"),
        ("spki", "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=\n-----END PUBLIC KEY-----\n"),
        ("ssh public", "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIA1B2C3D4E5F6G7H8I9J0K user@host\n"),
        ("bid delimiter", "-----BEGIN BID-----\nsome text here\n-----END BID-----\n"),
        ("prose", "the file starts with -----BEGIN PRIVATE KEY----- as expected\n"),
        ("csr", "-----BEGIN CERTIFICATE REQUEST-----\nMIICvDCCAaQCAQAwdzELMAkGA1UEBhMCVVM\n-----END CERTIFICATE REQUEST-----\n"),
        ("changelog banner", "## -----BEGIN----- section reworked in 1.4\n"),
    ] {
        assert!(
            !contains_accusable_key_block(text),
            "{name} accused"
        );
    }
}

#[test]
fn the_positive_control_still_fires_so_this_suite_is_not_vacuous() {
    // If nothing accused, every assertion above would pass for the wrong
    // reason.
    assert!(contains_accusable_key_block(PKCS8));
    assert!(contains_accusable_key_block(RSA));
}

#[test]
fn a_key_nested_inside_json_or_markdown_still_accuses() {
    let json = format!("{{\"deploy_key\": \"{}\"}}", PKCS8.replace('\n', "\\n"));
    // The escaped form has no real newlines; the body is still key-shaped.
    assert!(
        contains_accusable_key_block(&json.replace("\\n", "\n")),
        "a key inside JSON did not accuse once decoded"
    );
    let md = format!("Here is the key:\n\n```\n{PKCS8}```\n");
    assert!(contains_accusable_key_block(&md));
}

// ---------------------------------------------------------------------------
// The verifier's three constructed non-keys, which the SHAPE proxy still
// accused. The accuser now decodes the body and asks what the bytes are.
// ---------------------------------------------------------------------------

/// base64 of ASCII, produced here rather than pasted, so the test cannot be
/// "fixed" by an encoder that disagrees with the one under test.
fn b64(text: &str) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = text.as_bytes();
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[test]
fn a_key_banner_over_base64_that_is_not_key_material_does_not_accuse() {
    // The shape proxy — base64 alphabet, 32+ characters, mixed case and digits
    // — passed all three of these. Reachability is narrow (you need a key
    // banner over non-key base64), which is why it was a residual rather than a
    // blocker; it is closed here because the discriminating version is exact.
    let cases = [
        (
            "base64 of ordinary prose",
            "PRIVATE KEY",
            b64("The deployment key lives in the vault, not in this repository at all."),
        ),
        (
            "an OPENSSH banner over a non-key blob",
            "OPENSSH PRIVATE KEY",
            b64("this is definitely not an openssh private key container, just words"),
        ),
        (
            "base64 of a git SHA",
            "PRIVATE KEY",
            b64("0f0d9f075ecfcaf8e325a570d2d2fbdd164beba8852c6e74a036c7215259652f"),
        ),
        (
            "base64 of a hex digest beginning with a zero",
            "PRIVATE KEY",
            b64("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"),
        ),
    ];
    for (name, label, body) in cases {
        let text = format!("-----BEGIN {label}-----\n{body}\n-----END {label}-----\n");
        // The masker still masks — the redactor's over-mask is fail-safe and
        // stays exactly as it was.
        assert!(
            masked_families(&text).contains(&"private_key_block".to_string()),
            "{name}: the masker stopped masking; this fix must not weaken the redactor"
        );
        assert!(
            !contains_accusable_key_block(&text),
            "{name} would falsely accuse a developer of committing a secret"
        );
    }
}

#[test]
fn every_real_private_key_encoding_still_accuses() {
    // The control that keeps the decoder from being a blanket refusal. One
    // fixture per structure the decoder recognises, so a narrowing that broke
    // any of them fails here rather than silently letting a real key through.
    let cases = [
        // PKCS#8: SEQUENCE { INTEGER 0, SEQUENCE { OID }, OCTET STRING }
        (
            "pkcs8 ed25519",
            "PRIVATE KEY",
            "MC4CAQAwBQYDK2VwBCIEIL9k1sQFCJTZ1cVMcVQmpTfbUvC7Y0f5wY6wZ3vJqK9M",
        ),
        // PKCS#1 RSA: SEQUENCE { INTEGER 0, INTEGER n, … } with a long form length
        (
            "pkcs1 rsa",
            "RSA PRIVATE KEY",
            "MIIEowIBAAKCAQEA7Q9Xn2mKfWvGqRt3sYbLpZcNdEhJkMnOpQrStUvWxYzAbCd",
        ),
        // SEC1 EC: SEQUENCE { INTEGER 1, OCTET STRING, … }
        (
            "sec1 ec",
            "EC PRIVATE KEY",
            "MHcCAQEEIF3rBqvGzYxHkQnJmLpNoRsTuVwXyZaBcDeFgHiJkLmNoAoGCCqGSM49",
        ),
        // Encrypted PKCS#8: SEQUENCE { SEQUENCE { OID … }, OCTET STRING }
        (
            "encrypted pkcs8",
            "ENCRYPTED PRIVATE KEY",
            "MIIFHDBOBgkqhkiG9w0BBQ0wQTApBgkqhkiG9w0BBQwwHAQIabcdefghijkKAgIIAA",
        ),
        // OpenSSH's own container.
        (
            "openssh",
            "OPENSSH PRIVATE KEY",
            "b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW",
        ),
    ];
    for (name, label, body) in cases {
        let text = format!("-----BEGIN {label}-----\n{body}\n-----END {label}-----\n");
        assert!(
            contains_accusable_key_block(&text),
            "{name} is real key material and did not accuse"
        );
    }
}
