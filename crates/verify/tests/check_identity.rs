//! What a check's *identity* is allowed to be — BUILD-SPEC §6.1, §6.3.
//!
//! `CheckView` used to carry `seq: u64` and `argv: Vec<String>`, so a caller
//! with no record of either had to invent one. It invented `u64::MAX` and `[]`,
//! and both invented values read as agreement:
//!
//! - `u64::MAX` sorts after every edit, so §6.1 rule 2's observed half skipped
//!   the edit record entirely and reported "nothing relevant changed";
//! - `[]` carries no narrowing flag, so §6.3's narrowing detector reported
//!   nothing about a `pytest -k` run.
//!
//! Both are `Option` now, so the absence is representable and every reader has
//! to decide what to do about it. These tests fix what that decision is.

use thruline_verify::binding::{
    bind, Binding, CheckView, EditView, ParsedView, SessionView, SnapshotView,
};
use thruline_verify::tests_pass::{verify, Inputs};
use thruline_verify::verdict::{Severity, Verdict};

fn parsed() -> Option<ParsedView> {
    Some(ParsedView {
        tests_total: 4,
        passed: 4,
        failed: 0,
        skipped: 0,
        xfail: 0,
    })
}

fn check(seq: Option<u64>, tree: &str, argv: Option<&[&str]>) -> CheckView {
    CheckView {
        id: format!("chk_{}", tree),
        seq,
        category: "test".into(),
        runner: "pytest".into(),
        bypass: None,
        argv: argv.map(|a| a.iter().map(|s| (*s).to_string()).collect()),
        exit_code: 0,
        input_tree_hash: tree.into(),
        // The tree half sees only a test-file change: on its own it would let
        // the check bind, which is what makes the observed half decisive here.
        paths_changed_since_final: Some(vec!["tests/test_app.py".into()]),
        stdio: Some("pipe".into()),
        reexecution: None,
        parsed: parsed(),
    }
}

/// Two snapshots, and one non-test edit sitting between them.
fn session(checks: Vec<CheckView>) -> SessionView {
    SessionView {
        session_id: "s".into(),
        snapshots: vec![
            SnapshotView {
                seq: 10,
                tree_hash: "aaa".into(),
            },
            SnapshotView {
                seq: 40,
                tree_hash: "zzz".into(),
            },
        ],
        edits: vec![EditView {
            seq: 20,
            // Ignored by git, so no tree diff can ever name it. The observed
            // half is the only thing that can see this write.
            path: "vendor/sitecustomize.py".into(),
        }],
        checks,
        head_commit: Some("c0ffee".into()),
        capture_degraded: false,
        degraded_reason: None,
    }
}

// ---------------------------------------------------------------------------
// §6.1 — a position is a recorded fact or it is nothing
// ---------------------------------------------------------------------------

/// The control: a check whose position the record holds is ordered by it.
#[test]
fn a_check_the_record_places_is_ordered_against_the_edits() {
    let s = session(vec![check(Some(10), "aaa", Some(&["pytest", "-q"]))]);
    assert_eq!(
        bind(&s, &s.checks[0]),
        Binding::SupersededByEdit {
            path: "vendor/sitecustomize.py".into(),
            seq: 20,
        }
    );
}

/// The same check, with no recorded position, is **not** thereby last.
///
/// The old sentinel made this case identical to "ran after every edit", which
/// is the strongest possible reading of a fact that is simply absent.
#[test]
fn a_check_the_record_does_not_place_is_not_treated_as_the_latest_one() {
    let s = session(vec![check(None, "aaa", Some(&["pytest", "-q"]))]);
    let binding = bind(&s, &s.checks[0]);
    assert!(
        matches!(binding, Binding::Unobservable { .. }),
        "expected a refusal, got {binding:?}"
    );
    assert!(!binding.counts(), "{binding:?}");
    // And the refusal is `unverifiable`, never `unsupported`: nothing here
    // contradicts the claim, and §6 forbids the collapse in both directions.
    let v = verify(
        "clm",
        &s,
        &Inputs {
            scope_is_all: true,
            ..Inputs::default()
        },
    );
    assert_eq!(v.verdict, Verdict::Unverifiable, "{v:#?}");
}

/// Rule 1 still short-circuits: a check that ran on the final tree needs no
/// position at all, which is the ordinary case for a re-execution.
#[test]
fn a_check_on_the_final_tree_needs_no_position() {
    let s = session(vec![check(None, "zzz", Some(&["pytest", "-q"]))]);
    assert_eq!(bind(&s, &s.checks[0]), Binding::RanOnFinalTree);
    let v = verify(
        "clm",
        &s,
        &Inputs {
            scope_is_all: true,
            ..Inputs::default()
        },
    );
    assert_eq!(v.verdict, Verdict::Supported, "{v:#?}");
}

/// Two checks with unknown positions are not thereby simultaneous — each is
/// ordered by the tree the record says it ran against.
#[test]
fn two_checks_anchored_to_different_trees_are_distinguishable() {
    let mut s = session(vec![]);
    s.snapshots.push(SnapshotView {
        seq: 30,
        tree_hash: "mmm".into(),
    });
    // One anchored before the edit, one after it.
    let early = CheckView {
        seq: s.first_snapshot_seq_of("aaa"),
        ..check(None, "aaa", Some(&["pytest", "-q"]))
    };
    let late = CheckView {
        seq: s.first_snapshot_seq_of("mmm"),
        ..check(None, "mmm", Some(&["pytest", "-q"]))
    };
    assert_eq!(early.seq, Some(10));
    assert_eq!(late.seq, Some(30));
    assert!(matches!(bind(&s, &early), Binding::SupersededByEdit { .. }));
    assert!(matches!(
        bind(&s, &late),
        Binding::NoRelevantChangeSince { .. }
    ));
}

/// A tree the chain never recorded anchors nothing — no nearest-neighbour
/// guess, no "probably the last one".
#[test]
fn a_tree_the_chain_never_recorded_anchors_nothing() {
    let s = session(vec![]);
    assert_eq!(s.first_snapshot_seq_of("not-a-tree-in-this-session"), None);
    assert_eq!(s.first_snapshot_seq_of(""), None);
}

// ---------------------------------------------------------------------------
// §6.3 — a command the record does not hold is not a command with no flags
// ---------------------------------------------------------------------------

/// The control: a full run's command is read, and no narrowing is found.
#[test]
fn a_recorded_full_command_raises_nothing() {
    let s = session(vec![check(Some(40), "zzz", Some(&["pytest", "-q"]))]);
    let v = verify(
        "clm",
        &s,
        &Inputs {
            scope_is_all: true,
            ..Inputs::default()
        },
    );
    assert_eq!(v.verdict, Verdict::Supported, "{v:#?}");
    assert!(v.findings.is_empty(), "{v:#?}");
}

/// An **unrecorded** command produces a finding, not the same silence.
#[test]
fn an_unrecorded_command_is_not_read_as_a_full_suite_run() {
    for argv in [None, Some(&[][..])] {
        let s = session(vec![CheckView {
            argv: argv.map(|a: &[&str]| a.iter().map(|s| (*s).to_string()).collect()),
            ..check(Some(40), "zzz", None)
        }]);
        let v = verify(
            "clm",
            &s,
            &Inputs {
                scope_is_all: true,
                ..Inputs::default()
            },
        );
        let found: Vec<&str> = v.findings.iter().map(|f| f.detector.as_str()).collect();
        assert!(
            found.contains(&"narrowing_unchecked"),
            "argv {argv:?} produced {found:?}"
        );
        assert_eq!(
            v.verdict,
            Verdict::Unverifiable,
            "a `scope: all` claim whose narrowing clause was never evaluated must \
             not be supported: {v:#?}"
        );
        // `unverifiable`, not `unsupported` — the finding is a `warn`, because
        // nothing here contradicts the claim.
        assert!(v
            .findings
            .iter()
            .any(|f| f.detector == "narrowing_unchecked" && f.severity == Severity::Warn));
    }
}

/// A narrower claim is unaffected: §6.3 permits a narrowed run for
/// `scope: named`, so an unknown command cannot cost anything there.
#[test]
fn an_unrecorded_command_costs_nothing_when_the_claim_was_not_about_everything() {
    let s = session(vec![check(Some(40), "zzz", None)]);
    let v = verify(
        "clm",
        &s,
        &Inputs {
            scope_is_all: false,
            ..Inputs::default()
        },
    );
    assert_eq!(v.verdict, Verdict::Supported, "{v:#?}");
}

/// Two runs whose commands are unrecorded are **not** the same command, so
/// they cannot make each other flaky.
#[test]
fn unrecorded_commands_are_not_compared_for_flakiness() {
    let mut a = check(Some(40), "zzz", None);
    a.id = "chk_a".into();
    let mut b = check(Some(40), "zzz", None);
    b.id = "chk_b".into();
    b.parsed = Some(ParsedView {
        tests_total: 4,
        passed: 1,
        failed: 0,
        skipped: 3,
        xfail: 0,
    });
    let s = session(vec![a, b]);
    let v = verify(
        "clm",
        &s,
        &Inputs {
            scope_is_all: false,
            ..Inputs::default()
        },
    );
    let found: Vec<&str> = v.findings.iter().map(|f| f.detector.as_str()).collect();
    assert!(
        !found.contains(&"nondeterministic"),
        "two checks with no recorded command are not known to be the same command: \
         {v:#?}"
    );
}

/// `command()` is for humans and says so, rather than rendering an absence as
/// an empty string a detector might match on.
#[test]
fn an_unrecorded_command_renders_as_an_absence() {
    let c = check(Some(1), "aaa", None);
    assert_eq!(c.command(), "<the command was not recorded>");
    assert_eq!(
        check(Some(1), "aaa", Some(&["pytest", "-q"])).command(),
        "pytest -q"
    );
}
