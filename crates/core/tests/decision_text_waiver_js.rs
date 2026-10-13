//! C31 excludes exact node:test functions, with adjacent source still checked.
use serde_json::json;
use std::process::Command;
use thruline_core::decisions::checks::{check_at, CheckOutcome, VerdictState};
use thruline_core::decisions::params::parse_params;

fn fixture(source: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("guard.test.mjs"), source).unwrap();
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
fn check(dir: &tempfile::TempDir, name: &str) -> CheckOutcome {
    let mut body: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/decisions/text_absent/term-selector.json"
    ))
    .unwrap();
    body["test_allowlist"] = json!([{"path":"guard.test.mjs","name_pattern":name}]);
    check_at(
        dir.path(),
        "HEAD",
        &parse_params("text_absent", 1, &body.to_string()).unwrap(),
    )
}

#[test]
fn default_node_test_scope_excludes_only_its_exact_declaration_and_callback() {
    let dir=fixture("import test from 'node:test';\nfunction product(){return 'trial'}\ntest('no trial offer', async () => { const rejected='trial'; }); function neighbor(){return 'trial'}\n");
    let result = check(&dir, "no trial offer");
    assert_eq!(result.state, VerdictState::Broken);
    assert_eq!(
        result.findings.iter().map(|f| f.line).collect::<Vec<_>>(),
        vec![2, 3]
    );
    assert_eq!(result.not_looked_at.len(), 1);
    assert_eq!(
        result.not_looked_at[0].test_name.as_deref(),
        Some("no trial offer")
    );
}

#[test]
fn an_imported_named_test_alias_is_a_function_scope() {
    let dir=fixture("import {test as guard} from 'node:test';\nguard('stale field is refused', () => { const legacy='trial'; });\n");
    assert_eq!(
        check(&dir, "stale field is refused").state,
        VerdictState::Holds
    );
}

#[test]
fn a_local_or_shadowed_test_label_does_not_grant_a_waiver() {
    for source in [
        "function test(name, cb){cb()} test('guard',()=>{const live='trial'});",
        "import test from 'node:test'; function f(test){test('guard',()=>{const live='trial'})}",
    ] {
        assert_eq!(
            check(&fixture(source), "guard").state,
            VerdictState::Unverifiable
        );
    }
}

#[test]
fn skipped_dynamic_or_duplicate_labels_cannot_hide_source() {
    for source in ["import test from 'node:test'; test.skip('guard',()=>{const live='trial'});", "import test from 'node:test'; const label='guard'; test(label,()=>{const live='trial'});", "import test from 'node:test'; test('guard',()=>{const a='trial'}); test('guard',()=>{const b='trial'});"] {
        assert_eq!(check(&fixture(source),"guard").state,VerdictState::Unverifiable);
    }
}

#[test]
fn callback_scope_reads_exact_git_and_refuses_malformed_source() {
    let dir = fixture("import test from 'node:test'; test('guard',()=>{const legacy='trial'});\n");
    let initial = check(&dir, "guard");
    assert_eq!(initial.state, VerdictState::Holds);
    std::fs::write(
        dir.path().join("guard.test.mjs"),
        "function product(){return 'trial'}",
    )
    .unwrap();
    assert_eq!(check(&dir, "guard"), initial);
    assert_eq!(
        check(
            &fixture("import test from 'node:test'; test('guard',()=>{ const legacy='trial';"),
            "guard"
        )
        .state,
        VerdictState::Unverifiable
    );
}

#[test]
fn competing_import_or_reassigned_test_binding_is_not_a_trusted_scope() {
    for source in [
        "import test from 'node:test'; import test from './product.js'; test('guard',()=>{const live='trial'});",
        "import test from 'node:test'; test=product; test('guard',()=>{const live='trial'});",
    ] {
        assert_eq!(check(&fixture(source), "guard").state, VerdictState::Unverifiable);
    }
}
