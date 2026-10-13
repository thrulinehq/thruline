//! The Go families, end to end — the source pair through the detectors, the
//! **captured run** through this crate's `go test` parser, and both into
//! `verify::tests_pass::verify`.
//!
//! # Why both families, and why not a detector-only test
//!
//! `crates/verify/tests/honest_and_cheat_families.rs` states the reason and it
//! is not a style preference: the previous control was a single honest edit —
//! changing an expected literal — and that is the **one** honest shape that
//! trips no `fail` detector, which is exactly why it passed while four others
//! failed. Two of §6.7's severities are decided by the *run* and not by the
//! detector (`skip_marker_added` is escalated to `fail` or lowered to `info` by
//! `tests_pass::escalate`, reading `parsed.skipped`), so a test that stops at
//! the finding list cannot see them.
//!
//! # The runs here are captures, not arguments
//!
//! `tests/fixtures/families/go/runs/*.txt` is real `go test -v ./...` output
//! from go1.27.1 on this machine, one file per head tree, and the `ParsedView`
//! every verdict below rests on is produced by running
//! `runners::gotest::parse` over those bytes. Nothing in this file states a
//! tally by hand. `tests/fixtures/families/go/README.md` names the command and
//! the exit code behind each capture; that convention is
//! `tests/fixtures/runners/README.md`'s, and it exists because a parser proven
//! against a hand-written fixture is not proven.
//!
//! # Three things Go does not have, named here rather than smuggled
//!
//! 1. **Go has no xfail.** `ParsedView::xfail` is never set by any `go test`
//!    output, in either the console or the `-json` form — the actions are
//!    `pass`, `fail` and `skip` and there is no fourth. So the pytest family's
//!    "flaky test marked `xfail`, run reports it as such" has no transcription
//!    here. The Go idiom that quarantines a flaky assertion **without** leaving
//!    it unrun is a tolerance, and that is the honest case in its place. The
//!    other Go option — `t.Skip("flaky")` — leaves the test unrun, the run says
//!    so, and §6.7 refutes it. That is the design, and
//!    `go_has_no_xfail_so_quarantining_by_skip_is_refuted_and_that_is_deliberate`
//!    holds it to a test.
//!
//! 2. **`assertion_swallowed` can never fire for Go.** See
//!    `cheat_a_body_wrapped_in_defer_recover_is_currently_invisible`. The
//!    fixture is a real swallow, measured: exit 1 unwrapped, exit 0 wrapped,
//!    same assertion. The detector says nothing, and the fault is in the shared
//!    engine rather than in the pack.
//!
//! 3. **Two of the seven cheats are `warn` by §6.7 and so do not refute.**
//!    `mocked_unit_under_test` and `sleep_or_retry_added` are `Severity::Warn`
//!    in `crates/verify/src/tamper.rs` itself, and `VerdictRecord::with_finding`
//!    moves a verdict only on a `fail`. This crate must mirror verify's
//!    severities exactly — `tests/detector_name_parity.rs` is the guard on the
//!    names and the same argument applies to the severities — so the two cheats
//!    below come out `Supported` with a warning attached. Each has an active
//!    test asserting the detector and its real severity, and an `#[ignore]`d
//!    test recording the verdict the stage brief asked for and where the change
//!    that would produce it has to land.
//!
//! # And one thing the families found that nothing else was looking for
//!
//! An **honest** case comes out refuted: a rename whose comment was reworded on
//! the way past — `isn't sorted` to `is unsorted` — is reported as
//! `test_deleted` at `fail`. `tamper::as_test_fn` builds `body_tokens` by
//! handing raw function text to `shape::strip_language_literals`, which reads
//! the apostrophe as a rune-literal delimiter and eats the rest of the body;
//! 13 tokens become 5, `jaccard` scores 0.3, and `similar_bodies` says the
//! renamed test is a different test. It is a `fail`-severity false accusation
//! against routine work, in the shared engine, reachable from all four packs.
//! `honest_a_rename_that_also_tidies_a_contraction_is_falsely_refuted` holds it
//! to a test; the same root cause inside `Go::normalise` — where a pack *does*
//! have a hook — was fixed this session and is regression-tested in
//! `src/langs/go.rs`.

use std::path::PathBuf;

use thruline_langpacks::langs::go::Go;
use thruline_langpacks::runners::gotest;
use thruline_langpacks::tamper::diff_test_file_with_symbols;
use thruline_verify::binding::{CheckView, ParsedView, SessionView, SnapshotView};
use thruline_verify::parse::RunnerOutcome;
use thruline_verify::tests_pass::{verify, Inputs};
use thruline_verify::verdict::{Severity, Verdict};

fn fixture(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/families/go")
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
    match gotest::parse(&bytes) {
        RunnerOutcome::Parsed(p) => ParsedView {
            tests_total: p.tests_total,
            passed: p.passed,
            failed: p.failed,
            skipped: p.skipped,
            xfail: p.xfail,
        },
        RunnerOutcome::Unverifiable { reason, hint } => panic!(
            "runs/{run} is a real `go test` capture and the parser could not read it: \
             {reason} ({hint})"
        ),
    }
}

/// A session whose suite ran on the final tree, carrying the captured run.
///
/// Field for field `crates/verify/tests/honest_and_cheat_families.rs`'s, with
/// the runner and argv swapped for Go's. `go test ./...` names no path that
/// `tests_pass::narrowing_flag` recognises, so no `narrowed_run` finding is in
/// play and the verdicts below are about §6.7 alone.
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
            runner: "go test".into(),
            bypass: None,
            argv: Some(vec![
                "go".into(),
                "test".into(),
                "-v".into(),
                "./...".into(),
            ]),
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

/// One family case: the base tree's `rate_test.go` against the case's, then the
/// verifier, exactly as the CLI does it.
fn verdict_for(case: &str) -> (Verdict, Vec<(String, Severity)>) {
    verdict_for_with_symbols(case, &[])
}

/// The base tree a case is diffed against.
///
/// Every case uses `base/rate_test.go`. The one exception carries its own, and
/// carries it because the defect it demonstrates lives in the base: see
/// `honest/comment_tidy/base_rate_test.go`.
fn base_for(case: &str) -> String {
    let own = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/families/go")
        .join(case)
        .join("base_rate_test.go");
    if own.exists() {
        fixture(&format!("{case}/base_rate_test.go"))
    } else {
        fixture("base/rate_test.go")
    }
}

fn verdict_for_with_symbols(
    case: &str,
    changed_symbols: &[String],
) -> (Verdict, Vec<(String, Severity)>) {
    let base = base_for(case);
    let head = fixture(&format!("{case}/rate_test.go"));
    let findings = diff_test_file_with_symbols("rate_test.go", &base, &head, &Go, changed_symbols)
        .expect("the go grammar loads and both fixture trees parse");
    let run = format!("{}.txt", case.replace('/', "-"));
    let record = verify(
        "clm",
        &session(&run),
        &Inputs {
            scope_is_all: true,
            tamper_findings: findings,
            source_changed: true,
            test_files_changed: vec!["rate_test.go".into()],
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
    "honest/table_driven",
    "honest/negative_error",
];

/// The honest case that does **not** come out `supported`, kept out of
/// `HONEST` so the separation test stays a true statement about that list and
/// this one is not quietly counted as a pass. See
/// `honest_a_rename_that_also_tidies_a_contraction_is_falsely_refuted`.
const HONEST_FALSELY_REFUTED: &str = "honest/comment_tidy";

/// The four cheats §6.7 gives a `fail` severity to. The other three are
/// `cheat/mocked_unit_under_test`, `cheat/sleep_or_retry_added` (both `warn`
/// by verify's own table) and `cheat/assertion_swallowed` (which fires
/// nothing at all for Go). Each is covered by its own test below, and the
/// separation test names all three rather than dropping them from the list.
const REFUTING_CHEATS: &[&str] = &[
    "cheat/assertion_removed",
    "cheat/assertion_swallowed",
    "cheat/assertion_weakened",
    "cheat/skip_marker_added",
    "cheat/test_deleted",
];

/// The two cheats that do not refute, listed rather than omitted.
///
/// **Neither is a gap in this crate.** BUILD-SPEC §6.7 files
/// `mocked_unit_under_test` and a `sleep`/retry loop under `warn`, and
/// `VerdictRecord::with_finding` moves a verdict only on a `fail`. Both
/// detectors fire, at the severity the spec gives them; the verdict staying
/// `supported` is the spec's decision, not a miss. A pack that escalated on its
/// own would be a second §6.7, and the copy that drifts is the one nothing
/// gates.
///
/// `cheat/assertion_swallowed` was here until 2026-09-05, when
/// `tamper::swallows` grew its third path and the swallow became visible.
const NON_REFUTING_CHEATS: &[&str] =
    &["cheat/mocked_unit_under_test", "cheat/sleep_or_retry_added"];

/// Every case on disk. The tests that check the fixtures themselves iterate
/// this, so a case cannot be added to the directory and left unexercised, and
/// none of the four lists above can quietly shrink.
fn all_cases() -> Vec<&'static str> {
    HONEST
        .iter()
        .copied()
        .chain(std::iter::once(HONEST_FALSELY_REFUTED))
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

    // The two facts `escalate` actually reads, and the only two places in this
    // file where the *run* rather than the source decides a severity.
    assert_eq!(
        parsed_view("honest-conditional_skip.txt").skipped,
        0,
        "the conditional skip is guarded on plan9 and this run is not plan9, \
         so the marker left nothing unrun"
    );
    assert_eq!(
        parsed_view("cheat-skip_marker_added.txt").skipped,
        1,
        "the unconditional skip took effect and `go test` reported it"
    );

    // Go has no xfail and no capture here invents one.
    for case in HONEST {
        let run = format!("{}.txt", case.replace('/', "-"));
        assert_eq!(
            parsed_view(&run).xfail,
            0,
            "{case}: `go test` has no xfail action; a non-zero one would mean \
             the parser invented it"
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
fn honest_a_conditional_skip_that_skipped_nothing_here_is_supported() {
    let (verdict, findings) = verdict_for("honest/conditional_skip");
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        has(&findings, "skip_marker_added", Severity::Info),
        "the marker should still be REPORTED, just not as a refutation — and the \
         thing that lowered it is the captured run's `skipped == 0`, not the \
         detector: {findings:?}"
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
    // Go's stand-in for `@pytest.mark.xfail`. There is no xfail action in
    // `go test`, so the only way to quarantine a flaky assertion without
    // leaving the test unrun is to assert the tolerance the value is actually
    // known to. `Equality -> Approximate` is a weakening and §6.7 says so —
    // but it is not a weakening to `Truthy`, so it is a `warn`, and a `warn`
    // does not refute.
    let (verdict, findings) = verdict_for("honest/flaky_tolerance");
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        has(&findings, "assertion_weakened", Severity::Warn),
        "the loosening should be surfaced, at warn: {findings:?}"
    );
}

#[test]
fn honest_two_assertions_folded_into_one_table_driven_subtest_is_supported() {
    // The assertion count per named test changes honestly: `TestRateLarge` had
    // two `if` comparisons and now has one, inside a `t.Run` loop over a table.
    // `assertion_count_decreased` is the right finding and it is a `warn`;
    // `assertion_removed` would be wrong, because the test still asserts.
    let (verdict, findings) = verdict_for("honest/table_driven");
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        has(&findings, "assertion_count_decreased", Severity::Warn),
        "the fold is a real count decrease and should be reported: {findings:?}"
    );
    assert!(
        !findings.iter().any(|(d, _)| d == "assertion_removed"),
        "the subtest still asserts; its assertions roll up into the parent: \
         {findings:?}"
    );
}

#[test]
fn honest_a_negative_test_expecting_an_error_is_supported_and_not_swallowing() {
    // Go's expects-a-throw idiom is `if err == nil { t.Fatal(...) }`, and this
    // is the false positive the structural catch-detector in `tamper::swallows`
    // was written to avoid in Java, C# and Ruby.
    //
    // For Go it cannot arise at all, and that is worth stating precisely rather
    // than claiming a detector passed a test it never ran: `Go::spec()` ships
    // an empty `swallow_markers` and an empty `catch_kinds`, so `swallows()`
    // returns false for every Go function ever written. The assertion below is
    // therefore a guard on a future pack that fills either field in — the day
    // someone adds `defer`/`recover` markers, this test is what stops the
    // honest negative test from becoming collateral.
    let (verdict, findings) = verdict_for("honest/negative_error");
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        !findings.iter().any(|(d, _)| d == "assertion_swallowed"),
        "an honest negative test is not a swallowed assertion: {findings:?}"
    );
    assert!(
        has(&findings, "test_added", Severity::Info),
        "the new test should be reported as added: {findings:?}"
    );
}

// ---------------------------------------------------------------------------
// The honest case that comes out refuted, and the defect that refutes it
// ---------------------------------------------------------------------------

/// Fixed 2026-09-05 in `src/tamper.rs`: comments are blanked structurally
/// before any text-derived fact is read off a test.
///
/// This was `#[ignore]`d beside a characterisation test asserting the false
/// accusation — a rename that also reworded `isn't` in a comment came out
/// `test_deleted` at `fail`, because the apostrophe opened a rune literal in
/// the token builder that ate the rest of the body (13 tokens to 5, jaccard
/// 0.3). The characterisation test has been deleted, as its own failure
/// message instructed.
#[test]
fn honest_a_rename_that_also_tidies_a_contraction_is_supported() {
    let (verdict, findings) = verdict_for(HONEST_FALSELY_REFUTED);
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        findings.iter().any(|(d, _)| d == "test_renamed"),
        "{findings:?}"
    );
}

// ---------------------------------------------------------------------------
// The cheat family — the four §6.7 gives a `fail` to
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
fn cheat_an_equality_loosened_to_an_ordering_is_refuted() {
    // `got != 9.0` became `got <= 0`, which `Rate` cannot fail for any weight
    // it accepts. Ordering is deliberately `Truthy` in `Go::classify` — the
    // ladder has no rung for it, and calling it `Equality` would make exactly
    // this cheat invisible.
    let (verdict, findings) = verdict_for("cheat/assertion_weakened");
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(
        has(&findings, "assertion_weakened", Severity::Fail),
        "{findings:?}"
    );
}

#[test]
fn cheat_a_skip_marker_that_actually_skipped_is_refuted() {
    // The severity here is not the detector's. `diff_test_file` emits
    // `skip_marker_added` at `warn` for both this case and the honest
    // conditional skip above; what separates them is `parsed.skipped`, read off
    // the captured run by `tests_pass::escalate`. This is the assertion a
    // detector-only test cannot make.
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
fn cheat_a_test_deleted_outright_is_refuted() {
    let (verdict, findings) = verdict_for("cheat/test_deleted");
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(
        has(&findings, "test_deleted", Severity::Fail),
        "{findings:?}"
    );
}

// ---------------------------------------------------------------------------
// The three cheats that do not come out `unsupported`, and exactly why
// ---------------------------------------------------------------------------

/// Fixed 2026-09-05: `tamper::swallows` grew a third path for wrappers that
/// neutralise a failure with no handler clause — Go's
/// `defer func() { recover() }()`.
///
/// The rule is [`catch_swallows`]' rule read off a different tree shape: the
/// wrapper swallows when the assertions are outside it and the wrapper itself
/// neither asserts nor re-raises. The subtlety that made the first attempt
/// exempt this very fixture is that Go admits every top-most `if` as a
/// candidate assertion, so the deferred `if r := recover(); r != nil {
/// t.Logf(...) }` counted as "the wrapper asserts". Counting only assertions
/// that can FAIL — non-`Truthy` by the pack's own classifier, since `t.Logf`
/// cannot fail a test and `t.Error` can — is what separates this cheat from
/// the honest `if r := recover(); r == nil { t.Error(...) }` negative test.
#[test]
fn cheat_a_body_wrapped_so_a_failure_no_longer_propagates_is_refuted() {
    let (verdict, findings) = verdict_for("cheat/assertion_swallowed");
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(
        has(&findings, "assertion_swallowed", Severity::Fail),
        "{findings:?}"
    );
}

#[test]
fn cheat_a_mock_of_a_symbol_the_session_changed_is_reported_at_warn() {
    // The detector is right and it fires: the test stopped calling `Rate` and
    // started asserting what a stub was told to answer, and `Rate` is the
    // symbol this session changed.
    //
    // The verdict does not move, because `mocked_unit_under_test` is
    // `Severity::Warn` in `crates/verify/src/tamper.rs` and
    // `VerdictRecord::with_finding` moves a verdict only on a `fail`. This
    // crate mirrors verify's severities rather than choosing its own — a pack
    // that escalated on its own would be a second §6.7, and the copy that
    // drifts is the one nothing gates.
    let changed = vec!["Rate".to_string()];
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
    let changed = vec!["Rate".to_string()];
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
// Go has no xfail, and the consequence is a refusal
// ---------------------------------------------------------------------------

#[test]
fn go_has_no_xfail_so_quarantining_by_skip_is_refuted_and_that_is_deliberate() {
    // The pytest family's honest case 5 is a flaky test marked `xfail`, which
    // the run reports as an xfail and §6.3 warns about without refuting.
    // `go test` has no such action. Its only quarantine is `t.Skip`, and a skip
    // that took effect leaves the test unrun — so the same intent that is
    // honest in pytest is a refutation in Go.
    //
    // This is not a defect in the pack and there is nothing here to fix. It is
    // recorded because the alternative — quietly treating a Go skip as an xfail
    // so the family looks symmetric — would make `skip_marker_added`
    // unescalatable for the one language where the marker lives in the body.
    let (verdict, _) = verdict_for("cheat/skip_marker_added");
    assert_eq!(verdict, Verdict::Unsupported);
    assert_eq!(parsed_view("cheat-skip_marker_added.txt").xfail, 0);
    assert_eq!(parsed_view("cheat-skip_marker_added.txt").skipped, 1);
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

    // `HONEST_FALSELY_REFUTED` was in this position until the engine fix of
    // 2026-09-05; it is now an ordinary member of `HONEST` above and is
    // asserted `Supported` with the rest. The name is kept so the fixture
    // directory and MERGE-PLAN still resolve.
    assert_eq!(
        verdict_for(HONEST_FALSELY_REFUTED).0,
        Verdict::Supported,
        "{HONEST_FALSELY_REFUTED} is refuted again — the comment-blanking in \
         tamper::as_test_fn has regressed"
    );
}

#[test]
fn every_fixture_pair_actually_differs_from_the_base() {
    // A head file accidentally identical to the base would make its case
    // vacuously `supported`, and three of the honest cases assert exactly that.
    for case in all_cases() {
        let head = fixture(&format!("{case}/rate_test.go"));
        assert_ne!(
            head,
            base_for(case),
            "{case} is byte-identical to the base tree it is diffed against"
        );
    }
}

#[test]
fn every_case_directory_on_disk_is_exercised_by_a_list_above() {
    // The guard against a fixture pair that exists and is never run. A case
    // added to the directory and forgotten is indistinguishable, in a test
    // report, from a case that was never written.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/families/go");
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
