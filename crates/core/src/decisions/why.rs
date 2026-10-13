//! Read-only six-step proof for an accepted decision on one exact tree.
//!
//! Every sentence is assembled from accepted revision, source and verdict
//! rows. A missing receipt stays missing. A stored verdict on another tree is
//! shown as moved; this module never rewrites its historical basis state.

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::Value;

use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::redact::Masked;

use crate::decisions::authority::{self, PersonActKind};
use crate::decisions::params::parse_params;
use crate::decisions::storage::DecisionSource;

pub const PROOF_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ChainLabel {
    #[serde(rename = "DECIDED")]
    Decided,
    #[serde(rename = "CHECK")]
    Check,
    #[serde(rename = "RAN ON")]
    RanOn,
    #[serde(rename = "FOUND")]
    Found,
    #[serde(rename = "SINCE")]
    Since,
    #[serde(rename = "SO")]
    So,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkState {
    Holds,
    Missing,
    Moved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChainStep {
    pub number: u8,
    pub label: ChainLabel,
    pub state: LinkState,
    pub sentence: String,
    pub source_ids: Vec<String>,
    pub finding_ids: Vec<String>,
    pub version_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DecisionProof {
    pub schema_version: u32,
    pub decision_id: String,
    pub revision: u32,
    pub verdict_id: Option<String>,
    pub selected_version_id: String,
    pub selected_tree_hash: String,
    pub basis_state: Option<String>,
    pub display_state: Option<String>,
    pub not_looked_at: Vec<super::text_exclusions::NotLookedAt>,
    pub chain: Vec<ChainStep>,
}

struct AcceptedRow {
    revision: u32,
    actor: String,
    accepted_at: String,
    kind: String,
    params_schema_version: u32,
    params_json: String,
    span: String,
    sources_json: String,
}

struct VerdictRow {
    rowid: i64,
    id: String,
    version_id: String,
    tree_hash: String,
    state: String,
    reason: Option<String>,
    checkable_if: Option<String>,
    findings_json: String,
    tool_versions_json: String,
    basis_json: String,
}

/// Resolve an exact decision or verdict id to the accepted rule and its proof.
/// A proposal has no proof chain. `version_id` and `tree_hash` are supplied by
/// the caller's exact version resolver; this read never invokes a checker.
pub fn proof_chain(
    ledger: &Ledger,
    decision_or_verdict_id: &str,
    version_id: &str,
    tree_hash: &str,
) -> Result<Option<DecisionProof>> {
    if decision_or_verdict_id.trim().is_empty()
        || version_id.trim().is_empty()
        || tree_hash.trim().is_empty()
    {
        return Err(CoreError::Config(
            "decision id, version id and tree hash are required".into(),
        ));
    }
    let (decision_id, requested_revision, verdict_id) = resolve_id(ledger, decision_or_verdict_id)?;
    let Some(accepted) = accepted_row(
        ledger,
        &decision_id,
        requested_revision,
        version_id,
        tree_hash,
    )?
    else {
        return Ok(None);
    };
    let verdict = verdict_row(
        ledger,
        &decision_id,
        accepted.revision,
        verdict_id.as_deref(),
        version_id,
        tree_hash,
    )?;
    let exact = verdict
        .as_ref()
        .is_some_and(|row| row.version_id == version_id && row.tree_hash == tree_hash);
    let source = source_step(&accepted);
    let not_looked_at: Vec<super::text_exclusions::NotLookedAt> = verdict
        .as_ref()
        .map(|row| {
            let basis: Value = serde_json::from_str(&row.basis_json)?;
            basis
                .get("not_looked_at")
                .cloned()
                .map(serde_json::from_value::<Vec<super::text_exclusions::NotLookedAt>>)
                .transpose()
                .map(|v| v.unwrap_or_default())
        })
        .transpose()?
        .unwrap_or_default();
    let mut check = check_step(&accepted);
    if let Some(summary) = super::text_exclusions::summary(&not_looked_at) {
        check.sentence.push(' ');
        check.sentence.push_str(&summary);
    }
    let ran_on = run_step(verdict.as_ref(), version_id, tree_hash, exact);
    let found = found_step(verdict.as_ref(), exact);
    let since = since_step(verdict.as_ref(), version_id, tree_hash, exact);
    let so = conclusion_step(
        ledger,
        &decision_id,
        &accepted,
        verdict.as_ref(),
        version_id,
        exact,
    )?;
    let basis_state = verdict.as_ref().map(|row| row.state.clone());
    let display_state = verdict.as_ref().map(|row| {
        if exact {
            row.state.clone()
        } else {
            "stale".into()
        }
    });
    Ok(Some(DecisionProof {
        schema_version: PROOF_SCHEMA_VERSION,
        decision_id,
        revision: accepted.revision,
        verdict_id: verdict.as_ref().map(|row| row.id.clone()),
        selected_version_id: version_id.into(),
        selected_tree_hash: tree_hash.into(),
        basis_state,
        display_state,
        not_looked_at,
        chain: vec![source, check, ran_on, found, since, so],
    }))
}

fn resolve_id(ledger: &Ledger, id: &str) -> Result<(String, Option<u32>, Option<String>)> {
    let verdict: Option<(String, u32)> = ledger
        .conn()
        .query_row(
            "SELECT decision_id,rev FROM decision_verdicts WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    Ok(match verdict {
        Some((decision_id, revision)) => (decision_id, Some(revision), Some(id.into())),
        None => (id.into(), None, None),
    })
}

fn accepted_row(
    ledger: &Ledger,
    decision_id: &str,
    revision: Option<u32>,
    version_id: &str,
    tree_hash: &str,
) -> Result<Option<AcceptedRow>> {
    let mut statement = ledger.conn().prepare(
        "SELECT a.id,a.rev,a.actor,a.accepted_at,r.kind,r.params_schema_version,
                    r.params_json,r.text,r.sources_json
             FROM decision_acceptances a
             JOIN decision_revisions r ON r.decision_id=a.decision_id AND r.rev=a.rev
             WHERE a.decision_id=?1 AND (?2 IS NULL OR a.rev=?2)
             ORDER BY CASE WHEN EXISTS(
                 SELECT 1 FROM decision_verdicts v
                 WHERE v.decision_id=a.decision_id AND v.rev=a.rev
                   AND v.version_id=?3 AND v.tree_hash=?4
             ) THEN 0 ELSE 1 END, a.rowid DESC",
    )?;
    let rows = statement.query_map(
        params![decision_id, revision, version_id, tree_hash],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                AcceptedRow {
                    revision: row.get(1)?,
                    actor: row.get(2)?,
                    accepted_at: row.get(3)?,
                    kind: row.get(4)?,
                    params_schema_version: row.get(5)?,
                    params_json: row.get(6)?,
                    span: row.get(7)?,
                    sources_json: row.get(8)?,
                },
            ))
        },
    )?;
    for row in rows {
        let (act_id, accepted) = row?;
        if authority::bridge::verified(ledger, PersonActKind::Accept, &act_id)? {
            return Ok(Some(accepted));
        }
    }
    Ok(None)
}

fn verdict_row(
    ledger: &Ledger,
    decision_id: &str,
    revision: u32,
    verdict_id: Option<&str>,
    version_id: &str,
    tree_hash: &str,
) -> Result<Option<VerdictRow>> {
    let query = if verdict_id.is_some() {
        "SELECT v.rowid,v.id,v.version_id,v.tree_hash,v.verdict,v.reason,v.checkable_if,
                v.findings_json,v.tool_versions_json,v.basis_json
         FROM decision_verdicts v WHERE v.id=?1 AND v.decision_id=?2 AND v.rev=?3
         LIMIT 1"
    } else {
        "SELECT v.rowid,v.id,v.version_id,v.tree_hash,v.verdict,v.reason,v.checkable_if,
                v.findings_json,v.tool_versions_json,v.basis_json
         FROM decision_verdicts v
         WHERE v.decision_id=?1 AND v.rev=?2 AND v.version_id=?3
         ORDER BY CASE WHEN v.tree_hash=?4 THEN 0 ELSE 1 END, v.rowid DESC
         LIMIT 1"
    };
    let row = |row: &rusqlite::Row<'_>| {
        Ok(VerdictRow {
            rowid: row.get(0)?,
            id: row.get(1)?,
            version_id: row.get(2)?,
            tree_hash: row.get(3)?,
            state: row.get(4)?,
            reason: row.get(5)?,
            checkable_if: row.get(6)?,
            findings_json: row.get(7)?,
            tool_versions_json: row.get(8)?,
            basis_json: row.get(9)?,
        })
    };
    if let Some(id) = verdict_id {
        ledger
            .conn()
            .query_row(query, params![id, decision_id, revision], row)
            .optional()
            .map_err(CoreError::from)
    } else {
        ledger
            .conn()
            .query_row(
                query,
                params![decision_id, revision, version_id, tree_hash],
                row,
            )
            .optional()
            .map_err(CoreError::from)
    }
}

fn source_step(accepted: &AcceptedRow) -> ChainStep {
    let sources = serde_json::from_str::<Vec<DecisionSource>>(&accepted.sources_json).ok();
    let source_ids = sources
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter_map(|source| source.event_id.as_ref().or(source.file_anchor.as_ref()))
        .cloned()
        .collect();
    let first = sources.as_ref().and_then(|sources| sources.first());
    let (state, sentence) = match first {
        Some(source) if source.raw_line > 0 && !source.raw_file.trim().is_empty() => (
            LinkState::Holds,
            format!(
                "Decided by {} at {}: \"{}\" ({}:{}).",
                safe(&accepted.actor),
                safe(&accepted.accepted_at),
                safe(&accepted.span),
                safe(&source.raw_file),
                source.raw_line
            ),
        ),
        _ => (
            LinkState::Missing,
            format!(
                "Accepted by {} at {}, but the source file and line are missing.",
                safe(&accepted.actor),
                safe(&accepted.accepted_at)
            ),
        ),
    };
    step(
        1,
        ChainLabel::Decided,
        state,
        sentence,
        source_ids,
        Vec::new(),
        None,
    )
}

fn check_step(accepted: &AcceptedRow) -> ChainStep {
    let (state, sentence) = match parse_params(
        &accepted.kind,
        accepted.params_schema_version,
        &accepted.params_json,
    ) {
        Ok(params) => match serde_json::to_string(&params) {
            Ok(text) => (
                LinkState::Holds,
                format!(
                    "Accepted check {} with parameters {}.",
                    safe(&accepted.kind),
                    safe(&text)
                ),
            ),
            Err(_) => (
                LinkState::Missing,
                "Accepted check parameters cannot be read.".into(),
            ),
        },
        Err(_) => (
            LinkState::Missing,
            "Accepted check parameters cannot be read.".into(),
        ),
    };
    step(
        2,
        ChainLabel::Check,
        state,
        sentence,
        Vec::new(),
        Vec::new(),
        None,
    )
}

fn run_step(
    verdict: Option<&VerdictRow>,
    version_id: &str,
    tree_hash: &str,
    exact: bool,
) -> ChainStep {
    let (state, sentence) = match verdict {
        Some(row) if exact => (
            LinkState::Holds,
            format!(
                "Ran on {} at tree {} with tools {}.",
                safe(&row.version_id),
                safe(&row.tree_hash),
                safe(&row.tool_versions_json)
            ),
        ),
        Some(row) => (
            LinkState::Moved,
            format!(
                "Stored run is on {} at tree {} with tools {}; selected {} at tree {} differs.",
                safe(&row.version_id),
                safe(&row.tree_hash),
                safe(&row.tool_versions_json),
                safe(version_id),
                safe(tree_hash)
            ),
        ),
        None => (
            LinkState::Missing,
            format!(
                "No stored run names {} at tree {}.",
                safe(version_id),
                safe(tree_hash)
            ),
        ),
    };
    step(
        3,
        ChainLabel::RanOn,
        state,
        sentence,
        Vec::new(),
        Vec::new(),
        Some(version_id.into()),
    )
}

fn found_step(verdict: Option<&VerdictRow>, exact: bool) -> ChainStep {
    let Some(row) = verdict else {
        return step(
            4,
            ChainLabel::Found,
            LinkState::Missing,
            "No finding receipt was recorded for this version.".into(),
            Vec::new(),
            Vec::new(),
            None,
        );
    };
    let findings = serde_json::from_str::<Value>(&row.findings_json)
        .ok()
        .and_then(|value| value.as_array().cloned());
    let Some(findings) = findings else {
        return step(
            4,
            ChainLabel::Found,
            LinkState::Missing,
            "Stored finding data cannot be read.".into(),
            Vec::new(),
            Vec::new(),
            Some(row.version_id.clone()),
        );
    };
    if findings.is_empty() {
        let (state, sentence) = if row.state == "holds" && exact {
            (
                LinkState::Holds,
                "Stored check recorded no matching places.",
            )
        } else {
            (LinkState::Missing, "No finding receipt was recorded.")
        };
        return step(
            4,
            ChainLabel::Found,
            state,
            sentence.into(),
            Vec::new(),
            Vec::new(),
            Some(row.version_id.clone()),
        );
    }
    let finding_ids = findings
        .iter()
        .enumerate()
        .map(|(index, finding)| {
            finding
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{}:finding:{}", row.id, index + 1))
        })
        .collect();
    let first = &findings[0];
    let path = first.get("path").and_then(Value::as_str);
    let line = first.get("line").and_then(Value::as_u64);
    let span = first.get("span").and_then(Value::as_str);
    let Some((path, line)) = path.zip(line) else {
        return step(
            4,
            ChainLabel::Found,
            LinkState::Missing,
            "Stored finding lacks a file and line.".into(),
            Vec::new(),
            finding_ids,
            Some(row.version_id.clone()),
        );
    };
    let count = findings.len();
    let mut sentence = format!(
        "{} {}: {}:{}",
        count,
        if count == 1 { "place" } else { "places" },
        safe(path),
        line
    );
    if let Some(span) = span {
        sentence.push_str(&format!(" · \"{}\"", safe(span)));
    }
    if let Some(edit_id) = first.get("edit_id").and_then(Value::as_str) {
        sentence.push_str(&format!(" · introduced by edit {}", safe(edit_id)));
        if let Some(session_id) = first.get("session_id").and_then(Value::as_str) {
            sentence.push_str(&format!(" in session {}", safe(session_id)));
        }
        if let Some(at) = first.get("introduced_at").and_then(Value::as_str) {
            sentence.push_str(&format!(" at {}", safe(at)));
        }
    } else if let Some(since) = first.get("present_since_version").and_then(Value::as_str) {
        sentence.push_str(&format!(" · present since {}", safe(since)));
    } else {
        sentence.push_str(" · introducing edit is not linked");
    }
    sentence.push('.');
    step(
        4,
        ChainLabel::Found,
        if exact {
            LinkState::Holds
        } else {
            LinkState::Moved
        },
        sentence,
        Vec::new(),
        finding_ids,
        Some(row.version_id.clone()),
    )
}

fn since_step(
    verdict: Option<&VerdictRow>,
    version_id: &str,
    tree_hash: &str,
    exact: bool,
) -> ChainStep {
    let (state, sentence) = match verdict {
        Some(_) if exact => (
            LinkState::Holds,
            "Selected tree matches the recorded run. Later edit attribution is not linked in this view."
                .into(),
        ),
        Some(_) => (
            LinkState::Moved,
            "Selected tree differs from the recorded run; no relevant edit receipt is linked."
                .into(),
        ),
        None => (
            LinkState::Missing,
            "Without a stored run, later relevant edits cannot be compared.".into(),
        ),
    };
    let _ = tree_hash;
    step(
        5,
        ChainLabel::Since,
        state,
        sentence,
        Vec::new(),
        Vec::new(),
        Some(version_id.into()),
    )
}

fn conclusion_step(
    ledger: &Ledger,
    decision_id: &str,
    accepted: &AcceptedRow,
    verdict: Option<&VerdictRow>,
    version_id: &str,
    exact: bool,
) -> Result<ChainStep> {
    let Some(row) = verdict else {
        return Ok(step(
            6,
            ChainLabel::So,
            LinkState::Missing,
            "No verdict is recorded for this exact version and tree.".into(),
            Vec::new(),
            Vec::new(),
            Some(version_id.into()),
        ));
    };
    if !exact {
        return Ok(step(
            6,
            ChainLabel::So,
            LinkState::Moved,
            "Stored verdict is stale for the selected tree; check this tree before drawing a conclusion."
                .into(),
            Vec::new(),
            Vec::new(),
            Some(version_id.into()),
        ));
    }
    let span = safe(&accepted.span);
    let (state, sentence) = match row.state.as_str() {
        "holds" => {
            let count = finding_count(&row.findings_json);
            if count == Some(0) {
                (
                    LinkState::Holds,
                    format!(
                        "Decision holds on {}: \"{}\" — no findings in the stored check.",
                        safe(version_id),
                        span
                    ),
                )
            } else {
                (
                    LinkState::Missing,
                    "Stored hold and finding receipts do not agree.".into(),
                )
            }
        }
        "broken" => {
            let count = finding_count(&row.findings_json).unwrap_or(0);
            if count == 0 {
                (
                    LinkState::Missing,
                    "Stored break has no readable finding receipt.".into(),
                )
            } else {
                let last_held: Option<String> = ledger
                    .conn()
                    .query_row(
                        "SELECT version_id FROM decision_verdicts
                         WHERE decision_id=?1 AND rev=?2 AND verdict='holds' AND rowid<?3
                         ORDER BY rowid DESC LIMIT 1",
                        params![decision_id, accepted.revision, row.rowid],
                        |found| found.get(0),
                    )
                    .optional()?;
                let mut sentence = format!(
                    "Decision broken on {}: \"{}\" — {} {}.",
                    safe(version_id),
                    span,
                    count,
                    if count == 1 { "place" } else { "places" }
                );
                if let Some(previous) = last_held {
                    sentence.push_str(&format!(" Last held on {}.", safe(&previous)));
                }
                (LinkState::Holds, sentence)
            }
        }
        "unverifiable" => match (&row.reason, &row.checkable_if) {
            (Some(reason), Some(condition))
                if !reason.trim().is_empty() && !condition.trim().is_empty() =>
            {
                (
                    LinkState::Holds,
                    format!(
                        "Can't check \"{}\" on {}: {}. It can be checked if {}.",
                        span,
                        safe(version_id),
                        safe(reason),
                        safe(condition)
                    ),
                )
            }
            _ => (
                LinkState::Missing,
                "Stored unverifiable result lacks a reason or checkable condition.".into(),
            ),
        },
        "not_applicable" => match &row.reason {
            Some(reason) if !reason.trim().is_empty() => (
                LinkState::Holds,
                format!(
                    "Decision is not applicable on {}: {}.",
                    safe(version_id),
                    safe(reason)
                ),
            ),
            _ => (
                LinkState::Missing,
                "Stored non-applicability lacks a reason.".into(),
            ),
        },
        _ => (
            LinkState::Missing,
            "Stored verdict state is unknown.".into(),
        ),
    };
    Ok(step(
        6,
        ChainLabel::So,
        state,
        sentence,
        Vec::new(),
        Vec::new(),
        Some(version_id.into()),
    ))
}

fn finding_count(json: &str) -> Option<usize> {
    serde_json::from_str::<Value>(json)
        .ok()?
        .as_array()
        .map(Vec::len)
}

fn safe(raw: &str) -> String {
    Masked::mask(raw)
        .into_text()
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

fn step(
    number: u8,
    label: ChainLabel,
    state: LinkState,
    sentence: String,
    source_ids: Vec<String>,
    finding_ids: Vec<String>,
    version_id: Option<String>,
) -> ChainStep {
    ChainStep {
        number,
        label,
        state,
        sentence,
        source_ids,
        finding_ids,
        version_id,
    }
}
