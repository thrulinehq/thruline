//! Tree identity alone cannot freeze later recorded evidence.

#[path = "support/person_action.rs"]
mod person_action;

use std::process::Command;

use person_action::PersonRecorder;
use serde_json::json;
use thruline_core::decisions::checks::VerdictState;
use thruline_core::decisions::engine;
use thruline_core::Ledger;

#[test]
fn a_j_verdict_keeps_accepted_selectors_and_missing_source_facts_in_its_basis() {
    let repo = tempfile::tempdir().expect("repository");
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo.path())
            .output()
            .expect("git");
        assert!(output.status.success(), "git {args:?}");
        String::from_utf8(output.stdout)
            .expect("Git text")
            .trim()
            .to_owned()
    };
    git(&["init", "-q"]);
    git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "--allow-empty",
        "-qm",
        "selected tree",
    ]);
    let version = git(&["rev-parse", "HEAD"]);
    let store = tempfile::tempdir().expect("scratch ledger directory");
    let ledger = Ledger::open(store.path().join("ledger.db")).expect("scratch ledger");
    ledger.conn().execute(
        "INSERT INTO projects(id,root_path,source,created_at) VALUES('p',?1,'codex','2026-10-07T00:00:00Z')",
        [repo.path().to_string_lossy().as_ref()],
    ).expect("project");
    ledger.conn().execute(
        "INSERT INTO decisions(id,project_id,created_at) VALUES('run-rule','p','2026-10-07T00:00:00Z')", [],
    ).expect("decision");
    ledger.conn().execute(
        "INSERT INTO decision_revisions(decision_id,rev,kind,params_schema_version,params_json,text,sources_json,author_actor,recorded_at)
         VALUES('run-rule',1,'decision_satisfied',1,?1,'Tests must pass','[]','person','2026-10-07T00:00:00Z')",
        [json!({"kind":"decision_satisfied","claim_id":"selected-claim","decision_id":"selected-decision","binding_basis":"explicit"}).to_string()],
    ).expect("revision");
    ledger.conn().execute(
        "INSERT INTO decision_acceptances(id,decision_id,rev,actor,accepted_at,rationale,severity,applies_from_version)
         VALUES('run-accept','run-rule',1,'person','2026-10-07T00:01:00Z','keep a recorded test run','warns',?1)",
        [&version],
    ).expect("acceptance");
    let mut person = PersonRecorder::new(&ledger, "p");
    person.bind_bridge_acceptance(&ledger, "run-accept");
    let result = engine::check_decision_version(
        &ledger,
        "p",
        repo.path(),
        &version,
        "2026-10-07T00:02:00Z",
        false,
        "run-rule",
    )
    .expect("ordinary J check");
    assert_eq!(result[0].state, VerdictState::Unverifiable);
    let basis: String = ledger
        .conn()
        .query_row(
            "SELECT basis_json FROM decision_verdicts WHERE id=?1",
            [result[0].verdict_id.as_deref().unwrap()],
            |row| row.get(0),
        )
        .expect("stored J receipt");
    let basis: serde_json::Value = serde_json::from_str(&basis).expect("basis JSON");
    let projection = &basis["record_projection"];
    assert_eq!(
        projection["selectors"],
        json!({"kind":"decision_satisfied","claim_id":"selected-claim","decision_id":"selected-decision","binding_basis":"explicit"})
    );
    assert_eq!(projection["sources"], json!([]));
    assert!(!projection["missing"]
        .as_array()
        .expect("missing facts")
        .is_empty());
    let (reason, checkable_if): (Option<String>, Option<String>) = ledger
        .conn()
        .query_row(
            "SELECT reason,checkable_if FROM decision_verdicts WHERE id=?1",
            [result[0].verdict_id.as_deref().expect("stored result")],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("stored explanation");
    assert_eq!(
        reason.as_deref(),
        Some(
            "The claim has no verified link to an accepted decision verdict on its exact version."
        )
    );
    assert!(checkable_if
        .as_deref()
        .is_some_and(|text| text.starts_with("v0.3: ")));
}
