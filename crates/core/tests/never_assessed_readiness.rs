//! NEVER-ASSESSED-NOT-READY (R-2026-09-28-25) — a criterion whose
//! applicability was never assessed does not count toward Ready, the same way
//! Couldn't check does not (R-2026-09-27-14), and the answer says what is
//! missing and the one act that would assess it.
//!
//! Built through the real derivation — a real ledger, a real captured build
//! run bound to the Change's own version, `derive_links`, `coverage()`,
//! `readiness()` — so the answer asserted is the one every surface reads
//! (the coverage route, the readiness route, the screen, the packet and the
//! exported file all serve this `Readiness`, and none of them re-derives it).
//!
//! The fixture: two accepted lines, one build run on this version that
//! backs both, and an applicability assessment ("still applies") for line 2
//! only. At base this was Ready and rested on line 1; it must not be.
//!
//! Every string here is synthetic. Nothing came off a real machine.

use std::collections::BTreeMap;

use thruline_core::change_evidence::{coverage, derive_links, CoverageState, Observed};
use thruline_core::change_propose::ProfileEnvironment;
use thruline_core::change_readiness::{readiness, Readiness};
use thruline_core::evidence_applicability::{self, ApplicabilityState};
use thruline_core::examined_inputs::{self, ExaminedInputs, Input, SubjectKind, Uncommitted};
use thruline_core::ledger::Ledger;

const AT: &str = "2026-02-01T12:00:00Z";
const LINE_1: &str = "The service builds cleanly after a restart.";
const LINE_2: &str = "The release build compiles.";
const RUN_KEY: &str = "s_build";

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
            rusqlite::params![
                json(&[LINE_1, LINE_2]),
                json(&["crates/**"]),
                json(&["clippy"])
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

        let intended = inputs_for(SubjectKind::Intended, "c1");
        examined_inputs::record(&ledger, &intended, "2026-01-02T09:45:00Z").expect("intended");

        // One build run on this version: exit 0, both streams captured whole.
        conn.execute(
            "INSERT INTO evidence_bindings(id, seq, recorded_at, schema_version, scope_class,
                 scope_key, root, repository_identity, pre_identity, post_identity,
                 exit_code, term_signal, bound_at, source_session, doc, doc_sha256)
             VALUES('eb_build',0,'2026-01-03T09:00:00Z',1,'build',?1,'/tmp/synthetic','repo',
                    'pre','post',0,NULL,'2026-01-03T09:00:00Z',?2,?3,'ddd')",
            rusqlite::params![
                "bash:/tmp/synthetic:cargo clippy --workspace",
                RUN_KEY,
                "{\"stdout\":{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":10},\
                  \"stderr\":{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":10}}"
            ],
        )
        .expect("binding");
        // NEVER-ASSESSED-NOT-READY-R2: the run carries its own recorded tree
        // (a real capture, stored), so the act the served sentence names —
        // `change prepare` — can in fact assess it. Without one, and with the
        // standing binding above carrying no snapshot either, nothing on
        // record says what the files were when it ran, and the line reads
        // `readiness_could_not_assess` instead (never_assessed_no_baseline.rs).
        std::fs::create_dir_all(dir.path().join("work")).expect("work dir");
        let own =
            thruline_core::snapshot::capture(&dir.path().join("work"), "2026-01-03T08:59:00Z")
                .expect("capture the run's own tree");
        thruline_core::snapshot::store(&ledger, &own).expect("store the run's own tree");
        let mut examined = inputs_for(SubjectKind::CheckRun, "eb_build");
        examined.snapshot_id = Input::known(own.id);
        examined_inputs::record(&ledger, &examined, "2026-01-03T09:00:00Z").expect("examined");

        derive_links(&ledger, "c1", AT).expect("derive");
        Fixture { _dir: dir, ledger }
    }

    /// The standing link ids for one accepted line.
    fn links_of(&self, criterion_index: u32) -> Vec<String> {
        let mut statement = self
            .ledger
            .conn()
            .prepare(
                "SELECT id FROM change_criterion_evidence
                  WHERE change_id='c1' AND criterion_index=?1 ORDER BY id",
            )
            .expect("prepare");
        let ids = statement
            .query_map(rusqlite::params![criterion_index], |row| {
                row.get::<_, String>(0)
            })
            .expect("query")
            .collect::<Result<Vec<_>, _>>()
            .expect("rows");
        ids
    }

    /// "Still applies", recorded for every link of one line, by the module's
    /// own writer.
    fn assess_still_applies(&self, criterion_index: u32) {
        for link in self.links_of(criterion_index) {
            evidence_applicability::record(
                &self.ledger,
                "c1",
                &link,
                criterion_index,
                1,
                "snap_old",
                "snap_new",
                ApplicabilityState::Applies,
                None,
                &[],
                None,
                None,
                "2026-01-03T10:00:00Z",
            )
            .expect("assessment");
        }
    }

    fn answer(&self) -> (Readiness, serde_json::Value) {
        let mut env = ProfileEnvironment::empty();
        env.tools_present.insert("cargo".to_string());
        let view = coverage(&self.ledger, "c1", &env).expect("coverage");
        // The premise, asserted so this test cannot pass for another reason:
        // both lines are backed by a Proven run on THIS version, and nothing
        // about the run is stale or stopped.
        assert_eq!(view.criteria.len(), 2);
        for row in &view.criteria {
            assert_eq!(row.state, CoverageState::Evidence, "line {}", row.index);
            assert!(
                row.items.iter().any(|i| i.bears
                    && i.observed == Observed::Proven
                    && i.applies == Some(true)
                    && i.fresh != Some(false)),
                "line {} is backed by a Proven run on this version: {:?}",
                row.index,
                row.items
            );
        }
        let derived = readiness(&self.ledger, "c1", &view).expect("readiness");
        let json = serde_json::to_value(&derived).expect("serialise");
        (derived, json)
    }
}

fn obligation<'a>(json: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    json["obligations"]
        .as_array()
        .expect("obligations")
        .iter()
        .find(|o| o["id"] == id)
        .unwrap_or_else(|| panic!("{id} is served"))
}

#[test]
fn a_line_whose_applicability_was_never_assessed_does_not_count_toward_ready() {
    let f = Fixture::new();
    f.assess_still_applies(2);
    let (derived, json) = f.answer();

    // The premise of the ruling: line 1's evidence was never assessed, line
    // 2's was and still applies.
    let view_fresh = |n: usize| -> Vec<Option<bool>> {
        let mut env = ProfileEnvironment::empty();
        env.tools_present.insert("cargo".to_string());
        coverage(&f.ledger, "c1", &env).expect("coverage").criteria[n]
            .items
            .iter()
            .map(|i| i.fresh)
            .collect()
    };
    assert!(
        view_fresh(0).iter().all(Option::is_none),
        "line 1 never assessed"
    );
    assert!(view_fresh(1).contains(&Some(true)), "line 2 still applies");

    assert!(
        !derived.ready,
        "Ready must not rest on a line whose applicability was never assessed"
    );
    assert_eq!(json["ready"], false);
    assert!(
        derived.rests_on.is_empty(),
        "a not-ready answer rests on nothing"
    );
    assert_eq!(derived.missing, vec!["criterion:1".to_string()]);

    // It says plainly what is missing: this line, for this reason.
    let line_1 = obligation(&json, "criterion:1");
    assert_eq!(line_1["outcome"], "needs_evidence");
    assert_eq!(line_1["unsatisfied_slug"], "readiness_never_assessed");
    assert_eq!(line_1["unsatisfied"], "never_assessed");
    assert_eq!(line_1["text"], LINE_1);
    let line_2 = obligation(&json, "criterion:2");
    assert_eq!(
        line_2["outcome"], "satisfied",
        "the assessed line still counts"
    );
    assert_eq!(json["headline_slug"], "readiness_needs_evidence_one");
}

#[test]
fn assessing_it_and_finding_it_still_applies_makes_it_ready() {
    let f = Fixture::new();
    f.assess_still_applies(2);
    let (before, _) = f.answer();
    assert!(!before.ready, "the unassessed line holds it open");

    // The act: the Change's tree version is assessed against the tree the run
    // was produced on, and nothing moved — every linked line still applies.
    // This is the function `thruline change prepare` reaches through
    // `on_intended_bound`.
    let tree: BTreeMap<String, String> = BTreeMap::new();
    let records = evidence_applicability::assess_tree_move(
        &f.ledger,
        "c1",
        "snap_old",
        "snap_new",
        &tree,
        &tree,
        None,
        "2026-01-03T11:00:00Z",
    )
    .expect("assess");
    assert!(
        records
            .iter()
            .all(|r| r.state == ApplicabilityState::Applies),
        "nothing moved, so everything still applies: {records:?}"
    );

    let (after, json) = f.answer();
    assert!(
        after.ready,
        "every line proven on this version and assessed"
    );
    assert_eq!(json["ready"], true);
    let rested: Vec<&str> = after.rests_on.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(rested, vec![LINE_1, LINE_2]);
    assert!(after.missing.is_empty());
}

#[test]
fn the_sentence_for_it_exists_in_both_registers_and_names_the_act() {
    let line = thruline_core::voice::readiness_line("readiness_never_assessed")
        .expect("the never-assessed reason has a sentence");
    assert!(
        line.plain.contains("never assessed"),
        "it says what is missing: {}",
        line.plain
    );
    assert!(
        line.plain.contains("thruline change prepare"),
        "it names the one act that would assess it: {}",
        line.plain
    );
    assert!(!line.technical.trim().is_empty());
}

/// Every key the committed document names equals the engine's served value,
/// recursively; arrays compare element by element and must be the same
/// length. Keys the document leaves out are not compared.
fn assert_subset(path: &str, pinned: &serde_json::Value, served: &serde_json::Value) {
    match (pinned, served) {
        (serde_json::Value::Object(p), serde_json::Value::Object(s)) => {
            for (key, value) in p {
                let here = format!("{path}.{key}");
                let other = s
                    .get(key)
                    .unwrap_or_else(|| panic!("{here}: the engine serves no such field"));
                assert_subset(&here, value, other);
            }
        }
        (serde_json::Value::Array(p), serde_json::Value::Array(s)) => {
            assert_eq!(p.len(), s.len(), "{path}: length");
            for (n, (a, b)) in p.iter().zip(s).enumerate() {
                assert_subset(&format!("{path}[{n}]"), a, b);
            }
        }
        _ => assert_eq!(pinned, served, "{path}"),
    }
}

/// **The pin the card asks for: the UI and the engine cannot disagree about
/// which lines were never assessed.** `ui/src/workspace/never-assessed.readiness.json`
/// is the engine's served answer for this fixture, committed; this test holds
/// it equal to what `readiness()` serves, and `never-assessed.test.js` feeds
/// that same file to the screen's derivation and holds `neverAssessed()` to
/// the lines it names. Change either side alone and one of the two fails.
#[test]
fn the_served_answer_is_the_document_the_screen_is_pinned_against() {
    let f = Fixture::new();
    f.assess_still_applies(2);
    let (_, json) = f.answer();
    let pinned: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../ui/src/workspace/never-assessed.readiness.json"),
        )
        .expect("the pinned document is readable"),
    )
    .expect("the pinned document is JSON");
    assert_subset("readiness", &pinned, &json);
}
