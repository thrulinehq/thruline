//! A209-C15 — decorated END lines, and the all-caps false terminator.
//!
//! Round 15 made one predicate do two jobs with opposite safety
//! requirements. `head_is_decoration` was consumed by the end-of-field
//! fall-through — where failing to recognise a decoration over-masks, and
//! is safe — AND by `terminating_line`, whose result sets `terminated`,
//! which **gates detection**. On that second path an unrecognised
//! decoration does not over-mask: it strips the block of terminated
//! status and demotes it into the payload heuristic, whose failure set is
//! the disclosed residual. The failure mode is *not detected at all*.
//!
//! So the fail-safe claim A209-C14 made was structurally false, and these
//! controls are the measurement that says so: a key with a matching END
//! masks clean at every width, until the END line alone is wrapped in an
//! ANSI colour — what `bat`, `grep --color` and any coloured CI log emit —
//! and then the whole body ships at `masked=0`.
//!
//! The second family is the ruling's own hole, and mine for implementing
//! it as "no lowercase survives": that accepts every shouted sentence, so
//! a MENTION of the END banner inside an upper-case sentence became a
//! standalone terminator. `WARNING: DO NOT COMMIT ANYTHING THROUGH
//! -----END PRIVATE KEY-----` ships every payload line verbatim under a
//! `[masked: private_key_block]` marker. That is A209-C13's leak
//! resurrected, differing only in letter case — and every mention control
//! shipped before this file is lowercase.
//!
//! Each control was verified FAILING against `9bf1a00` first.

mod keymat;

use keymat::{body_of, rewrap_pem, EC256_PKCS8, ED25519, OPENSSH_ED25519, REAL_RSA_KEY, X25519};
use thruline_core::redact::mask_recorded;

fn key_types() -> Vec<(&'static str, &'static str)> {
    vec![
        ("pkcs8 ed25519", ED25519),
        ("pkcs8 x25519", X25519),
        ("ec p-256 pkcs8", EC256_PKCS8),
        ("openssh ed25519", OPENSSH_ED25519),
    ]
}

/// Decorations applied to the END LINE ONLY. Every one is what some real
/// tool emits around a line it is printing.
fn end_decorations() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("ansi cyan", "\u{1b}[36m", "\u{1b}[0m"),
        ("ansi bold", "\u{1b}[1m", "\u{1b}[0m"),
        ("ansi 256-colour", "\u{1b}[38;5;45m", "\u{1b}[0m"),
        ("journalctl", "Sep 01 12:00:00 host app[123]: ", ""),
        (
            "windows event log",
            "Information\t9/1/2026 12:00:00 PM\tApp\t",
            "",
        ),
        ("xml line element", "<line>", "</line>"),
        ("html span", "<span class=\"k\">", "</span>"),
        ("latex texttt", "\\texttt{", "}"),
        ("stdout tag", "[stdout] ", ""),
        ("docker compose", "web_1  | ", ""),
        ("kubectl multi-pod", "[pod/app-7f8d9/web] ", ""),
        ("csv row", "12,\"", "\",ok"),
        ("head containing the word end", "at end: ", ""),
    ]
}

/// A key block whose END line carries a decoration.
fn decorated_end(pem: &str, w: usize, pre: &str, post: &str) -> String {
    let wrapped = rewrap_pem(pem, w);
    let lines: Vec<&str> = wrapped.lines().collect();
    let mut out: Vec<String> = lines[..lines.len() - 1]
        .iter()
        .map(|s| s.to_string())
        .collect();
    out.push(format!("{pre}{}{post}", lines[lines.len() - 1]));
    out.join("\n")
}

fn ships(out: &str, body: &str) -> bool {
    body.len() >= 24 && (0..=(body.len() - 24)).any(|i| out.contains(&body[i..i + 24]))
}

/// BLOCKING 1: a decoration on the END line must never cost DETECTION.
#[test]
fn a_decorated_end_line_never_costs_detection() {
    let mut failures = Vec::new();
    let mut checked = 0usize;
    for (kname, pem) in key_types() {
        let body = body_of(pem);
        for (dname, pre, post) in end_decorations() {
            for w in [24usize, 31, 39, 48, 55, 63] {
                let doc = decorated_end(pem, w, pre, post);
                checked += 1;
                let (out, rec) = mask_recorded(&doc);
                if !rec.families.contains("private_key_block") || ships(&out, &body) {
                    failures.push(format!("  {kname} @ {w} + {dname}"));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {checked} (key x END-decoration x width) shapes leaked:\n{}",
        failures.len(),
        failures
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The narrowest statement of the same defect: an otherwise pristine
/// 64-wrap key, with nothing changed but an ANSI colour around its END.
#[test]
fn an_ansi_coloured_end_on_a_clean_key_is_still_masked() {
    let body = body_of(REAL_RSA_KEY);
    let doc = decorated_end(REAL_RSA_KEY, 64, "\u{1b}[36m", "\u{1b}[0m");
    let (out, rec) = mask_recorded(&doc);
    assert!(
        rec.families.contains("private_key_block"),
        "an ANSI colour on the END line cost detection entirely: {rec:?}"
    );
    assert!(!ships(&out, &body), "the body shipped: {out}");
}

/// BLOCKING 2: an all-caps sentence is prose, not decoration, so a
/// MENTION inside one must never become a terminator.
#[test]
fn an_all_caps_mention_is_not_a_terminator() {
    let heads = [
        "WARNING: DO NOT COMMIT ANYTHING THROUGH ",
        "ERROR: EXPECTED ",
        "TODO REMOVE BLOCK ENDING AT ",
        "SEE ",
        "PASTE EVERYTHING THROUGH ",
        "NOTE: THE BLOCK ENDS WITH ",
    ];
    let mut failures = Vec::new();
    for (kname, pem) in key_types().into_iter().chain([("rsa-2048", REAL_RSA_KEY)]) {
        let body = body_of(pem);
        let lines: Vec<&str> = pem.lines().collect();
        for head in heads {
            let doc = format!(
                "{}\n{head}-----END PRIVATE KEY-----\nfor example:\n{}\n",
                lines[0],
                lines[1..lines.len() - 1].join("\n")
            );
            let (out, _rec) = mask_recorded(&doc);
            if ships(&out, &body) {
                failures.push(format!("  {kname}: `{head}`"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} all-caps mention shapes shipped a key body:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The negative side, so the fix cannot be "mask everything". Text with no
/// key banner at all is untouched, and a field discussing keys in capitals
/// cannot reach beyond its own field.
#[test]
fn prose_without_a_banner_is_untouched_and_masking_stays_within_its_field() {
    let plain = "WARNING: DO NOT COMMIT PRIVATE KEYS. ROTATE THEM QUARTERLY.";
    let (out, rec) = mask_recorded(plain);
    assert_eq!(rec.masked, 0, "prose with no banner was masked: {out}");
    assert_eq!(out, plain);

    // A second field is a separate call and cannot be reached by the
    // first field's span, however far that span runs.
    let discussing = "WARNING: EVERYTHING THROUGH -----END PRIVATE KEY----- IS SECRET";
    let (_o1, _r1) = mask_recorded(discussing);
    let neighbour = "the deploy step reads config from env";
    let (o2, r2) = mask_recorded(neighbour);
    assert_eq!(r2.masked, 0);
    assert_eq!(o2, neighbour, "a neighbouring field was affected");
}

/// An ANSI sequence immediately followed by multi-byte UTF-8.
///
/// The decoder's escape scan can stop mid-character, and slicing there
/// panics. This was found by running the live ledger, not by any suite —
/// the corpus above is all ASCII. A panic in `crates/core` is a bug by
/// project invariant, not merely a failed mask.
#[test]
fn an_ansi_sequence_beside_multibyte_text_does_not_panic() {
    for text in [
        "\u{1b}[36m→ key material follows\u{1b}[0m",
        "\u{1b}[1m—\u{1b}[0m -----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2Vw\n",
        "\u{1b}[38;5;45m日本語のログ\u{1b}[0m -----END PRIVATE KEY-----",
        "\u{1b}[m\u{e9}\u{1b}",
        "\u{1b}]0;título\u{7}ok",
    ] {
        let (out, _rec) = mask_recorded(text);
        assert!(!out.is_empty() || text.is_empty());
    }
}
