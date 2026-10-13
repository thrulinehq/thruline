//! BUILD-SPEC §10 — Explain: the half of the product people love.
//!
//! Answers the six §10 questions **from the ledger alone**: what happened, why
//! it stopped, what it forgot, what it promised and did not do, what it claimed
//! versus what it proved, and what is risky.
//!
//! # §6.9 lives here, and only here
//!
//! **[RULING R2 / BUILD-PLAN C.9.]** Honest-failure recognition —
//! "couldn't", "failed to", "not able to" — is **narrative only**. It never
//! alters a verdict, a gate outcome, or a receipt.
//!
//! The enforcement is the dependency direction: `crates/explain` depends on
//! `crates/verify`, and `crates/verify` must never depend on `crates/explain`.
//! A verifier therefore *cannot* read the marker — a compile-time
//! impossibility rather than a convention. `dependency_direction.rs` asserts it
//! so a future edit cannot quietly reverse it.
//!
//! Why it matters: §6.9 as written let prose suppress a gate failure, which is
//! trivially exploitable — an agent that claims success and appends "I couldn't
//! get the tests running" would have its missing evidence excused by a sentence
//! it chose to write. Extraction may be conservative; verdicts may not be
//! excused.

pub mod compaction;
pub mod html;
pub mod narrative;
pub mod stop;
pub mod timeline;

pub use compaction::{CompactionLoss, LostItem};
pub use html::{render, Report};
pub use narrative::{honest_failure, HonestFailure};
pub use stop::{classify_stop, StopClassification, StopReasonKind};
pub use timeline::{Entry, Timeline};

/// The BUILD-SPEC section that governs this crate.
pub const SPEC_SECTION: &str = "§10";
