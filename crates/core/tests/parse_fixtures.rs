//! Normalization snapshots over the committed real-transcript fixtures, plus
//! the malformed-input contract. Fixtures and their provenance are documented
//! in `tests/fixtures/README.md`.

use std::path::PathBuf;

use thruline_core::parse::{parse_file, ParseStats};
use thruline_core::EventKind;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn normalizes_the_twelve_real_specimens() {
    let mut stats = ParseStats::default();
    let lines = parse_file(fixture("claude_code_sessions.jsonl"), &mut stats)
        .expect("fixture must parse without an i/o error");

    assert_eq!(lines.len(), 12, "one ParsedLine per fixture line");
    assert_eq!(stats.lines_malformed, 0, "no fixture line is malformed");

    // Snapshot the whole normalization, so any mapping drift shows up as a diff.
    insta::assert_json_snapshot!("sessions_normalized", lines);
}

#[test]
fn fan_out_matches_block_count() {
    let mut stats = ParseStats::default();
    let lines =
        parse_file(fixture("claude_code_sessions.jsonl"), &mut stats).expect("fixture must parse");

    // Manifest line 11: assistant with thinking + text + tool_use.
    let fan = lines
        .iter()
        .find(|l| l.events.len() == 3)
        .expect("the thinking+text+tool_use specimen must fan out to 3 events");
    let kinds: Vec<EventKind> = fan.events.iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![
            EventKind::AssistantThinking,
            EventKind::AssistantText,
            EventKind::ToolUse
        ],
        "events must come out in block order"
    );

    // Every event fanned from one line shares the line's identity.
    for line in &lines {
        assert_eq!(line.sha256.len(), 64);
        for ev in &line.events {
            assert_eq!(ev.src_uuid, fan_src(line, ev));
        }
    }
}

fn fan_src(
    line: &thruline_core::ParsedLine,
    _ev: &thruline_core::NormalizedEvent,
) -> Option<String> {
    // All events from a line carry that line's uuid; recompute from the first.
    line.events.first().and_then(|e| e.src_uuid.clone())
}

#[test]
fn session_id_comes_from_sessionid_never_session_id() {
    // §8: on `sessionKind:"bg"` files `session_id` is the PARENT session.
    // Manifest line 12 is exactly that case.
    let raw =
        std::fs::read_to_string(fixture("claude_code_sessions.jsonl")).expect("fixture readable");
    let last: serde_json::Value =
        serde_json::from_str(raw.lines().nth(11).expect("fixture has 12 lines"))
            .expect("valid json");

    let camel = last["sessionId"].as_str().expect("sessionId present");
    let snake = last["session_id"].as_str().expect("session_id present");
    assert_ne!(camel, snake, "fixture must exercise the disagreement");

    let mut stats = ParseStats::default();
    let lines =
        parse_file(fixture("claude_code_sessions.jsonl"), &mut stats).expect("fixture must parse");
    assert_eq!(
        lines[11].session_id.as_deref(),
        Some(camel),
        "parser must take sessionId, not session_id"
    );
}

#[test]
fn unknown_types_are_counted_and_become_meta() {
    let mut stats = ParseStats::default();
    let lines =
        parse_file(fixture("claude_code_sessions.jsonl"), &mut stats).expect("fixture must parse");

    // queue-operation, last-prompt, system, file-history-snapshot
    assert_eq!(stats.unknown_types.len(), 4, "{:?}", stats.unknown_types);
    for t in [
        "queue-operation",
        "last-prompt",
        "system",
        "file-history-snapshot",
    ] {
        assert_eq!(
            stats.unknown_types.get(t),
            Some(&1),
            "{t} must be counted, not dropped"
        );
    }

    // Each produced exactly one meta event carrying its type token.
    let meta_texts: Vec<&str> = lines
        .iter()
        .flat_map(|l| &l.events)
        .filter(|e| e.kind == EventKind::Meta)
        .filter_map(|e| e.text.as_deref())
        .collect();
    assert_eq!(
        meta_texts,
        vec![
            "queue-operation",
            "last-prompt",
            "system",
            "file-history-snapshot"
        ]
    );
}

#[test]
fn tool_result_envelope_carries_content_and_tool_use_result() {
    let mut stats = ParseStats::default();
    let lines =
        parse_file(fixture("claude_code_sessions.jsonl"), &mut stats).expect("fixture must parse");

    let results: Vec<_> = lines
        .iter()
        .flat_map(|l| &l.events)
        .filter(|e| e.kind == EventKind::ToolResult)
        .collect();
    // Four: three dedicated specimens plus the tool_result the bg-session
    // line happens to carry.
    assert_eq!(results.len(), 4, "tool_result specimens");

    for ev in &results {
        let raw = ev.tool_result.as_deref().expect("envelope present");
        let env: serde_json::Value = serde_json::from_str(raw).expect("envelope is json");
        assert!(env.get("content").is_some(), "content is always present");
        assert!(
            ev.tool_use_id.is_some(),
            "tool_use_id links to the tool_use"
        );
    }

    // One specimen has is_error:true; the others do not.
    assert_eq!(results.iter().filter(|e| e.is_error).count(), 1);

    // String content and array content both survive.
    let shapes: Vec<&str> = results
        .iter()
        .map(|e| {
            let env: serde_json::Value =
                serde_json::from_str(e.tool_result.as_deref().unwrap_or("{}")).unwrap_or_default();
            match env.get("content") {
                Some(serde_json::Value::String(_)) => "string",
                Some(serde_json::Value::Array(_)) => "array",
                _ => "other",
            }
        })
        .collect();
    assert!(shapes.contains(&"string"), "{shapes:?}");
    assert!(shapes.contains(&"array"), "{shapes:?}");
}

#[test]
fn malformed_lines_are_counted_and_listed_not_fatal() {
    let mut stats = ParseStats::default();
    let lines = parse_file(fixture("malformed.jsonl"), &mut stats)
        .expect("a file full of damage must not be an error");

    // Two well-formed user lines + one unknown-type line survive.
    assert_eq!(lines.len(), 3, "good lines still parse");

    // Unterminated JSON, non-JSON text, and the truncated final line.
    assert_eq!(stats.lines_malformed, 3, "{:?}", stats.malformed);

    let reasons: Vec<String> = stats
        .malformed
        .iter()
        .map(|m| format!("{}:{}", m.line, m.reason))
        .collect();
    insta::assert_debug_snapshot!("malformed_reasons", reasons);

    // Blank and whitespace-only lines are separators, not defects.
    assert!(
        stats.malformed.iter().all(|m| m.line != 3 && m.line != 4),
        "blank lines must not be reported as malformed"
    );
}

#[test]
fn invalid_utf8_is_reported_and_surrounding_lines_still_parse() {
    let mut stats = ParseStats::default();
    let lines =
        parse_file(fixture("bad_utf8.jsonl"), &mut stats).expect("bad bytes must not be fatal");

    assert_eq!(lines.len(), 2, "clean lines on both sides survive");
    assert_eq!(stats.lines_malformed, 1);
    assert!(matches!(
        stats.malformed[0].reason,
        thruline_core::MalformedReason::InvalidUtf8
    ));
    assert_eq!(stats.malformed[0].line, 2);
}

#[test]
fn stats_tally_every_event() {
    let mut stats = ParseStats::default();
    let lines =
        parse_file(fixture("claude_code_sessions.jsonl"), &mut stats).expect("fixture must parse");

    let counted: u64 = stats.events_by_kind.values().sum();
    let actual: u64 = lines.iter().map(|l| l.events.len() as u64).sum();
    assert_eq!(counted, actual);
    assert_eq!(stats.events, actual);
    assert_eq!(stats.lines_read, 12);

    insta::assert_debug_snapshot!("sessions_stats_by_kind", stats.events_by_kind);
}

#[test]
fn many_consecutive_blank_lines_do_not_blow_the_stack() {
    // Regression: the line reader used to recurse per skipped line.
    let mut blob = String::new();
    blob.push_str(
        r#"{"type":"user","uuid":"00000000-0000-4000-8000-00000000b001","sessionId":"cccccccc-0000-4000-8000-000000000001","timestamp":"2026-07-29T00:00:00.000Z","parentUuid":null,"isSidechain":false,"message":{"role":"user","content":"before"}}"#,
    );
    blob.push('\n');
    for _ in 0..200_000 {
        blob.push('\n');
    }
    blob.push_str(
        r#"{"type":"user","uuid":"00000000-0000-4000-8000-00000000b002","sessionId":"cccccccc-0000-4000-8000-000000000001","timestamp":"2026-07-29T00:00:01.000Z","parentUuid":null,"isSidechain":false,"message":{"role":"user","content":"after"}}"#,
    );
    blob.push('\n');

    let mut stats = ParseStats::default();
    let lines = thruline_core::parse::parse_reader(blob.as_bytes(), "blank.jsonl", &mut stats)
        .expect("blank lines must not be an error");

    assert_eq!(lines.len(), 2);
    assert_eq!(stats.lines_malformed, 0);
    // Line numbers still match the file: the second real line is 200_002.
    assert_eq!(lines[0].line, 1);
    assert_eq!(lines[1].line, 200_002);
}
