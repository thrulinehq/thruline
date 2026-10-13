//! Deterministic verifiers, detectors and runner output parsers (BUILD-SPEC §6).
//!
//! **No model is ever in this path.** §0 principle 1: a verdict is produced only
//! by deterministic code over captured evidence. LLM judges score AUROC ≤ 0.65
//! on false-success detection; lightweight deterministic checks beat them, and
//! a judge that is sometimes right is worse than a check that says
//! `unverifiable` when it does not know.
//!
//! Implemented so far: §6.2 output parsers for `pytest` and `cargo test`, the
//! §13 `sh -c` payload parser, and the §3.3 `Check` draft built from a captured
//! `exec` event. Everything else in §6 is scaffold — in particular **no
//! verdict is computed anywhere in this crate yet**, which is why a `Check`
//! draft carries a [`RunnerOutcome`] and not a verdict.

pub mod behavior_unchanged;
pub mod binding;
pub mod check;
pub mod coverage;
pub mod diff_evidence;
pub mod integrations;
pub mod parse;
pub mod shell;
pub mod special_casing;
pub mod tamper;
pub mod tests_pass;
pub mod tool_checks;
pub mod verdict;

pub use check::{CheckDraft, CheckOutcome};
pub use parse::{parse, Parsed, Runner, RunnerOutcome};
pub use shell::{shell_payload, split_commands, ShellPayload};
pub use verdict::{Finding, Severity, Verdict, VerdictRecord};

/// The BUILD-SPEC section that governs this crate.
pub const SPEC_SECTION: &str = "§6";
