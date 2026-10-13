//! MANDATE-v4 §13 (A207) — real process and evidence binding, `EVB_V1`.
//!
//! A textual tool result is not sufficient proof that a command executed
//! (§13, first sentence). This module is the other side of that boundary:
//! the schema-versioned **EvidenceBinding** document for an independently
//! captured process execution — argv and cwd as spawned, the real exit code
//! or terminating signal, stdout/stderr digests with their capture
//! completeness, the pre- and post-command workspace identities (Stage 3's
//! `wsid1:` digests), declared artifacts, source, and both moments — plus
//! the freshness engine that re-observes and classifies a binding against
//! the workspace as it stands now.
//!
//! Four laws:
//!
//! 1. **ThruLine reads; it never runs.** Nothing in this module spawns a
//!    process. The one door to a captured execution is the explicitly
//!    user-invoked runner wrapper in the CLI (`thruline check -- <argv>`):
//!    the user runs their own command *through* ThruLine. This module only
//!    describes, stores, and re-evaluates what that wrapper captured.
//! 2. **Transcript claims stay below captured executions.** A command claim
//!    derived from a transcript — however realistic its output looks — caps
//!    at `RECORD_SUPPORTED` (§3): the record shows the text, nothing more.
//!    [`assess_transcript_command`] has no code path that returns anything
//!    stronger, and no type in this module can carry a remote fact (commit
//!    on a remote, pushed ref, deployment) derived from either a transcript
//!    *or* a captured `git push`: a real remote claim requires an
//!    independent remote query, which is a later stage. The refusal is
//!    pinned; the capability is absent on purpose.
//! 3. **Conservative freshness, structurally.** A binding is fresh only
//!    when a complete re-observation under the binding's own frozen scope
//!    equals the bound identity exactly. ANY workspace-identity change is
//!    relevant (§13's safe initial behavior); dependency-aware narrowing is
//!    forbidden until independently proven conservative, and no parameter
//!    of the evaluation API can express it.
//! 4. **The trail only appends.** A binding row is never edited. Staleness
//!    and invalidation are new observations (`EVIDENCE_STALE`,
//!    `EVIDENCE_INVALIDATED`) on the A205 chain, deduplicated by content
//!    (one per binding-and-detected-state pair), with the T1 record
//!    preserved forever — *unsupported when asserted at T1; subsequently
//!    confirmed at T2* never rewrites T1 (§9).
//!
//! ## The EVB_V1 canonical layout
//!
//! [`crate::chain`]'s injective conventions: `lp(s)` = u64-LE byte length
//! then UTF-8 bytes; every option is a tag byte (`0x00` absent, `0x01` then
//! `lp`); every list is a u64-LE count then items. In order:
//!
//! ```text
//! lp("EVB_V1")
//! lp("argv"); count; lp(arg)…                       (spawn order)
//! lp("mode"); lp("direct"|"shell")
//! lp("cwd"); lp(cwd)
//! lp("root"); lp(root)
//! lp("repo"); lp(repository_identity)
//! lp("scope"); lp(scope_class); lp(scope_key)
//! lp("wsid-scope"); include_untracked byte; count; lp(exclusion)…  (sorted, deduped)
//! lp("pre"); opt(identity); opt(incomplete_reason)
//! lp("post"); opt(identity); opt(incomplete_reason)
//! lp("exit"); opt(exit_code decimal); opt(term_signal decimal)
//! lp("stdout"); 0x01 lp(sha256) u64(bytes)  |  0x00 lp(reason)
//! lp("stderr"); same
//! lp("artifacts"); count; per: lp(path) opt(sha256)  (sorted by path)
//! lp("defects"); count; lp(defect)…                  (occurrence order)
//! lp("started"); lp(started_at)
//! lp("bound"); lp(bound_at)
//! lp("source"); lp(source_adapter); lp(source_session)
//! lp("state"); lp(state_at_binding)                  (§9, verbatim, derived)
//! ```
//!
//! The binding id is the SHA-256 of those bytes, lowercase hex. The state
//! at the end is **derived**, not caller-supplied — [`state_at_binding`] is
//! deterministic over the other fields, so the rule itself is hash-covered:
//! changing the rule changes every id, which is a versioned migration to
//! `EVB_V2`, never a refactor. Golden vectors computed outside this code
//! (python3, two independent assemblies) pin the layout.
//!
//! ## Storage
//!
//! One append-only `evidence_bindings` table (additive DDL, paid through
//! `thruline upgrade` per A193): content-addressed id, dense seq, the
//! queryable columns (`scope_key`, `repository_identity`, identities, exit)
//! and the full JSON doc with its own digest, `workspace_snapshots`-style.
//! "Latest binding for scope X" is one indexed query — the alternative
//! (bindings living only inside observation receipts) would make every
//! freshness read a full-chain scan-and-parse, which is the exact aging
//! failure `observation::verify_chain` just unlearned. The `EVIDENCE_BOUND`
//! observation cites the row as a derived reference beside its
//! process-execution terminal and carries the binding digest in the
//! hash-covered receipt, so a tampered row breaks both its own digests and
//! the chained receipt that names it.

use std::path::Path;

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::chain::sha256_hex;
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::observation::{
    append, AppendOutcome, Completeness, DerivedRef, NewObservation, ObservationType,
    ReceiptTerminal, SourceReceipt,
};
use crate::workspace_identity::{self, Observation, Scope};

/// The version literal inside the canonical document.
pub const EVB_VERSION: &str = "EVB_V1";

/// The stored envelope schema for `evidence_bindings` rows.
pub const SCHEMA_VERSION: i64 = 1;

/* ── §9: the authoritative state vocabulary, verbatim ───────────────── */

/// MANDATE-v4 §9, entering the engine as a closed vocabulary — the first
/// engine adoption. Every user-visible string these states produce is the
/// §9 name verbatim; no legacy verdict string leaks into the new surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceState {
    Asserted,
    RecordSupported,
    CurrentStateConfirmed,
    PolicySatisfied,
    Unknown,
    IncompleteSearch,
    Stale,
    InvalidatedByLaterChange,
    SubsequentlyConfirmed,
    Contradicted,
    HumanAdjudicated,
}

impl EvidenceState {
    /// Every §9 state, in §9's order. Closed — tests pin the size.
    pub const ALL: [EvidenceState; 11] = [
        EvidenceState::Asserted,
        EvidenceState::RecordSupported,
        EvidenceState::CurrentStateConfirmed,
        EvidenceState::PolicySatisfied,
        EvidenceState::Unknown,
        EvidenceState::IncompleteSearch,
        EvidenceState::Stale,
        EvidenceState::InvalidatedByLaterChange,
        EvidenceState::SubsequentlyConfirmed,
        EvidenceState::Contradicted,
        EvidenceState::HumanAdjudicated,
    ];

    /// The §9 name, verbatim.
    pub fn as_str(self) -> &'static str {
        match self {
            EvidenceState::Asserted => "ASSERTED",
            EvidenceState::RecordSupported => "RECORD_SUPPORTED",
            EvidenceState::CurrentStateConfirmed => "CURRENT_STATE_CONFIRMED",
            EvidenceState::PolicySatisfied => "POLICY_SATISFIED",
            EvidenceState::Unknown => "UNKNOWN",
            EvidenceState::IncompleteSearch => "INCOMPLETE_SEARCH",
            EvidenceState::Stale => "STALE",
            EvidenceState::InvalidatedByLaterChange => "INVALIDATED_BY_LATER_CHANGE",
            EvidenceState::SubsequentlyConfirmed => "SUBSEQUENTLY_CONFIRMED",
            EvidenceState::Contradicted => "CONTRADICTED",
            EvidenceState::HumanAdjudicated => "HUMAN_ADJUDICATED",
        }
    }

    /// Parse a stored name. `None` outside the closed set — never a panic.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|v| v.as_str() == s)
    }
}

/* ── scope: the closed command classification, reused from §5i ──────── */

/// What kind of check a command is, per the closed §5i recognition sets in
/// [`crate::identity`]. `Unclassified` still binds — the execution is real —
/// but can never satisfy an "all tests"-style claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScopeClass {
    Test,
    Build,
    Unclassified,
}

impl ScopeClass {
    pub fn as_str(self) -> &'static str {
        match self {
            ScopeClass::Test => "test",
            ScopeClass::Build => "build",
            ScopeClass::Unclassified => "unclassified",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "test" => Some(ScopeClass::Test),
            "build" => Some(ScopeClass::Build),
            "unclassified" => Some(ScopeClass::Unclassified),
            _ => None,
        }
    }
}

/// The declared scope of one captured command: its class and its §5i
/// identity key (which preserves suite selectors — `--filter X` is a
/// different scope from the bare runner, which is what makes "a narrow
/// suite cannot establish an all-tests claim" enforceable).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandScope {
    pub class: ScopeClass,
    pub key: String,
}

/// Classify one command line under the closed §5i sets. The key is the
/// [`crate::identity::identity_key`] for a shell invocation at `cwd`.
pub fn classify_command(command: &str, cwd: &str) -> CommandScope {
    let input = serde_json::json!({ "command": command }).to_string();
    let key = crate::identity::identity_key("Bash", &input, Some(cwd));
    let class = if crate::identity::is_test_invocation(&key) {
        ScopeClass::Test
    } else if crate::identity::is_build_invocation(&key) {
        ScopeClass::Build
    } else {
        ScopeClass::Unclassified
    };
    CommandScope { class, key }
}

/// The bare test-runner phrases whose un-narrowed invocation is a whole
/// suite. Mirrors the §5i test set on purpose; a test asserts every entry
/// here still satisfies [`crate::identity::is_test_invocation`], so the two
/// lists cannot drift apart silently.
const BARE_TEST_RUNNERS: [&str; 10] = [
    "cargo test",
    "npm test",
    "pnpm test",
    "yarn test",
    "swift test",
    "node --test",
    "pytest",
    "vitest",
    "jest",
    "go test",
];

/// Can this scope support an "all tests"-style claim?
///
/// A necessary condition only: the invocation must be an un-narrowed run of
/// a recognized test runner (or a bare test script with no surviving
/// arguments). Whether that runner is the repository's *canonical* suite is
/// a policy question (§20) for a later stage. `Unclassified` and `Build`
/// scopes always answer no; so does any invocation that kept a selector,
/// package flag, or filter stage through §5i canonicalisation.
pub fn establishes_all_tests(scope: &CommandScope) -> bool {
    if scope.class != ScopeClass::Test {
        return false;
    }
    let Some(rest) = scope.key.strip_prefix("bash:") else {
        return false;
    };
    let Some((_cwd, invocation)) = rest.split_once(':') else {
        return false;
    };
    if BARE_TEST_RUNNERS.contains(&invocation) {
        return true;
    }
    // A test script counts only when it is the whole invocation: any
    // surviving argument could be a selector, and undecidability resolves
    // toward "narrower than everything" here.
    let mut tokens = invocation.split_whitespace();
    match (tokens.next(), tokens.next()) {
        (Some(single), None) => {
            let probe = format!("bash:_:{single}");
            crate::identity::is_test_invocation(&probe)
        }
        _ => false,
    }
}

/* ── the binding document ───────────────────────────────────────────── */

/// How the wrapper executed the argv.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecMode {
    /// The argv was spawned directly — no shell interposition.
    Direct,
    /// `bash -o pipefail -c <command>`: pipeline exit status is preserved
    /// by construction (§13: pipeline exit status must not be lost through
    /// shell wrappers), and that cannot be disabled.
    Shell,
}

impl ExecMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ExecMode::Direct => "direct",
            ExecMode::Shell => "shell",
        }
    }
}

/// One side of the workspace-identity bracket. Exactly one field is
/// populated: a complete observation's `wsid1:` identity, or the stated
/// reason the observation was incomplete — following the Stage 3 rule that
/// an incomplete observation carries no digest at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityEdge {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incomplete_reason: Option<String>,
}

impl IdentityEdge {
    pub fn complete(identity: String) -> Self {
        IdentityEdge {
            identity: Some(identity),
            incomplete_reason: None,
        }
    }

    pub fn incomplete(reason: String) -> Self {
        IdentityEdge {
            identity: None,
            incomplete_reason: Some(reason),
        }
    }

    /// From a Stage 3 observation outcome.
    pub fn from_observation(obs: &Observation) -> Self {
        match obs {
            Observation::Complete(fp) => IdentityEdge::complete(fp.identity().to_string()),
            Observation::Incomplete { reason, .. } => IdentityEdge::incomplete(reason.clone()),
        }
    }
}

/// What one output stream's capture established. `Truncated` carries **no
/// digest by type** — a digest over partial bytes would claim a completeness
/// the capture does not have.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum StreamCapture {
    Captured { sha256: String, bytes: u64 },
    Truncated { reason: String },
}

/// One declared artifact, digested after the command finished. A file that
/// could not be read carries no digest and says nothing more.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactDigest {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// The declared workspace-identity scope the bracket was observed under —
/// frozen into the binding, so freshness re-observes under the same scope
/// (the Stage 3 freeze rule, carried into persistence as plain data; the
/// non-serializable `WorkspaceFingerprint` itself is never persisted).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsidScopeDecl {
    pub include_untracked: bool,
    pub exclusions: Vec<String>,
}

impl WsidScopeDecl {
    pub fn everything() -> Self {
        WsidScopeDecl {
            include_untracked: true,
            exclusions: Vec::new(),
        }
    }

    pub fn to_scope(&self) -> Scope {
        Scope {
            include_untracked: self.include_untracked,
            exclusions: self.exclusions.clone(),
        }
    }
}

/// The EVB_V1 document: everything §13 says a captured execution must
/// record. Stored as JSON beside its canonical digest; never edited.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceBinding {
    /// The argv exactly as spawned (for `Shell` mode this includes the
    /// shell and its flags — the truth, not the abbreviation).
    pub argv: Vec<String>,
    pub mode: ExecMode,
    pub cwd: String,
    /// The canonical workspace root the identities speak for.
    pub root: String,
    pub repository_identity: String,
    pub scope: CommandScope,
    pub wsid_scope: WsidScopeDecl,
    pub pre: IdentityEdge,
    pub post: IdentityEdge,
    /// The child's real exit code; `None` when it died to a signal.
    pub exit_code: Option<i64>,
    /// The terminating signal, where there was one.
    pub term_signal: Option<i64>,
    pub stdout: StreamCapture,
    pub stderr: StreamCapture,
    pub artifacts: Vec<ArtifactDigest>,
    /// Wrapper defects observed during the run — recorded separately from
    /// the child's outcome, which they never alter.
    pub wrapper_defects: Vec<String>,
    pub started_at: String,
    pub bound_at: String,
    pub source_adapter: String,
    pub source_session: String,
}

/// The §9 state a binding earns **at its own bound moment** — derived, not
/// caller-supplied, and hashed into the id.
///
/// `CURRENT_STATE_CONFIRMED` (as of `bound_at`) requires everything: both
/// identity edges complete and equal, both stream captures complete.
/// Anything less is `UNKNOWN` with the reason readable off the fields —
/// pre≠post means the run straddled two states and nobody can say which one
/// the results describe. The freshness engine, not this stored state, is
/// what says whether the evidence applies *now*.
pub fn state_at_binding(b: &EvidenceBinding) -> EvidenceState {
    let identities_bind = matches!(
        (&b.pre.identity, &b.post.identity),
        (Some(pre), Some(post)) if pre == post
    );
    let streams_complete = matches!(b.stdout, StreamCapture::Captured { .. })
        && matches!(b.stderr, StreamCapture::Captured { .. });
    if identities_bind && streams_complete {
        EvidenceState::CurrentStateConfirmed
    } else {
        EvidenceState::Unknown
    }
}

/// The identity a fresh observation must equal for this binding to still
/// apply: the bracketed identity, present only when pre and post agree.
pub fn bound_identity(b: &EvidenceBinding) -> Option<&str> {
    match (&b.pre.identity, &b.post.identity) {
        (Some(pre), Some(post)) if pre == post => Some(post.as_str()),
        _ => None,
    }
}

/* ── canonical encoding ─────────────────────────────────────────────── */

fn push_lp(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u64).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn push_opt(out: &mut Vec<u8>, v: Option<&str>) {
    match v {
        None => out.push(0x00),
        Some(s) => {
            out.push(0x01);
            push_lp(out, s);
        }
    }
}

fn push_count(out: &mut Vec<u8>, n: usize) {
    out.extend_from_slice(&(n as u64).to_le_bytes());
}

fn push_capture(out: &mut Vec<u8>, c: &StreamCapture) {
    match c {
        StreamCapture::Captured { sha256, bytes } => {
            out.push(0x01);
            push_lp(out, sha256);
            out.extend_from_slice(&bytes.to_le_bytes());
        }
        StreamCapture::Truncated { reason } => {
            out.push(0x00);
            push_lp(out, reason);
        }
    }
}

/// The documented EVB_V1 canonical bytes. Exclusions are normalized
/// (sorted, deduped) and artifacts sorted by path here, so arrival order is
/// nuisance, never identity. Golden vectors computed outside Rust pin this
/// layout — changing any byte of it is a migration to EVB_V2.
pub fn canonical_bytes(b: &EvidenceBinding) -> Vec<u8> {
    let mut out = Vec::with_capacity(1024);
    push_lp(&mut out, EVB_VERSION);
    push_lp(&mut out, "argv");
    push_count(&mut out, b.argv.len());
    for a in &b.argv {
        push_lp(&mut out, a);
    }
    push_lp(&mut out, "mode");
    push_lp(&mut out, b.mode.as_str());
    push_lp(&mut out, "cwd");
    push_lp(&mut out, &b.cwd);
    push_lp(&mut out, "root");
    push_lp(&mut out, &b.root);
    push_lp(&mut out, "repo");
    push_lp(&mut out, &b.repository_identity);
    push_lp(&mut out, "scope");
    push_lp(&mut out, b.scope.class.as_str());
    push_lp(&mut out, &b.scope.key);
    push_lp(&mut out, "wsid-scope");
    out.push(u8::from(b.wsid_scope.include_untracked));
    let mut exclusions = b.wsid_scope.exclusions.clone();
    exclusions.sort();
    exclusions.dedup();
    push_count(&mut out, exclusions.len());
    for e in &exclusions {
        push_lp(&mut out, e);
    }
    push_lp(&mut out, "pre");
    push_opt(&mut out, b.pre.identity.as_deref());
    push_opt(&mut out, b.pre.incomplete_reason.as_deref());
    push_lp(&mut out, "post");
    push_opt(&mut out, b.post.identity.as_deref());
    push_opt(&mut out, b.post.incomplete_reason.as_deref());
    push_lp(&mut out, "exit");
    push_opt(&mut out, b.exit_code.map(|c| c.to_string()).as_deref());
    push_opt(&mut out, b.term_signal.map(|s| s.to_string()).as_deref());
    push_lp(&mut out, "stdout");
    push_capture(&mut out, &b.stdout);
    push_lp(&mut out, "stderr");
    push_capture(&mut out, &b.stderr);
    push_lp(&mut out, "artifacts");
    let mut artifacts = b.artifacts.clone();
    artifacts.sort_by(|a, z| a.path.cmp(&z.path));
    push_count(&mut out, artifacts.len());
    for a in &artifacts {
        push_lp(&mut out, &a.path);
        push_opt(&mut out, a.sha256.as_deref());
    }
    push_lp(&mut out, "defects");
    push_count(&mut out, b.wrapper_defects.len());
    for d in &b.wrapper_defects {
        push_lp(&mut out, d);
    }
    push_lp(&mut out, "started");
    push_lp(&mut out, &b.started_at);
    push_lp(&mut out, "bound");
    push_lp(&mut out, &b.bound_at);
    push_lp(&mut out, "source");
    push_lp(&mut out, &b.source_adapter);
    push_lp(&mut out, &b.source_session);
    push_lp(&mut out, "state");
    push_lp(&mut out, state_at_binding(b).as_str());
    out
}

/// The content-addressed binding id: SHA-256 of [`canonical_bytes`].
pub fn binding_id(b: &EvidenceBinding) -> String {
    sha256_hex(&canonical_bytes(b))
}

/* ── storage ────────────────────────────────────────────────────────── */

/// One stored binding, digests re-verified on load.
#[derive(Debug, Clone, Serialize)]
pub struct StoredBinding {
    pub id: String,
    pub seq: i64,
    pub recorded_at: String,
    pub binding: EvidenceBinding,
}

/// What a store produced: a new row, or the row this content already is.
#[derive(Debug)]
pub enum StoreOutcome {
    Stored(StoredBinding),
    AlreadyStored(StoredBinding),
}

impl StoreOutcome {
    pub fn stored(&self) -> &StoredBinding {
        match self {
            StoreOutcome::Stored(s) | StoreOutcome::AlreadyStored(s) => s,
        }
    }
}

/// Append one binding row. One short IMMEDIATE transaction; identical
/// content is a no-op returning the standing row (content identity).
pub fn store(ledger: &Ledger, binding: &EvidenceBinding) -> Result<StoreOutcome> {
    let id = binding_id(binding);
    let doc = serde_json::to_string_pretty(binding)?;
    let doc_sha256 = sha256_hex(doc.as_bytes());
    let recorded_at = crate::ingest::iso8601_utc_now();
    let conn = ledger.conn();
    conn.busy_timeout(std::time::Duration::from_millis(3_000))?;
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let outcome = (|| -> Result<StoreOutcome> {
        if let Some(existing) = load_tx(conn, &id)? {
            return Ok(StoreOutcome::AlreadyStored(existing));
        }
        let seq: i64 = conn.query_row(
            "SELECT COALESCE(MAX(seq), -1) + 1 FROM evidence_bindings",
            [],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT INTO evidence_bindings(
                 id, seq, recorded_at, schema_version, scope_class, scope_key,
                 root, repository_identity, pre_identity, post_identity,
                 exit_code, term_signal, bound_at, source_session, doc, doc_sha256)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            rusqlite::params![
                id,
                seq,
                recorded_at,
                SCHEMA_VERSION,
                binding.scope.class.as_str(),
                binding.scope.key,
                binding.root,
                binding.repository_identity,
                binding.pre.identity,
                binding.post.identity,
                binding.exit_code,
                binding.term_signal,
                binding.bound_at,
                binding.source_session,
                doc,
                doc_sha256,
            ],
        )?;
        Ok(StoreOutcome::Stored(StoredBinding {
            id,
            seq,
            recorded_at,
            binding: binding.clone(),
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

fn row_to_stored(r: &rusqlite::Row<'_>) -> rusqlite::Result<(String, i64, String, String, String)> {
    Ok((
        r.get(0)?, // id
        r.get(1)?, // seq
        r.get(2)?, // recorded_at
        r.get(3)?, // doc
        r.get(4)?, // doc_sha256
    ))
}

fn verify_row(row: (String, i64, String, String, String)) -> Result<StoredBinding> {
    let (id, seq, recorded_at, doc, doc_sha256) = row;
    if sha256_hex(doc.as_bytes()) != doc_sha256 {
        return Err(CoreError::Integrity(format!(
            "evidence binding {id} does not match its recorded document digest"
        )));
    }
    let binding: EvidenceBinding = serde_json::from_str(&doc)?;
    if binding_id(&binding) != id {
        return Err(CoreError::Integrity(format!(
            "evidence binding {id} does not re-derive its own canonical id — \
             the stored document was altered"
        )));
    }
    Ok(StoredBinding {
        id,
        seq,
        recorded_at,
        binding,
    })
}

const SELECT: &str = "id, seq, recorded_at, doc, doc_sha256 FROM evidence_bindings";

fn load_tx(conn: &rusqlite::Connection, id: &str) -> Result<Option<StoredBinding>> {
    let row = conn
        .query_row(
            &format!("SELECT {SELECT} WHERE id = ?1"),
            [id],
            row_to_stored,
        )
        .optional()?;
    row.map(verify_row).transpose()
}

/// One binding by id, digests re-verified.
pub fn load(ledger: &Ledger, id: &str) -> Result<Option<StoredBinding>> {
    load_tx(ledger.conn(), id)
}

/// The latest binding for one scope key — a single indexed query.
pub fn latest_for_scope(ledger: &Ledger, scope_key: &str) -> Result<Option<StoredBinding>> {
    let row = ledger
        .conn()
        .query_row(
            &format!("SELECT {SELECT} WHERE scope_key = ?1 ORDER BY seq DESC LIMIT 1"),
            [scope_key],
            row_to_stored,
        )
        .optional()?;
    row.map(verify_row).transpose()
}

/// The latest binding per scope key for one repository, in seq order.
pub fn latest_per_scope(ledger: &Ledger, repository_identity: &str) -> Result<Vec<StoredBinding>> {
    let conn = ledger.conn();
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT}
          WHERE repository_identity = ?1
            AND seq IN (SELECT MAX(seq) FROM evidence_bindings
                         WHERE repository_identity = ?1 GROUP BY scope_key)
          ORDER BY seq"
    ))?;
    let rows = stmt.query_map([repository_identity], row_to_stored)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(verify_row(row?)?);
    }
    Ok(out)
}

/// Every binding for one repository, in seq order.
pub fn all_for_repo(ledger: &Ledger, repository_identity: &str) -> Result<Vec<StoredBinding>> {
    let conn = ledger.conn();
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT} WHERE repository_identity = ?1 ORDER BY seq"
    ))?;
    let rows = stmt.query_map([repository_identity], row_to_stored)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(verify_row(row?)?);
    }
    Ok(out)
}

/// The newest later binding of the same scope whose bound identity equals
/// `current` — the superseder, when one exists.
fn superseder_at(
    ledger: &Ledger,
    scope_key: &str,
    after_seq: i64,
    current: &str,
) -> Result<Option<StoredBinding>> {
    let conn = ledger.conn();
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT} WHERE scope_key = ?1 AND seq > ?2 ORDER BY seq DESC"
    ))?;
    let rows = stmt.query_map(rusqlite::params![scope_key, after_seq], row_to_stored)?;
    for row in rows {
        let stored = verify_row(row?)?;
        if bound_identity(&stored.binding) == Some(current) {
            return Ok(Some(stored));
        }
    }
    Ok(None)
}

/* ── observation writers (foreground only) ──────────────────────────── */

/// The rendered command line for receipts: the argv, shell-ishly joined.
/// Display only — the binding's `argv` field is the truth.
pub fn rendered_command(argv: &[String]) -> String {
    argv.join(" ")
}

/// Append `COMMAND_STARTED`. Called by the wrapper right after a successful
/// spawn — a started observation means a real child process exists.
pub fn record_command_started(
    ledger: &Ledger,
    binding_seed: &EvidenceBinding,
    pid: u32,
) -> Result<AppendOutcome> {
    record_command_started_with_metadata(ledger, binding_seed, pid, &serde_json::Map::new())
}

/// Append the real command start with phase metadata preserved in its receipt.
pub fn record_command_started_with_metadata(
    ledger: &Ledger,
    binding_seed: &EvidenceBinding,
    pid: u32,
    metadata: &serde_json::Map<String, serde_json::Value>,
) -> Result<AppendOutcome> {
    let mut receipt = SourceReceipt::new(ReceiptTerminal::ProcessExecution {
        command: rendered_command(&binding_seed.argv),
        exit_code: None,
    });
    receipt.extra.extend(metadata.clone());
    receipt
        .extra
        .insert("cwd".to_string(), binding_seed.cwd.clone().into());
    receipt.extra.insert("pid".to_string(), pid.into());
    receipt.extra.insert(
        "scope_key".to_string(),
        binding_seed.scope.key.clone().into(),
    );
    if let Some(reason) = &binding_seed.pre.incomplete_reason {
        receipt
            .extra
            .insert("pre_incomplete_reason".to_string(), reason.clone().into());
    }
    let completeness = if binding_seed.pre.identity.is_some() {
        Completeness::Complete
    } else {
        Completeness::Incomplete
    };
    append(
        ledger,
        &NewObservation {
            observation_type: ObservationType::CommandStarted,
            source_adapter: &binding_seed.source_adapter,
            source_session: &binding_seed.source_session,
            repository_identity: &binding_seed.repository_identity,
            workspace_identity: binding_seed.pre.identity.as_deref(),
            observed_at: &binding_seed.started_at,
            completeness,
            receipt: &receipt,
        },
    )
}

/// Append `COMMAND_FINISHED` and `EVIDENCE_BOUND` for a stored binding.
/// Returns the two observation ids, in that order.
pub fn record_command_bound(ledger: &Ledger, stored: &StoredBinding) -> Result<(String, String)> {
    record_command_bound_with_metadata(ledger, stored, &serde_json::Map::new())
}

/// Phase metadata belongs to the observation receipt, never the binding schema.
pub fn record_command_bound_with_metadata(
    ledger: &Ledger,
    stored: &StoredBinding,
    metadata: &serde_json::Map<String, serde_json::Value>,
) -> Result<(String, String)> {
    let b = &stored.binding;
    let command = rendered_command(&b.argv);
    let completeness = match state_at_binding(b) {
        EvidenceState::CurrentStateConfirmed => Completeness::Complete,
        _ => Completeness::Incomplete,
    };

    let mut finished = SourceReceipt::new(ReceiptTerminal::ProcessExecution {
        command: command.clone(),
        exit_code: b.exit_code,
    });
    finished.extra.extend(metadata.clone());
    if let Some(sig) = b.term_signal {
        finished.extra.insert("term_signal".to_string(), sig.into());
    }
    finished
        .extra
        .insert("started_at".to_string(), b.started_at.clone().into());
    let finished_out = append(
        ledger,
        &NewObservation {
            observation_type: ObservationType::CommandFinished,
            source_adapter: &b.source_adapter,
            source_session: &b.source_session,
            repository_identity: &b.repository_identity,
            workspace_identity: b.post.identity.as_deref(),
            observed_at: &b.bound_at,
            completeness,
            receipt: &finished,
        },
    )?;

    let mut bound = SourceReceipt::new(ReceiptTerminal::ProcessExecution {
        command,
        exit_code: b.exit_code,
    });
    bound.derived_refs.push(DerivedRef {
        table: "evidence_bindings".to_string(),
        id: stored.id.clone(),
    });
    bound
        .extra
        .insert("binding_sha256".to_string(), stored.id.clone().into());
    bound
        .extra
        .insert("scope_class".to_string(), b.scope.class.as_str().into());
    bound
        .extra
        .insert("scope_key".to_string(), b.scope.key.clone().into());
    bound.extra.insert(
        "state_at_binding".to_string(),
        state_at_binding(b).as_str().into(),
    );
    let bound_out = append(
        ledger,
        &NewObservation {
            observation_type: ObservationType::EvidenceBound,
            source_adapter: &b.source_adapter,
            source_session: &b.source_session,
            repository_identity: &b.repository_identity,
            workspace_identity: b.post.identity.as_deref(),
            observed_at: &b.bound_at,
            completeness,
            receipt: &bound,
        },
    )?;
    Ok((
        finished_out.record().id.clone(),
        bound_out.record().id.clone(),
    ))
}

/* ── the freshness engine ───────────────────────────────────────────── */

/// How one binding stands against the workspace as re-observed now. The §9
/// rendering is [`Freshness::state`]; the conservative rule is law 3 of the
/// module doc: any identity change is relevant, no narrowing exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Freshness {
    /// The re-observed identity equals the bound identity exactly.
    CurrentStateConfirmed,
    /// The workspace moved after the evidence, and no later binding covers
    /// the current state.
    Stale { bound: String, current: String },
    /// The workspace moved, and a later binding of the same scope is bound
    /// to the current state — this evidence no longer applies.
    InvalidatedByLaterChange { superseded_by: String },
    /// The binding never bracketed one state (incomplete edges, or pre≠post)
    /// — there is nothing a re-observation could confirm.
    Unknown { reason: String },
    /// The current workspace could not be completely inspected.
    IncompleteSearch { reason: String },
}

impl Freshness {
    pub fn state(&self) -> EvidenceState {
        match self {
            Freshness::CurrentStateConfirmed => EvidenceState::CurrentStateConfirmed,
            Freshness::Stale { .. } => EvidenceState::Stale,
            Freshness::InvalidatedByLaterChange { .. } => EvidenceState::InvalidatedByLaterChange,
            Freshness::Unknown { .. } => EvidenceState::Unknown,
            Freshness::IncompleteSearch { .. } => EvidenceState::IncompleteSearch,
        }
    }
}

/// One binding's evaluation: both moments, the classification, and any
/// observations the detection appended (ids). `bound_at` is T1 truth and is
/// never rewritten by an evaluation at T2.
#[derive(Debug, Clone)]
pub struct Evaluation {
    pub binding_id: String,
    pub scope: CommandScope,
    pub bound_at: String,
    pub evaluated_at: String,
    pub freshness: Freshness,
    pub appended_observations: Vec<String>,
}

/// Evaluate one binding against the workspace as it stands.
pub fn evaluate(ledger: &Ledger, stored: &StoredBinding, evaluated_at: &str) -> Result<Evaluation> {
    evaluate_with(ledger, stored, evaluated_at, || {})
}

/// [`evaluate`] with the observation seam exposed — the same seam Stage 3
/// carries, so the incomplete-search law is testable with a real
/// interleaved write rather than a timing simulation.
pub fn evaluate_with(
    ledger: &Ledger,
    stored: &StoredBinding,
    evaluated_at: &str,
    between: impl FnOnce(),
) -> Result<Evaluation> {
    let b = &stored.binding;
    let done = |freshness: Freshness, appended: Vec<String>| {
        Ok(Evaluation {
            binding_id: stored.id.clone(),
            scope: b.scope.clone(),
            bound_at: b.bound_at.clone(),
            evaluated_at: evaluated_at.to_string(),
            freshness,
            appended_observations: appended,
        })
    };

    let Some(bound) = bound_identity(b) else {
        let reason = match (&b.pre.incomplete_reason, &b.post.incomplete_reason) {
            (Some(r), _) | (None, Some(r)) => format!(
                "the binding never bracketed one workspace state ({r}) — \
                 there is nothing a re-observation could confirm"
            ),
            (None, None) => "the pre- and post-command identities differ — the run \
                             straddled two workspace states"
                .to_string(),
        };
        return done(Freshness::Unknown { reason }, Vec::new());
    };

    let current = match workspace_identity::observe_with(
        Path::new(&b.root),
        b.wsid_scope.to_scope(),
        between,
    ) {
        Ok(Observation::Complete(fp)) => fp.identity().to_string(),
        Ok(Observation::Incomplete { reason, .. }) => {
            return done(Freshness::IncompleteSearch { reason }, Vec::new());
        }
        Err(e) => {
            return done(
                Freshness::IncompleteSearch {
                    reason: format!("the workspace at {} could not be observed: {e}", b.root),
                },
                Vec::new(),
            );
        }
    };

    if current == bound {
        return done(Freshness::CurrentStateConfirmed, Vec::new());
    }

    if let Some(superseder) = superseder_at(ledger, &b.scope.key, stored.seq, &current)? {
        let appended = record_detection(
            ledger,
            stored,
            ObservationType::EvidenceInvalidated,
            &current,
            evaluated_at,
            Some(&superseder.id),
        )?;
        return done(
            Freshness::InvalidatedByLaterChange {
                superseded_by: superseder.id,
            },
            appended.into_iter().collect(),
        );
    }

    let appended = record_detection(
        ledger,
        stored,
        ObservationType::EvidenceStale,
        &current,
        evaluated_at,
        None,
    )?;
    done(
        Freshness::Stale {
            bound: bound.to_string(),
            current,
        },
        appended.into_iter().collect(),
    )
}

/// Append one `EVIDENCE_STALE` / `EVIDENCE_INVALIDATED` detection, unless
/// the chain already records this exact detection (same binding, same
/// detected current identity, same superseder) — the trail stays
/// append-only AND deduplicated: every row is a real, novel detection.
fn record_detection(
    ledger: &Ledger,
    stored: &StoredBinding,
    kind: ObservationType,
    current_identity: &str,
    evaluated_at: &str,
    superseded_by: Option<&str>,
) -> Result<Option<String>> {
    if detection_exists(ledger, kind, &stored.id, current_identity, superseded_by)? {
        return Ok(None);
    }
    let b = &stored.binding;
    let current_digest = current_identity
        .strip_prefix(workspace_identity::IDENTITY_PREFIX)
        .unwrap_or(current_identity)
        .to_string();
    let mut receipt = SourceReceipt::new(ReceiptTerminal::Filesystem {
        path: b.root.clone(),
        sha256: Some(current_digest),
    });
    receipt.derived_refs.push(DerivedRef {
        table: "evidence_bindings".to_string(),
        id: stored.id.clone(),
    });
    if let Some(sup) = superseded_by {
        receipt.derived_refs.push(DerivedRef {
            table: "evidence_bindings".to_string(),
            id: sup.to_string(),
        });
        receipt
            .extra
            .insert("superseded_by".to_string(), sup.to_string().into());
    }
    receipt
        .extra
        .insert("binding_id".to_string(), stored.id.clone().into());
    receipt.extra.insert(
        "bound_identity".to_string(),
        bound_identity(b).unwrap_or("").to_string().into(),
    );
    receipt.extra.insert(
        "current_identity".to_string(),
        current_identity.to_string().into(),
    );
    receipt
        .extra
        .insert("scope_key".to_string(), b.scope.key.clone().into());
    receipt
        .extra
        .insert("binding_bound_at".to_string(), b.bound_at.clone().into());
    let session = format!("cli:evidence:{evaluated_at}");
    let out = append(
        ledger,
        &NewObservation {
            observation_type: kind,
            source_adapter: "thruline",
            source_session: &session,
            repository_identity: &b.repository_identity,
            workspace_identity: Some(current_identity),
            observed_at: evaluated_at,
            completeness: Completeness::Complete,
            receipt: &receipt,
        },
    )?;
    Ok(Some(out.record().id.clone()))
}

/// Cursor-shaped scan for a standing detection — rows are checked as they
/// arrive, never collected.
fn detection_exists(
    ledger: &Ledger,
    kind: ObservationType,
    binding_id: &str,
    current_identity: &str,
    superseded_by: Option<&str>,
) -> Result<bool> {
    let conn = ledger.conn();
    let mut stmt = conn.prepare(
        "SELECT source_receipt FROM observations WHERE observation_type = ?1 ORDER BY seq",
    )?;
    let rows = stmt.query_map([kind.as_str()], |r| r.get::<_, String>(0))?;
    for row in rows {
        let text = row?;
        let Ok(receipt) = SourceReceipt::from_json(&text) else {
            continue;
        };
        let same_binding =
            receipt.extra.get("binding_id").and_then(|v| v.as_str()) == Some(binding_id);
        let same_current = receipt
            .extra
            .get("current_identity")
            .and_then(|v| v.as_str())
            == Some(current_identity);
        let same_superseder =
            receipt.extra.get("superseded_by").and_then(|v| v.as_str()) == superseded_by;
        if same_binding && same_current && same_superseder {
            return Ok(true);
        }
    }
    Ok(false)
}

/* ── the scope story: temporal truth across evaluations ─────────────── */

/// The story of one scope across time. `state` is the §9 roll-up:
/// `SUBSEQUENTLY_CONFIRMED` when the scope's evidence was recorded stale or
/// invalidated at some T1 and the latest binding confirms the current state
/// at T2 — both moments carried, T1's chain rows untouched. Otherwise the
/// latest evaluation's own state.
#[derive(Debug, Clone)]
pub struct ScopeStory {
    pub latest: Evaluation,
    /// Standing stale/invalidation detections recorded for this scope
    /// before `evaluated_at` — the T1 trail, preserved.
    pub prior_detections: u64,
    pub first_detection_at: Option<String>,
    pub state: EvidenceState,
}

/// Evaluate the latest binding for a scope and fold in the recorded trail.
pub fn scope_story(
    ledger: &Ledger,
    stored: &StoredBinding,
    evaluated_at: &str,
) -> Result<ScopeStory> {
    let latest = evaluate(ledger, stored, evaluated_at)?;
    let (prior_detections, first_detection_at) = detections_for_scope(
        ledger,
        &stored.binding.scope.key,
        &latest.appended_observations,
    )?;
    let state = match latest.freshness {
        Freshness::CurrentStateConfirmed if prior_detections > 0 => {
            EvidenceState::SubsequentlyConfirmed
        }
        ref f => f.state(),
    };
    Ok(ScopeStory {
        latest,
        prior_detections,
        first_detection_at,
        state,
    })
}

/// Count the standing detections for one scope, excluding observations this
/// very evaluation appended. Cursor-shaped.
fn detections_for_scope(
    ledger: &Ledger,
    scope_key: &str,
    exclude: &[String],
) -> Result<(u64, Option<String>)> {
    let conn = ledger.conn();
    let mut stmt = conn.prepare(
        "SELECT id, observed_at, source_receipt FROM observations
          WHERE observation_type IN ('EVIDENCE_STALE','EVIDENCE_INVALIDATED')
          ORDER BY seq",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    let mut count = 0u64;
    let mut first: Option<String> = None;
    for row in rows {
        let (id, observed_at, text) = row?;
        if exclude.contains(&id) {
            continue;
        }
        let Ok(receipt) = SourceReceipt::from_json(&text) else {
            continue;
        };
        if receipt.extra.get("scope_key").and_then(|v| v.as_str()) == Some(scope_key) {
            count += 1;
            if first.is_none() {
                first = Some(observed_at);
            }
        }
    }
    Ok((count, first))
}

/* ── transcript-derived command claims: the §3 ceiling ──────────────── */

/// What a transcript-derived command claim can establish. There is no
/// remote-fact field here and no constructor for one: a Git-looking
/// transcript is never an independently verified Git state (§3), and this
/// stage pins the refusal — an independent remote query is a later stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptAssessment {
    pub state: EvidenceState,
    pub note: String,
    /// Present when the tool result is an overflow reference whose full
    /// bytes were not inspected.
    pub overflow: Option<crate::overflow::OverflowRef>,
}

/// Assess a transcript tool result standing in for a command claim.
///
/// The ceiling is `RECORD_SUPPORTED` — the record shows this text, and the
/// record shows nothing more. No code path in this function (or module)
/// returns `CURRENT_STATE_CONFIRMED` or derives a commit, push, remote ref,
/// or deployment from transcript text, however realistic the output looks.
/// A tool result that is an unresolved overflow reference is
/// `INCOMPLETE_SEARCH`: only the excerpt was inspectable, and §13 forbids
/// searching the excerpt while claiming a complete search.
pub fn assess_transcript_command(tool_result_text: &str) -> TranscriptAssessment {
    if let Some(overflow) = crate::overflow::overflow_reference(tool_result_text) {
        let note = format!(
            "the tool result overflowed to a side file ({}) whose bytes were not \
             inspected — the search over this result is incomplete",
            overflow.path
        );
        return TranscriptAssessment {
            state: EvidenceState::IncompleteSearch,
            note,
            overflow: Some(overflow),
        };
    }
    TranscriptAssessment {
        state: EvidenceState::RecordSupported,
        note: "the captured record contains this output; it does not establish that \
               the command executed, and it establishes no commit, push, remote ref, \
               or deployment — a real remote claim requires an independent remote \
               query"
            .to_string(),
        overflow: None,
    }
}
