//! M3.10.2 — how much of the *matched evidence* can we actually interpret?
//!
//! The denominator is deliberately narrow: premises behind **marker claims**
//! that are already `parsed` and already the matched pair (`match_strength ≥
//! 0.6`). Those are the pairs a verdict can rest on, so those are the pairs
//! whose outcome we must be able to read. Measuring against all premises would
//! flatter the number with file reads and diffs that were never going to
//! convict anything.
//!
//! A36 is the reason this test exists at all: the renderer reported 100%
//! `parsed` while `classify_outcome` interpreted 1.4%, and nothing in the
//! suite measured the gap between those two properties.
//!
//! `#[ignore]`d — needs the real ledger.

use std::path::PathBuf;

use thruline_core::ledger::Ledger;
use thruline_core::premise::{classify_outcome, Outcome};

#[path = "support/live_store.rs"]
mod live_store;

fn db() -> PathBuf {
    let db = PathBuf::from(std::env::var("HOME").expect("HOME")).join(".thruline/ledger.db");
    live_store::refuse_live_store(&db);
    db
}

const DEV: &str = "-Users-dev-Documents-ATLAS";

/// §5e's dev partition. The holdout is not touched here.
#[test]
#[ignore]
fn matched_evidence_outcome_coverage_on_dev() {
    let ledger = Ledger::open(db()).expect("ledger");
    let mut stmt = ledger
        .conn()
        .prepare(
            "SELECT DISTINCT e.tool_use_id, e.tool_name
             FROM evidence_links l
             JOIN events e  ON e.id = l.evidence_event_id
             JOIN claims c  ON c.id = l.claim_id
             JOIN events ce ON ce.id = c.event_id
             WHERE ce.project_id = ?1 AND c.kind = 'marker'
               AND l.fidelity = 'parsed' AND l.match_strength >= 0.6",
        )
        .expect("prepare");
    let ids: Vec<(String, String)> = stmt
        .query_map([DEV], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            ))
        })
        .expect("query")
        .filter_map(|r| r.ok())
        .filter_map(|(a, b)| a.map(|a| (a, b)))
        .collect();
    let mut unread: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut samples: Vec<String> = Vec::new();
    // A payload is *outcome-bearing* if it contains any lexeme a run would
    // print. Stated explicitly so the denominator is auditable rather than
    // chosen to flatter: file reads, greps, listings and empty output are not
    // runs, and counting them as coverage misses would measure the corpus, not
    // the parser.
    fn outcome_bearing(t: &str) -> bool {
        let l = t.to_ascii_lowercase();
        [
            "# pass",
            "# fail",
            "# tests",
            "test result",
            "test run with",
            "pass:",
            "fail:",
            "exit code",
            "build succeeded",
            "build failed",
            "error:",
            "✓",
            "✗",
            "✔",
            "✘",
            " passed",
            " failed",
            " passing",
            " failing",
        ]
        .iter()
        .any(|k| l.contains(k))
    }
    let (mut bearing, mut bearing_read) = (0usize, 0usize);

    let (mut total, mut tests, mut build, mut unknown) = (0usize, 0usize, 0usize, 0usize);
    for (tid, tool) in &ids {
        let tail: Option<String> = ledger
            .conn()
            .query_row(
                "SELECT tool_result FROM events
                 WHERE tool_use_id = ?1 AND kind = 'tool_result' ORDER BY seq LIMIT 1",
                [tid],
                |r| r.get(0),
            )
            .ok()
            .flatten();
        let Some(tail) = tail else { continue };
        total += 1;
        let is_bearing = outcome_bearing(&tail) && !tail.contains("completed with no output");
        if is_bearing {
            bearing += 1;
        }
        match classify_outcome(&tail) {
            Outcome::Tests { .. } => {
                tests += 1;
                if is_bearing {
                    bearing_read += 1;
                }
            }
            Outcome::Succeeded | Outcome::Failed => {
                build += 1;
                if is_bearing {
                    bearing_read += 1;
                }
            }
            Outcome::Unknown => {
                unknown += 1;
                *unread.entry(tool.clone()).or_default() += 1;
                if tool == "Bash" && samples.len() < 14 {
                    let t = tail.replace("\\n", " | ");
                    samples.push(t.chars().take(110).collect());
                }
            }
        }
    }

    let interpreted = tests + build;
    let pct = 100.0 * interpreted as f64 / total.max(1) as f64;
    println!("\n  dev partition ({DEV})");
    println!("  matched-evidence payloads   {total}");
    println!("  tests                       {tests}");
    println!("  build                       {build}");
    println!("  uninterpretable             {unknown}");
    println!("  INTERPRETED                 {pct:.1}%\n");

    println!("  uninterpretable by tool:");
    for (k, v) in &unread {
        println!("    {k:<22} {v}");
    }
    println!("\n  sample uninterpretable Bash payloads:");
    for s in &samples {
        println!("    {s}");
    }
    let bpct = 100.0 * bearing_read as f64 / bearing.max(1) as f64;
    println!("\n  outcome-bearing payloads    {bearing}");
    println!("  of those, interpreted       {bearing_read}  ({bpct:.1}%)");

    // The gate is on the outcome-bearing denominator, because that is the one
    // that measures the PARSER. The all-matched-evidence figure is reported
    // beside it and is dominated by payloads that were never runs.
    assert!(
        bpct >= 80.0,
        "outcome coverage on outcome-bearing matched evidence is {bpct:.1}%, gate is 80%"
    );
}
