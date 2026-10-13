//! Read-only standing-decision check at the Change approval boundary.
//!
//! The gate reads a cached verdict for the exact candidate version. It never
//! runs a check while the approval write lock is held. An uncheckable result
//! follows the project's closed Decisions policy; without one, it blocks.

use std::path::Path;

use rusqlite::{params, OptionalExtension};
use thruline_policy::decisions::{OnUnverifiable, RequireHold};
use thruline_policy::Policy;

use crate::error::{CoreError, Result};
use crate::ledger::Ledger;

use super::authority::{self, bridge, PersonActKind};
use super::engine::CHECK_IMPL_VERSION;
use super::storage;

/// The product reads the same authority and policy predicate as Approve.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct ApprovalGateState {
    pub active: bool,
    pub reason: Option<String>,
}

pub fn approval_gate_state(ledger: &Ledger, project_id: &str) -> Result<ApprovalGateState> {
    let reason = if !bridge::gates_active(ledger, project_id)? {
        Some("Standing blocking decisions await person acceptance in this record.")
    } else if !project_policy_enables_gate(ledger, project_id)? {
        Some("The project has no enabled Decisions approval policy.")
    } else {
        None
    };
    Ok(ApprovalGateState {
        active: reason.is_none(),
        reason: reason.map(str::to_owned),
    })
}

/// A pending row, warning, retirement or valid exact-version waiver cannot
/// create a refusal. This function reads; it never runs a project check.
pub fn decision_blocks_approval(
    ledger: &Ledger,
    project_id: &str,
    decision_id: &str,
    version_id: &str,
    at: &str,
) -> Result<bool> {
    if !approval_gate_state(ledger, project_id)?.active {
        return Ok(false);
    }
    let Some(decision) = storage::accepted_for_project(ledger, project_id)?
        .into_iter()
        .find(|decision| decision.id == decision_id)
    else {
        return Ok(false);
    };
    Ok(blocking_reason(ledger, &decision, version_id, at)?.is_some())
}

pub fn require_approval_clear(
    ledger: &Ledger,
    change_id: &str,
    version_id: &str,
    at: &str,
) -> Result<()> {
    let project_id: String = ledger
        .conn()
        .query_row(
            "SELECT project_id FROM changes WHERE id=?1",
            [change_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::Config("change has no project".into()))?;
    if !approval_gate_state(ledger, &project_id)?.active {
        return Ok(());
    }
    for decision in storage::accepted_for_project(ledger, &project_id)? {
        if let Some(reason) = blocking_reason(ledger, &decision, version_id, at)? {
            return Err(CoreError::Config(reason));
        }
    }
    Ok(())
}

fn blocking_reason(
    ledger: &Ledger,
    decision: &storage::AcceptedDecision,
    version_id: &str,
    at: &str,
) -> Result<Option<String>> {
    if decision.severity != "blocks"
        || target_predates_start(ledger, &decision.applies_from_version, version_id)?
        || valid_waiver(ledger, &decision.id, version_id, at)?
    {
        return Ok(None);
    }
    let project_id: String = ledger.conn().query_row(
        "SELECT project_id FROM decisions WHERE id=?1",
        [&decision.id],
        |row| row.get(0),
    )?;
    let verdict: Option<(String, String, String)> = ledger
        .conn()
        .query_row(
            "SELECT verdict, findings_json, check_impl_version FROM decision_verdicts
                 WHERE decision_id=?1 AND rev=?2 AND version_id=?3
                 ORDER BY rowid DESC LIMIT 1",
            params![decision.id, decision.revision, version_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    if verdict
        .as_ref()
        .is_some_and(|(_, _, impl_version)| impl_version != CHECK_IMPL_VERSION)
    {
        return Ok(Some(format!(
            "decision {} has a stale check receipt on {}; check it again before Approve",
            decision.id, version_id
        )));
    }
    match verdict.as_ref().map(|(state, _, _)| state.as_str()) {
            Some("holds" | "not_applicable") => Ok(None),
            Some("unverifiable") if project_policy_warns_unverifiable(ledger, &project_id)? => Ok(None),
            Some("broken" | "unverifiable") => {
                if let Some((state, findings, _)) = &verdict {
                    let place = first_place(findings);
                    return Ok(Some(format!(
                        "decision {} is {} on {}{}; check it or record a named waiver before Approve",
                        decision.id, state, version_id, place
                    )));
                }
                Ok(None)
            }
            None if project_policy_warns_unverifiable(ledger, &project_id)? => Ok(None),
            _ => {
                Ok(Some(format!(
                    "decision {} has no checkable verdict on {}; check it or record a named waiver before Approve",
                    decision.id, version_id
                )))
            }
        }
}

fn project_decisions_policy(
    ledger: &Ledger,
    project_id: &str,
) -> Result<Option<thruline_policy::decisions::DecisionsPolicy>> {
    let root: String = ledger.conn().query_row(
        "SELECT root_path FROM projects WHERE id=?1",
        [project_id],
        |row| row.get(0),
    )?;
    let path = Path::new(&root).join(".thruline/policy.yml");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(CoreError::io(&path, error)),
    };
    let policy = Policy::parse(&text)
        .map_err(|error| CoreError::Config(format!("decision policy is invalid: {error}")))?;
    Ok(policy.decisions)
}

fn project_policy_enables_gate(ledger: &Ledger, project_id: &str) -> Result<bool> {
    Ok(project_decisions_policy(ledger, project_id)?
        .is_some_and(|settings| settings.require_hold != RequireHold::None))
}

fn project_policy_warns_unverifiable(ledger: &Ledger, project_id: &str) -> Result<bool> {
    Ok(project_decisions_policy(ledger, project_id)?
        .is_some_and(|settings| settings.on_unverifiable == OnUnverifiable::Warn))
}

fn first_place(findings_json: &str) -> String {
    let Ok(findings) = serde_json::from_str::<serde_json::Value>(findings_json) else {
        return String::new();
    };
    let Some((path, line)) = findings.as_array().and_then(|items| {
        items.iter().find_map(|item| {
            Some((
                item.get("path")?.as_str()?.to_owned(),
                item.get("line")?.as_u64()?,
            ))
        })
    }) else {
        return String::new();
    };
    format!(" at {path}:{line}")
}

/// An earlier immutable snapshot is outside a rule whose accepted start is a
/// later snapshot. Other version families need a positive lineage receipt;
/// lacking one, a blocking decision remains conservative at Approve.
fn target_predates_start(ledger: &Ledger, from: &str, target: &str) -> Result<bool> {
    if from == target {
        return Ok(false);
    }
    let taken_at = |version: &str| -> Result<Option<String>> {
        Ok(ledger
            .conn()
            .query_row(
                "SELECT taken_at FROM workspace_snapshots WHERE id=?1",
                [version],
                |row| row.get(0),
            )
            .optional()?)
    };
    Ok(match (taken_at(from)?, taken_at(target)?) {
        (Some(start), Some(candidate)) => candidate < start,
        _ => false,
    })
}

fn valid_waiver(ledger: &Ledger, decision_id: &str, version_id: &str, at: &str) -> Result<bool> {
    let mut statement = ledger.conn().prepare(
        "SELECT id, waived_at, expires_version, expires_at FROM decision_waivers
         WHERE decision_id=?1 AND version_id=?2 ORDER BY rowid DESC",
    )?;
    let rows = statement.query_map(params![decision_id, version_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
        ))
    })?;
    for row in rows {
        let (act_id, waived_at, expires_version, expires_at) = row?;
        if waived_at.as_str() > at
            || expires_version.as_deref() == Some(version_id)
            || expires_at.as_deref().is_some_and(|expiry| expiry <= at)
            || !authority::bridge::verified(ledger, PersonActKind::Waive, &act_id)?
        {
            continue;
        }
        return Ok(true);
    }
    Ok(false)
}
