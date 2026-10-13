//! T-3-5 (`B7.6`–`B7.12`, `Q1`, `Q3`, lens rule 5) — "Ready for approval",
//! and every way it must refuse.
//!
//! The substance is `every_unsatisfying_class_refuses`: a table with one row
//! per unsatisfying class of `B7.6`, each built on the same fixture Change and
//! each asserting not-ready **with the right reason slug**, so a class that
//! starts reading as a pass fails here rather than on a screen.
//!
//! Every fixture is synthetic. No string here came off a real machine.

use thruline_core::change_evidence::{
    CandidateView, CoverageState, CoverageView, CriterionCoverage, EvidenceItemView, EvidenceKind,
    LinkBasis, Observed, RunClass,
};
use thruline_core::change_readiness::{
    compute, policy_requires_review, ObligationWaiver, Outcome, ReadinessInput, Unsatisfied,
    WaiverKind, NON_WAIVABLE,
};
use thruline_core::run_execution::ExecutionState;

/// One linked piece of evidence that satisfies its line: bearing, proven,
/// about this version, not stale, and an execution that reached the end.
fn admissible_item() -> EvidenceItemView {
    EvidenceItemView {
        link_id: "l1".into(),
        kind: EvidenceKind::CheckRun,
        evidence_id: "e1".into(),
        session_id: "s1".into(),
        basis: LinkBasis::Declared,
        basis_slug: LinkBasis::Declared.label_slug(),
        detail: None,
        actor: None,
        asserted_at: "2026-09-22T10:00:00Z".into(),
        reason: None,
        name: "cargo clippy".into(),
        observed: Observed::Proven,
        bears: true,
        note_slug: None,
        applies: Some(true),
        applies_slug: Some("inputs_applies"),
        fresh: Some(true),
        fresh_slug: Some("applicability_still_applies"),
        assessable: None,
        execution: ExecutionState::Completed,
        execution_slug: ExecutionState::Completed.slug(),
        // T-3-6-F1/F3 added these three beside the item; a readiness fixture
        // holds no count and no failure line, which is what an ordinary run
        // that simply passed looks like.
        failure_line: None,
        evidence_class: None,
        evidence_class_slug: None,
    }
}

fn criterion(items: Vec<EvidenceItemView>, state: CoverageState) -> CriterionCoverage {
    CriterionCoverage {
        index: 1,
        text: "The totals add up".into(),
        class: RunClass::Build,
        state,
        state_slug: state.slug(),
        absent_tool: None,
        items,
        removed: Vec::new(),
    }
}

fn view(criteria: Vec<CriterionCoverage>, scope_version: Option<u32>) -> CoverageView {
    CoverageView {
        schema_version: 1,
        change_id: "ch1".into(),
        scope_version,
        criteria,
        candidates: Vec::<CandidateView>::new(),
        executions: Vec::new(),
        link_rows: 1,
        nothing_to_link_slug: None,
        nothing_bound_slug: None,
        execution_sessions: Vec::new(),
        masking: Default::default(),
    }
}

fn input<'a>(coverage: &'a CoverageView, waivers: &'a [ObligationWaiver]) -> ReadinessInput<'a> {
    ReadinessInput {
        change_id: "ch1",
        coverage,
        target_version: Some("e71c5b0aaaa".into()),
        has_examined_inputs: true,
        review_required: false,
        review_recorded: false,
        waivers,
    }
}

/// The one shape that is ready, kept in one place so every refusal below is a
/// single-field difference from it.
fn ready_view() -> CoverageView {
    view(
        vec![criterion(vec![admissible_item()], CoverageState::Evidence)],
        Some(1),
    )
}

#[test]
fn the_fully_satisfied_case_is_ready_and_says_what_it_rests_on() {
    let coverage = ready_view();
    let answer = compute(&input(&coverage, &[]));
    assert!(
        answer.ready,
        "every obligation is satisfied by one admissible item"
    );
    assert_eq!(answer.headline_slug, "readiness_ready");
    assert_eq!(answer.target_version.as_deref(), Some("e71c5b0aaaa"));
    // Lens rule 5: it LISTS what it rests on, and the list is not empty.
    assert_eq!(answer.rests_on.len(), 1);
    assert_eq!(answer.rests_on[0].text, "The totals add up");
    assert_eq!(
        answer.rests_on[0].evidence_name.as_deref(),
        Some("cargo clippy")
    );
    assert!(answer.missing.is_empty());
}

#[test]
fn a_ready_answer_never_has_an_empty_rests_on_list() {
    // The invariant lens rule 5 turns on, asserted as a property rather than
    // as one case: if it is ready, it names at least one thing it rests on.
    let coverage = ready_view();
    let answer = compute(&input(&coverage, &[]));
    assert!(!(answer.ready && answer.rests_on.is_empty()));
}

#[test]
fn an_empty_criteria_set_is_never_ready() {
    // `B7.7`: no all-clear over an empty set, in either of the two ways a set
    // can be empty.
    for coverage in [view(Vec::new(), Some(1)), view(Vec::new(), None)] {
        let answer = compute(&input(&coverage, &[]));
        assert!(!answer.ready, "nothing accepted can never be ready");
        assert_eq!(answer.headline_slug, "readiness_nothing_accepted");
        assert!(answer.rests_on.is_empty());
    }
    // And a Change with criteria but no accepted scope version is the same.
    let coverage = view(
        vec![criterion(vec![admissible_item()], CoverageState::Evidence)],
        None,
    );
    let answer = compute(&input(&coverage, &[]));
    assert!(!answer.ready);
    assert_eq!(answer.headline_slug, "readiness_nothing_accepted");
}

#[test]
fn every_unsatisfying_class_refuses_with_its_own_reason() {
    // `B7.6`, one row per class. Each is the ready fixture with ONE field
    // changed, so nothing but that field can be what refused it.
    let mut stopped = admissible_item();
    stopped.execution = ExecutionState::Interrupted;
    let mut cancelled = admissible_item();
    cancelled.execution = ExecutionState::Cancelled;
    let mut blocked = admissible_item();
    blocked.execution = ExecutionState::Blocked;
    let mut running = admissible_item();
    running.execution = ExecutionState::Running;
    // **Verifier finding 2, as a table row.** The shape a stopped run REALLY
    // has once the derivation has seen it: it settled nothing, so it does not
    // bear, so `coverage()` calls the line `RanNothing` and never `Evidence`.
    // The four rows above hold the state hand-built; these two hold the state
    // the engine actually derives, and `a_stopped_check_is_named_as_stopped`
    // below builds them on a real ledger rather than by hand.
    let mut cancelled_as_derived = admissible_item();
    cancelled_as_derived.execution = ExecutionState::Cancelled;
    cancelled_as_derived.bears = false;
    cancelled_as_derived.observed = Observed::Couldnt;
    cancelled_as_derived.applies = None;
    cancelled_as_derived.applies_slug = None;
    let mut interrupted_as_derived = cancelled_as_derived.clone();
    interrupted_as_derived.execution = ExecutionState::Interrupted;
    let mut stale = admissible_item();
    stale.fresh = Some(false);
    let mut not_proven = admissible_item();
    not_proven.observed = Observed::NotProven;
    let mut unknown = admissible_item();
    unknown.observed = Observed::Couldnt;
    let mut other_version = admissible_item();
    other_version.applies = Some(false);
    let mut never_asked = admissible_item();
    never_asked.applies = None;
    let mut checked_nothing = admissible_item();
    checked_nothing.bears = false;
    let mut a_claim = admissible_item();
    a_claim.kind = EvidenceKind::Finding;
    a_claim.observed = Observed::Loose;

    let cases: Vec<(&str, CriterionCoverage, Unsatisfied)> = vec![
        (
            "nothing linked at all",
            criterion(Vec::new(), CoverageState::None),
            Unsatisfied::NothingLooked,
        ),
        (
            "the check cannot run here",
            criterion(Vec::new(), CoverageState::AbsentTool),
            Unsatisfied::ToolAbsent,
        ),
        (
            "something ran and checked nothing",
            criterion(vec![checked_nothing], CoverageState::RanNothing),
            Unsatisfied::SettledNothing,
        ),
        (
            "everything was about another version",
            criterion(vec![other_version], CoverageState::OtherVersion),
            Unsatisfied::OtherVersion,
        ),
        (
            "nobody asked which version it was about",
            criterion(vec![never_asked], CoverageState::Evidence),
            Unsatisfied::OtherVersion,
        ),
        (
            "interrupted",
            criterion(vec![stopped], CoverageState::Evidence),
            Unsatisfied::Stopped,
        ),
        (
            "cancelled",
            criterion(vec![cancelled], CoverageState::Evidence),
            Unsatisfied::Stopped,
        ),
        (
            "blocked",
            criterion(vec![blocked], CoverageState::Evidence),
            Unsatisfied::Stopped,
        ),
        (
            "still running",
            criterion(vec![running], CoverageState::Evidence),
            Unsatisfied::Stopped,
        ),
        (
            "cancelled, as the derivation really shapes it",
            criterion(vec![cancelled_as_derived], CoverageState::RanNothing),
            Unsatisfied::Stopped,
        ),
        (
            "interrupted with no inputs record of its own",
            criterion(vec![interrupted_as_derived], CoverageState::RanNothing),
            Unsatisfied::Stopped,
        ),
        (
            "stale",
            criterion(vec![stale], CoverageState::Evidence),
            Unsatisfied::Stale,
        ),
        (
            "the record holds the opposite",
            criterion(vec![not_proven], CoverageState::Evidence),
            Unsatisfied::NotProven,
        ),
        (
            "zero tests, skipped, or otherwise unknown",
            criterion(vec![unknown], CoverageState::Evidence),
            Unsatisfied::Unknown,
        ),
        (
            "a completion claim is not an examination",
            criterion(vec![a_claim], CoverageState::Evidence),
            Unsatisfied::SettledNothing,
        ),
    ];

    for (what, row, expected) in cases {
        let coverage = view(vec![row], Some(1));
        let answer = compute(&input(&coverage, &[]));
        assert!(!answer.ready, "{what} must never read as ready");
        assert!(answer.rests_on.is_empty(), "{what} rests on nothing");
        assert_eq!(answer.missing, vec!["criterion:1".to_string()], "{what}");
        let obligation = answer
            .obligations
            .iter()
            .find(|o| o.id == "criterion:1")
            .expect("the criterion obligation is listed");
        assert_eq!(obligation.outcome, Outcome::NeedsEvidence, "{what}");
        assert_eq!(obligation.unsatisfied, Some(expected), "{what}");
        assert_eq!(obligation.unsatisfied_slug, Some(expected.slug()), "{what}");
        // Missing evidence is never rendered as a pass: no satisfied
        // obligation exists for a line nothing backs.
        assert!(!obligation.outcome.is_evidence(), "{what}");
    }
}

#[test]
fn one_unsatisfied_line_among_many_holds_the_whole_answer() {
    let mut second = criterion(vec![], CoverageState::None);
    second.index = 2;
    second.text = "Nothing else breaks".into();
    let coverage = view(
        vec![
            criterion(vec![admissible_item()], CoverageState::Evidence),
            second,
        ],
        Some(1),
    );
    let answer = compute(&input(&coverage, &[]));
    assert!(!answer.ready);
    assert_eq!(answer.headline_slug, "readiness_needs_evidence_one");
    assert_eq!(answer.missing_count(), 1);
    // And a ready answer's list is not built from a partial one.
    assert!(answer.rests_on.is_empty());
}

#[test]
fn the_candidate_identity_prerequisite_holds_readiness_and_cannot_be_waived() {
    let coverage = ready_view();
    let mut request = input(&coverage, &[]);
    request.has_examined_inputs = false;
    let answer = compute(&request);
    assert!(
        !answer.ready,
        "`B7.12`: no candidate identity, no readiness"
    );
    assert!(answer.missing.contains(&"candidate_identity".to_string()));
    let obligation = answer
        .obligations
        .iter()
        .find(|o| o.id == "candidate_identity")
        .expect("listed");
    assert!(!obligation.waivable);
    assert_eq!(obligation.unsatisfied, Some(Unsatisfied::InputsMissing));

    // And no stored record could dispose of it, because the record type
    // refuses to be one.
    for id in NON_WAIVABLE {
        let refused = ObligationWaiver {
            id: "cx_test".into(),
            obligation_id: (*id).into(),
            kind: WaiverKind::Waived,
            actor: "sam".into(),
            authority: "the accepted check policy".into(),
            rationale: "we are in a hurry".into(),
            at: "2026-09-22T12:00:00Z".into(),
        };
        assert!(
            refused.validate().is_err(),
            "{id} must refuse a waiver by construction"
        );
        // A refused waiver also never takes effect if one is passed in.
        let waivers = vec![refused];
        let mut request = input(&coverage, &waivers);
        request.has_examined_inputs = false;
        assert!(!compute(&request).ready);
    }
}

#[test]
fn a_waiver_needs_an_actor_an_authority_and_a_reason() {
    let complete = ObligationWaiver {
        id: "cx_test".into(),
        obligation_id: "criterion:1".into(),
        kind: WaiverKind::Waived,
        actor: "sam".into(),
        authority: "the accepted check policy".into(),
        rationale: "this line is covered by the design review".into(),
        at: "2026-09-22T12:00:00Z".into(),
    };
    complete.validate().expect("a complete record is accepted");
    for missing in ["actor", "authority", "rationale", "obligation"] {
        let mut broken = complete.clone();
        match missing {
            "actor" => broken.actor = "  ".into(),
            "authority" => broken.authority = String::new(),
            "rationale" => broken.rationale = String::new(),
            _ => broken.obligation_id = String::new(),
        }
        assert!(
            broken.validate().is_err(),
            "a waiver with no {missing} must be refused"
        );
    }
}

#[test]
fn a_waiver_clears_the_line_and_is_never_counted_as_evidence() {
    // `B7.9`–`B7.11`. The line has nothing behind it; a person waives it under
    // a named policy. Readiness stops being held open, and the waiver is
    // visible, separate, and absent from what the answer rests on.
    let coverage = view(vec![criterion(Vec::new(), CoverageState::None)], Some(1));
    let waivers = vec![ObligationWaiver {
        id: "cx_test".into(),
        obligation_id: "criterion:1".into(),
        kind: WaiverKind::Waived,
        actor: "sam".into(),
        authority: "the accepted check policy".into(),
        rationale: "this line is covered by the design review".into(),
        at: "2026-09-22T12:00:00Z".into(),
    }];
    let answer = compute(&input(&coverage, &waivers));
    assert!(answer.ready);
    assert_eq!(answer.waived, vec!["criterion:1".to_string()]);
    assert!(
        answer.rests_on.is_empty(),
        "a waiver is never something a readiness answer rests on"
    );
    let obligation = &answer.obligations[1];
    assert_eq!(obligation.outcome, Outcome::Waived);
    assert!(
        !obligation.outcome.is_evidence(),
        "a waiver is not evidence"
    );
    assert!(
        obligation.waiver.is_some(),
        "it is visible, with its record"
    );
    assert_eq!(
        obligation.waiver.as_ref().map(|d| d.actor.as_str()),
        Some("sam")
    );

    // "Does not apply here" is its own outcome and is equally not evidence.
    let mut not_applicable = waivers.clone();
    not_applicable[0].kind = WaiverKind::NotApplicable;
    let answer = compute(&input(&coverage, &not_applicable));
    assert!(answer.ready);
    assert_eq!(answer.not_applicable, vec!["criterion:1".to_string()]);
    assert!(answer.waived.is_empty());
    assert!(answer.rests_on.is_empty());
    assert!(!answer.obligations[1].outcome.is_evidence());
}

#[test]
fn an_incomplete_waiver_disposes_of_nothing() {
    let coverage = view(vec![criterion(Vec::new(), CoverageState::None)], Some(1));
    let waivers = vec![ObligationWaiver {
        id: "cx_test".into(),
        obligation_id: "criterion:1".into(),
        kind: WaiverKind::Waived,
        actor: String::new(),
        authority: String::new(),
        rationale: String::new(),
        at: "2026-09-22T12:00:00Z".into(),
    }];
    let answer = compute(&input(&coverage, &waivers));
    assert!(!answer.ready, "an unattributed waiver clears nothing");
    assert_eq!(answer.missing, vec!["criterion:1".to_string()]);
}

#[test]
fn a_waiver_never_hides_a_result_the_record_holds() {
    // A line the record has already settled keeps its own outcome: a waiver
    // over evidence would let one be used to bury the other.
    let coverage = ready_view();
    let waivers = vec![ObligationWaiver {
        id: "cx_test".into(),
        obligation_id: "criterion:1".into(),
        kind: WaiverKind::Waived,
        actor: "sam".into(),
        authority: "the accepted check policy".into(),
        rationale: "not needed".into(),
        at: "2026-09-22T12:00:00Z".into(),
    }];
    let answer = compute(&input(&coverage, &waivers));
    assert_eq!(answer.obligations[1].outcome, Outcome::Satisfied);
    assert!(answer.waived.is_empty());
    assert_eq!(answer.rests_on.len(), 1);
}

#[test]
fn a_recorded_review_policy_is_an_obligation_and_is_unsatisfied_by_default() {
    // The documentation-only policy is recorded, not invented: with no policy
    // entry there is no review obligation at all, and with one there is an
    // obligation nothing satisfies until a review exists.
    assert!(!policy_requires_review(&["cargo test".to_string()]));
    assert!(policy_requires_review(&[
        "policy:review-instead-of-tests".to_string()
    ]));
    assert!(policy_requires_review(&[
        "policy:review-required".to_string()
    ]));

    let coverage = ready_view();
    let mut request = input(&coverage, &[]);
    request.review_required = true;
    let answer = compute(&request);
    assert!(!answer.ready, "a recorded review requirement holds it open");
    assert!(answer.missing.contains(&"review".to_string()));
    let obligation = answer
        .obligations
        .iter()
        .find(|o| o.id == "review")
        .unwrap();
    assert_eq!(obligation.unsatisfied, Some(Unsatisfied::ReviewMissing));

    // Waived under the policy it rests on: cleared, and still not evidence.
    let waivers = vec![ObligationWaiver {
        id: "cx_test".into(),
        obligation_id: "review".into(),
        kind: WaiverKind::Waived,
        actor: "sam".into(),
        authority: "the accepted check policy".into(),
        rationale: "reviewed in the pull request".into(),
        at: "2026-09-22T12:00:00Z".into(),
    }];
    let mut request = input(&coverage, &waivers);
    request.review_required = true;
    let answer = compute(&request);
    assert!(answer.ready);
    assert_eq!(answer.waived, vec!["review".to_string()]);
    assert!(answer.rests_on.iter().all(|r| r.obligation_id != "review"));
}

#[test]
fn a_version_it_cannot_name_is_never_ready() {
    let coverage = ready_view();
    let mut request = input(&coverage, &[]);
    request.target_version = None;
    let answer = compute(&request);
    assert!(
        !answer.ready,
        "`ready` is meaningless without the version it is about"
    );
}

#[test]
fn the_same_records_give_the_same_answer() {
    // `Q1` replay determinism: the function reads no clock, no environment and
    // no store, so two runs over the same records are byte-identical.
    let coverage = ready_view();
    let first = compute(&input(&coverage, &[]));
    let second = compute(&input(&coverage, &[]));
    assert_eq!(first, second);
    assert_eq!(
        serde_json::to_string(&first).unwrap(),
        serde_json::to_string(&second).unwrap()
    );
}

/// NEVER-ASSESSED-NOT-READY (R-2026-09-28-25). The ready fixture with ONE
/// field changed: nothing in the record says whether the evidence still
/// applies. It is kept out of Ready in the same place Couldn't check is, and
/// the reason names it — including beside another item that WAS assessed but
/// satisfies nothing, and beside a stopped run, because assessing this item is
/// the one act that would settle the line.
#[test]
fn an_item_whose_applicability_was_never_assessed_satisfies_nothing() {
    let mut unassessed = admissible_item();
    unassessed.fresh = None;
    unassessed.fresh_slug = None;
    let mut assessed_but_unsettled = admissible_item();
    assessed_but_unsettled.observed = Observed::Couldnt;
    let mut stopped = admissible_item();
    stopped.execution = ExecutionState::Cancelled;
    stopped.fresh = None;
    stopped.fresh_slug = None;
    for (what, items) in [
        ("alone", vec![unassessed.clone()]),
        (
            "beside an assessed item that settles nothing",
            vec![assessed_but_unsettled, unassessed.clone()],
        ),
        ("beside a stopped run", vec![stopped, unassessed.clone()]),
    ] {
        let coverage = view(vec![criterion(items, CoverageState::Evidence)], Some(1));
        let answer = compute(&input(&coverage, &[]));
        assert!(
            !answer.ready,
            "{what}: never assessed must not read as ready"
        );
        assert!(answer.rests_on.is_empty(), "{what}");
        assert_eq!(answer.missing, vec!["criterion:1".to_string()], "{what}");
        let obligation = &answer.obligations[1];
        assert_eq!(obligation.outcome, Outcome::NeedsEvidence, "{what}");
        assert_eq!(
            obligation.unsatisfied_slug,
            Some("readiness_never_assessed"),
            "{what}"
        );
    }
    // The control: the same item assessed and still applying is ready.
    let coverage = view(
        vec![criterion(vec![admissible_item()], CoverageState::Evidence)],
        Some(1),
    );
    assert!(compute(&input(&coverage, &[])).ready);
}
