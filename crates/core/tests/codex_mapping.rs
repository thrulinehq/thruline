use serde_json::{json, Value};
use thruline_core::adapter::stable_source_session_id;
use thruline_core::codex::{
    normalize_codex_line, CodexEmissionRole, CodexLineContext, CodexNormalizedLine,
};
use thruline_core::{EventKind, Source};

const RAW_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn normalize(owner: &str, value: Value) -> CodexNormalizedLine {
    normalize_codex_line(
        CodexLineContext {
            owner_session_id: owner,
            source_session_id: owner,
            file_id: "codex_file_synthetic",
            turn_scope: "turn:synthetic-turn",
            turn_boundary_line: Some(2),
            turn_boundary_sha256: Some(RAW_SHA),
            line: 7,
            raw_sha256: RAW_SHA,
        },
        &value,
    )
    .expect("synthetic Codex envelope must normalize")
}

fn assert_hash(value: &str) {
    assert_eq!(value.len(), 64);
    assert!(value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
}

#[test]
fn committed_synthetic_fixture_is_grammar_preserving_and_private_free() {
    let fixture = include_str!("fixtures/codex/representative.jsonl");
    let mut families = Vec::new();
    let mut kinds = Vec::new();
    for (index, raw) in fixture.lines().enumerate() {
        let value: Value = serde_json::from_str(raw).expect("synthetic fixture row is JSON");
        assert!(
            !raw.contains("/Users/") && !raw.contains(".codex/"),
            "committed fixture must never contain a private source path"
        );
        let mapped = normalize_codex_line(
            CodexLineContext {
                owner_session_id: "synthetic-session",
                source_session_id: "synthetic-session",
                file_id: "synthetic-fixture",
                turn_scope: "turn:synthetic-turn",
                turn_boundary_line: Some(2),
                turn_boundary_sha256: Some(RAW_SHA),
                line: index as u64 + 1,
                raw_sha256: RAW_SHA,
            },
            &value,
        )
        .expect("synthetic fixture envelope normalizes");
        families.push(mapped.envelope_type);
        kinds.extend(mapped.events.into_iter().map(|event| event.event.kind));
    }

    assert_eq!(fixture.lines().count(), 11);
    assert!(families.contains(&"session_meta".to_string()));
    assert!(families.contains(&"turn_context".to_string()));
    assert!(families.contains(&"response_item".to_string()));
    assert!(families.contains(&"event_msg".to_string()));
    assert!(families.contains(&"world_state".to_string()));
    assert!(families.contains(&"inter_agent_communication_metadata".to_string()));
    assert!(families.contains(&"synthetic_future_envelope".to_string()));
    assert!(kinds.contains(&EventKind::AssistantText));
    assert!(kinds.contains(&EventKind::AssistantThinking));
    assert!(kinds.contains(&EventKind::ToolUse));
    assert!(kinds.contains(&EventKind::ToolResult));
    assert!(kinds.contains(&EventKind::Meta));
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| **kind == EventKind::AssistantThinking)
            .count(),
        1,
        "encrypted-only reasoning remains metadata, not assistant prose"
    );
}

fn mirror_key(line: &CodexNormalizedLine) -> &str {
    match &line.events[0].emission {
        CodexEmissionRole::CanonicalItem {
            mirror_key: Some(key),
        }
        | CodexEmissionRole::FallbackCandidate { mirror_key: key } => key,
        other => panic!("expected message mirror role, got {other:?}"),
    }
}

fn normalize_for_owner(owner: &str, value: Value) -> CodexNormalizedLine {
    let raw_session = "shared-native-session";
    let canonical = stable_source_session_id(owner, Source::Codex, raw_session);
    normalize_codex_line(
        CodexLineContext {
            owner_session_id: &canonical,
            source_session_id: raw_session,
            file_id: &canonical,
            turn_scope: "turn:shared-native-turn",
            turn_boundary_line: Some(2),
            turn_boundary_sha256: Some(RAW_SHA),
            line: 7,
            raw_sha256: RAW_SHA,
        },
        &value,
    )
    .expect("owner-scoped synthetic envelope normalizes")
}

#[test]
fn identical_native_records_are_owner_scoped_at_every_normalized_identity() {
    let message = json!({
        "type":"response_item",
        "payload":{
            "type":"message",
            "id":"shared-item",
            "role":"assistant",
            "content":[{"type":"output_text","text":"Same synthetic words"}]
        }
    });
    let message_a = normalize_for_owner("owner-a", message.clone());
    let message_b = normalize_for_owner("owner-b", message);
    assert_ne!(
        message_a.events[0].event.src_uuid,
        message_b.events[0].event.src_uuid
    );
    assert_ne!(mirror_key(&message_a), mirror_key(&message_b));
    assert_ne!(
        message_a.events[0].event.source_turn_id,
        message_b.events[0].event.source_turn_id
    );
    assert_eq!(
        message_a.events[0].event.line_session_id.as_deref(),
        Some("shared-native-session")
    );
    assert_eq!(
        message_a.events[0].event.line_session_id, message_b.events[0].event.line_session_id,
        "raw source provenance is preserved without granting identity authority"
    );

    let tool = json!({
        "type":"response_item",
        "payload":{
            "type":"function_call",
            "id":"shared-tool-item",
            "call_id":"shared-call",
            "name":"synthetic_tool",
            "arguments":{"value":1}
        }
    });
    let tool_a = normalize_for_owner("owner-a", tool.clone());
    let tool_b = normalize_for_owner("owner-b", tool);
    assert_ne!(
        tool_a.events[0].event.tool_use_id,
        tool_b.events[0].event.tool_use_id
    );
    assert_ne!(
        tool_a.events[0].event.src_uuid,
        tool_b.events[0].event.src_uuid
    );
}

#[test]
fn metadata_compaction_inter_agent_and_unknown_are_explicitly_accounted() {
    let session = normalize(
        "physical-owner",
        json!({
            "timestamp": "2026-08-01T00:00:00Z",
            "type": "session_meta",
            "payload": {"id": "later-lineage-id", "cwd": "/synthetic/project"}
        }),
    );
    assert_eq!(
        session.session_owner_hint.as_deref(),
        Some("later-lineage-id")
    );
    assert_eq!(session.events.len(), 1);
    assert_eq!(session.events[0].event.kind, EventKind::Meta);
    assert_eq!(
        session.events[0].event.text.as_deref(),
        Some("codex.session_meta")
    );
    assert_eq!(
        session.events[0].event.line_session_id.as_deref(),
        Some("physical-owner"),
        "a later session_meta hint must not rebind the physical owner"
    );
    assert_eq!(
        session.events[0].event.ts.as_deref(),
        Some("2026-08-01T00:00:00Z")
    );

    for (envelope, token) in [
        (
            json!({"type":"turn_context","payload":{"cwd":"/must/not/enrich","branch":"other"}}),
            "codex.turn_context",
        ),
        (
            json!({"type":"world_state","payload":{"status":"running"}}),
            "codex.world_state",
        ),
        (
            json!({"type":"inter_agent_communication_metadata","payload":{"agent_id":"a1","message":"not evidence"}}),
            "codex.inter_agent_communication_metadata",
        ),
    ] {
        let mapped = normalize("physical-owner", envelope);
        assert_eq!(mapped.events.len(), 1);
        assert_eq!(mapped.events[0].event.kind, EventKind::Meta);
        assert_eq!(mapped.events[0].event.text.as_deref(), Some(token));
        assert_eq!(mapped.events[0].event.tool_input, None);
        assert_eq!(mapped.events[0].emission, CodexEmissionRole::Metadata);
    }

    let compacted = normalize(
        "physical-owner",
        json!({
            "type":"compacted",
            "payload":{
                "message":"Synthetic compact summary",
                "replacement_history":[{"message":"must not replay"}]
            }
        }),
    );
    assert_eq!(compacted.events.len(), 1);
    assert_eq!(compacted.events[0].event.kind, EventKind::Summary);
    assert_eq!(
        compacted.events[0].event.text.as_deref(),
        Some("Synthetic compact summary")
    );

    let unknown = normalize(
        "physical-owner",
        json!({
            "type":"future family/one",
            "payload":{"private_payload":"must not enter the safe Meta token"}
        }),
    );
    assert_eq!(unknown.envelope_type, "future_family_one");
    assert_eq!(unknown.events.len(), 1);
    assert_eq!(unknown.events[0].event.kind, EventKind::Meta);
    assert_eq!(
        unknown.events[0].event.text.as_deref(),
        Some("codex.unknown.future_family_one")
    );
    assert!(!unknown.events[0]
        .event
        .text
        .as_deref()
        .unwrap_or_default()
        .contains("private_payload"));
}

#[test]
fn response_messages_reasoning_and_event_fallbacks_preserve_precedence() {
    let assistant = normalize(
        "owner-a",
        json!({
            "type":"response_item",
            "payload":{
                "type":"message",
                "id":"item-a",
                "role":"assistant",
                "content":[{"type":"output_text","text":"Synthetic answer"}]
            }
        }),
    );
    assert_eq!(assistant.events.len(), 1);
    assert_eq!(assistant.events[0].event.kind, EventKind::AssistantText);
    assert_eq!(assistant.events[0].event.role.as_deref(), Some("assistant"));
    assert_eq!(
        assistant.events[0].event.text.as_deref(),
        Some("Synthetic answer")
    );

    let assistant_mirror = normalize(
        "owner-a",
        json!({
            "type":"event_msg",
            "payload":{"type":"agent_message","message":"Synthetic answer"}
        }),
    );
    assert_eq!(
        assistant_mirror.events[0].event.kind,
        EventKind::AssistantText
    );
    assert_eq!(mirror_key(&assistant), mirror_key(&assistant_mirror));
    assert!(matches!(
        assistant_mirror.events[0].emission,
        CodexEmissionRole::FallbackCandidate { .. }
    ));
    assert_ne!(
        assistant.events[0].event.src_uuid, assistant_mirror.events[0].event.src_uuid,
        "a fallback locator must not collide with response-item identity"
    );

    let user = normalize(
        "owner-a",
        json!({
            "type":"response_item",
            "payload":{
                "type":"message",
                "id":"item-u",
                "role":"user",
                "content":[
                    {"type":"input_text","text":"Synthetic question"},
                    {"type":"input_image","image_url":"data:image/png;base64,synthetic"}
                ]
            }
        }),
    );
    assert_eq!(user.events.len(), 2);
    assert_eq!(user.events[0].event.kind, EventKind::UserMsg);
    assert_eq!(user.events[1].event.kind, EventKind::Meta);
    assert_eq!(
        user.events[1].event.text.as_deref(),
        Some("codex.response_item.message.user.input_image")
    );

    let user_mirror = normalize(
        "owner-a",
        json!({
            "type":"event_msg",
            "payload":{"type":"user_message","message":"Synthetic question"}
        }),
    );
    assert_eq!(mirror_key(&user), mirror_key(&user_mirror));

    let reasoning = normalize(
        "owner-a",
        json!({
            "type":"response_item",
            "payload":{
                "type":"reasoning",
                "id":"reason-a",
                "summary":[{"type":"summary_text","text":"Visible reasoning summary"}],
                "encrypted_content":"must never be decoded"
            }
        }),
    );
    assert_eq!(reasoning.events.len(), 1);
    assert_eq!(reasoning.events[0].event.kind, EventKind::AssistantThinking);
    assert_eq!(
        reasoning.events[0].event.text.as_deref(),
        Some("Visible reasoning summary")
    );

    let hidden_reasoning = normalize(
        "owner-a",
        json!({
            "type":"response_item",
            "payload":{
                "type":"reasoning",
                "id":"reason-hidden",
                "encrypted_content":"must never be decoded"
            }
        }),
    );
    assert_eq!(hidden_reasoning.events[0].event.kind, EventKind::Meta);
    assert_eq!(
        hidden_reasoning.events[0].event.text.as_deref(),
        Some("codex.response_item.reasoning.hidden")
    );
}

#[test]
fn native_item_fanout_has_distinct_slots_and_preserves_turn_scope_without_a_parent() {
    let mapped = normalize(
        "owner-fanout",
        json!({
            "type":"response_item",
            "payload":{
                "type":"message",
                "id":"multi-block",
                "role":"assistant",
                "content":[
                    {"type":"output_text","text":"First block"},
                    {"type":"output_text","text":"Second block"}
                ]
            }
        }),
    );
    assert_eq!(mapped.events.len(), 2);
    let first = &mapped.events[0].event;
    let second = &mapped.events[1].event;
    assert_ne!(
        first.src_uuid, second.src_uuid,
        "fanout slots must be unique"
    );
    assert_ne!(
        first.source_message_key, second.source_message_key,
        "different exact text blocks are different mirror slots"
    );
    assert_eq!(first.source_turn_id, second.source_turn_id);
    assert_eq!(first.source_turn_boundary_line, Some(2));
    assert_eq!(first.source_turn_boundary_sha256.as_deref(), Some(RAW_SHA));
    assert_eq!(
        first.parent_uuid, None,
        "a turn boundary is not a parent edge"
    );
    assert_eq!(second.parent_uuid, None);
}

#[test]
fn function_custom_search_image_and_web_items_map_without_bare_ids() {
    let function = normalize(
        "owner-tools",
        json!({
            "type":"response_item",
            "payload":{
                "type":"function_call",
                "id":"item-call",
                "call_id":"call-1",
                "name":"shell_command",
                "arguments":"{\"z\":1,\"command\":\"cargo test\"}"
            }
        }),
    );
    let function_event = &function.events[0].event;
    assert_eq!(function_event.kind, EventKind::ToolUse);
    assert_eq!(function_event.tool_name.as_deref(), Some("Bash"));
    assert_eq!(
        function_event.tool_input.as_deref(),
        Some("{\"command\":\"cargo test\",\"z\":1}")
    );
    let function_tool_id = function_event
        .tool_use_id
        .as_deref()
        .expect("function call has a namespaced tool id");
    assert_hash(function_tool_id);
    assert_ne!(function_tool_id, "call-1");

    let function_output = normalize(
        "owner-tools",
        json!({
            "type":"response_item",
            "payload":{
                "type":"function_call_output",
                "call_id":"call-1",
                "output":{"stdout":"ok","exit_code":0}
            }
        }),
    );
    assert_eq!(function_output.events[0].event.kind, EventKind::ToolResult);
    assert_eq!(
        function_output.events[0].event.tool_use_id.as_deref(),
        Some(function_tool_id)
    );
    assert_eq!(
        function_output.events[0].event.tool_result.as_deref(),
        Some("{\"content\":{\"exit_code\":0,\"stdout\":\"ok\"}}")
    );

    let custom = normalize(
        "owner-tools",
        json!({
            "type":"response_item",
            "payload":{
                "type":"custom_tool_call",
                "call_id":"patch-1",
                "name":"apply_patch",
                "input":"*** synthetic patch ***"
            }
        }),
    );
    assert_eq!(custom.events[0].event.tool_name.as_deref(), Some("Edit"));
    assert_eq!(
        custom.events[0].event.tool_input.as_deref(),
        Some("{\"patch\":\"*** synthetic patch ***\"}")
    );
    let custom_output = normalize(
        "owner-tools",
        json!({
            "type":"response_item",
            "payload":{
                "type":"custom_tool_call_output",
                "call_id":"patch-1",
                "output":"Done"
            }
        }),
    );
    assert_eq!(
        custom.events[0].event.tool_use_id,
        custom_output.events[0].event.tool_use_id
    );

    let search = normalize(
        "owner-tools",
        json!({
            "type":"response_item",
            "payload":{
                "type":"tool_search_call",
                "call_id":"search-1",
                "query":"synthetic capability"
            }
        }),
    );
    assert_eq!(
        search.events[0].event.tool_name.as_deref(),
        Some("ToolSearch")
    );
    assert_eq!(
        search.events[0].event.tool_input.as_deref(),
        Some("{\"query\":\"synthetic capability\"}")
    );
    let search_output = normalize(
        "owner-tools",
        json!({
            "type":"response_item",
            "payload":{
                "type":"tool_search_output",
                "call_id":"search-1",
                "output":[{"name":"synthetic_tool"}]
            }
        }),
    );
    assert_eq!(
        search.events[0].event.tool_use_id,
        search_output.events[0].event.tool_use_id
    );

    let image = normalize(
        "owner-tools",
        json!({
            "type":"response_item",
            "payload":{
                "type":"image_generation_call",
                "id":"image-1",
                "prompt":"synthetic diagram"
            }
        }),
    );
    assert_eq!(
        image.events[0].event.tool_name.as_deref(),
        Some("ImageGeneration")
    );
    assert_eq!(
        image.events[0].event.tool_input.as_deref(),
        Some("{\"prompt\":\"synthetic diagram\"}")
    );

    let web = normalize(
        "owner-tools",
        json!({
            "type":"response_item",
            "payload":{
                "type":"web_search_call",
                "id":"web-1",
                "action":{"type":"search","query":"synthetic query"}
            }
        }),
    );
    assert_eq!(web.events[0].event.tool_name.as_deref(), Some("WebSearch"));
    assert_eq!(
        web.events[0].event.tool_input.as_deref(),
        Some("{\"query\":\"synthetic query\",\"type\":\"search\"}")
    );

    let generic = normalize(
        "owner-tools",
        json!({
            "type":"response_item",
            "payload":{
                "type":"function_call",
                "call_id":"generic-1",
                "name":"vendor_future_tool",
                "arguments":"{\"value\":2}"
            }
        }),
    );
    assert_eq!(
        generic.events[0].event.tool_name.as_deref(),
        Some("vendor_future_tool"),
        "unknown tool names remain generic rather than borrowing semantics"
    );
}

#[test]
fn lifecycle_outputs_and_inter_agent_response_items_remain_metadata() {
    for event_type in [
        "exec_command_end",
        "patch_apply_end",
        "mcp_tool_call_end",
        "sub_agent_activity",
        "task_complete",
        "token_count",
        "turn_aborted",
        "image_generation_end",
        "web_search_end",
    ] {
        let mapped = normalize(
            "owner-telemetry",
            json!({
                "type":"event_msg",
                "payload":{"type":event_type,"call_id":"telemetry-1","output":"not a second action"}
            }),
        );
        assert_eq!(mapped.events.len(), 1, "{event_type}");
        assert_eq!(mapped.events[0].event.kind, EventKind::Meta, "{event_type}");
        assert_eq!(mapped.events[0].emission, CodexEmissionRole::Metadata);
        assert_eq!(
            mapped.events[0].event.text.as_deref(),
            Some(format!("codex.event_msg.{event_type}").as_str())
        );
    }

    let inter_agent = normalize(
        "owner-telemetry",
        json!({
            "type":"response_item",
            "payload":{"type":"agent_message","id":"agent-1","message":"synthetic report"}
        }),
    );
    assert_eq!(inter_agent.events[0].event.kind, EventKind::Meta);
    assert_eq!(
        inter_agent.events[0].event.text.as_deref(),
        Some("codex.response_item.agent_message")
    );
}

#[test]
fn event_ids_are_canonical_owner_scoped_and_payload_sensitive() {
    let first: Value = serde_json::from_str(
        r#"{"type":"response_item","payload":{"role":"assistant","content":[{"text":"Stable text","type":"output_text"}],"id":"native-1","type":"message"}}"#,
    )
    .expect("valid synthetic json");
    let reordered: Value = serde_json::from_str(
        r#"{"payload":{"type":"message","id":"native-1","content":[{"type":"output_text","text":"Stable text"}],"role":"assistant"},"type":"response_item"}"#,
    )
    .expect("valid synthetic json");

    let a = normalize("owner-a", first);
    let b = normalize("owner-a", reordered);
    let owner_changed = normalize(
        "owner-b",
        json!({
            "type":"response_item",
            "payload":{"type":"message","id":"native-1","role":"assistant","content":[{"type":"output_text","text":"Stable text"}]}
        }),
    );
    let native_changed = normalize(
        "owner-a",
        json!({
            "type":"response_item",
            "payload":{"type":"message","id":"native-2","role":"assistant","content":[{"type":"output_text","text":"Stable text"}]}
        }),
    );
    let payload_changed = normalize(
        "owner-a",
        json!({
            "type":"response_item",
            "payload":{"type":"message","id":"native-1","role":"assistant","content":[{"type":"output_text","text":"Changed text"}]}
        }),
    );

    let id = a.events[0]
        .event
        .src_uuid
        .as_deref()
        .expect("Codex event has a deterministic source id");
    assert_hash(id);
    assert_eq!(a.events[0].event.src_uuid, b.events[0].event.src_uuid);
    assert_ne!(
        a.events[0].event.src_uuid,
        owner_changed.events[0].event.src_uuid
    );
    assert_ne!(
        a.events[0].event.src_uuid,
        native_changed.events[0].event.src_uuid
    );
    assert_ne!(
        a.events[0].event.src_uuid,
        payload_changed.events[0].event.src_uuid
    );
    assert_ne!(id, "native-1", "native provider ids are never stored bare");
}
