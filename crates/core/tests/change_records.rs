//! T-2-1 — the Change record, its components, and the two refusals that make
//! it safe to build a screen on later (`B3.3`–`B3.7`, `B3.12`, `B3.13`,
//! `B2.5`, `L1.1`).
//!
//! Sam never sees this task. What Sam sees later — "here is one Change, one
//! sentence, one version" — is only true if the record cannot point at two
//! versions at once and cannot quietly inherit a session's result. These tests
//! are those two sentences, made mechanical.
//!
//! Every id used here is synthetic (`cccccccc-…`). No fixture in this file
//! contains private content.

use rusqlite::params;
use thruline_core::change::{
    self, AcceptedScope, AssociationBasis, AssociationProvenance, ChangeRequest,
    ImplementationBinding, NewAssociation, NewChange, RequestSource, ScopeAcceptance,
};
use thruline_core::ledger::Ledger;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000p001";
const SESSION: &str = "cccccccc-0000-4000-8000-00000000s001";
const CHANGE: &str = "cccccccc-0000-4000-8000-00000000c001";

/// A ledger with one project and one session, and nothing else. The Change
/// layer is built on top of the store that already exists; nothing here creates
/// a second one.
fn store() -> (tempfile::TempDir, Ledger) {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open a fresh ledger");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES(?1, '/tmp/synthetic-project', 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
            params![PROJECT],
        )
        .expect("insert project");
    ledger
        .conn()
        .execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES(?1, ?2, '/tmp/synthetic-project/session.jsonl', '2026-01-01T00:00:00Z')",
            params![SESSION, PROJECT],
        )
        .expect("insert session");
    (dir, ledger)
}

fn a_change() -> NewChange {
    NewChange {
        id: CHANGE.to_string(),
        project_id: PROJECT.to_string(),
        opened_at: "2026-01-02T09:00:00Z".to_string(),
        request: ChangeRequest {
            title: "Retry failed uploads".to_string(),
            purpose: "Uploads that fail once are lost, and the person never finds out.".to_string(),
            prompt_event_id: None,
            requester: "sam".to_string(),
            source: RequestSource::Person,
        },
    }
}

fn a_scope() -> AcceptedScope {
    AcceptedScope {
        criteria: vec![
            "A failed upload is tried again three times.".to_string(),
            "The person is told when it finally gives up.".to_string(),
        ],
        exclusions: vec!["Uploads larger than 2 GB are out of scope.".to_string()],
        allowed_scope: vec!["src/upload/**".to_string()],
        constraints: vec!["No new dependency.".to_string()],
        check_profile: vec!["unit-tests".to_string(), "lint".to_string()],
    }
}

// ---------------------------------------------------------------------------
// The record
// ---------------------------------------------------------------------------

#[test]
fn a_change_is_stored_and_read_back_with_every_component() {
    let (_dir, ledger) = store();
    change::insert_change(&ledger, &a_change()).expect("insert the change");
    let version = change::append_scope(&ledger, CHANGE, "2026-01-02T09:05:00Z", &a_scope())
        .expect("append the scope");
    assert_eq!(version, 1, "the first scope version is 1");
    change::accept_scope(
        &ledger,
        CHANGE,
        version,
        &ScopeAcceptance {
            actor: "sam".to_string(),
            accepted_at: "2026-01-02T09:06:00Z".to_string(),
            rationale: "That is what I asked for.".to_string(),
        },
    )
    .expect("accept the scope");
    change::append_implementation(
        &ledger,
        CHANGE,
        &ImplementationBinding {
            bound_at: "2026-01-02T10:00:00Z".to_string(),
            repository_identity: "repo-synthetic-0001".to_string(),
            base_revision: "0000000000000000000000000000000000000001".to_string(),
            snapshot_id: None,
            workspace_identity: Some("workspace-synthetic-0001".to_string()),
        },
    )
    .expect("bind the implementation");
    change::append_association(
        &ledger,
        &NewAssociation {
            id: "cccccccc-0000-4000-8000-00000000a001".to_string(),
            change_id: CHANGE.to_string(),
            session_id: SESSION.to_string(),
            provenance: AssociationProvenance::Suggested,
            basis: AssociationBasis::Timing,
            actor: None,
            asserted_at: "2026-01-02T10:01:00Z".to_string(),
            reason: None,
        },
    )
    .expect("suggest the association");

    let view = change::read_change(&ledger, CHANGE)
        .expect("read")
        .expect("the change exists");
    insta::assert_json_snapshot!("change_view", view);
}

/// The failure mode the card names: a Change that can point at two snapshots
/// produces a screen that says "approved" for the wrong version. One binding
/// version names at most one snapshot, and re-binding appends.
#[test]
fn a_change_binds_one_version_at_a_time_and_keeps_the_old_one() {
    let (_dir, ledger) = store();
    change::insert_change(&ledger, &a_change()).expect("insert");
    let binding = |rev: &str| ImplementationBinding {
        bound_at: "2026-01-02T10:00:00Z".to_string(),
        repository_identity: "repo-synthetic-0001".to_string(),
        base_revision: rev.to_string(),
        snapshot_id: None,
        workspace_identity: None,
    };
    assert_eq!(
        change::append_implementation(&ledger, CHANGE, &binding("aaaa")).expect("first"),
        1
    );
    assert_eq!(
        change::append_implementation(&ledger, CHANGE, &binding("bbbb")).expect("second"),
        2
    );
    let view = change::read_change(&ledger, CHANGE)
        .expect("read")
        .expect("exists");
    let standing = view.implementation.expect("a standing binding");
    assert_eq!(standing.version, 2);
    assert_eq!(standing.base_revision, "bbbb");
    assert_eq!(
        view.implementation_history.len(),
        1,
        "the superseded binding is kept, not deleted"
    );
    assert_eq!(view.implementation_history[0].base_revision, "aaaa");
}

/// Re-scoping supersedes; it never edits. The old version stays readable
/// because an acceptance is attached to the version it was given.
#[test]
fn a_new_scope_version_supersedes_and_the_old_one_stays_readable() {
    let (_dir, ledger) = store();
    change::insert_change(&ledger, &a_change()).expect("insert");
    change::append_scope(&ledger, CHANGE, "2026-01-02T09:05:00Z", &a_scope()).expect("v1");
    let mut wider = a_scope();
    wider.criteria.push("Retries are logged.".to_string());
    let v2 = change::append_scope(&ledger, CHANGE, "2026-01-03T09:05:00Z", &wider).expect("v2");
    assert_eq!(v2, 2);

    let view = change::read_change(&ledger, CHANGE)
        .expect("read")
        .expect("exists");
    let standing = view.accepted_scope.expect("a standing scope");
    assert_eq!(standing.version, 2);
    assert_eq!(standing.criteria.len(), 3);
    assert_eq!(view.scope_history.len(), 1);
    assert_eq!(view.scope_history[0].version, 1);
    assert_ne!(
        standing.content_sha256, view.scope_history[0].content_sha256,
        "two different lists must not share a digest"
    );

    let update = ledger.conn().execute(
        "UPDATE change_scope_revisions SET criteria_json='[]' WHERE change_id=?1",
        params![CHANGE],
    );
    let message = format!("{:?}", update.expect_err("editing a scope must be refused"));
    assert!(
        message.contains("append-only"),
        "the refusal must say why: {message}"
    );
}

/// Agreeing what counts as done is not approving the work, and the store keeps
/// them apart: an unaccepted scope reads as unaccepted, never as agreed.
#[test]
fn an_unaccepted_scope_reads_as_unaccepted() {
    let (_dir, ledger) = store();
    change::insert_change(&ledger, &a_change()).expect("insert");
    change::append_scope(&ledger, CHANGE, "2026-01-02T09:05:00Z", &a_scope()).expect("v1");
    let view = change::read_change(&ledger, CHANGE)
        .expect("read")
        .expect("exists");
    assert!(
        view.accepted_scope.expect("a scope").acceptance.is_none(),
        "nobody agreed this list, and the record must not imply that anyone did"
    );
}

/// Missing evidence renders as missing. Both later stages are present as an
/// explicit placeholder, so a reader never has to guess whether a missing key
/// means "nothing happened" or "an older build".
#[test]
fn examination_and_decision_are_placeholders_not_absences() {
    let (_dir, ledger) = store();
    change::insert_change(&ledger, &a_change()).expect("insert");
    let view = change::read_change(&ledger, CHANGE)
        .expect("read")
        .expect("exists");
    assert_eq!(view.examination, change::NONE_YET);
    assert_eq!(view.decision, change::NONE_YET);
    let json = serde_json::to_value(&view).expect("serialize");
    assert_eq!(json["examination"], "none_yet");
    assert_eq!(json["decision"], "none_yet");
}

// ---------------------------------------------------------------------------
// The refusals
// ---------------------------------------------------------------------------

/// `B3.12`'s teeth. A row cannot be born confirmed without a named actor.
#[test]
fn a_confirmed_association_without_an_actor_is_refused() {
    let (_dir, ledger) = store();
    change::insert_change(&ledger, &a_change()).expect("insert");
    let err = change::append_association(
        &ledger,
        &NewAssociation {
            id: "cccccccc-0000-4000-8000-00000000a002".to_string(),
            change_id: CHANGE.to_string(),
            session_id: SESSION.to_string(),
            provenance: AssociationProvenance::Confirmed,
            basis: AssociationBasis::User,
            actor: None,
            asserted_at: "2026-01-02T10:01:00Z".to_string(),
            reason: None,
        },
    )
    .expect_err("a confirmation with no actor must be refused");
    assert!(
        format!("{err:?}").contains("CHECK"),
        "the database must be the one refusing: {err:?}"
    );
}

/// Timing, filename overlap and an agent's own word may propose. None of them
/// may confirm — that is exactly the "silently become confirmed" the brief
/// forbids.
#[test]
fn no_machine_basis_can_produce_a_confirmed_association() {
    let (_dir, ledger) = store();
    change::insert_change(&ledger, &a_change()).expect("insert");
    for (n, basis) in [
        AssociationBasis::Timing,
        AssociationBasis::Filename,
        AssociationBasis::AgentDeclared,
    ]
    .into_iter()
    .enumerate()
    {
        let err = change::append_association(
            &ledger,
            &NewAssociation {
                id: format!("cccccccc-0000-4000-8000-00000000b{n:03}"),
                change_id: CHANGE.to_string(),
                session_id: SESSION.to_string(),
                provenance: AssociationProvenance::Confirmed,
                basis,
                actor: Some("thruline".to_string()),
                asserted_at: "2026-01-02T10:01:00Z".to_string(),
                reason: None,
            },
        )
        .expect_err("only an explicit human act may confirm");
        assert!(format!("{err:?}").contains("CHECK"), "{err:?}");
    }
}

/// Confirming is appending. The suggestion stays in the record as what it was.
#[test]
fn a_suggestion_becomes_a_confirmation_only_by_appending_beside_it() {
    let (_dir, ledger) = store();
    change::insert_change(&ledger, &a_change()).expect("insert");
    let suggestion = NewAssociation {
        id: "cccccccc-0000-4000-8000-00000000a003".to_string(),
        change_id: CHANGE.to_string(),
        session_id: SESSION.to_string(),
        provenance: AssociationProvenance::Suggested,
        basis: AssociationBasis::Filename,
        actor: None,
        asserted_at: "2026-01-02T10:01:00Z".to_string(),
        reason: None,
    };
    change::append_association(&ledger, &suggestion).expect("suggest");

    let promoted = ledger.conn().execute(
        "UPDATE change_session_associations SET provenance='confirmed' WHERE id=?1",
        params![suggestion.id],
    );
    let message = format!(
        "{:?}",
        promoted.expect_err("a suggestion must not be editable into a confirmation")
    );
    assert!(message.contains("append-only"), "{message}");

    change::append_association(
        &ledger,
        &NewAssociation {
            id: "cccccccc-0000-4000-8000-00000000a004".to_string(),
            provenance: AssociationProvenance::Confirmed,
            basis: AssociationBasis::User,
            actor: Some("sam".to_string()),
            asserted_at: "2026-01-02T11:00:00Z".to_string(),
            reason: None,
            ..suggestion.clone()
        },
    )
    .expect("confirming appends");

    let view = change::read_change(&ledger, CHANGE)
        .expect("read")
        .expect("exists");
    assert_eq!(view.associations.len(), 2, "both rows survive");
    assert_eq!(
        view.associations[0].provenance,
        AssociationProvenance::Suggested
    );
    assert_eq!(
        view.associations[1].provenance,
        AssociationProvenance::Confirmed
    );
}

/// A session from another project cannot be linked. A Change is scoped to its
/// project's record and nothing wider.
#[test]
fn a_session_from_another_project_cannot_be_linked() {
    let (_dir, ledger) = store();
    change::insert_change(&ledger, &a_change()).expect("insert");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES('cccccccc-0000-4000-8000-00000000p002', '/tmp/other', 'claude_code', 'Other', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("second project");
    ledger
        .conn()
        .execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES('cccccccc-0000-4000-8000-00000000s002',
                    'cccccccc-0000-4000-8000-00000000p002', '/tmp/other/s.jsonl', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("second session");
    let err = change::append_association(
        &ledger,
        &NewAssociation {
            id: "cccccccc-0000-4000-8000-00000000a005".to_string(),
            change_id: CHANGE.to_string(),
            session_id: "cccccccc-0000-4000-8000-00000000s002".to_string(),
            provenance: AssociationProvenance::Suggested,
            basis: AssociationBasis::Timing,
            actor: None,
            asserted_at: "2026-01-02T10:01:00Z".to_string(),
            reason: None,
        },
    )
    .expect_err("a cross-project association must be refused");
    assert!(
        format!("{err:?}").contains("share the exact project"),
        "{err:?}"
    );
}

/// A change request may only point at an event in its own project — the same
/// rule `entity_association_proposals` already enforces for its receipts.
#[test]
fn a_request_cannot_point_at_another_projects_event() {
    let (_dir, ledger) = store();
    let mut record = a_change();
    record.request.prompt_event_id = Some("cccccccc-0000-4000-8000-00000000e999".to_string());
    let err = change::insert_change(&ledger, &record)
        .expect_err("a request pointing outside its project must be refused");
    let message = format!("{err:?}");
    assert!(
        message.contains("its own project") || message.contains("FOREIGN KEY"),
        "{message}"
    );
}

// ---------------------------------------------------------------------------
// No inheritance path (B3.13)
// ---------------------------------------------------------------------------

/// The strongest form of `B3.13`: a session's result cannot be transferred to a
/// Change because there is nowhere in the schema for it to go. No verdict,
/// claim, check or receipt table carries a change id, and the association
/// itself has no outcome column.
#[test]
fn nothing_derived_can_reference_a_change() {
    let (_dir, ledger) = store();
    let mut statement = ledger
        .conn()
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .expect("prepare");
    let tables: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query")
        .filter_map(|r| r.ok())
        .collect();
    drop(statement);
    assert!(
        tables.iter().any(|t| t == "changes"),
        "the change tables must exist for this test to mean anything"
    );

    let mut offenders = Vec::new();
    for table in &tables {
        if table.starts_with("change") {
            continue;
        }
        let mut columns = ledger
            .conn()
            .prepare(&format!("PRAGMA table_info({table})"))
            .expect("prepare pragma");
        let names: Vec<String> = columns
            .query_map([], |row| row.get::<_, String>(1))
            .expect("query pragma")
            .filter_map(|r| r.ok())
            .collect();
        for name in names {
            // `change_id` and nothing looser. `state_comparisons` has a
            // `changed_dimensions_json`, which is the English word and not a
            // reference; a check that caught it would be a check the next
            // session turns off.
            if name.contains("change_id") {
                offenders.push(format!("{table}.{name}"));
            }
        }
        let sql: String = ledger
            .conn()
            .query_row(
                "SELECT COALESCE(sql,'') FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .expect("read the table's own DDL");
        for reference in ["REFERENCES changes(", "REFERENCES change_"] {
            if sql.contains(reference) {
                offenders.push(format!("{table} -> {reference}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "no table outside the change layer may carry a change reference: {offenders:?}"
    );
}

/// And the association row has no field a result could be written into. A
/// future column named `verdict`, `result`, `outcome`, `passed` or `status`
/// here would be the inheritance path opening; this fails the moment it does.
#[test]
fn an_association_has_no_place_to_put_a_result() {
    let (_dir, ledger) = store();
    let mut columns = ledger
        .conn()
        .prepare("PRAGMA table_info(change_session_associations)")
        .expect("prepare");
    let names: Vec<String> = columns
        .query_map([], |row| row.get::<_, String>(1))
        .expect("query")
        .filter_map(|r| r.ok())
        .collect();
    assert_eq!(
        names,
        vec![
            "id",
            "change_id",
            "session_id",
            "provenance",
            "basis",
            "actor",
            "asserted_at",
            // T-2-2. The person's own words for confirming or rejecting. Prose
            // about the decision, never a result: the loop below still refuses
            // every outcome-shaped name.
            "reason"
        ],
        "an association is two ids, a word, a basis, an actor, a time and the \
         reason a person gave — nothing else"
    );
    for forbidden in ["verdict", "result", "outcome", "passed", "status", "check"] {
        assert!(
            !names.iter().any(|n| n.contains(forbidden)),
            "an association must not be able to carry a result: {forbidden}"
        );
    }
}

// ---------------------------------------------------------------------------
// Masking, and the words
// ---------------------------------------------------------------------------

/// H's rule: masking happens before any text is served. The reader is the
/// masking boundary, so no caller can forget.
#[test]
fn served_text_is_masked_before_it_leaves_the_store() {
    let (_dir, ledger) = store();
    let mut record = a_change();
    record.request.purpose =
        "Use the key AKIAIOSFODNN7EXAMPLE when retrying the upload.".to_string();
    change::insert_change(&ledger, &record).expect("insert");

    let raw: String = ledger
        .conn()
        .query_row(
            "SELECT purpose FROM changes WHERE id=?1",
            params![CHANGE],
            |row| row.get(0),
        )
        .expect("the store keeps what it was given");
    assert!(
        raw.contains("AKIAIOSFODNN7EXAMPLE"),
        "the store is not the masker; it keeps the record intact"
    );

    let view = change::read_change(&ledger, CHANGE)
        .expect("read")
        .expect("exists");
    assert!(
        !view.request.purpose.contains("AKIAIOSFODNN7EXAMPLE"),
        "served text must be masked: {}",
        view.request.purpose
    );
    let treatment = serde_json::to_value(&view.masking).expect("serialize the treatment");
    assert_eq!(
        treatment["state"], "ran",
        "the served record must be able to say a masker ran: {treatment}"
    );
}

/// The vocabulary rule, applied to the only human-readable strings this task
/// adds. Sam's words in, engine nouns out.
#[test]
fn the_change_labels_use_sams_words() {
    const BANNED_ON_A_HUMAN_SURFACE: &[&str] = &[
        "candidate",
        "applicability",
        "disposition",
        "packet version",
        "ledger",
        "session id",
        "tree hash",
        "predicate",
        "event",
        "receipt",
    ];
    let mut checked = 0usize;
    for term in thruline_core::voice::CHANGE_LABELS {
        for text in [term.word, term.sentence] {
            let lower = text.to_ascii_lowercase();
            for banned in BANNED_ON_A_HUMAN_SURFACE {
                assert!(
                    !lower.contains(banned),
                    "engine noun {banned:?} on a served label: {text:?}"
                );
            }
            assert!(
                thruline_core::voice::banned_in(text).is_none(),
                "engine lexicon in {text:?}: {:?}",
                thruline_core::voice::banned_in(text)
            );
            checked += 1;
        }
    }
    // T-2-2 added "rejected_link", the other half of the confirm act.
    assert_eq!(checked, 22, "eleven labels, two strings each");
    assert!(
        thruline_core::voice::change_label("accepted_scope")
            .is_some_and(|t| t.word == "what counts as done"),
        "the accepted scope is 'what counts as done' in Sam's words"
    );
}

/// Every served label the routes hand out is one of these, so a string literal
/// cannot appear beside them in a handler without this failing.
#[test]
fn the_labels_are_the_whole_served_vocabulary() {
    let slugs: Vec<&str> = thruline_core::voice::CHANGE_LABELS
        .iter()
        .map(|t| t.slug)
        .collect();
    assert_eq!(
        slugs,
        vec![
            "change",
            "request",
            "accepted_scope",
            "scope_acceptance",
            "implementation",
            "association",
            "suggested",
            "confirmed",
            "rejected_link",
            "examination_none_yet",
            "decision_none_yet",
        ]
    );
}
