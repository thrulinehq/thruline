//! The Codex adapter, against the real record.
//!
//! The fixtures below are **copied from `~/.codex/sessions` on this machine**,
//! trimmed for length, not written from the spec. BUILD-PLAN B.2 records why
//! that matters: §4.2's `.codex/sessions/*.jsonl` does not exist, and an
//! adapter built from the spec alone would find nothing anywhere and report a
//! clean absence.

use super::*;

const PATCH_OK: &str = r#"{"timestamp":"2026-04-20T14:22:48.317Z","type":"event_msg","payload":{"type":"patch_apply_end","call_id":"call_7Du","turn_id":"019dab40","stdout":"Success. Updated the following files:\nM /repo/a.ts\n","stderr":"","success":true,"changes":{"/repo/a.ts":{"type":"update","unified_diff":"@@ -1 +1 @@\n-a\n+b\n","move_path":null},"/repo/new.ts":{"type":"add","content":"x\n"}},"status":"completed"}}"#;

const PATCH_FAILED: &str = r#"{"timestamp":"2026-07-11T07:37:18.851Z","type":"event_msg","payload":{"type":"patch_apply_end","call_id":"exec-2ce","turn_id":"019f5016","stdout":"","stderr":"Failed to write file /repo/w.swift\n","success":false,"changes":{"/repo/w.swift":{"type":"update","unified_diff":"@@ -1 +1 @@\n-a\n+b\n","move_path":null}}}}"#;

const COMPACTED_BOUNDARY: &str = r#"{"timestamp":"2026-05-03T18:28:08.162Z","type":"event_msg","payload":{"type":"context_compacted"}}"#;

const COMPACTED_ENVELOPE: &str = r#"{"timestamp":"2026-05-03T18:28:08.160Z","type":"compacted","payload":{"message":"summary text","replacement_history":[{"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}]}]}}"#;

const SESSION_META: &str = r#"{"timestamp":"2026-04-20T19:46:44.000Z","type":"session_meta","payload":{"id":"019dab40-313d-7b33-a774-6a29cbce14d4","cwd":"/repo","originator":"codex_cli_rs","cli_version":"0.146.0"}}"#;

// ---------------------------------------------------------------------------
// The two record types this reader exists to recover
// ---------------------------------------------------------------------------

#[test]
fn a_successful_patch_becomes_one_fs_change_per_path() {
    let r = recover(7, PATCH_OK).expect("recovered");
    assert_eq!(r.ordinal, 7);
    assert_eq!(r.ts, "2026-04-20T14:22:48.317Z");
    let RecoveredKind::FsChanges { call_id, changes } = r.kind else {
        panic!("expected fs changes, got {:?}", r.kind);
    };
    assert_eq!(call_id, "call_7Du");
    let mut ops: Vec<(String, &str)> = changes
        .iter()
        .map(|c| (c.path.clone(), c.op))
        .collect::<Vec<_>>();
    ops.sort();
    assert_eq!(
        ops,
        vec![
            ("/repo/a.ts".to_string(), "modify"),
            ("/repo/new.ts".to_string(), "create"),
        ]
    );
}

/// **The one that matters.** A failed patch still carries a full `changes`
/// map — measured on this machine, with `stderr: "Failed to write file …"`.
/// A reader that mapped `changes` unconditionally would record file mutations
/// that did not happen: §0.2 exactly backwards.
#[test]
fn a_failed_patch_is_not_a_file_change() {
    let r = recover(0, PATCH_FAILED).expect("recovered");
    match r.kind {
        RecoveredKind::PatchFailed { stderr, paths, .. } => {
            assert!(stderr.contains("Failed to write file"));
            assert_eq!(paths, vec!["/repo/w.swift".to_string()]);
        }
        other => panic!("a failed patch must not become an fs_change: {other:?}"),
    }
}

/// `compacted` and `context_compacted` share a timestamp to the millisecond
/// and both mean "a compaction happened". They are **different records**: one
/// carries the post state, the other is the boundary and nothing else.
/// Treating them as one double-counts compactions.
#[test]
fn the_two_compaction_records_are_not_the_same_record() {
    let boundary = recover(1, COMPACTED_BOUNDARY).expect("recovered");
    let post = recover(2, COMPACTED_ENVELOPE).expect("recovered");
    assert_eq!(boundary.kind, RecoveredKind::CompactionBoundary);
    let RecoveredKind::CompactionPost {
        summary,
        replacement_hash,
    } = post.kind
    else {
        panic!("expected a post record");
    };
    assert_eq!(summary, "summary text");
    assert_eq!(replacement_hash.len(), 64, "a sha256 of the replacement");
    assert_ne!(boundary.raw_sha256, post.raw_sha256);
}

/// The boundary record carries **nothing** but its own type, so §3.1's
/// `compaction_pre` fields cannot be derived from it. Pinned so a future reader
/// does not invent them.
#[test]
fn the_boundary_record_carries_no_pre_state() {
    let value: serde_json::Value = serde_json::from_str(COMPACTED_BOUNDARY).expect("json");
    let payload = value.get("payload").expect("payload");
    let keys: Vec<&String> = payload.as_object().expect("object").keys().collect();
    assert_eq!(
        keys,
        vec!["type"],
        "if Codex ever adds pre-state here, map it — until then it is absent, \
         not defaulted"
    );
}

#[test]
fn session_meta_binds_a_rollout_to_a_repository() {
    let r = recover(0, SESSION_META).expect("recovered");
    assert_eq!(
        r.kind,
        RecoveredKind::SessionMeta {
            session_id: "019dab40-313d-7b33-a774-6a29cbce14d4".into(),
            cwd: "/repo".into(),
        }
    );
}

// ---------------------------------------------------------------------------
// Ordering and refusal — the `seq = u64::MAX` lesson, applied ahead of time
// ---------------------------------------------------------------------------

/// The position is the file's, so a re-read derives the same number.
#[test]
fn ordering_comes_from_the_file_and_is_reproducible() {
    let lines = [SESSION_META, PATCH_OK, COMPACTED_BOUNDARY];
    let first: Vec<u64> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| recover(i as u64, l).expect("recovered").ordinal)
        .collect();
    let again: Vec<u64> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| recover(i as u64, l).expect("recovered").ordinal)
        .collect();
    assert_eq!(first, vec![0, 1, 2]);
    assert_eq!(first, again);
}

/// A line with no timestamp is **refused**, not stamped with "now".
///
/// An invented time is indistinguishable from a recorded one afterwards, which
/// is the whole shape of the defect this build has just finished removing from
/// the verdict path.
#[test]
fn a_line_without_a_timestamp_is_refused_rather_than_defaulted() {
    let no_ts = r#"{"type":"event_msg","payload":{"type":"context_compacted"}}"#;
    assert_eq!(recover(0, no_ts), Err(Refusal::NoTimestamp));

    let no_type = r#"{"timestamp":"2026-01-01T00:00:00Z","payload":{}}"#;
    assert_eq!(recover(0, no_type), Err(Refusal::NoEnvelopeType));

    assert_eq!(recover(0, "not json"), Err(Refusal::Unparseable));
    assert_eq!(recover(0, "[1,2,3]"), Err(Refusal::Unparseable));
}

/// A shape this reader has no §3.1 kind for is **named and kept**, never
/// dropped. This is the invariant that preserved 1.4 M legacy rows.
#[test]
fn an_unmapped_shape_is_named_rather_than_dropped() {
    let unknown = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"world_state","payload":{}}"#;
    let r = recover(0, unknown).expect("recovered");
    assert_eq!(
        r.kind,
        RecoveredKind::Unmapped {
            family: "world_state".into()
        }
    );
    let unknown_msg = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"event_msg","payload":{"type":"token_count"}}"#;
    assert_eq!(
        recover(0, unknown_msg).expect("recovered").kind,
        RecoveredKind::Unmapped {
            family: "event_msg/token_count".into()
        }
    );
}

/// The digest names the bytes the event came from, so a later reader can check
/// the claim rather than take it.
#[test]
fn a_recovered_event_names_the_line_it_came_from() {
    let r = recover(3, PATCH_OK).expect("recovered");
    assert_eq!(
        r.raw_sha256,
        thruline_core::chain::sha256_hex(PATCH_OK.as_bytes())
    );
}

// ---------------------------------------------------------------------------
// The two Rule 7 conflicts, pinned so they cannot be "corrected" back
// ---------------------------------------------------------------------------

/// CONFLICT-5: the sessions directory is user-global and date-nested.
#[test]
fn the_sessions_root_is_user_global_and_date_nested() {
    let home = std::path::Path::new("/home/u");
    assert_eq!(
        sessions_root(home),
        std::path::PathBuf::from("/home/u/.codex/sessions")
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let nested = dir.path().join("2026").join("04").join("20");
    std::fs::create_dir_all(&nested).expect("mkdir");
    std::fs::write(nested.join("rollout-2026-04-20T19-46-44-abc.jsonl"), "").expect("w");
    std::fs::write(nested.join("notes.txt"), "").expect("w");
    // §4.2's flat glob would find neither of these.
    std::fs::write(dir.path().join("rollout-flat.jsonl"), "").expect("w");
    let found = rollouts(dir.path());
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().all(|p| p
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with("rollout-"))));
}

/// CONFLICT-4 and B.4: Codex has no `PostToolUseFailure`, because its
/// `PostToolUse` already fires after a non-zero exit. Subscribing to a
/// non-existent event captures nothing and looks fine, which is why it is
/// asserted rather than assumed.
#[test]
fn codex_has_one_tool_post_event_and_not_two() {
    assert!(SUBSCRIBED_EVENTS.contains(&"PostToolUse"));
    assert!(
        !SUBSCRIBED_EVENTS.contains(&"PostToolUseFailure"),
        "that event is Claude Code's; Codex reports a non-zero Bash exit \
         through PostToolUse"
    );
    assert_eq!(event_kind_for("PostToolUse"), Some("tool_result"));
    assert_eq!(event_kind_for("Stop"), Some("stop"));
    assert_eq!(event_kind_for("PreCompact"), Some("compaction_pre"));
    assert_eq!(event_kind_for("PostCompact"), Some("compaction_post"));
    assert_eq!(event_kind_for("AfterTool"), None, "that is Gemini's name");
}
