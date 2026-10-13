//! BUILD-SPEC §6.7, one fixture per detector per language.
//!
//! Every case is a **pair**: the tampering, and the honest edit it must be
//! distinguished from. A detector that fires on both is worse than none — it
//! trains a reviewer to ignore it, which is how a real finding gets missed.

use thruline_verify::tamper::{diff_test_file, parse_tests, Language, Strength};
use thruline_verify::verdict::Severity;

fn detectors(path: &str, base: &str, head: &str, lang: Language) -> Vec<(String, Severity)> {
    diff_test_file(path, base, head, lang)
        .unwrap_or_else(|e| panic!("{path}: {e}"))
        .into_iter()
        .map(|f| (f.detector, f.severity))
        .collect()
}

fn has(found: &[(String, Severity)], detector: &str) -> bool {
    found.iter().any(|(d, _)| d == detector)
}

// ---------------------------------------------------------------------------
// Python — the language M1's done-when runs in
// ---------------------------------------------------------------------------

const PY_BASE: &str = "\
def test_rate_small():
    assert rate(2) == 9.0

def test_rate_large():
    assert rate(10) == 29.0
";

#[test]
fn python_equality_to_containment_is_a_weakened_assertion() {
    let head = "\
def test_rate_small():
    assert \"12\" in str(rate(2))

def test_rate_large():
    assert rate(10) == 29.0
";
    let found = detectors("test_shipping.py", PY_BASE, head, Language::Python);
    assert!(has(&found, "assertion_weakened"), "{found:?}");
}

#[test]
fn python_an_assertion_deleted_outright_is_a_removal_and_a_fail() {
    let head = "\
def test_rate_small():
    assert rate(2) == 9.0

def test_rate_large():
    rate(10)
";
    let found = detectors("test_shipping.py", PY_BASE, head, Language::Python);
    assert!(has(&found, "assertion_removed"), "{found:?}");
    assert!(found
        .iter()
        .any(|(d, s)| d == "assertion_removed" && *s == Severity::Fail));
}

#[test]
fn python_a_skip_marker_added_is_reported_as_a_candidate_not_a_conclusion() {
    let head = "\
import pytest

@pytest.mark.skip(reason=\"flaky\")
def test_rate_small():
    assert rate(2) == 9.0

def test_rate_large():
    assert rate(10) == 29.0
";
    let found = detectors("test_shipping.py", PY_BASE, head, Language::Python);
    assert!(has(&found, "skip_marker_added"), "{found:?}");
    // The DETECTOR proposes; only the run concludes. Firing `fail` here
    // accused every deliberate `@pytest.mark.skipif(sys.platform == "win32")`
    // of cheating — four of five honest edit shapes came out `unsupported`.
    assert!(
        found
            .iter()
            .any(|(d, sev)| d == "skip_marker_added" && *sev == Severity::Warn),
        "the detector concluded rather than proposed: {found:?}"
    );
}

#[test]
fn python_a_deleted_test_is_a_fail() {
    let head = "def test_rate_small():\n    assert rate(2) == 9.0\n";
    let found = detectors("test_shipping.py", PY_BASE, head, Language::Python);
    assert!(has(&found, "test_deleted"), "{found:?}");
    assert!(found
        .iter()
        .any(|(d, s)| d == "test_deleted" && *s == Severity::Fail));
}

#[test]
fn a_rename_that_keeps_the_body_is_a_rename_and_not_a_deletion() {
    // §6.7's text is "test renamed **to escape a pattern**", and the qualifier
    // is the whole rule. Failing every rename accused routine refactoring:
    // a refactor renaming two tests produced `test_deleted` twice and
    // `test_added` twice, and the claim came out `unsupported`.
    let head = "\
def test_shipping_rate_small():
    assert rate(2) == 9.0

def test_shipping_rate_large():
    assert rate(10) == 29.0
";
    let found = detectors("test_shipping.py", PY_BASE, head, Language::Python);
    assert!(has(&found, "test_renamed"), "{found:?}");
    assert!(
        !has(&found, "test_deleted"),
        "a rename was called a deletion: {found:?}"
    );
    assert!(
        found.iter().all(|(_, s)| *s != Severity::Fail),
        "a rename produced a fail: {found:?}"
    );
}

#[test]
fn a_rename_whose_body_is_gone_is_still_a_deletion() {
    // The control. If any rename were excused, deleting a test and adding an
    // unrelated one under a similar name would launder the deletion.
    let head = "\
def test_rate_small():
    assert rate(2) == 9.0

def test_something_else_entirely():
    assert config.load().timeout == 30
";
    let found = detectors("test_shipping.py", PY_BASE, head, Language::Python);
    assert!(has(&found, "test_deleted"), "{found:?}");
}

#[test]
fn python_an_honest_update_of_an_expected_value_is_not_a_weakening() {
    // The control that matters most: an agent that changes behaviour and
    // updates the test to pin the NEW behaviour has done the right thing.
    let head = "\
def test_rate_small():
    assert rate(2) == 12.5

def test_rate_large():
    assert rate(10) == 32.5
";
    let found = detectors("test_shipping.py", PY_BASE, head, Language::Python);
    assert!(
        !has(&found, "assertion_weakened"),
        "an honest expected-value update was called a weakening: {found:?}"
    );
    assert!(!has(&found, "assertion_removed"), "{found:?}");
    assert!(
        found.iter().all(|(_, s)| *s != Severity::Fail),
        "an honest edit produced a fail-severity finding: {found:?}"
    );
}

#[test]
fn python_a_new_test_is_info_and_never_a_fail() {
    let head = format!("{PY_BASE}\ndef test_rate_zero():\n    assert rate(0) == 4.0\n");
    let found = detectors("test_shipping.py", PY_BASE, &head, Language::Python);
    assert!(has(&found, "test_added"), "{found:?}");
    assert!(found.iter().all(|(_, s)| *s != Severity::Fail), "{found:?}");
}

#[test]
fn python_a_comment_mentioning_skip_is_not_a_skip_marker() {
    // The reason node selection is structural. A regex finds this.
    let head = "\
def test_rate_small():
    # we used to @pytest.mark.skip this, but it is stable now
    assert rate(2) == 9.0

def test_rate_large():
    assert rate(10) == 29.0
";
    let found = detectors("test_shipping.py", PY_BASE, head, Language::Python);
    assert!(
        !has(&found, "skip_marker_added"),
        "a comment was read as a marker: {found:?}"
    );
}

// ---------------------------------------------------------------------------
// Rust
// ---------------------------------------------------------------------------

const RS_BASE: &str = "\
#[test]
fn adds_two() {
    assert_eq!(add(2, 2), 4);
}

#[test]
fn adds_three() {
    assert_eq!(add(1, 2), 3);
}
";

#[test]
fn rust_ignore_attribute_added_is_a_skip_marker() {
    let head = "\
#[test]
#[ignore]
fn adds_two() {
    assert_eq!(add(2, 2), 4);
}

#[test]
fn adds_three() {
    assert_eq!(add(1, 2), 3);
}
";
    let found = detectors("src/lib.rs", RS_BASE, head, Language::Rust);
    assert!(has(&found, "skip_marker_added"), "{found:?}");
}

#[test]
fn rust_assert_eq_to_bare_assert_is_a_weakening() {
    let head = "\
#[test]
fn adds_two() {
    assert!(add(2, 2) > 0);
}

#[test]
fn adds_three() {
    assert_eq!(add(1, 2), 3);
}
";
    let found = detectors("src/lib.rs", RS_BASE, head, Language::Rust);
    assert!(has(&found, "assertion_weakened"), "{found:?}");
}

#[test]
fn rust_a_non_test_function_is_not_examined() {
    let base = "fn helper() { assert_eq!(1, 1); }\n";
    let head = "fn helper() { }\n";
    let found = detectors("src/lib.rs", base, head, Language::Rust);
    assert!(
        found.is_empty(),
        "a plain function was treated as a test: {found:?}"
    );
}

// ---------------------------------------------------------------------------
// JavaScript / TypeScript and Go — the remaining two §6.7 names
// ---------------------------------------------------------------------------

#[test]
fn javascript_it_skip_is_a_skip_marker() {
    let base = "it('adds', () => { expect(add(2,2)).toEqual(4); });\n";
    let head = "it.skip('adds', () => { expect(add(2,2)).toEqual(4); });\n";
    let found = detectors("add.test.js", base, head, Language::JavaScript);
    assert!(has(&found, "skip_marker_added"), "{found:?}");
}

#[test]
fn typescript_toequal_to_tocontain_is_a_weakening() {
    let base = "it('adds', () => { expect(sum(2,2)).toEqual(4); });\n";
    let head = "it('adds', () => { expect(String(sum(2,2))).toContain('4'); });\n";
    let found = detectors("add.test.ts", base, head, Language::TypeScript);
    assert!(has(&found, "assertion_weakened"), "{found:?}");
}

#[test]
fn go_t_skip_is_a_skip_marker() {
    let base = "\
func TestAdd(t *testing.T) {
\tif add(2, 2) != 4 {
\t\tt.Errorf(\"bad\")
\t}
}
";
    let head = "\
func TestAdd(t *testing.T) {
\tt.Skip(\"flaky\")
\tif add(2, 2) != 4 {
\t\tt.Errorf(\"bad\")
\t}
}
";
    let found = detectors("add_test.go", base, head, Language::Go);
    assert!(has(&found, "skip_marker_added"), "{found:?}");
}

#[test]
fn go_a_deleted_test_is_detected() {
    let base = "\
func TestAdd(t *testing.T) {
\tt.Errorf(\"x\")
}

func TestSub(t *testing.T) {
\tt.Errorf(\"y\")
}
";
    let head = "func TestAdd(t *testing.T) {\n\tt.Errorf(\"x\")\n}\n";
    let found = detectors("add_test.go", base, head, Language::Go);
    assert!(has(&found, "test_deleted"), "{found:?}");
}

// ---------------------------------------------------------------------------
// The pieces underneath
// ---------------------------------------------------------------------------

#[test]
fn strength_is_ordered_so_loosening_is_a_comparison_not_a_table() {
    assert!(Strength::Equality > Strength::Approximate);
    assert!(Strength::Approximate > Strength::Containment);
    assert!(Strength::Containment > Strength::Truthy);
}

#[test]
fn language_recognition_reports_the_unknown_rather_than_assuming_it_is_clean() {
    assert_eq!(Language::from_path("a/test_x.py"), Some(Language::Python));
    assert_eq!(Language::from_path("a/x_test.go"), Some(Language::Go));
    assert_eq!(
        Language::from_path("a/x.test.tsx"),
        Some(Language::TypeScript)
    );
    // A language this build cannot parse is `None` — the caller must say so
    // rather than record "no findings", which reads as "nothing wrong".
    assert_eq!(Language::from_path("a/XTest.java"), None);
    assert_eq!(Language::from_path("a/x.rb"), None);
}

#[test]
fn only_test_functions_are_collected() {
    let tests = parse_tests(
        "def helper():\n    assert 1 == 1\n\ndef test_real():\n    assert 2 == 2\n",
        Language::Python,
    )
    .expect("parse");
    let names: Vec<&str> = tests.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["test_real"]);
}

// ---------------------------------------------------------------------------
// §6.7's mock-target detector — the last of its `warn` list
// ---------------------------------------------------------------------------

#[test]
fn mocking_the_unit_under_test_is_flagged_and_mocking_a_neighbour_is_not() {
    // §6.7: "new mock/patch whose target resolves to a symbol changed in the
    // diff (mocking the unit under test)". The distinction is the whole value:
    // mocking a network client is ordinary and good; mocking the function the
    // session just changed removes the test's reason to exist.
    let base = "def test_rate():\n    assert rate(2) == 9.0\n";
    let head = "from unittest.mock import patch\n\ndef test_rate():\n    \
                with patch(\"app.shipping.rate\", return_value=9.0):\n        \
                assert rate(2) == 9.0\n";
    let changed = vec!["rate/1".to_string()];

    let found = thruline_verify::tamper::diff_test_file_with_symbols(
        "test_shipping.py",
        base,
        head,
        Language::Python,
        &changed,
    )
    .expect("diff");
    assert!(
        found.iter().any(|f| f.detector == "mocked_unit_under_test"),
        "{found:?}"
    );

    // The control: the same mock, of something the session did not touch.
    let found = thruline_verify::tamper::diff_test_file_with_symbols(
        "test_shipping.py",
        base,
        head,
        Language::Python,
        &["unrelated_helper/2".to_string()],
    )
    .expect("diff");
    assert!(
        !found.iter().any(|f| f.detector == "mocked_unit_under_test"),
        "mocking an unrelated symbol was flagged: {found:?}"
    );
}

#[test]
fn a_mock_that_was_already_there_is_not_a_new_finding() {
    // "New mock/patch" — a test that has always mocked its network client is
    // not doing anything in this session.
    let src = "from unittest.mock import patch\n\ndef test_rate():\n    \
               with patch(\"app.shipping.rate\"):\n        \
               assert rate(2) == 9.0\n";
    let found = thruline_verify::tamper::diff_test_file_with_symbols(
        "test_shipping.py",
        src,
        src,
        Language::Python,
        &["rate/1".to_string()],
    )
    .expect("diff");
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn mock_targets_resolve_in_the_module_form_and_the_symbol_form() {
    use thruline_verify::tamper::mock_hits_changed_symbol;
    let changed = vec!["rate/1".to_string(), "shipping/0".to_string()];
    assert!(mock_hits_changed_symbol("app.shipping.rate", &changed));
    assert!(mock_hits_changed_symbol("./shipping", &changed));
    assert!(mock_hits_changed_symbol("rate", &changed));
    assert!(!mock_hits_changed_symbol("requests.get", &changed));
    assert!(!mock_hits_changed_symbol("boto3.client", &changed));
}
