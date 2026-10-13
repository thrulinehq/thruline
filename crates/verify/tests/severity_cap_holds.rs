//! R8 — a capped finding cannot be raised past its cap by any site in verify's
//! severity path, in any order.
//!
//! # The defect this pins
//!
//! Operator ruling, 2026-09-06: *"take G's structural shape. Every finding
//! carries its language, and every severity-raising site consults the cap —
//! enforced by the type, not by emission order. A cap that depends on which
//! crate runs first is a cap that will be silently lost on the next
//! refactor."*
//!
//! Track G capped Ruby's findings at `warn` at the point they are emitted,
//! under the earlier ruling that a detector over its ambiguity budget "may
//! inform; it may not accuse". Within the hour its own family suite reported:
//!
//! ```text
//! cheat_skip_added_spec.rb refuted — findings
//!   [("skipped_tests", Warn), ("skip_marker_added", Fail)]
//!   verdict: Unsupported
//! ```
//!
//! The finding left the pack at `warn`; `tests_pass::escalate` raised it. A cap
//! applied at emission cannot survive a raise downstream of it.
//!
//! # What is asserted, and what is not
//!
//! Both raising sites, in both directions, with an uncapped control beside each
//! so a green run cannot mean the engine was disarmed. Plus the order the
//! second site's safety currently rests on — G's ordering test observes that
//! `claims_cmd` computes `test_side_failed` from pre-escalation findings — is
//! here made *not to matter*: the cap holds whichever order the two sites run
//! in, which is the half of the ruling that retires G's test.
//!
//! Not asserted here: that nothing anywhere assigns `Finding::severity`
//! directly. That is a source census, and it is the last case in this file.

use std::collections::BTreeSet;

use thruline_verify::binding::{CheckView, EditView, ParsedView, SessionView, SnapshotView};
use thruline_verify::special_casing::escalate_with_test_findings;
use thruline_verify::tests_pass::{verify, Inputs};
use thruline_verify::verdict::{Finding, Severity, Verdict};

/// A pack under a ceiling, the way Ruby is: `warn`, never `fail`.
fn capped(detector: &str) -> Finding {
    Finding::new(Severity::Warn, detector, "a capped pack noticed something")
        .at("spec/widget_spec.rb")
        .from_pack("ruby", Some(Severity::Warn))
}

/// The same finding from a pack under no ceiling. Every capped assertion below
/// is paired with this so "nothing reached fail" cannot mean "nothing works".
fn uncapped(detector: &str) -> Finding {
    Finding::new(
        Severity::Warn,
        detector,
        "an uncapped pack noticed something",
    )
    .at("tests/test_widget.py")
    .from_pack("python", None)
}

fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|s| (*s).to_string()).collect()
}

/// A session whose run left tests unrun — the condition `escalate` raises on.
fn session_with_skips(skipped: u64) -> SessionView {
    SessionView {
        session_id: "s".into(),
        snapshots: vec![
            SnapshotView {
                seq: 10,
                tree_hash: "aaa".into(),
            },
            SnapshotView {
                seq: 30,
                tree_hash: "bbb".into(),
            },
        ],
        edits: vec![EditView {
            seq: 12,
            path: "src/app.rb".into(),
        }],
        checks: vec![CheckView {
            id: "chk_20".into(),
            seq: Some(20),
            category: "test".to_string(),
            runner: "rspec".to_string(),
            bypass: None,
            argv: Some(argv(&["rspec"])),
            exit_code: 0,
            input_tree_hash: "bbb".to_string(),
            paths_changed_since_final: None,
            stdio: Some("pipe".into()),
            reexecution: None,
            parsed: Some(ParsedView {
                tests_total: 4,
                passed: 4,
                failed: 0,
                skipped,
                xfail: 0,
            }),
        }],
        head_commit: Some("c0ffee".into()),
        capture_degraded: false,
        degraded_reason: None,
    }
}

fn severity_of(record: &thruline_verify::verdict::VerdictRecord, detector: &str) -> Severity {
    record
        .findings
        .iter()
        .find(|f| f.detector == detector)
        .unwrap_or_else(|| panic!("{detector} is not in {:#?}", record.findings))
        .severity
}

// ---------------------------------------------------------------------------
// Site 1 — tests_pass::escalate, the site R8 names
// ---------------------------------------------------------------------------

#[test]
fn a_capped_skip_marker_cannot_reach_fail_through_the_run_escalation() {
    let session = session_with_skips(3);
    let inputs = Inputs {
        scope_is_all: true,
        tamper_findings: vec![capped("skip_marker_added")],
        ..Inputs::default()
    };
    let v = verify("clm_1", &session, &inputs);

    assert_eq!(
        severity_of(&v, "skip_marker_added"),
        Severity::Warn,
        "a capped finding was raised past its ceiling by the run escalation"
    );
    // And the consequence the ruling is actually about: it must not refute.
    assert_ne!(
        v.verdict,
        Verdict::Unsupported,
        "a detector ruled ineligible to accuse refuted a claim: {v:#?}"
    );
}

#[test]
fn the_control_an_uncapped_skip_marker_still_reaches_fail() {
    // Without this, the test above passes just as well on an engine that has
    // stopped escalating anything at all.
    let session = session_with_skips(3);
    let inputs = Inputs {
        scope_is_all: true,
        tamper_findings: vec![uncapped("skip_marker_added")],
        ..Inputs::default()
    };
    let v = verify("clm_1", &session, &inputs);

    assert_eq!(severity_of(&v, "skip_marker_added"), Severity::Fail);
    assert_eq!(v.verdict, Verdict::Unsupported, "{v:#?}");
}

#[test]
fn the_escalations_lowering_arm_is_not_a_cap_and_still_lowers() {
    // A cap is a ceiling, never a floor. With no skipped tests the marker
    // "left nothing unrun" and the finding drops to info — below the cap, and
    // it must stay there.
    let session = session_with_skips(0);
    let inputs = Inputs {
        scope_is_all: true,
        tamper_findings: vec![capped("skip_marker_added")],
        ..Inputs::default()
    };
    let v = verify("clm_1", &session, &inputs);
    assert_eq!(severity_of(&v, "skip_marker_added"), Severity::Info);
}

// ---------------------------------------------------------------------------
// Site 2 — special_casing::escalate_with_test_findings, the site R8's title
// covers and its body did not name
// ---------------------------------------------------------------------------

#[test]
fn a_capped_six_eight_finding_cannot_reach_fail_through_the_test_side_escalation() {
    let out = escalate_with_test_findings(vec![capped("branch_on_test_literal")], true);
    assert_eq!(out[0].severity, Severity::Warn, "{out:#?}");
    assert_eq!(out[0].severity_cap(), Some(Severity::Warn));
    // The escalation still happened — the sentence is appended — it just did
    // not carry the severity with it. A silent no-op would be a different bug.
    assert!(
        out[0].detail.contains("§6.8's escalation condition"),
        "{out:#?}"
    );
}

#[test]
fn the_control_an_uncapped_six_eight_finding_still_reaches_fail() {
    let out = escalate_with_test_findings(vec![uncapped("branch_on_test_literal")], true);
    assert_eq!(out[0].severity, Severity::Fail, "{out:#?}");

    let untouched = escalate_with_test_findings(vec![uncapped("branch_on_test_literal")], false);
    assert_eq!(untouched[0].severity, Severity::Warn, "{untouched:#?}");
}

// ---------------------------------------------------------------------------
// The half that retires G's ordering test: the order stops mattering
// ---------------------------------------------------------------------------

#[test]
fn neither_order_of_the_two_sites_can_lift_a_capped_finding() {
    // G's `the_special_casing_escalation_reads_pre_cap_findings_and_must_keep_
    // doing_so` observes that `claims_cmd` computes `test_side_failed` from
    // findings as emitted, so a capped Ruby finding cannot make it true. That
    // is an ordering, and the ruling says a cap may not rest on one. Here the
    // *worst* order is run deliberately — the run escalation first, then the
    // test-side escalation over its output with `test_side_failed` forced true
    // — which is exactly the refactor G warned would turn one leak into a
    // class of them.
    let session = session_with_skips(3);
    let inputs = Inputs {
        scope_is_all: true,
        tamper_findings: vec![capped("skip_marker_added")],
        ..Inputs::default()
    };
    let after_run_escalation: Vec<Finding> = verify("clm_1", &session, &inputs)
        .findings
        .into_iter()
        .filter(|f| f.detector == "skip_marker_added")
        .collect();
    assert_eq!(after_run_escalation.len(), 1);

    let both_ways = escalate_with_test_findings(after_run_escalation, true);
    assert_eq!(
        both_ways[0].severity,
        Severity::Warn,
        "two escalations in sequence lifted a capped finding: {both_ways:#?}"
    );

    // And the reverse order, for the same reason.
    let test_side_first = escalate_with_test_findings(vec![capped("skip_marker_added")], true);
    let inputs = Inputs {
        scope_is_all: true,
        tamper_findings: test_side_first,
        ..Inputs::default()
    };
    let v = verify("clm_1", &session, &inputs);
    assert_eq!(
        severity_of(&v, "skip_marker_added"),
        Severity::Warn,
        "{v:#?}"
    );
}

#[test]
fn a_cap_cannot_be_widened_after_the_fact() {
    // `from_pack` clamps on the way in, so a pack cannot emit above its own
    // ceiling either — the cap is not only consulted on the way up.
    let born_high = Finding::new(Severity::Fail, "skip_marker_added", "d")
        .from_pack("ruby", Some(Severity::Warn));
    assert_eq!(born_high.severity, Severity::Warn);

    // And `raised_to` never lowers: a site asking for `info` on a `warn`
    // finding does not get to demote it under the guise of a raise.
    let asked_down = capped("skip_marker_added").raised_to(Severity::Info);
    assert_eq!(asked_down.severity, Severity::Warn);
}

// ---------------------------------------------------------------------------
// The census: the one hole the type cannot close
// ---------------------------------------------------------------------------

/// `Finding::severity` is a public field, so `f.severity = Severity::Fail`
/// compiles and skips the cap entirely.
///
/// It stays public for one measured reason: two `#[cfg(test)]` lines in
/// `crates/langpacks` (`langs/csharp.rs`, `langs/go.rs`) read it as a field,
/// and that crate is Track G's lane, which the core does not edit. Making it
/// private is a one-line change in each and is filed for G rather than done
/// here.
///
/// Until then the hole is watched rather than regretted. This censuses the
/// crates the core owns, in the shape the rest of this program uses for the
/// same job: derive the search space from the source, and fail on a new
/// occurrence rather than trusting review to notice one.
#[test]
fn no_site_outside_verdict_rs_writes_a_severity_directly() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root")
        .to_path_buf();

    // Assembled rather than written out, so this file does not match itself.
    let assignment = format!("{}{}", ".severity ", "= ");
    let literal = format!("{} {}", "Finding {", "severity:");

    let mut offenders: BTreeSet<String> = BTreeSet::new();
    let mut scanned = 0usize;
    for dir in ["crates/verify/src", "crates/cli/src"] {
        let mut stack = vec![root.join(dir)];
        while let Some(d) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().is_none_or(|x| x != "rs") {
                    continue;
                }
                // The one file allowed to set a severity: the type's own.
                if p.ends_with("verify/src/verdict.rs") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&p) else {
                    continue;
                };
                scanned += 1;
                for (i, line) in text.lines().enumerate() {
                    let bare = line.trim_start();
                    if bare.starts_with("//") || bare.starts_with("///") {
                        continue;
                    }
                    if line.contains(&assignment) || line.contains(&literal) {
                        offenders.insert(format!(
                            "{}:{}: {}",
                            p.strip_prefix(&root).unwrap_or(&p).display(),
                            i + 1,
                            bare.trim()
                        ));
                    }
                }
            }
        }
    }

    assert!(
        scanned > 20,
        "the census read only {scanned} files — it is searching the wrong place, \
         which is how a census passes forever"
    );
    assert!(
        offenders.is_empty(),
        "a severity is set outside `Finding`'s own module, which skips the pack \
         cap that `raised_to` consults:\n  {}\n\nRaise through \
         `Finding::raised_to`, which clamps to the emitting pack's ceiling.",
        offenders.into_iter().collect::<Vec<_>>().join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// R9 — the accessor lands before the field closes
// ---------------------------------------------------------------------------

#[test]
fn the_accessor_and_the_field_are_the_same_value() {
    // The whole point of adding `severity()` beside a public field is that a
    // caller can migrate without a behaviour change. If the two could differ,
    // the migration would be a rewrite.
    for f in [
        capped("skip_marker_added"),
        uncapped("skip_marker_added"),
        Finding::new(Severity::Fail, "narrowed_run", "d"),
        Finding::new(Severity::Info, "x", "d").raised_to(Severity::Warn),
        Finding::new(Severity::Fail, "y", "d").lowered_to(Severity::Info),
    ] {
        assert_eq!(f.severity(), f.severity, "{f:?}");
    }
}
