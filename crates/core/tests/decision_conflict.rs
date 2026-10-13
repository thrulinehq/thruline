#[path = "fixtures/decision_conflict/mod.rs"]
mod fixture;
#[path = "support/person_action.rs"]
mod person_action;
use fixture::*;
use thruline_core::decisions::{
    conflict::{self, ConflictState},
    params::{CheckParams, PathPresentParams},
    storage::{self, RetirementInput},
};

#[test]
fn later_successor_revision_cannot_inherit_an_older_retirement() {
    let mut f = Fixture::new();
    let old = f.decision("p", default("true"), true);
    let new = f.decision("p", default("false"), true);
    let act = storage::retire(
        &f.ledger,
        RetirementInput {
            decision_id: old.clone(),
            actor: "person".into(),
            at: "2026-10-07T00:04:00Z".into(),
            because: "replace exact predecessor".into(),
            superseded_by: Some(new.clone()),
        },
    )
    .expect("retirement");
    let mut person = person_action::PersonRecorder::bridge_only(&f.ledger, "p");
    person.bind_retirement(&f.ledger, &act);
    f.people.push(person);
    let (span, sources): (String, String) = f
        .ledger
        .conn()
        .query_row(
            "SELECT text,sources_json FROM decision_revisions WHERE decision_id=?1 AND rev=1",
            [&new],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("original source");
    let revision = storage::append_revision(
        &f.ledger,
        storage::RevisionInput {
            decision_id: new.clone(),
            span,
            sources: serde_json::from_str(&sources).expect("sources"),
            author_actor: "person".into(),
            recorded_at: "2026-10-07T00:06:00Z".into(),
            params: default("true"),
        },
    )
    .expect("revision");
    let acceptance = storage::accept(
        &f.ledger,
        storage::AcceptanceInput {
            decision_id: new,
            revision,
            actor: "person".into(),
            at: "2026-10-07T00:06:00Z".into(),
            because: "new exact revision".into(),
            severity: "warns".into(),
            applies_from_version: f.version.clone(),
        },
    )
    .expect("acceptance");
    let mut person = person_action::PersonRecorder::bridge_only(&f.ledger, "p");
    person.bind_acceptance(&f.ledger, &acceptance);
    f.people.push(person);
    assert!(storage::verified_retirement_for_decision(&f.ledger, &old)
        .expect("standing")
        .is_none());
    assert!(storage::accepted_for_project(&f.ledger, "p")
        .expect("active")
        .iter()
        .any(|d| d.id == old));
}

#[test]
fn required_known_blob_conflicts_with_unwaived_absence() {
    let mut f = Fixture::new();
    let bytes = b"const OBSOLETE: bool = true;\n";
    std::fs::write(f.root().join("app.rs"), bytes).expect("fixture source");
    f.version = commit(f.root());
    f.decision("p", absence(), true);
    f.decision(
        "p",
        CheckParams::PathPresent(PathPresentParams {
            path: "app.rs".into(),
            sha256: Some(thruline_core::chain::sha256_hex(bytes)),
        }),
        true,
    );
    let r = conflict::project_conflicts(&f.ledger, "p", f.root(), &f.version).expect("report");
    assert_eq!(r.conflicts[0].state, ConflictState::Conflict);
    assert!(r.conflicts[0].left_authority.is_some());
    assert!(r.conflicts[0].right_authority.is_some());
}

#[test]
fn accepted_path_exclusion_prevents_absence_conflict() {
    let mut f = Fixture::new();
    let bytes = b"const OBSOLETE: bool = true;\n";
    std::fs::write(f.root().join("app.rs"), bytes).expect("fixture source");
    f.version = commit(f.root());
    let mut params = absence();
    if let CheckParams::TextAbsent(text) = &mut params {
        text.allowlist.push("app.rs".into());
    }
    f.decision("p", params, true);
    f.decision(
        "p",
        CheckParams::PathPresent(PathPresentParams {
            path: "app.rs".into(),
            sha256: Some(thruline_core::chain::sha256_hex(bytes)),
        }),
        true,
    );
    assert!(
        conflict::project_conflicts(&f.ledger, "p", f.root(), &f.version)
            .expect("report")
            .conflicts
            .is_empty()
    );
}

#[test]
fn lost_successor_receipt_restores_predecessor_enforcement() {
    let mut f = Fixture::new();
    let old = f.decision("p", default("true"), true);
    let new = f.decision("p", default("false"), true);
    let successor_source = f.people.last().expect("recorder").raw_file().to_owned();
    let act = storage::retire(
        &f.ledger,
        RetirementInput {
            decision_id: old.clone(),
            actor: "person".into(),
            at: "2026-10-07T00:04:00Z".into(),
            because: "replace exact predecessor".into(),
            superseded_by: Some(new),
        },
    )
    .expect("retirement");
    let mut person = person_action::PersonRecorder::bridge_only(&f.ledger, "p");
    person.bind_retirement(&f.ledger, &act);
    f.people.push(person);
    std::fs::write(successor_source, "{}\n").expect("alter fixture receipt");
    assert!(storage::verified_retirement_for_decision(&f.ledger, &old)
        .expect("standing")
        .is_none());
    assert!(storage::accepted_for_project(&f.ledger, "p")
        .expect("active")
        .iter()
        .any(|d| d.id == old));
}
#[test]
fn contradictory_defaults_stay_visible_and_never_retire() {
    let mut f = Fixture::new();
    f.decision("p", default("true"), true);
    f.decision("p", default("false"), true);
    let r = conflict::project_conflicts(&f.ledger, "p", f.root(), &f.version).expect("report");
    assert_eq!(r.conflicts.len(), 1);
    assert_eq!(r.conflicts[0].state, ConflictState::Conflict);
    assert_eq!(
        f.ledger
            .conn()
            .query_row("SELECT COUNT(*) FROM decision_retirements", [], |r| r
                .get::<_, i64>(0))
            .expect("count"),
        0
    );
}
#[test]
fn disjoint_defaults_do_not_conflict() {
    let mut f = Fixture::new();
    f.decision("p", default("true"), true);
    let mut p = default("false");
    if let CheckParams::FeatureDefaultPreserved(p) = &mut p {
        p.path = "another.rs".into();
    }
    f.decision("p", p, true);
    assert!(
        conflict::project_conflicts(&f.ledger, "p", f.root(), &f.version)
            .expect("report")
            .conflicts
            .is_empty()
    );
}
#[test]
fn equal_defaults_do_not_conflict() {
    let mut f = Fixture::new();
    f.decision("p", default("true"), true);
    f.decision("p", default("true"), true);
    assert!(
        conflict::project_conflicts(&f.ledger, "p", f.root(), &f.version)
            .expect("report")
            .conflicts
            .is_empty()
    );
}
#[test]
fn proposed_or_foreign_decisions_do_not_conflict() {
    let mut f = Fixture::new();
    f.decision("p", default("true"), true);
    f.decision("p", default("false"), false);
    f.decision("other", default("false"), true);
    assert!(
        conflict::project_conflicts(&f.ledger, "p", f.root(), &f.version)
            .expect("report")
            .conflicts
            .is_empty()
    );
}
#[test]
fn empty_file_presence_is_not_content_presence() {
    let mut f = Fixture::new();
    f.decision("p", absence(), true);
    f.decision(
        "p",
        CheckParams::PathPresent(PathPresentParams {
            path: "app.rs".into(),
            sha256: None,
        }),
        true,
    );
    assert!(
        conflict::project_conflicts(&f.ledger, "p", f.root(), &f.version)
            .expect("report")
            .conflicts
            .is_empty()
    );
}
#[test]
fn different_exact_file_hashes_conflict() {
    let mut f = Fixture::new();
    for hash in ["0".repeat(64), "1".repeat(64)] {
        f.decision(
            "p",
            CheckParams::PathPresent(PathPresentParams {
                path: "app.rs".into(),
                sha256: Some(hash),
            }),
            true,
        );
    }
    assert_eq!(
        conflict::project_conflicts(&f.ledger, "p", f.root(), &f.version)
            .expect("report")
            .conflicts[0]
            .state,
        ConflictState::Conflict
    );
}
#[test]
fn unresolved_version_is_explicitly_unverifiable() {
    let mut f = Fixture::new();
    f.decision("p", default("true"), true);
    f.decision("p", default("false"), true);
    let r = conflict::project_conflicts(&f.ledger, "p", f.root(), &"a".repeat(40)).expect("report");
    assert_eq!(r.conflicts[0].state, ConflictState::Unverifiable);
}
#[test]
fn supersession_refuses_unknown_foreign_pending_and_older_targets() {
    for case in ["unknown", "foreign", "pending", "older"] {
        let mut f = Fixture::new();
        let first = f.decision("p", default("true"), true);
        let target = match case {
            "unknown" => "no-target".into(),
            "foreign" => f.decision("other", default("false"), true),
            "pending" => f.decision("p", default("false"), false),
            _ => first.clone(),
        };
        let old = if case == "older" {
            f.decision("p", default("false"), true)
        } else {
            first
        };
        assert!(
            storage::retire(
                &f.ledger,
                RetirementInput {
                    decision_id: old,
                    actor: "person".into(),
                    at: "2026-10-07T00:04:00Z".into(),
                    because: "replace exact predecessor".into(),
                    superseded_by: Some(target)
                }
            )
            .is_err(),
            "case {case}"
        );
    }
}
#[test]
fn explicit_verified_supersession_keeps_both_rows() {
    let mut f = Fixture::new();
    let old = f.decision("p", default("true"), true);
    let new = f.decision("p", default("false"), true);
    let act = storage::retire(
        &f.ledger,
        RetirementInput {
            decision_id: old.clone(),
            actor: "person".into(),
            at: "2026-10-07T00:04:00Z".into(),
            because: "replace exact predecessor".into(),
            superseded_by: Some(new),
        },
    )
    .expect("retirement");
    let mut person = person_action::PersonRecorder::bridge_only(&f.ledger, "p");
    person.bind_retirement(&f.ledger, &act);
    f.people.push(person);
    assert!(storage::verified_retirement_for_decision_read_only(
        &f.ledger,
        &old,
        std::time::Duration::from_secs(2),
    )
    .expect("budgeted standing")
    .is_some());
    assert!(storage::verified_retirement_for_decision_read_only(
        &f.ledger,
        &old,
        std::time::Duration::ZERO,
    )
    .expect("expired standing")
    .is_none());
    let r = conflict::project_conflicts(&f.ledger, "p", f.root(), &f.version).expect("report");
    assert_eq!(r.conflicts[0].state, ConflictState::Superseded);
    assert_eq!(
        f.ledger
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM decisions WHERE project_id='p'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .expect("count"),
        2
    );
}

#[test]
fn changed_acceptance_source_is_visible_as_missing_evidence() {
    let mut f = Fixture::new();
    f.decision("p", default("true"), true);
    f.decision("p", default("false"), true);
    let source = f
        .people
        .last()
        .expect("person source")
        .raw_file()
        .to_owned();
    std::fs::write(source, "{}\n").expect("changed source");
    let report = conflict::project_conflicts(&f.ledger, "p", f.root(), &f.version).expect("report");
    assert!(!report.missing_evidence.is_empty());
}
