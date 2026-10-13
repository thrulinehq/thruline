//! `thruline bench` reports what is on disk, and never the retracted finding.
//!
//! # Why the retraction gets its own assertion
//!
//! This command used to print, to the user, that §12's sources "publish task
//! environments, not the false-success-labelled trajectories §12 asks for".
//! That was five harness-only repositories generalised into a claim about the
//! field, and τ2-bench refutes it. A wrong finding in a doc comment is a bad
//! comment; a wrong finding this command *prints* is the product asserting it.
//! So one test below is simply that the sentence is gone and cannot come back
//! without turning red.
//!
//! The corpus file here is **synthetic and tiny**, and it is not a benchmark
//! fixture: it exercises the command's plumbing — the loader runs, the census
//! runs, the counts print — and no §12 number is computed from it. The real
//! corpus is fetched by `fixtures/bench/download.sh` and measured by
//! `crates/bench/tests/tau2_class_one.rs`.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_thruline");

fn tmp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("tl-bench-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

fn run(fixtures: &std::path::Path, json: bool) -> (String, i32) {
    let mut c = Command::new(BIN);
    c.arg("bench").arg("--fixtures").arg(fixtures);
    if json {
        c.arg("--json");
    }
    let out = c.output().expect("run thruline bench");
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (text, out.status.code().unwrap_or(-1))
}

#[test]
fn an_empty_fixtures_directory_refuses_with_three_and_names_the_sources() {
    let dir = tmp("empty");
    let (text, code) = run(&dir, false);
    assert_eq!(code, 3, "{text}");
    assert!(text.contains("No labelled corpus is present"), "{text}");
    // The corrected row is the one a reader needs: τ2-bench is fetchable.
    assert!(text.contains("tau2-bench"), "{text}");
    assert!(text.contains("download.sh"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// **The retraction, as a test.** These are the words the command used to print.
#[test]
fn the_command_no_longer_prints_the_finding_that_was_wrong() {
    let dir = tmp("retracted");
    let (text, _) = run(&dir, false);
    for retracted in [
        "not the false-success-labelled trajectories",
        "No normaliser has seen these formats",
        "there is nothing to measure",
    ] {
        assert!(
            !text.contains(retracted),
            "`thruline bench` still prints a finding that τ2-bench refutes: \
             {retracted:?}\n\n{text}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A corpus directory that exists and holds a readable file: the command loads
/// it and prints counts it computed, rather than asserting anything about the
/// format.
#[test]
fn a_present_corpus_is_loaded_and_its_evidence_counted() {
    let dir = tmp("loaded");
    let tau2 = dir.join("tau2");
    std::fs::create_dir_all(&tau2).expect("mkdir");
    // Two simulations, one scored 1.0 and one scored 0.0, with tool calls that
    // are domain APIs rather than code checks — the shape of the real corpus.
    let doc = serde_json::json!({
        "simulations": [
            {
                "task_id": "t1", "trial": 0,
                "reward_info": { "reward": 1.0 },
                "messages": [
                    { "role": "assistant", "tool_calls": [ { "name": "get_reservation_details" } ] }
                ]
            },
            {
                "task_id": "t2", "trial": 0,
                "reward_info": { "reward": 0.0 },
                "messages": [
                    { "role": "assistant", "tool_calls": [ { "name": "book_reservation" } ] }
                ]
            }
        ]
    });
    std::fs::write(
        tau2.join("synthetic_plumbing_check.json"),
        serde_json::to_vec_pretty(&doc).expect("json"),
    )
    .expect("write");

    let (text, code) = run(&dir, true);
    assert_eq!(code, 3, "{text}");
    let v: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    let corpora = v["corpora"].as_array().expect("corpora");
    assert_eq!(corpora.len(), 1, "{text}");
    assert_eq!(corpora[0]["trajectories"], 2);
    assert_eq!(corpora[0]["labelled"], 2);
    assert_eq!(corpora[0]["tool_calls"], 2);
    assert_eq!(corpora[0]["code_check_executions"], 0);
    // The digest is reported and explicitly NOT pinned, because this command
    // has no fetch record to pin against.
    assert_eq!(corpora[0]["digest_pinned"], false);
    assert_eq!(
        corpora[0]["sha256"]
            .as_str()
            .map(|s| s.len())
            .unwrap_or_default(),
        64,
        "{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
