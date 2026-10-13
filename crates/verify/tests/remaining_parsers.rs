//! §6.2's remaining parsers.
//!
//! **jest, mocha and JUnit XML are now captured from real runs** —
//! `tests/fixtures/runners/`, produced by mocha 11 and jest 30 on node
//! v22.19.0 and by `pytest --junit-xml`. They were shape fixtures written from
//! documentation until it turned out this machine could run all three; the
//! `pytest -q` drift is why that mattered, and capturing them immediately found
//! a real defect in the JUnit reader.
//!
//! **`go test` is still a documented shape** — there is no Go toolchain here.
//! It is named as such rather than presented as a capture.

use std::path::PathBuf;

use thruline_verify::parse::{parse, Runner};

/// A fixture captured from a real run. See `fixtures/runners/README.md`.
fn captured(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/runners")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn argv(w: &[&str]) -> Vec<String> {
    w.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn every_new_runner_is_recognised_from_its_argv() {
    for (a, want) in [
        (argv(&["go", "test", "./..."]), Runner::GoTest),
        (argv(&["jest"]), Runner::Jest),
        (argv(&["vitest", "run"]), Runner::Jest),
        (argv(&["mocha", "test/"]), Runner::Mocha),
        (argv(&["dotnet", "test"]), Runner::DotnetTest),
        (argv(&["mvn", "test"]), Runner::Maven),
        (argv(&["gradle", "test"]), Runner::Gradle),
        (argv(&["npx", "jest", "--ci"]), Runner::Jest),
        (argv(&["yarn", "vitest", "run"]), Runner::Jest),
    ] {
        assert_eq!(Runner::from_argv(&a), Some(want), "{a:?}");
    }
}

#[test]
fn npm_test_is_not_attributed_because_it_runs_whatever_the_script_says() {
    // §6.2's "pass-through to nested runner detection via child exec events".
    // Guessing here would attribute a result to a runner that may not have run.
    assert_eq!(Runner::from_argv(&argv(&["npm", "test"])), None);
    assert_eq!(Runner::from_argv(&argv(&["make", "test"])), None);
}

/// **Shape fixtures, not captures.** There is no Go toolchain on this machine,
/// so these are written from the documented `-v` and `-json` formats. Named
/// here and in `REPORT.md` rather than presented as captures — the `pytest -q`
/// drift is exactly this risk, and a gap that is named is fine.
#[test]
fn go_test_parses_both_the_verbose_and_the_json_forms_from_documented_shapes() {
    let verbose = "=== RUN   TestAdd\n--- PASS: TestAdd (0.00s)\n=== RUN   TestSub\n\
                   --- FAIL: TestSub (0.01s)\n--- SKIP: TestMul (0.00s)\nFAIL\n";
    let p = parse(Some(Runner::GoTest), verbose)
        .parsed()
        .cloned()
        .expect("verbose");
    assert_eq!((p.passed, p.failed, p.skipped, p.tests_total), (1, 1, 1, 3));
    assert!(p.test_ids.contains("TestAdd"));

    let json = "{\"Action\":\"run\",\"Test\":\"TestAdd\"}\n\
                {\"Action\":\"pass\",\"Test\":\"TestAdd\"}\n\
                {\"Action\":\"fail\",\"Test\":\"TestSub\"}\n\
                {\"Action\":\"pass\",\"Package\":\"x\"}\n";
    let p = parse(Some(Runner::GoTest), json)
        .parsed()
        .cloned()
        .expect("json");
    // The package-level `pass` is not a test and must not be counted.
    assert_eq!((p.passed, p.failed, p.tests_total), (1, 1, 2));
}

#[test]
fn a_go_package_that_merely_compiled_is_not_a_suite_that_ran() {
    // Shape fixture; see the note above.
    // `ok  example.com/pkg  0.002s` reports no counts. Reading it as a clean
    // run is the collapse §6 forbids.
    let outcome = parse(Some(Runner::GoTest), "ok  \texample.com/pkg\t0.002s\n");
    assert!(outcome.is_unverifiable(), "{outcome:?}");
}

#[test]
fn jest_is_parsed_from_a_real_captured_run() {
    // `Tests: 1 failed, 1 skipped, 1 todo, 2 passed, 5 total` — jest 30's real
    // wording, including the `todo` bucket a hand-written fixture would not
    // have had.
    let out = captured("jest-mixed.txt");
    assert!(out.contains("1 todo"), "the capture lost its todo bucket");
    let p = parse(Some(Runner::Jest), &out)
        .parsed()
        .cloned()
        .expect("jest");
    assert_eq!(p.tests_total, 5);
    assert_eq!(p.passed, 2);
    assert_eq!(p.failed, 1);
    // Skipped and todo are both "did not run".
    assert_eq!(p.skipped, 2);
}

#[test]
fn a_real_jest_collection_error_reports_zero_tests_and_is_unverifiable() {
    // The case that matters most: jest prints `Tests: 0 total` when a suite
    // fails to load. Reading that as a clean run is the collapse §6 forbids.
    let out = captured("jest-collection-error.txt");
    assert!(
        out.contains("Tests:       0 total"),
        "the capture changed shape"
    );
    assert!(
        parse(Some(Runner::Jest), &out).is_unverifiable(),
        "a suite that failed to load parsed as a result"
    );
}

#[test]
fn a_real_jest_run_with_no_tests_prints_no_summary_and_is_unverifiable() {
    let out = captured("jest-no-tests.txt");
    assert!(out.contains("No tests found"));
    assert!(parse(Some(Runner::Jest), &out).is_unverifiable());
}

#[test]
fn mocha_is_parsed_from_a_real_captured_run() {
    // The capture carries the trailing duration — `2 passing (3ms)` — that
    // broke the first implementation: a line-suffix match found nothing and
    // reported ZERO passing, a suite that ran and passed none.
    let out = captured("mocha-mixed.txt");
    assert!(
        out.contains("passing ("),
        "the capture lost its trailing duration"
    );
    let p = parse(Some(Runner::Mocha), &out)
        .parsed()
        .cloned()
        .expect("mocha");
    assert_eq!((p.passed, p.failed, p.skipped, p.tests_total), (2, 1, 1, 4));
}

#[test]
fn a_real_mocha_failure_stack_does_not_confuse_the_counts() {
    // The capture includes an AssertionError with a full stack trace and an
    // `expected/actual` diff. None of it is a summary line.
    let out = captured("mocha-mixed.txt");
    assert!(
        out.contains("AssertionError"),
        "the capture lost its stack trace"
    );
    let p = parse(Some(Runner::Mocha), &out)
        .parsed()
        .cloned()
        .expect("mocha");
    assert_eq!(p.failed, 1, "the stack trace was counted");
}

#[test]
fn a_real_mocha_run_with_no_files_or_a_load_error_is_unverifiable() {
    for name in ["mocha-no-test-files.txt", "mocha-collection-error.txt"] {
        let out = captured(name);
        assert!(
            parse(Some(Runner::Mocha), &out).is_unverifiable(),
            "{name} parsed as a result"
        );
    }
}

#[test]
fn junit_xml_is_parsed_from_a_real_pytest_report() {
    // Real `pytest --junit-xml` output, which wraps the suite in a
    // `<testsuites>` element carrying only a name. Matching `<testsuite`
    // matches the WRAPPER first, and the wrapper has no counts — the reader
    // has to reach the inner element rather than the first prefix hit.
    let xml = captured("pytest-junit.xml");
    assert!(
        xml.contains("<testsuites name="),
        "the capture lost its wrapper"
    );
    for runner in [
        Runner::Maven,
        Runner::Gradle,
        Runner::DotnetTest,
        Runner::JUnitXml,
    ] {
        let p = parse(Some(runner), &xml).parsed().cloned().expect("junit");
        assert_eq!(
            (p.tests_total, p.failed, p.skipped, p.passed),
            (3, 1, 1, 1),
            "{runner:?}"
        );
    }
}

#[test]
fn a_junit_runner_with_no_xml_in_its_output_is_unverifiable_with_a_hint() {
    let outcome = parse(Some(Runner::Maven), "[INFO] BUILD SUCCESS\n");
    assert!(outcome.is_unverifiable(), "{outcome:?}");
    // §6.2: "Unknown runner → unverifiable with a hint on how to enable
    // parsing." A build that succeeded is not a suite that passed.
    match outcome {
        thruline_verify::parse::RunnerOutcome::Unverifiable { hint, .. } => {
            assert!(hint.contains("surefire") || hint.contains("XML"), "{hint}")
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn no_new_parser_reports_a_clean_run_from_empty_output() {
    // The one mistake this module must make impossible, applied to every
    // runner added here.
    for runner in [
        Runner::GoTest,
        Runner::Jest,
        Runner::Mocha,
        Runner::Maven,
        Runner::Gradle,
        Runner::DotnetTest,
        Runner::JUnitXml,
    ] {
        for output in ["", "\n", "some unrelated noise\n"] {
            assert!(
                parse(Some(runner), output).is_unverifiable(),
                "{runner:?} read {output:?} as a result"
            );
        }
    }
}

#[test]
fn the_junit_reader_reaches_the_inner_element_and_not_the_wrapper() {
    // The defect the real capture exposed. `<testsuites>` carries no counts,
    // and matching the first `<testsuite` prefix finds the wrapper. It passed
    // only because the inner element fell inside the read window; a wrapper
    // with more attributes pushes it out.
    //
    // This fixture has exactly that wrapper, so it fails against the
    // first-prefix-match reader and passes against the corrected one.
    let padded = format!(
        "<?xml version=\"1.0\"?><testsuites name=\"{}\" time=\"1.0\" \
         id=\"{}\" package=\"{}\">\
         <testsuite name=\"inner\" tests=\"7\" failures=\"2\" errors=\"0\" skipped=\"1\">\
         </testsuite></testsuites>",
        "n".repeat(300),
        "i".repeat(200),
        "p".repeat(200)
    );
    let p = parse(Some(Runner::JUnitXml), &padded)
        .parsed()
        .cloned()
        .expect("the inner suite was not reached");
    assert_eq!((p.tests_total, p.failed, p.skipped, p.passed), (7, 2, 1, 4));
}

// ---------------------------------------------------------------------------
// Real captures of the three shapes that broke the first implementations
// ---------------------------------------------------------------------------

#[test]
fn a_jest_suite_that_failed_to_load_does_not_leave_a_tidy_green_tally() {
    // `Tests:` counts only what LOADED. A file with a syntax error contributes
    // zero tests, so a whole broken suite leaves `Tests: 2 passed, 2 total`
    // beside `Test Suites: 1 failed`. That is the `conftest collect_ignore`
    // cheat in a different runner.
    let out = captured("jest-suite-failed-tests-green.txt");
    assert!(
        out.contains("Test Suites: 1 failed") && out.contains("Tests:       2 passed, 2 total"),
        "the capture no longer has the dangerous shape"
    );
    let outcome = parse(Some(Runner::Jest), &out);
    assert!(
        outcome.is_unverifiable(),
        "a suite that never loaded parsed as a clean run: {outcome:?}"
    );
    match outcome {
        thruline_verify::parse::RunnerOutcome::Unverifiable { reason, .. } => {
            assert!(reason.contains("only the suites that loaded"), "{reason}")
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_run_where_a_real_test_failed_still_reports_the_failure_rather_than_unknown() {
    // The control. Every failing suite has an explanation when a test in it
    // failed, and downgrading a real failure to "unknown" would be worse than
    // the bug being fixed.
    let out = captured("jest-partial-load-failure.txt");
    let p = parse(Some(Runner::Jest), &out)
        .parsed()
        .cloned()
        .expect("a run with real test failures should still report them");
    assert_eq!(p.failed, 1);
    assert_eq!(p.tests_total, 7);
}

#[test]
fn ansi_colour_does_not_defeat_a_parser_because_ci_is_where_colour_lives() {
    // Real `mocha --color`: `\x1b[32m 2 passing\x1b[0m\x1b[90m (2ms)\x1b[0m`.
    // It failed safe before — unverifiable, never a wrong number — but a parser
    // that cannot read what CI produces does not work where it matters.
    let out = captured("mocha-ansi-color.txt");
    assert!(
        out.contains('\u{1b}'),
        "the capture lost its escape sequences"
    );
    let p = parse(Some(Runner::Mocha), &out)
        .parsed()
        .cloned()
        .expect("coloured output was unreadable");
    assert_eq!((p.passed, p.failed, p.skipped), (2, 1, 1));

    // And the plain capture still gives the same answer: stripping must be a
    // no-op on output that has nothing to strip.
    let plain = parse(Some(Runner::Mocha), &captured("mocha-mixed.txt"))
        .parsed()
        .cloned()
        .expect("plain");
    assert_eq!((plain.passed, plain.failed, plain.skipped), (2, 1, 1));
}

#[test]
fn strip_ansi_removes_sequences_without_touching_ordinary_text() {
    use thruline_verify::parse::strip_ansi;
    assert_eq!(strip_ansi("\u{1b}[32m 2 passing\u{1b}[0m"), " 2 passing");
    assert_eq!(strip_ansi("\u{1b}]0;title\u{7}ok"), "ok");
    assert_eq!(strip_ansi("no escapes here"), "no escapes here");
    // A bracket that is not an escape survives.
    assert_eq!(strip_ansi("array[0] == 3"), "array[0] == 3");
}

#[test]
fn jest_json_is_parsed_because_the_spec_names_it_and_ci_produces_it() {
    // §6.2: "jest/vitest (default **and `--json`**)".
    let out = captured("jest-report.json");
    let p = parse(Some(Runner::Jest), &out)
        .parsed()
        .cloned()
        .expect("the --json report was unreadable");
    assert_eq!(p.tests_total, 7);
    assert_eq!(p.passed, 4);
    assert_eq!(p.failed, 1);
    // pending + todo are both "did not run".
    assert_eq!(p.skipped, 2);
}

#[test]
fn a_jest_json_report_with_a_broken_suite_and_no_failing_test_is_unverifiable() {
    // The same suite-level rule as the text form, in the shape CI emits.
    let json = r#"{"numFailedTestSuites":1,"numFailedTests":0,"numPassedTests":2,
                   "numPendingTests":0,"numRuntimeErrorTestSuites":1,"numTodoTests":0,
                   "numTotalTestSuites":2,"numTotalTests":2,"success":false}"#;
    let outcome = parse(Some(Runner::Jest), json);
    assert!(outcome.is_unverifiable(), "{outcome:?}");
}

// Summaries observed from actual Vitest 4.1.10 on Node 22.19.0 during the
// installed L0 TypeScript qualification, 2026-09-07. Mutations below are
// deliberately synthetic counterexamples to those captured summaries.
const VITEST4_PASS: &str = " RUN  v4.1.10 /fixture\n\n Test Files  1 passed (1)\n      Tests  1 passed (1)\n   Start at  06:48:47\n   Duration  106ms (transform 11ms, setup 0ms, import 16ms, tests 1ms, environment 0ms)\n";

#[test]
fn vitest4_real_pass_fail_and_skip_summaries_preserve_actual_counts() {
    for (word, want) in [
        ("passed", (1, 0, 0)),
        ("failed", (0, 1, 0)),
        ("skipped", (0, 0, 1)),
    ] {
        let text = VITEST4_PASS.replace("passed", word);
        let outcome = parse(Some(Runner::Jest), &text);
        let p = outcome.parsed().expect("complete observed summary");
        assert_eq!((p.passed, p.failed, p.skipped), want);
        assert_eq!(p.tests_total, 1);
    }
}

#[test]
fn vitest4_missing_footer_or_inconsistent_counts_cannot_look_like_a_clean_run() {
    let before_duration = VITEST4_PASS.find("   Duration").unwrap();
    for cut in 0..=before_duration {
        assert!(
            parse(Some(Runner::Jest), &VITEST4_PASS[..cut])
                .parsed()
                .is_none(),
            "prefix {cut} acquired authority"
        );
    }
    for text in [
        VITEST4_PASS.replace("Tests  1 passed (1)", "Tests  1 passed (2)"),
        VITEST4_PASS.replace(
            "Test Files  1 passed (1)",
            "Test Files  1 failed | 1 passed (2)",
        ),
        VITEST4_PASS.replace("Tests  1 passed (1)", "Tests  1 passed | 1 passed (2)"),
        VITEST4_PASS.replace("Tests  1 passed (1)", "Tests  1 mystery (1)"),
        VITEST4_PASS.replace(
            "Tests  1 passed (1)",
            "Tests  18446744073709551615 passed | 1 skipped (0)",
        ),
        format!("{VITEST4_PASS}\n Errors  1 error\n"),
        format!("{VITEST4_PASS}{VITEST4_PASS}"),
    ] {
        assert!(
            parse(Some(Runner::Jest), &text).parsed().is_none(),
            "{text}"
        );
    }
}
