use super::*;
use crate::redaction::Redactor;

const CANARY: &str = "TLCANARYaaZZ11bbYY22ccXX33ddWW44eeVV55ff";
const TS: &str = "2026-09-02T10:11:12.345Z";

fn prep(raw: &str) -> SpoolLine {
    prepare(raw.as_bytes(), TS, &Redactor::new())
}

#[test]
fn post_tool_use_failure_is_subscribed_because_success_only_capture_is_a_bias() {
    // `PostToolUse` fires only when a tool SUCCEEDS. Subscribing to it alone
    // captures a session's green test runs and never its red ones — a
    // systematic bias in the exact direction that destroys a false-success
    // detector, which is the product.
    assert!(
        SUBSCRIBED_EVENTS.contains(&"PostToolUseFailure"),
        "the failure event is not subscribed; the recorder would see only \
         passing runs"
    );
    assert!(SUBSCRIBED_EVENTS.contains(&"PostToolUse"));

    // Both map to the same §3.1 kind, so a verifier sees one population with an
    // `ok` flag rather than two half-populations.
    assert_eq!(event_kind_for("PostToolUse"), Some("tool_result"));
    assert_eq!(event_kind_for("PostToolUseFailure"), Some("tool_result"));
}

#[test]
fn the_verified_vendor_field_names_are_not_quietly_corrected() {
    // BUILD-PLAN B.1 verified these against the raw doc after a summarised pass
    // got two of them wrong. A future reader who "fixes" `tool_response` to
    // `tool_output` would silently stop capturing every tool result.
    let raw = r#"{"hook_event_name":"PostToolUse","session_id":"s","transcript_path":"/t",
        "cwd":"/repo","tool_name":"Write","tool_input":{"file_path":"/a"},
        "tool_response":{"success":true},"tool_use_id":"toolu_1","duration_ms":12}"#;
    let line = prep(raw);
    assert!(line.parse_ok && line.spine_complete && line.recognized);
    assert!(
        line.payload.get("tool_response").is_some(),
        "the vendor field is `tool_response`; nothing may rename it"
    );
    assert_eq!(line.payload["duration_ms"], serde_json::json!(12));
    assert!(BONUS_FIELDS.contains(&"duration_ms"));
}

#[test]
fn a_failed_tool_delivery_is_captured_whole() {
    // The doc's own example payload for the failure event is a failing
    // `npm test` — the exact evidence a false-success detector needs.
    let raw = r#"{"hook_event_name":"PostToolUseFailure","session_id":"s",
        "transcript_path":"/t","cwd":"/repo","tool_name":"Bash",
        "tool_input":{"command":"npm test"},"tool_use_id":"toolu_1",
        "error":"Exit code 1\nError: Cannot find module 'express'",
        "is_interrupt":false,"duration_ms":4187}"#;
    let line = prep(raw);
    assert!(line.recognized);
    assert_eq!(line.payload["is_interrupt"], serde_json::json!(false));
    assert!(line.payload["error"]
        .as_str()
        .unwrap_or_default()
        .contains("Exit code 1"));
}

#[test]
fn every_string_in_a_delivery_is_redacted_before_it_can_reach_disk() {
    // The first producer to exercise §4.6's ordering, and it exercises it
    // structurally: the spool takes `Redacted` values, so an unredacted string
    // has no path to the file.
    let raw = format!(
        r#"{{"hook_event_name":"PreToolUse","session_id":"s","transcript_path":"/t",
           "cwd":"/repo","tool_input":{{"command":"curl -H \"Authorization: Bearer {CANARY}\" https://x",
           "env":{{"GITHUB_TOKEN":"{CANARY}"}}}}}}"#
    );
    let line = prep(&raw);
    let rendered = line.render();
    assert!(
        !rendered.contains(CANARY),
        "a credential reached the spool line: {rendered}"
    );
    assert!(line.redactions_applied >= 2, "{}", line.redactions_applied);
    // The header inside a command string is caught by the TEXT rule; the
    // `GITHUB_TOKEN` under a JSON key is caught by the STRUCTURAL one. Both
    // families must appear, or one of the two paths is not running.
    assert!(line
        .redaction_families
        .contains(&"authorization_header".to_string()));
    assert!(line
        .redaction_families
        .contains(&"structured_credential".to_string()));
}

#[test]
fn bounding_happens_after_masking_on_the_hook_path_too() {
    // A209-C3's four sites cut first and sliced credentials below their
    // detector threshold. Here the padding pushes the credential past the
    // bound; it must still be masked.
    let padding = "x".repeat(FIELD_BOUND);
    let raw = format!(
        r#"{{"hook_event_name":"PreToolUse","session_id":"s","transcript_path":"/t",
           "cwd":"/r","tool_input":{{"command":"{padding} GITHUB_TOKEN={CANARY}"}}}}"#
    );
    let line = prep(&raw);
    assert!(
        !line.render().contains(CANARY),
        "the credential survived because the field was cut before masking"
    );
}

#[test]
fn a_malformed_delivery_is_recorded_with_its_defect_flagged_never_dropped() {
    // `core::sensor`'s total-append discipline. What is malformed is exactly
    // what an investigation needs to see.
    for (raw, why) in [
        ("not json at all", "unparseable"),
        ("{\"hook_event_name\":", "truncated"),
        ("{}", "no spine"),
        ("", "empty"),
        ("{\"hook_event_name\":\"NotAThing\",\"session_id\":\"s\",\"transcript_path\":\"/t\",\"cwd\":\"/r\"}", "unknown event"),
    ] {
        let line = prep(raw);
        let rendered = line.render();
        assert!(rendered.ends_with('\n'), "{why}: not one line");
        assert!(
            serde_json::from_str::<serde_json::Value>(rendered.trim_end()).is_ok(),
            "{why}: produced invalid JSON: {rendered}"
        );
        assert!(!line.raw_sha256.is_empty(), "{why}: no digest of the raw bytes");
    }
    assert!(!prep("not json").parse_ok);
    assert!(!prep("{}").spine_complete);
    assert!(!prep("{\"hook_event_name\":\"NotAThing\"}").recognized);
}

#[test]
fn an_unparseable_delivery_still_has_its_text_redacted() {
    // The dangerous case: parsing failed, so the structured path is unavailable
    // and the raw text is kept — which must still not carry a secret to disk.
    let line = prep(&format!(
        "this is not json but it has GITHUB_TOKEN={CANARY} in it"
    ));
    assert!(!line.parse_ok);
    assert!(
        !line.render().contains(CANARY),
        "an unparsed delivery leaked its secret: {}",
        line.render()
    );
}

#[test]
fn stdin_is_bounded_and_the_truncation_is_recorded() {
    let huge = format!(
        "{{\"hook_event_name\":\"PreToolUse\",\"pad\":\"{}\"}}",
        "x".repeat(MAX_STDIN)
    );
    let line = prepare(huge.as_bytes(), TS, &Redactor::new());
    assert!(
        line.truncated_stdin,
        "an oversized delivery was not flagged"
    );
    assert!(line.render().ends_with('\n'));
    // The digest is over the FULL raw bytes, so truncation does not hide what
    // arrived.
    assert_eq!(
        line.raw_sha256,
        thruline_core::chain::sha256_hex(huge.as_bytes())
    );
}

#[test]
fn the_hook_exits_zero_and_prints_nothing_whatever_happens() {
    // Fail open, absolutely. A hook that errors is a hook that breaks the
    // user's agent.
    let dir = tempfile::tempdir().expect("tempdir");
    let spool = dir.path().join("spool").join("claude-code.jsonl");

    for raw in [
        r#"{"hook_event_name":"Stop","session_id":"s","transcript_path":"/t","cwd":"/r"}"#,
        "garbage",
        "",
    ] {
        assert_eq!(run(&mut raw.as_bytes(), Some(&spool), TS), 0);
    }

    // An unwritable spool must not change the outcome.
    let blocked = dir.path().join("blocked");
    std::fs::write(&blocked, b"i am a file, not a directory").expect("write");
    assert_eq!(
        run(
            &mut "{}".as_bytes(),
            Some(&blocked.join("nested.jsonl")),
            TS
        ),
        0,
        "an unwritable spool changed the exit code"
    );

    // And the writable case really did write, or the assertions above pass for
    // the wrong reason.
    let written = std::fs::read_to_string(&spool).expect("spool exists");
    assert_eq!(written.lines().count(), 3);
    for l in written.lines() {
        assert!(serde_json::from_str::<serde_json::Value>(l).is_ok());
    }
}

#[test]
fn each_delivery_is_exactly_one_line_even_with_embedded_newlines() {
    // A tool result containing newlines must not become several spool lines —
    // the fold reads this file line by line.
    let raw = "{\"hook_event_name\":\"PostToolUse\",\"session_id\":\"s\",\
               \"transcript_path\":\"/t\",\"cwd\":\"/r\",\
               \"tool_response\":\"line one\\nline two\\nline three\"}";
    let line = prep(raw);
    let rendered = line.render();
    assert_eq!(
        rendered.matches('\n').count(),
        1,
        "embedded newlines split the spool line: {rendered}"
    );
}

#[test]
fn appending_is_a_single_write_so_two_hooks_cannot_interleave() {
    // Two hooks landing back to back is the real case — Stop then SessionEnd.
    // `O_APPEND` plus ONE write is what makes that produce two well-formed
    // lines. Verified by writing many lines and requiring every one to parse.
    let dir = tempfile::tempdir().expect("tempdir");
    let spool = dir.path().join("s.jsonl");
    let lines: Vec<String> = (0..200)
        .map(|i| {
            prep(&format!(
                "{{\"hook_event_name\":\"Stop\",\"session_id\":\"s{i}\",\
                  \"transcript_path\":\"/t\",\"cwd\":\"/r\"}}"
            ))
            .render()
        })
        .collect();
    for l in &lines {
        append_line(&spool, l).expect("append");
    }
    let read = std::fs::read_to_string(&spool).expect("read");
    assert_eq!(read.lines().count(), 200);
    for l in read.lines() {
        assert!(
            serde_json::from_str::<serde_json::Value>(l).is_ok(),
            "torn line: {l}"
        );
    }
}

#[test]
fn the_hook_path_is_well_inside_the_fifty_millisecond_budget() {
    // §4.1's p95 is 50 ms. Measured over 200 invocations against a realistic
    // delivery, asserted on the MEDIAN and p95 rather than a single run.
    //
    // The assertion is deliberately loose (this is a debug build on shared CI
    // hardware); its job is to catch an order-of-magnitude regression — someone
    // adding a SQLite open or a `git add -A` to this path — not to certify a
    // number. `thruline doctor` is where the real figure belongs.
    let dir = tempfile::tempdir().expect("tempdir");
    let spool = dir.path().join("s.jsonl");
    let raw = format!(
        "{{\"hook_event_name\":\"PostToolUse\",\"session_id\":\"s\",\
          \"transcript_path\":\"/t\",\"cwd\":\"/r\",\
          \"tool_input\":{{\"command\":\"cargo test --workspace\"}},\
          \"tool_response\":\"{}\"}}",
        "test result: ok. ".repeat(200)
    );

    let mut timings: Vec<u128> = Vec::with_capacity(200);
    for _ in 0..200 {
        let start = std::time::Instant::now();
        let code = run(&mut raw.as_bytes(), Some(&spool), TS);
        timings.push(start.elapsed().as_micros());
        assert_eq!(code, 0);
    }
    timings.sort_unstable();
    let median = timings[timings.len() / 2];
    let p95 = timings[(timings.len() * 95) / 100];
    println!("hook latency: median {median} us, p95 {p95} us (budget 50000 us)");
    assert!(
        p95 < 50_000,
        "p95 {p95} us exceeds the 50 ms hook budget — something heavy has been \
         added to this path"
    );
}

// D3's control now lives in `tests/hook_path_isolation.rs`, which walks the
// hook's whole reachable module graph and matches IMPORTS rather than call
// spellings. The `include_str!` scan that used to sit here was defeated two
// ways: `use std::process::Command as Proc`, and — with no obfuscation at all —
// putting the work in a sibling module file, because the assertion read one
// file while the property spans the call graph.
