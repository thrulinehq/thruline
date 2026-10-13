//! Deterministic normalization for Codex rollout JSONL envelopes.
//!
//! This module deliberately stops before discovery, mirror sealing, authority
//! indexing, and ledger writes. In particular, [`CodexEmissionRole`] describes
//! precedence between Codex envelope families; it does not confer accepted-
//! generation or truth authority. A caller must resolve every
//! [`CodexEmissionRole::FallbackCandidate`] against its complete, disk-backed
//! session index before emitting append-only ledger events.

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::chain::{sha256_hex, to_hex};
use crate::model::{EventKind, NormalizedEvent};
use crate::Result;

const SOURCE_NAMESPACE: &str = "codex";

/// Immutable, same-line provenance supplied by the generic ingest owner.
///
/// `owner_session_id` is the privacy-safe, owner+source-scoped canonical
/// ledger session identity. `source_session_id` is the raw physical rollout
/// owner selected from the first authoritative `session_meta`; it is retained
/// only as line provenance and never carries cross-owner identity authority.
/// `file_id` must be a sanitized stable identity, not a private source path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodexLineContext<'a> {
    pub owner_session_id: &'a str,
    pub source_session_id: &'a str,
    pub file_id: &'a str,
    /// Affirmative turn identity from the most recent `turn_context`, or the
    /// explicit pre-turn segment before the first such boundary. Mirror
    /// reconciliation never crosses this scope on text equality alone.
    pub turn_scope: &'a str,
    pub turn_boundary_line: Option<u64>,
    pub turn_boundary_sha256: Option<&'a str>,
    pub line: u64,
    pub raw_sha256: &'a str,
}

/// How a mapped event participates in Codex's envelope-first reconciliation.
///
/// This is a normalization role only. A canonical item is still powerless
/// until the complete sealed generation transaction commits (A71).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexEmissionRole {
    /// A `response_item` (or another canonical top-level family). Text messages
    /// expose a key used to suppress an exact `event_msg` mirror.
    CanonicalItem { mirror_key: Option<String> },
    /// Event-only text may be emitted only after the session index proves that
    /// no canonical item has this key.
    FallbackCandidate { mirror_key: String },
    /// Counted provenance or telemetry. It has no language/evidence authority.
    Metadata,
}

/// One existing canonical event plus the Codex reconciliation role it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexMappedEvent {
    pub event: NormalizedEvent,
    pub emission: CodexEmissionRole,
}

/// Normalization result for one well-formed, nonblank Codex JSONL envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexNormalizedLine {
    pub envelope_type: String,
    pub item_type: Option<String>,
    /// An identity candidate from `session_meta.payload.id`. The stream owner
    /// decides whether it is the first authoritative value; this module never
    /// rebinds [`CodexLineContext::owner_session_id`].
    pub session_owner_hint: Option<String>,
    /// Always non-empty. Unknown families become one safe, counted `Meta`.
    pub events: Vec<CodexMappedEvent>,
}

/// Normalize one already-parsed Codex JSONL envelope.
///
/// The function is intentionally stateless. Mirror suppression and repeated-id
/// divergence require a complete per-session authority index and therefore
/// belong to the adapter's two-pass owner, not to a line mapper.
pub fn normalize_codex_line(
    context: CodexLineContext<'_>,
    envelope: &Value,
) -> Result<CodexNormalizedLine> {
    let (envelope_type, item_type) = codex_envelope_family(envelope);
    let payload = envelope.get("payload").unwrap_or(&Value::Null);
    let timestamp = string_field(envelope, "timestamp");
    let session_owner_hint = (envelope_type == "session_meta")
        .then(|| string_field(payload, "id"))
        .flatten();

    let mut mapper = LineMapper {
        context,
        timestamp,
        next_fanout: 0,
        events: Vec::new(),
    };

    match envelope_type.as_str() {
        "session_meta" => mapper.push_meta("codex.session_meta", payload)?,
        "turn_context" => mapper.push_meta("codex.turn_context", payload)?,
        "response_item" => normalize_response_item(&mut mapper, payload)?,
        "event_msg" => normalize_event_message(&mut mapper, payload)?,
        "compacted" => normalize_compacted(&mut mapper, payload)?,
        "world_state" => mapper.push_meta("codex.world_state", payload)?,
        "inter_agent_communication_metadata" => {
            mapper.push_meta("codex.inter_agent_communication_metadata", payload)?;
        }
        other => mapper.push_meta(&format!("codex.unknown.{other}"), payload)?,
    }

    // Every well-formed nonblank envelope receives an explicit disposition.
    if mapper.events.is_empty() {
        mapper.push_meta("codex.empty", payload)?;
    }

    Ok(CodexNormalizedLine {
        envelope_type,
        item_type,
        session_owner_hint,
        events: mapper.events,
    })
}

pub(crate) fn codex_envelope_family(envelope: &Value) -> (String, Option<String>) {
    let envelope_type = safe_token(envelope.get("type").and_then(Value::as_str));
    let item_type = envelope
        .get("payload")
        .and_then(|payload| payload.get("type"))
        .and_then(Value::as_str)
        .map(|value| safe_token(Some(value)));
    (envelope_type, item_type)
}

struct LineMapper<'a> {
    context: CodexLineContext<'a>,
    timestamp: Option<String>,
    next_fanout: usize,
    events: Vec<CodexMappedEvent>,
}

impl LineMapper<'_> {
    fn push(
        &mut self,
        mut event: NormalizedEvent,
        emission: CodexEmissionRole,
        native_id: Option<&str>,
        identity_payload: &Value,
    ) -> Result<()> {
        let fanout = self.next_fanout;
        self.next_fanout += 1;
        let native = native_id.map(str::to_owned).unwrap_or_else(|| {
            fallback_native(
                self.context,
                fanout,
                identity_payload.get("id").and_then(Value::as_str),
            )
        });
        let payload_hash = canonical_hash(identity_payload)?;
        let fanout = fanout.to_string();
        event.src_uuid = Some(namespace_hash(&[
            SOURCE_NAMESPACE,
            self.context.owner_session_id,
            &native,
            &payload_hash,
            &fanout,
        ]));
        event.ts = self.timestamp.clone();
        event.line_session_id = Some(self.context.source_session_id.to_string());
        event.source_turn_id =
            source_turn_identity(self.context.owner_session_id, self.context.turn_scope);
        event.source_turn_boundary_line = self.context.turn_boundary_line;
        event.source_turn_boundary_sha256 = self.context.turn_boundary_sha256.map(str::to_owned);
        event.source_message_key = match &emission {
            CodexEmissionRole::CanonicalItem {
                mirror_key: Some(key),
            } => Some(key.clone()),
            _ => None,
        };
        self.events.push(CodexMappedEvent { event, emission });
        Ok(())
    }

    fn push_meta(&mut self, token: &str, identity_payload: &Value) -> Result<()> {
        self.push_meta_with_native(token, None, identity_payload)
    }

    fn push_meta_with_native(
        &mut self,
        token: &str,
        native_id: Option<&str>,
        identity_payload: &Value,
    ) -> Result<()> {
        let mut event = NormalizedEvent::new(EventKind::Meta);
        event.text = Some(token.to_string());
        self.push(
            event,
            CodexEmissionRole::Metadata,
            native_id,
            identity_payload,
        )
    }
}

fn normalize_response_item(mapper: &mut LineMapper<'_>, payload: &Value) -> Result<()> {
    let item_type = safe_token(payload.get("type").and_then(Value::as_str));
    match item_type.as_str() {
        "message" => normalize_response_message(mapper, payload),
        "reasoning" => normalize_reasoning(mapper, payload),
        "function_call"
        | "custom_tool_call"
        | "tool_search_call"
        | "image_generation_call"
        | "web_search_call" => normalize_tool_call(mapper, payload, &item_type),
        "function_call_output" | "custom_tool_call_output" | "tool_search_output" => {
            normalize_tool_output(mapper, payload)
        }
        // Inter-agent prose is not support evidence without an affirmative
        // completion discriminator. Keep it counted and non-linguistic.
        "agent_message" => mapper.push_meta_with_native(
            "codex.response_item.agent_message",
            item_native_id(payload).as_deref(),
            payload,
        ),
        other => mapper.push_meta_with_native(
            &format!("codex.response_item.{other}"),
            item_native_id(payload).as_deref(),
            payload,
        ),
    }
}

fn normalize_response_message(mapper: &mut LineMapper<'_>, payload: &Value) -> Result<()> {
    let role = safe_token(payload.get("role").and_then(Value::as_str));
    let native_id = item_native_id(payload);
    let Some(content) = payload.get("content") else {
        return mapper.push_meta_with_native(
            "codex.response_item.message.missing_content",
            native_id.as_deref(),
            payload,
        );
    };

    if let Some(text) = content.as_str() {
        return push_message_text(mapper, &role, text, native_id.as_deref(), payload);
    }

    let Some(blocks) = content.as_array() else {
        return mapper.push_meta_with_native(
            "codex.response_item.message.invalid_content",
            native_id.as_deref(),
            payload,
        );
    };

    for block in blocks {
        let block_type = safe_token(block.get("type").and_then(Value::as_str));
        let compatible = matches!(
            (role.as_str(), block_type.as_str()),
            ("assistant", "output_text") | ("user", "input_text")
        );
        if compatible {
            if let Some(text) = block.get("text").and_then(Value::as_str) {
                push_message_text(mapper, &role, text, native_id.as_deref(), payload)?;
            } else {
                mapper.push_meta_with_native(
                    &format!("codex.response_item.message.{role}.{block_type}.missing_text"),
                    native_id.as_deref(),
                    payload,
                )?;
            }
        } else {
            // Input images/files and unknown blocks are visible receipts but
            // not language or evidence events in M4.R2.
            mapper.push_meta_with_native(
                &format!("codex.response_item.message.{role}.{block_type}"),
                native_id.as_deref(),
                payload,
            )?;
        }
    }
    Ok(())
}

fn push_message_text(
    mapper: &mut LineMapper<'_>,
    role: &str,
    text: &str,
    native_id: Option<&str>,
    identity_payload: &Value,
) -> Result<()> {
    let kind = match role {
        "assistant" => EventKind::AssistantText,
        "user" => EventKind::UserMsg,
        _ => {
            return mapper.push_meta_with_native(
                &format!("codex.response_item.message.{role}"),
                native_id,
                identity_payload,
            );
        }
    };
    let mut event = NormalizedEvent::new(kind);
    event.role = Some(role.to_string());
    event.text = Some(text.to_string());
    let mirror_key = message_mirror_key(
        mapper.context.owner_session_id,
        mapper.context.turn_scope,
        role,
        text,
    );
    mapper.push(
        event,
        CodexEmissionRole::CanonicalItem {
            mirror_key: Some(mirror_key),
        },
        native_id,
        identity_payload,
    )
}

fn normalize_reasoning(mapper: &mut LineMapper<'_>, payload: &Value) -> Result<()> {
    let native_id = item_native_id(payload);
    let before = mapper.events.len();
    match payload.get("summary") {
        Some(Value::String(text)) => {
            push_thinking(mapper, text, native_id.as_deref(), payload)?;
        }
        Some(Value::Array(parts)) => {
            for part in parts {
                if let Some(text) = summary_text(part) {
                    push_thinking(mapper, &text, native_id.as_deref(), payload)?;
                } else {
                    mapper.push_meta_with_native(
                        "codex.response_item.reasoning.summary",
                        native_id.as_deref(),
                        payload,
                    )?;
                }
            }
        }
        _ => {}
    }

    if mapper.events.len() == before {
        // Raw/encrypted hidden reasoning is never decoded or classified.
        mapper.push_meta_with_native(
            "codex.response_item.reasoning.hidden",
            native_id.as_deref(),
            payload,
        )?;
    }
    Ok(())
}

fn push_thinking(
    mapper: &mut LineMapper<'_>,
    text: &str,
    native_id: Option<&str>,
    identity_payload: &Value,
) -> Result<()> {
    let mut event = NormalizedEvent::new(EventKind::AssistantThinking);
    event.role = Some("assistant".to_string());
    event.text = Some(text.to_string());
    mapper.push(
        event,
        CodexEmissionRole::CanonicalItem { mirror_key: None },
        native_id,
        identity_payload,
    )
}

fn normalize_tool_call(
    mapper: &mut LineMapper<'_>,
    payload: &Value,
    item_type: &str,
) -> Result<()> {
    let fallback_name = match item_type {
        "tool_search_call" => "ToolSearch",
        "image_generation_call" => "ImageGeneration",
        "web_search_call" => "WebSearch",
        _ => "Tool",
    };
    let raw_name = payload
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(fallback_name);
    let tool_name = canonical_tool_name(raw_name);
    // A112: canonicalisation destroys the vendor's name for every mapped tool.
    // Preserve it when — and only when — it differs, so `None` means "nothing
    // was lost" rather than "nobody recorded it".
    let source_tool_name = (tool_name != raw_name).then(|| raw_name.to_string());
    let native_id = item_native_id(payload).unwrap_or_else(|| {
        fallback_native(
            mapper.context,
            mapper.next_fanout,
            payload.get("call_id").and_then(Value::as_str),
        )
    });
    let call_id = payload
        .get("call_id")
        .and_then(Value::as_str)
        .or_else(|| payload.get("id").and_then(Value::as_str))
        .map(str::to_owned)
        .unwrap_or_else(|| native_id.clone());
    let input = tool_input(payload, item_type, &tool_name)?;

    let mut event = NormalizedEvent::new(EventKind::ToolUse);
    event.tool_name = Some(tool_name);
    event.source_tool_name = source_tool_name;
    event.tool_use_id = Some(source_tool_identity(
        mapper.context.owner_session_id,
        &call_id,
    ));
    event.tool_input = Some(canonical_json(&input)?);
    mapper.push(
        event,
        CodexEmissionRole::CanonicalItem { mirror_key: None },
        Some(&native_id),
        payload,
    )
}

fn normalize_tool_output(mapper: &mut LineMapper<'_>, payload: &Value) -> Result<()> {
    let native_id = item_native_id(payload).unwrap_or_else(|| {
        fallback_native(
            mapper.context,
            mapper.next_fanout,
            payload.get("call_id").and_then(Value::as_str),
        )
    });
    let call_id = payload
        .get("call_id")
        .and_then(Value::as_str)
        .or_else(|| payload.get("id").and_then(Value::as_str))
        .map(str::to_owned)
        .unwrap_or_else(|| native_id.clone());
    let content = payload
        .get("output")
        .or_else(|| payload.get("result"))
        .or_else(|| payload.get("content"))
        .or_else(|| payload.get("tools"))
        .or_else(|| payload.get("execution"))
        .cloned()
        .unwrap_or(Value::Null);
    let explicit_exit_error = explicit_nonzero_exit(&content);
    let mut result = Map::new();
    result.insert("content".to_string(), content);

    let mut event = NormalizedEvent::new(EventKind::ToolResult);
    event.tool_use_id = Some(source_tool_identity(
        mapper.context.owner_session_id,
        &call_id,
    ));
    event.tool_result = Some(canonical_json(&Value::Object(result))?);
    event.is_error = payload
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || matches!(
            payload.get("status").and_then(Value::as_str),
            Some("failed" | "error")
        )
        || explicit_exit_error;
    mapper.push(
        event,
        CodexEmissionRole::CanonicalItem { mirror_key: None },
        Some(&native_id),
        payload,
    )
}

fn normalize_event_message(mapper: &mut LineMapper<'_>, payload: &Value) -> Result<()> {
    let event_type = safe_token(payload.get("type").and_then(Value::as_str));
    let (role, kind) = match event_type.as_str() {
        "agent_message" => ("assistant", EventKind::AssistantText),
        "user_message" => ("user", EventKind::UserMsg),
        _ => {
            // Lifecycle, usage, model/status, rollback, image/web end, MCP,
            // exec, patch, and subagent rows remain separate counted telemetry.
            return mapper.push_meta(&format!("codex.event_msg.{event_type}"), payload);
        }
    };

    let Some(text) = event_message_text(payload) else {
        return mapper.push_meta(
            &format!("codex.event_msg.{event_type}.missing_text"),
            payload,
        );
    };
    let mut event = NormalizedEvent::new(kind);
    event.role = Some(role.to_string());
    event.text = Some(text.clone());
    let mirror_key = message_mirror_key(
        mapper.context.owner_session_id,
        mapper.context.turn_scope,
        role,
        &text,
    );
    // Event-message identity always includes the manifest locator. It cannot
    // globally fold replayed content or collide with a response-item identity.
    let native = fallback_native(
        mapper.context,
        mapper.next_fanout,
        payload
            .get("id")
            .or_else(|| payload.get("item_id"))
            .and_then(Value::as_str),
    );
    mapper.push(
        event,
        CodexEmissionRole::FallbackCandidate { mirror_key },
        Some(&native),
        payload,
    )
}

fn normalize_compacted(mapper: &mut LineMapper<'_>, payload: &Value) -> Result<()> {
    let Some(text) = payload.get("message").and_then(message_value_text) else {
        return mapper.push_meta("codex.compacted.missing_message", payload);
    };
    let mut event = NormalizedEvent::new(EventKind::Summary);
    event.text = Some(text);
    // Replacement history intentionally remains raw provenance and is not
    // replayed into the current conversation.
    mapper.push(
        event,
        CodexEmissionRole::CanonicalItem { mirror_key: None },
        None,
        payload,
    )
}

fn tool_input(payload: &Value, item_type: &str, tool_name: &str) -> Result<Value> {
    let selected = match item_type {
        "function_call" => payload.get("arguments"),
        "custom_tool_call" => payload.get("input"),
        "tool_search_call" => payload
            .get("arguments")
            .or_else(|| payload.get("input"))
            .or_else(|| payload.get("query"))
            .or_else(|| payload.get("execution")),
        "image_generation_call" => payload.get("input").or_else(|| payload.get("prompt")),
        "web_search_call" => payload
            .get("action")
            .or_else(|| payload.get("input"))
            .or_else(|| payload.get("query")),
        _ => None,
    }
    .cloned()
    .unwrap_or_else(|| Value::Object(Map::new()));

    if let Value::String(raw) = selected {
        if item_type == "function_call" {
            if let Ok(parsed) = serde_json::from_str::<Value>(&raw) {
                return Ok(parsed);
            }
            return Ok(single_field_object("arguments", Value::String(raw)));
        }
        if item_type == "custom_tool_call" && tool_name == "Edit" {
            return Ok(single_field_object("patch", Value::String(raw)));
        }
        let key = match item_type {
            "tool_search_call" | "web_search_call" => "query",
            "image_generation_call" => "prompt",
            _ => "input",
        };
        return Ok(single_field_object(key, Value::String(raw)));
    }
    Ok(selected)
}

/// Closed-world source-status recognition. A non-zero exit flag is admitted
/// only when every explicitly recognized code in the same payload agrees.
fn explicit_nonzero_exit(value: &Value) -> bool {
    fn collect(value: &Value, out: &mut Vec<i64>) {
        match value {
            Value::Object(map) => {
                for (key, value) in map {
                    if matches!(key.as_str(), "exit_code" | "exitCode") {
                        if let Some(code) = value.as_i64() {
                            out.push(code);
                        }
                    }
                    collect(value, out);
                }
            }
            Value::Array(values) => {
                for value in values {
                    collect(value, out);
                }
            }
            Value::String(text) => {
                let lower = text.to_ascii_lowercase();
                for marker in [
                    "process exited with code",
                    "exit_code:",
                    "exit_code =",
                    "exit_code\":",
                    "exitcode:",
                    "exit code:",
                    "exit code =",
                ] {
                    let mut rest = lower.as_str();
                    while let Some(index) = rest.find(marker) {
                        rest = &rest[index + marker.len()..];
                        let number = rest.trim_start();
                        let end = number
                            .char_indices()
                            .take_while(|(i, ch)| ch.is_ascii_digit() || (*i == 0 && *ch == '-'))
                            .map(|(i, ch)| i + ch.len_utf8())
                            .last()
                            .unwrap_or(0);
                        if end > 0 {
                            if let Ok(code) = number[..end].parse::<i64>() {
                                out.push(code);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let mut codes = Vec::new();
    collect(value, &mut codes);
    !codes.is_empty()
        && codes.iter().all(|code| *code != 0)
        && codes.windows(2).all(|w| w[0] == w[1])
}

fn single_field_object(key: &str, value: Value) -> Value {
    let mut object = Map::new();
    object.insert(key.to_string(), value);
    Value::Object(object)
}

fn canonical_tool_name(name: &str) -> String {
    let token = name.trim().replace(['-', ' '], "_").to_ascii_lowercase();
    match token.as_str() {
        "bash" | "shell" | "shell_command" | "exec_command" | "terminal" => "Bash".to_string(),
        "apply_patch" | "patch" | "edit" | "edit_file" | "notebook_edit" => "Edit".to_string(),
        "write" | "write_file" => "Write".to_string(),
        "read" | "read_file" => "Read".to_string(),
        _ => name.to_string(),
    }
}

fn item_native_id(payload: &Value) -> Option<String> {
    payload
        .get("id")
        .and_then(Value::as_str)
        .or_else(|| payload.get("call_id").and_then(Value::as_str))
        .map(str::to_owned)
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn summary_text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| string_field(value, "text"))
        .or_else(|| string_field(value, "summary_text"))
}

fn event_message_text(payload: &Value) -> Option<String> {
    payload
        .get("message")
        .and_then(message_value_text)
        .or_else(|| payload.get("text").and_then(message_value_text))
}

fn message_value_text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.get("text").and_then(Value::as_str).map(str::to_owned))
        .or_else(|| {
            value
                .get("content")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
}

fn message_mirror_key(owner_session_id: &str, turn_scope: &str, role: &str, text: &str) -> String {
    namespace_hash(&[
        SOURCE_NAMESPACE,
        owner_session_id,
        "message_mirror",
        turn_scope,
        role,
        text,
    ])
}

/// Source-scoped identity used by both transcript ingest and M5 capture
/// delivery pairing. Native call ids are never stored bare.
pub fn source_tool_identity(owner_session_id: &str, call_id: &str) -> String {
    namespace_hash(&[SOURCE_NAMESPACE, owner_session_id, "tool_call", call_id])
}

fn turn_identity(owner_session_id: &str, turn_id: &str) -> String {
    namespace_hash(&[SOURCE_NAMESPACE, owner_session_id, "turn", turn_id])
}

/// Canonical source turn identity shared by transcript ingest and native
/// capture callbacks. A raw turn id never crosses into the accepted ledger.
pub fn source_turn_identity(owner_session_id: &str, turn_scope: &str) -> Option<String> {
    turn_scope
        .strip_prefix("turn:")
        .map(|turn| turn_identity(owner_session_id, turn))
}

fn fallback_native(
    context: CodexLineContext<'_>,
    fanout: usize,
    native_hint: Option<&str>,
) -> String {
    let hint = native_hint.unwrap_or("");
    let line = context.line.to_string();
    let fanout = fanout.to_string();
    namespace_hash(&[
        SOURCE_NAMESPACE,
        "fallback",
        context.file_id,
        &line,
        &fanout,
        context.raw_sha256,
        hint,
    ])
}

fn namespace_hash(fields: &[&str]) -> String {
    let mut hash = Sha256::new();
    for field in fields {
        hash.update((field.len() as u64).to_le_bytes());
        hash.update(field.as_bytes());
    }
    to_hex(&hash.finalize())
}

fn canonical_hash(value: &Value) -> Result<String> {
    Ok(sha256_hex(canonical_json(value)?.as_bytes()))
}

fn canonical_json(value: &Value) -> Result<String> {
    Ok(serde_json::to_string(&canonical_value(value))?)
}

fn canonical_value(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(canonical_value).collect()),
        Value::Object(object) => {
            let mut keys: Vec<&String> = object.keys().collect();
            keys.sort_unstable();
            let mut canonical = Map::new();
            for key in keys {
                if let Some(value) = object.get(key) {
                    canonical.insert(key.clone(), canonical_value(value));
                }
            }
            Value::Object(canonical)
        }
        scalar => scalar.clone(),
    }
}

fn safe_token(raw: Option<&str>) -> String {
    let Some(raw) = raw else {
        return "missing".to_string();
    };
    let mut safe = String::with_capacity(raw.len().min(64));
    for character in raw.chars().take(64) {
        if character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.') {
            safe.push(character);
        } else {
            safe.push('_');
        }
    }
    if safe.is_empty() {
        "empty".to_string()
    } else {
        safe
    }
}
