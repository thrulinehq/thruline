//! T-3-5 verifier finding 2 — **a check a person stopped is named as stopped**,
//! derived rather than asserted.
//!
//! The table in `change_readiness.rs` hands `compute` a coverage row it built
//! itself. That is a fine test of the rule and a useless test of the routing:
//! the live product read a cancelled check as "something ran, and it settled
//! nothing about this", and the table could not see it, because the table
//! never asked `coverage()` what shape a stopped run really has.
//!
//! So every fixture below goes through the real derivation — real ledger,
//! real `evidence_bindings` row, real `change_run_executions` rows written by
//! the engine's own writers, `derive_links`, `coverage()`, `readiness()` — and
//! the assertion is made twice: on the typed model, and on the JSON the route
//! serves from it, which is what a screen actually reads.
//!
//! Every string here is synthetic. Nothing came off a real machine.

use thruline_core::change_evidence::{coverage, derive_links, CoverageState};
use thruline_core::change_propose::ProfileEnvironment;
use thruline_core::change_readiness::{readiness, Unsatisfied};
use thruline_core::examined_inputs::{self, ExaminedInputs, Input, SubjectKind, Uncommitted};
use thruline_core::ledger::Ledger;
use thruline_core::run_execution::{self, ExecutionState, Finish, Reason, Start};

const AT: &str = "2026-02-01T12:00:00Z";
/// The one accepted line, and the class it names is the one the run has.
const CRITERION: &str = "The test suite passes on the packages that changed.";
/// `check` starts an execution under `run_key = source_session` and stores the
/// binding it produced under that same `source_session`. That equality is the
/// join the fix rests on, so the fixture spells it once.
const RUN_KEY: &str = "s_run";

struct Fixture {
    _dir: tempfile::TempDir,
    ledger: Ledger,
}

/// One examined-inputs record, with every field the applicability predicate
/// asks about known, so two of these compare equal rather than "unknown".
fn inputs_for(kind: SubjectKind, subject: &str) -> ExaminedInputs {
    let mut inputs = ExaminedInputs::unknown_for(kind, subject);
    // `base_revision` rather than `snapshot_id`: readiness takes either as the
    // version it is about, and a snapshot id is a foreign key into a table
    // this fixture has no business inventing rows in.
    inputs.base_revision = Input::known("e71c5b0aaaa");
    inputs.worktree_path = Input::known("/tmp/synthetic");
    inputs.source_identity = Input::known("wsid1:aaaa");
    inputs.uncommitted = Uncommitted::Clean;
    inputs
}

impl Fixture {
    /// A Change with one accepted criterion, one confirmed session, one
    /// examined-inputs record of its own, and one captured check run that was
    /// killed: no exit code, a term signal, nothing it could conclude.
    ///
    /// `check_run_inputs` says whether the run got far enough to record what
    /// it was looking at — the difference between the verifier's live case
    /// (it did, so the line reads `ran_nothing`) and the card's other named
    /// case (it did not, so the line reads `other_version`). Both must be
    /// named as stopped, which is the whole point.
    fn new(check_run_inputs: bool) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
        let root = dir
            .path()
            .canonicalize()
            .unwrap_or_else(|_| dir.path().to_path_buf())
            .display()
            .to_string();
        let conn = ledger.conn();
        conn.execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES('p1',?1,'claude_code','Synthetic','2026-01-01T00:00:00Z')",
            rusqlite::params![root],
        )
        .expect("project");
        conn.execute(
            "INSERT INTO workspace_identities(project_id, repo_id, basis, resolved_root,
                 resolved_via, sessions_sampled, observed_at)
             VALUES('p1','repo','git',?1,'recorded_cwd',1,'2026-01-01T00:00:00Z')",
            rusqlite::params![root],
        )
        .expect("identity");
        conn.execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES(?1,'p1',?2,'2026-01-01T00:00:00Z')",
            rusqlite::params![RUN_KEY, format!("/tmp/synthetic/{RUN_KEY}.jsonl")],
        )
        .expect("session");
        conn.execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                 tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES('e1','p1',?1,1,'2026-01-01T00:10:00Z','tool_use','Edit',?2,
                    '/tmp/synthetic/x.jsonl',1,'aa','','bb')",
            rusqlite::params![
                RUN_KEY,
                format!("{{\"file_path\":\"{root}/crates/core/a.rs\"}}")
            ],
        )
        .expect("event");
        conn.execute(
            "INSERT INTO changes(id, project_id, opened_at, title, purpose, requester,
                 request_source)
             VALUES('c1','p1','2026-01-02T09:00:00Z','Keep the suite green',
                    'The suite should pass on what changed.','sam','person')",
            [],
        )
        .expect("change");
        let json = |v: &[&str]| serde_json::to_string(v).expect("json");
        conn.execute(
            "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at,
                 criteria_json, exclusions_json, allowed_scope_json, constraints_json,
                 check_profile_json, content_sha256)
             VALUES('c1',1,'2026-01-02T09:30:00Z',?1,'[]',?2,'[]',?3,'digest')",
            rusqlite::params![
                json(&[CRITERION]),
                json(&["crates/**"]),
                json(&["cargo-test"])
            ],
        )
        .expect("scope");
        conn.execute(
            "INSERT INTO change_scope_acceptances(change_id, scope_version, accepted_by,
                 accepted_at, rationale)
             VALUES('c1',1,'sam','2026-01-02T09:40:00Z','this is what counts')",
            [],
        )
        .expect("acceptance");
        conn.execute(
            "INSERT INTO change_session_associations(id, change_id, session_id, provenance,
                 basis, actor, asserted_at, reason)
             VALUES('a1','c1',?1,'confirmed','user','sam','2026-01-02T10:00:00Z','mine')",
            rusqlite::params![RUN_KEY],
        )
        .expect("confirm");

        // What this Change says will be examined (`B7.12`), so readiness has a
        // candidate identity and a version to be about.
        let intended = inputs_for(SubjectKind::Intended, "c1");
        examined_inputs::record(&ledger, &intended, "2026-01-02T09:45:00Z").expect("intended");

        // The captured run: killed, so no exit code and a signal. This is the
        // binding a stopped `thruline check` leaves behind.
        conn.execute(
            "INSERT INTO evidence_bindings(id, seq, recorded_at, schema_version, scope_class,
                 scope_key, root, repository_identity, pre_identity, post_identity,
                 exit_code, term_signal, bound_at, source_session, doc, doc_sha256)
             VALUES('eb_test',0,'2026-01-03T09:00:00Z',1,'test',?1,'/tmp/synthetic','repo',
                    'pre','post',NULL,9,'2026-01-03T09:00:00Z',?2,?3,'ddd')",
            rusqlite::params![
                "bash:/tmp/synthetic:cargo test --workspace",
                RUN_KEY,
                "{\"stdout\":{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":10},\
                  \"stderr\":{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":10}}"
            ],
        )
        .expect("binding");

        if check_run_inputs {
            let examined = inputs_for(SubjectKind::CheckRun, "eb_test");
            examined_inputs::record(&ledger, &examined, "2026-01-03T09:00:00Z").expect("examined");
        }

        // The supervisor row the run opened for itself, through the writer.
        run_execution::start(
            &ledger,
            &Start {
                run_key: RUN_KEY.into(),
                change_id: Some("c1".into()),
                name: "cargo test --workspace".into(),
                pid: Some(4242),
                process_group: Some(4242),
                started_at: "2026-01-03T08:59:00Z".into(),
                examined_inputs_id: None,
            },
        )
        .expect("start");

        derive_links(&ledger, "c1", AT).expect("derive");
        Fixture { _dir: dir, ledger }
    }
}

fn env() -> ProfileEnvironment {
    let mut env = ProfileEnvironment::empty();
    env.tools_present.insert("cargo".to_string());
    env
}

/// The derived answer, and the JSON the route serves from it.
fn answer(
    f: &Fixture,
    expect_state: CoverageState,
) -> (
    thruline_core::change_readiness::Readiness,
    serde_json::Value,
) {
    let view = coverage(&f.ledger, "c1", &env()).expect("coverage");
    // The coverage state a stopped run really produces. Asserted, so that if
    // it ever becomes `Evidence` this test keeps meaning what it says rather
    // than passing for a new reason: the whole defect was that a stopped run
    // never reaches the `Evidence` arm at all.
    assert_eq!(
        view.criteria[0].state, expect_state,
        "a run that settled nothing does not bear, so the line never reads `evidence`"
    );
    let derived = readiness(&f.ledger, "c1", &view).expect("readiness");
    let json = serde_json::to_value(&derived).expect("serialise");
    (derived, json)
}

fn assert_stopped(derived: &thruline_core::change_readiness::Readiness, json: &serde_json::Value) {
    assert!(!derived.ready, "a stopped check never makes anything ready");
    assert!(derived.rests_on.is_empty());
    let obligation = derived
        .obligations
        .iter()
        .find(|o| o.id == "criterion:1")
        .expect("the criterion is listed");
    assert_eq!(
        obligation.unsatisfied,
        Some(Unsatisfied::Stopped),
        "the screen must say the check was stopped, not that something settled nothing"
    );
    assert_eq!(
        obligation.unsatisfied_slug,
        Some("readiness_stopped"),
        "and it must say it by the slug the copy file answers"
    );
    // The same thing again in the JSON a screen actually reads.
    let served = json["obligations"]
        .as_array()
        .expect("obligations")
        .iter()
        .find(|o| o["id"] == "criterion:1")
        .expect("the criterion is served");
    assert_eq!(served["unsatisfied_slug"], "readiness_stopped");
    assert_eq!(served["unsatisfied"], "stopped");
    assert_eq!(json["ready"], false);
    assert_ne!(
        served["unsatisfied_slug"], "readiness_settled_nothing",
        "the sentence the verifier found on the live screen"
    );
}

/// **The live failure, reproduced through the whole derivation.** A person
/// types `thruline change cancel-run`: `cancelled` is appended with no binding
/// to name, because the wrapper has not stored one yet.
#[test]
fn a_check_a_person_stopped_is_named_as_stopped() {
    let f = Fixture::new(true);
    run_execution::finish(
        &f.ledger,
        &Finish {
            run_key: RUN_KEY.into(),
            state: ExecutionState::Cancelled,
            reason: Some(Reason::CancelledByPerson),
            actor: Some("sam".into()),
            // Exactly what `cancel-run` writes: it has no binding to name.
            binding_id: None,
            cleanup_verified: Some(true),
            survivors: Some(0),
            at: "2026-01-03T09:00:30Z".into(),
        },
    )
    .expect("cancel");

    let (derived, json) = answer(&f, CoverageState::RanNothing);
    assert_stopped(&derived, &json);
}

/// The other half the card names: **interrupted, with no examined-inputs
/// record of its own.** The wrapper reaped a signalled child, named the
/// binding, and the run never got far enough to record what it examined — so
/// `applies` is `None` and the older routing sent it to "settled nothing".
#[test]
fn an_interrupted_run_with_no_inputs_record_is_named_as_stopped() {
    let f = Fixture::new(false);
    run_execution::finish(
        &f.ledger,
        &Finish {
            run_key: RUN_KEY.into(),
            state: ExecutionState::Interrupted,
            reason: Some(Reason::ProcessSignalled),
            actor: None,
            binding_id: Some("eb_test".into()),
            cleanup_verified: None,
            survivors: None,
            at: "2026-01-03T09:00:30Z".into(),
        },
    )
    .expect("interrupt");

    let view = coverage(&f.ledger, "c1", &env()).expect("coverage");
    let item = &view.criteria[0].items[0];
    assert_eq!(item.execution, ExecutionState::Interrupted);
    assert_eq!(
        item.applies,
        Some(false),
        "the run recorded nothing about which version it examined"
    );
    assert_eq!(item.applies_slug, Some("inputs_missing"));

    let (derived, json) = answer(&f, CoverageState::OtherVersion);
    assert_stopped(&derived, &json);
}

/// The negative control, on the same fixture path: a run that **finished** is
/// not reported as stopped. Without this, returning `Stopped` unconditionally
/// would pass both tests above.
#[test]
fn a_run_that_finished_is_not_reported_as_stopped() {
    let f = Fixture::new(true);
    run_execution::finish(
        &f.ledger,
        &Finish {
            run_key: RUN_KEY.into(),
            state: ExecutionState::Completed,
            reason: None,
            actor: None,
            binding_id: Some("eb_test".into()),
            cleanup_verified: None,
            survivors: None,
            at: "2026-01-03T09:00:30Z".into(),
        },
    )
    .expect("complete");

    let view = coverage(&f.ledger, "c1", &env()).expect("coverage");
    let derived = readiness(&f.ledger, "c1", &view).expect("readiness");
    let obligation = derived
        .obligations
        .iter()
        .find(|o| o.id == "criterion:1")
        .expect("the criterion is listed");
    assert!(!derived.ready, "a killed command still proves nothing");
    assert_ne!(
        obligation.unsatisfied,
        Some(Unsatisfied::Stopped),
        "nothing about this run's EXECUTION was left open"
    );
    assert_eq!(obligation.unsatisfied, Some(Unsatisfied::SettledNothing));
}
