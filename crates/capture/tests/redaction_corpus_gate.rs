//! The §4.6 layer is held to the SAME corpus as the core masker it wraps.
//!
//! `crates/core/tests/spec_redaction_corpus.rs` gates `redact::mask`. This runs
//! the identical 1,958 rows through `capture::redaction::Redactor`, which adds
//! two context rules and a user-config pass on top. Without this file those
//! additions would be ungated, and the layer that runs before every hash would
//! be the only unmeasured thing in the pipeline.
//!
//! **The bar is the same and it is an identity set, not a count**: the wrapper
//! must fail exactly the 210 disclosed residuals and no others. A different 210
//! is a regression at the same number. The two directions are reported
//! separately because they are not equally urgent: a newly-failing must-mask row
//! is a leak, a newly-failing nomask row is over-masking, and a newly-*passing*
//! row means a disclosed residual was fixed and the pin needs re-deriving.

use std::collections::BTreeSet;
use std::path::PathBuf;

use thruline_capture::redaction::Redactor;

const WINDOW: usize = 24;

/// The corpus lives in `core`'s fixtures; this reads the same files rather than
/// copying them. A copied corpus is two corpora, and they drift.
fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .join("core/tests/fixtures/redaction-corpus")
}

fn read(name: &str) -> String {
    let p = fixture_dir().join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// Sliding windows over the WHOLE payload, never the first line.
///
/// A209-C11: every prior control asserted the first payload line was absent
/// while both of that round's regressions were at the body's END.
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

fn expected_failures() -> BTreeSet<String> {
    read("expected-failures.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// Never consults the presence of a `[masked: …]` marker.
///
/// A209-C4 B1 found forged markers live in the operator's ledger, and A209-C13
/// found a reported mask sitting over a shipped key. Survival of key material is
/// the only measure.
fn evaluate(row: &Row, redactor: &Redactor) -> bool {
    let out = redactor.redact(&row.text).into_text();
    match row.expect.as_str() {
        "mask" => !row.secrets.iter().any(|s| leaks(&out, s)),
        "nomask" => out == row.text,
        other => panic!("unknown expect `{other}` on {}", row.family),
    }
}

fn failures(redactor: &Redactor) -> BTreeSet<String> {
    corpus()
        .iter()
        .filter(|r| !evaluate(r, redactor))
        .map(|r| r.family.clone())
        .collect()
}

/// The rows the §4.6 context rules fix that the core masker leaks.
///
/// **Measured, and pinned by identity.** All twenty belong to A209-C3's
/// "unknown vendor formats" residual class — credentials whose *shape* is not in
/// the detector table — delivered in the three positions a context rule can see:
/// an `Authorization:`-style header, a `.env` assignment, and a bare
/// `KEY=VALUE`.
///
/// That is exactly the class the context rules exist for, and exactly the class
/// A209-C2 shows a shape table cannot close: two real GitHub token prefixes were
/// missing from that table for thirty-one rounds while the evidence sat three
/// lines above it. A rule keyed on position catches the format nobody has
/// catalogued yet.
///
/// Pinned rather than counted so that un-fixing one is a failure. A count would
/// let a regression here be paid for by an accidental fix elsewhere.
const FIXED_BY_CONTEXT_RULES: &[&str] = &[
    "token/digitalocean/header",
    "token/digitalocean/quoted",
    "token/docker-pat/env",
    "token/docker-pat/equals",
    "token/docker-pat/header",
    "token/docker-pat/quoted",
    "token/gitlab-glpat/env",
    "token/gitlab-glpat/equals",
    "token/gitlab-glpat/header",
    "token/gitlab-glpat/quoted",
    "token/google-oauth/header",
    "token/google-oauth/quoted",
    "token/huggingface/env",
    "token/huggingface/equals",
    "token/huggingface/header",
    "token/huggingface/quoted",
    "token/npm/env",
    "token/npm/equals",
    "token/npm/header",
    "token/npm/quoted",
    "token/stripe-live/env",
    "token/stripe-live/equals",
    "token/stripe-live/header",
    "token/stripe-live/quoted",
    "token/stripe-whsec/env",
    "token/stripe-whsec/equals",
    "token/stripe-whsec/header",
    "token/stripe-whsec/quoted",
];

#[test]
fn the_capture_layer_fails_exactly_the_core_residuals_minus_the_ones_it_fixes() {
    // The identity set for THIS layer, derived from the core's pinned 210 rather
    // than copied into a second file — two corpora drift, and the drift is
    // silent.
    let core_pinned = expected_failures();
    let fixed: BTreeSet<String> = FIXED_BY_CONTEXT_RULES
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert!(
        fixed.is_subset(&core_pinned),
        "a family listed as fixed is not among the core's disclosed residuals: {:#?}",
        fixed.difference(&core_pinned).collect::<Vec<_>>()
    );
    let want: BTreeSet<String> = core_pinned.difference(&fixed).cloned().collect();

    let got = failures(&Redactor::new());

    let new_failures: Vec<&String> = got.difference(&want).collect();
    let unexpectedly_passing: Vec<&String> = want.difference(&got).collect();

    assert!(
        new_failures.is_empty(),
        "the §4.6 layer leaks rows the core masker does not. Every one is a \
         REGRESSION introduced by this crate — the Authorization or .env context \
         rule, or a user config:\n  {new_failures:#?}"
    );
    assert!(
        unexpectedly_passing.is_empty(),
        "these disclosed residuals now pass through the §4.6 layer but are not \
         listed in FIXED_BY_CONTEXT_RULES:\n  {unexpectedly_passing:#?}\nGood \
         news, and it still fails: the set is pinned by identity so a fix has to \
         be recorded, or a later regression could hide behind it."
    );
    assert_eq!(got.len(), core_pinned.len() - FIXED_BY_CONTEXT_RULES.len());
    assert_eq!(
        got.len(),
        182,
        "measured: 210 core residuals minus 28 fixed"
    );
}

#[test]
fn the_layer_never_leaks_anything_the_core_masker_catches() {
    // The one-directional guarantee, asserted independently of the pinned sets
    // so that a mistake in FIXED_BY_CONTEXT_RULES cannot hide a real leak.
    // Wrapping a masker must only ever mask MORE.
    let redactor = Redactor::new();
    let mut regressions = Vec::new();
    for row in corpus() {
        if row.expect != "mask" {
            continue;
        }
        let core = thruline_core::redact::mask(&row.text).0;
        let core_leaks = row.secrets.iter().any(|s| leaks(&core, s));
        let layer_leaks = row
            .secrets
            .iter()
            .any(|s| leaks(&redactor.redact(&row.text).into_text(), s));
        if layer_leaks && !core_leaks {
            regressions.push(row.family);
        }
    }
    assert!(
        regressions.is_empty(),
        "the §4.6 layer leaks key material the core masker had already caught: \
         {regressions:#?}"
    );
}

#[test]
fn the_gate_can_fail_in_both_directions() {
    // A gate never observed failing is an assertion about itself. A209-C4 B6,
    // A209-C5 (1) and A209-C6 (4) are three consecutive rounds whose controls
    // tested an easier case than the defect.
    let rows = corpus();

    // Under-mask: a redactor that does nothing must fail every must-mask row.
    let must_mask = rows.iter().filter(|r| r.expect == "mask").count();
    let leaked = rows
        .iter()
        .filter(|r| r.expect == "mask" && r.secrets.iter().any(|s| leaks(&r.text, s)))
        .count();
    assert_eq!(
        leaked, must_mask,
        "with no masking at all every must-mask row must be detected as leaking"
    );

    // Over-mask: a redactor that destroys everything must fail every nomask row.
    let nomask = rows.iter().filter(|r| r.expect == "nomask").count();
    assert!(nomask > 0, "the corpus has no over-mask controls");
    let destroyed = rows
        .iter()
        .filter(|r| r.expect == "nomask" && "[destroyed]" != r.text)
        .count();
    assert_eq!(
        destroyed, nomask,
        "the over-mask direction is not detectable on this corpus"
    );
}

#[test]
fn the_layer_only_changes_output_where_a_context_rule_can_fire() {
    // The layer masks MORE than the core masker on 66 rows. That is intended —
    // but it must only happen where a context rule could see something, i.e. a
    // header, a `.env` assignment or a bare `KEY=VALUE`. A difference anywhere
    // else means one of the new rules is firing on text it was never meant to
    // read, which is the D1 context-destruction direction.
    let redactor = Redactor::new();
    let mut unexplained = Vec::new();
    for row in corpus() {
        let core = thruline_core::redact::mask(&row.text).0;
        let layered = redactor.redact(&row.text).into_text();
        if core == layered {
            continue;
        }
        let explainable = row.family.ends_with("/header")
            || row.family.ends_with("/env")
            || row.family.ends_with("/equals")
            || row.family.ends_with("/quoted");
        if !explainable {
            unexplained.push(row.family);
        }
    }
    assert!(
        unexplained.is_empty(),
        "the §4.6 layer changed output on rows where no context rule should \
         apply:\n  {unexplained:#?}"
    );
}

#[test]
fn the_layer_is_a_fixpoint_across_the_entire_corpus() {
    // S3. `Redactor::redact` was not idempotent: `GITHUB_TOKEN=[masked: env_value]`
    // re-masked to `[masked: env_value] env_value]` on 44 of 1,958 rows. The
    // trigger is a second LAYER pass, which is the pipeline A209-C3 documents —
    // and the reasoning that missed it is worth more than the bug: the safety
    // argument was "markers carry no detector prefix", which is true of the CORE
    // masker and irrelevant to these rules, because they are context-keyed and
    // need no prefix at all.
    //
    // Whole corpus, not a sample: the defect appeared on 2% of rows and any
    // hand-picked set would plausibly have missed it.
    let redactor = Redactor::new();
    let mut unstable = Vec::new();
    for row in corpus() {
        let once = redactor.redact(&row.text).into_text();
        let twice = redactor.redact(&once).into_text();
        if once != twice {
            unstable.push((row.family, once, twice));
        }
    }
    assert!(
        unstable.is_empty(),
        "the layer is not a fixpoint on {} rows; first three:\n{:#?}",
        unstable.len(),
        unstable.iter().take(3).collect::<Vec<_>>()
    );
}

#[test]
fn a_third_pass_is_also_stable_and_the_control_can_fail() {
    // Idempotence at n=2 can hold while n=3 drifts if a rule alternates. And a
    // fixpoint test over text nothing ever masks would pass vacuously, so this
    // asserts the corpus actually exercises masking.
    let redactor = Redactor::new();
    let mut ever_masked = 0usize;
    for row in corpus() {
        let a = redactor.redact(&row.text).into_text();
        if a != row.text {
            ever_masked += 1;
        }
        let b = redactor.redact(&a).into_text();
        let c = redactor.redact(&b).into_text();
        assert_eq!(b, c, "{}: unstable at the third pass", row.family);
    }
    assert!(
        ever_masked > 1_000,
        "only {ever_masked} rows were masked at all — the fixpoint test is \
         close to vacuous"
    );
}
