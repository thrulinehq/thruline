//! T8 — an unobservable binding carries a **typed** cause, not only prose.
//!
//! `bind` refuses when the tree moved after a check and the changed paths
//! could not be computed (git could not answer). Until T8 that refusal said so
//! only in a sentence, and every reader downstream had to either ignore it or
//! match words. The cause is now a closed slug on the serialized binding, so
//! the UI can render "we can't tell which code this was checked on" as Fog
//! from a field, never from prose.

use serde_json::Value;
use thruline_verify::binding::{bind, CheckView, EditView, ParsedView, SessionView, SnapshotView};
use thruline_verify::tests_pass::{verify, Inputs};
use thruline_verify::verdict::Verdict;

fn check(tree: &str, changed: Option<Vec<String>>) -> CheckView {
    CheckView {
        id: "chk_20".to_string(),
        seq: Some(20),
        category: "test".to_string(),
        runner: "pytest".to_string(),
        bypass: None,
        argv: Some(vec!["pytest".to_string(), "-q".to_string()]),
        exit_code: 0,
        input_tree_hash: tree.to_string(),
        paths_changed_since_final: changed,
        stdio: Some("pipe".into()),
        reexecution: None,
        parsed: Some(ParsedView {
            tests_total: 4,
            passed: 4,
            failed: 0,
            skipped: 0,
            xfail: 0,
        }),
    }
}

/// A check on tree `aaa`, a final tree `bbb`, and no edit event after the
/// check: only the tree half of §6.1 rule 2 can decide.
fn session(changed: Option<Vec<String>>) -> SessionView {
    SessionView {
        session_id: "s".into(),
        snapshots: vec![
            SnapshotView {
                seq: 10,
                tree_hash: "aaa".into(),
            },
            SnapshotView {
                seq: 30,
                tree_hash: "bbb".into(),
            },
        ],
        edits: vec![EditView {
            seq: 12,
            path: "src/app.py".into(),
        }],
        checks: vec![check("aaa", changed)],
        head_commit: Some("c0ffee".into()),
        capture_degraded: false,
        degraded_reason: None,
    }
}

fn cause_of(binding: &Value) -> Option<&str> {
    binding
        .get("unobservable")
        .and_then(|u| u.get("cause"))
        .and_then(Value::as_str)
}

#[test]
fn unobservable_carries_a_typed_cause() {
    let s = session(None);
    let binding = serde_json::to_value(bind(&s, &s.checks[0])).expect("serialize");
    assert_eq!(
        cause_of(&binding),
        Some("workspace_unreadable"),
        "the tree moved and git could not name the paths: the cause is typed, \
         not only a sentence — {binding}"
    );
    let v = verify("clm_1", &s, &all_scope());
    assert_eq!(v.verdict, Verdict::Unverifiable, "{v:#?}");
}

#[test]
fn every_unobservable_binding_names_its_cause() {
    // No final tree.
    let mut none = session(None);
    none.snapshots.clear();
    let b = serde_json::to_value(bind(&none, &none.checks[0])).expect("serialize");
    assert_eq!(cause_of(&b), Some("no_final_tree"), "{b}");

    // A check with no input tree.
    let mut no_input = session(None);
    no_input.checks[0].input_tree_hash = String::new();
    let b = serde_json::to_value(bind(&no_input, &no_input.checks[0])).expect("serialize");
    assert_eq!(cause_of(&b), Some("no_input_tree"), "{b}");

    // A check the chain does not place.
    let mut unplaced = session(None);
    unplaced.checks[0].seq = None;
    let b = serde_json::to_value(bind(&unplaced, &unplaced.checks[0])).expect("serialize");
    assert_eq!(cause_of(&b), Some("unplaced"), "{b}");
}

#[test]
fn when_git_answers_the_same_record_binds_as_computed() {
    // The control: the same trees with the paths known. Fog is caused by the
    // missing fact, not by the fixture.
    let s = session(Some(vec!["tests/test_app.py".into()]));
    let binding = serde_json::to_value(bind(&s, &s.checks[0])).expect("serialize");
    assert_eq!(cause_of(&binding), None, "{binding}");
    assert!(
        binding.get("no_relevant_change_since").is_some(),
        "{binding}"
    );
    let v = verify("clm_1", &s, &all_scope());
    assert_eq!(v.verdict, Verdict::Supported, "{v:#?}");
}

fn all_scope() -> Inputs {
    Inputs {
        scope_is_all: true,
        ..Inputs::default()
    }
}
