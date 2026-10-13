//! Closed deferred clauses. The condition and action are exact source slices.
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeferredClause {
    pub condition: String,
    pub action: String,
    pub condition_start: usize,
    pub condition_end: usize,
    pub action_start: usize,
    pub action_end: usize,
}

/// Ambiguous "later" without a named condition stays a Note in the caller.
pub fn read(text: &str) -> Option<DeferredClause> {
    let lower = text.to_ascii_lowercase();
    let prefix = if lower.starts_with("after ") {
        6
    } else if lower.starts_with("once ") {
        5
    } else {
        return None;
    };
    let mut quoted = false;
    let mut backtick = false;
    let mut comma = None;
    for (at, ch) in text[prefix..].char_indices() {
        match ch {
            '`' => backtick = !backtick,
            '"' if !backtick => quoted = !quoted,
            ',' if !quoted && !backtick => {
                comma = Some(prefix + at);
                break;
            }
            _ => {}
        }
    }
    let comma = comma?;
    let condition = text[prefix..comma].trim();
    let action = text[comma + 1..].trim();
    let marker = condition.to_ascii_lowercase();
    if condition.is_empty()
        || action.is_empty()
        || !(marker.starts_with("tag ")
            || marker.starts_with("date ")
            || marker.starts_with("published release ")
            || marker == "publish")
    {
        return None;
    }
    let imperative = action.to_ascii_lowercase();
    if ![
        "keep ",
        "remove ",
        "merge ",
        "sign ",
        "run ",
        "must pass ",
        "add ",
        "publish ",
    ]
    .iter()
    .any(|verb| imperative.starts_with(verb))
    {
        return None;
    }
    let condition_start =
        prefix + (text[prefix..comma].len() - text[prefix..comma].trim_start().len());
    let action_start = comma + 1 + (text[comma + 1..].len() - text[comma + 1..].trim_start().len());
    Some(DeferredClause {
        condition: condition.into(),
        action: action.into(),
        condition_start,
        condition_end: condition_start + condition.len(),
        action_start,
        action_end: action_start + action.len(),
    })
}

/// Parameters remain suggestions. A missing exact marker or unsupported
/// action returns None so the person can supply the closed check explicitly.
pub fn suggest_params(
    clause: &DeferredClause,
    repository: &str,
) -> Option<super::deferred::DeferredDecisionParams> {
    use super::{
        deferred::{DeferredCondition, DeferredDecisionParams},
        params::{CheckParams, CommandPassesParams, PathPresentParams},
    };
    let lower = clause.condition.to_ascii_lowercase();
    let condition = if lower.starts_with("tag ") {
        DeferredCondition::Tag {
            name: clause.condition[4..].to_string(),
            repository: repository.into(),
        }
    } else if lower.starts_with("date ") {
        DeferredCondition::Date {
            at_utc: clause.condition[5..].to_string(),
        }
    } else if lower.starts_with("published release ") {
        let rest = &clause.condition[18..];
        let (tag, repo) = rest.split_once(" in ")?;
        DeferredCondition::PublishedRelease {
            repository: repo.into(),
            tag: tag.into(),
        }
    } else {
        return None;
    };
    let action_lower = clause.action.to_ascii_lowercase();
    let action = if action_lower.starts_with("keep ") {
        let path = clause.action[5..].trim_matches('`');
        CheckParams::PathPresent(PathPresentParams {
            path: path.into(),
            sha256: None,
        })
    } else if action_lower.starts_with("run `") && clause.action.ends_with('`') {
        CheckParams::CommandPasses(CommandPassesParams {
            command: clause.action[5..clause.action.len() - 1].into(),
            is_test: false,
            min_passed: 0,
        })
    } else {
        return None;
    };
    let params = DeferredDecisionParams {
        condition,
        action: Box::new(action),
    };
    params.validate().ok()?;
    Some(params)
}
