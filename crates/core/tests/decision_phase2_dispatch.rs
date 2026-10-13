//! RED integration contract for J1–J6, F1–F4 and B3–B5.
//!
//! The pure checkers have separate complete-evidence fixtures in
//! `decision_claims`, `decision_process` and `decision_preservation`. This
//! file requires the same kinds to pass the closed parameter boundary and
//! reach the accepted-decision engine. Today's ledger cannot supply a complete
//! exact-version census for them, so its honest production result is
//! `unverifiable`. No unrecorded synthetic evidence is injected into a verdict.

// Keep the complete-evidence fixture checks in this focused target as well:
// they prove the typed checker semantics while the dispatch cases below stay
// red. J2, J3, J5 and J6 intentionally abstain on contradictory evidence.
#[path = "decision_claims.rs"]
mod complete_claim_evidence;
#[path = "decision_preservation.rs"]
mod complete_preservation_evidence;
#[path = "decision_process.rs"]
mod complete_process_evidence;
#[path = "support/person_action.rs"]
mod person_action;

use std::path::Path;
use std::process::Command;

use person_action::PersonRecorder;
use rusqlite::params;
use serde_json::{json, Value};
use thruline_core::decisions::checks::VerdictState;
use thruline_core::decisions::engine;
use thruline_core::decisions::params::parse_params;
use thruline_core::ledger::Ledger;

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("fixture git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git output is UTF-8")
        .trim()
        .to_owned()
}

fn sample(kind: &str) -> Value {
    match kind {
        "decision_satisfied" => {
            json!({"kind":kind,"claim_id":"claim-1","decision_id":"decision-1","binding_basis":"explicit"})
        }
        "stale_claim" => json!({"kind":kind,"claim_id":"claim-1"}),
        "summed_totals" => json!({"kind":kind,"claim_id":"claim-1","run_ids":["run-1","run-2"]}),
        "empty_run" => json!({"kind":kind,"claim_id":"claim-1","run_id":"run-1"}),
        "delegated_run" => json!({"kind":kind,"claim_id":"claim-1","run_source":"same_session"}),
        "edit_during_run" => json!({"kind":kind,"run_id":"run-1","edit_ids":["edit-1"]}),
        "command_forbidden" => json!({"kind":kind,"patterns":["git push"],"session_scope":null}),
        "tests_before_code" => {
            json!({"kind":kind,"change_id":"change-1","test_paths":["tests/**"],"source_paths":["src/**"]})
        }
        "consent_recorded" => {
            json!({"kind":kind,"action_class":"publish","consent_event_kind":"approval"})
        }
        "one_heavy_job" => {
            json!({"kind":kind,"command_patterns":["cargo test"],"window":{"start_ms":1,"end_ms":100}})
        }
        "assertion_floor" => {
            json!({"kind":kind,"paths":["tests/**"],"minimum":2,"baseline_version":"baseline-commit"})
        }
        "ci_step_present" => {
            json!({"kind":kind,"workflow":".github/workflows/ci.yml","step":"test","threshold_pct":80})
        }
        "rule_file_present" => {
            json!({"kind":kind,"paths":["CLAUDE.md"],"person_edit_basis":"same_version"})
        }
        _ => panic!("unknown test kind {kind}"),
    }
}

fn assert_closed_and_fail_closed(kind: &str) {
    let raw = sample(kind).to_string();
    let parsed =
        parse_params(kind, 1, &raw).expect("a documented Phase 2 kind has closed parameters");
    assert_eq!(parsed.kind(), kind);
    assert!(
        parse_params("different_kind", 1, &raw).is_err(),
        "kind mismatch must be rejected"
    );
    let mut unknown = sample(kind);
    unknown["unaccepted_override"] = json!(true);
    assert!(
        parse_params(kind, 1, &unknown.to_string()).is_err(),
        "unknown fields must be rejected"
    );

    let repo = tempfile::tempdir().expect("scratch repo");
    git(repo.path(), &["init", "-q"]);
    std::fs::write(repo.path().join("CLAUDE.md"), "fixture\n").expect("fixture file");
    git(repo.path(), &["add", "."]);
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
    );
    let version = git(repo.path(), &["rev-parse", "HEAD"]);
    let tree = git(repo.path(), &["rev-parse", "HEAD^{tree}"]);

    let store = tempfile::tempdir().expect("scratch ledger directory");
    let ledger = Ledger::open(store.path().join("ledger.db")).expect("scratch ledger");
    ledger.conn().execute(
        "INSERT INTO projects(id,root_path,source,created_at) VALUES('phase2-project',?1,'codex','2026-10-07T00:00:00Z')",
        [repo.path().to_string_lossy().as_ref()],
    ).expect("project");
    let id = format!("phase2-{kind}");
    ledger.conn().execute(
        "INSERT INTO decisions(id,project_id,created_at) VALUES(?1,'phase2-project','2026-10-07T00:00:00Z')",
        [&id],
    ).expect("decision");
    ledger.conn().execute(
        "INSERT INTO decision_revisions(decision_id,rev,kind,params_schema_version,params_json,text,sources_json,author_actor,recorded_at)
         VALUES(?1,1,?2,1,?3,'fixture','[]','person','2026-10-07T00:00:00Z')",
        params![id, kind, raw],
    ).expect("revision");
    ledger.conn().execute(
        "INSERT INTO decision_acceptances(id,decision_id,rev,actor,accepted_at,rationale,severity,applies_from_version)
         VALUES(?1,?2,1,'person','2026-10-07T00:01:00Z','fixture','warns',?3)",
        params![format!("accept-{kind}"), id, version],
    ).expect("accepted fixture");
    let mut person = PersonRecorder::new(&ledger, "phase2-project");
    person.bind_bridge_acceptance(&ledger, &format!("accept-{kind}"));

    let result = engine::check_decision_version(
        &ledger,
        "phase2-project",
        repo.path(),
        &version,
        "2026-10-07T00:02:00Z",
        false,
        &id,
    )
    .expect("accepted kind dispatches to a fail-closed result");
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].tree_hash.as_deref(), Some(tree.as_str()));
    assert_eq!(
        result[0].state,
        VerdictState::Unverifiable,
        "{kind} has no complete exact-version record; it cannot hold or break yet"
    );
    assert!(result[0].outcome.reason.is_some());
    if let Some(expected) = match kind {
        "tests_before_code" => Some(("The record cannot link this Change's test edits and runs to code edits in a verified order.", "event_specific_change_and_run_order")),
        "consent_recorded" => Some(("The record has no typed person consent answer linked before this action.", "typed_person_consent")),
        "one_heavy_job" => Some(("The record has no complete start and end times for matching heavy jobs across sessions.", "command_end_time")),
        "rule_file_present" => Some(("The protected rule files exist, but the record cannot prove who edited them on this version.", "person_edit_provenance")),
        "decision_satisfied" => Some(("The claim has no verified link to an accepted decision verdict on its exact version.", "claim_exact_version_tree_binding")),
        "stale_claim" => Some(("The record cannot order this claim, later relevant edits, and a rerun on one exact version.", "complete_closed_session_and_relevant_edit_census")),
        "summed_totals" => Some(("The claimed total has no verified single-run tally on this exact version.", "claim_to_run_same_session_binding")),
        "empty_run" => Some(("The selected run has no complete verified result and examined-input receipt on this exact version.", "selected_run_lacks_a_complete_exact_tree_wrapper_receipt")),
        "delegated_run" => Some(("The record cannot link the claimed pass to a run's actual session or CI origin.", "claim_to_run_same_session_binding")),
        "edit_during_run" => Some(("The record cannot order the selected edit inside a verified run interval.", "J6_requires_a_run_and_edit_adapter_not_a_synthetic_claim")),
        _ => None,
    } {
        assert_eq!(result[0].outcome.reason.as_deref(), Some(expected.0), "{kind}");
        let (checkable_if, basis): (Option<String>, String) = ledger.conn().query_row(
            "SELECT checkable_if,basis_json FROM decision_verdicts WHERE id=?1",
            [result[0].verdict_id.as_deref().expect("stored verdict")],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).expect("stored limitation receipt");
        assert!(checkable_if.as_deref().is_some_and(|text| text.starts_with("v0.3: ")), "{kind}: {checkable_if:?}");
        let basis: Value = serde_json::from_str(&basis).expect("basis JSON");
        assert!(basis["record_projection"]["missing"]
            .as_array().expect("structured missing facts")
            .contains(&json!(expected.1)), "{kind}: {basis}");
    }
    if kind == "rule_file_present" {
        std::fs::remove_file(repo.path().join("CLAUDE.md")).expect("remove protected file");
        git(repo.path(), &["add", "-u"]);
        git(
            repo.path(),
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-qm",
                "missing rule file",
            ],
        );
        let missing_version = git(repo.path(), &["rev-parse", "HEAD"]);
        let missing = engine::check_decision_version(
            &ledger,
            "phase2-project",
            repo.path(),
            &missing_version,
            "2026-10-07T00:03:00Z",
            false,
            &id,
        )
        .expect("missing rule file check");
        assert_eq!(missing[0].state, VerdictState::Broken);
        assert!(!missing[0].outcome.findings.is_empty());
        let checkable_if: Option<String> = ledger
            .conn()
            .query_row(
                "SELECT checkable_if FROM decision_verdicts WHERE id=?1",
                [missing[0]
                    .verdict_id
                    .as_deref()
                    .expect("stored broken verdict")],
                |row| row.get(0),
            )
            .expect("stored broken checkable_if");
        assert!(checkable_if.is_none(), "broken file is an actual witness");
    }
}

macro_rules! phase2_case {
    ($name:ident, $kind:literal) => {
        #[test]
        fn $name() {
            assert_closed_and_fail_closed($kind);
        }
    };
}

phase2_case!(j1_decision_satisfied, "decision_satisfied");
phase2_case!(j2_stale_claim, "stale_claim");
phase2_case!(j3_summed_totals, "summed_totals");
phase2_case!(j4_empty_run, "empty_run");
phase2_case!(j5_delegated_run, "delegated_run");
phase2_case!(j6_edit_during_run, "edit_during_run");
phase2_case!(f1_command_forbidden, "command_forbidden");
phase2_case!(f2_tests_before_code, "tests_before_code");
phase2_case!(f3_consent_recorded, "consent_recorded");
phase2_case!(f4_one_heavy_job, "one_heavy_job");
phase2_case!(b3_assertion_floor, "assertion_floor");
phase2_case!(b4_ci_step_present, "ci_step_present");
phase2_case!(b5_rule_file_present, "rule_file_present");
