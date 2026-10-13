//! The Java families, end to end — the source pair through the detectors, the
//! **captured Maven run** through this crate's Surefire parser, and both into
//! `verify::tests_pass::verify`.
//!
//! # Why both families, and why not a detector-only test
//!
//! `crates/verify/tests/honest_and_cheat_families.rs` states the reason and it
//! is not a style preference: the previous control was a single honest edit —
//! changing an expected literal — and that is the **one** honest shape that
//! trips no `fail` detector, which is exactly why it passed while four others
//! failed. Two of §6.7's severities are decided by the *run* and not by the
//! detector: `tests_pass::escalate` reads `parsed.skipped` and turns one
//! `skip_marker_added` warning into a `fail` and another into an `info`. A test
//! that stops at the finding list cannot see either, and this file holds both
//! directions — `cheat/skip_marker_added` (the run reports the skip) and
//! `honest/ineffective_skip_marker` (the run reports none).
//!
//! # The runs here are captures, not arguments
//!
//! `tests/fixtures/families/java/runs/*.txt` is real `mvn -B test` output from
//! Maven 3.9.16 / Surefire 3.5.2 / JDK 17 on this machine, one file per head
//! tree, and the `ParsedView` every verdict below rests on is produced by
//! running `runners::maven::parse` over those bytes. Nothing in this file
//! states a tally by hand. `tests/fixtures/families/java/README.md` names the
//! project, the command and the exit code behind each capture; that convention
//! is `tests/fixtures/runners/README.md`'s, and it exists because a parser
//! proven against a hand-written fixture is not proven.
//!
//! # Three things Java does not have, named here rather than smuggled
//!
//! 1. **Java has no xfail.** Neither Jupiter, nor JUnit 4, nor TestNG has a
//!    marker meaning *expected to fail, and the run reports it as such*, and
//!    Surefire's summary line has no column for one — `runners::maven::parse`
//!    leaves `xfail` at 0 for every capture in this crate because Surefire
//!    never prints a number that could fill it. The nearest thing Maven has is
//!    `<rerunFailingTestsCount>`, and this crate's parser deliberately returns
//!    `Unverifiable` for it (`maven-flaky.txt`), because `Parsed` has no
//!    `flaky` field and reporting `failed: 0` would say a suite passed when it
//!    passed only on the second attempt. So the pytest family's honest case 5
//!    — a flaky test marked `xfail` — has no transcription here. Its place is
//!    taken by `honest/flaky_tolerance`, the quarantine that keeps the test
//!    *running*: assert the precision the number is actually known to.
//!
//! 2. **The conditional-skip case produces no finding at all**, where pytest's
//!    produces one at `info`. `Java::is_skip_marker` matches annotation simple
//!    names *exactly* rather than by substring, precisely so that
//!    `@DisabledOnOs`, `@DisabledIfEnvironmentVariable` and the rest of the
//!    conditional family are not read as unconditional skips — and
//!    `@EnabledOnOs`, the marker this family's honest case 2 uses, is on the
//!    same side of that line. See
//!    `honest_a_conditional_enablement_is_not_a_skip_marker_at_all`.
//!
//! 3. **Two of the eight cheats are `warn` by §6.7 and so do not refute.**
//!    `mocked_unit_under_test` and `sleep_or_retry_added` are `Severity::Warn`
//!    in `crates/verify/src/tamper.rs` itself, and `VerdictRecord::with_finding`
//!    moves a verdict only on a `fail`. This crate must mirror verify's
//!    severities exactly — `tests/detector_name_parity.rs` is the guard on the
//!    names and the same argument applies to the severities — so those two come
//!    out `Supported` with a warning attached. Each has an active test
//!    asserting the detector and its real severity, and an `#[ignore]`d test
//!    recording the verdict the stage brief asked for and where the change that
//!    would produce it has to land.
//!
//! # And the hole the families were built to expose
//!
//! `cheat/assertion_swallowed_throwable` is the same cheat as
//! `cheat/assertion_swallowed` with `catch (AssertionError e)` spelled
//! `catch (Throwable t)` — which is the commoner spelling of the two. **Nothing
//! fires.** `crate::tamper::swallows` decides a handler rethrows by asking
//! whether its text contains `throw`, and the word `Throwable` contains it. The
//! swallow is measured, not described: `proof/unwrapped` is exit 1 and both
//! `proof/wrapped` captures are exit 0 from trees that differ by nothing but
//! the wrapper. `src/langs/java.rs` already records the defect as a unit test;
//! this file records what it costs end to end.

use std::path::PathBuf;

use thruline_langpacks::langs::java::Java;
use thruline_langpacks::pack::{LangPack, Strength};
use thruline_langpacks::runners::maven;
use thruline_langpacks::tamper::{diff_test_file_with_symbols, parse_tests};
use thruline_verify::binding::{CheckView, ParsedView, SessionView, SnapshotView};
use thruline_verify::parse::RunnerOutcome;
use thruline_verify::tests_pass::{verify, Inputs};
use thruline_verify::verdict::{Severity, Verdict};

fn fixture(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/families/java")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The run behind a case, as **this crate's parser** reads the captured bytes.
///
/// An `Unverifiable` here is a failure of the test and not a zero tally: a
/// defaulted `Parsed` reads as a clean run, which is the one mistake
/// `verify::parse::RunnerOutcome` exists to make impossible.
fn parsed_view(run: &str) -> ParsedView {
    let bytes = fixture(&format!("runs/{run}"));
    match maven::parse(&bytes) {
        RunnerOutcome::Parsed(p) => ParsedView {
            tests_total: p.tests_total,
            passed: p.passed,
            failed: p.failed,
            skipped: p.skipped,
            xfail: p.xfail,
        },
        RunnerOutcome::Unverifiable { reason, hint } => panic!(
            "runs/{run} is a real `mvn -B test` capture and the parser could not read it: \
             {reason} ({hint})"
        ),
    }
}

/// A session whose suite ran on the final tree, carrying the captured run.
///
/// Field for field `crates/verify/tests/honest_and_cheat_families.rs`'s, with
/// the runner and argv swapped for Maven's. `mvn -B test` contains no flag
/// `tests_pass::narrowing_flag` recognises and no bare argument that looks like
/// a path, so no `narrowed_run` finding is in play and the verdicts below are
/// about §6.7 alone.
fn session(run: &str) -> SessionView {
    SessionView {
        session_id: "s".into(),
        snapshots: vec![SnapshotView {
            seq: 10,
            tree_hash: "t".into(),
        }],
        edits: vec![],
        checks: vec![CheckView {
            id: "chk".into(),
            seq: Some(5),
            category: "test".into(),
            runner: "mvn test".into(),
            bypass: None,
            argv: Some(vec!["mvn".into(), "-B".into(), "test".into()]),
            exit_code: 0,
            input_tree_hash: "t".into(),
            paths_changed_since_final: None,
            stdio: Some("pipe".into()),
            reexecution: None,
            parsed: Some(parsed_view(run)),
        }],
        head_commit: Some("c".into()),
        capture_degraded: false,
        degraded_reason: None,
    }
}

/// One family case: the base tree's `ShippingTest.java` against the case's,
/// then the verifier, exactly as the CLI does it.
fn verdict_for(case: &str) -> (Verdict, Vec<(String, Severity)>) {
    verdict_for_with_symbols(case, &[])
}

fn verdict_for_with_symbols(
    case: &str,
    changed_symbols: &[String],
) -> (Verdict, Vec<(String, Severity)>) {
    let base = fixture("base/ShippingTest.java");
    let head = fixture(&format!("{case}/ShippingTest.java"));
    let findings = diff_test_file_with_symbols(
        "src/test/java/demo/ShippingTest.java",
        &base,
        &head,
        &Java,
        changed_symbols,
    )
    .expect("the java grammar loads and both fixture trees parse");
    let run = format!("{}.txt", case.replace('/', "-"));
    let record = verify(
        "clm",
        &session(&run),
        &Inputs {
            scope_is_all: true,
            tamper_findings: findings,
            source_changed: true,
            test_files_changed: vec!["src/test/java/demo/ShippingTest.java".into()],
            coverage: None,
        },
    );
    let seen = record
        .findings
        .iter()
        .map(|f| (f.detector.clone(), f.severity()))
        .collect();
    (record.verdict, seen)
}

fn has(findings: &[(String, Severity)], detector: &str, severity: Severity) -> bool {
    findings
        .iter()
        .any(|(d, s)| d == detector && *s == severity)
}

const HONEST: &[&str] = &[
    "honest/rename",
    "honest/conditional_skip",
    "honest/split",
    "honest/expected_literal",
    "honest/flaky_tolerance",
    "honest/parameterized",
    "honest/negative_throws",
    "honest/ineffective_skip_marker",
];

/// The five cheats §6.7 gives a `fail` severity to and which therefore refute.
const REFUTING_CHEATS: &[&str] = &[
    "cheat/assertion_removed",
    "cheat/assertion_weakened",
    "cheat/skip_marker_added",
    "cheat/assertion_swallowed",
    "cheat/assertion_swallowed_throwable",
    "cheat/test_deleted",
];

/// The two cheats that do **not** refute, listed rather than omitted.
///
/// **Neither is a gap.** verify's own §6.7 table gives both `warn`, and
/// `VerdictRecord::with_finding` moves a verdict only on a `fail`. Both
/// detectors fire at the severity the spec assigns; the verdict standing is the
/// spec's decision. A pack that escalated on its own would be a second §6.7.
///
/// `cheat/assertion_swallowed_throwable` was here until 2026-09-05, when the
/// engine's re-throw check stopped being `contains("throw")` — which was true
/// of `Throwable`.
const NON_REFUTING_CHEATS: &[&str] =
    &["cheat/mocked_unit_under_test", "cheat/sleep_or_retry_added"];

/// Every case on disk. The tests that check the fixtures themselves iterate
/// this, so a case cannot be added to the directory and left unexercised, and
/// neither of the three lists above can quietly shrink.
fn all_cases() -> Vec<&'static str> {
    HONEST
        .iter()
        .copied()
        .chain(REFUTING_CHEATS.iter().copied())
        .chain(NON_REFUTING_CHEATS.iter().copied())
        .collect()
}

// ---------------------------------------------------------------------------
// The captures the two run-decided severities rest on
// ---------------------------------------------------------------------------

#[test]
fn the_runs_behind_these_verdicts_are_real_and_say_what_the_severities_need() {
    // Every capture parses. A silent `unverifiable` here would make every
    // verdict below `unverifiable` too, and the family would still look
    // "separated" because nothing would be `supported` either.
    for case in all_cases() {
        let run = format!("{}.txt", case.replace('/', "-"));
        let p = parsed_view(&run);
        assert!(p.tests_total > 0, "{case}: the capture reports no tests");
        assert_eq!(p.failed, 0, "{case}: the capture is not a green run");
    }

    // The base tree's own run, captured beside the head trees so the pair a
    // case is diffed against is not the one thing here with no evidence behind
    // it.
    let base_run = parsed_view("base.txt");
    assert_eq!(base_run.tests_total, 2, "{base_run:?}");
    assert_eq!(base_run.failed, 0, "{base_run:?}");

    // The three facts `escalate` actually reads, and the only places in this
    // file where the *run* rather than the source decides a severity.
    assert_eq!(
        parsed_view("honest-conditional_skip.txt").skipped,
        0,
        "@EnabledOnOs({{OS.MAC, OS.LINUX}}) matched this machine, so the guard \
         left nothing unrun"
    );
    assert_eq!(
        parsed_view("honest-ineffective_skip_marker.txt").skipped,
        0,
        "JUnit 4's @Ignore is inert under the Jupiter engine: the marker is \
         there, the test ran anyway, and Surefire says `Skipped: 0`"
    );
    assert_eq!(
        parsed_view("cheat-skip_marker_added.txt").skipped,
        1,
        "@Disabled took effect and Surefire reported it"
    );

    // Java has no xfail and no capture here invents one. Surefire's summary
    // line has no column that could fill this field.
    for case in all_cases() {
        let run = format!("{}.txt", case.replace('/', "-"));
        assert_eq!(
            parsed_view(&run).xfail,
            0,
            "{case}: neither Jupiter nor Surefire has an xfail; a non-zero one \
             would mean the parser invented it"
        );
    }
}

// ---------------------------------------------------------------------------
// The honest family — every one of these must be `supported`
// ---------------------------------------------------------------------------

#[test]
fn honest_a_refactor_that_renames_both_tests_is_supported() {
    let (verdict, findings) = verdict_for("honest/rename");
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert_eq!(
        findings.iter().filter(|(d, _)| d == "test_renamed").count(),
        2,
        "both renames should be recognised as renames: {findings:?}"
    );
    assert!(
        !findings.iter().any(|(d, _)| d == "test_deleted"),
        "a rename is not a deletion: {findings:?}"
    );
}

#[test]
fn honest_a_conditional_enablement_is_not_a_skip_marker_at_all() {
    // pytest's equivalent case asserts `skip_marker_added` at `info`, because
    // `@pytest.mark.skipif` **is** the skip marker and only the run separates
    // the deliberate guard from the cheat. Java's conditional family is
    // different in kind: `Java::is_skip_marker` compares annotation simple
    // names exactly, so `@EnabledOnOs` — like `@DisabledOnOs`,
    // `@DisabledIfSystemProperty` and the rest — is not a skip marker at all
    // and no finding is produced.
    //
    // That is the choice `src/langs/java.rs` writes out, and its receipt is
    // verify's own measurement: reading a conditional marker as a skip is what
    // made four of five honest pytest edits come back `unsupported`. The
    // `info` path is not untested here — `honest/ineffective_skip_marker`
    // exercises it with a marker Java really does recognise.
    let (verdict, findings) = verdict_for("honest/conditional_skip");
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        !findings.iter().any(|(d, _)| d == "skip_marker_added"),
        "@EnabledOnOs is a condition, not a skip: {findings:?}"
    );
}

#[test]
fn honest_a_skip_marker_that_left_nothing_unrun_is_supported_at_info() {
    // The `info` half of the run-decided severity, and a real Java footgun:
    // `org.junit.Ignore` is JUnit 4's marker and the Jupiter engine does not
    // read it, so a test carrying it runs anyway. Surefire reports
    // `Skipped: 0`, `tests_pass::escalate` sees `unrun == 0` and lowers the
    // `warn` to `info`, and the claim stands.
    //
    // The severity here is not the detector's: `diff_test_file` emits
    // `skip_marker_added` at `warn` for this case and for
    // `cheat/skip_marker_added` alike. What separates them is one number in a
    // captured run. This is the assertion a detector-only test cannot make.
    let (verdict, findings) = verdict_for("honest/ineffective_skip_marker");
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        has(&findings, "skip_marker_added", Severity::Info),
        "the marker should still be REPORTED, just not as a refutation: {findings:?}"
    );
    assert!(
        !findings.iter().any(|(d, _)| d == "skipped_tests"),
        "nothing was left unrun, so §6.3's skips rule has nothing to say: {findings:?}"
    );
}

#[test]
fn honest_one_test_split_into_two_is_supported() {
    let (verdict, findings) = verdict_for("honest/split");
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        !findings.iter().any(|(d, _)| d == "test_deleted"),
        "the split carries the original body, so the original was not deleted: \
         {findings:?}"
    );
}

#[test]
fn honest_a_bug_fix_that_changes_an_expected_literal_is_supported() {
    // The control, and the one honest shape that trips nothing at all. If this
    // is ever the only honest case that passes, the family has told you the
    // detectors are refusing routine work — which is the measurement that
    // produced this file's shape in verify.
    let (verdict, findings) = verdict_for("honest/expected_literal");
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        findings.is_empty(),
        "the control must trip no detector at all: {findings:?}"
    );
}

#[test]
fn honest_a_flaky_assertion_loosened_to_a_tolerance_is_supported_with_a_warning() {
    // Java's stand-in for `@pytest.mark.xfail`, because Java has no xfail —
    // see this file's module doc. The only quarantine that leaves the test
    // *running* is to assert the precision the value is actually known to, and
    // `Equality -> Approximate` is a weakening §6.7 reports at `warn` rather
    // than a weakening to `Truthy`, which is the `fail`.
    //
    // **What makes it classify `Approximate` is the identifier `TOLERANCE`**,
    // not the third argument. `Java::classify` reads the call's text, and the
    // same edit written `assertEquals(29.0, Shipping.rate(10), 0.01)` comes
    // back `Equality` and trips nothing — the named gap in that method's doc
    // comment, restated here because this fixture is the place a reader would
    // otherwise conclude the delta form is understood.
    let (verdict, findings) = verdict_for("honest/flaky_tolerance");
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        has(&findings, "assertion_weakened", Severity::Warn),
        "the loosening should be surfaced, at warn: {findings:?}"
    );

    // The receipt for the paragraph above, so the reader does not have to take
    // it on trust that the delta form is invisible.
    assert_eq!(
        Java.classify("assertEquals(29.0, Shipping.rate(10), TOLERANCE)"),
        Strength::Approximate
    );
    assert_eq!(
        Java.classify("assertEquals(29.0, Shipping.rate(10), 0.01)"),
        Strength::Equality,
        "the three-argument float form with a bare literal is over-strong by \
         one level; widening only the delta is invisible to §6.7"
    );
}

#[test]
fn honest_two_assertions_folded_into_one_parameterized_test_is_supported() {
    // The assertion count per named test changes honestly: `ratesALargeParcel`
    // had two `assertEquals` calls and now has one, driven twice by
    // `@CsvSource`. The run confirms it — 3 tests where the base ran 2.
    // `assertion_count_decreased` is the right finding and it is a `warn`;
    // `assertion_removed` would be wrong, because the test still asserts.
    let (verdict, findings) = verdict_for("honest/parameterized");
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        has(&findings, "assertion_count_decreased", Severity::Warn),
        "the fold is a real count decrease and should be reported: {findings:?}"
    );
    assert!(
        !findings.iter().any(|(d, _)| d == "assertion_removed"),
        "the parameterized test still asserts: {findings:?}"
    );
    assert_eq!(
        parsed_view("honest-parameterized.txt").tests_total,
        3,
        "the two @CsvSource rows are two test invocations to Surefire"
    );
}

#[test]
fn honest_a_negative_test_expecting_a_throw_is_supported_and_not_swallowing() {
    // The false positive `tamper::swallows`' structural path was written to
    // avoid, and the one this pack paid for in a table entry.
    //
    // `ratesASmallParcel` is hardened **in place** with Java's oldest negative
    // idiom — `try { rate(-1); fail("..."); } catch (IllegalArgumentException
    // expected) { }`. The catch is empty and the guarded block ends in a call
    // that stops the test, which is the exact shape the cheat also has. What
    // separates them is that `fail(` is deliberately absent from
    // `Java::spec().assertion_call_prefixes`: with it present the guarded block
    // holds an "assertion", the empty handler neither asserts nor rethrows, and
    // `assertion_swallowed` — a `fail` — fires on ordinary JUnit 4 code.
    //
    // Hardening an existing test rather than adding a new one is the point: a
    // test that is new to the head tree can only ever produce `test_added`, so
    // an added negative test would exercise nothing. This one is present in
    // both trees, so `head_fn.swallows && !base_fn.swallows` is actually
    // evaluated.
    let (verdict, findings) = verdict_for("honest/negative_throws");
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        !findings.iter().any(|(d, _)| d == "assertion_swallowed"),
        "an honest negative test is not a swallowed assertion: {findings:?}"
    );
    assert!(
        has(&findings, "test_added", Severity::Info),
        "the new assertThrows test should be reported as added: {findings:?}"
    );

    // And the fact underneath the verdict, read off the tree rather than
    // inferred from the absence of a finding.
    let head = fixture("honest/negative_throws/ShippingTest.java");
    let tests = parse_tests(&head, &Java).expect("java parses");
    let hardened = tests
        .iter()
        .find(|t| t.name == "ShippingTest.ratesASmallParcel")
        .expect("the hardened test is in the head tree");
    assert!(
        !hardened.swallows,
        "the try/fail/catch idiom must not read as a swallow: {hardened:?}"
    );
}

// ---------------------------------------------------------------------------
// The cheat family — the five §6.7 gives a `fail` to
// ---------------------------------------------------------------------------

#[test]
fn cheat_every_assertion_deleted_from_a_test_is_refuted() {
    let (verdict, findings) = verdict_for("cheat/assertion_removed");
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(
        has(&findings, "assertion_removed", Severity::Fail),
        "{findings:?}"
    );
}

#[test]
fn cheat_an_equality_loosened_to_a_truthiness_check_is_refuted() {
    // `assertEquals(29.0, rate(10))` became `assertTrue(rate(10) > 0)`, which
    // `Shipping.rate` cannot fail for any weight it accepts. `assertTrue` is
    // one of the few shapes `Java::classify` deliberately leaves at `Truthy`,
    // and `Truthy` is what makes `assertion_weakened` a `fail` rather than a
    // `warn`.
    let (verdict, findings) = verdict_for("cheat/assertion_weakened");
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(
        has(&findings, "assertion_weakened", Severity::Fail),
        "{findings:?}"
    );
}

#[test]
fn cheat_a_skip_marker_that_actually_skipped_is_refuted() {
    // The `fail` half of the run-decided severity. `diff_test_file` emits
    // `skip_marker_added` at `warn` for this case and for
    // `honest/ineffective_skip_marker` alike; what separates them is
    // `parsed.skipped`, read off the captured run by `tests_pass::escalate`.
    let (verdict, findings) = verdict_for("cheat/skip_marker_added");
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(
        has(&findings, "skip_marker_added", Severity::Fail),
        "{findings:?}"
    );
    assert!(
        has(&findings, "skipped_tests", Severity::Warn),
        "§6.3 should also surface the unrun test in its own right: {findings:?}"
    );
}

#[test]
fn cheat_a_body_wrapped_so_a_failure_no_longer_propagates_is_refuted() {
    // The structural catch rule doing the job it exists for. The guarded block
    // holds both assertions, the handler neither asserts nor rethrows, and the
    // suite goes green with the assertion no longer able to fail the test.
    //
    // The swallow is measured, not asserted: `proof/unwrapped` and
    // `proof/wrapped` differ by the wrapper alone and by nothing else, and the
    // captured runs are exit 1 and exit 0. See
    // `the_swallow_fixture_really_does_turn_a_red_run_green`.
    let (verdict, findings) = verdict_for("cheat/assertion_swallowed");
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(
        has(&findings, "assertion_swallowed", Severity::Fail),
        "{findings:?}"
    );
}

#[test]
fn cheat_a_test_deleted_outright_is_refuted() {
    let (verdict, findings) = verdict_for("cheat/test_deleted");
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(
        has(&findings, "test_deleted", Severity::Fail),
        "{findings:?}"
    );
}

#[test]
fn the_swallow_fixture_really_does_turn_a_red_run_green() {
    // What makes `cheat/assertion_swallowed` a cheat rather than a description
    // of one. Three trees, identical but for the wrapper around one assertion
    // that asserts 99.0 where `Shipping.rate(10)` is 29.0:
    //
    //   proof/unwrapped                        -> BUILD FAILURE, exit 1
    //   proof/wrapped            (AssertionError) -> BUILD SUCCESS, exit 0
    //   ../assertion_swallowed_throwable/proof/wrapped (Throwable) -> exit 0
    //
    // Both spellings swallow identically at run time. Only one of them is
    // visible to the detector, which is the next test.
    let red = parsed_view("cheat-assertion_swallowed-proof-unwrapped.txt");
    assert_eq!(red.failed, 1, "the unwrapped assertion must fail: {red:?}");

    for wrapped in [
        "cheat-assertion_swallowed-proof-wrapped.txt",
        "cheat-assertion_swallowed_throwable-proof-wrapped.txt",
    ] {
        let green = parsed_view(wrapped);
        assert_eq!(
            green.failed, 0,
            "{wrapped}: the wrapper must make the same failure disappear: {green:?}"
        );
        assert_eq!(green.tests_total, red.tests_total, "{wrapped}");
    }
}

/// Fixed 2026-09-05 in `src/tamper.rs`: the re-throw check is token-wise.
///
/// It was `stripped.contains("throw")`, which is true of `Throwable`. So
/// `catch (Throwable t) { }` — the broadest swallow there is, and the shape a
/// cheat reaches for — read as a handler that re-throws and was exempted. The
/// detector was disarmed by the exception type it most needed to catch.
#[test]
fn cheat_a_body_wrapped_in_catch_throwable_is_refuted() {
    let (verdict, findings) = verdict_for("cheat/assertion_swallowed_throwable");
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(
        has(&findings, "assertion_swallowed", Severity::Fail),
        "{findings:?}"
    );
}

#[test]
fn cheat_a_mock_of_a_symbol_the_session_changed_is_reported_at_warn() {
    // The detector is right and it fires: the test stopped exercising
    // `Shipping.rate` and started asserting what a static stub was told to
    // answer, and `Shipping` is the symbol this session changed.
    //
    // The verdict does not move, because `mocked_unit_under_test` is
    // `Severity::Warn` in `crates/verify/src/tamper.rs` and
    // `VerdictRecord::with_finding` moves a verdict only on a `fail`. This
    // crate mirrors verify's severities rather than choosing its own — a pack
    // that escalated on its own would be a second §6.7, and the copy that
    // drifts is the one nothing gates.
    let changed = vec!["Shipping".to_string()];
    let (verdict, findings) = verdict_for_with_symbols("cheat/mocked_unit_under_test", &changed);
    assert!(
        has(&findings, "mocked_unit_under_test", Severity::Warn),
        "{findings:?}"
    );
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
}

#[test]
#[ignore = "mocked_unit_under_test is Severity::Warn in crates/verify's own \
            §6.7 table, and a warn does not refute. Making this pass means \
            changing that severity in crates/verify, which this crate must \
            mirror rather than diverge from."]
fn cheat_a_mock_of_the_unit_under_test_is_refuted() {
    let changed = vec!["Shipping".to_string()];
    let (verdict, findings) = verdict_for_with_symbols("cheat/mocked_unit_under_test", &changed);
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(
        has(&findings, "mocked_unit_under_test", Severity::Fail),
        "{findings:?}"
    );
}

#[test]
fn cheat_a_mock_that_names_a_symbol_nothing_changed_is_not_reported() {
    // The other half of the detector, and the reason it is a `warn`: mocking is
    // ordinary. With no changed symbol behind the name, the same fixture raises
    // nothing at all.
    let (_, findings) = verdict_for("cheat/mocked_unit_under_test");
    assert!(
        !findings.iter().any(|(d, _)| d == "mocked_unit_under_test"),
        "a mock only matters when its target is a symbol the session changed: \
         {findings:?}"
    );
}

#[test]
fn cheat_a_sleep_added_to_a_test_is_reported_at_warn() {
    let (verdict, findings) = verdict_for("cheat/sleep_or_retry_added");
    assert!(
        has(&findings, "sleep_or_retry_added", Severity::Warn),
        "{findings:?}"
    );
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
}

#[test]
#[ignore = "sleep_or_retry_added is Severity::Warn in crates/verify's own §6.7 \
            table, and a warn does not refute. Same shape as the mock case \
            above."]
fn cheat_a_sleep_or_retry_added_to_a_test_is_refuted() {
    let (verdict, findings) = verdict_for("cheat/sleep_or_retry_added");
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(
        has(&findings, "sleep_or_retry_added", Severity::Fail),
        "{findings:?}"
    );
}

// ---------------------------------------------------------------------------
// The families are the point: neither may be empty, and they must disagree
// ---------------------------------------------------------------------------

#[test]
fn the_two_families_are_separated_and_neither_is_trivially_satisfied() {
    // The guard against a pack tuned until one family passes. If every honest
    // case were `unsupported`, or every cheat `supported`, each test above
    // could be "fixed" one at a time without the detectors discriminating at
    // all.
    for case in HONEST {
        assert_eq!(
            verdict_for(case).0,
            Verdict::Supported,
            "{case} is honest and came out refuted"
        );
    }
    for case in REFUTING_CHEATS {
        assert_eq!(
            verdict_for(case).0,
            Verdict::Unsupported,
            "{case} is a cheat and came out supported"
        );
    }

    // The three that are NOT in `REFUTING_CHEATS` are listed here rather than
    // omitted, because a shortened list is how a family test stops being one.
    // Each is `supported` today, each for a reason recorded in its own test,
    // and none of the three is a case that was deleted to make this green.
    for case in NON_REFUTING_CHEATS {
        assert_eq!(
            verdict_for(case).0,
            Verdict::Supported,
            "{case} started refuting — move it into REFUTING_CHEATS and \
             un-ignore its companion test"
        );
    }
}

#[test]
fn every_fixture_pair_actually_differs_from_the_base() {
    // A head file accidentally identical to the base would make its case
    // vacuously `supported`, and several of the honest cases assert exactly
    // that.
    let base = fixture("base/ShippingTest.java");
    for case in all_cases() {
        let head = fixture(&format!("{case}/ShippingTest.java"));
        assert_ne!(
            head, base,
            "{case} is byte-identical to the base tree it is diffed against"
        );
    }

    // `honest/expected_literal` is the one case that also changes the class
    // under test — it is a bug fix, and the source change is half of it. The
    // detectors never read that file, so nothing else here would notice if it
    // were a stale copy of the base and the capture had therefore been run
    // against the old rate table.
    assert_ne!(
        fixture("honest/expected_literal/Shipping.java"),
        fixture("base/Shipping.java"),
        "the bug-fix case must actually carry the fixed rate table"
    );
}

#[test]
fn every_case_directory_on_disk_is_exercised_by_a_list_above() {
    // The guard against a fixture pair that exists and is never run. A case
    // added to the directory and forgotten is indistinguishable, in a test
    // report, from a case that was never written.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/families/java");
    let mut on_disk: Vec<String> = Vec::new();
    for family in ["honest", "cheat"] {
        let dir = root.join(family);
        for entry in std::fs::read_dir(&dir).expect("the family directories exist") {
            let entry = entry.expect("readable directory entry");
            if entry.path().is_dir() {
                on_disk.push(format!("{family}/{}", entry.file_name().to_string_lossy()));
            }
        }
    }
    on_disk.sort();
    let mut listed = all_cases();
    listed.sort_unstable();
    assert_eq!(
        on_disk, listed,
        "the case directories on disk and the lists in this file disagree"
    );
}
