//! The gate: `src/measured.rs` and `MEASUREMENTS.toml` must agree.
//!
//! # Why a gate rather than a comment
//!
//! `src/measured.rs` used to carry `RENAME_SIMILARITY = 0.5` under a doc
//! comment saying "measured, not chosen". It was neither: it was `verify`'s
//! number, inherited from a Python/Rust/JS table, sitting behind a citation to
//! a `MEASUREMENTS.toml` that did not exist. Nothing failed. A reader found it.
//!
//! That is the shape of the defect this whole crate detects — a claim with no
//! evidence behind it, where the absence is invisible — so the claim and its
//! basis are checked together here or the constant is not trustworthy.
//!
//! Measured, it is **0.4**.
//!
//! # The TOML reader is hand-rolled on purpose
//!
//! Adding a `toml` dependency to the crate so a test can read one file would
//! put a parser in the dependency graph of a crate whose entire job is reading
//! source. The subset needed here — `[section]`, `key = value`, and arrays of
//! inline tables — is thirty lines, and a reader that only understands what
//! this file contains cannot silently accept a file that means something else.

use std::collections::BTreeMap;
use std::path::PathBuf;

use thruline_langpacks::measured::{
    measurement, AMBIGUITY_BUDGET_PCT, ASSERTION_FLOOR, MEASURED, RENAME_SIMILARITY,
};

fn measurements_toml() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("MEASUREMENTS.toml");
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{} could not be read: {e}. The constants in src/measured.rs are \
             generated from it; without it they are unbacked numbers and this \
             gate is the only thing that says so.",
            path.display()
        )
    })
}

/// `section -> key -> value`, for the scalar subset this file uses.
fn parse(text: &str) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut out: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut section = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.to_string();
            out.entry(section.clone()).or_default();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim().trim_end_matches(',').trim().trim_matches('"');
        out.entry(section.clone())
            .or_default()
            .insert(k.trim().to_string(), v.to_string());
    }
    out
}

fn num(t: &BTreeMap<String, BTreeMap<String, String>>, section: &str, key: &str) -> f64 {
    t.get(section)
        .and_then(|s| s.get(key))
        .unwrap_or_else(|| panic!("MEASUREMENTS.toml has no `{key}` under [{section}]"))
        .parse()
        .unwrap_or_else(|e| panic!("[{section}].{key} is not a number: {e}"))
}

/// A guard on the reader. A parser that returned nothing would make every
/// assertion below vacuous — the same "empty search space passes forever"
/// failure the core's own commit hook refuses for its deny list.
#[test]
fn the_reader_finds_the_sections_it_is_about_to_assert_on() {
    let t = parse(&measurements_toml());
    for section in [
        "basis",
        "language.java",
        "language.csharp",
        "language.ruby",
        "language.go",
        "rename_similarity",
    ] {
        assert!(
            t.contains_key(section),
            "MEASUREMENTS.toml has no [{section}] — the reader is broken or the \
             file changed shape, and either way the gate below is not checking \
             what it claims. Sections found: {:?}",
            t.keys().collect::<Vec<_>>()
        );
    }
}

#[test]
fn every_constant_matches_the_recorded_measurement() {
    let t = parse(&measurements_toml());

    assert_eq!(
        RENAME_SIMILARITY,
        num(&t, "rename_similarity", "value"),
        "RENAME_SIMILARITY in src/measured.rs disagrees with MEASUREMENTS.toml. \
         Regenerate the constant; do not edit it to match a value you preferred."
    );
    assert_eq!(
        AMBIGUITY_BUDGET_PCT,
        num(&t, "basis", "ambiguity_budget_pct"),
        "the budget in the code and the budget the measurement was taken against \
         differ — one of them moved after the fact"
    );
    assert_eq!(ASSERTION_FLOOR as f64, num(&t, "basis", "assertion_floor"));

    for m in MEASURED {
        let s = format!("language.{}", m.language);
        assert_eq!(m.assertions as f64, num(&t, &s, "assertions"), "{s}");
        assert_eq!(m.ambiguous as f64, num(&t, &s, "ambiguous"), "{s}");
        assert_eq!(
            m.measured_files as f64,
            num(&t, &s, "measured_files"),
            "{s}"
        );
        assert!(
            (m.ambiguity_pct - num(&t, &s, "ambiguity_pct")).abs() < 1e-9,
            "{s}: ambiguity_pct disagrees"
        );
    }
}

#[test]
fn every_language_clears_the_assertion_floor() {
    for m in MEASURED {
        assert!(
            m.assertions >= ASSERTION_FLOOR,
            "{} was measured on {} assertions, below the floor of {ASSERTION_FLOOR}. \
             A classifier measured on too little is not measured.",
            m.language,
            m.assertions
        );
    }
}

#[test]
fn meets_budget_is_consistent_with_the_recorded_rate() {
    // The flag and the number must not be able to disagree — a `meets_budget:
    // true` beside a rate over the budget is exactly the kind of unbacked claim
    // this file exists to catch.
    for m in MEASURED {
        assert_eq!(
            m.meets_budget,
            m.ambiguity_pct < AMBIGUITY_BUDGET_PCT,
            "{}: meets_budget says {} but the rate is {:.4}% against a budget of \
             {AMBIGUITY_BUDGET_PCT}%",
            m.language,
            m.meets_budget,
            m.ambiguity_pct
        );
    }
}

/// **The named miss.** Ruby is over budget and the crate says so.
///
/// This test fails in BOTH directions on purpose. If Ruby quietly improves,
/// it fails and tells you to delete it and flip the flag — a stale
/// "known failure" that has silently started passing is how a real fix goes
/// unnoticed and how an expired exemption becomes permanent.
#[test]
fn ruby_is_over_budget_and_it_is_recorded() {
    let ruby = measurement("ruby").expect("ruby is measured");
    assert!(
        !ruby.meets_budget && ruby.ambiguity_pct >= AMBIGUITY_BUDGET_PCT,
        "Ruby now measures {:.4}%, at or under the {AMBIGUITY_BUDGET_PCT}% budget. \
         That is good news and this test is now wrong: set `meets_budget: true` \
         in src/measured.rs, update MEASUREMENTS.toml from a fresh run, and \
         DELETE this test. Leaving it would record a miss that no longer exists.",
        ruby.ambiguity_pct
    );
    assert!(
        MEASURED.iter().filter(|m| !m.meets_budget).count() == 1,
        "more than one language is over budget; each one needs its own recorded \
         reason rather than sharing Ruby's"
    );
}
