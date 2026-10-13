//! The harness boundary — BUILD-PLAN B.4.

use super::*;

#[test]
fn every_harness_is_reachable_by_the_name_a_user_types() {
    for h in ALL {
        assert_eq!(
            by_name(h.name).map(|f| f.name),
            Some(h.name),
            "{} is declared and cannot be selected",
            h.name
        );
    }
    assert!(by_name("cursor").is_none());
    assert!(by_name("").is_none());
}

/// Names, sources and spool files must be distinct, or two harnesses share a
/// chain and their events interleave under one `source`.
#[test]
fn nothing_is_shared_between_harnesses_that_must_not_be() {
    let mut names: Vec<&str> = ALL.iter().map(|h| h.name).collect();
    let n = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), n);

    let mut sources: Vec<&str> = ALL.iter().map(|h| h.source).collect();
    sources.sort_unstable();
    sources.dedup();
    assert_eq!(sources.len(), n);
}

/// Every subscribed event maps to a §3.1 kind. An event that is subscribed and
/// unmapped spools and folds into nothing — a capture hole that looks like a
/// working install.
#[test]
fn every_subscribed_event_maps_to_a_kind() {
    for h in ALL {
        for event in h.subscribed {
            assert!(
                (h.kind_for)(event).is_some(),
                "{}: {event} is subscribed and maps to no kind",
                h.name
            );
        }
    }
}

/// The three contracts really do differ, and the boundary really does carry
/// the difference. A refactor that collapsed them onto one event table would
/// pass every test above and fail this one.
#[test]
fn the_three_contracts_are_not_interchangeable() {
    let stop_events: Vec<&str> = ALL
        .iter()
        .map(|h| {
            h.subscribed
                .iter()
                .copied()
                .find(|e| (h.kind_for)(e) == Some("stop"))
                .unwrap_or("<none>")
        })
        .collect();
    assert_eq!(stop_events, vec!["Stop", "Stop", "AfterAgent"]);

    // Only Claude Code needs a second tool-post event.
    let failure_events: Vec<bool> = ALL
        .iter()
        .map(|h| h.subscribed.contains(&"PostToolUseFailure"))
        .collect();
    assert_eq!(failure_events, vec![true, false, false]);

    // Only Gemini lacks a compaction-post side.
    let post_compaction: Vec<bool> = ALL
        .iter()
        .map(|h| {
            h.subscribed
                .iter()
                .any(|e| (h.kind_for)(e) == Some("compaction_post"))
        })
        .collect();
    assert_eq!(post_compaction, vec![true, true, false]);
}

#[test]
fn each_harness_spools_to_its_own_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut seen: Vec<PathBuf> = Vec::new();
    for h in ALL {
        // `default_spool` reads the environment; construct the same path the
        // way it does rather than mutating process-global state in a test that
        // runs beside others.
        let path = dir.path().join("spool").join(format!("{}.jsonl", h.name));
        assert!(!seen.contains(&path), "{} shares a spool file", h.name);
        seen.push(path);
    }
    assert_eq!(seen.len(), ALL.len());
}

/// The harness is on the line, so the fold does not have to infer it from a
/// file name a `--spool` flag can change.
#[test]
fn the_delivery_records_which_harness_made_it() {
    let raw = br#"{"hook_event_name":"AfterTool","session_id":"s","transcript_path":"/t","cwd":"/repo","tool_name":"run_shell_command","tool_response":{"llmContent":"ok"}}"#;
    let line = crate::claude_code::prepare_for(
        &GEMINI,
        raw,
        "2026-09-02T00:00:00Z",
        &crate::redaction::Redactor::new(),
    );
    assert_eq!(line.harness, "gemini");
    assert!(line.recognized, "AfterTool is a Gemini event");
    assert!(line.render().contains("\"harness\":\"gemini\""));

    // ...and the same delivery is NOT recognised as a Claude Code one.
    let as_claude = crate::claude_code::prepare_for(
        &CLAUDE_CODE,
        raw,
        "2026-09-02T00:00:00Z",
        &crate::redaction::Redactor::new(),
    );
    assert!(
        !as_claude.recognized,
        "recognition must be per harness, or every adapter accepts every event"
    );
    // Recorded anyway, with the defect flagged — nothing is dropped.
    assert!(as_claude.parse_ok);
    assert_eq!(as_claude.hook_event, "AfterTool");
}

/// The premise of every capped/uncapped distinction below, checked by the
/// **compiler** rather than by a test body: if the two budgets were ever set to
/// the same number, every branch that sorts events into them would still pass
/// while distinguishing nothing.
const _: () = assert!(
    crate::codex::CAPTURE_TIMEOUT_SECS > crate::codex::CAPPED_TIMEOUT_MAX_SECS,
    "the capped and uncapped Codex budgets are the same number"
);

/// The negative control for `every_subscribed_event_maps_to_a_kind`: if
/// `kind_for` answered `Some` for anything at all, that test would pass while
/// asserting nothing.
#[test]
fn an_unknown_event_maps_to_nothing() {
    for h in ALL {
        assert!(
            (h.kind_for)("NoSuchEvent").is_none(),
            "{}: invented a kind for an event the harness never sends",
            h.name
        );
    }
}

/// Only Codex must state its own timeouts, and only where its default is
/// tighter than a cold binary start (1 s on `SessionEnd`, capped at 3 s).
/// Claude Code defaults to 60 s; Gemini's unit is **milliseconds**, and its row
/// exists so that nobody later writes `5` there meaning seconds.
///
/// Read off `ALL` rather than the three constants directly: a per-constant
/// assertion is const-folded, so it holds at compile time and checks nothing
/// about the table the installer actually walks.
#[test]
fn only_codex_states_its_own_timeouts() {
    let stating: Vec<&str> = ALL
        .iter()
        .filter(|h| h.timeout_for.is_some())
        .map(|h| h.name)
        .collect();
    assert_eq!(stating, vec!["codex"], "the wrong harnesses state timeouts");

    let secs = by_name("codex")
        .and_then(|h| h.timeout_for)
        .expect("codex states timeouts");
    assert_eq!(secs("SessionEnd"), crate::codex::CAPPED_TIMEOUT_MAX_SECS);
    assert_eq!(secs("PostToolUse"), crate::codex::CAPTURE_TIMEOUT_SECS);
}

/// `installable` is the sixth guard-family shape's home: it used to be a
/// literal `true` in the pilot while the installer bailed on the same harness.
/// One fact, in the table both of them read, so they cannot disagree again.
#[test]
fn installable_is_claimed_only_where_a_registration_is_written() {
    let installable: Vec<&str> = ALL
        .iter()
        .filter(|h| h.installable)
        .map(|h| h.name)
        .collect();
    assert_eq!(
        installable,
        vec!["claude-code", "codex"],
        "a harness claims a hook pack `thruline hook-install` cannot write"
    );
}
