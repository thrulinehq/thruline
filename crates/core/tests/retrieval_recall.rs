//! M3.11.3: proof in correlated results and natural-language targets is found.

use std::fs;
use std::path::PathBuf;

use thruline_core::annotate::annotate;
use thruline_core::config::Config;
use thruline_core::derive::rebuild;
use thruline_core::ingest::ingest;
use thruline_core::ledger::Ledger;
use thruline_core::nli::{EntailmentScorer, Scores};
use thruline_core::Result;

const PROJ: &str = "-Users-dev-retrieval";
const S: &str = "dddddddd-1111-4222-8333-444444444444";

struct Supports;

impl EntailmentScorer for Supports {
    fn score(&mut self, _premise: &str, _hypothesis: &str) -> Result<Scores> {
        Ok(Scores {
            entailment: 0.99,
            neutral: 0.01,
            contradiction: 0.0,
        })
    }
}

fn u(n: u32) -> String {
    format!("00000000-0000-4000-8001-{n:012}")
}

fn transcript() -> String {
    let mut lines = vec![
        format!(
            r#"{{"type":"assistant","uuid":"{}","parentUuid":null,"isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-30T00:00:00.000Z","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"review","name":"Task","input":{{"description":"collect audit"}}}}]}}}}"#,
            u(1)
        ),
        format!(
            r#"{{"type":"user","uuid":"{}","parentUuid":"{}","isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-30T00:00:01.000Z","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"review","content":"Independent reviewer ran 16 placements and recorded the table."}}]}}}}"#,
            u(2),
            u(1)
        ),
    ];
    // Put the proof outside the ±40 local window. Only result-aware lexical
    // retrieval can recover it.
    for n in 3..47 {
        lines.push(format!(
            r#"{{"type":"user","uuid":"{}","parentUuid":null,"isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-30T00:00:{n:02}.000Z","message":{{"role":"user","content":"filler turn {n}"}}}}"#,
            u(n)
        ));
    }
    lines.push(format!(
        r#"{{"type":"assistant","uuid":"{}","parentUuid":null,"isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-30T00:00:47.000Z","message":{{"role":"assistant","content":[{{"type":"text","text":"The reviewer built `PlacementGate` and ran **16 placements** trying to get a provider past the gate."}}]}}}}"#,
        u(47)
    ));
    lines.push(format!(
        r#"{{"type":"assistant","uuid":"{}","parentUuid":null,"isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-30T00:00:48.000Z","message":{{"role":"assistant","content":[{{"type":"text","text":"Now the gallery needs updating — the composition changes."}}]}}}}"#,
        u(48)
    ));
    lines.push(format!(
        r#"{{"type":"assistant","uuid":"{}","parentUuid":null,"isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-30T00:00:49.000Z","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"gallery","name":"Edit","input":{{"file_path":"/repo/Gallery/DemoStatesPage.swift","old_string":"old","new_string":"new"}}}}]}}}}"#,
        u(49)
    ));
    lines.push(format!(
        r#"{{"type":"user","uuid":"{}","parentUuid":null,"isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-30T00:00:50.000Z","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"gallery","content":"DemoStatesPage.swift updated"}}]}}}}"#,
        u(50)
    ));
    for n in 51..62 {
        lines.push(format!(
            r#"{{"type":"user","uuid":"{}","parentUuid":null,"isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-30T00:00:{n}.000Z","message":{{"role":"user","content":"later turn {n}"}}}}"#,
            u(n)
        ));
    }
    lines.join("\n") + "\n"
}

#[test]
fn result_only_proof_and_natural_target_names_are_retrieved() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("projects");
    let project = root.join(PROJ);
    fs::create_dir_all(&project).expect("mkdir");
    fs::write(project.join(format!("{S}.jsonl")), transcript()).expect("write");
    let mut ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    ingest(
        &mut ledger,
        std::slice::from_ref(&PathBuf::from(&root)),
        &dir.path().join("mirrors"),
    )
    .expect("ingest");

    annotate(
        &ledger,
        PROJ,
        &Config::default(),
        &mut Supports,
        "retrieval-stub-v1",
    )
    .expect("annotate");

    let result_links: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM evidence_links l
             JOIN claims c ON c.id=l.claim_id
             WHERE c.text='The reviewer built `PlacementGate` and ran **16 placements** trying to get a provider past the gate.'
               AND l.pass='backward'",
            [],
            |r| r.get(0),
        )
        .expect("result link count");
    let claims: Vec<(String, String)> = {
        let mut stmt = ledger
            .conn()
            .prepare("SELECT text, kind FROM claims ORDER BY text")
            .expect("claim query");
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .expect("claims")
            .collect::<std::result::Result<_, _>>()
            .expect("claim collect")
    };
    assert!(
        result_links > 0,
        "proof outside the local window was not recovered from tool_result: {claims:?}"
    );

    let (searched, names): (i64, i64) = ledger
        .conn()
        .query_row(
            "SELECT cs.searched, cs.name_matches FROM claim_searches cs
             JOIN claims c ON c.id=cs.claim_id
             WHERE c.text='Now the gallery needs updating — the composition changes.'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("gallery search");
    assert_eq!(searched, 1);
    assert!(
        names > 0,
        "the natural target `gallery` did not match the later Gallery path"
    );

    let graph = rebuild(&ledger, PROJ, &Config::default()).expect("rebuild");
    let gallery = graph
        .nodes
        .iter()
        .flat_map(|n| n.verdicts.iter())
        .find(|v| v.text == "Now the gallery needs updating — the composition changes.")
        .expect("gallery proposal verdict");
    assert_ne!(
        gallery.state.as_str(),
        "dropped",
        "a same-session edit is affirmative follow-through, never a ghost: {gallery:?}"
    );
}
