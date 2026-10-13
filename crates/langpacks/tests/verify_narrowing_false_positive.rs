//! `verify` reports a full Gradle run as narrowed, and refuses its evidence.
//!
//! # The claim, made runnable
//!
//! `verify::tests_pass::NARROWING_FLAGS` is one flat list matched against every
//! argv regardless of which runner produced it. `-t` is in that list because it
//! is pytest's `--tag`-ish spelling and RSpec's `--tag`.
//!
//! For Gradle 9.7.1 on this machine, `gradle -h` says:
//!
//! ```text
//!   --continuous, -t                   Enables continuous build. Gradle does not
//!                                      exit and will re-execute tasks when task
//!                                      file inputs change.
//! ```
//!
//! For Apache Maven 3.9.16, `mvn -h` says:
//!
//! ```text
//!  -t,--toolchains <arg>                   Alternate path for the user
//!                                          toolchains file
//! ```
//!
//! Neither runs a subset of anything. `--continuous` re-runs the *same* set of
//! tasks on file change; `--toolchains` points at a config file.
//!
//! # Why a false positive here is the expensive direction
//!
//! §6.3 makes a narrowed run unable to support a `scope: all` claim. So this is
//! not a cosmetic mislabel: an honest engineer who ran the **entire** suite with
//! `gradle test -t` has that evidence refused, and the claim comes back
//! `unsupported`. `verify`'s own comments record this lesson twice — the bare
//! `cargo test` subcommand once read as a named path, and
//! `@pytest.mark.skipif` once accusing four of five honest edits — each time
//! noting that refusing honest evidence is the costly direction.
//!
//! # Read-only about verify
//!
//! These assertions state what `verify` does today. When it is fixed they fail
//! and say so, and the fix and this file's deletion are one change — recorded
//! in interface request R3.

use thruline_verify::binding::{CheckView, ParsedView, SessionView, SnapshotView};
use thruline_verify::tests_pass::{narrowing_flag, verify, Inputs};
use thruline_verify::verdict::Verdict;

fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|s| (*s).to_string()).collect()
}

/// A session in which the FULL suite ran green on the final tree.
fn full_green_run(runner: &str, command: &[&str]) -> SessionView {
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
            argv: Some(argv(command)),
            exit_code: 0,
            input_tree_hash: "t".into(),
            paths_changed_since_final: None,
            stdio: Some("pipe".into()),
            reexecution: None,
            parsed: Some(ParsedView {
                tests_total: 12,
                passed: 12,
                failed: 0,
                skipped: 0,
                xfail: 0,
            }),
        }],
        head_commit: Some("c".into()),
        capture_degraded: false,
        degraded_reason: None,
    }
}

fn verdict_for(runner: &str, command: &[&str]) -> (Verdict, Vec<String>) {
    let record = verify(
        "clm",
        &full_green_run(runner, command),
        &Inputs {
            scope_is_all: true,
            tamper_findings: vec![],
            source_changed: true,
            test_files_changed: vec![],
            coverage: None,
        },
    );
    let detectors = record.findings.iter().map(|f| f.detector.clone()).collect();
    (record.verdict, detectors)
}

/// The flag-level fact, isolated.
#[test]
fn verify_reads_gradles_continuous_flag_as_narrowing() {
    assert_eq!(
        narrowing_flag(&argv(&["gradle", "test", "-t"])),
        Some("-t".to_string()),
        "verify no longer reads `-t` as narrowing — if `NARROWING_FLAGS` was made \
         runner-aware, this file is obsolete and should go with the fix"
    );
    assert_eq!(
        narrowing_flag(&argv(&["mvn", "-t", "/etc/toolchains.xml", "test"])),
        Some("-t".to_string()),
        "verify no longer reads Maven's `--toolchains` short form as narrowing"
    );
}

/// The consequence: an honest full run is refused.
#[test]
fn an_honest_full_gradle_run_is_reported_narrowed_and_cannot_support_scope_all() {
    let (verdict, detectors) = verdict_for("gradle", &["gradle", "test", "-t"]);
    assert!(
        detectors.iter().any(|d| d == "narrowed_run"),
        "expected a `narrowed_run` finding for `gradle test -t`; got {detectors:?}"
    );
    assert_ne!(
        verdict,
        Verdict::Supported,
        "a full, green, 12-of-12 Gradle run with `-t` (--continuous) is reported \
         {verdict:?}. Twelve tests passed and nothing was skipped; the only thing \
         standing between this evidence and the claim is a flag that means \
         `--continuous`."
    );
}

/// The control: the identical run without `-t` is accepted.
///
/// Without this, the test above would also pass if `verify` refused *every*
/// Gradle run for some unrelated reason — and the finding would be about
/// nothing.
#[test]
fn the_identical_run_without_the_flag_is_supported() {
    let (verdict, detectors) = verdict_for("gradle", &["gradle", "test"]);
    assert_eq!(
        verdict,
        Verdict::Supported,
        "the control run is not supported either ({detectors:?}), so the \
         difference the sibling test attributes to `-t` is not attributable to \
         `-t` at all"
    );
    assert!(
        !detectors.iter().any(|d| d == "narrowed_run"),
        "the control run was also reported narrowed: {detectors:?}"
    );
}

/// `-t` really does narrow for dotnet — so the fix is runner-keying, not deletion.
///
/// `dotnet test --help` on .NET SDK 9.0.317:
/// `-t, --list-tests   List the discovered tests instead of running the tests.`
/// That one genuinely does not run the suite. Deleting `-t` from the flat list
/// would trade this false positive for a false negative.
#[test]
fn the_same_flag_is_a_real_narrowing_for_a_different_runner() {
    assert_eq!(
        narrowing_flag(&argv(&["dotnet", "test", "-t"])),
        Some("-t".to_string()),
        "the flat list no longer fires for dotnet either — check that the \
         replacement is runner-keyed rather than simply shorter"
    );
}
