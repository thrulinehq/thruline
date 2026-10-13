//! Accepted exclusions disclose exact paths and named Rust test spans.
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;
use thruline_core::decisions::checks::{check_at, VerdictState};
use thruline_core::decisions::params::parse_params;

fn fixture(source: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::create_dir(dir.path().join("docs")).unwrap();
    std::fs::write(dir.path().join("src/gate.rs"), source).unwrap();
    std::fs::write(dir.path().join("docs/history.md"), "trial history\n").unwrap();
    std::fs::write(dir.path().join(".thrulineignore"), "docs/history.md\n").unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
    ] {
        assert!(Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status()
            .unwrap()
            .success());
    }
    dir
}
fn params(scopes: Value, paths: Value) -> Value {
    json!({"kind":"text_absent","terms":["trial"],"synonyms":[],"surfaces":["code","docs","tests","ui_copy"],"allowlist":paths,"test_allowlist":scopes})
}
fn check(root: &Path, p: Value) -> thruline_core::decisions::checks::CheckOutcome {
    let parsed = parse_params("text_absent", 1, &p.to_string()).unwrap();
    check_at(root, "HEAD", &parsed)
}
fn scope() -> Value {
    json!([{"path":"src/gate.rs","name_pattern":"no_trial*"}])
}

#[test]
fn every_accepted_path_and_recorded_ignore_is_visible_as_not_looked_at() {
    let dir = fixture("fn main() { let _ = \"trial\"; }\n");
    let outcome = check(dir.path(), params(json!([]), json!(["src/gate.rs"])));
    assert_eq!(outcome.state, VerdictState::Holds);
    let rows = serde_json::to_value(outcome).unwrap()["not_looked_at"]
        .as_array()
        .unwrap()
        .clone();
    assert!(rows
        .iter()
        .any(|r| r["path"] == "src/gate.rs" && r["source"] == "accepted_allowlist"));
    assert!(rows
        .iter()
        .any(|r| r["path"] == "docs/history.md" && r["source"] == "recorded_ignore"));
}

#[test]
fn named_test_scope_keeps_product_and_helper_text_in_the_census() {
    let dir=fixture("fn product() { let _ = \"trial\"; }\nfn helper() { let _ = \"trial\"; }\n#[test]\nfn no_trial_guard() { assert!(!\"free\".contains(\"trial\")); }\n");
    let outcome = check(dir.path(), params(scope(), json!([])));
    assert_eq!(outcome.state, VerdictState::Broken);
    assert_eq!(
        outcome.findings.iter().map(|f| f.line).collect::<Vec<_>>(),
        vec![1, 2]
    );
    let rows = serde_json::to_value(outcome).unwrap()["not_looked_at"]
        .as_array()
        .unwrap()
        .clone();
    assert!(rows.iter().any(|r| r["path"] == "src/gate.rs"
        && r["test_name"] == "no_trial_guard"
        && r["start_line"] == 4
        && r["end_line"] == 4));
}

#[test]
fn an_unmarked_or_malformed_function_cannot_be_excluded_as_a_test() {
    for source in [
        "fn no_trial_guard() { let _ = \"trial\"; }\n",
        "#[test]\nfn no_trial_guard( { \"trial\"\n",
    ] {
        let dir = fixture(source);
        assert_eq!(
            check(dir.path(), params(scope(), json!([]))).state,
            VerdictState::Unverifiable
        );
    }
}

#[test]
fn a_named_test_on_a_shared_line_does_not_hide_its_product_neighbor() {
    let dir = fixture(
        "#[test] fn no_trial_guard() { let _ = \"trial\"; } fn product() { let _ = \"trial\"; }\n",
    );
    let outcome = check(dir.path(), params(scope(), json!([])));
    assert_eq!(outcome.state, VerdictState::Broken);
    assert_eq!(outcome.findings.len(), 1);
    assert_eq!(outcome.findings[0].line, 1);
}

#[test]
fn named_test_exclusions_read_the_commit_instead_of_dirty_source() {
    let dir = fixture("#[test]\nfn no_trial_guard() { let _ = \"trial\"; }\nfn product() {}\n");
    let initial = check(dir.path(), params(scope(), json!([])));
    assert_eq!(initial.state, VerdictState::Holds);
    std::fs::write(
        dir.path().join("src/gate.rs"),
        "fn product() { let _ = \"trial\"; }\n",
    )
    .unwrap();
    assert_eq!(check(dir.path(), params(scope(), json!([]))), initial);
}

#[test]
fn scope_parameters_refuse_unsafe_paths_and_an_unbounded_name() {
    for row in [
        json!({"path":"../gate.rs","name_pattern":"no_trial*"}),
        json!({"path":"src/*.rs","name_pattern":"no_trial*"}),
        json!({"path":"src/gate.rs","name_pattern":"*"}),
        json!({"path":"src/gate.py","name_pattern":"no_trial*"}),
    ] {
        assert!(parse_params(
            "text_absent",
            1,
            &params(json!([row]), json!([])).to_string()
        )
        .is_err());
    }
}

#[cfg(unix)]
#[test]
fn a_symlink_blob_is_not_a_rust_test_file() {
    let dir = fixture("fn product() {}\n");
    let path = dir.path().join("src/gate.rs");
    std::fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink("#[test]\nfn no_trial_guard() { let _ = \"trial\"; }", path)
        .unwrap();
    assert!(Command::new("git")
        .args(["add", "-A"])
        .current_dir(dir.path())
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "symlink"
        ])
        .current_dir(dir.path())
        .status()
        .unwrap()
        .success());
    assert_eq!(
        check(dir.path(), params(scope(), json!([]))).state,
        VerdictState::Unverifiable
    );
}
