//! D1, D2, E1 and I4 use recorded evidence from one exact version.

use thruline_core::decisions::checks::{CheckOutcome, VerdictState};
use thruline_core::decisions::params::{
    CommandPassesParams, EditsWithinPathsParams, PathsUntouchedParams,
};

// Compile the new checker before the shared module registration lands.
mod decisions {
    pub use thruline_core::decisions::{checks, params};
}
mod change_link {
    pub use thruline_core::change_link::*;
}
mod redact {
    pub use thruline_core::redact::*;
}
mod test_tally {
    pub use thruline_core::test_tally::*;
}
#[path = "../src/decisions/checks_record.rs"]
mod checks_record;

use checks_record::{
    check_command_passes, check_decision_without_implementation, check_edits_within_paths,
    check_paths_untouched, EditActor, EditCensus, RunCensus,
};

fn edits_holds() -> EditCensus {
    serde_json::from_str(include_str!(
        "fixtures/decisions/record_checks/edits_holds.json"
    ))
    .expect("valid edit fixture")
}

fn edits_broken() -> EditCensus {
    serde_json::from_str(include_str!(
        "fixtures/decisions/record_checks/edits_broken.json"
    ))
    .expect("valid edit fixture")
}

fn edits_empty() -> EditCensus {
    serde_json::from_str(include_str!(
        "fixtures/decisions/record_checks/edits_empty.json"
    ))
    .expect("valid edit fixture")
}

fn runs_holds() -> RunCensus {
    serde_json::from_str(include_str!(
        "fixtures/decisions/record_checks/runs_holds.json"
    ))
    .expect("valid run fixture")
}

fn runs_broken() -> RunCensus {
    serde_json::from_str(include_str!(
        "fixtures/decisions/record_checks/runs_broken.json"
    ))
    .expect("valid run fixture")
}

fn target(state: VerdictState) -> CheckOutcome {
    CheckOutcome {
        state,
        tree_hash: Some("tree-v2".into()),
        reason: None,
        checked_places: 0,
        findings: vec![],
        surfaces: vec![],
        not_looked_at: Vec::new(),
        visual: vec![],
    }
}

#[test]
fn d1_all_edits_stay_inside_accepted_paths_or_name_the_outside_file() {
    let params = EditsWithinPathsParams {
        allowed_paths: vec!["ui/**".into(), "LICENSE".into()],
    };
    let held = check_edits_within_paths("v2", "tree-v2", &params, &edits_holds());
    assert_eq!(held.state, VerdictState::Holds);
    assert!(held.findings.is_empty());

    let broken = check_edits_within_paths("v2", "tree-v2", &params, &edits_broken());
    assert_eq!(broken.state, VerdictState::Broken);
    assert_eq!(broken.findings.len(), 1);
    assert_eq!(broken.findings[0].path, "src/lib.rs");

    let mut incomplete = edits_holds();
    incomplete.complete = false;
    assert_eq!(
        check_edits_within_paths("v2", "tree-v2", &params, &incomplete).state,
        VerdictState::Unverifiable
    );
    let unsupported = EditsWithinPathsParams {
        allowed_paths: vec!["src/*.rs".into()],
    };
    assert_eq!(
        check_edits_within_paths("v2", "tree-v2", &unsupported, &edits_broken()).state,
        VerdictState::Unverifiable
    );
}

#[test]
fn d2_person_edit_is_allowed_but_agent_edit_to_named_path_is_broken() {
    let params = PathsUntouchedParams {
        paths: vec!["LICENSE".into(), ".github/workflows/**".into()],
    };
    assert_eq!(
        check_paths_untouched("v2", "tree-v2", &params, &edits_holds()).state,
        VerdictState::Holds
    );
    let broken = check_paths_untouched("v2", "tree-v2", &params, &edits_broken());
    assert_eq!(broken.state, VerdictState::Broken);
    assert_eq!(broken.findings.len(), 1);
    assert_eq!(broken.findings[0].path, "LICENSE");

    let mut unknown = edits_holds();
    unknown.edits[1].actor = EditActor::Unknown;
    assert_eq!(
        check_paths_untouched("v2", "tree-v2", &params, &unknown).state,
        VerdictState::Unverifiable
    );
}

#[test]
fn e1_uses_last_complete_run_on_the_exact_version_and_test_tally() {
    let params = CommandPassesParams {
        command: "cargo test".into(),
        is_test: true,
        min_passed: 3,
    };
    assert_eq!(
        check_command_passes("v2", "tree-v2", &params, &runs_holds()).state,
        VerdictState::Holds
    );
    let broken = check_command_passes("v2", "tree-v2", &params, &runs_broken());
    assert_eq!(broken.state, VerdictState::Broken);
    assert_eq!(broken.findings.len(), 1);

    let mut stale = runs_holds();
    stale.runs[0].version_id = "v1".into();
    assert_eq!(
        check_command_passes("v2", "tree-v2", &params, &stale).state,
        VerdictState::Unverifiable,
        "an earlier version's green run cannot pass this version"
    );
    let mut no_errors_basis = runs_holds();
    no_errors_basis.runs[0].errors = None;
    assert_eq!(
        check_command_passes("v2", "tree-v2", &params, &no_errors_basis).state,
        VerdictState::Unverifiable
    );
    let mut wrong_tree = runs_holds();
    wrong_tree.runs[0].tree_hash = "tree-v1".into();
    assert_eq!(
        check_command_passes("v2", "tree-v2", &params, &wrong_tree).state,
        VerdictState::Unverifiable
    );
    let mut tied_last_runs = runs_broken();
    tied_last_runs.runs[1].seq = tied_last_runs.runs[0].seq;
    assert_eq!(
        check_command_passes("v2", "tree-v2", &params, &tied_last_runs).state,
        VerdictState::Unverifiable,
        "two different results cannot both be the last recorded run"
    );
}

#[test]
fn e1_non_test_command_needs_a_complete_zero_exit_run_but_no_test_tally() {
    let mut census = runs_holds();
    census.runs[0].command = "cargo build".into();
    census.runs[0].tally = None;
    census.runs[0].errors = None;
    let params = CommandPassesParams {
        command: "cargo build".into(),
        is_test: false,
        min_passed: 0,
    };
    assert_eq!(
        check_command_passes("v2", "tree-v2", &params, &census).state,
        VerdictState::Holds
    );
    census.runs[0].complete = false;
    assert_eq!(
        check_command_passes("v2", "tree-v2", &params, &census).state,
        VerdictState::Unverifiable
    );
}

#[test]
fn i4_no_edit_is_a_break_only_when_the_target_check_is_broken() {
    let empty = edits_empty();
    let paths = vec!["src/**".into()];
    assert_eq!(
        check_decision_without_implementation(
            "v2",
            "tree-v2",
            "v1",
            &paths,
            &target(VerdictState::Holds),
            &empty
        )
        .state,
        VerdictState::Holds,
        "a rule already held at acceptance"
    );
    let broken = check_decision_without_implementation(
        "v2",
        "tree-v2",
        "v1",
        &paths,
        &target(VerdictState::Broken),
        &empty,
    );
    assert_eq!(broken.state, VerdictState::Broken);
    assert!(broken
        .reason
        .as_deref()
        .unwrap_or_default()
        .contains("nothing changed"));

    let mut missing = empty;
    missing.complete = false;
    assert_eq!(
        check_decision_without_implementation(
            "v2",
            "tree-v2",
            "v1",
            &paths,
            &target(VerdictState::Broken),
            &missing
        )
        .state,
        VerdictState::Unverifiable
    );
    assert_eq!(
        check_decision_without_implementation(
            "v2",
            "tree-v2",
            "v1",
            &paths,
            &target(VerdictState::Unverifiable),
            &edits_empty()
        )
        .state,
        VerdictState::Unverifiable
    );
}
