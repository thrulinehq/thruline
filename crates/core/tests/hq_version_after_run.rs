//! C54 changed#12: an edit after a recorded test run changes the version that
//! the run examined, even when the agent did not make a Git commit between them.

mod scratch;

use scratch::TempTree;
use serde_json::{json, Value};
use thruline_core::hq;
use thruline_core::ingest::ingest;
use thruline_core::Ledger;

#[test]
fn a_test_run_and_a_later_edit_without_a_commit_have_different_versions() {
    let tmp = TempTree::new();
    let db = tmp.path().join("ledger.db");
    let source = tmp.path().join("claude/-work-version-after-run");
    std::fs::create_dir_all(&source).unwrap();
    let session = "cccccccc-1111-4111-8111-111111111111";
    let row = |n: u32, kind: &str, content: Value| {
        json!({
            "parentUuid": null, "isSidechain": false,
            "uuid": format!("cccccccc-0000-4000-8000-{n:012}"),
            "timestamp": format!("2026-10-01T09:0{n}:00Z"),
            "cwd": "/work/version-after-run", "sessionId": session,
            "type": kind,
            "message": { "role": if kind == "user" { "user" } else { "assistant" }, "content": content }
        })
    };
    let lines = [
        row(1, "user", json!("Fix the cart total and check it.")),
        row(
            2,
            "assistant",
            json!([{"type":"tool_use","id":"edit-first","name":"Edit","input":{"file_path":"/work/version-after-run/cart/total.py","old_string":"a","new_string":"b"}}]),
        ),
        row(
            3,
            "user",
            json!([{"type":"tool_result","tool_use_id":"edit-first","content":"Updated successfully.","is_error":false}]),
        ),
        row(
            4,
            "assistant",
            json!([{"type":"tool_use","id":"run-tests","name":"Bash","input":{"command":"pytest -q cart/test_total.py"}}]),
        ),
        row(
            5,
            "user",
            json!([{"type":"tool_result","tool_use_id":"run-tests","content":"4 passed in 0.01s","is_error":false}]),
        ),
        row(
            6,
            "assistant",
            json!([{"type":"tool_use","id":"edit-later","name":"Edit","input":{"file_path":"/work/version-after-run/cart/later.py","old_string":"a","new_string":"b"}}]),
        ),
        row(
            7,
            "user",
            json!([{"type":"tool_result","tool_use_id":"edit-later","content":"Updated successfully.","is_error":false}]),
        ),
    ];
    std::fs::write(
        source.join(format!("{session}.jsonl")),
        lines
            .iter()
            .map(|line| format!("{line}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let mut ledger = Ledger::open(&db).unwrap();
    ingest(&mut ledger, &[source], &tmp.path().join("mirror")).unwrap();
    let project_id = hq::project_id_for_root("/work/version-after-run");
    let now = hq::parse_ms("2026-10-02T00:00:00Z").unwrap();
    let project = hq::project(&ledger, &project_id, now).unwrap().unwrap();
    let thread_id = &project.threads[0].id;
    let thread = hq::thread(&ledger, &project_id, thread_id, now)
        .unwrap()
        .unwrap();
    let run = thread
        .commands
        .iter()
        .find(|c| c.is_test)
        .expect("test run");
    let later = thread
        .edits
        .iter()
        .find(|e| e.file.ends_with("cart/later.py"))
        .expect("later edit");
    assert_ne!(
        run.version_id, later.version_id,
        "the later edit changed the files the run examined"
    );
    assert_eq!(thread.current_version_id, later.version_id);
}
