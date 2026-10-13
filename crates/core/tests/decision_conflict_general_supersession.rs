#[path = "fixtures/decision_conflict/mod.rs"]
mod fixture;
#[path = "fixtures/decision_conflict_general/mod.rs"]
mod general;
#[path = "support/person_action.rs"]
mod person_action;
use fixture::*;
use general::*;
use thruline_core::decisions::{
    authority::{bridge, PersonActKind},
    storage,
};

#[test]
fn general_review_pins_both_exact_acceptances_and_stored_params_bytes() {
    let mut f = Fixture::new();
    let old = f.decision("p", default("true"), true);
    let new = f.decision("p", default("false"), true);
    let act = retirement(&f, &old, &new);
    let review = reviewed(&f, &act);
    assert_eq!(
        review["confirmation"]["thruline_general_supersession"],
        metadata(&f, &old, &new)
    );
    assert_eq!(review["supersession"], metadata(&f, &old, &new));
}
#[test]
fn synthetic_exact_general_receipt_verifies_and_remains_read_only() {
    let mut f = Fixture::new();
    let old = f.decision("p", default("true"), true);
    let new = f.decision("p", default("false"), true);
    let act = retirement(&f, &old, &new);
    let mut review = reviewed(&f, &act);
    review["confirmation"]["thruline_general_supersession"] = metadata(&f, &old, &new);
    bind_review(&f, &act, &review);
    assert_valid(&f, &act);
    assert!(storage::verified_retirement_for_decision_read_only(
        &f.ledger,
        &old,
        std::time::Duration::from_secs(2)
    )
    .expect("standing")
    .is_some());
}
#[test]
fn later_successor_acceptance_cannot_inherit_exact_old_retirement() {
    let mut f = Fixture::new();
    let old = f.decision("p", default("true"), true);
    let new = f.decision("p", default("false"), true);
    let act = retirement(&f, &old, &new);
    let mut review = reviewed(&f, &act);
    review["confirmation"]["thruline_general_supersession"] = metadata(&f, &old, &new);
    bind_review(&f, &act, &review);
    assert_valid(&f, &act);
    revise(&mut f, &new, "true", true);
    assert!(!bridge::verified(&f.ledger, PersonActKind::Retire, &act).expect("verify"));
    assert!(storage::verified_retirement_for_decision(&f.ledger, &old)
        .expect("standing")
        .is_none());
}
#[test]
fn later_predecessor_acceptance_cannot_be_retired_by_an_old_exact_tuple() {
    let mut f = Fixture::new();
    let old = f.decision("p", default("true"), true);
    let new = f.decision("p", default("false"), true);
    let act = retirement(&f, &old, &new);
    let mut review = reviewed(&f, &act);
    review["confirmation"]["thruline_general_supersession"] = metadata(&f, &old, &new);
    let later = revise(&mut f, &old, "false", false);
    bind_review(&f, &act, &review);
    assert_valid(&f, &act);
    let mut person = person_action::PersonRecorder::bridge_only(&f.ledger, "p");
    person.bind_acceptance(&f.ledger, &later);
    f.people.push(person);
    assert!(!bridge::verified(&f.ledger, PersonActKind::Retire, &act).expect("verify"));
    assert!(storage::accepted_for_project(&f.ledger, "p")
        .expect("active")
        .iter()
        .any(|row| row.id == old && row.revision == 2));
}
#[test]
fn lost_pinned_successor_source_makes_exact_retirement_inert() {
    let mut f = Fixture::new();
    let old = f.decision("p", default("true"), true);
    let new = f.decision("p", default("false"), true);
    let source = f.people.last().expect("person").raw_file().to_owned();
    let act = retirement(&f, &old, &new);
    let mut review = reviewed(&f, &act);
    review["confirmation"]["thruline_general_supersession"] = metadata(&f, &old, &new);
    bind_review(&f, &act, &review);
    assert_valid(&f, &act);
    std::fs::write(source, "{}\n").expect("changed source");
    assert!(!bridge::verified(&f.ledger, PersonActKind::Retire, &act).expect("verify"));
    assert!(storage::verified_retirement_for_decision(&f.ledger, &old)
        .expect("standing")
        .is_none());
}
#[test]
fn exact_review_changes_when_successor_changes_before_authority_writes() {
    let mut f = Fixture::new();
    let old = f.decision("p", default("true"), true);
    let new = f.decision("p", default("false"), true);
    let act = retirement(&f, &old, &new);
    let before = reviewed(&f, &act);
    revise(&mut f, &new, "true", true);
    let after = reviewed(&f, &act);
    assert_ne!(before, after);
    let proofs: i64 = f
        .ledger
        .conn()
        .query_row(
            "SELECT count(*) FROM decision_bridge_proofs WHERE act_row_id=?1",
            [act],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(proofs, 0);
}
#[test]
fn latest_verified_successor_can_be_explicitly_pinned_after_earlier_revisions() {
    let mut f = Fixture::new();
    let old = f.decision("p", default("true"), true);
    let new = f.decision("p", default("false"), true);
    revise(&mut f, &new, "true", true);
    let act = retirement(&f, &old, &new);
    let review = reviewed(&f, &act);
    assert_eq!(
        review["confirmation"]["thruline_general_supersession"]["successor_revision"],
        2
    );
    bind_review(&f, &act, &review);
    assert_valid(&f, &act);
}
#[test]
fn foreign_or_pending_target_cannot_be_reviewed_as_verified_supersession() {
    for project in ["other", "p"] {
        let mut f = Fixture::new();
        let old = f.decision("p", default("true"), true);
        let new = f.decision(project, default("false"), project != "p");
        let act = ulid::Ulid::generate().to_string();
        f.ledger.conn().execute("INSERT INTO decision_retirements(id,decision_id,actor,retired_at,reason,superseded_by) VALUES(?1,?2,'person','2026-10-07T00:04:00Z','exact fixture',?3)",rusqlite::params![act,old,new]).expect("malformed pending fixture");
        assert!(bridge::review_terminal_act(&f.ledger, PersonActKind::Retire, &act, None).is_err());
    }
}

#[test]
fn legacy_retirement_cannot_switch_target_after_old_target_source_is_lost() {
    let mut f = Fixture::new();
    let old = f.decision("p", default("true"), true);
    let new = f.decision("p", default("false"), true);
    let first_source = f.people.last().expect("person").raw_file().to_owned();
    let act = retirement(&f, &old, &new);
    let mut person = person_action::PersonRecorder::bridge_only(&f.ledger, "p");
    person.bind_retirement(&f.ledger, &act);
    f.people.push(person);
    std::fs::write(first_source, "{}\n").expect("lost first target proof");
    revise(&mut f, &new, "true", true);
    assert!(storage::verified_retirement_for_decision(&f.ledger, &old)
        .expect("standing")
        .is_none());
}

#[test]
fn general_pin_hashes_exact_stored_bytes_without_reserializing_json() {
    let mut f = Fixture::new();
    let old = f.decision("p", default("true"), true);
    let new = f.decision("p", default("false"), true);
    let pretty = serde_json::to_string_pretty(&default("false")).expect("pretty params");
    f.ledger.conn().execute(
        "INSERT INTO decision_revisions(decision_id,rev,kind,params_schema_version,params_json,text,sources_json,author_actor,recorded_at) SELECT decision_id,2,kind,params_schema_version,?2,text,sources_json,author_actor,'2026-10-07T00:06:00Z' FROM decision_revisions WHERE decision_id=?1 AND rev=1",
        rusqlite::params![new, pretty],
    ).expect("authored noncanonical stored bytes");
    let acceptance = storage::accept(
        &f.ledger,
        storage::AcceptanceInput {
            decision_id: new.clone(),
            revision: 2,
            actor: "person".into(),
            at: "2026-10-07T00:06:00Z".into(),
            because: "exact bytes".into(),
            severity: "warns".into(),
            applies_from_version: f.version.clone(),
        },
    )
    .expect("acceptance");
    let mut person = person_action::PersonRecorder::bridge_only(&f.ledger, "p");
    person.bind_acceptance(&f.ledger, &acceptance);
    f.people.push(person);
    let act = retirement(&f, &old, &new);
    let review = reviewed(&f, &act);
    let digest = thruline_core::chain::sha256_hex(pretty.as_bytes());
    assert_eq!(
        review["confirmation"]["thruline_general_supersession"]["successor_params_sha256"],
        digest
    );
    assert_ne!(
        digest,
        thruline_core::chain::sha256_hex(
            serde_json::to_string(
                &serde_json::from_str::<serde_json::Value>(&pretty).expect("params")
            )
            .expect("reserialized")
            .as_bytes()
        )
    );
    assert_eq!(
        review["supersession_decisions"]["successor"]["params_sha256"],
        digest
    );
    bind_review(&f, &act, &review);
    assert_valid(&f, &act);
}

#[test]
fn consistent_raw_tuple_cannot_substitute_a_changed_general_pin() {
    for field in [
        "successor_params_sha256",
        "successor_acceptance_id",
        "successor_revision",
        "extra_field",
    ] {
        let mut f = Fixture::new();
        let old = f.decision("p", default("true"), true);
        let new = f.decision("p", default("false"), true);
        let act = retirement(&f, &old, &new);
        let mut review = reviewed(&f, &act);
        review["confirmation"]["thruline_general_supersession"][field] = match field {
            "successor_revision" => serde_json::json!(999),
            _ => serde_json::json!("0".repeat(64)),
        };
        bind_review(&f, &act, &review);
        assert!(!bridge::verified(&f.ledger, PersonActKind::Retire, &act).expect("verify"));
        assert!(storage::verified_retirement_for_decision(&f.ledger, &old)
            .expect("standing")
            .is_none());
    }
}
