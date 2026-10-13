//! The BUILD-SPEC §11 ledger: SQLite, WAL, per-repo namespaces, append-only
//! events.
//!
//! # Where it lives, and why not where the spec says
//!
//! §11 says `~/.thruline/ledger.db`. That file already exists on this machine
//! at **6.9 GB, 2,046,353 events, 56 tables**, holding the pre-spec engine's
//! transcript-derived record, and the decisive detail is that its schema
//! version *is* `sha256` of its DDL string
//! (`thruline_core::ledger::schema_fingerprint`). Adding a single character to
//! that string forces a full sixteen-step migration over two million rows on
//! every machine that installs the new binary — for tables the old schema has
//! nothing to do with.
//!
//! So this store is `~/.thruline/exec.db` (BUILD-PLAN C.2, **[SPEC-DEVIATION
//! D1]**). The rename is one path constant plus one migration step, and is an
//! M6 packaging slice gated on the legacy CLI surface being repointed.
//!
//! # Why it is a different store and not just a different file
//!
//! The legacy `events` table is `CHECK`-constrained to seven transcript kinds
//! (`user_msg`, `assistant_text`, `assistant_thinking`, `tool_use`,
//! `tool_result`, `summary`, `meta`) with **zero overlap** with the §3.1 kinds
//! that carry execution evidence (`exec`, `fs_change`, `snapshot`,
//! `compaction_*`, `stop`). The old table records *what the agent said*; this
//! one records *what the machine did*. BUILD-SPEC §0 principle 2 makes those
//! different evidentiary tiers, and sharing a table is how a future verifier
//! silently accepts a transcript row as execution evidence.
//!
//! # Migrations
//!
//! `PRAGMA user_version` plus an ordered list of embedded statements.
//!
//! **Neither of the two mechanisms §11 names is available.** `refinery 0.9.2`
//! constrains `rusqlite` to `>=0.23, <=0.39`; this workspace is pinned to
//! `rusqlite 0.40.1` with the `bundled` feature, and `libsqlite3-sys` declares
//! `links = "sqlite3"`, so cargo refuses the graph outright rather than linking
//! two SQLite C libraries — measured, not predicted. `sqlx migrate` means sqlx,
//! which means an async runtime; CLAUDE.md forbids tokio before the watcher
//! daemon and BUILD-PLAN records zero `#[tokio::test]` in the tree as
//! deliberate. Surfaced as a Rule 7 conflict rather than blended: the contract
//! §11 wants from a migration tool — ordered, versioned, idempotent, recorded —
//! is met here in about forty lines, and the DDL-fingerprint scheme the legacy
//! store uses is deliberately *not* reproduced, because that scheme is the
//! reason a one-character change costs a two-million-row pass.

pub mod acceptance;
pub mod importer;
pub mod schema;

pub use importer::{ImportOptions, ImportReport, Importer};
pub use schema::{
    CheckRow, EventRow, Ledger, LedgerError, LegacyProvenance, PromiseRow, RepoId, SessionSummary,
    SCHEMA_VERSION,
};
