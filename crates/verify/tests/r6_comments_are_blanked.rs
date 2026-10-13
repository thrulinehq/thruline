//! R6 (Track G) — an apostrophe in a comment accused an honest rename of being
//! a deletion, at `fail`.
//!
//! # The measurement, which is G's
//!
//! `as_test_fn` handed the function's **raw** text to `strip_string_literals`.
//! That function treats `'` as a literal delimiter and knows nothing about
//! comments, so an apostrophe in a comment opens a literal that never closes
//! and truncates everything after it. The shortened token set falls below
//! `RENAME_SIMILARITY` in `similar_bodies`, and the rename is reported as
//! `test_deleted` at **`fail`** with `test_added` beside it.
//!
//! §13.1: *"accusing honest users of gaming … is the most expensive error this
//! product can make."* This is that error, produced by rewording a comment.
//!
//! # Why the control is the whole argument
//!
//! Every case below is paired with the same rename written without an
//! apostrophe. If the pair disagrees, the apostrophe is the cause; if a fix
//! makes both come back clean, it fixed the cause rather than moving the
//! threshold.

use thruline_verify::tamper::{diff_test_file, parse_tests, Language};

/// G's fixture: a rename that also rewords its own comment, assertions
/// untouched.
const BASE_APOSTROPHE: &str = "def test_rate_isnt_negative():\n    # the rate isn't negative for a small parcel\n    assert rate(2) == 9.0\n    assert rate(3) == 13.5\n    assert rate(4) == 18.0\n";
const HEAD_APOSTROPHE: &str = "def test_rate_is_non_negative():\n    # the rate is non-negative for a small parcel\n    assert rate(2) == 9.0\n    assert rate(3) == 13.5\n    assert rate(4) == 18.0\n";

/// The same rename, comment reworded without an apostrophe.
const BASE_PLAIN: &str = "def test_rate_isnt_negative():\n    # the rate is not negative for a small parcel\n    assert rate(2) == 9.0\n    assert rate(3) == 13.5\n    assert rate(4) == 18.0\n";
const HEAD_PLAIN: &str = "def test_rate_is_non_negative():\n    # the rate is non-negative for a small parcel\n    assert rate(2) == 9.0\n    assert rate(3) == 13.5\n    assert rate(4) == 18.0\n";

fn detectors(base: &str, head: &str) -> Vec<(String, thruline_verify::verdict::Severity)> {
    diff_test_file("tests/test_rate.py", base, head, Language::Python)
        .expect("diff")
        .into_iter()
        .map(|f| (f.detector.clone(), f.severity()))
        .collect()
}

#[test]
fn an_apostrophe_in_a_comment_no_longer_turns_a_rename_into_a_deletion() {
    let with_apostrophe = detectors(BASE_APOSTROPHE, HEAD_APOSTROPHE);
    assert!(
        !with_apostrophe
            .iter()
            .any(|(d, _)| d == "test_deleted" || d == "test_added"),
        "an honest rename was reported as a deletion because its comment \
         contains an apostrophe: {with_apostrophe:?}"
    );

    // The control: the same rename without the apostrophe. If this ever
    // disagrees with the case above, the apostrophe is back in the causal path.
    let plain = detectors(BASE_PLAIN, HEAD_PLAIN);
    assert_eq!(
        with_apostrophe, plain,
        "the two rewordings of one rename still produce different findings, so \
         the apostrophe is still doing something"
    );
}

/// The second fact that reached the raw text by the same route.
///
/// **In Go, not Python, and that is the point.** The in-body skip search only
/// runs where `skip_in_body` is true, which Python is not — its markers are
/// decorators. Written first against Python, this test passed under the
/// mutation that restores the defect, because the route it names was never
/// reached. A test that cannot fail for the reason it gives is worse than no
/// test: it is a guard reported as covering something it does not touch.
#[test]
fn a_skip_marker_mentioned_in_a_comment_is_not_a_skip_marker() {
    let src = "func TestRate(t *testing.T) {\n\t// we used to t.Skip( this one\n\tif got != want {\n\t\tt.Errorf(\"no\")\n\t}\n}\n";
    let tests = parse_tests(src, Language::Go).expect("parse");
    assert_eq!(tests.len(), 1, "the Go fixture did not parse as a test");
    assert!(
        !tests[0].has_skip_marker,
        "a comment MENTIONING a skip marker was read as one — the same class of \
         failure this crate's own Python control fixture caught once already"
    );

    // The control: a real marker is still found. Without this, the assertion
    // above is satisfied by a build that stopped looking for skip markers.
    let real = "func TestRate(t *testing.T) {\n\tt.Skip(\"flaky\")\n}\n";
    let tests = parse_tests(real, Language::Go).expect("parse");
    assert!(
        tests[0].has_skip_marker,
        "the real t.Skip( stopped being found"
    );
}

#[test]
fn sleep_and_retry_mentioned_in_a_comment_are_not_flaky_signals() {
    // The third of the three G named.
    let src = "def test_rate():\n    # no sleep( or retry needed here any more\n    assert rate(2) == 9.0\n";
    let tests = parse_tests(src, Language::Python).expect("parse");
    assert!(
        !tests[0].has_sleep_or_retry,
        "a comment saying a test no longer sleeps was read as a test that sleeps"
    );

    let real = "def test_rate():\n    time.sleep(2)\n    assert rate(2) == 9.0\n";
    let tests = parse_tests(real, Language::Python).expect("parse");
    assert!(tests[0].has_sleep_or_retry, "a real sleep stopped counting");
}

#[test]
fn a_mock_target_named_only_in_a_comment_is_not_a_mock_target() {
    // The fourth, which G did not name and which was on the same route.
    // Blanked here too rather than left as the one raw path: leaving one is how
    // a fixed bug comes back wearing a different field name. This test IS the
    // measurement for that decision rather than an assumption behind it.
    let src = "def test_rate():\n    # we removed the patch(\"app.rate\") that used to be here\n    assert rate(2) == 9.0\n";
    let tests = parse_tests(src, Language::Python).expect("parse");
    assert!(
        tests[0].mock_targets.is_empty(),
        "a mock target named in a comment was collected: {:?}",
        tests[0].mock_targets
    );

    let real = "def test_rate():\n    patch(\"app.rate\")\n    assert rate(2) == 9.0\n";
    let tests = parse_tests(real, Language::Python).expect("parse");
    assert_eq!(
        tests[0].mock_targets,
        vec!["app.rate".to_string()],
        "a real mock target stopped being collected"
    );
}

#[test]
fn blanking_preserves_byte_length_so_nothing_downstream_shifts() {
    // The contract `crates/langpacks`' own `blank_comments` keeps, matched here
    // deliberately: two implementations of one idea that disagree about offsets
    // are worse than either. Asserted through a fact that would move if the
    // lengths did — the token set of a body whose comment is the longest line.
    let with_comment = "def test_rate():\n    # a very long comment indeed, far longer than the code\n    assert rate(2) == 9.0\n";
    let without = "def test_rate():\n    \n    assert rate(2) == 9.0\n";
    let a = &parse_tests(with_comment, Language::Python).expect("parse")[0];
    let b = &parse_tests(without, Language::Python).expect("parse")[0];
    assert_eq!(
        a.body_tokens, b.body_tokens,
        "a comment still contributes tokens, so it is not blanked"
    );
}
