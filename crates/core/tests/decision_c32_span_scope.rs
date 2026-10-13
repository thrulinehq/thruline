//! C32 exact, source-pinned scopes for human-reviewed support and removal spans.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{path::Path, process::Command};
use thruline_core::decisions::checks::{check_at, CheckOutcome, VerdictState};
use thruline_core::decisions::params::parse_params;

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
fn commit(repo: &Path) -> String {
    git(repo, &["add", "-A"]);
    git(
        repo,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "scope fixture",
        ],
    );
    git(repo, &["rev-parse", "HEAD"])
}
fn fixture(files: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-q"]);
    for (path, text) in files {
        let path = repo.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let head = commit(repo.path());
    (repo, head)
}
fn sha(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn scope(path: &str, text: &str, lo: u64, hi: u64, start: usize, end: usize) -> Value {
    json!({"path":path, "source_blob_sha256":sha(text), "start_line":lo,
        "end_line":hi, "start_column":start, "end_column":end,
        "purpose":"removal_comment", "supports":[]})
}
fn base(spans: Vec<Value>) -> Value {
    json!({"kind":"text_absent", "terms":["blocked"], "synonyms":[],
        "surfaces":["code","tests","ci","docs","other","ui_copy"],
        "allowlist":[], "span_allowlist":spans})
}
fn outcome(repo: &Path, head: &str, raw: Value) -> CheckOutcome {
    let params = parse_params("text_absent", 1, &raw.to_string()).unwrap();
    check_at(repo, head, &params)
}
#[test]
fn one_pinned_span_leaves_same_line_sibling_visible() {
    let text = "blocked blocked\n";
    let (repo, head) = fixture(&[("src/gate.rs", text)]);
    let result = outcome(
        repo.path(),
        &head,
        base(vec![scope("src/gate.rs", text, 1, 1, 0, 7)]),
    );
    assert_eq!(result.state, VerdictState::Broken);
    assert_eq!(result.findings.len(), 1);
    assert_eq!(result.checked_places, 2, "partial line remains searched");
    let receipt = result
        .not_looked_at
        .iter()
        .find(|r| r.source == "accepted_span_scope")
        .unwrap();
    assert_eq!(
        (
            receipt.start_line,
            receipt.end_line,
            receipt.start_column,
            receipt.end_column
        ),
        (Some(1), Some(1), Some(0), Some(7))
    );
}
#[test]
fn pinned_full_line_scope_holds_without_whole_file_waiver() {
    let text = "// blocked removed\nclean\n";
    let (repo, head) = fixture(&[("src/gate.rs", text)]);
    let result = outcome(
        repo.path(),
        &head,
        base(vec![scope("src/gate.rs", text, 1, 1, 0, 18)]),
    );
    assert_eq!(result.state, VerdictState::Holds);
    assert_eq!(
        result.checked_places, 2,
        "one full line and the pathname remain"
    );
    assert_eq!(result.not_looked_at.len(), 1);
}
#[test]
fn multiline_scope_excludes_only_contained_term_coordinates() {
    let text = "blocked blocked\nblocked\nblocked blocked\n";
    let (repo, head) = fixture(&[("src/gate.rs", text)]);
    let result = outcome(
        repo.path(),
        &head,
        base(vec![scope("src/gate.rs", text, 1, 3, 8, 7)]),
    );
    assert_eq!(result.findings.len(), 2);
    assert_eq!(
        result.findings.iter().map(|f| f.line).collect::<Vec<_>>(),
        [1, 3]
    );
    assert_eq!(result.checked_places, 3);
}
#[test]
fn pathname_scope_excludes_name_only_and_keeps_content_visible() {
    let path = "src/blocked.rs";
    let text = "blocked\n";
    let (repo, head) = fixture(&[(path, text)]);
    let result = outcome(
        repo.path(),
        &head,
        base(vec![scope(path, text, 0, 0, 0, path.len())]),
    );
    assert_eq!(result.findings.len(), 1);
    assert_eq!(result.findings[0].line, 1);
    assert_eq!(result.checked_places, 1);
    assert_eq!(result.not_looked_at[0].start_line, Some(0));
}
#[test]
fn content_scope_does_not_waive_pathname_or_another_path() {
    let text = "blocked\n";
    let (repo, head) = fixture(&[("src/blocked.rs", text), ("src/sibling.rs", text)]);
    let result = outcome(
        repo.path(),
        &head,
        base(vec![scope("src/blocked.rs", text, 1, 1, 0, 7)]),
    );
    assert_eq!(result.findings.len(), 2);
    assert!(result.findings.iter().any(|f| f.line == 0));
    assert!(result.findings.iter().any(|f| f.path == "src/sibling.rs"));
}
#[test]
fn any_committed_blob_change_invalidates_pin_and_preserves_broken() {
    let text = "blocked\nclean\n";
    let (repo, first) = fixture(&[("src/gate.rs", text)]);
    let raw = base(vec![scope("src/gate.rs", text, 1, 1, 0, 7)]);
    assert_eq!(
        outcome(repo.path(), &first, raw.clone()).state,
        VerdictState::Holds
    );
    std::fs::write(repo.path().join("src/gate.rs"), "blocked\nchanged\n").unwrap();
    let second = commit(repo.path());
    let result = outcome(repo.path(), &second, raw.clone());
    assert_eq!(result.state, VerdictState::Broken);
    assert_eq!(result.findings.len(), 1);
    assert!(result.reason.unwrap().contains("source blob"));
    assert!(result.not_looked_at.is_empty());
    assert_eq!(outcome(repo.path(), &first, raw).state, VerdictState::Holds);
}
#[test]
fn dirty_worktree_does_not_change_exact_git_scope() {
    let text = "blocked\n";
    let (repo, head) = fixture(&[("src/gate.rs", text)]);
    std::fs::write(repo.path().join("src/gate.rs"), "different\n").unwrap();
    assert_eq!(
        outcome(
            repo.path(),
            &head,
            base(vec![scope("src/gate.rs", text, 1, 1, 0, 7)])
        )
        .state,
        VerdictState::Holds
    );
}
#[test]
fn wrong_pin_and_missing_path_leave_other_broken_witnesses_visible() {
    let text = "blocked\n";
    let (repo, head) = fixture(&[("src/gate.rs", text)]);
    let mut selected = scope("src/gate.rs", text, 1, 1, 0, 7);
    selected["source_blob_sha256"] = json!("0".repeat(64));
    for selected in [selected, scope("src/missing.rs", text, 1, 1, 0, 7)] {
        let result = outcome(repo.path(), &head, base(vec![selected]));
        assert_eq!(result.state, VerdictState::Broken);
        assert_eq!(result.findings.len(), 1);
        assert!(result.reason.is_some());
        assert!(result.not_looked_at.is_empty());
    }
}
#[test]
fn unresolved_scope_without_witness_is_unverifiable() {
    let (repo, head) = fixture(&[("src/gate.rs", "clean\n")]);
    let result = outcome(
        repo.path(),
        &head,
        base(vec![scope("src/missing.rs", "clean\n", 1, 1, 0, 5)]),
    );
    assert_eq!(result.state, VerdictState::Unverifiable);
    assert!(result.reason.is_some());
}
#[test]
fn utf8_boundaries_and_out_of_source_bounds_do_not_exclude() {
    let text = "é blocked\n";
    let (repo, head) = fixture(&[("src/gate.rs", text)]);
    for (lo, hi, start, end) in [(1, 1, 1, 10), (1, 1, 0, 11), (1, 2, 0, 7)] {
        let result = outcome(
            repo.path(),
            &head,
            base(vec![scope("src/gate.rs", text, lo, hi, start, end)]),
        );
        assert_eq!(result.state, VerdictState::Broken);
        assert_eq!(result.findings.len(), 1);
        assert!(result.reason.is_some());
    }
}
#[test]
fn exact_test_support_is_bound_to_waived_ast_function() {
    let text = "const LABEL: &str = \"blocked\";\n#[test]\nfn rejects_label() { assert!(!LABEL.is_empty()); }\n";
    let path = "tests/gate.rs";
    let (repo, head) = fixture(&[(path, text)]);
    let named = json!({"path":path,"name_pattern":"rejects_label"});
    let mut selected = scope(path, text, 1, 1, 0, 30);
    selected["purpose"] = json!("test_support");
    selected["supports"] = json!([named]);
    let mut raw = base(vec![selected]);
    raw["test_allowlist"] = json!([named]);
    let result = outcome(repo.path(), &head, raw);
    assert_eq!(result.state, VerdictState::Holds);
    assert_eq!(
        result.checked_places, 2,
        "support and test line excluded; attribute and path retained"
    );
}
#[test]
fn test_and_explicit_line_exclusions_count_union_once() {
    let text =
        "#[test]\nfn rejects_label() { let label = \"blocked\"; assert!(!label.is_empty()); }\n";
    let path = "tests/gate.rs";
    let (repo, head) = fixture(&[(path, text)]);
    let mut raw = base(vec![scope(
        path,
        text,
        2,
        2,
        0,
        text.lines().nth(1).unwrap().len(),
    )]);
    raw["test_allowlist"] = json!([{"path":path,"name_pattern":"rejects_label"}]);
    let result = outcome(repo.path(), &head, raw);
    assert_eq!(result.state, VerdictState::Holds);
    assert_eq!(result.checked_places, 2);
}
#[test]
fn invalid_test_support_does_not_hide_known_broken() {
    let text = "const LABEL: &str = \"blocked\";\nfn ordinary() {}\n";
    let path = "tests/gate.rs";
    let (repo, head) = fixture(&[(path, text)]);
    let named = json!({"path":path,"name_pattern":"ordinary"});
    let mut selected = scope(path, text, 1, 1, 0, 30);
    selected["purpose"] = json!("test_support");
    selected["supports"] = json!([named]);
    let mut raw = base(vec![selected]);
    raw["test_allowlist"] = json!([named]);
    let result = outcome(repo.path(), &head, raw);
    assert_eq!(result.state, VerdictState::Broken);
    assert_eq!(result.findings.len(), 1);
    assert!(result.reason.is_some());
    assert!(result.not_looked_at.is_empty());
}
#[test]
fn duplicate_named_ast_tests_do_not_grant_support_scope() {
    let text = "const LABEL: &str = \"blocked\";\nmod a { #[test] fn rejects_label() { assert!(!super::LABEL.is_empty()); } }\nmod b { #[test] fn rejects_label() { assert!(!super::LABEL.is_empty()); } }\n";
    let path = "tests/gate.rs";
    let (repo, head) = fixture(&[(path, text)]);
    let named = json!({"path":path,"name_pattern":"rejects_label"});
    let mut selected = scope(path, text, 1, 1, 0, 30);
    selected["purpose"] = json!("test_support");
    selected["supports"] = json!([named]);
    let mut raw = base(vec![selected]);
    raw["test_allowlist"] = json!([named]);
    let result = outcome(repo.path(), &head, raw);
    assert_eq!(result.state, VerdictState::Broken);
    assert!(result.reason.is_some());
    assert!(!result
        .not_looked_at
        .iter()
        .any(|r| r.source == "accepted_span_scope"));
}
#[test]
fn unsafe_overlap_duplicate_and_unbounded_selectors_are_rejected() {
    let valid = scope("src/gate.rs", "blocked\n", 1, 1, 0, 7);
    for path in [
        "../gate.rs",
        "/gate.rs",
        "src/*",
        "src/gate?.rs",
        "src\\gate.rs",
        "src//gate.rs",
    ] {
        let mut selected = valid.clone();
        selected["path"] = json!(path);
        assert!(parse_params("text_absent", 1, &base(vec![selected]).to_string()).is_err());
    }
    for (field, value) in [
        ("source_blob_sha256", json!("A".repeat(64))),
        ("purpose", json!("anything")),
        ("start_line", json!(0)),
        ("end_column", json!(0)),
    ] {
        let mut selected = valid.clone();
        selected[field] = value;
        assert!(parse_params("text_absent", 1, &base(vec![selected]).to_string()).is_err());
    }
    assert!(parse_params(
        "text_absent",
        1,
        &base(vec![valid.clone(), valid.clone()]).to_string()
    )
    .is_err());
    let other = scope("src/gate.rs", "blocked\n", 1, 1, 3, 7);
    assert!(parse_params(
        "text_absent",
        1,
        &base(vec![valid.clone(), other]).to_string()
    )
    .is_err());
    let mut selected = valid.clone();
    selected["purpose"] = json!("test_support");
    assert!(parse_params("text_absent", 1, &base(vec![selected.clone()]).to_string()).is_err());
    selected["supports"] = json!([{"path":"tests/gate.rs","name_pattern":"rejects*"}]);
    assert!(parse_params("text_absent", 1, &base(vec![selected.clone()]).to_string()).is_err());
    selected["supports"] = json!([{"path":"tests/gate.rs","name_pattern":"rejects_label"}]);
    assert!(
        parse_params("text_absent", 1, &base(vec![selected]).to_string()).is_err(),
        "support must already be in the accepted test allowlist"
    );
    assert!(parse_params("text_absent", 1, &base(vec![valid; 257]).to_string()).is_err());
}
#[test]
fn pathname_must_cover_whole_path_and_no_content_coordinates() {
    let text = "blocked\n";
    let path = "src/blocked.rs";
    let (repo, head) = fixture(&[(path, text)]);
    let raw = base(vec![scope(path, text, 0, 0, 1, path.len())]);
    assert!(parse_params("text_absent", 1, &raw.to_string()).is_err());
    let raw = base(vec![scope(path, text, 0, 0, 0, path.len() - 1)]);
    assert!(parse_params("text_absent", 1, &raw.to_string()).is_err());
    let result = outcome(
        repo.path(),
        &head,
        base(vec![scope(path, text, 0, 0, 0, path.len())]),
    );
    assert_eq!(result.findings.len(), 1);
}
#[cfg(unix)]
#[test]
fn symlink_git_mode_never_grants_a_scope() {
    use std::os::unix::fs::symlink;
    let (repo, _) = fixture(&[("src/target.rs", "blocked\n")]);
    symlink("target.rs", repo.path().join("src/blocked.rs")).unwrap();
    let head = commit(repo.path());
    let path = "src/blocked.rs";
    let result = outcome(
        repo.path(),
        &head,
        base(vec![scope(path, "target.rs", 0, 0, 0, path.len())]),
    );
    assert_eq!(result.state, VerdictState::Broken);
    assert!(result.findings.iter().any(|f| f.line == 0));
    assert!(result.reason.is_some());
    assert!(result.not_looked_at.is_empty());
}
#[test]
fn empty_new_field_preserves_existing_normalized_bytes_and_digest() {
    let old = json!({"kind":"text_absent", "terms":["blocked"],"synonyms":[],"surfaces":["code"],"allowlist":[]});
    let before = parse_params("text_absent", 1, &old.to_string())
        .unwrap()
        .json()
        .unwrap();
    let mut updated = old;
    updated["span_allowlist"] = json!([]);
    let after = parse_params("text_absent", 1, &updated.to_string())
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(sha(&before), sha(&after));
    assert!(!after.contains("span_allowlist"));
}

#[test]
fn exact_absence_guard_purpose_does_not_expand_scope() {
    let text = "assert absence of blocked\nblocked is current\n";
    let path = ".github/workflows/check.yml";
    let (repo, head) = fixture(&[(path, text)]);
    let mut selected = scope(path, text, 1, 1, 0, text.lines().next().unwrap().len());
    selected["purpose"] = json!("absence_guard");
    let result = outcome(repo.path(), &head, base(vec![selected.clone()]));
    assert_eq!(result.state, VerdictState::Broken);
    assert_eq!(result.findings.len(), 1);
    assert_eq!(result.findings[0].line, 2);
    selected["supports"] = json!([{"path":"tests/gate.rs","name_pattern":"rejects_label"}]);
    assert!(parse_params("text_absent", 1, &base(vec![selected]).to_string()).is_err());
}

#[test]
fn source_size_bound_retains_known_finding() {
    let text = format!("blocked\n{}", "x".repeat(2 * 1024 * 1024));
    let (repo, head) = fixture(&[("src/gate.rs", &text)]);
    let result = outcome(
        repo.path(),
        &head,
        base(vec![scope("src/gate.rs", &text, 1, 1, 0, 7)]),
    );
    assert_eq!(result.state, VerdictState::Broken);
    assert_eq!(result.findings.len(), 1);
    assert!(result.reason.unwrap().contains("two MiB"));
    assert!(result.not_looked_at.is_empty());
}

#[test]
fn non_utf8_scope_source_keeps_independent_broken_witness() {
    let (repo, _) = fixture(&[("src/gate.rs", "blocked\n")]);
    std::fs::write(repo.path().join("src/bytes.rs"), b"clean\xff\n").unwrap();
    let head = commit(repo.path());
    let result = outcome(
        repo.path(),
        &head,
        base(vec![scope("src/bytes.rs", "clean\n", 1, 1, 0, 5)]),
    );
    assert_eq!(result.state, VerdictState::Broken);
    assert_eq!(result.findings.len(), 1);
    assert!(result.reason.unwrap().contains("UTF-8"));
}

#[test]
fn valid_utf8_byte_columns_leave_sibling_term_visible() {
    let text = "é blocked blocked\r\n";
    let (repo, head) = fixture(&[("src/gate.rs", text)]);
    let result = outcome(
        repo.path(),
        &head,
        base(vec![scope("src/gate.rs", text, 1, 1, 3, 10)]),
    );
    assert_eq!(result.state, VerdictState::Broken);
    assert_eq!(result.findings.len(), 1);
    assert!(result.reason.is_none());
    assert_eq!(result.checked_places, 2);
}
