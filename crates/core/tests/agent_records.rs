//! M3.15.2: task-completion notifications are first-class support evidence.

use std::fs;
use std::path::PathBuf;

use thruline_core::annotate::annotate;
use thruline_core::config::Config;
use thruline_core::derive::rebuild;
use thruline_core::ingest::ingest;
use thruline_core::ledger::Ledger;
use thruline_core::nli::{EntailmentScorer, Scores};
use thruline_core::Result;

const PROJ: &str = "-Users-dev-agent-records";
const S: &str = "dddddddd-2222-4333-8444-555555555555";

struct RecordAware;

impl EntailmentScorer for RecordAware {
    fn score(&mut self, premise: &str, _hypothesis: &str) -> Result<Scores> {
        let supports = premise.contains("CacheSchema")
            && premise.contains("strict fields")
            && premise.contains("agent/task record");
        Ok(if supports {
            Scores {
                entailment: 0.99,
                neutral: 0.01,
                contradiction: 0.0,
            }
        } else {
            Scores {
                entailment: 0.01,
                neutral: 0.99,
                contradiction: 0.0,
            }
        })
    }
}

fn transcript() -> String {
    [
        format!(
            r#"{{"type":"user","uuid":"00000000-0000-4000-8001-000000000001","parentUuid":null,"isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-30T00:00:00.000Z","message":{{"role":"user","content":"<task-notification>\n<task-id>job42</task-id>\n<status>completed</status>\n<summary>Schema audit completed</summary>\n<result>{{\"component\":\"CacheSchema\",\"verdict\":\"built\",\"evidence\":\"strict fields are validated and wired into the compiler\"}}</result>\n</task-notification>"}}}}"#
        ),
        format!(
            r#"{{"type":"assistant","uuid":"00000000-0000-4000-8001-000000000002","parentUuid":null,"isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-30T00:00:01.000Z","message":{{"role":"assistant","content":[{{"type":"text","text":"I've built `CacheSchema` with strict fields."}}]}}}}"#
        ),
    ]
    .join("\n")
        + "\n"
}

#[test]
fn task_notification_is_scanned_rendered_tabled_and_support_eligible() {
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
    let report = annotate(
        &ledger,
        PROJ,
        &Config::default(),
        &mut RecordAware,
        "agent-record-stub-v1",
    )
    .expect("annotate");
    assert!(report.family_agent > 0, "{report:?}");

    let (tool, family): (String, String) = ledger
        .conn()
        .query_row(
            "SELECT e.tool_name, l.ev_family
             FROM evidence_links l
             JOIN claims c ON c.id=l.claim_id
             JOIN events e ON e.id=l.evidence_event_id
             WHERE c.text='I''ve built `CacheSchema` with strict fields.'
             LIMIT 1",
            [],
            |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?
                        .unwrap_or_else(|| "TaskNotification".to_string()),
                    r.get(1)?,
                ))
            },
        )
        .expect("agent evidence link");
    // The source event is a user_msg, so the persisted tool name is null; the
    // renderer assigns the structural TaskNotification family transiently.
    assert!(tool == "TaskNotification" || tool.is_empty());
    assert_eq!(
        family, "none",
        "agent prose must never gain arithmetic authority"
    );

    let graph = rebuild(&ledger, PROJ, &Config::default()).expect("rebuild");
    let verdict = graph
        .nodes
        .iter()
        .flat_map(|n| n.verdicts.iter())
        .find(|v| v.text.contains("CacheSchema"))
        .expect("claim verdict");
    assert_eq!(verdict.state.as_str(), "supported", "{verdict:?}");
}

#[test]
fn later_task_completion_defeats_a_same_session_ghost() {
    let transcript = [
        format!(
            r#"{{"type":"assistant","uuid":"10000000-0000-4000-8001-000000000001","parentUuid":null,"isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-30T01:00:00.000Z","message":{{"role":"assistant","content":[{{"type":"text","text":"I'll complete the `ReconLog` audit through the delegated task."}}]}}}}"#
        ),
        format!(
            r#"{{"type":"user","uuid":"10000000-0000-4000-8001-000000000002","parentUuid":null,"isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-30T01:00:01.000Z","message":{{"role":"user","content":"<task-notification>\n<task-id>recon42</task-id>\n<status>completed</status>\n<summary>ReconLog audit completed</summary>\n<result>{{\"artifact\":\"ReconLog\",\"audit\":\"complete\",\"exceptions\":0}}</result>\n</task-notification>"}}}}"#
        ),
    ]
    .into_iter()
    .chain((2..=12).map(|n| {
        format!(
            r#"{{"type":"user","uuid":"10000000-0000-4000-8001-{n:012}","parentUuid":null,"isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-30T01:00:{n:02}.000Z","message":{{"role":"user","content":"later checkpoint {n}"}}}}"#
        )
    }))
    .collect::<Vec<_>>()
    .join("\n")
        + "\n";

    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("projects");
    let project = root.join(PROJ);
    fs::create_dir_all(&project).expect("mkdir");
    fs::write(project.join(format!("{S}.jsonl")), transcript).expect("write");

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
        &mut RecordAware,
        "agent-record-forward-stub-v1",
    )
    .expect("annotate");
    let graph = rebuild(&ledger, PROJ, &Config::default()).expect("rebuild");
    let verdict = graph
        .nodes
        .iter()
        .flat_map(|n| n.verdicts.iter())
        .find(|v| v.text.contains("ReconLog"))
        .expect("proposal verdict");
    assert_ne!(
        verdict.state.as_str(),
        "dropped",
        "an affirmative task completion is follow-through, never a ghost: {verdict:?}"
    );
}
