//! Detector coverage for the four languages `crates/verify` has not measured:
//! **Java, C#, Ruby and Go**.
//!
//! # What this crate is, and what `verify` still owns
//!
//! `crates/verify` owns the verdict. Nothing here computes one: this crate
//! produces the same `Finding`s, with the same detector names and the same
//! severities, and hands them to `verify::tests_pass::verify` — which is what
//! `tests/families_java.rs`, `tests/families_csharp.rs`,
//! `tests/families_ruby.rs` and `tests/families_go.rs` each actually do, end to
//! end, for their language.
//!
//! That is deliberate. §6.7's severities are not all decided by the detector:
//! `skip_marker_added` is a `warn` that the **run** escalates to `fail` or
//! lowers to `info`. A pack that emitted its own verdicts would have to
//! reimplement that, and the copy that drifts is the one nothing gates.
//!
//! # Why it is a separate crate rather than four arms in `verify::tamper`
//!
//! Because it cannot be four arms: `verify::tamper::Language` is a closed enum
//! whose per-language table (`LangSpec`) is private and reached through a
//! private method. [`pack::LangPack`] is that interface published. The merge plan
//! states the registration change in `crates/verify`, verbatim and minimal.
//!
//! # Every constant here is measured
//!
//! [`measured`] is generated from `MEASUREMENTS.toml`, which records the
//! corpus — repository, commit, file count, assertion count — behind each
//! number. `tests/measured_constants.rs` fails if the code and the file
//! disagree. The budget is **< 0.1% ambiguous assertions per language over at
//! least 5,000 real assertions**; the measured residue for all four is in
//! `MEASUREMENTS.toml` and reproduced by `cargo run --bin measure`.

pub mod langs;
pub mod measured;
pub mod narrowing;
pub mod pack;
pub mod runners;
pub mod shape;
pub mod tamper;

pub use pack::{pack_for, pack_for_path, packs, LangPack, LangSpec, PackLanguage, SkipSite};
pub use tamper::{diff_test_file, diff_test_file_with_symbols, parse_tests, TamperError, TestFn};

/// The BUILD-SPEC sections this crate implements for four more languages.
pub const SPEC_SECTIONS: &[&str] = &["§6.2", "§6.3", "§6.7"];
