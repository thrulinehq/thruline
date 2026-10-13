//! Versioned exact AST declarations augment, never loosen, legacy test scopes.
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
            "declarations",
        ],
    );
    git(repo, &["rev-parse", "HEAD"])
}
fn fixture(path: &str, source: &str) -> (tempfile::TempDir, String) {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-q"]);
    let file = repo.path().join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, source).unwrap();
    let head = commit(repo.path());
    (repo, head)
}
fn digest(source: &str) -> String {
    Sha256::digest(source.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn coordinates(source: &str, byte: usize) -> (u64, usize) {
    let prefix = &source[..byte];
    (
        prefix.bytes().filter(|b| *b == b'\n').count() as u64 + 1,
        prefix.rsplit('\n').next().unwrap().len(),
    )
}
fn select(path: &str, source: &str, fragment: &str, purpose: &str) -> Value {
    let start = source.find(fragment).unwrap();
    let (start_line, start_column) = coordinates(source, start);
    let (end_line, end_column) = coordinates(source, start + fragment.len());
    json!({"path":path,"source_blob_sha256":digest(source),"start_line":start_line,
        "start_column":start_column,"end_line":end_line,"end_column":end_column,
        "purpose":purpose,"supports":[]})
}
fn reference(scope: &Value) -> Value {
    let mut reference = scope.clone();
    reference.as_object_mut().unwrap().remove("purpose");
    reference.as_object_mut().unwrap().remove("supports");
    reference
}
fn params(spans: Vec<Value>) -> Value {
    json!({"kind":"text_absent","terms":["blocked"],"synonyms":[],
        "surfaces":["code","tests","ci","docs","other","ui_copy"],
        "allowlist":[],"span_allowlist":spans})
}
fn outcome(repo: &Path, head: &str, raw: Value) -> CheckOutcome {
    let parsed = parse_params("text_absent", 1, &raw.to_string())
        .expect("versioned scope must be supported");
    check_at(repo, head, &parsed)
}
fn refused_or_broken(repo: &Path, head: &str, raw: Value) {
    if let Ok(parsed) = parse_params("text_absent", 1, &raw.to_string()) {
        let result = check_at(repo, head, &parsed);
        assert_eq!(result.state, VerdictState::Broken);
        assert!(
            !result.findings.is_empty(),
            "known finding must remain visible"
        );
        assert!(result
            .not_looked_at
            .iter()
            .all(|r| r.source != "accepted_test_declaration_v1"));
    }
}

#[test]
fn exact_literal_callback_keeps_same_line_sibling_checked() {
    let call = "test('refuses', () => { assert.doesNotMatch('blocked', /x/) })";
    let source = format!("import test from 'node:test'\n{call}; const sibling = 'blocked';\n");
    let path = "tests/guard.js";
    let (repo, head) = fixture(path, &source);
    let result = outcome(
        repo.path(),
        &head,
        params(vec![select(path, &source, call, "test_declaration_v1")]),
    );
    assert_eq!(result.state, VerdictState::Broken);
    assert_eq!(result.findings.len(), 1);
    assert!(result
        .not_looked_at
        .iter()
        .any(|r| r.source == "accepted_test_declaration_v1"));
}

#[test]
fn computed_title_callback_in_direct_for_of_body_is_exactly_selected() {
    let call = "test('rejects ' + what, () => { assert.doesNotMatch('blocked', /x/) })";
    let source = format!("import test from 'node:test'\nconst CASES = ['one', 'two']\nfor (const what of CASES) {{\n  {call}\n}}\n");
    let path = "tests/guard.js";
    let (repo, head) = fixture(path, &source);
    let result = outcome(
        repo.path(),
        &head,
        params(vec![select(path, &source, call, "test_declaration_v1")]),
    );
    assert_eq!(result.state, VerdictState::Holds);
    assert_eq!(result.not_looked_at.len(), 1);
    assert_eq!(result.not_looked_at[0].start_line, Some(4));
    assert_eq!(result.not_looked_at[0].end_line, Some(4));
}

#[test]
fn exact_declaration_allows_129_total_functions_without_changing_legacy_limit() {
    let mut source = String::new();
    for index in 0..129 {
        source.push_str(&format!(
            "#[test]\nfn guard_{index}() {{ let s = \"blocked\"; assert!(!s.is_empty()); }}\n"
        ));
    }
    let fragment = "fn guard_128() { let s = \"blocked\"; assert!(!s.is_empty()); }";
    let path = "tests/guard.rs";
    let (repo, head) = fixture(path, &source);
    let mut raw = params(vec![select(path, &source, fragment, "test_declaration_v1")]);
    raw["test_allowlist"] = json!((0..128)
        .map(|i| json!({"path":path,"name_pattern":format!("guard_{i}")}))
        .collect::<Vec<_>>());
    let result = outcome(repo.path(), &head, raw.clone());
    assert_eq!(result.state, VerdictState::Holds);
    assert_eq!(result.not_looked_at.len(), 129);
    raw["test_allowlist"]
        .as_array_mut()
        .unwrap()
        .push(json!({"path":path,"name_pattern":"guard_128"}));
    assert!(
        parse_params("text_absent", 1, &raw.to_string()).is_err(),
        "legacy128 cap remains"
    );
}

#[test]
fn helper_references_only_an_accepted_and_resolved_exact_declaration() {
    let call = "test('rejects ' + what, () => { assert.doesNotMatch('ok', LABEL) })";
    let support = "const LABEL = /blocked/";
    let source = format!("import test from 'node:test'\n{support}\nconst CASES = ['one']\nfor (const what of CASES) {{\n  {call}\n}}\nconst sibling = 'blocked'\n");
    let path = "tests/guard.js";
    let (repo, head) = fixture(path, &source);
    let declaration = select(path, &source, call, "test_declaration_v1");
    let mut helper = select(path, &source, support, "test_support");
    helper["declaration_supports"] = json!([reference(&declaration)]);
    let result = outcome(repo.path(), &head, params(vec![helper, declaration]));
    assert_eq!(result.state, VerdictState::Broken);
    assert_eq!(result.findings.len(), 1);
    assert_eq!(
        result.not_looked_at.len(),
        2,
        "support resolved independent of selector order"
    );
}

#[test]
fn missing_declaration_reference_cannot_be_accepted_as_test_support() {
    let source = "const LABEL = /blocked/\n";
    let path = "tests/guard.js";
    let mut helper = select(path, source, "const LABEL = /blocked/", "test_support");
    helper["declaration_supports"] = json!([{"path":path,"source_blob_sha256":digest(source),
        "start_line":2,"start_column":0,"end_line":3,"end_column":1}]);
    assert!(parse_params("text_absent", 1, &params(vec![helper]).to_string()).is_err());
}

#[test]
fn non_test_function_and_ordinary_call_cannot_grant_a_declaration_waiver() {
    for (path, source, fragment) in [
        (
            "tests/guard.rs",
            "fn ordinary() { let s = \"blocked\"; }\n",
            "fn ordinary() { let s = \"blocked\"; }",
        ),
        (
            "tests/guard.js",
            "function ordinary() { return 'blocked' }\n",
            "function ordinary() { return 'blocked' }",
        ),
        (
            "tests/guard.js",
            "test('x', () => 'blocked')\n",
            "test('x', () => 'blocked')",
        ),
    ] {
        let (repo, head) = fixture(path, source);
        refused_or_broken(
            repo.path(),
            &head,
            params(vec![select(path, source, fragment, "test_declaration_v1")]),
        );
    }
}

#[test]
fn callback_body_instead_of_complete_test_declaration_cannot_exclude() {
    let source = "import test from 'node:test'\ntest('x', () => { return 'blocked' })\n";
    let path = "tests/guard.js";
    let (repo, head) = fixture(path, source);
    refused_or_broken(
        repo.path(),
        &head,
        params(vec![select(
            path,
            source,
            "() => { return 'blocked' }",
            "test_declaration_v1",
        )]),
    );
}

#[test]
fn shadowed_import_and_nested_non_loop_callback_cannot_exclude() {
    for source in [
        "import test from 'node:test'\nfunction run(test) { test('x', () => 'blocked') }\n",
        "import test from 'node:test'\nfunction run() { test('x', () => 'blocked') }\n",
        "import test from 'untrusted:test'\ntest('x', () => 'blocked')\n",
    ] {
        let path = "tests/guard.js";
        let (repo, head) = fixture(path, source);
        refused_or_broken(
            repo.path(),
            &head,
            params(vec![select(
                path,
                source,
                "test('x', () => 'blocked')",
                "test_declaration_v1",
            )]),
        );
    }
}

#[test]
fn dynamic_title_call_and_dynamic_loop_inventory_cannot_exclude() {
    for source in [
        "import test from 'node:test'\ntest(makeTitle(), () => 'blocked')\n",
        "import test from 'node:test'\nfor (const what of getCases()) { test('x' + what, () => 'blocked') }\n",
    ] {
        let call = if source.contains("makeTitle") { "test(makeTitle(), () => 'blocked')" } else { "test('x' + what, () => 'blocked')" };
        let path = "tests/guard.js";
        let (repo, head) = fixture(path, source);
        refused_or_broken(repo.path(), &head, params(vec![select(path, source, call, "test_declaration_v1")]));
    }
}

#[test]
fn whole_blob_change_invalidates_exact_declaration_and_its_helper() {
    let call = "test('x', () => { assert.doesNotMatch('ok', LABEL) })";
    let support = "const LABEL = /blocked/";
    let source = format!("import test from 'node:test'\n{support}\n{call}\n");
    let path = "tests/guard.js";
    let (repo, head) = fixture(path, &source);
    let declaration = select(path, &source, call, "test_declaration_v1");
    let mut helper = select(path, &source, support, "test_support");
    helper["declaration_supports"] = json!([reference(&declaration)]);
    let raw = params(vec![helper, declaration]);
    assert_eq!(
        outcome(repo.path(), &head, raw.clone()).state,
        VerdictState::Holds
    );
    std::fs::write(repo.path().join(path), format!("{source}// changed\n")).unwrap();
    let next = commit(repo.path());
    let result = outcome(repo.path(), &next, raw);
    assert_eq!(result.state, VerdictState::Broken);
    assert!(result.not_looked_at.is_empty());
}

#[test]
fn duplicate_declarations_and_more_than_256_spans_remain_rejected() {
    let source = "#[test]\nfn guard() { let s = \"blocked\"; }\n";
    let scope = select(
        "tests/guard.rs",
        source,
        "fn guard() { let s = \"blocked\"; }",
        "test_declaration_v1",
    );
    assert!(parse_params(
        "text_absent",
        1,
        &params(vec![scope.clone(), scope.clone()]).to_string()
    )
    .is_err());
    assert!(parse_params("text_absent", 1, &params(vec![scope; 257]).to_string()).is_err());
}

#[test]
fn empty_new_support_list_preserves_legacy_parameter_bytes() {
    let source = "// blocked removed\n";
    let legacy = params(vec![select(
        "src/gate.rs",
        source,
        "// blocked removed",
        "removal_comment",
    )]);
    let parsed = parse_params("text_absent", 1, &legacy.to_string()).unwrap();
    let serialized = serde_json::to_value(parsed).unwrap();
    assert!(serialized["span_allowlist"][0]
        .get("declaration_supports")
        .is_none());
}
