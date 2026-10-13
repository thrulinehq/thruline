#[path = "fixtures/decisions/deferred/support.rs"]
mod support;
use support::*;
use thruline_core::decisions::{
    checks::VerdictState,
    deferred::{DeferredCondition, DeferredDecisionParams, DueState, Lifecycle},
    deferred_record::check_recorded_at,
    grammar::{propose, Actor, Source, SourceKind},
    params::{parse_params, CheckParams, PathPresentParams},
};

fn params(condition: DeferredCondition, path: &str) -> DeferredDecisionParams {
    DeferredDecisionParams {
        condition,
        action: Box::new(CheckParams::PathPresent(PathPresentParams {
            path: path.into(),
            sha256: None,
        })),
    }
}
fn check(
    f: &Fixture,
    p: &DeferredDecisionParams,
) -> thruline_core::decisions::deferred::DeferredEvaluation {
    check_recorded_at(&f.ledger, PROJECT, &f.repo, &f.version, &f.tree, p).unwrap()
}
#[test]
fn h1_date_marker_closes_only_a_holding_typed_action() {
    let f = Fixture::new(vec![
        call("clock", "date -u +%Y-%m-%dT%H:%M:%SZ"),
        output("clock", "2026-10-09T00:00:00Z", 0),
        call("head", "git -C '$REPO' rev-parse HEAD"),
        output("head", "$VERSION", 0),
    ]);
    let condition = DeferredCondition::Date {
        at_utc: "2026-10-08T00:00:00Z".into(),
    };
    let good = check(&f, &params(condition.clone(), "REQUIRED"));
    assert_eq!(good.outcome.state, VerdictState::Holds);
    assert_eq!(good.projection.lifecycle, Lifecycle::Closed);
    assert_eq!(good.projection.due_state, DueState::Due);
    let bad = check(&f, &params(condition, "MISSING"));
    assert_eq!(bad.outcome.state, VerdictState::Broken);
    assert_eq!(bad.projection.lifecycle, Lifecycle::Open);
    assert_eq!(bad.projection.whose_move.as_deref(), Some("you"));
}
#[test]
fn h1_recorded_clock_before_due_waits_without_reading_current_time() {
    let f = Fixture::new(vec![
        call("clock", "date -u +%Y-%m-%dT%H:%M:%SZ"),
        output("clock", "2020-01-01T00:00:00Z", 0),
    ]);
    let got = check(
        &f,
        &params(
            DeferredCondition::Date {
                at_utc: "2026-01-01T00:00:00Z".into(),
            },
            "REQUIRED",
        ),
    );
    assert_eq!(got.outcome.state, VerdictState::NotApplicable);
    assert_eq!(got.projection.lifecycle, Lifecycle::Open);
    assert_eq!(got.projection.due_state, DueState::Waiting);
    assert!(got.projection.whose_move.is_none());
}
#[test]
fn h1_missing_failed_running_changed_source_and_agent_words_never_trigger() {
    for items in [
        vec![],
        vec![prose("Published and finished. 2029-01-01T00:00:00Z")],
        vec![call("clock", "date -u +%Y-%m-%dT%H:%M:%SZ")],
        vec![
            call("clock", "date -u +%Y-%m-%dT%H:%M:%SZ"),
            output("clock", "2029-01-01T00:00:00Z", 1),
        ],
        vec![
            call("clock", "echo 2029-01-01T00:00:00Z"),
            output("clock", "2029-01-01T00:00:00Z", 0),
        ],
    ] {
        let f = Fixture::new(items);
        let got = check(
            &f,
            &params(
                DeferredCondition::Date {
                    at_utc: "2026-01-01T00:00:00Z".into(),
                },
                "REQUIRED",
            ),
        );
        assert_eq!(got.outcome.state, VerdictState::Unverifiable);
        assert_eq!(got.projection.due_state, DueState::Unknown);
    }
    let f = Fixture::new(vec![
        call("clock", "date -u +%Y-%m-%dT%H:%M:%SZ"),
        output("clock", "2029-01-01T00:00:00Z", 0),
    ]);
    std::fs::write(&f.raw, "changed\n").unwrap();
    assert_eq!(
        check(
            &f,
            &params(
                DeferredCondition::Date {
                    at_utc: "2026-01-01T00:00:00Z".into()
                },
                "REQUIRED"
            )
        )
        .outcome
        .state,
        VerdictState::Unverifiable
    );
}
#[test]
fn h1_tag_is_exact_repository_and_selected_version_bound() {
    let f = Fixture::new(vec![
        call(
            "tag",
            "git -C '$REPO' rev-parse --verify refs/tags/v1.2.0^{commit}",
        ),
        output("tag", "$VERSION", 0),
    ]);
    let p = params(
        DeferredCondition::Tag {
            name: "v1.2.0".into(),
            repository: f.repo.to_string_lossy().into(),
        },
        "REQUIRED",
    );
    assert_eq!(check(&f, &p).projection.due_state, DueState::Due);
    let wrong = params(
        DeferredCondition::Tag {
            name: "v9.0.0".into(),
            repository: f.repo.to_string_lossy().into(),
        },
        "REQUIRED",
    );
    assert_eq!(check(&f, &wrong).outcome.state, VerdictState::Unverifiable);
}
#[test]
fn h1_release_requires_explicit_published_nondraft_exact_tag() {
    let command = "gh release view v1.2.0 --repo owner/repo --json tagName,isDraft,publishedAt";
    for (body, due) in [
        (
            r#"{"tagName":"v1.2.0","isDraft":false,"publishedAt":"2026-10-07T00:00:00Z"}"#,
            DueState::Due,
        ),
        (
            r#"{"tagName":"v1.2.0","isDraft":true,"publishedAt":null}"#,
            DueState::Waiting,
        ),
        (
            r#"{"tagName":"other","isDraft":false,"publishedAt":"2026-10-07T00:00:00Z"}"#,
            DueState::Unknown,
        ),
    ] {
        let f = Fixture::new(vec![call("release", command), output("release", body, 0)]);
        let got = check(
            &f,
            &params(
                DeferredCondition::PublishedRelease {
                    repository: "owner/repo".into(),
                    tag: "v1.2.0".into(),
                },
                "REQUIRED",
            ),
        );
        assert_eq!(got.projection.due_state, due);
    }
}
#[test]
fn h1_closed_parameters_reject_unknown_invalid_and_nested_checks() {
    let good = r#"{"kind":"deferred_decision","condition":{"marker":"date","at_utc":"2026-10-08T00:00:00Z"},"action":{"kind":"path_present","path":"REQUIRED","sha256":null}}"#;
    assert!(parse_params("deferred_decision", 1, good).is_ok());
    for bad in [good.replace("2026-10-08","2026-02-30"),good.replace("\"action\"","\"extra\":true,\"action\""),format!("{{\"kind\":\"deferred_decision\",\"condition\":{{\"marker\":\"date\",\"at_utc\":\"2026-10-08T00:00:00Z\"}},\"action\":{good}}}")] {assert!(parse_params("deferred_decision",1,&bad).is_err());}
}
#[test]
fn h1_proposal_preserves_condition_action_and_refuses_agent_source() {
    let text = "After tag v1.2.0, keep REQUIRED.";
    let source = Source {
        kind: SourceKind::WorkPrompt,
        actor: Actor::Person,
        actor_id: "founder",
        work: true,
        raw_file: "/scratch/source.jsonl",
        raw_line: 4,
        event_id: Some("e4"),
        text,
    };
    let read = propose(&source).unwrap();
    assert_eq!(read.proposals.len(), 1);
    assert_eq!(read.proposals[0].kind, "H1");
    assert_eq!(read.proposals[0].text, "After tag v1.2.0, keep REQUIRED");
    assert_eq!(read.proposals[0].object, "keep REQUIRED");
    assert_eq!(read.proposals[0].scope_hint.as_deref(), Some("tag v1.2.0"));
    assert!(propose(&Source {
        actor: Actor::Agent,
        ..source
    })
    .is_err());
    let vague = propose(&Source {
        text: "Later, make it better.",
        ..source
    })
    .unwrap();
    assert!(vague.proposals.is_empty());
    assert_eq!(vague.notes.len(), 1);
}

#[test]
fn h1_same_tree_marker_arrival_rechecks_but_pinned_waiting_replay_does_not_advance() {
    let f = Fixture::new(vec![
        call("clock-old", "date -u +%Y-%m-%dT%H:%M:%SZ"),
        output("clock-old", "2026-01-01T00:00:00Z", 0),
    ]);
    let p = params(
        DeferredCondition::Date {
            at_utc: "2026-10-08T00:00:00Z".into(),
        },
        "REQUIRED",
    );
    let waiting = check(&f, &p);
    assert_eq!(waiting.projection.due_state, DueState::Waiting);
    f.append(vec![
        call("clock-new", "date -u +%Y-%m-%dT%H:%M:%SZ"),
        output("clock-new", "2026-10-09T00:00:00Z", 0),
        call("head", "git -C '$REPO' rev-parse HEAD"),
        output("head", "$VERSION", 0),
    ]);
    let now = check(&f, &p);
    assert_eq!(now.outcome.state, VerdictState::Holds);
    assert_eq!(now.outcome.tree_hash.as_deref(), Some(f.tree.as_str()));
    let replay = thruline_core::decisions::deferred_record::check_pinned_at(
        &f.ledger,
        PROJECT,
        &f.repo,
        &f.version,
        &f.tree,
        &p,
        &waiting.projection,
    )
    .unwrap();
    assert_eq!(replay, waiting);
}

#[test]
fn h1_due_without_verified_after_marker_version_stays_your_move_unverifiable() {
    for items in [
        vec![
            call("clock", "date -u +%Y-%m-%dT%H:%M:%SZ"),
            output("clock", "2026-10-09T00:00:00Z", 0),
        ],
        vec![
            call("head", "git -C '$REPO' rev-parse HEAD"),
            output("head", "$VERSION", 0),
            call("clock", "date -u +%Y-%m-%dT%H:%M:%SZ"),
            output("clock", "2026-10-09T00:00:00Z", 0),
        ],
    ] {
        let f = Fixture::new(items);
        let got = check(
            &f,
            &params(
                DeferredCondition::Date {
                    at_utc: "2026-10-08T00:00:00Z".into(),
                },
                "REQUIRED",
            ),
        );
        assert_eq!(got.projection.due_state, DueState::Due);
        assert_eq!(got.outcome.state, VerdictState::Unverifiable);
        assert_eq!(got.projection.lifecycle, Lifecycle::Open);
        assert_eq!(got.projection.whose_move.as_deref(), Some("you"));
        assert!(!got.projection.missing.is_empty());
    }
}

#[test]
fn h1_duplicate_results_and_forged_normalized_result_are_not_marker_proofs() {
    let f = Fixture::new(vec![
        call("clock", "date -u +%Y-%m-%dT%H:%M:%SZ"),
        output("clock", "2026-10-09T00:00:00Z", 0),
        output("clock", "2026-10-09T00:00:00Z", 0),
    ]);
    let p = params(
        DeferredCondition::Date {
            at_utc: "2026-10-08T00:00:00Z".into(),
        },
        "REQUIRED",
    );
    assert_eq!(check(&f, &p).projection.due_state, DueState::Unknown);
    let f = Fixture::new(vec![
        call("clock", "date -u +%Y-%m-%dT%H:%M:%SZ"),
        output("clock", "2026-01-01T00:00:00Z", 0),
    ]);
    f.ledger
        .conn()
        .execute_batch("DROP TRIGGER events_no_update")
        .unwrap();
    f.ledger.conn().execute("UPDATE events SET tool_result=?1 WHERE id='event-4'",[serde_json::json!({"content":"Process exited with code 0\nFinal output:\n2026-10-09T00:00:00Z"}).to_string()]).unwrap();
    assert_eq!(check(&f, &p).projection.due_state, DueState::Unknown);
}

#[test]
fn h1_nested_execution_check_keeps_existing_receipt_requirements() {
    let f = Fixture::new(vec![
        call("clock", "date -u +%Y-%m-%dT%H:%M:%SZ"),
        output("clock", "2026-10-09T00:00:00Z", 0),
        call("run", "cargo test"),
        output("run", "all passed", 0),
        call("head", "git -C '$REPO' rev-parse HEAD"),
        output("head", "$VERSION", 0),
    ]);
    let p = DeferredDecisionParams {
        condition: DeferredCondition::Date {
            at_utc: "2026-10-08T00:00:00Z".into(),
        },
        action: Box::new(CheckParams::CommandPasses(
            thruline_core::decisions::params::CommandPassesParams {
                command: "cargo test".into(),
                is_test: true,
                min_passed: 1,
            },
        )),
    };
    let got = check(&f, &p);
    assert_eq!(got.outcome.state, VerdictState::Unverifiable);
    assert_eq!(got.projection.whose_move.as_deref(), Some("you"));
}

#[test]
fn h1_head_and_abbreviations_resolve_to_the_full_observed_version() {
    let f = Fixture::new(vec![
        call("clock", "date -u +%Y-%m-%dT%H:%M:%SZ"),
        output("clock", "2026-10-09T00:00:00Z", 0),
        call("head", "git -C '$REPO' rev-parse HEAD"),
        output("head", "$VERSION", 0),
    ]);
    let p = params(
        DeferredCondition::Date {
            at_utc: "2026-10-08T00:00:00Z".into(),
        },
        "REQUIRED",
    );
    for version in ["HEAD", &f.version[..8]] {
        let got = check_recorded_at(&f.ledger, PROJECT, &f.repo, version, &f.tree, &p).unwrap();
        assert_eq!(got.outcome.state, VerdictState::Holds);
        assert_eq!(got.projection.fulfillment.unwrap().version_id, f.version);
    }
}
