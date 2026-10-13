//! Claim and promise extraction (BUILD-SPEC §5), rules-first and deterministic.
//!
//! # Two rules bind everything here
//!
//! **§0.1 — no model in the verdict path.** This crate may one day gain §5.3's
//! optional local-model extractor. It cannot leak into a verdict, because
//! verifiers are handed [`VerifiableClaim`], which structurally cannot carry
//! `text`, `span` or `extractor`. See [`claim`] for why `confidence` was
//! deleted rather than defaulted.
//!
//! **§6 — `unverifiable` never collapses into `supported`.** Nothing in this
//! crate produces a verdict at all; extraction answers only *whether a claim
//! exists*. That separation is the point of ruling C.9: a sentence may stop a
//! claim from being extracted, and may never excuse one that was.

pub mod claim;
pub mod extract;
pub mod rules;

pub use claim::{
    Claim, ClaimArgs, ClaimKind, Extractor, Promise, PromiseKind, PromiseStatus, Scope, Span,
    VerifiableClaim,
};
pub use extract::{extract, unclaimed_completion, Extraction, Source};
pub use rules::{Origin, Shadow, Table, TableError};

/// The BUILD-SPEC section that governs this crate.
pub const SPEC_SECTION: &str = "§5";

#[cfg(test)]
mod tests;
