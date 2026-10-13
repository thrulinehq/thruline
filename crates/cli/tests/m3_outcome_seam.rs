//! M3.3's seam: the re-execution **comparison outcome**, spelled in two places.
//!
//! The outcome set exists twice. `thruline_reexec::Comparison` serialises three
//! of it (`agrees`, `disagrees`, `not_comparable`); `reexec_cmd::record_comparison`
//! writes the fourth (`flaky`) as raw JSON because the enum has no variant for
//! it. `thruline_verify::tests_pass` reads all four back as **strings**, in a
//! `match` whose arms are the only place the two spellings meet.
//!
//! Nothing held them together but a doc comment. This file holds them together
//! with a test, in both directions:
//!
//! 1. **Every outcome the writer can write is one the reader knows.** The set is
//!    *derived* — the three enum variants by serialising them, the raw ones by
//!    reading the literals out of `reexec_cmd.rs` — never transcribed here. A
//!    new variant or a new raw literal that `tests_pass` has not learned turns
//!    this red (A205 doctrine: re-derive, do not transcribe).
//! 2. **An outcome the reader does not know moves the verdict DOWN.** The
//!    `match`'s catch-all was `Severity::Warn`, which moves no verdict, so an
//!    unrecognised outcome left `Supported` standing — the seam failing open.
//!    §6's rule is that `unverifiable` is never collapsed into `supported`;
//!    an outcome nobody can read is the same failure wearing a fourth name.
//!
//! This test lives in `crates/cli` because `cli` is the only crate that can see
//! both spellings: `verify` does not depend on `reexec`, and neither depends on
//! the binary that writes the raw JSON.

use std::collections::BTreeSet;

use thruline_verify::binding::{CheckView, ParsedView, Reexecution, SessionView, SnapshotView};
use thruline_verify::tests_pass::{verify, Inputs};
use thruline_verify::verdict::{Severity, Verdict};

const FINAL_TREE: &str = "cccccccccccccccccccccccccccccccccccccccc";
const UNRECOGNISED: &str = "flakey";

fn parsed() -> ParsedView {
    ParsedView {
        tests_total: 3,
        passed: 3,
        failed: 0,
        skipped: 0,
        xfail: 0,
    }
}

/// The agent's own run: green, bound to the final tree, nothing wrong with it.
fn honest_capture() -> CheckView {
    CheckView {
        id: "chk_captured".into(),
        seq: Some(10),
        category: "test".into(),
        runner: "pytest".into(),
        bypass: None,
        argv: Some(vec!["pytest".into(), "-q".into()]),
        exit_code: 0,
        input_tree_hash: FINAL_TREE.into(),
        paths_changed_since_final: None,
        parsed: Some(parsed()),
        stdio: Some("pipe".into()),
        reexecution: None,
    }
}

/// A re-executed row carrying one comparison outcome, spelled as the store
/// holds it: a string.
fn reexecuted(comparison: &str) -> CheckView {
    CheckView {
        id: "chk_reexecuted".into(),
        seq: Some(20),
        category: "test".into(),
        runner: "pytest".into(),
        bypass: None,
        argv: Some(vec!["pytest".into(), "-q".into()]),
        exit_code: 0,
        input_tree_hash: FINAL_TREE.into(),
        paths_changed_since_final: None,
        parsed: Some(parsed()),
        stdio: Some("pipe".into()),
        reexecution: Some(Reexecution {
            of_check: Some("chk_captured".into()),
            comparison: Some(comparison.to_string()),
            image_digest: "sha256:deadbeef".into(),
        }),
    }
}

fn session(checks: Vec<CheckView>) -> SessionView {
    SessionView {
        session_id: "s".into(),
        snapshots: vec![SnapshotView {
            seq: 30,
            tree_hash: FINAL_TREE.into(),
        }],
        edits: vec![],
        checks,
        head_commit: None,
        capture_degraded: false,
        degraded_reason: None,
    }
}

fn verdict_for(comparison: &str) -> thruline_verify::verdict::VerdictRecord {
    verify(
        "clm_1",
        &session(vec![honest_capture(), reexecuted(comparison)]),
        &Inputs {
            scope_is_all: true,
            ..Inputs::default()
        },
    )
}

/// Every comparison outcome the writer can put in the store, **derived**.
///
/// Two sources, because there are two writers:
///
/// * `thruline_reexec::Comparison` — serialised, not read off the variant
///   names, so a `#[serde(rename)]` is followed rather than guessed at.
/// * `reexec_cmd::record_comparison` — the raw `"outcome": "…"` literals, read
///   out of the source file. `cli` is a binary crate, so its private helper
///   cannot be called from an integration test; the literal is the artefact.
fn outcomes_the_store_can_hold() -> BTreeSet<String> {
    let mut out = BTreeSet::new();

    for variant in [
        thruline_reexec::Comparison::Agrees {
            detail: String::new(),
        },
        thruline_reexec::Comparison::Disagrees {
            detail: String::new(),
        },
        thruline_reexec::Comparison::NotComparable {
            detail: String::new(),
        },
    ] {
        let value = serde_json::to_value(&variant).expect("Comparison serialises");
        let outcome = value
            .get("outcome")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| {
                panic!("`Comparison` no longer serialises with an `outcome` tag: {value}")
            });
        out.insert(outcome.to_string());
    }

    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/reexec_cmd.rs"),
    )
    .expect("reading reexec_cmd.rs");
    let mut raw = 0usize;
    const TAG: &str = "\"outcome\":";
    for (at, _) in source.match_indices(TAG) {
        let rest = &source[at + TAG.len()..];
        let literal = rest
            .split_once('"')
            .and_then(|(_, after)| after.split_once('"'))
            .map(|(value, _)| value.to_string());
        // Skipping a literal we cannot parse would silently shrink the set this
        // test derives, which is the one direction that makes it weaker.
        let literal = literal.unwrap_or_else(|| {
            panic!("an `\"outcome\":` in reexec_cmd.rs is not followed by a string literal")
        });
        raw += 1;
        out.insert(literal);
    }
    assert!(
        raw > 0,
        "no raw `\"outcome\": \"…\"` literal was found in reexec_cmd.rs. Either the raw \
         writer moved — in which case this derivation now reads nothing and would pass \
         whatever the reader knows — or the seam closed and `Comparison` grew the variant."
    );

    // `Comparison` is a closed enum with three variants; a fourth would change
    // the count above without changing this file, and the assertion below is
    // what makes that visible. The number is the count of *derived* spellings,
    // not a list of them: the list is what is being derived.
    assert!(
        out.len() >= 4,
        "derived only {} comparison outcome(s) ({out:?}); the seam has at least four",
        out.len()
    );
    out
}

/// **Direction 1.** Everything the writer can write, the reader recognises.
#[test]
fn every_comparison_outcome_the_store_can_hold_is_one_the_verifier_knows() {
    let mut unknown = Vec::new();
    for outcome in outcomes_the_store_can_hold() {
        let record = verdict_for(&outcome);
        if record
            .findings
            .iter()
            .any(|f| f.detector == "reexecution_outcome_unrecognised")
        {
            unknown.push(outcome);
        }
    }
    assert!(
        unknown.is_empty(),
        "`thruline reexec` can write these comparison outcomes and `tests_pass` does not \
         know how to read them: {unknown:?}. The outcome set is spelled in \
         crates/cli/src/reexec_cmd.rs and matched in crates/verify/src/tests_pass.rs; the \
         two have diverged."
    );
}

/// **Direction 2.** An outcome the reader does not know must move the verdict
/// down, never leave it standing.
///
/// Paired against the same row spelled `agrees` — which *does* reach
/// `Supported` — so this cannot pass because some unrelated clause refused the
/// fixture.
#[test]
fn an_unrecognised_comparison_outcome_is_never_left_standing() {
    let recognised = verdict_for("agrees");
    assert_eq!(
        recognised.verdict,
        Verdict::Supported,
        "the control fixture must reach `supported`, or this test proves nothing about the \
         catch-all: {:?}",
        recognised.reasons
    );

    let record = verdict_for(UNRECOGNISED);
    assert_ne!(
        record.verdict,
        Verdict::Supported,
        "a re-execution comparison outcome this verifier cannot read left the claim \
         `supported`. An unreadable comparison is not agreement: findings={:?}",
        record.findings
    );
    let finding = record
        .findings
        .iter()
        .find(|f| f.detector == "reexecution_outcome_unrecognised")
        .expect("the unrecognised outcome must be named as a finding, not swallowed");
    assert_eq!(
        finding.severity,
        Severity::Fail,
        "the catch-all's severity is what moves the verdict; `Warn` moves nothing and is \
         how this seam failed open"
    );
    assert!(
        finding.detail.contains(UNRECOGNISED),
        "the finding must quote the outcome it could not read, so a reader can act on it: \
         {}",
        finding.detail
    );
}
