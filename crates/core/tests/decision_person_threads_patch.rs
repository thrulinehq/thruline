#[path = "fixtures/decisions/deferred/support.rs"]
mod support;

use serde_json::{json, Value};
use support::*;
use thruline_core::decisions::person_threads::{open_for_project, Coverage};

const MODIFY: &str = "*** Begin Patch\n*** Update File: REQUIRED\n@@\n-ready\n+done\n*** End Patch";
const SUCCESS: &str = "Success. Updated the following files:\n";

fn patch(id: &str, text: &str) -> Value {
    json!({"type":"response_item","payload":{"type":"custom_tool_call","name":"apply_patch","call_id":id,"input":text}})
}

fn fixture(request: &str, body: &str, code: i32) -> Fixture {
    Fixture::new(vec![
        prompt("Fix the approval button."),
        patch("patch-one", request),
        output("patch-one", body, code),
    ])
}

fn retained(request: &str, body: &str) {
    let f = fixture(request, body, 0);
    let changes = f.ledger.conn().total_changes();
    let got = open_for_project(&f.ledger, PROJECT).unwrap();
    assert_eq!(
        got.threads.len(),
        1,
        "ineligible completion must retain its work interval"
    );
    assert_eq!(got.observed_edit_threads, 0);
    assert_eq!(got.threads[0].coverage, Coverage::MissingSource);
    assert!(got.missing.iter().any(|why| why.contains("event-4")));
    assert!(got.threads[0].qualification.contains("completion"));
    assert_eq!(f.ledger.conn().total_changes(), changes);
}

#[test]
fn l1_patch_header_without_a_file_is_not_completion_evidence() {
    retained(MODIFY, SUCCESS);
}

#[test]
fn l1_patch_result_requires_named_closed_actions() {
    for suffix in [
        "M ",
        "M",
        "X REQUIRED",
        "modified REQUIRED",
        "M REQUIRED\n\n",
        "\n",
    ] {
        retained(MODIFY, &format!("{SUCCESS}{suffix}"));
    }
}

#[test]
fn l1_patch_result_must_match_request_path_and_action() {
    for suffix in [
        "M OTHER",
        "A REQUIRED",
        "D REQUIRED",
        "M REQUIRED\nM OTHER",
        "M REQUIRED\nM REQUIRED",
    ] {
        retained(MODIFY, &format!("{SUCCESS}{suffix}"));
    }
}

#[test]
fn l1_patch_result_cannot_hide_extra_prose_or_an_unmatched_entry() {
    for body in [
        format!("{SUCCESS}M REQUIRED\nDone."),
        format!("{SUCCESS}M REQUIRED\n*** End Patch"),
        format!("{SUCCESS}M REQUIRED\nA "),
        format!("{SUCCESS}M REQUIRED\nM\tOTHER"),
    ] {
        retained(MODIFY, &body);
    }
}

#[test]
fn l1_patch_request_requires_closed_named_sections() {
    for request in [
        "*** Begin Patch\n*** End Patch",
        "*** Update File: REQUIRED\n@@\n-ready\n+done\n*** End Patch",
        "*** Begin Patch\n*** Update File: REQUIRED\n@@\n-ready\n+done",
        "*** Begin Patch\n*** Update File: \n@@\n-ready\n+done\n*** End Patch",
        "*** Begin Patch\n*** Update File: REQUIRED\nnot patch content\n*** End Patch",
        "*** Begin Patch\n*** Update File: REQUIRED\n@@\n ready\n*** End Patch",
        "*** Begin Patch\n*** Update File: REQUIRED\n*** Move to: OTHER\n@@\n-ready\n+done\n*** End Patch",
        "*** Begin Patch\n*** Delete File: REQUIRED\n+unexpected\n*** End Patch",
        "*** Begin Patch\n*** Update File: REQUIRED\n@@\n-ready\n+done\n*** End Patch\nextra",
        "*** Begin Patch\n*** Update File: REQUIRED\n@@\n-ready\n+done\n*** Update File: REQUIRED\n@@\n-done\n+again\n*** End Patch",
    ] {
        retained(request, &format!("{SUCCESS}M REQUIRED"));
    }
}

#[test]
fn l1_patch_all_requested_files_need_exact_completed_entries() {
    let request = "*** Begin Patch\n*** Add File: new file.txt\n+created\n*** Update File: REQUIRED\n@@\n-ready\n+done\n*** Delete File: old.txt\n*** End Patch";
    retained(request, &format!("{SUCCESS}M REQUIRED"));
    let f = fixture(
        request,
        &format!("{SUCCESS}D old.txt\nA new file.txt\nM REQUIRED\n"),
        0,
    );
    let changes = f.ledger.conn().total_changes();
    let got = open_for_project(&f.ledger, PROJECT).unwrap();
    assert!(got.threads.is_empty());
    assert_eq!(got.observed_edit_threads, 1);
    assert!(got.missing.is_empty());
    assert_eq!(f.ledger.conn().total_changes(), changes);
}

#[test]
fn l1_patch_native_add_modify_delete_entries_are_completion_observations() {
    for (request, body) in [
        (
            "*** Begin Patch\n*** Add File: new.txt\n+created\n*** End Patch",
            "A new.txt",
        ),
        (MODIFY, "M REQUIRED"),
        (
            "*** Begin Patch\n*** Delete File: old.txt\n*** End Patch",
            "D old.txt",
        ),
    ] {
        let f = fixture(request, &format!("{SUCCESS}{body}"), 0);
        let got = open_for_project(&f.ledger, PROJECT).unwrap();
        assert!(got.threads.is_empty());
        assert_eq!(got.observed_edit_threads, 1);
        assert!(got.missing.is_empty());
    }
}

#[test]
fn l1_patch_source_and_normalized_fields_remain_verified() {
    let f = fixture(MODIFY, &format!("{SUCCESS}M REQUIRED"), 0);
    // Deliberate normalized-row corruption on this authored scratch fixture.
    // The production append-only guard remains unchanged.
    f.ledger
        .conn()
        .execute_batch("DROP TRIGGER events_no_update")
        .unwrap();
    f.ledger
        .conn()
        .execute(
            "UPDATE events SET tool_input=?1 WHERE id='event-4'",
            [
                json!({"patch":"*** Begin Patch\n*** Delete File: OTHER\n*** End Patch"})
                    .to_string(),
            ],
        )
        .unwrap();
    let got = open_for_project(&f.ledger, PROJECT).unwrap();
    assert_eq!(got.threads.len(), 1);
    assert_eq!(got.threads[0].coverage, Coverage::MissingSource);
    assert_eq!(got.observed_edit_threads, 0);

    let f = fixture(MODIFY, &format!("{SUCCESS}M REQUIRED"), 0);
    let raw = std::fs::read_to_string(&f.raw).unwrap();
    std::fs::write(&f.raw, raw.replace("M REQUIRED", "M OTHER")).unwrap();
    let got = open_for_project(&f.ledger, PROJECT).unwrap();
    assert_eq!(got.threads.len(), 1);
    assert_eq!(got.threads[0].coverage, Coverage::MissingSource);
    assert_eq!(got.observed_edit_threads, 0);
}

#[test]
fn l1_patch_request_alone_failure_and_wrong_call_result_stay_open() {
    for items in [
        vec![
            prompt("Fix the approval button."),
            patch("patch-one", MODIFY),
        ],
        vec![
            prompt("Fix the approval button."),
            patch("patch-one", MODIFY),
            output("patch-one", &format!("{SUCCESS}M REQUIRED"), 1),
        ],
        vec![
            prompt("Fix the approval button."),
            patch("patch-one", MODIFY),
            output("other-call", &format!("{SUCCESS}M REQUIRED"), 0),
        ],
    ] {
        let f = Fixture::new(items);
        let got = open_for_project(&f.ledger, PROJECT).unwrap();
        assert_eq!(got.threads.len(), 1);
        assert_eq!(got.observed_edit_threads, 0);
    }
}
