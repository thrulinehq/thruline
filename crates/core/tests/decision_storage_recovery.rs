//! Accepted acts survive an export failure and can be retried without
//! appending a second person act. File anchors are scoped to their project.

#[path = "support/person_action.rs"]
mod person_action;

use person_action::PersonRecorder;

use std::error::Error;

use thruline_core::decisions::params::{CheckParams, Surface, TextAbsentParams};
use thruline_core::decisions::storage::{
    self, AcceptanceInput, DecisionSource, NewDecision, RetirementInput, SourceKind,
};
use thruline_core::ledger::Ledger;

fn project(ledger: &Ledger, id: &str) -> Result<(), Box<dyn Error>> {
    ledger.conn().execute(
        "INSERT INTO projects(id,root_path,source,created_at) VALUES(?1,?2,'codex','2026-10-07T00:00:00Z')",
        rusqlite::params![id, format!("/tmp/{id}")],
    )?;
    Ok(())
}

fn proposal(project_id: &str, anchor: &str) -> NewDecision {
    let span = "No obsolete label anywhere";
    NewDecision {
        project_id: project_id.into(),
        source_anchor: anchor.into(),
        span: span.into(),
        sources: vec![DecisionSource {
            source_kind: SourceKind::RuleFile,
            event_id: None,
            file_anchor: Some(anchor.into()),
            raw_file: "AGENTS.md".into(),
            raw_line: 2,
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
            surfaces: vec![Surface::Code],
            allowlist: vec![],
        }),
    }
}

fn acceptance(decision_id: &str, at: &str, because: &str) -> AcceptanceInput {
    AcceptanceInput {
        decision_id: decision_id.into(),
        revision: 1,
        actor: "founder".into(),
        at: at.into(),
        because: because.into(),
        severity: "blocks".into(),
        applies_from_version: "v1".into(),
    }
}

fn count(ledger: &Ledger, table: &str) -> Result<i64, Box<dyn Error>> {
    let sql = format!("SELECT count(*) FROM {table}");
    Ok(ledger.conn().query_row(&sql, [], |row| row.get(0))?)
}

#[test]
fn identical_file_anchors_in_two_projects_have_distinct_ids() -> Result<(), Box<dyn Error>> {
    let ledger = Ledger::open_in_memory()?;
    project(&ledger, "project-a")?;
    project(&ledger, "project-b")?;
    let a = storage::create(&ledger, proposal("project-a", "AGENTS.md:2"))?;
    let b = storage::create(&ledger, proposal("project-b", "AGENTS.md:2"))?;
    assert_ne!(a, b);
    assert_eq!(count(&ledger, "decisions")?, 2);
    Ok(())
}

#[test]
fn export_failure_can_be_repaired_by_retrying_the_same_person_acts() -> Result<(), Box<dyn Error>> {
    let ledger_dir = tempfile::tempdir()?;
    let ledger = Ledger::open(ledger_dir.path().join("ledger.db"))?;
    project(&ledger, "project-a")?;
    let decision_id = storage::create(&ledger, proposal("project-a", "AGENTS.md:2"))?;
    let root = tempfile::tempdir()?;
    let blocker = root.path().join(".thruline");
    std::fs::write(&blocker, "not a directory")?;

    let first = storage::accept(
        &ledger,
        acceptance(&decision_id, "2026-10-07T01:00:00Z", "Standing rule"),
    )?;
    let mut person = PersonRecorder::new(&ledger, "project-a");
    person.bind_acceptance(&ledger, &first);
    assert!(storage::write_derived_file(&ledger, "project-a", root.path()).is_err());
    assert_eq!(count(&ledger, "decision_acceptances")?, 1);
    std::fs::remove_file(&blocker)?;
    let retry = storage::accept(
        &ledger,
        acceptance(&decision_id, "2026-10-07T01:01:00Z", "Standing rule"),
    )?;
    assert_eq!(retry, first, "a retry returns the original acceptance id");
    storage::write_derived_file(&ledger, "project-a", root.path())?;
    assert!(storage::verify_derived_file(
        &ledger,
        "project-a",
        root.path()
    )?);
    assert_eq!(count(&ledger, "decision_acceptances")?, 1);
    assert!(storage::accept(
        &ledger,
        acceptance(&decision_id, "2026-10-07T01:02:00Z", "Different rationale"),
    )
    .is_err());

    std::fs::remove_file(root.path().join(".thruline/decisions.yml"))?;
    std::fs::remove_dir(root.path().join(".thruline"))?;
    std::fs::write(&blocker, "not a directory")?;
    let first_retirement = storage::retire(
        &ledger,
        RetirementInput {
            decision_id: decision_id.clone(),
            actor: "founder".into(),
            at: "2026-10-07T02:00:00Z".into(),
            because: "Replaced".into(),
            superseded_by: None,
        },
    )?;
    person.bind_retirement(&ledger, &first_retirement);
    assert!(storage::write_derived_file(&ledger, "project-a", root.path()).is_err());
    std::fs::remove_file(&blocker)?;
    let retry_retirement = storage::retire(
        &ledger,
        RetirementInput {
            decision_id,
            actor: "founder".into(),
            at: "2026-10-07T02:01:00Z".into(),
            because: "Replaced".into(),
            superseded_by: None,
        },
    )?;
    assert_eq!(retry_retirement, first_retirement);
    storage::write_derived_file(&ledger, "project-a", root.path())?;
    assert!(storage::verify_derived_file(
        &ledger,
        "project-a",
        root.path()
    )?);
    assert_eq!(count(&ledger, "decision_retirements")?, 1);
    Ok(())
}
