//! T-3-2 — the examined-inputs record: written for every examination, never
//! guessing, never edited, and the coverage that refuses to count a result
//! about another version (`B6.2`, `B6.3`, `B6.4`, `B7.2`, `Q4`).
//!
//! Every id is synthetic and no fixture here holds private content.

use rusqlite::params;
use thruline_core::change::{
    self, AcceptedScope, AssociationBasis, AssociationProvenance, ChangeRequest, NewAssociation,
    NewChange, RequestSource, ScopeAcceptance,
};
use thruline_core::change_evidence::{self, CoverageState};
use thruline_core::change_propose::ProfileEnvironment;
use thruline_core::examined_inputs::{
    self as exi, ExaminedInputs, Input, SubjectKind, Target, Uncommitted,
};
use thruline_core::ledger::Ledger;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000p001";
const SESSION: &str = "cccccccc-0000-4000-8000-00000000s001";
const CHANGE: &str = "cccccccc-0000-4000-8000-00000000c001";
const BINDING: &str = "cccccccc-0000-4000-8000-00000000eb01";
const REV_A: &str = "1111111111111111111111111111111111111111";
const REV_B: &str = "2222222222222222222222222222222222222222";
const TREE_A: &str = "/tmp/synthetic-project";

/// A store with one project, one session, one Change that has accepted one
/// criterion about tests, one confirmed association and one captured test run.
fn store() -> (tempfile::TempDir, Ledger) {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open a fresh ledger");
    // The project's working folder is a real folder, and the store says how it
    // was established (T-3-2b): a changed path that is absolute only becomes
    // scope under a root the capture named, so a project whose root is not a
    // folder on this machine has no readable changed paths at all.
    let root_path = dir.path().join("synthetic-project");
    std::fs::create_dir_all(&root_path).expect("the project folder");
    let root = root_path
        .canonicalize()
        .unwrap_or(root_path)
        .display()
        .to_string();
    let conn = ledger.conn();
    conn.execute(
        "INSERT INTO projects(id, root_path, source, display_name, created_at)
         VALUES(?1, ?2, 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
        params![PROJECT, root],
    )
    .expect("project");
    conn.execute(
        "INSERT INTO workspace_identities(project_id, repo_id, basis, resolved_root,
             resolved_via, sessions_sampled, observed_at)
         VALUES(?1,'repo','git',?2,'recorded_cwd',1,'2026-01-01T00:00:00Z')",
        params![PROJECT, root],
    )
    .expect("workspace identity");
    conn.execute(
        "INSERT INTO sessions(id, project_id, file_path, started_at)
         VALUES(?1, ?2, ?3, '2026-01-02T09:30:00Z')",
        params![SESSION, PROJECT, format!("{root}/s.jsonl")],
    )
    .expect("session");
    conn.execute(
        "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name, tool_input,
             raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
         VALUES('e1',?1,?2,1,'2026-01-02T09:40:00Z','tool_use','Edit',?3,?4,1,'aa','','bb')",
        params![
            PROJECT,
            SESSION,
            format!("{{\"file_path\":\"{root}/src/retry.rs\"}}"),
            format!("{root}/s.jsonl")
        ],
    )
    .expect("event");
    conn.execute(
        "INSERT INTO evidence_bindings(id, seq, recorded_at, schema_version, scope_class,
             scope_key, root, repository_identity, pre_identity, post_identity, exit_code,
             term_signal, bound_at, source_session, doc, doc_sha256)
         VALUES(?1,0,'2026-01-03T09:00:00Z',1,'build','bash:/tmp/synthetic-project:cargo clippy retry',
                ?2,'repo','pre','post',0,NULL,'2026-01-03T09:00:00Z',?3,
                '{\"stdout\":{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":1},
                  \"stderr\":{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":1}}','ddd')",
        params![BINDING, TREE_A, SESSION],
    )
    .expect("binding");

    change::insert_change(
        &ledger,
        &NewChange {
            id: CHANGE.to_string(),
            project_id: PROJECT.to_string(),
            opened_at: "2026-01-02T09:00:00Z".to_string(),
            request: ChangeRequest {
                title: "Retry failed deliveries".to_string(),
                purpose: "Synthetic.".to_string(),
                prompt_event_id: None,
                requester: "sam".to_string(),
                source: RequestSource::Person,
            },
        },
    )
    .expect("change");
    change::append_scope(
        &ledger,
        CHANGE,
        "2026-01-02T10:00:00Z",
        &AcceptedScope {
            criteria: vec!["The retry code builds cleanly.".to_string()],
            allowed_scope: vec!["src/**".to_string()],
            check_profile: vec!["cargo-clippy".to_string()],
            ..AcceptedScope::default()
        },
    )
    .expect("scope");
    change::accept_scope(
        &ledger,
        CHANGE,
        1,
        &ScopeAcceptance {
            actor: "sam".to_string(),
            accepted_at: "2026-01-02T10:05:00Z".to_string(),
            rationale: "this is what counts".to_string(),
        },
    )
    .expect("acceptance");
    change::append_association(
        &ledger,
        &NewAssociation {
            id: "assoc1".to_string(),
            change_id: CHANGE.to_string(),
            session_id: SESSION.to_string(),
            provenance: AssociationProvenance::Confirmed,
            basis: AssociationBasis::User,
            actor: Some("sam".to_string()),
            asserted_at: "2026-01-02T11:00:00Z".to_string(),
            reason: Some("mine".to_string()),
        },
    )
    .expect("association");
    (dir, ledger)
}

fn source_record(kind: SubjectKind, id: &str, revision: &str) -> ExaminedInputs {
    let mut inputs = ExaminedInputs::unknown_for(kind, id);
    inputs.base_revision = Input::known(revision);
    inputs.source_identity = Input::known(format!("wsid1:{revision}"));
    inputs.worktree_path = Input::known(TREE_A);
    inputs.uncommitted = Uncommitted::Clean;
    inputs
}

fn coverage(ledger: &Ledger) -> thruline_core::change_evidence::CoverageView {
    let at = "2026-01-03T10:00:00Z";
    change_evidence::derive_links(ledger, CHANGE, at).expect("derive");
    let mut environment = ProfileEnvironment::empty();
    environment.tools_present.insert("cargo".to_string());
    environment
        .recipes_present
        .insert("cargo-clippy".to_string());
    change_evidence::coverage(ledger, CHANGE, &environment).expect("coverage")
}

// ---------------------------------------------------------------------------
// The record itself
// ---------------------------------------------------------------------------

#[test]
fn a_record_is_written_for_an_examination_and_read_back_whole() {
    let (_dir, ledger) = store();
    let inputs = source_record(SubjectKind::CheckRun, BINDING, REV_A);
    let outcome = exi::record(&ledger, &inputs, "2026-01-03T09:05:00Z").expect("record");
    assert!(outcome.inserted);

    let back = exi::standing(&ledger, SubjectKind::CheckRun, BINDING)
        .expect("read")
        .expect("a record must be there");
    assert_eq!(back.base_revision, Input::known(REV_A));
    assert_eq!(back.worktree_path, Input::known(TREE_A));
    assert_eq!(back.uncommitted, Uncommitted::Clean);
    assert_eq!(back.target, Target::None);

    // Content-addressed: the same record written twice inserts once.
    let again = exi::record(&ledger, &inputs, "2026-01-03T09:06:00Z").expect("record again");
    assert!(!again.inserted);
    assert_eq!(again.id, outcome.id);
    assert_eq!(
        exi::record_count(&ledger, SubjectKind::CheckRun, BINDING).expect("count"),
        1
    );
}

#[test]
fn an_examination_that_does_not_exist_cannot_have_a_record() {
    let (_dir, ledger) = store();
    let inputs = source_record(SubjectKind::CheckRun, "no-such-binding", REV_A);
    let err = exi::record(&ledger, &inputs, "2026-01-03T09:05:00Z")
        .expect_err("a record about nothing must be refused");
    assert!(
        format!("{err}").contains("examination that exists"),
        "{err}"
    );
}

#[test]
fn a_record_is_never_edited_or_removed() {
    let (_dir, ledger) = store();
    let inputs = source_record(SubjectKind::CheckRun, BINDING, REV_A);
    let id = exi::record(&ledger, &inputs, "2026-01-03T09:05:00Z")
        .expect("record")
        .id;
    let update = ledger.conn().execute(
        "UPDATE examination_inputs SET base_revision=?1 WHERE id=?2",
        params![REV_B, id],
    );
    assert!(update.is_err(), "an update must be refused");
    let delete = ledger
        .conn()
        .execute("DELETE FROM examination_inputs WHERE id=?1", params![id]);
    assert!(delete.is_err(), "a delete must be refused");

    // A second examination of the same thing APPENDS, and the standing one is
    // the later: B6.7 needs the old reading to still be there.
    let later = source_record(SubjectKind::CheckRun, BINDING, REV_B);
    exi::record(&ledger, &later, "2026-01-03T10:00:00Z").expect("append");
    assert_eq!(
        exi::record_count(&ledger, SubjectKind::CheckRun, BINDING).expect("count"),
        2
    );
    let standing = exi::standing(&ledger, SubjectKind::CheckRun, BINDING)
        .expect("read")
        .expect("some");
    assert_eq!(standing.base_revision, Input::known(REV_B));
}

#[test]
fn the_word_unknown_is_refused_as_a_value_by_the_module_and_by_the_database() {
    let (_dir, ledger) = store();
    let mut inputs = source_record(SubjectKind::CheckRun, BINDING, REV_A);
    inputs.base_revision = Input::known(exi::NOT_A_VALUE);
    let err = exi::record(&ledger, &inputs, "2026-01-03T09:05:00Z")
        .expect_err("the module must refuse the word");
    assert!(format!("{err}").contains("unknown"), "{err}");

    // And again at the store, for a caller that bypasses the module.
    let direct = ledger.conn().execute(
        "INSERT INTO examination_inputs(id, subject_kind, subject_id, recorded_at,
             schema_version, base_revision, uncommitted_state, config_files_json,
             tools_json, target_kind)
         VALUES('x','check_run',?1,'2026-01-03T09:05:00Z',1,'unknown','clean','[]','[]','none')",
        params![BINDING],
    );
    assert!(
        direct.is_err(),
        "the database must refuse the literal word too"
    );
}

#[test]
fn an_examination_that_looked_at_no_app_cannot_carry_facts_about_one() {
    let (_dir, ledger) = store();
    let direct = ledger.conn().execute(
        "INSERT INTO examination_inputs(id, subject_kind, subject_id, recorded_at,
             schema_version, uncommitted_state, config_files_json, tools_json,
             target_kind, app_owner_exe)
         VALUES('x','check_run',?1,'2026-01-03T09:05:00Z',1,'clean','[]','[]','none','/bin/serve')",
        params![BINDING],
    );
    assert!(direct.is_err(), "app facts without an app must be refused");
}

#[test]
fn a_clean_tree_and_an_unreadable_one_are_different_facts() {
    let (_dir, ledger) = store();
    let mut clean = source_record(SubjectKind::CheckRun, BINDING, REV_A);
    clean.uncommitted = Uncommitted::Clean;
    exi::record(&ledger, &clean, "2026-01-03T09:05:00Z").expect("clean");
    let back = exi::standing(&ledger, SubjectKind::CheckRun, BINDING)
        .expect("read")
        .expect("some");
    assert_eq!(back.uncommitted, Uncommitted::Clean);

    let mut unreadable = source_record(SubjectKind::CheckRun, BINDING, REV_B);
    unreadable.uncommitted = Uncommitted::Unknown;
    exi::record(&ledger, &unreadable, "2026-01-03T09:06:00Z").expect("unknown");
    let back = exi::standing(&ledger, SubjectKind::CheckRun, BINDING)
        .expect("read")
        .expect("some");
    assert_eq!(back.uncommitted, Uncommitted::Unknown);
}

// ---------------------------------------------------------------------------
// What the Verify view counts, and what it refuses to count
// ---------------------------------------------------------------------------

#[test]
fn a_result_from_the_candidates_own_version_counts() {
    let (_dir, ledger) = store();
    exi::record(
        &ledger,
        &source_record(SubjectKind::Intended, CHANGE, REV_A),
        "2026-01-03T08:00:00Z",
    )
    .expect("intended");
    exi::record(
        &ledger,
        &source_record(SubjectKind::CheckRun, BINDING, REV_A),
        "2026-01-03T09:05:00Z",
    )
    .expect("examined");

    let view = coverage(&ledger);
    let one = &view.criteria[0];
    assert_eq!(one.state, CoverageState::Evidence, "{one:?}");
    assert_eq!(one.items.len(), 1);
    assert_eq!(one.items[0].applies, Some(true));
    assert_eq!(one.items[0].applies_slug, Some("inputs_applies"));
}

#[test]
fn a_result_from_a_different_running_copy_of_the_app_counts_for_nothing() {
    let (_dir, ledger) = store();
    let mut intended = source_record(SubjectKind::Intended, CHANGE, REV_A);
    intended.target = Target::RunningApp {
        port: 45781,
        owner_pid: Input::known("101"),
        owner_exe: Input::known("/tmp/synthetic-project/target/debug/serve"),
        database_path: Input::known("/tmp/synthetic-project/data/outbox.sqlite3"),
    };
    exi::record(&ledger, &intended, "2026-01-03T08:00:00Z").expect("intended");

    let mut examined = source_record(SubjectKind::CheckRun, BINDING, REV_A);
    examined.target = Target::RunningApp {
        port: 45782,
        owner_pid: Input::known("202"),
        owner_exe: Input::known("/tmp/synthetic-project-sibling/target/debug/serve"),
        database_path: Input::known("/tmp/synthetic-project-sibling/data/outbox.sqlite3"),
    };
    exi::record(&ledger, &examined, "2026-01-03T09:05:00Z").expect("examined");

    let view = coverage(&ledger);
    let one = &view.criteria[0];
    assert_eq!(
        one.state,
        CoverageState::OtherVersion,
        "a result about another running copy is not evidence here: {one:?}"
    );
    assert_eq!(one.items[0].applies, Some(false));
    assert_eq!(one.items[0].applies_slug, Some("inputs_other_app"));
    // The item is still shown. It is not deleted to make the screen tidy.
    assert_eq!(one.items.len(), 1);
}

#[test]
fn a_result_from_a_different_version_of_the_code_counts_for_nothing() {
    let (_dir, ledger) = store();
    exi::record(
        &ledger,
        &source_record(SubjectKind::Intended, CHANGE, REV_A),
        "2026-01-03T08:00:00Z",
    )
    .expect("intended");
    exi::record(
        &ledger,
        &source_record(SubjectKind::CheckRun, BINDING, REV_B),
        "2026-01-03T09:05:00Z",
    )
    .expect("examined");

    let view = coverage(&ledger);
    let one = &view.criteria[0];
    assert_eq!(one.state, CoverageState::OtherVersion);
    assert_eq!(one.items[0].applies_slug, Some("inputs_other_revision"));
}

#[test]
fn an_examination_with_no_record_at_all_counts_for_nothing() {
    let (_dir, ledger) = store();
    exi::record(
        &ledger,
        &source_record(SubjectKind::Intended, CHANGE, REV_A),
        "2026-01-03T08:00:00Z",
    )
    .expect("intended");
    // No record for the binding.
    let view = coverage(&ledger);
    let one = &view.criteria[0];
    assert_eq!(one.state, CoverageState::OtherVersion);
    assert_eq!(one.items[0].applies, Some(false));
    assert_eq!(one.items[0].applies_slug, Some("inputs_missing"));
}

#[test]
fn a_change_that_has_not_said_what_it_is_about_invents_no_mismatch() {
    let (_dir, ledger) = store();
    // No intended record. Nothing is compared, and nothing is accused.
    let view = coverage(&ledger);
    let one = &view.criteria[0];
    assert_eq!(one.items[0].applies, None);
    assert_eq!(one.items[0].applies_slug, None);
    assert_eq!(one.state, CoverageState::Evidence);
}

// ---------------------------------------------------------------------------
// The Prepare view
// ---------------------------------------------------------------------------

#[test]
fn the_prepare_view_says_nothing_is_written_down_before_anything_is() {
    let (_dir, ledger) = store();
    let view = exi::prepare_view(&ledger, CHANGE).expect("prepare view");
    assert!(view.inputs.is_none());
    assert_eq!(
        view.lines.iter().map(|l| l.slug).collect::<Vec<_>>(),
        vec!["prepare_nothing_recorded"],
        "an empty stage teaches: it says what would put something in it"
    );
    assert_eq!(view.record_count, 0);
}

#[test]
fn the_prepare_view_names_the_folder_the_starting_point_and_the_unsaved_changes() {
    let (_dir, ledger) = store();
    let mut intended = source_record(SubjectKind::Intended, CHANGE, REV_A);
    intended.uncommitted = Uncommitted::Present {
        files: 2,
        diff_sha256: Input::known("dd"),
    };
    intended.target = Target::RunningApp {
        port: 45781,
        owner_pid: Input::known("101"),
        owner_exe: Input::known("/tmp/synthetic-project/target/debug/serve"),
        database_path: Input::known("/tmp/synthetic-project/data/outbox.sqlite3"),
    };
    exi::record(&ledger, &intended, "2026-01-03T08:00:00Z").expect("intended");

    let view = exi::prepare_view(&ledger, CHANGE).expect("prepare view");
    let slugs: Vec<&str> = view.lines.iter().map(|l| l.slug).collect();
    assert_eq!(
        slugs,
        vec![
            "prepare_repository",
            "prepare_base",
            "prepare_uncommitted_included",
            "prepare_running_app"
        ]
    );
    assert_eq!(view.app_port, Some(45781));
    assert_eq!(view.app_program.as_deref(), Some("serve"));
    assert_eq!(
        view.lines[1].value.as_deref(),
        Some(REV_A),
        "the whole revision is served; a surface shortens it"
    );
    assert_eq!(view.lines[2].value.as_deref(), Some("2"));
}

#[test]
fn a_clean_tree_says_so_and_an_unreadable_one_says_something_else() {
    let (_dir, ledger) = store();
    let mut clean = source_record(SubjectKind::Intended, CHANGE, REV_A);
    clean.uncommitted = Uncommitted::Clean;
    exi::record(&ledger, &clean, "2026-01-03T08:00:00Z").expect("clean");
    let view = exi::prepare_view(&ledger, CHANGE).expect("view");
    assert_eq!(view.lines[2].slug, "prepare_uncommitted_none");

    let mut unreadable = source_record(SubjectKind::Intended, CHANGE, REV_B);
    unreadable.uncommitted = Uncommitted::Unknown;
    exi::record(&ledger, &unreadable, "2026-01-03T08:10:00Z").expect("unknown");
    let view = exi::prepare_view(&ledger, CHANGE).expect("view");
    assert_eq!(view.lines[2].slug, "prepare_uncommitted_unknown");
}

#[test]
fn the_served_prepare_view_is_stable() {
    let (_dir, ledger) = store();
    let mut intended = source_record(SubjectKind::Intended, CHANGE, REV_A);
    intended.uncommitted = Uncommitted::Present {
        files: 2,
        diff_sha256: Input::known("dd"),
    };
    intended.tools = vec![
        exi::ToolInput {
            name: "cargo".to_string(),
            version: Input::known("cargo 1.97.1"),
            present: true,
        },
        exi::ToolInput {
            name: "ruff".to_string(),
            version: Input::Unknown,
            present: false,
        },
    ];
    intended.target = Target::RunningApp {
        port: 45781,
        owner_pid: Input::known("101"),
        owner_exe: Input::known("/tmp/synthetic-project/target/debug/serve"),
        database_path: Input::known("/tmp/synthetic-project/data/outbox.sqlite3"),
    };
    exi::record(&ledger, &intended, "2026-01-03T08:00:00Z").expect("intended");
    let view = exi::prepare_view(&ledger, CHANGE).expect("view");
    insta::assert_json_snapshot!(view);
}
