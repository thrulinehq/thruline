//! C1 reaches the visual checker through the accepted exact-version engine.

#[path = "support/person_action.rs"]
mod person_action;

use person_action::PersonRecorder;

use std::path::Path;
use std::process::Command;

use serde_json::{json, Value};
use thruline_core::decisions::engine::check_decision_version;
use thruline_core::decisions::params::{parse_params, PARAMS_SCHEMA_VERSION};
use thruline_core::decisions::storage::{
    self, AcceptanceInput, DecisionSource, NewDecision, SourceKind,
};
use thruline_core::ledger::Ledger;

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("Git runs");
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8(output.stdout)
        .expect("Git text")
        .trim()
        .into()
}

#[test]
fn accepted_c1_uses_visual_checker_and_records_visual_tool_identity() {
    let repo = tempfile::tempdir().expect("repo");
    git(repo.path(), &["init", "-q"]);
    std::fs::write(repo.path().join("README.md"), "Visual test tree\n").expect("tree");
    git(repo.path(), &["add", "README.md"]);
    git(
        repo.path(),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "visual fixture",
        ],
    );
    let version = git(repo.path(), &["rev-parse", "HEAD"]);
    let ledger = Ledger::open(repo.path().join("scratch-ledger.db")).expect("scratch ledger");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id,root_path,source,created_at) VALUES(?1,?2,'codex','2026-10-07T00:00:00Z')",
            rusqlite::params!["visual-project", repo.path().display().to_string()],
        )
        .expect("project");
    let params = json!({
        "kind":"visual_match",
        "tolerance_pct":0.5,
        "screens":[{
            "name":"Inbox",
            "route":"/#/design/Inbox",
            "scenario":"design",
            "reference_path":"docs/hq-next/reference/Inbox.png",
            "reference_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "width":1512,
            "height":920,
            "full_page":false,
            "mask_rectangles":[]
        }]
    });
    let params = parse_params("visual_match", PARAMS_SCHEMA_VERSION, &params.to_string())
        .expect("visual params");
    let span = "Screens match the agreed design";
    let id = storage::create(
        &ledger,
        NewDecision {
            project_id: "visual-project".into(),
            source_anchor: "founder-design-prompt".into(),
            span: span.into(),
            sources: vec![DecisionSource {
                source_kind: SourceKind::WorkPrompt,
                event_id: Some("founder-design-prompt".into()),
                file_anchor: None,
                raw_file: "session.jsonl".into(),
                raw_line: 7,
                span_start: 0,
                span_end: span.len() as u64,
                span: span.into(),
                actor: Some("founder".into()),
                recorded_at: Some("2026-10-07T00:00:00Z".into()),
            }],
            author_actor: "founder".into(),
            recorded_at: "2026-10-07T00:00:00Z".into(),
            params,
        },
    )
    .expect("proposal");
    let acceptance_id = storage::accept(
        &ledger,
        AcceptanceInput {
            decision_id: id.clone(),
            revision: 1,
            actor: "founder".into(),
            at: "2026-10-07T00:01:00Z".into(),
            because: "Agreed canvas".into(),
            severity: "blocks".into(),
            applies_from_version: version.clone(),
        },
    )
    .expect("acceptance");
    let mut person = PersonRecorder::new(&ledger, "visual-project");
    person.bind_bridge_acceptance(&ledger, &acceptance_id);
    let results = check_decision_version(
        &ledger,
        "visual-project",
        repo.path(),
        &version,
        "2026-10-07T00:02:00Z",
        false,
        &id,
    )
    .expect("C1 result");
    assert_eq!(results.len(), 1);
    assert_ne!(
        results[0].outcome.reason.as_deref(),
        Some("check kind is not implemented"),
        "accepted C1 must reach its checker"
    );
    assert_ne!(
        results[0].outcome.reason.as_deref(),
        Some("visual capture needs a file-backed ledger"),
        "the scratch ledger path must reach C1"
    );
    let tools_json: String = ledger
        .conn()
        .query_row(
            "SELECT tool_versions_json FROM decision_verdicts WHERE decision_id=?1",
            [&id],
            |row| row.get(0),
        )
        .expect("tool identity");
    let tools: Value = serde_json::from_str(&tools_json).expect("tool identity JSON");
    assert!(tools.get("visual").is_some(), "{tools}");
    let basis_json: String = ledger
        .conn()
        .query_row(
            "SELECT basis_json FROM decision_verdicts WHERE decision_id=?1",
            [&id],
            |row| row.get(0),
        )
        .expect("basis");
    let basis: Value = serde_json::from_str(&basis_json).expect("basis JSON");
    assert!(
        basis["visual"].is_array(),
        "visual report must persist: {basis}"
    );

    let second = check_decision_version(
        &ledger,
        "visual-project",
        repo.path(),
        &version,
        "2026-10-07T00:03:00Z",
        false,
        &id,
    )
    .expect("fresh visual computation");
    assert!(!second[0].from_cache, "record-backed C1 input may change");
    let verdict_count: i64 = ledger
        .conn()
        .query_row(
            "SELECT count(*) FROM decision_verdicts WHERE decision_id=?1",
            [&id],
            |row| row.get(0),
        )
        .expect("verdict count");
    assert_eq!(verdict_count, 2);
    let replay = check_decision_version(
        &ledger,
        "visual-project",
        repo.path(),
        &version,
        "2026-10-07T00:04:00Z",
        true,
        &id,
    )
    .expect("replay compares fresh visual result");
    assert_eq!(replay[0].outcome, second[0].outcome);
}
