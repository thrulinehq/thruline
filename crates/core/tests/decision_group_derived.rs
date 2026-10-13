#[path = "support/person_action.rs"]
mod person_action;

use person_action::PersonRecorder;
use thruline_core::decisions::params::{CheckParams, Surface, TextAbsentParams};
use thruline_core::decisions::storage::{
    self, AcceptanceInput, DecisionSource, NewDecision, SourceKind,
};
use thruline_core::ledger::Ledger;

#[test]
fn one_hq_root_file_contains_accepted_decisions_from_every_member() {
    let store = tempfile::tempdir().expect("ledger directory");
    let ledger = Ledger::open(store.path().join("ledger.db")).expect("ledger");
    for (id, root) in [("member-a", "/tmp/member-a"), ("member-b", "/tmp/member-b")] {
        ledger
            .conn()
            .execute(
                "INSERT INTO projects(id,root_path,source,created_at) VALUES(?1,?2,'codex','2026-10-07T00:00:00Z')",
                rusqlite::params![id, root],
            )
            .expect("project");
    }
    let mut people = Vec::new();
    for (project_id, anchor, term) in [
        ("member-a", "source-a", "obsolete"),
        ("member-b", "source-b", "legacy"),
    ] {
        let span = format!("No {term} label anywhere");
        let decision_id = storage::create(
            &ledger,
            NewDecision {
                project_id: project_id.into(),
                source_anchor: anchor.into(),
                span: span.clone(),
                sources: vec![DecisionSource {
                    source_kind: SourceKind::WorkPrompt,
                    event_id: Some(anchor.into()),
                    file_anchor: None,
                    raw_file: "record.jsonl".into(),
                    raw_line: 1,
                    span_start: 0,
                    span_end: span.len() as u64,
                    span,
                    actor: Some("founder".into()),
                    recorded_at: Some("2026-10-07T00:00:00Z".into()),
                }],
                author_actor: "founder".into(),
                recorded_at: "2026-10-07T00:00:00Z".into(),
                params: CheckParams::TextAbsent(TextAbsentParams {
                    test_allowlist: Vec::new(),
                    json_field_allowlist: Vec::new(),
                    conditional_path_allowlist: Vec::new(),
                    span_allowlist: Vec::new(),
                    terms: vec![term.into()],
                    synonyms: vec![],
                    surfaces: vec![Surface::Code],
                    allowlist: vec![],
                }),
            },
        )
        .expect("proposal");
        let acceptance_id = storage::accept(
            &ledger,
            AcceptanceInput {
                decision_id,
                revision: 1,
                actor: "founder".into(),
                at: "2026-10-07T00:01:00Z".into(),
                because: "Keep the label away".into(),
                severity: "blocks".into(),
                applies_from_version: "v1".into(),
            },
        )
        .expect("acceptance");
        let mut person = PersonRecorder::new(&ledger, project_id);
        person.bind_bridge_acceptance(&ledger, &acceptance_id);
        people.push(person);
    }
    let root = tempfile::tempdir().expect("root");
    let path = storage::write_derived_file_for_projects(
        &ledger,
        "hq-root",
        &["member-b".into(), "member-a".into(), "member-a".into()],
        root.path(),
    )
    .expect("combined derived copy");
    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).expect("copy")).expect("JSON YAML");
    assert_eq!(document["project_id"], "hq-root");
    assert_eq!(document["decisions"].as_array().map(Vec::len), Some(2));
    assert!(storage::verify_derived_file_for_projects(
        &ledger,
        "hq-root",
        &["member-a".into(), "member-b".into()],
        root.path(),
    )
    .expect("verify"));
}
