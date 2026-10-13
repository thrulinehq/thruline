//! Schema, migrations, append and queries.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
use thruline_core::spec::chain::{ChainError, ChainHead, ChainWalker};
use thruline_core::spec::event::{Event, EventWire};

/// The schema version this build writes and reads.
///
/// A plain integer in `PRAGMA user_version`. See the crate docs for why this is
/// not the legacy store's `sha256(DDL)` fingerprint.
pub const SCHEMA_VERSION: u32 = 4;

/// The first `prev_hash` of a session's gate chain.
///
/// A constant rather than an empty string: `""` is also what a hand-written row
/// leaves behind, and the two must not be the same value.
pub const GATE_CHAIN_GENESIS: &str = "gate-block-chain-v1-genesis";

/// One recorded Stop-gate block. See [`Ledger::gate_blocks`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateBlock {
    pub seq: u64,
    /// `sha256(kind|text)` — the claim as the agent stated it, which is what
    /// "exactly once per claim" is keyed on.
    pub claim_key: String,
    pub claim_kind: String,
    pub at: String,
    pub prev_hash: String,
    pub hash: String,
}

/// One session as a chooser sees it. See [`Ledger::session_summaries`].
///
/// Every field is a fact the store carries. `started_at`, `ended_at`,
/// `stop_reason` and `model` are deliberately absent: they are NULL on every
/// row of a real corpus, and four permanently-null fields would teach a reader
/// that the store has no timestamps rather than that these columns were never
/// written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummary {
    pub id: String,
    /// `repos.key`. On 41 of 43 real repos this is the **lossy dash form**
    /// (`-Users-a-b`), which is ambiguous between `/Users/a/b` and `/Users/a-b`
    /// — it is carried verbatim and must never be un-mangled, because a guessed
    /// path is a fabricated receipt.
    pub repo_key: String,
    /// `repos.root_path` when it is not empty — the real path, present on 2 of
    /// 43 rows.
    pub repo_root: Option<String>,
    pub harness: String,
    /// `sessions.event_count`. Exact: no row of the operator's store disagrees
    /// with `COUNT(events)`. `last_seq` is NOT this and is not returned — it is
    /// the harness's own counter (97,200 beside 3,252 events) and next to a
    /// real event count it would read as a second, contradictory one.
    pub event_count: u64,
    pub user_prompts: u64,
    /// The gate on everything downstream: without a `stop` event there is no
    /// final message, so `explain` and `claims` have nothing to extract.
    pub stop_events: u64,
    pub file_change_events: u64,
    /// `stop` events whose payload carries a non-empty `last_assistant_message`
    /// — what the extractor reads. Not a claim that a claim will come out of
    /// it; only opening the session answers that.
    pub final_messages: u64,
    /// `MIN(events.ts)` / `MAX(events.ts)`, and `None` for the epoch sentinel.
    /// This is an event's *recorded* time, not a session's start or end.
    pub first_event_at: Option<String>,
    pub last_event_at: Option<String>,
    pub head_hash: Option<String>,
    pub capture_degraded: bool,
}

/// A timestamp the record actually carries, or `None`.
///
/// 55,578 event rows of the operator's store carry the epoch sentinel
/// `1970-01-01T00:00:00Z`. It sorts last, which is the right ordering, but
/// rendering it as a date would be inventing a fact — exactly the class of
/// claim this product exists to catch.
fn recorded_time(ts: &str) -> Option<String> {
    if ts.is_empty() || ts.starts_with("1970-01-01T00:00:00") {
        None
    } else {
        Some(ts.to_string())
    }
}

/// The chain link. Length-prefixed, so no field can impersonate another by
/// containing a separator — the same construction `capsule` signatures use.
fn gate_block_hash(
    prev: &str,
    session_id: &str,
    seq: u64,
    claim_key: &str,
    claim_kind: &str,
    at: &str,
) -> String {
    let seq = seq.to_string();
    let mut bytes = Vec::new();
    for field in [
        b"thruline-gate-block-v1".as_slice(),
        prev.as_bytes(),
        session_id.as_bytes(),
        seq.as_bytes(),
        claim_key.as_bytes(),
        claim_kind.as_bytes(),
        at.as_bytes(),
    ] {
        bytes.extend_from_slice(&(field.len() as u64).to_le_bytes());
        bytes.extend_from_slice(field);
    }
    thruline_core::chain::sha256_hex(&bytes)
}

/// The ordered migrations. Index + 1 is the version each one produces.
///
/// **Append only.** Editing a migration that has shipped changes the schema of
/// databases that already ran it and will never run it again.
const MIGRATIONS: &[&str] = &[
    // v1 — §11's tables.
    r#"
-- Per-repo namespace (§11 "per-repo namespaces"). `key` is the canonical
-- remote-or-path; two clones of one repository share a namespace, and two
-- unrelated repositories never collide.
CREATE TABLE repos(
  id          TEXT PRIMARY KEY,
  key         TEXT NOT NULL UNIQUE,
  root_path   TEXT,
  created_at  TEXT NOT NULL
);

CREATE TABLE sessions(
  id               TEXT PRIMARY KEY,
  repo_id          TEXT NOT NULL REFERENCES repos(id),
  harness          TEXT NOT NULL,
  model            TEXT,
  started_at       TEXT,
  ended_at         TEXT,
  -- §13: a session whose capture is known to be incomplete can never support a
  -- claim. Stored on the session because it is a property of how the record was
  -- obtained, not of any one event.
  capture_degraded INTEGER NOT NULL DEFAULT 0,
  degraded_reason  TEXT,
  -- A second, independent reason a session cannot reach `supported`: its source
  -- transcript no longer exists, so the recorded digest cannot be re-derived
  -- and the receipt path it names does not resolve. 75,741 of 76,203 legacy
  -- sessions are in this state.
  source_digest_unverifiable INTEGER NOT NULL DEFAULT 0,
  head_hash        TEXT,
  event_count      INTEGER NOT NULL DEFAULT 0,
  last_seq         INTEGER NOT NULL DEFAULT 0,
  stop_reason      TEXT
);
CREATE INDEX idx_sessions_repo ON sessions(repo_id);
CREATE INDEX idx_sessions_stop_reason ON sessions(stop_reason);

-- §3.1 events. `payload` is the canonical JSON object; `hash`/`prev_hash` are
-- the chain. `redactions_applied` is §4.6's per-event count — it is NOT a §3.1
-- field, and putting it in the payload would change the hashed wire format, so
-- it lives beside the event instead.
CREATE TABLE events(
  id                 TEXT PRIMARY KEY,
  repo_id            TEXT NOT NULL REFERENCES repos(id),
  session_id         TEXT NOT NULL REFERENCES sessions(id),
  seq                INTEGER NOT NULL,
  v                  INTEGER NOT NULL,
  ts                 TEXT NOT NULL,
  source             TEXT NOT NULL,
  kind               TEXT NOT NULL,
  payload            TEXT NOT NULL,
  prev_hash          TEXT NOT NULL,
  hash               TEXT NOT NULL,
  redactions_applied INTEGER NOT NULL DEFAULT 0,
  redaction_families TEXT NOT NULL DEFAULT '[]',
  -- Provenance for imported rows: the raw transcript line this came from.
  legacy_raw_file    TEXT,
  legacy_raw_line    INTEGER,
  legacy_raw_sha256  TEXT,
  legacy_kind        TEXT,
  legacy_chain_hash  TEXT,
  UNIQUE(session_id, seq)
);
CREATE INDEX idx_events_session ON events(session_id, seq);
CREATE INDEX idx_events_repo_kind ON events(repo_id, kind);
CREATE INDEX idx_events_hash ON events(hash);

-- Append-only, enforced by the database rather than by discipline. The legacy
-- store has these on `events` and NOT on `sessions`, `claims` or `nodes`; that
-- shape is not inherited here for anything that carries evidence.
CREATE TRIGGER events_no_update BEFORE UPDATE ON events
  BEGIN SELECT RAISE(ABORT,'events are append-only'); END;
CREATE TRIGGER events_no_delete BEFORE DELETE ON events
  BEGIN SELECT RAISE(ABORT,'events are append-only'); END;

CREATE TABLE snapshots(
  tree_hash           TEXT NOT NULL,
  session_id          TEXT NOT NULL REFERENCES sessions(id),
  event_id            TEXT NOT NULL REFERENCES events(id),
  head_commit         TEXT,
  dirty               INTEGER NOT NULL,
  changed_paths_count INTEGER NOT NULL,
  -- The §4.4 content-filter finding: a clean filter makes the tree hash stop
  -- naming the bytes on disk, so a snapshot has to carry whether it was taken
  -- under one.
  content_filters     TEXT NOT NULL DEFAULT '',
  capture_degraded    INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(session_id, event_id)
);
CREATE INDEX idx_snapshots_tree ON snapshots(tree_hash);

CREATE TABLE claims(
  id         TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(id),
  kind       TEXT NOT NULL,
  text       TEXT NOT NULL,
  span_event TEXT,
  span_start INTEGER,
  span_end   INTEGER,
  args       TEXT NOT NULL DEFAULT '{}',
  extractor  TEXT NOT NULL,
  confidence REAL
);
CREATE INDEX idx_claims_session ON claims(session_id);

CREATE TABLE promises(
  id              TEXT PRIMARY KEY,
  session_id      TEXT NOT NULL REFERENCES sessions(id),
  repo_id         TEXT NOT NULL REFERENCES repos(id),
  text            TEXT NOT NULL,
  kind            TEXT NOT NULL,
  status          TEXT NOT NULL,
  opened_event_id TEXT,
  closed_event_id TEXT,
  closed_reason   TEXT
);
CREATE INDEX idx_promises_open ON promises(repo_id, status);

CREATE TABLE checks(
  id              TEXT PRIMARY KEY,
  session_id      TEXT NOT NULL REFERENCES sessions(id),
  exec_event_id   TEXT,
  category        TEXT NOT NULL,
  runner          TEXT NOT NULL,
  argv_hash       TEXT NOT NULL,
  cwd_hash        TEXT NOT NULL,
  env_hash        TEXT NOT NULL,
  input_tree_hash TEXT NOT NULL,
  exit_code       INTEGER NOT NULL,
  parsed          TEXT NOT NULL DEFAULT '{}',
  narrowed        TEXT,
  reruns          INTEGER NOT NULL DEFAULT 0,
  deterministic   INTEGER,
  reexecuted      INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_checks_tree ON checks(input_tree_hash);

CREATE TABLE verdicts(
  claim_id TEXT PRIMARY KEY REFERENCES claims(id),
  verdict  TEXT NOT NULL CHECK(verdict IN ('supported','unsupported','unverifiable')),
  verifier TEXT NOT NULL,
  evidence TEXT NOT NULL DEFAULT '[]',
  reasons  TEXT NOT NULL DEFAULT '[]',
  warnings TEXT NOT NULL DEFAULT '[]'
);
CREATE INDEX idx_verdicts_verdict ON verdicts(verdict);

CREATE TABLE receipts(
  id            TEXT PRIMARY KEY,
  session_id    TEXT NOT NULL REFERENCES sessions(id),
  repo_id       TEXT NOT NULL REFERENCES repos(id),
  commit_sha    TEXT,
  tree_hash     TEXT,
  envelope_hash TEXT NOT NULL,
  rekor_uuid    TEXT,
  created_at    TEXT NOT NULL,
  envelope      TEXT NOT NULL
);
CREATE INDEX idx_receipts_commit ON receipts(commit_sha);

CREATE TABLE findings(
  id         TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(id),
  check_id   TEXT,
  severity   TEXT NOT NULL CHECK(severity IN ('info','warn','fail')),
  detector   TEXT NOT NULL,
  path       TEXT,
  detail     TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX idx_findings_session ON findings(session_id, severity);

-- Import bookkeeping, so a resumed import does not duplicate and a reader can
-- tell an imported session from a captured one without guessing.
CREATE TABLE imports(
  id            TEXT PRIMARY KEY,
  source_path   TEXT NOT NULL,
  source_kind   TEXT NOT NULL,
  started_at    TEXT NOT NULL,
  finished_at   TEXT,
  rows_read     INTEGER NOT NULL DEFAULT 0,
  events_written INTEGER NOT NULL DEFAULT 0,
  sessions_seen INTEGER NOT NULL DEFAULT 0,
  note          TEXT
);
"#,
    // v2 — the gate's block ledger (Track R, G2).
    //
    // The count that decides whether the Stop gate fires lived in
    // `$THRULINE_HOME/gate/<session>.blocks`, a loose JSON file with no
    // integrity of any kind. One `printf '{"count":9999,"claims":[]}'` spent a
    // budget that had never been used, and the gate stayed installed, exited 0
    // and stopped gating. Deriving the count from the claims beside it defeated
    // that one-line version and nothing more: a file anything running as the
    // user can write cannot be authenticated, because authentication needs a
    // secret and every store the gate can read the agent can read too.
    //
    // So the count moves to a table with the same append-only triggers `events`
    // has, and the same hash chain, which changes the property from *believed*
    // to *checkable*. What that does and does not buy is stated at
    // `Ledger::gate_blocks`, and in `docs/DECISIONS-PENDING.md` D20 — it makes
    // forgery DETECTABLE, not impossible.
    //
    // No foreign key to `sessions(id)`. A block is recorded at a Stop, and the
    // gate reads its own history BEFORE it folds the spool, so the session row
    // may not exist yet on the read path. A referential constraint that can
    // fail is a gate that fails, and this gate's whole contract is that it
    // never does.
    r#"
CREATE TABLE gate_blocks(
  session_id  TEXT NOT NULL,
  seq         INTEGER NOT NULL,
  claim_key   TEXT NOT NULL,
  claim_kind  TEXT NOT NULL,
  at          TEXT NOT NULL,
  prev_hash   TEXT NOT NULL,
  hash        TEXT NOT NULL,
  PRIMARY KEY(session_id, seq)
);
CREATE INDEX idx_gate_blocks_session ON gate_blocks(session_id, seq);

CREATE TRIGGER gate_blocks_no_update BEFORE UPDATE ON gate_blocks
  BEGIN SELECT RAISE(ABORT,'gate blocks are append-only'); END;
CREATE TRIGGER gate_blocks_no_delete BEFORE DELETE ON gate_blocks
  BEGIN SELECT RAISE(ABORT,'gate blocks are append-only'); END;
"#,
    // v3 — locally accepted requirements. Independent of session prose; old
    // revisions remain append-only and are linked by the storage API.
    r#"
CREATE TABLE acceptance_revisions(
  repo_id TEXT NOT NULL REFERENCES repos(id),
  seq INTEGER NOT NULL CHECK(seq > 0),
  document TEXT NOT NULL,
  source_commit TEXT,
  accepted_by TEXT NOT NULL,
  accepted_at TEXT NOT NULL,
  prev_hash TEXT NOT NULL,
  hash TEXT NOT NULL,
  PRIMARY KEY(repo_id, seq)
);
CREATE TRIGGER acceptance_no_update BEFORE UPDATE ON acceptance_revisions
  BEGIN SELECT RAISE(ABORT,'accepted requirements are append-only'); END;
CREATE TRIGGER acceptance_no_delete BEFORE DELETE ON acceptance_revisions
  BEGIN SELECT RAISE(ABORT,'accepted requirements are append-only'); END;
"#,
    // v4 — exact accepted working-tree baseline. NULL on older revisions;
    // old metadata must never be invented from the current checkout.
    "ALTER TABLE acceptance_revisions ADD COLUMN source_tree TEXT;",
];

/// Something went wrong with the ledger.
#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("source store open refused: {0}")]
    SourceOpenGuard(#[from] thruline_core::error::CoreError),
    #[error("could not create {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("no home directory: set THRULINE_HOME or pass an explicit path")]
    NoHome,
    #[error(
        "this ledger is at schema version {found}, but this build understands up to \
         {supported}. It was written by a newer ThruLine; upgrade rather than \
         letting an older build interpret it."
    )]
    FromTheFuture { found: u32, supported: u32 },
    #[error("event {id} is unsealed and cannot be appended")]
    UnsealedEvent { id: String },
    /// The event carries a hash that is not the hash of its contents.
    ///
    /// Refused at append because `events` is append-only: once written, the row
    /// cannot be repaired, and the session it belongs to is unverifiable
    /// forever. Rejecting the batch is recoverable; accepting it is not.
    #[error("event {id} carries a hash that does not match its contents and cannot be appended")]
    CorruptEvent {
        id: String,
        #[source]
        source: thruline_core::spec::event::EventError,
    },
    #[error("serializing an event: {0}")]
    Json(#[from] serde_json::Error),
    /// The Stop gate's block chain does not verify for this session.
    ///
    /// **Not "no blocks yet".** Track R's G2: the count that decides whether the
    /// gate fires must never read a tampered store as a fresh one, because that
    /// is the state in which `max_stop_blocks` bounds nothing.
    #[error("the gate's block ledger for session {session} does not verify: {detail}")]
    GateChainBroken { session: String, detail: String },
    #[error("the chain read back from the ledger does not verify")]
    Chain(#[from] ChainError),
    #[error("session {session_id} is not known to this ledger")]
    UnknownSession { session_id: String },
    #[error("acceptance history does not verify: {0}")]
    AcceptanceHistory(String),
    #[error(
        "accepted requirements changed since preview; inspect the current history before accepting"
    )]
    AcceptanceChanged,
}

/// A §3.5 `Promise` as the store holds it.
///
/// `promises` is deliberately mutable, unlike `events`: a promise's *status* is
/// derived state that later evidence revises, and revising it is the point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromiseRow {
    pub id: String,
    pub session_id: String,
    pub text: String,
    pub kind: String,
    pub status: String,
    pub opened_event_id: String,
}

/// A §3.3 `Check` as the store holds it.
///
/// `parsed` is a JSON string rather than a typed block because the parsers live
/// in `crates/verify` and this crate must not depend on them: the ledger stores
/// evidence, it does not interpret it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckRow {
    pub id: String,
    pub session_id: String,
    pub exec_event_id: String,
    pub category: String,
    pub runner: String,
    pub argv_hash: String,
    pub cwd_hash: String,
    pub env_hash: String,
    /// The tree the check ran against (§6.1). **Empty when no snapshot could be
    /// taken** — an empty string can never equal a tree hash, so an unbound
    /// check can never be mistaken for a bound one.
    pub input_tree_hash: String,
    pub exit_code: i64,
    /// The §3.3 `parsed` block as JSON, or `{"unverifiable": {...}}`. Never
    /// `{}`: a defaulted parsed block reads as a clean run.
    pub parsed: String,
}

/// A repository namespace id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepoId(String);

impl RepoId {
    /// Derive from the canonical remote-or-path.
    ///
    /// Hashed rather than munged, because the key is frequently an absolute
    /// path containing a username and this id appears in exported artefacts.
    pub fn from_key(key: &str) -> RepoId {
        RepoId(thruline_core::chain::sha256_hex(key.as_bytes()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An event, plus the §4.6 facts that travel beside it rather than inside its
/// hash.
#[derive(Debug, Clone)]
pub struct EventRow {
    pub event: Event,
    pub redactions_applied: u64,
    pub redaction_families: BTreeSet<String>,
    pub legacy: Option<LegacyProvenance>,
}

impl EventRow {
    /// An event with no redaction pass recorded against it.
    ///
    /// Used by tests and by the importer, whose rows were masked (or not) by
    /// the legacy engine and whose count this store cannot honestly restate.
    pub fn bare(event: Event) -> EventRow {
        EventRow {
            event,
            redactions_applied: 0,
            redaction_families: BTreeSet::new(),
            legacy: None,
        }
    }
}

/// Where an imported row came from in the legacy store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyProvenance {
    pub raw_file: String,
    pub raw_line: i64,
    /// The digest the legacy ledger recorded. **Carried forward, never
    /// recomputed** — 75,741 of 76,203 source transcripts no longer exist, so
    /// 99.4% of the corpus cannot be re-hashed and an importer that tried would
    /// fail on almost all of it.
    pub raw_sha256: String,
    pub legacy_kind: String,
    pub legacy_chain_hash: String,
}

/// The §11 store.
pub struct Ledger {
    conn: Connection,
}

impl Ledger {
    /// `$THRULINE_HOME/exec.db`, defaulting to `~/.thruline/exec.db`.
    pub fn default_path() -> Result<PathBuf, LedgerError> {
        let home = std::env::var_os("THRULINE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".thruline")))
            .ok_or(LedgerError::NoHome)?;
        Ok(home.join("exec.db"))
    }

    /// Open (creating if absent) and migrate to [`SCHEMA_VERSION`].
    pub fn open(path: &Path) -> Result<Ledger, LedgerError> {
        if let Some(parent) = path.parent() {
            thruline_core::private_store::create_private_dir(parent).map_err(|source| {
                LedgerError::Io {
                    path: parent.to_path_buf(),
                    source,
                }
            })?;
        }
        let conn = Connection::open(path)?;
        // Before `prepare` sets `journal_mode`, so the WAL and shared-memory
        // files SQLite creates inherit the database's owner-only mode. This
        // store holds unredacted transcript; the umask should not decide who
        // else on the machine can read it. Advisory — see `private_store`.
        thruline_core::private_store::harden_store(path);
        Ledger::prepare(conn)
    }

    /// An in-memory ledger, for tests.
    pub fn open_in_memory() -> Result<Ledger, LedgerError> {
        Ledger::prepare(Connection::open_in_memory()?)
    }

    /// Open existing evidence without migrations or writes to the database.
    /// Older execution schemas remain readable; a missing v3 acceptance table
    /// means no acceptance was recorded there, not an implicitly accepted file.
    pub fn open_read_only(path: &Path) -> Result<Ledger, LedgerError> {
        let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.busy_timeout(Duration::from_millis(3_000))?;
        let ledger = Ledger { conn };
        let found = ledger.user_version()?;
        if found > SCHEMA_VERSION {
            return Err(LedgerError::FromTheFuture {
                found,
                supported: SCHEMA_VERSION,
            });
        }
        Ok(ledger)
    }

    fn prepare(conn: Connection) -> Result<Ledger, LedgerError> {
        // WAL (§11) and foreign keys. `busy_timeout` copies the pattern from
        // `core::sensor`, which needed it because Stop and SessionEnd arrive
        // back to back: the second writer waits rather than being lost to
        // SQLITE_BUSY.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(Duration::from_millis(3_000))?;
        let ledger = Ledger { conn };
        ledger.migrate()?;
        Ok(ledger)
    }

    fn user_version(&self) -> Result<u32, LedgerError> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?
            as u32)
    }

    fn migrate(&self) -> Result<(), LedgerError> {
        let current = self.user_version()?;
        let supported = MIGRATIONS.len() as u32;
        if current > supported {
            // Refuse loudly. An older build interpreting a newer schema is how
            // a ledger gets half-read and silently reported as complete.
            return Err(LedgerError::FromTheFuture {
                found: current,
                supported,
            });
        }
        for (index, migration) in MIGRATIONS.iter().enumerate().skip(current as usize) {
            let version = (index as u32) + 1;
            self.conn.execute_batch("BEGIN IMMEDIATE")?;
            let applied = (|| -> Result<(), LedgerError> {
                self.conn.execute_batch(migration)?;
                // `pragma_update` cannot be parameterised for user_version.
                self.conn
                    .execute_batch(&format!("PRAGMA user_version = {version}"))?;
                Ok(())
            })();
            match applied {
                Ok(()) => {
                    self.conn.execute_batch("COMMIT")?;
                    tracing::debug!(version, "applied ledger migration");
                }
                Err(error) => {
                    let _ = self.conn.execute_batch("ROLLBACK");
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn schema_version(&self) -> Result<u32, LedgerError> {
        self.user_version()
    }

    /// Get or create the namespace for `key`.
    pub fn repo(&self, key: &str, root_path: Option<&str>) -> Result<RepoId, LedgerError> {
        let id = RepoId::from_key(key);
        self.conn.execute(
            "INSERT INTO repos(id, key, root_path, created_at)
             VALUES(?1, ?2, ?3, datetime('now'))
             ON CONFLICT(id) DO NOTHING",
            params![id.as_str(), key, root_path],
        )?;
        Ok(id)
    }

    /// Register a session, or leave an existing one alone.
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_session(
        &self,
        session_id: &str,
        repo: &RepoId,
        harness: &str,
        model: Option<&str>,
        capture_degraded: bool,
        degraded_reason: Option<&str>,
    ) -> Result<(), LedgerError> {
        self.conn.execute(
            "INSERT INTO sessions(id, repo_id, harness, model, capture_degraded, degraded_reason)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
               capture_degraded = MAX(sessions.capture_degraded, excluded.capture_degraded),
               degraded_reason  = COALESCE(excluded.degraded_reason, sessions.degraded_reason)",
            params![
                session_id,
                repo.as_str(),
                harness,
                model,
                capture_degraded as i64,
                degraded_reason
            ],
        )?;
        Ok(())
    }

    /// Append a batch of sealed events in one transaction.
    ///
    /// Batched because §16 requires it (`ledger writes are batched`) and
    /// because a hook that opens a transaction per event cannot meet a 50 ms
    /// budget. `BEGIN IMMEDIATE` takes the write lock up front, so two writers
    /// serialise rather than one discovering at COMMIT that it must retry.
    pub fn append_events(&self, repo: &RepoId, rows: &[EventRow]) -> Result<u64, LedgerError> {
        if rows.is_empty() {
            return Ok(0);
        }
        // The last place a corrupt row can be rejected before the harm becomes
        // IRREVERSIBLE. `events` is append-only by trigger, so a row written
        // with a hash that does not match its contents can never be updated or
        // deleted: one forgotten re-seal permanently makes a session
        // unverifiable.
        //
        // This checked only `hash().is_none()` and therefore accepted a STALE
        // hash and wrote it — measured: carried `b8012697…`, recomputed
        // `7ffe1f5b…`, `verify_hash` correctly failing, and the row written
        // anyway. Refusing an unsealed event while accepting a mis-sealed one
        // was the weaker half of the check.
        for row in rows {
            if row.event.hash().is_none() {
                return Err(LedgerError::UnsealedEvent {
                    id: row.event.id().to_string(),
                });
            }
            row.event
                .verify_hash()
                .map_err(|source| LedgerError::CorruptEvent {
                    id: row.event.id().to_string(),
                    source,
                })?;
        }
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let written = (|| -> Result<u64, LedgerError> {
            let mut written = 0u64;
            {
                let mut insert = self.conn.prepare_cached(
                    "INSERT INTO events(
                        id, repo_id, session_id, seq, v, ts, source, kind, payload,
                        prev_hash, hash, redactions_applied, redaction_families,
                        legacy_raw_file, legacy_raw_line, legacy_raw_sha256,
                        legacy_kind, legacy_chain_hash)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)
                     ON CONFLICT(session_id, seq) DO NOTHING",
                )?;
                for row in rows {
                    let e = &row.event;
                    let source = serde_json::to_value(e.source())?
                        .as_str()
                        .unwrap_or_default()
                        .to_string();
                    let families = serde_json::to_string(&row.redaction_families)?;
                    let legacy = row.legacy.as_ref();
                    let n = insert.execute(params![
                        e.id().as_str(),
                        repo.as_str(),
                        e.session_id(),
                        e.seq() as i64,
                        e.v() as i64,
                        e.ts(),
                        source,
                        e.kind().as_str(),
                        serde_json::to_string(e.payload())?,
                        e.prev_hash(),
                        e.hash().unwrap_or_default(),
                        row.redactions_applied as i64,
                        families,
                        legacy.map(|l| l.raw_file.clone()),
                        legacy.map(|l| l.raw_line),
                        legacy.map(|l| l.raw_sha256.clone()),
                        legacy.map(|l| l.legacy_kind.clone()),
                        legacy.map(|l| l.legacy_chain_hash.clone()),
                    ])?;
                    written += n as u64;
                }
            }
            // Session head, maintained at append so reading it is one row
            // rather than a fold. Derived state: `verify_session` recomputes it
            // from `events` and the two must agree.
            let mut sessions: Vec<&str> = rows.iter().map(|r| r.event.session_id()).collect();
            sessions.sort_unstable();
            sessions.dedup();
            for session in sessions {
                self.conn.execute(
                    "UPDATE sessions SET
                       head_hash = (SELECT hash FROM events WHERE session_id = ?1
                                    ORDER BY seq DESC LIMIT 1),
                       event_count = (SELECT COUNT(*) FROM events WHERE session_id = ?1),
                       last_seq = (SELECT COALESCE(MAX(seq),0) FROM events WHERE session_id = ?1)
                     WHERE id = ?1",
                    params![session],
                )?;
            }
            Ok(written)
        })();
        match written {
            Ok(n) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(n)
            }
            Err(error) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    /// The `SELECT` both readers share, so they cannot drift apart.
    const EVENT_COLUMNS: &'static str =
        "SELECT id, session_id, seq, v, ts, source, kind, payload, prev_hash, hash
         FROM events WHERE session_id = ?1 ORDER BY seq ASC";

    /// Build an `Event` straight from its columns.
    ///
    /// Deliberately not via an intermediate `serde_json::Value` for the whole
    /// event: that allocated a map, ten keys and ten values per row on the way
    /// to a struct that was about to be dropped. Only `source` and `kind` go
    /// through a `Value`, because they are enums whose wire spellings live in
    /// serde attributes — re-deriving those spellings here would be a second
    /// rule that can disagree with the first.
    fn event_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
        use rusqlite::types::Type;
        // A generic fn, not a closure: a closure would be monomorphised to the
        // first type it is used with, and this converts three different ones.
        fn convert<T: serde::de::DeserializeOwned>(
            index: usize,
            text: String,
        ) -> rusqlite::Result<T> {
            serde_json::from_value(serde_json::Value::String(text))
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(index, Type::Text, e.into()))
        }
        let payload_text: String = row.get(7)?;
        // Through `EventWire`, so a row read back from the store passes exactly
        // the same gate as one parsed from JSON or built by `Event::new`. This
        // was the F1 bypass: the read path built an `Event` by struct literal,
        // so no invariant applied to anything the ledger returned.
        let wire = EventWire {
            id: convert(0, row.get(0)?)?,
            session_id: row.get(1)?,
            seq: row.get::<_, i64>(2)? as u64,
            v: row.get::<_, i64>(3)? as u32,
            ts: row.get(4)?,
            source: convert(5, row.get(5)?)?,
            kind: convert(6, row.get(6)?)?,
            payload: serde_json::from_str(&payload_text)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(7, Type::Text, e.into()))?,
            prev_hash: row.get(8)?,
            hash: Some(row.get(9)?),
        };
        Event::try_from(wire)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, Type::Text, e.into()))
    }

    /// Stream a session's events in `seq` order, one at a time.
    ///
    /// The reader that scales. `session_events` materialises the whole session;
    /// on a two-million-event imported session that is unbounded memory, and
    /// the ledger's own verification must not be the thing that runs a machine
    /// out of RAM.
    pub fn for_each_event<F>(&self, session_id: &str, mut visit: F) -> Result<u64, LedgerError>
    where
        F: FnMut(Event) -> Result<(), LedgerError>,
    {
        let mut stmt = self.conn.prepare_cached(Self::EVENT_COLUMNS)?;
        let mut rows = stmt.query(params![session_id])?;
        let mut seen = 0u64;
        while let Some(row) = rows.next()? {
            visit(Self::event_from_row(row)?)?;
            seen += 1;
        }
        Ok(seen)
    }

    /// Record one §3.5 promise. Idempotent on its derived id.
    ///
    /// **No DDL** — `promises` has existed since the first migration.
    pub fn insert_promise(&self, repo: &RepoId, row: &PromiseRow) -> Result<u64, LedgerError> {
        let n = self.conn.execute(
            "INSERT INTO promises(id, session_id, repo_id, text, kind, status, opened_event_id)
             VALUES(?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(id) DO NOTHING",
            params![
                row.id,
                row.session_id,
                repo.as_str(),
                row.text,
                row.kind,
                row.status,
                row.opened_event_id
            ],
        )?;
        Ok(n as u64)
    }

    /// Close a promise. `promises` is mutable by design: a promise's *status*
    /// is derived state that later evidence revises, unlike an event.
    pub fn close_promise(
        &self,
        id: &str,
        status: &str,
        closed_event_id: &str,
        reason: &str,
    ) -> Result<(), LedgerError> {
        self.conn.execute(
            "UPDATE promises SET status = ?2, closed_event_id = ?3, closed_reason = ?4
             WHERE id = ?1",
            params![id, status, closed_event_id, reason],
        )?;
        Ok(())
    }

    /// Every promise for a repository: `(id, session_id, text, kind, status)`.
    pub fn promises_for_repo(&self, repo: &RepoId) -> Result<Vec<PromiseRow>, LedgerError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, session_id, text, kind, status, COALESCE(opened_event_id,'')
             FROM promises WHERE repo_id = ?1 ORDER BY rowid",
        )?;
        let rows = stmt.query_map(params![repo.as_str()], |r| {
            Ok(PromiseRow {
                id: r.get(0)?,
                session_id: r.get(1)?,
                text: r.get(2)?,
                kind: r.get(3)?,
                status: r.get(4)?,
                opened_event_id: r.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Paths any session in this repo changed *after* `after_session` was last
    /// written to, with the session that changed them.
    ///
    /// Used to decide whether a promise was acted on later. Ordered by the
    /// global row order, which is append order.
    pub fn paths_changed_after_session(
        &self,
        repo: &RepoId,
        after_session: &str,
    ) -> Result<Vec<(String, String, String)>, LedgerError> {
        let mut stmt = self.conn.prepare(
            "SELECT session_id, id, payload FROM events
             WHERE repo_id = ?1 AND kind = 'fs_change'
               AND rowid > COALESCE((SELECT MAX(rowid) FROM events WHERE session_id = ?2), 0)
             ORDER BY rowid",
        )?;
        let rows = stmt.query_map(params![repo.as_str(), after_session], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (session, event, payload) = row?;
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload) else {
                continue;
            };
            if let Some(path) = value.get("path").and_then(serde_json::Value::as_str) {
                out.push((session, event, path.to_string()));
            }
        }
        Ok(out)
    }

    /// Insert one §3.3 `Check`.
    ///
    /// `INSERT OR IGNORE` on a deterministic id, so re-deriving a check from an
    /// event that is already stored is a no-op and the fold stays idempotent.
    /// Returns how many rows were actually written.
    ///
    /// **No DDL.** `checks` has existed since the first migration; this is the
    /// first writer for it. Adding a column here would cost a migration on
    /// every installed store, which is the reason `exec.db` exists at all
    /// (BUILD-PLAN C.2).
    pub fn insert_check(&self, row: &CheckRow) -> Result<u64, LedgerError> {
        let n = self.conn.execute(
            "INSERT INTO checks(
                id, session_id, exec_event_id, category, runner, argv_hash,
                cwd_hash, env_hash, input_tree_hash, exit_code, parsed)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
             ON CONFLICT(id) DO NOTHING",
            params![
                row.id,
                row.session_id,
                row.exec_event_id,
                row.category,
                row.runner,
                row.argv_hash,
                row.cwd_hash,
                row.env_hash,
                row.input_tree_hash,
                row.exit_code,
                row.parsed,
            ],
        )?;
        Ok(n as u64)
    }

    /// Everything needed to compare *what a tool said it ran* with *what the
    /// recorder captured*, per session.
    ///
    /// Rows are `(session_id, "tool" | "exec", text)`: for a `tool_call` the
    /// redacted tool input, for an `exec` its argv joined by a space. The
    /// caller does the recognising, because the parsers live in
    /// `crates/verify` and this crate must not depend on them — the ledger
    /// stores evidence, it does not interpret it.
    ///
    /// **Counts, not a set membership test.** The first version asked whether a
    /// session had *no* execs at all, which meant a single captured run hid
    /// every bypassed one after it: `pytest` (captured, green) followed by
    /// `/usr/bin/pytest --ignore=failing` (invisible) reported clean.
    ///
    /// The transcript is used here only to say the record is **incomplete**.
    /// §0.2 forbids a transcript deciding a verdict; it does not forbid one
    /// saying evidence is missing, because downgrading on a hint is safe where
    /// upgrading on one is not.
    pub fn runner_evidence(&self) -> Result<Vec<(String, String, String)>, LedgerError> {
        let mut stmt = self.conn.prepare(
            "SELECT session_id, kind, payload FROM events
             WHERE kind IN ('tool_call','exec')
             ORDER BY session_id, seq",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (session, kind, payload) = row?;
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload) else {
                continue;
            };
            let text = if kind == "exec" {
                let Some(argv) = value.get("argv").and_then(serde_json::Value::as_array) else {
                    continue;
                };
                argv.iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                match value
                    .get("input_redacted")
                    .and_then(serde_json::Value::as_str)
                {
                    Some(text) => text.to_string(),
                    None => continue,
                }
            };
            out.push((session, kind, text));
        }
        Ok(out)
    }

    /// Every check recorded for a session, in insertion order.
    pub fn checks_for_session(&self, session_id: &str) -> Result<Vec<CheckRow>, LedgerError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, session_id, exec_event_id, category, runner, argv_hash,
                    cwd_hash, env_hash, input_tree_hash, exit_code, parsed
             FROM checks WHERE session_id = ?1 ORDER BY rowid",
        )?;
        let rows = stmt.query_map(params![session_id], |r| {
            Ok(CheckRow {
                id: r.get(0)?,
                session_id: r.get(1)?,
                exec_event_id: r.get(2)?,
                category: r.get(3)?,
                runner: r.get(4)?,
                argv_hash: r.get(5)?,
                cwd_hash: r.get(6)?,
                env_hash: r.get(7)?,
                input_tree_hash: r.get(8)?,
                exit_code: r.get(9)?,
                parsed: r.get(10)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Every event of a session, in `seq` order.
    ///
    /// Convenient, and O(session) in memory. Prefer [`Ledger::for_each_event`]
    /// for anything that runs over a whole store.
    pub fn session_events(&self, session_id: &str) -> Result<Vec<Event>, LedgerError> {
        let mut events = Vec::new();
        self.for_each_event(session_id, |event| {
            events.push(event);
            Ok(())
        })?;
        Ok(events)
    }

    /// Read a session's events back and prove the chain.
    ///
    /// Not a convenience: the head stored on `sessions` is maintained at append
    /// and is therefore a *cache*. This is the recomputation that says whether
    /// the cache is telling the truth.
    pub fn verify_session(&self, session_id: &str) -> Result<ChainHead, LedgerError> {
        let mut walker = ChainWalker::new(session_id);
        let mut failure = None;
        let seen = self.for_each_event(session_id, |event| {
            // The walker's error is carried out rather than returned from the
            // closure, so the statement is driven to completion and the row
            // cursor is not abandoned mid-iteration.
            if failure.is_none() {
                if let Err(error) = walker.push(&event) {
                    failure = Some(error);
                }
            }
            Ok(())
        })?;
        if let Some(error) = failure {
            return Err(error.into());
        }
        if seen == 0 {
            return Err(LedgerError::UnknownSession {
                session_id: session_id.to_string(),
            });
        }
        Ok(walker.finish()?)
    }

    /// The last event actually stored for a session: `(seq, hash)`.
    ///
    /// Read from `events`, **not** from the `sessions` cache. The importer
    /// resumes from this, and resuming from a cached head is how a resumed
    /// import re-chains rows that are already stored — measured, see
    /// `an_interrupted_import_resumes_without_duplicating_and_continues_the_chain`.
    /// It is also the shape the legacy store got wrong: its
    /// `sessions.head_chain_hash` has no append-only trigger behind it, so a
    /// stored head there is a claim rather than evidence.
    pub fn last_event(&self, session_id: &str) -> Result<Option<(u64, String)>, LedgerError> {
        Ok(self
            .conn
            .query_row(
                "SELECT seq, hash FROM events WHERE session_id = ?1 ORDER BY seq DESC LIMIT 1",
                params![session_id],
                |row| Ok((row.get::<_, i64>(0)? as u64, row.get::<_, String>(1)?)),
            )
            .optional()?)
    }

    /// The highest `seq` stored for a session that came from a legacy row.
    ///
    /// Distinct from [`Ledger::last_event`] because the importer also writes a
    /// synthetic census note, and using *that* as the resume cursor would skip
    /// the legacy row occupying the same seq. Two different questions, two
    /// different queries.
    pub fn last_imported_seq(&self, session_id: &str) -> Result<Option<u64>, LedgerError> {
        Ok(self
            .conn
            .query_row(
                "SELECT MAX(seq) FROM events
                 WHERE session_id = ?1 AND legacy_raw_file IS NOT NULL",
                params![session_id],
                |row| row.get::<_, Option<i64>>(0),
            )
            .optional()?
            .flatten()
            .map(|seq| seq as u64))
    }

    /// Whether a session already carries an imported census note.
    ///
    /// The `LIKE` is **not** a table scan, and an earlier version of this
    /// comment claimed it was. `idx_events_session (session_id, seq)` serves
    /// the equality; the `LIKE` then filters only that one session's rows.
    /// Measured by the independent verifier against the real 680,182-row store:
    /// `SEARCH events USING INDEX idx_events_session (session_id=?)`,
    /// **0.029 ms per probe, 2.2 s extrapolated across all 76,203 sessions**.
    ///
    /// I reported this as quadratic and was wrong. The correction is recorded
    /// rather than quietly reverted because the "fix" was worse than the
    /// defect: it added a column and a partial index to a two-million-row
    /// table, and per CLAUDE.md a DDL change costs a full pass over the ledger
    /// on every machine that has one. An honest wrong self-report is cheap; a
    /// migration bought with it is not.
    pub fn has_census_note(&self, session_id: &str) -> Result<bool, LedgerError> {
        Ok(self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM events
                           WHERE session_id = ?1
                             AND payload LIKE '%legacy_unknown_field_census%')",
            params![session_id],
            |row| row.get::<_, i64>(0),
        )? != 0)
    }

    /// Record that a session's source transcript can no longer be re-hashed.
    pub fn mark_source_digest_unverifiable(&self, session_id: &str) -> Result<(), LedgerError> {
        self.conn.execute(
            "UPDATE sessions SET source_digest_unverifiable = 1 WHERE id = ?1",
            params![session_id],
        )?;
        Ok(())
    }

    pub fn source_digest_unverifiable(&self, session_id: &str) -> Result<bool, LedgerError> {
        Ok(self
            .conn
            .query_row(
                "SELECT source_digest_unverifiable FROM sessions WHERE id = ?1",
                params![session_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0)
            != 0)
    }

    /// The head this ledger *claims* for a session, without recomputing.
    pub fn stored_head(&self, session_id: &str) -> Result<Option<String>, LedgerError> {
        Ok(self
            .conn
            .query_row(
                "SELECT head_hash FROM sessions WHERE id = ?1",
                params![session_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }

    /// Every session id in the store, in id order.
    ///
    /// Streamed into the caller's closure rather than returned as a `Vec`: on
    /// the real corpus this is 76,203 ids, and the whole point of the reader
    /// rework is that a store-wide walk does not size itself to the store.
    pub fn for_each_session_id<F>(&self, mut visit: F) -> Result<u64, LedgerError>
    where
        F: FnMut(&str) -> Result<(), LedgerError>,
    {
        let mut stmt = self.conn.prepare("SELECT id FROM sessions ORDER BY id")?;
        let mut rows = stmt.query([])?;
        let mut seen = 0u64;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            visit(&id)?;
            seen += 1;
        }
        Ok(seen)
    }

    /// How many sessions the store holds, so a bounded list can say what it
    /// left out instead of implying it showed everything.
    pub fn session_total(&self) -> Result<u64, LedgerError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |row| {
                row.get::<_, i64>(0)
            })? as u64)
    }

    /// The `limit` most-substantial sessions, ranked by what the record
    /// actually carries.
    ///
    /// # Why not `ORDER BY id`, and why not `started_at`
    ///
    /// `for_each_session_id` orders by id, and the last row of that walk is
    /// what `explain`/`claims` open when no `--session` is given. On a real
    /// corpus that is the lexicographic maximum uuid — an arbitrary session,
    /// usually a two-event one, and the reason the product opens on nothing.
    /// This is the query that lets a person choose instead.
    ///
    /// `sessions.started_at`, `ended_at`, `stop_reason` and `model` are NULL on
    /// every one of the 44,107 rows of the operator's store, so they are not
    /// read here and are not returned. The orderable facts are `MAX(events.ts)`
    /// (real recency), `MAX(events.rowid)` (fold order — a total order that
    /// never ties), whether a `stop` was recorded, and `sessions.event_count`.
    ///
    /// The whole store is aggregated before the cut, because there is no index
    /// on `events(ts)`. That is the price of ranking honestly and it is paid
    /// once per call; it grows with the event count, not the session count.
    pub fn session_summaries(&self, limit: u32) -> Result<Vec<SessionSummary>, LedgerError> {
        self.ranked_session_summaries(limit, false)
    }

    /// Keep a bounded set of sessions for every recorded repository, so one
    /// busy repository cannot hide the others from the project picker.
    pub fn session_summaries_by_project(
        &self,
        limit: u32,
    ) -> Result<Vec<SessionSummary>, LedgerError> {
        self.ranked_session_summaries(limit, true)
    }

    fn ranked_session_summaries(
        &self,
        limit: u32,
        per_project: bool,
    ) -> Result<Vec<SessionSummary>, LedgerError> {
        let mut stmt = self.conn.prepare(
            "WITH ranked AS (SELECT
               s.id,
               COALESCE(r.key, ''),
               COALESCE(r.root_path, ''),
               s.harness,
               s.event_count,
               s.head_hash,
               s.capture_degraded,
               COALESCE(a.first_ts, ''),
               COALESCE(a.last_ts, ''),
               COALESCE(a.stops, 0),
               COALESCE(a.file_changes, 0),
               COALESCE(a.prompts, 0),
               COALESCE(a.final_messages, 0),
               ROW_NUMBER() OVER (
                 PARTITION BY s.repo_id
                 ORDER BY (COALESCE(a.stops, 0) > 0) DESC,
                          a.last_ts DESC, a.max_rowid DESC, s.id DESC
               ) AS project_rank,
               (COALESCE(a.stops, 0) > 0) AS has_stop,
               a.last_ts AS sort_ts,
               a.max_rowid AS sort_rowid
             FROM sessions s
             LEFT JOIN repos r ON r.id = s.repo_id
             LEFT JOIN (
               SELECT session_id,
                      MIN(ts)    AS first_ts,
                      MAX(ts)    AS last_ts,
                      MAX(rowid) AS max_rowid,
                      SUM(CASE WHEN kind = 'stop'        THEN 1 ELSE 0 END) AS stops,
                      SUM(CASE WHEN kind = 'fs_change'   THEN 1 ELSE 0 END) AS file_changes,
                      SUM(CASE WHEN kind = 'user_prompt' THEN 1 ELSE 0 END) AS prompts,
                      SUM(CASE WHEN kind = 'stop'
                                AND COALESCE(json_extract(payload, '$.last_assistant_message'), '') <> ''
                               THEN 1 ELSE 0 END) AS final_messages
               FROM events
               GROUP BY session_id
             ) a ON a.session_id = s.id)
             SELECT * FROM ranked
             WHERE ?2 = 0 OR project_rank <= ?1
             ORDER BY has_stop DESC, sort_ts DESC, sort_rowid DESC, id DESC
             LIMIT CASE WHEN ?2 = 1 THEN -1 ELSE ?1 END",
        )?;
        let rows = stmt.query_map(params![limit, per_project], |row| {
            let root: String = row.get(2)?;
            let first: String = row.get(7)?;
            let last: String = row.get(8)?;
            Ok(SessionSummary {
                id: row.get(0)?,
                repo_key: row.get(1)?,
                repo_root: (!root.is_empty()).then_some(root),
                harness: row.get(3)?,
                event_count: row.get::<_, i64>(4)? as u64,
                head_hash: row.get(5)?,
                capture_degraded: row.get::<_, i64>(6)? != 0,
                first_event_at: recorded_time(&first),
                last_event_at: recorded_time(&last),
                stop_events: row.get::<_, i64>(9)? as u64,
                file_change_events: row.get::<_, i64>(10)? as u64,
                user_prompts: row.get::<_, i64>(11)? as u64,
                final_messages: row.get::<_, i64>(12)? as u64,
            })
        })?;
        rows.collect::<Result<_, _>>().map_err(LedgerError::from)
    }

    /// §11: open promises across sessions for a repo.
    pub fn open_promises(&self, repo: &RepoId) -> Result<Vec<(String, String)>, LedgerError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, text FROM promises WHERE repo_id = ?1 AND status = 'open' ORDER BY id",
        )?;
        let rows = stmt.query_map(params![repo.as_str()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// §11: claims by verdict.
    pub fn claims_by_verdict(&self, verdict: &str) -> Result<Vec<(String, String)>, LedgerError> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.text FROM claims c
             JOIN verdicts v ON v.claim_id = c.id
             WHERE v.verdict = ?1 ORDER BY c.id",
        )?;
        let rows = stmt.query_map(params![verdict], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// §11: sessions by stop reason.
    pub fn sessions_by_stop_reason(&self, reason: &str) -> Result<Vec<String>, LedgerError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM sessions WHERE stop_reason = ?1 ORDER BY id")?;
        let rows = stmt.query_map(params![reason], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// §11: receipts by commit.
    pub fn receipts_by_commit(&self, commit: &str) -> Result<Vec<String>, LedgerError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM receipts WHERE commit_sha = ?1 ORDER BY created_at")?;
        let rows = stmt.query_map(params![commit], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Whether a session's capture is known to be incomplete (§13).
    /// The blocks this session's Stop gate has already issued, **chain-verified**.
    ///
    /// # What this buys, and what it does not (Track R, G2)
    ///
    /// The count that decides whether the gate fires used to live in
    /// `$THRULINE_HOME/gate/<session>.blocks`. One `printf` spent a budget that
    /// had never been used. Here the rows are append-only by trigger and
    /// chained by hash, so an edit **breaks the chain and this function says
    /// so**.
    ///
    /// **The property is DETECTABLE, not prevented, and the difference is the
    /// whole point of stating it.** An agent that could write the loose file can
    /// also write `exec.db`; SQLite is not a permission boundary and this
    /// process runs as the same user. What changed is that a forgery now has to
    /// either produce a valid chain — which requires the hashes of every block
    /// before it — or leave a chain that does not verify. The first is real
    /// work; the second is visible.
    ///
    /// And because the gate is **fail-open by design** (§4.1: a gate that hangs
    /// is worse than one that abstains), a detected tamper still ends in an
    /// allow for that stop. What it does not end in any more is silence: the
    /// caller turns `Err` into an allow **with a stated reason** on stderr,
    /// where `--debug` and `doctor` read it. Overclaiming this as "forgery
    /// prevented" would be the exact failure this product exists to catch.
    ///
    /// `Err` means *this ledger cannot be trusted to say*, which is not the
    /// same as *no blocks yet* and must never be collapsed into it — that
    /// collapse is what made `max_stop_blocks` bound nothing once already.
    pub fn gate_blocks(&self, session_id: &str) -> Result<Vec<GateBlock>, LedgerError> {
        let mut stmt = self.conn.prepare(
            "SELECT seq, claim_key, claim_kind, at, prev_hash, hash
               FROM gate_blocks WHERE session_id = ?1 ORDER BY seq",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            Ok(GateBlock {
                seq: row.get::<_, i64>(0)? as u64,
                claim_key: row.get(1)?,
                claim_kind: row.get(2)?,
                at: row.get(3)?,
                prev_hash: row.get(4)?,
                hash: row.get(5)?,
            })
        })?;
        let blocks: Vec<GateBlock> = rows.collect::<Result<_, _>>()?;

        // The chain, walked here rather than trusted. A row whose `hash` does
        // not equal the hash of its own contents, or whose `prev_hash` does not
        // equal its predecessor's `hash`, is a row somebody wrote by hand.
        let mut prev = GATE_CHAIN_GENESIS.to_string();
        for (i, b) in blocks.iter().enumerate() {
            let expected_seq = (i as u64) + 1;
            if b.seq != expected_seq {
                return Err(LedgerError::GateChainBroken {
                    session: session_id.to_string(),
                    detail: format!(
                        "row {} has seq {} where {expected_seq} was due",
                        i + 1,
                        b.seq
                    ),
                });
            }
            if b.prev_hash != prev {
                return Err(LedgerError::GateChainBroken {
                    session: session_id.to_string(),
                    detail: format!("block {} does not follow block {}", b.seq, b.seq - 1),
                });
            }
            let want =
                gate_block_hash(&prev, session_id, b.seq, &b.claim_key, &b.claim_kind, &b.at);
            if b.hash != want {
                return Err(LedgerError::GateChainBroken {
                    session: session_id.to_string(),
                    detail: format!(
                        "block {}'s contents do not hash to its recorded hash",
                        b.seq
                    ),
                });
            }
            prev = b.hash.clone();
        }
        Ok(blocks)
    }

    /// Append one block to the session's chain.
    ///
    /// Reads the chain first, so a tampered ledger refuses the write rather than
    /// extending a chain that does not verify — an honest block appended after a
    /// forged one would launder it.
    /// The timestamp is taken from SQLite (`datetime('now')`), the same source
    /// every other row in this store uses, and it is read back BEFORE hashing —
    /// a value the database generates during the insert could not be covered by
    /// the chain, and an uncovered field is a field a forger can edit freely.
    pub fn record_gate_block(
        &self,
        session_id: &str,
        claim_key: &str,
        claim_kind: &str,
    ) -> Result<u64, LedgerError> {
        let at: String = self
            .conn
            .query_row("SELECT datetime('now')", [], |row| row.get(0))?;
        let at = at.as_str();
        let existing = self.gate_blocks(session_id)?;
        let seq = existing.len() as u64 + 1;
        let prev = existing
            .last()
            .map(|b| b.hash.clone())
            .unwrap_or_else(|| GATE_CHAIN_GENESIS.to_string());
        let hash = gate_block_hash(&prev, session_id, seq, claim_key, claim_kind, at);
        self.conn.execute(
            "INSERT INTO gate_blocks(session_id, seq, claim_key, claim_kind, at, prev_hash, hash)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![session_id, seq as i64, claim_key, claim_kind, at, prev, hash],
        )?;
        Ok(seq)
    }

    /// **Why** a session's capture was degraded, as the writer recorded it.
    ///
    /// The column has existed since the schema's first version and had no
    /// reader, so every consumer of [`Ledger::capture_degraded`] got the fact
    /// without the reason and had to invent one or print none. §8.1's
    /// `degraded_causes` needs it: a receipt that says capture was degraded and
    /// gives a reader nothing to search for is the half-fix Track Q's D18
    /// warned about.
    ///
    /// `None` means the row states no reason — which is **not** the same as
    /// "not degraded", and a caller must not read it as one. No DDL: this reads
    /// a column that is already there.
    pub fn degraded_reason(&self, session_id: &str) -> Result<Option<String>, LedgerError> {
        Ok(self
            .conn
            .query_row(
                "SELECT degraded_reason FROM sessions WHERE id = ?1",
                params![session_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }

    pub fn capture_degraded(&self, session_id: &str) -> Result<bool, LedgerError> {
        Ok(self
            .conn
            .query_row(
                "SELECT capture_degraded FROM sessions WHERE id = ?1",
                params![session_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0)
            != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_append_only_and_the_version_matches() {
        assert_eq!(
            MIGRATIONS.len() as u32,
            SCHEMA_VERSION,
            "SCHEMA_VERSION must equal the number of migrations, or `open` will \
             stamp a version it did not reach"
        );
    }

    #[test]
    fn repo_ids_are_hashes_not_paths() {
        // The id reaches exported artefacts, and the key is usually an absolute
        // path with a username in it.
        let id = RepoId::from_key("/Users/someone/secret-project");
        assert_eq!(id.as_str().len(), 64);
        assert!(!id.as_str().contains("someone"));
        assert_eq!(id, RepoId::from_key("/Users/someone/secret-project"));
        assert_ne!(id, RepoId::from_key("/Users/someone/other-project"));
    }

    #[test]
    fn the_epoch_sentinel_is_not_a_recorded_time() {
        // 55,578 event rows of the operator's store carry it. Rendering it as a
        // date would be stating a fact the record does not have.
        assert_eq!(recorded_time("1970-01-01T00:00:00Z"), None);
        assert_eq!(recorded_time(""), None);
        assert_eq!(
            recorded_time("2026-09-05T13:52:53Z"),
            Some("2026-09-05T13:52:53Z".to_string())
        );
    }

    #[test]
    fn an_empty_store_lists_no_sessions_rather_than_failing() {
        let ledger = Ledger::open_in_memory().expect("ledger");
        assert_eq!(ledger.session_total().expect("total"), 0);
        assert!(ledger.session_summaries(50).expect("rows").is_empty());
        assert!(ledger
            .session_summaries_by_project(50)
            .expect("rows")
            .is_empty());
    }

    #[test]
    fn project_listing_keeps_quiet_repositories_outside_the_global_session_limit() {
        let ledger = Ledger::open_in_memory().expect("ledger");
        for (key, sessions) in [("/work/quiet", vec!["a"]), ("/work/busy", vec!["y", "z"])] {
            let repo = ledger.repo(key, Some(key)).expect("repo");
            for session in sessions {
                ledger
                    .upsert_session(session, &repo, "codex", None, false, None)
                    .expect("session");
            }
        }
        let global = ledger.session_summaries(1).expect("global rows");
        assert_eq!(global.len(), 1);
        assert_eq!(global[0].id, "z");
        let projects = ledger
            .session_summaries_by_project(1)
            .expect("project rows");
        assert_eq!(
            projects.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["z", "a"]
        );
        assert_eq!(projects[1].repo_root.as_deref(), Some("/work/quiet"));
        assert_eq!(
            ledger
                .session_summaries_by_project(2)
                .expect("two each")
                .len(),
            3
        );
        assert!(ledger
            .session_summaries_by_project(0)
            .expect("zero each")
            .is_empty());
    }

    /// `exec.db` holds unredacted command output and transcript. It was
    /// created world-readable, because `Connection::open` takes the umask.
    #[cfg(unix)]
    #[test]
    fn a_newly_created_exec_store_is_not_group_or_other_readable() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("nested").join("exec.db");
        drop(Ledger::open(&db).expect("ledger"));

        let mode = |path: &Path| {
            std::fs::symlink_metadata(path)
                .expect("stat")
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(
            mode(&db) & 0o077,
            0,
            "a new exec store is readable by group or other: {:o}",
            mode(&db)
        );
        assert_eq!(
            mode(db.parent().expect("parent")) & 0o077,
            0,
            "the directory created for a new exec store is traversable by group or other"
        );
        for suffix in ["-wal", "-shm"] {
            let mut sidecar = db.clone().into_os_string();
            sidecar.push(suffix);
            let sidecar = PathBuf::from(sidecar);
            if sidecar.exists() {
                assert_eq!(
                    mode(&sidecar) & 0o077,
                    0,
                    "a WAL sidecar is readable by group or other"
                );
            }
        }
    }
}
