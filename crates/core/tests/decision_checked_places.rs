//! Checked places count the tracked paths and text lines actually censused.

#[path = "support/person_action.rs"]
mod person_action;

use person_action::PersonRecorder;

use std::error::Error;
use std::path::Path;
use std::process::Command;

use serde_json::Value;
use thruline_core::decisions::checks::{check_text_absent_at, VerdictState};
use thruline_core::decisions::engine;
use thruline_core::decisions::params::{CheckParams, Surface, TextAbsentParams};
use thruline_core::decisions::storage::{
    self, AcceptanceInput, DecisionSource, NewDecision, SourceKind,
};
use thruline_core::ledger::Ledger;

fn git(repo: &Path, args: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = Command::new("git").args(args).current_dir(repo).output()?;
    if !output.status.success() {
        return Err(format!("git {args:?}: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn fixture() -> Result<tempfile::TempDir, Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    for folder in ["src", "docs", "tests", "bench"] {
        std::fs::create_dir(repo.path().join(folder))?;
    }
    std::fs::write(
        repo.path().join("src/main.rs"),
        "fn main() {}\ntrial\nlast\n",
    )?;
    std::fs::write(repo.path().join("src/trial.rs"), "clean\n")?;
    std::fs::write(repo.path().join("src/empty.rs"), "")?;
    std::fs::write(
        repo.path().join("src/data.rs"),
        [0, b't', b'r', b'i', b'a', b'l'],
    )?;
    std::fs::write(repo.path().join("docs/readme.md"), "Trial guide\nplain\n")?;
    std::fs::write(repo.path().join("docs/ignored.md"), "trial\ntrial\n")?;
    std::fs::write(repo.path().join("tests/flow.rs"), "trial\n")?;
    std::fs::write(repo.path().join("bench/trial.rs"), "trial\n")?;
    std::fs::write(repo.path().join(".thrulineignore"), "docs/ignored.md\n")?;
    git(repo.path(), &["init", "-q"])?;
    git(repo.path(), &["add", "-A"])?;
    git(
        repo.path(),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
    )?;
    Ok(repo)
}

fn params(term: &str) -> TextAbsentParams {
    TextAbsentParams {
        test_allowlist: Vec::new(),
        json_field_allowlist: Vec::new(),
        conditional_path_allowlist: Vec::new(),
        span_allowlist: Vec::new(),
        terms: vec![term.into()],
        synonyms: vec![],
        surfaces: vec![Surface::Code, Surface::Docs, Surface::Tests],
        allowlist: vec!["bench/**".into()],
    }
}

fn surface_count(
    outcome: &thruline_core::decisions::checks::CheckOutcome,
    surface: Surface,
) -> u64 {
    outcome
        .surfaces
        .iter()
        .find(|row| row.surface == surface)
        .expect("surface row")
        .checked_places
}

#[test]
fn a1_counts_each_eligible_tracked_path_and_each_searched_line_once() -> Result<(), Box<dyn Error>>
{
    let repo = fixture()?;
    let broken = check_text_absent_at(repo.path(), "HEAD", &params("trial"));
    assert_eq!(broken.state, VerdictState::Broken);
    assert_eq!(broken.checked_places, 15);
    assert_eq!(surface_count(&broken, Surface::Code), 10);
    assert_eq!(surface_count(&broken, Surface::Docs), 3);
    assert_eq!(surface_count(&broken, Surface::Tests), 2);
    assert_eq!(
        broken
            .surfaces
            .iter()
            .map(|s| s.checked_places)
            .sum::<u64>(),
        broken.checked_places
    );
    assert!(broken
        .findings
        .iter()
        .any(|f| f.path == "src/trial.rs" && f.line == 0));
    assert!(!broken
        .findings
        .iter()
        .any(|f| f.path == "docs/ignored.md" || f.path == "bench/trial.rs"));
    assert!(broken.checked_places > broken.findings.len() as u64);

    let holds = check_text_absent_at(repo.path(), "HEAD", &params("nonexistent"));
    assert_eq!(holds.state, VerdictState::Holds);
    assert_eq!(holds.findings.len(), 0);
    assert_eq!(
        holds.checked_places, 15,
        "zero findings does not mean zero inspected places"
    );
    std::fs::write(repo.path().join("src/main.rs"), "trial\n".repeat(100))?;
    assert_eq!(
        check_text_absent_at(repo.path(), "HEAD", &params("trial")),
        broken
    );
    Ok(())
}

#[test]
fn verdict_row_keeps_checked_places_and_cache_reads_them_back() -> Result<(), Box<dyn Error>> {
    let repo = fixture()?;
    let ledger = Ledger::open(repo.path().join(".git/decision-ledger.db"))?;
    ledger.conn().execute(
        "INSERT INTO projects(id,root_path,source,created_at) VALUES(?1,?2,'codex','2026-10-07T00:00:00Z')",
        rusqlite::params!["project-a", repo.path().to_string_lossy().as_ref()],
    )?;
    let span = "No trial anywhere";
    let decision_id = storage::create(
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
                raw_line: 1,
                span_start: 0,
                span_end: span.len() as u64,
                span: span.into(),
                actor: Some("founder".into()),
                recorded_at: Some("2026-10-07T00:00:00Z".into()),
            }],
            author_actor: "founder".into(),
            recorded_at: "2026-10-07T00:00:00Z".into(),
            params: CheckParams::TextAbsent(params("trial")),
        },
    )?;
    let acceptance_id = storage::accept(
        &ledger,
        AcceptanceInput {
            decision_id: decision_id.clone(),
            revision: 1,
            actor: "founder".into(),
            at: "2026-10-07T01:00:00Z".into(),
            because: "Standing check".into(),
            severity: "blocks".into(),
            applies_from_version: "HEAD".into(),
        },
    )?;
    let mut person = PersonRecorder::new(&ledger, "project-a");
    person.bind_acceptance(&ledger, &acceptance_id);
    let first = engine::check_decision_version(
        &ledger,
        "project-a",
        repo.path(),
        "HEAD",
        "2026-10-07T02:00:00Z",
        false,
        &decision_id,
    )?;
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].outcome.checked_places, 15);
    let verdict_id = first[0].verdict_id.as_ref().expect("stored verdict");
    let (basis, surfaces): (String, String) = ledger.conn().query_row(
        "SELECT basis_json,surfaces_json FROM decision_verdicts WHERE id=?1",
        [verdict_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let basis: Value = serde_json::from_str(&basis)?;
    let surfaces: Value = serde_json::from_str(&surfaces)?;
    assert_eq!(basis["checked_places"], 15);
    assert!(basis["not_looked_at"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["path"] == "docs/ignored.md" && row["source"] == "recorded_ignore"));
    assert_eq!(surfaces.as_array().expect("surface rows").len(), 3);
    assert_eq!(surfaces[0]["checked_places"], 10);

    let second = engine::check_decision_version(
        &ledger,
        "project-a",
        repo.path(),
        "HEAD",
        "2026-10-07T03:00:00Z",
        false,
        &decision_id,
    )?;
    assert!(second[0].from_cache);
    assert_eq!(second[0].outcome.checked_places, 15);
    assert_eq!(surface_count(&second[0].outcome, Surface::Docs), 3);
    assert_eq!(
        second[0].outcome.not_looked_at,
        first[0].outcome.not_looked_at
    );
    Ok(())
}
