//! NEVER-ASSESSED-NOT-READY-R2 — "Run `thruline change prepare` and it will
//! be." is true in every shape, or the answer says the line could not be
//! assessed and why; never a silent skip.
//!
//! The round-1 adversary's reproduction, re-created here on the real engine
//! (`examined_inputs::{record, standing}`, `snapshot::{capture, store}`,
//! `evidence_applicability::on_intended_bound`, `coverage`, `readiness`) with
//! real, loadable snapshot rows. The shape: the Change's standing `intended`
//! binding has no stored snapshot (D-051, SNAPSHOT-CHILD-TIMEOUT, in
//! `examined_inputs::observe_source_with_snapshot`), and the next bind — the
//! one `change prepare` makes, whose OWN capture succeeds — is what the
//! served sentence promises will assess the line.
//!
//! At 3476eb1f that next bind wrote nothing for the whole Change, and the
//! line went on reading "Run `thruline change prepare` and it will be."
//!
//! Every string here is synthetic. Nothing came off a real machine.

use thruline_core::change_evidence::{coverage, derive_links, CoverageState, Observed};
use thruline_core::change_propose::ProfileEnvironment;
use thruline_core::change_readiness::readiness;
use thruline_core::evidence_applicability::{self, ApplicabilityState};
use thruline_core::examined_inputs::{self, ExaminedInputs, Input, SubjectKind, Uncommitted};
use thruline_core::ledger::Ledger;

const AT: &str = "2026-02-01T12:00:00Z";
const LINE_1: &str = "The service builds cleanly after a restart.";
const NEVER_ASSESSED: &str = "readiness_never_assessed";
const COULD_NOT_ASSESS: &str = "readiness_could_not_assess";

fn inputs_for(kind: SubjectKind, subject: &str) -> ExaminedInputs {
    let mut inputs = ExaminedInputs::unknown_for(kind, subject);
    inputs.base_revision = Input::known("e71c5b0aaaa");
    inputs.worktree_path = Input::known("/tmp/synthetic");
    inputs.source_identity = Input::known("wsid1:aaaa");
    inputs.uncommitted = Uncommitted::Clean;
    inputs
}

struct Fixture {
    dir: tempfile::TempDir,
    ledger: Ledger,
}

impl Fixture {
    /// One accepted line; `intended` bound once with a real, stored snapshot;
    /// one Proven, completed build run on this version linked to the line.
    /// `run_has_own_snapshot` says whether the run's own examined-inputs
    /// record carries a stored snapshot of the tree it ran against.
    fn new(run_has_own_snapshot: bool) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("work")).expect("work dir");
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
             VALUES('s_build','p1','/tmp/synthetic/s_build.jsonl','2026-01-01T00:00:00Z')",
            [],
        )
        .expect("session");
        conn.execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                 tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES('e1','p1','s_build',1,'2026-01-01T00:10:00Z','tool_use','Edit',?1,
                    '/tmp/synthetic/x.jsonl',1,'aa','','bb')",
            rusqlite::params![format!("{{\"file_path\":\"{root}/crates/core/a.rs\"}}")],
        )
        .expect("event");
        conn.execute(
            "INSERT INTO changes(id, project_id, opened_at, title, purpose, requester,
                 request_source)
             VALUES('c1','p1','2026-01-02T09:00:00Z','Keep the build clean',
                    'The build should stay clean.','sam','person')",
            [],
        )
        .expect("change");
        let json = |v: &[&str]| serde_json::to_string(v).expect("json");
        conn.execute(
            "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at,
                 criteria_json, exclusions_json, allowed_scope_json, constraints_json,
                 check_profile_json, content_sha256)
             VALUES('c1',1,'2026-01-02T09:30:00Z',?1,'[]',?2,'[]',?3,'digest')",
            rusqlite::params![json(&[LINE_1]), json(&["crates/**"]), json(&["clippy"])],
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
             VALUES('a1','c1','s_build','confirmed','user','sam','2026-01-02T10:00:00Z','mine')",
            [],
        )
        .expect("confirm");

        let f = Fixture { dir, ledger };
        // The first bind, healthy: a real stored snapshot.
        let mut intended = inputs_for(SubjectKind::Intended, "c1");
        intended.snapshot_id = Input::known(f.snap("2026-01-02T09:44:00Z"));
        examined_inputs::record(&f.ledger, &intended, "2026-01-02T09:45:00Z").expect("intended");

        f.ledger
            .conn()
            .execute(
                "INSERT INTO evidence_bindings(id, seq, recorded_at, schema_version, scope_class,
                     scope_key, root, repository_identity, pre_identity, post_identity,
                     exit_code, term_signal, bound_at, source_session, doc, doc_sha256)
                 VALUES('eb_build',0,'2026-01-03T09:00:00Z',1,'build',?1,'/tmp/synthetic',
                        'repo','pre','post',0,NULL,'2026-01-03T09:00:00Z','s_build',?2,'ddd')",
                rusqlite::params![
                    "bash:/tmp/synthetic:cargo clippy --workspace",
                    "{\"stdout\":{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":10},\
                      \"stderr\":{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":10}}"
                ],
            )
            .expect("binding");
        let mut examined = inputs_for(SubjectKind::CheckRun, "eb_build");
        if run_has_own_snapshot {
            examined.snapshot_id = Input::known(f.snap("2026-01-03T08:59:00Z"));
        }
        examined_inputs::record(&f.ledger, &examined, "2026-01-03T09:00:00Z").expect("examined");

        derive_links(&f.ledger, "c1", AT).expect("derive");
        f
    }

    /// A real capture of the (unchanging) work tree, stored. Snapshot ids
    /// are fresh on every capture, never content-derived.
    fn snap(&self, at: &str) -> String {
        let snap =
            thruline_core::snapshot::capture(&self.dir.path().join("work"), at).expect("capture");
        thruline_core::snapshot::store(&self.ledger, &snap).expect("store");
        snap.id
    }

    /// One binding whose capture did not finish (D-051): no snapshot.
    fn bind_without_snapshot(&self, at: &str) {
        let standing = examined_inputs::standing(&self.ledger, SubjectKind::Intended, "c1")
            .expect("standing")
            .expect("an intended record");
        let mut poisoned = standing.clone();
        poisoned.snapshot_id = Input::Unknown;
        let previous = Some(standing);
        examined_inputs::record(&self.ledger, &poisoned, at).expect("poisoned bind");
        evidence_applicability::on_intended_bound(
            &self.ledger,
            "c1",
            previous.as_ref(),
            &poisoned,
            at,
        )
        .expect("sweep");
    }

    /// Exactly what `change prepare` does, with its own capture succeeding:
    /// read the standing record, write the new one, sweep.
    fn prepare(&self, at: &str) -> Vec<evidence_applicability::ApplicabilityRecord> {
        let previous =
            examined_inputs::standing(&self.ledger, SubjectKind::Intended, "c1").expect("standing");
        let mut current = inputs_for(SubjectKind::Intended, "c1");
        current.snapshot_id = Input::known(self.snap(at));
        examined_inputs::record(&self.ledger, &current, at).expect("bind");
        evidence_applicability::on_intended_bound(
            &self.ledger,
            "c1",
            previous.as_ref(),
            &current,
            at,
        )
        .expect("sweep")
    }

    fn line_1(&self) -> (bool, Option<&'static str>) {
        let mut env = ProfileEnvironment::empty();
        env.tools_present.insert("cargo".to_string());
        let view = coverage(&self.ledger, "c1", &env).expect("coverage");
        assert_eq!(view.criteria.len(), 1);
        assert_eq!(view.criteria[0].state, CoverageState::Evidence);
        assert!(view.criteria[0]
            .items
            .iter()
            .any(|i| i.bears && i.observed == Observed::Proven && i.applies == Some(true)));
        let r = readiness(&self.ledger, "c1", &view).expect("readiness");
        let slug = r
            .obligations
            .iter()
            .find(|o| o.id == "criterion:1")
            .and_then(|o| o.unsatisfied_slug);
        (r.ready, slug)
    }

    fn applies_records(&self) -> i64 {
        self.ledger
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM change_criterion_freshness
                  WHERE change_id='c1' AND state='applies'",
                [],
                |row| row.get(0),
            )
            .expect("count")
    }
}

/// The premise: before anything goes wrong, the line reads never assessed,
/// and the sentence names `change prepare` — which is then true: one prepare
/// assesses it.
#[test]
fn premise_and_control_a_healthy_previous_binding_is_assessed_by_one_prepare() {
    for own in [false, true] {
        let f = Fixture::new(own);
        assert_eq!(f.line_1(), (false, Some(NEVER_ASSESSED)), "own={own}");
        let records = f.prepare("2026-01-03T10:00:00Z");
        assert_eq!(records.len(), 1, "own={own}: {records:?}");
        assert_eq!(records[0].state, ApplicabilityState::Applies);
        assert_eq!(f.line_1(), (true, None), "own={own}");
    }
}

/// The adversary's break, with the evidence carrying its own recorded tree:
/// the previous binding has no snapshot, and one prepare must still assess
/// the line against the tree the NEW binding captured, from the evidence's
/// own recorded inputs.
#[test]
fn after_a_binding_with_no_snapshot_one_prepare_assesses_from_the_evidences_own_tree() {
    let f = Fixture::new(true);
    f.bind_without_snapshot("2026-01-03T09:30:00Z");
    assert_eq!(f.line_1(), (false, Some(NEVER_ASSESSED)));

    let records = f.prepare("2026-01-03T10:00:00Z");
    assert_eq!(
        records.len(),
        1,
        "one prepare assesses the linked row: {records:?}"
    );
    assert_eq!(records[0].state, ApplicabilityState::Applies);
    let own = examined_inputs::standing(&f.ledger, SubjectKind::CheckRun, "eb_build")
        .expect("standing")
        .expect("own record");
    assert_eq!(
        Some(records[0].old_snapshot.as_str()),
        own.snapshot_id.value(),
        "compared from the evidence's own tree"
    );
    assert_eq!(f.line_1(), (true, None));
}

/// The adversary's exact shape: the evidence carries no tree of its own, and
/// the binding before this prepare has none either, so nothing on record says
/// what the files were when it was checked. One prepare cannot assess it; the
/// line must SAY so — not repeat "Run `thruline change prepare` and it will
/// be." — and must not read Ready.
#[test]
fn after_a_binding_with_no_snapshot_a_row_with_no_tree_of_its_own_says_it_could_not_be_assessed() {
    let f = Fixture::new(false);
    f.bind_without_snapshot("2026-01-03T09:30:00Z");
    // Before the prepare, prepare would not settle it either: said already.
    assert_eq!(f.line_1(), (false, Some(COULD_NOT_ASSESS)));

    let records = f.prepare("2026-01-03T10:00:00Z");
    assert!(records.is_empty(), "nothing to compare from: {records:?}");
    assert_eq!(f.line_1(), (false, Some(COULD_NOT_ASSESS)));

    let line = thruline_core::voice::readiness_line(COULD_NOT_ASSESS).expect("a served sentence");
    assert!(
        !line.plain.contains("change prepare"),
        "the sentence must not promise what prepare cannot do: {}",
        line.plain
    );
    assert!(
        line.plain.contains("could not be assessed"),
        "{}",
        line.plain
    );
}

/// Claims less, never more: a SECOND prepare after the gap must not compare
/// two trees captured after the gap and call the line "still applies" — what
/// the files were when it was checked is still not on record.
#[test]
fn a_second_prepare_after_the_gap_does_not_launder_the_line_to_still_applies() {
    let f = Fixture::new(false);
    f.bind_without_snapshot("2026-01-03T09:30:00Z");
    f.prepare("2026-01-03T10:00:00Z");
    let records = f.prepare("2026-01-03T10:30:00Z");
    assert!(records.is_empty(), "{records:?}");
    assert_eq!(f.applies_records(), 0);
    assert_eq!(f.line_1(), (false, Some(COULD_NOT_ASSESS)));
}

/// The one thing that does settle it is named, and is true: a new run of the
/// check records its own tree, and the next bind assesses from it.
#[test]
fn a_new_run_of_the_check_settles_a_line_that_could_not_be_assessed() {
    let f = Fixture::new(false);
    f.bind_without_snapshot("2026-01-03T09:30:00Z");
    f.prepare("2026-01-03T10:00:00Z");
    assert_eq!(f.line_1(), (false, Some(COULD_NOT_ASSESS)));

    f.ledger
        .conn()
        .execute(
            "INSERT INTO evidence_bindings(id, seq, recorded_at, schema_version, scope_class,
                 scope_key, root, repository_identity, pre_identity, post_identity,
                 exit_code, term_signal, bound_at, source_session, doc, doc_sha256)
             VALUES('eb_build2',1,'2026-01-03T11:00:00Z',1,'build',?1,'/tmp/synthetic',
                    'repo','pre','post',0,NULL,'2026-01-03T11:00:00Z','s_build',?2,'ddd')",
            rusqlite::params![
                "bash:/tmp/synthetic:cargo clippy --workspace",
                "{\"stdout\":{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":10},\
                  \"stderr\":{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":10}}"
            ],
        )
        .expect("second run");
    let mut examined = inputs_for(SubjectKind::CheckRun, "eb_build2");
    examined.snapshot_id = Input::known(f.snap("2026-01-03T10:59:00Z"));
    examined_inputs::record(&f.ledger, &examined, "2026-01-03T11:00:00Z").expect("examined");
    derive_links(&f.ledger, "c1", "2026-01-03T11:00:01Z").expect("derive");

    f.prepare("2026-01-03T11:30:00Z");
    assert_eq!(f.line_1(), (true, None));
}
