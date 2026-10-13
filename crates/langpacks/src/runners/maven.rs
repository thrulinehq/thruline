//! §6.2 — Maven console output: Surefire and Failsafe, under the JUnit 5,
//! JUnit 4 and TestNG providers.
//!
//! Written against 21 captured `mvn` runs (Maven 3.9.16, Surefire 3.5.2,
//! JDK 17.0.19); `tests/fixtures/runners/README.md` names the command and the
//! exit code behind each one.
//!
//! # The one decision this file is built on: sum every `Results:` block
//!
//! Maven prints a summary per *execution*, not per invocation, and it never
//! aggregates them. Four captured shapes make that unavoidable:
//!
//! - `maven-verify-it-failure.txt` — `mvn verify` runs Surefire and then
//!   Failsafe, so the file holds `Tests run: 2, Failures: 0` and then
//!   `Tests run: 1, Failures: 1`. **A parser that takes the first match reads a
//!   clean run out of a failed build.**
//! - `maven-reactor-mixed.txt` — two modules, two summaries, un-summed, plus a
//!   Reactor Summary that names SUCCESS/FAILURE per module and carries no
//!   counts.
//! - `maven-parallel-reactor-mixed.txt` — the same with `-T 2`: two ` T E S T S`
//!   banners, no module prefix on any line, and the modules interleaved into
//!   one undifferentiated stream. Nothing in the bytes says which count belongs
//!   to which module; the sum is the only fact available, and it is the right
//!   one.
//! - `maven-flaky.txt` — one execution, but Surefire prints the class summary
//!   **twice** (the run and the rerun) and only the last block is the verdict.
//!
//! Summing needs the summary lines to be told from the per-class lines, and one
//! byte does it: a per-class line carries `, Time elapsed:` and a summary line
//! does not. That holds through `mvn -q` (`maven-quiet-mixed.txt`), where the
//! `[INFO] Results:` header is suppressed and the summary line survives alone,
//! so keying on the header would lose it.
//!
//! # Three shapes that are green in one channel and not in the other
//!
//! None of them is read from the exit code, because §0 principle 2 says the
//! evidence is what the process wrote:
//!
//! - `maven-failure-ignore.txt` — `-Dmaven.test.failure.ignore=true`:
//!   `BUILD SUCCESS`, exit 0, over an honest `Tests run: 6, Failures: 1,
//!   Errors: 1`. Parsed here as two failures, which is what happened.
//! - `maven-fail-never.txt` — `--fail-never`: the text says `BUILD FAILURE` and
//!   the process exits **0**. The only fixture in the directory where the two
//!   disagree, and neither is consulted.
//! - `maven-vm-crash.txt` — a test called `System.exit(1)`; Surefire prints a
//!   clean `Tests run: 0, Failures: 0, Errors: 0, Skipped: 0` at `[INFO]`. The
//!   runner itself supplies the zeroed shape §6.2 forbids returning, so this is
//!   refused by two independent routes — see [`parse`].
//!
//! # Two captured Java runners this crate does NOT parse, and why not
//!
//! Stage 1 captured six fixtures from two standalone launchers, and nothing
//! here reads them. That is a decision, not an oversight, and it has one
//! reason: **there is no name to put on the answer that would not be a lie.**
//!
//! - `testng-standalone-mixed.txt`, `testng-standalone-all-pass.txt` —
//!   `java -cp … org.testng.TestNG`, whose whole result is one line:
//!   `Total tests run: 6, Passes: 3, Failures: 2, Skips: 1`.
//! - `junit-console-mixed.txt`, `junit-console-ansi.txt`,
//!   `junit-console-zero-tests.txt`, `junit-console-discovery-error.txt` —
//!   `java -jar junit-platform-console-standalone-1.11.4.jar`, whose tally is a
//!   `[ N tests successful ]` box.
//!
//! Neither is Maven and neither is Gradle. `verify::parse::Runner` and
//! `super::PackRunner` have no variant for either, and `runners::mod`'s own
//! rule is explicit about what returning a near-miss variant would be:
//! *"That is a lie in a field a report prints, and it is the kind of lie this
//! product exists to catch."* The program that produces both is `java`, which
//! `thruline_capture::shim::WRAPPED` does not wrap, so neither is reachable
//! from capture either. The merge plan asks for the two variants; until they
//! exist, these six fixtures are used as **negative controls** — proof that
//! handing this parser another Java runner's output yields `unverifiable`
//! rather than a Maven-labelled number — and that is all they are used for.
//!
//! `testng-standalone-all-pass.txt` is worth reading anyway. It is a run
//! narrowed with `-methods` to three passing methods of a class whose full run
//! fails, and its 380 bytes are indistinguishable from an honest green suite.
//! Nothing in the output can reveal that; the narrowing is an argv fact, and
//! `crate::narrowing` is where it is read.

use std::collections::BTreeSet;

use thruline_verify::parse::strip_ansi;

use super::{parsed, unverifiable, Runner, RunnerOutcome};

/// Recognise `mvn` from an argv.
///
/// # The grammar differs from `verify::parse::Runner::from_argv`'s, in three
/// ways that each change a verdict
///
/// `verify` asks `argv.iter().skip(1).any(|a| a == "test" || a == "verify")`.
///
/// 1. **It misses the lifecycle phases that run tests.** Surefire binds to the
///    `test` phase, and every phase after it runs the phases before it, so
///    `mvn package`, `mvn install` and `mvn deploy` all execute the suite.
///    `verify` reports `None` for each, and a real test run with no runner
///    attached is reported by §6.3 as *"no test check was captured"* — the
///    agent looks like it ran nothing.
/// 2. **It reads an option's value as a phase.** `mvn -pl test install`, where
///    `test` is a module name, matches `any(|a| a == "test")`. So does
///    `mvn -Dtest=Foo somethingelse`? — no, that one is attached — but `-pl`,
///    `-P`, `-f`, `-T` and `-rf` all take a separate word, and any of them
///    holding the string `test` or `verify` makes an arbitrary command look
///    like a test run.
/// 3. **It ignores the skip flags.** `mvn -DskipTests install` and
///    `mvn -Dmaven.test.skip=true package` compile and run **nothing**, and
///    both are recognised as test runs. That is the expensive direction: a
///    check is recorded, its output has no `Tests run:` line, and the claim
///    comes back `unverifiable` with a hint about enabling a report that was
///    never the problem.
///
/// Everything else is `verify`'s: `argv[0]` is matched on its basename, and no
/// arm exists for a program `thruline_capture::shim::WRAPPED` does not wrap —
/// `mvn` is on that list, `mvnw` is not, and `./mvnw` is a path that never
/// consults PATH at all.
pub fn from_argv(argv: &[String]) -> Option<Runner> {
    let program = argv.first()?;
    let base = program.rsplit('/').next().unwrap_or(program);
    if base != "mvn" {
        return None;
    }

    let mut goals = Vec::new();
    let mut at = 1usize;
    while let Some(arg) = argv.get(at) {
        at += 1;
        if skips_the_tests(arg) {
            return None;
        }
        if arg.starts_with('-') {
            if MAVEN_VALUE_OPTIONS.contains(&arg.as_str()) {
                at += 1;
            }
            continue;
        }
        goals.push(arg.as_str());
    }
    goals.iter().any(|g| runs_tests(g)).then_some(Runner::Maven)
}

/// Maven options whose value travels in a **separate word**.
///
/// `-D…` and `-P…` may also be written attached, which needs no entry: the
/// point of the list is that `-pl core` must not leave `core` standing where a
/// goal belongs. Anything missing here costs recall — an extra bare word is
/// read as a goal, which at worst recognises a test run that is one — never
/// precision about the skip flags, which are checked before this.
const MAVEN_VALUE_OPTIONS: &[&str] = &[
    "-f",
    "--file",
    "-pl",
    "--projects",
    "-P",
    "--activate-profiles",
    "-s",
    "--settings",
    "-gs",
    "--global-settings",
    "-t",
    "--toolchains",
    "-gt",
    "--global-toolchains",
    "-T",
    "--threads",
    "-b",
    "--builder",
    "-l",
    "--log-file",
    "-rf",
    "--resume-from",
    "-D",
    "-emp",
    "-ep",
];

/// Does this argument switch the tests off entirely?
///
/// `-DskipTests`, `-DskipTests=true`, `-Dmaven.test.skip`,
/// `-Dmaven.test.skip=true`, `-Dskip.surefire.tests=true`. The `=false`
/// spellings are the explicit opposite and must not match, which is why this is
/// not a `contains`.
fn skips_the_tests(arg: &str) -> bool {
    let Some(property) = arg.strip_prefix("-D") else {
        return false;
    };
    let (name, value) = match property.split_once('=') {
        Some((n, v)) => (n, v),
        None => (property, "true"),
    };
    matches!(
        name,
        "skipTests" | "maven.test.skip" | "skipITs" | "skip.surefire.tests"
    ) && value.eq_ignore_ascii_case("true")
}

/// Does this goal or phase execute the test suite?
///
/// The phases are ordered, so anything at or after `test` runs it. The plugin
/// goals are here because a build script that pins `surefire:test` skips the
/// lifecycle entirely and is still a test run.
fn runs_tests(goal: &str) -> bool {
    matches!(
        goal,
        "test" | "verify" | "package" | "install" | "deploy" | "integration-test"
    ) || goal.ends_with(":test")
        || goal.ends_with(":integration-test")
        || goal.ends_with(":verify")
}

/// Parse Maven console output.
pub fn parse(output: &str) -> RunnerOutcome {
    // ANSI first, per §6.2. `maven-ansi-color.txt` was captured with
    // `-Dstyle.color=always` and no `-B` (batch mode disables colour on its
    // own); the SGR sequences fall **between** `Tests` and `run:` on every
    // failing per-class line, so the literal substring `Tests run:` does not
    // occur in the raw bytes at all.
    let plain = strip_ansi(output);
    let text: &str = &plain;

    let mut p = parsed(Runner::Maven);
    let mut summaries = 0usize;
    let mut flakes = 0u64;
    let mut classes_announced = 0usize;
    let mut classes_reported = 0usize;
    let mut seconds = 0.0f64;
    let mut ids = BTreeSet::new();

    for line in text.lines() {
        if let Some(rest) = after_marker(line, "Running ") {
            if !rest.trim().is_empty() {
                classes_announced += 1;
            }
            continue;
        }
        if let Some(id) = failing_test_id(line) {
            ids.insert(id);
            continue;
        }
        let Some(counts) = tally(line) else {
            continue;
        };
        if counts.per_class {
            classes_reported += 1;
            seconds += counts.seconds;
            continue;
        }
        summaries += 1;
        p.tests_total += counts.total;
        p.failed += counts.failures + counts.errors;
        p.skipped += counts.skipped;
        flakes += counts.flakes;
    }

    // --- the states that are not a tally ----------------------------------

    if text.contains("COMPILATION ERROR") || text.contains("Compilation failure") {
        return unverifiable(
            "the test sources did not compile, so no test in this module ran",
            "fix the compile error and re-run; `mvn` reports `BUILD FAILURE` for a \
             compile error and for a test failure with the same exit code, and only the \
             `COMPILATION ERROR` banner tells them apart",
        );
    }

    // A forked JVM that dies takes its results with it, and Surefire then
    // prints a summary of the tests it managed to collect — which is none.
    // `maven-vm-crash.txt` really does carry
    // `[INFO] Tests run: 0, Failures: 0, Errors: 0, Skipped: 0` under
    // `BUILD FAILURE`: the zeroed shape is supplied by the runner, not by a
    // defaulting parser, and it reads downstream as a clean run of nothing.
    //
    // Two routes, because either alone can be absent: Surefire's own words, and
    // the structural fact that a class was announced with `Running demo.X` and
    // never reported a result.
    let crashed = text.contains("The forked VM terminated without properly saying goodbye")
        || text.contains("Crashed tests:")
        || text.contains("Error occurred in starting fork");
    if crashed || (classes_announced > classes_reported && summaries > 0 && p.tests_total == 0) {
        return unverifiable(
            format!(
                "the forked test JVM did not report its results: {classes_announced} class(es) \
                 were announced with `Running …` and {classes_reported} reported a result, and \
                 the summary Surefire printed for them is a zeroed \
                 `Tests run: 0, Failures: 0, Errors: 0, Skipped: 0`"
            ),
            "a `System.exit` inside a test, an OOM-killed fork or a crashing native library \
             all do this; read `target/surefire-reports/*.dumpstream`. The zeroed summary is \
             the runner's, not this parser's, and it is not a passing run",
        );
    }

    if summaries == 0 {
        // A genuine zero-test run, told apart from an unreadable one. Surefire
        // says so in as many words and prints no `Tests run:` line anywhere.
        // Returned as `Parsed` with a total of zero rather than as
        // `Unverifiable`, following `verify::parse::parse_pytest`, which admits
        // `no tests ran` as a summary for the same reason: the output was read,
        // and "zero tests ran" is the answer. §6.3 then reports *"the run
        // reported zero tests; a suite that ran nothing proves nothing"*, which
        // is more precise than "could not read the output".
        if text.contains("No tests to run.") {
            return RunnerOutcome::Parsed(p);
        }
        return unverifiable(
            if text.trim().is_empty() {
                "the output is empty: no `Tests run:` summary, and nothing else either"
            } else {
                "no `Tests run: …` summary line is present in the output"
            },
            "Surefire prints `Tests run: N, Failures: N, Errors: N, Skipped: N` once per \
             execution, and `mvn -q` prints it without the `[INFO] Results:` header. Zero \
             bytes of output is what `mvn -q` produces for a fully green run \
             (`maven-quiet-all-pass.txt` is 0 bytes) — indistinguishable from a command \
             that never ran, so it is refused rather than guessed",
        );
    }

    // A rerun rescued a failure, and the summary reports the rescue rather than
    // the failure: `maven-flaky.txt` prints `Failures: 1 <<< FAILURE!` on the
    // class line and `Failures: 0, …, Flakes: 1` in the summary, under
    // `BUILD SUCCESS` and exit 0.
    //
    // This is refused rather than parsed, and the reason is that the question
    // §6.3 asks — did the suite pass? — has no answer here. It passed on the
    // second attempt. `Parsed` has no field for that, so reporting
    // `failed: 0` would say the suite passed, which is how
    // `<rerunFailingTestsCount>` becomes a way to make a failing test green.
    // The merge plan asks for a `flaky` field; until there is one, silence is
    // the only honest answer.
    if flakes > 0 {
        return unverifiable(
            format!(
                "Surefire reports {flakes} flaky test(s): the run summary says \
                 `Failures: 0, …, Flakes: {flakes}` because a rerun passed what the first \
                 attempt failed"
            ),
            "`<rerunFailingTestsCount>` makes a failing test green on a later attempt; the \
             suite did not pass on the tree as given. Re-run without reruns to find out \
             whether it passes, or fix the flake",
        );
    }

    p.passed = p.tests_total.saturating_sub(p.failed + p.skipped);
    p.test_ids = ids;
    p.duration_ms = Some((seconds * 1000.0).round() as u64);
    RunnerOutcome::Parsed(p)
}

/// One `Tests run:` line, taken apart.
struct Tally {
    total: u64,
    failures: u64,
    errors: u64,
    skipped: u64,
    flakes: u64,
    seconds: f64,
    /// A per-class line, not an execution summary. The two are told apart by
    /// `, Time elapsed:`, which only the per-class form carries — including
    /// under `mvn -q`, where the `Results:` header that would otherwise mark
    /// the summary is suppressed.
    per_class: bool,
}

fn tally(line: &str) -> Option<Tally> {
    let at = line.find("Tests run:")?;
    let body = &line[at..];
    let mut t = Tally {
        total: 0,
        failures: 0,
        errors: 0,
        skipped: 0,
        flakes: 0,
        seconds: 0.0,
        per_class: body.contains("Time elapsed:"),
    };
    for field in body.split(',') {
        let Some((name, value)) = field.split_once(':') else {
            continue;
        };
        let name = name.trim();
        let value = value.trim();
        let number = value
            .split_whitespace()
            .next()
            .and_then(|n| n.parse::<u64>().ok());
        match (name, number) {
            ("Tests run", Some(n)) => t.total = n,
            ("Failures", Some(n)) => t.failures = n,
            ("Errors", Some(n)) => t.errors = n,
            ("Skipped", Some(n)) => t.skipped = n,
            ("Flakes", Some(n)) => t.flakes = n,
            ("Time elapsed", _) => {
                t.seconds = value
                    .split_whitespace()
                    .next()
                    .and_then(|n| n.parse::<f64>().ok())
                    .unwrap_or(0.0);
            }
            _ => {}
        }
    }
    Some(t)
}

/// The text after a Maven log-level prefix and a marker, if the line is one.
///
/// `[INFO] Running demo.CalculatorTest` and a bare `Running demo.X` both count;
/// a line that merely mentions the marker in prose does not, because the marker
/// has to sit at the start of the message.
fn after_marker<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let message = line.trim_start();
    let message = match message.starts_with('[') {
        true => message.split_once(']')?.1.trim_start(),
        false => message,
    };
    message.strip_prefix(marker)
}

/// The fully-qualified test named by a per-test failure line.
///
/// `[ERROR] demo.OrderServiceTest.looksUpAMissingCustomer -- Time elapsed: 0.005 s <<< ERROR!`
///
/// The per-*class* line has the same tail and begins `Tests run:`, so it is
/// excluded explicitly rather than by hoping the shapes differ.
fn failing_test_id(line: &str) -> Option<String> {
    if !(line.contains("<<< FAILURE!") || line.contains("<<< ERROR!")) {
        return None;
    }
    let message = after_any_prefix(line);
    if message.starts_with("Tests run:") {
        return None;
    }
    let id = message.split(" -- ").next()?.trim();
    (id.contains('.') && !id.contains(' ')).then(|| id.to_string())
}

fn after_any_prefix(line: &str) -> &str {
    let message = line.trim_start();
    match message.starts_with('[') {
        true => message
            .split_once(']')
            .map(|(_, r)| r.trim_start())
            .unwrap_or(message),
        false => message,
    }
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
    fn argv_grammar_including_the_three_verify_gets_wrong() {
        assert_eq!(from_argv(&argv("mvn test")), Some(Runner::Maven));
        assert_eq!(from_argv(&argv("mvn -B test")), Some(Runner::Maven));
        assert_eq!(from_argv(&argv("mvn clean verify")), Some(Runner::Maven));
        assert_eq!(
            from_argv(&argv("/opt/homebrew/bin/mvn -B -Dtest=CalculatorTest test")),
            Some(Runner::Maven)
        );
        assert_eq!(from_argv(&argv("mvn -T 2 test")), Some(Runner::Maven));
        assert_eq!(from_argv(&argv("mvn surefire:test")), Some(Runner::Maven));

        // 1. The lifecycle phases after `test`. `verify` returns None for all
        //    three, and each of them runs the suite.
        assert_eq!(from_argv(&argv("mvn package")), Some(Runner::Maven));
        assert_eq!(from_argv(&argv("mvn clean install")), Some(Runner::Maven));
        assert_eq!(from_argv(&argv("mvn deploy")), Some(Runner::Maven));

        // 2. A value that happens to read as a phase. `verify`'s `any()` sees
        //    the module named `test` and reports a test run.
        assert_eq!(from_argv(&argv("mvn -pl test compile")), None);
        assert_eq!(from_argv(&argv("mvn -rf verify clean")), None);

        // 3. The skip flags, which turn the suite off.
        assert_eq!(from_argv(&argv("mvn -DskipTests install")), None);
        assert_eq!(from_argv(&argv("mvn -Dmaven.test.skip=true package")), None);
        assert_eq!(
            from_argv(&argv("mvn -DskipTests=false test")),
            Some(Runner::Maven),
            "the explicit opposite is not a skip"
        );

        assert_eq!(from_argv(&argv("mvn compile")), None);
        assert_eq!(from_argv(&argv("gradle test")), None);
        assert_eq!(from_argv(&[]), None);
    }

    // --- the states that must parse ---------------------------------------

    #[test]
    fn all_pass() {
        let p = ok("maven-all-pass.txt");
        assert_eq!((p.tests_total, p.passed, p.failed, p.skipped), (3, 3, 0, 0));
        assert!(p.duration_ms.is_some_and(|ms| ms > 0));
    }

    #[test]
    fn mixed_counts_an_error_and_a_failure_as_two_failures() {
        let p = ok("maven-mixed.txt");
        assert_eq!((p.tests_total, p.passed, p.failed), (6, 4, 2));
        assert!(p
            .test_ids
            .contains("demo.OrderServiceTest.looksUpAMissingCustomer"));
        assert!(p
            .test_ids
            .contains("demo.OrderServiceTest.appliesDiscountToLargeOrders"));
    }

    #[test]
    fn skipped_is_surefires_one_word_for_disabled_and_for_a_failed_assumption() {
        let p = ok("maven-skipped.txt");
        assert_eq!((p.tests_total, p.passed, p.skipped, p.failed), (3, 1, 2, 0));
    }

    #[test]
    fn narrowed_is_fully_green_and_the_output_never_says_so() {
        // `-Dtest=CalculatorTest` over the project whose unfiltered run fails.
        // Nothing in the bytes distinguishes it from a full green run — the
        // narrowing is an argv fact, which is why `crate::narrowing` reads it
        // from the argv and this parser does not try.
        let p = ok("maven-narrowed.txt");
        assert_eq!((p.tests_total, p.passed, p.failed), (3, 3, 0));
        assert!(!fixture("maven-narrowed.txt").contains("-Dtest"));
    }

    #[test]
    fn quiet_mode_has_no_results_header_and_still_parses() {
        // `mvn -q` suppresses `[INFO] Results:`, so a parser keyed on the
        // header finds no summary. The `, Time elapsed:` discriminator does
        // not need it.
        let text = fixture("maven-quiet-mixed.txt");
        assert!(!text.contains("Results:"));
        let p = ok("maven-quiet-mixed.txt");
        assert_eq!((p.tests_total, p.passed, p.failed), (6, 4, 2));
    }

    #[test]
    fn colour_splits_the_summary_words_and_stripping_puts_them_back() {
        let text = fixture("maven-ansi-color.txt");
        assert!(text.contains('\u{1b}'), "the fixture really is coloured");
        assert!(
            !text.contains("[ERROR] Tests run: 3, Failures: 1"),
            "the SGR codes fall between `Tests` and `run:` on the failing class line"
        );
        let p = ok("maven-ansi-color.txt");
        assert_eq!((p.tests_total, p.passed, p.failed), (6, 4, 2));
    }

    #[test]
    fn a_cold_cache_buries_the_summary_under_three_hundred_download_lines() {
        // Every one of those lines carries digits and units. None of them
        // contains `Tests run:`.
        let text = fixture("maven-cold-cache.txt");
        assert!(text.matches("Downloading").count() > 100);
        let p = ok("maven-cold-cache.txt");
        assert_eq!((p.tests_total, p.passed, p.failed), (3, 3, 0));
    }

    // --- the summaries that must be summed --------------------------------

    #[test]
    fn mvn_verify_has_two_blocks_and_the_first_one_is_clean() {
        // Surefire's unit tests pass; Failsafe's `CheckoutIT` fails. First-match
        // parsing reads `Tests run: 2, Failures: 0` and reports a clean run out
        // of a failed build.
        let text = fixture("maven-verify-it-failure.txt");
        assert_eq!(text.matches("[INFO] Results:").count(), 2);
        let p = ok("maven-verify-it-failure.txt");
        assert_eq!((p.tests_total, p.passed, p.failed), (3, 2, 1));
    }

    #[test]
    fn a_reactor_prints_one_summary_per_module_and_sums_none_of_them() {
        let p = ok("maven-reactor-mixed.txt");
        assert_eq!((p.tests_total, p.passed, p.failed), (6, 4, 2));
    }

    #[test]
    fn a_parallel_reactor_interleaves_with_no_module_prefix_on_any_line() {
        // `-T 2`: two ` T E S T S` banners, both modules' `Running` lines in one
        // stream, and two separate un-summed `Results:` blocks. Module identity
        // is simply not in the bytes; the sum is.
        let text = fixture("maven-parallel-reactor-mixed.txt");
        assert_eq!(text.matches(" T E S T S").count(), 2);
        let p = ok("maven-parallel-reactor-mixed.txt");
        assert_eq!((p.tests_total, p.passed, p.failed), (6, 4, 2));
    }

    #[test]
    fn forked_parallel_classes_return_out_of_order_and_the_summary_is_still_last() {
        // `forkCount=2, reuseForks=false`: two `Running demo.XTest` lines
        // precede any result and the per-class results come back out of source
        // order. The summary is single and correct.
        let p = ok("maven-parallel-forks-mixed.txt");
        assert_eq!((p.tests_total, p.passed, p.failed), (8, 7, 1));
    }

    // --- the two benign-looking verdicts ----------------------------------

    #[test]
    fn failure_ignore_is_green_in_both_channels_and_the_output_is_not() {
        // `BUILD SUCCESS`, exit 0, `Tests run: 6, Failures: 1, Errors: 1`.
        let text = fixture("maven-failure-ignore.txt");
        assert!(text.contains("BUILD SUCCESS"));
        let p = ok("maven-failure-ignore.txt");
        assert_eq!((p.tests_total, p.failed), (6, 2));
    }

    #[test]
    fn fail_never_makes_the_text_and_the_exit_code_disagree() {
        // The file says `BUILD FAILURE`; the process exited 0. Neither is read.
        let text = fixture("maven-fail-never.txt");
        assert!(text.contains("BUILD FAILURE"));
        let p = ok("maven-fail-never.txt");
        assert_eq!((p.tests_total, p.failed), (6, 2));
    }

    // --- the states that must NOT parse to a number -----------------------

    #[test]
    fn a_compile_error_is_unverifiable_and_not_zero_tests() {
        let reason = why("maven-compile-error.txt");
        assert!(reason.contains("did not compile"), "{reason}");
    }

    #[test]
    fn a_dead_fork_is_refused_even_though_surefire_printed_a_clean_summary() {
        // The trap in one line: the fixture really does contain a zeroed
        // `Tests run: 0, Failures: 0, Errors: 0, Skipped: 0` at `[INFO]`.
        let text = fixture("maven-vm-crash.txt");
        assert!(text.contains("[INFO] Tests run: 0, Failures: 0, Errors: 0, Skipped: 0"));
        let reason = why("maven-vm-crash.txt");
        assert!(reason.contains("did not report its results"), "{reason}");
    }

    #[test]
    fn a_rerun_rescued_failure_is_refused_rather_than_reported_as_green() {
        let text = fixture("maven-flaky.txt");
        assert!(text.contains("Flakes: 1") && text.contains("BUILD SUCCESS"));
        let reason = why("maven-flaky.txt");
        assert!(reason.contains("flaky"), "{reason}");
    }

    #[test]
    fn zero_bytes_of_output_is_refused_rather_than_read_as_green() {
        // `mvn -q` on a fully green project writes nothing at all —
        // indistinguishable from a command that never ran.
        assert_eq!(fixture("maven-quiet-all-pass.txt").len(), 0);
        let reason = why("maven-quiet-all-pass.txt");
        assert!(reason.contains("empty"), "{reason}");
    }

    #[test]
    fn a_project_with_no_tests_parses_to_zero_and_is_not_a_clean_run() {
        // `[INFO] No tests to run.` and `BUILD SUCCESS`, with no `Tests run:`
        // line anywhere. Read, not guessed: the total is zero, and §6.3 says
        // what a zero-test run proves.
        let text = fixture("maven-zero-tests.txt");
        assert!(!text.contains("Tests run:"));
        let p = ok("maven-zero-tests.txt");
        assert_eq!((p.tests_total, p.passed, p.failed), (0, 0, 0));
    }

    // --- the other providers ----------------------------------------------

    #[test]
    fn junit4_under_surefire() {
        // 7 tests: 3 pass, 1 AssertionError, 1 NullPointerException, 1 @Ignore,
        // 1 failed assumeTrue. Surefire counts the last two as Skipped.
        let p = ok("maven-junit4-mixed.txt");
        assert_eq!((p.tests_total, p.passed, p.failed, p.skipped), (7, 3, 2, 2));
        assert!(p
            .test_ids
            .contains("demo.OrderServiceTest.appliesTheBulkDiscount"));
    }

    #[test]
    fn testng_under_surefire_reports_a_skipexception_as_skipped() {
        // TestNG has no error/failure distinction, so the escaped
        // NullPointerException lands in `Failures: 2` rather than in `Errors:`.
        let p = ok("maven-testng-mixed.txt");
        assert_eq!((p.tests_total, p.passed, p.failed, p.skipped), (6, 3, 2, 1));
    }

    // --- negative controls: output from a runner this parser is not --------

    #[test]
    fn output_from_another_java_runner_does_not_produce_a_maven_number() {
        // Neither of these is Maven output. Both name tests and carry counts, so
        // a loose parser would report a tally with `Runner::Maven` on it — a
        // wrong label on a real number, which `runners::mod` calls the kind of
        // lie this product exists to catch.
        for name in [
            "gradle-mixed.txt",
            "junit-console-mixed.txt",
            "testng-standalone-mixed.txt",
        ] {
            match parse(&fixture(name)) {
                RunnerOutcome::Unverifiable { .. } => {}
                RunnerOutcome::Parsed(p) => {
                    panic!("{name} is not Maven output and parsed to {p:?}")
                }
            }
        }
    }
}
