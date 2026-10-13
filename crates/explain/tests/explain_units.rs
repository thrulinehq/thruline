//! §10, one fixture per question, each with the control that keeps it honest.

use thruline_explain::compaction::{compaction_loss, PreCompaction};
use thruline_explain::narrative::honest_failure;
use thruline_explain::stop::{classify_stop, EventView, StopReasonKind};
use thruline_explain::timeline::{build, TimelineEvent};

fn ev(seq: u64, kind: &str, summary: &str) -> EventView {
    EventView {
        seq,
        kind: kind.into(),
        summary: summary.into(),
        stop_reason: None,
        failed_tool: None,
    }
}

#[test]
fn a_stop_with_no_stop_event_is_named_rather_than_guessed() {
    let c = classify_stop(&[ev(1, "session_start", "start"), ev(2, "tool_call", "Edit")]);
    assert_eq!(c.kind, StopReasonKind::NoStopRecorded);
    assert!(c.summary.contains("still open"));
}

#[test]
fn a_finished_turn_whose_last_act_failed_is_reported_as_an_error() {
    // "Finished" is the harness's word for "the turn ended", not for "the work
    // succeeded". Reporting it as a clean finish is how a failed session looks
    // fine in a summary.
    let mut failed = ev(2, "tool_result", "Bash failed");
    failed.failed_tool = Some("`npm test` exited 1".into());
    let mut stop = ev(3, "stop", "stop");
    stop.stop_reason = Some("agent_finished".into());
    let c = classify_stop(&[ev(1, "session_start", "start"), failed, stop]);
    assert_eq!(c.kind, StopReasonKind::Error);
    assert!(c.summary.contains("npm test"), "{}", c.summary);
}

#[test]
fn a_clean_finish_stays_a_clean_finish() {
    let mut stop = ev(3, "stop", "stop");
    stop.stop_reason = Some("agent_finished".into());
    let c = classify_stop(&[
        ev(1, "session_start", "s"),
        ev(2, "tool_result", "ok"),
        stop,
    ]);
    assert_eq!(c.kind, StopReasonKind::AgentFinished);
}

#[test]
fn the_last_three_events_before_the_stop_are_carried() {
    let mut stop = ev(9, "stop", "stop");
    stop.stop_reason = Some("user_interrupt".into());
    let c = classify_stop(&[
        ev(1, "tool_call", "a"),
        ev(2, "tool_call", "b"),
        ev(3, "tool_call", "c"),
        ev(4, "tool_call", "d"),
        stop,
    ]);
    assert_eq!(c.last_events, vec!["b", "c", "d"]);
    assert_eq!(c.kind, StopReasonKind::UserInterrupt);
}

#[test]
fn a_promise_never_mentioned_after_a_compaction_is_lost() {
    let pre = PreCompaction {
        seq: 10,
        open_promises: vec!["add retry handling to the uploader".into()],
        open_tasks: vec!["update the changelog".into()],
        ..PreCompaction::default()
    };
    let after = vec![(11u64, "I updated the changelog and shipped".to_string())];
    let loss = compaction_loss(&[pre], &after);
    assert_eq!(loss.compactions, 1);
    let lost: Vec<&str> = loss.lost.iter().map(|l| l.what.as_str()).collect();
    assert!(lost.iter().any(|l| l.contains("retry")), "{lost:?}");
    assert!(
        !lost.iter().any(|l| l.contains("changelog")),
        "a task that WAS picked up again was reported as lost: {lost:?}"
    );
}

#[test]
fn an_empty_compaction_event_is_unmeasurable_and_says_so() {
    // The distinction that keeps an empty report from reading as a clean one.
    let loss = compaction_loss(
        &[PreCompaction {
            seq: 1,
            ..PreCompaction::default()
        }],
        &[],
    );
    assert!(loss.lost.is_empty());
    assert!(loss.unmeasurable.is_some(), "{loss:?}");
    assert!(loss
        .unmeasurable
        .unwrap()
        .contains("not because nothing was lost"));
}

#[test]
fn no_compaction_means_no_report_and_no_warning() {
    let loss = compaction_loss(&[], &[]);
    assert_eq!(loss.compactions, 0);
    assert!(loss.unmeasurable.is_none());
}

#[test]
fn an_honest_failure_with_no_claim_reads_as_honest() {
    let h = honest_failure("I couldn't get the tests running on this machine.", false);
    assert!(h.reported_failure && h.without_completion_claim);
    assert!(h
        .narrative()
        .unwrap()
        .contains("claimed nothing it did not do"));
}

#[test]
fn a_failure_marker_beside_a_completion_claim_is_named_and_excuses_nothing() {
    // The shape that made §6.9 exploitable. Explain says both things; the
    // verdict is unaffected, because the verdict never sees this at all.
    let h = honest_failure(
        "All tests pass. I couldn't run the integration suite.",
        true,
    );
    assert!(h.reported_failure);
    assert!(!h.without_completion_claim);
    assert!(h
        .narrative()
        .unwrap()
        .contains("judged on evidence regardless"));
}

#[test]
fn a_message_with_no_markers_produces_no_narrative() {
    let h = honest_failure("Added the handling fee and updated the tests.", true);
    assert!(!h.reported_failure);
    assert!(h.narrative().is_none());
}

#[test]
fn consecutive_calls_on_one_tool_collapse_so_the_timeline_stays_readable() {
    let events: Vec<TimelineEvent> = (0..5)
        .map(|i| TimelineEvent {
            seq: i,
            kind: "tool_call".into(),
            text: "Edit".into(),
            argv: vec![],
            command: None,
            ..Default::default()
        })
        .collect();
    let t = build(&events);
    assert_eq!(t.entries.len(), 1);
    assert_eq!(t.entries[0].text, "Edit (x5)");
    assert_eq!(t.grouped, 4);
}

/// The run key is the whole call, not the 50-character line `summarize` cuts.
///
/// Two `Bash` calls that share a long prologue render the same `text`, and the
/// old key folded them into one line reading `(x2)` — a line that asserted two
/// different commands were the same call, and that now would have to carry one
/// of the two `input`s as if it belonged to both.
#[test]
fn two_different_commands_that_share_a_prefix_are_not_folded_into_one_line() {
    let same_text = "Bash {\"command\":\"cd /repo && set -e && make…";
    let events = vec![
        TimelineEvent {
            seq: 1,
            kind: "tool_call".into(),
            text: same_text.into(),
            tool: Some("Bash".into()),
            input: Some("{\"command\":\"cd /repo && set -e && make build\"}".into()),
            ..Default::default()
        },
        TimelineEvent {
            seq: 2,
            kind: "tool_call".into(),
            text: same_text.into(),
            tool: Some("Bash".into()),
            input: Some("{\"command\":\"cd /repo && set -e && make deploy\"}".into()),
            ..Default::default()
        },
    ];
    let t = build(&events);
    assert_eq!(
        t.entries.len(),
        2,
        "two distinct commands were folded into one line: {:?}",
        t.entries
    );
    assert_eq!(t.grouped, 0);
    assert!(t.entries[1].input.as_deref().unwrap().contains("deploy"));
}

/// Genuinely identical calls still fold, and the folded line keeps the call's
/// own arguments — `(xN)` means N of exactly this.
#[test]
fn identical_calls_still_fold_and_the_folded_line_keeps_its_arguments() {
    let events: Vec<TimelineEvent> = (0..3)
        .map(|i| TimelineEvent {
            seq: i,
            kind: "tool_call".into(),
            text: "Read {\"file_path\":\"src/lib.rs\"}".into(),
            tool: Some("Read".into()),
            input: Some("{\"file_path\":\"src/lib.rs\"}".into()),
            ..Default::default()
        })
        .collect();
    let t = build(&events);
    assert_eq!(t.entries.len(), 1);
    assert_eq!(t.grouped, 2);
    assert_eq!(t.entries[0].tool.as_deref(), Some("Read"));
    assert_eq!(
        t.entries[0].input.as_deref(),
        Some("{\"file_path\":\"src/lib.rs\"}")
    );
}

/// A row with no tool carries neither field, and serializes to the same bytes
/// it did before they existed — the property that keeps this additive.
#[test]
fn an_event_with_no_tool_serializes_exactly_as_it_did_before() {
    let t = build(&[TimelineEvent {
        seq: 7,
        kind: "user_prompt".into(),
        text: "fix the flaky test".into(),
        ..Default::default()
    }]);
    let row = serde_json::to_string(&t.entries[0]).unwrap();
    assert_eq!(
        row,
        r#"{"seq":7,"kind":"user_prompt","text":"fix the flaky test"}"#
    );
}

#[test]
fn a_destructive_command_is_flagged_and_never_grouped_away() {
    let events = vec![
        TimelineEvent {
            seq: 1,
            kind: "tool_call".into(),
            text: "Bash".into(),
            argv: vec![],
            command: Some("ls".into()),
            ..Default::default()
        },
        TimelineEvent {
            seq: 2,
            kind: "tool_call".into(),
            text: "Bash".into(),
            argv: vec![],
            command: Some("rm -rf build/".into()),
            ..Default::default()
        },
        TimelineEvent {
            seq: 3,
            kind: "tool_call".into(),
            text: "Bash".into(),
            argv: vec![],
            command: Some("ls".into()),
            ..Default::default()
        },
    ];
    let t = build(&events);
    assert_eq!(t.risks.len(), 1);
    assert!(t.risks[0].risk.as_ref().unwrap().contains("rm -rf"));
    assert_eq!(
        t.entries.len(),
        3,
        "a risky call was grouped out of sight: {:?}",
        t.entries
    );
}

#[test]
fn risk_is_judged_on_argv_when_there_is_one() {
    let events = vec![TimelineEvent {
        seq: 1,
        kind: "exec".into(),
        text: "exec".into(),
        argv: vec!["git".into(), "reset".into(), "--hard".into()],
        command: None,
        ..Default::default()
    }];
    assert_eq!(build(&events).risks.len(), 1);
}

#[test]
fn an_ordinary_command_is_not_flagged() {
    let events = vec![TimelineEvent {
        seq: 1,
        kind: "exec".into(),
        text: "exec".into(),
        argv: vec!["pytest".into(), "-q".into()],
        command: None,
        ..Default::default()
    }];
    assert!(build(&events).risks.is_empty());
}
