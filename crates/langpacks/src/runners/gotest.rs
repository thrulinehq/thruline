//! §6.2 — `go test`, console (`-v` and bare) and `-json`.
//!
//! # This replaces a parser written from documentation, and every claim in it
//! was re-checked against a toolchain
//!
//! `crates/verify::parse::parse_go_test` was written with no Go on the machine
//! — its own fixture README says so. With `go1.27.1` installed, four of its
//! behaviours are wrong on captured bytes, and all four fail in the one
//! direction §6.2 forbids: they turn *I could not tell* into *it passed*.
//! `tests/fixtures/runners/README.md` names the captures; `MERGE-PLAN` carries
//! the defects. They are, in order of how badly they mislead:
//!
//! 1. **A package that never compiled reads as a clean run.**
//!    `gotest-build-failed-verbose.txt` is one stream carrying
//!    `FAIL\t…/broken [build failed]` *and* `--- PASS: TestTwo` from a
//!    different package. Tallying `--- ` lines gives `{passed: 1, failed: 0}`.
//!    The `-json` form has the same trap through a different door:
//!    `"Action":"build-fail"` objects carry an `ImportPath` and no `Test` key,
//!    so the `!t.contains("\"Test\"")` guard skips them.
//! 2. **A timeout reads as a clean run.** `gotest-timeout.txt` has
//!    `--- PASS: TestQuick` and then `panic: test timed out after 5s` with no
//!    result line at all for the hung test: `{passed: 1, failed: 0}`, exit 1.
//! 3. **Subtests are double-counted.** `verify` trims the line before
//!    `strip_prefix("--- ")`, so the indented `    --- FAIL: TestUpper/q` is
//!    tallied alongside its own parent's `--- FAIL: TestUpper`.
//! 4. **`ok` is read as a verdict.** It is not one:
//!    `gotest-benign-summary.txt` has a `--- FAIL` line, a package line reading
//!    `ok\texample.com/capture/benign\t0.478s`, and exit code 0, because
//!    `TestMain` called `os.Exit(0)` after `m.Run()`.
//!
//! # What "one test" means here, and why the two forms had to agree
//!
//! A subtest is a test to the runner: `t.Run` produces its own `--- PASS:
//! TestX/sub` line and its own `-json` `run`/`pass` events, *and* its parent
//! still reports its own result. Counting both double-counts every subtest;
//! counting only subtests loses every test that has none. **Only top-level
//! tests are counted**, in both forms — an unindented `--- ` line in the
//! console, a `Test` name without a `/` in the JSON — so the same run parsed
//! from `-v` and from `-json` yields the same numbers. A failing subtest always
//! fails its parent, so no failure is lost by this; skipped subtests under a
//! passing parent are, and that is named rather than hidden. Subtest names are
//! still recorded in `test_ids`, so `test_ids.len()` is deliberately larger
//! than `tests_total` for a suite with subtests.

use std::collections::BTreeSet;

use thruline_verify::parse::strip_ansi;

use super::{parsed, unverifiable, Runner, RunnerOutcome};

/// Recognise `go test` from an argv.
///
/// **The grammar differs from `verify::parse::Runner::from_argv`'s in one
/// real way**, and it is not cosmetic. `verify` takes the first argument that
/// does not begin with `-` and requires it to be `test`. Since Go 1.20 the
/// `go` command accepts a global `-C <dir>` *before* the subcommand
/// (`go -C ./service test ./...`, verified against go1.27.1 on this machine),
/// and its value is a bare word — so `verify` reads `./service` as the
/// subcommand and returns `None`. A real test run then arrives with no runner
/// attached, which §6.3 reports as "no test check was captured": the agent
/// looks like it ran nothing.
///
/// Everything else is `verify`'s: `argv[0]` is matched on its basename, and no
/// arm is added for a program the capture shim does not wrap. `gotestsum` is
/// deliberately absent — it is not installed here, so no fixture proves its
/// output, and an arm with no capture behind it is coverage that does not
/// exist.
pub fn from_argv(argv: &[String]) -> Option<Runner> {
    let program = argv.first()?;
    let base = program.rsplit('/').next().unwrap_or(program);
    if base != "go" {
        return None;
    }
    let mut at = 1usize;
    while let Some(arg) = argv.get(at) {
        at += 1;
        // `-C dir` is the only global flag whose value travels in a separate
        // word. Consuming it is what keeps the directory from being read as
        // the subcommand.
        if arg == "-C" {
            at += 1;
            continue;
        }
        if arg.starts_with('-') {
            continue;
        }
        return (arg == "test").then_some(Runner::GoTest);
    }
    None
}

/// Parse `go test` output, in whichever of its two forms this is.
pub fn parse(output: &str) -> RunnerOutcome {
    // ANSI first, per §6.2 — but with a Go-specific step in front of it.
    //
    // **The `testing` package doubles the ESC byte and `fmt` does not.**
    // Measured against the compiled test binary, not the `go` command:
    // `t.Logf("\x1b[32m…")` reaches the stream as `1b 1b 5b 33 32 6d` while a
    // `fmt.Println` from the same test arrives as `1b 5b 33 32 6d`.
    // `strip_ansi` reads the first ESC of the pair as a two-character escape,
    // consumes the second, and then emits `[32m` as literal text — so the
    // sequence is not removed, it is turned into visible garbage.
    // `gotest-ansi-mixed-escapes.txt` carries both encodings in one stream.
    let plain = strip_ansi(&collapse_doubled_escapes(output));
    if is_json_stream(&plain) {
        parse_json(&plain)
    } else {
        parse_console(&plain)
    }
}

/// Collapse a run of ESC bytes down to one, so `strip_ansi` sees a CSI.
fn collapse_doubled_escapes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_was_esc = false;
    for c in text.chars() {
        if c == '\u{1b}' {
            if last_was_esc {
                continue;
            }
            last_was_esc = true;
        } else {
            last_was_esc = false;
        }
        out.push(c);
    }
    out
}

/// Is this the `-json` event stream?
///
/// Decided by finding at least one line that actually **parses** as an event
/// with an `Action`, not by looking for a `{`. A test that prints JSON of its
/// own is the obvious way to get this wrong, and a test that prints a *test
/// event* is not a case any parser can defend against — noted, not solved.
fn is_json_stream(output: &str) -> bool {
    output
        .lines()
        .filter_map(event)
        .any(|e| !e.action.is_empty())
}

/// One `go test -json` event. Only the fields that decide an outcome.
#[derive(serde::Deserialize)]
struct Event {
    #[serde(rename = "Action")]
    action: String,
    #[serde(rename = "Test")]
    test: Option<String>,
    #[serde(rename = "Elapsed")]
    elapsed: Option<f64>,
}

fn event(line: &str) -> Option<Event> {
    let line = line.trim();
    if !line.starts_with('{') {
        return None;
    }
    serde_json::from_str::<Event>(line).ok()
}

/// `go test -json`.
fn parse_json(output: &str) -> RunnerOutcome {
    let mut p = parsed(Runner::GoTest);
    let mut package_failed = false;
    let mut build_failed = false;
    let mut seconds = 0.0f64;
    let mut subtests = BTreeSet::new();

    for line in output.lines() {
        let Some(e) = event(line) else {
            // `go: downloading …` and anything else the shell interleaved.
            // Ignored rather than treated as a parse failure — the event
            // stream is the evidence and it is still intact.
            continue;
        };
        // A build failure carries an `ImportPath` and NO `Test` key, which is
        // exactly why `verify`'s `"Test"`-presence guard steps over it.
        if e.action == "build-fail" {
            build_failed = true;
            continue;
        }
        match (e.action.as_str(), e.test.as_deref()) {
            // Package-level result: no `Test` key. Never a test.
            ("pass" | "fail" | "skip", None) => {
                if e.action == "fail" {
                    package_failed = true;
                }
                seconds += e.elapsed.unwrap_or(0.0);
            }
            ("pass" | "fail" | "skip", Some(name)) => {
                if name.contains('/') {
                    // A subtest. Named, not counted — see the module docs.
                    subtests.insert(name.to_string());
                    continue;
                }
                p.test_ids.insert(name.to_string());
                match e.action.as_str() {
                    "pass" => p.passed += 1,
                    "fail" => p.failed += 1,
                    _ => p.skipped += 1,
                }
            }
            _ => {}
        }
    }

    if build_failed {
        return unverifiable(
            "the `-json` stream carries a `build-fail` event: at least one package did not \
             compile, so its tests never ran",
            "fix the compile error and re-run; the test events from the packages that DID \
             build describe only those packages, and `go test` exits non-zero for the whole \
             invocation",
        );
    }
    if p.test_ids.is_empty() {
        return unverifiable(
            "the `-json` stream carries no test-level pass/fail/skip events — every event \
             in it is package-level",
            "a package with no `_test.go` files emits a package-level `skip` and no test \
             events, and `-run` that matches nothing emits none either; a run that produced \
             no test outcomes is not a passing run",
        );
    }
    if package_failed && p.failed == 0 {
        return unverifiable(
            "a package reported `fail` while no individual test did — the failure is \
             outside the tests the stream names",
            "a `TestMain` that returns non-zero, a panic in an init function or a timeout \
             all fail a package without failing a named test; read the package's `output` \
             events for the cause",
        );
    }

    p.tests_total = p.passed + p.failed + p.skipped;
    p.test_ids.extend(subtests);
    p.duration_ms = Some((seconds * 1000.0).round() as u64);
    RunnerOutcome::Parsed(p)
}

/// `go test`, console form — both `-v` and the summary-only default.
fn parse_console(output: &str) -> RunnerOutcome {
    let mut p = parsed(Runner::GoTest);
    let mut package_failed = false;
    let mut build_failed = Vec::new();
    let mut timed_out = false;
    let mut panicked = false;
    let mut zero_test_packages = 0usize;
    let mut seconds = 0.0f64;
    let mut subtests = BTreeSet::new();

    for line in output.lines() {
        // A test binary can be killed by the timeout watchdog, which prints a
        // panic and no result line for the test that hung. The tally that
        // survives is a *prefix* of the run.
        if line.starts_with("panic: test timed out after") {
            timed_out = true;
            continue;
        }
        if line.starts_with("panic:") || line.starts_with("fatal error:") {
            panicked = true;
            continue;
        }
        if line.starts_with("FAIL")
            && (line.contains("[build failed]") || line.contains("[setup failed]"))
        {
            build_failed.push(package_of(line));
            continue;
        }
        if line.starts_with('?') && line.contains("[no test files]") {
            zero_test_packages += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix("ok  ") {
            seconds += package_seconds(rest);
            if rest.contains("[no tests to run]") {
                zero_test_packages += 1;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("FAIL\t") {
            package_failed = true;
            seconds += package_seconds(rest);
            continue;
        }

        // A per-test result line.
        //
        // The segment after the LAST tab, not the trimmed line. Two captured
        // facts force each half of that:
        //
        // - Subtest results are indented with **spaces**, so a trimmed line
        //   makes `    --- FAIL: TestUpper/q` indistinguishable from its
        //   parent's `--- FAIL: TestUpper` — `verify`'s double-count.
        // - `go test` writes concurrent packages to one stream, and
        //   `gotest-cover.txt` contains
        //   `\texample.com/capture/notests\t\t--- FAIL: TestUpper (0.00s)`,
        //   where a package line and a result line collided. That is not a
        //   truncation; it is what the process wrote, and taking the text
        //   after the final tab reads the result out of it.
        let candidate = line.rsplit('\t').next().unwrap_or(line);
        let indented = candidate.starts_with(' ');
        let Some(rest) = candidate.trim_start().strip_prefix("--- ") else {
            continue;
        };
        let Some((verdict, name)) = rest.split_once(": ") else {
            continue;
        };
        let name = name.split_whitespace().next().unwrap_or("").to_string();
        if indented || name.contains('/') {
            subtests.insert(name);
            continue;
        }
        match verdict {
            "PASS" => p.passed += 1,
            "FAIL" => p.failed += 1,
            "SKIP" => p.skipped += 1,
            _ => continue,
        }
        p.test_ids.insert(name);
    }

    if !build_failed.is_empty() {
        return unverifiable(
            format!(
                "{} did not compile ([build failed] / [setup failed]), so no test in it ran",
                build_failed.join(", ")
            ),
            "a build failure and another package's `--- PASS` share one stream, so the \
             per-test tally describes only the packages that compiled; fix the compile or \
             vet error and re-run",
        );
    }
    if timed_out {
        return unverifiable(
            "the run was killed by `go test`'s timeout: `panic: test timed out` aborts the \
             binary, and the test that hung has no result line at all",
            "the results printed before the panic are a prefix of the suite, not a summary \
             of it — raise `-timeout`, or fix the hang, and re-run",
        );
    }
    if p.test_ids.is_empty() {
        let reason = if zero_test_packages > 0 || output.contains("no tests to run") {
            "the run matched no tests: every package reported `[no test files]` or \
             `[no tests to run]`"
        } else {
            "no per-test result lines are present in the output"
        };
        return unverifiable(
            reason,
            "run `go test -v` or `go test -json`; a bare `ok\tpackage` line reports no \
             counts, `ok` is printed for a package whose `-run` pattern matched nothing, \
             and a package that compiled is not a suite that ran",
        );
    }
    if panicked && p.failed == 0 {
        return unverifiable(
            "the test binary panicked and no test recorded a failure — the panic aborted \
             the run, so the tests after it never ran",
            "a panic outside a test (an `init`, a `TestMain`, or a goroutine) fails the \
             package without failing any named test; the counts above the panic are a \
             prefix of the suite",
        );
    }
    if package_failed && p.failed == 0 {
        return unverifiable(
            "a package line reads `FAIL` while no test result line does — the failure is \
             outside the tests the output names",
            "a `TestMain` returning non-zero, a `-race` report or output the parser could \
             not attribute all do this; re-run with `-json`, whose events attribute a \
             failure to a package explicitly",
        );
    }

    p.tests_total = p.passed + p.failed + p.skipped;
    p.test_ids.extend(subtests);
    p.duration_ms = Some((seconds * 1000.0).round() as u64);
    RunnerOutcome::Parsed(p)
}

/// The package named on a `FAIL\tpkg [build failed]` line.
fn package_of(line: &str) -> String {
    line.trim_start_matches("FAIL")
        .split_whitespace()
        .next()
        .unwrap_or("a package")
        .to_string()
}

/// The elapsed seconds on a package summary line, if it carries any.
///
/// `(cached)` carries none — a cache hit reports no elapsed time and no
/// per-test lines, which is why a fully cached green multi-package run parses
/// to `unverifiable` rather than to a clean tally.
fn package_seconds(rest: &str) -> f64 {
    rest.split('\t')
        .filter_map(|f| f.trim().strip_suffix('s'))
        .filter_map(|n| n.parse::<f64>().ok())
        .next_back()
        .unwrap_or(0.0)
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

    // --- argv --------------------------------------------------------------

    #[test]
    fn argv_grammar_including_the_one_verify_misreads() {
        let argv = |s: &str| s.split(' ').map(str::to_string).collect::<Vec<_>>();
        assert_eq!(from_argv(&argv("go test ./...")), Some(Runner::GoTest));
        assert_eq!(
            from_argv(&argv("/opt/homebrew/bin/go test -v -count=1 ./...")),
            Some(Runner::GoTest)
        );
        assert_eq!(
            from_argv(&argv("go test -run TestAdd ./allpass/")),
            Some(Runner::GoTest)
        );
        // `go -C dir test` — legal since Go 1.20, verified on go1.27.1.
        // `verify::parse::Runner::from_argv` reads `./service` as the
        // subcommand here and returns None.
        assert_eq!(
            from_argv(&argv("go -C ./service test ./...")),
            Some(Runner::GoTest)
        );
        assert_eq!(from_argv(&argv("go build ./...")), None);
        assert_eq!(from_argv(&argv("go vet ./...")), None);
        assert_eq!(from_argv(&argv("go -C ./service build ./...")), None);
        assert_eq!(from_argv(&argv("gotestsum --format short")), None);
        assert_eq!(from_argv(&[]), None);
    }

    // --- console, the states that must parse ------------------------------

    #[test]
    fn all_pass_verbose() {
        let p = ok("gotest-all-pass.txt");
        assert_eq!((p.tests_total, p.passed, p.failed, p.skipped), (4, 4, 0, 0));
        assert!(p.test_ids.contains("TestAdd"));
        assert_eq!(p.duration_ms, Some(211));
    }

    #[test]
    fn mixed_verbose() {
        let p = ok("gotest-mixed.txt");
        assert_eq!((p.tests_total, p.passed, p.failed, p.skipped), (5, 2, 2, 1));
        assert!(p.test_ids.contains("TestDivByZero"));
    }

    #[test]
    fn skips_are_counted_as_skips_and_not_as_passes() {
        let p = ok("gotest-skipped.txt");
        assert_eq!((p.tests_total, p.passed, p.failed, p.skipped), (4, 1, 0, 3));
    }

    #[test]
    fn narrowed_is_a_clean_run_of_one_test() {
        let p = ok("gotest-narrowed.txt");
        assert_eq!((p.tests_total, p.passed, p.failed), (1, 1, 0));
    }

    #[test]
    fn a_panic_that_a_test_recorded_as_a_failure_still_parses() {
        // `--- FAIL: TestNilDeref` is present, so the failure is named and the
        // tally is not a clean run. Reporting it beats discarding it.
        let p = ok("gotest-panic.txt");
        assert_eq!((p.tests_total, p.passed, p.failed), (2, 1, 1));
    }

    // --- the four states that must NOT read as a clean run ----------------

    #[test]
    fn a_package_that_did_not_compile_is_never_a_clean_run() {
        for name in [
            "gotest-build-failed.txt",
            "gotest-build-failed-verbose.txt",
            "gotest-vet-failed.txt",
            "gotest-setup-failed.txt",
            "gotest-json-build-failed.txt",
        ] {
            let reason = why(name);
            assert!(
                reason.contains("compile") || reason.contains("build"),
                "{name}: {reason}"
            );
        }
    }

    #[test]
    fn the_build_failure_that_shares_a_stream_with_a_passing_package() {
        // This is `verify`'s defect (1), on the bytes that produce it: a
        // `--- PASS` from `example.com/broken/fine` in the same stream as
        // `FAIL …/broken [build failed]`.
        let text = fixture("gotest-build-failed-verbose.txt");
        assert!(text.contains("--- PASS: TestTwo"));
        assert!(parse(&text).is_unverifiable());
    }

    #[test]
    fn a_timeout_is_never_a_clean_run() {
        let text = fixture("gotest-timeout.txt");
        // The trap: one test passed, none is marked FAIL.
        assert!(text.contains("--- PASS: TestQuick"));
        assert!(!text.contains("--- FAIL"));
        assert!(why("gotest-timeout.txt").contains("timeout"));
    }

    #[test]
    fn a_zero_test_run_is_distinguishable_from_a_clean_one() {
        for name in [
            "gotest-no-test-files.txt",
            "gotest-narrowed-no-match.txt",
            "gotest-summary-only.txt",
            "gotest-corpus-summary-only.txt",
        ] {
            assert!(
                parse(&fixture(name)).is_unverifiable(),
                "{name} must not parse to a tally"
            );
        }
        assert!(why("gotest-no-test-files.txt").contains("no tests"));
        assert!(why("gotest-narrowed-no-match.txt").contains("no tests"));
        // Summary-only is a DIFFERENT unknown: tests certainly ran, and the
        // output does not say how many.
        assert!(why("gotest-summary-only.txt").contains("no per-test result lines"));
    }

    #[test]
    fn ok_is_not_a_verdict() {
        // `TestMain` called `os.Exit(0)` after `m.Run()`: the package line says
        // `ok`, the exit code was 0, and a test failed. The parser reports the
        // failure, which is the only reading that is not a lie.
        let p = ok("gotest-benign-summary.txt");
        assert_eq!((p.tests_total, p.passed, p.failed), (2, 1, 1));
        assert!(fixture("gotest-benign-summary.txt").contains("ok  \texample.com/capture/benign"));
    }

    // --- subtests ----------------------------------------------------------

    #[test]
    fn subtests_are_named_but_not_counted_twice() {
        let p = ok("gotest-subtests.txt");
        // Two top-level tests: TestUpper (failed, because a subtest did) and
        // TestParallelGroup (passed). `verify` counts nine.
        assert_eq!((p.tests_total, p.passed, p.failed), (2, 1, 1));
        assert!(p.test_ids.contains("TestUpper"));
        assert!(p.test_ids.contains("TestUpper/q"), "{:?}", p.test_ids);
        assert!(p.test_ids.contains("TestParallelGroup/one"));
    }

    #[test]
    fn the_two_forms_of_the_same_run_agree() {
        // The whole reason top-level-only is the counting rule.
        let console = ok("gotest-mixed.txt");
        let json = ok("gotest-json-mixed.txt");
        assert_eq!(
            (
                console.tests_total,
                console.passed,
                console.failed,
                console.skipped
            ),
            (json.tests_total, json.passed, json.failed, json.skipped)
        );
    }

    // --- multi-package -----------------------------------------------------

    #[test]
    fn multipackage_console_counts_tests_and_not_package_lines() {
        // Six packages: `ok`, `FAIL`, `?` and a trailing bare `FAIL`. Only
        // failures print without `-v`, so `passed` is a floor and not a census
        // — which is a property of the bytes, not of the parser.
        let p = ok("gotest-multipackage.txt");
        assert_eq!((p.failed, p.passed, p.skipped), (3, 0, 0));
        assert!(p.test_ids.contains("TestUpper"));
    }

    #[test]
    fn multipackage_verbose_counts_every_top_level_test_once() {
        let p = ok("gotest-multipackage-verbose.txt");
        // allpass 4 + mixed 5 + skipped 4 + subtests 2 + benign 2 = 17. The
        // `skipped` package contributes only 2 skips here and 3 in
        // `gotest-skipped.txt`, because that capture passed `-short` and this
        // one did not — the same package, a different number of skips, and the
        // reason `parsed.skipped` is a fact about the run rather than the code.
        assert_eq!(p.tests_total, 17);
        assert_eq!((p.passed, p.failed, p.skipped), (10, 4, 3));
    }

    #[test]
    fn the_interleaved_line_a_race_produced_is_still_read() {
        // `gotest-cover.txt` contains
        // `\texample.com/capture/notests\t\t--- FAIL: TestUpper (0.00s)` —
        // two packages' output collided in one line. Trimming the line loses
        // that failure; taking the text after the final tab does not.
        let text = fixture("gotest-cover.txt");
        assert!(text.contains("notests\t\t--- FAIL: TestUpper"));
        let p = ok("gotest-cover.txt");
        assert_eq!(p.failed, 3, "{:?}", p.test_ids);
        assert!(p.test_ids.contains("TestUpper"));
    }

    #[test]
    fn a_cached_package_reports_no_per_test_lines() {
        // `ok pkg (cached)` and then a real failing package. The failures are
        // read; the cached package contributes nothing, and no elapsed time.
        let p = ok("gotest-cached.txt");
        assert_eq!((p.failed, p.passed), (2, 0));
    }

    // --- json --------------------------------------------------------------

    #[test]
    fn json_all_pass() {
        let p = ok("gotest-json.txt");
        assert_eq!((p.tests_total, p.passed, p.failed), (4, 4, 0));
        assert!(p.duration_ms.is_some_and(|ms| ms > 0));
    }

    #[test]
    fn json_narrowed() {
        let p = ok("gotest-json-narrowed.txt");
        assert_eq!((p.tests_total, p.passed), (1, 1));
    }

    #[test]
    fn json_package_level_events_are_not_tests() {
        // Four packages. `notests` emits a package-level `"Action":"skip"`
        // with no `Test` key — counting it would report a skipped test that
        // does not exist. `allpass` and `benign` emit package-level `pass`.
        let p = ok("gotest-json-multipackage.txt");
        assert_eq!(
            (p.tests_total, p.passed, p.failed, p.skipped),
            (11, 7, 3, 1)
        );
        assert!(!p.test_ids.iter().any(|id| id.contains("capture/")));
    }

    #[test]
    fn json_carries_the_benign_summary_trap_too() {
        // `benign`'s package event is `pass` while `TestFailsButIsHidden`'s
        // test event is `fail`. The test-level event is the one that counts.
        let text = fixture("gotest-json-multipackage.txt");
        assert!(text.contains(r#""Action":"fail","Package":"example.com/capture/benign","Test":"TestFailsButIsHidden""#));
        assert!(ok("gotest-json-multipackage.txt").failed >= 1);
    }

    // --- ansi --------------------------------------------------------------

    #[test]
    fn ansi_written_by_the_test_parses() {
        let text = fixture("gotest-ansi-from-test.txt");
        assert!(text.contains('\u{1b}'), "the fixture really is coloured");
        let p = ok("gotest-ansi-from-test.txt");
        assert_eq!((p.tests_total, p.passed, p.failed), (2, 1, 1));
    }

    #[test]
    fn both_ansi_encodings_go_emits_are_removed() {
        // `t.Logf` doubles the ESC (1b 1b 5b), `fmt.Println` does not (1b 5b).
        let text = fixture("gotest-ansi-mixed-escapes.txt");
        assert!(
            text.contains("\u{1b}\u{1b}["),
            "the doubled form is present"
        );
        assert!(
            text.contains("\u{1b}[35m"),
            "the single form is present too"
        );
        let cleaned = strip_ansi(&collapse_doubled_escapes(&text));
        assert!(
            !cleaned.contains('\u{1b}') && !cleaned.contains("[35m") && !cleaned.contains("[32m"),
            "no escape and no residue survives: {cleaned:?}"
        );
        let p = ok("gotest-ansi-mixed-escapes.txt");
        assert_eq!((p.tests_total, p.passed, p.failed), (3, 2, 1));
    }

    // --- -race and -shuffle ------------------------------------------------

    #[test]
    fn a_race_report_is_read_from_the_result_lines_and_not_from_the_warning() {
        // `-race` splices a `WARNING: DATA RACE` block into the same stream as
        // the results: goroutine dumps, `==================` rules, absolute
        // source paths, and a `testing.go:1865: race detected` line indented
        // with spaces. None of that is a result line, and the tally has to come
        // from the `--- ` lines alone. The dump is also the one part of a Go
        // capture that carries file paths from the machine that produced it,
        // which is why nothing in this parser reads a path.
        let text = fixture("gotest-race.txt");
        assert!(text.contains("WARNING: DATA RACE"));
        let p = ok("gotest-race.txt");
        assert_eq!((p.tests_total, p.passed, p.failed, p.skipped), (2, 1, 1, 0));
        assert!(p.test_ids.contains("TestCounterConcurrent"));
        assert_eq!(
            p.test_ids.len(),
            2,
            "no dump line became a test: {:?}",
            p.test_ids
        );
        assert_eq!(p.duration_ms, Some(1098));
    }

    #[test]
    fn the_race_detector_blamed_a_bystander_and_the_parser_repeats_the_runner() {
        // Not a parser defect — a fact about `go test -race`, measured. The
        // race is in a goroutine `TestLeaks` started and did not wait for, so
        // by the time the detector fires `TestLeaks` has already printed
        // `--- PASS` and the failure is charged to whichever test is running:
        // `TestSleepsPastTheRace`, which only sleeps.
        //
        // The parser must NOT try to re-attribute it. It reports the verdict
        // `go test` published — the same one the agent's terminal showed — and
        // the misattribution is named here rather than silently corrected into
        // something the run never said. Stable over 3 runs on this machine;
        // whether another machine blames the same bystander is not measured.
        let p = ok("gotest-race-bystander.txt");
        assert_eq!((p.tests_total, p.passed, p.failed), (2, 1, 1));
        assert!(p.test_ids.contains("TestSleepsPastTheRace"));
        assert!(p.test_ids.contains("TestLeaks"));
    }

    #[test]
    fn shuffled_output_parses_to_the_same_set_because_order_is_not_evidence() {
        // `-shuffle=on` prints `-test.shuffle <seed>` ahead of the first
        // `=== RUN` and permutes the results. This capture is the same four
        // tests as `gotest-all-pass.txt` in a different order, so the two must
        // parse to the same set and the same counts — anything that keyed off
        // position, or that treated the first line of the stream as
        // structural, disagrees here.
        let shuffled = ok("gotest-shuffle.txt");
        let ordered = ok("gotest-all-pass.txt");
        assert_eq!(shuffled.test_ids, ordered.test_ids);
        assert_eq!(
            (shuffled.tests_total, shuffled.passed, shuffled.failed),
            (4, 4, 0)
        );
        assert_eq!(shuffled.duration_ms, Some(5082));

        // Without `-v` the seed line is printed anyway, because the package
        // failed — so a non-verbose capture can still open with a line that is
        // neither a result nor a package summary.
        let failing = ok("gotest-shuffle-failing.txt");
        assert_eq!(
            (failing.tests_total, failing.passed, failing.failed),
            (2, 0, 2)
        );
        assert!(fixture("gotest-shuffle-failing.txt").starts_with("-test.shuffle "));
    }

    // --- the real third-party suites --------------------------------------

    #[test]
    fn a_real_suite_verbose() {
        // A clone of github.com/spf13/cobra, 56 KB, all green.
        let p = ok("gotest-corpus-verbose.txt");
        assert_eq!(p.failed, 0);
        assert_eq!(p.tests_total, 279, "top-level tests only");
        // 150 subtests are named and not counted.
        assert_eq!(p.test_ids.len(), 279 + 150);
    }

    #[test]
    fn a_real_suite_with_a_real_regression() {
        // The same clone with `len(args) != n` changed to `len(args) < n` at
        // args.go:109. Not an upstream failure — a regression introduced for
        // the capture, and recorded as such in the fixture README.
        let p = ok("gotest-corpus-mixed.txt");
        assert_eq!(p.failed, 6);
        assert!(p.test_ids.contains("TestExactArgs_WithInvalidCount"));
    }

    #[test]
    fn a_real_narrowed_run_over_the_same_suite() {
        let p = ok("gotest-corpus-narrowed.txt");
        assert_eq!(p.failed, 3);
    }

    // --- the invariant this module exists for ------------------------------

    #[test]
    fn no_fixture_anywhere_parses_to_a_zero_failure_zero_test_tally() {
        // §6.2: `unverifiable` must never collapse into `supported`. A
        // `Parsed` with no tests in it is the shape that reads as a clean run,
        // and there must not be one for ANY captured byte sequence.
        let dir = format!("{}/tests/fixtures/runners", env!("CARGO_MANIFEST_DIR"));
        let mut checked = 0;
        for entry in std::fs::read_dir(&dir).expect("the fixture directory exists") {
            let path = entry.expect("a readable entry").path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if !name.starts_with("gotest-") {
                continue;
            }
            checked += 1;
            let text = std::fs::read_to_string(&path).expect("a readable fixture");
            if let RunnerOutcome::Parsed(p) = parse(&text) {
                assert!(
                    p.tests_total > 0,
                    "{name} parsed to a tally with no tests in it"
                );
                assert_eq!(
                    p.tests_total,
                    p.passed + p.failed + p.skipped,
                    "{name}: the counts do not add up"
                );
            }
        }
        // The count is asserted, not just the loop, so that a fixture added
        // later cannot slip through this sweep without a test that names it.
        // It moved 30 -> 34 when the two `-race` and two `-shuffle` captures
        // landed, and that is exactly the signal it exists to give.
        assert_eq!(checked, 34, "every captured go fixture is exercised");
    }
}
