//! The BUILD-SPEC §3.1 event: append-only, hash-chained, RFC 8785 canonical.
//!
//! ```text
//! { "v", "id", "session_id", "seq", "ts", "source", "kind", "payload",
//!   "prev_hash", "hash" }
//! ```
//!
//! Three properties this module holds structurally rather than by convention:
//!
//! 1. **`kind` cannot disagree with `payload`.** The only constructor takes a
//!    [`Payload`], and derives `kind` from it. A mismatched pair can enter only
//!    by deserializing foreign bytes, where [`Event::validate`] reports it.
//! 2. **The hash preimage is every field except `hash`.** It is computed by
//!    serializing the event and deleting one key, not by mirroring the field
//!    list into a second struct that can drift. `preimage_covers_every_field_but_hash`
//!    pins this: add a field, and the test proves it entered the preimage.
//! 3. **Redaction precedes hashing.** Not enforced here — it cannot be, this
//!    module never sees the raw text — but stated because it is the reason
//!    `input_redacted` / `*_excerpt` are the *only* text-bearing payload
//!    fields, and why they are named for the treatment they have already had.
//!    `thruline-capture` owns that ordering and enforces it with a type.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ulid::Ulid;
use unicode_normalization::{IsNormalized, UnicodeNormalization};

use super::canonical::{self, CanonicalError};

/// The `v` of every event this build writes.
pub const EVENT_SCHEMA_VERSION: u32 = 1;

/// The `evt_` prefix on every event id (§3.1: `"id": "evt_01J…"`).
pub const EVENT_ID_PREFIX: &str = "evt_";

/// Something is wrong with an event's shape or identity.
#[derive(Debug, thiserror::Error)]
pub enum EventError {
    /// The event could not be reduced to canonical bytes, so it has no hash.
    #[error("event is not canonicalizable")]
    Canonical(#[from] CanonicalError),
    /// An id that is not `evt_` followed by a ULID.
    #[error("{0:?} is not an event id: expected `evt_` followed by a 26-character ULID")]
    MalformedId(String),
    /// A timestamp that is not RFC 3339 UTC, which would let two spellings of
    /// one instant hash differently.
    #[error("{0:?} is not an RFC 3339 UTC timestamp (`YYYY-MM-DDTHH:MM:SS[.fff]Z`)")]
    MalformedTimestamp(String),
    /// `v` is not a version this build understands.
    #[error("event schema version {0} is not supported (this build writes and reads v{EVENT_SCHEMA_VERSION})")]
    UnsupportedVersion(u32),
    /// `kind` and `payload` describe different things.
    #[error("event declares kind {declared:?} but carries a {actual:?} payload")]
    KindPayloadMismatch { declared: String, actual: String },
    /// A `kind` this build has no payload type for. Kept as an error rather
    /// than a silent pass so an unrecognised producer is *reported*, per the
    /// project's "never silently drop" invariant.
    #[error("event kind {0:?} is not one of the §3.1 kinds")]
    UnknownKind(String),
    /// The payload does not carry the minimum fields §3.1 requires for its kind.
    #[error("payload for kind {kind:?} is malformed: {source}")]
    MalformedPayload {
        kind: String,
        #[source]
        source: serde_json::Error,
    },
    /// The event carries a hash that is not the hash of its own contents.
    #[error("event {id} carries hash {carried} but its contents hash to {computed}")]
    HashMismatch {
        id: String,
        carried: String,
        computed: String,
    },
    /// A sealed hash was expected and the event does not have one.
    #[error("event {0} is unsealed: it has no hash")]
    Unsealed(String),
    /// A number that RFC 8785 cannot carry losslessly.
    ///
    /// See [`reject_unhashable_numbers`] for why this is refused rather than
    /// canonicalized.
    #[error("{path} carries {value}, which RFC 8785 cannot represent exactly: {reason}")]
    UnhashableNumber {
        path: String,
        value: String,
        reason: &'static str,
    },
}

/// The largest integer an IEEE-754 double represents exactly: 2^53 - 1.
///
/// RFC 8785 serializes every number through ECMAScript `Number::toString`,
/// which is defined over doubles. Beyond this magnitude, distinct integers
/// share a representation.
pub const MAX_SAFE_INTEGER: i128 = 9_007_199_254_740_991;

/// `evt_` + ULID.
///
/// A newtype rather than a `String` because the prefix is load-bearing: ids of
/// other kinds (`clm_`, `chk_`, `prm_`) appear in the same JSON documents, and
/// a bare string lets one be assigned where another belongs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EventId(String);

impl EventId {
    /// A fresh id from a new ULID.
    ///
    /// ULIDs are monotonic within a millisecond only if generated through a
    /// monotonic source; this is not that, and event ordering comes from `seq`
    /// and the chain, never from id ordering.
    pub fn generate() -> Self {
        EventId(format!("{EVENT_ID_PREFIX}{}", Ulid::generate()))
    }

    /// A **deterministic** id derived from `material`.
    ///
    /// The fold uses this, and it matters more than it looks. With random ids,
    /// re-folding the same spool produces a different chain head — measured:
    /// same event count, same `last_seq`, different hash. That would make the
    /// head not a function of the evidence, and the head is exactly what a
    /// receipt publishes as its claim about the record. A rebuild that cannot
    /// reproduce it is a rebuild that cannot check it.
    ///
    /// The first 16 bytes of `sha256(material)`, read as a ULID. Every 128-bit
    /// value is a valid ULID — 26 Crockford characters carry 130 bits — so no
    /// masking is needed, and `derived_ids_are_valid_event_ids` proves it over
    /// the extremes rather than assuming it.
    ///
    /// The timestamp prefix a random ULID carries is therefore meaningless
    /// here. That is already the documented contract: ordering comes from `seq`
    /// and the chain, never from id ordering.
    pub fn derive(material: &[u8]) -> Self {
        let hex = crate::chain::sha256_hex(material);
        let mut bytes = [0u8; 16];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = u8::from_str_radix(hex.get(i * 2..i * 2 + 2).unwrap_or("00"), 16).unwrap_or(0);
        }
        EventId(format!("{EVENT_ID_PREFIX}{}", Ulid::from_bytes(bytes)))
    }

    /// Parse, rejecting anything that is not `evt_` + a valid ULID.
    pub fn parse(raw: &str) -> Result<Self, EventError> {
        let Some(body) = raw.strip_prefix(EVENT_ID_PREFIX) else {
            return Err(EventError::MalformedId(raw.to_string()));
        };
        // `Ulid::from_string` is the authority on the alphabet and the length;
        // re-deriving Crockford base32 here would be a second, divergent rule.
        if Ulid::from_string(body).is_err() {
            return Err(EventError::MalformedId(raw.to_string()));
        }
        Ok(EventId(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for EventId {
    type Error = EventError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        EventId::parse(&value)
    }
}

impl From<EventId> for String {
    fn from(value: EventId) -> Self {
        value.0
    }
}

/// Where an event came from (§3.1 `source`).
///
/// `Other` exists because the enum is a *closed* list in a document that
/// travels between machines running different builds. A source this build has
/// never heard of must round-trip byte-identically or its event's hash breaks,
/// which would look exactly like tampering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    ClaudeCodeHook,
    CodexHook,
    GeminiHook,
    CodexSessionFile,
    ExecRecorder,
    FsWatcher,
    Git,
    Thruline,
    #[serde(untagged)]
    Other(String),
}

/// What kind of thing happened (§3.1 `kind`).
///
/// See [`Source::Other`] for why the list is open at the edges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    SessionStart,
    UserPrompt,
    ToolCall,
    ToolResult,
    Exec,
    FsChange,
    Snapshot,
    CompactionPre,
    CompactionPost,
    SubagentStop,
    Stop,
    SessionEnd,
    Note,
    #[serde(untagged)]
    Other(String),
}

impl EventKind {
    /// The wire spelling, taken from the same serde attributes that write it.
    pub fn as_str(&self) -> String {
        match serde_json::to_value(self) {
            Ok(Value::String(s)) => s,
            // Unreachable: every variant serializes to a string. Falling back
            // to the Debug form keeps this total without an unwrap.
            _ => format!("{self:?}"),
        }
    }
}

/// A payload field §3.1 does not specify.
///
/// §3.1 gives minimum fields for eight of the thirteen kinds. For the other
/// five (`session_start`, `user_prompt`, `subagent_stop`, `session_end`,
/// `note`) it gives none. Rather than invent a contract and let downstream
/// code mistake the invention for the spec, those kinds carry a free-form
/// object, and the gap is a named type so it is visible at every use site.
///
/// **Open decision for the orchestrator**, not a design: `user_prompt` in
/// particular plainly wants `{ prompt_redacted, prompt_hash, cwd }`, but
/// picking those here would be guessing at a wire format that receipts will
/// later be signed over.
pub type OpenPayload = BTreeMap<String, Value>;

/// Unknown-but-preserved payload fields.
///
/// A producer on a newer build may add fields. They are inside the hash
/// preimage, so dropping them on a typed round-trip would break verification
/// of a perfectly honest event.
pub type Extra = BTreeMap<String, Value>;

/// §3.1 `tool_call`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool: String,
    pub input_hash: String,
    /// Bounded, secrets stripped — in that order (redact, then bound).
    pub input_redacted: String,
    pub cwd: String,
    #[serde(flatten)]
    pub extra: Extra,
}

/// §3.1 `tool_result`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool: String,
    pub call_id: String,
    pub ok: bool,
    pub output_hash: String,
    pub output_excerpt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// §3.1 `exec`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Exec {
    pub argv: Vec<String>,
    pub argv_hash: String,
    pub cwd: String,
    pub env_hash: String,
    pub started: String,
    pub ended: String,
    pub exit_code: i64,
    pub stdout_hash: String,
    pub stderr_hash: String,
    pub stdout_excerpt: String,
    pub stderr_excerpt: String,
    pub pid: u32,
    pub parent: u32,
    #[serde(flatten)]
    pub extra: Extra,
}

/// §3.1 `fs_change.op`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FsOp {
    Create,
    Modify,
    Delete,
    Rename,
}

/// §3.1 `fs_change`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FsChange {
    pub path: String,
    pub op: FsOp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_hash: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// §3.1 `snapshot` — the event form of [`super::snapshot::TreeSnapshot`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotPayload {
    pub tree_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_commit: Option<String>,
    pub dirty: bool,
    pub changed_paths_count: u64,
    #[serde(flatten)]
    pub extra: Extra,
}

/// §3.1 `compaction_pre`.
///
/// The three lists are `Value`s, not `Promise`/`Claim`, because those types
/// belong to `crates/claims` (§3.2, §3.5) and do not exist yet. Typing them
/// prematurely would fix a wire format for a crate that has not been designed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactionPre {
    #[serde(default)]
    pub open_promises: Vec<Value>,
    #[serde(default)]
    pub open_tasks: Vec<Value>,
    #[serde(default)]
    pub recent_claims: Vec<Value>,
    pub transcript_hash: String,
    #[serde(flatten)]
    pub extra: Extra,
}

/// §3.1 `compaction_post`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactionPost {
    pub transcript_hash: String,
    #[serde(flatten)]
    pub extra: Extra,
}

/// §3.1 `stop.reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    AgentFinished,
    HookBlocked,
    UserInterrupt,
    Error,
    PermissionDenied,
    ContextLimit,
    Unknown,
}

/// §3.1 `stop`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stop {
    pub reason: StopReason,
    pub last_assistant_message_hash: String,
    /// Bounded, and redacted before it was bounded.
    pub last_assistant_message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_hook_active: Option<bool>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A kind and its payload, inseparably.
///
/// Adjacently tagged, so `serde_json::to_value` of this yields exactly
/// `{"kind": …, "payload": …}` — the two §3.1 fields — and the pair can never
/// be built apart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum Payload {
    SessionStart(OpenPayload),
    UserPrompt(OpenPayload),
    ToolCall(ToolCall),
    ToolResult(ToolResult),
    Exec(Exec),
    FsChange(FsChange),
    Snapshot(SnapshotPayload),
    CompactionPre(CompactionPre),
    CompactionPost(CompactionPost),
    SubagentStop(OpenPayload),
    Stop(Stop),
    SessionEnd(OpenPayload),
    Note(OpenPayload),
}

impl Payload {
    /// The kind this payload *is*.
    ///
    /// A hand-written match rather than a serde round-trip, so adding a
    /// variant is a compile error here. `kind_matches_serde_tag_for_every_variant`
    /// pins that the strings agree with the serde attributes.
    pub fn kind(&self) -> EventKind {
        match self {
            Payload::SessionStart(_) => EventKind::SessionStart,
            Payload::UserPrompt(_) => EventKind::UserPrompt,
            Payload::ToolCall(_) => EventKind::ToolCall,
            Payload::ToolResult(_) => EventKind::ToolResult,
            Payload::Exec(_) => EventKind::Exec,
            Payload::FsChange(_) => EventKind::FsChange,
            Payload::Snapshot(_) => EventKind::Snapshot,
            Payload::CompactionPre(_) => EventKind::CompactionPre,
            Payload::CompactionPost(_) => EventKind::CompactionPost,
            Payload::SubagentStop(_) => EventKind::SubagentStop,
            Payload::Stop(_) => EventKind::Stop,
            Payload::SessionEnd(_) => EventKind::SessionEnd,
            Payload::Note(_) => EventKind::Note,
        }
    }

    /// The `payload` object alone, without the `kind` tag.
    fn into_body(self) -> Result<Value, EventError> {
        let kind = self.kind();
        let mut whole =
            serde_json::to_value(&self).map_err(|source| EventError::MalformedPayload {
                kind: kind.as_str(),
                source,
            })?;
        match whole.get_mut("payload").map(Value::take) {
            Some(body) => Ok(body),
            // Unreachable while `Payload` is adjacently tagged with
            // `content = "payload"` — and an ERROR rather than an empty object,
            // because the failure mode is silent and total: if a serde attribute
            // change removed the `payload` key, every event would hash as `{}`,
            // every one would still seal, and every one would still verify. A
            // chain of empty preimages that passes every check is the worst
            // possible outcome for this function, so it refuses instead.
            // `every_variant_produces_a_payload_body` proves the arm is
            // unreachable rather than asserting it in a comment.
            None => Err(EventError::MalformedPayload {
                kind: kind.as_str(),
                source: <serde_json::Error as serde::de::Error>::custom(
                    "the adjacently-tagged Payload enum produced no `payload` key",
                ),
            }),
        }
    }
}

/// BUILD-SPEC §3.1.
///
/// `hash` is `Option` because an event exists in two states and the difference
/// matters: **unsealed** (its own hash not yet computed) and **sealed**. The
/// hash is taken over the canonical bytes of the event with the `hash` field
/// absent, so the unsealed form is not a draft — it is literally the preimage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "EventWire")]
pub struct Event {
    v: u32,
    id: EventId,
    session_id: String,
    seq: u64,
    ts: String,
    source: Source,
    kind: EventKind,
    payload: Value,
    prev_hash: String,
    /// `sha256:…` of this event's canonical bytes without this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hash: Option<String>,
}

/// The §3.1 wire form: the ten fields, with no invariant attached.
///
/// This type is what parsing and row-reading produce, and it is **the only way
/// to build an [`Event`]**. Everything that must be true of a hashed event is
/// checked in `TryFrom<EventWire> for Event`, so there is exactly one gate and
/// every path goes through it.
///
/// # Why this exists
///
/// The first version enforced the hashed-number, NFC and timestamp rules inside
/// `Event::new` and left `Event` with `pub` fields and a derived `Deserialize`.
/// That is one invariant with three enforcement points, guarded at one — the
/// same shape as the predicate-with-two-callers class that cost this repository
/// seventeen rounds in `redact.rs`. The bypass was measured end to end:
/// `Event::new` refused `383151015300511101`, `serde_json::from_value` accepted
/// it, `append_events` wrote it, and it sealed to the same hash as
/// `…102` — one event substitutable for another with `verify_session` unable to
/// tell. The same route defeated the float rule and NFC normalisation.
///
/// So the fields of `Event` are private and `Deserialize` is `try_from` this.
/// A struct literal cannot be written outside this module, and
/// `crates/core/tests/spec_event_construction.rs` asserts no other path exists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventWire {
    pub v: u32,
    pub id: EventId,
    pub session_id: String,
    pub seq: u64,
    pub ts: String,
    pub source: Source,
    pub kind: EventKind,
    pub payload: Value,
    pub prev_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}

impl EventWire {
    /// Whether the gate will rewrite this wire's text to NFC.
    ///
    /// **Necessary but NOT sufficient to identify an honest NFD producer**, and
    /// the first version of this doc comment claimed otherwise. Measured on raw
    /// wires before conversion:
    ///
    /// | | `needs_normalization` | `verify_hash` after gate | [`EventWire::hash_over_original`] |
    /// |---|---|---|---|
    /// | honest NFD producer | true | fails | **matches** |
    /// | NFD-camouflaged tamperer | true | fails | **does not match** |
    ///
    /// The first two columns are identical for both, so a reader who took
    /// `true` to mean "benign" would **excuse a tamper**. That is the A209-C15
    /// shape again: a predicate whose stated purpose exceeds what it can decide.
    ///
    /// Use [`EventWire::is_honest_normalization_difference`], which requires
    /// both conditions.
    pub fn needs_normalization(&self) -> bool {
        !is_nfc(&self.payload)
            || !is_nfc(&Value::String(self.session_id.clone()))
            || !is_nfc(&Value::String(self.prev_hash.clone()))
    }

    /// The hash this wire's ORIGINAL bytes produce, before any normalisation.
    ///
    /// The column that separates an honest NFD producer from a tamperer. The
    /// producer computed its hash over these exact bytes, so for an honest
    /// event this equals the carried hash; for a tampered one it does not,
    /// because the tamper changed content the producer never hashed.
    ///
    /// Exposed because the crate previously did not, and an auditor had to
    /// hand-roll it from `to_value` + `canonical_bytes` + `digest_of_bytes` —
    /// three internal steps that must agree exactly with what sealing does, or
    /// the comparison is meaningless.
    pub fn hash_over_original(&self) -> Result<String, EventError> {
        let mut value =
            serde_json::to_value(self).map_err(|source| EventError::MalformedPayload {
                kind: self.kind.as_str(),
                source,
            })?;
        if let Value::Object(map) = &mut value {
            map.remove("hash");
        }
        Ok(canonical::digest_of_bytes(&canonical::canonical_bytes(
            &value,
        )?))
    }

    /// Whether this wire's carried hash is explained by normalisation alone.
    ///
    /// `true` means: the text is not NFC (so conversion will change it and the
    /// carried hash will stop matching) **and** the carried hash is exactly the
    /// hash of the original bytes — i.e. an honest producer on a platform that
    /// decomposes. `false` on a wire whose hash does not match its own original
    /// bytes, which is a tamper wearing NFD as camouflage.
    ///
    /// Both conditions, because either alone is decided by something the other
    /// can fake.
    pub fn is_honest_normalization_difference(&self) -> bool {
        let Some(carried) = self.hash.as_deref() else {
            return false;
        };
        if !self.needs_normalization() {
            return false;
        }
        matches!(self.hash_over_original(), Ok(computed) if computed == carried)
    }
}

impl TryFrom<EventWire> for Event {
    type Error = EventError;

    /// **The one gate.** Every `Event` in the process passed through here.
    ///
    /// Normalises rather than rejects where normalising is safe and lossless
    /// (NFC), and rejects where the content cannot be carried honestly (numbers
    /// RFC 8785 would round, timestamps with no single spelling).
    ///
    /// Deliberately does NOT reject an unknown `kind`/`source` or an unexpected
    /// `v`: those are *semantic* facts about a foreign producer, they cannot
    /// forge a hash collision, and rejecting them at parse would make a foreign
    /// event unreadable rather than reportable. [`Event::validate`] reports them.
    fn try_from(wire: EventWire) -> Result<Event, EventError> {
        validate_timestamp(&wire.ts)?;
        let mut payload = wire.payload;
        normalize_nfc(&mut payload);
        reject_unhashable_numbers(&payload, "payload")?;
        reject_unhashable_scalars(wire.seq)?;
        Ok(Event {
            v: wire.v,
            id: wire.id,
            session_id: wire.session_id.nfc().collect(),
            seq: wire.seq,
            ts: wire.ts,
            source: wire.source,
            kind: wire.kind,
            payload,
            // R2: normalised like every other hashed string. ASCII by
            // construction today — it is always `sha256:<hex>` — but the NFC
            // guarantee was stated as TOTAL and was not: an NFD `prev_hash`
            // hashed to `cf85e3c2…` against NFC's `4d9df32e…`, and
            // `needs_normalization` returned false on it, so the discriminator
            // was blind here too. A guarantee with an unstated exception is the
            // failure this module keeps finding.
            prev_hash: wire.prev_hash.nfc().collect(),
            hash: wire.hash,
        })
    }
}

impl Event {
    /// An unsealed event. `kind` is taken from `payload`, so the two agree by
    /// construction.
    ///
    /// Goes through [`EventWire`] like every other path, so the invariants are
    /// checked in exactly one place.
    pub fn new(
        session_id: impl Into<String>,
        seq: u64,
        ts: impl Into<String>,
        source: Source,
        payload: Payload,
        prev_hash: impl Into<String>,
    ) -> Result<Self, EventError> {
        let kind = payload.kind();
        Event::try_from(EventWire {
            v: EVENT_SCHEMA_VERSION,
            id: EventId::generate(),
            session_id: session_id.into(),
            seq,
            ts: ts.into(),
            source,
            kind,
            payload: payload.into_body()?,
            prev_hash: prev_hash.into(),
            hash: None,
        })
    }

    pub fn v(&self) -> u32 {
        self.v
    }
    pub fn id(&self) -> &EventId {
        &self.id
    }
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    pub fn seq(&self) -> u64 {
        self.seq
    }
    pub fn ts(&self) -> &str {
        &self.ts
    }
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn kind(&self) -> &EventKind {
        &self.kind
    }
    pub fn payload(&self) -> &Value {
        &self.payload
    }
    pub fn prev_hash(&self) -> &str {
        &self.prev_hash
    }

    /// Replace the id. Cannot violate any hashed-content invariant — an
    /// `EventId` is already validated by its own newtype — so it needs no gate.
    /// Used by the importer to keep a legacy row's identity stable, and by the
    /// golden fixture to be reproducible.
    pub fn with_id(mut self, id: EventId) -> Self {
        self.id = id;
        self.hash = None;
        self
    }

    /// Decompose into the wire form, **dropping the hash**.
    ///
    /// Named rather than a `From` impl on purpose. `From` is reachable as
    /// `.into()`, as `Into::into`, and as a function pointer — and in the A209
    /// work a text-scanning call-site census was defeated by exactly that kind
    /// of alias, passing `fmt` and `clippy` cleanly on the way through. One
    /// spelling is one thing to look for.
    ///
    /// The hash is dropped because carrying it across a decomposition is the
    /// only way to end up with an event asserting a hash it no longer has. A
    /// caller that genuinely wants that — every one of them is a tampering
    /// test — has to assign `wire.hash` back explicitly, which is unmistakable
    /// at the call site and greppable in the census.
    pub fn into_wire(self) -> EventWire {
        EventWire {
            v: self.v,
            id: self.id,
            session_id: self.session_id,
            seq: self.seq,
            ts: self.ts,
            source: self.source,
            kind: self.kind,
            payload: self.payload,
            prev_hash: self.prev_hash,
            // Deliberately not `self.hash`. See above.
            hash: None,
        }
    }

    /// Edit through the wire form and re-seal. **The only editing form
    /// production may call.**
    ///
    /// The previous API was `edit`, which preserved the carried hash so that
    /// tampering tests could build an event whose hash disagrees with its
    /// contents. That is correct for a test and a loaded gun in production: a
    /// caller who edits and forgets to re-seal gets an event that silently
    /// asserts the wrong hash, and only `verify_hash` will ever say so.
    ///
    /// So the hash-preserving form is gone. This one re-runs the construction
    /// gate *and* re-seals, and `spec_event_construction.rs` keeps a census of
    /// every non-test `into_wire` call site — belt and braces, because a census
    /// alone can be walked around by an alias and a type boundary alone cannot
    /// express "this seam is only for tests".
    pub fn edit_and_seal(self, edit: impl FnOnce(&mut EventWire)) -> Result<Event, EventError> {
        let mut wire = self.into_wire();
        edit(&mut wire);
        // Whatever the edit did to `hash`, the result is sealed fresh.
        wire.hash = None;
        Event::try_from(wire)?.seal()
    }

    /// Link this event onto a predecessor. Clears any existing hash, because
    /// the hash covered the old `prev_hash`.
    pub fn with_prev_hash(mut self, prev_hash: impl Into<String>) -> Self {
        self.prev_hash = prev_hash.into();
        self.hash = None;
        self
    }

    /// The exact bytes the hash is taken over: this event, canonicalized,
    /// with the `hash` field removed.
    ///
    /// Built by deletion rather than by a mirrored struct. A mirrored struct
    /// is a second field list that silently stops matching the first; deletion
    /// cannot, because it starts from the real one.
    pub fn hash_preimage(&self) -> Result<Vec<u8>, EventError> {
        let mut value =
            serde_json::to_value(self).map_err(|source| EventError::MalformedPayload {
                kind: self.kind.as_str(),
                source,
            })?;
        if let Value::Object(map) = &mut value {
            map.remove("hash");
        }
        Ok(canonical::canonical_bytes(&value)?)
    }

    /// `sha256:…` over [`Event::hash_preimage`].
    pub fn compute_hash(&self) -> Result<String, EventError> {
        Ok(canonical::digest_of_bytes(&self.hash_preimage()?))
    }

    /// Stamp the computed hash. Idempotent: sealing a sealed event recomputes
    /// and overwrites, which is the same value unless the event was mutated.
    pub fn seal(mut self) -> Result<Self, EventError> {
        self.hash = None;
        self.hash = Some(self.compute_hash()?);
        Ok(self)
    }

    /// The event's own hash, if it has been sealed.
    pub fn hash(&self) -> Option<&str> {
        self.hash.as_deref()
    }

    /// Recompute and compare. This is the whole tamper check for one event:
    /// every field except `hash` is inside the preimage, so any edit to any of
    /// them lands here.
    pub fn verify_hash(&self) -> Result<(), EventError> {
        let Some(carried) = self.hash.as_deref() else {
            return Err(EventError::Unsealed(self.id.to_string()));
        };
        let computed = self.compute_hash()?;
        if carried != computed {
            return Err(EventError::HashMismatch {
                id: self.id.to_string(),
                carried: carried.to_string(),
                computed,
            });
        }
        Ok(())
    }

    /// Structural checks that deserialization alone does not make:
    /// supported `v`, RFC 3339 UTC `ts`, a known `kind`, and a `payload` that
    /// carries the §3.1 minimum fields for that kind.
    ///
    /// Deliberately *not* called from `Deserialize`: an event that fails these
    /// must still be readable, or a malformed record could not be reported —
    /// only discarded, which the project's invariants forbid.
    pub fn validate(&self) -> Result<(), EventError> {
        if self.v != EVENT_SCHEMA_VERSION {
            return Err(EventError::UnsupportedVersion(self.v));
        }
        validate_timestamp(&self.ts)?;
        reject_unhashable_numbers(&self.payload, "payload")?;
        reject_unhashable_scalars(self.seq)?;
        let typed = self.payload_typed()?;
        if typed.kind() != self.kind {
            return Err(EventError::KindPayloadMismatch {
                declared: self.kind.as_str(),
                actual: typed.kind().as_str(),
            });
        }
        Ok(())
    }

    /// The payload, parsed against the §3.1 struct for this event's `kind`.
    ///
    /// A **view**. Re-serializing it is not guaranteed to reproduce
    /// [`Event::payload`] byte-for-byte, so never build a new event from one;
    /// the authoritative payload is the `Value` the event carries.
    pub fn payload_typed(&self) -> Result<Payload, EventError> {
        if let EventKind::Other(name) = &self.kind {
            return Err(EventError::UnknownKind(name.clone()));
        }
        let tagged = serde_json::json!({
            "kind": self.kind,
            "payload": self.payload,
        });
        serde_json::from_value(tagged).map_err(|source| EventError::MalformedPayload {
            kind: self.kind.as_str(),
            source,
        })
    }
}

/// Rewrite every string in `value` to Unicode Normalization Form C.
///
/// **Invariant N2.** RFC 8785 deliberately does *not* normalise — its own
/// reference corpus carries an "Unnormalized Unicode" vector — so `"Å"`
/// (U+00C5) and `"A" + U+030A` are different strings with different hashes.
/// Correct, and a silent cross-platform fork:
///
/// > macOS filesystems commonly hand back **decomposed** filenames while Linux
/// > hands back whatever was written. The same repository, the same file,
/// > captured on two machines, yields two different `path` strings and
/// > therefore two different chains — with no tampering anywhere.
///
/// That failure has a specific victim: BUILD-SPEC M2's done-when is *"a receipt
/// signed on one machine verifies on another"*. It would fail there, in the
/// field, on a repository with one accented filename, and the symptom would look
/// exactly like forgery.
///
/// So normalisation happens **once, at construction**, before the hash exists —
/// not at verification, where it would mean re-deriving a preimage the signer
/// never produced. NFC rather than NFD because it is the web and JSON default,
/// and because it is what a Linux-authored filename usually already is, so the
/// common case is a no-op.
///
/// This does rewrite recorded text, which is a real cost and worth stating: a
/// producer that deliberately wrote decomposed text will find composed text in
/// the ledger. Every normalisation form denotes the same abstract characters, so
/// nothing a reader can see is lost — but the bytes are not the producer's.
pub fn normalize_nfc(value: &mut Value) {
    match value {
        Value::String(s) => {
            if !unicode_normalization::is_nfc_quick(s.chars()).eq(&IsNormalized::Yes) {
                *s = s.nfc().collect();
            }
        }
        Value::Array(items) => items.iter_mut().for_each(normalize_nfc),
        Value::Object(map) => {
            // Keys too: an object key is inside the canonical bytes and sorts by
            // UTF-16 code unit, so a decomposed key changes both the hash and
            // the field order.
            let needs = map
                .keys()
                .any(|k| !unicode_normalization::is_nfc_quick(k.chars()).eq(&IsNormalized::Yes));
            if needs {
                let rebuilt: serde_json::Map<String, Value> = map
                    .iter()
                    .map(|(k, v)| (k.nfc().collect::<String>(), v.clone()))
                    .collect();
                *map = rebuilt;
            }
            map.values_mut().for_each(normalize_nfc);
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

/// Whether every string in `value` is already NFC.
pub fn is_nfc(value: &Value) -> bool {
    match value {
        Value::String(s) => unicode_normalization::is_nfc_quick(s.chars()) == IsNormalized::Yes,
        Value::Array(items) => items.iter().all(is_nfc),
        Value::Object(map) => map.iter().all(|(k, v)| {
            unicode_normalization::is_nfc_quick(k.chars()) == IsNormalized::Yes && is_nfc(v)
        }),
        Value::Null | Value::Bool(_) | Value::Number(_) => true,
    }
}

/// Refuse any number a hash preimage cannot carry losslessly.
///
/// **BUILD-PLAN C.4 decision D2, widened by measurement.** Two rules, and the
/// second was found by a property test rather than by reading:
///
/// **1. No non-integer numbers.** RFC 8785 serializes numbers with ECMAScript
/// `Number::toString`. This crate implements that correctly, but it is the
/// single most common source of disagreement between conforming
/// implementations, because it demands shortest-round-trip decimal output that
/// most languages' default float printers do not produce. An event whose hash
/// depends on `333333333.33333329` printing as `333333333.3333333` depends on
/// every future reader, in every language, implementing Ryū correctly. Nothing
/// in §3.1 needs a float — durations are integer milliseconds, exit codes and
/// counts are integers — so the class is removed rather than managed.
///
/// **2. No integers outside ±(2^53 − 1).** `Number::toString` is defined over
/// doubles, so an integer beyond [`MAX_SAFE_INTEGER`] is *rounded* on its way
/// into the preimage. That is not a formatting nicety; it is a hash collision.
/// Measured, through the real canonicalizer:
///
/// ```text
///   383151015300511101 -> 383151015300511100
///   383151015300511102 -> 383151015300511100
///         u64::MAX     -> 18446744073709552000
/// ```
///
/// Two distinct events differing only in such a field produce **the same
/// hash** — an event could be swapped for another and the chain would still
/// verify. `hashed_numbers_beyond_the_safe_integer_range_would_collide` pins
/// the collision pair so the reason is not merely asserted.
///
/// This rule is why `seq` is checked as well as the payload: `seq` is a `u64`
/// on the event itself, and a session that reached 2^53 events would start
/// producing colliding preimages.
///
/// Percentages that genuinely want a fraction (§3.3's
/// `coverage_changed_lines_pct: 91.2`) and quantities beyond the safe range
/// live in the *receipt predicate*, which DSSE signs as exact bytes and never
/// re-canonicalizes, or are carried in events as integer basis points and
/// strings.
pub fn reject_unhashable_numbers(value: &Value, path: &str) -> Result<(), EventError> {
    match value {
        Value::Number(n) => {
            let magnitude = if let Some(i) = n.as_i64() {
                i128::from(i)
            } else if let Some(u) = n.as_u64() {
                i128::from(u)
            } else {
                return Err(EventError::UnhashableNumber {
                    path: path.to_string(),
                    value: n.to_string(),
                    reason: "hashed numbers are integers only — RFC 8785 float \
                             serialization is the classic cross-implementation \
                             divergence and no §3.1 payload needs one",
                });
            };
            if !(-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&magnitude) {
                return Err(EventError::UnhashableNumber {
                    path: path.to_string(),
                    value: n.to_string(),
                    reason: "beyond ±(2^53 − 1), RFC 8785 rounds through an IEEE-754 \
                             double, so distinct values share a hash preimage",
                });
            }
            Ok(())
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                reject_unhashable_numbers(item, &format!("{path}[{i}]"))?;
            }
            Ok(())
        }
        Value::Object(map) => {
            for (key, item) in map {
                reject_unhashable_numbers(item, &format!("{path}.{key}"))?;
            }
            Ok(())
        }
        Value::Null | Value::Bool(_) | Value::String(_) => Ok(()),
    }
}

/// Every number on the event itself that is not inside `payload`.
///
/// `v` is a small constant. `seq` is the one that can grow.
fn reject_unhashable_scalars(seq: u64) -> Result<(), EventError> {
    reject_unhashable_numbers(&Value::from(seq), "seq")
}

/// `YYYY-MM-DDTHH:MM:SS` with an optional fractional part, and `Z`.
///
/// Strict on purpose. `+00:00` and `Z` name the same instant and hash
/// differently, so accepting both would make the chain depend on a producer's
/// formatting choice. A full date-time library is not pulled in for this: the
/// grammar is fixed-width, and range-checking months and days is arithmetic,
/// not calendar logic.
fn validate_timestamp(ts: &str) -> Result<(), EventError> {
    let malformed = || EventError::MalformedTimestamp(ts.to_string());
    let body = ts.strip_suffix('Z').ok_or_else(malformed)?;
    let (date, time) = body.split_once('T').ok_or_else(malformed)?;

    let mut date_parts = date.split('-');
    let (y, mo, d) = match (date_parts.next(), date_parts.next(), date_parts.next()) {
        (Some(y), Some(mo), Some(d)) if date_parts.next().is_none() => (y, mo, d),
        _ => return Err(malformed()),
    };
    let year = fixed_width_number(y, 4).ok_or_else(malformed)?;
    let month = fixed_width_number(mo, 2).ok_or_else(malformed)?;
    let day = fixed_width_number(d, 2).ok_or_else(malformed)?;

    let (clock, frac) = match time.split_once('.') {
        Some((clock, frac)) => (clock, Some(frac)),
        None => (time, None),
    };
    let mut clock_parts = clock.split(':');
    let (h, mi, s) = match (clock_parts.next(), clock_parts.next(), clock_parts.next()) {
        (Some(h), Some(mi), Some(s)) if clock_parts.next().is_none() => (h, mi, s),
        _ => return Err(malformed()),
    };
    let hour = fixed_width_number(h, 2).ok_or_else(malformed)?;
    let minute = fixed_width_number(mi, 2).ok_or_else(malformed)?;
    let second = fixed_width_number(s, 2).ok_or_else(malformed)?;

    if let Some(frac) = frac {
        if frac.is_empty() || !frac.bytes().all(|b| b.is_ascii_digit()) {
            return Err(malformed());
        }
    }

    // Range, not calendar: a leap-second `60` is legal RFC 3339, and Feb 30 is
    // rejected by the day bound for the month rather than by a real calendar.
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return Err(malformed()),
    };
    if day == 0 || day > days_in_month || hour > 23 || minute > 59 || second > 60 {
        return Err(malformed());
    }
    Ok(())
}

/// Parse exactly `width` ASCII digits, rejecting `+`, `-`, spaces and short
/// forms that `str::parse` would otherwise accept.
fn fixed_width_number(s: &str, width: usize) -> Option<u32> {
    if s.len() != width || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_payloads() -> Vec<Payload> {
        vec![
            Payload::SessionStart(OpenPayload::new()),
            Payload::UserPrompt(OpenPayload::new()),
            Payload::ToolCall(ToolCall {
                tool: "Bash".into(),
                input_hash: "sha256:aa".into(),
                input_redacted: "ls".into(),
                cwd: "/repo".into(),
                extra: Extra::new(),
            }),
            Payload::ToolResult(ToolResult {
                tool: "Bash".into(),
                call_id: "call_1".into(),
                ok: true,
                output_hash: "sha256:bb".into(),
                output_excerpt: "ok".into(),
                exit_code: Some(0),
                extra: Extra::new(),
            }),
            Payload::Exec(Exec {
                argv: vec!["cargo".into(), "test".into()],
                argv_hash: "sha256:cc".into(),
                cwd: "/repo".into(),
                env_hash: "sha256:dd".into(),
                started: "2026-09-02T10:11:12.345Z".into(),
                ended: "2026-09-02T10:11:13.000Z".into(),
                exit_code: 0,
                stdout_hash: "sha256:ee".into(),
                stderr_hash: "sha256:ff".into(),
                stdout_excerpt: "1 passed".into(),
                stderr_excerpt: String::new(),
                pid: 42,
                parent: 41,
                extra: Extra::new(),
            }),
            Payload::FsChange(FsChange {
                path: "src/lib.rs".into(),
                op: FsOp::Modify,
                before_hash: Some("sha256:11".into()),
                after_hash: Some("sha256:22".into()),
                extra: Extra::new(),
            }),
            Payload::Snapshot(SnapshotPayload {
                tree_hash: "4b825dc642cb6eb9a060e54bf8d69288fbee4904".into(),
                head_commit: None,
                dirty: false,
                changed_paths_count: 0,
                extra: Extra::new(),
            }),
            Payload::CompactionPre(CompactionPre {
                open_promises: vec![json!({"text": "add tests"})],
                open_tasks: vec![],
                recent_claims: vec![],
                transcript_hash: "sha256:33".into(),
                extra: Extra::new(),
            }),
            Payload::CompactionPost(CompactionPost {
                transcript_hash: "sha256:44".into(),
                extra: Extra::new(),
            }),
            Payload::SubagentStop(OpenPayload::new()),
            Payload::Stop(Stop {
                reason: StopReason::AgentFinished,
                last_assistant_message_hash: "sha256:55".into(),
                last_assistant_message: "All tests pass".into(),
                stop_hook_active: Some(false),
                extra: Extra::new(),
            }),
            Payload::SessionEnd(OpenPayload::new()),
            Payload::Note(OpenPayload::new()),
        ]
    }

    fn event_with(payload: Payload) -> Event {
        Event::new(
            "sess-1",
            1,
            "2026-09-02T10:11:12.345Z",
            Source::ClaudeCodeHook,
            payload,
            "sha256:00",
        )
        .expect("constructible")
    }

    #[test]
    fn every_kind_is_covered_by_a_sample_and_the_tags_agree_with_serde() {
        // Guards the hand-written `Payload::kind` match against the serde
        // rename attributes drifting apart.
        let samples = sample_payloads();
        assert_eq!(samples.len(), 13, "§3.1 lists thirteen kinds");
        for payload in samples {
            let declared = payload.kind();
            let tagged = serde_json::to_value(&payload).expect("serializable");
            let serde_tag = tagged
                .get("kind")
                .and_then(Value::as_str)
                .expect("adjacently tagged");
            assert_eq!(
                declared.as_str(),
                serde_tag,
                "Payload::kind disagrees with the serde tag for {payload:?}"
            );
        }
    }

    #[test]
    fn every_variant_produces_a_payload_body() {
        // Pins that `Payload::into_body`'s error arm is unreachable. Without
        // this the arm is an untested claim about a serde attribute, and the
        // thing it guards against — every payload hashing as `{}` while every
        // event still seals and verifies — is silent and total.
        for payload in sample_payloads() {
            let tagged = serde_json::to_value(&payload).expect("serializable");
            assert!(
                tagged.get("payload").is_some(),
                "the adjacent tag lost its content key for {payload:?}"
            );
            let kind = payload.kind();
            let body = payload.into_body().expect("has a body");
            // And the body is the payload object, not the whole tagged form.
            assert!(
                body.get("kind").is_none(),
                "{}: into_body returned the tagged wrapper, not the payload",
                kind.as_str()
            );
        }
    }

    #[test]
    fn wire_shape_is_the_ten_fields_of_section_3_1() {
        let event = event_with(sample_payloads()[4].clone())
            .seal()
            .expect("seals");
        let value = serde_json::to_value(&event).expect("serializable");
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "hash",
                "id",
                "kind",
                "payload",
                "prev_hash",
                "seq",
                "session_id",
                "source",
                "ts",
                "v",
            ]
        );
        assert_eq!(value["kind"], json!("exec"));
        assert_eq!(value["source"], json!("claude-code-hook"));
        assert_eq!(value["v"], json!(1));
        assert!(event.id.as_str().starts_with("evt_"));
    }

    #[test]
    fn preimage_covers_every_field_but_hash() {
        // The drift guard. If a field is added to `Event` and the preimage
        // stops covering it, that field becomes silently unauthenticated —
        // editable without breaking any hash. This derives both sides from the
        // real struct, so it cannot go stale.
        let event = event_with(sample_payloads()[2].clone())
            .seal()
            .expect("seals");
        let full = serde_json::to_value(&event).expect("serializable");
        let preimage: Value =
            serde_json::from_slice(&event.hash_preimage().expect("preimage")).expect("json");

        let full_keys: Vec<&String> = full.as_object().expect("object").keys().collect();
        let pre_keys: Vec<&String> = preimage.as_object().expect("object").keys().collect();

        assert!(full_keys.iter().any(|k| *k == "hash"));
        assert!(!pre_keys.iter().any(|k| *k == "hash"));
        for key in &full_keys {
            if *key == "hash" {
                continue;
            }
            assert!(
                pre_keys.contains(key),
                "field {key:?} is outside the hash preimage — it would be editable \
                 without breaking the chain"
            );
            assert_eq!(
                &full[*key], &preimage[*key],
                "field {key} differs in the preimage"
            );
        }
        assert_eq!(pre_keys.len(), full_keys.len() - 1);
    }

    #[test]
    fn sealing_is_deterministic_and_verifies() {
        for payload in sample_payloads() {
            let event = event_with(payload).seal().expect("seals");
            event.verify_hash().expect("verifies");
            let again = event.clone().seal().expect("reseals");
            assert_eq!(event.hash, again.hash, "sealing is not deterministic");
            assert!(event.hash().unwrap_or_default().starts_with("sha256:"));
        }
    }

    #[test]
    fn an_unsealed_event_does_not_verify() {
        let event = event_with(sample_payloads()[0].clone());
        assert!(matches!(event.verify_hash(), Err(EventError::Unsealed(_))));
    }

    #[test]
    fn every_field_is_authenticated_by_the_hash() {
        // One mutation per field, each asserted to break verification. This is
        // the unit-level statement of the property the chain proptest makes
        // over whole sessions.
        let base = event_with(sample_payloads()[4].clone())
            .seal()
            .expect("seals");
        type Mutator = (&'static str, fn(&mut Event));
        let mutators: Vec<Mutator> = vec![
            ("v", |e| e.v = 2),
            ("id", |e| e.id = EventId::generate()),
            ("session_id", |e| e.session_id.push('x')),
            ("seq", |e| e.seq += 1),
            ("ts", |e| e.ts = "2026-09-02T10:11:12.346Z".into()),
            ("source", |e| e.source = Source::Git),
            ("kind", |e| e.kind = EventKind::Note),
            ("payload", |e| {
                if let Value::Object(map) = &mut e.payload {
                    map.insert("exit_code".into(), json!(1));
                }
            }),
            ("prev_hash", |e| e.prev_hash.push('0')),
        ];
        for (field, mutate) in mutators {
            let mut tampered = base.clone();
            mutate(&mut tampered);
            assert_ne!(tampered, base, "mutator for {field} did nothing");
            assert!(
                matches!(tampered.verify_hash(), Err(EventError::HashMismatch { .. })),
                "mutating {field} did not break the event hash"
            );
        }
    }

    #[test]
    fn foreign_fields_in_a_payload_survive_a_typed_round_trip() {
        // A newer producer's extra field is inside the preimage. Dropping it on
        // parse would make an honest event fail verification downstream.
        let raw = json!({
            "v": 1,
            "id": EventId::generate(),
            "session_id": "s",
            "seq": 7,
            "ts": "2026-09-02T10:11:12.345Z",
            "source": "exec-recorder",
            "kind": "tool_call",
            "payload": {
                "tool": "Bash", "input_hash": "sha256:aa",
                "input_redacted": "ls", "cwd": "/r",
                "shell_profile": "zsh"
            },
            "prev_hash": "sha256:00"
        });
        let event: Event = serde_json::from_value(raw).expect("parses");
        event.validate().expect("valid");
        let Payload::ToolCall(call) = event.payload_typed().expect("typed") else {
            panic!("wrong variant");
        };
        assert_eq!(call.extra.get("shell_profile"), Some(&json!("zsh")));
        assert_eq!(
            serde_json::to_value(Payload::ToolCall(call)).expect("re-serializes")["payload"],
            event.payload,
            "typed round-trip lost or added a field"
        );
    }

    #[test]
    fn unknown_kinds_and_sources_round_trip_instead_of_failing() {
        let raw = json!({
            "v": 1, "id": EventId::generate(), "session_id": "s", "seq": 1,
            "ts": "2026-09-02T10:11:12.345Z",
            "source": "cursor-hook", "kind": "context_swap",
            "payload": {"a": 1}, "prev_hash": "sha256:00"
        });
        let event: Event = serde_json::from_value(raw.clone()).expect("parses");
        assert_eq!(event.source, Source::Other("cursor-hook".into()));
        assert_eq!(event.kind, EventKind::Other("context_swap".into()));
        assert_eq!(
            serde_json::to_value(&event).expect("re-serializes"),
            raw,
            "an unrecognised producer's event must round-trip byte-identically \
             or its hash breaks and honest data looks tampered"
        );
        // Reported, not silently accepted.
        assert!(matches!(event.validate(), Err(EventError::UnknownKind(k)) if k == "context_swap"));
    }

    #[test]
    fn a_kind_payload_mismatch_is_reported() {
        let raw = json!({
            "v": 1, "id": EventId::generate(), "session_id": "s", "seq": 1,
            "ts": "2026-09-02T10:11:12.345Z", "source": "git",
            "kind": "snapshot",
            "payload": {"tool": "Bash", "input_hash": "x", "input_redacted": "y", "cwd": "/"},
            "prev_hash": "sha256:00"
        });
        let event: Event = serde_json::from_value(raw).expect("parses");
        assert!(matches!(
            event.validate(),
            Err(EventError::MalformedPayload { .. })
        ));
    }

    #[test]
    fn floats_are_refused_everywhere_inside_a_payload() {
        // Nested, so this cannot pass by only checking the top level.
        let mut payload = OpenPayload::new();
        payload.insert(
            "outer".into(),
            json!({"list": [1, 2, {"deep": 91.2}], "ok": 7}),
        );
        let err = Event::new(
            "s",
            1,
            "2026-09-02T10:11:12.345Z",
            Source::Thruline,
            Payload::Note(payload),
            "sha256:00",
        )
        .expect_err("a float must not enter a hash preimage");
        assert!(
            matches!(&err, EventError::UnhashableNumber { path, .. }
                     if path == "payload.outer.list[2].deep"),
            "the error must name the exact path, got {err}"
        );

        // Integers within the safe range, of both signs, are fine.
        let mut ok = OpenPayload::new();
        ok.insert("a".into(), json!([-1i64, 0, MAX_SAFE_INTEGER as i64]));
        Event::new(
            "s",
            1,
            "2026-09-02T10:11:12.345Z",
            Source::Thruline,
            Payload::Note(ok),
            "sha256:00",
        )
        .expect("safe integers are fine");

        // And DESERIALIZATION refuses it too. This is the F1 hole: the rule
        // used to live in `Event::new` only, so `serde_json::from_value`
        // accepted a float, `append_events` wrote it, and the ledger's own
        // read path built an `Event` that had never seen the gate.
        let raw = json!({
            "v": 1, "id": EventId::generate(), "session_id": "s", "seq": 1,
            "ts": "2026-09-02T10:11:12.345Z", "source": "git", "kind": "note",
            "payload": {"pct": 91.2}, "prev_hash": "sha256:00"
        });
        assert!(
            serde_json::from_value::<Event>(raw).is_err(),
            "deserialization bypassed the number gate"
        );
    }

    #[test]
    fn hashed_numbers_beyond_the_safe_integer_range_would_collide() {
        // The measurement behind the rule, not a restatement of it. These two
        // integers differ by one and canonicalize to the SAME bytes, because
        // RFC 8785 routes every number through an IEEE-754 double. Left
        // unchecked, two different events hash identically and one can be
        // substituted for the other inside a verifying chain.
        let a = 383_151_015_300_511_101u64;
        let b = 383_151_015_300_511_102u64;
        assert_ne!(a, b);
        let ca = canonical::canonical_string(&json!(a)).expect("canonicalizable");
        let cb = canonical::canonical_string(&json!(b)).expect("canonicalizable");
        assert_eq!(
            ca, cb,
            "if these ever differ, RFC 8785 number handling changed and this rule \
             should be re-derived rather than kept out of habit"
        );
        assert_eq!(ca, "383151015300511100");

        // So both are refused, in the payload...
        for n in [a, b, u64::MAX] {
            let mut payload = OpenPayload::new();
            payload.insert("n".into(), json!(n));
            assert!(
                matches!(
                    Event::new(
                        "s",
                        1,
                        "2026-09-02T10:11:12.345Z",
                        Source::Thruline,
                        Payload::Note(payload),
                        "sha256:00"
                    ),
                    Err(EventError::UnhashableNumber { .. })
                ),
                "{n} was allowed into a hash preimage"
            );
        }

        // ...and in `seq`, which is a u64 on the event itself and is the field
        // that can actually grow into this range.
        assert!(matches!(
            Event::new(
                "s",
                a,
                "2026-09-02T10:11:12.345Z",
                Source::Thruline,
                Payload::Note(OpenPayload::new()),
                "sha256:00"
            ),
            Err(EventError::UnhashableNumber { path, .. }) if path == "seq"
        ));
        // The boundary itself is allowed.
        Event::new(
            "s",
            MAX_SAFE_INTEGER as u64,
            "2026-09-02T10:11:12.345Z",
            Source::Thruline,
            Payload::Note(OpenPayload::new()),
            "sha256:00",
        )
        .expect("2^53 - 1 is exactly representable");
    }

    #[test]
    fn nanosecond_precision_timestamps_do_not_collide() {
        // `ts` is a STRING, deliberately. Had it been a number of nanoseconds,
        // it would sit ~197x past 2^53 and two events 256 ns apart would hash
        // identically — the same collision class as
        // `hashed_numbers_beyond_the_safe_integer_range_would_collide`, reached
        // by a field nobody would think of as large. Pinned here so a future
        // "optimisation" to an integer timestamp fails loudly.
        let a = "2026-09-02T10:11:12.000000001Z";
        let b = "2026-09-02T10:11:12.000000002Z";
        let make = |ts: &str| {
            Event::new(
                "s",
                1,
                ts,
                Source::Thruline,
                Payload::Note(OpenPayload::new()),
                "sha256:00",
            )
            .expect("constructible")
            .seal()
            .expect("seals")
        };
        let (ea, eb) = (make(a), make(b));
        assert_ne!(
            ea.hash(),
            eb.hash(),
            "two events one nanosecond apart hash identically"
        );
    }

    #[test]
    fn construction_normalizes_to_nfc_so_two_platforms_agree() {
        // Invariant N2, and the test that would have caught M2's done-when
        // failing in the field. The warning that came with the ruling matters
        // more than the rule: an all-ASCII proptest passes while this is
        // completely broken, so the cases here are deliberately composed and
        // decomposed pairs plus astral-plane text.
        // A FIXED id on both sides. The first version of this test let
        // `Event::new` mint a fresh ULID for each, so the two hashes differed
        // no matter what normalisation did — the assertion could not fail for
        // the reason it was written for, and its `assert_ne!` predecessor
        // passed for that same empty reason.
        let fixed = EventId::parse("evt_01ARZ3NDEKTSV4RRFFQ69G5FAV").expect("valid id");

        let cases: [(&str, &str); 4] = [
            ("\u{c5}", "A\u{30a}"),          // Å
            ("caf\u{e9}", "cafe\u{301}"),    // café
            ("\u{1e69}", "s\u{323}\u{307}"), // ṩ, two combining marks
            ("\u{fb01}ne", "\u{fb01}ne"),    // ﬁ ligature: NFC leaves it alone
        ];
        for (composed, decomposed) in cases {
            let make = |text: &str| {
                let mut payload = OpenPayload::new();
                payload.insert("path".into(), json!(text));
                payload.insert(text.to_string(), json!("as a key too"));
                Event::new(
                    "s",
                    1,
                    "2026-09-02T10:11:12.345Z",
                    Source::FsWatcher,
                    Payload::Note(payload),
                    "sha256:00",
                )
                .expect("constructible")
            };
            let mut a = make(composed);
            let mut b = make(decomposed);
            a.id = fixed.clone();
            b.id = fixed.clone();
            let a = a.seal().expect("seals");
            let b = b.seal().expect("seals");
            assert_eq!(
                a.hash(),
                b.hash(),
                "the two normalisation forms of {composed:?} still hash differently — \
                 the same file captured on macOS and on Linux would fork the chain"
            );
            a.validate().expect("valid");
        }

        // The session id too, since it is inside the preimage.
        let by_form = |sid: &str| {
            let mut event = Event::new(
                sid,
                1,
                "2026-09-02T10:11:12.345Z",
                Source::FsWatcher,
                Payload::Note(OpenPayload::new()),
                "sha256:00",
            )
            .expect("constructible");
            event.id = fixed.clone();
            event.seal().expect("seals")
        };
        assert_eq!(by_form("caf\u{e9}").hash(), by_form("cafe\u{301}").hash());

        // The control that makes the assertions above mean something: with the
        // id fixed, two events that genuinely differ must still hash
        // differently. Without it, a `make` that returned a constant would
        // satisfy every equality above.
        assert_ne!(by_form("caf\u{e9}").hash(), by_form("tea").hash());

        // Astral-plane text is untouched — NFC must not mangle what it does not
        // decompose, and a normaliser that dropped it would pass every
        // assertion above.
        let mut payload = OpenPayload::new();
        payload.insert("emoji".into(), json!("fix 😂 the \u{10ffff} test"));
        let event = Event::new(
            "s",
            1,
            "2026-09-02T10:11:12.345Z",
            Source::Thruline,
            Payload::Note(payload),
            "sha256:00",
        )
        .expect("constructible");
        assert_eq!(event.payload["emoji"], json!("fix 😂 the \u{10ffff} test"));
    }

    #[test]
    fn a_foreign_event_carrying_decomposed_text_is_normalized_and_distinguishable() {
        // The gate normalises on the way in, so nothing in this process ever
        // holds non-NFC text and our chains cannot fork across platforms.
        //
        // The consequence is real and stated rather than hidden: a foreign
        // producer that emitted NFD computed its hash over the NFD bytes, so
        // after normalisation its carried hash no longer matches. That must not
        // be indistinguishable from tampering, which is what
        // `EventWire::needs_normalization` is for — asked BEFORE conversion,
        // while the original bytes are still in hand.
        let raw = json!({
            "v": 1, "id": EventId::generate(), "session_id": "s", "seq": 1,
            "ts": "2026-09-02T10:11:12.345Z", "source": "fs-watcher", "kind": "note",
            "payload": {"path": "cafe\u{301}"}, "prev_hash": "sha256:00"
        });
        let wire: EventWire = serde_json::from_value(raw.clone()).expect("wire parses");
        assert!(
            wire.needs_normalization(),
            "the reader cannot tell an NFD producer from a tamperer"
        );

        let event: Event = serde_json::from_value(raw).expect("parses");
        assert_eq!(
            event.payload()["path"],
            json!("caf\u{e9}"),
            "the gate did not normalise on the way in"
        );

        // A wire that is already NFC is left alone and says so.
        let clean = EventWire {
            v: 1,
            id: EventId::generate(),
            session_id: "s".into(),
            seq: 1,
            ts: "2026-09-02T10:11:12.345Z".into(),
            source: Source::Git,
            kind: EventKind::Note,
            payload: json!({"path": "caf\u{e9}"}),
            prev_hash: "sha256:00".into(),
            hash: None,
        };
        assert!(!clean.needs_normalization());
    }

    #[test]
    fn rfc8785_itself_does_not_normalize_which_is_why_n2_exists() {
        // The fact underneath invariant N2, asserted at the canonicalizer rather
        // than through `Event`, because `Event::new` now normalizes and would
        // hide it. If RFC 8785 ever grew a normalisation step, N2 would become
        // redundant and this is where that would show up.
        let composed = json!({"p": "\u{c5}"});
        let decomposed = json!({"p": "A\u{30a}"});
        assert_ne!(
            canonical::canonical_string(&composed).expect("canonicalizable"),
            canonical::canonical_string(&decomposed).expect("canonicalizable"),
            "RFC 8785 normalizes now — re-derive N2 rather than keeping it out of habit"
        );
    }

    #[test]
    fn event_ids_must_be_evt_plus_a_ulid() {
        assert!(EventId::parse("evt_01ARZ3NDEKTSV4RRFFQ69G5FAV").is_ok());
        for bad in [
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",      // no prefix
            "clm_01ARZ3NDEKTSV4RRFFQ69G5FAV",  // wrong prefix
            "evt_",                            // no ulid
            "evt_01ARZ3NDEKTSV4RRFFQ69G5FA",   // too short
            "evt_01ARZ3NDEKTSV4RRFFQ69G5FAVV", // too long
            "evt_01ARZ3NDEKTSV4RRFFQ69G5FAU",  // U is not in Crockford base32
        ] {
            assert!(
                EventId::parse(bad).is_err(),
                "{bad:?} was accepted as an event id"
            );
        }
        // And the newtype refuses it at the serde boundary too.
        assert!(serde_json::from_value::<EventId>(json!("nope")).is_err());
    }

    #[test]
    fn timestamps_must_be_rfc3339_utc() {
        for good in [
            "2026-09-02T10:11:12Z",
            "2026-09-02T10:11:12.345Z",
            "2026-09-02T10:11:12.345678901Z",
            "2024-02-29T00:00:00Z",
            "2026-12-31T23:59:60Z",
        ] {
            validate_timestamp(good).unwrap_or_else(|e| panic!("{good} rejected: {e}"));
        }
        for bad in [
            "2026-09-02T10:11:12+00:00", // same instant, different bytes, different hash
            "2026-09-02T10:11:12",       // no zone
            "2026-09-02 10:11:12Z",      // space separator
            "2026-9-02T10:11:12Z",       // not fixed width
            "2026-13-02T10:11:12Z",      // month
            "2026-02-30T10:11:12Z",      // day for month
            "2025-02-29T10:11:12Z",      // not a leap year
            "2026-09-02T24:11:12Z",      // hour
            "2026-09-02T10:61:12Z",      // minute
            "2026-09-02T10:11:12.Z",     // empty fraction
            "+026-09-02T10:11:12Z",      // sign smuggled past a naive parse
            "",
        ] {
            assert!(validate_timestamp(bad).is_err(), "{bad:?} was accepted");
        }
        assert!(matches!(
            Event::new(
                "s",
                1,
                "not a time",
                Source::Git,
                Payload::Note(OpenPayload::new()),
                "sha256:00"
            ),
            Err(EventError::MalformedTimestamp(_))
        ));
    }
}
