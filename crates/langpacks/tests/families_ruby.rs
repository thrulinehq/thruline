//! The Ruby families, not the one case.
//!
//! `crates/verify/tests/honest_and_cheat_families.rs` states why this file has
//! to exist for every language and not just for Python, and the reason is a
//! measurement rather than a principle: the previous control there was a single
//! honest edit — changing an expected literal — and that is the **one** honest
//! shape that trips no `fail` detector. It passed while a rename, a `skipif`, a
//! split and an `xfail` were all reported `unsupported`. Three `fail` detectors
//! fired on routine refactoring while the one `warn` detector caught the
//! realistic cheat.
//!
//! So both families are built here too, end to end through
//! `thruline_verify::tests_pass::verify`, for two reasons no detector-only test
//! can reach:
//!
//! - `skip_marker_added` is emitted as `warn` and then escalated to `fail` or
//!   lowered to `info` by `escalate`, on `parsed.skipped`. A detector-only test
//!   sees `warn` for both HONEST 2 and CHEAT 3 and cannot tell them apart. They
//!   differ here only in the run: 0 skipped against 1.
//! - every other §6.7 finding reaches the verdict through
//!   `VerdictRecord::with_finding`, which moves the verdict only on `fail`. A
//!   detector-only assertion that "the detector fired" therefore proves nothing
//!   about whether the claim was refuted — see the three `gap_` tests at the
//!   bottom, where the detector fires and the claim stands anyway.
//!
//! # What Ruby adds that the Python family could not test
//!
//! Ruby has no test *declaration*: `it "…" do` is an ordinary call and
//! `def test_x` is an ordinary method. Both dialects are exercised, from two
//! base files, because the second one carries the case the first cannot spell —
//! `begin` / `rescue`, where the honest negative test (HONEST 9) and the
//! swallowed assertion (CHEAT 4) are made of the same keywords in the same
//! order and are told apart only by **where the assertion sits**.
//!
//! # Three cases here do not pass, and they are `#[ignore]`d rather than removed
//!
//! Deleting a case to make a suite green is the tampering this crate detects.
//! They are two different kinds of gap and the difference matters:
//!
//! - **`assertion_swallowed` is missing coverage.** BUILD-SPEC §6.7 puts *"test
//!   body wrapped in try/except-pass"* under `fail`, and Ruby detects it not at
//!   all — `Ruby::spec().catch_kinds` is empty because no value of it makes the
//!   shared `tamper::catch_swallows` fire on `tree-sitter-ruby`'s tree shape.
//! - **`mocked_unit_under_test` and `sleep_or_retry_added` are a Rule 7
//!   conflict.** §6.7 files both under `warn`, in as many words, and a `warn`
//!   never moves a verdict. The stage brief asks for `unsupported`. That is the
//!   spec and an instruction disagreeing, which is reported and not blended:
//!   both detectors fire, with the name and the severity §6.7 assigns, and the
//!   claim stands.
//!
//! Each of the three is paired with a live `gap_` test that pins what the code
//! does **today**, so the two fail in opposite directions: change the engine
//! and the `gap_` test fails, pointing at the `#[ignore]` to lift. An
//! `#[ignore]` with nothing watching it is a case that quietly stopped
//! existing.

use std::path::PathBuf;

use thruline_langpacks::pack::pack_for_path;
use thruline_langpacks::tamper::{diff_test_file, diff_test_file_with_symbols};
use thruline_verify::binding::{CheckView, ParsedView, SessionView, SnapshotView};
use thruline_verify::tests_pass::{verify, Inputs};
use thruline_verify::verdict::{Severity, Verdict};

/// Where the RSpec half of the session's edit is recorded as living.
///
/// A real repository path rather than the fixture's own filename: `verify`'s
/// `narrowing_flag` and `is_test_path` both read paths out of the record, and a
/// fixture named `honest_split_spec.rb` in a `tests/fixtures` directory would
/// be exercising this file's layout instead of a suite's.
/// The §6.7 detectors this crate emits.
///
/// Separated from `tests_pass`'s own §6.3 run-level findings (`skipped_tests`,
/// `narrowed_run`, …), which travel in the same `Vec` and are not governed by
/// the severity cap.
const TAMPER_DETECTORS: &[&str] = &[
    "assertion_count_decreased",
    "assertion_removed",
    "assertion_swallowed",
    "assertion_weakened",
    "mocked_unit_under_test",
    "skip_marker_added",
    "sleep_or_retry_added",
    "test_added",
    "test_deleted",
    "test_renamed",
];

/// The subset that ACCUSES — the detectors whose presence on an honest edit
/// would be a false positive.
///
/// The complement is not noise, it is the detectors §6.7 makes ambiguous **on
/// purpose**. `assertion_count_decreased` is the clearest case: folding three
/// assertions into the table they read genuinely decreases the count, and §6.7
/// files a decrease under `warn` (a *removal* is the `fail`) precisely because
/// the honest and the dishonest version look identical from the count alone.
/// `test_added` and `test_renamed` are `info` by construction.
///
/// Asserting that an honest edit raises NO tamper warn conflated the two and
/// failed on `honest_table_driven_spec.rb`, which is a fixture written to be
/// honest and to decrease the count. The separation this test claims is about
/// accusation, so it is measured on the accusing set.
const ACCUSING_DETECTORS: &[&str] = &[
    "assertion_removed",
    "assertion_swallowed",
    "assertion_weakened",
    "mocked_unit_under_test",
    "skip_marker_added",
    "sleep_or_retry_added",
    "test_deleted",
];

const SPEC_PATH: &str = "spec/shipping_spec.rb";
const TEST_PATH: &str = "test/shipping_test.rb";

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/families/ruby")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// What the runner reported. The half of every case that no source detector can
/// see.
#[derive(Clone, Copy)]
struct Run {
    total: u64,
    skipped: u64,
    xfail: u64,
}

impl Run {
    const fn green(total: u64) -> Run {
        Run {
            total,
            skipped: 0,
            xfail: 0,
        }
    }
}

/// A session whose suite passed on the final tree.
///
/// `argv` is deliberately a bundler invocation with no path in it: `bundle exec
/// rspec spec/shipping_spec.rb` would be a *narrowed* run, which is its own
/// `fail` under `scope: all`, and a cheat refuted by `narrowed_run` would be a
/// cheat this file never actually caught.
fn session(runner: &str, argv: &[&str], run: Run) -> SessionView {
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
            runner: runner.into(),
            bypass: None,
            argv: Some(argv.iter().map(|a| (*a).to_string()).collect()),
            exit_code: 0,
            input_tree_hash: "t".into(),
            paths_changed_since_final: None,
            stdio: Some("pipe".into()),
            reexecution: None,
            parsed: Some(ParsedView {
                tests_total: run.total,
                passed: run.total - run.skipped - run.xfail,
                failed: 0,
                skipped: run.skipped,
                xfail: run.xfail,
            }),
        }],
        head_commit: Some("c".into()),
        capture_degraded: false,
        degraded_reason: None,
    }
}

/// One source pair, run through the detectors and then through the verifier —
/// exactly the order the CLI uses.
///
/// `changed_symbols` is the session's non-test change, which only
/// `mocked_unit_under_test` reads: a mock is a finding when its target resolves
/// to something this session edited, and identical source with an empty list is
/// not a finding at all.
fn judge(
    path: &str,
    base_file: &str,
    head_file: &str,
    run: Run,
    changed_symbols: &[String],
) -> (Verdict, Vec<(String, Severity)>) {
    let pack = pack_for_path(path).expect("`.rb` resolves to the Ruby pack");
    let (base, head) = (fixture(base_file), fixture(head_file));
    // Both public entry points, on the path each is actually called on. The
    // four-argument form is what a consumer with no symbol index calls, and it
    // is a delegation today — a delegation nothing exercises is a delegation
    // that can quietly stop delegating.
    let findings = if changed_symbols.is_empty() {
        diff_test_file(path, &base, &head, pack)
    } else {
        diff_test_file_with_symbols(path, &base, &head, pack, changed_symbols)
    }
    .expect("both fixtures parse as Ruby");

    let (runner, argv) = if path.ends_with("_spec.rb") {
        ("rspec", ["bundle", "exec", "rspec"])
    } else {
        ("minitest", ["bundle", "exec", "rake"])
    };

    let record = verify(
        "clm",
        &session(runner, &argv, run),
        &Inputs {
            scope_is_all: true,
            tamper_findings: findings,
            // The session edited `lib/shipping.rb`; without this every case
            // would also carry `test_changed_without_source_change`, which is
            // a true finding about a different situation.
            source_changed: true,
            test_files_changed: vec![path.to_string()],
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

fn spec(head: &str, run: Run) -> (Verdict, Vec<(String, Severity)>) {
    judge(SPEC_PATH, "base_spec.rb", head, run, &[])
}

fn unit(head: &str, run: Run) -> (Verdict, Vec<(String, Severity)>) {
    judge(TEST_PATH, "base_test.rb", head, run, &[])
}

fn fired(findings: &[(String, Severity)], detector: &str, severity: Severity) -> bool {
    findings
        .iter()
        .any(|(d, s)| d == detector && *s == severity)
}

// ---------------------------------------------------------------------------
// The honest family — every one of these must be `supported`
// ---------------------------------------------------------------------------

#[test]
fn honest_renaming_two_examples_is_supported() {
    let (verdict, findings) = spec("honest_renamed_spec.rb", Run::green(4));
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert_eq!(
        findings.iter().filter(|(d, _)| d == "test_renamed").count(),
        2,
        "both renames should be reported as renames, not as deletions: {findings:?}"
    );
    // `test_added` is the assertion that makes the two above discriminating.
    // Measured, by mutating this fixture until it lied: `diff_test_file` pairs
    // a vanished base test with the FIRST similar head test and does not mark
    // it taken, so two base tests can both pair to the same head. Rewrite one
    // renamed body into something unrecognisable and the count of
    // `test_renamed` is still 2 — both base tests pair with the surviving
    // head — while the rewritten one shows up here as new. Without this line
    // the test passes on a fixture where a rename became a replacement.
    assert!(
        !findings
            .iter()
            .any(|(d, _)| d == "test_deleted" || d == "test_added"),
        "a rename is neither a deletion nor an addition: {findings:?}"
    );
}

#[test]
fn honest_a_conditional_skip_that_skipped_nothing_here_is_supported() {
    // `skip("…") if RUBY_PLATFORM =~ /mswin/`. The pack has no way to tell a
    // guarded `skip` from a bare one — Ruby's marker is a method call, not an
    // annotation with a predicate — so the detector fires either way and the
    // RUN is what separates this from CHEAT 3. Nothing was skipped here.
    let (verdict, findings) = spec("honest_conditional_skip_spec.rb", Run::green(4));
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        fired(&findings, "skip_marker_added", Severity::Info),
        "the marker must still be REPORTED, just not as a refutation: {findings:?}"
    );
}

#[test]
fn honest_one_example_split_into_two_is_supported() {
    let (verdict, findings) = spec("honest_split_spec.rb", Run::green(5));
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        fired(&findings, "test_renamed", Severity::Info),
        "the half that kept the body should pair with the original: {findings:?}"
    );
}

#[test]
fn honest_a_tariff_change_that_moves_the_expected_figures_is_supported() {
    // The control, and the one honest shape that trips nothing at all. It is
    // here so that a suite which passes cannot be a suite that only ever sees
    // this case — which is precisely how `verify`'s single-case control passed
    // while four honest edits were being refuted beside it.
    let (verdict, findings) = spec("honest_literal_changed_spec.rb", Run::green(4));
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        findings.is_empty(),
        "the control must trip no detector at all: {findings:?}"
    );
}

#[test]
fn honest_a_flaky_example_marked_pending_is_supported_with_a_warning() {
    // RSpec's `pending` RUNS the example and requires it to fail, so it lands
    // in the tally as a pending example — an xfail — and not as a skip.
    // `escalate` reads `parsed.skipped` only, deliberately: §6.3's own skips
    // rule already warns about the xfail, and counting it in
    // `skip_marker_added` too would refute the claim twice for one honest
    // decision.
    let (verdict, findings) = spec(
        "honest_pending_flaky_spec.rb",
        Run {
            total: 4,
            skipped: 0,
            xfail: 1,
        },
    );
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        fired(&findings, "skip_marker_added", Severity::Info),
        "{findings:?}"
    );
    assert!(
        fired(&findings, "skipped_tests", Severity::Warn),
        "the pending example was not surfaced at all: {findings:?}"
    );
}

#[test]
fn honest_folding_three_assertions_into_the_table_they_read_is_supported() {
    // The example keeps its name and covers exactly what it covered before, so
    // its assertion count honestly drops 3 -> 1. `assertion_count_decreased` is
    // the right finding and `warn` is the right severity: a count that fell is
    // worth saying, and it is not by itself evidence that anything was dropped.
    let (verdict, findings) = spec("honest_table_driven_spec.rb", Run::green(4));
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        fired(&findings, "assertion_count_decreased", Severity::Warn),
        "{findings:?}"
    );
    assert!(
        !findings.iter().any(|(d, _)| d == "assertion_removed"),
        "one assertion is not none: {findings:?}"
    );
}

#[test]
fn honest_an_rspec_negative_test_does_not_trip_assertion_swallowed() {
    // `expect { … }.to raise_error(ArgumentError)`. The assertion IS the fact
    // that something was raised — the shape `tamper::swallows` was made
    // structural to avoid mistaking for a discarded failure.
    //
    // **What this case really proves for Ruby, stated so it is not oversold:**
    // the `assertion_swallowed` half is vacuous here, because `catch_kinds` is
    // empty and no Ruby file can produce that finding at all. What it does
    // prove is the classifier: `be_nil` and `raise_error` both land on
    // `Truthy`, so replacing one with the other is not a weakening — and if
    // `raise_error` fell through to the `==` fallback instead, this honest edit
    // would be a `fail`.
    let (verdict, findings) = spec("honest_raise_error_spec.rb", Run::green(4));
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        !findings.iter().any(|(d, _)| d == "assertion_swallowed"),
        "{findings:?}"
    );
    assert!(
        !findings.iter().any(|(d, _)| d == "assertion_weakened"),
        "`be_nil` and `raise_error` are both Truthy; neither weakens the other: {findings:?}"
    );
}

#[test]
fn honest_a_minitest_negative_test_does_not_trip_assertion_swallowed() {
    let (verdict, findings) = unit("honest_assert_raises_test.rb", Run::green(3));
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        !findings.iter().any(|(d, _)| d == "assertion_swallowed"),
        "{findings:?}"
    );
    assert!(
        findings.is_empty(),
        "`assert_nil` -> `assert_raises` is one Truthy assertion for another: {findings:?}"
    );
}

#[test]
fn honest_a_begin_rescue_negative_test_does_not_trip_assertion_swallowed() {
    // **The case the structural detector exists for**, and — for Ruby — the one
    // this suite cannot yet claim credit for. It passes, but it passes
    // vacuously: `Ruby::spec().catch_kinds` is empty, so `catch_swallows` never
    // runs on any Ruby file and every Ruby test is "not swallowing" by
    // construction. See `cheat_a_swallowed_assertion_is_refuted` below for the
    // other end of that same hole.
    //
    // It is asserted anyway, because when the engine learns Ruby's `begin`
    // shape this is the file that must still come out `supported`, and a case
    // written after a detector is the case that gets written to match it.
    let (verdict, findings) = unit("honest_rescue_negative_test.rb", Run::green(3));
    assert_eq!(verdict, Verdict::Supported, "{findings:?}");
    assert!(
        !findings.iter().any(|(d, _)| d == "assertion_swallowed"),
        "{findings:?}"
    );
}

// ---------------------------------------------------------------------------
// The cheat family — every one fires its detector, and NONE of them refutes
//
// **The cap, not a miss.** Operator ruling 2026-09-05: Ruby's measured
// ambiguity is 0.1275% against the 0.1% budget, so *"a detector over budget may
// inform; it may not accuse"*. `Ruby::severity_cap()` holds every finding at
// `warn` at emission, and `VerdictRecord::with_finding` moves a verdict only on
// a `fail` — so a real, correctly-detected Ruby cheat comes back `supported`.
//
// That is the intended behaviour and these tests assert it deliberately. What
// they must NOT be read as is "Ruby detects nothing": every one asserts its
// detector FIRES, and asserts the severity is exactly the cap. The day
// `MEASUREMENTS.toml` records Ruby under budget, the cap comes off,
// `severity_cap.rs::the_cap_and_the_measurement_agree` fails, and these
// assertions get flipped back to `Unsupported` in the same change.
//
// The uncapped severity each detector WOULD carry is named in each test, so the
// flip is mechanical rather than a re-derivation.
// ---------------------------------------------------------------------------

#[test]
fn cheat_every_assertion_deleted_from_an_example_is_refuted() {
    // Capped: this would be `Severity::Fail` uncapped.
    let (verdict, findings) = spec("cheat_assertion_removed_spec.rb", Run::green(4));
    assert!(
        fired(&findings, "assertion_removed", Severity::Warn),
        "the detector must still FIRE — the cap changes its severity, not \
         whether Ruby sees the cheat: {findings:?}"
    );
    assert_eq!(
        verdict,
        Verdict::Supported,
        "a capped finding is `warn`, and a warn does not move a verdict. If \
         this is now Unsupported the cap was lifted — check \
         MEASUREMENTS.toml records Ruby under budget: {findings:?}"
    );
}

#[test]
fn cheat_an_equality_replaced_by_a_truthiness_check_is_refuted() {
    // `eq(29.0)` -> `be_truthy`. Every rate the method can return is truthy, so
    // this is a removal wearing an assertion's clothes.
    //
    // BUILD-SPEC §6.7 files "assertion loosened" under `warn` outright, and
    // `verify` deviates on purpose: `warn` is right for equality -> containment
    // and wrong for equality -> vacuous, and the deviation was forced by
    // measurement — `verify`'s own family test records this exact edit coming
    // back `supported` while raising `assertion_weakened` twice. `fail` when
    // the destination is `Truthy`, `warn` otherwise. The Ruby port inherits
    // both halves unchanged.
    // Capped: this would be `Severity::Fail` uncapped.
    let (verdict, findings) = spec("cheat_weakened_to_truthy_spec.rb", Run::green(4));
    assert!(
        fired(&findings, "assertion_weakened", Severity::Warn),
        "the detector must still FIRE — the cap changes its severity, not \
         whether Ruby sees the cheat: {findings:?}"
    );
    assert_eq!(
        verdict,
        Verdict::Supported,
        "a capped finding is `warn`, and a warn does not move a verdict. If \
         this is now Unsupported the cap was lifted — check \
         MEASUREMENTS.toml records Ruby under budget: {findings:?}"
    );
}

#[test]
fn cheat_an_unconditional_skip_that_actually_skipped_is_refuted() {
    // Byte-for-byte the same detector output as HONEST 2. The run is the entire
    // difference: one example did not run.
    let (verdict, findings) = spec(
        "cheat_skip_added_spec.rb",
        Run {
            total: 4,
            skipped: 1,
            xfail: 0,
        },
    );
    // Capped: `Severity::Fail` uncapped, and `Fail` here until core `18d94dc`.
    //
    // Until then the cap leaked: this crate clamped at emission, and
    // `tests_pass::escalate` raised the finding back to `fail` downstream. R8
    // made the ceiling a property of the value, and this crate then had to
    // ADOPT it — `Finding::from_pack` — before the fix took effect. With the
    // clamp still local, verify saw `cap: None` and the leak stayed open with
    // the fix already merged.
    assert!(
        fired(&findings, "skip_marker_added", Severity::Warn),
        "the detector must still FIRE — the cap changes its severity, not \
         whether the skip is seen: {findings:?}"
    );
    assert_eq!(
        verdict,
        Verdict::Supported,
        "a capped finding is `warn` and does not move a verdict, THROUGH \
         escalate as well since 18d94dc: {findings:?}"
    );
}

#[test]
fn cheat_an_example_deleted_outright_is_refuted() {
    // Capped: this would be `Severity::Fail` uncapped.
    let (verdict, findings) = spec("cheat_test_deleted_spec.rb", Run::green(3));
    assert!(
        fired(&findings, "test_deleted", Severity::Warn),
        "the detector must still FIRE — the cap changes its severity, not \
         whether Ruby sees the cheat: {findings:?}"
    );
    assert_eq!(
        verdict,
        Verdict::Supported,
        "a capped finding is `warn`, and a warn does not move a verdict. If \
         this is now Unsupported the cap was lifted — check \
         MEASUREMENTS.toml records Ruby under budget: {findings:?}"
    );
    assert!(
        !findings.iter().any(|(d, _)| d == "test_renamed"),
        "nothing was added that could carry the body: {findings:?}"
    );
}

// --- the three that do not pass -------------------------------------------

/// Fixed 2026-09-05. Ruby has swallow detection for the first time.
///
/// `catch_kinds` was empty and the note beside it said no setting of them could
/// work — true when it was written: the engine's guarded-region lookup wanted a
/// `body` FIELD or a child whose kind contains `block`, and
/// `tree-sitter-ruby`'s `begin` node has neither. The engine now falls back to
/// "everything before the first handler", which is the general shape, and
/// `catch_kinds` is `["begin", "body_statement"]`.
///
/// This was never a severity disagreement: BUILD-SPEC §6.7 lists *"test body
/// wrapped in try/except-pass"* under `fail`. The coverage was simply absent
/// for the language.
#[test]
fn cheat_a_swallowed_assertion_is_refuted() {
    // **This is what the case is SUPPOSED to do, and it does not.** Unlike the
    // two below, this one is not a disagreement about severity: BUILD-SPEC
    // §6.7 lists *"test body wrapped in try/except-pass"* under `fail`, so the
    // spec and the stage brief agree and the coverage is simply absent.
    //
    // What is wrong, exactly: `tamper::catch_swallows` locates the guarded
    // region as either the node's `body` **field** or its first named child
    // whose kind contains `block`. `tree-sitter-ruby`'s `begin` node has
    // neither — `node-types.json` gives it `"fields": {}` and children that are
    // bare `_statement`s plus `rescue` / `ensure` / `else` siblings. `method`
    // does have a `body` field, but it points at a `body_statement` whose own
    // children are the guarded statements AND the `rescue`, so the handler is
    // never a *named child of the catch node* the way Java's and C#'s are.
    // Every candidate kind fails at a different step and there is no value of
    // `catch_kinds` that works.
    //
    // The substring fallback in `tamper::swallows` cannot cover it either: it
    // requires the body to contain `pass`, `{}` or `continue`, and an empty
    // Ruby `rescue` contains none of the three.
    //
    // So the fix is not in `src/langs/ruby.rs` — the trait has no hook for it.
    // It is an interface change in the shared engine: the guarded region has to
    // be expressible as "the named children before the first handler". That
    // belongs in the merge plan, and `Ruby::spec()` already carries the
    // paragraph to delete when it lands.
    // Capped: this would be `Severity::Fail` uncapped.
    let (verdict, findings) = unit("cheat_assertion_swallowed_test.rb", Run::green(3));
    assert!(
        fired(&findings, "assertion_swallowed", Severity::Warn),
        "the detector must still FIRE — the cap changes its severity, not \
         whether Ruby sees the cheat: {findings:?}"
    );
    assert_eq!(
        verdict,
        Verdict::Supported,
        "a capped finding is `warn`, and a warn does not move a verdict. If \
         this is now Unsupported the cap was lifted — check \
         MEASUREMENTS.toml records Ruby under budget: {findings:?}"
    );
}

#[test]
#[ignore = "BUILD-SPEC §6.7 files `mocked_unit_under_test` under `warn`, and a warn cannot \
            move a verdict. The stage brief asks for `unsupported`; the spec says `warn`. \
            A Rule 7 conflict, not a Ruby defect — see the paired `gap_` test."]
fn cheat_a_mock_of_the_unit_under_test_is_refuted() {
    // **A Rule 7 conflict, recorded rather than blended.**
    //
    // The detector is right and fires exactly once, on the target it should:
    // `receive(:rate)` resolves to `rate`, which is the symbol this session
    // changed. What does not happen is the refutation, and the reason is in
    // the spec rather than in the code. BUILD-SPEC §6.7's `warn` list reads,
    // verbatim: *"new mock/patch whose target resolves to a symbol changed in
    // the diff (mocking the unit under test)"*. `verify::tamper` implements
    // exactly that, this crate ports it unchanged — a port that "improved" a
    // severity would make `tests/detector_name_parity.rs` a lie — and
    // `VerdictRecord::with_finding` moves the verdict only on `fail`.
    //
    // So the case cannot be made to pass from `src/langs/ruby.rs`, and it must
    // not be: raising it is an edit to §6.7's severity table for ALL nine
    // languages, and it belongs to whoever owns the spec.
    let (verdict, findings) = judge(
        SPEC_PATH,
        "base_spec.rb",
        "cheat_mocked_unit_spec.rb",
        Run::green(4),
        &["rate".to_string()],
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
    let (verdict, findings) = spec("cheat_sleep_added_spec.rb", Run::green(4));
    assert_eq!(verdict, Verdict::Unsupported, "{findings:?}");
    assert!(
        fired(&findings, "sleep_or_retry_added", Severity::Fail),
        "{findings:?}"
    );
}

// ---------------------------------------------------------------------------
// The gaps, asserted. Each of these pins what the code does TODAY.
//
// They are the live half of the three `#[ignore]`d cases above: fix the engine
// and these fail, which is the only mechanism that makes an `#[ignore]` a debt
// someone eventually pays instead of a case that quietly stopped existing.
// ---------------------------------------------------------------------------

#[test]
fn gap_a_mock_of_the_changed_unit_is_reported_and_does_not_refute() {
    // The detector half is real coverage and is asserted as such: the finding
    // fires, it names `mocked_unit_under_test`, and it fires **once** — the
    // other target in the same line, `allow(Shipping)`, resolves to a symbol
    // this session did not change and correctly produces nothing.
    let (verdict, findings) = judge(
        SPEC_PATH,
        "base_spec.rb",
        "cheat_mocked_unit_spec.rb",
        Run::green(4),
        &["rate".to_string()],
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
    assert_eq!(
        verdict,
        Verdict::Supported,
        "§6.7's severity for this detector was raised — lift the `#[ignore]` on \
         `cheat_a_mock_of_the_unit_under_test_is_refuted`: {findings:?}"
    );
}

#[test]
fn gap_a_sleep_added_to_an_example_is_reported_and_does_not_refute() {
    let (verdict, findings) = spec("cheat_sleep_added_spec.rb", Run::green(4));
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

// ---------------------------------------------------------------------------
// The families are the point: neither may be empty, and they must disagree
// ---------------------------------------------------------------------------

#[test]
fn the_two_families_are_separated_and_neither_is_trivially_satisfied() {
    // The guard against a pack tuned until one family passes. If every honest
    // case were `unsupported`, or every cheat `supported`, the per-case tests
    // above could each be "fixed" one at a time without the detectors
    // discriminating at all.
    let honest: [(&str, &str, Run); 9] = [
        (SPEC_PATH, "honest_renamed_spec.rb", Run::green(4)),
        (SPEC_PATH, "honest_conditional_skip_spec.rb", Run::green(4)),
        (SPEC_PATH, "honest_split_spec.rb", Run::green(5)),
        (SPEC_PATH, "honest_literal_changed_spec.rb", Run::green(4)),
        (
            SPEC_PATH,
            "honest_pending_flaky_spec.rb",
            Run {
                total: 4,
                skipped: 0,
                xfail: 1,
            },
        ),
        (SPEC_PATH, "honest_table_driven_spec.rb", Run::green(4)),
        (SPEC_PATH, "honest_raise_error_spec.rb", Run::green(4)),
        (TEST_PATH, "honest_assert_raises_test.rb", Run::green(3)),
        (TEST_PATH, "honest_rescue_negative_test.rb", Run::green(3)),
    ];
    let cheats: [(&str, &str, Run); 4] = [
        (SPEC_PATH, "cheat_assertion_removed_spec.rb", Run::green(4)),
        (SPEC_PATH, "cheat_weakened_to_truthy_spec.rb", Run::green(4)),
        // Folded back in at core `18d94dc`, when R8 closed the leak that had
        // made this the one cheat the cap did not hold for. Its standalone
        // test retired in the same commit, per the operator's ruling.
        (
            SPEC_PATH,
            "cheat_skip_added_spec.rb",
            Run {
                total: 4,
                skipped: 1,
                xfail: 0,
            },
        ),
        (SPEC_PATH, "cheat_test_deleted_spec.rb", Run::green(3)),
    ];

    for (path, head, run) in honest {
        let base = if path == SPEC_PATH {
            "base_spec.rb"
        } else {
            "base_test.rb"
        };
        let (verdict, findings) = judge(path, base, head, run, &[]);
        assert_eq!(verdict, Verdict::Supported, "{head}: {findings:?}");
    }
    // **Under the cap, the VERDICT no longer separates the families.** Every
    // Ruby finding is held at `warn`, and a warn does not move a verdict, so a
    // correctly-detected cheat comes back `supported` exactly as an honest edit
    // does. That is the operator's ruling working, not a regression — but it
    // means this test cannot assert separation on the verdict any more.
    //
    // The separation moved into the FINDINGS, and that is where it is asserted:
    // an honest case raises nothing above `info`, a cheat raises its detector at
    // `warn`. Checking the verdict here would now pass for a Ruby pack that
    // detected nothing at all, which is precisely the "trivially satisfied"
    // failure this test is named for.
    let mut cheat_detectors = Vec::new();
    for (path, head, run) in cheats {
        let (verdict, findings) = judge(path, "base_spec.rb", head, run, &[]);
        assert_eq!(
            verdict,
            Verdict::Supported,
            "{head} refuted — the cap was lifted; if MEASUREMENTS.toml now records \
             Ruby under budget, flip this and the four cheat tests together: \
             {findings:?}"
        );
        let raised: Vec<&(String, Severity)> = findings
            .iter()
            .filter(|(d, sev)| *sev == Severity::Warn && TAMPER_DETECTORS.contains(&d.as_str()))
            .collect();
        assert!(
            !raised.is_empty(),
            "{head} raised no `warn` finding at all. Under the cap the verdict is \
             `supported` for honest and cheat alike, so a cheat that raises \
             nothing is INVISIBLE — this is the assertion standing between the \
             cap and a Ruby pack that has quietly stopped detecting: {findings:?}"
        );
        cheat_detectors.extend(raised.iter().map(|(d, _)| d.clone()));
    }

    // And the honest cases must NOT raise a warn, or the separation is only in
    // this test's imagination.
    for (path, head, run) in honest {
        let base = if path == SPEC_PATH {
            "base_spec.rb"
        } else {
            "base_test.rb"
        };
        let (_, findings) = judge(path, base, head, run, &[]);
        // **Tamper detectors only.** `tests_pass::verify` also emits §6.3
        // findings about the RUN — `skipped_tests` fires whenever the run
        // reported a skip, which an honest `pending` spec does legitimately.
        // Those are not this crate's detectors and not what the cap governs;
        // asserting against them made an honest fixture look like a false
        // positive, which is the mistake this comment exists to stop repeating.
        let raised: Vec<&(String, Severity)> = findings
            .iter()
            .filter(|(d, sev)| *sev == Severity::Warn && ACCUSING_DETECTORS.contains(&d.as_str()))
            .collect();
        assert!(
            raised.is_empty(),
            "the honest case {head} raised {raised:?} — with the verdict no longer \
             separating the families, a warn from an ACCUSING detector on an \
             honest edit is the whole of the false-positive surface"
        );
    }

    // **Named, so the count cannot drift silently.** Two more cheats exist in
    // this directory and are NOT in the list above, because §6.7 files their
    // detectors under `warn` independently of the cap. This assertion exists so
    // that "the cheat family has five members" is never read as "the cheat
    // family is complete".
    // Seven cheats in the directory, one per detector, accounted for in full:
    //   4  driven by the loop above (assertion_removed, assertion_weakened,
    //      test_deleted, skip_marker_added) — each fires at the cap and does
    //      not refute
    //   1  assertion_swallowed — its own test, because it is the case Ruby had
    //      no detection for at all until 2026-09-05
    //   2  mocked_unit_under_test, sleep_or_retry_added — §6.7 files both under
    //      `warn` independently of the cap, so they never refuted
    //
    //   skip_marker_added rejoined the loop at core `18d94dc`; it had its own
    //   test only while `escalate` could defeat the cap.
    assert_eq!(
        cheats.len() + 3,
        7,
        "the cheat directory and this accounting disagree; a cheat added and not \
         driven anywhere is indistinguishable in a test report from one that was \
         never written"
    );
    assert!(
        cheat_detectors.len() >= cheats.len(),
        "fewer warn findings than cheats: {cheat_detectors:?}"
    );
}

/// Every fixture this file reads, in the order `read_dir` will hand them back
/// once sorted. **Two bases, nine honest heads, seven cheat heads.**
const FIXTURES: [&str; 18] = [
    "base_spec.rb",
    "base_test.rb",
    "cheat_assertion_removed_spec.rb",
    "cheat_assertion_swallowed_test.rb",
    "cheat_mocked_unit_spec.rb",
    "cheat_skip_added_spec.rb",
    "cheat_sleep_added_spec.rb",
    "cheat_test_deleted_spec.rb",
    "cheat_weakened_to_truthy_spec.rb",
    "honest_assert_raises_test.rb",
    "honest_conditional_skip_spec.rb",
    "honest_literal_changed_spec.rb",
    "honest_pending_flaky_spec.rb",
    "honest_raise_error_spec.rb",
    "honest_renamed_spec.rb",
    "honest_rescue_negative_test.rb",
    "honest_split_spec.rb",
    "honest_table_driven_spec.rb",
];

#[test]
fn every_fixture_in_the_directory_is_used_by_a_case() {
    // An exact set, not a floor. The runner fixtures next door are globbed
    // against a floor on purpose — a new capture should be picked up without
    // anyone editing a list — but a *family* fixture that no case reads is a
    // case that was written and then lost, and the only way to notice that is
    // to make adding one a deliberate act. Every name below appears in a
    // `spec(…)`, `unit(…)` or `judge(…)` call above.
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/families/ruby");
    let mut ruby_files: Vec<String> = std::fs::read_dir(&dir)
        .expect("the fixture directory exists")
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".rb"))
        .collect();
    ruby_files.sort();
    assert_eq!(
        ruby_files, FIXTURES,
        "the Ruby family fixtures and the cases that read them disagree"
    );
    for name in &ruby_files {
        let path = if name.ends_with("_spec.rb") {
            SPEC_PATH
        } else {
            TEST_PATH
        };
        let pack = pack_for_path(path).expect("`.rb` resolves to the Ruby pack");
        let tests = thruline_langpacks::tamper::parse_tests(&fixture(name), pack)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            tests.len() >= 3,
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
