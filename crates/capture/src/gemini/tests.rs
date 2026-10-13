//! The Gemini adapter, against the contract as documented — checked, not
//! assumed.
//!
//! The envelopes below are transcribed from `docs/hooks/reference.md` in
//! `google-gemini/gemini-cli` (retrieved 2026-09-02). The Claude Code adapter
//! earned its correctness the same way: checking found that `PostToolUse`
//! fires only on success, without which this product would have been blind to
//! every failing test run.

use super::*;

fn after_tool(response: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "session_id": "s",
        "transcript_path": "/t",
        "cwd": "/repo",
        "hook_event_name": "AfterTool",
        "timestamp": "2026-09-02T10:00:00Z",
        "tool_name": "run_shell_command",
        "tool_input": { "command": "pytest -q" },
        "tool_response": response,
    })
}

// ---------------------------------------------------------------------------
// CONFLICT-9: the failure is a field, not an event
// ---------------------------------------------------------------------------

#[test]
fn a_successful_tool_has_no_error_field() {
    let payload = after_tool(serde_json::json!({
        "llmContent": "2 passed",
        "returnDisplay": "2 passed",
    }));
    assert_eq!(tool_ok(&payload), Ok(true));
}

/// The whole reason this adapter needs no second event.
#[test]
fn a_failing_tool_arrives_on_the_same_event_with_an_error() {
    let payload = after_tool(serde_json::json!({
        "llmContent": "1 failed",
        "returnDisplay": "1 failed",
        "error": { "message": "Command failed with exit code 1" },
    }));
    assert_eq!(
        tool_ok(&payload),
        Ok(false),
        "reading every AfterTool as a success reproduces exactly the Claude \
         Code blindness the vendor-doc check caught"
    );
    // A null error is an absent one; the doc calls the field optional.
    let null_error = after_tool(serde_json::json!({ "llmContent": "ok", "error": null }));
    assert_eq!(tool_ok(&null_error), Ok(true));
}

/// **The control the coordinator asked for**: an envelope shape this adapter
/// has not seen is refused, not coerced.
///
/// The only value a coercion could pick is `true`, and then "the record did
/// not say" would be indistinguishable from "the tool succeeded" — the same
/// collapse, arriving through a different door.
#[test]
fn an_unseen_envelope_shape_is_refused_rather_than_coerced() {
    let missing = serde_json::json!({
        "hook_event_name": "AfterTool",
        "tool_name": "run_shell_command",
        "tool_input": { "command": "pytest -q" },
    });
    assert_eq!(tool_ok(&missing), Err(EnvelopeRefusal::MissingToolResponse));

    // A bare string: `.get("error")` on it would be `None`, which reads as
    // success. It must not.
    let stringly = after_tool(serde_json::json!("2 passed"));
    assert_eq!(
        tool_ok(&stringly),
        Err(EnvelopeRefusal::ToolResponseNotAnObject)
    );
    let arrayish = after_tool(serde_json::json!(["2 passed"]));
    assert_eq!(
        tool_ok(&arrayish),
        Err(EnvelopeRefusal::ToolResponseNotAnObject)
    );

    for refusal in [
        EnvelopeRefusal::MissingToolResponse,
        EnvelopeRefusal::ToolResponseNotAnObject,
    ] {
        assert!(!refusal.as_str().is_empty());
    }
}

// ---------------------------------------------------------------------------
// CONFLICT-6: no Stop, no PostCompress
// ---------------------------------------------------------------------------

#[test]
fn after_agent_is_the_stop_analogue_and_stop_does_not_exist() {
    assert_eq!(event_kind_for("AfterAgent"), Some("stop"));
    assert_eq!(
        event_kind_for("Stop"),
        None,
        "Gemini has no Stop event; subscribing to one captures nothing and \
         looks like a working gate"
    );
    let payload = serde_json::json!({
        "hook_event_name": "AfterAgent",
        "prompt": "make the tests pass",
        "prompt_response": "All tests pass.",
        "stop_hook_active": false,
    });
    assert_eq!(final_message(&payload), Some("All tests pass."));
}

#[test]
fn compaction_has_a_pre_side_and_no_post_side() {
    assert_eq!(event_kind_for("PreCompress"), Some("compaction_pre"));
    assert_eq!(event_kind_for("PostCompress"), None);
    assert_eq!(event_kind_for("PostCompact"), None);
    assert!(
        !SUBSCRIBED_EVENTS.iter().any(|e| e.contains("Post")),
        "a `Post*` subscription here would be an event this harness never \
         fires: {SUBSCRIBED_EVENTS:?}"
    );
}

/// `AfterModel` fires per chunk. Not subscribing is a decision, so it is
/// asserted rather than left looking like an omission.
#[test]
fn the_per_chunk_model_hook_is_deliberately_not_subscribed() {
    assert!(!SUBSCRIBED_EVENTS.contains(&"AfterModel"));
    assert!(!SUBSCRIBED_EVENTS.contains(&"BeforeModel"));
}

/// Milliseconds, not seconds. A `5` here would be a five-millisecond budget
/// that killed every delivery, and it would look like a working install.
#[test]
fn the_timeout_is_in_milliseconds() {
    assert_eq!(TIMEOUT_MS, 5_000);
    // A seconds-shaped number in a milliseconds-shaped field is the easiest
    // possible bug here, and it would look like a working install: every
    // delivery killed at 5 ms, silently. Compared through a variable so the
    // check is a comparison and not a constant the compiler folds away.
    let floor: u64 = std::hint::black_box(1_000);
    assert!(
        TIMEOUT_MS >= floor,
        "{TIMEOUT_MS} ms is a seconds-shaped value"
    );
}

#[test]
fn the_subscription_covers_every_kind_this_adapter_maps() {
    for event in SUBSCRIBED_EVENTS {
        assert!(
            event_kind_for(event).is_some(),
            "{event} is subscribed and maps to no §3.1 kind, so its deliveries \
             would spool and fold into nothing"
        );
    }
}

/// The permission signal §10's stop-reason classifier needs, and its ceiling.
#[test]
fn the_permission_signal_is_observability_only() {
    assert!(SUBSCRIBED_EVENTS.contains(&"Notification"));
    assert_eq!(event_kind_for("Notification"), Some("note"));
}
