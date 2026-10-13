//! M3.15.2 mined dev grammars for agent records and compound receipts.

use serde::Deserialize;
use thruline_core::annotate::relatedness;
use thruline_core::derive::EvidenceRow;
use thruline_core::premise::{render_tagged_for_claim, Fidelity};

#[derive(Deserialize)]
struct Fixture {
    note: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    family: String,
    claim: String,
    tool: String,
    path: Option<String>,
    tail: String,
    detail: String,
    must_contain: String,
}

fn row(c: &Case) -> EvidenceRow {
    EvidenceRow {
        kind: "command",
        path: c.path.clone(),
        summary: c.family.clone(),
        tool_name: c.tool.clone(),
        command: (c.tool == "Bash").then(|| "run evidence check".to_string()),
        result_tail: Some(c.tail.clone()),
        new_content: None,
        result_excerpt: None,
        tool_use_id: Some(c.id.clone()),
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

#[test]
fn all_nine_round6_false_unsupported_grammars_recover_claim_bearing_proof() {
    let f: Fixture = serde_json::from_str(include_str!("fixtures/m315-evidence-record-dev.json"))
        .expect("dev grammar fixture");
    assert!(f.note.contains("mined from Round-6"));
    assert_eq!(f.cases.len(), 9);

    for c in &f.cases {
        let rendered = render_tagged_for_claim(&row(c), &c.claim, Some(&c.detail));
        assert_eq!(
            rendered.fidelity,
            Fidelity::Parsed,
            "{} ({}) did not become parsed evidence: {}",
            c.id,
            c.family,
            rendered.text
        );
        assert!(
            rendered.text.contains(&c.must_contain),
            "{} ({}) lost the claim-bearing receipt fragment: {}",
            c.id,
            c.family,
            rendered.text
        );
        assert!(
            relatedness(&c.claim, &rendered.text) > 0.0,
            "{} ({}) recovered text is still unretrievable: {}",
            c.id,
            c.family,
            rendered.text
        );
    }
}
