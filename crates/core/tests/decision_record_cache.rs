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
fn an_unrecorded_run_is_rechecked_instead_of_frozen_by_a_tree_cache() {
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
    let ledger = Ledger::open(repo.path().join(".git/decision-ledger.db")).expect("scratch ledger");
    ledger.conn().execute(
        "INSERT INTO projects(id,root_path,source,created_at) VALUES('p',?1,'codex','2026-10-07T00:00:00Z')",
        [repo.path().to_string_lossy().as_ref()],
    ).expect("project");
    ledger.conn().execute(
        "INSERT INTO decisions(id,project_id,created_at) VALUES('run-rule','p','2026-10-07T00:00:00Z')", [],
    ).expect("decision");
    ledger.conn().execute(
        "INSERT INTO decision_revisions(decision_id,rev,kind,params_schema_version,params_json,text,sources_json,author_actor,recorded_at)
         VALUES('run-rule',1,'command_passes',1,?1,'Tests must pass','[]','person','2026-10-07T00:00:00Z')",
        [json!({"kind":"command_passes","command":"cargo test","is_test":true,"min_passed":1}).to_string()],
    ).expect("revision");
    ledger.conn().execute(
        "INSERT INTO decision_acceptances(id,decision_id,rev,actor,accepted_at,rationale,severity,applies_from_version)
         VALUES('run-accept','run-rule',1,'person','2026-10-07T00:01:00Z','keep a recorded test run','warns',?1)",
        [&version],
    ).expect("acceptance");
    let mut person = PersonRecorder::new(&ledger, "p");
    person.bind_acceptance(&ledger, "run-accept");
    for _ in 0..2 {
        let result = engine::check_decision_version(
            &ledger,
            "p",
            repo.path(),
            &version,
            "2026-10-07T00:02:00Z",
            false,
            "run-rule",
        )
        .expect("ordinary check");
        assert_eq!(result[0].state, VerdictState::Unverifiable);
        assert!(
            !result[0].from_cache,
            "new runs can arrive without changing the tree"
        );
    }
    let count: i64 = ledger
        .conn()
        .query_row(
            "SELECT count(*) FROM decision_verdicts WHERE decision_id='run-rule'",
            [],
            |row| row.get(0),
        )
        .expect("historical verdicts");
    assert_eq!(
        count, 2,
        "each check must evaluate its current record basis"
    );
}
