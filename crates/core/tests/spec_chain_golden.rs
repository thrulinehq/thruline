//! A golden 20-event chain, pinned to a committed head hash.
//!
//! The property tests next door prove the chain's *behaviour*. This pins its
//! *wire format*. They are different claims and only one of them survives a
//! refactor of the canonical form:
//!
//! - A property test asks "does tampering break verification?", and keeps
//!   passing if the entire hash function is replaced, as long as the
//!   replacement is also a hash function.
//! - This asks "do these exact 20 events still produce this exact hash?", and
//!   fails the moment field order, key spelling, number formatting, timestamp
//!   handling, or the preimage rule changes.
//!
//! That second question is the one that matters across versions and across
//! machines: a receipt signed by v0.1 names a chain head, and a v0.2 that
//! computes a different head for the same events has silently invalidated every
//! receipt ever issued. When this test fails, the correct response is almost
//! never to update the constant.
//!
//! Regenerate deliberately with `THRULINE_BLESS_GOLDEN=1 cargo test -p
//! thruline-core --test spec_chain_golden`, and say in the commit message why
//! the wire format changed.

use std::path::{Path, PathBuf};

use thruline_core::spec::chain::{genesis_prev_hash, verify_chain};
use thruline_core::spec::event::{
    CompactionPost, CompactionPre, Event, EventId, Exec, FsChange, FsOp, OpenPayload, Payload,
    SnapshotPayload, Source, Stop, StopReason, ToolCall, ToolResult,
};

/// The chain head of the twenty events below.
///
/// Changing this constant changes what every previously issued receipt means.
const GOLDEN_HEAD: &str = "sha256:940944c07d8099c64f71bec090095a49db59dbf1f3fa8fcd62ede8f1f282ca30";

const SESSION_ID: &str = "golden-session-01";

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/spec/chain-20.jsonl")
}

/// Fixed ULIDs, so the fixture is reproducible. Crockford base32, 26 chars.
fn id(n: usize) -> EventId {
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let hi = ALPHABET[(n / 32) % 32] as char;
    let lo = ALPHABET[n % 32] as char;
    EventId::parse(&format!("evt_01ARZ3NDEKTSV4RRFFQ69G5F{hi}{lo}"))
        .unwrap_or_else(|e| panic!("generated id {n} is not a ULID: {e}"))
}

fn empty(pairs: &[(&str, serde_json::Value)]) -> OpenPayload {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect()
}

/// Twenty events covering all thirteen §3.1 kinds, both `Option` states of every
/// optional field, an `Other` source, unicode and escapes in text, and the
/// integer extremes.
fn golden_payloads() -> Vec<(Source, &'static str, Payload)> {
    vec![
        (
            Source::ClaudeCodeHook,
            "2026-09-02T10:11:12.345Z",
            Payload::SessionStart(empty(&[("harness", serde_json::json!("claude-code"))])),
        ),
        (
            Source::ClaudeCodeHook,
            "2026-09-02T10:11:13Z",
            Payload::UserPrompt(empty(&[(
                // Unicode, an escape, and a non-BMP character: the three things
                // RFC 8785 string handling can get wrong.
                "prompt_redacted",
                serde_json::json!("fix the \"café\" test 😂\n\tsecond line"),
            )])),
        ),
        (
            Source::ClaudeCodeHook,
            "2026-09-02T10:11:14.000Z",
            Payload::ToolCall(ToolCall {
                tool: "Bash".into(),
                input_hash:
                    "sha256:1111111111111111111111111111111111111111111111111111111111111111".into(),
                input_redacted: "cargo test".into(),
                cwd: "/repo".into(),
                extra: Default::default(),
            }),
        ),
        (
            Source::ClaudeCodeHook,
            "2026-09-02T10:11:15.500Z",
            Payload::ToolResult(ToolResult {
                tool: "Bash".into(),
                call_id: "toolu_01ABC".into(),
                ok: true,
                output_hash:
                    "sha256:2222222222222222222222222222222222222222222222222222222222222222".into(),
                output_excerpt: "test result: ok. 42 passed".into(),
                exit_code: Some(0),
                extra: Default::default(),
            }),
        ),
        (
            Source::ClaudeCodeHook,
            "2026-09-02T10:11:16Z",
            // The same kind with the optional field absent — the two serialize
            // to different bytes, and `skip_serializing_if` is exactly the kind
            // of attribute a refactor silently drops.
            Payload::ToolResult(ToolResult {
                tool: "Read".into(),
                call_id: "toolu_01DEF".into(),
                ok: false,
                output_hash:
                    "sha256:3333333333333333333333333333333333333333333333333333333333333333".into(),
                output_excerpt: String::new(),
                exit_code: None,
                extra: Default::default(),
            }),
        ),
        (
            Source::ExecRecorder,
            "2026-09-02T10:11:17.001Z",
            Payload::Exec(Exec {
                argv: vec!["cargo".into(), "test".into(), "--workspace".into()],
                argv_hash:
                    "sha256:4444444444444444444444444444444444444444444444444444444444444444".into(),
                cwd: "/repo".into(),
                env_hash: "sha256:5555555555555555555555555555555555555555555555555555555555555555"
                    .into(),
                started: "2026-09-02T10:11:16.900Z".into(),
                ended: "2026-09-02T10:11:17.000Z".into(),
                exit_code: 0,
                stdout_hash:
                    "sha256:6666666666666666666666666666666666666666666666666666666666666666".into(),
                stderr_hash:
                    "sha256:7777777777777777777777777777777777777777777777777777777777777777".into(),
                stdout_excerpt: "42 passed; 0 failed".into(),
                stderr_excerpt: String::new(),
                pid: 4242,
                parent: 4241,
                extra: Default::default(),
            }),
        ),
        (
            Source::ExecRecorder,
            "2026-09-02T10:11:18Z",
            // A non-zero exit, the case the Claude Code adapter would have
            // missed entirely if it only subscribed to PostToolUse (CONFLICT-1).
            Payload::Exec(Exec {
                argv: vec!["npm".into(), "test".into()],
                argv_hash:
                    "sha256:8888888888888888888888888888888888888888888888888888888888888888".into(),
                cwd: "/repo".into(),
                env_hash: "sha256:9999999999999999999999999999999999999999999999999999999999999999"
                    .into(),
                started: "2026-09-02T10:11:17.500Z".into(),
                ended: "2026-09-02T10:11:18.000Z".into(),
                exit_code: 1,
                stdout_hash:
                    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                stderr_hash:
                    "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
                stdout_excerpt: String::new(),
                stderr_excerpt: "Error: Cannot find module 'express'".into(),
                pid: 4243,
                parent: 4241,
                extra: Default::default(),
            }),
        ),
        (
            Source::FsWatcher,
            "2026-09-02T10:11:19Z",
            Payload::FsChange(FsChange {
                path: "src/lib.rs".into(),
                op: FsOp::Modify,
                before_hash: Some(
                    "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                        .into(),
                ),
                after_hash: Some(
                    "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
                        .into(),
                ),
                extra: Default::default(),
            }),
        ),
        (
            Source::FsWatcher,
            "2026-09-02T10:11:20Z",
            Payload::FsChange(FsChange {
                path: "tests/new_test.rs".into(),
                op: FsOp::Create,
                before_hash: None,
                after_hash: Some(
                    "sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
                        .into(),
                ),
                extra: Default::default(),
            }),
        ),
        (
            Source::FsWatcher,
            "2026-09-02T10:11:21Z",
            Payload::FsChange(FsChange {
                path: "old.rs".into(),
                op: FsOp::Delete,
                before_hash: Some(
                    "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
                        .into(),
                ),
                after_hash: None,
                extra: Default::default(),
            }),
        ),
        (
            Source::FsWatcher,
            "2026-09-02T10:11:22Z",
            Payload::FsChange(FsChange {
                path: "renamed.rs".into(),
                op: FsOp::Rename,
                before_hash: None,
                after_hash: None,
                extra: Default::default(),
            }),
        ),
        (
            Source::Git,
            "2026-09-02T10:11:23Z",
            Payload::Snapshot(SnapshotPayload {
                tree_hash: "4b825dc642cb6eb9a060e54bf8d69288fbee4904".into(),
                head_commit: Some("0000000000000000000000000000000000000000".into()),
                dirty: true,
                changed_paths_count: 3,
                extra: Default::default(),
            }),
        ),
        (
            Source::Git,
            "2026-09-02T10:11:24Z",
            // Unborn HEAD, and the largest count a hash preimage may carry:
            // 2^53 - 1. `u64::MAX` was here first and was REFUSED by the model
            // — see `hashed_numbers_beyond_the_safe_integer_range_would_collide`
            // in spec::event. RFC 8785 rounds beyond this through an IEEE-754
            // double, so two different counts would produce one hash.
            Payload::Snapshot(SnapshotPayload {
                tree_hash: "4b825dc642cb6eb9a060e54bf8d69288fbee4904".into(),
                head_commit: None,
                dirty: false,
                changed_paths_count: 9_007_199_254_740_991,
                extra: Default::default(),
            }),
        ),
        (
            Source::ClaudeCodeHook,
            "2026-09-02T10:11:25Z",
            Payload::CompactionPre(CompactionPre {
                open_promises: vec![serde_json::json!({"text": "I'll add integration tests"})],
                open_tasks: vec![serde_json::json!("wire up the parser")],
                recent_claims: vec![serde_json::json!({"kind": "tests_pass"})],
                transcript_hash:
                    "sha256:1010101010101010101010101010101010101010101010101010101010101010".into(),
                extra: Default::default(),
            }),
        ),
        (
            Source::ClaudeCodeHook,
            "2026-09-02T10:11:26Z",
            Payload::CompactionPre(CompactionPre {
                open_promises: vec![],
                open_tasks: vec![],
                recent_claims: vec![],
                transcript_hash:
                    "sha256:2020202020202020202020202020202020202020202020202020202020202020".into(),
                extra: Default::default(),
            }),
        ),
        (
            Source::ClaudeCodeHook,
            "2026-09-02T10:11:27Z",
            Payload::CompactionPost(CompactionPost {
                transcript_hash:
                    "sha256:3030303030303030303030303030303030303030303030303030303030303030".into(),
                extra: Default::default(),
            }),
        ),
        (
            Source::ClaudeCodeHook,
            "2026-09-02T10:11:28Z",
            Payload::SubagentStop(empty(&[("agent_type", serde_json::json!("Explore"))])),
        ),
        (
            Source::ClaudeCodeHook,
            "2026-09-02T10:11:29Z",
            Payload::Stop(Stop {
                reason: StopReason::AgentFinished,
                last_assistant_message_hash:
                    "sha256:4040404040404040404040404040404040404040404040404040404040404040".into(),
                last_assistant_message: "All tests pass. Done.".into(),
                stop_hook_active: Some(true),
                extra: Default::default(),
            }),
        ),
        (
            // An unrecognised producer, preserved verbatim.
            Source::Other("cursor-hook".into()),
            "2026-09-02T10:11:30Z",
            Payload::Note(empty(&[
                ("negative", serde_json::json!(-9_007_199_254_740_991i64)),
                ("empty_object", serde_json::json!({})),
                ("empty_array", serde_json::json!([])),
                ("null", serde_json::Value::Null),
            ])),
        ),
        (
            Source::Thruline,
            "2026-09-02T10:11:31.999Z",
            Payload::SessionEnd(empty(&[("reason", serde_json::json!("clear"))])),
        ),
    ]
}

fn build_golden() -> Vec<Event> {
    let mut chain: Vec<Event> = Vec::new();
    for (n, (source, ts, payload)) in golden_payloads().into_iter().enumerate() {
        let prev = match chain.last() {
            Some(last) => last
                .hash()
                .unwrap_or_else(|| panic!("event {n} predecessor is unsealed"))
                .to_string(),
            None => genesis_prev_hash(SESSION_ID),
        };
        let event = Event::new(SESSION_ID, (n as u64) + 1, ts, source, payload, prev)
            .unwrap_or_else(|e| panic!("event {n} is not constructible: {e}"))
            // Fixed id, so the fixture is byte-reproducible.
            .with_id(id(n));
        chain.push(
            event
                .seal()
                .unwrap_or_else(|e| panic!("event {n} does not seal: {e}")),
        );
    }
    chain
}

fn to_jsonl(chain: &[Event]) -> String {
    let mut out = String::new();
    for event in chain {
        out.push_str(&serde_json::to_string(event).unwrap_or_else(|e| panic!("serialize: {e}")));
        out.push('\n');
    }
    out
}

#[test]
fn the_golden_chain_matches_its_committed_bytes_and_head() {
    let chain = build_golden();
    assert_eq!(chain.len(), 20, "the golden chain is twenty events");
    let rendered = to_jsonl(&chain);

    if std::env::var_os("THRULINE_BLESS_GOLDEN").is_some() {
        let path = fixture_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("mkdir: {e}"));
        }
        std::fs::write(&path, &rendered).unwrap_or_else(|e| panic!("write: {e}"));
        let head = verify_chain(&chain).unwrap_or_else(|e| panic!("verify: {e}"));
        panic!(
            "BLESSED. Wrote {} and computed head {}. Set GOLDEN_HEAD to it, drop \
             THRULINE_BLESS_GOLDEN, and explain in the commit message why the wire \
             format changed.",
            path.display(),
            head.head_hash
        );
    }

    let committed = std::fs::read_to_string(fixture_path())
        .unwrap_or_else(|e| panic!("reading the golden fixture: {e}"));
    assert_eq!(
        rendered, committed,
        "the serialized form of the twenty golden events no longer matches the \
         committed fixture — the event wire format has changed"
    );

    let head = verify_chain(&chain).unwrap_or_else(|e| panic!("the golden chain must verify: {e}"));
    assert_eq!(
        head.head_hash, GOLDEN_HEAD,
        "the golden chain head moved. Every receipt already issued names a head \
         computed by the old rule; updating this constant is almost never the \
         right fix."
    );
    assert_eq!(head.event_count, 20);
    assert_eq!(head.last_seq, 20);
}

#[test]
fn the_committed_fixture_parses_back_into_the_same_chain() {
    // Round-trip through the file, not through memory: this is what a second
    // machine does with a chain it received.
    let committed = std::fs::read_to_string(fixture_path())
        .unwrap_or_else(|e| panic!("reading the golden fixture: {e}"));
    let parsed: Vec<Event> = committed
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("parsing {line}: {e}")))
        .collect();

    assert_eq!(parsed, build_golden(), "parse(serialize(x)) != x");
    let head = verify_chain(&parsed).unwrap_or_else(|e| panic!("verify: {e}"));
    assert_eq!(head.head_hash, GOLDEN_HEAD);

    for event in &parsed {
        // The `Other` source has no payload type, so it is reported rather than
        // validated — the other nineteen must validate cleanly.
        match event.validate() {
            Ok(()) => {}
            Err(e) => panic!("golden event {} does not validate: {e}", event.id()),
        }
    }
}

#[test]
fn every_kind_appears_in_the_golden_chain() {
    // Without this, a kind could be dropped from the model and the golden test
    // would keep passing on the nineteen that remain.
    use std::collections::BTreeSet;
    let kinds: BTreeSet<String> = build_golden().iter().map(|e| e.kind().as_str()).collect();
    let expected: BTreeSet<String> = [
        "session_start",
        "user_prompt",
        "tool_call",
        "tool_result",
        "exec",
        "fs_change",
        "snapshot",
        "compaction_pre",
        "compaction_post",
        "subagent_stop",
        "stop",
        "session_end",
        "note",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    assert_eq!(
        kinds, expected,
        "the golden chain no longer covers every §3.1 kind"
    );
}

#[test]
fn flipping_any_byte_of_the_committed_fixture_is_detected() {
    // The fixture is the artefact a second machine verifies. Prove that no
    // single-byte edit to it survives — this is the file-level statement of what
    // the proptest proves at the struct level.
    let committed = std::fs::read_to_string(fixture_path())
        .unwrap_or_else(|e| panic!("reading the golden fixture: {e}"));
    let bytes = committed.as_bytes();

    // Every 97th byte, so the walk is spread across all twenty lines and every
    // field position without taking a minute to run.
    let mut checked = 0usize;
    for i in (0..bytes.len()).step_by(97) {
        let mut mutated = bytes.to_vec();
        // Flip to a different printable character that keeps the JSON parseable
        // most of the time; when it does not parse, that is also a detection.
        mutated[i] = if mutated[i] == b'0' { b'1' } else { b'0' };
        let text = match String::from_utf8(mutated) {
            Ok(t) => t,
            Err(_) => continue, // split a multi-byte char: detected by definition
        };
        let parsed: Result<Vec<Event>, _> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(serde_json::from_str::<Event>)
            .collect();
        let detected = match parsed {
            Err(_) => true,
            Ok(events) => verify_chain(&events).is_err(),
        };
        assert!(
            detected,
            "flipping byte {i} of the golden fixture produced a chain that still \
             verifies"
        );
        checked += 1;
    }
    assert!(
        checked > 50,
        "only {checked} byte positions were checked — the walk is not covering the \
         fixture"
    );
}
