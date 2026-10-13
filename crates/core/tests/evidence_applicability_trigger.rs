//! T-S-2 (continued) — the trigger: binding a new `intended` examined-inputs
//! record for a Change is the moment T-3-2 already marks "the tree version
//! moved," and it is now also the moment `evidence_applicability::assess_tree_move`
//! runs, from the two stored workspace snapshots either side of the bind and
//! nothing else (no git diff, no inference beyond what was recorded).

use std::process::Command;

use thruline_core::evidence_applicability::{standing_for_change, ApplicabilityState, ReasonClass};
use thruline_core::examined_inputs::bind_intended_at;
use thruline_core::ledger::Ledger;

fn git(root: &std::path::Path, args: &[&str]) {
    let status = Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .arg("-C")
        .arg(root)
        .args(args)
        .status()
        .expect("git must run");
    assert!(status.success(), "git {args:?} failed");
}

#[test]
fn editing_a_file_under_scope_between_two_bindings_invalidates_the_criterion() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    std::fs::write(root.join("a.rs"), b"fn one() {}\n").unwrap();
    git(root, &["init", "-q"]);
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "init"]);

    let store = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(store.path().join("ledger.db")).expect("open");
    let conn = ledger.conn();
    conn.execute(
        "INSERT INTO projects(id, root_path, source, display_name, created_at)
         VALUES('p1',?1,'claude_code','Synthetic','2026-01-01T00:00:00Z')",
        [root.to_string_lossy().to_string()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO changes(id, project_id, opened_at, title, purpose, requester, request_source)
         VALUES('c1','p1','2026-01-02T09:00:00Z','T','P','sam','person')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at, criteria_json,
             exclusions_json, allowed_scope_json, constraints_json, check_profile_json,
             content_sha256)
         VALUES('c1',1,'2026-01-02T09:30:00Z','[\"the retry test passes\"]','[]',
                '[\"**\"]','[]','[\"cargo-test\"]','digest1')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO change_scope_acceptances(change_id, scope_version, accepted_by, accepted_at,
             rationale)
         VALUES('c1',1,'sam','2026-01-02T09:40:00Z','this is what counts')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sessions(id, project_id, file_path, started_at)
         VALUES('s1','p1','/tmp/synthetic/s1.jsonl','2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO evidence_bindings(id, seq, recorded_at, schema_version, scope_class,
             scope_key, root, repository_identity, pre_identity, post_identity, exit_code,
             term_signal, bound_at, source_session, doc, doc_sha256)
         VALUES('eb1',0,'2026-01-03T09:00:00Z',1,'build','bash:root:clippy',
                ?1,'repo','pre','post',0,NULL,'2026-01-03T09:00:00Z','s1',
                '{\"stdout\":{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":1},
                  \"stderr\":{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":1}}','ddd')",
        [root.to_string_lossy().to_string()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO change_criterion_evidence(id, change_id, scope_version, criterion_index,
             evidence_kind, evidence_id, session_id, basis, detail, actor, asserted_at, reason)
         VALUES('cce1','c1',1,1,'check_run','eb1','s1','declared','cargo-test',NULL,
                '2026-01-03T09:10:00Z',NULL)",
        [],
    )
    .unwrap();
    // The session is confirmed, in scope, so the criterion reads Proven
    // before anything moves — the state this test proves an invalidation
    // takes away.
    conn.execute(
        "INSERT INTO change_session_associations(id, change_id, session_id, provenance, basis,
             actor, asserted_at, reason)
         VALUES('a1','c1','s1','confirmed','user','sam','2026-01-02T10:00:00Z','mine')",
        [],
    )
    .unwrap();
    // s1 is a captured execution (T-3-2b), which is what lets the scope gate
    // pass it with no changed-file events of its own.
    conn.execute(
        "INSERT INTO change_check_sessions(session_id, binding_id, change_id, ran_in, actor,
             recorded_at)
         VALUES('s1','eb1','c1','test','sam','2026-01-03T09:00:00Z')",
        [],
    )
    .unwrap();
    let _ = conn;

    // First bind: nothing standing yet, so nothing to compare — the baseline.
    bind_intended_at(&ledger, "c1", "2026-01-03T09:20:00Z", Some(root))
        .expect("first bind")
        .expect("a change with an established root binds");
    let before = standing_for_change(&ledger, "c1").unwrap();
    assert!(
        !before.contains_key("cce1"),
        "a first binding is not a tree move and must write no record"
    );

    // The check run itself examined the same tree the Change is now bound
    // to — recorded the way `thruline check` records it, so the T-3-2
    // "applies" axis reads true and this test isolates the T-S-2 freshness
    // axis rather than riding on the other one.
    let mut check_run_inputs = thruline_core::examined_inputs::standing(
        &ledger,
        thruline_core::examined_inputs::SubjectKind::Intended,
        "c1",
    )
    .unwrap()
    .expect("the intended record was just bound");
    check_run_inputs.subject_kind = thruline_core::examined_inputs::SubjectKind::CheckRun;
    check_run_inputs.subject_id = "eb1".to_string();
    thruline_core::examined_inputs::record(&ledger, &check_run_inputs, "2026-01-03T09:10:00Z")
        .expect("record the check run's own examined inputs");

    let environment =
        thruline_core::change_propose::machine_environment(&ledger, "c1").expect("environment");
    let coverage_before =
        thruline_core::change_evidence::coverage(&ledger, "c1", &environment).expect("coverage");
    assert_eq!(
        coverage_before.criteria[0].state,
        thruline_core::change_evidence::CoverageState::Evidence,
        "before the tree moves the criterion must read Proven: {:?}",
        coverage_before.criteria[0]
    );

    // The tree moves: a file under the accepted scope is edited without a
    // new commit (uncommitted, but still a real, recorded change).
    std::fs::write(root.join("a.rs"), b"fn one() { let x = 1; }\n").unwrap();

    // Second bind: this is the trigger.
    bind_intended_at(&ledger, "c1", "2026-01-03T09:30:00Z", Some(root))
        .expect("second bind")
        .expect("still bound");

    let after = standing_for_change(&ledger, "c1").unwrap();
    let record = after
        .get("cce1")
        .expect("the tree move must have produced a freshness record for cce1");
    assert_eq!(record.state, ApplicabilityState::NoLongerApplies);
    assert_eq!(record.reason_class, Some(ReasonClass::SourceChanged));
    assert!(
        record.reason_paths.iter().any(|p| p.ends_with("a.rs")),
        "the path must be named: {:?}",
        record.reason_paths
    );

    // The conclusion: the same criterion, freshly invalidated, no longer
    // counts as proven — an unrelated green run does not paper over it.
    let coverage_after =
        thruline_core::change_evidence::coverage(&ledger, "c1", &environment).expect("coverage");
    assert_ne!(
        coverage_after.criteria[0].state,
        thruline_core::change_evidence::CoverageState::Evidence,
        "an invalidated criterion must not still read Proven: {:?}",
        coverage_after.criteria[0]
    );
    assert_eq!(
        coverage_after.criteria[0].items[0].fresh,
        Some(false),
        "the item itself must say it no longer applies"
    );
}
