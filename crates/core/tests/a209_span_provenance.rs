//! A209-C14 — span-end PROVENANCE, asserted at the source.
//!
//! Three rounds running, the defect was in the code the round itself
//! added, and every one had the same shape: a span end computed from
//! something other than observed payload. Round 12 ended a span at a
//! detection window; round 13 ended it at a terminator that turned out to
//! be a MENTION; round 14's near-miss put a mention on the banner's own
//! line. Each looked tighter than the default it replaced, and each
//! shipped a key.
//!
//! So the rule is made mechanical rather than remembered: **every span end
//! must come from observed payload, from a standalone banner line, or from
//! the end-of-field default.** Nothing else may reach it — not a mention,
//! not a heuristic guess, not a bound that merely looks tighter.
//!
//! This is a source-level assertion because that is the level the property
//! lives at. A behavioural test can only cover shapes someone thought of;
//! fourteen rounds say that is exactly what fails.

use std::collections::BTreeSet;

fn source() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/redact.rs"))
        .expect("read redact.rs")
}

/// The arms of `let last_line = match (end_after_payload, last_payload)`,
/// as raw right-hand-side expressions.
fn last_line_arms(src: &str) -> Vec<String> {
    let at = src
        .find("let last_line = match (end_after_payload, last_payload) {")
        .expect("the span-end match still exists");
    let rest = &src[at..];
    let close = rest.find("\n        };").expect("match closes");
    rest[..close]
        .lines()
        .filter_map(|l| {
            let t = l.trim();
            if t.starts_with("//") || !t.contains("=>") {
                return None;
            }
            t.split_once("=>")
                .map(|(_, rhs)| rhs.trim().trim_end_matches(',').to_string())
        })
        .collect()
}

/// Every expression a span end is allowed to come from, with the evidence
/// that licenses it.
fn permitted() -> BTreeSet<&'static str> {
    [
        // `end_after_payload` is only ever set from a STANDALONE banner
        // line that was reached AFTER payload was observed.
        "Some(e)",
        // `last_payload` is set only from a line whose base64 run was
        // observed to qualify.
        "Some(p)",
        // `terminator` is `terminating_line`, which accepts only
        // standalone banner lines.
        "terminator",
        // The safe default: we cannot see where the block ends.
        "None",
    ]
    .into_iter()
    .collect()
}

#[test]
fn every_span_end_comes_from_payload_a_standalone_banner_or_the_safe_default() {
    let src = source();
    let arms = last_line_arms(&src);
    assert!(
        arms.len() >= 3,
        "the span-end match was not parsed: {arms:?}"
    );
    let allowed = permitted();
    let rogue: Vec<&String> = arms
        .iter()
        .filter(|a| !allowed.contains(a.as_str()))
        .collect();
    assert!(
        rogue.is_empty(),
        "a span end is derived from an unlicensed source: {rogue:?}\n\
         Every arm must come from observed payload, a standalone banner, or None."
    );
}

/// `terminator` is licensed above ONLY because `terminating_line` accepts
/// standalone banner lines. If that ever weakens to any matching END, the
/// licence is void — that exact weakening is the A209-C13 leak.
#[test]
fn the_terminator_is_built_only_from_standalone_banner_lines() {
    let src = source();
    let at = src
        .find("fn terminating_line(")
        .expect("terminating_line exists");
    let body_end = src[at..].find("\n}\n").expect("function closes") + at;
    let body = &src[at..body_end];
    assert!(
        body.contains("standalone_end_banner("),
        "terminating_line no longer requires a standalone banner"
    );
    assert!(
        !body.contains("find_end_banner("),
        "terminating_line accepts a bare END match again — this is the A209-C13 leak"
    );
}

/// The same requirement inside span extension: a span may only be closed
/// by a standalone banner.
#[test]
fn span_extension_closes_only_on_a_standalone_banner() {
    let src = source();
    let at = src
        .find("let mut last_payload: Option<usize> = None;")
        .expect("span extension exists");
    let end = src[at..]
        .find("let last_line = match")
        .expect("extension precedes the span-end match")
        + at;
    let body = &src[at..end];
    let closes: Vec<&str> = body
        .lines()
        .filter(|l| l.contains("end_after_payload = Some("))
        .collect();
    assert_eq!(
        closes.len(),
        1,
        "expected exactly one place a span is closed, found {closes:?}"
    );
    // The guard above that assignment must be the standalone test.
    let guard_at = body
        .find("end_after_payload = Some(")
        .expect("the closing assignment");
    let preceding = &body[..guard_at];
    let last_cond = preceding
        .rfind("if ")
        .map(|i| &preceding[i..])
        .unwrap_or_default();
    assert!(
        preceding.contains("standalone_end_banner(lines[j], &banner.label)"),
        "span extension no longer requires a standalone banner to close a span"
    );
    assert!(
        !last_cond.contains("find_end_banner("),
        "a span is closed on a bare END match: {last_cond}"
    );
}

/// The assertion is not vacuous: mutating the source the way each of the
/// last three rounds did must be rejected.
#[test]
fn the_provenance_assertion_rejects_the_mutations_that_shipped_keys() {
    let src = source();
    let allowed = permitted();

    // A209-C13's actual regression, before the standalone restriction:
    // ending a span at whatever END was found first.
    let mutated = src.replace(
        "        if standalone_end_banner(line, label).is_some() {",
        "        if find_end_banner(line, label).is_some() {",
    );
    assert_ne!(mutated, src, "the terminator mutation did not apply");
    let at = mutated
        .find("fn terminating_line(")
        .expect("terminating_line");
    let body_end = mutated[at..].find("\n}\n").expect("closes") + at;
    assert!(
        mutated[at..body_end].contains("find_end_banner("),
        "control setup failed"
    );

    // An arm that ends a span at a bound with no evidence behind it —
    // the shape of round 12's window and round 13's mention.
    let rogue_arm = src.replace(
        "(None, None) => terminator,",
        "(None, None) => Some(i + 1),",
    );
    assert_ne!(rogue_arm, src, "the arm mutation did not apply");
    let arms = last_line_arms(&rogue_arm);
    let rejected = arms.iter().any(|a| !allowed.contains(a.as_str()));
    assert!(
        rejected,
        "an unlicensed span end was accepted; the assertion is vacuous: {arms:?}"
    );
}

/// A209-C15 — the behavioural companion, and an admission about the
/// assertions above.
///
/// Every control in this file is a SOURCE-TEXT assertion. They prove that
/// terminator selection is *spelled* with `standalone_end_banner`, and
/// they passed unchanged through both round-15 defects: an ANSI colour on
/// an END line cost a whole key its detection, and an all-caps mention
/// became a terminator. Non-vacuous for their class, blind to that one.
///
/// The property that actually failed is behavioural and belongs here
/// beside them: **a decoration on the END line may cost precision, but it
/// must never cost DETECTION.**
#[test]
fn a_decorated_end_line_never_costs_detection() {
    // A real PKCS#8 Ed25519 key, wrapped narrow enough that no payload
    // heuristic can rescue it: detection has to come from structure.
    const BODY: &str = "MC4CAQAwBQYDK2VwBCIEIB670Y9rX2WANBNk9dFSpprFWswqdWcQOVqJPKSa4q3c";
    let wrapped: Vec<String> = BODY
        .as_bytes()
        .chunks(31)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect();
    let decorations = [
        ("ansi cyan", "\u{1b}[36m", "\u{1b}[0m"),
        ("ansi bold", "\u{1b}[1m", "\u{1b}[0m"),
        ("xml element", "<line>", "</line>"),
        ("html span", "<span class=\"k\">", "</span>"),
        ("latex", "\\texttt{", "}"),
        ("stdout tag", "[stdout] ", ""),
        ("docker compose", "web_1  | ", ""),
        ("journalctl", "Sep 01 12:00:00 host app[123]: ", ""),
        (
            "windows event log",
            "Information\t9/1/2026 12:00:00 PM\tApp\t",
            "",
        ),
        ("csv", "12,\"", "\",ok"),
        ("head with the word end", "at end: ", ""),
        ("kubectl", "[pod/app-7f8d9/web] ", ""),
        ("ansi 256", "\u{1b}[38;5;45m", "\u{1b}[0m"),
    ];
    let mut failures = Vec::new();
    for (name, pre, post) in decorations {
        let doc = format!(
            "-----BEGIN PRIVATE KEY-----\n{}\n{pre}-----END PRIVATE KEY-----{post}",
            wrapped.join("\n")
        );
        let (out, rec) = thruline_core::redact::mask_recorded(&doc);
        let shipped = (0..=(BODY.len() - 24)).any(|i| out.contains(&BODY[i..i + 24]));
        if !rec.families.contains("private_key_block") || shipped {
            failures.push(format!(
                "  {name}: fired={:?} shipped={shipped}",
                rec.families
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} decorated END lines cost detection:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
