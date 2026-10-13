//! C31: one exact key in one exact Git JSON blob, with no whole-file waiver.

use std::path::Path;
use std::process::Command;
use thruline_core::decisions::c31_json_field::{json_field_spans_at, JsonFieldScope};

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn fixture(source: &str) -> (tempfile::TempDir, String) {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-q"]);
    let path = repo.path().join("vendor/paddle.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, source).unwrap();
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
    let head = git(repo.path(), &["rev-parse", "HEAD"]);
    (repo, head)
}

fn scope() -> JsonFieldScope {
    let bundle: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/decisions/c31/selectors.json")).unwrap();
    serde_json::from_value(bundle["vendor_scope"].clone()).unwrap()
}

#[test]
fn exact_key_term_span_leaves_sibling_and_value_visible() {
    let (repo, head) = fixture("{\n  \"data\": {\"items\": [{\"price\": {\n    \"trial_period\": \"trial in value\", \"trial_label\": \"trial\"\n  }}]}\n}\n");
    let spans = json_field_spans_at(repo.path(), &head, &[scope()]).unwrap();
    assert_eq!(spans.len(), 1);
    let span = &spans[0];
    assert_eq!(span.line, 3);
    assert_eq!((span.start_column, span.end_column), (5, 10));
    assert!(span.matches("vendor/paddle.json", 3, 5, 10, "trial"));
    assert!(!span.matches("vendor/paddle.json", 3, 22, 27, "trial"));
    assert!(!span.matches("vendor/paddle.json", 3, 40, 45, "trial"));
    assert!(!span.matches("vendor/other.json", 3, 5, 10, "trial"));
}

#[test]
fn selector_is_bound_to_committed_blob_not_worktree() {
    let (repo, head) = fixture("{\"data\":{\"items\":[{\"price\":{\"trial_period\":null}}]}}\n");
    let original = json_field_spans_at(repo.path(), &head, &[scope()]).unwrap();
    std::fs::write(
        repo.path().join("vendor/paddle.json"),
        "{\"not_the_blob\":true}\n",
    )
    .unwrap();
    assert_eq!(
        json_field_spans_at(repo.path(), &head, &[scope()]).unwrap(),
        original
    );
}

#[test]
fn wrong_pointer_missing_key_and_escaped_spelling_fail_closed() {
    let (repo, head) = fixture("{\"data\":{\"items\":[{\"price\":{\"trial_period\":null}}]}}\n");
    let mut wrong = scope();
    wrong.pointer = "/data/items/1/price/trial_period".into();
    assert!(json_field_spans_at(repo.path(), &head, &[wrong]).is_err());
    let mut wrong = scope();
    wrong.term = "subscription".into();
    assert!(json_field_spans_at(repo.path(), &head, &[wrong]).is_err());

    let (repo, head) =
        fixture("{\"data\":{\"items\":[{\"price\":{\"tri\\u0061l_period\":null}}]}}\n");
    assert!(json_field_spans_at(repo.path(), &head, &[scope()]).is_err());
}

#[test]
fn duplicate_semantic_key_and_malformed_json_fail_closed() {
    let (repo, head) = fixture(
        "{\"data\":{\"items\":[{\"price\":{\"trial_period\":null,\"trial_period\":1}}]}}\n",
    );
    assert!(json_field_spans_at(repo.path(), &head, &[scope()]).is_err());
    let (repo, head) = fixture("{\"data\":{\"items\":[{\"price\":{\"trial_period\":null}}]\n");
    assert!(json_field_spans_at(repo.path(), &head, &[scope()]).is_err());
}

#[test]
fn same_key_at_another_pointer_does_not_expand_the_exclusion() {
    let (repo, head) = fixture("{\"data\":{\"items\":[{\"price\":{\"trial_period\":null}},{\"price\":{\"trial_period\":null}}]}}\n");
    let spans = json_field_spans_at(repo.path(), &head, &[scope()]).unwrap();
    assert_eq!(spans.len(), 1);
    let line = std::fs::read_to_string(repo.path().join("vendor/paddle.json")).unwrap();
    let first = line.find("trial_period").unwrap();
    let second = line.rfind("trial_period").unwrap();
    assert!(spans[0].matches("vendor/paddle.json", 1, first, first + 5, "trial"));
    assert!(!spans[0].matches("vendor/paddle.json", 1, second, second + 5, "trial"));
}
