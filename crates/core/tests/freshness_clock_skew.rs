//! NEVER-ASSESSED-NOT-READY-R3 — the standing freshness row is the last one
//! appended, never the one with the latest clock.
//!
//! Re-created from the round-2 adversary's reproduction
//! (`adv_freshness_clock_skew`, gate NEVER-ASSESSED-NOT-READY-R2 at 9ff20680).
//! `evidence_applicability::standing_for_change` used to fold every
//! `change_criterion_freshness` row for a link by
//! `ORDER BY criterion_evidence_id, recorded_at ASC, rowid ASC`, so the
//! caller-supplied `at` string chose the standing row and `rowid` (the true,
//! append-only write order) only broke ties. A backward clock step between two
//! real `change prepare` runs then served the OLDER "applies" row, and the
//! criterion read Ready although the latest assessment, against the tree the
//! new binding captured, said "no longer applies".
//!
//! The control (forward clock) must pass before and after the fix; the attack
//! (backward clock) is red before it.
//!
//! Every string here is synthetic. Nothing came off a real machine.

use thruline_core::change_evidence::{coverage, derive_links, Observed};
use thruline_core::change_propose::ProfileEnvironment;
use thruline_core::change_readiness::readiness;
use thruline_core::evidence_applicability::{self, ApplicabilityState};
use thruline_core::examined_inputs::{self, ExaminedInputs, Input, SubjectKind, Uncommitted};
use thruline_core::ledger::Ledger;

const LINE_1: &str = "The service builds cleanly after a restart.";

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
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("work/crates/core")).expect("work dir");
        std::fs::write(dir.path().join("work/crates/core/a.rs"), b"fn a() {}\n")
            .expect("seed file");
        let work = dir.path().join("work");
        let git = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(&work)
                .env("GIT_AUTHOR_NAME", "adv")
                .env("GIT_AUTHOR_EMAIL", "adv@example.com")
                .env("GIT_COMMITTER_NAME", "adv")
                .env("GIT_COMMITTER_EMAIL", "adv@example.com")
                .status()
                .expect("git");
            assert!(status.success(), "git {args:?} failed");
        };
        git(&["init", "-q"]);
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "seed"]);
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
        // The first intended bind, healthy: a real stored snapshot.
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
        // The evidence's OWN examined-inputs record carries its own snapshot
        // (T-3-3-F1 path) taken BEFORE the file is edited below, so a later
        // comparison against it can genuinely detect the edit.
        let mut examined = inputs_for(SubjectKind::CheckRun, "eb_build");
        examined.snapshot_id = Input::known(f.snap("2026-01-03T08:59:00Z"));
        examined_inputs::record(&f.ledger, &examined, "2026-01-03T09:00:00Z").expect("examined");

        derive_links(&f.ledger, "c1", "2026-01-03T09:05:00Z").expect("derive");
        f
    }

    fn snap(&self, at: &str) -> String {
        let snap =
            thruline_core::snapshot::capture(&self.dir.path().join("work"), at).expect("capture");
        thruline_core::snapshot::store(&self.ledger, &snap).expect("store");
        snap.id
    }

    /// Exactly what `change prepare` does, with its own capture succeeding:
    /// read the standing record, write the new one, sweep. `at` is
    /// caller-supplied, exactly as the CLI supplies a wall-clock read.
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

    fn edit_tracked_file(&self) {
        std::fs::write(
            self.dir.path().join("work/crates/core/a.rs"),
            b"fn a() { changed(); }\n",
        )
        .expect("edit");
    }

    fn line_1(&self) -> (bool, Option<bool>) {
        let mut env = ProfileEnvironment::empty();
        env.tools_present.insert("cargo".to_string());
        let view = coverage(&self.ledger, "c1", &env).expect("coverage");
        assert_eq!(view.criteria.len(), 1);
        let fresh = view.criteria[0]
            .items
            .iter()
            .find(|i| i.observed == Observed::Proven)
            .and_then(|i| i.fresh);
        let r = readiness(&self.ledger, "c1", &view).expect("readiness");
        (r.ready, fresh)
    }
}

/// Control: two prepares in the natural, forward-clock order. The second
/// one, after the tracked file is edited, correctly reports stale and the
/// criterion is correctly not Ready.
#[test]
fn control_forward_clock_correctly_reports_stale_after_an_edit() {
    let f = Fixture::new();
    let r1 = f.prepare("2026-01-03T10:00:00Z");
    assert_eq!(r1.len(), 1);
    assert_eq!(r1[0].state, ApplicabilityState::Applies);
    assert_eq!(
        f.line_1(),
        (true, Some(true)),
        "control: ready after first prepare"
    );

    f.edit_tracked_file();
    let r2 = f.prepare("2026-01-03T11:00:00Z");
    assert_eq!(r2.len(), 1);
    assert_eq!(
        r2[0].state,
        ApplicabilityState::NoLongerApplies,
        "the edit must be detected: {:?}",
        r2[0]
    );
    assert_eq!(
        f.line_1(),
        (false, Some(false)),
        "control: correctly stale and not ready after the edit"
    );
}

/// The attack: the SAME sequence of real edits and real prepares, except the
/// second (truly later, rowid-later) bind is stamped with an EARLIER `at`
/// than the first — a clock correction between two `change prepare` runs,
/// or a second machine whose clock is behind. Nothing in `record()` or
/// `bind_intended_at` requires `at` to be non-decreasing.
///
/// If `standing_for_change` is sound, the truly-latest assessment (against
/// the truly-current, edited tree) must still be what is served: the
/// criterion must NOT read Ready, and the item must NOT read "still
/// applies", no matter what string the caller stamped it with.
#[test]
fn a_backward_clock_between_two_real_prepares_must_not_serve_the_older_applies_row() {
    let f = Fixture::new();
    let r1 = f.prepare("2026-01-03T10:00:00Z");
    assert_eq!(r1.len(), 1);
    assert_eq!(r1[0].state, ApplicabilityState::Applies);
    assert_eq!(
        f.line_1(),
        (true, Some(true)),
        "ready after the first, honest prepare"
    );

    f.edit_tracked_file();
    // The SAME real edit as the control, the SAME real second prepare call,
    // against the SAME evidence's own tree — only the `at` stamp differs,
    // and it goes backward instead of forward.
    let r2 = f.prepare("2026-01-03T09:30:00Z");
    assert_eq!(r2.len(), 1, "the edit is still assessed: {r2:?}");
    assert_eq!(
        r2[0].state,
        ApplicabilityState::NoLongerApplies,
        "the record ITSELF correctly says the edit invalidates it: {:?}",
        r2[0]
    );

    // The record layer got it right (r2 above). The question is what is
    // SERVED — the whole point of a "standing" fold.
    let (ready, fresh) = f.line_1();
    eprintln!("SERVED after backward-clock prepare: ready={ready} fresh={fresh:?}");
    assert!(
        !ready,
        "BROKEN: the criterion reads Ready off a row that was not compared \
         against the tree the new binding actually captured (served ready={ready}, fresh={fresh:?})."
    );
    assert_eq!(
        fresh,
        Some(false),
        "BROKEN: the served item reads `fresh: {fresh:?}` (still applies) even \
         though the truly latest, truly real assessment against the truly \
         current tree recorded no_longer_applies — standing_for_change chose \
         the OLDER row because it sorts by the caller-supplied `at` string \
         before `rowid`, and `at` went backward between two real prepares."
    );
}
