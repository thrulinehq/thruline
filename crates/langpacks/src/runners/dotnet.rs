//! §6.2 — the .NET runners: `dotnet test`, `dotnet vstest`, a
//! Microsoft.Testing.Platform host run directly, and xunit v3's own runner.
//!
//! # One command, six summary grammars — and that is a measurement
//!
//! `tests/fixtures/runners/README.md` counts them over the shipped bytes.
//! Written out, with the fixture that carries each:
//!
//! ```text
//! VSTest piped     Failed!  - Failed:     2, Passed:     5, Skipped:     1, Total:     8, …
//!                  dotnet-mixed.txt
//! VSTest detailed  Test Run Failed. / Total tests: 8 / Passed: 5 / Failed: 2 / Skipped: 1
//!                  dotnet-console-detailed.txt
//! TerminalLogger   Test summary: total: 8, failed: 2, succeeded: 5, skipped: 1, duration: 0.4s
//!                  dotnet-terminallogger-mixed.txt, dotnet-terminallogger-piped-mixed.txt
//! MTP direct       Test run summary: Failed! - …dll (net9.0|arm64)
//!                    total: 7 / failed: 2 / succeeded: 4 / skipped: 1 / duration: 110ms
//!                  mtp-direct-mixed.txt
//! MTP dotnet test  Failed! - Failed: 2, Passed: 4, Skipped: 1, Total: 7, Duration: 77ms
//!                  mtp-dotnettest-mixed.txt
//! xunit v3         === TEST EXECUTION SUMMARY ===
//!                     Shipping.V3  Total: 8, Errors: 0, Failed: 2, Skipped: 1, Not Run: 0, …
//!                  xunit3-nocolor-mixed.txt
//! ```
//!
//! Which one a run produces is decided by the logger flag, by an environment
//! variable (`MSBUILDTERMINALLOGGER=on` selects TerminalLogger **into a pipe**,
//! so "stdout is not a tty" does not imply the piped grammar), and by a
//! property in the `.csproj` of the project under test that this parser cannot
//! see. So all six are read, and which one an output carries is decided by
//! anchors rather than by anything the caller claims.
//!
//! The six are mutually exclusive on the 47 console fixtures — measured, not
//! assumed: no fixture carries a strict VSTest/MTP summary line (one line with
//! `Total:`, `Passed:` **and** `Failed:`) and the xunit v3 banner, and no
//! fixture carries two of the four block grammars. The `Passed:` clause is what
//! makes that true: xunit v3's line has `Total:` and `Failed:` and no `Passed:`
//! at all, so a two-key test would route nine xunit v3 fixtures into the wrong
//! reader.
//!
//! # The rules that are not obvious, each with the capture that forced it
//!
//! 1. **Per-assembly summaries are SUMMED, never read from the last line.**
//!    `dotnet test` on a two-project solution interleaves output; over eight
//!    consecutive runs of the same *failing* solution the last summary line was
//!    `Passed!` three times. `dotnet-multiproject-last-line-green.txt` is one of
//!    those three. A reader that takes the last line is green on a red run, one
//!    time in three, intermittently.
//! 2. **A verdict word that contradicts its own tally is `unverifiable`.**
//!    `mtp-dotnettest-zero-tests.txt` reads `Failed! - Failed: 0, Passed: 0,
//!    Skipped: 0, Total: 0`. Reading the tally alone gives `failed = 0` for a
//!    suite that failed to be a suite. This is `verify::parse::parse_cargo_test`'s
//!    rule (*"a `test result: FAILED` line counted zero failures — the verdict
//!    word and the counts disagree"*) arriving in a second runner, which is the
//!    evidence that it was never cargo-specific.
//! 3. **The parts must sum to the total, exactly.** Every one of the six
//!    grammars accounts for every test it counts, on every capture. A summary
//!    whose parts do not add up has a bucket this parser cannot name, and an
//!    unnamed bucket is exactly *I could not tell*.
//! 4. **Counts come from a tally, never from prose.** `dotnet-zero-tests.txt`
//!    says `No test is available in …Shipping.Tests.dll` and prints no summary
//!    at all, exit 0. That sentence is emitted both for a project that declares
//!    no tests and for one whose adapter failed to load, so "zero" would be this
//!    parser's inference and not the runner's count. It is `unverifiable`. The
//!    zero-test runs that *do* carry a tally (`Skipped! - … Total: 3`,
//!    `Test summary: total: 0`, MTP's `Zero tests ran` over `total: 0`,
//!    `<Counters total="0" …/>`) are parsed as the zero they state, and
//!    `tests_pass` refuses those on its own `tests_total == 0` clause.
//! 5. **An aborted run is refused whatever else it printed.**
//!    `dotnet-testhost-crash.txt` and `dotnet-blame-crash.txt` reach
//!    `Starting test execution` and end at `Test Run Aborted.` — whether any of
//!    the five tests completed before the host died is not recoverable, so it is
//!    not claimed.
//! 6. **`strip_ansi` is not a CI-only concern here.** xunit v3 4.0.0 writes 25
//!    escape sequences into a plain **pipe**, unasked (`xunit3-mixed.txt`), and
//!    the pty captures carry an OSC 8 `file://` hyperlink *inside the summary
//!    line* (`mtp-ansi-color.txt`). A CSI-only stripper leaves `]8;;file://…`
//!    sitting in the middle of the line it is trying to parse;
//!    `verify::parse::strip_ansi` handles OSC, which is why it is the one used.
//!
//! # Two documented flags defeat a verifier that reads exit codes
//!
//! `--ignore-exit-code 2` (MTP) and `-ignoreFailures` (xunit v3) leave the
//! summary intact — `failed: 2` is right there — and return **0**.
//! `mtp-direct-ignore-exit-code.txt` and `xunit3-ignore-failures.txt` are those
//! runs, and both parse to `failed: 2` here. Neither is a cheat someone had to
//! invent for a detector; both are in `--help`.

use std::collections::BTreeSet;

use thruline_verify::parse::strip_ansi;

use super::{parsed, unverifiable, Runner, RunnerOutcome};

/// Recognise a .NET test run from an argv.
///
/// # Two differences from `verify::parse::Runner::from_argv`, both measured
///
/// `verify` takes the first argument that does not begin with `-` and requires
/// it to be exactly `test`. That is right for `dotnet test` and for
/// `dotnet -d test` (`-d` is a bare global flag; verified against SDK 9.0.317),
/// and there is no global flag that takes a separate-word value in front of a
/// subcommand — `dotnet --roll-forward Major test` is rejected by the SDK with
/// *"dotnet---roll-forward does not exist"*, also verified. So the scan itself
/// is sound and is kept.
///
/// 1. **`dotnet vstest <assembly.dll>` is recognised here and is not by
///    `verify`.** It is a real, captured test run (`dotnet-vstest-mixed.txt`,
///    VSTest 17.14.1) that takes assemblies instead of projects and skips the
///    build, so it lacks the `Determining projects to restore` preamble but ends
///    in the identical summary line. `verify` reads `vstest` as "not `test`" and
///    returns `None`, and a run with no runner attached is reported by §6.3 as
///    *"no test check was captured"* — the agent looks as though it ran nothing.
/// 2. **A Microsoft.Testing.Platform or xunit v3 assembly run directly
///    (`dotnet Foo.dll …`) is recognised only when the argv carries a
///    test-platform option.**
///
/// # What (2) deliberately does not recognise, and why that is the safe side
///
/// **A bare `dotnet Shipping.Mtp.dll` is not recognised, and four shipped
/// fixtures come from exactly that argv** (`mtp-direct-mixed.txt`,
/// `mtp-direct-pass.txt`, `mtp-direct-zero-tests.txt`, `xunit3-mixed.txt`).
/// Their bytes parse here; their command line does not reach this function's
/// `Some`. That is a named hole, not an oversight.
///
/// The reason is `verify::check`: `Runner::from_argv` returning `Some` is what
/// marks an `exec` event as a **test check** (`crates/verify/src/check.rs:203`),
/// and `dotnet <anything>.dll` is how every .NET *application* on the machine is
/// started. Recognising it unconditionally would file `dotnet myservice.dll` as
/// a test run, whose unreadable output then becomes the check `tests_pass`
/// selects — turning an ordinary program launch into evidence about a test
/// claim. Refusing it costs a run that is not identifiable from its argv;
/// accepting it costs the meaning of "test check". Neither can support a false
/// claim, and the second one corrupts a category.
///
/// # The option list is derived, not remembered
///
/// Re-derive it with, against the projects the fixtures were captured from:
///
/// ```text
/// dotnet bin/Debug/net9.0/Shipping.Mtp.dll --help     # MTP 1.4.3 + MSTest 3.6.4
/// dotnet bin/Debug/net9.0/Shipping.MtpTrx.dll --help  # + Microsoft.Testing.Extensions.TrxReport
/// dotnet bin/Debug/net9.0/Shipping.V3.dll --help      # xunit.v3 4.0.0 in-process runner
/// ```
pub fn from_argv(argv: &[String]) -> Option<Runner> {
    let program = argv.first()?;
    let base = program.rsplit('/').next().unwrap_or(program);
    // No `dotnet.exe` arm: this crate's captures are all macOS, and
    // `thruline_capture::shim::WRAPPED` names `dotnet`. An arm with no capture
    // behind it reads as coverage that does not exist.
    if base != "dotnet" {
        return None;
    }
    let mut at = 1usize;
    while argv.get(at).is_some_and(|a| a.starts_with('-')) {
        at += 1;
    }
    match argv.get(at)?.as_str() {
        // `dotnet test` — verify's arm, unchanged.
        "test" => return Some(Runner::DotnetTest),
        // Difference 1.
        "vstest" => return Some(Runner::DotnetTest),
        // `dotnet exec Foo.dll …` is the spelling MTP's own help prints.
        "exec" => {
            at += 1;
            while argv.get(at).is_some_and(|a| a.starts_with('-')) {
                at += 1;
            }
        }
        _ => {}
    }
    // Difference 2: a test assembly run directly, and only with an option that
    // names it as one.
    if !argv.get(at)?.to_ascii_lowercase().ends_with(".dll") {
        return None;
    }
    argv.iter()
        .skip(at + 1)
        .any(|a| is_test_platform_option(a))
        .then_some(Runner::DotnetTest)
}

/// Options that only a test platform takes.
///
/// Derived from the three `--help` outputs named on [`from_argv`], minus the
/// words any console application might also accept: `--help`, `--info`,
/// `--output`, `--settings`, `--timeout`, `--diagnostic*`, `-debug`, `-wait`,
/// `-pause`, `-culture`, `-id`, `-run`. Those are dropped **because** they are
/// generic — recognising `dotnet myapp.dll --output ./x` as a test run is the
/// mistake this list exists to avoid.
fn is_test_platform_option(arg: &str) -> bool {
    let arg = arg.split('=').next().unwrap_or(arg).to_ascii_lowercase();
    const MTP: &[&str] = &[
        "--filter",
        "--list-tests",
        "--minimum-expected-tests",
        "--ignore-exit-code",
        "--exit-on-process-exit",
        "--results-directory",
        "--test-parameter",
        "--no-progress",
        "--no-ansi",
        "--report-trx",
        "--report-trx-filename",
    ];
    const XUNIT_V3: &[&str] = &[
        "-assemblyinfo",
        "-class",
        "-explicit",
        "-failskips",
        "-failwarns",
        "-filter",
        "-filtervstest",
        "-ignorefailures",
        "-list",
        "-longrunning",
        "-maxthreads",
        "-method",
        "-methoddisplay",
        "-namespace",
        "-nocolor",
        "-nologo",
        "-parallelalgorithm",
        "-parallelmode",
        "-preenumeratetheories",
        "-reporter",
        "-result-ctrf",
        "-result-html",
        "-result-junit",
        "-result-nunit",
        "-result-trx",
        "-result-xml",
        "-result-xmlv1",
        "-showliveoutput",
        "-stoponfail",
        "-trait",
        "-useansicolor",
    ];
    // xunit v3 negates a switch with a trailing `-`: `-failSkips-`.
    let stem = arg.strip_suffix('-').unwrap_or(&arg);
    MTP.contains(&arg.as_str()) || XUNIT_V3.contains(&arg.as_str()) || XUNIT_V3.contains(&stem)
}

/// Parse .NET test output — console in any of its six grammars, or a TRX.
pub fn parse(output: &str) -> RunnerOutcome {
    // §6.2, first step, and for C# it is load-bearing rather than defensive:
    // see rule 6 in the module docs.
    let plain = strip_ansi(output);
    let text = plain.as_str();

    if text.contains("<TestRun") || text.contains("<ResultSummary") {
        return parse_trx(text);
    }
    // A report this reader is not the reader for. Both shapes are shipped as
    // C# fixtures (`xunit3-junit.xml` from xunit v3's own `-result-junit`,
    // `dotnet-junit-logger-mixed.xml` from `JunitXml.TestLogger` 4.1.0), and
    // saying which reader owns them is more use than a generic refusal.
    if text.contains("<testsuite") {
        return unverifiable(
            "this is a JUnit XML report, not a TRX and not console output",
            "read it with §6.2's generic JUnit XML reader; `dotnet test --logger junit` \
             and xunit v3's `-result-junit` both write this shape, and its counts live \
             on the INNER `<testsuite>` — xunit v3's wrapper carries every counter \
             except `skipped`",
        );
    }
    if text.trim_start().starts_with('{') && text.contains("\"reportFormat\":\"CTRF\"") {
        return unverifiable(
            "this is a CTRF JSON report (xunit v3 `-result-ctrf`), which this parser \
             does not read",
            "re-run with `-result-junit` or `--report-trx`, or hand the console output \
             to this parser instead; `xunit3-ctrf.json` is shipped as a fixture and no \
             reader in this crate claims it",
        );
    }
    parse_console(text)
}

/// Lines, splitting on CR as well as LF.
///
/// The pty captures redraw with bare carriage returns — `dotnet-terminallogger-pass.txt`
/// is mostly `ESC[2F CR` cursor motion — so a `\n`-only split can leave a
/// summary sharing a line with the progress frame that was overwritten to make
/// room for it.
fn logical_lines(text: &str) -> Vec<&str> {
    text.split(['\n', '\r']).collect()
}

/// One run's counts, accumulated across every per-assembly summary in it.
#[derive(Debug, Default)]
struct Tally {
    total: u64,
    passed: u64,
    failed: u64,
    skipped: u64,
    duration_ms: Option<u64>,
    ids: BTreeSet<String>,
    /// How many summaries were folded in. Zero means no grammar matched, which
    /// is a different sentence from "the counts were zero".
    summaries: usize,
}

impl Tally {
    fn add_duration(&mut self, ms: Option<u64>) {
        // **Max, not sum.** A solution run executes its projects in parallel
        // (two interleaved `VSTest version` banners in
        // `dotnet-multiproject-mixed.txt`), so adding their durations invents
        // wall-clock time that never elapsed. The longest one is at least a
        // lower bound on what the run cost.
        if let Some(ms) = ms {
            self.duration_ms = Some(self.duration_ms.map_or(ms, |cur| cur.max(ms)));
        }
    }

    /// Turn the accumulated counts into an outcome, refusing anything that does
    /// not add up.
    fn finish(self, grammar: &str) -> RunnerOutcome {
        if self.summaries == 0 {
            return unverifiable(
                format!("no {grammar} summary could be read from the output"),
                "capture the runner's whole output; a .NET summary is written last, \
                 so a truncated capture loses it first",
            );
        }
        // Rule 3 in the module docs. Every captured grammar accounts for every
        // test it counts; a run where the parts do not add up has a bucket this
        // parser cannot name, and naming it `passed` is how a defaulted field
        // becomes a clean run.
        if self.passed + self.failed + self.skipped != self.total {
            return unverifiable(
                format!(
                    "the {grammar} summary does not add up: {} passed + {} failed + \
                     {} skipped is not {} total, so at least one test is in a bucket \
                     this parser cannot name",
                    self.passed, self.failed, self.skipped, self.total
                ),
                "capture the runner's whole output; if the summary is complete, this \
                 runner has a result category this parser has never been shown and it \
                 must be measured before it can be counted",
            );
        }
        let mut p = parsed(Runner::DotnetTest);
        p.tests_total = self.total;
        p.passed = self.passed;
        p.failed = self.failed;
        p.skipped = self.skipped;
        // `xfail` stays 0: none of xunit, NUnit or MSTest has an
        // expected-failure state, and inventing one from `[Ignore]` would put a
        // skip in a field §6.3 reports separately.
        p.test_ids = self.ids;
        p.duration_ms = self.duration_ms;
        RunnerOutcome::Parsed(p)
    }
}

fn parse_console(text: &str) -> RunnerOutcome {
    let lines = logical_lines(text);

    // --- the refusals that outrank any tally in the same output -------------
    //
    // Both are checked before the summary grammars on purpose. A run that
    // aborted or a project that did not compile can still have printed counts
    // for the part that got that far, and the part that did not is exactly what
    // is unknown.
    if lines
        .iter()
        .any(|l| l.contains("Test Run Aborted") || l.contains("The active test run was aborted"))
    {
        return unverifiable(
            "the test run was aborted — VSTest reports `Test host process crashed`, and \
             the output carries no tally for the tests that had already run",
            "whether any test passed before the crash is not recoverable from this \
             output; `--blame` writes a Sequence.xml beside the run naming the test that \
             was in flight when the host died, and that file — not stdout — is where \
             the answer is. `--blame-crash` alone does not put it in stdout: its capture \
             here differs from the plain one by a single `Blame: Attaching crash dump \
             utility` line and carries no counts either",
        );
    }
    if let Some(err) = lines.iter().find(|l| is_compile_error(l)) {
        return unverifiable(
            format!("the test project did not compile: {}", err.trim()),
            "fix the compilation error and re-run; no test binary was produced, so \
             nothing here is evidence about the tests",
        );
    }

    // --- the six grammars ---------------------------------------------------
    if lines
        .iter()
        .any(|l| l.trim() == "=== TEST EXECUTION SUMMARY ===")
    {
        return parse_xunit_v3(&lines);
    }
    if lines.iter().any(|l| l.contains("Test run summary:")) {
        return parse_mtp_direct(&lines);
    }
    if lines
        .iter()
        .any(|l| l.trim_start().starts_with("Test summary:"))
    {
        return parse_terminal_logger(&lines);
    }
    if lines.iter().any(|l| is_vstest_summary_shaped(l)) {
        return parse_vstest_console(&lines);
    }
    if lines
        .iter()
        .any(|l| l.trim_start().starts_with("Total tests:"))
    {
        return parse_vstest_detailed(&lines);
    }

    // --- no summary: say which silence this is ------------------------------
    if lines
        .iter()
        .any(|l| l.contains("The following Tests are available"))
    {
        return unverifiable(
            "this is a test **listing** (`--list-tests`), not a run: the names were \
             discovered and nothing was executed",
            "re-run without `--list-tests`; the process exits 0 after printing the \
             names, so its exit code says nothing about the tests",
        );
    }
    if lines.iter().any(|l| l.contains("No test is available in")) {
        return unverifiable(
            "VSTest discovered no test at all and printed no tally — a message it emits \
             both for a project that declares no tests and for one whose test adapter \
             failed to load",
            "check that the project references a test adapter and that the framework \
             version matches; a run that discovered nothing exits 0, so the exit code \
             is not a second opinion",
        );
    }
    if lines
        .iter()
        .any(|l| l.contains("No test matches the given testcase filter"))
    {
        return unverifiable(
            "the `--filter` expression matched no test, and the run printed no tally",
            "check the filter expression; a filter that matches nothing exits 0, which \
             is how one typo turns a suite into a green no-op",
        );
    }
    if lines
        .iter()
        .any(|l| l.contains("is invalid. Please use the /help option"))
    {
        return unverifiable(
            "vstest rejected its argument — the test assembly named on the command line \
             does not exist, which is what `--no-build` does over a cleaned output \
             directory",
            "build before running, or drop `--no-build`",
        );
    }
    if lines.iter().any(|l| l.contains("error run failed")) {
        return unverifiable(
            "the Microsoft.Testing.Platform host exited with `error run failed` and \
             printed no summary; a rejected command-line option and a genuinely failing \
             suite produce the identical output, and the option's own error message goes \
             to the host's log file rather than to stdout",
            "read the `.log` file the error line names, or re-run the assembly directly \
             (`dotnet <assembly>.dll`) where the option error reaches stdout",
        );
    }
    Tally::default().finish("dotnet test")
}

/// `…/ShippingTests.cs(10,39): error CS1002: ; expected […csproj]`
///
/// The `CS` prefix and the digits are both required. `error run failed:` is
/// MSBuild reporting a **failed test run** under MTP and appears in four
/// fixtures that also carry a valid summary; reading it as a compile error
/// would refuse them all.
fn is_compile_error(line: &str) -> bool {
    let Some(at) = line.find("error CS") else {
        return false;
    };
    line[at + "error CS".len()..]
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit())
}

/// A one-line VSTest / MTP-under-`dotnet test` summary.
///
/// All three keys are required. `Passed:` is what separates this grammar from
/// xunit v3's line, which has `Total:` and `Failed:` and no passed count at all
/// — see the module docs.
fn is_vstest_summary_shaped(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("total:") && lower.contains("passed:") && lower.contains("failed:")
}

/// Grammar 1 and 5: `Failed!  - Failed: 2, Passed: 5, Skipped: 1, Total: 8, …`
///
/// **Split on whitespace, never match the literal.** VSTest pads its marker to
/// eight columns (`Failed!  - `, two spaces) and `Skipped! - ` gets one; MTP
/// pads nothing at all (`Failed! - Failed: 2,`). A parser anchored on
/// `"Failed!  - "` reads every MTP run as having no summary.
fn parse_vstest_console(lines: &[&str]) -> RunnerOutcome {
    let mut tally = Tally::default();
    for line in lines {
        if !is_vstest_summary_shaped(line) {
            continue;
        }
        let verdict = line.split_whitespace().next().unwrap_or("");
        // A summary-shaped line whose leading word is not one of the three this
        // parser has been shown. Refusing loudly beats skipping it silently:
        // silently skipping is how a run with counts becomes "no summary".
        if !matches!(verdict, "Passed!" | "Failed!" | "Skipped!") {
            return unverifiable(
                format!(
                    "a summary-shaped line whose verdict word is not one this parser has \
                     measured (`Passed!`, `Failed!`, `Skipped!`): {}",
                    line.trim()
                ),
                "capture the runner's whole output and report the line; a verdict word \
                 this parser cannot read is a grammar it has not been measured against",
            );
        }
        let (Some(total), Some(passed), Some(failed)) = (
            counter(line, "total:"),
            counter(line, "passed:"),
            counter(line, "failed:"),
        ) else {
            return unverifiable(
                format!("a summary line with an unreadable count: {}", line.trim()),
                "capture the runner's whole output; a summary cut mid-line loses its \
                 later clauses first",
            );
        };
        // Rule 2. Proven by `mtp-dotnettest-zero-tests.txt`, where MTP prints
        // `Failed!` over a tally of nothing at all.
        if verdict == "Failed!" && failed == 0 {
            return unverifiable(
                format!(
                    "the summary reads `Failed!` and counts zero failures — the verdict \
                     word and the tally disagree, and only one of the two readings is \
                     safe: {}",
                    line.trim()
                ),
                "this is what Microsoft.Testing.Platform prints under `dotnet test` when \
                 a project contains no tests at all; check whether the suite ran",
            );
        }
        // `Skipped:` is not defaulted when absent — `Tally::finish` refuses a
        // summary whose parts do not add up, which is what catches its absence.
        tally.total += total;
        tally.passed += passed;
        tally.failed += failed;
        tally.skipped += counter(line, "skipped:").unwrap_or(0);
        tally.add_duration(duration_after(line, "duration:"));
        tally.summaries += 1;
    }
    tally.ids = vstest_test_ids(lines);
    tally.finish("dotnet test")
}

/// Grammar 2: the `--logger "console;verbosity=detailed"` block.
///
/// ```text
/// Total tests: 8
///      Passed: 5
///      Failed: 2
///     Skipped: 1
/// ```
///
/// The keys sit on their own lines, so the per-test lines above them
/// (`  Passed Shipping.Tests.…OversizeThreshold(kg: 30, expected: False) [17 ms]`)
/// cannot be confused with them: those have no colon after the verb.
fn parse_vstest_detailed(lines: &[&str]) -> RunnerOutcome {
    let mut tally = Tally::default();
    let mut at = 0usize;
    while at < lines.len() {
        let line = lines[at];
        at += 1;
        if !line.trim_start().starts_with("Total tests:") {
            continue;
        }
        let Some(total) = counter(line, "total tests:") else {
            return unverifiable(
                format!("a `Total tests:` line with an unreadable count: {line}"),
                "capture the runner's whole output",
            );
        };
        tally.total += total;
        tally.summaries += 1;
        // The block that follows, in whatever order it comes.
        for follow in lines[at..].iter().take(6) {
            let head = follow.trim_start();
            if let Some(n) = head.strip_prefix("Passed:").and_then(read_count) {
                tally.passed += n;
            } else if let Some(n) = head.strip_prefix("Failed:").and_then(read_count) {
                tally.failed += n;
            } else if let Some(n) = head.strip_prefix("Skipped:").and_then(read_count) {
                tally.skipped += n;
            } else if head.starts_with("Total time:") {
                tally.add_duration(duration_after(follow, "total time:"));
            }
        }
    }
    tally.ids = vstest_test_ids(lines);
    tally.finish("dotnet test --logger console;verbosity=detailed")
}

/// Grammar 3: MSBuild's TerminalLogger.
///
/// `Test summary: total: 8, failed: 2, succeeded: 5, skipped: 1, duration: 0.4s`
///
/// Note `succeeded:`, not `passed:`. This grammar appears whenever stdout is a
/// tty **and** whenever `MSBUILDTERMINALLOGGER=on` or `-tl:on` is set, pipe or
/// not, so it is not the exotic case.
fn parse_terminal_logger(lines: &[&str]) -> RunnerOutcome {
    let mut tally = Tally::default();
    for line in lines {
        if !line.trim_start().starts_with("Test summary:") {
            continue;
        }
        let (Some(total), Some(failed), Some(passed)) = (
            counter(line, "total:"),
            counter(line, "failed:"),
            counter(line, "succeeded:"),
        ) else {
            return unverifiable(
                format!(
                    "a `Test summary:` line with an unreadable count: {}",
                    line.trim()
                ),
                "capture the runner's whole output",
            );
        };
        tally.total += total;
        tally.passed += passed;
        tally.failed += failed;
        tally.skipped += counter(line, "skipped:").unwrap_or(0);
        tally.add_duration(duration_after(line, "duration:"));
        tally.summaries += 1;
    }
    // TerminalLogger names no test: its per-test detail is the MSBuild
    // `TESTERROR` block, which carries a file and a line and not a test id.
    tally.finish("MSBuild TerminalLogger")
}

/// Grammar 4: a Microsoft.Testing.Platform assembly run directly.
///
/// ```text
/// Test run summary: Failed! - …/Shipping.Mtp.dll (net9.0|arm64)
///   total: 7
///   failed: 2
///   succeeded: 4
///   skipped: 1
///   duration: 110ms
/// ```
///
/// The counts are on the **five following lines**, so a reader that parses one
/// line finds a verdict word and no numbers.
fn parse_mtp_direct(lines: &[&str]) -> RunnerOutcome {
    let mut tally = Tally::default();
    for (at, line) in lines.iter().enumerate() {
        let Some(after) = line.split_once("Test run summary:") else {
            continue;
        };
        let verdict = after.1.split(" - ").next().unwrap_or("").trim().to_string();
        let mut total = None;
        let mut failed = None;
        let mut passed = None;
        let mut skipped = 0u64;
        // Bounded to the block. Five keys plus the blank segments a CR split
        // leaves behind; a `total:` further down the file belongs to something
        // else and must not be absorbed into this block.
        for follow in lines[at + 1..].iter().take(12) {
            let head = follow.trim_start();
            if let Some(n) = head.strip_prefix("total:").and_then(read_count) {
                total = Some(n);
            } else if let Some(n) = head.strip_prefix("failed:").and_then(read_count) {
                failed = Some(n);
            } else if let Some(n) = head.strip_prefix("succeeded:").and_then(read_count) {
                passed = Some(n);
            } else if let Some(n) = head.strip_prefix("skipped:").and_then(read_count) {
                skipped = n;
            } else if head.starts_with("duration:") {
                tally.add_duration(duration_after(follow, "duration:"));
            } else if !head.is_empty() {
                // The block ends at the first line that is not one of its keys.
                break;
            }
        }
        let (Some(total), Some(failed), Some(passed)) = (total, failed, passed) else {
            return unverifiable(
                format!(
                    "a `Test run summary:` block missing one of its counts (`total:`, \
                     `failed:`, `succeeded:`), after the verdict `{verdict}`"
                ),
                "capture the runner's whole output; MTP writes its counts on the five \
                 lines after the summary line, so a capture cut at the summary keeps the \
                 verdict word and loses every number",
            );
        };
        // Rule 2 again, in this grammar's spelling.
        if verdict.starts_with("Failed!") && failed == 0 {
            return unverifiable(
                "the MTP summary reads `Failed!` and counts zero failures — the verdict \
                 word and the tally disagree"
                    .to_string(),
                "check whether the suite ran at all; MTP also has a `Zero tests ran` \
                 verdict, and a `Failed!` over an empty tally is not it",
            );
        }
        tally.total += total;
        tally.passed += passed;
        tally.failed += failed;
        tally.skipped += skipped;
        tally.summaries += 1;
    }
    tally.ids = mtp_test_ids(lines);
    tally.finish("Microsoft.Testing.Platform")
}

/// Grammar 6: xunit v3's own runner.
///
/// ```text
/// === TEST EXECUTION SUMMARY ===
///    Shipping.V3  Total: 8, Errors: 0, Failed: 2, Skipped: 1, Not Run: 0, Time: 0.072s
/// ```
///
/// Three things here exist in no other .NET grammar:
///
/// - **there is no passed count** — it is `Total` minus everything else, which
///   is why [`is_vstest_summary_shaped`] requires `Passed:`;
/// - **`Errors:` and `Not Run:` are separate buckets.** `Errors` is a failure
///   outside a test body (a fixture or a collection), so it is counted as
///   failed. `Not Run` is counted as skipped — and that arm is **unmeasured**:
///   it is 0 in all nine xunit v3 captures, so nothing here proves the choice;
/// - **the assembly is named on the line**, so a multi-assembly run emits one
///   line per assembly and they must be summed. That is `parse_cargo_test`'s
///   workspace rule arriving in C#. No multi-assembly xunit v3 run is captured,
///   so the summing is written but unproven for this grammar.
///
/// `xunit3-filter-no-match.txt` truncates the line to `   Shipping.V3  Total: 0`
/// with no other clause at all. A missing clause is **not** read as zero: the
/// line is accepted only when the total it states is itself zero, where nothing
/// can be hiding in the clauses that are absent.
fn parse_xunit_v3(lines: &[&str]) -> RunnerOutcome {
    let banner = lines
        .iter()
        .position(|l| l.trim() == "=== TEST EXECUTION SUMMARY ===");
    let Some(banner) = banner else {
        return unverifiable(
            "the xunit v3 execution-summary banner was matched and then lost",
            "capture the runner's whole output",
        );
    };
    let mut tally = Tally::default();
    for line in lines[banner + 1..].iter() {
        let Some(total) = counter(line, "total:") else {
            continue;
        };
        let failed = counter(line, "failed:");
        let errors = counter(line, "errors:");
        let skipped = counter(line, "skipped:");
        let not_run = counter(line, "not run:");
        match (failed, errors, skipped, not_run) {
            (Some(failed), Some(errors), Some(skipped), Some(not_run)) => {
                let bad = failed + errors;
                let out = skipped + not_run;
                if bad + out > total {
                    return unverifiable(
                        format!(
                            "an xunit v3 summary counting more results than tests: {}",
                            line.trim()
                        ),
                        "capture the runner's whole output",
                    );
                }
                tally.total += total;
                tally.failed += bad;
                tally.skipped += out;
                tally.passed += total - bad - out;
            }
            _ if total == 0 => {
                // The truncated form. Nothing can have failed among zero tests,
                // so the absent clauses cost nothing here — and only here.
                tally.summaries += 1;
                continue;
            }
            _ => {
                return unverifiable(
                    format!(
                        "an xunit v3 summary line missing a clause over a non-empty run, \
                         so the missing count is unknown rather than zero: {}",
                        line.trim()
                    ),
                    "re-run without `-stopOnFail`/a filter that empties the run, or \
                     capture the whole output; xunit v3 drops the `Failed:`/`Skipped:` \
                     clauses only when it has nothing to report",
                );
            }
        }
        tally.add_duration(duration_after(line, "time:"));
        tally.summaries += 1;
    }
    tally.ids = xunit_v3_test_ids(lines);
    tally.finish("xunit v3")
}

/// `<Counters total="8" executed="7" passed="5" failed="2" …/>` under
/// `<ResultSummary outcome="…">`.
///
/// # Read the counters, never the prose, and never `outcome=`
///
/// `outcome` is `"Completed"` for a green run **and** for a run of nothing
/// (`dotnet-zero-tests.trx`), and `"Failed"` for a failing one — it does not
/// separate the two cases that matter. Worse, `outcome=` is not a per-test
/// attribute: `dotnet-mixed.trx` carries **12** of them for 8 tests, three of
/// them on `<RunInfo>` elements that merely echo the adapter's console lines.
/// Counting `outcome="Failed"` over the document gives 3 for a run with 2
/// failures.
///
/// # `notExecuted` is populated by MTP and not by VSTest
///
/// ```text
/// VSTest  <Counters total="8" executed="7" … notExecuted="0" …/>   1 NotExecuted result
/// MTP     <Counters total="7" executed="6" … notExecuted="1" …/>   1 NotExecuted result
/// ```
///
/// Same extension, same attribute, two writers, two meanings. So the skipped
/// count is **`total - executed`**, which is right for both, where trusting the
/// attribute is right for one.
///
/// # Nothing here is defaulted
///
/// Every attribute this reads is required. `verify::parse::parse_junit_xml`
/// learned that the hard way — a report whose `failures` attribute was absent
/// read as a clean run — and the fix there was to demand evidence of
/// completeness rather than to fill a gap with a zero. Here that means: a
/// missing counter is a refusal, and a document without its closing
/// `</TestRun>` is a refusal, because the `<ResultSummary>` a TRX ends with is
/// the first thing a truncated write loses.
fn parse_trx(text: &str) -> RunnerOutcome {
    if !text.contains("</TestRun>") {
        return unverifiable(
            "the TRX report has no closing `</TestRun>`, so it was cut off mid-write",
            "re-run and let the report finish writing; a runner killed mid-write, a full \
             disk and an OOM-killed container all produce this shape, and a TRX puts its \
             `<ResultSummary>` last",
        );
    }
    let Some(summary_at) = text.find("<ResultSummary") else {
        return unverifiable(
            "the TRX report has no `<ResultSummary>` element, so it carries no counts",
            "re-run with `--logger \"trx;LogFileName=…\"`; the per-test `<UnitTestResult>` \
             elements are not a tally",
        );
    };
    let rest = &text[summary_at..];
    let Some(counters_at) = rest.find("<Counters") else {
        return unverifiable(
            "the TRX `<ResultSummary>` carries no `<Counters>` element",
            "re-run and keep the whole report",
        );
    };
    let counters = &rest[counters_at..];
    let counters = match counters.find('>') {
        Some(end) => &counters[..end],
        None => {
            return unverifiable(
                "the TRX `<Counters>` element is unterminated",
                "re-run and keep the whole report",
            )
        }
    };
    let attr = |name: &str| -> Option<u64> {
        let key = format!("{name}=\"");
        let at = counters.find(&key)? + key.len();
        counters[at..].split('"').next()?.trim().parse().ok()
    };
    // Six required attributes. All six are present in every TRX this crate has
    // captured, from both writers; a report missing one is a writer this parser
    // has not been measured against, and guessing its zero is the bug above.
    let names = [
        "total", "executed", "passed", "failed", "error", "timeout", "aborted",
    ];
    let mut read = Vec::new();
    for name in names {
        let Some(value) = attr(name) else {
            return unverifiable(
                format!("the TRX `<Counters>` element has no `{name}` attribute"),
                "a missing counter is unknown, not zero — an absent `failed` attribute \
                 is how a truncated report reads as a clean run",
            );
        };
        read.push(value);
    }
    let (total, executed, passed) = (read[0], read[1], read[2]);
    // `error`, `timeout` and `aborted` are **0 in every capture**, so folding
    // them into `failed` is read off the TRX schema rather than measured. It is
    // the direction that over-reports failure rather than under-reporting it,
    // and `Tally::finish`'s sum check catches the case where the fold
    // double-counts a test already in `failed`.
    let failed = read[3] + read[4] + read[5] + read[6];
    if executed > total {
        return unverifiable(
            format!("the TRX counters disagree: executed={executed} of total={total}"),
            "re-run and keep the whole report",
        );
    }
    let mut tally = Tally {
        total,
        passed,
        failed,
        skipped: total - executed,
        summaries: 1,
        ..Tally::default()
    };
    // `testName` on a `<UnitTestResult>` — the element name is part of the
    // query, and this is the only place in a .NET run where a fully qualified
    // identifier exists for all three frameworks. NUnit and MSTest name bare
    // methods in the console.
    for at in text.match_indices("<UnitTestResult").map(|(i, _)| i) {
        let element = &text[at..];
        let element = &element[..element.find('>').unwrap_or(element.len())];
        if let Some(name) = attribute_value(element, "testName") {
            tally.ids.insert(name);
        }
    }
    tally.finish("TRX")
}

fn attribute_value(element: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let at = element.find(&key)? + key.len();
    let value = element[at..].split('"').next()?;
    // The five predefined XML entities. No captured `testName` carries one, so
    // this is schema-derived rather than measured; it is decoding, and a wrong
    // decode changes an id and never a count.
    Some(
        value
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&"),
    )
}

/// VSTest's per-test lines: `  Failed <name> [2 ms]`, `  Skipped <name>`.
///
/// Two leading spaces and then the verb, which is what keeps `  Error Message:`
/// and NUnit's `1)    at Shipping.NUnit…` stack-trace lines out. The names are
/// fully qualified for xunit and bare method names for NUnit and MSTest, and
/// the duration bracket is not guaranteed — MSTest's `  Skipped
/// InternationalSurcharge` has none.
///
/// Only non-passing tests appear at default verbosity, so this set is normally
/// a subset of the run. `Parsed::test_ids` documents that: *"Empty is normal …
/// and is not evidence of anything."*
fn vstest_test_ids(lines: &[&str]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in lines {
        let Some(rest) = line.strip_prefix("  ") else {
            continue;
        };
        let Some(name) = ["Passed ", "Failed ", "Skipped "]
            .iter()
            .find_map(|verb| rest.strip_prefix(verb))
        else {
            continue;
        };
        let name = strip_trailing_bracket(name.trim());
        if !name.is_empty() {
            out.insert(name.to_string());
        }
    }
    out
}

/// MTP's per-test lines: `failed LargeParcelUsesPerKilo (11ms)`, at column 0
/// and lower-case.
///
/// Only non-passing tests are printed unless `--output Detailed` was passed
/// (`mtp-direct-detailed.txt` is the one capture that names all seven), so this
/// set is normally the failures and skips alone.
fn mtp_test_ids(lines: &[&str]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in lines {
        let Some(name) = ["passed ", "failed ", "skipped "]
            .iter()
            .find_map(|verb| line.strip_prefix(verb))
        else {
            continue;
        };
        // `passed OversizeThreshold (31,True) (0ms)` — the name itself can
        // carry parentheses, so only a trailing duration is cut.
        let name = strip_trailing_duration(name.trim());
        if !name.is_empty() {
            out.insert(name.to_string());
        }
    }
    out
}

/// xunit v3's per-test lines: `    Shipping.V3.…OversizeRejectsHeavyParcel [FAIL]`.
fn xunit_v3_test_ids(lines: &[&str]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in lines {
        let trimmed = line.trim();
        let Some(name) = ["[FAIL]", "[SKIP]", "[PASS]"]
            .iter()
            .find_map(|tag| trimmed.strip_suffix(tag))
        else {
            continue;
        };
        let name = name.trim();
        if !name.is_empty() && !name.contains(' ') {
            out.insert(name.to_string());
        }
    }
    out
}

fn strip_trailing_bracket(name: &str) -> &str {
    match name.strip_suffix(']').and_then(|n| n.rfind(" [")) {
        Some(at) => name[..at].trim_end(),
        None => name,
    }
}

/// Cut the ` (11ms)` MTP appends, and nothing else.
///
/// The bracket has to be a **duration**: digits, then a unit ending in `s`.
/// A theory case carries its own parentheses — `OversizeThreshold (31,True)` in
/// `mtp-direct-detailed.txt` — and a single-argument case would be
/// `OversizeThreshold (31)`, which a "trailing parenthesised group" rule would
/// eat. Every captured MTP line ends in its duration, so the name's own group is
/// never last; requiring the unit means it does not have to be.
fn strip_trailing_duration(name: &str) -> &str {
    let Some(open) = name.strip_suffix(')').and_then(|n| n.rfind(" (")) else {
        return name;
    };
    let inside = &name[open + 2..name.len() - 1];
    let digits = inside
        .trim_end_matches(|c: char| c.is_ascii_alphabetic())
        .to_string();
    let unit = &inside[digits.len()..];
    let is_duration = !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit() || c == '.')
        && unit.ends_with('s');
    if is_duration {
        name[..open].trim_end()
    } else {
        name
    }
}

/// The number after `key` on a line, case-insensitively.
///
/// Lower-cases a copy and indexes into it: `to_ascii_lowercase` changes no
/// byte's length, so an offset found in the copy is a char boundary in it, and
/// the digits are ASCII either way. Non-ASCII in the line (MTP draws its
/// progress as `[✓2/x1/↓1]`) is carried through untouched.
fn counter(line: &str, key: &str) -> Option<u64> {
    let lower = line.to_ascii_lowercase();
    let at = lower.find(&key.to_ascii_lowercase())? + key.len();
    read_count(&lower[at..])
}

fn read_count(rest: &str) -> Option<u64> {
    let digits: String = rest
        .chars()
        .skip_while(char::is_ascii_whitespace)
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// The duration after `key`, in milliseconds.
///
/// Five spellings across the six grammars, all measured: `Duration: 20 ms`
/// (VSTest), `Duration: 77ms` (MTP under `dotnet test`), `duration: 110ms` (MTP
/// direct), `duration: 0.4s` (TerminalLogger), `Time: 0.072s` (xunit v3) and
/// `Total time: 6.5411 Seconds` (detailed). A unit this does not know yields
/// `None` — `duration_ms` is an `Option`, and `None` says *not recorded* where
/// a 0 would say *instant*.
fn duration_after(line: &str, key: &str) -> Option<u64> {
    let lower = line.to_ascii_lowercase();
    let at = lower.find(&key.to_ascii_lowercase())? + key.len();
    let rest = lower[at..].trim_start();
    let number: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let value: f64 = number.parse().ok()?;
    let unit = rest[number.len()..].trim_start();
    // `ms` before `s`, or every millisecond becomes a second.
    if unit.starts_with("ms") {
        Some(value.round() as u64)
    } else if unit.starts_with('s') {
        Some((value * 1000.0).round() as u64)
    } else {
        None
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

    fn counts(p: &Parsed) -> (u64, u64, u64, u64) {
        (p.tests_total, p.passed, p.failed, p.skipped)
    }

    // --- argv ---------------------------------------------------------------

    #[test]
    fn argv_grammar_including_the_two_verify_misses() {
        let argv = |s: &str| s.split(' ').map(str::to_string).collect::<Vec<_>>();
        assert_eq!(from_argv(&argv("dotnet test")), Some(Runner::DotnetTest));
        assert_eq!(
            from_argv(&argv("dotnet test --filter FullyQualifiedName~Oversize")),
            Some(Runner::DotnetTest)
        );
        // `-d` is a bare global flag and is accepted before the subcommand —
        // measured against SDK 9.0.317 (`dotnet -d test --help` prints the test
        // driver's help).
        assert_eq!(from_argv(&argv("dotnet -d test")), Some(Runner::DotnetTest));
        assert_eq!(
            from_argv(&argv("/Users/x/.dotnet/dotnet test")),
            Some(Runner::DotnetTest)
        );
        // Difference 1: `dotnet vstest <assembly>`, which verify reads as "not
        // `test`". `dotnet-vstest-mixed.txt` is its output.
        assert_eq!(
            from_argv(&argv("dotnet vstest ./bin/Debug/net9.0/Shipping.Tests.dll")),
            Some(Runner::DotnetTest)
        );
        // Difference 2: an MTP/xunit-v3 assembly, but only with an option that
        // names it as a test platform.
        assert_eq!(
            from_argv(&argv("dotnet Shipping.Mtp.dll --list-tests")),
            Some(Runner::DotnetTest)
        );
        assert_eq!(
            from_argv(&argv("dotnet exec Shipping.MtpTrx.dll --report-trx")),
            Some(Runner::DotnetTest)
        );
        assert_eq!(
            from_argv(&argv("dotnet Shipping.V3.dll -noColor -stopOnFail")),
            Some(Runner::DotnetTest)
        );
        // xunit v3 negates a switch with a trailing dash.
        assert_eq!(
            from_argv(&argv("dotnet Shipping.V3.dll -failSkips-")),
            Some(Runner::DotnetTest)
        );
        // The named hole: four shipped fixtures come from this argv and it is
        // deliberately not recognised. See `from_argv`'s docs — recognising it
        // would file every `dotnet myservice.dll` as a test check.
        assert_eq!(from_argv(&argv("dotnet Shipping.Mtp.dll")), None);
        // Generic options must not be enough on their own.
        assert_eq!(from_argv(&argv("dotnet myapp.dll --output ./x")), None);
        assert_eq!(from_argv(&argv("dotnet build")), None);
        assert_eq!(from_argv(&argv("dotnet run --project ./x")), None);
        assert_eq!(from_argv(&argv("msbuild /t:VSTest")), None);
        assert_eq!(from_argv(&[]), None);
    }

    // --- the six grammars, every captured state -----------------------------

    #[test]
    fn vstest_piped_summary() {
        assert_eq!(counts(&ok("dotnet-pass.txt")), (6, 6, 0, 0));
        assert_eq!(counts(&ok("dotnet-mixed.txt")), (8, 5, 2, 1));
        assert_eq!(counts(&ok("dotnet-narrowed.txt")), (3, 2, 1, 0));
        assert_eq!(counts(&ok("dotnet-narrowed-green.txt")), (1, 1, 0, 0));
        // Three frameworks, one summary grammar.
        assert_eq!(counts(&ok("dotnet-nunit-mixed.txt")), (7, 4, 2, 1));
        assert_eq!(counts(&ok("dotnet-mstest-mixed.txt")), (7, 4, 2, 1));
        // `dotnet vstest` — no build preamble, identical summary.
        assert_eq!(counts(&ok("dotnet-vstest-mixed.txt")), (8, 5, 2, 1));
        // MTP under `dotnet test`: the same grammar with no column padding.
        assert_eq!(counts(&ok("mtp-dotnettest-mixed.txt")), (7, 4, 2, 1));
        assert_eq!(counts(&ok("mtp-dotnettest-pass.txt")), (7, 7, 0, 0));
        assert_eq!(counts(&ok("mtp-dotnettest-narrowed.txt")), (3, 2, 1, 0));
        assert_eq!(counts(&ok("xunit3-dotnettest-mixed.txt")), (8, 5, 2, 1));
    }

    #[test]
    fn test_ids_are_harvested_in_each_grammars_own_spelling() {
        // Per-test lines are the one thing the six grammars do NOT share, and
        // the console names only non-passing tests unless asked otherwise — so
        // these sets are deliberately partial, which `Parsed::test_ids`
        // documents as normal and not as evidence of anything.

        // VSTest: `  Failed <fully qualified> [2 ms]`, two leading spaces.
        let ids = ok("dotnet-mixed.txt").test_ids;
        assert_eq!(
            ids.iter().cloned().collect::<Vec<_>>(),
            vec![
                "Shipping.Tests.ShippingRateTests.InternationalSurcharge",
                "Shipping.Tests.ShippingRateTests.LargeParcelUsesPerKilo",
                "Shipping.Tests.ShippingRateTests.OversizeRejectsHeavyParcel",
            ],
            "the two failures and the skip; the five passes are not printed at \
             default verbosity"
        );

        // NUnit and MSTest name bare methods through the same logger, and
        // MSTest's skipped line has no `[N ms]` bracket at all.
        assert!(ok("dotnet-nunit-mixed.txt")
            .test_ids
            .contains("LargeParcelUsesPerKilo"));
        assert!(
            ok("dotnet-mstest-mixed.txt")
                .test_ids
                .contains("InternationalSurcharge"),
            "`  Skipped InternationalSurcharge` carries no duration bracket"
        );
        // NUnit writes `1)    at Shipping.NUnit…` stack-trace lines. They must
        // not become test ids — mocha's numbered-failure heuristic does not
        // travel to C#, and neither does reading a stack frame as a name.
        assert!(
            !ok("dotnet-nunit-mixed.txt")
                .test_ids
                .iter()
                .any(|id| id.starts_with("at ") || id.contains("ShippingTests.cs")),
            "a stack frame is not a test id"
        );

        // MTP: lower-case verbs at column 0, and a theory case keeps its own
        // parentheses while the trailing duration is cut.
        let ids = ok("mtp-direct-detailed.txt").test_ids;
        assert_eq!(
            ids.len(),
            7,
            "`--output Detailed` names every test: {ids:?}"
        );
        assert!(ids.contains("OversizeThreshold (31,True)"), "{ids:?}");
        // The same binary through a pty: the ids survive the SGR and the OSC 8
        // hyperlink.
        assert!(ok("mtp-ansi-color.txt")
            .test_ids
            .contains("OversizeRejectsHeavyParcel"));

        // xunit v3: `    Shipping.V3.…OversizeRejectsHeavyParcel [FAIL]`.
        assert!(ok("xunit3-mixed.txt")
            .test_ids
            .contains("Shipping.V3.ShippingRateTests.OversizeRejectsHeavyParcel"));

        // TerminalLogger names no test at all: its per-test detail is an
        // MSBuild `TESTERROR` block carrying a file and a line.
        assert!(ok("dotnet-terminallogger-mixed.txt").test_ids.is_empty());

        // The bracket that gets cut is a duration and only a duration. No
        // capture holds a single-argument theory case, so this is the guard
        // that keeps `Threshold (31)` from being read as `Threshold`.
        assert_eq!(
            strip_trailing_duration("Threshold (31,True) (0ms)"),
            "Threshold (31,True)"
        );
        assert_eq!(strip_trailing_duration("Threshold (31)"), "Threshold (31)");
        assert_eq!(strip_trailing_duration("Threshold (1.5s)"), "Threshold");
        assert_eq!(strip_trailing_bracket("Rate [< 1 ms]"), "Rate");
        assert_eq!(
            strip_trailing_bracket("Rate(kg: 30) [17 ms]"),
            "Rate(kg: 30)"
        );
    }

    #[test]
    fn a_skipped_marker_is_one_space_and_a_failed_marker_is_two() {
        // The padding difference the whitespace split exists for: `Skipped! - `
        // against `Failed!  - `. A parser anchored on either literal loses the
        // other, and MTP pads neither.
        let p = ok("dotnet-all-skipped.txt");
        assert_eq!(counts(&p), (3, 0, 0, 3));
        assert!(
            fixture("dotnet-all-skipped.txt").contains("Skipped! - Failed:"),
            "the fixture is the one-space form"
        );
        assert!(
            fixture("dotnet-mixed.txt").contains("Failed!  - Failed:"),
            "the fixture is the two-space form"
        );
    }

    #[test]
    fn a_solution_run_sums_its_projects_and_never_reads_the_last_line() {
        // Both orderings of the same failing solution. The second is the one
        // where the LAST summary line reads `Passed!  - … Total: 2` — three
        // runs in eight land that way, so a last-line reader is green on a red
        // run, intermittently.
        assert_eq!(counts(&ok("dotnet-multiproject-mixed.txt")), (5, 3, 2, 0));
        assert_eq!(
            counts(&ok("dotnet-multiproject-last-line-green.txt")),
            (5, 3, 2, 0)
        );
        let green = fixture("dotnet-multiproject-last-line-green.txt");
        let last = green
            .lines()
            .rev()
            .find(|l| l.contains("Total:"))
            .unwrap_or_default();
        assert!(
            last.trim_start().starts_with("Passed!"),
            "the fixture's last summary line must be the green one for this test \
             to mean anything: {last}"
        );
    }

    #[test]
    fn terminal_logger_grammar_says_succeeded_not_passed() {
        assert_eq!(counts(&ok("dotnet-terminallogger-pass.txt")), (6, 6, 0, 0));
        assert_eq!(counts(&ok("dotnet-terminallogger-mixed.txt")), (8, 5, 2, 1));
        // No pty at all: `MSBUILDTERMINALLOGGER=on` selects this grammar into a
        // plain pipe, so "stdout is not a tty" does not imply the piped grammar.
        assert_eq!(
            counts(&ok("dotnet-terminallogger-piped-mixed.txt")),
            (8, 5, 2, 1)
        );
        // A zero-test run that DOES carry a tally: parsed as the zero it states.
        assert_eq!(
            counts(&ok("dotnet-terminallogger-zero-tests.txt")),
            (0, 0, 0, 0)
        );
    }

    #[test]
    fn the_detailed_logger_block() {
        let p = ok("dotnet-console-detailed.txt");
        assert_eq!(counts(&p), (8, 5, 2, 1));
        // This is the one console grammar that names every test, passes
        // included.
        assert!(
            p.test_ids.len() >= 8,
            "the detailed logger names all eight: {:?}",
            p.test_ids
        );
        assert!(p
            .test_ids
            .contains("Shipping.Tests.ShippingRateTests.ZeroWeightStillCostsBase"));
        assert!(
            p.test_ids
                .iter()
                .any(|id| id.contains("OversizeThreshold(kg: 30")),
            "a theory case keeps its parameters, and the duration bracket is cut: {:?}",
            p.test_ids
        );
    }

    #[test]
    fn mtp_puts_its_counts_on_the_following_lines() {
        assert_eq!(counts(&ok("mtp-direct-mixed.txt")), (7, 4, 2, 1));
        assert_eq!(counts(&ok("mtp-direct-pass.txt")), (7, 7, 0, 0));
        assert_eq!(counts(&ok("mtp-direct-detailed.txt")), (7, 4, 2, 1));
        assert_eq!(counts(&ok("mtp-direct-narrowed.txt")), (3, 2, 1, 0));
        assert_eq!(counts(&ok("mtp-direct-narrowed-green.txt")), (1, 1, 0, 0));
        // `Zero tests ran` over `total: 0` — a tally, so it is read as one.
        assert_eq!(counts(&ok("mtp-direct-zero-tests.txt")), (0, 0, 0, 0));
        assert_eq!(counts(&ok("mtp-direct-filter-no-match.txt")), (0, 0, 0, 0));
        // A policy violation is not a test failure: one test ran and passed,
        // and the exit code (9) is what carries the violation.
        assert_eq!(counts(&ok("mtp-direct-min-expected.txt")), (1, 1, 0, 0));
    }

    #[test]
    fn xunit_v3_has_no_passed_count_and_two_buckets_nothing_else_has() {
        assert_eq!(counts(&ok("xunit3-nocolor-mixed.txt")), (8, 5, 2, 1));
        assert_eq!(counts(&ok("xunit3-narrowed.txt")), (3, 2, 1, 0));
        assert_eq!(counts(&ok("xunit3-narrowed-green.txt")), (1, 1, 0, 0));
        // `-stopOnFail` ends an 8-test suite at `Total: 1`.
        assert_eq!(counts(&ok("xunit3-stop-on-fail.txt")), (1, 0, 1, 0));
        // `-failSkips` recounts the skip as a failure: same binary, same tests.
        assert_eq!(counts(&ok("xunit3-fail-skips.txt")), (8, 5, 3, 0));
        // The truncated summary — `Total: 0` and no other clause. Accepted only
        // because the total it states is zero.
        assert_eq!(counts(&ok("xunit3-filter-no-match.txt")), (0, 0, 0, 0));
    }

    #[test]
    fn a_truncated_xunit_summary_over_a_non_empty_run_is_refused() {
        // The same shape as `xunit3-filter-no-match.txt` with a non-zero total.
        // Hand-built, and labelled as such: it is a mutation of captured bytes
        // to prove the rule the capture cannot reach, not a claimed capture.
        let mutated = fixture("xunit3-filter-no-match.txt")
            .replace("Shipping.V3  Total: 0", "Shipping.V3  Total: 8");
        let RunnerOutcome::Unverifiable { reason, .. } = parse(&mutated) else {
            panic!("a summary claiming 8 tests with no Failed: clause must be refused");
        };
        assert!(reason.contains("missing a clause"), "{reason}");
    }

    // --- ANSI ---------------------------------------------------------------

    #[test]
    fn colour_parses_including_an_osc_hyperlink_inside_the_summary() {
        // xunit v3 writes ANSI into a plain pipe, unasked.
        assert_eq!(counts(&ok("xunit3-mixed.txt")), (8, 5, 2, 1));
        assert!(
            fixture("xunit3-mixed.txt").contains('\u{1b}'),
            "the fixture must actually carry escapes for this to mean anything"
        );
        // A pty capture, whose summary line wraps the assembly path in an
        // OSC 8 `file://` hyperlink.
        assert_eq!(counts(&ok("xunit3-ansi-color.txt")), (8, 5, 2, 1));
        assert_eq!(counts(&ok("mtp-ansi-color.txt")), (7, 4, 2, 1));
        assert!(
            fixture("mtp-ansi-color.txt").contains("\u{1b}]8;;file://"),
            "a CSI-only stripper would leave `]8;;file://…` inside the line being parsed"
        );
    }

    // --- the runs that must never read as clean -----------------------------

    #[test]
    fn a_zero_test_run_is_never_a_clean_run() {
        // No tally at all: prose is not a count.
        assert!(why("dotnet-zero-tests.txt").contains("discovered no test"));
        assert!(why("dotnet-filter-no-match.txt").contains("matched no test"));
        // A tally of zero: parsed as zero, and §6.3 refuses it on
        // `tests_total == 0`. Either way it is not a clean run.
        for name in [
            "dotnet-terminallogger-zero-tests.txt",
            "mtp-direct-zero-tests.txt",
            "mtp-direct-filter-no-match.txt",
            "xunit3-filter-no-match.txt",
            "dotnet-zero-tests.trx",
        ] {
            let p = ok(name);
            assert_eq!(p.tests_total, 0, "{name}");
            assert_eq!(p.passed, 0, "{name}");
        }
    }

    #[test]
    fn a_verdict_word_that_contradicts_its_tally_is_unverifiable() {
        // `Failed! - Failed: 0, Passed: 0, Skipped: 0, Total: 0` — MTP under
        // `dotnet test` on a project with no tests. Reading the tally alone
        // gives `failed = 0` for a suite that failed to be a suite.
        let reason = why("mtp-dotnettest-zero-tests.txt");
        assert!(
            reason.contains("verdict word and the tally disagree"),
            "{reason}"
        );
    }

    #[test]
    fn a_compile_error_and_a_crashed_host_are_not_zero_failures() {
        assert!(why("dotnet-compile-error.txt").contains("did not compile"));
        assert!(why("dotnet-testhost-crash.txt").contains("aborted"));
        // `--blame-crash` adds one line and changes nothing else.
        assert!(why("dotnet-blame-crash.txt").contains("aborted"));
        assert!(why("dotnet-no-build-missing-binary.txt").contains("rejected its argument"));
    }

    #[test]
    fn a_rejected_option_and_a_failing_suite_are_the_same_bytes() {
        // `dotnet test -- --report-trx` against a project without the TRX
        // extension. The option error goes to the host's log file; stdout gets
        // `error run failed` and no summary — the identical shape a genuinely
        // failing run produces. So neither may be read as a tally.
        let reason = why("mtp-dotnettest-bad-option.txt");
        assert!(reason.contains("error run failed"), "{reason}");
    }

    #[test]
    fn a_discovery_listing_is_not_a_run() {
        let reason = why("mtp-direct-list-tests.txt");
        assert!(reason.contains("listing"), "{reason}");
    }

    #[test]
    fn a_documented_flag_cannot_zero_a_failure_here() {
        // Both runs exit 0 by request and both summaries still say two
        // failures. A verifier that reads exit codes is defeated by a flag in
        // `--help`; one that reads the summary is not.
        assert_eq!(ok("mtp-direct-ignore-exit-code.txt").failed, 2);
        assert_eq!(ok("xunit3-ignore-failures.txt").failed, 2);
    }

    // --- TRX ----------------------------------------------------------------

    #[test]
    fn trx_counters_are_read_and_skipped_is_derived() {
        assert_eq!(counts(&ok("dotnet-mixed.trx")), (8, 5, 2, 1));
        assert_eq!(counts(&ok("dotnet-pass.trx")), (6, 6, 0, 0));
        assert_eq!(counts(&ok("dotnet-zero-tests.trx")), (0, 0, 0, 0));
        // The MTP writer, whose `notExecuted` attribute IS populated. Both
        // files land on the same skipped count because it is derived from
        // `total - executed` and not read from the attribute.
        assert_eq!(counts(&ok("mtp-mixed.trx")), (7, 4, 2, 1));
        assert!(
            fixture("dotnet-mixed.trx").contains("notExecuted=\"0\""),
            "the VSTest writer says notExecuted=0 for a run with a skipped test"
        );
        assert!(
            fixture("mtp-mixed.trx").contains("notExecuted=\"1\""),
            "the MTP writer says notExecuted=1 for the same situation"
        );
    }

    #[test]
    fn trx_names_every_test_and_only_from_unit_test_results() {
        let p = ok("dotnet-mixed.trx");
        assert_eq!(
            p.test_ids.len(),
            8,
            "a TRX names all eight, fully qualified: {:?}",
            p.test_ids
        );
        assert!(p
            .test_ids
            .contains("Shipping.Tests.ShippingRateTests.InternationalSurcharge"));
        // The document carries 12 `outcome="…"` attributes for 8 tests, three
        // of them on `<RunInfo>` elements. Counting outcomes document-wide is
        // wrong by exactly that much; the element name is part of the query.
        assert_eq!(
            fixture("dotnet-mixed.trx").matches("outcome=\"").count(),
            12
        );
    }

    #[test]
    fn a_truncated_trx_is_refused_where_a_defaulted_attribute_would_read_green() {
        let whole = fixture("dotnet-mixed.trx");
        let cut = &whole[..whole.find("<ResultSummary").unwrap_or(whole.len())];
        let reason = match parse(cut) {
            RunnerOutcome::Unverifiable { reason, .. } => reason,
            RunnerOutcome::Parsed(p) => panic!("a TRX cut before its counters parsed to {p:?}"),
        };
        assert!(reason.contains("cut off mid-write"), "{reason}");

        // And a counter removed rather than the tail: the case
        // `parse_junit_xml` was measured to get wrong before it was fixed.
        let no_failed = whole.replace(" failed=\"2\"", "");
        let reason = match parse(&no_failed) {
            RunnerOutcome::Unverifiable { reason, .. } => reason,
            RunnerOutcome::Parsed(p) => {
                panic!("a TRX with no `failed` attribute parsed to {p:?} — a defaulted zero")
            }
        };
        assert!(reason.contains("no `failed` attribute"), "{reason}");
    }

    #[test]
    fn a_report_this_reader_does_not_own_says_which_reader_does() {
        // Both are shipped C# fixtures. Neither is read here, and both say so
        // rather than returning an empty parse.
        let reason = why("xunit3-junit.xml");
        assert!(reason.contains("JUnit XML"), "{reason}");
        let reason = why("dotnet-junit-logger-mixed.xml");
        assert!(reason.contains("JUnit XML"), "{reason}");
        let reason = why("xunit3-ctrf.json");
        assert!(reason.contains("CTRF"), "{reason}");
    }

    // --- the invariants that span every fixture -----------------------------

    /// Every C# fixture in the directory, by its own name.
    fn every_csharp_fixture() -> Vec<String> {
        let dir = format!("{}/tests/fixtures/runners", env!("CARGO_MANIFEST_DIR"));
        let mut out: Vec<String> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{dir}: {e}"))
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| {
                n.starts_with("dotnet-") || n.starts_with("mtp-") || n.starts_with("xunit3-")
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn the_six_grammars_are_mutually_exclusive_on_every_capture() {
        // The claim the module docs open with, gated instead of asserted. The
        // C# section of this crate has already had three prose counts drift
        // (see `langs::csharp`'s module docs); a measurement stated in a comment
        // and checked nowhere is the shape that drifts.
        let mut console = 0usize;
        for name in every_csharp_fixture() {
            if !name.ends_with(".txt") {
                continue;
            }
            console += 1;
            let plain = strip_ansi(&fixture(&name));
            let lines = logical_lines(&plain);
            let anchors = [
                (
                    "VSTest/MTP one-line",
                    lines.iter().any(|l| is_vstest_summary_shaped(l)),
                ),
                (
                    "TerminalLogger",
                    lines
                        .iter()
                        .any(|l| l.trim_start().starts_with("Test summary:")),
                ),
                (
                    "console detailed",
                    lines
                        .iter()
                        .any(|l| l.trim_start().starts_with("Total tests:")),
                ),
                (
                    "MTP direct",
                    lines.iter().any(|l| l.contains("Test run summary:")),
                ),
                (
                    "xunit v3",
                    lines
                        .iter()
                        .any(|l| l.trim() == "=== TEST EXECUTION SUMMARY ==="),
                ),
            ];
            let hit: Vec<&str> = anchors
                .iter()
                .filter(|(_, y)| *y)
                .map(|(n, _)| *n)
                .collect();
            assert!(
                hit.len() <= 1,
                "{name} carries {} summary grammars at once ({hit:?}), so the dispatch \
                 order in `parse_console` decides which one wins — and the module docs \
                 claim it never has to",
                hit.len()
            );
        }
        assert_eq!(
            console, 47,
            "the module docs say 47 console fixtures; the directory holds {console}"
        );
    }

    #[test]
    fn every_csharp_capture_gets_an_answer_and_never_a_silent_zero() {
        // The sweep: nothing panics, nothing comes back as a `Parsed` with a
        // total of zero passed off as a clean run, and every refusal carries the
        // hint §6.2 requires.
        let mut parsed_count = 0usize;
        let mut refused = 0usize;
        for name in every_csharp_fixture() {
            match parse(&fixture(&name)) {
                RunnerOutcome::Parsed(p) => {
                    parsed_count += 1;
                    assert_eq!(
                        p.passed + p.failed + p.skipped,
                        p.tests_total,
                        "{name} parsed to counts that do not add up: {p:?}"
                    );
                    assert!(
                        p.tests_total > 0 || (p.passed == 0 && p.failed == 0),
                        "{name} reported results for a run of zero tests: {p:?}"
                    );
                }
                RunnerOutcome::Unverifiable { reason, hint } => {
                    refused += 1;
                    assert!(!reason.is_empty() && !hint.is_empty(), "{name}");
                }
            }
        }
        assert_eq!(
            (parsed_count, refused),
            (42, 12),
            "54 C# fixtures: 42 carry a tally this parser reads and 12 are refused — \
             the compile error, the two aborted runs (`testhost-crash`, `blame-crash`), \
             the missing binary, the two VSTest silences (`zero-tests`, \
             `filter-no-match`), the rejected MTP option, MTP's zero-test `Failed!`, \
             the discovery listing, the two JUnit reports and the CTRF report"
        );
    }

    #[test]
    fn no_captured_dotnet_fixture_ever_parses_to_a_clean_run_it_was_not() {
        // Every C# fixture in the directory, run through the parser: whatever
        // it returns, a run that this crate's README records as having failures
        // must never come back with `failed == 0`.
        let dir = format!("{}/tests/fixtures/runners", env!("CARGO_MANIFEST_DIR"));
        let failing: &[&str] = &[
            "dotnet-mixed.txt",
            "dotnet-narrowed.txt",
            "dotnet-nunit-mixed.txt",
            "dotnet-mstest-mixed.txt",
            "dotnet-multiproject-mixed.txt",
            "dotnet-multiproject-last-line-green.txt",
            "dotnet-console-detailed.txt",
            "dotnet-terminallogger-mixed.txt",
            "dotnet-terminallogger-piped-mixed.txt",
            "dotnet-vstest-mixed.txt",
            "mtp-direct-mixed.txt",
            "mtp-direct-detailed.txt",
            "mtp-direct-narrowed.txt",
            "mtp-direct-ignore-exit-code.txt",
            "mtp-ansi-color.txt",
            "mtp-dotnettest-mixed.txt",
            "mtp-dotnettest-narrowed.txt",
            "xunit3-mixed.txt",
            "xunit3-nocolor-mixed.txt",
            "xunit3-narrowed.txt",
            "xunit3-ignore-failures.txt",
            "xunit3-stop-on-fail.txt",
            "xunit3-fail-skips.txt",
            "xunit3-ansi-color.txt",
            "xunit3-dotnettest-mixed.txt",
            "dotnet-mixed.trx",
            "mtp-mixed.trx",
        ];
        for name in failing {
            let path = format!("{dir}/{name}");
            assert!(
                std::path::Path::new(&path).exists(),
                "{name} is named here and missing from the fixture directory"
            );
            let p = ok(name);
            assert!(
                p.failed > 0,
                "{name} is a failing run and parsed to failed = 0: {p:?}"
            );
        }
    }
}
