//! BUILD-SPEC §6.2 — runner output parsers.
//!
//! # The rule that shapes every line of this file
//!
//! > *"Unknown runner → `unverifiable` with a hint on how to enable parsing."*
//! > *"Never collapse `unverifiable` into `supported`."* (§6.2, §6)
//!
//! So the return type is not `Option<Parsed>`. An `Option` invites
//! `unwrap_or_default()`, and a defaulted `Parsed` is `{total: 0, failed: 0}` —
//! which reads downstream as *a clean run*. The one plausible mistake in this
//! module turns "I could not tell" into "everything passed", which is the exact
//! failure the product exists to catch, committed by the product.
//!
//! [`RunnerOutcome`] therefore has no default and no `Parsed` inside its
//! `Unverifiable` arm. A caller that wants numbers has to handle the case where
//! there are none.
//!
//! # Parsing is over stdout+stderr, never over the agent's description
//!
//! §0 principle 2. What the agent said it ran is a hint; what the process wrote
//! is the evidence.

use std::collections::BTreeSet;

/// Which runner produced some output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Runner {
    Pytest,
    CargoTest,
    GoTest,
    Jest,
    Mocha,
    DotnetTest,
    Maven,
    Gradle,
    /// A JUnit XML report, whoever produced it. §6.2's generic fallback.
    JUnitXml,
}

impl Runner {
    pub fn as_str(self) -> &'static str {
        match self {
            Runner::Pytest => "pytest",
            Runner::CargoTest => "cargo test",
            Runner::GoTest => "go test",
            Runner::Jest => "jest",
            Runner::Mocha => "mocha",
            Runner::DotnetTest => "dotnet test",
            Runner::Maven => "mvn test",
            Runner::Gradle => "gradle test",
            Runner::JUnitXml => "junit-xml",
        }
    }

    /// Recognise a runner from its argv.
    ///
    /// Parses the argv rather than matching a substring of a command line:
    /// §13's obfuscated-command defence. `sh -c "pytest -q && echo ok"` is
    /// handled by the caller splitting the payload, not by this seeing the
    /// word `pytest` somewhere in a string.
    ///
    /// # An arm here is only worth what the capture layer can deliver to it
    ///
    /// Every argv that reaches this on the live path comes from an `exec`
    /// event, and `exec` events are written by exactly one thing: the PATH
    /// shim (`thruline_capture::shim`). So `argv[0]` is always a program in
    /// `shim::WRAPPED`, and an arm whose program is not in that list is
    /// **unreachable from real capture** however correct it is.
    ///
    /// That is not hypothetical. The `python`/`python3` arm below parsed
    /// `python -m pytest` for as long as it has existed while `python` was not
    /// wrapped — a parser written for input the capture layer could not
    /// produce, discovered only when a corpus was measured. The arms still
    /// unreachable from `WRAPPED` today are `py.test`, `jest`, `vitest`,
    /// `mocha` and `gradlew`; each is reachable only through the
    /// `npx`/`npm`/`pnpm`/`yarn` arm, through a `sh -c` payload, or through
    /// re-execution, and `gradlew` is doubly so because it is invoked as
    /// `./gradlew`, a path, which never consults PATH at all.
    ///
    /// **Adding an arm and adding the wrapper are one change.** Half of it is
    /// worth nothing, and reads as coverage that does not exist.
    pub fn from_argv(argv: &[String]) -> Option<Runner> {
        let program = argv.first()?;
        let base = program.rsplit('/').next().unwrap_or(program);
        match base {
            "pytest" | "py.test" => Some(Runner::Pytest),
            "cargo" => argv
                .iter()
                .skip(1)
                .find(|a| !a.starts_with('-'))
                .filter(|sub| *sub == "test" || *sub == "nextest")
                .map(|_| Runner::CargoTest),
            "go" => argv
                .iter()
                .skip(1)
                .find(|a| !a.starts_with('-'))
                .filter(|sub| *sub == "test")
                .map(|_| Runner::GoTest),
            "jest" | "vitest" => Some(Runner::Jest),
            "mocha" => Some(Runner::Mocha),
            "dotnet" => argv
                .iter()
                .skip(1)
                .find(|a| !a.starts_with('-'))
                .filter(|sub| *sub == "test")
                .map(|_| Runner::DotnetTest),
            "mvn" => argv
                .iter()
                .skip(1)
                .any(|a| a == "test" || a == "verify")
                .then_some(Runner::Maven),
            "gradle" | "gradlew" => argv
                .iter()
                .skip(1)
                .any(|a| a == "test" || a == "check")
                .then_some(Runner::Gradle),
            // `npx jest`, `npm test -- --ci`, `yarn vitest run`.
            "npx" | "npm" | "pnpm" | "yarn" => {
                let inner: Vec<String> = argv
                    .iter()
                    .skip(1)
                    .skip_while(|a| {
                        a.starts_with('-') || matches!(a.as_str(), "run" | "exec" | "--")
                    })
                    .cloned()
                    .collect();
                match inner.first().map(String::as_str) {
                    Some("jest") | Some("vitest") => Some(Runner::Jest),
                    Some("mocha") => Some(Runner::Mocha),
                    // `npm test` runs whatever `scripts.test` is, which this
                    // cannot know. The shim captures the nested exec, and THAT
                    // is where the runner is recognised — §6.2's "pass-through
                    // to nested runner detection via child exec events".
                    _ => None,
                }
            }
            "python" | "python3" => {
                // `python -m pytest`
                let mut it = argv.iter().skip(1);
                while let Some(a) = it.next() {
                    if a == "-m" {
                        return it.next().filter(|m| *m == "pytest").map(|_| Runner::Pytest);
                    }
                }
                None
            }
            // `uv run pytest`, `uvx pytest`, `uv tool run pytest`.
            //
            // **219 of Terminal-Bench's 241 tasks run their tests this way**,
            // and without this arm wrapping `uv` buys an exec row with no
            // runner attached: still no test check, still `unverifiable`. The
            // wrapper and this arm are one change; either alone does nothing.
            //
            // The recursion is deliberate — `uv run cargo test` and
            // `uv run go test` get the same treatment as `uv run pytest`
            // without restating any of it.
            "uv" | "uvx" => {
                let mut at = 1;
                // `uv` needs its `run` subcommand; `uvx` IS `uv tool run`, so
                // it needs none. Requiring it is what keeps `uv pip install
                // pytest` — which installs a runner and runs nothing — from
                // being read as a test run.
                if base == "uv" {
                    let mut saw_run = false;
                    while let Some(a) = argv.get(at) {
                        at += 1;
                        if a == "run" {
                            saw_run = true;
                            break;
                        }
                        // `uv tool run <runner>`.
                        if a == "tool" {
                            continue;
                        }
                        if !a.starts_with('-') {
                            return None;
                        }
                        if UV_VALUE_OPTIONS.contains(&a.as_str()) {
                            at += 1;
                        }
                    }
                    if !saw_run {
                        return None;
                    }
                }
                // Skip `uv run`'s own options before the program. A value that
                // travels in a separate word is consumed with its option, so
                // `uv run --with pytest mytool` reads `mytool` as the program
                // rather than mistaking the *dependency* `pytest` for it.
                while let Some(a) = argv.get(at) {
                    if a == "--" {
                        at += 1;
                        continue;
                    }
                    if !a.starts_with('-') {
                        break;
                    }
                    at += 1;
                    if UV_VALUE_OPTIONS.contains(&a.as_str()) {
                        at += 1;
                    }
                }
                // An unlisted value-taking option leaves its value standing
                // where the program should be, which recurses to `None` —
                // `unverifiable`, never a runner this did not see.
                Runner::from_argv(argv.get(at..).unwrap_or(&[]))
            }
            _ => None,
        }
    }
}

/// `uv run` options whose value is a separate word.
///
/// Only the ones whose value could plausibly *be* a program name are listed:
/// the point is not completeness, it is that `--with pytest` must not be read
/// as `pytest`. Anything missing here costs recall (the parse returns `None`
/// and the claim is `unverifiable`), never precision.
const UV_VALUE_OPTIONS: &[&str] = &[
    "--with",
    "--with-editable",
    "--with-requirements",
    "--python",
    "-p",
    "--directory",
    "--project",
    "--package",
    "--extra",
    "--group",
    "--env-file",
    "--index",
    "--from",
];

/// The §3.3 `parsed` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub runner: Runner,
    pub tests_total: u64,
    pub passed: u64,
    pub failed: u64,
    pub skipped: u64,
    pub xfail: u64,
    /// Named tests the output identified. Empty is normal — `pytest -q` names
    /// only failures — and is not evidence of anything.
    pub test_ids: BTreeSet<String>,
    pub duration_ms: Option<u64>,
}

/// What a parse attempt concluded.
///
/// **No `Default`, and no `Parsed` in the `Unverifiable` arm.** See the module
/// docs: a defaulted `Parsed` reads as a clean run, and that is the one mistake
/// this module must make impossible rather than merely avoid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnerOutcome {
    Parsed(Parsed),
    /// The output could not be parsed. `hint` says what would fix it.
    Unverifiable {
        reason: String,
        hint: String,
    },
}

impl RunnerOutcome {
    pub fn parsed(&self) -> Option<&Parsed> {
        match self {
            RunnerOutcome::Parsed(p) => Some(p),
            RunnerOutcome::Unverifiable { .. } => None,
        }
    }

    pub fn is_unverifiable(&self) -> bool {
        matches!(self, RunnerOutcome::Unverifiable { .. })
    }
}

/// Parse a runner's combined output.
/// Remove ANSI escape sequences.
///
/// **CI is where colour lives.** Real `mocha --color` emits
/// `\x1b[32m 2 passing\x1b[0m\x1b[90m (2ms)\x1b[0m`, and a forced-colour CI
/// capture therefore produced no verdict at all. It failed safe —
/// `unverifiable`, never a wrong number — but a parser that cannot read the
/// output CI actually produces is a parser that does not work where it matters.
///
/// The transport decoder already strips these for redaction; the parsers need
/// the same treatment, and it belongs here so every runner gets it rather than
/// each remembering to.
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            // CSI: `ESC [ … final-byte`
            Some('[') => {
                chars.next();
                for c in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&c) {
                        break;
                    }
                }
            }
            // OSC: `ESC ] … BEL` or `ESC ] … ESC \`
            Some(']') => {
                chars.next();
                while let Some(c) = chars.next() {
                    if c == '\u{7}' {
                        break;
                    }
                    if c == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            // Any other two-character escape.
            Some(_) => {
                chars.next();
            }
            None => {}
        }
    }
    out
}

pub fn parse(runner: Option<Runner>, output: &str) -> RunnerOutcome {
    let plain = strip_ansi(output);
    let output: &str = &plain;
    // A JUnit XML report anywhere in the output is the most reliable thing
    // present, whatever produced it: §6.2 names it as the generic fallback, and
    // mvn/gradle/dotnet all emit it.
    if matches!(
        runner,
        Some(Runner::Maven)
            | Some(Runner::Gradle)
            | Some(Runner::DotnetTest)
            | Some(Runner::JUnitXml)
    ) {
        if let Some(parsed) = parse_junit_xml(output, runner.unwrap_or(Runner::JUnitXml)) {
            return parsed;
        }
    }
    match runner {
        Some(Runner::Pytest) => parse_pytest(output),
        Some(Runner::CargoTest) => parse_cargo_test(output),
        Some(Runner::GoTest) => parse_go_test(output),
        Some(Runner::Jest) => parse_jest(output),
        Some(Runner::Mocha) => parse_mocha(output),
        Some(other) => RunnerOutcome::Unverifiable {
            reason: format!(
                "no {} summary could be read from the output",
                other.as_str()
            ),
            hint: "these runners are parsed from a JUnit XML report; pass \
                   `--logger trx`/`surefire` or point ThruLine at the XML"
                .to_string(),
        },
        None => RunnerOutcome::Unverifiable {
            reason: "the runner was not recognised from its argv".to_string(),
            hint: "supported runners are pytest and cargo test; for anything else, \
                   emit JUnit XML and point ThruLine at it"
                .to_string(),
        },
    }
}

/// `pytest`'s terminal summary line.
///
/// The authority is the `===== ... =====` summary, not the progress dots: `-q`
/// prints dots, `-v` prints names, and both print the same summary. Parsing the
/// summary is what makes the two verbose levels agree.
fn parse_pytest(output: &str) -> RunnerOutcome {
    // Two shapes, both real, and only the first was in the fixtures.
    //
    // Default and `-v` print the banner: `===== 2 passed in 0.01s =====`.
    // **`-q` prints `2 passed in 0.00s` with no banner at all** — measured
    // against pytest 9.0.1, captured through the shim, after this parser had
    // been "proven" against hand-written `-q` fixtures that carried a banner
    // pytest does not emit. The fixture had drifted toward the rule.
    //
    // The bannerless form needs a strict grammar or a test that prints
    // "2 passed" becomes a summary. [`is_pytest_tally`] requires the WHOLE line
    // to be count/word pairs with an optional `in <duration>` tail, so
    // `assert "3 passed" in out` is not one.
    let summary = output.lines().rev().find(|l| {
        (l.contains('=')
            && (l.contains("passed")
                || l.contains("failed")
                || l.contains("error")
                || l.contains("no tests ran")))
            || is_pytest_tally(l)
    });
    let Some(summary) = summary else {
        return RunnerOutcome::Unverifiable {
            reason: "no pytest summary line found in the output".to_string(),
            hint: "pytest writes `===== N passed in Xs =====` last; if the output \
                   was truncated or the run crashed, the result is unknown, not clean"
                .to_string(),
        };
    };

    let mut p = Parsed {
        runner: Runner::Pytest,
        tests_total: 0,
        passed: 0,
        failed: 0,
        skipped: 0,
        xfail: 0,
        test_ids: BTreeSet::new(),
        duration_ms: None,
    };
    // `N passed`, `N failed`, `N skipped`, `N xfailed`, `N xpassed`, `N error(s)`
    let tokens: Vec<&str> = summary.split_whitespace().collect();
    for pair in tokens.windows(2) {
        let Ok(n) = pair[0].parse::<u64>() else {
            continue;
        };
        let word = pair[1].trim_end_matches(',');
        match word {
            "passed" => p.passed += n,
            "failed" | "error" | "errors" => p.failed += n,
            "skipped" | "deselected" => p.skipped += n,
            "xfailed" => p.xfail += n,
            // An xpassed test is a test expected to fail that did not. It is a
            // PASS for counting and a warning for §6.3; counted here, judged
            // there.
            "xpassed" => p.passed += n,
            _ => {}
        }
    }
    p.tests_total = p.passed + p.failed + p.skipped + p.xfail;
    p.duration_ms = duration_after_in(summary);
    p.test_ids = pytest_ids(output);

    if p.tests_total == 0 && !summary.contains("no tests ran") {
        return RunnerOutcome::Unverifiable {
            reason: format!("a pytest summary was found but carried no counts: {summary:?}"),
            hint: "this usually means a collection error; the run produced no test \
                   outcomes, which is not the same as passing"
                .to_string(),
        };
    }
    RunnerOutcome::Parsed(p)
}

/// `go test` — both the plain and the `-json` forms.
fn parse_go_test(output: &str) -> RunnerOutcome {
    let mut p = blank(Runner::GoTest);
    let mut saw = false;
    for line in output.lines() {
        let t = line.trim();
        // `-json`: one object per event, `"Action":"pass"` at Test level.
        if t.starts_with('{') && t.contains("\"Action\"") {
            if !t.contains("\"Test\"") {
                continue;
            }
            saw = true;
            if t.contains("\"Action\":\"pass\"") {
                p.passed += 1;
            } else if t.contains("\"Action\":\"fail\"") {
                p.failed += 1;
            } else if t.contains("\"Action\":\"skip\"") {
                p.skipped += 1;
            }
            continue;
        }
        // Plain `-v`: `--- PASS: TestX (0.00s)`.
        if let Some(rest) = t.strip_prefix("--- ") {
            saw = true;
            if rest.starts_with("PASS") {
                p.passed += 1;
            } else if rest.starts_with("FAIL") {
                p.failed += 1;
            } else if rest.starts_with("SKIP") {
                p.skipped += 1;
            }
            if let Some(name) = rest.split(':').nth(1) {
                p.test_ids
                    .insert(name.split_whitespace().next().unwrap_or("").to_string());
            }
        }
    }
    if !saw {
        return RunnerOutcome::Unverifiable {
            reason: "no per-test result lines found in the go test output".to_string(),
            hint: "run `go test -v` or `go test -json`; a bare `ok  package` line \
                   reports no counts, and a package that compiled is not a suite that ran"
                .to_string(),
        };
    }
    p.tests_total = p.passed + p.failed + p.skipped;
    RunnerOutcome::Parsed(p)
}

/// jest's `--json` report. §6.2 names it explicitly, and it is the form CI
/// configurations most often produce.
fn parse_jest_json(output: &str) -> Option<RunnerOutcome> {
    let at = output.find("{\"numFailedTestSuites\"").or_else(|| {
        output
            .find("\"numTotalTests\"")
            .map(|i| output[..i].rfind('{').unwrap_or(0))
    })?;
    let value: serde_json::Value = serde_json::from_str(output[at..].trim()).ok()?;
    let n = |k: &str| value.get(k).and_then(serde_json::Value::as_u64);
    let total = n("numTotalTests")?;
    let mut p = blank(Runner::Jest);
    p.tests_total = total;
    p.passed = n("numPassedTests").unwrap_or(0);
    p.failed = n("numFailedTests").unwrap_or(0);
    p.skipped = n("numPendingTests").unwrap_or(0) + n("numTodoTests").unwrap_or(0);

    // The same suite-level rule as the text form. A runtime error in a suite
    // means the tally counts only what loaded.
    let broken = n("numRuntimeErrorTestSuites").unwrap_or(0);
    let failed_suites = n("numFailedTestSuites").unwrap_or(0);
    if p.failed == 0 && (broken > 0 || failed_suites > 0) {
        return Some(RunnerOutcome::Unverifiable {
            reason: format!(
                "{failed_suites} test suite(s) failed while no individual test did — the \
                 counts describe only the suites that loaded"
            ),
            hint: "a suite that fails to load contributes zero tests, so the tally looks \
                   clean; fix the load error and re-run"
                .to_string(),
        });
    }
    if total == 0 {
        return Some(RunnerOutcome::Unverifiable {
            reason: "the jest report counts zero tests".to_string(),
            hint: "a run that produced no test outcomes is not a passing run".to_string(),
        });
    }
    Some(RunnerOutcome::Parsed(p))
}

/// Jest's colon summary and Vitest's separately validated terminal summary.
fn parse_jest(output: &str) -> RunnerOutcome {
    if let Some(outcome) = parse_jest_json(output) {
        return outcome;
    }
    if output
        .lines()
        .any(|line| line.trim_start().starts_with("Test Files "))
    {
        return parse_vitest_text(output);
    }
    let line = output
        .lines()
        .rev()
        .find(|l| l.trim_start().starts_with("Tests:"));
    let Some(line) = line else {
        return RunnerOutcome::Unverifiable {
            reason: "no `Tests:` summary line found in the jest/vitest output".to_string(),
            hint: "jest and vitest print `Tests: N passed, N total` last; a truncated \
                   or crashed run reports nothing, which is not a clean run"
                .to_string(),
        };
    };
    let mut p = blank(Runner::Jest);
    let mut total = 0u64;
    let tokens: Vec<&str> = line.split_whitespace().collect();
    for pair in tokens.windows(2) {
        let Ok(n) = pair[0].parse::<u64>() else {
            continue;
        };
        match pair[1].trim_end_matches(',') {
            "passed" => p.passed += n,
            "failed" => p.failed += n,
            "skipped" | "todo" => p.skipped += n,
            "total" => total = n,
            _ => {}
        }
    }
    p.tests_total = if total > 0 {
        total
    } else {
        p.passed + p.failed + p.skipped
    };

    // **`Tests:` counts only what loaded.** A file with a syntax error
    // contributes zero tests, so a whole broken suite leaves a tidy green
    // tally: measured on jest 30 as `Test Suites: 1 failed, 1 passed, 2 total`
    // beside `Tests: 2 passed, 2 total`. That is the `conftest collect_ignore`
    // cheat in a different runner, and reading the tally alone walks into it.
    if let Some(suites) = output
        .lines()
        .rev()
        .find(|l| l.trim_start().starts_with("Test Suites:"))
    {
        let failed_suites: u64 = suites
            .split_whitespace()
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|w| w[1].trim_end_matches(',') == "failed")
            .filter_map(|w| w[0].parse::<u64>().ok())
            .sum();
        if failed_suites > 0 && p.failed == 0 {
            return RunnerOutcome::Unverifiable {
                reason: format!(
                    "{failed_suites} test suite(s) failed while no individual test did — \
                     the counts describe only the suites that loaded"
                ),
                hint: "a suite that fails to load contributes zero tests, so the tally \
                       looks clean; fix the load error and re-run"
                    .to_string(),
            };
        }
    }

    if p.tests_total == 0 {
        return RunnerOutcome::Unverifiable {
            reason: format!("a jest summary was found but carried no counts: {line:?}"),
            hint: "a run that produced no test outcomes is not a passing run".to_string(),
        };
    }
    RunnerOutcome::Parsed(p)
}

/// Vitest 4's observed default reporter: `Tests  1 passed (1)`.
/// Require its complete summary and consistent file/test totals; accepting only
/// the passing line would hide load errors and a cut before the terminal footer.
fn parse_vitest_text(output: &str) -> RunnerOutcome {
    let unknown = || {
        RunnerOutcome::Unverifiable {
        reason: "Vitest summary is incomplete, inconsistent, or reports a suite/runtime error".into(),
        hint: "capture the complete Test Files, Tests, Start at and Duration summary; repair load errors before rerunning".into(),
    }
    };
    fn counts(text: &str) -> Option<(u64, u64, u64, u64)> {
        let (buckets, total) = text.trim().rsplit_once(" (")?;
        let total: u64 = total.strip_suffix(')')?.parse().ok()?;
        let mut seen = BTreeSet::new();
        let (mut passed, mut failed, mut skipped) = (0u64, 0u64, 0u64);
        for bucket in buckets.split('|') {
            let words: Vec<_> = bucket.split_whitespace().collect();
            if words.len() != 2 || !seen.insert(words[1]) {
                return None;
            }
            let n: u64 = words[0].parse().ok()?;
            match words[1] {
                "passed" => passed = n,
                "failed" => failed = n,
                "skipped" | "todo" => skipped = skipped.checked_add(n)?,
                _ => return None,
            }
        }
        (passed.checked_add(failed)?.checked_add(skipped)? == total)
            .then_some((total, passed, failed, skipped))
    }
    let lines: Vec<_> = output.lines().map(str::trim).collect();
    let summaries: Vec<_> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| line.strip_prefix("Test Files ").map(|s| (i, s)))
        .collect();
    let [(at, files)] = summaries.as_slice() else {
        return unknown();
    };
    let Some((_, _, failed_files, _)) = counts(files) else {
        return unknown();
    };
    let Some(tests) = lines.get(at + 1).and_then(|s| s.strip_prefix("Tests ")) else {
        return unknown();
    };
    let Some((total, passed, failed, skipped)) = counts(tests) else {
        return unknown();
    };
    if total == 0
        || (failed_files > 0 && failed == 0)
        || !lines
            .get(at + 2)
            .is_some_and(|s| s.starts_with("Start at "))
        || !lines
            .get(at + 3)
            .is_some_and(|s| s.starts_with("Duration ") && s.ends_with(')'))
        || lines.iter().any(|s| s.starts_with("Errors "))
    {
        return unknown();
    }
    let mut p = blank(Runner::Jest);
    p.tests_total = total;
    p.passed = passed;
    p.failed = failed;
    p.skipped = skipped;
    RunnerOutcome::Parsed(p)
}

/// mocha's spec reporter.
///
/// `4 passing (23ms)` carries a trailing duration, so the count is read from a
/// token window rather than from a line suffix — a suffix match found nothing
/// and reported zero passing, which would have read as a suite that ran and
/// passed none.
fn parse_mocha(output: &str) -> RunnerOutcome {
    let mut p = blank(Runner::Mocha);
    let mut saw = false;
    for line in output.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        for pair in tokens.windows(2) {
            let Ok(n) = pair[0].parse::<u64>() else {
                continue;
            };
            match pair[1].trim_end_matches(',') {
                "passing" => {
                    p.passed = n;
                    saw = true;
                }
                "failing" => {
                    p.failed = n;
                    saw = true;
                }
                "pending" => {
                    p.skipped = n;
                    saw = true;
                }
                _ => {}
            }
        }
    }
    if !saw {
        return RunnerOutcome::Unverifiable {
            reason: "no `N passing` line found in the mocha output".to_string(),
            hint: "mocha's spec reporter prints `N passing` last".to_string(),
        };
    }

    // **The summary is three separate lines, and a cut between them reads
    // green.** `2 passing (3ms)` / `1 pending` / `1 failing` are printed on
    // consecutive lines, so output truncated after the first says `passed=2,
    // failed=0` — an incomplete document reporting a clean pass. Found by the
    // truncation sweep at a 134-byte prefix of a real failing run, which is the
    // same defect the JUnit reader had, in a different runner.
    //
    // The document carries its own contradiction: mocha numbers each failure
    // `1)`, `2)`, … in the spec body, *above* the summary. So a marker with no
    // `failing` count behind it is the run telling us the summary is not the
    // whole story.
    let marked = numbered_failures(output);
    if p.failed < marked {
        return RunnerOutcome::Unverifiable {
            reason: format!(
                "the mocha output marks {marked} numbered failure(s) and its summary \
                 counts {}. The summary block is three separate lines — `passing`, \
                 `pending`, `failing` — so output cut between them reports a clean \
                 pass for a run that failed.",
                p.failed
            ),
            hint: "capture the whole run; mocha prints `N failing` after `N passing`".to_string(),
        };
    }

    p.tests_total = p.passed + p.failed + p.skipped;
    RunnerOutcome::Parsed(p)
}

/// How many distinct failures mocha numbered in this output.
///
/// Distinct, because each one appears **twice** — once in the spec body and
/// once above its stack trace — and counting occurrences would double every
/// failure and refuse every honest failing run.
fn numbered_failures(output: &str) -> u64 {
    let mut seen: BTreeSet<u64> = BTreeSet::new();
    for line in output.lines() {
        let t = line.trim_start();
        let Some((n, rest)) = t.split_once(')') else {
            continue;
        };
        // `1) shipping` — a number, a paren, then a space and a title. Not
        // `(3ms)`, and not a bare `1)` at the end of a truncated line.
        if !rest.starts_with(' ') || rest.trim().is_empty() {
            continue;
        }
        if let Ok(n) = n.parse::<u64>() {
            seen.insert(n);
        }
    }
    seen.len() as u64
}

/// §6.2's generic JUnit XML, which mvn, gradle and dotnet all emit.
///
/// Attributes are read from the `<testsuite>` / `<testsuites>` element rather
/// than by counting `<testcase>` children, because a large report is often
/// truncated in a terminal and the header is what survives.
///
/// # A truncated report used to read as a clean pass
///
/// `tests` is required, so a document without it is refused — that part was
/// always right. But once `tests` parsed, `failures` and `errors` **defaulted
/// to 0** and nothing asked whether the document had ended. Measured, on the
/// same five-test suite:
///
/// ```text
/// complete, 1 failure                 total=5 passed=4 failed=1
/// failures attribute ABSENT           total=5 passed=5 failed=0
/// TRUNCATED mid-file (no close tags)  total=5 passed=5 failed=0   <- GREEN
/// ```
///
/// A runner killed mid-write, a full disk, and a container OOM-killed during
/// teardown all produce exactly that shape, and it fails toward **supported**.
/// It is the same class as the sentinels swept out of the verdict path — a
/// value the process chose standing where a recorded one belongs — reached
/// through a defaulted attribute instead of a missed lookup. This module's own
/// doc already states the principle for the whole `Parsed` struct (*"a
/// defaulted `Parsed` is `{total: 0, failed: 0}`, which is a clean run"*); the
/// defaulted *attribute* is that argument one level in.
///
/// So completeness is now evidence the reader requires, by any of three
/// routes, and a report that supplies none of them is `unverifiable`. The cost
/// is stated rather than hidden: **a genuinely terminal-truncated report is
/// `unverifiable` now**, where it used to be read from its header. That is the
/// safe direction and the honest one — an incomplete document is incomplete —
/// but it is a behaviour change for a real case, and the sentence above about
/// the header being what survives no longer excuses reading it alone.
fn parse_junit_xml(output: &str, runner: Runner) -> Option<RunnerOutcome> {
    // **The inner element, not the first prefix match.** Real
    // `pytest --junit-xml` wraps everything in `<testsuites name="pytest
    // tests">`, which carries no counts — and `find("<testsuite")` matches the
    // WRAPPER. It happened to work only because the inner element fell inside
    // the window; a wrapper with a few more attributes would have pushed it out
    // and the reader would have found no counts in a report that has them.
    //
    // Found by capturing a real report instead of writing one.
    let at = output
        .match_indices("<testsuite")
        .map(|(i, _)| i)
        .find(|i| {
            output[i + "<testsuite".len()..]
                .chars()
                .next()
                .is_some_and(|c| c == ' ' || c == '\t' || c == '\n' || c == '>')
        })
        // A report with only the plural form and no inner suite: fall back to
        // it rather than refusing, since some writers put counts there.
        .or_else(|| output.find("<testsuite"))?;
    let head = &output[at..(at + 600).min(output.len())];
    let attr = |name: &str| -> Option<u64> {
        let key = format!("{name}=\"");
        let i = head.find(&key)? + key.len();
        head[i..].split('"').next()?.trim().parse().ok()
    };
    let total = attr("tests")?;

    // --- completeness, before any count is believed ------------------------
    //
    // Three routes, any one of which is evidence the document ended. They are
    // ORed rather than ANDed because writers differ, and requiring all three
    // would refuse well-formed reports from real tools.
    let rest = &output[at..];
    let element = if head.starts_with("<testsuites") {
        "</testsuites>"
    } else {
        "</testsuite>"
    };
    // 1. The element this reader took its counts from was closed.
    let closed = rest.contains(element);
    // 2. It is self-closing — a suite with no children, which is well-formed
    //    and has no closing tag to find.
    let self_closing = head
        .split_once('>')
        .is_some_and(|(open, _)| open.trim_end().ends_with('/'));
    // 3. Every `<testcase>` the header promised is present. This is the route
    //    that saves a report whose tail was cut *after* the last case.
    let cases = rest.matches("<testcase").count() as u64;
    if !(closed || self_closing || cases >= total) {
        return Some(RunnerOutcome::Unverifiable {
            reason: format!(
                "the JUnit report is incomplete: its header claims {total} test(s),                  {cases} `<testcase>` element(s) are present, and no closing                  `{element}` was found. A report cut off mid-write says nothing                  about the tests it never got to."
            ),
            hint: "re-run the suite and let the report finish writing; a runner                    killed mid-write, a full disk and an OOM-killed container all                    produce this shape"
                .to_string(),
        });
    }

    // --- counts: declared, or counted, but never defaulted ----------------
    //
    // `unwrap_or(0)` here read an ABSENT `failures` attribute as "no failures",
    // which is a default standing where a fact belongs. The JUnit schema does
    // make the attribute optional, so its absence is not itself a defect — but
    // the answer then comes from counting the `<failure>` and `<error>`
    // *elements*, which is a measurement of the same document, rather than from
    // a constant this reader chose.
    let failures = match (attr("failures"), attr("errors")) {
        (None, None) => (rest.matches("<failure").count() + rest.matches("<error").count()) as u64,
        (f, e) => f.unwrap_or(0) + e.unwrap_or(0),
    };
    let skipped = match attr("skipped") {
        Some(n) => n,
        None => rest.matches("<skipped").count() as u64,
    };
    let mut p = blank(runner);
    p.tests_total = total;
    p.failed = failures;
    p.skipped = skipped;
    p.passed = total.saturating_sub(failures + skipped);
    Some(RunnerOutcome::Parsed(p))
}

/// Node's TAP footer, measured through a piped Node 22 child. Require every
/// deciding line, in order, exactly once in the final footer. Partial output
/// and other reporters have no authoritative counts here.
pub fn parse_node_test(output: &str) -> Result<NodeTestCounts, &'static str> {
    let unknown =
        || Err("no complete, consistent Node TAP footer was captured; use the TAP reporter");
    let plain = strip_ansi(output);
    let output = plain.as_str();
    let lines: Vec<_> = output.trim_end().lines().collect();
    if lines.len() < 10 || !lines.contains(&"TAP version 13") {
        return unknown();
    }
    let footer = &lines[lines.len() - 9..];
    if footer[0]
        .strip_prefix("1..")
        .and_then(|n| n.parse::<u64>().ok())
        .is_none()
    {
        return unknown();
    }
    let labels = [
        "tests",
        "suites",
        "pass",
        "fail",
        "cancelled",
        "skipped",
        "todo",
    ];
    let mut values = Vec::new();
    for (line, label) in footer[1..8].iter().zip(labels) {
        let Some(value) = line
            .strip_prefix(&format!("# {label} "))
            .and_then(|v| v.parse::<u64>().ok())
        else {
            return unknown();
        };
        values.push(value);
    }
    let Some(duration) = footer[8]
        .strip_prefix("# duration_ms ")
        .and_then(|v| v.parse::<f64>().ok())
    else {
        return unknown();
    };
    if !duration.is_finite() || duration < 0.0 || values[4] > 0 {
        return unknown();
    }
    let total = values[2..]
        .iter()
        .try_fold(0u64, |sum, n| sum.checked_add(*n));
    if total != Some(values[0]) {
        return unknown();
    }
    Ok(NodeTestCounts {
        tests_total: values[0],
        passed: values[2],
        failed: values[3],
        skipped: values[5],
        todo: values[6],
    })
}

/// Counts from an explicitly captured Node check. Kept outside the shared
/// runner enum until the ordinary PATH capture integration is qualified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeTestCounts {
    pub tests_total: u64,
    pub passed: u64,
    pub failed: u64,
    pub skipped: u64,
    pub todo: u64,
}

fn blank(runner: Runner) -> Parsed {
    Parsed {
        runner,
        tests_total: 0,
        passed: 0,
        failed: 0,
        skipped: 0,
        xfail: 0,
        test_ids: BTreeSet::new(),
        duration_ms: None,
    }
}

/// Is this line, in its entirety, a bannerless pytest tally?
///
/// `2 passed in 0.00s`, `1 failed, 6 passed, 1 skipped in 3.42s`,
/// `no tests ran in 0.01s`. **Not** `assert "3 passed" in output`, and not a
/// log line that happens to contain the words: every token must belong to the
/// grammar, which is what keeps a test's own stdout from being read as the
/// run's result.
fn is_pytest_tally(line: &str) -> bool {
    let line = line.trim();
    if line.is_empty() || line.contains('=') {
        return false;
    }
    // Drop the `in <duration>` tail if present.
    let head = match line.rfind(" in ") {
        Some(at) => {
            let tail = line[at + 4..].trim();
            // `0.01s`, `1.20s`, `1m 30.5s` — digits, dots, colons, `s`/`m`/`h`
            // and spaces, nothing else.
            if tail.is_empty()
                || !tail
                    .chars()
                    .all(|c| c.is_ascii_digit() || matches!(c, '.' | ':' | 's' | 'm' | 'h' | ' '))
            {
                return false;
            }
            &line[..at]
        }
        None => line,
    };
    if head == "no tests ran" {
        return true;
    }
    let mut saw_one = false;
    for part in head.split(',') {
        let mut words = part.split_whitespace();
        let (Some(count), Some(word)) = (words.next(), words.next()) else {
            return false;
        };
        if words.next().is_some() || count.parse::<u64>().is_err() {
            return false;
        }
        if !matches!(
            word,
            "passed"
                | "failed"
                | "error"
                | "errors"
                | "skipped"
                | "deselected"
                | "xfailed"
                | "xpassed"
                | "warning"
                | "warnings"
        ) {
            return false;
        }
        saw_one = true;
    }
    saw_one
}

/// Named tests from `pytest -v` (`path::test_name PASSED`) and from the
/// `FAILED path::name` lines `-q` prints.
fn pytest_ids(output: &str) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for line in output.lines() {
        let line = line.trim();
        for marker in ["PASSED", "FAILED", "ERROR", "SKIPPED", "XFAIL", "XPASS"] {
            if let Some(rest) = line.strip_prefix(&format!("{marker} ")) {
                if let Some(id) = rest.split_whitespace().next() {
                    ids.insert(id.to_string());
                }
            } else if let Some(at) = line.find(&format!(" {marker}")) {
                let candidate = line[..at].trim();
                if candidate.contains("::") {
                    ids.insert(candidate.to_string());
                }
            }
        }
    }
    ids
}

/// `cargo test`'s per-binary `test result:` lines.
///
/// A workspace run emits one per test target, and they must be SUMMED. Reading
/// only the last is the classic error: it reports the final crate's result as
/// the whole workspace's, so a failure in an earlier crate disappears.
fn parse_cargo_test(output: &str) -> RunnerOutcome {
    let mut p = Parsed {
        runner: Runner::CargoTest,
        tests_total: 0,
        passed: 0,
        failed: 0,
        skipped: 0,
        xfail: 0,
        test_ids: BTreeSet::new(),
        duration_ms: None,
    };
    let mut saw_summary = false;
    // A summary LINE is not a summary. `test result:` with nothing after it
    // parsed as a clean run of zero tests before this — found by the property
    // test, and exactly the collapse this module forbids: downstream, zero
    // failures reads as a pass.
    let mut counts_seen = 0u32;
    let mut total_secs = 0.0f64;
    let mut said_failed = false;
    let mut saw_complete_summary = false;
    let mut truncated_summary: Option<String> = None;

    for line in output.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("test result:") {
            saw_summary = true;
            // **A summary line cut mid-way is not a summary either.** `test
            // result: FAILED. 2 passed` — output truncated before the
            // `; 1 failed;` clause — parsed as `passed=2, failed=0`, with the
            // word FAILED sitting unread in the same line. Found by the
            // truncation sweep at a 160-byte prefix of a real failing run.
            //
            // Cargo always ends the line `finished in <n>s`, so its presence is
            // the line's own end marker. The verdict word is a second, cheaper
            // check on the same line: `FAILED` with no failures counted is the
            // line contradicting itself.
            // Cargo always ends the line `finished in <n>s`, so its presence
            // is the line's own end marker. A line without it is **skipped, not
            // refused**: an incomplete `test result:` can also appear as noise
            // in a test's own stdout, and vetoing the whole parse on one would
            // let any test print `test result:` and make its suite
            // unverifiable. Skipping means it contributes no counts and does
            // not satisfy `saw_summary`, so an output containing *only* a
            // truncated line still reaches the "no summary" refusal below.
            if !rest.contains("finished in ") {
                truncated_summary = Some(line.to_string());
                continue;
            }
            saw_complete_summary = true;
            said_failed |= rest.trim_start().starts_with("FAILED");
            let tokens: Vec<&str> = rest.split_whitespace().collect();
            for pair in tokens.windows(2) {
                let Ok(n) = pair[0].parse::<u64>() else {
                    continue;
                };
                match pair[1].trim_end_matches(';') {
                    "passed" => {
                        p.passed += n;
                        counts_seen += 1;
                    }
                    "failed" => {
                        p.failed += n;
                        counts_seen += 1;
                    }
                    "ignored" => {
                        p.skipped += n;
                        counts_seen += 1;
                    }
                    _ => {}
                }
            }
            if let Some(secs) = rest
                .split("finished in ")
                .nth(1)
                .and_then(|s| s.split_whitespace().next())
                // `finished in 0.12s` — the unit is attached to the number.
                .and_then(|s| s.trim_end_matches('s').parse::<f64>().ok())
            {
                total_secs += secs;
            }
        } else if let Some(rest) = line.strip_prefix("test ") {
            // `test some::name ... ok` — the name, not the outcome word.
            if let Some(name) = rest.split(" ... ").next() {
                if !name.is_empty() && !name.starts_with("result:") {
                    p.test_ids.insert(name.to_string());
                }
            }
        }
    }

    // The verdict word and the counts must agree. `FAILED` with nothing counted
    // as failed is a line disagreeing with itself, and only one of the two
    // readings is safe.
    if said_failed && p.failed == 0 {
        return RunnerOutcome::Unverifiable {
            reason: "a `test result: FAILED` line counted zero failures — the \
                     verdict word and the counts disagree"
                .to_string(),
            hint: "capture the runner's whole output".to_string(),
        };
    }
    if saw_summary && !saw_complete_summary {
        return RunnerOutcome::Unverifiable {
            reason: format!(
                "every `test result:` line in this output is truncated — none \
                 reaches `finished in`, so its counts are whatever survived the \
                 cut: {}",
                truncated_summary.unwrap_or_default()
            ),
            hint: "capture the runner's whole output; a summary line cut mid-way \
                   loses its later clauses first, and `failed` comes after \
                   `passed`"
                .to_string(),
        };
    }
    if saw_summary && counts_seen == 0 {
        return RunnerOutcome::Unverifiable {
            reason: "a `test result:` line was found but carried no counts".to_string(),
            hint: "the line was truncated or is not cargo's; a result with no \
                   numbers is unknown, not a clean run of zero tests"
                .to_string(),
        };
    }
    if !saw_summary {
        return RunnerOutcome::Unverifiable {
            reason: "no `test result:` line found in the output".to_string(),
            hint: "cargo test writes one `test result:` line per test binary; \
                   without any, the run did not reach a result and its outcome is \
                   unknown, not clean"
                .to_string(),
        };
    }
    p.tests_total = p.passed + p.failed + p.skipped;
    p.duration_ms = Some((total_secs * 1000.0).round() as u64);
    RunnerOutcome::Parsed(p)
}

/// `... in 1.23s ...` / `in 1.23 seconds`.
fn duration_after_in(line: &str) -> Option<u64> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let at = tokens.iter().position(|t| *t == "in")?;
    let raw = tokens.get(at + 1)?;
    let secs: f64 = raw.trim_end_matches(['s', ',']).parse().ok()?;
    Some((secs * 1000.0).round() as u64)
}

#[cfg(test)]
mod tests;
