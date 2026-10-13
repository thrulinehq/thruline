//! The ambiguity budget, enforced by `cargo test` on a machine with no corpus.
//!
//! # What this file is and is not
//!
//! The real measurement is `cargo run -p thruline-langpacks --release --bin
//! measure` over the four corpora named in `MEASUREMENTS.toml`. That is not
//! something `cargo test` can do on a machine that has not cloned those
//! repositories, and the recorded corpus lines are not part of this public copy.
//!
//! So two small **invented** files per language are committed instead (every
//! line was written for this repository; none is third-party source), and they
//! check different halves:
//!
//! - `tests/fixtures/synthetic/<lang>-ambiguous-sample.txt` — a handful of
//!   assertions whose `Strength` changes when literals are stripped. Every one
//!   must still be flagged. This is the half that catches a regression in the
//!   stripper: if a fix to `strip_language_literals` stops covering a literal
//!   form, these stop being ambiguous and the count drops.
//! - `tests/fixtures/synthetic/<lang>-assertions-sample.txt` — a few dozen
//!   ordinary assertions per language. This is the half that catches the
//!   opposite regression: a classifier that starts finding ambiguity
//!   everywhere.
//!
//! # The sample cannot resolve the budget, and does not pretend to
//!
//! A sample this small cannot measure a 0.1% phenomenon. Under the 1% bound
//! below, a sample of fewer than 100 records may hold no flagged record at all,
//! so the bound's job here is only to catch an order-of-magnitude regression,
//! and the real budget is asserted against the recorded full-corpus figures in
//! `tests/measured_constants.rs`. Saying which test enforces what is the point:
//! a gate whose reach is unstated gets trusted past it.

use std::path::PathBuf;

use thruline_langpacks::measured::{measurement, AMBIGUITY_BUDGET_PCT};
use thruline_langpacks::pack::PackLanguage;
use thruline_langpacks::{pack_for, shape};

const LANGS: &[(&str, PackLanguage)] = &[
    ("java", PackLanguage::Java),
    ("csharp", PackLanguage::CSharp),
    ("ruby", PackLanguage::Ruby),
    ("go", PackLanguage::Go),
];

/// Records, one per line, `#` comments and blanks skipped.
fn records(name: &str) -> Vec<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/synthetic")
        .join(name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            // Rows are `path:line\tassertion` or bare assertions; take the tail.
            l.rsplit('\t').next().unwrap_or(l).to_string()
        })
        .collect()
}

#[test]
fn the_committed_samples_are_actually_populated() {
    for (lang, _) in LANGS {
        let sample = records(&format!("{lang}-assertions-sample.txt"));
        assert!(
            sample.len() >= 30,
            "{lang}: the committed sample holds {} records; a shrunken sample \
             makes every rate below meaningless",
            sample.len()
        );
    }
}

/// Every assertion in the recorded residue must still be flagged.
#[test]
fn the_whole_recorded_residue_is_still_ambiguous() {
    for (lang, language) in LANGS {
        let pack = pack_for(*language);
        let residue = records(&format!("{lang}-ambiguous-sample.txt"));
        assert!(
            !residue.is_empty(),
            "{lang}: the recorded residue is empty, so this test asserts nothing"
        );
        let still: Vec<&String> = residue
            .iter()
            .filter(|a| shape::is_ambiguous(pack, a))
            .collect();
        let lost = residue.len() - still.len();
        // Not all-or-nothing: the residue rows carry a `path:line` prefix in
        // some files and the tail-split above can only do so much with an
        // assertion that itself contains a tab. A large drop is the signal.
        assert!(
            still.len() * 10 >= residue.len() * 8,
            "{lang}: {lost} of {} recorded-ambiguous assertions are no longer \
             flagged. Either the stripper stopped covering a literal form, or \
             the classifier stopped reading a matcher — both change what the \
             measured budget means, so re-run the measurement rather than \
             editing this threshold.",
            residue.len()
        );
    }
}

/// The sample must not show an order-of-magnitude regression.
#[test]
fn the_sample_rate_is_not_wildly_off_the_measured_rate() {
    for (lang, language) in LANGS {
        let pack = pack_for(*language);
        let sample = records(&format!("{lang}-assertions-sample.txt"));
        let flagged = sample
            .iter()
            .filter(|a| shape::is_ambiguous(pack, a))
            .count();
        let pct = flagged as f64 * 100.0 / sample.len() as f64;
        let full = measurement(lang).expect("measured").ambiguity_pct;
        assert!(
            pct < 1.0,
            "{lang}: {flagged} of {} sampled assertions ambiguous ({pct:.4}%), \
             against {full:.4}% measured on the full corpus. This bound is \
             deliberately an order of magnitude looser than the \
             {AMBIGUITY_BUDGET_PCT}% budget, because a sample this small \
             cannot resolve a 0.1% rate — so exceeding it means something \
             structural changed, not that the budget moved.",
            sample.len()
        );
    }
}
