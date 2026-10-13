//! RUN-LINK-TRUTH (path steps 6 and 8: PW-F9, PW-F14) — a passing check run
//! from the Change that no accepted line is linked to is NOT "No test ran for
//! this." The line's reason is `readiness_unlinked_run`, the Change is not
//! Ready, and only a named person's link (write address #4) makes the line
//! rest on the run — after which, under R-2026-09-28-25, one more assessment
//! ("Check it again") settles it.
//!
//! Built through the real derivation — a real ledger, a real captured test run
//! with its own pass count, `derive_links`, `coverage()`, `readiness()`,
//! `change_evidence::act` — so the answer asserted is the one every surface
//! reads.
//!
//! The fixture is the path walk's: one accepted line that no automatic rule
//! can link (Unclassified, and it shares no word with the run's name), and one
//! completed test run for the Change: 2 passed, 0 failed.
//!
//! Every string here is synthetic. Nothing came off a real machine.

use std::collections::BTreeMap;

use thruline_core::change_evidence::{
    act, coverage, derive_links, CoverageState, EvidenceKind, LinkAct, LinkBasis, Observed,
};
use thruline_core::change_propose::ProfileEnvironment;
use thruline_core::change_readiness::{readiness, Readiness};
use thruline_core::evidence_applicability::{self, ApplicabilityState};
use thruline_core::examined_inputs::{self, ExaminedInputs, Input, SubjectKind, Uncommitted};
use thruline_core::ledger::Ledger;
use thruline_core::run_execution::{self, ExecutionState, Finish, Start};
use thruline_core::test_tally::{self, TestTally};

const AT: &str = "2026-02-01T12:00:00Z";
const LINE: &str = "When the cart is empty, the total shows zero.";
const RUN_KEY: &str = "s_walk";
const RUN: &str = "eb_test";
const LINKER: &str = "walker";

struct Fixture {
    _dir: tempfile::TempDir,
    ledger: Ledger,
}

fn inputs_for(kind: SubjectKind, subject: &str) -> ExaminedInputs {
    let mut inputs = ExaminedInputs::unknown_for(kind, subject);
    inputs.base_revision = Input::known("e71c5b0aaaa");
    inputs.worktree_path = Input::known("/tmp/synthetic");
    inputs.source_identity = Input::known("wsid1:aaaa");
    inputs.uncommitted = Uncommitted::Clean;
    inputs
}

impl Fixture {
    fn new() -> Self {
        Self::with_count(true)
    }

    fn with_count(counted: bool) -> Self {
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
            rusqlite::params![RUN_KEY, format!("{{\"file_path\":\"{root}/src/cart.rs\"}}")],
        )
        .expect("event");
        conn.execute(
            "INSERT INTO changes(id, project_id, opened_at, title, purpose, requester,
                 request_source)
             VALUES('c1','p1','2026-01-02T09:00:00Z','Empty cart total',
                    'An empty cart totals zero.','sam','person')",
            [],
        )
        .expect("change");
        let json = |v: &[&str]| serde_json::to_string(v).expect("json");
        conn.execute(
            "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at,
                 criteria_json, exclusions_json, allowed_scope_json, constraints_json,
                 check_profile_json, content_sha256)
             VALUES('c1',1,'2026-01-02T09:30:00Z',?1,'[]',?2,'[]','[]','digest')",
            rusqlite::params![json(&[LINE]), json(&["src/**"])],
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

        let intended = inputs_for(SubjectKind::Intended, "c1");
        examined_inputs::record(&ledger, &intended, "2026-01-02T09:45:00Z").expect("intended");

        // One completed test run for this Change, on this version: exit 0,
        // both streams captured whole, and its own count — 2 passed, 0 failed.
        conn.execute(
            "INSERT INTO evidence_bindings(id, seq, recorded_at, schema_version, scope_class,
                 scope_key, root, repository_identity, pre_identity, post_identity,
                 exit_code, term_signal, bound_at, source_session, doc, doc_sha256)
             VALUES(?1,0,'2026-01-03T09:00:00Z',1,'test',?2,'/tmp/synthetic','repo',
                    'pre','post',0,NULL,'2026-01-03T09:00:00Z',?3,?4,'ddd')",
            rusqlite::params![
                RUN,
                "bash:/tmp/synthetic:cargo test",
                RUN_KEY,
                "{\"stdout\":{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":10},\
                  \"stderr\":{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":10}}"
            ],
        )
        .expect("binding");
        if counted {
            test_tally::record(
                &ledger,
                RUN,
                &TestTally {
                    passed: 2,
                    failed: 0,
                    ignored: 0,
                    suites: 1,
                },
                "2026-01-03T09:00:00Z",
            )
            .expect("tally");
        }
        // It ran to the end, as a --for-change run does: the supervisor's
        // `running` row, then `completed` naming the binding it produced.
        run_execution::start(
            &ledger,
            &Start {
                run_key: RUN_KEY.to_string(),
                change_id: Some("c1".to_string()),
                name: "cargo test".to_string(),
                pid: None,
                process_group: None,
                started_at: "2026-01-03T08:59:00Z".to_string(),
                examined_inputs_id: None,
            },
        )
        .expect("start");
        run_execution::finish(
            &ledger,
            &Finish {
                run_key: RUN_KEY.to_string(),
                state: ExecutionState::Completed,
                reason: None,
                actor: None,
                binding_id: Some(RUN.to_string()),
                cleanup_verified: Some(true),
                survivors: Some(0),
                at: "2026-01-03T09:00:00Z".to_string(),
            },
        )
        .expect("finish");
        // C10-SEAM-PAIR (as NEVER-ASSESSED-NOT-READY-R2 did for its own
        // fixture): the run carries its own recorded tree — a real capture,
        // stored — so "Check it again" can in fact assess it. Without one,
        // and with the standing `intended` record above carrying no snapshot
        // either, nothing on record says what the files were when it ran,
        // and a linked line reads `readiness_could_not_assess` instead.
        std::fs::create_dir_all(dir.path().join("work")).expect("work dir");
        let own =
            thruline_core::snapshot::capture(&dir.path().join("work"), "2026-01-03T08:59:00Z")
                .expect("capture the run's own tree");
        thruline_core::snapshot::store(&ledger, &own).expect("store the run's own tree");
        let mut examined = inputs_for(SubjectKind::CheckRun, RUN);
        examined.snapshot_id = Input::known(own.id);
        examined_inputs::record(&ledger, &examined, "2026-01-03T09:00:00Z").expect("examined");

        derive_links(&ledger, "c1", AT).expect("derive");
        Fixture { _dir: dir, ledger }
    }

    fn view(&self) -> thruline_core::change_evidence::CoverageView {
        coverage(&self.ledger, "c1", &ProfileEnvironment::empty()).expect("coverage")
    }

    fn answer(&self) -> (Readiness, serde_json::Value) {
        let view = self.view();
        let derived = readiness(&self.ledger, "c1", &view).expect("readiness");
        let json = serde_json::to_value(&derived).expect("serialise");
        (derived, json)
    }

    /// Write address #4's act: a named person links the run to line 1.
    fn link_by(&self, actor: &str) -> Result<String, thruline_core::error::CoreError> {
        act(
            &self.ledger,
            "c1",
            &LinkAct {
                criterion_index: 1,
                kind: EvidenceKind::CheckRun,
                evidence_id: RUN.to_string(),
                basis: LinkBasis::Manual,
                actor: actor.to_string(),
                reason: "this test covers the empty cart".to_string(),
                at: "2026-01-03T09:30:00Z".to_string(),
            },
        )
    }

    /// "Check it again": the Change's version is assessed against the tree the
    /// run was produced on, and nothing moved. The function `change prepare`
    /// and a --for-change run reach through `on_intended_bound`.
    fn check_again(&self) {
        let tree: BTreeMap<String, String> = BTreeMap::new();
        let records = evidence_applicability::assess_tree_move(
            &self.ledger,
            "c1",
            "snap_old",
            "snap_new",
            &tree,
            &tree,
            None,
            "2026-01-03T11:00:00Z",
        )
        .expect("assess");
        assert!(records
            .iter()
            .all(|r| r.state == ApplicabilityState::Applies));
    }
}

fn line_1(json: &serde_json::Value) -> &serde_json::Value {
    json["obligations"]
        .as_array()
        .expect("obligations")
        .iter()
        .find(|o| o["id"] == "criterion:1")
        .expect("line 1 is served")
}

#[test]
fn a_passing_run_no_line_links_is_named_as_unlinked_and_the_change_is_not_ready() {
    let f = Fixture::new();
    let view = f.view();

    // The premise, asserted so this test cannot pass for another reason: the
    // line has nothing linked, and the Change holds a completed, bearing,
    // Proven check run that no line links.
    assert_eq!(view.criteria.len(), 1);
    assert_eq!(view.criteria[0].state, CoverageState::None);
    assert!(view.criteria[0].items.is_empty(), "no automatic link");
    assert_eq!(view.link_rows, 0, "no link row of any basis");
    assert!(
        view.candidates.iter().any(|c| c.evidence_id == RUN
            && c.kind == EvidenceKind::CheckRun
            && c.execution == ExecutionState::Completed
            && c.bears
            && c.observed == Observed::Proven),
        "the passing run is on record: {:?}",
        view.candidates
    );

    let (derived, json) = f.answer();
    assert!(!derived.ready, "an unlinked run makes nothing Ready");
    assert_eq!(json["ready"], false);
    assert_eq!(derived.missing, vec!["criterion:1".to_string()]);
    let line = line_1(&json);
    assert_eq!(line["outcome"], "needs_evidence");
    assert_eq!(line["unsatisfied_slug"], "readiness_unlinked_run");
    assert_eq!(line["unsatisfied"], "unlinked_run");
    assert_ne!(line["unsatisfied_slug"], "readiness_nothing_looked");
}

#[test]
fn a_run_that_proves_nothing_leaves_the_old_reason() {
    // The same Change whose run has no count of passes: it proves nothing
    // (Couldn't check), so it is not a passing run no line links, and the old
    // sentence is still the one the line gets.
    let f = Fixture::with_count(false);
    let (derived, json) = f.answer();
    assert!(!derived.ready);
    assert_eq!(
        line_1(&json)["unsatisfied_slug"],
        "readiness_nothing_looked"
    );
}

#[test]
fn a_named_link_then_one_check_again_makes_the_line_rest_on_the_run_and_names_the_linker() {
    let f = Fixture::new();

    // Nobody's name, no link.
    assert!(f.link_by("  ").is_err(), "a link needs a name");
    let (still, still_json) = f.answer();
    assert!(!still.ready);
    assert_eq!(
        line_1(&still_json)["unsatisfied_slug"],
        "readiness_unlinked_run"
    );

    // Step 1: a named person links the run. Under R-2026-09-28-25 a link made
    // after the last assessment is never assessed, so it is not Ready yet —
    // and the reason is no longer the unlinked one.
    f.link_by(LINKER).expect("a named person's link");
    let (linked, linked_json) = f.answer();
    assert!(!linked.ready, "a link alone is not an assessment");
    assert_eq!(
        line_1(&linked_json)["unsatisfied_slug"],
        "readiness_never_assessed"
    );

    // Step 2: check it again. The line rests on that run, and on the person
    // who linked it, by name.
    f.check_again();
    let (after, json) = f.answer();
    assert!(after.ready, "linked by a named person and assessed");
    assert_eq!(after.rests_on.len(), 1);
    assert_eq!(after.rests_on[0].text, LINE);
    assert_eq!(
        after.rests_on[0].evidence_name.as_deref(),
        Some("bash:/tmp/synthetic:cargo test")
    );
    assert_eq!(line_1(&json)["linked_by"], LINKER);
    assert_eq!(json["rests_on"][0]["linked_by"], LINKER);
}

#[test]
fn the_unlinked_reason_has_a_sentence_in_both_registers_and_it_is_the_screens() {
    let line = thruline_core::voice::readiness_line("readiness_unlinked_run")
        .expect("the unlinked-run reason has a sentence");
    assert_eq!(
        line.plain,
        "A test ran for this Change but isn't linked to this line — link it."
    );
    assert!(!line.technical.trim().is_empty());
}
