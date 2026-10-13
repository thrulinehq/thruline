//! ThruLine core: parse AI coding transcripts into a hash-chained,
//! append-only ledger.
//!
//! Invariants this crate holds (CLAUDE.md):
//! * no `unsafe` (forbidden at the workspace level)
//! * no `unwrap()`/`expect()` outside tests
//! * no non-loopback network calls; explicit local-model clients stay on this machine
//! * unknown input is preserved, counted and reported — never dropped

pub mod adapter;
pub mod adjudicate;
pub mod agent_log;
pub mod annotate;
pub mod attempt_budget;
pub mod attention;
pub mod autopsy;
pub mod bounded_child;
pub mod build_environment;
pub mod candidates;
pub mod capsule;
pub mod capsule_v2;
pub mod capture;
pub mod capture_exclusion;
pub mod capture_hold;
pub mod capture_pause;
pub mod capture_product;
pub mod cargo_fetch;
pub mod chain;
pub mod change;
pub mod change_agent_message;
pub mod change_code_moved;
pub mod change_decision;
pub mod change_decision_second;
pub mod change_evidence;
pub mod change_export;
pub mod change_link;
pub mod change_note;
pub mod change_propose;
pub mod change_readiness;
pub mod change_review;
pub mod classification_store;
pub mod codex;
pub mod codex_ingest;
pub mod config;
pub mod continuity;
pub mod contract;
pub mod decisions;
pub mod demo;
pub mod derive;
pub mod entity_association;
pub mod error;
pub mod eval;
pub mod evidence_applicability;
pub mod evidence_binding;
pub mod examined_inputs;
pub mod export;
pub mod extract;
pub mod finding_standing;
pub mod generation_spool;
pub mod git_isolation;
pub mod guardian;
pub mod hq;
pub mod identity;
pub mod inbox_hide;
pub mod ingest;
pub mod language;
pub mod ledger;
pub mod mcp;
pub mod mind;
pub mod mind_eval;
pub mod mind_grounded;
pub mod mirror;
pub mod model;
pub mod model_supply;
pub mod nli;
pub mod now;
pub mod observation;
pub mod operations;
pub mod overflow;
pub mod packet;
pub mod parse;
pub mod perf_basis;
pub mod premise;
pub mod private_store;
pub mod protected_store;
pub mod record_verification;
pub mod recovery_demo;
pub mod redact;
pub mod run_execution;
pub mod run_permission;
pub mod run_record;
pub mod sensor;
pub mod session_status;
pub mod snapshot;
pub mod speaker;
pub mod speaker_gate;
pub mod spec;
pub mod statement_objection;
pub mod store_hold;
pub mod test_tally;
pub mod upgrade;
pub mod verifier;
pub mod voice;
pub mod workspace;
pub mod workspace_identity;

pub(crate) mod raw_scope;

pub use derive::{
    Currency, DerivedEdge, DerivedGraph, DerivedNode, EvidenceState, Lifecycle, NodeDimensions,
    NodeState,
};
pub use error::{CoreError, Result};
pub use ledger::{Ledger, SessionDigest, SessionState, StoredLink};
pub use model::{
    EventKind, LedgerEvent, MalformedLine, MalformedReason, NormalizedEvent, ProjectRef,
    RawLineRef, SessionRef, Source,
};
pub use parse::{LineOutcome, ParseStats, ParsedLine, SessionLines};
