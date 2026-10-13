//! Six-step proof chain is read only and names absent links.

#[path = "support/person_action.rs"]
mod person_action;

use person_action::PersonRecorder;

use serde_json::json;
use thruline_core::decisions::params::{CheckParams, Surface, TextAbsentParams};
use thruline_core::decisions::storage::{
    self, AcceptanceInput, DecisionSource, NewDecision, SourceKind, VerdictInput,
};
use thruline_core::decisions::text_exclusions;
use thruline_core::ledger::Ledger;

// Compile the new module before shared registration.
mod decisions {
    pub use thruline_core::decisions::{authority, params, storage};
}
mod error {
    pub use thruline_core::error::*;
}
mod ledger {
    pub use thruline_core::ledger::*;
}
mod redact {
    pub use thruline_core::redact::*;
}
#[path = "../src/decisions/why.rs"]
mod why;

use why::{proof_chain, ChainLabel, LinkState};

fn accepted() -> (tempfile::TempDir, Ledger, String, PersonRecorder) {
    let store = tempfile::tempdir().expect("ledger directory");
    let ledger = Ledger::open(store.path().join("ledger.db")).expect("ledger");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id,root_path,source,created_at) VALUES('project-a','/tmp/project-a','codex','2026-10-07T00:00:00Z')",
            [],
        )
        .expect("project");
    let span = "No obsolete label anywhere";
    let id = storage::create(
        &ledger,
        NewDecision {
            project_id: "project-a".into(),
            source_anchor: "source-1".into(),
            span: span.into(),
            sources: vec![DecisionSource {
                source_kind: SourceKind::RuleFile,
                event_id: None,
                file_anchor: Some("source-1".into()),
                raw_file: "notes/decision.md".into(),
                raw_line: 12,
                span_start: 0,
                span_end: span.len() as u64,
                span: span.into(),
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
                terms: vec!["obsolete".into()],
                synonyms: vec![],
                surfaces: vec![Surface::Code, Surface::Docs],
                allowlist: vec!["bench/**".into()],
            }),
        },
    )
    .expect("proposal");
    let acceptance_id = storage::accept(
        &ledger,
        AcceptanceInput {
            decision_id: id.clone(),
            revision: 1,
            actor: "founder".into(),
            at: "2026-10-07T01:00:00Z".into(),
            because: "Keep the name out of product surfaces".into(),
            severity: "blocks".into(),
            applies_from_version: "v1".into(),
        },
    )
    .expect("acceptance");
    let mut person = PersonRecorder::new(&ledger, "project-a");
    person.bind_bridge_acceptance(&ledger, &acceptance_id);
    (store, ledger, id, person)
}

fn broken_verdict(ledger: &Ledger, decision_id: &str) -> String {
    storage::record_verdict(
        ledger,
        VerdictInput {
            decision_id: decision_id.into(),
            revision: 1,
            version_id: "v2".into(),
            tree_hash: "tree-v2".into(),
            verdict: "broken".into(),
            reason: None,
            checkable_if: None,
            findings: json!([{
                "id": "finding-1",
                "surface": "code",
                "path": "src/lib.rs",
                "line": 7,
                "span": "obsolete",
                "edit_id": "edit-7",
                "session_id": "session-7",
                "introduced_at": "2026-10-07T01:15:00Z"
            }]),
            surfaces: json!([{"surface":"code","state":"broken","finding_count":1}]),
            check_impl_version: "a1.v1".into(),
            tool_versions: json!({"git":"git 2.46"}),
            computed_at: "2026-10-07T02:00:00Z".into(),
            basis: json!({"tree_hash":"tree-v2"}),
        },
    )
    .expect("verdict")
}

#[test]
fn accepted_broken_verdict_has_six_ordered_typed_links_and_recorded_attribution() {
    let (_store, ledger, decision_id, _person) = accepted();
    let verdict_id = broken_verdict(&ledger, &decision_id);
    let proof = proof_chain(&ledger, &decision_id, "v2", "tree-v2")
        .expect("read proof")
        .expect("accepted decision");
    assert_eq!(proof.verdict_id.as_deref(), Some(verdict_id.as_str()));
    assert_eq!(proof.chain.len(), 6);
    assert_eq!(
        proof
            .chain
            .iter()
            .map(|step| step.label)
            .collect::<Vec<_>>(),
        vec![
            ChainLabel::Decided,
            ChainLabel::Check,
            ChainLabel::RanOn,
            ChainLabel::Found,
            ChainLabel::Since,
            ChainLabel::So,
        ]
    );
    for (index, step) in proof.chain.iter().enumerate() {
        assert_eq!(step.number, (index + 1) as u8);
    }
    assert_eq!(proof.chain[0].state, LinkState::Holds);
    assert_eq!(proof.chain[0].source_ids, ["source-1"]);
    assert!(proof.chain[0].sentence.contains("founder"));
    assert!(proof.chain[0].sentence.contains("notes/decision.md:12"));
    assert!(proof.chain[1].sentence.contains("text_absent"));
    assert!(proof.chain[1].sentence.contains("bench/**"));
    assert!(proof.chain[2].sentence.contains("tree-v2"));
    assert!(proof.chain[2].sentence.contains("git 2.46"));
    assert_eq!(proof.chain[3].finding_ids, ["finding-1"]);
    assert!(proof.chain[3].sentence.contains("src/lib.rs:7"));
    assert!(proof.chain[3].sentence.contains("edit-7"));
    assert!(proof.chain[3].sentence.contains("session-7"));
    assert_eq!(proof.chain[4].state, LinkState::Holds);
    assert!(proof.chain[5].sentence.contains("broken"));
    assert!(proof.chain[5].sentence.contains("1 place"));
}

#[test]
fn absent_verdict_leaves_run_found_since_and_conclusion_missing() {
    let (_store, ledger, decision_id, _person) = accepted();
    let proof = proof_chain(&ledger, &decision_id, "v2", "tree-v2")
        .expect("read proof")
        .expect("accepted decision");
    assert_eq!(proof.verdict_id, None);
    assert_eq!(
        proof
            .chain
            .iter()
            .map(|step| step.state)
            .collect::<Vec<_>>(),
        vec![
            LinkState::Holds,
            LinkState::Holds,
            LinkState::Missing,
            LinkState::Missing,
            LinkState::Missing,
            LinkState::Missing,
        ]
    );
    assert!(proof.chain[5].sentence.contains("No verdict"));
}

#[test]
fn changed_exact_tree_marks_stored_result_moved_without_rewriting_its_basis() {
    let (_store, ledger, decision_id, _person) = accepted();
    let verdict_id = broken_verdict(&ledger, &decision_id);
    let proof = proof_chain(&ledger, &decision_id, "v2", "tree-v3")
        .expect("read proof")
        .expect("accepted decision");
    assert_eq!(proof.verdict_id.as_deref(), Some(verdict_id.as_str()));
    assert_eq!(proof.chain[2].state, LinkState::Moved);
    assert_eq!(proof.chain[4].state, LinkState::Moved);
    assert_eq!(proof.chain[5].state, LinkState::Moved);
    assert!(proof.chain[5].sentence.contains("stale"));
    let stored: String = ledger
        .conn()
        .query_row(
            "SELECT verdict FROM decision_verdicts WHERE id=?1",
            [&verdict_id],
            |row| row.get(0),
        )
        .expect("stored verdict");
    assert_eq!(stored, "broken");
}

#[test]
fn proposal_without_person_acceptance_has_no_proof_chain() {
    let (_store, ledger, decision_id, _person) = accepted();
    ledger
        .conn()
        .execute(
            "INSERT INTO decisions(id,project_id,created_at) VALUES('proposal-only','project-a','2026-10-07T03:00:00Z')",
            [],
        )
        .expect("proposal row");
    assert!(proof_chain(&ledger, "proposal-only", "v2", "tree-v2")
        .expect("read proof")
        .is_none());
    assert!(proof_chain(&ledger, &decision_id, "v2", "tree-v2")
        .expect("read accepted proof")
        .is_some());
}

#[test]
fn unverifiable_result_names_reason_and_checkable_condition() {
    let (_store, ledger, decision_id, _person) = accepted();
    storage::record_verdict(
        &ledger,
        VerdictInput {
            decision_id: decision_id.clone(),
            revision: 1,
            version_id: "v2".into(),
            tree_hash: "tree-v2".into(),
            verdict: "unverifiable".into(),
            reason: Some("renderer unavailable".into()),
            checkable_if: Some("install the pinned renderer".into()),
            findings: json!([]),
            surfaces: json!([]),
            check_impl_version: "c1.v1".into(),
            tool_versions: json!({}),
            computed_at: "2026-10-07T02:00:00Z".into(),
            basis: json!({"tree_hash":"tree-v2"}),
        },
    )
    .expect("verdict");
    let proof = proof_chain(&ledger, &decision_id, "v2", "tree-v2")
        .expect("read proof")
        .expect("accepted decision");
    assert!(proof.chain[5].sentence.contains("renderer unavailable"));
    assert!(proof.chain[5]
        .sentence
        .contains("install the pinned renderer"));
    assert_eq!(proof.chain[3].state, LinkState::Missing);
}
