//! Closed decision policy and a pure PR-gate evaluator.
//!
//! The caller supplies accepted decisions from the verified derived copy and
//! verdicts from a receipt bound to one exact version and tree. This module
//! never infers a `holds` result from absent evidence or model output.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Conclusion, Node, ParseError, Reason};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequireHold {
    All,
    Blocking,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnUnverifiable {
    Block,
    Warn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnStale {
    Recompute,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecisionsPolicy {
    pub require_hold: RequireHold,
    pub on_unverifiable: OnUnverifiable,
    pub on_stale: OnStale,
}

impl Default for DecisionsPolicy {
    fn default() -> Self {
        Self {
            require_hold: RequireHold::Blocking,
            on_unverifiable: OnUnverifiable::Block,
            on_stale: OnStale::Block,
        }
    }
}

pub(crate) fn parse(node: &Node) -> Result<DecisionsPolicy, ParseError> {
    let Node::Map(fields) = node else {
        return Err(ParseError::Type {
            key: "decisions".into(),
            want: "a map of decision gate rules",
            found: format!("{node:?}"),
        });
    };
    Ok(DecisionsPolicy {
        require_hold: match scalar(fields, "require_hold", "blocking")? {
            "all" => RequireHold::All,
            "blocking" => RequireHold::Blocking,
            "none" => RequireHold::None,
            other => {
                return Err(invalid(
                    "require_hold",
                    "`all`, `blocking` or `none`",
                    other,
                ))
            }
        },
        on_unverifiable: match scalar(fields, "on_unverifiable", "block")? {
            "block" => OnUnverifiable::Block,
            "warn" => OnUnverifiable::Warn,
            other => return Err(invalid("on_unverifiable", "`block` or `warn`", other)),
        },
        on_stale: match scalar(fields, "on_stale", "block")? {
            "recompute" => OnStale::Recompute,
            "block" => OnStale::Block,
            other => return Err(invalid("on_stale", "`recompute` or `block`", other)),
        },
    })
}

fn scalar<'a>(
    fields: &'a BTreeMap<String, Node>,
    key: &str,
    default: &'a str,
) -> Result<&'a str, ParseError> {
    match fields.get(key) {
        None => Ok(default),
        Some(Node::Scalar(value)) => Ok(value),
        Some(other) => Err(invalid(key, "a scalar", &format!("{other:?}"))),
    }
}

fn invalid(key: &str, want: &'static str, found: &str) -> ParseError {
    ParseError::Type {
        key: format!("decisions.{key}"),
        want,
        found: found.into(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionState {
    Holds,
    Broken,
    Unverifiable,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accepted {
    pub id: String,
    pub revision: u32,
    pub severity: String,
    /// True only after the caller checked a person-authored, unexpired waiver
    /// for the candidate version against the local decision ledger.
    pub waived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptVerdict {
    pub decision_id: String,
    pub revision: u32,
    pub state: DecisionState,
    pub verdict_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub schema_version: u32,
    pub project_id: String,
    pub version_id: String,
    pub tree_hash: String,
    pub derived_content_sha256: String,
    pub verdicts: Vec<ReceiptVerdict>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject {
    pub project_id: String,
    pub version_id: String,
    pub tree_hash: String,
    pub derived_content_sha256: String,
    pub accepted: Vec<Accepted>,
    pub receipt: Option<Receipt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateAction {
    None,
    Recompute,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateDecision {
    pub conclusion: Conclusion,
    pub reasons: Vec<Reason>,
    pub action: GateAction,
}

impl DecisionsPolicy {
    pub fn evaluate(&self, subject: &Subject) -> GateDecision {
        let mut judged = GateDecision {
            conclusion: Conclusion::Success,
            reasons: Vec::new(),
            action: GateAction::None,
        };
        if self.require_hold == RequireHold::None {
            return judged;
        }
        let Some(receipt) = &subject.receipt else {
            judged.add(self.unverifiable_reason(None, "exact-version decision receipt is missing"));
            return judged;
        };
        if receipt.schema_version != 1
            || receipt.project_id != subject.project_id
            || receipt.version_id != subject.version_id
            || receipt.tree_hash != subject.tree_hash
            || receipt.derived_content_sha256 != subject.derived_content_sha256
        {
            let detail = match self.on_stale {
                OnStale::Recompute => {
                    judged.action = GateAction::Recompute;
                    "decision receipt is stale; recompute it for this exact version and tree"
                }
                OnStale::Block => "decision receipt is stale for this exact version and tree",
            };
            judged.add(Reason {
                claim: None,
                rule: "decisions.on_stale",
                detail: detail.into(),
                conclusion: Conclusion::Failure,
            });
            return judged;
        }
        let mut accepted = BTreeMap::new();
        for decision in &subject.accepted {
            if !matches!(decision.severity.as_str(), "blocks" | "warns")
                || accepted
                    .insert((&decision.id, decision.revision), decision)
                    .is_some()
            {
                judged.add(integrity_reason("accepted decision set is invalid"));
                return judged;
            }
        }
        let mut verdicts = BTreeMap::new();
        for verdict in &receipt.verdicts {
            if verdict.verdict_id.is_empty()
                || verdicts
                    .insert((&verdict.decision_id, verdict.revision), verdict)
                    .is_some()
            {
                judged.add(integrity_reason(
                    "decision receipt has duplicate or invalid verdicts",
                ));
                return judged;
            }
        }
        if accepted.keys().collect::<Vec<_>>() != verdicts.keys().collect::<Vec<_>>() {
            judged.add(integrity_reason(
                "decision receipt does not match the accepted decision set and revisions",
            ));
            return judged;
        }
        for (key, decision) in accepted {
            if decision.waived
                || self.require_hold == RequireHold::Blocking && decision.severity == "warns"
            {
                continue;
            }
            let Some(verdict) = verdicts.get(&key) else {
                judged.add(integrity_reason("decision receipt is incomplete"));
                return judged;
            };
            match verdict.state {
                DecisionState::Holds | DecisionState::NotApplicable => {}
                DecisionState::Broken => judged.add(Reason {
                    claim: Some(decision.id.clone()),
                    rule: "decisions.require_hold",
                    detail: format!(
                        "accepted decision {} is broken on the exact version",
                        decision.id
                    ),
                    conclusion: Conclusion::Failure,
                }),
                DecisionState::Unverifiable => judged.add(self.unverifiable_reason(
                    Some(&decision.id),
                    "accepted decision could not be checked on the exact version",
                )),
            }
        }
        judged
    }

    fn unverifiable_reason(&self, id: Option<&str>, detail: &str) -> Reason {
        Reason {
            claim: id.map(str::to_string),
            rule: "decisions.on_unverifiable",
            detail: detail.into(),
            conclusion: match self.on_unverifiable {
                OnUnverifiable::Block => Conclusion::Failure,
                OnUnverifiable::Warn => Conclusion::Neutral,
            },
        }
    }
}

fn integrity_reason(detail: &str) -> Reason {
    Reason {
        claim: None,
        rule: "decisions.receipt_integrity",
        detail: detail.into(),
        conclusion: Conclusion::Failure,
    }
}

impl GateDecision {
    fn add(&mut self, reason: Reason) {
        self.conclusion = match (self.conclusion, reason.conclusion) {
            (Conclusion::Failure, _) | (_, Conclusion::Failure) => Conclusion::Failure,
            (Conclusion::Neutral, _) | (_, Conclusion::Neutral) => Conclusion::Neutral,
            _ => Conclusion::Success,
        };
        self.reasons.push(reason);
    }
}
