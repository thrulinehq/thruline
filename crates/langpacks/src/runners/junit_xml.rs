//! §6.2 — the JUnit XML report, which is the one thing Maven, Gradle, the
//! JUnit Platform console launcher and `dotnet test --logger junit` all write.
//!
//! # Why this crate reads the XML itself instead of calling verify's reader
//!
//! `verify::parse::parse_junit_xml` exists and works on a single-suite report,
//! which is what Surefire and Gradle write one of per test class. It is not
//! edited here — `crates/verify` is read-only for this crate — but reading it
//! against the twelve captured Java reports in
//! `tests/fixtures/runners/` turned up five things it does not do, and the
//! first two of them are wrong rather than merely absent. All five are stated
//! in the merge plan; they are repeated here because this file is where a
//! reader will be standing when they matter:
//!
//! 1. **Counts come from the first `<testsuite>`; failures counted as a
//!    fallback come from the whole document.** `attr("tests")` reads the first
//!    suite element, but `rest.matches("<failure")` counts from that element to
//!    end of file. A document holding two suites — a concatenated report, or a
//!    writer that emits siblings — reports suite one's total against every
//!    suite's failures.
//! 2. **The attribute window is 600 bytes and is not bounded by the opening
//!    tag's `>`.** Surefire's opening tag is ~270 bytes because of two `xmlns`
//!    attributes; one more attribute, or a long suite `name`, pushes an
//!    attribute out of the window, `attr("tests")?` returns `None`, and the
//!    whole XML path silently falls through to the console parser.
//! 3. **A non-JUnit XML dialect is indistinguishable from "not XML".** TestNG
//!    writes `testng-results.xml` beside the JUnit-shaped report, rooted at
//!    `<testng-results total="6" passed="3" failed="2" skipped="1">`. It
//!    contains no `<testsuite` at all, so the reader returns `None` and the
//!    caller reports "no mvn test summary could be read" — true, but the hint
//!    it prints is about enabling a report that is already enabled.
//! 4. **`test_ids` is never filled**, though `<testcase name= classname=>`
//!    names every test in the document. §6.2 asks for it where the format
//!    names tests, and this is the format that always does.
//! 5. **`duration_ms` is never filled**, though every suite carries `time=`.
//!
//! # The three dialects these fixtures actually contain
//!
//! Same schema, three incompatible conventions, and a reader that assumed any
//! one of them would be wrong about the other two:
//!
//! | | `<error>` vs `<failure>` | test name | skipped vs aborted |
//! |---|---|---|---|
//! | Surefire (`maven-surefire-mixed.xml`) | separates them | `looksUpAMissingCustomer` | both `<skipped>` |
//! | Gradle (`gradle-junit-mixed.xml`) | **everything is `<failure>`**, so `errors="0"` always | `looksUpAMissingCustomer()` | both `<skipped>` |
//! | Console launcher (`junit-console-report.xml`) | separates them | `looksUpAMissingCustomer()` | flattened to `skipped="2"` |
//!
//! So `failed` is `failures + errors` in every dialect — never one of them —
//! and a test id has its trailing `()` trimmed, so the same test carries the
//! same id whichever tool wrote the report.

use std::collections::BTreeSet;

use thruline_verify::parse::strip_ansi;

use super::{parsed, unverifiable, Runner, RunnerOutcome};

/// **No argv produces a JUnit XML report.**
///
/// Every other parser in this module recognises a runner from the command that
/// was executed. This one cannot, and saying so is the point: a JUnit report is
/// a *file on disk* that some other command left behind, so it is reached by
/// pointing ThruLine at the file, never by matching `argv[0]`.
/// `verify::parse::Runner::from_argv` has no arm for `Runner::JUnitXml` either,
/// for the same reason.
///
/// The signature is kept so the runner modules present one shape, and so that
/// nobody adds an arm here later on the assumption that the omission was an
/// oversight.
pub fn from_argv(_argv: &[String]) -> Option<Runner> {
    None
}

/// Read a JUnit XML report, labelled as the generic `Runner::JUnitXml`.
pub fn parse(output: &str) -> RunnerOutcome {
    parse_as(output, Runner::JUnitXml)
}

/// Read a JUnit XML report, labelled with the runner that produced it.
///
/// `maven` and `gradle` call this when they are handed a report rather than
/// console output, so a report read from `target/surefire-reports/` is
/// attributed to Maven instead of to a nameless XML writer.
pub fn parse_as(output: &str, runner: Runner) -> RunnerOutcome {
    let plain = strip_ansi(output);
    let text: &str = &plain;

    // TestNG writes `testng-results.xml` into the same directory as the
    // JUnit-shaped report, and its root element is a different schema
    // entirely: `<testng-results ignored="0" total="6" passed="3" failed="2"
    // skipped="1">`, with `<test-method … status="FAIL">` where JUnit has
    // `<testcase>`. It has no `<testsuite` anywhere in it. Reading a `0` out of
    // it is exactly the failure §6.2's return type exists to prevent, so it is
    // refused by name — before the generic "no suite element" branch, whose
    // hint would be wrong here.
    if text.contains("<testng-results") {
        return unverifiable(
            "this is TestNG's own `testng-results.xml`, not a JUnit-shaped report: its \
             root is `<testng-results total= passed= failed= skipped=>` and it contains \
             no `<testsuite>` element",
            "read `target/surefire-reports/TEST-<class>.xml` instead — Surefire's TestNG \
             provider writes a JUnit-shaped report beside this one — or teach ThruLine \
             the `testng-results` dialect; the counts here are real and this reader \
             cannot see them",
        );
    }

    let suites: Vec<usize> = suite_offsets(text);
    if suites.is_empty() {
        // The plural wrapper with no inner suite: some writers put the counts
        // there. Fall back to it rather than refusing a well-formed report.
        if let Some(at) = text.find("<testsuites") {
            return from_elements(text, &[at], runner, "</testsuites>");
        }
        return unverifiable(
            "no `<testsuite>` element is present, so this is not a JUnit XML report",
            "point ThruLine at `target/surefire-reports/TEST-*.xml` (Maven), \
             `build/test-results/test/TEST-*.xml` (Gradle) or the `--reports-dir` \
             output of the JUnit Platform console launcher",
        );
    }
    from_elements(text, &suites, runner, "</testsuite>")
}

/// Sum every suite element in the document.
fn from_elements(text: &str, offsets: &[usize], runner: Runner, closer: &str) -> RunnerOutcome {
    let mut p = parsed(runner);
    let mut seconds = 0.0f64;
    let mut ids = BTreeSet::new();

    for (i, &at) in offsets.iter().enumerate() {
        let end = offsets.get(i + 1).copied().unwrap_or(text.len());
        let span = &text[at..end];
        let tag = opening_tag(span);

        let Some(total) = attr(tag, "tests").and_then(|v| v.parse::<u64>().ok()) else {
            return unverifiable(
                format!(
                    "a `<testsuite>` element carries no readable `tests` attribute: {:?}",
                    &tag[..tag.len().min(160)]
                ),
                "the JUnit schema makes the count optional, so a report without it says \
                 nothing about how many tests ran; re-run with a writer that emits it, or \
                 read the runner's console output instead",
            );
        };

        // Completeness, before any count is believed. Three routes, ORed:
        // writers differ, and requiring all three would refuse well-formed
        // reports. This is `verify::parse::parse_junit_xml`'s check, applied
        // per suite rather than once for the document.
        let cases = span.matches("<testcase").count() as u64;
        let closed = span.contains(closer);
        let self_closing = tag.trim_end().ends_with('/');
        if !(closed || self_closing || cases >= total) {
            return unverifiable(
                format!(
                    "the JUnit report is incomplete: a suite header claims {total} test(s), \
                     {cases} `<testcase>` element(s) are present, and no closing `{closer}` \
                     was found. A report cut off mid-write says nothing about the tests it \
                     never got to."
                ),
                "re-run the suite and let the report finish writing; a runner killed \
                 mid-write, a full disk and an OOM-killed container all produce this shape",
            );
        }

        // Declared, or counted, but never defaulted.
        //
        // `unwrap_or(0)` on an absent attribute is a constant standing where a
        // fact belongs. The schema does make `failures`/`errors` optional, so
        // absence is not a defect — but then the answer comes from counting the
        // elements, which is a measurement of the same document.
        //
        // Both are summed, never one: Gradle writes an escaped exception and an
        // assertion failure both as `<failure>` and reports `errors="0"`
        // always, while Surefire separates them. Reading only `failures` loses
        // every Surefire error; reading only `errors` loses everything Gradle
        // reports.
        let declared_f = attr(tag, "failures").and_then(|v| v.parse::<u64>().ok());
        let declared_e = attr(tag, "errors").and_then(|v| v.parse::<u64>().ok());
        let failed = match (declared_f, declared_e) {
            (None, None) => {
                (span.matches("<failure").count() + span.matches("<error").count()) as u64
            }
            (f, e) => f.unwrap_or(0) + e.unwrap_or(0),
        };
        let skipped = match attr(tag, "skipped").and_then(|v| v.parse::<u64>().ok()) {
            Some(n) => n,
            None => span.matches("<skipped").count() as u64,
        };

        if let Some(t) = attr(tag, "time").and_then(|v| v.parse::<f64>().ok()) {
            seconds += t;
        }
        ids.extend(test_ids_in(span));

        p.tests_total += total;
        p.failed += failed;
        p.skipped += skipped;
        p.passed += total.saturating_sub(failed + skipped);
    }

    p.test_ids = ids;
    p.duration_ms = Some((seconds * 1000.0).round() as u64);
    RunnerOutcome::Parsed(p)
}

/// Byte offsets of every `<testsuite` element, excluding `<testsuites`.
///
/// The distinction is the whole reason this is not `find("<testsuite")`: real
/// `pytest --junit-xml` wraps everything in `<testsuites name="pytest tests">`,
/// which carries no counts, and matching the wrapper reads a report with counts
/// as one without.
fn suite_offsets(text: &str) -> Vec<usize> {
    text.match_indices("<testsuite")
        .filter(|(i, _)| {
            text[i + "<testsuite".len()..]
                .chars()
                .next()
                .is_some_and(|c| c.is_whitespace() || c == '>' || c == '/')
        })
        .map(|(i, _)| i)
        .collect()
}

/// The opening tag at the start of `span`, up to its unquoted `>`.
///
/// Quote-aware, and bounded by the tag rather than by a byte budget. A fixed
/// window that overruns the `>` can read an attribute off a *child* element,
/// and a window too small silently loses the attributes past it — the second
/// defect listed in this module's docs.
fn opening_tag(span: &str) -> &str {
    let mut in_quote: Option<char> = None;
    for (i, c) in span.char_indices() {
        match (in_quote, c) {
            (None, '"') | (None, '\'') => in_quote = Some(c),
            (Some(q), c) if c == q => in_quote = None,
            (None, '>') => return &span[..i],
            _ => {}
        }
    }
    span
}

/// One attribute's value from an opening tag.
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let mut from = 0usize;
    while let Some(at) = tag[from..].find(&key) {
        let start = from + at;
        // A word boundary, so `errors="0"` is not found inside
        // `xsi:schemaErrors="…"` and `tests="3"` is not found inside
        // `retests="9"`.
        let boundary = start == 0 || {
            let b = tag.as_bytes()[start - 1];
            !(b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b':')
        };
        let value_at = start + key.len();
        if boundary {
            return tag[value_at..].split('"').next();
        }
        from = value_at;
    }
    None
}

/// `classname.name` for every `<testcase>` in a span.
///
/// The trailing `()` Gradle and the console launcher keep on a method name is
/// trimmed, because Surefire strips it and the same test must carry the same id
/// whichever tool wrote the report. Parameterised names — `param(int)[1]` —
/// keep everything inside the parentheses; only a literal empty pair is
/// removed.
fn test_ids_in(span: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (at, _) in span.match_indices("<testcase") {
        let tag = opening_tag(&span[at..]);
        let Some(name) = attr(tag, "name") else {
            continue;
        };
        let name = name.strip_suffix("()").unwrap_or(name);
        match attr(tag, "classname") {
            Some(class) if !class.is_empty() => out.insert(format!("{class}.{name}")),
            _ => out.insert(name.to_string()),
        };
    }
    out
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

    #[test]
    fn no_argv_names_this_runner() {
        let argv = |s: &str| s.split(' ').map(str::to_string).collect::<Vec<_>>();
        assert_eq!(from_argv(&argv("mvn test")), None);
        assert_eq!(from_argv(&argv("junit-xml")), None);
        assert_eq!(from_argv(&[]), None);
    }

    // --- Surefire ----------------------------------------------------------

    #[test]
    fn surefire_all_pass() {
        let p = ok("maven-surefire-all-pass.xml");
        assert_eq!((p.tests_total, p.passed, p.failed, p.skipped), (3, 3, 0, 0));
        assert!(p.test_ids.contains("demo.CalculatorTest.addsTwoNumbers"));
    }

    #[test]
    fn surefire_separates_error_from_failure_and_both_are_failures() {
        // `errors="1" failures="1"` over three tests. Reading either attribute
        // alone reports one failure where there are two.
        let text = fixture("maven-surefire-mixed.xml");
        assert!(text.contains("errors=\"1\"") && text.contains("failures=\"1\""));
        let p = ok("maven-surefire-mixed.xml");
        assert_eq!((p.tests_total, p.passed, p.failed), (3, 1, 2));
    }

    #[test]
    fn surefire_skipped() {
        let p = ok("maven-surefire-skipped.xml");
        assert_eq!((p.tests_total, p.passed, p.skipped, p.failed), (3, 1, 2, 0));
    }

    #[test]
    fn failsafe_integration_failure() {
        let p = ok("maven-failsafe-it-failure.xml");
        assert_eq!((p.tests_total, p.failed), (1, 1));
        assert!(p.test_ids.contains("demo.CheckoutIT.chargesTheCardOnce"));
    }

    #[test]
    fn surefire_junit4() {
        // 7 tests: 1 assertion failure, 1 escaped NPE, 2 skipped.
        let p = ok("maven-junit4-surefire.xml");
        assert_eq!((p.tests_total, p.passed, p.failed, p.skipped), (7, 3, 2, 2));
    }

    #[test]
    fn surefire_testng_has_no_error_failure_distinction() {
        // TestNG reports an escaped NullPointerException as a `<failure>`, so
        // `errors="0"` and `failures="2"`. A reader keyed on `errors` for
        // "exception escaped" sees none here and two in the JUnit 4 report of
        // the same six tests.
        let text = fixture("maven-testng-surefire.xml");
        assert!(text.contains("errors=\"0\"") && text.contains("failures=\"2\""));
        let p = ok("maven-testng-surefire.xml");
        assert_eq!((p.tests_total, p.passed, p.failed, p.skipped), (6, 3, 2, 1));
    }

    // --- Gradle ------------------------------------------------------------

    #[test]
    fn gradle_all_pass() {
        let p = ok("gradle-junit-all-pass.xml");
        assert_eq!((p.tests_total, p.passed, p.failed), (3, 3, 0));
        // Gradle keeps the parentheses; the id does not.
        assert!(fixture("gradle-junit-all-pass.xml").contains("name=\"addsTwoNumbers()\""));
        assert!(p.test_ids.contains("demo.CalculatorTest.addsTwoNumbers"));
    }

    #[test]
    fn gradle_writes_every_failure_as_a_failure_so_errors_is_always_zero() {
        let text = fixture("gradle-junit-mixed.xml");
        assert!(text.contains("failures=\"2\"") && text.contains("errors=\"0\""));
        assert!(
            !text.contains("<error"),
            "the escaped NullPointerException is written as <failure>"
        );
        let p = ok("gradle-junit-mixed.xml");
        assert_eq!((p.tests_total, p.passed, p.failed), (3, 1, 2));
    }

    #[test]
    fn gradle_skipped() {
        let p = ok("gradle-junit-skipped.xml");
        assert_eq!((p.tests_total, p.passed, p.skipped), (3, 1, 2));
    }

    #[test]
    fn gradle_vintage_loses_the_ignore_reason_but_not_the_count() {
        // `@Ignore("reason")` degrades to a bare `<skipped/>` while the failed
        // assumption keeps `message=` and `type=`. The count survives either
        // way, which is what this reader needs.
        let text = fixture("gradle-vintage-junit.xml");
        assert!(text.contains("<skipped/>"));
        let p = ok("gradle-vintage-junit.xml");
        assert_eq!((p.tests_total, p.passed, p.failed, p.skipped), (7, 3, 2, 2));
    }

    // --- the console launcher ---------------------------------------------

    #[test]
    fn the_console_launcher_flattens_aborted_back_into_skipped() {
        // Its own console tally splits skipped from aborted (1 and 1); the XML
        // it writes from the same run reports `skipped="2"`. The XML is the
        // consistent one.
        let p = ok("junit-console-report.xml");
        assert_eq!((p.tests_total, p.passed, p.failed, p.skipped), (9, 5, 2, 2));
        assert!(p
            .test_ids
            .contains("demo.OrderServiceTest.totalsAnEmptyOrder"));
    }

    // --- the dialect that must not parse to zero --------------------------

    #[test]
    fn testng_results_xml_is_refused_by_name_and_not_read_as_zero() {
        let text = fixture("maven-testng-results.xml");
        assert!(text.contains("<testng-results"));
        assert!(
            !text.contains("<testsuite"),
            "it carries no JUnit element at all"
        );
        let reason = why("maven-testng-results.xml");
        assert!(reason.contains("testng-results"), "{reason}");
    }

    #[test]
    fn console_output_is_not_a_report() {
        let reason = why("maven-all-pass.txt");
        assert!(reason.contains("not a JUnit XML report"), "{reason}");
    }

    // --- the properties the reader is built on ----------------------------

    #[test]
    fn a_report_cut_off_mid_write_is_refused() {
        // Real bytes, truncated: the header still claims three tests and the
        // document now ends before the last case and before `</testsuite>`.
        let full = fixture("maven-surefire-all-pass.xml");
        let at = full.find("<testcase").expect("a case is present");
        let truncated = &full[..at + 40];
        match parse(truncated) {
            RunnerOutcome::Unverifiable { reason, .. } => {
                assert!(reason.contains("incomplete"), "{reason}")
            }
            RunnerOutcome::Parsed(p) => panic!("a truncated report parsed to {p:?}"),
        }
    }

    #[test]
    fn two_suites_in_one_document_are_summed_not_read_from_the_first() {
        // The defect this reader was written against: a first clean suite and a
        // second failing one. Concatenating two captured reports produces
        // exactly that document.
        let clean = fixture("maven-surefire-all-pass.xml");
        let failing = fixture("maven-surefire-mixed.xml");
        let both = format!("{clean}\n{failing}");
        let RunnerOutcome::Parsed(p) = parse(&both) else {
            panic!("two real reports concatenated must parse")
        };
        assert_eq!((p.tests_total, p.passed, p.failed), (6, 4, 2));
    }

    #[test]
    fn the_runner_label_is_the_callers_to_set() {
        let text = fixture("maven-surefire-all-pass.xml");
        let RunnerOutcome::Parsed(p) = parse_as(&text, Runner::Maven) else {
            panic!("parses")
        };
        assert_eq!(p.runner, Runner::Maven);
        assert_eq!(
            parse(&text).parsed().map(|p| p.runner),
            Some(Runner::JUnitXml)
        );
    }

    #[test]
    fn attributes_are_read_from_the_opening_tag_and_on_word_boundaries() {
        assert_eq!(
            attr("<testsuite tests=\"3\" errors=\"0\"", "tests"),
            Some("3")
        );
        // `retests="9"` must not answer a request for `tests`.
        assert_eq!(
            attr("<testsuite retests=\"9\" tests=\"3\"", "tests"),
            Some("3")
        );
        assert_eq!(attr("<testsuite name=\"a\"", "tests"), None);
        // The tag ends at the unquoted `>`, so a child's attribute is out of
        // reach even when the tag is short.
        assert_eq!(
            opening_tag("<testsuite name=\"a>b\" tests=\"3\"><testcase tests=\"99\"/>"),
            "<testsuite name=\"a>b\" tests=\"3\""
        );
    }
}
