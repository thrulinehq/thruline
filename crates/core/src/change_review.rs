//! T-4-1 — durable, append-only review records bound to one exact version of
//! one Change (`B9.1`–`B9.9`, `Q8`, `Q11`).
//!
//! A review is neither a check verdict nor an approval. It says who looked,
//! at which version, what they found, and whether it finished. The reviewer's
//! overall recommendation, when it gave one, is its own opinion and lives in a
//! closed vocabulary of its own; nothing in this module turns it, a finding
//! count or a provider score into a result.
//!
//! **The version rule is the approval's rule.** A review records the
//! `target_revision` and accepted-criteria `scope_version` that
//! [`crate::change_decision::standing_target`] named when it was accepted, and
//! "does this review still apply" is that same function asked again at read
//! time. Two rules for one fact is how a screen comes to say a review applies
//! to a version the Approve door says has moved.
//!
//! **Integrity is per review.** The parent row carries `record_sha256`, a
//! digest over every column and every child row (raw bytes digest, parsed
//! record, typed findings). A review whose children no longer match is served
//! as `altered` or `missing`, with no findings, no recommendation and no
//! current credit, and its raw/parsed readers refuse. One damaged review never
//! makes the rest of the Change unreadable.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::chain::sha256_hex;
use crate::change::mask_text;
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::redact::MaskRecord;

/// The declared limit on one review's raw bytes. Mirrors the DDL CHECK.
pub const RAW_LIMIT: usize = 8 * 1024 * 1024;

/// The one loopback destination the PR-Agent review route may name. The plan
/// fixes it (`workflow.rs`); a review naming any other is refused.
pub const ROUTE_ENDPOINT: &str = "http://127.0.0.1:11436";

/// The PR-Agent source commit the route is pinned to (`container_native.py`
/// `SOURCE_PINS['R14']`).
pub const PR_AGENT_SOURCE_COMMIT: &str = "f3b385ea2927247ddcff2fe252472380b9c8f5fc";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewerType {
    Model,
    Human,
    Tool,
    Local,
}

impl ReviewerType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Human => "human",
            Self::Tool => "tool",
            Self::Local => "local",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "model" => Ok(Self::Model),
            "human" => Ok(Self::Human),
            "tool" => Ok(Self::Tool),
            "local" => Ok(Self::Local),
            _ => Err(CoreError::Config("unknown reviewer type".into())),
        }
    }
}

/// Whether the review FINISHED — never what it concluded.
///
/// `TimedOut` and `Cancelled` are separate words because they are separate
/// facts (`Q11`, R04): the product's own deadline ending an attempt is not a
/// person stopping it, and neither is a review that found nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewState {
    Complete,
    Incomplete,
    TimedOut,
    Cancelled,
    Unavailable,
    ParseFailed,
}

impl ReviewState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Incomplete => "incomplete",
            Self::TimedOut => "timed_out",
            Self::Cancelled => "cancelled",
            Self::Unavailable => "unavailable",
            Self::ParseFailed => "parse_failed",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "complete" => Ok(Self::Complete),
            "incomplete" => Ok(Self::Incomplete),
            "timed_out" => Ok(Self::TimedOut),
            "cancelled" => Ok(Self::Cancelled),
            "unavailable" => Ok(Self::Unavailable),
            "parse_failed" => Ok(Self::ParseFailed),
            _ => Err(CoreError::Config("unknown review state".into())),
        }
    }
}

/// The reviewer's own overall opinion, when it gave one. Advisory metadata
/// (`B9.7`): never a verdict, never an approval, never inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Recommendation {
    Approved,
    ChangesRequested,
    ApprovedWithExceptions,
}

impl Recommendation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::ChangesRequested => "changes_requested",
            Self::ApprovedWithExceptions => "approved_with_exceptions",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "approved" => Ok(Self::Approved),
            "changes_requested" => Ok(Self::ChangesRequested),
            "approved_with_exceptions" => Ok(Self::ApprovedWithExceptions),
            _ => Err(CoreError::Config("unknown review recommendation".into())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReviewFinding {
    pub kind: String,
    pub message: String,
    pub path: Option<String>,
    pub line: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewReview {
    pub id: String,
    pub change_id: String,
    pub review_round: u32,
    pub reviewer_identity: String,
    pub reviewer_type: ReviewerType,
    pub review_source: String,
    /// Relationship is a recorded claim with its source. A provider or model
    /// name does not establish reviewer independence (`B9.9`).
    pub reviewer_relationship: String,
    pub relationship_provenance: String,
    /// The version the review looked at, as `standing_target` named it.
    pub target_revision: String,
    /// The accepted-criteria version at that moment, if any was accepted.
    pub scope_version: Option<u32>,
    pub examined_inputs_id: String,
    pub diff_sha256: String,
    /// Canonical JSON naming the accepted endpoint, model, runtime, image,
    /// provider source and plan. Data, never executable configuration.
    pub route_identity_json: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub state: ReviewState,
    pub recommendation: Option<Recommendation>,
    pub limitations: Vec<String>,
    pub raw_bytes: Vec<u8>,
    pub parser_id: String,
    pub parsed_json: String,
    pub parse_complete: bool,
    pub findings: Vec<ReviewFinding>,
}

/// Whether a review's stored parts still agree with the digest its parent
/// row sealed them under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Integrity {
    Intact,
    /// A child row (raw bytes or parsed record) is absent.
    Missing,
    /// A child row is present and no longer matches.
    Altered,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArtifactIntegrity {
    pub available: bool,
    pub sha256: Option<String>,
    pub bytes: Option<u64>,
    pub intact: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParsedIntegrity {
    pub available: bool,
    pub parser_id: Option<String>,
    pub sha256: Option<String>,
    pub parse_complete: bool,
    pub intact: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReviewView {
    pub id: String,
    pub review_round: u32,
    pub reviewer_identity: String,
    pub reviewer_type: ReviewerType,
    pub review_source: String,
    pub reviewer_relationship: String,
    pub relationship_provenance: String,
    /// The version this review looked at.
    pub target_revision: String,
    pub scope_version: Option<u32>,
    /// The version the record stands at now, so a reader can name both.
    pub current_revision: Option<String>,
    pub examined_inputs_id: String,
    pub diff_sha256: String,
    pub route_identity_json: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub state: ReviewState,
    /// Advisory only. It is not a verification verdict or an approval.
    pub recommendation: Option<Recommendation>,
    pub limitations: Vec<String>,
    /// How many findings the record holds — the one number a screen may show
    /// about them. Zero on a review that did not finish or is not intact.
    pub finding_count: u32,
    pub findings: Vec<ReviewFinding>,
    pub raw: ArtifactIntegrity,
    pub parsed: ParsedIntegrity,
    pub integrity: Integrity,
    /// True only when the review finished, its parts are intact, and both its
    /// version and its accepted-criteria version are still the ones the
    /// record stands at.
    pub applies_to_current_version: bool,
}

/// The binding a review must carry, read from the record as it stands now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentBinding {
    pub target_revision: String,
    pub scope_version: Option<u32>,
    pub examined_inputs_id: String,
    pub worktree_path: Option<String>,
}

/// The standing `intended` examined-inputs record's id for a Change: the same
/// row `examined_inputs::standing` reads (latest by rowid).
pub fn standing_intended_id(ledger: &Ledger, change_id: &str) -> Result<Option<String>> {
    Ok(ledger
        .conn()
        .query_row(
            "SELECT id FROM examination_inputs
              WHERE subject_kind='intended' AND subject_id=?1
              ORDER BY rowid DESC LIMIT 1",
            params![change_id],
            |row| row.get(0),
        )
        .optional()?)
}

/// What a review of this Change would bind to right now, or `None` when the
/// record names no version yet.
pub fn current_binding(ledger: &Ledger, change_id: &str) -> Result<Option<CurrentBinding>> {
    let (revision, scope_version) = crate::change_decision::standing_target(ledger, change_id)?;
    let Some(target_revision) = revision else {
        return Ok(None);
    };
    let Some(examined_inputs_id) = standing_intended_id(ledger, change_id)? else {
        return Ok(None);
    };
    let worktree_path: Option<String> = ledger.conn().query_row(
        "SELECT worktree_path FROM examination_inputs WHERE id=?1",
        params![examined_inputs_id],
        |row| row.get(0),
    )?;
    Ok(Some(CurrentBinding {
        target_revision,
        scope_version,
        examined_inputs_id,
        worktree_path,
    }))
}

/// The next review round for a Change: one more than the highest recorded.
pub fn next_round(ledger: &Ledger, change_id: &str) -> Result<u32> {
    let round: i64 = ledger.conn().query_row(
        "SELECT COALESCE(MAX(review_round), 0) + 1 FROM change_reviews WHERE change_id=?1",
        params![change_id],
        |row| row.get(0),
    )?;
    u32::try_from(round).map_err(|_| CoreError::Integrity("review round out of range".into()))
}

/// The parent row seals every column and every child under one digest. It
/// detects a changed child even when that child's own digest column was
/// changed with it. It is not a claim to detect a wholesale rewrite of every
/// row at once.
#[derive(Serialize)]
struct RecordBinding<'a> {
    schema: &'static str,
    id: &'a str,
    change_id: &'a str,
    review_round: u32,
    reviewer_identity: &'a str,
    reviewer_type: &'a str,
    review_source: &'a str,
    reviewer_relationship: &'a str,
    relationship_provenance: &'a str,
    target_revision: &'a str,
    scope_version: Option<u32>,
    examined_inputs_id: &'a str,
    diff_sha256: &'a str,
    route_identity_json: &'a str,
    started_at: &'a str,
    ended_at: Option<&'a str>,
    state: &'a str,
    recommendation: Option<&'a str>,
    limitations_json: &'a str,
    raw_sha256: &'a str,
    raw_bytes: u64,
    parser_id: &'a str,
    parsed_sha256: &'a str,
    parse_complete: bool,
    findings: &'a [ReviewFinding],
}

const BINDING_SCHEMA: &str = "thruline.change-review-binding.v1";

fn record_binding_sha256(binding: &RecordBinding<'_>) -> Result<String> {
    Ok(sha256_hex(&serde_json::to_vec(binding)?))
}

fn require(field: &'static str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(CoreError::Config(format!(
            "review {field} must not be empty"
        )));
    }
    Ok(())
}

fn is_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// A review may name no alternate destination or provider. The identity is
/// structured so a reader can compare it with the route, and checked here so
/// an opaque note can never stand in for one.
pub fn validate_route_identity(value: &str) -> Result<()> {
    let route = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(value)
        .map_err(|_| CoreError::Config("review route identity is not a JSON object".into()))?;
    const FIELDS: [&str; 7] = [
        "endpoint",
        "model",
        "model_manifest_sha256",
        "runtime_sha256",
        "provider_source_commit",
        "image_id",
        "plan_digest",
    ];
    if route.len() != FIELDS.len() || FIELDS.iter().any(|f| !route.contains_key(*f)) {
        return Err(CoreError::Config(
            "review route identity has unsupported or missing fields".into(),
        ));
    }
    let text = |field: &str| route.get(field).and_then(serde_json::Value::as_str);
    if text("endpoint") != Some(ROUTE_ENDPOINT) {
        return Err(CoreError::Config(
            "review route identity does not name the accepted loopback endpoint".into(),
        ));
    }
    if text("provider_source_commit") != Some(PR_AGENT_SOURCE_COMMIT) {
        return Err(CoreError::Config(
            "review route identity does not name the pinned provider source".into(),
        ));
    }
    let model = text("model").unwrap_or_default();
    if model.trim().is_empty() || model.len() > 128 || model.chars().any(char::is_control) {
        return Err(CoreError::Config(
            "review route identity model is not a model name".into(),
        ));
    }
    for field in ["model_manifest_sha256", "runtime_sha256", "plan_digest"] {
        if !text(field).is_some_and(is_digest) {
            return Err(CoreError::Config(format!(
                "review route identity {field} is not a digest"
            )));
        }
    }
    if !text("image_id")
        .and_then(|v| v.strip_prefix("sha256:"))
        .is_some_and(is_digest)
    {
        return Err(CoreError::Config(
            "review route identity image_id is not an image digest".into(),
        ));
    }
    Ok(())
}

/// Append one review attempt — finished or not. Earlier rounds are never
/// replaced; a later version gets a later round.
pub fn append_review(ledger: &Ledger, review: &NewReview) -> Result<()> {
    for (field, value) in [
        ("id", review.id.as_str()),
        ("change id", review.change_id.as_str()),
        ("reviewer identity", review.reviewer_identity.as_str()),
        ("review source", review.review_source.as_str()),
        (
            "reviewer relationship",
            review.reviewer_relationship.as_str(),
        ),
        (
            "relationship provenance",
            review.relationship_provenance.as_str(),
        ),
        ("target revision", review.target_revision.as_str()),
        ("examined inputs id", review.examined_inputs_id.as_str()),
        ("parser id", review.parser_id.as_str()),
    ] {
        require(field, value)?;
    }
    if review.review_round == 0 {
        return Err(CoreError::Config("review round must start at one".into()));
    }
    if !is_digest(&review.diff_sha256) {
        return Err(CoreError::Config("review diff digest is invalid".into()));
    }
    if review.raw_bytes.len() > RAW_LIMIT {
        return Err(CoreError::Config(
            "review raw artifact exceeds its 8 MiB limit".into(),
        ));
    }
    validate_route_identity(&review.route_identity_json)?;
    serde_json::from_str::<serde_json::Value>(&review.parsed_json)
        .map_err(|_| CoreError::Config("review parsed artifact is not JSON".into()))?;
    match (
        review.state,
        review.ended_at.as_deref(),
        review.recommendation,
    ) {
        (ReviewState::Complete, Some(at), _) if !at.trim().is_empty() && review.parse_complete => {}
        (ReviewState::Complete, _, _) => {
            return Err(CoreError::Config(
                "a complete review requires a completion time and a complete parsed record".into(),
            ));
        }
        (_, _, Some(_)) => {
            return Err(CoreError::Config(
                "a review that did not finish cannot carry a recommendation".into(),
            ));
        }
        _ => {}
    }
    if review.state != ReviewState::Complete && !review.findings.is_empty() {
        return Err(CoreError::Config(
            "a review that did not finish cannot carry findings".into(),
        ));
    }
    for finding in &review.findings {
        require("finding kind", &finding.kind)?;
    }

    let raw_sha256 = sha256_hex(&review.raw_bytes);
    let parsed_sha256 = sha256_hex(review.parsed_json.as_bytes());
    let limitations_json = serde_json::to_string(&review.limitations)?;
    let record_sha256 = record_binding_sha256(&RecordBinding {
        schema: BINDING_SCHEMA,
        id: &review.id,
        change_id: &review.change_id,
        review_round: review.review_round,
        reviewer_identity: &review.reviewer_identity,
        reviewer_type: review.reviewer_type.as_str(),
        review_source: &review.review_source,
        reviewer_relationship: &review.reviewer_relationship,
        relationship_provenance: &review.relationship_provenance,
        target_revision: &review.target_revision,
        scope_version: review.scope_version,
        examined_inputs_id: &review.examined_inputs_id,
        diff_sha256: &review.diff_sha256,
        route_identity_json: &review.route_identity_json,
        started_at: &review.started_at,
        ended_at: review.ended_at.as_deref(),
        state: review.state.as_str(),
        recommendation: review.recommendation.map(Recommendation::as_str),
        limitations_json: &limitations_json,
        raw_sha256: &raw_sha256,
        raw_bytes: review.raw_bytes.len() as u64,
        parser_id: &review.parser_id,
        parsed_sha256: &parsed_sha256,
        parse_complete: review.parse_complete,
        findings: &review.findings,
    })?;
    let tx = ledger.conn().unchecked_transaction()?;
    tx.execute(
        "INSERT INTO change_reviews(id, change_id, review_round, reviewer_identity,
             reviewer_type, review_source, reviewer_relationship, relationship_provenance,
             target_revision, scope_version, examined_inputs_id, diff_sha256,
             route_identity_json, started_at, ended_at, state, recommendation,
             limitations_json, record_sha256)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",
        params![
            review.id,
            review.change_id,
            review.review_round,
            review.reviewer_identity,
            review.reviewer_type.as_str(),
            review.review_source,
            review.reviewer_relationship,
            review.relationship_provenance,
            review.target_revision,
            review.scope_version,
            review.examined_inputs_id,
            review.diff_sha256,
            review.route_identity_json,
            review.started_at,
            review.ended_at,
            review.state.as_str(),
            review.recommendation.map(Recommendation::as_str),
            limitations_json,
            record_sha256,
        ],
    )?;
    tx.execute(
        "INSERT INTO change_review_raw_artifacts(review_id, raw_sha256, raw_bytes, byte_length)
         VALUES(?1,?2,?3,?4)",
        params![
            review.id,
            raw_sha256,
            review.raw_bytes,
            review.raw_bytes.len() as i64
        ],
    )?;
    tx.execute(
        "INSERT INTO change_review_parsed_artifacts(review_id, parser_id, parsed_json,
             parsed_sha256, parse_complete) VALUES(?1,?2,?3,?4,?5)",
        params![
            review.id,
            review.parser_id,
            review.parsed_json,
            parsed_sha256,
            i64::from(review.parse_complete),
        ],
    )?;
    for (ordinal, finding) in review.findings.iter().enumerate() {
        tx.execute(
            "INSERT INTO change_review_findings(review_id, ordinal, kind, message, path, line)
             VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                review.id,
                ordinal as i64,
                finding.kind,
                finding.message,
                finding.path,
                finding.line.map(|line| line as i64),
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// One review row with its children, exactly as stored.
struct Stored {
    id: String,
    change_id: String,
    review_round: u32,
    reviewer_identity: String,
    reviewer_type: String,
    review_source: String,
    reviewer_relationship: String,
    relationship_provenance: String,
    target_revision: String,
    scope_version: Option<u32>,
    examined_inputs_id: String,
    diff_sha256: String,
    route_identity_json: String,
    started_at: String,
    ended_at: Option<String>,
    state: String,
    recommendation: Option<String>,
    limitations_json: String,
    record_sha256: String,
    raw: Option<(String, Vec<u8>)>,
    parsed: Option<(String, String, String, bool)>,
    findings: Vec<ReviewFinding>,
}

const SELECT_STORED: &str = "SELECT r.id, r.change_id, r.review_round, r.reviewer_identity,
        r.reviewer_type, r.review_source, r.reviewer_relationship, r.relationship_provenance,
        r.target_revision, r.scope_version, r.examined_inputs_id, r.diff_sha256,
        r.route_identity_json, r.started_at, r.ended_at, r.state, r.recommendation,
        r.limitations_json, r.record_sha256, raw.raw_sha256, raw.raw_bytes,
        parsed.parser_id, parsed.parsed_json, parsed.parsed_sha256, parsed.parse_complete
   FROM change_reviews r
   LEFT JOIN change_review_raw_artifacts raw ON raw.review_id=r.id
   LEFT JOIN change_review_parsed_artifacts parsed ON parsed.review_id=r.id";

fn stored_rows(ledger: &Ledger, filter: &str, key: &str) -> Result<Vec<Stored>> {
    let sql = format!("{SELECT_STORED} WHERE {filter} ORDER BY r.review_round, r.started_at, r.id");
    let mut statement = ledger.conn().prepare(&sql)?;
    let rows = statement.query_map(params![key], |row| {
        let raw_sha: Option<String> = row.get(19)?;
        let raw_bytes: Option<Vec<u8>> = row.get(20)?;
        let parser_id: Option<String> = row.get(21)?;
        let parsed_json: Option<String> = row.get(22)?;
        let parsed_sha: Option<String> = row.get(23)?;
        let parse_complete: Option<i64> = row.get(24)?;
        Ok(Stored {
            id: row.get(0)?,
            change_id: row.get(1)?,
            review_round: row.get(2)?,
            reviewer_identity: row.get(3)?,
            reviewer_type: row.get(4)?,
            review_source: row.get(5)?,
            reviewer_relationship: row.get(6)?,
            relationship_provenance: row.get(7)?,
            target_revision: row.get(8)?,
            scope_version: row.get(9)?,
            examined_inputs_id: row.get(10)?,
            diff_sha256: row.get(11)?,
            route_identity_json: row.get(12)?,
            started_at: row.get(13)?,
            ended_at: row.get(14)?,
            state: row.get(15)?,
            recommendation: row.get(16)?,
            limitations_json: row.get(17)?,
            record_sha256: row.get(18)?,
            raw: raw_sha.zip(raw_bytes),
            parsed: match (parser_id, parsed_json, parsed_sha, parse_complete) {
                (Some(a), Some(b), Some(c), Some(d)) => Some((a, b, c, d != 0)),
                _ => None,
            },
            findings: vec![],
        })
    })?;
    let mut out = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    for stored in &mut out {
        let mut statement = ledger.conn().prepare(
            "SELECT kind, message, path, line FROM change_review_findings
              WHERE review_id=?1 ORDER BY ordinal",
        )?;
        stored.findings = statement
            .query_map(params![stored.id], |f| {
                Ok(ReviewFinding {
                    kind: f.get(0)?,
                    message: f.get(1)?,
                    path: f.get(2)?,
                    line: f.get::<_, Option<i64>>(3)?.map(|l| l.max(0) as u64),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
    }
    Ok(out)
}

/// Check one stored review against the digest its parent row sealed.
fn integrity_of(stored: &Stored) -> Result<Integrity> {
    let (Some((raw_sha, raw_bytes)), Some((parser_id, parsed_json, parsed_sha, complete))) =
        (&stored.raw, &stored.parsed)
    else {
        return Ok(Integrity::Missing);
    };
    if sha256_hex(raw_bytes) != *raw_sha
        || sha256_hex(parsed_json.as_bytes()) != *parsed_sha
        || serde_json::from_str::<serde_json::Value>(parsed_json).is_err()
    {
        return Ok(Integrity::Altered);
    }
    let sealed = record_binding_sha256(&RecordBinding {
        schema: BINDING_SCHEMA,
        id: &stored.id,
        change_id: &stored.change_id,
        review_round: stored.review_round,
        reviewer_identity: &stored.reviewer_identity,
        reviewer_type: &stored.reviewer_type,
        review_source: &stored.review_source,
        reviewer_relationship: &stored.reviewer_relationship,
        relationship_provenance: &stored.relationship_provenance,
        target_revision: &stored.target_revision,
        scope_version: stored.scope_version,
        examined_inputs_id: &stored.examined_inputs_id,
        diff_sha256: &stored.diff_sha256,
        route_identity_json: &stored.route_identity_json,
        started_at: &stored.started_at,
        ended_at: stored.ended_at.as_deref(),
        state: &stored.state,
        recommendation: stored.recommendation.as_deref(),
        limitations_json: &stored.limitations_json,
        raw_sha256: raw_sha,
        raw_bytes: raw_bytes.len() as u64,
        parser_id,
        parsed_sha256: parsed_sha,
        parse_complete: *complete,
        findings: &stored.findings,
    })?;
    Ok(if sealed == stored.record_sha256 {
        Integrity::Intact
    } else {
        Integrity::Altered
    })
}

/// Every review of one Change, oldest round first. Free text is masked here,
/// before anything is served. Raw bytes are never part of this view.
pub fn read_change_reviews(
    ledger: &Ledger,
    change_id: &str,
    masking: &mut MaskRecord,
) -> Result<Vec<ReviewView>> {
    let (current_revision, current_scope) =
        crate::change_decision::standing_target(ledger, change_id)?;
    let mut views = Vec::new();
    for stored in stored_rows(ledger, "r.change_id=?1", change_id)? {
        let integrity = integrity_of(&stored)?;
        let intact = integrity == Integrity::Intact;
        let state = ReviewState::parse(&stored.state)?;
        let raw = match &stored.raw {
            Some((sha, bytes)) => ArtifactIntegrity {
                available: true,
                sha256: Some(sha.clone()),
                bytes: Some(bytes.len() as u64),
                intact: sha256_hex(bytes) == *sha && intact,
            },
            None => ArtifactIntegrity {
                available: false,
                sha256: None,
                bytes: None,
                intact: false,
            },
        };
        let parsed = match &stored.parsed {
            Some((parser_id, json, sha, complete)) => ParsedIntegrity {
                available: true,
                parser_id: Some(parser_id.clone()),
                sha256: Some(sha.clone()),
                parse_complete: *complete,
                intact: sha256_hex(json.as_bytes()) == *sha && intact,
            },
            None => ParsedIntegrity {
                available: false,
                parser_id: None,
                sha256: None,
                parse_complete: false,
                intact: false,
            },
        };
        // A review that is not intact contributes nothing it claims: no
        // findings, no recommendation, no limitations text, no credit.
        let findings: Vec<ReviewFinding> = if intact {
            stored
                .findings
                .into_iter()
                .map(|f| ReviewFinding {
                    kind: mask_text(&f.kind, masking),
                    message: mask_text(&f.message, masking),
                    path: f.path.map(|p| mask_text(&p, masking)),
                    line: f.line,
                })
                .collect()
        } else {
            vec![]
        };
        let limitations = if intact {
            serde_json::from_str::<Vec<String>>(&stored.limitations_json)
                .unwrap_or_default()
                .into_iter()
                .map(|v| mask_text(&v, masking))
                .collect()
        } else {
            vec![]
        };
        let recommendation = if intact {
            stored
                .recommendation
                .as_deref()
                .map(Recommendation::parse)
                .transpose()?
        } else {
            None
        };
        let applies = intact
            && state == ReviewState::Complete
            && current_revision.as_deref() == Some(stored.target_revision.as_str())
            && current_scope == stored.scope_version;
        views.push(ReviewView {
            id: stored.id,
            review_round: stored.review_round,
            reviewer_identity: mask_text(&stored.reviewer_identity, masking),
            reviewer_type: ReviewerType::parse(&stored.reviewer_type)?,
            review_source: mask_text(&stored.review_source, masking),
            reviewer_relationship: mask_text(&stored.reviewer_relationship, masking),
            relationship_provenance: mask_text(&stored.relationship_provenance, masking),
            target_revision: stored.target_revision,
            scope_version: stored.scope_version,
            current_revision: current_revision.clone(),
            examined_inputs_id: stored.examined_inputs_id,
            diff_sha256: stored.diff_sha256,
            route_identity_json: mask_text(&stored.route_identity_json, masking),
            started_at: stored.started_at,
            ended_at: stored.ended_at,
            state,
            recommendation,
            limitations,
            finding_count: u32::try_from(findings.len()).unwrap_or(u32::MAX),
            findings,
            raw,
            parsed,
            integrity,
            applies_to_current_version: applies,
        });
    }
    Ok(views)
}

/// Why a drill-down read refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectRefusal {
    /// No review with that id belongs to that Change.
    NotFound,
    /// The review's stored parts no longer agree with its sealed digest.
    Integrity(Integrity),
    /// The bytes cannot be rendered as text safely.
    NotText,
}

/// A labelled, masked inspection derivative of one stored artifact. The
/// original bytes never leave the store; their identity is given beside the
/// derivative's own, so nobody mistakes one for the other.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Inspection {
    pub review_id: String,
    pub artifact: &'static str,
    pub original_sha256: String,
    pub original_bytes: u64,
    /// The digest of the compact JSON serialisation of `content`.
    pub derivative_sha256: String,
    pub derivative: &'static str,
    pub masking: crate::redact::Treatment,
    pub content: serde_json::Value,
}

fn one_stored(ledger: &Ledger, change_id: &str, review_id: &str) -> Result<Option<Stored>> {
    Ok(stored_rows(ledger, "r.id=?1", review_id)?
        .into_iter()
        .find(|s| s.change_id == change_id))
}

/// Strict standard-alphabet base64 with padding — the only form the capture
/// writes. Anything else is `None`, and the caller renders a note instead.
fn decode_base64(text: &str) -> Option<Vec<u8>> {
    fn value(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some(u32::from(c - b'A')),
            b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for (index, chunk) in bytes.chunks(4).enumerate() {
        let last = index + 1 == bytes.len() / 4;
        let pad = chunk.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 || (pad > 0 && !last) {
            return None;
        }
        let mut n = 0u32;
        for &c in &chunk[..4 - pad] {
            n = (n << 6) | value(c)?;
        }
        n <<= 6 * pad as u32;
        let decoded = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&decoded[..3 - pad]);
    }
    Some(out)
}

/// Walk a JSON value, decoding any `bytes_base64` field that holds UTF-8 text
/// into a masked `text` field, and masking every other string and key.
/// Undecodable or binary payloads are replaced by a note, never passed on.
fn mask_value(value: serde_json::Value, record: &mut MaskRecord) -> serde_json::Value {
    match value {
        serde_json::Value::String(s) => serde_json::Value::String(mask_text(&s, record)),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(|v| mask_value(v, record)).collect())
        }
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, v) in map {
                if key == "bytes_base64" {
                    let decoded = v.as_str().and_then(decode_base64);
                    match decoded.and_then(|b| String::from_utf8(b).ok()) {
                        Some(text) => {
                            out.insert(
                                "text".into(),
                                serde_json::Value::String(mask_text(&text, record)),
                            );
                        }
                        None => {
                            out.insert(
                                "text_unavailable".into(),
                                serde_json::Value::String(
                                    "binary or undecodable bytes are not rendered".into(),
                                ),
                            );
                        }
                    }
                } else {
                    out.insert(mask_text(&key, record), mask_value(v, record));
                }
            }
            serde_json::Value::Object(out)
        }
        other => other,
    }
}

fn inspection(
    review_id: String,
    artifact: &'static str,
    original: &[u8],
    content: serde_json::Value,
    record: MaskRecord,
) -> Result<Inspection> {
    let derivative_sha256 = sha256_hex(&serde_json::to_vec(&content)?);
    Ok(Inspection {
        review_id,
        artifact,
        original_sha256: sha256_hex(original),
        original_bytes: original.len() as u64,
        derivative_sha256,
        derivative: "masked inspection copy; the original bytes stay on this machine unchanged",
        masking: record.to_treatment(),
        content,
    })
}

/// The raw report, as a masked derivative. Refuses on any integrity failure:
/// a missing or altered artifact is a provenance failure, never a fallback.
pub fn inspect_raw(
    ledger: &Ledger,
    change_id: &str,
    review_id: &str,
) -> Result<std::result::Result<Inspection, InspectRefusal>> {
    let Some(stored) = one_stored(ledger, change_id, review_id)? else {
        return Ok(Err(InspectRefusal::NotFound));
    };
    let integrity = integrity_of(&stored)?;
    if integrity != Integrity::Intact {
        return Ok(Err(InspectRefusal::Integrity(integrity)));
    }
    let Some((_, bytes)) = &stored.raw else {
        return Ok(Err(InspectRefusal::Integrity(Integrity::Missing)));
    };
    let mut record = MaskRecord::default();
    let content = match serde_json::from_slice::<serde_json::Value>(bytes) {
        Ok(value) => mask_value(value, &mut record),
        Err(_) => match std::str::from_utf8(bytes) {
            Ok(text) => serde_json::Value::String(mask_text(text, &mut record)),
            Err(_) => return Ok(Err(InspectRefusal::NotText)),
        },
    };
    Ok(Ok(inspection(
        stored.id.clone(),
        "raw",
        bytes,
        content,
        record,
    )?))
}

/// The parsed record, as a masked derivative, under the same integrity rule.
pub fn inspect_parsed(
    ledger: &Ledger,
    change_id: &str,
    review_id: &str,
) -> Result<std::result::Result<Inspection, InspectRefusal>> {
    let Some(stored) = one_stored(ledger, change_id, review_id)? else {
        return Ok(Err(InspectRefusal::NotFound));
    };
    let integrity = integrity_of(&stored)?;
    if integrity != Integrity::Intact {
        return Ok(Err(InspectRefusal::Integrity(integrity)));
    }
    let Some((_, json, _, _)) = &stored.parsed else {
        return Ok(Err(InspectRefusal::Integrity(Integrity::Missing)));
    };
    let mut record = MaskRecord::default();
    let value: serde_json::Value = serde_json::from_str(json)?;
    let content = mask_value(value, &mut record);
    Ok(Ok(inspection(
        stored.id.clone(),
        "parsed",
        json.as_bytes(),
        content,
        record,
    )?))
}
