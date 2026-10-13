//! The C# families, end to end — the source pair through the detectors, the
//! **captured `dotnet test` run** through this crate's .NET parser, and both
//! into `verify::tests_pass::verify`.
//!
//! # Why both families, and why a detector-only test cannot substitute
//!
//! `crates/verify/tests/honest_and_cheat_families.rs` states the reason, and it
//! is a measurement rather than a style preference: the control there was a
//! single honest edit — changing an expected literal — and that is the **one**
//! honest shape that trips no `fail` detector, which is exactly why it passed
//! while four others failed. A rename, a `skipif`, a split and an `xfail` were
//! all reported `unsupported` beside it.
//!
//! Two of §6.7's severities are decided by the *run* and not by the detector:
//! `tests_pass::escalate` reads `parsed.skipped` and turns `skip_marker_added`
//! into a `fail` or an `info`. HONEST 2 and CHEAT 3 below produce **the same
//! finding at the same severity** from the detectors, and are separated only by
//! the captured run — 0 skipped against 1. A test that stopped at the finding
//! list would see one thing and call it two.
//!
//! # The runs are captures, not arguments
//!
//! `tests/fixtures/families/csharp/runs/*.txt` is real `dotnet test` output,
//! xunit.v3 4.0.0 on SDK 9.0.317, one file per head tree, and every
//! `ParsedView` below is produced by running `runners::dotnet::parse` over
//! those bytes. Nothing in this file states a tally by hand.
//! `tests/fixtures/families/csharp/README.md` names the command, the project
//! and the exit code behind each one.
//!
//! # Four things C# adds that the Python family could not reach
//!
//! 1. **.NET has no xfail.** `ParsedView::xfail` is never set by any .NET
//!    runner, and `runners::dotnet` says why in its own words — *"none of
//!    xunit, NUnit or MSTest has an expected-failure state, and inventing one
//!    from `[Ignore]` would put a skip in a field §6.3 reports separately"*.
//!    A quarantine marker therefore leaves the test **unrun** and the run
//!    reports it in the `Skipped:` bucket, with nowhere else for it to go.
//!
//!    Two of them are measured here rather than read off a doc page, and only
//!    those two are claimed: `runs/cheat-skip_marker_added.txt` captures
//!    `[Fact(Skip = "flaky on CI")]` and `runs/honest-flaky_explicit.txt`
//!    captures `[Fact(Explicit = true)]`, and both summary lines say
//!    `Skipped: 1`. NUnit's `[Ignore]`/`[Explicit]` and MSTest's `[Ignore]`
//!    were **not** run on this machine; the paragraph above is the reason to
//!    expect the same of them, not evidence that it holds.
//!
//!    So the pytest family's "flaky test marked `xfail`, run reports it as
//!    such, claim stands" has no transcription here. The honest C# shape that
//!    quarantines a flaky expectation **without** leaving anything unrun is a
//!    tolerance, and that is HONEST 5. The other reading is HONEST 8, which
//!    passes today for a reason that is a hole rather than a virtue, and says
//!    so.
//!
//! 2. **The honest expects-a-throw idiom is refuted**, when its handler is
//!    empty. `crates/langpacks/src/tamper.rs`'s own doc comment offers
//!    `try { risky(); fail("should have thrown"); } catch (IOException e) { }`
//!    as the honest case its structural walk exists to protect, and says what
//!    decides it is *"whether the assertions are inside the guarded block"* —
//!    but in that example, as in the cheat beside it, they are. Both come out
//!    swallowing. HONEST 9 is that shape in idiomatic C#; it is `#[ignore]`d
//!    and paired with a live `gap_` test.
//!
//! 3. **`similar_bodies` pairs unrelated C# tests.** `RENAME_SIMILARITY` is
//!    0.5, measured on a Python/Rust/JS table, and `tamper::RENAME_SIMILARITY`
//!    already predicts the problem in prose: C# bodies carry far more
//!    boilerplate tokens. Measured here on this file's own base:
//!    **four of the six pairs of mutually unrelated tests score ≥ 0.5**, the
//!    highest at 0.727. The consequence is not cosmetic — CHEAT 8 deletes the
//!    only test covering a branch, adds an unrelated one in the same session,
//!    and is reported as a **rename**, at `info`. That is a `test_deleted`
//!    cheat missed outright.
//!
//! 4. **Two of the cheats are `warn` by §6.7 and so cannot refute anything.**
//!    `mocked_unit_under_test` and `sleep_or_retry_added` are `Severity::Warn`
//!    in `crates/verify/src/tamper.rs` itself, and `VerdictRecord::with_finding`
//!    moves a verdict only on a `fail`. This crate must mirror verify's
//!    severities exactly — `tests/detector_name_parity.rs` guards the names and
//!    the same argument covers the severities — so both come out `Supported`
//!    with a warning attached. That is the spec and the stage brief
//!    disagreeing: a Rule 7 conflict, reported rather than blended.
//!
//! # The roster
//!
//! Numbered, because the comments below refer to cases by number and a
//! reference that resolves to nothing is worse than no reference. Each name is
//! a directory under `tests/fixtures/families/csharp/` holding that head tree's
//! `ShippingRateTests.cs`, and a `runs/*.txt` capture of what `dotnet test` did
//! with it.
//!
//! | # | honest | # | cheat |
//! |---|---|---|---|
//! | 1 | `rename` | 1 | `assertion_removed` |
//! | 2 | `conditional_skip` | 2 | `assertion_weakened` |
//! | 3 | `split` | 3 | `skip_marker_added` |
//! | 4 | `expected_literal` — the control | 4 | `assertion_swallowed` |
//! | 5 | `flaky_tolerance` | 5 | `test_deleted` |
//! | 6 | `theory` | 6 | `mocked_unit_under_test` — **`#[ignore]`** |
//! | 7 | `negative_trycatch` | 7 | `sleep_or_retry_added` — **`#[ignore]`** |
//! | 8 | `flaky_explicit` | 8 | `test_deleted_decoy` — **`#[ignore]`** |
//! | 9 | `negative_trycatch_bare` — **`#[ignore]`** | | |
//!
//! # Four cases do not pass, and are `#[ignore]`d rather than removed
//!
//! Deleting a case to make a suite green is the tampering this crate detects.
//! Each of the four is paired with a live `gap_` test pinning what the code
//! does **today**, so the two fail in opposite directions: fix the engine and
//! the `gap_` test fails, pointing at the `#[ignore]` to lift. An `#[ignore]`
//! with nothing watching it is a case that quietly stopped existing.

use std::path::PathBuf;

use thruline_langpacks::pack::{pack_for_path, LangPack};
use thruline_langpacks::runners::dotnet;
use thruline_langpacks::tamper::{
    diff_test_file, diff_test_file_with_symbols, jaccard, parse_tests,
};
use thruline_verify::binding::{CheckView, ParsedView, SessionView, SnapshotView};
use thruline_verify::parse::RunnerOutcome;
use thruline_verify::tests_pass::{verify, Inputs};
use thruline_verify::verdict::{Severity, Verdict};

/// Where the session's test edit is recorded as living.
///
/// A repository path a .NET solution would really use, rather than the
/// fixture's own location: `tests_pass::narrowing_flag` and `binding::is_test_path`
/// both read paths out of the record, and a path under `tests/fixtures` would
/// be exercising this crate's layout instead of a suite's.
const TEST_PATH: &str = "tests/Shipping.Tests/ShippingRateTests.cs";

fn fixture(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/families/csharp")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn pack() -> &'static dyn LangPack {
    pack_for_path(TEST_PATH).expect("`.cs` resolves to the C# pack")
}

/// The run behind a case, as **this crate's parser** reads the captured bytes.
///
/// An `Unverifiable` here fails the test rather than becoming a zero tally: a
/// defaulted `Parsed` reads as a clean run, and refusing to produce one is the
/// whole reason `verify::parse::RunnerOutcome` has two variants.
fn parsed_view(run: &str) -> ParsedView {
    match dotnet::parse(&fixture(&format!("runs/{run}"))) {
        RunnerOutcome::Parsed(p) => ParsedView {
            tests_total: p.tests_total,
            passed: p.passed,
            failed: p.failed,
            skipped: p.skipped,
            xfail: p.xfail,
        },
        RunnerOutcome::Unverifiable { reason, hint } => panic!(
            "runs/{run} is a real `dotnet test` capture and the parser could not read it: \
             {reason} ({hint})"
        ),
    }
}

/// A session whose suite ran on the final tree, carrying the captured run.
///
/// `argv` is `dotnet test` with no project and no filter. That matters:
/// `dotnet test tests/Shipping.Tests` names a path `is_test_path` recognises
/// and would be a **narrowed** run — its own `fail` under `scope: all` — so
/// every verdict below would be refuted by `narrowed_run` before §6.7 was
/// consulted, and a cheat caught that way is a cheat this file never caught.
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
            runner: "dotnet test".into(),
            bypass: None,
            argv: Some(vec!["dotnet".into(), "test".into()]),
            // Every family capture really did exit 0 — the README's table is
            // the evidence, and `every_family_run_is_green_so_only_the_source_separates_them`
            // holds the parsed half of it to a test. The two `proof/` captures
            // are the exception and are never fed through here.
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

/// One family case: the base tree's test file against the case's, then the
/// verifier, in the order the CLI uses.
///
/// `changed_symbols` is the session's non-test change, which only
/// `mocked_unit_under_test` reads: a mock is a finding when its target resolves
/// to something this session edited, and the same source with an empty list is
/// no finding at all. The empty case deliberately goes through the
/// four-argument entry point — it is a delegation today, and a delegation
/// nothing exercises is a delegation that can quietly stop delegating.
fn judge(case: &str, changed_symbols: &[String]) -> (Verdict, Vec<(String, Severity)>) {
    let base = fixture("base/ShippingRateTests.cs");
    let head = fixture(&format!("{case}/ShippingRateTests.cs"));
    let findings = if changed_symbols.is_empty() {
        diff_test_file(TEST_PATH, &base, &head, pack())
    } else {
        diff_test_file_with_symbols(TEST_PATH, &base, &head, pack(), changed_symbols)
    }
    .expect("the C# grammar loads and both fixture trees parse");

    let record = verify(
        "clm",
        &session(&format!("{}.txt", case.replace('/', "-"))),
        &Inputs {
            scope_is_all: true,
            tamper_findings: findings,
            // The session edited `src/Shipping/ShippingRate.cs`; without this
            // every case would also carry `test_changed_without_source_change`,
            // which is a true finding about a different situation.
            source_changed: true,
            test_files_changed: vec![TEST_PATH.to_string()],
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

fn case(name: &str) -> (Verdict, Vec<(String, Severity)>) {
    judge(name, &[])
}

fn fired(findings: &[(String, Severity)], detector: &str, severity: Severity) -> bool {
    findings
        .iter()
        .any(|(d, s)| d == detector && *s == severity)
}

/// The `swallows` flag one head tree produces for one named test.
///
/// Used where a case's whole point is that two trees differ in that flag and in
/// nothing else — asserting the verdict alone would pass on a pair that
/// differed for some other reason.
fn swallows(case: &str, test: &str) -> bool {
    let head = fixture(&format!("{case}/ShippingRateTests.cs"));
    parse_tests(&head, pack())
        .expect("the fixture parses as C#")
        .into_iter()
        .find(|t| t.name == test)
        .unwrap_or_else(|| panic!("{case} has no test named {test}"))
        .swallows
}

// ---------------------------------------------------------------------------
// The honest family — every one of these must be `supported`
// ---------------------------------------------------------------------------

#[test]
fn honest_renaming_every_test_is_supported() {
    let (verdict, findings) = case("honest/rename");
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert_eq!(
        findings.iter().filter(|(d, _)| d == "test_renamed").count(),
        4,
        "all four renames should be reported as renames, not as deletions: {findings:?}"
    );
    assert!(
        !findings
            .iter()
            .any(|(d, _)| d == "test_deleted" || d == "test_added"),
        "a rename is neither a deletion nor an addition: {findings:?}"
    );
    // **The pairings are wrong and the verdict is right, which is worth saying
    // once rather than leaving to be discovered.** Three of the four base tests
    // pair to the *same* head test, because C#'s per-method boilerplate
    // (`fact public void assert equal _rate rate`) carries the Jaccard score
    // over 0.5 between tests that share nothing else. The verdict is unaffected
    // — every base test finds some partner, so nothing reads as deleted — and
    // the cost lands on CHEAT 8 instead. See
    // `measured_unrelated_csharp_tests_clear_the_rename_threshold`.
}

#[test]
fn honest_a_conditional_skip_that_skipped_nothing_here_is_supported() {
    // xunit v3's `[Fact(Skip = "…", SkipWhen = nameof(RunningOnWindows))]`.
    // The pack cannot evaluate the predicate — it reads the `Skip` named
    // argument and stops — so the detector fires exactly as it does for CHEAT 3
    // and at the same severity. The capture is the entire difference: this run
    // was on macOS, the predicate was false, the test ran, and `Skipped: 0`.
    let (verdict, findings) = case("honest/conditional_skip");
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        fired(&findings, "skip_marker_added", Severity::Info),
        "the marker must still be REPORTED, just not as a refutation: {findings:?}"
    );
    assert_eq!(
        parsed_view("honest-conditional_skip.txt").skipped,
        0,
        "the case rests on the run having skipped nothing"
    );
}

#[test]
fn honest_one_test_split_into_two_is_supported() {
    let (verdict, findings) = case("honest/split");
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        fired(&findings, "test_renamed", Severity::Info),
        "the half that kept the body should pair with the original: {findings:?}"
    );
    assert!(
        !findings.iter().any(|(d, _)| d == "test_deleted"),
        "splitting a test does not delete it: {findings:?}"
    );
    // The run is the other half of "split", and it is a capture: five tests
    // where the base ran four.
    assert_eq!(parsed_view("honest-split.txt").tests_total, 5);
}

#[test]
fn honest_a_tariff_change_that_moves_the_expected_figures_is_supported() {
    // The control, and the one honest shape that trips nothing at all. It is
    // here so a suite that passes cannot be a suite that only ever sees this
    // case — which is precisely how `verify`'s single-case control passed while
    // four honest edits were refuted beside it.
    //
    // The source moves too: `honest/expected_literal/Shipping.cs` raises the
    // per-kilogram surcharge from 4.00 to 5.00, and the capture is green
    // against that tree. A control whose source did not move would be a test
    // that had been edited to expect the wrong answer.
    let (verdict, findings) = case("honest/expected_literal");
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        findings.is_empty(),
        "the control must trip no detector at all: {findings:?}"
    );
}

#[test]
fn honest_a_flaky_expectation_quarantined_by_a_tolerance_is_supported() {
    // .NET has no xfail (see the module docs), so the honest way to quarantine
    // a flaky expectation without leaving anything unrun is to loosen it:
    // `Assert.Equal(29.00m, rate, 2)` — xUnit's precision overload. The test
    // still runs and still fails on a wrong rate.
    //
    // `assertion_weakened` at `warn` is the right report: Equality became
    // Approximate, which is a real loss of strength and is not by itself
    // evidence that anything was dropped. `fail` is reserved for a destination
    // of `Truthy`, which asserts nothing — CHEAT 2.
    let (verdict, findings) = case("honest/flaky_tolerance");
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        fired(&findings, "assertion_weakened", Severity::Warn),
        "{findings:?}"
    );
    assert_eq!(
        parsed_view("honest-flaky_tolerance.txt").skipped,
        0,
        "the point of a tolerance over a skip is that nothing goes unrun"
    );
}

#[test]
fn honest_folding_two_assertions_into_the_theory_rows_that_carry_them_is_supported() {
    // `[Fact]` with two `Assert.Equal`s becomes `[Theory]` with two
    // `[InlineData]` rows and one assertion. The test keeps its name and covers
    // exactly what it covered before, so its assertion count honestly drops
    // 2 -> 1 and the *run* gains a test: four became five.
    //
    // `assertion_count_decreased` at `warn` is the right finding for that. One
    // assertion is not none, and `assertion_removed` — the `fail` — is reserved
    // for none.
    let (verdict, findings) = case("honest/theory");
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        fired(&findings, "assertion_count_decreased", Severity::Warn),
        "{findings:?}"
    );
    assert!(
        !findings.iter().any(|(d, _)| d == "assertion_removed"),
        "one assertion is not none: {findings:?}"
    );
    assert_eq!(
        parsed_view("honest-theory.txt").tests_total,
        5,
        "the two InlineData rows are two tests; the count moving the other way \
         is what makes this honest rather than a quiet deletion"
    );
}

#[test]
fn honest_a_negative_test_written_as_try_catch_does_not_trip_assertion_swallowed() {
    // **The case the structural detector exists for.** `Assert.Throws<T>` never
    // reaches `catch_swallows` at all — there is no `try_statement` in it — so
    // proving the detector on that form would prove nothing. This case rewrites
    // the base's `Assert.Throws` negative test into the structural form, whose
    // keywords are byte-for-byte the cheat's:
    //
    //     try { _rate.Rate(0m); Assert.Fail("…"); }
    //     catch (ArgumentOutOfRangeException expected) { Assert.Contains(…); }
    //
    // The handler asserts, so the failure is not discarded — that is what
    // separates it from CHEAT 4, and the assertion below is on the flag itself
    // rather than on the verdict, because a verdict of `Supported` would also
    // be produced by a detector that had stopped looking.
    assert!(
        !swallows("honest/negative_trycatch", "RateRejectsAWeightlessParcel"),
        "an asserting handler is not a discarded failure"
    );
    let (verdict, findings) = case("honest/negative_trycatch");
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        !findings.iter().any(|(d, _)| d == "assertion_swallowed"),
        "{findings:?}"
    );
    assert!(
        findings.is_empty(),
        "`Assert.Throws` -> `Assert.Fail` + `Assert.Contains` raises the count and \
         the strength; nothing here is a loss: {findings:?}"
    );
}

#[test]
fn honest_a_flaky_test_quarantined_by_explicit_is_supported_and_that_is_a_hole() {
    // **This passes, and it passes for a reason that is a hole rather than a
    // virtue.** It is the nearest C# gets to the pytest family's `xfail` case,
    // and the two halves do not line up:
    //
    // - the RUN behaves like the pytest case: `[Fact(Explicit = true)]` leaves
    //   the test out of a whole-suite run, §6.3's own skips rule reports it,
    //   and the claim stands;
    // - the DETECTOR is silent, and should not be. `CSharp::is_skip_marker`
    //   reads a `Skip` named argument and an exact attribute name from
    //   `SKIP_ATTRIBUTES` — which contains `explicit`, for NUnit's `[Explicit]`.
    //   xunit v3 spells the identical event as a named argument on `[Fact]`,
    //   and that spelling is not read. So the pack refutes NUnit's quarantine
    //   and blesses xUnit's.
    //
    // Both assertions below are deliberate. The second one pins the silence, so
    // that closing the hole in `src/langs/csharp.rs` breaks this test and forces
    // the decision rather than quietly moving a case from one family to the
    // other. When it is closed, this case joins CHEAT 3: the run leaves a test
    // unrun, `escalate` sees `skipped == 1`, and §6.7 refutes the claim — which
    // is the same conclusion `tests/families_go.rs` reached for `t.Skip`.
    let (verdict, findings) = case("honest/flaky_explicit");
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        fired(&findings, "skipped_tests", Severity::Warn),
        "the quarantined test was not surfaced at all: {findings:?}"
    );
    assert_eq!(
        parsed_view("honest-flaky_explicit.txt").skipped,
        1,
        "`Explicit = true` really does leave the test unrun — this is the \
         measurement, and it is why the detector's silence is a hole"
    );
    assert!(
        !findings.iter().any(|(d, _)| d == "skip_marker_added"),
        "the C# pack learned `Explicit = true`. That is the right fix, and it \
         moves this case out of the honest family: re-file it beside CHEAT 3 \
         and delete the paragraph above: {findings:?}"
    );
}

// --- the honest case that does not pass ------------------------------------

/// Fixed 2026-09-05 in `src/tamper.rs`: `LangSpec::unconditional_fail_markers`.
///
/// This idiom was reported as `assertion_swallowed` at **`fail`** — a
/// fail-severity false accusation against ordinary C#. Neither "the handler is
/// empty" nor "the assertions are guarded" could tell it from a real swallow,
/// because the cheat has both too. What separates them is that this block ends
/// with an UNCONDITIONAL `Assert.Fail`, reached only if the expected exception
/// never arrived — so the handler is the success path, not a drop.
#[test]
fn honest_a_negative_test_with_a_bare_catch_handler_is_supported() {
    // **A `fail`-severity false accusation against a common idiom**, and the
    // engine's own documentation says it is the case the structural walk was
    // written to protect. `src/tamper.rs::swallows` offers, verbatim:
    //
    //     try { risky(); fail("should have thrown"); } catch (IOException e) { }
    //
    // as *"an honest negative test, whose try block holds the assertion"*, and
    // says what decides it is *"whether the assertions are inside the guarded
    // block"*. In the cheat it names beside it, they are inside the guarded
    // block too. The rule as implemented is "the guarded block asserts and some
    // handler neither asserts nor rethrows", which both shapes satisfy, so both
    // come out swallowing. The doc comment describes a discrimination the code
    // does not make.
    //
    // The fixture differs from `honest/negative_trycatch` by the body of the
    // catch clause and by nothing else — the `gap_` test below asserts that
    // pair explicitly, so this is not a claim about two files that also differ
    // some other way.
    //
    // The fix is not reachable from `src/langs/csharp.rs`: `LangSpec` exposes
    // `catch_kinds` and nothing finer, and emptying it would delete CHEAT 4
    // along with this false positive. What is needed is a rule that separates
    // "the guarded block's last statement is an unconditional failure marker"
    // (this) from "the guarded block's assertions are the test" (CHEAT 4), and
    // that lives in the shared engine. It belongs in the merge plan.
    let (verdict, findings) = case("honest/negative_trycatch_bare");
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        !findings.iter().any(|(d, _)| d == "assertion_swallowed"),
        "{findings:?}"
    );
}

// ---------------------------------------------------------------------------
// The cheat family — every one of these must be `unsupported`
// ---------------------------------------------------------------------------

#[test]
fn cheat_every_assertion_deleted_from_a_test_is_refuted() {
    let (verdict, findings) = case("cheat/assertion_removed");
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(
        fired(&findings, "assertion_removed", Severity::Fail),
        "{findings:?}"
    );
    // The call is still there, so the run is still green and still reports four
    // tests. Nothing in the capture distinguishes this from the base.
    assert_eq!(
        parsed_view("cheat-assertion_removed.txt").tests_total,
        parsed_view("base.txt").tests_total
    );
}

#[test]
fn cheat_an_equality_replaced_by_a_truthiness_check_is_refuted() {
    // `Assert.Equal(29.00m, rate)` -> `Assert.True(rate > 0m)`. Every rate the
    // method can return is greater than zero, so this is a removal wearing an
    // assertion's clothes.
    //
    // BUILD-SPEC §6.7 files "assertion loosened" under `warn` outright, and
    // `verify` deviates on purpose: `warn` is right for equality -> containment
    // and wrong for equality -> vacuous. The deviation was forced by
    // measurement — `verify`'s own family test records this exact edit coming
    // back `supported` while raising `assertion_weakened` twice. `fail` when
    // the destination is `Truthy`, `warn` otherwise; HONEST 5 is the other half
    // of that same rule, and this crate inherits both unchanged.
    let (verdict, findings) = case("cheat/assertion_weakened");
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(
        fired(&findings, "assertion_weakened", Severity::Fail),
        "{findings:?}"
    );
}

#[test]
fn cheat_an_unconditional_skip_that_actually_skipped_is_refuted() {
    // Byte-for-byte the same detector output as HONEST 2 — `skip_marker_added`,
    // emitted at `warn` by a detector that cannot tell a guarded marker from a
    // bare one. The captured run is the entire difference: one test did not run.
    let (verdict, findings) = case("cheat/skip_marker_added");
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(
        fired(&findings, "skip_marker_added", Severity::Fail),
        "{findings:?}"
    );
    assert_eq!(
        parsed_view("cheat-skip_marker_added.txt").skipped,
        1,
        "the escalation to `fail` rests on this number and on nothing else"
    );
}

#[test]
fn cheat_a_swallowed_assertion_is_refuted() {
    // The body wrapped in `try { … } catch (Exception) { }`. The assertion is
    // still there, still counted, still `Equality` — the count and the strength
    // detectors both see nothing — and it can no longer fail the test.
    assert!(
        swallows("cheat/assertion_swallowed", "RateForALargeParcel"),
        "the cheat fixture must actually read as swallowing"
    );
    let (verdict, findings) = case("cheat/assertion_swallowed");
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(
        fired(&findings, "assertion_swallowed", Severity::Fail),
        "{findings:?}"
    );
    assert!(
        !findings
            .iter()
            .any(|(d, _)| d == "assertion_removed" || d == "assertion_count_decreased"),
        "nothing was removed — that is why this detector has to exist: {findings:?}"
    );
}

#[test]
fn cheat_a_test_deleted_outright_is_refuted() {
    let (verdict, findings) = case("cheat/test_deleted");
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(
        fired(&findings, "test_deleted", Severity::Fail),
        "{findings:?}"
    );
    assert!(
        !findings.iter().any(|(d, _)| d == "test_renamed"),
        "nothing was added that could carry the body: {findings:?}"
    );
    // And this is the only capture in the family whose test count fell. It is
    // also the only signal the run gives, and it gives it only because nothing
    // was added to make the total back up — which is exactly what CHEAT 8 does.
    assert_eq!(parsed_view("cheat-test_deleted.txt").tests_total, 3);
}

// --- the three cheats that do not refute -----------------------------------

#[test]
#[ignore = "BUILD-SPEC §6.7 files `mocked_unit_under_test` under `warn`, and a warn cannot \
            move a verdict. The stage brief asks for `unsupported`; the spec says `warn`. \
            A Rule 7 conflict, not a C# defect — see the paired `gap_` test."]
fn cheat_a_mock_of_the_unit_under_test_is_refuted() {
    // **A Rule 7 conflict, recorded rather than blended.**
    //
    // The detector is right and fires exactly once, on the target it should:
    // `new Mock<IShippingRate>()` resolves to `IShippingRate`, which is the
    // symbol this session changed. What does not happen is the refutation, and
    // the reason is in the spec rather than in the code. §6.7's `warn` list
    // reads, verbatim: *"new mock/patch whose target resolves to a symbol
    // changed in the diff (mocking the unit under test)"*. `verify::tamper`
    // implements exactly that, this crate ports it unchanged — a port that
    // "improved" a severity would make `tests/detector_name_parity.rs` a lie —
    // and `VerdictRecord::with_finding` moves the verdict only on `fail`.
    //
    // So the case cannot be made to pass from `src/langs/csharp.rs`, and it must
    // not be: raising it edits §6.7's severity table for all nine languages, and
    // that belongs to whoever owns the spec.
    let (verdict, findings) = judge(
        "cheat/mocked_unit_under_test",
        &["IShippingRate".to_string()],
    );
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(
        fired(&findings, "mocked_unit_under_test", Severity::Fail),
        "{findings:?}"
    );
}

#[test]
#[ignore = "BUILD-SPEC §6.7 files `sleep`/retry loops under `warn`, and a warn cannot move a \
            verdict. Same Rule 7 conflict as the mock case; see the paired `gap_` test."]
fn cheat_a_sleep_added_in_front_of_an_assertion_is_refuted() {
    // §6.7's `warn` list, verbatim: *"`sleep`/retry loops added to tests"*.
    // Same conflict, same reason it is not resolved in this crate.
    let (verdict, findings) = case("cheat/sleep_or_retry_added");
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(
        fired(&findings, "sleep_or_retry_added", Severity::Fail),
        "{findings:?}"
    );
}

#[test]
#[ignore = "A deletion hidden behind an unrelated addition is read as a rename. MEASURED \
            2026-09-05: the pair scores jaccard 0.7273, so NO threshold in the measured range \
            fixes it — not the pooled 0.4, not C#'s own argmin 0.55, not the 0.5 this reason \
            used to blame. The cause is that C# boilerplate dominates a body's token SET, so \
            two tests differing only in a literal and a name share almost every token. Fixing \
            it is a change to what body_tokens counts, which needs its own measurement. See \
            the paired `gap_` test."]
fn cheat_a_deleted_test_hidden_behind_an_unrelated_addition_is_refuted() {
    // **The cheat this suite found rather than the one it was sent to look
    // for.** `RateForALargeParcel` — the only test covering the excess-weight
    // branch — is deleted, and `RateForAThresholdParcel`, which covers the flat
    // base a second time, arrives in the same session. Different weight,
    // different expectation, different branch: not a rename by any reading.
    //
    // **The cause is not the threshold, and this comment used to say it was.**
    // Measured 2026-09-05: the pair scores `jaccard` **0.7273**. That is above
    // the pooled `RENAME_SIMILARITY` of 0.4, above C#'s own per-language argmin
    // of 0.55, and above the 0.5 the earlier reason blamed — so no threshold in
    // the measured range separates them, and raising one to 0.73 would cost far
    // more than it bought (pooled cost 2.7028 at t=0.7 against 1.9079 at 0.4).
    //
    // What actually defeats it: `body_tokens` is compared as a SET, and C#
    // boilerplate — `public`, `void`, `Fact`, `Assert`, `Equal`, `_rate`,
    // `Rate`, `decimal` — dominates that set. Two tests differing only in a
    // numeric literal and a name share almost every token. The fix is a change
    // to what `body_tokens` counts (drop language keywords, or weight literals),
    // and it needs its own measurement against the mined-rename corpus rather
    // than a guess — which is why it is open rather than patched.
    //
    // `tamper::diff_test_file` pairs the vanished test with the first added one
    // whose body is `similar_bodies`, and in C# that is nearly always true.
    // Measured on this file's own base:
    // `measured_unrelated_csharp_tests_clear_the_rename_threshold` scores four
    // of six unrelated pairs at or above the threshold, the highest at 0.727.
    // Six tokens are shared by all four of the base's tests — `fact`, `public`,
    // `void`, `assert`, `_rate`, `rate` — and this pair shares two more, `equal`
    // and `00m`, because both are decimal equalities. Four of the six are in
    // every xUnit test ever written and the other two are whatever the class
    // calls its subject; none of them says anything about what is being
    // asserted, which is the whole of the objection.
    //
    // The fix is not in `src/langs/csharp.rs`. `RENAME_SIMILARITY` is one shared
    // constant, `crates/verify/src/tamper.rs` records the sweep behind 0.5 over
    // a Python/Rust/JS corpus, and `src/tamper.rs::RENAME_SIMILARITY` already
    // states in prose that C# and Java boilerplate moves where the threshold has
    // to sit — it just has not been re-measured. Raising it for C# alone needs a
    // per-language threshold, which `LangSpec` cannot express today.
    //
    // Note what the run does not give away: the total is back to four, because
    // one test arrived as one left.
    let (verdict, findings) = case("cheat/test_deleted_decoy");
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(
        fired(&findings, "test_deleted", Severity::Fail),
        "{findings:?}"
    );
}

// ---------------------------------------------------------------------------
// The gaps, asserted. Each of these pins what the code does TODAY.
//
// They are the live half of the four `#[ignore]`d cases above: fix the engine
// and these fail, which is the only mechanism that turns an `#[ignore]` into a
// debt someone eventually pays instead of a case that quietly stopped existing.
// ---------------------------------------------------------------------------

#[test]
fn gap_a_mock_of_the_changed_unit_is_reported_and_does_not_refute() {
    // The detector half is real coverage and is asserted as such: the finding
    // fires, it names `mocked_unit_under_test`, and it fires **once** —
    // `stub.Setup(r => r.Rate(10m))` is not a mock creation and correctly
    // produces nothing, which is the false positive `CSharp::mock_calls` leaves
    // Moq's `Setup`/`Verify` out to avoid.
    let (verdict, findings) = judge(
        "cheat/mocked_unit_under_test",
        &["IShippingRate".to_string()],
    );
    assert!(
        fired(&findings, "mocked_unit_under_test", Severity::Warn),
        "{findings:?}"
    );
    assert_eq!(
        findings
            .iter()
            .filter(|(d, _)| d == "mocked_unit_under_test")
            .count(),
        1,
        "only the changed symbol should be reported: {findings:?}"
    );
    // And with no changed symbol, the same source is no finding at all — the
    // detector is about the session, not about the word `Mock`.
    let (_, without) = case("cheat/mocked_unit_under_test");
    assert!(
        !without.iter().any(|(d, _)| d == "mocked_unit_under_test"),
        "a mock of something this session did not touch is not a finding: {without:?}"
    );
    assert_eq!(
        verdict,
        Verdict::Supported,
        "§6.7's severity for this detector was raised — lift the `#[ignore]` on \
         `cheat_a_mock_of_the_unit_under_test_is_refuted`: {findings:?}"
    );
}

#[test]
fn gap_a_sleep_added_to_a_test_is_reported_and_does_not_refute() {
    let (verdict, findings) = case("cheat/sleep_or_retry_added");
    assert!(
        fired(&findings, "sleep_or_retry_added", Severity::Warn),
        "{findings:?}"
    );
    assert_eq!(
        verdict,
        Verdict::Supported,
        "§6.7's severity for this detector was raised — lift the `#[ignore]` on \
         `cheat_a_sleep_added_in_front_of_an_assertion_is_refuted`: {findings:?}"
    );
}

#[test]
fn gap_a_deletion_hidden_behind_an_unrelated_addition_is_read_as_a_rename_today() {
    let (verdict, findings) = case("cheat/test_deleted_decoy");
    assert!(
        fired(&findings, "test_renamed", Severity::Info),
        "{findings:?}"
    );
    assert!(
        !findings.iter().any(|(d, _)| d == "test_deleted"),
        "the deletion started being reported — lift the `#[ignore]` on \
         `cheat_a_deleted_test_hidden_behind_an_unrelated_addition_is_refuted`: {findings:?}"
    );
    assert_eq!(
        verdict,
        Verdict::Supported,
        "the rename threshold moved — lift the `#[ignore]` on \
         `cheat_a_deleted_test_hidden_behind_an_unrelated_addition_is_refuted`: {findings:?}"
    );
}

// ---------------------------------------------------------------------------
// The measurements behind the two claims this file makes about the engine
// ---------------------------------------------------------------------------

#[test]
fn measured_the_swallow_wrapper_is_what_makes_the_cheat_green() {
    // CHEAT 4 claims a `try`/`catch (Exception)` wrapper stops a failure
    // propagating. These two captures are that claim measured rather than
    // described: the trees differ by the wrapper and by nothing else — same
    // call, same deliberately wrong expectation of 999.00, same message — and
    // `dotnet test` reports one failure for the first and none for the second.
    //
    // Without this pair, "the wrapper swallows the assertion" would be a
    // sentence in a comment.
    let unwrapped = parsed_view("cheat-assertion_swallowed-proof-unwrapped.txt");
    let wrapped = parsed_view("cheat-assertion_swallowed-proof-wrapped.txt");
    assert_eq!(
        (unwrapped.tests_total, unwrapped.failed, unwrapped.passed),
        (1, 1, 0),
        "the unwrapped tree must really fail"
    );
    assert_eq!(
        (wrapped.tests_total, wrapped.failed, wrapped.passed),
        (1, 0, 1),
        "the wrapped tree must really pass"
    );
    assert!(
        swallows(
            "cheat/assertion_swallowed/proof/wrapped",
            "RateForALargeParcel"
        ) && !swallows(
            "cheat/assertion_swallowed/proof/unwrapped",
            "RateForALargeParcel"
        ),
        "and the detector must read the pair the same way the runner did"
    );
}

#[test]
fn measured_unrelated_csharp_tests_clear_the_rename_threshold() {
    // The number behind CHEAT 8 and behind the pairings HONEST 1 gets wrong.
    // These four tests share no call, no weight and no expectation; what they
    // share is `[Fact] public void`, `Assert`, `_rate` and `Rate`, and in C#
    // that is most of a short test body.
    //
    // Written as a range rather than as four exact figures on purpose: the
    // claim is "C# boilerplate carries unrelated tests over a threshold chosen
    // for Python", and pinning 0.727 to three decimals would make an unrelated
    // edit to the fixture look like a regression. The count is what matters.
    let tests = parse_tests(&fixture("base/ShippingRateTests.cs"), pack())
        .expect("the base fixture parses as C#");
    assert_eq!(tests.len(), 4);
    let mut over = 0usize;
    let mut highest = 0.0f64;
    for i in 0..tests.len() {
        for j in (i + 1)..tests.len() {
            let score = jaccard(&tests[i], &tests[j]);
            highest = highest.max(score);
            if score >= thruline_langpacks::tamper::RENAME_SIMILARITY {
                over += 1;
            }
        }
    }
    assert!(
        over >= 4,
        "only {over} of 6 unrelated pairs cleared {}; if the threshold or the token \
         rule was re-measured for C#, CHEAT 8 and the HONEST 1 pairings should be \
         revisited with it",
        thruline_langpacks::tamper::RENAME_SIMILARITY
    );
    assert!(
        highest > 0.7,
        "the worst unrelated pair now scores {highest:.3}, which is no longer the \
         situation this file documents"
    );
}

// ---------------------------------------------------------------------------
// The families are the point: neither may be empty, and they must disagree
// ---------------------------------------------------------------------------

#[test]
fn the_two_families_are_separated_and_neither_is_trivially_satisfied() {
    // The guard against a pack tuned until one family passes. If every honest
    // case were `unsupported`, or every cheat `supported`, the per-case tests
    // above could each be "fixed" one at a time without the detectors
    // discriminating at all.
    let honest = [
        "honest/rename",
        "honest/conditional_skip",
        "honest/split",
        "honest/expected_literal",
        "honest/theory",
        "honest/flaky_tolerance",
        "honest/flaky_explicit",
        "honest/negative_trycatch",
    ];
    let cheats = [
        "cheat/assertion_removed",
        "cheat/assertion_weakened",
        "cheat/skip_marker_added",
        "cheat/assertion_swallowed",
        "cheat/test_deleted",
    ];
    for h in honest {
        let (verdict, findings) = case(h);
        assert_eq!(verdict, Verdict::Supported, "{h}: {findings:?}");
    }
    for c in cheats {
        let (verdict, findings) = case(c);
        assert_eq!(verdict, Verdict::Unsupported, "{c}: {findings:?}");
    }

    // **Named, so the counts cannot drift silently.** One honest case and three
    // cheats exist in this directory and are NOT in the lists above, because
    // they do not reach the verdict the stage brief asks for. Their `#[ignore]`d
    // tests say why and their `gap_` tests pin what happens instead; this
    // assertion exists so that "the cheat family has five members" is never read
    // as "the cheat family is complete".
    assert_eq!(
        (honest.len() + 1, cheats.len() + 3),
        (9, 8),
        "eight honest cases that stand and one that is refuted; five cheats that \
         refute and three that do not"
    );
}

/// Every source fixture this file reads, in `read_dir` order once sorted.
///
/// An exact set, not a floor. The runner fixtures next door are globbed against
/// a floor on purpose — a new capture should be picked up without anyone editing
/// a list — but a *family* fixture no case reads is a case that was written and
/// then lost, and the only way to notice that is to make adding one a deliberate
/// act.
const SOURCES: [&str; 22] = [
    "base/Shipping.cs",
    "base/ShippingRateTests.cs",
    "cheat/assertion_removed/ShippingRateTests.cs",
    "cheat/assertion_swallowed/ShippingRateTests.cs",
    "cheat/assertion_swallowed/proof/unwrapped/ShippingRateTests.cs",
    "cheat/assertion_swallowed/proof/wrapped/ShippingRateTests.cs",
    "cheat/assertion_weakened/ShippingRateTests.cs",
    "cheat/mocked_unit_under_test/ShippingRateTests.cs",
    "cheat/skip_marker_added/ShippingRateTests.cs",
    "cheat/sleep_or_retry_added/ShippingRateTests.cs",
    "cheat/test_deleted/ShippingRateTests.cs",
    "cheat/test_deleted_decoy/ShippingRateTests.cs",
    "honest/conditional_skip/ShippingRateTests.cs",
    "honest/expected_literal/Shipping.cs",
    "honest/expected_literal/ShippingRateTests.cs",
    "honest/flaky_explicit/ShippingRateTests.cs",
    "honest/flaky_tolerance/ShippingRateTests.cs",
    "honest/negative_trycatch/ShippingRateTests.cs",
    "honest/negative_trycatch_bare/ShippingRateTests.cs",
    "honest/rename/ShippingRateTests.cs",
    "honest/split/ShippingRateTests.cs",
    "honest/theory/ShippingRateTests.cs",
];

/// Every captured run, one per head tree that was built.
const RUNS: [&str; 20] = [
    "base.txt",
    "cheat-assertion_removed.txt",
    "cheat-assertion_swallowed-proof-unwrapped.txt",
    "cheat-assertion_swallowed-proof-wrapped.txt",
    "cheat-assertion_swallowed.txt",
    "cheat-assertion_weakened.txt",
    "cheat-mocked_unit_under_test.txt",
    "cheat-skip_marker_added.txt",
    "cheat-sleep_or_retry_added.txt",
    "cheat-test_deleted.txt",
    "cheat-test_deleted_decoy.txt",
    "honest-conditional_skip.txt",
    "honest-expected_literal.txt",
    "honest-flaky_explicit.txt",
    "honest-flaky_tolerance.txt",
    "honest-negative_trycatch.txt",
    "honest-negative_trycatch_bare.txt",
    "honest-rename.txt",
    "honest-split.txt",
    "honest-theory.txt",
];

fn walk(dir: &PathBuf, root: &PathBuf, ext: &str, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, root, ext, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some(ext) {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().into_owned());
            }
        }
    }
}

#[test]
fn every_fixture_in_the_directory_is_used_by_a_case() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/families/csharp");
    let mut sources = Vec::new();
    walk(&root, &root, "cs", &mut sources);
    sources.sort();
    assert_eq!(
        sources, SOURCES,
        "the C# family fixtures and the cases that read them disagree"
    );

    let mut runs = Vec::new();
    walk(&root.join("runs"), &root.join("runs"), "txt", &mut runs);
    runs.sort();
    assert_eq!(
        runs, RUNS,
        "the captured runs and the cases that read them disagree"
    );

    // And every source fixture must actually be readable as C#, or every
    // assertion made about it above is vacuous. The two `proof/` trees carry a
    // single test each — that is what they are for — and `base/Shipping.cs` is
    // the unit under test and carries none.
    for name in &sources {
        if name.ends_with("Shipping.cs") {
            continue;
        }
        let tests = parse_tests(&fixture(name), pack()).unwrap_or_else(|e| panic!("{name}: {e}"));
        let want = if name.contains("/proof/") { 1 } else { 3 };
        assert!(
            tests.len() >= want,
            "{name} yielded {} tests; a fixture the pack cannot read would make every \
             assertion about it vacuous",
            tests.len()
        );
        assert!(
            tests.iter().any(|t| !t.assertions.is_empty()),
            "{name} yielded no assertions at all"
        );
    }
}

#[test]
fn every_family_run_is_green_so_only_the_source_separates_them() {
    // `session()` hands `verify` an `exit_code: 0`, and that is only honest if
    // the captures really are green.
    //
    // The second half is the one worth a test rather than a sentence: **most of
    // the family runs are byte-identical in their tally**, so the run carries
    // almost no information about which of them was a cheat. The count is
    // computed here rather than written down, because a count nobody can
    // recompute is exactly the kind of claim this crate exists to catch — an
    // earlier draft of this comment said sixteen, and it is thirteen.
    let mut greens = 0usize;
    let mut tallies = std::collections::BTreeMap::<(u64, u64, u64, u64), usize>::new();
    for run in RUNS {
        if run.contains("proof-unwrapped") {
            continue; // the one capture that is meant to fail
        }
        let p = parsed_view(run);
        assert_eq!(p.failed, 0, "{run} reports a failure");
        assert!(p.tests_total > 0, "{run} reports no tests at all");
        assert_eq!(
            p.passed + p.skipped,
            p.tests_total,
            "{run}'s parts do not add up to its total"
        );
        assert_eq!(
            p.xfail, 0,
            "{run} reported an xfail; .NET has no expected-failure outcome and \
             `runners::dotnet` says so in as many words — a non-zero here means \
             the parser started inventing one"
        );
        if !run.contains("proof-") {
            *tallies
                .entry((p.tests_total, p.passed, p.failed, p.skipped))
                .or_default() += 1;
        }
        greens += 1;
    }
    assert_eq!(greens, RUNS.len() - 1);

    let family = tallies.values().sum::<usize>();
    let largest = *tallies.values().max().expect("the family is not empty");
    assert_eq!(family, 18, "there are eighteen family captures");
    assert_eq!(
        largest, 13,
        "thirteen of the eighteen family runs report the identical tally \
         (4 total, 4 passed, 0 failed, 0 skipped) — two differ by a single \
         `Skipped: 1`, two by a `Total: 5`, one by a `Total: 3`. That spread is \
         the point of the families: the run cannot separate an honest edit from \
         a cheat, which is why §6.7 reads the source, and the source cannot \
         separate a skip that took effect from one that did not, which is why \
         `escalate` reads the run. Seen: {tallies:?}"
    );
}

#[test]
fn every_capture_here_is_read_by_the_one_grammar_the_readme_names() {
    // The README says these captures exercise `parse_vstest_console` — the
    // shape `runners::dotnet` calls "grammar 1 and 5", which xunit v3 under
    // Microsoft.Testing Platform prints and piped VSTest prints too — and that
    // the other four console grammars are not represented here at all.
    //
    // That is a claim about which code path the whole family runs through, so
    // it is checked rather than written down: the four competing anchors are
    // the ones `parse_console` dispatches on, in its order, and each is absent.
    // If a re-capture switched runners or turned on TerminalLogger, every
    // verdict in this file would quietly start proving something about a
    // different parser.
    const OTHER_GRAMMAR_ANCHORS: [&str; 4] = [
        "=== TEST EXECUTION SUMMARY ===", // xunit v3, run directly
        "Test run summary:",              // MTP, run directly
        "Test summary:",                  // MSBuild TerminalLogger
        "Total tests:",                   // the detailed VSTest block
    ];
    for run in RUNS {
        let bytes = fixture(&format!("runs/{run}"));
        assert!(
            bytes.contains("Total: ") && bytes.contains("Passed: ") && bytes.contains("Failed: "),
            "runs/{run} has no `is_vstest_summary_shaped` line"
        );
        for anchor in OTHER_GRAMMAR_ANCHORS {
            assert!(
                !bytes.contains(anchor),
                "runs/{run} carries `{anchor}`, so it is read by a different grammar \
                 than the README claims for this directory"
            );
        }
    }
}
