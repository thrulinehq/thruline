//! Operator-attested navigation associations between two exact event receipts.
//!
//! This is deliberately **not** an identity merge. Projects, sessions, events,
//! chains, nodes, and source identities are never updated here. A proposal is a
//! content-addressed, append-only candidate; a separate human decision may
//! accept it as a navigation association, reject it, or abstain. Model output
//! can propose only through an existing classification annotation tied to one
//! of the two receipts, and can never adjudicate its own proposal.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::chain::sha256_hex;
use crate::error::{CoreError, Result};
use crate::language::ShadowLabel;
use crate::ledger::Ledger;

const CANDIDATE_REVISION: &str = "entity-association-v1";

/// Optional byte offsets into the subject event's recorded text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactSpan {
    pub start: u32,
    pub end: u32,
}

impl ExactSpan {
    pub fn new(start: u32, end: u32) -> Result<Self> {
        if start >= end {
            return Err(CoreError::Config(
                "an entity-association span must have start < end".to_string(),
            ));
        }
        Ok(Self { start, end })
    }
}

/// One exact receipt subject. Subjects are ordered canonically before hashing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssociationSubject {
    pub event_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<ExactSpan>,
}

impl AssociationSubject {
    pub fn new(event_id: impl Into<String>, span: Option<ExactSpan>) -> Result<Self> {
        let event_id = event_id.into();
        if event_id.trim() != event_id || event_id.is_empty() {
            return Err(CoreError::Config(
                "an entity-association event id must be nonempty with no surrounding whitespace"
                    .to_string(),
            ));
        }
        Ok(Self { event_id, span })
    }
}

/// The only two sources allowed to propose an association.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Proposer {
    Operator,
    Model,
}

impl Proposer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Operator => "operator",
            Self::Model => "model",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "operator" => Ok(Self::Operator),
            "model" => Ok(Self::Model),
            _ => Err(CoreError::Integrity(format!(
                "stored entity-association proposer is outside the closed domain: {value}"
            ))),
        }
    }
}

/// Provenance is closed by construction: an operator names the operator, while
/// a model names the immutable classification annotation that made the proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "proposer", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProposalProvenance {
    Operator { operator: String },
    Model { annotation_id: String },
}

impl ProposalProvenance {
    pub fn proposer(&self) -> Proposer {
        match self {
            Self::Operator { .. } => Proposer::Operator,
            Self::Model { .. } => Proposer::Model,
        }
    }

    fn operator(&self) -> Option<&str> {
        match self {
            Self::Operator { operator } => Some(operator),
            Self::Model { .. } => None,
        }
    }

    fn annotation_id(&self) -> Option<&str> {
        match self {
            Self::Operator { .. } => None,
            Self::Model { annotation_id } => Some(annotation_id),
        }
    }
}

/// Caller input for a read-only proposal preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalInput {
    pub project_id: String,
    pub first: AssociationSubject,
    pub second: AssociationSubject,
    pub label: String,
    pub provenance: ProposalProvenance,
    pub proposed_at_unix_ms: i64,
}

/// One immutable candidate row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssociationProposal {
    pub candidate_id: String,
    pub project_id: String,
    pub first: AssociationSubject,
    pub second: AssociationSubject,
    pub label: String,
    pub provenance: ProposalProvenance,
    pub proposed_at_unix_ms: i64,
}

/// A human's one-shot decision. Insufficient evidence is an abstention, never
/// a weaker acceptance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssociationDecision {
    Associate,
    Reject,
    InsufficientEvidence,
}

impl AssociationDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Associate => "associate",
            Self::Reject => "reject",
            Self::InsufficientEvidence => "insufficient_evidence",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "associate" => Ok(Self::Associate),
            "reject" => Ok(Self::Reject),
            "insufficient_evidence" => Ok(Self::InsufficientEvidence),
            _ => Err(CoreError::Integrity(format!(
                "stored entity-association decision is outside the closed domain: {value}"
            ))),
        }
    }
}

/// The separate, human-authored decision row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedDecision {
    pub candidate_id: String,
    pub decision: AssociationDecision,
    pub operator: String,
    pub route: String,
    pub decided_at_unix_ms: i64,
}

/// Proposal plus its optional human decision for list/read surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssociationRecord {
    pub proposal: AssociationProposal,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<RecordedDecision>,
}

impl AssociationRecord {
    pub fn accepted(&self) -> bool {
        self.decision
            .as_ref()
            .is_some_and(|decision| decision.decision == AssociationDecision::Associate)
    }
}

fn nonempty(field: &str, value: &str) -> Result<()> {
    if value.trim() != value || value.is_empty() {
        return Err(CoreError::Config(format!(
            "entity-association {field} must be nonempty with no surrounding whitespace"
        )));
    }
    Ok(())
}

fn safe_input_label(value: String) -> Result<String> {
    ShadowLabel::new(value)
        .map(|label| label.as_str().to_string())
        .map_err(|_| {
            CoreError::Config(
                "entity-association label must be a safe one-to-four-word shadow label".to_string(),
            )
        })
}

fn safe_stored_label(value: String) -> Result<String> {
    ShadowLabel::new(value)
        .map(|label| label.as_str().to_string())
        .map_err(|_| {
            CoreError::Integrity(
                "stored entity-association label violates the shadow-label contract".to_string(),
            )
        })
}

#[derive(Debug)]
struct EventReceipt {
    project_id: String,
    text: Option<String>,
}

fn event_receipt(ledger: &Ledger, event_id: &str) -> Result<EventReceipt> {
    ledger
        .conn()
        .query_row(
            "SELECT project_id, text FROM events WHERE id = ?1",
            params![event_id],
            |row| {
                Ok(EventReceipt {
                    project_id: row.get(0)?,
                    text: row.get(1)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::Config(format!(
                "entity-association event does not exist: {event_id}"
            ))
        })
}

fn validate_exact_span(subject: &AssociationSubject, receipt: &EventReceipt) -> Result<()> {
    let Some(span) = subject.span else {
        return Ok(());
    };
    let text = receipt.text.as_deref().ok_or_else(|| {
        CoreError::Config(format!(
            "entity-association event {} has no recorded text for an exact span",
            subject.event_id
        ))
    })?;
    let start = span.start as usize;
    let end = span.end as usize;
    if end > text.len() || !text.is_char_boundary(start) || !text.is_char_boundary(end) {
        return Err(CoreError::Config(format!(
            "entity-association span for event {} is outside recorded text or splits UTF-8",
            subject.event_id
        )));
    }
    Ok(())
}

fn immutable_model_binding(model: &str) -> bool {
    let Some((tag, digest)) = model.rsplit_once("@sha256:") else {
        return false;
    };
    !tag.is_empty()
        && digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_provenance(
    ledger: &Ledger,
    project_id: &str,
    first_event_id: &str,
    second_event_id: &str,
    provenance: &ProposalProvenance,
) -> Result<()> {
    match provenance {
        ProposalProvenance::Operator { operator } => nonempty("proposer operator", operator),
        ProposalProvenance::Model { annotation_id } => {
            nonempty("classification annotation id", annotation_id)?;
            let Some(annotation) = crate::classification_store::find_by_id(ledger, annotation_id)?
            else {
                return Err(CoreError::Config(
                    "model entity-association proposal names no existing annotation".to_string(),
                ));
            };
            if !immutable_model_binding(&annotation.model) {
                return Err(CoreError::Config(
                    "model entity-association proposal requires an immutable @sha256 model binding"
                        .to_string(),
                ));
            }
            let annotation_project = event_receipt(ledger, &annotation.event_id)?.project_id;
            if annotation_project != project_id
                || (annotation.event_id != first_event_id && annotation.event_id != second_event_id)
            {
                return Err(CoreError::Config(
                    "model entity-association annotation is not bound to either subject receipt"
                        .to_string(),
                ));
            }
            Ok(())
        }
    }
}

fn push_field(material: &mut Vec<u8>, value: &str) {
    material.extend_from_slice(&(value.len() as u64).to_be_bytes());
    material.extend_from_slice(value.as_bytes());
}

fn push_span(material: &mut Vec<u8>, span: Option<ExactSpan>) {
    match span {
        Some(span) => {
            material.push(1);
            material.extend_from_slice(&span.start.to_be_bytes());
            material.extend_from_slice(&span.end.to_be_bytes());
        }
        None => material.push(0),
    }
}

fn candidate_id(
    project_id: &str,
    first: &AssociationSubject,
    second: &AssociationSubject,
    label: &str,
    provenance: &ProposalProvenance,
) -> String {
    let mut material = Vec::new();
    push_field(&mut material, CANDIDATE_REVISION);
    push_field(&mut material, project_id);
    push_field(&mut material, &first.event_id);
    push_span(&mut material, first.span);
    push_field(&mut material, &second.event_id);
    push_span(&mut material, second.span);
    push_field(&mut material, label);
    push_field(&mut material, provenance.proposer().as_str());
    push_field(&mut material, provenance.operator().unwrap_or_default());
    push_field(
        &mut material,
        provenance.annotation_id().unwrap_or_default(),
    );
    format!("entity_assoc_{}", sha256_hex(&material))
}

/// Validate and build a candidate without writing anything.
pub fn prepare(ledger: &Ledger, mut input: ProposalInput) -> Result<AssociationProposal> {
    nonempty("project id", &input.project_id)?;
    if input.proposed_at_unix_ms < 0 {
        return Err(CoreError::Config(
            "entity-association proposal timestamp must be nonnegative".to_string(),
        ));
    }
    if input.first.event_id == input.second.event_id {
        return Err(CoreError::Config(
            "an entity association requires two distinct event receipts".to_string(),
        ));
    }
    let first_receipt = event_receipt(ledger, &input.first.event_id)?;
    let second_receipt = event_receipt(ledger, &input.second.event_id)?;
    if first_receipt.project_id != input.project_id || second_receipt.project_id != input.project_id
    {
        return Err(CoreError::Config(
            "both entity-association receipts must belong to the exact requested project"
                .to_string(),
        ));
    }
    validate_exact_span(&input.first, &first_receipt)?;
    validate_exact_span(&input.second, &second_receipt)?;
    if input.second < input.first {
        std::mem::swap(&mut input.first, &mut input.second);
    }
    let label = safe_input_label(input.label)?;
    validate_provenance(
        ledger,
        &input.project_id,
        &input.first.event_id,
        &input.second.event_id,
        &input.provenance,
    )?;
    let id = candidate_id(
        &input.project_id,
        &input.first,
        &input.second,
        &label,
        &input.provenance,
    );
    Ok(AssociationProposal {
        candidate_id: id,
        project_id: input.project_id,
        first: input.first,
        second: input.second,
        label,
        provenance: input.provenance,
        proposed_at_unix_ms: input.proposed_at_unix_ms,
    })
}

#[derive(Debug)]
struct RawProposal {
    candidate_id: String,
    project_id: String,
    first_event_id: String,
    first_span_start: Option<i64>,
    first_span_end: Option<i64>,
    second_event_id: String,
    second_span_start: Option<i64>,
    second_span_end: Option<i64>,
    label: String,
    proposer: String,
    proposer_operator: Option<String>,
    annotation_id: Option<String>,
    proposed_at_unix_ms: i64,
}

fn decode_span(start: Option<i64>, end: Option<i64>) -> Result<Option<ExactSpan>> {
    match (start, end) {
        (None, None) => Ok(None),
        (Some(start), Some(end)) => {
            let start = u32::try_from(start).map_err(|_| {
                CoreError::Integrity("stored entity-association span is negative".to_string())
            })?;
            let end = u32::try_from(end).map_err(|_| {
                CoreError::Integrity("stored entity-association span is out of range".to_string())
            })?;
            ExactSpan::new(start, end).map(Some).map_err(|_| {
                CoreError::Integrity("stored entity-association span is malformed".to_string())
            })
        }
        _ => Err(CoreError::Integrity(
            "stored entity-association span is only partially present".to_string(),
        )),
    }
}

fn proposal_from_raw(raw: RawProposal) -> Result<AssociationProposal> {
    let first = AssociationSubject::new(
        raw.first_event_id,
        decode_span(raw.first_span_start, raw.first_span_end)?,
    )?;
    let second = AssociationSubject::new(
        raw.second_event_id,
        decode_span(raw.second_span_start, raw.second_span_end)?,
    )?;
    let label = safe_stored_label(raw.label)?;
    let proposer = Proposer::parse(&raw.proposer)?;
    let provenance = match (proposer, raw.proposer_operator, raw.annotation_id) {
        (Proposer::Operator, Some(operator), None) => {
            nonempty("stored proposer operator", &operator)?;
            ProposalProvenance::Operator { operator }
        }
        (Proposer::Model, None, Some(annotation_id)) => {
            nonempty("stored classification annotation id", &annotation_id)?;
            ProposalProvenance::Model { annotation_id }
        }
        _ => {
            return Err(CoreError::Integrity(
                "stored entity-association proposer provenance is inconsistent".to_string(),
            ))
        }
    };
    Ok(AssociationProposal {
        candidate_id: raw.candidate_id,
        project_id: raw.project_id,
        first,
        second,
        label,
        provenance,
        proposed_at_unix_ms: raw.proposed_at_unix_ms,
    })
}

fn validate_stored_proposal(
    ledger: &Ledger,
    proposal: AssociationProposal,
) -> Result<AssociationProposal> {
    let prepared = prepare(
        ledger,
        ProposalInput {
            project_id: proposal.project_id.clone(),
            first: proposal.first.clone(),
            second: proposal.second.clone(),
            label: proposal.label.clone(),
            provenance: proposal.provenance.clone(),
            proposed_at_unix_ms: proposal.proposed_at_unix_ms,
        },
    )
    .map_err(|error| match error {
        CoreError::Integrity(_) => error,
        other => CoreError::Integrity(format!(
            "stored entity-association proposal failed canonical validation: {other}"
        )),
    })?;
    if prepared != proposal {
        return Err(CoreError::Integrity(
            "stored entity-association proposal disagrees with its canonical content address"
                .to_string(),
        ));
    }
    Ok(proposal)
}

fn query_proposals(
    ledger: &Ledger,
    where_clause: &str,
    value: Option<&str>,
) -> Result<Vec<AssociationProposal>> {
    let sql = format!(
        "SELECT candidate_id, project_id,
                first_event_id, first_span_start, first_span_end,
                second_event_id, second_span_start, second_span_end,
                label, proposer, proposer_operator, annotation_id, proposed_at_unix_ms
           FROM entity_association_proposals {where_clause}
          ORDER BY proposed_at_unix_ms, candidate_id"
    );
    let mut stmt = ledger.conn().prepare(&sql)?;
    let map = |row: &rusqlite::Row<'_>| {
        Ok(RawProposal {
            candidate_id: row.get(0)?,
            project_id: row.get(1)?,
            first_event_id: row.get(2)?,
            first_span_start: row.get(3)?,
            first_span_end: row.get(4)?,
            second_event_id: row.get(5)?,
            second_span_start: row.get(6)?,
            second_span_end: row.get(7)?,
            label: row.get(8)?,
            proposer: row.get(9)?,
            proposer_operator: row.get(10)?,
            annotation_id: row.get(11)?,
            proposed_at_unix_ms: row.get(12)?,
        })
    };
    let mut raw = Vec::new();
    match value {
        Some(value) => {
            let rows = stmt.query_map(params![value], map)?;
            for row in rows {
                raw.push(row?);
            }
        }
        None => {
            let rows = stmt.query_map([], map)?;
            for row in rows {
                raw.push(row?);
            }
        }
    }
    raw.into_iter()
        .map(proposal_from_raw)
        .map(|proposal| validate_stored_proposal(ledger, proposal?))
        .collect()
}

/// Read one immutable proposal by content address.
pub fn proposal_for(ledger: &Ledger, candidate_id: &str) -> Result<Option<AssociationProposal>> {
    Ok(
        query_proposals(ledger, "WHERE candidate_id = ?1", Some(candidate_id))?
            .into_iter()
            .next(),
    )
}

/// Append a previously prepared proposal. Repeating the identical proposal is
/// idempotent and returns `false`; a conflicting row or a changed source state
/// fails closed rather than replacing history.
pub fn record_proposal(ledger: &Ledger, proposal: &AssociationProposal) -> Result<bool> {
    let prepared = prepare(
        ledger,
        ProposalInput {
            project_id: proposal.project_id.clone(),
            first: proposal.first.clone(),
            second: proposal.second.clone(),
            label: proposal.label.clone(),
            provenance: proposal.provenance.clone(),
            proposed_at_unix_ms: proposal.proposed_at_unix_ms,
        },
    )?;
    if prepared != *proposal {
        return Err(CoreError::Integrity(
            "entity-association proposal does not match its canonical content address".to_string(),
        ));
    }
    if let Some(existing) = proposal_for(ledger, &proposal.candidate_id)? {
        if existing.candidate_id == proposal.candidate_id
            && existing.project_id == proposal.project_id
            && existing.first == proposal.first
            && existing.second == proposal.second
            && existing.label == proposal.label
            && existing.provenance == proposal.provenance
        {
            // The first observation time remains the immutable audit row. A
            // later preview of the same content-addressed candidate is a
            // replay, not a new candidate and not a reason to rewrite time.
            return Ok(false);
        }
        return Err(CoreError::Integrity(
            "entity-association candidate id already names different immutable content".to_string(),
        ));
    }
    ledger.conn().execute(
        "INSERT INTO entity_association_proposals(
             candidate_id, project_id,
             first_event_id, first_span_start, first_span_end,
             second_event_id, second_span_start, second_span_end,
             label, proposer, proposer_operator, annotation_id, proposed_at_unix_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![
            proposal.candidate_id,
            proposal.project_id,
            proposal.first.event_id,
            proposal.first.span.map(|span| i64::from(span.start)),
            proposal.first.span.map(|span| i64::from(span.end)),
            proposal.second.event_id,
            proposal.second.span.map(|span| i64::from(span.start)),
            proposal.second.span.map(|span| i64::from(span.end)),
            proposal.label,
            proposal.provenance.proposer().as_str(),
            proposal.provenance.operator(),
            proposal.provenance.annotation_id(),
            proposal.proposed_at_unix_ms,
        ],
    )?;
    Ok(true)
}

/// Convenience for the CLI's preview/apply boundary.
pub fn propose(
    ledger: &Ledger,
    input: ProposalInput,
    apply: bool,
) -> Result<(AssociationProposal, bool)> {
    let proposal = prepare(ledger, input)?;
    let appended = if apply {
        record_proposal(ledger, &proposal)?
    } else {
        false
    };
    Ok((proposal, appended))
}

/// The one decision already recorded for a candidate, if any.
pub fn decision_for(ledger: &Ledger, candidate_id: &str) -> Result<Option<RecordedDecision>> {
    let raw: Option<(String, String, String, String, i64)> = ledger
        .conn()
        .query_row(
            "SELECT candidate_id, decision, operator, route, decided_at_unix_ms
               FROM entity_association_decisions WHERE candidate_id = ?1",
            params![candidate_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?;
    raw.map(
        |(candidate_id, decision, operator, route, decided_at_unix_ms)| {
            nonempty("stored decision operator", &operator)?;
            nonempty("stored decision route", &route)?;
            if decided_at_unix_ms < 0 {
                return Err(CoreError::Integrity(
                    "stored entity-association decision timestamp is negative".to_string(),
                ));
            }
            Ok(RecordedDecision {
                candidate_id,
                decision: AssociationDecision::parse(&decision)?,
                operator,
                route,
                decided_at_unix_ms,
            })
        },
    )
    .transpose()
}

/// Append the human's one and only decision for a candidate.
pub fn adjudicate(
    ledger: &Ledger,
    candidate_id: &str,
    decision: AssociationDecision,
    operator: &str,
    route: &str,
    decided_at_unix_ms: i64,
) -> Result<RecordedDecision> {
    nonempty("decision operator", operator)?;
    nonempty("decision route", route)?;
    if decided_at_unix_ms < 0 {
        return Err(CoreError::Config(
            "entity-association decision timestamp must be nonnegative".to_string(),
        ));
    }
    if proposal_for(ledger, candidate_id)?.is_none() {
        return Err(CoreError::Config(format!(
            "entity-association candidate does not exist: {candidate_id}"
        )));
    }
    if decision_for(ledger, candidate_id)?.is_some() {
        return Err(CoreError::Config(
            "entity-association candidate already carries a human decision; history was not replaced"
                .to_string(),
        ));
    }
    ledger.conn().execute(
        "INSERT INTO entity_association_decisions(
             candidate_id, decision, operator, route, decided_at_unix_ms)
         VALUES(?1,?2,?3,?4,?5)",
        params![
            candidate_id,
            decision.as_str(),
            operator,
            route,
            decided_at_unix_ms
        ],
    )?;
    Ok(RecordedDecision {
        candidate_id: candidate_id.to_string(),
        decision,
        operator: operator.to_string(),
        route: route.to_string(),
        decided_at_unix_ms,
    })
}

/// List proposals in deterministic order, optionally limited to one project.
pub fn list(ledger: &Ledger, project_id: Option<&str>) -> Result<Vec<AssociationRecord>> {
    let proposals = match project_id {
        Some(project_id) => query_proposals(ledger, "WHERE project_id = ?1", Some(project_id))?,
        None => query_proposals(ledger, "", None)?,
    };
    proposals
        .into_iter()
        .map(|proposal| {
            Ok(AssociationRecord {
                decision: decision_for(ledger, &proposal.candidate_id)?,
                proposal,
            })
        })
        .collect()
}

/// Accepted navigation associations only. Rejection and abstention remain in
/// history but never enter this projection.
pub fn accepted(ledger: &Ledger, project_id: Option<&str>) -> Result<Vec<AssociationRecord>> {
    Ok(list(ledger, project_id)?
        .into_iter()
        .filter(AssociationRecord::accepted)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EventKind, NormalizedEvent, ProjectRef, RawLineRef, SessionRef, Source};

    const PROJECT_A: &str = "project-a";
    const PROJECT_B: &str = "project-b";

    struct Fixture {
        ledger: Ledger,
        a1: String,
        a2: String,
        b1: String,
    }

    fn add_project(ledger: &mut Ledger, project_id: &str, session_id: &str) -> Vec<String> {
        ledger
            .upsert_project(
                &ProjectRef {
                    id: project_id.to_string(),
                    root_path: format!("/tmp/{project_id}"),
                    source: Source::ClaudeCode,
                    display_name: Some("same display name".to_string()),
                },
                "2026-08-26T00:00:00Z",
            )
            .expect("project");
        ledger
            .upsert_session(&SessionRef {
                id: session_id.to_string(),
                project_id: project_id.to_string(),
                file_path: format!("/tmp/{project_id}/{session_id}.jsonl"),
                started_at: Some("2026-08-26T00:00:00Z".to_string()),
                raw_line_count: 0,
                raw_digest: None,
            })
            .expect("session");
        let items: Vec<_> = (1..=2)
            .map(|line| {
                let mut event = NormalizedEvent::new(EventKind::AssistantText);
                event.text = Some(if line == 1 {
                    "évidence subject".to_string()
                } else {
                    "recorded subject two".to_string()
                });
                (
                    RawLineRef {
                        file: format!("/tmp/{project_id}/{session_id}.jsonl"),
                        line,
                        sha256: format!("{line:064x}"),
                    },
                    event,
                )
            })
            .collect();
        ledger
            .append_session_events(project_id, session_id, &items)
            .expect("events")
            .into_iter()
            .map(|event| event.id)
            .collect()
    }

    fn fixture() -> Fixture {
        let mut ledger = Ledger::open_in_memory().expect("ledger");
        let a = add_project(&mut ledger, PROJECT_A, "session-a");
        let b = add_project(&mut ledger, PROJECT_B, "session-b");
        Fixture {
            ledger,
            a1: a[0].clone(),
            a2: a[1].clone(),
            b1: b[0].clone(),
        }
    }

    fn operator_input(first: &str, second: &str) -> ProposalInput {
        ProposalInput {
            project_id: PROJECT_A.to_string(),
            first: AssociationSubject::new(first, None).expect("first"),
            second: AssociationSubject::new(second, None).expect("second"),
            label: "Parser family".to_string(),
            provenance: ProposalProvenance::Operator {
                operator: "reviewer".to_string(),
            },
            proposed_at_unix_ms: 100,
        }
    }

    #[test]
    fn canonical_pair_order_is_one_idempotent_candidate() {
        let fixture = fixture();
        let forward =
            prepare(&fixture.ledger, operator_input(&fixture.a1, &fixture.a2)).expect("forward");
        let mut later_input = operator_input(&fixture.a2, &fixture.a1);
        later_input.proposed_at_unix_ms = 999;
        let reverse = prepare(&fixture.ledger, later_input).expect("reverse");
        assert_eq!(forward.candidate_id, reverse.candidate_id);
        assert_eq!(forward.first, reverse.first);
        assert_eq!(forward.second, reverse.second);
        assert!(record_proposal(&fixture.ledger, &forward).expect("first record"));
        assert!(!record_proposal(&fixture.ledger, &reverse).expect("idempotent record"));
        let listed = list(&fixture.ledger, Some(PROJECT_A)).expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].proposal.proposed_at_unix_ms, 100);
    }

    #[test]
    fn spans_and_proposer_provenance_are_part_of_candidate_identity() {
        let fixture = fixture();
        let plain =
            prepare(&fixture.ledger, operator_input(&fixture.a1, &fixture.a2)).expect("plain");
        let mut spanned_input = operator_input(&fixture.a2, &fixture.a1);
        spanned_input.first.span = Some(ExactSpan::new(1, 4).expect("span"));
        let spanned = prepare(&fixture.ledger, spanned_input).expect("spanned");
        assert_ne!(plain.candidate_id, spanned.candidate_id);
        assert!(spanned.first.span.is_none() || spanned.second.span.is_none());
        assert_eq!(
            [spanned.first.clone(), spanned.second.clone()]
                .iter()
                .filter(|subject| subject.span.is_some())
                .count(),
            1
        );

        let mut another_operator = operator_input(&fixture.a1, &fixture.a2);
        another_operator.provenance = ProposalProvenance::Operator {
            operator: "another-reviewer".to_string(),
        };
        assert_ne!(
            plain.candidate_id,
            prepare(&fixture.ledger, another_operator)
                .expect("other provenance")
                .candidate_id
        );
    }

    #[test]
    fn same_name_projects_never_merge_and_bad_subjects_fail_closed() {
        let fixture = fixture();
        assert!(prepare(&fixture.ledger, operator_input(&fixture.a1, &fixture.b1)).is_err());
        assert!(prepare(&fixture.ledger, operator_input(&fixture.a1, &fixture.a1)).is_err());
        assert!(prepare(
            &fixture.ledger,
            operator_input(&fixture.a1, "missing-event")
        )
        .is_err());
        assert!(ExactSpan::new(9, 9).is_err());
        let mut mid_codepoint = operator_input(&fixture.a1, &fixture.a2);
        mid_codepoint.first.span = Some(ExactSpan::new(1, 2).expect("numeric span"));
        assert!(prepare(&fixture.ledger, mid_codepoint).is_err());
        let mut out_of_range = operator_input(&fixture.a1, &fixture.a2);
        out_of_range.first.span = Some(ExactSpan::new(0, 999).expect("numeric span"));
        assert!(prepare(&fixture.ledger, out_of_range).is_err());
        let mut valid_unicode = operator_input(&fixture.a1, &fixture.a2);
        valid_unicode.first.span = Some(ExactSpan::new(0, 2).expect("unicode boundary"));
        assert!(prepare(&fixture.ledger, valid_unicode).is_ok());
        let project_count: i64 = fixture
            .ledger
            .conn()
            .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
            .expect("project count");
        assert_eq!(project_count, 2);
    }

    #[test]
    fn model_proposal_requires_an_exact_classification_receipt() {
        let fixture = fixture();
        let model = format!("local-model@sha256:{}", "a".repeat(64));
        let class_a = crate::classification_store::store(
            &fixture.ledger,
            crate::classification_store::NewClassification {
                event_id: &fixture.a1,
                model: &model,
                prompt: "classify a",
                output_json: "{}",
                confidence: 0.5,
                created_at: "now",
            },
        )
        .expect("classification annotation")
        .annotation;
        let class_b = crate::classification_store::store(
            &fixture.ledger,
            crate::classification_store::NewClassification {
                event_id: &fixture.b1,
                model: &model,
                prompt: "classify b",
                output_json: "{}",
                confidence: 0.5,
                created_at: "now",
            },
        )
        .expect("other classification annotation")
        .annotation;
        fixture
            .ledger
            .conn()
            .execute(
                "INSERT INTO annotations(id,event_id,layer,model,prompt_sha256,output_json,
                    output_sha256,confidence,created_at)
                 VALUES('entail-a',?1,'entailment','local','prompt','{}','output',NULL,'now')",
                params![fixture.a1],
            )
            .expect("wrong-layer annotation");
        fixture
            .ledger
            .conn()
            .execute(
                "INSERT INTO annotations(id,event_id,layer,model,prompt_sha256,output_json,
                    output_sha256,confidence,created_at)
                 VALUES('class-corrupt',?1,'classification',?2,'corrupt-prompt','{}',
                        'not-the-output-hash',0.5,'now')",
                params![fixture.a2, model],
            )
            .expect("corrupt classification annotation");
        fixture
            .ledger
            .conn()
            .execute(
                "INSERT INTO annotations(id,event_id,layer,model,prompt_sha256,output_json,
                    output_sha256,confidence,created_at)
                 VALUES('class-legacy',?1,'classification','legacy-tag','legacy-prompt','{}',
                        ?2,0.5,'now')",
                params![fixture.a2, sha256_hex(b"{}")],
            )
            .expect("legacy classification annotation");

        let model_input = |annotation_id: &str| ProposalInput {
            project_id: PROJECT_A.to_string(),
            first: AssociationSubject::new(&fixture.a1, None).expect("first"),
            second: AssociationSubject::new(&fixture.a2, None).expect("second"),
            label: "Parser family".to_string(),
            provenance: ProposalProvenance::Model {
                annotation_id: annotation_id.to_string(),
            },
            proposed_at_unix_ms: 101,
        };
        assert!(prepare(&fixture.ledger, model_input(&class_a.id)).is_ok());
        for bad in [
            "missing",
            class_b.id.as_str(),
            "entail-a",
            "class-corrupt",
            "class-legacy",
        ] {
            assert!(
                prepare(&fixture.ledger, model_input(bad)).is_err(),
                "accepted {bad}"
            );
        }
    }

    #[test]
    fn one_human_decision_is_append_only_and_abstention_is_not_acceptance() {
        let fixture = fixture();
        let proposal =
            prepare(&fixture.ledger, operator_input(&fixture.a1, &fixture.a2)).expect("proposal");
        record_proposal(&fixture.ledger, &proposal).expect("record");
        let decision = adjudicate(
            &fixture.ledger,
            &proposal.candidate_id,
            AssociationDecision::InsufficientEvidence,
            "human-reviewer",
            "cli:entity",
            200,
        )
        .expect("decision");
        assert_eq!(decision.operator, "human-reviewer");
        assert_eq!(decision.route, "cli:entity");
        assert_eq!(decision.decided_at_unix_ms, 200);
        assert!(accepted(&fixture.ledger, Some(PROJECT_A))
            .expect("accepted")
            .is_empty());
        assert!(adjudicate(
            &fixture.ledger,
            &proposal.candidate_id,
            AssociationDecision::Associate,
            "another-human",
            "cli:entity",
            201,
        )
        .is_err());
        assert!(fixture
            .ledger
            .conn()
            .execute(
                "UPDATE entity_association_decisions SET decision='associate' WHERE candidate_id=?1",
                params![proposal.candidate_id],
            )
            .is_err());
        assert!(fixture
            .ledger
            .conn()
            .execute(
                "INSERT OR REPLACE INTO entity_association_decisions(
                     candidate_id,decision,operator,route,decided_at_unix_ms)
                 VALUES(?1,'associate','replacement','cli:raw',202)",
                params![proposal.candidate_id],
            )
            .is_err());
        assert!(fixture
            .ledger
            .conn()
            .execute(
                "DELETE FROM entity_association_decisions WHERE candidate_id=?1",
                params![proposal.candidate_id],
            )
            .is_err());
        assert!(fixture
            .ledger
            .conn()
            .execute(
                "INSERT OR REPLACE INTO entity_association_proposals(
                     candidate_id,project_id,first_event_id,first_span_start,first_span_end,
                     second_event_id,second_span_start,second_span_end,label,proposer,
                     proposer_operator,annotation_id,proposed_at_unix_ms)
                 VALUES(?1,?2,?3,NULL,NULL,?4,NULL,NULL,'Other label',
                        'operator','replacement',NULL,101)",
                params![
                    proposal.candidate_id,
                    proposal.project_id,
                    proposal.first.event_id,
                    proposal.second.event_id
                ],
            )
            .is_err());
        assert!(fixture
            .ledger
            .conn()
            .execute(
                "INSERT INTO entity_association_proposals(
                     candidate_id,project_id,first_event_id,first_span_start,first_span_end,
                     second_event_id,second_span_start,second_span_end,label,proposer,
                     proposer_operator,annotation_id,proposed_at_unix_ms)
                 VALUES('partial-span',?1,?2,1,NULL,?3,NULL,NULL,'Parser family',
                        'operator','reviewer',NULL,102)",
                params![
                    proposal.project_id,
                    proposal.first.event_id,
                    proposal.second.event_id
                ],
            )
            .is_err());
        assert!(fixture
            .ledger
            .conn()
            .execute(
                "INSERT INTO entity_association_proposals(
                     candidate_id,project_id,first_event_id,first_span_start,first_span_end,
                     second_event_id,second_span_start,second_span_end,label,proposer,
                     proposer_operator,annotation_id,proposed_at_unix_ms)
                 VALUES('operator-with-annotation',?1,?2,NULL,NULL,?3,NULL,NULL,
                        'Parser family','operator','reviewer','missing',103)",
                params![
                    proposal.project_id,
                    proposal.first.event_id,
                    proposal.second.event_id
                ],
            )
            .is_err());
    }

    #[test]
    fn stored_negative_decision_timestamp_fails_closed_at_the_read_boundary() {
        let fixture = fixture();
        let proposal =
            prepare(&fixture.ledger, operator_input(&fixture.a1, &fixture.a2)).expect("proposal");
        record_proposal(&fixture.ledger, &proposal).expect("record");
        fixture
            .ledger
            .conn()
            .execute_batch("PRAGMA ignore_check_constraints=ON;")
            .expect("simulate corrupted storage");
        fixture
            .ledger
            .conn()
            .execute(
                "INSERT INTO entity_association_decisions(
                     candidate_id,decision,operator,route,decided_at_unix_ms)
                 VALUES(?1,'associate','reviewer','cli:raw',-1)",
                params![proposal.candidate_id],
            )
            .expect("insert corrupt decision timestamp");
        assert!(matches!(
            decision_for(&fixture.ledger, &proposal.candidate_id),
            Err(CoreError::Integrity(message))
                if message == "stored entity-association decision timestamp is negative"
        ));
        assert!(accepted(&fixture.ledger, Some(PROJECT_A)).is_err());
    }

    #[test]
    fn association_rows_never_mutate_event_receipts_or_project_heads() {
        let fixture = fixture();
        let before_head = fixture
            .ledger
            .compute_project_head(PROJECT_A)
            .expect("head before");
        let before_events: Vec<(String, String, String)> = {
            let mut stmt = fixture
                .ledger
                .conn()
                .prepare(
                    "SELECT id, raw_sha256, chain_hash FROM events
                      WHERE project_id=?1 ORDER BY id",
                )
                .expect("prepare");
            stmt.query_map(params![PROJECT_A], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .expect("query")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("events")
        };
        let proposal =
            prepare(&fixture.ledger, operator_input(&fixture.a1, &fixture.a2)).expect("proposal");
        record_proposal(&fixture.ledger, &proposal).expect("record");
        adjudicate(
            &fixture.ledger,
            &proposal.candidate_id,
            AssociationDecision::Associate,
            "human-reviewer",
            "cli:entity",
            300,
        )
        .expect("associate");
        assert_eq!(
            accepted(&fixture.ledger, Some(PROJECT_A))
                .expect("accepted")
                .len(),
            1
        );
        let after_head = fixture
            .ledger
            .compute_project_head(PROJECT_A)
            .expect("head after");
        let after_events: Vec<(String, String, String)> = {
            let mut stmt = fixture
                .ledger
                .conn()
                .prepare(
                    "SELECT id, raw_sha256, chain_hash FROM events
                      WHERE project_id=?1 ORDER BY id",
                )
                .expect("prepare");
            stmt.query_map(params![PROJECT_A], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .expect("query")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("events")
        };
        assert_eq!(before_head, after_head);
        assert_eq!(before_events, after_events);
    }

    #[test]
    fn unsafe_labels_and_raw_replacement_are_refused() {
        let fixture = fixture();
        let mut unsafe_input = operator_input(&fixture.a1, &fixture.a2);
        unsafe_input.label = "Build finished".to_string();
        assert!(prepare(&fixture.ledger, unsafe_input).is_err());

        let proposal =
            prepare(&fixture.ledger, operator_input(&fixture.a1, &fixture.a2)).expect("proposal");
        record_proposal(&fixture.ledger, &proposal).expect("record");
        assert!(fixture
            .ledger
            .conn()
            .execute(
                "UPDATE entity_association_proposals SET label='Other label' WHERE candidate_id=?1",
                params![proposal.candidate_id],
            )
            .is_err());
        assert!(fixture
            .ledger
            .conn()
            .execute(
                "DELETE FROM entity_association_proposals WHERE candidate_id=?1",
                params![proposal.candidate_id],
            )
            .is_err());
    }

    #[test]
    fn read_projection_recomputes_the_content_address() {
        let fixture = fixture();
        let proposal =
            prepare(&fixture.ledger, operator_input(&fixture.a1, &fixture.a2)).expect("proposal");
        fixture
            .ledger
            .conn()
            .execute(
                "INSERT INTO entity_association_proposals(
                     candidate_id,project_id,first_event_id,first_span_start,first_span_end,
                     second_event_id,second_span_start,second_span_end,label,proposer,
                     proposer_operator,annotation_id,proposed_at_unix_ms)
                 VALUES('entity_assoc_forged',?1,?2,NULL,NULL,?3,NULL,NULL,?4,
                        'operator','reviewer',NULL,100)",
                params![
                    proposal.project_id,
                    proposal.first.event_id,
                    proposal.second.event_id,
                    proposal.label
                ],
            )
            .expect("raw forged candidate");
        let error = list(&fixture.ledger, Some(PROJECT_A)).expect_err("forged id rejected");
        assert!(error.to_string().contains("content address"), "{error}");
    }
}
