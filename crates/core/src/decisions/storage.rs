//! Append-only storage for project-scoped standing decisions.
//!
//! A proposal is the person's source span and closed check parameters. Only a
//! person act can start enforcement. Verdict rows describe one exact tree and
//! are never replaced; `stale` is a read-time projection over later edits.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ulid::Ulid;

use crate::chain::sha256_hex;
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::redact::Masked;

use super::authority::{self, PersonActKind};
use super::params::{parse_params, CheckParams, PARAMS_SCHEMA_VERSION};

pub const DERIVED_SCHEMA_VERSION: u32 = 1;
pub const DERIVED_FILE: &str = ".thruline/decisions.yml";

/// Stable project-scoped identity for one source span and closed check kind.
/// Source anchors in different projects may be the same file and line, while
/// `decisions.id` is a ledger-wide primary key.
pub fn id_for_source(project_id: &str, source_anchor: &str, kind: &str) -> String {
    sha256_hex(format!("{project_id}\0{source_anchor}\0{kind}").as_bytes())
        .chars()
        .take(16)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    WorkPrompt,
    RuleFile,
    Criterion,
    MarkedFinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionSource {
    pub source_kind: SourceKind,
    pub event_id: Option<String>,
    pub file_anchor: Option<String>,
    pub raw_file: String,
    pub raw_line: u64,
    pub span_start: u64,
    pub span_end: u64,
    pub span: String,
    pub actor: Option<String>,
    pub recorded_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewDecision {
    pub project_id: String,
    /// First source's event id or file anchor. This is an identity, not text.
    pub source_anchor: String,
    pub span: String,
    pub sources: Vec<DecisionSource>,
    pub author_actor: String,
    pub recorded_at: String,
    pub params: CheckParams,
}

#[derive(Debug, Clone)]
pub struct RevisionInput {
    pub decision_id: String,
    pub span: String,
    pub sources: Vec<DecisionSource>,
    pub author_actor: String,
    pub recorded_at: String,
    pub params: CheckParams,
}

#[derive(Debug, Clone)]
pub struct AcceptanceInput {
    pub decision_id: String,
    pub revision: u32,
    pub actor: String,
    pub at: String,
    pub because: String,
    pub severity: String,
    pub applies_from_version: String,
}

#[derive(Debug, Clone)]
pub struct RetirementInput {
    pub decision_id: String,
    pub actor: String,
    pub at: String,
    pub because: String,
    pub superseded_by: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WaiverInput {
    pub decision_id: String,
    pub version_id: String,
    pub actor: String,
    pub at: String,
    pub because: String,
    pub expires_version: Option<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VerdictInput {
    pub decision_id: String,
    pub revision: u32,
    pub version_id: String,
    pub tree_hash: String,
    pub verdict: String,
    pub reason: Option<String>,
    pub checkable_if: Option<String>,
    pub findings: Value,
    pub surfaces: Value,
    pub check_impl_version: String,
    pub tool_versions: Value,
    pub computed_at: String,
    pub basis: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct AcceptedDecision {
    pub id: String,
    pub revision: u32,
    pub kind: String,
    pub params_schema_version: u32,
    pub params: Value,
    pub span: String,
    pub sources: Value,
    pub severity: String,
    pub accepted_by: String,
    pub accepted_at: String,
    pub acceptance_reason: String,
    pub applies_from_version: String,
}

#[derive(Debug, Clone)]
pub struct VerifiedRetirement {
    pub superseded_by: Option<String>,
}

#[derive(Serialize)]
struct DerivedCopy<'a> {
    schema_version: u32,
    derived_copy: bool,
    project_id: &'a str,
    member_project_ids: &'a [String],
    content_sha256: String,
    decisions: &'a [AcceptedDecision],
}

/// Append a new decision and its first revision in one transaction.
pub fn create(ledger: &Ledger, input: NewDecision) -> Result<String> {
    nonempty("project_id", &input.project_id)?;
    nonempty("source_anchor", &input.source_anchor)?;
    let first = input
        .sources
        .first()
        .ok_or_else(|| CoreError::Config("a decision needs a person's source".into()))?;
    if first.event_id.as_deref() != Some(input.source_anchor.as_str())
        && first.file_anchor.as_deref() != Some(input.source_anchor.as_str())
    {
        return Err(CoreError::Config(
            "source_anchor must name the first source event or file anchor".into(),
        ));
    }
    validate_revision(
        &input.span,
        &input.sources,
        &input.author_actor,
        &input.recorded_at,
        &input.params,
    )?;
    let params_json = checked_params_json(&input.params)?;
    let sources_json = masked_sources_json(&input.sources)?;
    let kind = input.params.kind();
    let id = id_for_source(&input.project_id, &input.source_anchor, kind);
    let tx = ledger.conn().unchecked_transaction()?;
    tx.execute(
        "INSERT INTO decisions(id,project_id,created_at) VALUES(?1,?2,?3)",
        params![id, input.project_id, input.recorded_at],
    )?;
    tx.execute(
        "INSERT INTO decision_revisions(decision_id,rev,kind,params_schema_version,params_json,text,sources_json,author_actor,recorded_at)
         VALUES(?1,1,?2,?3,?4,?5,?6,?7,?8)",
        params![id, kind, PARAMS_SCHEMA_VERSION, params_json, Masked::mask(&input.span).text(),
            sources_json, Masked::mask(&input.author_actor).text(), input.recorded_at],
    )?;
    tx.commit()?;
    Ok(id)
}

/// Append a proposed revision. A standing acceptance still names its old rev.
pub fn append_revision(ledger: &Ledger, input: RevisionInput) -> Result<u32> {
    append_revision_guarded(ledger, input, None)
}

pub struct RevisionParent<'a> {
    pub revision: u32,
    pub params_sha256: &'a str,
    pub sources_sha256: &'a str,
}

/// Compare the parent under the same transaction as the append. A stale
/// proposal never silently revises somebody else's newer parameter document.
pub fn append_revision_if_parent(
    ledger: &Ledger,
    input: RevisionInput,
    parent: RevisionParent<'_>,
) -> Result<u32> {
    append_revision_guarded(ledger, input, Some(parent))
}

fn append_revision_guarded(
    ledger: &Ledger,
    input: RevisionInput,
    parent: Option<RevisionParent<'_>>,
) -> Result<u32> {
    nonempty("decision_id", &input.decision_id)?;
    validate_revision(
        &input.span,
        &input.sources,
        &input.author_actor,
        &input.recorded_at,
        &input.params,
    )?;
    let params_json = checked_params_json(&input.params)?;
    let sources_json = masked_sources_json(&input.sources)?;
    let tx = ledger.conn().unchecked_transaction()?;
    let next: u32 = tx.query_row(
        "SELECT COALESCE(MAX(rev),0)+1 FROM decision_revisions WHERE decision_id=?1",
        [&input.decision_id],
        |row| row.get(0),
    )?;
    if let Some(parent) = parent {
        let (params, sources): (String, String) = tx.query_row(
            "SELECT params_json,sources_json FROM decision_revisions WHERE decision_id=?1 AND rev=?2",
            params![input.decision_id, parent.revision], |row| Ok((row.get(0)?,row.get(1)?)),
        )?;
        if parent.revision.checked_add(1) != Some(next)
            || sha256_hex(params.as_bytes()) != parent.params_sha256
            || sha256_hex(sources.as_bytes()) != parent.sources_sha256
            || sources != sources_json
        {
            return Err(CoreError::Config(
                "revision parent changed; review the current proposal before appending".into(),
            ));
        }
    }
    tx.execute(
        "INSERT INTO decision_revisions(decision_id,rev,kind,params_schema_version,params_json,text,sources_json,author_actor,recorded_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![input.decision_id, next, input.params.kind(), PARAMS_SCHEMA_VERSION,
            params_json, Masked::mask(&input.span).text(), sources_json,
            Masked::mask(&input.author_actor).text(), input.recorded_at],
    )?;
    tx.commit()?;
    Ok(next)
}

/// A person accepts one exact revision; a proposal alone enforces nothing.
pub fn accept(ledger: &Ledger, input: AcceptanceInput) -> Result<String> {
    nonempty("actor", &input.actor)?;
    nonempty("because", &input.because)?;
    nonempty("applies_from_version", &input.applies_from_version)?;
    nonempty("accepted_at", &input.at)?;
    if !matches!(input.severity.as_str(), "blocks" | "warns") {
        return Err(CoreError::Config("severity must be blocks or warns".into()));
    }
    let retired = verified_retirement_for_decision(ledger, &input.decision_id)?.is_some();
    let tx = ledger.conn().unchecked_transaction()?;
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM decision_revisions WHERE decision_id=?1 AND rev=?2)",
        params![input.decision_id, input.revision],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(CoreError::Config("decision revision does not exist".into()));
    }
    let actor = Masked::mask(&input.actor).into_text();
    let because = Masked::mask(&input.because).into_text();
    let existing: Option<(String, String, String, String, String)> = tx
        .query_row(
            "SELECT id,actor,rationale,severity,applies_from_version
             FROM decision_acceptances WHERE decision_id=?1 AND rev=?2
             ORDER BY rowid DESC LIMIT 1",
            params![input.decision_id, input.revision],
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
    if let Some((id, old_actor, old_because, old_severity, old_version)) = existing {
        if old_actor == actor
            && old_because == because
            && old_severity == input.severity
            && old_version == input.applies_from_version
        {
            tx.commit()?;
            return Ok(id);
        }
        return Err(CoreError::Config(
            "revision was already accepted differently".into(),
        ));
    }
    if retired {
        return Err(CoreError::Config("decision is retired".into()));
    }
    let id = Ulid::generate().to_string();
    tx.execute(
        "INSERT INTO decision_acceptances(id,decision_id,rev,actor,accepted_at,rationale,severity,applies_from_version)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![id, input.decision_id, input.revision, actor,
            input.at, because, input.severity, input.applies_from_version],
    )?;
    tx.commit()?;
    Ok(id)
}

pub fn retire(ledger: &Ledger, input: RetirementInput) -> Result<String> {
    nonempty("actor", &input.actor)?;
    nonempty("because", &input.because)?;
    nonempty("retired_at", &input.at)?;
    if input.superseded_by.as_deref() == Some(input.decision_id.as_str()) {
        return Err(CoreError::Config(
            "a decision cannot supersede itself".into(),
        ));
    }
    if let Some(target) = input.superseded_by.as_deref() {
        if !supersession_valid(ledger, &input.decision_id, target, None, false)? {
            return Err(CoreError::Config(
                "supersession needs a later verified acceptance in the same project".into(),
            ));
        }
        let mut rows = ledger
            .conn()
            .prepare("SELECT id FROM decision_retirements WHERE decision_id=?1")?;
        for row in rows.query_map([target], |r| r.get::<_, String>(0))? {
            if receipt_verified(ledger, PersonActKind::Retire, &row?, None)? {
                return Err(CoreError::Config(
                    "superseding decision is already retired".into(),
                ));
            }
        }
    }
    let tx = ledger.conn().unchecked_transaction()?;
    require_accepted(&tx, &input.decision_id, None)?;
    let actor = Masked::mask(&input.actor).into_text();
    let because = Masked::mask(&input.because).into_text();
    let existing: Option<(String, String, String, Option<String>)> = tx
        .query_row(
            "SELECT id,actor,reason,superseded_by FROM decision_retirements
             WHERE decision_id=?1 ORDER BY rowid DESC LIMIT 1",
            [&input.decision_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    if let Some((id, old_actor, old_because, old_superseded_by)) = existing {
        if old_actor == actor && old_because == because && old_superseded_by == input.superseded_by
        {
            tx.commit()?;
            return Ok(id);
        }
        return Err(CoreError::Config(
            "decision is already retired differently".into(),
        ));
    }
    let id = Ulid::generate().to_string();
    tx.execute(
        "INSERT INTO decision_retirements(id,decision_id,actor,retired_at,reason,superseded_by)
         VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            id,
            input.decision_id,
            actor,
            input.at,
            because,
            input.superseded_by
        ],
    )?;
    tx.commit()?;
    Ok(id)
}

pub fn waive(ledger: &Ledger, input: WaiverInput) -> Result<String> {
    nonempty("actor", &input.actor)?;
    nonempty("because", &input.because)?;
    nonempty("version_id", &input.version_id)?;
    nonempty("waived_at", &input.at)?;
    require_accepted(ledger.conn(), &input.decision_id, None)?;
    let id = Ulid::generate().to_string();
    ledger.conn().execute(
        "INSERT INTO decision_waivers(id,decision_id,version_id,actor,waived_at,reason,expires_version,expires_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![id, input.decision_id, input.version_id, Masked::mask(&input.actor).text(),
            input.at, Masked::mask(&input.because).text(), input.expires_version, input.expires_at],
    )?;
    Ok(id)
}

pub fn record_verdict(ledger: &Ledger, input: VerdictInput) -> Result<String> {
    nonempty("version_id", &input.version_id)?;
    nonempty("tree_hash", &input.tree_hash)?;
    nonempty("check_impl_version", &input.check_impl_version)?;
    nonempty("computed_at", &input.computed_at)?;
    if !matches!(
        input.verdict.as_str(),
        "holds" | "broken" | "unverifiable" | "not_applicable"
    ) {
        return Err(CoreError::Config("unknown decision verdict".into()));
    }
    if input.verdict == "unverifiable"
        && (input.reason.as_deref().is_none_or(str::is_empty)
            || input.checkable_if.as_deref().is_none_or(str::is_empty))
    {
        return Err(CoreError::Config(
            "unverifiable needs reason and checkable_if".into(),
        ));
    }
    let active = verified_acceptance_for_decision(ledger, &input.decision_id)?;
    if active.as_ref().map(|accepted| accepted.revision) != Some(input.revision)
        || verified_retirement_for_decision(ledger, &input.decision_id)?.is_some()
    {
        return Err(CoreError::Config(
            "decision has no active recorded person acceptance for this revision".into(),
        ));
    }
    let id = Ulid::generate().to_string();
    ledger.conn().execute(
        "INSERT INTO decision_verdicts(id,decision_id,rev,version_id,tree_hash,verdict,reason,checkable_if,findings_json,surfaces_json,check_impl_version,tool_versions_json,computed_at,basis_json)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
        params![id, input.decision_id, input.revision, input.version_id, input.tree_hash,
            input.verdict, input.reason.as_deref().map(|s| Masked::mask(s).into_text()),
            input.checkable_if.as_deref().map(|s| Masked::mask(s).into_text()),
            serde_json::to_string(&mask_json(&input.findings))?,
            serde_json::to_string(&mask_json(&input.surfaces))?, input.check_impl_version,
            serde_json::to_string(&input.tool_versions)?, input.computed_at,
            serde_json::to_string(&mask_json(&input.basis))?],
    )?;
    Ok(id)
}

pub fn accepted_for_project(ledger: &Ledger, project_id: &str) -> Result<Vec<AcceptedDecision>> {
    let mut statement = ledger
        .conn()
        .prepare("SELECT id FROM decisions WHERE project_id=?1 ORDER BY id")?;
    let ids = statement
        .query_map([project_id], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut decisions = Vec::new();
    for id in ids {
        if verified_retirement_for_decision(ledger, &id)?.is_some() {
            continue;
        }
        if let Some(accepted) = verified_acceptance_for_decision(ledger, &id)? {
            decisions.push(accepted);
        }
    }
    Ok(decisions)
}

/// The latest acceptance backed by its exact recorded person confirmation.
/// A later pending row cannot replace an earlier verified revision.
pub fn verified_acceptance_for_decision(
    ledger: &Ledger,
    decision_id: &str,
) -> Result<Option<AcceptedDecision>> {
    verified_acceptance_until(ledger, decision_id, None)
}

/// Budgeted person-receipt selection for advisory readers. Expired evidence is inert.
pub fn verified_acceptance_for_decision_read_only(
    ledger: &Ledger,
    decision_id: &str,
    budget: Duration,
) -> Result<Option<AcceptedDecision>> {
    verified_acceptance_until(ledger, decision_id, Some(Instant::now() + budget))
}

fn verified_acceptance_until(
    ledger: &Ledger,
    decision_id: &str,
    deadline: Option<Instant>,
) -> Result<Option<AcceptedDecision>> {
    let mut statement = ledger.conn().prepare(
        "SELECT a.id,d.id,r.rev,r.kind,r.params_schema_version,r.params_json,r.text,
                r.sources_json,a.severity,a.actor,a.accepted_at,a.rationale,
                a.applies_from_version
         FROM decisions d
         JOIN decision_acceptances a ON a.decision_id=d.id
         JOIN decision_revisions r ON r.decision_id=a.decision_id AND r.rev=a.rev
         WHERE d.id=?1 ORDER BY a.rowid DESC",
    )?;
    let rows = statement.query_map([decision_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, u32>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, u32>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, String>(10)?,
            row.get::<_, String>(11)?,
            row.get::<_, String>(12)?,
        ))
    })?;
    for row in rows {
        let (
            act_id,
            id,
            revision,
            kind,
            params_schema_version,
            params_json,
            span,
            sources_json,
            severity,
            accepted_by,
            accepted_at,
            acceptance_reason,
            applies_from_version,
        ) = row?;
        if !receipt_verified(ledger, PersonActKind::Accept, &act_id, deadline)? {
            continue;
        }
        parse_params(&kind, params_schema_version, &params_json)?;
        return Ok(Some(AcceptedDecision {
            id,
            revision,
            kind,
            params_schema_version,
            params: mask_json(&serde_json::from_str(&params_json)?),
            span,
            sources: serde_json::from_str(&sources_json)?,
            severity,
            accepted_by,
            accepted_at,
            acceptance_reason,
            applies_from_version,
        }));
    }
    Ok(None)
}

/// A retirement is active only while its exact recorded confirmation verifies.
pub fn verified_retirement_for_decision(
    ledger: &Ledger,
    decision_id: &str,
) -> Result<Option<VerifiedRetirement>> {
    verified_retirement_until(ledger, decision_id, None)
}

fn verified_retirement_until(
    ledger: &Ledger,
    decision_id: &str,
    deadline: Option<Instant>,
) -> Result<Option<VerifiedRetirement>> {
    let mut statement = ledger.conn().prepare(
        "SELECT id,superseded_by FROM decision_retirements
         WHERE decision_id=?1 ORDER BY rowid DESC",
    )?;
    let rows = statement.query_map([decision_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
    })?;
    for row in rows {
        let (act_id, superseded_by) = row?;
        if receipt_verified(ledger, PersonActKind::Retire, &act_id, deadline)? {
            if let Some(target) = superseded_by.as_deref() {
                let exact =
                    authority::bridge::has_exact_general_supersession(ledger.conn(), &act_id)?;
                if !supersession_valid(ledger, decision_id, target, deadline, !exact)?
                    || (!exact
                        && !legacy_supersession_predates_receipt(
                            ledger,
                            decision_id,
                            target,
                            &act_id,
                            deadline,
                        )?)
                {
                    continue;
                }
            }
            return Ok(Some(VerifiedRetirement { superseded_by }));
        }
    }
    Ok(None)
}

/// Budgeted advisory lifecycle read. Missing or expired evidence is inert.
pub fn verified_retirement_for_decision_read_only(
    ledger: &Ledger,
    decision_id: &str,
    budget: Duration,
) -> Result<Option<VerifiedRetirement>> {
    verified_retirement_until(ledger, decision_id, Some(Instant::now() + budget))
}

fn receipt_verified(
    ledger: &Ledger,
    action: PersonActKind,
    act_id: &str,
    deadline: Option<Instant>,
) -> Result<bool> {
    match deadline {
        None => authority::bridge::verified(ledger, action, act_id),
        Some(until) => {
            let now = Instant::now();
            if now >= until {
                return Ok(false);
            }
            authority::bridge::verified_read_only(ledger.conn(), action, act_id, until - now)
        }
    }
}

fn supersession_valid(
    ledger: &Ledger,
    predecessor: &str,
    successor: &str,
    deadline: Option<Instant>,
    legacy_single_target: bool,
) -> Result<bool> {
    if predecessor == successor || deadline.is_some_and(|until| Instant::now() >= until) {
        return Ok(false);
    }
    let same: bool = ledger.conn().query_row("SELECT EXISTS(SELECT 1 FROM decisions a JOIN decisions b ON a.project_id=b.project_id WHERE a.id=?1 AND b.id=?2)", params![predecessor, successor], |r| r.get(0))?;
    if !same {
        return Ok(false);
    }
    let Some(old) = verified_acceptance_until(ledger, predecessor, deadline)? else {
        return Ok(false);
    };
    let Some(new) = verified_acceptance_until(ledger, successor, deadline)? else {
        return Ok(false);
    };
    // The existing retirement tuple names a decision ID, not a successor
    // revision or params digest. Multiple verified revisions cannot inherit
    // that older act. Keep the predecessor active until an exact tuple exists.
    if legacy_single_target {
        let mut other_revisions = ledger.conn().prepare(
            "SELECT id FROM decision_acceptances WHERE decision_id=?1 AND rev<>?2
         ORDER BY rowid DESC LIMIT 257",
        )?;
        let others = other_revisions
            .query_map(params![successor, new.revision], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if others.len() > 256 {
            return Ok(false);
        }
        for act in others {
            if deadline.is_some_and(|until| Instant::now() >= until)
                || receipt_verified(ledger, PersonActKind::Accept, &act, deadline)?
            {
                return Ok(false);
            }
        }
    }
    let old_order = acceptance_order(ledger, &old.id, old.revision, deadline)?;
    let new_order = acceptance_order(ledger, &new.id, new.revision, deadline)?;
    Ok(
        matches!((old_order, new_order), (Some(old), Some(new)) if new > old)
            && deadline.is_none_or(|until| Instant::now() < until),
    )
}

// A legacy ID-only act cannot adopt an acceptance whose terminal receipt
// landed after its own receipt, even if the original target proof disappeared.
fn legacy_supersession_predates_receipt(
    ledger: &Ledger,
    predecessor: &str,
    successor: &str,
    retirement: &str,
    deadline: Option<Instant>,
) -> Result<bool> {
    let retirement_order: i64 = ledger.conn().query_row(
        "SELECT b.rowid FROM decision_bridge_proofs b JOIN decision_person_acts p ON p.id=b.receipt_id WHERE p.action='retire' AND p.act_row_id=?1",
        [retirement], |r| r.get(0),
    )?;
    for decision in [predecessor, successor] {
        let Some(accepted) = verified_acceptance_until(ledger, decision, deadline)? else {
            return Ok(false);
        };
        let Some(order) = acceptance_order(ledger, decision, accepted.revision, deadline)? else {
            return Ok(false);
        };
        let proof_order: Option<i64> = ledger.conn().query_row(
            "SELECT b.rowid FROM decision_acceptances a JOIN decision_person_acts p ON p.action='accept' AND p.act_row_id=a.id JOIN decision_bridge_proofs b ON b.receipt_id=p.id WHERE a.rowid=?1",
            [order], |r| r.get(0),
        ).optional()?;
        if !proof_order.is_some_and(|value| value < retirement_order) {
            return Ok(false);
        }
    }
    Ok(deadline.is_none_or(|until| Instant::now() < until))
}

pub(crate) fn acceptance_order(
    ledger: &Ledger,
    decision_id: &str,
    revision: u32,
    deadline: Option<Instant>,
) -> Result<Option<i64>> {
    let mut query = ledger.conn().prepare("SELECT id,rowid FROM decision_acceptances WHERE decision_id=?1 AND rev=?2 ORDER BY rowid DESC")?;
    for row in query.query_map(params![decision_id, revision], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
    })? {
        let (id, order) = row?;
        if receipt_verified(ledger, PersonActKind::Accept, &id, deadline)? {
            return Ok(Some(order));
        }
        if deadline.is_some_and(|until| Instant::now() >= until) {
            return Ok(None);
        }
    }
    Ok(None)
}

/// A pending blocking acceptance is an explicit gate refusal, even if an old
/// verified revision remains active. This is scoped to each decision's latest
/// attempted acceptance so superseded pending rows do not block forever.
pub fn unverified_blocking_acceptances_for_project(
    ledger: &Ledger,
    project_id: &str,
) -> Result<Vec<String>> {
    let mut statement = ledger.conn().prepare(
        "SELECT d.id,a.id FROM decisions d
         JOIN decision_acceptances a ON a.decision_id=d.id
         WHERE d.project_id=?1 AND a.severity='blocks'
           AND a.rowid=(SELECT max(a2.rowid) FROM decision_acceptances a2
                        WHERE a2.decision_id=d.id)
         ORDER BY d.id",
    )?;
    let rows = statement
        .query_map([project_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut pending = Vec::new();
    for (decision_id, act_id) in rows {
        if verified_retirement_for_decision(ledger, &decision_id)?.is_none()
            && !authority::bridge::verified(ledger, PersonActKind::Accept, &act_id)?
        {
            pending.push(decision_id);
        }
    }
    Ok(pending)
}

/// Write a whole-file derived copy of accepted rules. JSON is a YAML 1.2
/// document; the `.yml` path is kept for the repository policy convention.
pub fn write_derived_file(
    ledger: &Ledger,
    project_id: &str,
    project_root: &Path,
) -> Result<PathBuf> {
    write_derived_file_for_projects(ledger, project_id, &[project_id.into()], project_root)
}

/// One HQ root can group several ledger projects at the same repository root.
/// Its derived copy must include every accepted member decision in one file.
pub fn write_derived_file_for_projects(
    ledger: &Ledger,
    group_id: &str,
    member_project_ids: &[String],
    project_root: &Path,
) -> Result<PathBuf> {
    let path = project_root.join(DERIVED_FILE);
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::Config("derived decision path has no parent".into()))?;
    if parent.exists() {
        refuse_symlink(parent)?;
    } else {
        fs::create_dir(parent).map_err(|error| CoreError::io(parent, error))?;
    }
    if path.exists() {
        refuse_symlink(&path)?;
    }
    let bytes = derived_bytes_for_projects(ledger, group_id, member_project_ids)?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|error| CoreError::io(parent, error))?;
    use std::io::Write;
    temporary
        .write_all(&bytes)
        .map_err(|error| CoreError::io(temporary.path(), error))?;
    temporary
        .persist(&path)
        .map_err(|error| CoreError::io(&path, error.error))?;
    Ok(path)
}

/// A hand edit is detected against the ledger and never imported.
pub fn verify_derived_file(ledger: &Ledger, project_id: &str, project_root: &Path) -> Result<bool> {
    verify_derived_file_for_projects(ledger, project_id, &[project_id.into()], project_root)
}

pub fn verify_derived_file_for_projects(
    ledger: &Ledger,
    group_id: &str,
    member_project_ids: &[String],
    project_root: &Path,
) -> Result<bool> {
    let path = project_root.join(DERIVED_FILE);
    if !path.exists() {
        return Ok(false);
    }
    refuse_symlink(&path)?;
    let found = fs::read(&path).map_err(|error| CoreError::io(&path, error))?;
    Ok(found == derived_bytes_for_projects(ledger, group_id, member_project_ids)?)
}

fn derived_bytes_for_projects(
    ledger: &Ledger,
    group_id: &str,
    member_project_ids: &[String],
) -> Result<Vec<u8>> {
    nonempty("group_id", group_id)?;
    if member_project_ids.is_empty() {
        return Err(CoreError::Config(
            "derived copy needs member projects".into(),
        ));
    }
    let mut members = member_project_ids.to_vec();
    members.sort();
    members.dedup();
    let mut decisions = Vec::new();
    for project_id in &members {
        nonempty("member project id", project_id)?;
        let exists: bool = ledger.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
            [project_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(CoreError::Config(format!(
                "derived copy member project {project_id} is absent"
            )));
        }
        decisions.extend(accepted_for_project(ledger, project_id)?);
    }
    decisions.sort_by(|a, b| a.id.cmp(&b.id));
    let content = serde_json::to_vec(&(&members, &decisions))?;
    let document = DerivedCopy {
        schema_version: DERIVED_SCHEMA_VERSION,
        derived_copy: true,
        project_id: group_id,
        member_project_ids: &members,
        content_sha256: sha256_hex(&content),
        decisions: &decisions,
    };
    let mut bytes = serde_json::to_vec_pretty(&document)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn require_accepted(
    conn: &rusqlite::Connection,
    decision_id: &str,
    rev: Option<u32>,
) -> Result<()> {
    let accepted: bool = if let Some(rev) = rev {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM decision_acceptances WHERE decision_id=?1 AND rev=?2)",
            params![decision_id, rev],
            |row| row.get(0),
        )?
    } else {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM decision_acceptances WHERE decision_id=?1)",
            [decision_id],
            |row| row.get(0),
        )?
    };
    if !accepted {
        return Err(CoreError::Config(
            "decision has no person acceptance".into(),
        ));
    }
    Ok(())
}

fn validate_revision(
    span: &str,
    sources: &[DecisionSource],
    actor: &str,
    at: &str,
    params: &CheckParams,
) -> Result<()> {
    nonempty("span", span)?;
    nonempty("author_actor", actor)?;
    nonempty("recorded_at", at)?;
    if sources.is_empty() || !sources.iter().any(|source| source.span == span) {
        return Err(CoreError::Config(
            "decision span must be in a person's source".into(),
        ));
    }
    for source in sources {
        if source.raw_line == 0
            || source.span_end <= source.span_start
            || (source.event_id.is_none() && source.file_anchor.is_none())
        {
            return Err(CoreError::Config(
                "source needs a line, span and anchor".into(),
            ));
        }
    }
    checked_params_json(params)?;
    Ok(())
}

fn checked_params_json(params: &CheckParams) -> Result<String> {
    let json = params.json()?;
    parse_params(params.kind(), PARAMS_SCHEMA_VERSION, &json)?;
    Ok(json)
}

fn masked_sources_json(sources: &[DecisionSource]) -> Result<String> {
    let masked: Vec<DecisionSource> = sources
        .iter()
        .cloned()
        .map(|mut source| {
            source.span = Masked::mask(&source.span).into_text();
            source.raw_file = Masked::mask(&source.raw_file).into_text();
            source.actor = source.actor.map(|actor| Masked::mask(&actor).into_text());
            source
        })
        .collect();
    Ok(serde_json::to_string(&masked)?)
}

fn mask_json(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(Masked::mask(text).into_text()),
        Value::Array(items) => Value::Array(items.iter().map(mask_json).collect()),
        Value::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), mask_json(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn nonempty(field: &str, text: &str) -> Result<()> {
    if text.trim().is_empty() {
        return Err(CoreError::Config(format!("{field} must be nonempty")));
    }
    Ok(())
}

fn refuse_symlink(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|error| CoreError::io(path, error))?;
    if metadata.file_type().is_symlink() {
        return Err(CoreError::Config(format!(
            "refusing symlink at {}",
            path.display()
        )));
    }
    Ok(())
}
