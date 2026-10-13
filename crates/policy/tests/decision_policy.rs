//! The PR gate acts only on accepted decisions and an exact-version receipt.

use thruline_policy::decisions::{
    Accepted, DecisionState, GateAction, OnStale, OnUnverifiable, Receipt, ReceiptVerdict,
    RequireHold, Subject,
};
use thruline_policy::{Conclusion, Policy};

fn policy(block: &str) -> Policy {
    Policy::parse(&format!("version: 1\ndecisions:\n{block}")).expect("closed decision policy")
}

fn subject(states: &[DecisionState]) -> Subject {
    let accepted = states
        .iter()
        .enumerate()
        .map(|(index, _)| Accepted {
            id: format!("decision-{index}"),
            revision: 1,
            severity: if index == 0 { "blocks" } else { "warns" }.into(),
            waived: false,
        })
        .collect();
    let verdicts = states
        .iter()
        .enumerate()
        .map(|(index, state)| ReceiptVerdict {
            decision_id: format!("decision-{index}"),
            revision: 1,
            state: *state,
            verdict_id: format!("verdict-{index}"),
        })
        .collect();
    Subject {
        project_id: "project-alpha".into(),
        version_id: "a".repeat(40),
        tree_hash: "b".repeat(40),
        derived_content_sha256: "c".repeat(64),
        accepted,
        receipt: Some(Receipt {
            schema_version: 1,
            project_id: "project-alpha".into(),
            version_id: "a".repeat(40),
            tree_hash: "b".repeat(40),
            derived_content_sha256: "c".repeat(64),
            verdicts,
        }),
    }
}

#[test]
fn parser_is_closed_and_exposes_default_blocking_behavior() {
    let parsed = policy("");
    let settings = parsed.decisions.expect("configured decisions block");
    assert_eq!(settings.require_hold, RequireHold::Blocking);
    assert_eq!(settings.on_unverifiable, OnUnverifiable::Block);
    assert_eq!(settings.on_stale, OnStale::Block);
    let configured =
        policy("  require_hold: all\n  on_unverifiable: warn\n  on_stale: recompute\n");
    let settings = configured.decisions.expect("configured decisions block");
    assert_eq!(settings.require_hold, RequireHold::All);
    assert_eq!(settings.on_unverifiable, OnUnverifiable::Warn);
    assert_eq!(settings.on_stale, OnStale::Recompute);
    for invalid in [
        "  require_hold: maybe\n",
        "  on_unverifiable: allow\n",
        "  on_stale: ignore\n",
        "  inferred_by_model: true\n",
    ] {
        assert!(Policy::parse(&format!("version: 1\ndecisions:\n{invalid}")).is_err());
    }
    assert!(Policy::parse("version: 1\n")
        .expect("legacy policy")
        .decisions
        .is_none());
}

#[test]
fn decisions_gate_settings_are_bound_into_the_policy_digest() {
    let absent = Policy::parse("version: 1\n")
        .expect("legacy policy")
        .digest();
    let defaults = policy("").digest();
    assert_ne!(
        absent, defaults,
        "enabling the Decisions gate changes policy"
    );
    assert_eq!(
        defaults,
        policy("  require_hold: blocking\n  on_unverifiable: block\n  on_stale: block\n").digest(),
        "explicit defaults have the same rules"
    );
    for changed in [
        "  require_hold: all\n",
        "  require_hold: none\n",
        "  on_unverifiable: warn\n",
        "  on_stale: recompute\n",
    ] {
        assert_ne!(defaults, policy(changed).digest(), "{changed}");
    }
}

#[test]
fn blocking_decision_breaks_and_waiver_is_explicit() {
    let settings = policy("").decisions.expect("settings");
    let broken = subject(&[DecisionState::Broken, DecisionState::Broken]);
    let judged = settings.evaluate(&broken);
    assert_eq!(judged.conclusion, Conclusion::Failure);
    assert_eq!(judged.reasons[0].claim.as_deref(), Some("decision-0"));
    assert_eq!(judged.reasons.len(), 1, "warning decision is out of scope");
    let mut waived = broken;
    waived.accepted[0].waived = true;
    assert_eq!(settings.evaluate(&waived).conclusion, Conclusion::Success);
}

#[test]
fn all_policy_includes_warning_decisions_and_none_disables_gate() {
    let all = policy("  require_hold: all\n").decisions.expect("settings");
    let record = subject(&[DecisionState::Holds, DecisionState::Broken]);
    assert_eq!(all.evaluate(&record).conclusion, Conclusion::Failure);
    let none = policy("  require_hold: none\n")
        .decisions
        .expect("settings");
    assert_eq!(none.evaluate(&record).conclusion, Conclusion::Success);
}

#[test]
fn missing_unverifiable_and_stale_receipts_never_become_holds() {
    let strict = policy("").decisions.expect("settings");
    let mut missing = subject(&[DecisionState::Holds]);
    missing.receipt = None;
    assert_eq!(strict.evaluate(&missing).conclusion, Conclusion::Failure);
    let unknown = subject(&[DecisionState::Unverifiable]);
    assert_eq!(strict.evaluate(&unknown).conclusion, Conclusion::Failure);
    let warn = policy("  on_unverifiable: warn\n")
        .decisions
        .expect("settings");
    assert_eq!(warn.evaluate(&unknown).conclusion, Conclusion::Neutral);
    let mut stale = subject(&[DecisionState::Holds]);
    stale.receipt.as_mut().expect("receipt").tree_hash = "d".repeat(40);
    assert_eq!(strict.evaluate(&stale).conclusion, Conclusion::Failure);
    let recompute = policy("  on_stale: recompute\n")
        .decisions
        .expect("settings");
    let judged = recompute.evaluate(&stale);
    assert_eq!(judged.conclusion, Conclusion::Failure);
    assert_eq!(judged.action, GateAction::Recompute);
}

#[test]
fn receipt_must_match_derived_set_and_exact_version() {
    let settings = policy("").decisions.expect("settings");
    let mut record = subject(&[DecisionState::Holds, DecisionState::Holds]);
    record.receipt.as_mut().expect("receipt").verdicts.pop();
    assert_eq!(settings.evaluate(&record).conclusion, Conclusion::Failure);
    record = subject(&[DecisionState::Holds]);
    record.receipt.as_mut().expect("receipt").verdicts[0].revision = 2;
    assert_eq!(settings.evaluate(&record).conclusion, Conclusion::Failure);
    record = subject(&[DecisionState::Holds]);
    record
        .receipt
        .as_mut()
        .expect("receipt")
        .derived_content_sha256 = "d".repeat(64);
    assert_eq!(settings.evaluate(&record).conclusion, Conclusion::Failure);
    record = subject(&[DecisionState::Holds]);
    record.receipt.as_mut().expect("receipt").project_id = "other-project".into();
    assert_eq!(settings.evaluate(&record).conclusion, Conclusion::Failure);
}
