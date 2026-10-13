//! A80 — the attention surface's performance register.
//!
//! NORTHSTAR-v2's reliability §9: performance is correctness. "What needs me
//! now?" must never require loading full history, so the budget is the gate, not
//! a note. Same shape as the M5.R2 register (`m5_capture_cli.rs`): 20 warm
//! iterations of the real binary, p50/p95/worst, printed as JSON so a closure
//! record can quote a measurement rather than an impression.
//!
//! `#[ignore]`d for the same reason M5's are: it is a release gate, not part of
//! every `cargo test`.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

use serde_json::Value;
use tempfile::tempdir;

#[path = "support/qualified_binary.rs"]
mod qualified_binary;

/// Budgets, from the M5.R2 register extended by A80.
const NOW_P95_MS: f64 = 100.0;
const WHY_P95_MS: f64 = 150.0;

fn summary_ms(mut samples: Vec<f64>) -> Value {
    samples.sort_by(f64::total_cmp);
    let at = |fraction: f64| {
        let index = ((samples.len() as f64 * fraction).ceil() as usize)
            .saturating_sub(1)
            .min(samples.len().saturating_sub(1));
        samples[index]
    };
    serde_json::json!({
        "iterations": samples.len(),
        "p50_ms": at(0.5),
        "p95_ms": at(0.95),
        "worst_ms": samples.last().copied().unwrap_or_default(),
    })
}

const PROJ: &str = "-Users-dev-perf";
const SESSION: &str = "dddddddd-4444-4555-8666-777777777777";

fn write_transcript(dir: &Path, turns: usize) {
    let project = dir.join(PROJ);
    fs::create_dir_all(&project).unwrap();
    let u = |n: usize| format!("00000000-0000-4000-8000-{n:012}");
    let mut lines = Vec::new();
    for t in 0..turns {
        let (a, b) = (u(t * 2 + 1), u(t * 2 + 2));
        lines.push(format!(
            r#"{{"type":"user","uuid":"{a}","parentUuid":null,"isSidechain":false,"sessionId":"{SESSION}","timestamp":"2026-08-01T00:00:00.000Z","cwd":"/repo","message":{{"role":"user","content":"turn {t}"}}}}"#
        ));
        lines.push(format!(
            r#"{{"type":"assistant","uuid":"{b}","parentUuid":"{a}","isSidechain":false,"sessionId":"{SESSION}","timestamp":"2026-08-01T00:00:01.000Z","cwd":"/repo","message":{{"role":"assistant","content":[{{"type":"text","text":"all {t} tests pass."}}]}}}}"#
        ));
    }
    fs::write(
        project.join(format!("{SESSION}.jsonl")),
        lines.join("\n") + "\n",
    )
    .unwrap();
}

#[test]
#[ignore = "A80 release attention-surface performance gate"]
fn now_and_why_meet_release_budgets() {
    let temp = tempdir().unwrap();
    let binary = qualified_binary::thruline();
    let db = temp.path().join("ledger.db");
    let store = temp.path().join("store");
    write_transcript(&store, 2_000);

    let ingest = Command::new(binary)
        .args(["--db", &db.display().to_string(), "ingest"])
        .arg(&store)
        .arg("--apply")
        .arg("--mirror-root")
        .arg(temp.path().join("mirror"))
        .output()
        .unwrap();
    assert!(
        ingest.status.success(),
        "{}",
        String::from_utf8_lossy(&ingest.stderr)
    );

    // A contradiction so the timed path derives a real item, not just the calm
    // one. `nodes` is a derived projection, so writing one is what "derived"
    // means; the alternative is loading the scorer into a timing test.
    let anchor = Command::new("sqlite3")
        .arg(&db)
        .arg("SELECT id FROM events WHERE kind='assistant_text' ORDER BY seq DESC LIMIT 1;")
        .output()
        .unwrap();
    let anchor = String::from_utf8_lossy(&anchor.stdout).trim().to_string();
    assert!(!anchor.is_empty(), "no event to anchor to");
    let insert = Command::new("sqlite3")
        .arg(&db)
        .arg(format!(
            "INSERT OR REPLACE INTO nodes VALUES('n1','{PROJ}','all 1999 tests pass.',\
             'contradicted','completed','contradicted','current',0.9,'{SESSION}','{anchor}',1);"
        ))
        .output()
        .unwrap();
    assert!(insert.status.success());

    let run = |args: &[&str]| {
        let started = Instant::now();
        let out = Command::new(binary)
            .args(["--db", &db.display().to_string()])
            .args(args)
            .output()
            .unwrap();
        let ms = started.elapsed().as_secs_f64() * 1_000.0;
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        (ms, String::from_utf8_lossy(&out.stdout).to_string())
    };

    // Warm the page cache the way a real second invocation is warm.
    let (_, first) = run(&["now", "--json"]);
    let first: Value = serde_json::from_str(&first).unwrap();
    assert_eq!(first["shape"], "thruline.now");
    assert_eq!(first["schema_version"], 1);
    let item = first["items"][0]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["kind"] == "contradiction")
        .expect("the timed path must include the planted contradiction, not just a calm reading");
    let id = item["id"].as_str().unwrap().to_string();

    let mut now_samples = Vec::new();
    let mut all_samples = Vec::new();
    let mut why_samples = Vec::new();
    for _ in 0..20 {
        now_samples.push(run(&["now"]).0);
        all_samples.push(run(&["now", "--all"]).0);
        why_samples.push(run(&["why", &id]).0);
    }

    let now = summary_ms(now_samples);
    let all = summary_ms(all_samples);
    let why = summary_ms(why_samples);
    println!(
        "{}",
        serde_json::json!({
            "events": 4_000,
            "now": now, "now_all": all, "why": why,
            "budgets": {"now_p95_ms": NOW_P95_MS, "why_p95_ms": WHY_P95_MS},
            "watchers": 0, "pollers": 0, "daemons": 0, "network": 0,
        })
    );
    for (label, measured) in [("now", &now), ("now --all", &all)] {
        let p95 = measured["p95_ms"].as_f64().unwrap();
        assert!(
            p95 <= NOW_P95_MS,
            "{label} p95 {p95:.3} ms exceeds its budget"
        );
    }
    let p95 = why["p95_ms"].as_f64().unwrap();
    assert!(p95 <= WHY_P95_MS, "why p95 {p95:.3} ms exceeds its budget");
}
