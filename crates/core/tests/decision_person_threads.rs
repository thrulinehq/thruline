#[path = "fixtures/decisions/deferred/support.rs"]
mod support;
use support::*;
use thruline_core::decisions::person_threads::{open_for_project, Coverage};
#[test]
fn l1_canonical_person_work_stays_open_despite_agent_completion_words() {
    let f = Fixture::new(vec![
        prompt("Fix the approval button."),
        prose("Done, implemented and tested."),
    ]);
    let got = open_for_project(&f.ledger, PROJECT).unwrap();
    assert_eq!(got.threads.len(), 1);
    assert_eq!(got.threads[0].prompt, "Fix the approval button.");
    assert_eq!(got.threads[0].coverage, Coverage::ObservedPrefix);
    assert!(got.threads[0].qualification.contains("recorded"));
    assert_eq!(got.threads[0].source.event_id, "event-3");
}
#[test]
fn l1_verified_completed_edit_following_prompt_is_not_an_open_no_edit_row() {
    let f = Fixture::new(vec![
        prompt("Fix the approval button."),
        edit("edit-one"),
        output(
            "edit-one",
            "Success. Updated the following files:\nM REQUIRED",
            0,
        ),
    ]);
    let got = open_for_project(&f.ledger, PROJECT).unwrap();
    assert!(got.threads.is_empty());
    assert_eq!(got.observed_edit_threads, 1);
}
#[test]
fn l1_failed_missing_or_pre_prompt_edit_does_not_close_prompt() {
    for items in [
        vec![prompt("Fix the approval button."), edit("edit-one")],
        vec![
            prompt("Fix the approval button."),
            edit("edit-one"),
            output("edit-one", "failed", 1),
        ],
        vec![
            edit("edit-one"),
            output(
                "edit-one",
                "Success. Updated the following files:\nM REQUIRED",
                0,
            ),
            prompt("Fix the approval button."),
        ],
    ] {
        let f = Fixture::new(items);
        assert_eq!(
            open_for_project(&f.ledger, PROJECT).unwrap().threads.len(),
            1
        );
    }
}
#[test]
fn l1_source_failure_is_named_and_nonperson_injected_or_conversation_is_not_work() {
    let f = Fixture::new(vec![
        prompt("<subagent_notification>Fix the approval button.</subagent_notification>"),
        prompt("Thanks!"),
        prose("Fix the approval button."),
    ]);
    assert!(open_for_project(&f.ledger, PROJECT)
        .unwrap()
        .threads
        .is_empty());
    let f = Fixture::new(vec![prompt("Fix the approval button.")]);
    std::fs::write(&f.raw, "missing source\n").unwrap();
    let got = open_for_project(&f.ledger, PROJECT).unwrap();
    assert!(got.threads.is_empty());
    assert!(!got.missing.is_empty());
}
#[test]
fn l1_later_prompt_starts_a_distinct_observed_work_interval() {
    let f = Fixture::new(vec![
        prompt("Fix the approval button."),
        prompt("Add the status line."),
        edit("edit-two"),
        output(
            "edit-two",
            "Success. Updated the following files:\nM REQUIRED",
            0,
        ),
    ]);
    let got = open_for_project(&f.ledger, PROJECT).unwrap();
    assert_eq!(got.threads.len(), 1);
    assert_eq!(got.threads[0].prompt, "Fix the approval button.");
    assert_eq!(got.observed_edit_threads, 1);
}

#[test]
fn l1_edit_result_after_next_prompt_does_not_close_either_person_interval() {
    let f = Fixture::new(vec![
        prompt("Fix the approval button."),
        edit("edit-one"),
        prompt("Add the status line."),
        output(
            "edit-one",
            "Success. Updated the following files:\nM REQUIRED",
            0,
        ),
    ]);
    let got = open_for_project(&f.ledger, PROJECT).unwrap();
    assert_eq!(got.threads.len(), 2);
    assert_eq!(got.observed_edit_threads, 0);
}

fn native_edit(tool: &str, echo: serde_json::Value, is_error: bool) -> Fixture {
    use serde_json::json;
    Fixture::claude(vec![
        json!({"type":"user","message":{"role":"user","content":"Fix the approval button."}}),
        json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"native-edit","name":tool,"input":{"file_path":"REQUIRED","old_string":"ready","new_string":"done","content":"done"}}]}}),
        json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"native-edit","content":"updated","is_error":is_error}]},"toolUseResult":echo}),
    ])
}

#[test]
fn l1_native_claude_completed_edit_and_write_have_typed_echoes() {
    use serde_json::json;
    for (tool, echo) in [
        (
            "Edit",
            json!({"filePath":"REQUIRED","oldString":"ready","newString":"done"}),
        ),
        ("Write", json!({"filePath":"REQUIRED","content":"done"})),
    ] {
        let f = native_edit(tool, echo, false);
        let got = open_for_project(&f.ledger, PROJECT).unwrap();
        assert!(got.threads.is_empty());
        assert_eq!(got.observed_edit_threads, 1);
        assert!(got.missing.is_empty());
    }
}

#[test]
fn l1_native_claude_null_or_wrong_tool_echo_and_error_are_not_completed_edits() {
    use serde_json::json;
    for (tool, echo, error) in [
        (
            "Edit",
            json!({"filePath":"REQUIRED","oldString":null,"newString":null}),
            false,
        ),
        (
            "Write",
            json!({"filePath":"REQUIRED","content":null}),
            false,
        ),
        (
            "Edit",
            json!({"filePath":"REQUIRED","content":"done"}),
            false,
        ),
        (
            "Write",
            json!({"filePath":"REQUIRED","oldString":"ready","newString":"done"}),
            false,
        ),
        (
            "Edit",
            json!({"filePath":"REQUIRED","oldString":"ready","newString":"done"}),
            true,
        ),
    ] {
        let f = native_edit(tool, echo, error);
        let got = open_for_project(&f.ledger, PROJECT).unwrap();
        assert_eq!(got.threads.len(), 1, "{tool} must retain its open row");
        assert_eq!(got.observed_edit_threads, 0);
        assert!(got.missing.is_empty());
    }
}

#[test]
fn l1_missing_edit_receipt_is_named_and_qualifies_the_observed_interval() {
    let f = Fixture::new(vec![prompt("Fix the approval button."), edit("unfinished")]);
    let got = open_for_project(&f.ledger, PROJECT).unwrap();
    assert_eq!(got.threads.len(), 1);
    assert_eq!(got.threads[0].coverage, Coverage::MissingSource);
    assert!(got.missing.iter().any(|why| why.contains("event-4")));
    assert_eq!(got.observed_edit_threads, 0);
}
