//! Per-shape entailment sanity for every premise shape (M3.8b).
//!
//! A shape may only be tagged `parsed` if the scorer can actually *use* it:
//! a true claim about it must score high and a false one low. Without this,
//! raising parsed coverage would be a way of defeating the fidelity gate
//! rather than earning past it — the tag would stop meaning "we understood
//! this" and start meaning "we rendered this".
//!
//! ```text
//! cargo test --release --test shape_sanity -- --ignored --nocapture
//! ```

//! Requires the `ort` feature: this file exercises the local ONNX
//! entailment backend, which `--no-default-features` removes. Compiled out
//! rather than skipped, so a feature-off build has nothing to link.
#![cfg(feature = "ort")]

use thruline_core::derive::EvidenceRow;
use thruline_core::nli::{EntailmentScorer, OrtNli};
use thruline_core::premise::{render_tagged, Fidelity};

fn row(tool: &str) -> EvidenceRow {
    EvidenceRow {
        kind: if matches!(tool, "Edit" | "Write") {
            "edit"
        } else {
            "command"
        },
        path: None,
        summary: String::new(),
        tool_name: tool.to_string(),
        command: None,
        result_tail: None,
        new_content: None,
        result_excerpt: None,
        tool_use_id: None,
        is_error: false,
        tool_input: None,
        raw_file: None,
        raw_line: None,
        session_id: None,
        seq: None,
        cwd: None,
        git_branch: None,
        command_identity: None,
        raw_sha256: None,
    }
}

fn scorer() -> Option<OrtNli> {
    let dir = OrtNli::default_dir().ok()?;
    dir.join("model_quantized.onnx")
        .exists()
        .then(|| OrtNli::open(&dir).expect("load"))
}

struct Case {
    name: &'static str,
    row: EvidenceRow,
    /// A claim the premise genuinely supports.
    truthful: &'static str,
    /// A claim it genuinely does not.
    false_claim: &'static str,
}

#[test]
#[ignore]
fn every_parsed_shape_can_actually_be_reasoned_over() {
    let Some(mut nli) = scorer() else {
        eprintln!("SKIP: no model");
        return;
    };

    let mut cases: Vec<Case> = Vec::new();

    // 1. Bash, recognised test counts (the M3.2 baseline shape).
    let mut r = row("Bash");
    r.command = Some("cargo test".into());
    r.result_tail = Some("test result: ok. 214 passed; 0 failed".into());
    cases.push(Case {
        name: "bash/tests-ok",
        row: r,
        truthful: "All 214 tests pass.",
        false_claim: "The test suite failed.",
    });

    // 2. Bash, unrecognised output but a clean exit — the M3.8b shape that
    //    converted 60% of all unparsed evidence.
    let mut r = row("Bash");
    r.command = Some("./scripts/smoke q3".into());
    r.result_tail = Some("svc-a ~ svc-b ~ svc-c ~ | 3 warm | tok=9f2a".into());
    cases.push(Case {
        name: "bash/exit-ok+tail",
        row: r,
        truthful: "The smoke script ran successfully.",
        false_claim: "The smoke script failed to run.",
    });

    // 3. Bash, clean exit, no output at all.
    let mut r = row("Bash");
    r.command = Some("mkdir -p build/out".into());
    cases.push(Case {
        name: "bash/exit-ok+silent",
        row: r,
        truthful: "The mkdir command ran successfully.",
        false_claim: "The mkdir command failed.",
    });

    // 4. Bash, the harness reported an error.
    let mut r = row("Bash");
    r.command = Some("./deploy.sh".into());
    r.is_error = true;
    cases.push(Case {
        name: "bash/errored",
        row: r,
        truthful: "The deploy script failed.",
        false_claim: "The deploy script completed successfully.",
    });

    // 5. Edit with a path.
    let mut r = row("Edit");
    r.path = Some("/repo/src/webhook/client.rs".into());
    r.new_content =
        Some("async fn post_with_retry(&self) { for attempt in 0..3 { backoff() } }".into());
    cases.push(Case {
        name: "edit/with-path",
        row: r,
        truthful: "The file /repo/src/webhook/client.rs was edited.",
        false_claim: "No files were changed.",
    });

    // 6. Write whose path was recovered from the tool result.
    let mut r = row("Write");
    r.path = Some("/repo/docs/plan.md".into());
    r.new_content = Some("# Plan Step one Step two".into());
    cases.push(Case {
        name: "write/recovered-path",
        row: r,
        truthful: "The file /repo/docs/plan.md was created.",
        false_claim: "The plan document was deleted.",
    });

    // 7-9. Structured tools: meaning lives in the input.
    let mut r = row("TaskCreate");
    r.tool_input = Some(r#"{"subject":"Add retry logic to the webhook client"}"#.into());
    cases.push(Case {
        name: "taskcreate",
        row: r,
        truthful: "A task was created for adding retry logic.",
        false_claim: "No task was created.",
    });

    let mut r = row("TaskUpdate");
    r.tool_input = Some(r#"{"taskId":"4","status":"completed"}"#.into());
    cases.push(Case {
        name: "taskupdate",
        row: r,
        truthful: "Task 4 was marked completed.",
        false_claim: "Task 4 was deleted.",
    });

    let mut r = row("Workflow");
    r.tool_input = Some(r#"{"description":"Review changed files across dimensions"}"#.into());
    cases.push(Case {
        name: "workflow",
        row: r,
        truthful: "A workflow reviewing changed files was run.",
        false_claim: "No workflow was run.",
    });

    // 10. Generic tool, clean exit.
    let mut r = row("Read");
    r.path = Some("/repo/src/lib.rs".into());
    r.result_tail = Some("pub mod chain; pub mod config;".into());
    cases.push(Case {
        name: "generic-tool/ok",
        row: r,
        truthful: "The file /repo/src/lib.rs was read.",
        false_claim: "The read failed with an error.",
    });

    eprintln!(
        "\n{:<24} {:>6} {:>7}  {:<8}  verdict",
        "shape", "TRUE", "FALSE", "fidelity"
    );
    let mut failures: Vec<String> = Vec::new();
    for c in &cases {
        let r = render_tagged(&c.row);
        let t = nli.score(&r.text, c.truthful).expect("score").entailment;
        let f = nli.score(&r.text, c.false_claim).expect("score").entailment;
        // The bar the brief sets for a new shape.
        let ok = t >= 0.9 && f <= 0.1;
        eprintln!(
            "{:<24} {t:>6.3} {f:>7.3}  {:<8}  {}",
            c.name,
            r.fidelity.as_str(),
            if ok { "ok" } else { "OUT OF BAND" }
        );
        eprintln!("      premise: {}", r.text);
        if !ok && r.fidelity == Fidelity::Parsed {
            failures.push(format!(
                "{}: true {t:.3} (want >=0.9), false {f:.3} (want <=0.1)",
                c.name
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "shapes tagged `parsed` that the scorer cannot reason over — raising \
         coverage this way would defeat the fidelity gate rather than earn past \
         it:\n  {}",
        failures.join("\n  ")
    );
}
