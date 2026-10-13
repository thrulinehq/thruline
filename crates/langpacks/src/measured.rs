//! Constants measured against a real corpus, not chosen.
//!
//! **Generated from `MEASUREMENTS.toml`.** `tests/measured_constants.rs` parses
//! that file and fails if any value here disagrees with it, if a language's
//! recorded assertion count is below the floor, or if a language's recorded
//! ambiguity silently changes side of the budget.
//!
//! The convention is `docs/perf-basis.toml`'s, and it exists for the reason
//! that file states: a figure with no stamped basis is one nobody can tell has
//! gone stale. `thruline perf` refuses to print a stale figure; here, the gate
//! test fails instead.

/// The Jaccard similarity above which two test bodies are the same test renamed.
///
/// # Measured, and it is not `verify`'s number
///
/// `verify::tamper::RENAME_SIMILARITY` is **0.5**, chosen against a
/// Python/Rust/JS table. Measured across these four languages it is **0.4**:
/// the pooled argmin of a cost that adds up the three ways the threshold can be
/// wrong — a mined real rename scored below it (a rename called a deletion), a
/// synthetic rename with three edits scored below it, and an unrelated
/// coexisting pair scored above it (two different tests called a rename).
///
/// The rename evidence is mined from the corpora's own git history: 14 real 1:1
/// renames over 681 Java commits, 36 over 2,629 C# commits, 102 over 2,101 Ruby
/// commits, plus 47,000-odd synthetic rename-plus-N-edit pairs and 495,000
/// coexisting pairs as the negative class.
///
/// Per language the argmin differs — java 0.4, go 0.4, ruby 0.45, csharp 0.55
/// — and `MEASUREMENTS.toml` records all four. **One pooled value is used**
/// rather than four, because `tamper::similar_bodies` compares two `TestFn`s
/// with no language on them; threading one through is a change to the
/// signature `verify` would have to adopt, and it belongs in the merge plan
/// rather than in a constant that quietly means four different things.
///
/// C# remains the outlier and is still the one to watch: at the pooled 0.4 it
/// accepts more accidental pairings than the other three. That is a named cost
/// of pooling, not a hidden one.
///
/// **It was 0.7 in the first measurement.** The re-run after
/// `tamper::as_test_fn` began blanking comments structurally moved it to 0.55,
/// and moved the pooled cost at the argmin from 2.0586 to 1.9079. Comments were
/// being fed raw to a literal stripper that treats `'` as a delimiter, so an
/// apostrophe truncated a body's token set; C# has the most comment-heavy
/// fixtures of the four corpora, which is why it moved most. The chosen value
/// did not change, but the basis did, and `MEASUREMENTS.toml` records the new
/// one rather than leaving the old behind an unchanged number.
pub const RENAME_SIMILARITY: f64 = 0.4;

/// The ambiguity budget: the fraction of assertions whose shape a message
/// string would decide, above which the classifier is not trustworthy for
/// `fail`-severity findings.
pub const AMBIGUITY_BUDGET_PCT: f64 = 0.1;

/// The floor on corpus size, per language.
pub const ASSERTION_FLOOR: u64 = 5_000;

/// One language's measurement, as recorded in `MEASUREMENTS.toml`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurement {
    pub language: &'static str,
    pub measured_files: u64,
    pub assertions: u64,
    pub ambiguous: u64,
    pub ambiguity_pct: f64,
    /// **`false` for Ruby.** Recorded rather than hidden — see the note in
    /// `MEASUREMENTS.toml` and `MEASURED` below.
    pub meets_budget: bool,
}

/// Every language's measurement.
///
/// # Ruby does not meet the budget, and that is stated here
///
/// Ruby measures **0.1275%** over 58,839 assertions against a budget of 0.1%.
/// The residue is not a classifier error: all 75 are cases where stripping
/// yields the correct shape and the raw text would not — Ruby test code carries
/// more operators and matcher words inside literals than the other three
/// (`assert_converted_to('\===', "<p>===</p>")` is typical). Moving the budget
/// to cover it would make the budget meaningless, so the miss is carried in the
/// data where the gate test can see it.
pub const MEASURED: &[Measurement] = &[
    Measurement {
        language: "java",
        measured_files: 1672,
        assertions: 72_696,
        ambiguous: 66,
        ambiguity_pct: 0.0908,
        meets_budget: true,
    },
    Measurement {
        language: "csharp",
        measured_files: 1538,
        assertions: 67_923,
        ambiguous: 48,
        ambiguity_pct: 0.0707,
        meets_budget: true,
    },
    Measurement {
        language: "ruby",
        measured_files: 1438,
        assertions: 58_839,
        ambiguous: 75,
        ambiguity_pct: 0.1275,
        meets_budget: false,
    },
    Measurement {
        language: "go",
        measured_files: 2037,
        assertions: 47_700,
        ambiguous: 27,
        ambiguity_pct: 0.0566,
        meets_budget: true,
    },
];

/// The measurement for a language, by name.
pub fn measurement(language: &str) -> Option<&'static Measurement> {
    MEASURED.iter().find(|m| m.language == language)
}
