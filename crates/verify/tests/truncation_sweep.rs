//! **Can a truncated or partial document parse to a green tally?** — asked of
//! every parser §6.2 names, not just the one that failed.
//!
//! # The defect this generalises
//!
//! The JUnit reader took its counts from the `<testsuite>` header and never
//! asked whether the document had ended. `tests` was required, so a report
//! without it was refused; but once `tests` parsed, `failures` and `errors`
//! **defaulted to zero**. Measured on a five-test suite with one failure:
//!
//! ```text
//! complete, 1 failure                 total=5 passed=4 failed=1
//! TRUNCATED mid-file (no close tags)  total=5 passed=5 failed=0   <- GREEN
//! ```
//!
//! A runner killed mid-write, a full disk, and a container OOM-killed during
//! teardown all produce that shape, and it fails toward **supported**.
//!
//! # The property, and why it is the right one
//!
//! Not "a prefix must be unverifiable" — for a line-oriented runner a prefix is
//! often legitimately readable, and demanding otherwise would refuse honest
//! evidence. The property is one-directional:
//!
//! > **A prefix of a run's output may never report fewer failures than the
//! > whole output does, and may never report more passing tests.**
//!
//! Truncation removes information. A parser that gains a passing test or loses
//! a failure by being shown *less* is inventing, and inventing in the direction
//! that supports a claim.
//!
//! The sweep runs over **real captured output** — the same fixtures the parser
//! tests use, which came out of real runners — at every byte boundary. A
//! synthetic corpus would tell you about the corpus.

use std::path::{Path, PathBuf};

use thruline_verify::parse::{parse, Runner};

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/runners")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Does a **complete** read of this fixture produce a tally?
///
/// Declared per entry rather than counted, so "seven of ten read" cannot drift
/// into "three of ten read" while a floor still passes. Several of these exist
/// precisely to make the parser refuse — a jest suite that failed to load
/// beside a green `Tests:` tally, for one — and marking that expectation is
/// how a regression in either direction is visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reads {
    Yes,
    No,
}

/// `(runner, fixture, label, whether a complete read parses)`.
///
/// Every entry is a **real** captured file. `go test` has no entry because no
/// Go toolchain exists on this machine to capture one — named here rather than
/// filled with something written from the documentation, which is the standing
/// treatment for that parser.
fn corpus() -> Vec<(Runner, String, &'static str, Reads)> {
    use Reads::{No, Yes};
    vec![
        (Runner::JUnitXml, fixture("pytest-junit.xml"), "pytest --junit-xml", Yes),
        (Runner::Jest, fixture("jest-mixed.txt"), "jest, some failing", Yes),
        (Runner::Jest, fixture("jest-partial-load-failure.txt"), "jest, a suite failed to load", Yes),
        (Runner::Jest, fixture("jest-suite-failed-tests-green.txt"), "jest, green tally under a failed suite", No),
        (Runner::Jest, fixture("jest-collection-error.txt"), "jest, collection error", No),
        (Runner::Mocha, fixture("mocha-mixed.txt"), "mocha, some failing", Yes),
        (Runner::Mocha, fixture("mocha-ansi-color.txt"), "mocha with colour", Yes),
        (Runner::Mocha, fixture("mocha-collection-error.txt"), "mocha, collection error", No),
        (
            Runner::Pytest,
            "test_a.py ..F                                            [100%]\n\
             \n=================================== FAILURES ===================================\n\
             ____________________________________ test_c ____________________________________\n\
             \n    def test_c():\n>       assert 1 == 2\nE       assert 1 == 2\n\n\
             =========================== short test summary info ============================\n\
             FAILED test_a.py::test_c - assert 1 == 2\n\
             1 failed, 2 passed in 0.03s\n"
                .to_string(),
            "pytest, one failing",
            Yes,
        ),
        (
            Runner::CargoTest,
            "running 3 tests\ntest a ... ok\ntest b ... ok\ntest c ... FAILED\n\n\
             failures:\n\n---- c stdout ----\nthread 'c' panicked\n\n\
             failures:\n    c\n\n\
             test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n"
                .to_string(),
            "cargo test, one failing",
            Yes,
        ),
    ]
}

/// What a parse says, reduced to the two numbers the property is about.
///
/// `None` for `unverifiable`, which is always an allowed answer: refusing to
/// read a partial document is exactly the behaviour being asked for.
fn tally(runner: Runner, text: &str) -> Option<(u64, u64)> {
    parse(Some(runner), text)
        .parsed()
        .map(|p| (p.passed, p.failed))
}

// ---------------------------------------------------------------------------
// The sweep
// ---------------------------------------------------------------------------

/// No prefix of a failing run may lose its failure or gain a passing test.
#[test]
fn truncation_never_turns_a_failing_run_green() {
    let mut checked = 0usize;
    let mut refused = 0usize;
    for (runner, whole, label, expected) in corpus() {
        // `None` means the parser refuses the WHOLE output — which several of
        // these fixtures exist to make it do (a jest suite that failed to load
        // beside a green `Tests:` tally, for one). That is not a broken
        // fixture, it is a second property: a prefix may not be *more* readable
        // than the document it came from.
        let complete = tally(runner, &whole);
        let actually = if complete.is_some() {
            Reads::Yes
        } else {
            Reads::No
        };
        assert_eq!(
            actually, expected,
            "{label}: a complete read is {actually:?} and the corpus declares \
             {expected:?}. Either the parser changed or the declaration is stale, \
             and both are worth stopping for."
        );
        for cut in 1..whole.len() {
            // Never cut inside a UTF-8 character.
            if !whole.is_char_boundary(cut) {
                continue;
            }
            checked += 1;
            let Some(partial) = tally(runner, &whole[..cut]) else {
                refused += 1;
                continue;
            };
            let Some(complete) = complete else {
                panic!(
                    "{label}: the whole output is unverifiable, and a {cut}-byte \
                     prefix of it parsed to {partial:?}. A prefix cannot be more \
                     readable than the document it came from.\n--- prefix ---\n{}",
                    &whole[..cut]
                );
            };
            assert!(
                partial.1 >= complete.1,
                "{label}: a {cut}-byte prefix reports {} failure(s) where the whole \
                 output reports {}. Truncation removed information and the parser \
                 answered with a cleaner result.\n--- prefix ---\n{}",
                partial.1,
                complete.1,
                &whole[..cut]
            );
            assert!(
                partial.0 <= complete.0,
                "{label}: a {cut}-byte prefix reports {} passing test(s) where the \
                 whole output reports {}. A parser cannot gain a passing test by \
                 being shown less.",
                partial.0,
                complete.0
            );
        }
    }
    assert!(
        checked > 5_000,
        "the sweep examined only {checked} prefixes; it is not sweeping"
    );
    // Refusal is the expected answer for most prefixes. If nothing were ever
    // refused, every prefix would be parsing to something — which would mean
    // the property above is holding vacuously.
    assert!(
        refused > checked / 2,
        "only {refused} of {checked} prefixes were refused, which is too few for \
         partial documents — is anything actually being read as incomplete?"
    );
}

/// The specific shape the JUnit reader got wrong, pinned as its own case.
#[test]
fn an_incomplete_junit_report_is_unverifiable_rather_than_green() {
    let whole = fixture("pytest-junit.xml");
    // Cut after the header but before the document ends — the exact shape a
    // runner killed mid-write leaves behind.
    let head_end = whole.find("<testcase").expect("a testcase element");
    let truncated = &whole[..head_end + 20];
    assert!(
        truncated.contains("tests=\""),
        "the cut must keep the header, or this is testing the wrong thing"
    );
    let out = parse(Some(Runner::JUnitXml), truncated);
    assert!(
        out.is_unverifiable(),
        "an incomplete JUnit report parsed to {:?}",
        out.parsed()
    );
    let reason = match &out {
        thruline_verify::parse::RunnerOutcome::Unverifiable { reason, .. } => reason.clone(),
        other => panic!("{other:?}"),
    };
    assert!(
        reason.contains("incomplete"),
        "the reason must say what was wrong with the document: {reason}"
    );

    // The control: the COMPLETE fixture still reads, or the fix above is just
    // a parser that refuses everything.
    let complete = parse(Some(Runner::JUnitXml), &whole);
    let p = complete.parsed().expect("the complete report must parse");
    assert!(p.tests_total > 0, "{p:?}");
}

/// An absent `failures` attribute is answered by **counting the elements**,
/// not by a default.
///
/// The JUnit schema makes the attribute optional, so its absence is not itself
/// a defect — but `unwrap_or(0)` answered "no failures" for a document that
/// contained one, which is a constant standing where a measurement belongs.
#[test]
fn an_absent_failures_attribute_is_counted_rather_than_assumed() {
    let with_element = r#"<?xml version="1.0"?><testsuites><testsuite name="s" tests="5">
        <testcase name="a"/><testcase name="b"/><testcase name="c"/><testcase name="d"/>
        <testcase name="e"><failure message="boom">boom</failure></testcase></testsuite></testsuites>"#;
    let p = parse(Some(Runner::JUnitXml), with_element)
        .parsed()
        .expect("complete report")
        .clone();
    assert_eq!(
        (p.tests_total, p.passed, p.failed),
        (5, 4, 1),
        "a `<failure>` element with no `failures` attribute was not counted"
    );

    // The control: a complete report with no failure elements and no attribute
    // is genuinely green, so the counting has not simply started accusing.
    let clean = r#"<?xml version="1.0"?><testsuites><testsuite name="s" tests="2">
        <testcase name="a"/><testcase name="b"/></testsuite></testsuites>"#;
    let p = parse(Some(Runner::JUnitXml), clean)
        .parsed()
        .expect("complete report")
        .clone();
    assert_eq!((p.tests_total, p.passed, p.failed), (2, 2, 0));

    // And an `<error>` element counts the same way.
    let errored = r#"<?xml version="1.0"?><testsuites><testsuite name="s" tests="2">
        <testcase name="a"/><testcase name="b"><error message="x">x</error></testcase>
        </testsuite></testsuites>"#;
    let p = parse(Some(Runner::JUnitXml), errored)
        .parsed()
        .expect("complete report")
        .clone();
    assert_eq!((p.tests_total, p.passed, p.failed), (2, 1, 1));
}

/// A suite with no tests is well-formed and self-closing, and must still read.
///
/// The completeness check looks for a closing tag; a self-closing element has
/// none, and refusing it would be the fix creating a false positive.
#[test]
fn a_self_closing_empty_suite_still_reads() {
    let empty = r#"<?xml version="1.0"?><testsuites><testsuite name="s" tests="0" failures="0"/></testsuites>"#;
    let p = parse(Some(Runner::JUnitXml), empty)
        .parsed()
        .expect("an empty suite is a complete document")
        .clone();
    assert_eq!((p.tests_total, p.passed, p.failed), (0, 0, 0));
}

/// Every fixture directory entry is exercised by something.
///
/// A fixture nobody reads is a fixture that can rot into agreement with a
/// broken parser — the "fixtures drift toward the rule" failure, caught three
/// times in this build.
#[test]
fn every_captured_fixture_is_read_by_this_sweep_or_by_the_parser_tests() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runners");
    let names: Vec<String> = std::fs::read_dir(&dir)
        .expect("fixture dir")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n != "README.md")
        .collect();
    assert!(names.len() >= 8, "{names:?}");
    let mine: Vec<String> = corpus()
        .iter()
        .map(|(_, text, _, _)| text.clone())
        .collect::<Vec<_>>();
    let others = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/remaining_parsers.rs"),
    )
    .expect("remaining_parsers.rs");
    for name in &names {
        let read_here = std::fs::read_to_string(dir.join(name))
            .map(|t| mine.contains(&t))
            .unwrap_or(false);
        assert!(
            read_here || others.contains(name.as_str()),
            "{name} is committed as a fixture and nothing reads it"
        );
    }
}

/// The cargo reader has **two** completeness checks, and neither is decoration.
///
/// Measured while validating them: removing either one alone leaves the
/// truncation sweep green, and only removing both fails it. Overlapping guards
/// are fine — the standing rule is against an *unreachable* one — so this
/// separates them with an input each catches alone, which is what keeps the
/// redundancy honest rather than making one of them dead code somebody deletes
/// later on the grounds that nothing broke.
#[test]
fn each_cargo_completeness_check_catches_something_the_other_does_not() {
    // Only the `finished in` check can see this: the verdict word is `ok`, so
    // the word-versus-counts cross-check has nothing to disagree with.
    let cut_but_says_ok = "running 3 tests\ntest a ... ok\n\ntest result: ok. 2 passed";
    let out = parse(Some(Runner::CargoTest), cut_but_says_ok);
    assert!(
        out.is_unverifiable(),
        "a truncated `ok` summary parsed to {:?}",
        out.parsed()
    );

    // Only the cross-check can see this: the line is complete — it reaches
    // `finished in` — and says FAILED while counting zero failures.
    let complete_but_contradictory = "running 3 tests\ntest result: FAILED. 2 passed; 0 failed; \
                                      0 ignored; 0 measured; 0 filtered out; finished in 0.01s";
    let out = parse(Some(Runner::CargoTest), complete_but_contradictory);
    assert!(
        out.is_unverifiable(),
        "a summary saying FAILED with no failures counted parsed to {:?}",
        out.parsed()
    );

    // And the control: an ordinary complete summary still reads, both ways.
    let honest_pass = "test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; \
                       0 filtered out; finished in 0.01s";
    let p = parse(Some(Runner::CargoTest), honest_pass)
        .parsed()
        .expect("an honest passing run must read")
        .clone();
    assert_eq!((p.passed, p.failed), (3, 0));
    let honest_fail = "test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; \
                       0 filtered out; finished in 0.01s";
    let p = parse(Some(Runner::CargoTest), honest_fail)
        .parsed()
        .expect("an honest failing run must read")
        .clone();
    assert_eq!((p.passed, p.failed), (2, 1));
}

/// A `test result:` line inside a test's own stdout must not poison the run.
///
/// The first version of the completeness check *returned* on an incomplete
/// summary line, which meant any test that printed `test result:` made its
/// whole suite unverifiable — a false refusal, caught by an existing property
/// test whose noise generator produces exactly that string. The check skips
/// such lines now; only a run with **no** complete summary is refused.
#[test]
fn a_truncated_summary_line_as_noise_does_not_veto_a_real_one() {
    let text = "test result:\nsome test's own stdout\n\
                test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; \
                0 filtered out; finished in 0.01s\n";
    let p = parse(Some(Runner::CargoTest), text)
        .parsed()
        .expect("the real summary must still be read")
        .clone();
    assert_eq!((p.passed, p.failed), (1, 2));
}

/// The mocha cross-check counts **distinct** failure numbers.
///
/// Each failure appears twice in mocha's output — once in the spec body, once
/// above its stack trace — so counting occurrences would double every failure
/// and refuse every honest failing run.
#[test]
fn mocha_counts_distinct_failures_not_occurrences() {
    let whole = fixture("mocha-mixed.txt");
    let p = parse(Some(Runner::Mocha), &whole)
        .parsed()
        .expect("an honest failing mocha run must read")
        .clone();
    assert_eq!(
        (p.passed, p.failed, p.skipped),
        (2, 1, 1),
        "the marker cross-check refused an honest run"
    );
    assert!(
        whole.matches("1)").count() >= 2,
        "this fixture no longer has the duplicate markers the test is about"
    );

    // And the shape it exists to refuse: `passing` present, `failing` cut off.
    let cut = whole
        .find("1 failing")
        .expect("the fixture has a failing line");
    let out = parse(Some(Runner::Mocha), &whole[..cut]);
    assert!(
        out.is_unverifiable(),
        "a mocha run cut between `passing` and `failing` parsed to {:?}",
        out.parsed()
    );
}
