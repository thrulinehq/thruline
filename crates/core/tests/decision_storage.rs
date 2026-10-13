//! C24: person acts and exact-version computations append; the repo copy derives.

#[path = "support/person_action.rs"]
mod person_action;

use person_action::PersonRecorder;

use serde_json::json;
use thruline_core::decisions::params::{CheckParams, Surface, TextAbsentParams};
use thruline_core::decisions::storage::{
    self, AcceptanceInput, DecisionSource, NewDecision, RetirementInput, SourceKind, VerdictInput,
    WaiverInput,
};
use thruline_core::ledger::Ledger;

fn fixture_ledger(dir: &tempfile::TempDir) -> Ledger {
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("fresh ledger");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id,root_path,source,created_at) VALUES('project-a','/tmp/project-a','codex','2026-10-07T00:00:00Z')",
            [],
        )
        .expect("project");
    ledger
}

fn proposed_decision() -> NewDecision {
    NewDecision {
        project_id: "project-a".into(),
        source_anchor: "rule-anchor-1".into(),
        span: "No obsolete label anywhere".into(),
        sources: vec![DecisionSource {
            source_kind: SourceKind::RuleFile,
            event_id: None,
            file_anchor: Some("rule-anchor-1".into()),
            raw_file: "AGENTS.md".into(),
            raw_line: 12,
            span_start: 0,
            span_end: 27,
            span: "No obsolete label anywhere".into(),
            actor: Some("founder".into()),
            recorded_at: Some("2026-10-07T00:00:00Z".into()),
        }],
        author_actor: "founder".into(),
        recorded_at: "2026-10-07T00:00:00Z".into(),
        params: CheckParams::TextAbsent(TextAbsentParams {
            test_allowlist: Vec::new(),
            json_field_allowlist: Vec::new(),
            conditional_path_allowlist: Vec::new(),
            span_allowlist: Vec::new(),
            terms: vec!["obsolete".into()],
            synonyms: vec![],
            surfaces: vec![Surface::Code, Surface::UiCopy, Surface::Docs],
            allowlist: vec![],
        }),
    }
}

fn count(ledger: &Ledger, table: &str) -> i64 {
    let sql = format!("SELECT count(*) FROM {table}");
    ledger
        .conn()
        .query_row(&sql, [], |row| row.get(0))
        .expect("count")
}

fn decision_counts(ledger: &Ledger) -> [i64; 6] {
    [
        "decisions",
        "decision_revisions",
        "decision_acceptances",
        "decision_retirements",
        "decision_verdicts",
        "decision_waivers",
    ]
    .map(|table| count(ledger, table))
}

#[test]
fn person_acceptance_verdict_waiver_and_retirement_append_separate_rows() {
    let store = tempfile::tempdir().expect("ledger directory");
    let ledger = fixture_ledger(&store);
    assert_eq!(decision_counts(&ledger), [0; 6]);
    let decision_id = storage::create(&ledger, proposed_decision()).expect("create");
    assert_eq!(decision_counts(&ledger), [1, 1, 0, 0, 0, 0]);

    let acceptance_id = storage::accept(
        &ledger,
        AcceptanceInput {
            decision_id: decision_id.clone(),
            revision: 1,
            actor: "founder".into(),
            at: "2026-10-07T01:00:00Z".into(),
            because: "Keep the name out of every surface".into(),
            severity: "blocks".into(),
            applies_from_version: "v1".into(),
        },
    )
    .expect("accept");
    assert!(!acceptance_id.is_empty());
    assert_eq!(decision_counts(&ledger), [1, 1, 1, 0, 0, 0]);
    let mut person = PersonRecorder::new(&ledger, "project-a");
    person.bind_bridge_acceptance(&ledger, &acceptance_id);

    let verdict_id = storage::record_verdict(
        &ledger,
        VerdictInput {
            decision_id: decision_id.clone(),
            revision: 1,
            version_id: "v1".into(),
            tree_hash: "tree-1".into(),
            verdict: "broken".into(),
            reason: None,
            checkable_if: None,
            findings: json!([{"path":"src/lib.rs","line":3}]),
            surfaces: json!([{"surface":"code","state":"broken"}]),
            check_impl_version: "a1.v1".into(),
            tool_versions: json!({}),
            computed_at: "2026-10-07T01:01:00Z".into(),
            basis: json!({"tree_hash":"tree-1"}),
        },
    )
    .expect("verdict");
    assert!(!verdict_id.is_empty());
    assert_eq!(decision_counts(&ledger), [1, 1, 1, 0, 1, 0]);

    storage::waive(
        &ledger,
        WaiverInput {
            decision_id: decision_id.clone(),
            version_id: "v1".into(),
            actor: "founder".into(),
            at: "2026-10-07T01:02:00Z".into(),
            because: "One version only".into(),
            expires_version: Some("v2".into()),
            expires_at: None,
        },
    )
    .expect("waive");
    assert_eq!(decision_counts(&ledger), [1, 1, 1, 0, 1, 1]);

    storage::retire(
        &ledger,
        RetirementInput {
            decision_id: decision_id.clone(),
            actor: "founder".into(),
            at: "2026-10-07T01:03:00Z".into(),
            because: "Replaced by a later accepted rule".into(),
            superseded_by: None,
        },
    )
    .expect("retire");
    assert_eq!(decision_counts(&ledger), [1, 1, 1, 1, 1, 1]);

    for table in [
        "decisions",
        "decision_revisions",
        "decision_acceptances",
        "decision_retirements",
        "decision_verdicts",
        "decision_waivers",
    ] {
        assert_eq!(count(&ledger, table), 1, "{table}");
        for sql in [
            format!("UPDATE {table} SET rowid=rowid"),
            format!("DELETE FROM {table}"),
            format!("INSERT OR REPLACE INTO {table} SELECT * FROM {table} LIMIT 1"),
        ] {
            let error = ledger.conn().execute(&sql, []).expect_err(&sql);
            assert!(
                error.to_string().contains("append-only"),
                "{table} must reject direct mutation through its append-only trigger: {error}"
            );
        }
    }
    let still_broken: String = ledger
        .conn()
        .query_row(
            "SELECT verdict FROM decision_verdicts WHERE id=?1",
            [&verdict_id],
            |row| row.get(0),
        )
        .expect("historical verdict");
    assert_eq!(still_broken, "broken");
}

#[test]
fn a_revised_proposal_preserves_the_accepted_revision() {
    let store = tempfile::tempdir().expect("ledger directory");
    let ledger = fixture_ledger(&store);
    let decision_id = storage::create(&ledger, proposed_decision()).expect("create");
    let acceptance_id = storage::accept(
        &ledger,
        AcceptanceInput {
            decision_id: decision_id.clone(),
            revision: 1,
            actor: "founder".into(),
            at: "2026-10-07T01:00:00Z".into(),
            because: "Standing rule".into(),
            severity: "blocks".into(),
            applies_from_version: "v1".into(),
        },
    )
    .expect("accept first revision");
    let mut person = PersonRecorder::new(&ledger, "project-a");
    person.bind_bridge_acceptance(&ledger, &acceptance_id);
    let revised_span = "No obsolete or retired label anywhere";
    let mut revised_source = proposed_decision().sources.remove(0);
    revised_source.span = revised_span.into();
    revised_source.span_end = revised_span.len() as u64;
    let next = storage::append_revision(
        &ledger,
        storage::RevisionInput {
            decision_id: decision_id.clone(),
            span: revised_span.into(),
            sources: vec![revised_source],
            author_actor: "founder".into(),
            recorded_at: "2026-10-07T02:00:00Z".into(),
            params: CheckParams::TextAbsent(TextAbsentParams {
                test_allowlist: Vec::new(),
                json_field_allowlist: Vec::new(),
                conditional_path_allowlist: Vec::new(),
                span_allowlist: Vec::new(),
                terms: vec!["obsolete".into(), "retired".into()],
                synonyms: vec![],
                surfaces: vec![Surface::Code, Surface::UiCopy, Surface::Docs],
                allowlist: vec![],
            }),
        },
    )
    .expect("propose revision");
    assert_eq!(next, 2);
    assert_eq!(count(&ledger, "decision_revisions"), 2);
    let accepted = storage::accepted_for_project(&ledger, "project-a").expect("accepted");
    assert_eq!(accepted.len(), 1);
    assert_eq!(
        accepted[0].revision, 1,
        "proposal alone does not replace acceptance"
    );
}

#[test]
fn an_agent_cannot_skip_person_fields_or_accept_an_unknown_revision() {
    let store = tempfile::tempdir().expect("ledger directory");
    let ledger = fixture_ledger(&store);
    let decision_id = storage::create(&ledger, proposed_decision()).expect("create");
    for (revision, actor, because) in [
        (1, "", "reason"),
        (1, "person", ""),
        (2, "person", "reason"),
    ] {
        assert!(storage::accept(
            &ledger,
            AcceptanceInput {
                decision_id: decision_id.clone(),
                revision,
                actor: actor.into(),
                at: "2026-10-07T01:00:00Z".into(),
                because: because.into(),
                severity: "blocks".into(),
                applies_from_version: "v1".into(),
            },
        )
        .is_err());
    }
    assert_eq!(count(&ledger, "decision_acceptances"), 0);
}

#[test]
fn the_derived_copy_is_regenerated_from_accepted_rows_and_tampering_is_detected() {
    let store = tempfile::tempdir().expect("ledger directory");
    let ledger = fixture_ledger(&store);
    let decision_id = storage::create(&ledger, proposed_decision()).expect("create");
    let acceptance_id = storage::accept(
        &ledger,
        AcceptanceInput {
            decision_id,
            revision: 1,
            actor: "founder".into(),
            at: "2026-10-07T01:00:00Z".into(),
            because: "Standing rule".into(),
            severity: "warns".into(),
            applies_from_version: "v1".into(),
        },
    )
    .expect("accept");
    let mut person = PersonRecorder::new(&ledger, "project-a");
    person.bind_bridge_acceptance(&ledger, &acceptance_id);
    let dir = tempfile::tempdir().expect("project scratch");
    let copy = storage::write_derived_file(&ledger, "project-a", dir.path()).expect("write");
    assert_eq!(copy, dir.path().join(".thruline/decisions.yml"));
    assert!(storage::verify_derived_file(&ledger, "project-a", dir.path()).expect("verify"));
    std::fs::write(&copy, "derived_copy: true\ndecisions: []\n").expect("tamper scratch copy");
    assert!(!storage::verify_derived_file(&ledger, "project-a", dir.path()).expect("detect"));
    assert_eq!(count(&ledger, "decision_acceptances"), 1);
}
