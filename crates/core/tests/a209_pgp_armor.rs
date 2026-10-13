//! A209-C7 — the nine armored-key shapes that leaked at `53046ae`.
//!
//! Provenance and the reason this file exists are in
//! `fixtures/pgp_armor_shapes.md`. In short: round 7's B8-1 guard named
//! `gpg --export-secret-keys --armor` as the format under test and then
//! built every case as banner/body/banner with **no armor header** — the
//! one shape its own new gate accepted — and the live-proof fixture put an
//! RFC 1421 header inside a PGP armor block, a combination gpg has never
//! emitted. Fixture and rule were built in the same session and the
//! fixture drifted toward the rule.
//!
//! **This file was written before the rule was adjusted.** Every case here
//! was verified FAILING against `53046ae` first.

use thruline_core::redact::mask_recorded;

/// 64-character wraps, the real width `gpg --armor` emits.
const W1: &str = "lQOYBGqWSHUBCADeBlzk6F0imx97hU2oK86MTLCANARYaaZZ11bbYY22ccXX33dd";
const W2: &str = "2Xq/VJNjU4gRTQdhmuP2Fn/O6BG4TtxYwi8ODR0hNKM72A8b8oh5MzROk7HhZtVI";
/// The final short wrap and the CRC-24 checksum line real armor ends with.
const WLAST: &str = "B+9dJkzPZvmxbSbS/FdAGVXTocJ25S1NL9Qq5h+mTLCANARY";
const CRC: &str = "=Pc+N";

/// Shape 1 — modern gpg (2.5.x): banner, BLANK line, body, CRC, END.
/// Line structure verbatim from a real run; payload canary-substituted.
fn modern_armor() -> String {
    format!("-----BEGIN PGP PRIVATE KEY BLOCK-----\n\n{W1}\n{W2}\n{WLAST}\n{CRC}\n-----END PGP PRIVATE KEY BLOCK-----\n")
}

/// Shape 2 — legacy gpg (1.x / 2.0.x): the RFC 4880 §6.2 `Version:` header.
fn legacy_armor() -> String {
    format!("-----BEGIN PGP PRIVATE KEY BLOCK-----\nVersion: GnuPG v2.0.22 (GNU/Linux)\n\n{W1}\n{W2}\n{WLAST}\n{CRC}\n-----END PGP PRIVATE KEY BLOCK-----\n")
}

/// Shape 3 — the other RFC 4880 headers.
fn commented_armor() -> String {
    format!("-----BEGIN PGP PRIVATE KEY BLOCK-----\nVersion: GnuPG v1\nComment: https://www.gnupg.org\nMessageID: abcdefghijklmnopqrstuvwx\nCharset: utf-8\n\n{W1}\n{W2}\n{CRC}\n-----END PGP PRIVATE KEY BLOCK-----\n")
}

/// Every shape, named, so a failure says which one.
fn shapes() -> Vec<(&'static str, String)> {
    vec![
        ("1 modern gpg armor", modern_armor()),
        ("2 legacy gpg armor (Version:)", legacy_armor()),
        ("3 armor with Comment/MessageID/Charset", commented_armor()),
        (
            "4 one-line PEM, no whitespace before payload",
            format!("-----BEGIN RSA PRIVATE KEY-----{W1}{W2}-----END RSA PRIVATE KEY-----"),
        ),
        (
            "5 JSON with \\n escapes (service-account key file)",
            format!(
                r#"{{"type":"service_account","private_key":"-----BEGIN PRIVATE KEY-----\n{W1}\n{W2}\n-----END PRIVATE KEY-----\n"}}"#
            ),
        ),
        (
            "6 prose before the body inside the block",
            format!("-----BEGIN RSA PRIVATE KEY-----\nthe key follows\n{W1}\n{W2}\n-----END RSA PRIVATE KEY-----"),
        ),
        (
            "7 short first wrap then a full line",
            format!("-----BEGIN RSA PRIVATE KEY-----\nlQOYBGqWSHUBCADeBlzk6F0imx9\n{W2}\n-----END RSA PRIVATE KEY-----"),
        ),
        (
            "8 gutter-marked diff deletion",
            format!("-----BEGIN RSA PRIVATE KEY-----\n-{W1}\n-{W2}\n-----END RSA PRIVATE KEY-----"),
        ),
        (
            "9 RSA PEM with RFC 1421 headers",
            format!("-----BEGIN RSA PRIVATE KEY-----\nProc-Type: 4,ENCRYPTED\nDEK-Info: AES-128-CBC,ABC\n\n{W1}\n{W2}\n-----END RSA PRIVATE KEY-----"),
        ),
    ]
}

#[test]
fn every_real_armored_key_shape_is_masked() {
    let mut failures = Vec::new();
    for (name, input) in shapes() {
        let (out, record) = mask_recorded(&input);
        if !record.families.contains("private_key_block") {
            failures.push(format!("{name}: no detection"));
            continue;
        }
        if out.contains("TLCANARY") {
            failures.push(format!("{name}: key material survived -> {out}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of 9 armored-key shapes leaked:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The other direction, on the same rule: prose and scripts that merely
/// mention a banner must still survive. These are the shapes whose live
/// count the window scan must not regress.
#[test]
fn prose_and_scripts_mentioning_a_banner_still_survive() {
    for benign in [
        "grep -rn -- '-----BEGIN PRIVATE KEY-----' src/",
        "// the file starts with -----BEGIN RSA PRIVATE KEY----- and is PEM",
        "-----BEGIN CHANGELOG----- notes about PRIVATE KEY handling -----END CHANGELOG----- keep this prose",
        "if rg -n -- '-----BEGIN PRIVATE KEY-----' .; then exit 1; fi\n\
         if rg -n '^\\s*schedule:' .github/workflows; then exit 1; fi\n\
         SHA=da39a3ee5e6b4b0d3255bfef95601890afd80709aabbccddeeff0011\n",
        "pattern='-----BEGIN ([A-Z0-9 ]+ )?PRIVATE KEY-----'\n\
         developer_path_pattern='(^|[^A-Za-z0-9._-])/(Users|home)/[[:alnum:]_.-]+/'\n\
         digest=e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n",
        "-----BEGIN CERTIFICATE-----\nMIICabcdefghijklmnopqrst",
    ] {
        let (out, record) = mask_recorded(benign);
        assert_eq!(
            record.masked, 0,
            "prose masked as a key: {benign:?} -> {out}"
        );
        assert_eq!(out, benign, "content destroyed");
    }
}

/// RFC 4880 §6.2 headers must be recognised as armor headers, not as the
/// deciding content line. The disjointness from RFC 1421 is the whole
/// cause of shape 2's leak.
#[test]
fn rfc4880_armor_headers_do_not_decide_the_block() {
    for header in [
        "Version: GnuPG v2.0.22 (GNU/Linux)",
        "Comment: https://www.gnupg.org",
        "MessageID: abcdefghijklmnopqrstuvwx",
        "Hash: SHA256",
        "Charset: utf-8",
    ] {
        let input =
            format!("-----BEGIN PGP PRIVATE KEY BLOCK-----\n{header}\n\n{W1}\n{W2}\n-----END PGP PRIVATE KEY BLOCK-----");
        let (out, record) = mask_recorded(&input);
        assert!(
            record.families.contains("private_key_block") && !out.contains("TLCANARY"),
            "header {header:?} defeated the detector: {out}"
        );
    }
}

/// A transcript round-trips tool input through JSON, so a service-account
/// key file arrives with `\n` escaped **twice**. Measured live at
/// A209-C7: the single-escape form masked and the double-escape form
/// returned masked=0 on a real Google service-account key.
#[test]
fn an_escaped_newline_at_any_depth_is_a_line_break() {
    let body = W1;
    for (name, input) in [
        (
            "single escape",
            format!("-----BEGIN PGP PRIVATE KEY BLOCK-----\\n\\n{body}\\n"),
        ),
        (
            "double escape (JSON round-trip)",
            format!("-----BEGIN PGP PRIVATE KEY BLOCK-----\\\\n\\\\n{body}\\\\n"),
        ),
        (
            "double escape, quoted",
            format!("\"-----BEGIN PGP PRIVATE KEY BLOCK-----\\\\n\\\\n{body}\\\\n\""),
        ),
        (
            "service-account file shape",
            format!(
                r#"echo '{{"type": "service_account", "private_key": "-----BEGIN PRIVATE KEY-----\\n\\n{body}\\n{W2}\\n-----END PRIVATE KEY-----\\n"}}' > /tmp/sa.json"#
            ),
        ),
    ] {
        let (out, record) = mask_recorded(&input);
        assert!(
            record.families.contains("private_key_block"),
            "{name}: no detection -> {out}"
        );
        assert!(
            !out.contains("TLCANARY"),
            "{name}: key material survived: {out}"
        );
    }
}
