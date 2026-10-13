//! C54 approve#1: a successful Git commit owns its result row, so the current
//! version remains the recorded commit instead of a trailing empty worktree.

mod scratch;

use scratch::TempTree;
use serde_json::{json, Value};
use thruline_core::hq;
use thruline_core::ingest::ingest;
use thruline_core::Ledger;

#[test]
fn a_commit_result_does_not_create_a_new_uncommitted_current_version() {
    let tmp = TempTree::new();
    let db = tmp.path().join("ledger.db");
    let source = tmp.path().join("claude/-work-commit-result");
    std::fs::create_dir_all(&source).unwrap();
    let session = "cccccccc-2222-4222-8222-222222222222";
    let row = |n: u32, kind: &str, content: Value| {
        json!({
            "parentUuid": null, "isSidechain": false,
            "uuid": format!("cccccccc-0000-4000-8000-{n:012}"),
            "timestamp": format!("2026-10-01T09:0{n}:00Z"),
            "cwd": "/work/commit-result", "sessionId": session,
            "type": kind,
            "message": { "role": if kind == "user" { "user" } else { "assistant" }, "content": content }
        })
    };
    let lines = [
        row(1, "user", json!("Save the cart total fix.")),
        row(
            2,
            "assistant",
            json!([{"type":"tool_use","id":"edit-first","name":"Edit","input":{"file_path":"/work/commit-result/cart/total.py","old_string":"a","new_string":"b"}}]),
        ),
        row(
            3,
            "user",
            json!([{"type":"tool_result","tool_use_id":"edit-first","content":"Updated successfully.","is_error":false}]),
        ),
        row(
            4,
            "assistant",
            json!([{"type":"tool_use","id":"commit","name":"Bash","input":{"command":"git commit -m 'Save cart total'"}}]),
        ),
        row(
            5,
            "user",
            json!([{"type":"tool_result","tool_use_id":"commit","content":"[main 1a2b3c4] Save cart total\n 1 file changed","is_error":false}]),
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
    let project_id = hq::project_id_for_root("/work/commit-result");
    let now = hq::parse_ms("2026-10-02T00:00:00Z").unwrap();
    let project = hq::project(&ledger, &project_id, now).unwrap().unwrap();
    let thread = hq::thread(&ledger, &project_id, &project.threads[0].id, now)
        .unwrap()
        .unwrap();
    assert_eq!(thread.current_version_id.as_deref(), Some("1a2b3c4"));
    assert_eq!(
        thread.versions.last().map(|v| v.id.as_str()),
        Some("1a2b3c4")
    );
}
