//! A209-C16 — two more instances of the named pattern, and the pattern
//! made mechanical.
//!
//! Three rounds running the defect has been the same class: **one
//! predicate serving two consumers that disagree about which direction is
//! safe when it is wrong.** Round 16 split `head_is_decoration` on exactly
//! that ground. The crux dimension then found two more instances, both
//! exploitable, which is what says the class is enumerable rather than
//! mysterious.
//!
//! * `find_end_banner` — safe when it fires inside `any_end_banner`
//!   (detection), UNSAFE when it fires as a bare same-line span bound.
//! * `first_key_banner` — safe when it fires to open a block, UNSAFE
//!   inside `any_end_banner`, where firing `return false` **suppresses
//!   detection**. Note the inverted polarity: there it is RECOGNISING
//!   that leaks, not failing to recognise.
//!
//! Every assertion below checks for **surviving key material**, never for
//! the presence of a mask marker. Each of these leaks reports
//! `masked >= 1`, so a marker-based check calls them successes.
//!
//! Each control was verified FAILING against `542a18d` first, in an
//! isolated `CARGO_TARGET_DIR`.

mod keymat;

use keymat::{body_of, rewrap_pem, ED25519, X25519};
use thruline_core::redact::mask_recorded;

/// Does any 24-character window of `body` survive in `out`?
fn ships(out: &str, body: &str) -> bool {
    body.len() >= 24 && (0..=(body.len() - 24)).any(|i| out.contains(&body[i..i + 24]))
}

fn one_line(pem: &str) -> String {
    let lines: Vec<&str> = pem.lines().collect();
    format!(
        "{} {} {}",
        lines[0],
        lines[1..lines.len() - 1].join(" "),
        lines[lines.len() - 1]
    )
}

/// BLOCKING 1: a bare same-line END match bounded the span, so everything
/// after it on the same physical line was abandoned — including a second
/// complete key.
#[test]
fn a_second_key_on_the_same_line_is_never_left_behind() {
    let a = body_of(ED25519);
    let b = body_of(X25519);
    let cases = [
        (
            "kubernetes secret",
            format!(
                "data: tls.key={} ca.key={}",
                one_line(ED25519),
                one_line(X25519)
            ),
        ),
        (
            "docker-compose environment",
            format!(
                "environment: KEY_A={} KEY_B={}",
                one_line(ED25519),
                one_line(X25519)
            ),
        ),
        (
            "one-line json secrets dump",
            format!(
                "{{\"a\":\"{}\",\"b\":\"{}\"}}",
                one_line(ED25519),
                one_line(X25519)
            ),
        ),
        (
            "sed range then output",
            format!(
                "sed -n '/-----BEGIN PRIVATE KEY-----/,/-----END PRIVATE KEY-----/p' k.pem  {}  {}",
                one_line(ED25519),
                one_line(X25519)
            ),
        ),
    ];
    let mut failures = Vec::new();
    for (name, doc) in cases {
        let (out, _rec) = mask_recorded(&doc);
        if ships(&out, &a) {
            failures.push(format!("  {name}: first key shipped"));
        }
        if ships(&out, &b) {
            failures.push(format!("  {name}: SECOND key shipped"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} one-line key pairs leaked:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// `cat a.pem b.pem` where the first file has no trailing newline: key 1's
/// END lands on the same physical line as key 2's BEGIN.
#[test]
fn concatenated_pem_files_without_a_trailing_newline_both_mask() {
    let mut failures = Vec::new();
    for w in [24usize, 28, 32, 36, 64] {
        let first = rewrap_pem(ED25519, w);
        let second = rewrap_pem(X25519, w);
        // No newline between the files: the banners abut.
        let doc = format!("{first}{second}");
        let (out, _rec) = mask_recorded(&doc);
        if ships(&out, &body_of(ED25519)) {
            failures.push(format!("  w={w}: first key shipped"));
        }
        if ships(&out, &body_of(X25519)) {
            failures.push(format!("  w={w}: second key shipped"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} concatenated-PEM shapes leaked:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// BLOCKING 2: a BEGIN banner between payload and the matching END used to
/// veto detection outright. Recognising the banner is what leaked.
#[test]
fn a_begin_mention_between_payload_and_end_does_not_veto_detection() {
    let mut failures = Vec::new();
    for (kname, pem) in [("ed25519", ED25519), ("x25519", X25519)] {
        let body = body_of(pem);
        for w in [24usize, 28, 32, 36] {
            for mention in [
                "see -----BEGIN PRIVATE KEY----- for the format",
                "-----BEGIN PRIVATE KEY-----",
            ] {
                let wrapped = rewrap_pem(pem, w);
                let lines: Vec<&str> = wrapped.lines().collect();
                let doc = format!(
                    "{}\n{}\n{mention}\n{}",
                    lines[0],
                    lines[1..lines.len() - 1].join("\n"),
                    lines[lines.len() - 1]
                );
                let (out, _rec) = mask_recorded(&doc);
                if ships(&out, &body) {
                    failures.push(format!(
                        "  {kname} w={w}: `{}`",
                        &mention[..20.min(mention.len())]
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} BEGIN-mention shapes vetoed detection and shipped a key:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The measurement hazard itself, pinned: every leak above reports a mask.
/// A census that counts a mask marker as a success is blind to all of them.
#[test]
fn a_marker_is_not_evidence_that_a_key_was_masked() {
    let doc = format!(
        "data: tls.key={} ca.key={}",
        one_line(ED25519),
        one_line(X25519)
    );
    let (out, rec) = mask_recorded(&doc);
    // Whatever the marker says, the material is the evidence.
    if rec.masked > 0 {
        assert!(
            !ships(&out, &body_of(ED25519)) && !ships(&out, &body_of(X25519)),
            "a mask marker was reported while key material survived — the \
             census metric that accepts this is structurally blind"
        );
    }
}

// ---------------------------------------------------------------------
// The pattern, made mechanical.
//
// Three rounds produced the same defect, so it is enumerated rather than
// remembered. `PREDICATE_CONSUMERS` declares every call site of every
// safety-critical predicate and the direction it must err in. This gate
// asserts the declaration matches the source exactly, and that no
// predicate carries consumers that disagree.
// ---------------------------------------------------------------------

use std::collections::BTreeMap;
use thruline_core::redact::PREDICATE_CONSUMERS;

/// The module source with `#[cfg(test)]` blocks removed by brace depth.
fn production_source() -> String {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/redact.rs"))
        .expect("read redact.rs");
    let mut out = String::new();
    let mut rest = src.as_str();
    while let Some(at) = rest.find("#[cfg(test)]") {
        out.push_str(&rest[..at]);
        let after = &rest[at..];
        let Some(open) = after.find('{') else { break };
        let mut depth = 0usize;
        let mut end = open;
        for (k, c) in after[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + k + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// Every `(predicate, enclosing function)` call site actually present.
fn observed_sites(preds: &[&str]) -> BTreeMap<(String, String), usize> {
    let src = production_source();
    let mut found: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut current = String::from("<top>");
    for line in src.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("fn ").or_else(|| t.strip_prefix("pub fn ")) {
            current = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
        }
        if t.starts_with("//") || t.starts_with("///") {
            continue;
        }
        for p in preds {
            if t.starts_with(&format!("fn {p}")) || t.starts_with(&format!("pub fn {p}")) {
                continue;
            }
            let needle = format!("{p}(");
            let mut from = 0usize;
            while let Some(rel) = line[from..].find(&needle) {
                let at = from + rel;
                let boundary = at == 0
                    || !line.as_bytes()[at - 1].is_ascii_alphanumeric()
                        && line.as_bytes()[at - 1] != b'_';
                if boundary {
                    *found.entry((p.to_string(), current.clone())).or_insert(0) += 1;
                }
                from = at + needle.len();
            }
        }
    }
    found
}

#[test]
fn every_safety_critical_call_site_is_declared() {
    let preds: Vec<&str> = {
        let mut v: Vec<&str> = PREDICATE_CONSUMERS.iter().map(|e| e.0).collect();
        v.sort_unstable();
        v.dedup();
        v
    };
    let observed = observed_sites(&preds);
    let declared: BTreeMap<(String, String), usize> = PREDICATE_CONSUMERS
        .iter()
        .map(|(p, c, n, _, _)| ((p.to_string(), c.to_string()), *n))
        .collect();

    let mut problems = Vec::new();
    for (k, n) in &observed {
        match declared.get(k) {
            None => problems.push(format!(
                "  UNDECLARED call site: {}() inside {}() x{n}",
                k.0, k.1
            )),
            Some(d) if d != n => problems.push(format!(
                "  COUNT CHANGED: {}() inside {}() — declared {d}, found {n}",
                k.0, k.1
            )),
            _ => {}
        }
    }
    for k in declared.keys() {
        if !observed.contains_key(k) {
            problems.push(format!("  DECLARED BUT ABSENT: {}() inside {}()", k.0, k.1));
        }
    }
    assert!(
        problems.is_empty(),
        "the predicate-consumer register no longer matches the source:\n{}\n\n\
         Every new call site of a safety-critical predicate must be classified — \
         that classification is what three rounds of defects skipped.",
        problems.join("\n")
    );
}

/// Directions must not conflict. This is the class itself: a predicate
/// whose consumers disagree about which way is safe.
fn conflicts(table: &[(&str, &str, usize, &str, &str)]) -> Vec<String> {
    let mut by_pred: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
    for (p, c, _, dir, _) in table {
        by_pred.entry(p).or_default().push((c, dir));
    }
    let mut out = Vec::new();
    for (p, consumers) in by_pred {
        let has_over = consumers.iter().any(|(_, d)| *d == "over");
        let has_under = consumers.iter().any(|(_, d)| *d == "under");
        if has_over && has_under {
            out.push(format!("  {p}: consumers disagree — {consumers:?}"));
        }
        if consumers.len() > 1 && consumers.iter().any(|(_, d)| *d == "single") {
            out.push(format!(
                "  {p}: declared `single` but has {} consumers",
                consumers.len()
            ));
        }
    }
    out
}

#[test]
fn no_predicate_has_consumers_that_disagree_about_safety() {
    let found = conflicts(PREDICATE_CONSUMERS);
    assert!(
        found.is_empty(),
        "a safety-critical predicate is serving consumers with opposite \
         safety requirements — split it, as A209-C15 split head_is_decoration:\n{}",
        found.join("\n")
    );
}

/// The gate is not vacuous: both of round 16's defects are exactly this
/// conflict, and it detects them.
#[test]
fn the_agreement_gate_detects_both_round_16_defects() {
    // `find_end_banner` as it stood at 542a18d: safe when it fires in
    // detection, unsafe as a bare same-line span bound.
    let as_at_542a18d: &[(&str, &str, usize, &str, &str)] = &[
        (
            "find_end_banner",
            "any_end_banner",
            1,
            "over",
            "firing detects",
        ),
        (
            "find_end_banner",
            "key_spans",
            1,
            "under",
            "bare same-line bound with no payload test and no rescan",
        ),
    ];
    assert!(
        !conflicts(as_at_542a18d).is_empty(),
        "the gate does not detect the find_end_banner disagreement"
    );

    // `first_key_banner`: safe when it opens a block, and a detection
    // veto when it fires inside any_end_banner.
    let veto: &[(&str, &str, usize, &str, &str)] = &[
        (
            "first_key_banner",
            "key_spans",
            2,
            "over",
            "firing opens a block",
        ),
        (
            "first_key_banner",
            "any_end_banner",
            1,
            "under",
            "firing vetoes detection",
        ),
    ];
    assert!(
        !conflicts(veto).is_empty(),
        "the gate does not detect the first_key_banner veto disagreement"
    );

    // And an undeclared site must be caught: a predicate the register
    // does not mention at all.
    let observed = observed_sites(&["mask_pem_blocks"]);
    assert!(
        !observed.is_empty(),
        "the site scanner found nothing; it would accept any new call site"
    );
}
