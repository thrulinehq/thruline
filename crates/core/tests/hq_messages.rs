//! Every statement can show an agent sentence even when no claim rule matches it.

mod scratch;

use scratch::TempTree;
use serde_json::json;
use thruline_core::{hq, ingest::ingest, Ledger};

#[test]
fn thread_view_serves_unclassified_agent_text_as_masked_read_only_messages() {
    let tmp = TempTree::new();
    let source = tmp.path().join("-work-app");
    std::fs::create_dir_all(&source).unwrap();
    let session = "cccccccc-1111-4111-8111-111111111111";
    let row = |id: &str, time: &str, kind: &str, content: serde_json::Value| {
        json!({
            "parentUuid": null, "isSidechain": false, "uuid": id,
            "timestamp": time, "cwd": "/work/app", "sessionId": session,
            "version": "2.1.0", "gitBranch": "main", "userType": "external", "type": kind,
            "message": {"role": if kind == "user" { "user" } else { "assistant" }, "model": "m", "content": content}
        })
    };
    let lines = [
        row(
            "cccccccc-0000-4000-8000-000000000001",
            "2026-10-07T10:00:00Z",
            "user",
            json!("Fix the cart."),
        ),
        row(
            "cccccccc-0000-4000-8000-000000000002",
            "2026-10-07T10:01:00Z",
            "assistant",
            json!([{"type":"tool_use", "id":"edit-1", "name":"Edit", "input":{"file_path":"/work/app/cart.py", "old_string":"a", "new_string":"b"}}]),
        ),
        row(
            "cccccccc-0000-4000-8000-000000000003",
            "2026-10-07T10:01:01Z",
            "user",
            json!([{"type":"tool_result", "tool_use_id":"edit-1", "content":"updated", "is_error":false}]),
        ),
        row(
            "cccccccc-0000-4000-8000-000000000004",
            "2026-10-07T10:02:00Z",
            "assistant",
            json!([{"type":"text", "text":"Looking at the cart module first."}]),
        ),
        row(
            "cccccccc-0000-4000-8000-000000000005",
            "2026-10-07T10:03:00Z",
            "assistant",
            json!([{"type":"text", "text":"All tests pass."}]),
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
    let mut ledger = Ledger::open(tmp.path().join("ledger.db")).unwrap();
    ingest(&mut ledger, &[source], &tmp.path().join("mirror")).unwrap();
    let project = hq::hq_projects(&ledger).unwrap().remove(0);
    let index = hq::index(&ledger, &project).unwrap();
    let now = hq::parse_ms("2026-10-08T00:00:00Z").unwrap();
    let thread = index
        .view(now)
        .threads
        .into_iter()
        .find(|t| t.edits > 0)
        .unwrap();
    let view = index
        .thread_view(&ledger, &thread.id, now)
        .unwrap()
        .unwrap();
    let texts: Vec<&str> = view.messages.iter().map(|m| m.text.as_str()).collect();
    assert!(
        texts.contains(&"Looking at the cart module first."),
        "{texts:?}"
    );
    assert!(texts.contains(&"All tests pass."), "{texts:?}");
    assert!(view.claims.iter().any(|c| c.text == "All tests pass."));
    assert!(!view
        .claims
        .iter()
        .any(|c| c.text == "Looking at the cart module first."));
}
