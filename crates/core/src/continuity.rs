//! C4.3 — the legacy/local-only continuity handshake (A180, corrected by P1
//! Packet 1).
//!
//! **A new session is not "recovered" because it opened.** This module is the
//! pre-P1 local comparison retained for historical readability. It does not
//! observe or interrogate a successor session, so its rows are not verified
//! recovery evidence and never enter the P1 verified-continuation metric.
//!
//! Four laws:
//!
//! 1. **Deterministic checks only.** Legacy v1 compares lineage, digests, and the
//!    presence of recorded facts. It does **not** score whether a successor
//!    "understood" the capsule — that is semantic judgement, it belongs to
//!    Layer 3, and RB6 will bring it labeled and shadowed. A restatement check
//!    here records an operator acknowledgement, never an observation that a
//!    successor *comprehended* it.
//! 2. **A check that cannot be run is `Insufficient`, never a pass.** The
//!    coverage count is the honest output: "6 of 7 · 1 insufficient evidence"
//!    is a sentence a person can act on; a naked score is not.
//! 3. **An override never rewrites a verdict.** It appends a new row whose
//!    outcome is `resumed_with_warnings` and whose reason is recorded. The
//!    failed check keeps saying what it said.
//! 4. **No observed successor, no verified recovery.** The legacy path always
//!    emits an insufficient `successor evidence` check. A future protocol must
//!    supply current, protocol-valid evidence through a separate authority.

use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::capsule::{Capsule, Provenance};
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::snapshot::WorkspaceSnapshot;

/// One check's outcome. `Insufficient` is a first-class answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    Passed,
    Failed,
    /// The check could not be run — the input it needs is absent. Never a pass.
    Insufficient,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    pub state: CheckState,
    /// What was compared, in words a person can act on.
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Every mandatory check passed, including protocol-valid successor
    /// evidence. The legacy production path cannot construct this outcome.
    Verified,
    /// No mandatory check failed, but at least one is missing, stale, unknown,
    /// or could not run.
    Insufficient,
    /// A human accepted a failed or insufficient handshake, with a reason.
    ResumedWithWarnings,
    /// A check failed and nobody overrode it.
    Failed,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Verified => "verified",
            Outcome::Insufficient => "insufficient",
            Outcome::ResumedWithWarnings => "resumed_with_warnings",
            Outcome::Failed => "failed",
        }
    }
}

/// The legacy protocol's mandatory check set. Naming it here makes omission a
/// fail-closed condition: adding, removing, or failing to emit one cannot
/// silently reduce the denominator and produce success.
pub const MANDATORY_CHECK_NAMES: [&str; 7] = [
    "repository lineage",
    "HEAD lineage",
    "workspace digest",
    "objective acknowledged",
    "do-not-repeat",
    "capsule freshness",
    "successor evidence",
];

/// Stable reason carried by the legacy path when local checks are all that
/// exist. This is deliberately machine-readable inside the human detail.
pub const SUCCESSOR_EVIDENCE_MISSING: &str = "successor_evidence_missing";

/// Evaluate a set of checks against the explicit mandatory set.
///
/// Failure has priority because it is observed disagreement. Otherwise every
/// mandatory check must occur exactly once and pass. Missing, duplicated, or
/// insufficient required checks are non-success. Checks outside the mandatory
/// set are optional and cannot compensate for a required gap.
pub fn evaluate_required_checks(checks: &[Check]) -> Outcome {
    if checks.iter().any(|check| {
        MANDATORY_CHECK_NAMES.contains(&check.name.as_str()) && check.state == CheckState::Failed
    }) {
        return Outcome::Failed;
    }

    for required in MANDATORY_CHECK_NAMES {
        let mut matches = checks.iter().filter(|check| check.name == required);
        let Some(check) = matches.next() else {
            return Outcome::Insufficient;
        };
        if matches.next().is_some() || check.state != CheckState::Passed {
            return Outcome::Insufficient;
        }
    }

    Outcome::Verified
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handshake {
    pub id: String,
    pub checked_at: String,
    pub project_id: String,
    pub capsule_chain_position: String,
    pub snapshot_id: Option<String>,
    pub checks: Vec<Check>,
    pub outcome: Outcome,
    pub override_reason: Option<String>,
}

impl Handshake {
    pub fn passed(&self) -> u64 {
        self.count(CheckState::Passed)
    }
    pub fn insufficient(&self) -> u64 {
        self.count(CheckState::Insufficient)
    }
    pub fn failed(&self) -> u64 {
        self.count(CheckState::Failed)
    }
    fn count(&self, state: CheckState) -> u64 {
        self.checks.iter().filter(|c| c.state == state).count() as u64
    }
    pub fn total(&self) -> u64 {
        self.checks.len() as u64
    }

    /// The coverage sentence. Never a naked score (§4, and the C0.2 amendment).
    pub fn coverage(&self) -> String {
        let mut s = format!("Continuity: {} of {} checks", self.passed(), self.total());
        if self.insufficient() > 0 {
            s.push_str(&format!(" · {} insufficient evidence", self.insufficient()));
        }
        if self.failed() > 0 {
            s.push_str(&format!(" · {} did not match", self.failed()));
        }
        s
    }
}

/// Run the handshake.
///
/// `fresh` is the workspace as it stands NOW; the capsule carries the state it
/// was compiled beside. Every comparison is between those two, plus the rows
/// the successor left behind.
pub fn handshake(
    ledger: &Ledger,
    capsule: &Capsule,
    fresh: Option<&WorkspaceSnapshot>,
    checked_at: &str,
    acknowledged_objective: bool,
) -> Result<Handshake> {
    let mut checks = Vec::new();
    let cited = capsule.workspace_snapshot.as_ref();

    // 1 · repository lineage.
    checks.push(match (cited, fresh) {
        (Some(c), Some(f)) => {
            let fresh_branch = f.git.as_ref().and_then(|g| g.branch.clone());
            let same = c.branch == fresh_branch;
            Check {
                name: "repository lineage".to_string(),
                state: if same {
                    CheckState::Passed
                } else {
                    CheckState::Failed
                },
                detail: format!(
                    "capsule was compiled on {} · the workspace is on {}",
                    c.branch.as_deref().unwrap_or("(detached)"),
                    fresh_branch.as_deref().unwrap_or("(detached)")
                ),
            }
        }
        _ => Check {
            name: "repository lineage".to_string(),
            state: CheckState::Insufficient,
            detail: "no workspace snapshot to compare against".to_string(),
        },
    });

    // 2 · HEAD lineage.
    checks.push(match (cited, fresh) {
        (Some(c), Some(f)) => {
            let fresh_head = f.git.as_ref().and_then(|g| g.head.clone());
            let same = c.head == fresh_head;
            Check {
                name: "HEAD lineage".to_string(),
                state: if same {
                    CheckState::Passed
                } else {
                    CheckState::Failed
                },
                detail: format!(
                    "capsule expected {} · the workspace is at {}",
                    c.head.as_deref().map(short).unwrap_or("(none)".into()),
                    fresh_head.as_deref().map(short).unwrap_or("(none)".into())
                ),
            }
        }
        _ => Check {
            name: "HEAD lineage".to_string(),
            state: CheckState::Insufficient,
            detail: "no workspace snapshot to compare against".to_string(),
        },
    });

    // 3 · dirty-state digest.
    checks.push(match (cited, fresh) {
        (Some(c), Some(f)) => {
            // Compare the DIRTY TREE, never the snapshot envelope. The doc
            // carries a fresh id and timestamp on every capture, so an
            // envelope comparison can never pass — the kill-9 walk found this
            // by failing a check that was structurally unable to succeed.
            let same = match (&c.dirty_diff_sha256, &f.tree.dirty_diff_sha256) {
                (Some(a), Some(b)) => a == b,
                // No diff digest on either side: fall back to the envelope and
                // accept that it will differ, which reads as "moved" rather
                // than pretending to a comparison that was not made.
                _ => {
                    c.doc_sha256
                        == crate::chain::sha256_hex(
                            serde_json::to_string_pretty(f)
                                .unwrap_or_default()
                                .as_bytes(),
                        )
                }
            };
            Check {
                name: "workspace digest".to_string(),
                state: if same {
                    CheckState::Passed
                } else {
                    CheckState::Failed
                },
                detail: if same {
                    "the working tree is byte-identical to the preserved snapshot".to_string()
                } else {
                    "the working tree has moved since the capsule was compiled — expected when \
                     work continued, and stated rather than assumed"
                        .to_string()
                },
            }
        }
        _ => Check {
            name: "workspace digest".to_string(),
            state: CheckState::Insufficient,
            detail: "no workspace snapshot to compare against".to_string(),
        },
    });

    // 4 · objective acknowledged. Presence + operator confirmation, never a
    // semantic score: whether a successor UNDERSTOOD is Layer-3 and RB6's.
    let objective_present = capsule
        .sections
        .iter()
        .any(|s| s.key == "objective" && !s.lines.is_empty());
    checks.push(Check {
        name: "objective acknowledged".to_string(),
        state: match (objective_present, acknowledged_objective) {
            (false, _) => CheckState::Insufficient,
            (true, true) => CheckState::Passed,
            (true, false) => CheckState::Failed,
        },
        detail: if !objective_present {
            "the capsule carries no recorded opening ask, so there is nothing to acknowledge"
                .to_string()
        } else if acknowledged_objective {
            "the operator confirmed the successor restated the objective — a confirmation, \
             never a score of comprehension"
                .to_string()
        } else {
            "not confirmed by the operator".to_string()
        },
    });

    // 5 · do-not-repeat. RUNNABLE as of C5 (A182): the operation ledger gives
    // every side-effectful command a stable identity and a replay class
    // carried by a named rule, so "was one re-run?" is a digest comparison
    // rather than the judgement this check used to abstain from.
    let ops = crate::operations::derive(ledger, &capsule.scope.id)?;
    let must_not_repeat = crate::operations::do_not_repeat(&ops);
    checks.push(if must_not_repeat.is_empty() {
        Check {
            name: "do-not-repeat".to_string(),
            state: CheckState::Passed,
            detail: format!(
                "{} operation(s) observed, none of them completed with a replay class that \
                 needs a decision — there is nothing a successor could wrongly repeat",
                ops.len()
            ),
        }
    } else {
        let anchor = capsule
            .sections
            .iter()
            .flat_map(|s| s.lines.iter())
            .filter_map(|l| match &l.provenance {
                Provenance::EvidenceBacked { receipts } => {
                    receipts.first().map(|r| r.event_id.clone())
                }
                _ => None,
            })
            .max()
            .unwrap_or_default();
        let repeated =
            crate::operations::repeats_after(ledger, &capsule.scope.id, &must_not_repeat, &anchor)?;
        if repeated.is_empty() {
            Check {
                name: "do-not-repeat".to_string(),
                state: CheckState::Passed,
                detail: format!(
                    "{} completed operation(s) must not be repeated silently ({}); the record \
                     holds no re-run of any of them since the capsule was compiled",
                    must_not_repeat.len(),
                    must_not_repeat
                        .iter()
                        .map(|o| o.replay_class.as_str())
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            }
        } else {
            Check {
                name: "do-not-repeat".to_string(),
                state: CheckState::Failed,
                detail: format!(
                    "{} operation(s) were run again after the capsule was compiled: {}",
                    repeated.len(),
                    repeated.join(" · ")
                ),
            }
        }
    });

    // 6 · freshness: is the capsule still current?
    let (head, _total) = ledger.compute_project_head(&capsule.scope.id)?;
    checks.push(Check {
        name: "capsule freshness".to_string(),
        state: if head == capsule.binding.chain_position {
            CheckState::Passed
        } else {
            CheckState::Failed
        },
        detail: if head == capsule.binding.chain_position {
            "the record has not moved since this capsule was compiled".to_string()
        } else {
            "the record has moved since this capsule was compiled — recompile before relying \
             on it"
                .to_string()
        },
    });

    // 7 · observable successor evidence. Legacy v1 has no target-session id,
    // challenge, response, or bounded probe. An operator-supplied boolean is
    // not successor evidence, so this check must remain insufficient here.
    checks.push(Check {
        name: "successor evidence".to_string(),
        state: CheckState::Insufficient,
        detail: format!(
            "{SUCCESSOR_EVIDENCE_MISSING}: the legacy/local-only protocol observed no fresh \
             successor session and issued no protocol-valid continuity challenge"
        ),
    });

    let outcome = evaluate_required_checks(&checks);
    Ok(Handshake {
        id: Ulid::generate().to_string(),
        checked_at: checked_at.to_string(),
        project_id: capsule.scope.id.clone(),
        capsule_chain_position: capsule.binding.chain_position.clone(),
        snapshot_id: cited.map(|c| c.id.clone()),
        outcome,
        override_reason: None,
        checks,
    })
}

fn short(h: &str) -> String {
    h.chars().take(12).collect()
}

/// Apply an operator override. Never rewrites the verdict: the outcome becomes
/// `resumed_with_warnings` and the reason travels with it.
pub fn override_with(handshake: &Handshake, reason: &str) -> Handshake {
    Handshake {
        outcome: Outcome::ResumedWithWarnings,
        override_reason: Some(reason.to_string()),
        ..handshake.clone()
    }
}

/// Append one legacy/local-only result.
///
/// `continuity_checks` predates the P1 protocol and its CHECK constraint has no
/// `insufficient` value. To preserve the schema and every historical row, a
/// new insufficient result is encoded as the existing non-success `failed`
/// outcome with its exact insufficient count and check details intact. A
/// legacy `Verified` result is rejected: this path has no successor evidence.
pub fn record(ledger: &Ledger, h: &Handshake) -> Result<()> {
    let stored_outcome = match h.outcome {
        Outcome::Verified => {
            return Err(CoreError::Config(
                "legacy/local-only continuity cannot record verified recovery: successor \
                 evidence is unavailable"
                    .to_string(),
            ));
        }
        Outcome::Insufficient | Outcome::Failed => "failed",
        Outcome::ResumedWithWarnings => "resumed_with_warnings",
    };
    ledger.conn().execute(
        "INSERT INTO continuity_checks(id, checked_at, project_id, capsule_chain_position,
            snapshot_id, outcome, passed, total, insufficient, detail, override_reason)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        rusqlite::params![
            h.id,
            h.checked_at,
            h.project_id,
            h.capsule_chain_position,
            h.snapshot_id,
            stored_outcome,
            h.passed() as i64,
            h.total() as i64,
            h.insufficient() as i64,
            serde_json::to_string(&h.checks)?,
            h.override_reason,
        ],
    )?;
    Ok(())
}

/// Read the historical legacy/local-only coverage. These rows remain visible
/// for audit, but the result is not a verified-recovery metric.
pub fn legacy_local_continuity_rate(ledger: &Ledger) -> Result<(u64, u64)> {
    let (v, t): (i64, i64) = ledger.conn().query_row(
        "SELECT COALESCE(SUM(outcome = 'verified'), 0), COUNT(*) FROM continuity_checks",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok((v.max(0) as u64, t.max(0) as u64))
}

/// **The P1 north-star metric.** Only protocol-valid successor handshakes may
/// enter this denominator. Packet 1 implements no successor protocol, so the
/// honest result is zero of zero regardless of legacy rows on disk.
pub fn verified_continuation_rate(_ledger: &Ledger) -> Result<(u64, u64)> {
    Ok((0, 0))
}
