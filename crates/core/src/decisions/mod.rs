//! Deterministic standing decisions for exact project versions.
//!
//! Accepted source spans and checks are separate from agent claims. A check
//! reads a named version and produces bounded, reproducible findings.

pub mod assertion_adapter;
pub mod attested_command_adapter;
pub mod authority;
pub mod c31_archive;
pub mod c31_json_field;
pub mod c32_span_scope;
pub mod checks;
pub mod checks_claims;
pub mod checks_fidelity;
pub mod checks_k_assertions;
pub mod checks_k_candidates;
mod checks_k_fixture_lookup;
pub mod checks_k_history;
pub mod checks_k_provenance;
pub mod checks_k_structural;
pub mod checks_presence;
pub mod checks_preservation;
pub mod checks_process;
pub mod checks_record;
pub mod checks_terms;
pub mod checks_tests;
pub mod checks_visual;
pub mod ci_adapter;
pub mod claim_record_adapter;
pub mod conflict;
pub mod context;
pub mod deferred;
pub mod deferred_grammar;
pub mod deferred_record;
pub mod engine;
pub mod gate;
pub mod grammar;
pub mod inline;
pub mod kinds_k;
pub mod params;
pub mod person_threads;
pub mod record_adapter;
pub mod record_adapter_phase2;
pub mod replay_basis;
mod review_params;
pub mod revision_proposal;
pub mod revival;
pub mod rule_files;
pub mod speaker;
pub mod storage;
pub mod text_exclusions;
mod text_exclusions_js;
pub mod why;

mod capture_tree_basis;
mod record_source;
