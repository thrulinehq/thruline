//! MANDATE-v4 §11 (A205) — the unified observation stream.
//!
//! One append-only, hash-chained `observations` table, machine-wide (chain
//! scope `"observation"`), following the sensor / speaker / claim-verdict
//! chained-table precedent exactly: separate table, own chain scope,
//! no-update/no-delete triggers, chain content recomputable from the row's
//! own columns. The existing per-domain chains and the append-only-unchained
//! C-series tables stay untouched and authoritative for what they already
//! record — nothing is backfilled, migrated, or re-homed here. Later stages
//! write NEW facts as observations; where a legacy row and an observation
//! describe the same act, the observation's receipt points at the legacy row
//! *in addition to* a raw-evidence terminal, never instead of one.
//!
//! Three contracts this module carries:
//!
//! 1. **The envelope is §11, complete.** Every row holds the stable
//!    content-addressed id, schema version, observation type (closed set of
//!    22), source adapter, source session, repository identity, workspace
//!    identity where applicable, observed-at timestamp plus a dense ordering
//!    seq, a completeness state, a typed source receipt, and the two chain
//!    hashes.
//! 2. **Receipts terminate at raw evidence.** [`SourceReceipt`] carries
//!    exactly one [`ReceiptTerminal`] — one of the six §11 termination arms —
//!    as a non-optional field. Pointers at derived rows ([`DerivedRef`]) may
//!    ride along, but the type makes a receipt without a raw-evidence
//!    terminal unrepresentable: a receipt chain can never end at another
//!    derived claim.
//! 3. **The canonical encoding is injective and is the id.** The row's
//!    identity is the SHA-256 of a length-prefixed serialization of its
//!    content fields (the same convention as [`crate::chain`], and for the
//!    same reason: naive concatenation is ambiguous across field
//!    boundaries). Appending the same content twice is a no-op by
//!    construction — the id already exists — which is the §33 idempotency
//!    property, not a special case.
//!
//! Writers arrive per-stage; this stage fixes the vocabulary and the store.
//! Nothing on the hook path writes observations yet — the first stage that
//! does inherits the sensor's spill/short-transaction discipline (A199–A202).

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;

/// The chain scope. One machine-wide chain, per the sensor precedent: an
/// observation can concern work outside any project the ledger knows yet, and
/// the chain must not depend on a field that might be absent.
const CHAIN_SCOPE: &str = "observation";

/// The envelope schema this module writes. Bumping it is a versioned
/// migration (§7: public contracts change through versioned migrations), and
/// it is the first field of the canonical encoding, so v2 rows can never
/// collide with v1 ids.
pub const SCHEMA_VERSION: i64 = 1;

/// The §11 observation types, closed and complete: all 22, fixed now even
/// though writers arrive per-stage. The database CHECK constraint mirrors
/// this list; adding a type is a deliberate schema change, never a drive-by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObservationType {
    WorkspaceObserved,
    CommandStarted,
    CommandFinished,
    EvidenceBound,
    EvidenceStale,
    EvidenceInvalidated,
    SnapshotCreated,
    SnapshotVerified,
    CapsuleCreated,
    CapsuleOffered,
    CapsuleReceived,
    CapsuleDigestMatched,
    RepositoryIdentityMatched,
    WorkspaceIdentityMatched,
    OpenItemAcknowledged,
    OpenItemRejected,
    FirstRelatedActionObserved,
    RecoveryHumanAdjudicated,
    IncidentDetected,
    PolicyEvaluated,
    EvidencePacketCreated,
    ExternalAttestationReceived,
}

impl ObservationType {
    /// Every type, in §11's order. The closed set — tests pin its size.
    pub const ALL: [ObservationType; 22] = [
        ObservationType::WorkspaceObserved,
        ObservationType::CommandStarted,
        ObservationType::CommandFinished,
        ObservationType::EvidenceBound,
        ObservationType::EvidenceStale,
        ObservationType::EvidenceInvalidated,
        ObservationType::SnapshotCreated,
        ObservationType::SnapshotVerified,
        ObservationType::CapsuleCreated,
        ObservationType::CapsuleOffered,
        ObservationType::CapsuleReceived,
        ObservationType::CapsuleDigestMatched,
        ObservationType::RepositoryIdentityMatched,
        ObservationType::WorkspaceIdentityMatched,
        ObservationType::OpenItemAcknowledged,
        ObservationType::OpenItemRejected,
        ObservationType::FirstRelatedActionObserved,
        ObservationType::RecoveryHumanAdjudicated,
        ObservationType::IncidentDetected,
        ObservationType::PolicyEvaluated,
        ObservationType::EvidencePacketCreated,
        ObservationType::ExternalAttestationReceived,
    ];

    /// The stored string — §11's SCREAMING_SNAKE names, verbatim.
    pub fn as_str(self) -> &'static str {
        match self {
            ObservationType::WorkspaceObserved => "WORKSPACE_OBSERVED",
            ObservationType::CommandStarted => "COMMAND_STARTED",
            ObservationType::CommandFinished => "COMMAND_FINISHED",
            ObservationType::EvidenceBound => "EVIDENCE_BOUND",
            ObservationType::EvidenceStale => "EVIDENCE_STALE",
            ObservationType::EvidenceInvalidated => "EVIDENCE_INVALIDATED",
            ObservationType::SnapshotCreated => "SNAPSHOT_CREATED",
            ObservationType::SnapshotVerified => "SNAPSHOT_VERIFIED",
            ObservationType::CapsuleCreated => "CAPSULE_CREATED",
            ObservationType::CapsuleOffered => "CAPSULE_OFFERED",
            ObservationType::CapsuleReceived => "CAPSULE_RECEIVED",
            ObservationType::CapsuleDigestMatched => "CAPSULE_DIGEST_MATCHED",
            ObservationType::RepositoryIdentityMatched => "REPOSITORY_IDENTITY_MATCHED",
            ObservationType::WorkspaceIdentityMatched => "WORKSPACE_IDENTITY_MATCHED",
            ObservationType::OpenItemAcknowledged => "OPEN_ITEM_ACKNOWLEDGED",
            ObservationType::OpenItemRejected => "OPEN_ITEM_REJECTED",
            ObservationType::FirstRelatedActionObserved => "FIRST_RELATED_ACTION_OBSERVED",
            ObservationType::RecoveryHumanAdjudicated => "RECOVERY_HUMAN_ADJUDICATED",
            ObservationType::IncidentDetected => "INCIDENT_DETECTED",
            ObservationType::PolicyEvaluated => "POLICY_EVALUATED",
            ObservationType::EvidencePacketCreated => "EVIDENCE_PACKET_CREATED",
            ObservationType::ExternalAttestationReceived => "EXTERNAL_ATTESTATION_RECEIVED",
        }
    }

    /// Parse a stored string. `None` for anything outside the closed set — a
    /// row written by a future schema stays readable as a string and unknown
    /// as a type, never a panic (Rule 12).
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|t| t.as_str() == s)
    }
}

/// The completeness state of one observation — the closed §9-aligned
/// vocabulary this table uses. `complete` says the observer saw everything it
/// set out to see; `incomplete` says it knows it did not (and the receipt or
/// a later stage says what was missed); `unknown` says it cannot tell.
/// Deliberately three values and no more: anything stronger is a claim, and
/// claims live in verdict tables, not in observation envelopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Completeness {
    Complete,
    Incomplete,
    Unknown,
}

impl Completeness {
    pub fn as_str(self) -> &'static str {
        match self {
            Completeness::Complete => "complete",
            Completeness::Incomplete => "incomplete",
            Completeness::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "complete" => Some(Completeness::Complete),
            "incomplete" => Some(Completeness::Incomplete),
            "unknown" => Some(Completeness::Unknown),
            _ => None,
        }
    }
}

/// Where a receipt's evidence chain ends: §11's six termination arms,
/// verbatim. Every arm names raw evidence — a file and line, bytes on disk, a
/// captured execution, a manifest digest, a provider response, or a human who
/// said so. There is no arm for "another derived claim", on purpose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReceiptTerminal {
    /// Raw transcript file and line.
    Transcript { file: String, line: u64 },
    /// Independently observed filesystem state.
    Filesystem {
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        sha256: Option<String>,
    },
    /// Independently captured process execution.
    ProcessExecution {
        command: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i64>,
    },
    /// Local snapshot manifest, by content digest.
    SnapshotManifest { manifest_sha256: String },
    /// Provider API response, by provider name and response digest.
    ProviderResponse {
        provider: String,
        response_sha256: String,
    },
    /// Explicit human adjudication.
    HumanAdjudication {
        adjudicator: String,
        adjudicated_at: String,
    },
}

/// A pointer at a derived row — a legacy chained table, an earlier
/// observation, a verdict. Context, never evidence: it can only appear beside
/// a [`ReceiptTerminal`], because [`SourceReceipt::terminal`] is not optional.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedRef {
    /// The table holding the derived row (e.g. `"workspace_snapshots"`,
    /// `"observations"`).
    pub table: String,
    /// The row's primary key in that table.
    pub id: String,
}

/// One observation's source receipt. The structural rule of §11 — a receipt
/// chain must never terminate at another derived claim without reaching raw
/// evidence — is enforced by shape: `terminal` is mandatory, `derived_refs`
/// are optional companions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceReceipt {
    pub terminal: ReceiptTerminal,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived_refs: Vec<DerivedRef>,
    /// Fields this schema version does not know, preserved verbatim on read
    /// so a receipt written by a later version survives a round trip
    /// (unknown input is preserved, counted by the caller, never dropped).
    /// serde_json without `preserve_order` keeps this map sorted, so the
    /// canonical serialization is order-independent by construction.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

impl SourceReceipt {
    /// A receipt with just its terminal.
    pub fn new(terminal: ReceiptTerminal) -> Self {
        SourceReceipt {
            terminal,
            derived_refs: Vec::new(),
            extra: serde_json::Map::new(),
        }
    }

    /// Canonical JSON for hashing and storage. serde_json without
    /// `preserve_order` keeps map keys sorted, so unknown fields serialize in
    /// one order regardless of how they arrived — reordering a receipt's
    /// extra fields cannot change the observation id.
    pub fn to_canonical_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    /// Parse a stored receipt. An error, never a panic, on malformed text —
    /// and the caller still has the raw string in the row.
    pub fn from_json(text: &str) -> Result<Self> {
        Ok(serde_json::from_str(text)?)
    }
}

/// What a writer submits. Everything §11 requires of the envelope except the
/// parts the ledger assigns (seq, chain hashes, recorded-at).
#[derive(Debug, Clone)]
pub struct NewObservation<'a> {
    pub observation_type: ObservationType,
    /// Which adapter observed this (e.g. `"claude_code"`, `"codex"`,
    /// `"thruline"` for the product's own foreground commands).
    pub source_adapter: &'a str,
    /// The session the observation belongs to — a vendor session id, or a
    /// stated non-session identity for foreground commands.
    pub source_session: &'a str,
    /// Stable repository identity, as the workspace layer states it.
    pub repository_identity: &'a str,
    /// Workspace identity where applicable; `None` states inapplicability.
    pub workspace_identity: Option<&'a str>,
    /// When the observed thing happened (ISO-8601 UTC).
    pub observed_at: &'a str,
    pub completeness: Completeness,
    pub receipt: &'a SourceReceipt,
}

/// One observation as stored. `source_receipt` is the raw JSON exactly as
/// hashed — [`ObservationRecord::receipt`] parses it on demand.
#[derive(Debug, Clone, Serialize)]
pub struct ObservationRecord {
    pub id: String,
    pub seq: i64,
    /// When the ledger recorded the row. Bookkeeping, not evidence: it is
    /// outside the canonical encoding, so re-appending identical content at a
    /// later time is still the same observation.
    pub recorded_at: String,
    pub schema_version: i64,
    /// Stored string form. [`ObservationRecord::observation_type`] gives the
    /// typed view; a string outside the closed set stays readable here.
    pub observation_type_raw: String,
    pub source_adapter: String,
    pub source_session: String,
    pub repository_identity: String,
    pub workspace_identity: Option<String>,
    pub observed_at: String,
    pub completeness_raw: String,
    pub source_receipt: String,
    pub prev_hash: String,
    pub chain_hash: String,
}

impl ObservationRecord {
    pub fn observation_type(&self) -> Option<ObservationType> {
        ObservationType::parse(&self.observation_type_raw)
    }

    pub fn completeness(&self) -> Option<Completeness> {
        Completeness::parse(&self.completeness_raw)
    }

    /// The typed receipt. Fails loudly on text this schema cannot read; the
    /// raw string in `source_receipt` remains the stored truth either way.
    pub fn receipt(&self) -> Result<SourceReceipt> {
        SourceReceipt::from_json(&self.source_receipt)
    }
}

/// The canonical serialization of an observation's content — the id's
/// pre-image and the chain's hashed content.
///
/// Injective, like [`crate::chain::chain_hash`]'s encoding and for the same
/// reason: adjacent variable-width fields are ambiguous under naive
/// concatenation. Layout, in order:
///
/// * `schema_version` — fixed-width `i64` little-endian;
/// * each of `observation_type`, `source_adapter`, `source_session`,
///   `repository_identity` — `u64` little-endian byte length, then UTF-8
///   bytes;
/// * `workspace_identity` — one tag byte (`0x00` absent, `0x01` present),
///   then the length-prefixed value when present (so `None` and `Some("")`
///   differ);
/// * each of `observed_at`, `completeness`, `receipt_json` —
///   length-prefixed as above.
///
/// This encoding is part of the on-disk contract: changing it invalidates
/// every stored observation chain and is a versioned migration, never a
/// refactor. The golden vectors pinning it were computed outside this code.
#[allow(clippy::too_many_arguments)]
pub fn canonical_bytes(
    schema_version: i64,
    observation_type: &str,
    source_adapter: &str,
    source_session: &str,
    repository_identity: &str,
    workspace_identity: Option<&str>,
    observed_at: &str,
    completeness: &str,
    receipt_json: &str,
) -> Vec<u8> {
    fn push_lp(out: &mut Vec<u8>, s: &str) {
        out.extend_from_slice(&(s.len() as u64).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    }
    let mut out = Vec::with_capacity(
        8 + observation_type.len()
            + source_adapter.len()
            + source_session.len()
            + repository_identity.len()
            + workspace_identity.map_or(1, |w| 9 + w.len())
            + observed_at.len()
            + completeness.len()
            + receipt_json.len()
            + 8 * 7,
    );
    out.extend_from_slice(&schema_version.to_le_bytes());
    push_lp(&mut out, observation_type);
    push_lp(&mut out, source_adapter);
    push_lp(&mut out, source_session);
    push_lp(&mut out, repository_identity);
    match workspace_identity {
        None => out.push(0x00),
        Some(w) => {
            out.push(0x01);
            push_lp(&mut out, w);
        }
    }
    push_lp(&mut out, observed_at);
    push_lp(&mut out, completeness);
    push_lp(&mut out, receipt_json);
    out
}

/// The content-addressed observation id: SHA-256 of [`canonical_bytes`],
/// lowercase hex. Stable across machines and appends; identical content is
/// one observation.
#[allow(clippy::too_many_arguments)]
pub fn observation_id(
    schema_version: i64,
    observation_type: &str,
    source_adapter: &str,
    source_session: &str,
    repository_identity: &str,
    workspace_identity: Option<&str>,
    observed_at: &str,
    completeness: &str,
    receipt_json: &str,
) -> String {
    sha256_hex(&canonical_bytes(
        schema_version,
        observation_type,
        source_adapter,
        source_session,
        repository_identity,
        workspace_identity,
        observed_at,
        completeness,
        receipt_json,
    ))
}

/// The outcome of an append: a new link, or the row this content already is.
#[derive(Debug, Clone)]
pub enum AppendOutcome {
    Appended(ObservationRecord),
    /// The exact content is already on the chain — idempotency by content
    /// identity (§33). The existing row is returned untouched.
    AlreadyRecorded(ObservationRecord),
}

impl AppendOutcome {
    pub fn record(&self) -> &ObservationRecord {
        match self {
            AppendOutcome::Appended(r) | AppendOutcome::AlreadyRecorded(r) => r,
        }
    }
}

/// Append one observation. One short IMMEDIATE transaction, per the sensor
/// precedent; duplicate content is a quiet no-op returning the standing row.
/// Foreground writers only this stage — nothing here is hook-budget safe.
pub fn append(ledger: &Ledger, new: &NewObservation<'_>) -> Result<AppendOutcome> {
    let receipt_json = new.receipt.to_canonical_json()?;
    let id = observation_id(
        SCHEMA_VERSION,
        new.observation_type.as_str(),
        new.source_adapter,
        new.source_session,
        new.repository_identity,
        new.workspace_identity,
        new.observed_at,
        new.completeness.as_str(),
        &receipt_json,
    );
    let recorded_at = crate::ingest::iso8601_utc_now();
    let conn = ledger.conn();
    conn.busy_timeout(std::time::Duration::from_millis(3_000))?;
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let outcome = (|| -> Result<AppendOutcome> {
        if let Some(existing) = read_by_id_tx(conn, &id)? {
            return Ok(AppendOutcome::AlreadyRecorded(existing));
        }
        let head: Option<(String, i64)> = conn
            .query_row(
                "SELECT chain_hash, seq FROM observations ORDER BY seq DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (prev_hash, seq) = match head {
            Some((hash, last)) => (hash, last + 1),
            None => (genesis_prev_hash(CHAIN_SCOPE), 0),
        };
        let chain = chain_hash(&prev_hash, &id, CHAIN_SCOPE, seq);
        conn.execute(
            "INSERT INTO observations(
                 id, seq, recorded_at, schema_version, observation_type,
                 source_adapter, source_session, repository_identity,
                 workspace_identity, observed_at, completeness, source_receipt,
                 prev_hash, chain_hash)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            rusqlite::params![
                id,
                seq,
                recorded_at,
                SCHEMA_VERSION,
                new.observation_type.as_str(),
                new.source_adapter,
                new.source_session,
                new.repository_identity,
                new.workspace_identity,
                new.observed_at,
                new.completeness.as_str(),
                receipt_json,
                prev_hash,
                chain,
            ],
        )?;
        Ok(AppendOutcome::Appended(ObservationRecord {
            id,
            seq,
            recorded_at,
            schema_version: SCHEMA_VERSION,
            observation_type_raw: new.observation_type.as_str().to_string(),
            source_adapter: new.source_adapter.to_string(),
            source_session: new.source_session.to_string(),
            repository_identity: new.repository_identity.to_string(),
            workspace_identity: new.workspace_identity.map(str::to_owned),
            observed_at: new.observed_at.to_string(),
            completeness_raw: new.completeness.as_str().to_string(),
            source_receipt: receipt_json,
            prev_hash,
            chain_hash: chain,
        }))
    })();
    match outcome {
        Ok(stored) => {
            conn.execute_batch("COMMIT")?;
            Ok(stored)
        }
        Err(err) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(err)
        }
    }
}

const SELECT_COLUMNS: &str = "id, seq, recorded_at, schema_version, observation_type,
        source_adapter, source_session, repository_identity, workspace_identity,
        observed_at, completeness, source_receipt, prev_hash, chain_hash";

fn row_to_record(r: &rusqlite::Row<'_>) -> rusqlite::Result<ObservationRecord> {
    Ok(ObservationRecord {
        id: r.get(0)?,
        seq: r.get(1)?,
        recorded_at: r.get(2)?,
        schema_version: r.get(3)?,
        observation_type_raw: r.get(4)?,
        source_adapter: r.get(5)?,
        source_session: r.get(6)?,
        repository_identity: r.get(7)?,
        workspace_identity: r.get(8)?,
        observed_at: r.get(9)?,
        completeness_raw: r.get(10)?,
        source_receipt: r.get(11)?,
        prev_hash: r.get(12)?,
        chain_hash: r.get(13)?,
    })
}

fn read_by_id_tx(conn: &rusqlite::Connection, id: &str) -> Result<Option<ObservationRecord>> {
    Ok(conn
        .query_row(
            &format!("SELECT {SELECT_COLUMNS} FROM observations WHERE id = ?1"),
            [id],
            row_to_record,
        )
        .optional()?)
}

/// One observation by id, if it is on the chain.
pub fn read_by_id(ledger: &Ledger, id: &str) -> Result<Option<ObservationRecord>> {
    read_by_id_tx(ledger.conn(), id)
}

/// Every observation, in chain order.
pub fn read_all(ledger: &Ledger) -> Result<Vec<ObservationRecord>> {
    read_where(ledger, "1=1", &[])
}

/// A session's observations, in chain order.
pub fn for_session(ledger: &Ledger, source_session: &str) -> Result<Vec<ObservationRecord>> {
    read_where(ledger, "source_session = ?1", &[&source_session])
}

/// One type's observations, in chain order.
pub fn of_type(ledger: &Ledger, kind: ObservationType) -> Result<Vec<ObservationRecord>> {
    let s = kind.as_str();
    read_where(ledger, "observation_type = ?1", &[&s])
}

fn read_where(
    ledger: &Ledger,
    predicate: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Vec<ObservationRecord>> {
    let conn = ledger.conn();
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_COLUMNS} FROM observations WHERE {predicate} ORDER BY seq"
    ))?;
    let rows = stmt.query_map(params, row_to_record)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Re-fold the whole chain and compare link by link: the stored id must be
/// the content's own hash (recomputed from the row's columns, raw receipt
/// text included, so unknown receipt fields are hash-covered), and each link
/// must fold from the last. Returns the number of links checked; any mismatch
/// is an error naming the seq that broke.
///
/// A streaming fold — rows are verified as they arrive from the cursor,
/// never collected first. The first writer lands with this stage, so the
/// chain now grows in production; a verify that buffered the whole chain
/// into memory would age exactly as badly as the head-fold the ledger
/// already had to unlearn.
pub fn verify_chain(ledger: &Ledger) -> Result<u64> {
    let conn = ledger.conn();
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_COLUMNS} FROM observations ORDER BY seq"
    ))?;
    let rows = stmt.query_map([], row_to_record)?;
    let mut prev = genesis_prev_hash(CHAIN_SCOPE);
    let mut checked = 0u64;
    for row in rows {
        let row = row?;
        let recomputed_id = observation_id(
            row.schema_version,
            &row.observation_type_raw,
            &row.source_adapter,
            &row.source_session,
            &row.repository_identity,
            row.workspace_identity.as_deref(),
            &row.observed_at,
            &row.completeness_raw,
            &row.source_receipt,
        );
        let expect = chain_hash(&prev, &row.id, CHAIN_SCOPE, row.seq);
        if recomputed_id != row.id || row.prev_hash != prev || row.chain_hash != expect {
            return Err(CoreError::Integrity(format!(
                "observation chain does not add up at seq {}",
                row.seq
            )));
        }
        prev = row.chain_hash;
        checked += 1;
    }
    Ok(checked)
}
