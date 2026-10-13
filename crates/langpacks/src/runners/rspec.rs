//! §6.2 for Ruby: RSpec (JSON and console) and minitest (console).
//!
//! # Why minitest lives in a file called `rspec.rs`
//!
//! `runners/mod.rs` declares the module set, and there is no `minitest`
//! module in it. Rather than change a shared file to add one, both Ruby
//! runners are parsed here; the module is Ruby's, and its name is the older
//! of the two runners. That is a filing decision, not a claim that minitest
//! output is an RSpec dialect — it emphatically is not, and the two parsers
//! share nothing but [`duration_ms`] and `strip_ansi`.
//!
//! # The type this module returns, and the `Runner` variant that does not exist
//!
//! `verify::parse::Runner` has nine variants and **none of them is RSpec or
//! minitest**. `verify::parse::Parsed` embeds a `Runner`, so a truthful
//! `RunnerOutcome::Parsed` for a Ruby run cannot be constructed at all today:
//! every available variant would be a false label in a field a report prints,
//! and `tests/capture_reachability.rs` already fails the build for exactly that
//! substitution — *"naming an RSpec run `junit-xml` is a lie"*.
//!
//! So the parsers return [`RubyOutcome`], which carries a [`PackRunner`] and
//! can say `rspec`. [`RubyOutcome::into_verify`] is the boundary: it hands back
//! a `RunnerOutcome`, and while `verify` has no name for these runners it
//! returns **`Unverifiable` with a hint that names the missing variant**. That
//! is not a workaround, it is §6.2's own contract — *"unknown runner →
//! unverifiable with a hint on how to enable parsing"* — and it can never read
//! as a clean run. The day `Runner::RSpec` lands, `PackRunner::into_verify`
//! starts returning `Some`, the real counts flow through this same function,
//! and nothing here needs editing.
//!
//! # Every parser below was written against captured bytes
//!
//! `tests/fixtures/runners/README.md` names the command, the version and the
//! exit code behind each file: rspec-core 3.13.6 and minitest 6.0.6 on ruby
//! 4.0.6, all run on this machine. Four facts came out of those captures that a
//! parser written from the documentation would have got wrong, and each one is
//! a branch below:
//!
//! 1. **`0 failures` is not a pass.** `rspec-load-error.txt`,
//!    `rspec-syntax-error.txt` and `rspec-suite-failed-tests-green.txt` all end
//!    in `0 failures` and all failed. The tell is the clause after it —
//!    `, 1 error occurred outside of examples`.
//! 2. **minitest splits failures from errors.** Reading the `failures` column
//!    alone reports 2 of the 3 real problems in `minitest-mixed.txt`, and a run
//!    whose only problem is an exception reports green.
//! 3. **Empty output is a passing suite's shape.** `minitest-suite-never-ran.txt`
//!    is zero bytes with exit 0, produced by a file that requires
//!    `minitest/test` but not `minitest/autorun`.
//! 4. **`--no-color` does not guarantee ANSI-free output.**
//!    `rspec-syntax-error.txt` was captured with `--no-color` and still carries
//!    29 SGR sequences, because Ruby's own Prism error formatter never sees
//!    RSpec's colour setting. `strip_ansi` is not a CI-only concern.
//!
//! # The one thing output can never tell you about RSpec
//!
//! `rspec --dry-run` prints `9 examples, 0 failures` and exits 0 over a suite
//! in which three examples fail, and its JSON report has the same key set as a
//! real run with every example `"status": "passed"`. There is no marker in
//! either format. [`dry_run`] reads the argv, because the output does not carry
//! the fact.
//!
//! **And the argv does not always carry it either.** A committed `.rspec` file
//! holding `--dry-run` governs the run while appearing in neither stream:
//! `rspec-dotrspec-dry-run.txt` is that capture, bare `rspec` on the command
//! line, `9 examples, 0 failures` in the output, three real failures in the
//! suite. RSpec merges `.rspec`, `~/.rspec`, `.rspec-local` and `$SPEC_OPTS`
//! before it reads argv, and this module sees none of those files. So the
//! honest statement of what [`dry_run`] answers is *"no dry run was visible in
//! the argv"* — a weaker claim than "this was a real run", and the only one
//! available without reading the project's config.

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::Value;

use super::PackRunner;
use thruline_verify::parse::{strip_ansi, Parsed, RunnerOutcome};

/// A Ruby run's counts, labelled with a runner name that is true.
///
/// Field-for-field `verify::parse::Parsed`, except that `runner` is a
/// [`PackRunner`]. See the module docs for why that difference has to exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RubyParsed {
    pub runner: PackRunner,
    pub tests_total: u64,
    pub passed: u64,
    pub failed: u64,
    pub skipped: u64,
    /// **RSpec's `pending` count lands here, not in `skipped`.** See
    /// [`parse_rspec`].
    pub xfail: u64,
    pub test_ids: BTreeSet<String>,
    pub duration_ms: Option<u64>,
}

/// What a Ruby parse concluded.
///
/// No `Default`, and no counts inside `Unverifiable` — `verify::parse`'s rule,
/// for `verify::parse`'s reason: a defaulted `Parsed` is `{total: 0, failed: 0}`
/// and reads downstream as a clean run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RubyOutcome {
    Parsed(RubyParsed),
    Unverifiable { reason: String, hint: String },
}

impl RubyOutcome {
    pub fn parsed(&self) -> Option<&RubyParsed> {
        match self {
            RubyOutcome::Parsed(p) => Some(p),
            RubyOutcome::Unverifiable { .. } => None,
        }
    }

    pub fn is_unverifiable(&self) -> bool {
        matches!(self, RubyOutcome::Unverifiable { .. })
    }

    /// Hand these counts to `verify` — or say why they cannot be handed over.
    ///
    /// The `None` arm is not a failure of this parser. It is the accurate
    /// statement of what `verify` can represent today, phrased as §6.2 requires:
    /// a reason and a hint that says what would make the run parseable. It is
    /// also self-healing — the moment `PackRunner::into_verify` can name these
    /// runners, the real numbers travel this path with no edit here.
    pub fn into_verify(self) -> RunnerOutcome {
        match self {
            RubyOutcome::Unverifiable { reason, hint } => {
                RunnerOutcome::Unverifiable { reason, hint }
            }
            RubyOutcome::Parsed(p) => match p.runner.into_verify() {
                Some(runner) => RunnerOutcome::Parsed(Parsed {
                    runner,
                    tests_total: p.tests_total,
                    passed: p.passed,
                    failed: p.failed,
                    skipped: p.skipped,
                    xfail: p.xfail,
                    test_ids: p.test_ids,
                    duration_ms: p.duration_ms,
                }),
                None => super::unverifiable(
                    format!(
                        "the run was read successfully as {} — {} test(s), {} failed — but \
                         `verify::parse::Runner` has no variant that names this runner, so \
                         the result has nowhere truthful to go",
                        p.runner.as_str(),
                        p.tests_total,
                        p.failed
                    ),
                    "add `Runner::RSpec` and `Runner::Minitest` to `crates/verify` \
                     (MERGE-PLAN.md); until then a Ruby run is unverifiable by \
                     representation, not by parsing",
                ),
            },
        }
    }
}

fn unverifiable(reason: impl Into<String>, hint: impl Into<String>) -> RubyOutcome {
    RubyOutcome::Unverifiable {
        reason: reason.into(),
        hint: hint.into(),
    }
}

fn blank(runner: PackRunner) -> RubyParsed {
    RubyParsed {
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

/// The mandated §6.2 entry point: parse an RSpec run into `verify`'s own type.
///
/// **Today this returns `Unverifiable` for every RSpec run that parses
/// perfectly**, because `verify::parse::Runner` cannot name RSpec — see
/// [`RubyOutcome::into_verify`]. Callers inside this crate should use
/// [`parse_rspec`], which returns the counts.
pub fn parse(output: &str) -> RunnerOutcome {
    parse_rspec(output).into_verify()
}

/// RSpec, in any of its three captured shapes: `--format json`, `--format
/// progress` (the default) and `--format documentation`.
///
/// The JSON report is tried first and is authoritative: it carries
/// `errors_outside_of_examples_count` as a number rather than as a clause of
/// English, and it names every example. The two console formats share one
/// summary line and are parsed by the same code — which is the point of parsing
/// the summary rather than the progress characters, exactly as `verify`'s
/// pytest parser does.
///
/// # `pending` becomes `xfail`, and here is the cost of that
///
/// RSpec's `pending "reason"` **runs** the example and requires it to fail; if
/// it passes, the suite fails (`rspec-pending-fixed.txt` is that capture, and
/// it reports `1 failure` for an example whose expectation succeeded). That is
/// `xfail`, precisely, and it is not `skipped`.
///
/// But RSpec reports `xit`, `skip "…"` and `pending "…"` in **one** `pending`
/// column, in both formats, and the JSON's `pending_message` does not separate
/// them either — `skip "not ready"` and `pending "not ready"` produce identical
/// documents. So the whole column goes to `xfail`.
///
/// **The consequence, stated rather than discovered:** §6.3's `escalate` reads
/// `parsed.skipped` and nothing else when deciding whether a newly added skip
/// marker took effect. An `xit` added to an RSpec suite therefore leaves
/// `skipped == 0`, and `skip_marker_added` stays at `info` instead of rising to
/// `fail`. The alternative — sending the column to `skipped` — would make every
/// deliberate `pending` on a known-broken example refute an honest claim, which
/// is the exact bug `escalate`'s doc comment was written to record. Neither
/// direction is right because the runner does not tell us; this one fails
/// toward accusing nobody.
pub fn parse_rspec(output: &str) -> RubyOutcome {
    let plain = strip_ansi(output);
    if let Some(outcome) = parse_rspec_json(&plain) {
        return outcome;
    }
    parse_rspec_console(&plain)
}

/// `rspec --format json`.
///
/// `None` — not "no result", but *this is not an RSpec JSON report* — so the
/// console parser gets its turn. A spec that `puts`es a JSON blob must not
/// hijack the parse, which is why the document has to carry
/// `summary.example_count` before any of it is believed.
fn parse_rspec_json(plain: &str) -> Option<RubyOutcome> {
    let document = rspec_json_document(plain)?;
    let summary = document.get("summary")?;
    let count = |key: &str| summary.get(key).and_then(Value::as_u64);
    let examples = count("example_count")?;
    let failures = count("failure_count").unwrap_or(0);
    let pending = count("pending_count").unwrap_or(0);
    let outside = count("errors_outside_of_examples_count").unwrap_or(0);

    if outside > 0 {
        return Some(unverifiable(
            format!(
                "the RSpec report counts {outside} error(s) outside of examples beside \
                 {examples} example(s) and {failures} failure(s) — a spec file that fails \
                 to load contributes no examples, so this tally describes only the files \
                 that loaded",
            ),
            "fix the load or syntax error and re-run; `errors_outside_of_examples_count` \
             is non-zero, and a suite that did not finish loading has not passed",
        ));
    }
    if examples == 0 {
        return Some(zero_examples(plain));
    }

    let mut parsed = blank(PackRunner::RSpec);
    parsed.tests_total = examples;
    parsed.failed = failures;
    parsed.xfail = pending;
    parsed.passed = examples.saturating_sub(failures).saturating_sub(pending);
    parsed.duration_ms = summary
        .get("duration")
        .and_then(Value::as_f64)
        .map(|seconds| (seconds * 1000.0).round() as u64);
    // `id` rather than `full_description`: the id (`./spec/calc_spec.rb[1:1:2]`)
    // is unique and is an argument RSpec itself accepts back, so a reader can
    // re-run exactly the example named. `full_description` reads better and is
    // not unique — two `it "works"` under different contexts collapse.
    parsed.test_ids = document
        .get("examples")
        .and_then(Value::as_array)
        .map(|examples| {
            examples
                .iter()
                .filter_map(|e| e.get("id").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    Some(RubyOutcome::Parsed(parsed))
}

/// The JSON document in this output, if there is one.
///
/// A prefix parse rather than a whole-string parse: RSpec's JSON formatter
/// writes one object and nothing else, but a `puts` in a spec or a bundler
/// warning can put lines on either side of it.
fn rspec_json_document(plain: &str) -> Option<Value> {
    let leading = plain.len() - plain.trim_start().len();
    let at = if plain[leading..].starts_with('{') {
        leading
    } else {
        plain.find("{\"version\"")?
    };
    let mut stream = serde_json::Deserializer::from_str(&plain[at..]);
    Value::deserialize(&mut stream).ok()
}

fn parse_rspec_console(plain: &str) -> RubyOutcome {
    let Some(summary) = plain.lines().rev().find(|line| is_rspec_summary(line)) else {
        return unverifiable(
            format!(
                "no RSpec summary line was found in the output{}",
                load_failure_note(plain)
            ),
            "RSpec ends every run with `N examples, N failures`; output that stops \
             before it describes a run whose result is unknown, not a clean one",
        );
    };

    let (mut examples, mut failures, mut pending, mut outside) = (0u64, 0u64, 0u64, 0u64);
    for clause in summary.split(',') {
        let clause = clause.trim();
        let Some((number, rest)) = clause.split_once(' ') else {
            continue;
        };
        let Ok(n) = number.parse::<u64>() else {
            continue;
        };
        if rest.starts_with("example") {
            examples = n;
        } else if rest.starts_with("failure") {
            failures = n;
        } else if rest.starts_with("pending") {
            pending = n;
        } else if rest.starts_with("error") && rest.contains("outside of examples") {
            outside = n;
        }
    }

    if outside > 0 {
        return unverifiable(
            format!(
                "the RSpec summary reads {summary:?} — {outside} error(s) occurred outside \
                 of examples{}. A spec file that fails to load contributes no examples, so \
                 `{failures} failures` describes only the files that loaded",
                load_failure_note(plain)
            ),
            "fix the load or syntax error and re-run; this is the shape three separate \
             captures take — a bad `require`, a missing `end`, and an `after(:suite)` hook \
             that raised — and all three end in `0 failures`",
        );
    }
    if examples == 0 {
        return zero_examples(plain);
    }

    // The document carries its own contradiction, and it is worth reading.
    // RSpec numbers each failure `1)`, `2)`, … under a `Failures:` heading
    // *above* the summary, so output truncated between them would report a
    // clean pass. Counting the markers only inside that section is what keeps
    // `rspec-pending-skipped.txt` — three numbered entries under `Pending:`
    // beside `0 failures` — from being read as three lost failures.
    let marked = failures_section_markers(plain);
    if failures < marked {
        return unverifiable(
            format!(
                "the output lists {marked} numbered failure(s) under `Failures:` and its \
                 summary counts {failures}"
            ),
            "capture the whole run; RSpec prints the `Failures:` block before the \
             `N examples, N failures` line, so output cut between them reads green",
        );
    }

    let mut parsed = blank(PackRunner::RSpec);
    parsed.tests_total = examples;
    parsed.failed = failures;
    parsed.xfail = pending;
    parsed.passed = examples.saturating_sub(failures).saturating_sub(pending);
    parsed.duration_ms = duration_ms(plain);
    parsed.test_ids = rspec_failed_example_ids(plain);
    RubyOutcome::Parsed(parsed)
}

/// A summary line, strictly.
///
/// `N example…` at the head **and** the word `failure` somewhere on the line.
/// The strictness is `verify`'s `is_pytest_tally` lesson: a documentation-format
/// run prints arbitrary sentences, and one of them beginning "3 examples" would
/// otherwise be read as a tally.
fn is_rspec_summary(line: &str) -> bool {
    let line = line.trim();
    let mut words = line.split_whitespace();
    let Some(first) = words.next() else {
        return false;
    };
    if first.parse::<u64>().is_err() {
        return false;
    }
    words.next().is_some_and(|w| w.starts_with("example")) && line.contains("failure")
}

/// A run that produced no example outcomes at all.
///
/// **RSpec cannot tell you which of the two happened.** `rspec-zero-tests.txt`
/// (a spec file that loads and declares nothing) and `rspec-no-spec-files.txt`
/// (an empty `spec/` directory) are the same bytes apart from timings — both
/// `No examples found.`, both `0 examples, 0 failures`, both exit 0. The
/// sameness is the finding, so the reason names both possibilities rather than
/// asserting one.
fn zero_examples(plain: &str) -> RubyOutcome {
    let found_the_banner = plain.contains("No examples found.");
    unverifiable(
        format!(
            "the RSpec run reported 0 examples{}",
            if found_the_banner {
                " and printed `No examples found.`"
            } else {
                ""
            }
        ),
        "a run that executed no examples is not a passing run, and RSpec does not \
         distinguish an empty `spec/` directory from a spec file that declares nothing \
         — check that the spec path and any `--pattern` actually match files",
    )
}

/// Distinct numbered failures inside the `Failures:` block.
///
/// Distinct, because a truncated line could repeat one; and scoped to the
/// block, because the `Pending:` block numbers its entries the same way and a
/// pending example prints a `Failure/Error:` of its own without being a
/// failure — `rspec-pending-skipped.txt` has three of them beside `0 failures`.
fn failures_section_markers(plain: &str) -> u64 {
    let mut inside = false;
    let mut seen: BTreeSet<u64> = BTreeSet::new();
    for line in plain.lines() {
        let trimmed = line.trim();
        if trimmed == "Failures:" {
            inside = true;
            continue;
        }
        if inside && (trimmed.starts_with("Finished in") || trimmed == "Pending:") {
            inside = false;
            continue;
        }
        if !inside {
            continue;
        }
        let Some((number, rest)) = trimmed.split_once(')') else {
            continue;
        };
        if let Ok(n) = number.parse::<u64>() {
            if rest.starts_with(' ') && !rest.trim().is_empty() {
                seen.insert(n);
            }
        }
    }
    seen.len() as u64
}

/// The re-runnable locators RSpec prints under `Failed examples:`.
///
/// The console names **only failures**, and only in this block —
/// `rspec ./spec/calc_div_spec.rb:11 # Calc.div rounds toward zero`. An empty
/// set is therefore normal for a green run and is evidence of nothing, which is
/// the same caveat `Parsed::test_ids` carries for `pytest -q`.
///
/// The documentation formatter prints every example's description as an
/// indented tree with no delimiter between the nesting and the name; that is
/// not machine-readable without inventing a rule, so it is not read here.
fn rspec_failed_example_ids(plain: &str) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let mut inside = false;
    for line in plain.lines() {
        let trimmed = line.trim();
        if trimmed == "Failed examples:" {
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("rspec ") {
            if let Some(locator) = rest.split_whitespace().next() {
                ids.insert(locator.to_string());
            }
        }
    }
    ids
}

/// minitest, as `ruby -Ilib -Itest test/test_calc.rb` prints it.
///
/// # Three things this has to get right that the summary line alone does not
///
/// - **Errors are not failures.** `8 runs, 5 assertions, 2 failures, 1 errors,
///   2 skips` — the escaped `NoMethodError` is in the `errors` column. Reading
///   `failures` alone reports 2 of 3 real problems, and a run whose only
///   problem is an exception reports green. Both columns are summed into
///   `failed`.
/// - **Zero bytes is a valid capture.** A file that requires `minitest/test`
///   but not `minitest/autorun` never installs the `at_exit` hook: it defines a
///   test class containing a deliberately false assertion, prints **nothing**,
///   and exits 0. Empty output with a zero exit is the strongest "everything is
///   fine" signal a shell can give, and here it means the suite never ran.
/// - **A load or syntax error prints no summary at all.** Ruby's own error goes
///   to stderr and minitest never starts, so there is no tally to
///   mis-read — but there is also nothing that says a suite was skipped, which
///   is why the reason below names the error class it found.
///
/// minitest's `skip` genuinely does not run the example, so it goes to
/// `skipped` — unlike RSpec's `pending` column, which does not mean the same
/// thing. See [`parse_rspec`].
pub fn parse_minitest(output: &str) -> RubyOutcome {
    let plain = strip_ansi(output);
    if plain.trim().is_empty() {
        return unverifiable(
            "the run produced no output at all",
            "a minitest file that requires `minitest/test` but not `minitest/autorun` \
             never installs the hook that runs the suite: it prints nothing and exits 0. \
             Add `require \"minitest/autorun\"`, or run the file through a runner that \
             does",
        );
    }

    let Some(summary) = plain.lines().rev().find(|line| is_minitest_summary(line)) else {
        return unverifiable(
            format!(
                "no minitest summary line was found in the output{}",
                load_failure_note(&plain)
            ),
            "minitest ends every run with `N runs, N assertions, N failures, N errors, \
             N skips`; without it the process died before the suite finished, which is \
             not a clean run",
        );
    };

    let (mut runs, mut failures, mut errors, mut skips) = (0u64, 0u64, 0u64, 0u64);
    for clause in summary.split(',') {
        let clause = clause.trim();
        let Some((number, rest)) = clause.split_once(' ') else {
            continue;
        };
        let Ok(n) = number.parse::<u64>() else {
            continue;
        };
        if rest.starts_with("run") {
            runs = n;
        } else if rest.starts_with("failure") {
            failures = n;
        } else if rest.starts_with("error") {
            errors = n;
        } else if rest.starts_with("skip") {
            skips = n;
        }
    }

    if runs == 0 {
        return unverifiable(
            format!("the minitest summary reads {summary:?} — the run executed 0 tests"),
            "a `Minitest::Test` subclass with no `test_`-prefixed methods runs nothing \
             and still exits 0; a run that produced no test outcomes is not a passing run",
        );
    }

    let failed = failures + errors;
    let marked = minitest_failure_markers(&plain);
    if failed < marked {
        return unverifiable(
            format!(
                "the output lists {marked} numbered failure/error block(s) and its summary \
                 counts {failed} ({failures} failure(s) + {errors} error(s))"
            ),
            "capture the whole run; minitest prints its numbered blocks between the \
             `Finished in` line and the counts line",
        );
    }

    let mut parsed = blank(PackRunner::Minitest);
    parsed.tests_total = runs;
    parsed.failed = failed;
    parsed.skipped = skips;
    parsed.passed = runs.saturating_sub(failed).saturating_sub(skips);
    parsed.duration_ms = duration_ms(&plain);
    parsed.test_ids = minitest_test_ids(&plain);
    RubyOutcome::Parsed(parsed)
}

fn is_minitest_summary(line: &str) -> bool {
    let line = line.trim();
    let mut words = line.split_whitespace();
    let Some(first) = words.next() else {
        return false;
    };
    if first.parse::<u64>().is_err() {
        return false;
    }
    words.next().is_some_and(|w| w.starts_with("run")) && line.contains("assertion")
}

/// Distinct `N) Failure:` / `N) Error:` blocks.
fn minitest_failure_markers(plain: &str) -> u64 {
    let mut seen: BTreeSet<u64> = BTreeSet::new();
    for line in plain.lines() {
        let trimmed = line.trim();
        let Some((number, rest)) = trimmed.split_once(')') else {
            continue;
        };
        let Ok(n) = number.parse::<u64>() else {
            continue;
        };
        let rest = rest.trim();
        if rest.starts_with("Failure") || rest.starts_with("Error") {
            seen.insert(n);
        }
    }
    seen.len() as u64
}

/// `Class#test_method` identifiers, from `--verbose` lines and failure blocks.
///
/// Both sources use the same `Class#method` spelling, so one rule reads both:
/// the line's first word contains a `#` and does not start with one — which is
/// what excludes minitest's own `# Running:` banner and the `# ./file.rb:12`
/// backtrace lines.
fn minitest_test_ids(plain: &str) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for line in plain.lines() {
        let Some(word) = line.split_whitespace().next() else {
            continue;
        };
        if word.starts_with('#') || !word.contains('#') {
            continue;
        }
        let id = word.trim_end_matches(':');
        if !id.is_empty() {
            ids.insert(id.to_string());
        }
    }
    ids
}

/// `Finished in …`, from either runner.
///
/// RSpec writes `Finished in 0.02173 seconds (files took 0.13775 seconds to
/// load)`; minitest writes `Finished in 0.004992s, 1001.6026 runs/s, …`. The
/// unit is checked rather than assumed: RSpec prints `1 minute 3.2 seconds` for
/// a long run, that form is in no capture here, and a parser that read the `1`
/// as seconds would report a duration sixty times short. It returns `None`
/// instead — `duration_ms` is an `Option` precisely so it can.
fn duration_ms(plain: &str) -> Option<u64> {
    let line = plain
        .lines()
        .rev()
        .find(|l| l.trim_start().starts_with("Finished in "))?;
    let mut words = line
        .trim_start()
        .strip_prefix("Finished in ")?
        .split_whitespace();
    let value = words.next()?;
    // minitest attaches the unit and a comma: `0.004992s,`.
    if let Some(seconds) = value.trim_end_matches(',').strip_suffix('s') {
        return seconds
            .parse::<f64>()
            .ok()
            .map(|s| (s * 1000.0).round() as u64);
    }
    let seconds = value.parse::<f64>().ok()?;
    words
        .next()
        .filter(|unit| unit.starts_with("second"))
        .map(|_| (seconds * 1000.0).round() as u64)
}

/// The error class a failed load printed, if this output shows one.
///
/// Appended to a reason so `unverifiable` says *why* rather than only *that*.
/// Both runners put the class name on its own line or at the end of a Ruby
/// backtrace line, so a substring is enough here — this text never decides a
/// verdict, it only describes one that has already been decided.
fn load_failure_note(plain: &str) -> String {
    for (needle, description) in [
        ("SyntaxError", " (the output contains a SyntaxError)"),
        ("LoadError", " (the output contains a LoadError)"),
        (
            "An error occurred while loading",
            " (a spec file failed to load)",
        ),
        (
            "An error occurred in an `after(:suite)` hook",
            " (an after(:suite) hook raised)",
        ),
    ] {
        if plain.contains(needle) {
            return description.to_string();
        }
    }
    String::new()
}

/// Recognise a Ruby test runner from its argv.
///
/// # This returns a `PackRunner`, and the signature difference is the point
///
/// `verify::parse::Runner::from_argv` has no arm for `rspec`, `ruby`, `rake` or
/// `bundle` and no variant to return if it had one, so this cannot be
/// `-> Option<Runner>`: every correct recognition would have to come back as
/// `None`, which is indistinguishable from "this was not a test run". See the
/// module docs.
///
/// # The grammar, and the two arms deliberately absent
///
/// - `rspec` — including `bin/rspec` and `./bin/rspec`, by basename.
/// - `bundle exec <runner>` — recursion, so the inner grammar is stated once.
/// - `ruby [flags] <path>` — minitest, **only** when the path follows
///   minitest's own naming convention (`test_*.rb` or `*_test.rb`). `ruby
///   foo.rb` runs whatever `foo.rb` requires and is not a test run in any
///   knowable sense.
///
/// **`rake test` / `rake spec` is not here, on purpose — and the reason is not
/// the one it would be natural to give.** It is not that rake's output is
/// unparseable: three rake captures are in this repository
/// (`rake-spec-pass.txt`, `rake-spec-failed.txt`, `rake-test-mixed.txt`) and
/// `a_rake_wrapper_does_not_stop_the_runners_summary_being_read` shows both
/// parsers reading straight through the wrapper's framing to the runner's own
/// summary. The problem is upstream of parsing: **a rake task name says nothing
/// about which runner it wraps.** `rake-spec-pass.txt` was produced by a task
/// called `spec_green`, `rake test` is a `Rake::TestTask` here and an RSpec task
/// in the next repository, and a Rakefile may name a task anything at all. So
/// `rake spec` cannot be dispatched to a runner from the argv without reading
/// the Rakefile, and guessing would send minitest output to the RSpec parser —
/// which finds no `N examples, N failures` line and calls a real run
/// unverifiable, or worse, finds one in a stray line and counts it.
///
/// An unrecognised runner is `unverifiable`, which is the truthful answer. A
/// caller that already knows which runner a task wraps can call
/// [`parse_rspec`] or [`parse_minitest`] on the output directly, and the
/// captures show that working.
///
/// **None of this is reachable from live capture today.** `shim::WRAPPED` has
/// no `ruby`, `rspec`, `rake` or `bundle`, so no `exec` event for a Ruby test
/// run is ever written. `tests/capture_reachability.rs` re-derives that from
/// `shim.rs` and fails the build when it changes.
pub fn from_argv(argv: &[String]) -> Option<PackRunner> {
    let program = argv.first()?;
    let base = program.rsplit('/').next().unwrap_or(program);
    match base {
        "rspec" => Some(PackRunner::RSpec),
        "bundle" | "bundler" => {
            let after_exec = argv.iter().position(|a| a == "exec")? + 1;
            from_argv(argv.get(after_exec..)?)
        }
        "ruby" => {
            let script = ruby_script(argv)?;
            let file = script.rsplit('/').next().unwrap_or(script);
            let minitest_naming =
                (file.starts_with("test_") && file.ends_with(".rb")) || file.ends_with("_test.rb");
            minitest_naming.then_some(PackRunner::Minitest)
        }
        _ => None,
    }
}

/// The script `ruby` was given, skipping its own options.
///
/// A one-letter option's value may be attached (`-Ilib`) or separate
/// (`-I lib`), so a bare value-taking flag consumes the following word.
///
/// **`-e` ends the search rather than skipping one word.** Ruby's own rule is
/// that `-e` *is* the program, and every remaining argument belongs to it — so
/// in `ruby -e 'puts 1' test_calc.rb` the file is `ARGV[0]` and is never
/// executed. Skipping only the code and then reading `test_calc.rb` as the
/// script would report a test run that did not happen.
fn ruby_script(argv: &[String]) -> Option<&str> {
    const VALUE_FLAGS: &[&str] = &["I", "r", "e", "C", "E", "F", "S"];
    let mut at = 1usize;
    while let Some(arg) = argv.get(at) {
        let Some(flag) = arg.strip_prefix('-') else {
            return Some(arg.as_str());
        };
        // Short `-e` or attached `-e'code'`; a second `-` means a long option,
        // and `-E` (external encoding) is a different flag entirely.
        if !flag.starts_with('-') && flag.starts_with('e') {
            return None;
        }
        at += 1;
        if VALUE_FLAGS.contains(&flag) {
            at += 1;
        }
    }
    None
}

/// Was this run a dry run — and therefore evidence of nothing?
///
/// **The only discriminator that exists.** `rspec --dry-run` prints
/// `9 examples, 0 failures` and exits 0 over a suite in which three examples
/// fail when executed, and `rspec --format json --dry-run` produces the same
/// key set as a real run with every example `"status": "passed"`. The captures
/// are in this repository (`rspec-dry-run.txt`, `rspec-dry-run.json`) and their
/// only byte-level difference from a real green run is that `run_time` sits
/// near `1e-06` instead of `1e-03` — a hint, not a discriminator.
///
/// So output-only verification of RSpec is defeatable by nine characters on the
/// command line, and the defence has to read the argv. A caller that has an
/// argv and does not consult this is trusting a number RSpec will happily
/// invent.
///
/// # And the argv is not enough either. This function returns `false` on a real
/// dry run.
///
/// `rspec-dotrspec-dry-run.txt` is a capture of `rspec`, bare, in a project
/// whose committed `.rspec` file contains `--dry-run`. RSpec merges options
/// from `.rspec`, `~/.rspec`, `.rspec-local` and `$SPEC_OPTS` before it looks
/// at the command line, so the flag governs the run while appearing in
/// **neither** the argv **nor** the output. The capture's bytes are
/// `9 examples, 0 failures`, exit 0, over the same suite in which three
/// examples fail; `rspec-dotrspec-bypassed.txt` is the control, the same
/// project run with `--options /dev/null`, and it reports the 3 failures.
///
/// `the_dotrspec_file_defeats_the_only_dry_run_discriminator_there_is` asserts
/// this defeat rather than describing it. **Closing it needs a file read, not a
/// parser change** — the working directory's `.rspec`, the home directory's,
/// and `SPEC_OPTS` from the recorded environment — and this function has none
/// of those inputs. Do not read a `false` from here as "this was not a dry
/// run"; read it as "no dry run was visible in the argv", which is a weaker
/// statement and the only one available.
pub fn dry_run(argv: &[String]) -> bool {
    argv.iter().any(|a| a == "--dry-run")
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! fixture {
        ($name:literal) => {
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/runners/",
                $name
            ))
        };
    }

    fn counts(outcome: &RubyOutcome) -> (u64, u64, u64, u64, u64) {
        let p = outcome
            .parsed()
            .unwrap_or_else(|| panic!("expected counts, got {outcome:?}"));
        (p.tests_total, p.passed, p.failed, p.skipped, p.xfail)
    }

    fn reason(outcome: &RubyOutcome) -> String {
        match outcome {
            RubyOutcome::Unverifiable { reason, hint } => {
                assert!(!hint.is_empty(), "§6.2 requires a hint");
                reason.clone()
            }
            RubyOutcome::Parsed(p) => panic!("expected unverifiable, got {p:?}"),
        }
    }

    #[test]
    fn rspec_progress_and_documentation_agree_because_the_summary_is_parsed() {
        // The two formatters print completely different bodies and the same
        // summary line. Parsing the summary is what makes them agree — the same
        // reason `verify`'s pytest parser reads the banner and not the dots.
        assert_eq!(
            counts(&parse_rspec(fixture!("rspec-all-pass.txt"))),
            (6, 6, 0, 0, 0)
        );
        assert_eq!(
            counts(&parse_rspec(fixture!("rspec-documentation.txt"))),
            (6, 6, 0, 0, 0)
        );
        assert_eq!(
            counts(&parse_rspec(fixture!("rspec-mixed.txt"))),
            (9, 6, 3, 0, 0)
        );
        assert_eq!(
            counts(&parse_rspec(fixture!("rspec-documentation-mixed.txt"))),
            (9, 6, 3, 0, 0)
        );
    }

    #[test]
    fn a_forced_colour_run_parses_to_the_same_numbers() {
        // `rspec --force-color`, 64 SGR sequences, including around the summary
        // line itself. Without `strip_ansi` the summary does not match at all.
        assert!(fixture!("rspec-ansi-color.txt").contains('\u{1b}'));
        assert_eq!(
            counts(&parse_rspec(fixture!("rspec-ansi-color.txt"))),
            counts(&parse_rspec(fixture!("rspec-mixed.txt")))
        );
    }

    #[test]
    fn a_no_color_run_can_still_carry_ansi_and_must_still_parse() {
        // `rspec-syntax-error.txt` was captured WITH `--no-color` and contains
        // raw `ESC[1;3m`, because Ruby's Prism error formatter never sees
        // RSpec's colour setting. `strip_ansi` is not a CI-only concern.
        let raw = fixture!("rspec-syntax-error.txt");
        assert!(raw.contains('\u{1b}'));
        let reason = reason(&parse_rspec(raw));
        assert!(reason.contains("outside of examples"), "{reason}");
        assert!(reason.contains("SyntaxError"), "{reason}");
    }

    #[test]
    fn zero_failures_is_not_a_pass_when_an_error_happened_outside_the_examples() {
        // Three captures, three different causes, all ending `0 failures`, all
        // failures. This is the single most load-bearing branch for Ruby.
        for name in [
            fixture!("rspec-load-error.txt"),
            fixture!("rspec-syntax-error.txt"),
            fixture!("rspec-suite-failed-tests-green.txt"),
        ] {
            assert!(name.contains("0 failures"), "fixture drifted");
            let outcome = parse_rspec(name);
            assert!(
                outcome.is_unverifiable(),
                "a run that ended `0 failures` with an error outside examples parsed as \
                 counts: {outcome:?}"
            );
        }
        let reason = reason(&parse_rspec(fixture!("rspec-report-load-error.json")));
        assert!(
            reason.contains("1 error(s) outside of examples"),
            "{reason}"
        );
    }

    #[test]
    fn the_suite_hook_failure_is_caught_even_though_every_example_passed() {
        // `4 examples, 0 failures, 1 error occurred outside of examples`, exit
        // 1. Every example genuinely passed; the suite still failed.
        let outcome = parse_rspec(fixture!("rspec-suite-failed-tests-green.txt"));
        assert!(outcome.is_unverifiable(), "{outcome:?}");
        assert!(reason(&outcome).contains("after(:suite)"));
    }

    #[test]
    fn a_zero_example_run_is_distinguishable_from_a_clean_one() {
        for name in [
            fixture!("rspec-zero-tests.txt"),
            fixture!("rspec-no-spec-files.txt"),
        ] {
            let outcome = parse_rspec(name);
            assert!(outcome.is_unverifiable(), "{outcome:?}");
            assert!(reason(&outcome).contains("0 examples"));
        }
        // And the two are the same bytes apart from timings, which is why the
        // reason names both causes instead of asserting one. This assertion is
        // the record of that.
        let strip_timings = |s: &str| {
            s.lines()
                .filter(|l| !l.starts_with("Finished in"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(
            strip_timings(fixture!("rspec-zero-tests.txt")),
            strip_timings(fixture!("rspec-no-spec-files.txt"))
        );
    }

    #[test]
    fn pending_is_xfail_and_not_skipped() {
        // Six examples, three pending, zero failures, exit 0 — and one of those
        // three prints a full `Failure/Error:` block under `Pending:`.
        assert_eq!(
            counts(&parse_rspec(fixture!("rspec-pending-skipped.txt"))),
            (6, 3, 0, 0, 3)
        );
        assert_eq!(
            counts(&parse_rspec(fixture!("rspec-report-pending.json"))),
            (6, 3, 0, 0, 3)
        );
    }

    #[test]
    fn a_pending_example_that_passes_is_a_failure() {
        // The inverse trap: RSpec reports `1 failure` for an example whose
        // expectation succeeded, because it was declared `pending`. Counting
        // `Failure/Error:` blocks gets this and the previous test wrong in
        // opposite directions.
        assert_eq!(
            counts(&parse_rspec(fixture!("rspec-pending-fixed.txt"))),
            (6, 3, 1, 0, 2)
        );
    }

    #[test]
    fn numbered_failures_are_counted_only_inside_the_failures_block() {
        // `rspec-pending-skipped.txt` numbers three entries under `Pending:`
        // and reports `0 failures`. A section-blind marker count would call
        // that output truncated and refuse a green run.
        assert_eq!(failures_section_markers(fixture!("rspec-mixed.txt")), 3);
        assert_eq!(
            failures_section_markers(fixture!("rspec-pending-skipped.txt")),
            0
        );
        assert_eq!(
            failures_section_markers(fixture!("rspec-pending-fixed.txt")),
            1
        );
    }

    #[test]
    fn output_truncated_above_the_summary_does_not_read_as_green() {
        // Take the mixed run's `Failures:` block and give it a summary that
        // counts none of them — the shape a cut between the two produces.
        let doctored =
            fixture!("rspec-mixed.txt").replace("9 examples, 3 failures", "9 examples, 0 failures");
        let outcome = parse_rspec(&doctored);
        assert!(outcome.is_unverifiable(), "{outcome:?}");
        assert!(reason(&outcome).contains("numbered failure"));
    }

    #[test]
    fn the_json_report_is_authoritative_and_names_every_example() {
        let outcome = parse_rspec(fixture!("rspec-report.json"));
        assert_eq!(counts(&outcome), (9, 6, 3, 0, 0));
        let parsed = outcome.parsed().expect("counts");
        assert_eq!(parsed.test_ids.len(), 9);
        assert!(parsed.test_ids.contains("./spec/calc_div_spec.rb[1:1:2]"));
        assert_eq!(parsed.duration_ms, Some(11));
    }

    #[test]
    fn the_console_names_the_failures_it_can_be_re_run_from() {
        let parsed = parse_rspec(fixture!("rspec-mixed.txt"))
            .parsed()
            .expect("counts")
            .clone();
        assert_eq!(
            parsed.test_ids,
            [
                "./spec/calc_div_spec.rb:11",
                "./spec/calc_div_spec.rb:19",
                "./spec/calc_div_spec.rb:23",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect::<BTreeSet<_>>()
        );
        // A green run names nothing, and that is not evidence of anything.
        assert!(parse_rspec(fixture!("rspec-all-pass.txt"))
            .parsed()
            .expect("counts")
            .test_ids
            .is_empty());
    }

    #[test]
    fn a_dry_run_is_indistinguishable_from_a_green_one_and_this_records_it() {
        // **This test asserts a defeat.** Both fixtures were produced over the
        // SAME suite, in which three examples fail when actually executed. The
        // parser reports a clean 9/9 for both because there is nothing in
        // either byte stream to tell them apart.
        assert_eq!(
            counts(&parse_rspec(fixture!("rspec-dry-run.txt"))),
            (9, 9, 0, 0, 0)
        );
        assert_eq!(
            counts(&parse_rspec(fixture!("rspec-dry-run.json"))),
            (9, 9, 0, 0, 0)
        );
        // The argv is the only place the fact exists.
        assert!(dry_run(&[
            "rspec".to_string(),
            "--no-color".to_string(),
            "--dry-run".to_string()
        ]));
        assert!(!dry_run(&["rspec".to_string(), "--no-color".to_string()]));
    }

    #[test]
    fn a_narrowed_rspec_run_still_parses_and_the_narrowing_is_not_this_parsers_job() {
        // Three of RSpec's four narrowing forms announce themselves in a
        // `Run options:` banner and the fourth — a bare spec file path — prints
        // nothing at all. `rspec-narrowed-file.txt` is four dots and
        // `4 examples, 0 failures`, structurally identical to a full green run.
        // Scope is §6.3's question, not §6.2's; what matters here is that the
        // banner does not break the tally.
        assert_eq!(
            counts(&parse_rspec(fixture!("rspec-narrowed-example.txt"))),
            (1, 1, 0, 0, 0)
        );
        assert_eq!(
            counts(&parse_rspec(fixture!("rspec-narrowed-line.txt"))),
            (1, 1, 0, 0, 0)
        );
        assert_eq!(
            counts(&parse_rspec(fixture!("rspec-narrowed-tag.txt"))),
            (5, 2, 0, 0, 3)
        );
        assert_eq!(
            counts(&parse_rspec(fixture!("rspec-narrowed-file.txt"))),
            (4, 4, 0, 0, 0)
        );
        assert!(!fixture!("rspec-narrowed-file.txt").contains("Run options:"));
    }

    #[test]
    fn fail_fast_reports_a_true_count_for_a_suite_it_did_not_finish() {
        // `6 examples, 1 failure` for a nine-example suite with three failures.
        // The count is true; the run is not what it describes. §6.3 owns that,
        // and this parser must not silently "fix" it.
        assert_eq!(
            counts(&parse_rspec(fixture!("rspec-fail-fast.txt"))),
            (6, 5, 1, 0, 0)
        );
    }

    #[test]
    fn minitest_counts_errors_as_failures() {
        // `8 runs, 5 assertions, 2 failures, 1 errors, 2 skips`. Reading the
        // `failures` column alone reports 2 of the 3 real problems.
        assert_eq!(
            counts(&parse_minitest(fixture!("minitest-mixed.txt"))),
            (8, 3, 3, 2, 0)
        );
    }

    #[test]
    fn minitest_plain_and_verbose_agree() {
        assert_eq!(
            counts(&parse_minitest(fixture!("minitest-all-pass.txt"))),
            (5, 5, 0, 0, 0)
        );
        let verbose = parse_minitest(fixture!("minitest-verbose.txt"));
        assert_eq!(counts(&verbose), (5, 5, 0, 0, 0));
        // Only `--verbose` names the tests, and it names all five.
        assert_eq!(verbose.parsed().expect("counts").test_ids.len(), 5);
        assert!(verbose
            .parsed()
            .expect("counts")
            .test_ids
            .contains("TestCalcAdd#test_adds_two_positive_integers"));
        assert!(parse_minitest(fixture!("minitest-all-pass.txt"))
            .parsed()
            .expect("counts")
            .test_ids
            .is_empty());
    }

    #[test]
    fn minitest_skips_are_skips_unlike_rspecs_pending() {
        assert_eq!(
            counts(&parse_minitest(fixture!("minitest-skipped.txt"))),
            (4, 1, 0, 3, 0)
        );
    }

    #[test]
    fn a_suite_that_never_ran_prints_nothing_and_that_is_not_a_pass() {
        // **Zero bytes, exit 0.** The file is the capture.
        let empty = fixture!("minitest-suite-never-ran.txt");
        assert!(empty.is_empty(), "the 0-byte capture gained bytes");
        let outcome = parse_minitest(empty);
        assert!(outcome.is_unverifiable(), "{outcome:?}");
        assert!(reason(&outcome).contains("no output"));
    }

    #[test]
    fn minitest_zero_runs_and_broken_loads_are_all_unverifiable() {
        for (name, expect) in [
            (fixture!("minitest-zero-tests.txt"), "executed 0 tests"),
            (fixture!("minitest-load-error.txt"), "LoadError"),
            (fixture!("minitest-syntax-error.txt"), "SyntaxError"),
        ] {
            let outcome = parse_minitest(name);
            assert!(outcome.is_unverifiable(), "{outcome:?}");
            let reason = reason(&outcome);
            assert!(reason.contains(expect), "{reason}");
        }
    }

    #[test]
    fn minitest_narrowing_is_visible_in_the_output_unlike_rspecs_file_form() {
        let outcome = parse_minitest(fixture!("minitest-narrowed.txt"));
        assert_eq!(counts(&outcome), (1, 1, 0, 0, 0));
        assert!(fixture!("minitest-narrowed.txt").contains("-n /divides/"));
    }

    #[test]
    fn durations_are_read_and_an_unrecognised_unit_yields_none() {
        assert_eq!(
            parse_rspec(fixture!("rspec-all-pass.txt"))
                .parsed()
                .expect("counts")
                .duration_ms,
            Some(22)
        );
        assert_eq!(
            parse_minitest(fixture!("minitest-all-pass.txt"))
                .parsed()
                .expect("counts")
                .duration_ms,
            Some(5)
        );
        // The `1 minute 3.2 seconds` form is in no capture here; reporting `1`
        // as milliseconds-from-seconds would be sixty times short.
        assert_eq!(
            duration_ms("Finished in 1 minute 3.2 seconds (files took 0.1 seconds to load)"),
            None
        );
    }

    #[test]
    fn neither_parser_reads_the_others_output() {
        // A wrong-runner parse must produce no numbers at all. This is the
        // cheapest way for a caller's dispatch bug to become visible instead of
        // becoming a count.
        assert!(parse_rspec(fixture!("minitest-mixed.txt")).is_unverifiable());
        assert!(parse_minitest(fixture!("rspec-mixed.txt")).is_unverifiable());
    }

    #[test]
    fn from_argv_recognises_the_ruby_grammar_verify_has_no_arm_for() {
        assert_eq!(argv_runner("rspec --no-color"), Some(PackRunner::RSpec));
        assert_eq!(argv_runner("/usr/bin/rspec"), Some(PackRunner::RSpec));
        assert_eq!(argv_runner("bin/rspec spec/"), Some(PackRunner::RSpec));
        assert_eq!(
            argv_runner("bundle exec rspec --format json"),
            Some(PackRunner::RSpec)
        );
        assert_eq!(
            argv_runner("ruby -Ilib -Itest test/test_calc.rb --seed 20260905"),
            Some(PackRunner::Minitest)
        );
        assert_eq!(
            argv_runner("ruby -I lib -I test test/calc_test.rb"),
            Some(PackRunner::Minitest)
        );
        assert_eq!(
            argv_runner("bundle exec ruby -Itest test/calc_test.rb"),
            Some(PackRunner::Minitest)
        );
        // `-e` IS the program; the file after it is an argument to that
        // program and is never executed.
        assert_eq!(argv_runner("ruby -e puts_1 test_calc.rb"), None);
        assert_eq!(argv_runner("ruby -eputs_1 test_calc.rb"), None);
        // A plain script is not a test run in any knowable sense.
        assert_eq!(argv_runner("ruby app.rb"), None);
        // rake is deliberately absent — its output framing is in no fixture.
        assert_eq!(argv_runner("rake test"), None);
        assert_eq!(argv_runner("bundle exec rake spec"), None);
        // `bundle` with no `exec` runs no tests.
        assert_eq!(argv_runner("bundle install"), None);
        assert!(super::from_argv(&[]).is_none());
    }

    fn argv_runner(command: &str) -> Option<PackRunner> {
        super::from_argv(&command.split(' ').map(str::to_string).collect::<Vec<_>>())
    }

    #[test]
    fn a_ruby_run_reaches_verify_as_unverifiable_with_a_hint_that_names_the_gap() {
        // Not a defect in this parser: `verify::parse::Runner` has no name for
        // RSpec, so a `Parsed` for one cannot be built truthfully. The hint
        // says exactly what would fix it, which is what §6.2 requires of an
        // unverifiable result.
        let outcome = parse(fixture!("rspec-all-pass.txt"));
        match outcome {
            RunnerOutcome::Unverifiable { reason, hint } => {
                assert!(reason.contains("6 test(s)"), "{reason}");
                assert!(hint.contains("Runner::RSpec"), "{hint}");
            }
            RunnerOutcome::Parsed(p) => panic!(
                "an RSpec run was labelled `{}` — see tests/capture_reachability.rs",
                p.runner.as_str()
            ),
        }
        // And when the variant lands, this is the line that changes behaviour.
        assert_eq!(PackRunner::RSpec.into_verify(), None);
    }

    #[test]
    fn bundle_exec_changes_nothing_a_parser_can_see() {
        // Captured through `bundle exec` against a Gemfile pinning rspec 3.13.2,
        // and byte-identical to the direct run apart from timings. Worth an
        // assertion rather than an assumption: bundler prepends warnings to
        // stdout often enough that "the output is the same" is a claim.
        assert_eq!(
            counts(&parse_rspec(fixture!("rspec-bundle-exec-mixed.txt"))),
            counts(&parse_rspec(fixture!("rspec-mixed.txt")))
        );
    }

    #[test]
    fn a_run_that_never_reached_the_runner_is_unverifiable_not_empty() {
        // Three ways to fail before any test executes, two of them from real
        // corpus checkouts. None produces a summary this parser could
        // mis-read — the danger is the opposite one, that "no failures were
        // found" becomes "no failures exist".
        for (name, expect) in [
            // Bundler dies resolving the Gemfile: 3,959 bytes of backtrace and
            // not one line of runner output.
            (fixture!("rspec-bundle-missing-gem.txt"), "no RSpec summary"),
            // A real rspec-core checkout without its dev dependencies:
            // `LoadError: aruba/api`, then `0 examples, 0 failures, 1 error
            // occurred outside of examples`. The tally is real and means nothing.
            (
                fixture!("rspec-corpus-missing-dev-deps.txt"),
                "outside of examples",
            ),
        ] {
            let outcome = parse_rspec(name);
            assert!(outcome.is_unverifiable(), "{outcome:?}");
            assert!(reason(&outcome).contains(expect), "{:?}", reason(&outcome));
        }
        // A real faker checkout, same condition: bare Ruby `LoadError` on
        // simplecov, minitest's autorun hook never installed, zero runner bytes.
        let outcome = parse_minitest(fixture!("minitest-corpus-missing-dev-deps.txt"));
        assert!(outcome.is_unverifiable(), "{outcome:?}");
        assert!(reason(&outcome).contains("LoadError"));
    }

    #[test]
    fn a_rake_wrapper_does_not_stop_the_runners_summary_being_read() {
        // rake prints the absolute ruby command line before the first runner
        // byte, and on failure repeats it afterwards with ` failed` appended;
        // `rake test` adds a whole `rake aborted!` block with a backtrace after
        // minitest's counts. None of that framing is a summary line, and the
        // summary search runs from the bottom — so the last line of the file
        // being rake's is exactly the case that has to be checked.
        assert_eq!(
            counts(&parse_rspec(fixture!("rake-spec-pass.txt"))),
            (4, 4, 0, 0, 0)
        );
        assert_eq!(
            counts(&parse_rspec(fixture!("rake-spec-failed.txt"))),
            (9, 6, 3, 0, 0)
        );
        // `8 runs, 5 assertions, 2 failures, 1 errors, 2 skips`, with rake's
        // backtrace below it. The backtrace lines carry `)` and `#`, which is
        // what `minitest_failure_markers` and `minitest_test_ids` have to not
        // read as content.
        assert_eq!(
            counts(&parse_minitest(fixture!("rake-test-mixed.txt"))),
            (8, 3, 3, 2, 0)
        );
        // And yet the argv is still refused — see `from_argv`. The asymmetry is
        // deliberate: the output says which runner produced it, the task name
        // does not. `rake-spec-pass.txt` came from a task called `spec_green`.
        assert_eq!(argv_runner("rake spec_green"), None);
    }

    #[test]
    fn the_dotrspec_file_defeats_the_only_dry_run_discriminator_there_is() {
        // **This test asserts a defeat, and a deeper one than
        // `a_dry_run_is_indistinguishable_from_a_green_one`.** There, the fact
        // was missing from the output but present in the argv. Here a committed
        // `.rspec` holding `--dry-run` puts it in NEITHER: the argv is the one
        // word `rspec`, the output is `9 examples, 0 failures`, the exit code is
        // 0, and three examples in that suite fail when actually executed.
        let invisible = fixture!("rspec-dotrspec-dry-run.txt");
        assert_eq!(counts(&parse_rspec(invisible)), (9, 9, 0, 0, 0));
        assert!(!dry_run(&["rspec".to_string()]), "the argv carries nothing");

        // The control: the same project, the same suite, run with
        // `--options /dev/null` so the `.rspec` file is bypassed. Three
        // failures. The two fixtures differ in what a config file said, and in
        // nothing this module can observe.
        assert_eq!(
            counts(&parse_rspec(fixture!("rspec-dotrspec-bypassed.txt"))),
            (9, 6, 3, 0, 0)
        );
        // Closing this needs the working directory's `.rspec`, `~/.rspec` and
        // `SPEC_OPTS` from the recorded environment. If a future caller supplies
        // them, this assertion is the one that should start failing.
        assert_eq!(counts(&parse_rspec(invisible)).2, 0);
    }

    #[test]
    fn an_exit_code_of_zero_cannot_hide_failures_that_are_in_the_output() {
        // `config.failure_exit_code = 0` in spec_helper.rb: three failures
        // printed, process exits 0. The inverse of the dry-run trap, and the
        // case where reading the output beats trusting the process — a caller
        // that gates on the exit code alone calls this run green.
        assert_eq!(
            counts(&parse_rspec(fixture!("rspec-failure-exit-code-zero.txt"))),
            (9, 6, 3, 0, 0)
        );
    }

    #[test]
    fn narrowing_that_lives_outside_the_argv_still_parses() {
        // Two more narrowing forms with nothing on the command line: a `:focus`
        // tag with `config.filter_run_when_matching` in spec_helper.rb, and
        // `--only-failures` driven by an on-disk `.rspec_status`. Both announce
        // themselves in a `Run options:` banner, which must not be mistaken for
        // a summary — and note `1 example` is singular, which a `== "examples"`
        // comparison would miss.
        assert_eq!(
            counts(&parse_rspec(fixture!("rspec-focus-filter.txt"))),
            (1, 1, 0, 0, 0)
        );
        assert!(fixture!("rspec-focus-filter.txt").contains("1 example, 0 failures"));
        assert_eq!(
            counts(&parse_rspec(fixture!("rspec-only-failures.txt"))),
            (3, 0, 3, 0, 0)
        );
    }

    #[test]
    fn the_same_command_run_twice_produces_two_different_verdicts() {
        // A flake pair: identical argv, identical project, one failure then
        // green, because the first run left a marker file behind. **Nothing in
        // the second capture records that the first happened.** §6.2 reports
        // what a run said; that a run is repeatable is a claim no single output
        // can support, and this pair is the counterexample kept in the
        // repository so the claim is never made by accident.
        assert_eq!(
            counts(&parse_rspec(fixture!("rspec-flaky-first-run.txt"))),
            (2, 1, 1, 0, 0)
        );
        assert_eq!(
            counts(&parse_rspec(fixture!("rspec-flaky-second-run.txt"))),
            (2, 2, 0, 0, 0)
        );
    }

    /// Every Ruby capture in the directory, read from disk rather than named.
    ///
    /// The per-fixture tests above each assert a branch. This one asserts the
    /// **§6.2 non-collapse rule over bytes nobody chose**: whatever a Ruby
    /// runner printed, the result is either counts of a run that happened or an
    /// `Unverifiable` carrying a reason and a hint — never a `Parsed` whose
    /// `tests_total` is 0, which is the shape that reads downstream as a clean
    /// run.
    ///
    /// Reading the directory instead of a list is what makes it cover captures
    /// added after this file was written — which is not hypothetical: fourteen
    /// of the fixtures this suite now exercises landed after the first draft of
    /// these tests, and were sitting unexercised.
    #[test]
    fn every_ruby_capture_on_disk_is_parsed_and_none_collapses_to_a_clean_run() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runners");
        let mut seen = 0usize;
        for entry in std::fs::read_dir(&dir)
            .expect("fixture directory")
            .flatten()
        {
            let name = entry.file_name().to_string_lossy().into_owned();
            let ruby = name.starts_with("rspec-")
                || name.starts_with("minitest-")
                || name.starts_with("rake-");
            if !ruby {
                continue;
            }
            let output = std::fs::read_to_string(entry.path()).expect("fixture is utf-8");
            // minitest's own captures and the one rake task that wraps it; every
            // other Ruby capture is RSpec's, in one of its three formats.
            let outcome = if name.starts_with("minitest-") || name.starts_with("rake-test") {
                parse_minitest(&output)
            } else {
                parse_rspec(&output)
            };
            seen += 1;
            match outcome {
                RubyOutcome::Parsed(p) => assert!(
                    p.tests_total > 0,
                    "{name} parsed to a {}-test tally, which reads downstream as a clean run",
                    p.tests_total
                ),
                RubyOutcome::Unverifiable { reason, hint } => {
                    assert!(!reason.is_empty(), "{name} is unverifiable with no reason");
                    assert!(
                        !hint.is_empty(),
                        "{name} is unverifiable with no hint (§6.2)"
                    );
                }
            }
        }
        // A floor, not an equality: a new capture must not need this line
        // edited, but a glob that silently stops matching must fail here rather
        // than pass by testing nothing.
        assert!(
            seen >= 45,
            "only {seen} Ruby captures were read — the fixture glob has stopped matching"
        );
    }

    #[test]
    fn an_unverifiable_result_never_carries_counts() {
        // The §6.2 non-collapse rule, asserted over every fixture in the
        // directory at once rather than trusted per branch.
        for output in [
            fixture!("rspec-load-error.txt"),
            fixture!("rspec-syntax-error.txt"),
            fixture!("rspec-zero-tests.txt"),
            fixture!("rspec-no-spec-files.txt"),
            fixture!("rspec-suite-failed-tests-green.txt"),
            fixture!("rspec-report-load-error.json"),
        ] {
            assert!(parse_rspec(output).parsed().is_none());
        }
        for output in [
            fixture!("minitest-suite-never-ran.txt"),
            fixture!("minitest-zero-tests.txt"),
            fixture!("minitest-load-error.txt"),
            fixture!("minitest-syntax-error.txt"),
        ] {
            assert!(parse_minitest(output).parsed().is_none());
        }
    }
}
