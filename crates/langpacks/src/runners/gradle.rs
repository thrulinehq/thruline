//! §6.2 — Gradle console output, from the JUnit Platform and Vintage engines.
//!
//! Written against 17 captured `gradle` and `./gradlew` runs (Gradle 9.7.1,
//! JDK 17.0.19); `tests/fixtures/runners/README.md` names the command and the
//! exit code behind each one.
//!
//! # Gradle's counts are per *task*, and it aggregates none of them
//!
//! `N tests completed, M failed, K skipped` is printed by the `:test` task that
//! ran, once each. A two-module build prints it twice with nothing summing
//! them (`gradle-continue-mixed.txt`), and closes with
//! `FAILURE: Build completed with 2 failures.` — which counts failed **tasks**,
//! not tests, and is the number a loose parser reaches for. So the tallies are
//! summed and the build footer is never read as a count.
//!
//! # And it prints the tally only when something failed
//!
//! `gradle-all-pass.txt`, `gradle-skipped.txt`, `gradle-narrowed.txt` and
//! `gradlew-bootstrap-pass.txt` have **no tally line at all** — a green task
//! prints none. So the per-test `Class > method() PASSED` lines are the source
//! for a green run and the tally is the source when there is one, and the two
//! agree on every non-coloured fixture here.
//!
//! # Where they do not agree: `--console=rich`
//!
//! `gradle-ansi-color.txt` redraws a progress bar with cursor-motion escapes.
//! After the escapes are stripped, one line reads
//!
//! ```text
//! [############...] 85% EXECUTING [15s]> :test > 0 tests completed> :test > Executing test demo.CalculatorTestCalculatorTest > comparesGreaterThan() PASSED
//! ```
//!
//! — a real result glued onto the progress bar, and three of the six
//! `tests completed` strings in the file are mid-run running totals sitting in
//! lines like that one. Counting per-test lines gives **8** where the run had
//! 9; counting every `tests completed` string gives four different answers.
//!
//! Both are solved by requiring the *whole trimmed line* to be the thing:
//! [`whole_line_tally`] admits `9 tests completed, 2 failed, 2 skipped` and no
//! running total, and [`result_line`] rejects anything carrying a progress bar.
//! When colour is present and no whole-line tally survives, the per-test lines
//! are known to be an undercount and the parse is refused rather than reported
//! low — see [`parse`].
//!
//! # The four states with no test outcome in them, told apart
//!
//! `> Task :test NO-SOURCE` (no test sources exist), `> Task :test UP-TO-DATE`
//! (the task did not run; the result is a statement about a *previous* build),
//! `No tests found for given includes` (a `--tests` filter matched nothing) and
//! `gradle-default-logging.txt` (the task ran and no `testLogging` block was
//! configured, so **the commonest green Gradle shape carries no number
//! anywhere**). Only the first and third are zero-test runs; the other two are
//! unreadable, and they get different words.

use std::collections::{BTreeSet, HashSet};

use thruline_verify::parse::strip_ansi;

use super::{parsed, unverifiable, Runner, RunnerOutcome};

/// Recognise `gradle` / `gradlew` from an argv.
///
/// # The grammar differs from `verify::parse::Runner::from_argv`'s in two ways
///
/// `verify` asks `argv.iter().skip(1).any(|a| a == "test" || a == "check")`.
///
/// 1. **`-x test` reverses the meaning and is not read.** `gradle build -x test`
///    excludes the test task: it compiles, it assembles, and it runs no test at
///    all. `verify` matches the word `test` and reports a test run, so a check
///    is recorded whose output has no results in it. Here the exclusions are
///    collected and a task they name does not qualify.
/// 2. **A qualified task name is not matched.** `gradle :core:test` is how a
///    multi-project build runs one module's suite, and `:core:test` is not the
///    string `test`. `verify` returns `None` for it — a real test run with no
///    runner attached, which §6.3 reports as *"no test check was captured"*.
///
/// The task set is also wider on purpose: `build` and `check` both depend on
/// `test`, and a camel-cased `integrationTest`/`functionalTests` is the
/// standard name for a second suite. Recall costs nothing here — a
/// misrecognised non-test task produces output with no results in it, which is
/// `unverifiable` — while a missed one loses the evidence entirely.
///
/// `gradlew` is matched for completeness and is **not reachable from capture**:
/// `thruline_capture::shim::WRAPPED` wraps `gradle`, and `./gradlew` is a path,
/// which never consults PATH. `verify::parse::Runner::from_argv` records the
/// same limitation about the same program.
pub fn from_argv(argv: &[String]) -> Option<Runner> {
    let program = argv.first()?;
    let base = program.rsplit('/').next().unwrap_or(program);
    if !matches!(base, "gradle" | "gradlew" | "gradlew.bat") {
        return None;
    }

    let mut excluded: HashSet<&str> = HashSet::new();
    let mut tasks: Vec<&str> = Vec::new();
    let mut at = 1usize;
    while let Some(arg) = argv.get(at) {
        at += 1;
        if arg == "-x" || arg == "--exclude-task" {
            if let Some(value) = argv.get(at) {
                excluded.insert(task_name(value));
                at += 1;
            }
            continue;
        }
        if let Some(value) = arg
            .strip_prefix("--exclude-task=")
            .or_else(|| arg.strip_prefix("-x="))
        {
            excluded.insert(task_name(value));
            continue;
        }
        if arg.starts_with('-') {
            if GRADLE_VALUE_OPTIONS.contains(&arg.as_str()) {
                at += 1;
            }
            continue;
        }
        tasks.push(arg.as_str());
    }

    let runs_tests = tasks.iter().any(|t| {
        let name = task_name(t);
        if excluded.contains(name) {
            return false;
        }
        match name {
            "test" => true,
            // `check` and `build` reach the suite only through `test`, so
            // excluding `test` disarms them.
            "check" | "build" => !excluded.contains("test"),
            other => other.ends_with("Test") || other.ends_with("Tests"),
        }
    });
    runs_tests.then_some(Runner::Gradle)
}

/// Gradle options whose value travels in a **separate word**.
///
/// `--tests` is the load-bearing entry: without it,
/// `gradle test --tests demo.CalculatorTest` leaves the *pattern* standing
/// where a task name belongs, and `demo.CalculatorTest` ends with `Test`, so it
/// would qualify as a test task on its own — recognising the run for the wrong
/// reason, which survives until the pattern does not end that way.
const GRADLE_VALUE_OPTIONS: &[&str] = &[
    "--tests",
    "-p",
    "--project-dir",
    "-b",
    "--build-file",
    "-c",
    "--settings-file",
    "-g",
    "--gradle-user-home",
    "-I",
    "--init-script",
    "--console",
    "--max-workers",
    "--priority",
    "--include-build",
    "--warning-mode",
    "--project-cache-dir",
];

/// The task's own name, without its project path: `:core:test` -> `test`.
fn task_name(task: &str) -> &str {
    task.rsplit(':').next().unwrap_or(task)
}

/// Parse Gradle console output.
pub fn parse(output: &str) -> RunnerOutcome {
    // Whether colour was present has to be answered before it is removed: it is
    // what decides whether the per-test lines can be trusted as a census.
    let coloured = output.contains('\u{1b}');
    let plain = strip_ansi(output);
    let text: &str = &plain;

    let mut results = Vec::new();
    let mut tallies = Vec::new();
    for line in text.lines() {
        if let Some(r) = result_line(line) {
            results.push(r);
            continue;
        }
        if let Some(t) = whole_line_tally(line) {
            tallies.push(t);
        }
    }

    let mut p = parsed(Runner::Gradle);
    p.test_ids = results
        .iter()
        .map(|r| r.id.clone())
        .collect::<BTreeSet<_>>();

    // --- nothing to count -------------------------------------------------

    if results.is_empty() && tallies.is_empty() {
        return no_outcomes(text, coloured, p);
    }

    // --- colour, without a tally to correct it ----------------------------
    //
    // A `--console=rich` run glues result lines onto the redrawn progress bar,
    // and a glued line is rejected by `result_line` — correctly, since its
    // class name is corrupted, but that makes the survivors an undercount. On
    // `gradle-ansi-color.txt` the count would be 8 against a real 9. With a
    // whole-line tally present the tally settles it; without one there is
    // nothing to correct the undercount with, and a low number that looks like
    // a count is worse than no number.
    if coloured && tallies.is_empty() {
        return unverifiable(
            format!(
                "the output is coloured and carries no complete `N tests completed` line: \
                 {} per-test result line(s) survived, and a rich console glues results onto \
                 the progress bar it redraws, so that is a floor and not a count",
                results.len()
            ),
            "re-run with `--console=plain`, which prints one result per line and no \
             progress bar, or read `build/test-results/test/TEST-*.xml`",
        );
    }

    // --- counts -----------------------------------------------------------
    //
    // The tally is Gradle's own count and wins when there is one; the per-test
    // lines are the only source when the run was green, because a task that
    // passed prints no tally.
    if tallies.is_empty() {
        for r in &results {
            match r.verdict {
                Verdict::Passed => p.passed += 1,
                Verdict::Failed => p.failed += 1,
                Verdict::Skipped => p.skipped += 1,
            }
        }
        p.tests_total = p.passed + p.failed + p.skipped;
    } else {
        for t in &tallies {
            p.tests_total += t.total;
            p.failed += t.failed;
            p.skipped += t.skipped;
        }
        p.passed = p.tests_total.saturating_sub(p.failed + p.skipped);
    }

    // --- a build that failed for a reason no test reports ------------------
    //
    // The shape this guards is `gradle-multiproject-halted.txt`'s worst case:
    // without `--continue`, a failing `:core:test` stops the build and
    // `:web:test` never runs. In that capture `:core` failed, so the count is
    // non-zero and honest about what ran. Reverse the module order and the same
    // build produces a green tally, `BUILD FAILED`, and no trace whatsoever of
    // the module that never started — the string `web` does not occur in that
    // file at all. A green count under a failed build is therefore not evidence
    // that the suite passed, and it is refused.
    if p.failed == 0 && build_failed(text) {
        return unverifiable(
            format!(
                "the build failed while no test did: {} test(s) are reported, none of them \
                 failing, and `BUILD FAILED` has some other cause",
                p.tests_total
            ),
            "a compile failure in another module, a failed `check` task, or a `:test` task \
             that never ran because an earlier one halted the build all do this — and a task \
             that never ran leaves no trace in the log, so the tests it would have run are \
             not merely unreported, they are invisible. Re-run with `--continue`, or read \
             `build/test-results/*/TEST-*.xml` for every module",
        );
    }

    RunnerOutcome::Parsed(p)
}

/// The four no-outcome states, told apart by the marker each one leaves.
fn no_outcomes(text: &str, coloured: bool, p: super::Parsed) -> RunnerOutcome {
    if text.contains("> Task :compileTestJava FAILED")
        || text.contains("> Task :compileJava FAILED")
    {
        return unverifiable(
            "the sources did not compile, so the `:test` task never ran",
            "fix the compile error and re-run; Gradle reports `BUILD FAILED` for a compile \
             error and for a test failure with the same exit code, and only the `FAILED` \
             suffix on the compile task tells them apart",
        );
    }
    if text.contains("Could not install Gradle distribution")
        || text.contains("Exception in thread \"main\" java.io.IOException")
        || text.contains("Could not determine java version")
    {
        return unverifiable(
            "the Gradle wrapper never reached a build: the distribution download failed",
            "`./gradlew` downloads its own distribution on first use and fails before any \
             task exists; free the disk or fix the network and re-run. \
             `gradlew-bootstrap-pass.txt` is the same command after the download succeeded",
        );
    }

    // Zero tests, unambiguously, in two spellings. Returned as `Parsed` with a
    // total of zero rather than as `Unverifiable`, following
    // `verify::parse::parse_pytest`, which admits `no tests ran` as a summary
    // for the same reason: the output was read and the answer is "none". §6.3
    // then says what a zero-test run proves, which is more precise than "could
    // not read the output".
    if text.contains("> Task :test NO-SOURCE") {
        return RunnerOutcome::Parsed(p);
    }
    if text.contains("No tests found for given includes") {
        return RunnerOutcome::Parsed(p);
    }

    if text.contains(":test UP-TO-DATE") {
        return unverifiable(
            "the `:test` task was UP-TO-DATE, so it did not run: this output carries no \
             result from this invocation, only Gradle's claim that a previous one is still \
             valid",
            "run with `--rerun-tasks`, or `cleanTest test`, to make the suite actually \
             execute; `BUILD SUCCESSFUL` here is a statement about the build cache",
        );
    }
    if coloured {
        return unverifiable(
            "the output is coloured and carries neither a complete `N tests completed` line \
             nor a per-test result line",
            "re-run with `--console=plain`; a rich console redraws its progress bar with \
             cursor-motion escapes and can leave no complete line to read",
        );
    }
    unverifiable(
        "no per-test result line and no `N tests completed` line are present, so the run \
         reported no outcomes",
        "this is what a green Gradle build prints by default — `gradle-default-logging.txt` \
         is a fully passing run with no number anywhere in it. Add \
         `test { testLogging { events \"passed\", \"failed\", \"skipped\" } }` to the build \
         script, or read `build/test-results/test/TEST-*.xml`",
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Passed,
    Failed,
    Skipped,
}

struct Result_ {
    id: String,
    verdict: Verdict,
}

/// One `Class > method() PASSED` line, if the whole trimmed line is one.
///
/// The whole line, not a suffix match, and each half of that matters:
///
/// - `> Task :test FAILED` ends in ` FAILED` and is a task, not a test. It is
///   excluded by the leading `>`.
/// - The glued progress-bar line quoted in this module's docs ends in
///   ` PASSED` and carries a real result, but its class name is
///   `demo.CalculatorTestCalculatorTest` — two strings run together by a
///   cursor-motion escape. Admitting it would put a fabricated class name in
///   `test_ids`. It is excluded by the `[`, `]` and `%` a progress bar carries.
///
/// The id is `Class.method`, with the trailing `()` trimmed: the Vintage engine
/// prints no parentheses (`OrderServiceTest > comparesGreaterThan PASSED`)
/// where Jupiter prints them, and the same test must carry the same id under
/// both. Nested containers join with `.` for the same reason.
fn result_line(line: &str) -> Option<Result_> {
    let trimmed = line.trim();
    if trimmed.starts_with('>') {
        return None;
    }
    let (head, verdict) = [
        (" PASSED", Verdict::Passed),
        (" FAILED", Verdict::Failed),
        (" SKIPPED", Verdict::Skipped),
    ]
    .into_iter()
    .find_map(|(suffix, v)| trimmed.strip_suffix(suffix).map(|h| (h, v)))?;

    if head.contains('[') || head.contains(']') || head.contains('%') {
        return None;
    }
    let parts: Vec<&str> = head.split(" > ").map(str::trim).collect();
    if parts.len() < 2 || parts.iter().any(|p| p.is_empty()) {
        return None;
    }
    let id = parts
        .iter()
        .map(|p| p.strip_suffix("()").unwrap_or(p))
        .collect::<Vec<_>>()
        .join(".");
    Some(Result_ { id, verdict })
}

struct Tally {
    total: u64,
    failed: u64,
    skipped: u64,
}

/// `N tests completed[, N failed][, N skipped]`, and nothing else on the line.
///
/// The strictness is the point. `gradle-ansi-color.txt` contains six
/// `tests completed` strings, of which three are mid-run running totals
/// (`0 tests completed`, `4 tests completed`, `5 tests completed, 1 failed`)
/// embedded in redrawn progress-bar lines. Requiring the whole trimmed line to
/// match the grammar admits exactly the one that is the task's final count.
fn whole_line_tally(line: &str) -> Option<Tally> {
    let trimmed = line.trim();
    let mut t = Tally {
        total: 0,
        failed: 0,
        skipped: 0,
    };
    let mut saw_total = false;
    for (i, field) in trimmed.split(',').enumerate() {
        let words: Vec<&str> = field.split_whitespace().collect();
        let (Some(count), Some(word)) = (words.first(), words.get(1)) else {
            return None;
        };
        let Ok(n) = count.parse::<u64>() else {
            return None;
        };
        match (i, *word, words.len()) {
            (0, "test" | "tests", 3) if words[2] == "completed" => {
                t.total = n;
                saw_total = true;
            }
            (_, "failed", 2) if i > 0 => t.failed = n,
            (_, "skipped", 2) if i > 0 => t.skipped = n,
            _ => return None,
        }
    }
    saw_total.then_some(t)
}

/// Did the build itself fail?
///
/// `FAILURE: Build completed with 2 failures.` is deliberately matched as a
/// *build* signal only. Its number counts failed **tasks** — the two `:test`
/// tasks in `gradle-continue-mixed.txt`, over four tests — and reading it as a
/// test count is the mistake the whole-line tally rule exists to make
/// impossible.
fn build_failed(text: &str) -> bool {
    text.contains("BUILD FAILED")
        || text.contains("FAILURE: Build failed")
        || text.contains("FAILURE: Build completed with")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runners::Parsed;

    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/tests/fixtures/runners/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    fn ok(name: &str) -> Parsed {
        match parse(&fixture(name)) {
            RunnerOutcome::Parsed(p) => p,
            RunnerOutcome::Unverifiable { reason, hint } => {
                panic!("{name} parsed to unverifiable: {reason} / {hint}")
            }
        }
    }

    fn why(name: &str) -> String {
        match parse(&fixture(name)) {
            RunnerOutcome::Parsed(p) => panic!("{name} parsed to {p:?}, expected unverifiable"),
            RunnerOutcome::Unverifiable { reason, hint } => {
                assert!(!hint.is_empty(), "§6.2 requires a hint");
                reason
            }
        }
    }

    fn argv(s: &str) -> Vec<String> {
        s.split(' ').map(str::to_string).collect()
    }

    // --- argv --------------------------------------------------------------

    #[test]
    fn argv_grammar_including_the_two_verify_gets_wrong() {
        assert_eq!(from_argv(&argv("gradle test")), Some(Runner::Gradle));
        assert_eq!(
            from_argv(&argv("gradle --console=plain --no-daemon test")),
            Some(Runner::Gradle)
        );
        assert_eq!(from_argv(&argv("./gradlew test")), Some(Runner::Gradle));
        assert_eq!(from_argv(&argv("gradle check")), Some(Runner::Gradle));
        assert_eq!(from_argv(&argv("gradle build")), Some(Runner::Gradle));
        assert_eq!(
            from_argv(&argv("gradle integrationTest")),
            Some(Runner::Gradle)
        );

        // 1. `-x test` excludes the suite. `verify` sees the word `test` and
        //    reports a run that executes no test at all.
        assert_eq!(from_argv(&argv("gradle build -x test")), None);
        assert_eq!(from_argv(&argv("gradle -x test check")), None);
        assert_eq!(
            from_argv(&argv("gradle build -x check")),
            Some(Runner::Gradle),
            "excluding `check` leaves `build`'s dependency on `test` intact"
        );

        // 2. A project-qualified task. `verify` returns None for this, and it
        //    is how a multi-project build runs one module's suite.
        assert_eq!(from_argv(&argv("gradle :core:test")), Some(Runner::Gradle));

        // `--tests` takes a separate word, and that word must not be read as a
        // task — `demo.CalculatorTest` ends with `Test`.
        assert_eq!(
            from_argv(&argv("gradle test --tests demo.CalculatorTest")),
            Some(Runner::Gradle)
        );
        assert_eq!(
            from_argv(&argv("gradle assemble --tests demo.CalculatorTest")),
            None,
            "the pattern is a value, not a task"
        );

        assert_eq!(from_argv(&argv("gradle assemble")), None);
        assert_eq!(from_argv(&argv("gradle compileTestJava")), None);
        assert_eq!(from_argv(&argv("mvn test")), None);
        assert_eq!(from_argv(&[]), None);
    }

    // --- the states that must parse ---------------------------------------

    #[test]
    fn all_pass_has_no_tally_at_all_and_the_result_lines_are_the_count() {
        let text = fixture("gradle-all-pass.txt");
        assert!(
            !text.contains("tests completed"),
            "a green task prints no tally"
        );
        let p = ok("gradle-all-pass.txt");
        assert_eq!((p.tests_total, p.passed, p.failed), (3, 3, 0));
        assert!(p.test_ids.contains("CalculatorTest.addsTwoNumbers"));
    }

    #[test]
    fn mixed() {
        let p = ok("gradle-mixed.txt");
        assert_eq!((p.tests_total, p.passed, p.failed, p.skipped), (9, 5, 2, 2));
    }

    #[test]
    fn skipped() {
        let p = ok("gradle-skipped.txt");
        assert_eq!((p.tests_total, p.passed, p.skipped), (3, 1, 2));
    }

    #[test]
    fn narrowed_is_fully_green_and_the_output_never_mentions_the_filter() {
        let text = fixture("gradle-narrowed.txt");
        assert!(!text.contains("--tests"));
        let p = ok("gradle-narrowed.txt");
        assert_eq!((p.tests_total, p.passed), (3, 3));
    }

    #[test]
    fn ignore_failures_is_green_in_both_channels_and_the_output_is_not() {
        // `ignoreFailures = true` lives in build.gradle, not in the argv, so
        // nothing outside the output can reveal it: `BUILD SUCCESSFUL`, exit 0,
        // over `3 tests completed, 2 failed`.
        let text = fixture("gradle-ignore-failures.txt");
        assert!(text.contains("BUILD SUCCESSFUL"));
        let p = ok("gradle-ignore-failures.txt");
        assert_eq!((p.tests_total, p.failed), (3, 2));
    }

    #[test]
    fn the_vintage_engine_prints_no_parentheses_and_the_ids_still_match() {
        let text = fixture("gradle-vintage-mixed.txt");
        assert!(text.contains("OrderServiceTest > comparesGreaterThan PASSED"));
        let p = ok("gradle-vintage-mixed.txt");
        assert_eq!((p.tests_total, p.passed, p.failed, p.skipped), (7, 3, 2, 2));
        assert!(p.test_ids.contains("OrderServiceTest.comparesGreaterThan"));
    }

    #[test]
    fn the_wrapper_bootstrap_preamble_does_not_disturb_the_count() {
        let text = fixture("gradlew-bootstrap-pass.txt");
        assert!(text.contains("Downloading https://services.gradle.org/distributions/"));
        let p = ok("gradlew-bootstrap-pass.txt");
        assert_eq!((p.tests_total, p.passed), (3, 3));
    }

    #[test]
    fn parallel_forks_interleave_across_classes_and_the_tally_is_still_single() {
        let p = ok("gradle-parallel-forks-mixed.txt");
        assert_eq!((p.tests_total, p.passed, p.failed), (8, 7, 1));
    }

    // --- the tallies that must be summed ----------------------------------

    #[test]
    fn continue_prints_one_tally_per_task_and_sums_none_of_them() {
        // Two modules, both failing, `2 tests completed, 1 failed` twice, and a
        // footer reading `Build completed with 2 failures` — which counts
        // TASKS. The tests are 4 and the failures 2.
        let text = fixture("gradle-continue-mixed.txt");
        assert_eq!(text.matches("2 tests completed, 1 failed").count(), 2);
        assert!(text.contains("FAILURE: Build completed with 2 failures."));
        let p = ok("gradle-continue-mixed.txt");
        assert_eq!((p.tests_total, p.passed, p.failed), (4, 2, 2));
    }

    #[test]
    fn parallel_continue_races_the_module_order_and_not_the_sum() {
        let p = ok("gradle-parallel-continue.txt");
        assert_eq!((p.tests_total, p.passed, p.failed), (4, 2, 2));
    }

    #[test]
    fn a_halted_build_reports_what_ran_and_cannot_report_what_did_not() {
        // `:core:test` fails, the build stops, `:web:test` never runs. The
        // count is honest about the half that executed — and the string `web`
        // does not occur anywhere in the file, so no parser can know the other
        // half exists. That is a property of the bytes, not of this parser, and
        // the guard for its dangerous mirror image is
        // `a_green_count_under_a_failed_build_is_refused`.
        let text = fixture("gradle-multiproject-halted.txt");
        assert!(!text.contains("web"));
        let p = ok("gradle-multiproject-halted.txt");
        assert_eq!((p.tests_total, p.passed, p.failed), (2, 1, 1));
    }

    // --- colour ------------------------------------------------------------

    #[test]
    fn a_rich_console_glues_a_result_onto_the_progress_bar_and_the_tally_corrects_it() {
        let text = fixture("gradle-ansi-color.txt");
        assert!(text.contains('\u{1b}'), "the fixture really is coloured");
        // Six `tests completed` strings; three are mid-run running totals.
        let plain = strip_ansi(&text);
        assert_eq!(plain.matches("tests completed").count(), 6);
        assert_eq!(
            plain
                .lines()
                .filter(|l| whole_line_tally(l).is_some())
                .count(),
            1,
            "exactly one of the six is a complete line"
        );
        // And the per-test lines are an undercount: one result is glued onto a
        // progress-bar line and rejected.
        assert_eq!(
            plain.lines().filter(|l| result_line(l).is_some()).count(),
            8,
            "8 survive of the 9 the run reported"
        );
        let p = ok("gradle-ansi-color.txt");
        assert_eq!(
            (p.tests_total, p.passed, p.failed, p.skipped),
            (9, 5, 2, 2),
            "the tally is the count, not the surviving lines"
        );
    }

    #[test]
    fn colour_without_a_complete_tally_is_refused_rather_than_counted_low() {
        // **A composition of captured bytes, not a capture.** The real
        // `--console=rich` run is truncated immediately before its final
        // whole-line tally, which is the shape a green rich-console run has:
        // colour, per-test lines glued onto the progress bar, and no tally,
        // because a green task prints none. Counting the survivors would report
        // 8 of 9. The README convention is that a capture and a constructed
        // shape are never confused, so this says which it is.
        let full = fixture("gradle-ansi-color.txt");
        let at = full
            .rfind("9 tests completed")
            .expect("the final tally is present");
        let truncated = &full[..at];
        let reason = match parse(truncated) {
            RunnerOutcome::Unverifiable { reason, .. } => reason,
            RunnerOutcome::Parsed(p) => panic!("counted {p:?} from a coloured run with no tally"),
        };
        assert!(reason.contains("floor and not a count"), "{reason}");
    }

    // --- the states that must NOT parse to a number -----------------------

    #[test]
    fn the_commonest_green_gradle_shape_has_no_number_in_it() {
        // No `testLogging` block, which is what most real builds have. The
        // task ran, everything passed, and the output carries no count at all.
        let text = fixture("gradle-default-logging.txt");
        assert!(text.contains("BUILD SUCCESSFUL") && !text.contains("tests completed"));
        let reason = why("gradle-default-logging.txt");
        assert!(reason.contains("no outcomes"), "{reason}");
    }

    #[test]
    fn an_up_to_date_task_did_not_run_and_says_nothing_about_this_tree() {
        let reason = why("gradle-up-to-date.txt");
        assert!(reason.contains("UP-TO-DATE"), "{reason}");
    }

    #[test]
    fn a_compile_error_is_unverifiable_and_not_zero_tests() {
        let reason = why("gradle-compile-error.txt");
        assert!(reason.contains("did not compile"), "{reason}");
    }

    #[test]
    fn a_wrapper_that_never_reached_a_build_is_unverifiable() {
        let reason = why("gradlew-bootstrap-failure.txt");
        assert!(reason.contains("never reached a build"), "{reason}");
    }

    #[test]
    fn a_green_count_under_a_failed_build_is_refused() {
        // **A composition of captured bytes, not a capture.** The green
        // `gradle-all-pass.txt` run, with the failure footer from the real
        // halted multi-project build appended. That is the exact shape a
        // two-module build produces when the green module runs first and the
        // failing module's task never starts: a clean count, `BUILD FAILED`,
        // and no trace of the module that did not run. Stage 1 could not
        // capture it — reproducing it means a build whose module order is the
        // reverse of the one captured — so the rule is proven against real
        // bytes assembled into that shape, and this comment says so.
        let green = fixture("gradle-all-pass.txt");
        let halted = fixture("gradle-multiproject-halted.txt");
        let footer = &halted[halted.find("FAILURE: Build failed").unwrap_or(0)..];
        let composed = format!("{green}\n{footer}");
        let reason = match parse(&composed) {
            RunnerOutcome::Unverifiable { reason, .. } => reason,
            RunnerOutcome::Parsed(p) => panic!("a green count under BUILD FAILED parsed to {p:?}"),
        };
        assert!(reason.contains("no test did"), "{reason}");
    }

    #[test]
    fn no_source_and_no_match_are_zero_test_runs_and_not_clean_ones() {
        let no_source = ok("gradle-zero-tests.txt");
        assert_eq!(
            (no_source.tests_total, no_source.passed, no_source.failed),
            (0, 0, 0)
        );
        // `--tests demo.CalculatorTest.doesNotExist`: the one case where the
        // filter surfaces in the output at all.
        let text = fixture("gradle-filter-no-match.txt");
        assert!(text.contains("No tests found for given includes"));
        let no_match = ok("gradle-filter-no-match.txt");
        assert_eq!((no_match.tests_total, no_match.failed), (0, 0));
    }

    // --- negative controls -------------------------------------------------

    #[test]
    fn output_from_another_java_runner_does_not_produce_a_gradle_number() {
        for name in [
            "maven-mixed.txt",
            "junit-console-mixed.txt",
            "testng-standalone-mixed.txt",
        ] {
            match parse(&fixture(name)) {
                RunnerOutcome::Unverifiable { .. } => {}
                RunnerOutcome::Parsed(p) => {
                    panic!("{name} is not Gradle output and parsed to {p:?}")
                }
            }
        }
    }

    #[test]
    fn a_task_line_is_not_a_test_result() {
        assert!(result_line("> Task :test FAILED").is_none());
        assert!(result_line("> Task :core:test FAILED").is_none());
        assert!(result_line("CalculatorTest > addsTwoNumbers() PASSED").is_some());
        assert!(result_line("OrderServiceTest > comparesGreaterThan PASSED").is_some());
    }
}
