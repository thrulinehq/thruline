//! The BUILD-SPEC data model.
//!
//! `docs/BUILD-SPEC.md` §2 assigns the event model, the hash chain, ids,
//! errors and snapshots to `crates/core`. This module is where those live.
//!
//! # Why a submodule rather than the crate root
//!
//! The crate root already carries a full engine built against the earlier
//! doctrine, and three of the names the spec wants are taken by things that
//! mean something else: [`crate::chain`] is the *ledger row* chain
//! (`sha256(prev ‖ raw_sha256 ‖ session_id ‖ seq)`), [`crate::snapshot`] is
//! the read-only *workspace observation* (A171), and [`crate::model`] is the
//! transcript-parse model. The spec's `Event`, its RFC 8785 hash chain, and
//! its `git write-tree` snapshot engine are different objects with different
//! contracts.
//!
//! Collapsing them would be a silent semantic merge of two hash chains, which
//! is precisely the class of change the 13 Rules forbid doing by accident.
//! They therefore sit here, side by side with the existing engine, until the
//! reconciliation map says which of each pair survives. Nothing in this module
//! reads or writes the legacy tables; nothing in the legacy engine calls into
//! it.

pub mod canonical;
pub mod chain;
pub mod event;
pub mod git_env;
pub mod snapshot;

pub use canonical::{canonical_bytes, canonical_string, CanonicalError};
pub use chain::{genesis_prev_hash, verify_chain, ChainError, ChainHead, ChainWalker};
pub use event::{Event, EventError, EventId, EventKind, Payload, Source, EVENT_SCHEMA_VERSION};
pub use snapshot::{snapshot_tree, SnapshotError, TreeSnapshot};
