//! §10 question 2 — why it stopped.

use serde::{Deserialize, Serialize};

/// The classes §10 asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReasonKind {
    AgentFinished,
    HookBlocked,
    UserInterrupt,
    Error,
    PermissionDenied,
    ContextLimit,
    /// Ran to the end of the record with no `stop` event at all — the session
    /// is still open, or the hook never fired.
    NoStopRecorded,
    Unknown,
}

impl StopReasonKind {
    pub fn as_str(self) -> &'static str {
        match self {
            StopReasonKind::AgentFinished => "agent_finished",
            StopReasonKind::HookBlocked => "hook_blocked",
            StopReasonKind::UserInterrupt => "user_interrupt",
            StopReasonKind::Error => "error",
            StopReasonKind::PermissionDenied => "permission_denied",
            StopReasonKind::ContextLimit => "context_limit",
            StopReasonKind::NoStopRecorded => "no_stop_recorded",
            StopReasonKind::Unknown => "unknown",
        }
    }
}

/// Why it stopped, with the evidence that says so.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopClassification {
    pub kind: StopReasonKind,
    /// One sentence a human reads.
    pub summary: String,
    /// The last three events before the stop, as §10 requires.
    pub last_events: Vec<String>,
    /// Background work the session was waiting on, if the harness said so.
    pub background_tasks: Vec<String>,
}

/// One event, reduced to what the classifier reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventView {
    pub seq: u64,
    pub kind: String,
    pub summary: String,
    /// `stop.reason` when this is a stop event.
    pub stop_reason: Option<String>,
    /// A `tool_result` that failed, with its excerpt.
    pub failed_tool: Option<String>,
}

/// Classify the end of a session.
pub fn classify_stop(events: &[EventView]) -> StopClassification {
    let last_events: Vec<String> = events
        .iter()
        .rev()
        .filter(|e| e.kind != "stop" && e.kind != "session_end")
        .take(3)
        .map(|e| e.summary.clone())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    let Some(stop) = events.iter().rev().find(|e| e.kind == "stop") else {
        return StopClassification {
            kind: StopReasonKind::NoStopRecorded,
            summary: "no `stop` event is in the record: the session is still open, or the \
                      Stop hook never fired"
                .to_string(),
            last_events,
            background_tasks: Vec::new(),
        };
    };

    let kind = match stop.stop_reason.as_deref() {
        Some("agent_finished") => StopReasonKind::AgentFinished,
        Some("hook_blocked") => StopReasonKind::HookBlocked,
        Some("user_interrupt") => StopReasonKind::UserInterrupt,
        Some("error") => StopReasonKind::Error,
        Some("permission_denied") => StopReasonKind::PermissionDenied,
        Some("context_limit") => StopReasonKind::ContextLimit,
        _ => StopReasonKind::Unknown,
    };

    // A finished-looking stop whose last observable act was a failing tool is
    // reported as an error, because "finished" is the harness's word for "the
    // turn ended" and not for "the work succeeded".
    let (kind, summary) = match kind {
        StopReasonKind::AgentFinished => {
            match events
                .iter()
                .rev()
                .take(4)
                .find_map(|e| e.failed_tool.clone())
            {
                Some(detail) => (
                    StopReasonKind::Error,
                    format!("the turn ended normally, but the last thing it did failed: {detail}"),
                ),
                None => (
                    StopReasonKind::AgentFinished,
                    "the agent finished its turn".to_string(),
                ),
            }
        }
        StopReasonKind::HookBlocked => (
            kind,
            "a Stop hook blocked the turn and sent the agent back to work".to_string(),
        ),
        StopReasonKind::UserInterrupt => (kind, "the user interrupted it".to_string()),
        StopReasonKind::Error => (kind, "it stopped on an error".to_string()),
        StopReasonKind::PermissionDenied => {
            (kind, "it stopped after a tool call was denied".to_string())
        }
        StopReasonKind::ContextLimit => (kind, "it stopped at the context limit".to_string()),
        other => (other, "the record does not say why it stopped".to_string()),
    };

    StopClassification {
        kind,
        summary,
        last_events,
        background_tasks: Vec::new(),
    }
}
