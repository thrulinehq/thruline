#[path = "support/person_action.rs"]
mod person_action;
#[path = "fixtures/decisions/deferred/support.rs"]
mod support;
use support::*;
use thruline_core::decisions::{
    checks::VerdictState,
    deferred::{DeferredCondition, DeferredDecisionParams},
    engine,
    params::{CheckParams, PathPresentParams},
    storage::{self, AcceptanceInput, DecisionSource, NewDecision, SourceKind},
};

#[test]
fn accepted_h1_rechecks_record_arrival_on_same_tree_and_replays_source_pinned_basis() {
    let text = "After date 2026-10-08T00:00:00Z, keep REQUIRED.";
    let f = Fixture::new(vec![
        prompt(text),
        call("clock-old", "date -u +%Y-%m-%dT%H:%M:%SZ"),
        output("clock-old", "2026-01-01T00:00:00Z", 0),
    ]);
    let id = storage::create(
        &f.ledger,
        NewDecision {
            project_id: PROJECT.into(),
            source_anchor: "event-3".into(),
            span: text.into(),
            sources: vec![DecisionSource {
                source_kind: SourceKind::WorkPrompt,
                event_id: Some("event-3".into()),
                file_anchor: None,
                raw_file: f.raw.to_string_lossy().into(),
                raw_line: 3,
                span_start: 0,
                span_end: text.len() as u64,
                span: text.into(),
                actor: Some("person".into()),
                recorded_at: None,
            }],
            author_actor: "person".into(),
            recorded_at: "2026-10-07T00:00:00Z".into(),
            params: CheckParams::Deferred(DeferredDecisionParams {
                condition: DeferredCondition::Date {
                    at_utc: "2026-10-08T00:00:00Z".into(),
                },
                action: Box::new(CheckParams::PathPresent(PathPresentParams {
                    path: "REQUIRED".into(),
                    sha256: None,
                })),
            }),
        },
    )
    .unwrap();
    assert!(engine::check_project_version(
        &f.ledger,
        PROJECT,
        &f.repo,
        &f.version,
        "2026-10-07T00:00:01Z",
        false
    )
    .unwrap()
    .is_empty());
    let accept = storage::accept(
        &f.ledger,
        AcceptanceInput {
            decision_id: id.clone(),
            revision: 1,
            actor: "person".into(),
            at: "2026-10-07T00:00:02Z".into(),
            because: "Use this exact deferred check".into(),
            severity: "warns".into(),
            applies_from_version: f.version.clone(),
        },
    )
    .unwrap();
    let mut recorder = person_action::PersonRecorder::bridge_only(&f.ledger, PROJECT);
    recorder.bind_acceptance(&f.ledger, &accept);
    let first = engine::check_decision_version(
        &f.ledger,
        PROJECT,
        &f.repo,
        &f.version,
        "2026-10-07T00:00:03Z",
        false,
        &id,
    )
    .unwrap();
    assert_eq!(first[0].state, VerdictState::NotApplicable);
    assert!(!first[0].from_cache);
    f.append(vec![
        call("clock-new", "date -u +%Y-%m-%dT%H:%M:%SZ"),
        output("clock-new", "2026-10-09T00:00:00Z", 0),
        call("head", "git -C '$REPO' rev-parse HEAD"),
        output("head", "$VERSION", 0),
    ]);
    let second = engine::check_decision_version(
        &f.ledger,
        PROJECT,
        &f.repo,
        &f.version,
        "2099-10-07T00:00:04Z",
        false,
        &id,
    )
    .unwrap();
    assert_eq!(second[0].state, VerdictState::Holds);
    assert!(!second[0].from_cache);
    let basis: String = f
        .ledger
        .conn()
        .query_row(
            "SELECT basis_json FROM decision_verdicts WHERE id=?1",
            [second[0].verdict_id.as_ref().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    let basis: serde_json::Value = serde_json::from_str(&basis).unwrap();
    assert_eq!(basis["record_projection"]["lifecycle"], "closed");
    assert_eq!(
        basis["record_projection"]["sources"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    let replay = engine::check_decision_version(
        &f.ledger,
        PROJECT,
        &f.repo,
        &f.version,
        "1900-01-01T00:00:00Z",
        true,
        &id,
    )
    .unwrap();
    assert_eq!(replay[0].state, VerdictState::Holds);
    // Deferred work stays closed after an ancestor fulfilled it. This is a
    // one-time action, not a new standing presence rule on every descendant.
    std::fs::remove_file(f.repo.join("REQUIRED")).unwrap();
    git(&f.repo, &["add", "-A"]);
    git(&f.repo, &["commit", "-qm", "later version"]);
    let next = git(&f.repo, &["rev-parse", "HEAD"]);
    let later = engine::check_decision_version(
        &f.ledger,
        PROJECT,
        &f.repo,
        &next,
        "2026-10-07T00:00:05Z",
        false,
        &id,
    )
    .unwrap();
    assert_eq!(later[0].state, VerdictState::Holds);
    assert!(later[0]
        .outcome
        .reason
        .as_ref()
        .unwrap()
        .contains(&f.version));
    let later_replay = engine::check_decision_version(
        &f.ledger,
        PROJECT,
        &f.repo,
        &next,
        "2026-10-07T00:00:06Z",
        true,
        &id,
    )
    .unwrap();
    assert_eq!(later_replay[0].state, VerdictState::Holds);
    std::fs::write(&f.raw, "source no longer verifies\n").unwrap();
    assert!(engine::check_decision_version(
        &f.ledger,
        PROJECT,
        &f.repo,
        &f.version,
        "2026-10-07T00:00:05Z",
        true,
        &id
    )
    .is_err());
}
