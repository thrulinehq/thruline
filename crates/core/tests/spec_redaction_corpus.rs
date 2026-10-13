//! BUILD-SPEC §4.6 — the redaction regression gate.
//!
//! Seventeen rounds (A209, A209-C … A209-C15, `docs/P0-BUILD-BRIEF.md:2662-2903`)
//! discovered the secret-leak class one member at a time, each round finding its
//! next member by construction rather than by anticipation. This corpus encodes
//! **every generator those rounds named**, so a redaction implementation meets
//! seventeen rounds of knowledge on the day it is written rather than
//! rediscovering them over seventeen more.
//!
//! Provenance of every family is in
//! `tests/fixtures/redaction-corpus/README.md`. The corpus itself is generated
//! and committed, never minted at test time: a corpus that mints fresh keys is
//! not a regression baseline.
//!
//! ## The contract
//!
//! * `expect: mask`   — **no 24-character window of any string in `secrets`**
//!   may survive. Sliding windows over the whole payload, never the first line:
//!   A209-C11 records that every prior control asserted the *first* payload line
//!   was absent while both of that round's regressions were at the body's **end**.
//! * `expect: nomask` — the text must survive **byte-identical**. The over-mask
//!   direction is a real harm: A209-C3 measured a PEM repair destroying 104 of
//!   240 characters of a real opening ask across 70 live sessions.
//!
//! Presence or absence of a `[masked: …]` marker is **never** consulted.
//! A209-C4 B1 found forged markers live in the operator's ledger, and
//! A209-C13 found a reported mask sitting over a shipped key — "a reported mask
//! over a shipped key is worse than no mask".
//!
//! ## Why the gate is an identity set and not a count
//!
//! 210 of the 1,958 rows fail against the shipped redactor, and every one is an
//! explicitly disclosed residual (see `expected-failures.txt`). A count-only
//! assertion would let a **new leak swap in for a fixed residual** and stay
//! green. The assertion is therefore on the exact set, and a newly-failing
//! must-mask row is reported separately from a newly-passing one, because those
//! two directions are not equally urgent.

use std::collections::BTreeSet;
use std::path::PathBuf;

use thruline_core::redact;

const WINDOW: usize = 24;

/// Pinned so the gate cannot be weakened by editing its own fixture.
/// A209-C3 BLOCKING 4 is the case where the gate protecting the previous fix
/// was satisfiable by a comment; a gate whose corpus is freely editable is the
/// same fault one layer down.
const CORPUS_SHA256: &str = "1d0b40e4429f46e1bb850c2f526c71e3ba4ded851aa857ff04affc1c96e75e77";
const EXPECTED_SHA256: &str = "8640101b6277d20d15b9fa90992ba6d973970bec0a06168b05c322c5b526856b";

const TOTAL_ROWS: usize = 1958;
const DISCLOSED_RESIDUALS: usize = 210;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/redaction-corpus")
}

fn read(name: &str) -> String {
    let p = fixture_dir().join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// True when any `WINDOW`-character window of `needle` occurs in `hay`.
/// Character-wise, not byte-wise: the corpus carries CJK and accented Latin
/// beside ANSI escapes (A209-C15's panic control).
fn leaks(hay: &str, needle: &str) -> bool {
    let n: Vec<char> = needle.chars().collect();
    if n.len() < WINDOW {
        return hay.contains(needle);
    }
    (0..=n.len() - WINDOW).any(|i| {
        let w: String = n[i..i + WINDOW].iter().collect();
        hay.contains(&w)
    })
}

struct Row {
    family: String,
    expect: String,
    text: String,
    secrets: Vec<String>,
}

fn corpus() -> Vec<Row> {
    read("corpus.jsonl")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).expect("corpus row is JSON");
            Row {
                family: v["family"].as_str().expect("family").to_string(),
                expect: v["expect"].as_str().expect("expect").to_string(),
                text: v["text"].as_str().expect("text").to_string(),
                secrets: v["secrets"]
                    .as_array()
                    .expect("secrets")
                    .iter()
                    .map(|s| s.as_str().expect("secret").to_string())
                    .collect(),
            }
        })
        .collect()
}

/// Evaluate one row against a masking function. Returns true on PASS.
fn evaluate(row: &Row, mask: impl Fn(&str) -> String) -> bool {
    let out = mask(&row.text);
    match row.expect.as_str() {
        "mask" => !row.secrets.iter().any(|s| leaks(&out, s)),
        "nomask" => out == row.text,
        other => panic!("unknown expect `{other}` on {}", row.family),
    }
}

fn failures(rows: &[Row], mask: impl Fn(&str) -> String + Copy) -> BTreeSet<String> {
    rows.iter()
        .filter(|r| !evaluate(r, mask))
        .map(|r| r.family.clone())
        .collect()
}

#[test]
fn the_corpus_fixtures_are_the_ones_this_gate_was_calibrated_against() {
    for (name, want) in [
        ("corpus.jsonl", CORPUS_SHA256),
        ("expected-failures.txt", EXPECTED_SHA256),
    ] {
        let got = thruline_core::chain::sha256_hex(read(name).as_bytes());
        assert_eq!(
            got, want,
            "{name} has changed. That is allowed, but it is a RE-CALIBRATION, not a \
             refactor: re-run the corpus against the redactor, re-derive \
             expected-failures.txt, and update both pins in this file with the \
             measurement in the commit message."
        );
    }
}

#[test]
fn the_corpus_is_shaped_as_the_gate_expects() {
    let rows = corpus();
    assert_eq!(rows.len(), TOTAL_ROWS, "corpus row count");
    let families: BTreeSet<&str> = rows.iter().map(|r| r.family.as_str()).collect();
    assert_eq!(
        families.len(),
        rows.len(),
        "family names must be unique — the expected-failure set is keyed by them"
    );
    // Every must-mask row must actually carry a secret, or it passes vacuously.
    for r in rows.iter().filter(|r| r.expect == "mask") {
        assert!(
            !r.secrets.is_empty(),
            "{} is a must-mask row with no secret to look for",
            r.family
        );
        for s in &r.secrets {
            // Secrets shorter than the window (an AWS access key id is exactly
            // 20 characters) fall back to exact containment in `leaks`, which is
            // strictly stronger, not weaker. What must hold for every row is that
            // the secret is findable in the row's OWN text — otherwise the row
            // passes no matter what the masker does.
            assert!(
                leaks(&r.text, s),
                "{}: the secret is not present in the row's own text — this row \
                 cannot detect a leak",
                r.family
            );
        }
    }
}

/// The control that proves the harness can fail.
///
/// A209-C4 B6, A209-C5 (1) and A209-C6 (4) are three consecutive rounds whose
/// controls tested an easier case than the defect, or asserted a property the
/// arm could not reach. A gate that has never been observed failing is an
/// assertion about itself.
#[test]
fn the_gate_detects_a_leak_when_masking_does_nothing() {
    let rows = corpus();
    let leaked = failures(&rows, |t| t.to_string()); // identity: mask nothing
    let must_mask = rows.iter().filter(|r| r.expect == "mask").count();
    assert_eq!(
        leaked.len(),
        must_mask,
        "with no masking at all, every must-mask row must be reported as failing"
    );
    assert!(
        leaked.len() > DISCLOSED_RESIDUALS,
        "the vacuity control is weaker than the gate it guards"
    );
}

/// The control for the other direction: a masker that destroys everything must
/// fail the over-mask rows. Without this, "mask the whole input" scores 0 leaks.
#[test]
fn the_gate_detects_over_masking() {
    let rows = corpus();
    let broken = failures(&rows, |_| String::new()); // mask everything
    let nomask = rows.iter().filter(|r| r.expect == "nomask").count();
    assert_eq!(
        broken.len(),
        nomask,
        "a masker that destroys its input must fail every over-mask control"
    );
}

#[test]
fn the_redactor_leaks_exactly_the_disclosed_residuals_and_nothing_else() {
    let rows = corpus();
    let actual = failures(&rows, |t| redact::mask_recorded(t).0);
    let expected: BTreeSet<String> = read("expected-failures.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();

    assert_eq!(
        expected.len(),
        DISCLOSED_RESIDUALS,
        "expected-failures.txt no longer holds the calibrated residual set"
    );

    let new_leaks: Vec<&String> = actual.difference(&expected).collect();
    let newly_clean: Vec<&String> = expected.difference(&actual).collect();

    assert!(
        new_leaks.is_empty(),
        "REGRESSION — {} row(s) leak that did not leak at the calibration point.\n\
         These are NOT disclosed residuals:\n  {}\n\
         Every one is a shape seventeen rounds of A209 already paid for.",
        new_leaks.len(),
        new_leaks
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
    assert!(
        newly_clean.is_empty(),
        "{} disclosed residual(s) now pass:\n  {}\n\
         This is good news and it still fails the gate, deliberately: the residual \
         set is a measurement. Re-derive expected-failures.txt, update the pins, \
         and record which residual closed and why.",
        newly_clean.len(),
        newly_clean
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );

    assert_eq!(actual.len(), DISCLOSED_RESIDUALS);
    assert_eq!(rows.len() - actual.len(), TOTAL_ROWS - DISCLOSED_RESIDUALS);
}

/// The residual set is not an opaque allowlist: every member must belong to one
/// of the three classes the record discloses, so a leak cannot be parked in it
/// by adding a line.
#[test]
fn every_disclosed_residual_belongs_to_a_named_class() {
    // A209-C3 "Vendor scope, enumerated (D4)" — formats the detector table does
    // not know. A209-C13/C14/C15 — the 136/578 unterminated-block residual.
    // A209-C — the two named standing exceptions N62/N63.
    const UNKNOWN_VENDORS: &[&str] = &[
        "gitlab-glpat",
        "stripe-live",
        "stripe-whsec",
        "huggingface",
        "npm",
        "docker-pat",
        "digitalocean",
        "google-oauth",
    ];
    const NAMED_EXCEPTIONS: &[&str] = &["nomask/css-id-sk", "nomask/sk-market"];

    let (mut vendors, mut unterminated, mut named) = (0, 0, 0);
    for f in read("expected-failures.txt").lines().map(str::trim) {
        if f.is_empty() {
            continue;
        }
        let is_vendor = (f.starts_with("token/") || f.starts_with("straddle/"))
            && UNKNOWN_VENDORS
                .iter()
                .any(|v| f.split('/').nth(1) == Some(v));
        let is_unterminated = f.starts_with("width/") && f.contains("/unterminated-cut");
        let is_named = NAMED_EXCEPTIONS.contains(&f);
        match (is_vendor, is_unterminated, is_named) {
            (true, _, _) => vendors += 1,
            (_, true, _) => unterminated += 1,
            (_, _, true) => named += 1,
            _ => panic!(
                "`{f}` is in expected-failures.txt but belongs to no disclosed class. \
                 A residual that is not one of A209-C3's unknown vendor formats, \
                 A209-C13's unterminated blocks, or A209-C's N62/N63 is an \
                 UNDISCLOSED LEAK being parked in the allowlist."
            ),
        }
    }
    assert_eq!(vendors, 168, "A209-C3 unknown-vendor rows");
    assert_eq!(unterminated, 40, "A209-C13 unterminated-cut rows");
    assert_eq!(named, 2, "A209-C standing exceptions N62/N63");
}

/// No real key material is committed. The corpus keys are shape-derived per
/// `tests/fixtures/pgp_armor_shapes.md`'s standing rule; this pins that they
/// stay that way.
#[test]
fn the_committed_key_fixtures_carry_no_real_key_material() {
    let dir = fixture_dir().join("keys");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("keys dir") {
        let path = entry.expect("entry").path();
        let body = std::fs::read_to_string(&path).expect("read key fixture");
        let payload: String = body
            .lines()
            .skip(1)
            .filter(|l| !l.is_empty() && !l.starts_with('=') && !l.starts_with("-----"))
            .filter(|l| !l.contains(": "))
            .collect();
        assert!(
            payload.contains("TLCANARY"),
            "{}: payload does not carry the canary — is this real key material?",
            path.display()
        );
        checked += 1;
    }
    assert_eq!(checked, 11, "expected 11 shape-derived key fixtures");
}
