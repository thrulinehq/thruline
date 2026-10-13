use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// §7 `events.kind` domain.
///
/// The string forms below are the DDL `CHECK` values and part of the on-disk
/// contract — changing one is a migration, not a rename.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    UserMsg,
    AssistantText,
    AssistantThinking,
    ToolUse,
    ToolResult,
    /// Reachable only if a future CLI emits `type:"summary"`; no observed
    /// version does (see §8). Kept so the DDL domain stays complete.
    Summary,
    /// Anything the mapping does not recognise. The raw line is preserved and
    /// the occurrence is counted — never dropped, never fatal.
    Meta,
}

impl EventKind {
    pub const ALL: [EventKind; 7] = [
        EventKind::UserMsg,
        EventKind::AssistantText,
        EventKind::AssistantThinking,
        EventKind::ToolUse,
        EventKind::ToolResult,
        EventKind::Summary,
        EventKind::Meta,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::UserMsg => "user_msg",
            EventKind::AssistantText => "assistant_text",
            EventKind::AssistantThinking => "assistant_thinking",
            EventKind::ToolUse => "tool_use",
            EventKind::ToolResult => "tool_result",
            EventKind::Summary => "summary",
            EventKind::Meta => "meta",
        }
    }
}

impl fmt::Display for EventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for EventKind {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user_msg" => Ok(EventKind::UserMsg),
            "assistant_text" => Ok(EventKind::AssistantText),
            "assistant_thinking" => Ok(EventKind::AssistantThinking),
            "tool_use" => Ok(EventKind::ToolUse),
            "tool_result" => Ok(EventKind::ToolResult),
            "summary" => Ok(EventKind::Summary),
            "meta" => Ok(EventKind::Meta),
            other => Err(CoreError::UnknownEventKind(other.to_string())),
        }
    }
}

/// §7 `projects.source` domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    ClaudeCode,
    Codex,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::ClaudeCode => "claude_code",
            Source::Codex => "codex",
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A discovered project: one munged transcript directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRef {
    /// Deterministic and stable across re-ingest — the munged directory name.
    /// Chain genesis derives from this, so it must never be random.
    pub id: String,
    pub root_path: String,
    pub source: Source,
    pub display_name: Option<String>,
}

/// A discovered session: one `.jsonl` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRef {
    pub id: String,
    pub project_id: String,
    pub file_path: String,
    pub started_at: Option<String>,
    /// Every raw line of the file, malformed and blank included.
    pub raw_line_count: u64,
    /// Digest over all raw lines in order — tamper evidence for lines that
    /// produce no events (§7).
    pub raw_digest: Option<String>,
}

/// Where an event came from. All events fanned out of one JSONL line share
/// this exactly — same file, same line number, same line hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawLineRef {
    pub file: String,
    /// 1-based, matching what an editor shows.
    pub line: u64,
    /// SHA-256 of the raw line bytes, excluding the line terminator.
    pub sha256: String,
}

/// Parser output: one normalized event, before it enters the ledger.
///
/// Deliberately carries no `id`, `seq`, or chain fields — those are assigned by
/// the ledger, which owns ordering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedEvent {
    pub kind: EventKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The CANONICAL tool name (`Bash`, `Edit`, …).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// The vendor's RAW tool name, preserved when canonicalisation would
    /// otherwise destroy it (A112: `exec_command`, `shell_command`, `bash`,
    /// `shell`, `terminal` all store as `Bash`, and `apply_patch` as `Edit`).
    ///
    /// `None` means the raw name and the canonical name are the same, so there
    /// was nothing to preserve — read through `raw_tool_name`, never directly.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    /// Serialized JSON of the tool_use `input` object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<String>,
    /// Serialized tool_result content, plus the line's top-level
    /// `toolUseResult` when present (§8: it is a sibling of `message`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_result: Option<String>,
    pub is_error: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src_uuid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_uuid: Option<String>,
    pub is_sidechain: bool,
    /// Privacy-safe source-native turn identity when the adapter observes an
    /// affirmative turn boundary. This is scope metadata, not a parent edge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_turn_boundary_line: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_turn_boundary_sha256: Option<String>,
    /// Privacy-safe exact message-equivalence key for an authoritative source
    /// message slot. Fallback observations never set this field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_message_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<String>,
    /// The per-line `sessionId` from the JSONL, kept as metadata alongside the
    /// file-stem-derived `session_id`. Differs from it on a resumed session's
    /// copied prefix, which is what makes resume lineage queryable (§7).
    /// Chain-neutral: the chain hashes the raw line, not this field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_session_id: Option<String>,
}

impl NormalizedEvent {
    pub fn new(kind: EventKind) -> Self {
        NormalizedEvent {
            kind,
            role: None,
            text: None,
            tool_name: None,
            source_tool_name: None,
            tool_use_id: None,
            tool_input: None,
            tool_result: None,
            is_error: false,
            src_uuid: None,
            parent_uuid: None,
            is_sidechain: false,
            source_turn_id: None,
            source_turn_boundary_line: None,
            source_turn_boundary_sha256: None,
            source_message_key: None,
            ts: None,
            line_session_id: None,
        }
    }
}

/// A row of §7 `events`: a normalized event placed in the chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerEvent {
    /// ULID. Identity only — never an ordering or hashing input.
    pub id: String,
    pub project_id: String,
    pub session_id: String,
    /// Per-session monotonic, assigned in event order (see fan-out, §8).
    pub seq: i64,
    pub raw: RawLineRef,
    pub event: NormalizedEvent,
    pub prev_hash: String,
    pub chain_hash: String,
}

/// Why a raw line produced no events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MalformedReason {
    InvalidJson(String),
    InvalidUtf8,
    /// Final line cut mid-object — an interrupted session, not corruption.
    TruncatedFinalLine,
}

impl fmt::Display for MalformedReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MalformedReason::InvalidJson(m) => write!(f, "invalid JSON: {m}"),
            MalformedReason::InvalidUtf8 => f.write_str("invalid UTF-8 bytes"),
            MalformedReason::TruncatedFinalLine => {
                f.write_str("truncated final line (interrupted session)")
            }
        }
    }
}

/// One skipped line, reported with a `file:line` receipt. Never swallowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MalformedLine {
    pub file: String,
    pub line: u64,
    pub reason: MalformedReason,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_kind_strings_round_trip() {
        for kind in EventKind::ALL {
            let parsed: EventKind = kind
                .as_str()
                .parse()
                .expect("every ALL variant must parse back");
            assert_eq!(kind, parsed, "round trip failed for {kind}");
        }
    }

    #[test]
    fn unknown_event_kind_is_an_error_not_a_panic() {
        let err = "not_a_kind".parse::<EventKind>().unwrap_err();
        assert!(matches!(err, CoreError::UnknownEventKind(k) if k == "not_a_kind"));
    }

    #[test]
    fn event_kind_strings_match_the_ddl_check_domain() {
        // §7: CHECK(kind IN (...)). Drift here silently breaks inserts.
        let expected = [
            "user_msg",
            "assistant_text",
            "assistant_thinking",
            "tool_use",
            "tool_result",
            "summary",
            "meta",
        ];
        let actual: Vec<&str> = EventKind::ALL.iter().map(|k| k.as_str()).collect();
        assert_eq!(actual, expected);
    }
}
