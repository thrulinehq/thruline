//! K6 proves a bounded source association; intent and verdicts stay unproved.

use std::fs;
use std::path::Path;
use std::process::Command;

use thruline_core::decisions::checks_k_candidates::{
    scan_k_candidate, KCandidateOutcome, KCandidateState,
};

const OLD: &str = "def answer(value):\n    return value\n";
const TABLE: &str =
    "def answer(value):\n    return {'fixture-a': 42, 'fixture-b': 7}.get(value, 0)\n";
const TESTS: &str = "from src.answer import answer\n\ndef test_answers():\n    assert answer('fixture-a') == 42\n    assert answer('fixture-b') == 7\n";

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .expect("git");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("utf8")
        .trim()
        .to_owned()
}

struct Fixture {
    temp: tempfile::TempDir,
    baseline: String,
}

impl Fixture {
    fn new(production: &str, tests: Option<&str>) -> Self {
        let temp = tempfile::tempdir().expect("scratch");
        git(temp.path(), &["init", "-q"]);
        git(temp.path(), &["config", "user.name", "K6 Fixture"]);
        git(temp.path(), &["config", "user.email", "k6@example.invalid"]);
        write(temp.path(), "src/answer.py", production);
        if let Some(tests) = tests {
            write(temp.path(), "tests/test_answer.py", tests);
        }
        git(temp.path(), &["add", "."]);
        git(temp.path(), &["commit", "-qm", "baseline"]);
        let baseline = git(temp.path(), &["rev-parse", "HEAD"]);
        Self { temp, baseline }
    }

    fn commit(&self) -> String {
        git(self.temp.path(), &["add", "-A"]);
        git(
            self.temp.path(),
            &["commit", "--allow-empty", "-qm", "target"],
        );
        git(self.temp.path(), &["rev-parse", "HEAD"])
    }

    fn table(&self) -> String {
        write(self.temp.path(), "src/answer.py", TABLE);
        self.commit()
    }

    fn scan(&self, target: &str) -> KCandidateOutcome {
        scan_k_candidate(self.temp.path(), &self.baseline, target, "K6")
    }
}

fn write(repo: &Path, relative: &str, source: &str) {
    let path = repo.join(relative);
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(path, source).expect("source");
}

fn assert_state(outcome: KCandidateOutcome, expected: KCandidateState) {
    assert_eq!(outcome.state, expected, "{outcome:?}");
    assert!(
        outcome.reason.is_some(),
        "the scoped proof or gap must be explicit"
    );
    if expected == KCandidateState::Signal {
        assert!(
            outcome
                .observations
                .iter()
                .any(|row| row.path == "src/answer.py" && row.line == 2),
            "{outcome:?}"
        );
        assert!(
            outcome
                .observations
                .iter()
                .any(|row| row.path == "tests/test_answer.py" && row.line > 0),
            "{outcome:?}"
        );
    }
}

#[test]
fn unchanged_tracked_literal_assertions_link_a_new_production_table() {
    let fixture = Fixture::new(OLD, Some(TESTS));
    assert_state(fixture.scan(&fixture.table()), KCandidateState::Signal);
}

#[test]
fn one_stable_literal_input_output_pair_is_an_advisory_source_association() {
    let fixture = Fixture::new(OLD, Some("from src.answer import answer\n\ndef test_answer():\n    assert answer('fixture-a') == 42\n"));
    assert_state(fixture.scan(&fixture.table()), KCandidateState::Signal);
}

#[test]
fn matching_json_input_expected_without_a_test_call_is_unverifiable() {
    let fixture = Fixture::new(OLD, None);
    write(
        fixture.temp.path(),
        "tests/fixtures/answers.json",
        "{\"input\":\"fixture-a\",\"expected\":42}\n",
    );
    assert_state(
        fixture.scan(&fixture.table()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn imported_external_function_does_not_link_to_a_local_table() {
    let fixture = Fixture::new(OLD, Some("from external.answer import answer\n\ndef test_answer():\n    assert answer('fixture-a') == 42\n"));
    assert_state(
        fixture.scan(&fixture.table()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn multiple_imports_of_the_called_name_are_ambiguous() {
    let fixture = Fixture::new(OLD, Some("from src.answer import answer\nfrom src.other import answer\n\ndef test_answer():\n    assert answer('fixture-a') == 42\n"));
    write(fixture.temp.path(), "src/other.py", OLD);
    assert_state(
        fixture.scan(&fixture.table()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn aliased_imports_are_outside_the_bounded_link_language() {
    let fixture = Fixture::new(OLD, Some("from src.answer import answer as result\n\ndef test_answer():\n    assert result('fixture-a') == 42\n"));
    assert_state(
        fixture.scan(&fixture.table()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn changed_expected_output_is_not_a_stable_fixture_pair() {
    let fixture = Fixture::new(OLD, Some("from src.answer import answer\n\ndef test_answer():\n    assert answer('fixture-a') == 41\n"));
    write(fixture.temp.path(), "tests/test_answer.py", "from src.answer import answer\n\ndef test_answer():\n    assert answer('fixture-a') == 42\n");
    assert_state(
        fixture.scan(&fixture.table()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn table_output_must_equal_the_literal_expected_output() {
    let fixture = Fixture::new(OLD, Some("from src.answer import answer\n\ndef test_answer():\n    assert answer('fixture-a') == 43\n"));
    assert_state(
        fixture.scan(&fixture.table()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn a_baseline_existing_table_is_not_a_new_table_signal() {
    let fixture = Fixture::new(TABLE, Some(TESTS));
    write(
        fixture.temp.path(),
        "src/answer.py",
        &format!("# Documentation only.\n{TABLE}"),
    );
    assert_state(fixture.scan(&fixture.commit()), KCandidateState::NoSignal);
}

#[test]
fn moving_an_exact_table_between_production_files_is_netted() {
    let fixture = Fixture::new(TABLE, Some(TESTS));
    fs::remove_file(fixture.temp.path().join("src/answer.py")).expect("remove old path");
    write(fixture.temp.path(), "src/moved.py", TABLE);
    write(
        fixture.temp.path(),
        "tests/test_answer.py",
        &TESTS.replace("src.answer", "src.moved"),
    );
    assert_state(fixture.scan(&fixture.commit()), KCandidateState::NoSignal);
}

#[test]
fn comments_and_string_examples_cannot_create_a_table_signal() {
    let fixture = Fixture::new(OLD, Some(TESTS));
    write(fixture.temp.path(), "src/answer.py", "# return {'fixture-a': 42}.get(value, 0)\nEXAMPLE = \"{'fixture-a': 42}.get(value, 0)\"\n\ndef answer(value):\n    return value\n");
    assert_state(fixture.scan(&fixture.commit()), KCandidateState::NoSignal);
}

#[test]
fn dynamic_table_outputs_abstain() {
    let fixture = Fixture::new(OLD, Some(TESTS));
    write(
        fixture.temp.path(),
        "src/answer.py",
        "def answer(value):\n    return {'fixture-a': compute()}.get(value, 0)\n",
    );
    assert_state(
        fixture.scan(&fixture.commit()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn dynamic_test_arguments_abstain() {
    let fixture = Fixture::new(OLD, Some("from src.answer import answer\n\ndef test_answer():\n    assert answer(get_input()) == 42\n"));
    assert_state(
        fixture.scan(&fixture.table()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn unsupported_source_dialect_is_not_a_negative_census() {
    let fixture = Fixture::new(OLD, Some(TESTS));
    write(
        fixture.temp.path(),
        "src/answer.js",
        "export function answer(value) { return {'fixture-a': 42}[value]; }\n",
    );
    assert_state(
        fixture.scan(&fixture.commit()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn malformed_production_syntax_abstains() {
    let fixture = Fixture::new(OLD, Some(TESTS));
    write(
        fixture.temp.path(),
        "src/answer.py",
        "def answer(value):\n    return {'fixture-a': 42.get(value, 0)\n",
    );
    assert_state(
        fixture.scan(&fixture.commit()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn malformed_unchanged_test_syntax_abstains() {
    let fixture = Fixture::new(OLD, Some("from src.answer import answer\n\ndef test_answer():\n    assert answer('fixture-a' == 42\n"));
    assert_state(
        fixture.scan(&fixture.table()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn abbreviated_and_missing_commit_identities_abstain() {
    let fixture = Fixture::new(OLD, Some(TESTS));
    let target = fixture.table();
    assert_state(fixture.scan(&target[..8]), KCandidateState::Unverifiable);
    assert_state(fixture.scan(&"a".repeat(40)), KCandidateState::Unverifiable);
}

#[test]
fn mutable_working_tree_changes_do_not_affect_the_exact_tree_signal() {
    let fixture = Fixture::new(OLD, Some(TESTS));
    let target = fixture.table();
    write(fixture.temp.path(), "src/answer.py", OLD);
    write(
        fixture.temp.path(),
        "tests/test_answer.py",
        "this is not valid Python [\n",
    );
    assert_state(fixture.scan(&target), KCandidateState::Signal);
}

#[cfg(unix)]
#[test]
fn regular_executable_source_blobs_remain_supported() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new(OLD, Some(TESTS));
    write(fixture.temp.path(), "src/answer.py", TABLE);
    fs::set_permissions(
        fixture.temp.path().join("src/answer.py"),
        fs::Permissions::from_mode(0o755),
    )
    .expect("executable");
    assert_state(fixture.scan(&fixture.commit()), KCandidateState::Signal);
}

#[cfg(unix)]
#[test]
fn a_symlink_source_cannot_be_treated_as_python_blob_content() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new(OLD, Some(TESTS));
    fs::remove_file(fixture.temp.path().join("src/answer.py")).expect("remove");
    symlink("elsewhere.py", fixture.temp.path().join("src/answer.py")).expect("symlink");
    assert_state(
        fixture.scan(&fixture.commit()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn over_budget_unchanged_python_tests_abstain() {
    let tests = format!("#{}\n{TESTS}", "x".repeat(2 * 1024 * 1024));
    let fixture = Fixture::new(OLD, Some(&tests));
    assert_state(
        fixture.scan(&fixture.table()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn test_function_parameter_shadowing_the_import_abstains() {
    let fixture = Fixture::new(OLD, Some("from src.answer import answer\n\ndef test_answer(answer):\n    assert answer('fixture-a') == 42\n"));
    assert_state(
        fixture.scan(&fixture.table()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn test_function_local_rebinding_of_the_import_abstains() {
    let fixture = Fixture::new(OLD, Some("from src.answer import answer\n\ndef test_answer():\n    answer = lambda value: 42\n    assert answer('fixture-a') == 42\n"));
    assert_state(
        fixture.scan(&fixture.table()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn test_module_top_level_rebinding_of_the_import_abstains() {
    let fixture = Fixture::new(OLD, Some("from src.answer import answer\nanswer = lambda value: 42\n\ndef test_answer():\n    assert answer('fixture-a') == 42\n"));
    assert_state(
        fixture.scan(&fixture.table()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn duplicate_production_function_definitions_abstain() {
    let fixture = Fixture::new(OLD, Some(TESTS));
    write(
        fixture.temp.path(),
        "src/answer.py",
        &format!("{TABLE}\n{OLD}"),
    );
    assert_state(
        fixture.scan(&fixture.commit()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn duplicate_dictionary_keys_with_different_values_abstain() {
    let fixture = Fixture::new(OLD, Some(TESTS));
    write(
        fixture.temp.path(),
        "src/answer.py",
        "def answer(value):\n    return {'fixture-a': 42, 'fixture-a': 43}.get(value, 0)\n",
    );
    assert_state(
        fixture.scan(&fixture.commit()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn duplicate_blob_paths_still_consume_the_logical_tree_byte_budget() {
    let mut fixture = Fixture::new(OLD, Some(TESTS));
    let shared_source = format!("#{}\n", "x".repeat(1024 * 1024));
    for index in 0..17 {
        write(
            fixture.temp.path(),
            &format!("src/shared_{index}.py"),
            &shared_source,
        );
    }
    fixture.baseline = fixture.commit();
    assert_state(
        fixture.scan(&fixture.table()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn walrus_binding_in_an_earlier_test_assertion_abstains_for_the_whole_function() {
    let fixture = Fixture::new(OLD, Some("from src.answer import answer\n\ndef test_answer():\n    assert (answer := lambda value: 42)\n    assert answer('fixture-a') == 42\n"));
    assert_state(
        fixture.scan(&fixture.table()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn unknown_helper_assertion_before_a_literal_call_abstains_for_the_whole_function() {
    let fixture = Fixture::new(OLD, Some("from src.answer import answer\n\ndef test_answer():\n    assert reset_import()\n    assert answer('fixture-a') == 42\n"));
    assert_state(
        fixture.scan(&fixture.table()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn ancestor_package_initializers_cannot_redirect_a_claimed_namespace_link() {
    let fixture = Fixture::new(OLD, Some(TESTS));
    write(fixture.temp.path(), "src/__init__.py", "import sys\n# A package initializer may replace import bindings.\nsys.modules['src.answer'] = alternate\n");
    assert_state(
        fixture.scan(&fixture.table()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn even_passive_ancestor_initializers_are_outside_namespace_only_imports() {
    let fixture = Fixture::new(OLD, Some(TESTS));
    write(
        fixture.temp.path(),
        "src/__init__.py",
        "# Passive package initializer.\n",
    );
    assert_state(
        fixture.scan(&fixture.table()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn production_helper_defaults_can_execute_before_the_assertion_and_abstain() {
    let fixture = Fixture::new(OLD, Some(TESTS));
    write(
        fixture.temp.path(),
        "src/answer.py",
        &format!("{TABLE}\n\ndef helper(value=rebind()):\n    return value\n"),
    );
    assert_state(
        fixture.scan(&fixture.commit()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn production_helper_parameter_annotations_are_outside_the_binding_language() {
    let fixture = Fixture::new(OLD, Some(TESTS));
    write(
        fixture.temp.path(),
        "src/answer.py",
        &format!("{TABLE}\n\ndef helper(value: rebind()):\n    return value\n"),
    );
    assert_state(
        fixture.scan(&fixture.commit()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn production_helper_return_annotations_are_outside_the_binding_language() {
    let fixture = Fixture::new(OLD, Some(TESTS));
    write(
        fixture.temp.path(),
        "src/answer.py",
        &format!("{TABLE}\n\ndef helper(value) -> rebind():\n    return value\n"),
    );
    assert_state(
        fixture.scan(&fixture.commit()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn unrelated_test_helper_defaults_invalidate_the_import_binding_proof() {
    let fixture = Fixture::new(
        OLD,
        Some(&format!(
            "{TESTS}\n\ndef helper(value=rebind()):\n    return value\n"
        )),
    );
    assert_state(
        fixture.scan(&fixture.table()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn an_interpolated_module_string_is_not_a_passive_docstring() {
    let fixture = Fixture::new(OLD, Some(&format!("f\"{{rebind()}}\"\n{TESTS}")));
    assert_state(
        fixture.scan(&fixture.table()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn an_interpolated_production_constant_can_rebind_names_and_abstains() {
    let fixture = Fixture::new(OLD, Some(TESTS));
    write(
        fixture.temp.path(),
        "src/answer.py",
        &format!("EXAMPLE = f\"{{rebind()}}\"\n{TABLE}"),
    );
    assert_state(
        fixture.scan(&fixture.commit()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn removing_an_unused_legacy_table_does_not_hide_a_new_table_in_an_existing_linked_file() {
    let mut fixture = Fixture::new(OLD, Some(TESTS));
    write(fixture.temp.path(), "src/legacy.py", TABLE);
    fixture.baseline = fixture.commit();
    fs::remove_file(fixture.temp.path().join("src/legacy.py")).expect("remove legacy table");
    assert_state(fixture.scan(&fixture.table()), KCandidateState::Signal);
}

#[test]
fn a_changed_whole_file_cannot_claim_an_exact_unique_table_move() {
    let fixture = Fixture::new(TABLE, Some(TESTS));
    fs::remove_file(fixture.temp.path().join("src/answer.py")).expect("remove old path");
    write(
        fixture.temp.path(),
        "src/moved.py",
        &format!("# Changed whole-file bytes.\n{TABLE}"),
    );
    write(
        fixture.temp.path(),
        "tests/test_answer.py",
        &TESTS.replace("src.answer", "src.moved"),
    );
    assert_state(
        fixture.scan(&fixture.commit()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn unrelated_production_imports_are_outside_the_passive_binding_subset() {
    let fixture = Fixture::new(OLD, Some(TESTS));
    write(
        fixture.temp.path(),
        "src/answer.py",
        &format!("import rebind\n{TABLE}"),
    );
    assert_state(
        fixture.scan(&fixture.commit()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn unrelated_test_imports_are_outside_the_single_namespace_link_subset() {
    let fixture = Fixture::new(OLD, Some(&format!("from rebind import helper\n{TESTS}")));
    assert_state(
        fixture.scan(&fixture.table()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn a_baseline_helper_default_cannot_establish_a_unique_stable_source_link() {
    let fixture = Fixture::new(
        &format!("{OLD}\n\ndef helper(value=rebind()):\n    return value\n"),
        Some(TESTS),
    );
    assert_state(
        fixture.scan(&fixture.table()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn a_duplicate_baseline_production_function_cannot_establish_a_unique_stable_source_link() {
    let fixture = Fixture::new(&format!("{OLD}\n{OLD}"), Some(TESTS));
    assert_state(
        fixture.scan(&fixture.table()),
        KCandidateState::Unverifiable,
    );
}

#[test]
fn an_interpolated_lookup_fallback_is_outside_the_literal_function_subset() {
    let fixture = Fixture::new(OLD, Some(TESTS));
    write(
        fixture.temp.path(),
        "src/answer.py",
        "def answer(value):\n    return {'fixture-a': 42}.get(value, f\"{rebind()}\")\n",
    );
    assert_state(
        fixture.scan(&fixture.commit()),
        KCandidateState::Unverifiable,
    );
}
