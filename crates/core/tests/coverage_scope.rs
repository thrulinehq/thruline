//! M3.15.3: `unsupported` is emitted only after a comprehensive family scan.

use std::fs;
use std::path::PathBuf;

use thruline_core::annotate::annotate;
use thruline_core::config::Config;
use thruline_core::derive::{rebuild, unsupported_funnel};
use thruline_core::ingest::ingest;
use thruline_core::ledger::Ledger;
use thruline_core::nli::{EntailmentScorer, Scores};
use thruline_core::Result;

const PROJ: &str = "-Users-dev-coverage-scope";
const COVERED: &str = "eeeeeeee-1111-4222-8333-444444444444";
const UNCOVERED: &str = "eeeeeeee-2222-4333-8444-555555555555";

struct Neutral;

impl EntailmentScorer for Neutral {
    fn score(&mut self, _premise: &str, _hypothesis: &str) -> Result<Scores> {
        Ok(Scores {
            entailment: 0.01,
            neutral: 0.99,
            contradiction: 0.0,
        })
    }
}

fn covered_transcript() -> String {
    [
        format!(
            r#"{{"type":"assistant","uuid":"10000000-0000-4000-8000-000000000001","parentUuid":null,"isSidechain":false,"sessionId":"{COVERED}","timestamp":"2026-07-30T00:00:00.000Z","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"read-covered","name":"Read","input":{{"file_path":"/repo/unrelated.txt"}}}}]}}}}"#
        ),
        format!(
            r#"{{"type":"assistant","uuid":"10000000-0000-4000-8000-000000000002","parentUuid":null,"isSidechain":false,"sessionId":"{COVERED}","timestamp":"2026-07-30T00:00:01.000Z","message":{{"role":"assistant","content":[{{"type":"text","text":"I've built `CoveredWidget`."}}]}}}}"#
        ),
    ]
    .join("\n")
        + "\n"
}

fn uncovered_transcript() -> String {
    let mut lines = vec![format!(
        r#"{{"type":"assistant","uuid":"20000000-0000-4000-8000-000000000001","parentUuid":null,"isSidechain":false,"sessionId":"{UNCOVERED}","timestamp":"2026-07-30T01:00:00.000Z","message":{{"role":"assistant","content":[{{"type":"text","text":"I've built `UncoveredWidget`."}}]}}}}"#
    )];
    // Forty same-turn Read records are inside k=40, but the candidate cap is
    // thirty. The dominant local family therefore was not read completely.
    for n in 2..=41 {
        lines.push(format!(
            r#"{{"type":"assistant","uuid":"20000000-0000-4000-8000-{n:012}","parentUuid":null,"isSidechain":false,"sessionId":"{UNCOVERED}","timestamp":"2026-07-30T01:00:{n:02}.000Z","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"read-{n}","name":"Read","input":{{"file_path":"/repo/record-{n}.txt"}}}}]}}}}"#
        ));
    }
    lines.join("\n") + "\n"
}

#[test]
fn unsupported_funnel_gates_an_incompletely_read_dominant_family() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("projects");
    let project = root.join(PROJ);
    fs::create_dir_all(&project).expect("mkdir");
    fs::write(
        project.join(format!("{COVERED}.jsonl")),
        covered_transcript(),
    )
    .expect("covered transcript");
    fs::write(
        project.join(format!("{UNCOVERED}.jsonl")),
        uncovered_transcript(),
    )
    .expect("uncovered transcript");

    let mut ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    ingest(
        &mut ledger,
        std::slice::from_ref(&PathBuf::from(&root)),
        &dir.path().join("mirrors"),
    )
    .expect("ingest");
    let report = annotate(
        &ledger,
        PROJ,
        &Config::default(),
        &mut Neutral,
        "coverage-stub-v1",
    )
    .expect("annotate");

    assert_eq!(report.completed_claims, 2);
    assert_eq!(report.unsupported_coverage_eligible, 1, "{report:?}");
    assert_eq!(
        report.coverage_missing_by_family.get("read/search records"),
        Some(&1),
        "{report:?}"
    );

    let funnel = unsupported_funnel(&ledger, PROJ).expect("funnel");
    assert_eq!(
        (funnel.claims, funnel.coverage_eligible, funnel.emitted),
        (2, 1, 1)
    );

    let graph = rebuild(&ledger, PROJ, &Config::default()).expect("rebuild");
    let verdict = |needle: &str| {
        graph
            .nodes
            .iter()
            .flat_map(|n| n.verdicts.iter())
            .find(|v| v.text.contains(needle))
            .unwrap_or_else(|| panic!("missing {needle} verdict"))
    };
    assert_eq!(verdict("CoveredWidget").state.as_str(), "unsupported");
    let uncovered = verdict("UncoveredWidget");
    assert_eq!(uncovered.state.as_str(), "unknown");
    assert_eq!(
        uncovered.reason,
        "Proof may live in records not yet read (read/search records)."
    );
}
