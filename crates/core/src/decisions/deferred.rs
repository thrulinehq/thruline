//! Person-accepted deferred decisions: closed conditions and typed actions.
use super::{
    checks::{CheckOutcome, VerdictState},
    params::CheckParams,
};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "marker", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeferredCondition {
    Tag { name: String, repository: String },
    PublishedRelease { repository: String, tag: String },
    Date { at_utc: String },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeferredDecisionParams {
    pub condition: DeferredCondition,
    pub action: Box<CheckParams>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DueState {
    Waiting,
    Due,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    Open,
    Closed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceReceipt {
    pub event_id: String,
    pub session_id: String,
    pub seq: i64,
    pub raw_file: String,
    pub raw_line: i64,
    pub raw_sha256: String,
    pub chain_hash: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeferredProjection {
    pub lifecycle: Lifecycle,
    pub due_state: DueState,
    pub whose_move: Option<String>,
    pub waits_on: DeferredCondition,
    pub sources: Vec<SourceReceipt>,
    pub missing: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fulfillment: Option<DeferredFulfillment>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeferredFulfillment {
    pub version_id: String,
    pub tree_hash: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeferredEvaluation {
    pub outcome: CheckOutcome,
    pub projection: DeferredProjection,
}
pub(crate) fn unavailable(
    params: &DeferredDecisionParams,
    tree: &str,
    reason: &str,
) -> DeferredEvaluation {
    DeferredEvaluation {
        outcome: CheckOutcome {
            state: VerdictState::Unverifiable,
            tree_hash: Some(tree.into()),
            reason: Some(reason.into()),
            checked_places: 0,
            findings: vec![],
            surfaces: vec![],
            visual: vec![],
            not_looked_at: vec![],
        },
        projection: DeferredProjection {
            lifecycle: Lifecycle::Open,
            due_state: DueState::Unknown,
            whose_move: None,
            waits_on: params.condition.clone(),
            sources: vec![],
            missing: vec![reason.into()],
            fulfillment: None,
        },
    }
}

impl DeferredDecisionParams {
    pub fn validate(&self) -> crate::error::Result<()> {
        use crate::error::CoreError;
        let valid = match &self.condition {
            DeferredCondition::Date { at_utc } => valid_utc(at_utc),
            DeferredCondition::Tag { name, repository } => {
                valid_tag(name)
                    && std::path::Path::new(repository).is_absolute()
                    && !repository.chars().any(char::is_control)
            }
            DeferredCondition::PublishedRelease { repository, tag } => {
                valid_tag(tag)
                    && repository.split('/').count() == 2
                    && repository.split('/').all(|part| {
                        !part.is_empty()
                            && part.bytes().all(|b| {
                                b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')
                            })
                    })
            }
        };
        if !valid {
            return Err(CoreError::Config(
                "deferred condition requires an exact tag, release repository, or UTC date".into(),
            ));
        }
        if matches!(
            self.action.as_ref(),
            CheckParams::Deferred(_) | CheckParams::VisualMatch(_)
        ) {
            return Err(CoreError::Config("deferred actions require a nonrecursive typed check; visual capture needs a separate accepted decision".into()));
        }
        super::params::parse_params(
            self.action.kind(),
            super::params::PARAMS_SCHEMA_VERSION,
            &self.action.json()?,
        )?;
        Ok(())
    }
}

pub(crate) fn valid_tag(tag: &str) -> bool {
    !tag.is_empty()
        && !tag.starts_with('-')
        && !tag.starts_with('/')
        && !tag.ends_with(['.', '/'])
        && !tag.contains("..")
        && !tag.contains("//")
        && tag
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'))
}

/// Strict UTC second precision. No wall clock or event timestamp participates.
pub(crate) fn valid_utc(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() != 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'Z'
    {
        return false;
    }
    let number = |from: usize, to: usize| {
        value.get(from..to).and_then(|s| {
            if s.bytes().all(|b| b.is_ascii_digit()) {
                s.parse::<u32>().ok()
            } else {
                None
            }
        })
    };
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        number(0, 4),
        number(5, 7),
        number(8, 10),
        number(11, 13),
        number(14, 16),
        number(17, 19),
    ) else {
        return false;
    };
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    year > 0 && day > 0 && day <= days && hour < 24 && minute < 60 && second < 60
}

/// Derive lifecycle without accepting or retiring anything. Waiting is not a
/// passing verdict. Only a due condition plus a holding typed action closes.
pub fn evaluate(
    params: &DeferredDecisionParams,
    tree: &str,
    mut projection: DeferredProjection,
    action: Option<CheckOutcome>,
) -> DeferredEvaluation {
    projection.waits_on = params.condition.clone();
    projection.lifecycle = Lifecycle::Open;
    projection.whose_move = None;
    match projection.due_state {
        DueState::Unknown => {
            let mut result = unavailable(
                params,
                tree,
                "The record has no complete, verified due-marker result.",
            );
            result.projection = projection;
            result
        }
        DueState::Waiting => {
            let mut result = unavailable(
                params,
                tree,
                "The recorded due marker has not been reached.",
            );
            result.outcome.state = VerdictState::NotApplicable;
            result.projection = projection;
            result
        }
        DueState::Due => {
            projection.whose_move = Some("you".into());
            let mut outcome = action.unwrap_or_else(|| {
                CheckOutcome::unavailable(
                    "The due action has no complete typed check receipt.",
                    Some(tree.into()),
                )
            });
            if outcome.tree_hash.as_deref() != Some(tree) {
                outcome = CheckOutcome::unavailable(
                    "The action check does not name the selected tree.",
                    Some(tree.into()),
                );
            }
            if outcome.state == VerdictState::Holds {
                projection.lifecycle = Lifecycle::Closed;
                projection.whose_move = None;
            } else if outcome.state == VerdictState::Unverifiable {
                if let Some(reason) = &outcome.reason {
                    projection.missing.push(reason.clone());
                }
            }
            DeferredEvaluation {
                outcome,
                projection,
            }
        }
    }
}
