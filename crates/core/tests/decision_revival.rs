#[path = "fixtures/decision_conflict/mod.rs"]
mod fixture;
#[path = "support/person_action.rs"]
mod person_action;
use fixture::*;
use thruline_core::decisions::{
    engine,
    revival::{self, RevivalState},
};
fn setup(failed: bool, echo: bool, observed: bool) -> Fixture {
    setup_with_rows(failed, echo, observed, |_| {})
}

fn setup_with_rows(
    failed: bool,
    echo: bool,
    observed: bool,
    mutate: impl FnOnce(&mut Vec<serde_json::Value>),
) -> Fixture {
    setup_with_history(failed, echo, observed, true, mutate)
}

fn setup_with_history(
    failed: bool,
    echo: bool,
    observed: bool,
    record_hold: bool,
    mutate: impl FnOnce(&mut Vec<serde_json::Value>),
) -> Fixture {
    let mut f = Fixture::new();
    std::fs::write(f.root().join("app.rs"), "const ENABLED: bool = false;\n").expect("source");
    f.version = commit(f.root());
    f.decision("p", absence(), true);
    if record_hold {
        engine::check_project_version(
            &f.ledger,
            "p",
            f.root(),
            &f.version,
            "2026-10-07T00:02:00Z",
            false,
        )
        .expect("holds");
    }
    let before = "const ENABLED: bool = false;\n";
    let after = "const OBSOLETE: bool = true;\n";
    std::fs::write(f.root().join("app.rs"), after).expect("source");
    f.version = commit(f.root());
    let mut rows = edit_rows(before, after, &f.version, f.root(), failed, echo);
    if !observed {
        rows.truncate(2);
    }
    mutate(&mut rows);
    f.captured(rows);
    engine::check_project_version(
        &f.ledger,
        "p",
        f.root(),
        &f.version,
        "2026-10-07T00:05:00Z",
        false,
    )
    .expect("broken");
    f
}

#[test]
fn native_git_output_without_exit_field_still_proves_the_observed_version() {
    let f = setup_with_rows(false, true, true, |rows| {
        let native = rows[3]["toolUseResult"]
            .as_object_mut()
            .expect("native result");
        native.remove("exitCode");
        native.insert("interrupted".into(), serde_json::json!(false));
    });
    let r = revival::project_revivals(&f.ledger, "p", f.root(), &f.version).expect("report");
    assert_eq!(r.revivals[0].state, RevivalState::ReAdded);
}
#[test]
fn full_verified_edit_and_version_witness_attributes_readded_line() {
    let f = setup(false, true, true);
    let r = revival::project_revivals(&f.ledger, "p", f.root(), &f.version).expect("report");
    assert_eq!(r.revivals.len(), 1);
    let row = &r.revivals[0];
    assert_eq!(row.state, RevivalState::ReAdded);
    assert_eq!(row.path, "app.rs");
    assert_eq!(row.line, 1);
    assert_eq!(row.session_id.as_deref(), Some("agent"));
    assert!(row.witnesses.len() >= 3);
}
#[test]
fn failed_edit_never_attributes() {
    let f = setup(true, true, true);
    let r = revival::project_revivals(&f.ledger, "p", f.root(), &f.version).expect("report");
    assert_eq!(r.revivals[0].state, RevivalState::Unverifiable);
    assert!(r.revivals[0].session_id.is_none());
}
#[test]
fn request_without_result_echo_never_attributes() {
    let f = setup(false, false, true);
    let r = revival::project_revivals(&f.ledger, "p", f.root(), &f.version).expect("report");
    assert_eq!(r.revivals[0].state, RevivalState::Unverifiable);
}
#[test]
fn edit_without_exact_observed_version_never_attributes() {
    let f = setup(false, true, false);
    let r = revival::project_revivals(&f.ledger, "p", f.root(), &f.version).expect("report");
    assert_eq!(r.revivals[0].state, RevivalState::Unverifiable);
}
#[test]
fn changed_raw_source_never_attributes() {
    let f = setup(false, true, true);
    std::fs::write(f.root().join(".git/agent.jsonl"), "{}\n").expect("alter source");
    let r = revival::project_revivals(&f.ledger, "p", f.root(), &f.version).expect("report");
    assert_eq!(r.revivals[0].state, RevivalState::Unverifiable);
}
#[test]
fn dirty_worktree_does_not_replace_exact_version() {
    let f = setup(false, true, true);
    std::fs::write(f.root().join("app.rs"), "clean\n").expect("dirty source");
    let r = revival::project_revivals(&f.ledger, "p", f.root(), &f.version).expect("report");
    assert_eq!(r.revivals[0].state, RevivalState::ReAdded);
}

#[test]
fn interrupted_version_observation_never_attributes() {
    for exit in [Some(0), None] {
        let f = setup_with_rows(false, true, true, |rows| {
            let native = rows[3]["toolUseResult"].as_object_mut().expect("native");
            if exit.is_none() {
                native.remove("exitCode");
            }
            native.insert("interrupted".into(), serde_json::json!(true));
        });
        let r = revival::project_revivals(&f.ledger, "p", f.root(), &f.version).expect("report");
        assert_eq!(r.revivals[0].state, RevivalState::Unverifiable);
        assert!(r.revivals[0].session_id.is_none());
    }
}

#[test]
fn nonzero_version_result_never_attributes() {
    let f = setup_with_rows(false, true, true, |rows| {
        rows[3]["toolUseResult"]["exitCode"] = serde_json::json!(1);
        rows[3]["toolUseResult"]["interrupted"] = serde_json::json!(false);
    });
    let r = revival::project_revivals(&f.ledger, "p", f.root(), &f.version).expect("report");
    assert_eq!(r.revivals[0].state, RevivalState::Unverifiable);
}

#[test]
fn missing_recorded_earlier_hold_never_attributes() {
    let f = setup_with_history(false, true, true, false, |_| {});
    let r = revival::project_revivals(&f.ledger, "p", f.root(), &f.version).expect("report");
    assert_eq!(r.revivals[0].state, RevivalState::Unverifiable);
    assert!(r.revivals[0].held_version.is_none());
    assert!(r.revivals[0].session_id.is_none());
}

#[test]
fn two_completed_version_witnesses_are_ambiguous() {
    let f = setup_with_rows(false, true, true, |rows| {
        let mut call = rows[2].clone();
        let mut result = rows[3].clone();
        call["message"]["content"][0]["id"] = serde_json::json!("head2");
        result["message"]["content"][0]["tool_use_id"] = serde_json::json!("head2");
        rows.extend([call, result]);
    });
    let r = revival::project_revivals(&f.ledger, "p", f.root(), &f.version).expect("report");
    assert_eq!(r.revivals[0].state, RevivalState::Unverifiable);
    assert!(r.revivals[0].session_id.is_none());
}

#[test]
fn observed_commit_outside_target_ancestry_never_attributes() {
    let f = setup_with_rows(false, true, true, |rows| {
        let full = rows[0]["message"]["content"][0]["input"]["file_path"]
            .as_str()
            .expect("full path")
            .to_owned();
        let root = std::path::Path::new(&full).parent().expect("root");
        git(root, &["checkout", "--orphan", "other-history"]);
        std::fs::write(root.join("app.rs"), "const OBSOLETE: bool = false;\n")
            .expect("other source");
        let other = commit(root);
        rows[3]["toolUseResult"]["stdout"] = serde_json::json!(other);
        rows[3]["message"]["content"][0]["content"] = serde_json::json!(other);
    });
    let r = revival::project_revivals(&f.ledger, "p", f.root(), &f.version).expect("report");
    assert_eq!(r.revivals[0].state, RevivalState::Unverifiable);
    assert!(r.revivals[0].session_id.is_none());
}
