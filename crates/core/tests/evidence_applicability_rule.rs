//! T-S-2 — the dependency rule, the reason classes, replay determinism, and
//! "never relabel", exercised against a real ledger.

use std::collections::BTreeMap;

use rusqlite::params;
use thruline_core::evidence_applicability::{
    assess_tree_move, changed_paths, classify_path, glob_matches, history_for_link,
    invalidating_paths, reassess_for_scope_change, standing_for_change, ApplicabilityState,
    ReasonClass,
};
use thruline_core::ledger::Ledger;

fn seeded(path: &std::path::Path) -> Ledger {
    let ledger = Ledger::open(path).expect("open");
    let conn = ledger.conn();
    conn.execute(
        "INSERT INTO projects(id, root_path, source, display_name, created_at)
         VALUES('p1','/tmp/synthetic','claude_code','Synthetic','2026-01-01T00:00:00Z')",
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
                '[\"crates/core/**\",\"crates/verify/**\"]','[]','[\"cargo-test\"]','digest1')",
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
        "INSERT INTO evidence_bindings(id, seq, recorded_at, schema_version, scope_class,
             scope_key, root, repository_identity, pre_identity, post_identity, exit_code,
             term_signal, bound_at, source_session, doc, doc_sha256)
         VALUES('eb1',0,'2026-01-03T09:00:00Z',1,'build','bash:/tmp/synthetic:cargo clippy',
                '/tmp/synthetic','repo','pre','post',0,NULL,'2026-01-03T09:00:00Z','s1',
                '{\"stdout\":{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":1},
                  \"stderr\":{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":1}}','ddd')",
        [],
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
    let _ = conn;
    ledger
}

fn tree(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(p, h)| (p.to_string(), h.to_string()))
        .collect()
}

// ---------------------------------------------------------------------------
// The dependency rule: declared precision vs the conservative branch.
// ---------------------------------------------------------------------------

#[test]
fn declared_paths_only_invalidate_their_own_paths() {
    let changed = vec![
        "crates/core/src/a.rs".to_string(),
        "docs/notes.md".to_string(),
    ];
    let declared = vec!["crates/core/src/a.rs".to_string()];
    let allowed = vec!["crates/**".to_string(), "docs/**".to_string()];
    let out = invalidating_paths(&changed, Some(&declared), &allowed);
    assert_eq!(out, vec!["crates/core/src/a.rs".to_string()]);
}

#[test]
fn undeclared_falls_to_the_conservative_allowed_scope() {
    let changed = vec![
        "crates/core/src/a.rs".to_string(),
        "unrelated-repo/other.rs".to_string(),
    ];
    let out = invalidating_paths(&changed, None, &["crates/**".to_string()]);
    assert_eq!(out, vec!["crates/core/src/a.rs".to_string()]);
}

#[test]
fn a_change_outside_the_declared_paths_does_not_invalidate() {
    let changed = vec!["docs/notes.md".to_string()];
    let declared = vec!["crates/core/src/a.rs".to_string()];
    let out = invalidating_paths(&changed, Some(&declared), &["**".to_string()]);
    assert!(
        out.is_empty(),
        "an undeclared path must not invalidate: {out:?}"
    );
}

#[test]
fn glob_matching_supports_double_star_and_single_star() {
    assert!(glob_matches("crates/**", "crates/core/src/a.rs"));
    assert!(glob_matches("crates/core/*.rs", "crates/core/a.rs"));
    assert!(!glob_matches("crates/core/*.rs", "crates/core/sub/a.rs"));
    assert!(!glob_matches("crates/verify/**", "crates/core/src/a.rs"));
}

// ---------------------------------------------------------------------------
// Reason classes.
// ---------------------------------------------------------------------------

#[test]
fn each_reason_class_is_produced_by_its_matching_change() {
    assert_eq!(
        classify_path("crates/core/Cargo.lock"),
        ReasonClass::DependenciesChanged
    );
    assert_eq!(
        classify_path("crates/core/tests/foo.rs"),
        ReasonClass::TestsChanged
    );
    assert_eq!(
        classify_path("ui/src/workspace/change-view.test.js"),
        ReasonClass::TestsChanged
    );
    assert_eq!(
        classify_path("crates/core/Cargo.toml"),
        ReasonClass::ConfigurationChanged
    );
    assert_eq!(
        classify_path("crates/core/src/change_evidence.rs"),
        ReasonClass::SourceChanged
    );
}

// ---------------------------------------------------------------------------
// The ledger-backed assessment: replay determinism and never-relabel.
// ---------------------------------------------------------------------------

#[test]
fn editing_a_declared_scope_path_invalidates_the_criterion() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = seeded(&dir.path().join("ledger.db"));

    let old = tree(&[("crates/core/src/change_evidence.rs", "h1")]);
    let new = tree(&[("crates/core/src/change_evidence.rs", "h2")]);
    let records = assess_tree_move(
        &ledger,
        "c1",
        "ws-old",
        "ws-new",
        &old,
        &new,
        None,
        "2026-09-22T05:00:00Z",
    )
    .expect("assess");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].state, ApplicabilityState::NoLongerApplies);
    assert_eq!(records[0].reason_class, Some(ReasonClass::SourceChanged));
    assert_eq!(
        records[0].reason_paths,
        vec!["crates/core/src/change_evidence.rs".to_string()]
    );
}

#[test]
fn an_untouched_criterion_still_applies() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = seeded(&dir.path().join("ledger.db"));

    let old = tree(&[("crates/core/src/change_evidence.rs", "h1")]);
    let new = tree(&[
        ("crates/core/src/change_evidence.rs", "h1"),
        ("docs/unrelated.md", "h9"),
    ]);
    let records = assess_tree_move(
        &ledger,
        "c1",
        "ws-old",
        "ws-new",
        &old,
        &new,
        None,
        "2026-09-22T05:00:00Z",
    )
    .expect("assess");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].state, ApplicabilityState::Applies);
    assert_eq!(records[0].reason_class, None);
}

#[test]
fn a_lockfile_change_reads_dependencies_changed() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = seeded(&dir.path().join("ledger.db"));

    let old = tree(&[("crates/core/Cargo.lock", "h1")]);
    let new = tree(&[("crates/core/Cargo.lock", "h2")]);
    let records = assess_tree_move(
        &ledger,
        "c1",
        "ws-old",
        "ws-new",
        &old,
        &new,
        None,
        "2026-09-22T05:00:00Z",
    )
    .expect("assess");
    assert_eq!(
        records[0].reason_class,
        Some(ReasonClass::DependenciesChanged)
    );
}

#[test]
fn same_inputs_replay_to_the_same_record_id() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = seeded(&dir.path().join("ledger.db"));

    let old = tree(&[("crates/core/src/change_evidence.rs", "h1")]);
    let new = tree(&[("crates/core/src/change_evidence.rs", "h2")]);
    let first = assess_tree_move(
        &ledger,
        "c1",
        "ws-old",
        "ws-new",
        &old,
        &new,
        None,
        "2026-09-22T05:00:00Z",
    )
    .unwrap();
    let second = assess_tree_move(
        &ledger,
        "c1",
        "ws-old",
        "ws-new",
        &old,
        &new,
        None,
        "2026-09-22T05:00:00Z",
    )
    .unwrap();
    assert_eq!(
        first[0].id, second[0].id,
        "same inputs must replay to the same row"
    );

    let count: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM change_criterion_freshness WHERE criterion_evidence_id='cce1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "an identical replay must insert nothing new");
}

#[test]
fn the_old_run_keeps_its_own_record_after_a_second_tree_move() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = seeded(&dir.path().join("ledger.db"));

    let v1 = tree(&[("crates/core/src/change_evidence.rs", "h1")]);
    let v2 = tree(&[("crates/core/src/change_evidence.rs", "h2")]);
    let v3 = tree(&[("crates/core/src/change_evidence.rs", "h3")]);
    let first = assess_tree_move(
        &ledger,
        "c1",
        "ws1",
        "ws2",
        &v1,
        &v2,
        None,
        "2026-09-22T05:00:00Z",
    )
    .unwrap();
    let second = assess_tree_move(
        &ledger,
        "c1",
        "ws2",
        "ws3",
        &v2,
        &v3,
        None,
        "2026-09-22T05:05:00Z",
    )
    .unwrap();
    assert_ne!(first[0].id, second[0].id);

    let history = history_for_link(&ledger, "cce1").unwrap();
    assert_eq!(
        history.len(),
        2,
        "both assessments must be visible in history"
    );
    assert_eq!(history[0].id, first[0].id);
    assert_eq!(history[0].new_snapshot, "ws2");
    assert_eq!(history[0].recorded_at, "2026-09-22T05:00:00Z");
    assert_eq!(history[1].id, second[0].id);

    // The direct byte-check: the first row's own columns, read straight from
    // the table, are exactly what it was written with.
    let (new_snapshot, recorded_at): (String, String) = ledger
        .conn()
        .query_row(
            "SELECT new_snapshot, recorded_at FROM change_criterion_freshness WHERE id=?1",
            params![first[0].id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        new_snapshot, "ws2",
        "the old row's snapshot must not have moved"
    );
    assert_eq!(
        recorded_at, "2026-09-22T05:00:00Z",
        "the old row's time must not have moved"
    );
}

#[test]
fn no_writer_in_this_module_ever_issues_update_or_delete_against_the_table() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/evidence_applicability.rs"),
    )
    .expect("read the module source");
    for line in source.lines() {
        let lower = line.to_lowercase();
        if lower.contains("change_criterion_freshness") {
            assert!(
                !lower.contains("update ") && !lower.contains("delete from"),
                "a writer issues UPDATE/DELETE against change_criterion_freshness: {line:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Criteria-changed reassessment on a new accepted scope version.
// ---------------------------------------------------------------------------

#[test]
fn a_changed_criterion_is_flagged_and_an_unchanged_one_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = seeded(&dir.path().join("ledger.db"));
    let conn = ledger.conn();
    conn.execute(
        "INSERT INTO change_criterion_evidence(id, change_id, scope_version, criterion_index,
             evidence_kind, evidence_id, session_id, basis, detail, actor, asserted_at, reason)
         VALUES('cce2','c1',1,2,'check_run','eb1','s1','declared','cargo-test',NULL,
                '2026-01-03T09:10:00Z',NULL)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at, criteria_json,
             exclusions_json, allowed_scope_json, constraints_json, check_profile_json,
             content_sha256)
         VALUES('c1',2,'2026-01-04T09:30:00Z','[\"the retry test passes with a longer timeout\",\"lint is clean\"]',
                '[]','[\"crates/core/**\"]','[]','[\"cargo-test\"]','digest2')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO change_scope_acceptances(change_id, scope_version, accepted_by, accepted_at,
             rationale)
         VALUES('c1',2,'sam','2026-01-04T09:40:00Z','tightened it')",
        [],
    )
    .unwrap();
    let _ = conn;

    let records = reassess_for_scope_change(&ledger, "c1", 1, 2, "2026-01-04T09:41:00Z").unwrap();
    assert_eq!(
        records.len(),
        1,
        "only the changed criterion (index 1) is flagged"
    );
    assert_eq!(records[0].criterion_evidence_id, "cce1");
    assert_eq!(records[0].reason_class, Some(ReasonClass::CriteriaChanged));

    let standing = standing_for_change(&ledger, "c1").unwrap();
    assert!(
        !standing.contains_key("cce2"),
        "criterion 2's text did not change and must keep no new record"
    );
    assert_eq!(
        standing.get("cce1").unwrap().reason_class,
        Some(ReasonClass::CriteriaChanged)
    );
}

// ---------------------------------------------------------------------------
// T-3-3-F2 — the same-second tie-break. `recorded_at` is second-granular, so
// two assessments of one link inside one second tie on it and the order falls
// to the secondary key. `rowid` is the table's true append order; `id` is a
// content sha256 and therefore arbitrary. These two tests are deliberate
// mirrors over the SAME pair of records inserted in opposite orders, so under
// an arbitrary tie-break exactly one of them must go red whichever way the
// hashes happen to sort — the guard is deterministic, not probabilistic.
// ---------------------------------------------------------------------------

/// The tree the evidence was produced against, and the edited tree.
fn f2_trees() -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    (
        tree(&[("crates/core/src/change_evidence.rs", "h1")]),
        tree(&[("crates/core/src/change_evidence.rs", "h2")]),
    )
}

#[test]
fn two_assessments_in_one_second_stand_on_the_later_append() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = seeded(&dir.path().join("ledger.db"));
    let (base, edited) = f2_trees();
    let at = "2026-09-22T05:00:00Z";

    // Still against its own digests: applies.
    let first =
        assess_tree_move(&ledger, "c1", "ws-base", "ws-base", &base, &base, None, at).unwrap();
    // Then the edit lands, in the same second: no longer applies.
    let second = assess_tree_move(
        &ledger, "c1", "ws-base", "ws-edit", &base, &edited, None, at,
    )
    .unwrap();
    assert_eq!(first[0].state, ApplicabilityState::Applies);
    assert_eq!(second[0].state, ApplicabilityState::NoLongerApplies);

    let standing = standing_for_change(&ledger, "c1").unwrap();
    let standing = standing
        .get("cce1")
        .expect("the link has a standing record");
    assert_eq!(
        standing.id, second[0].id,
        "the last appended record stands, not the lowest id"
    );
    assert_eq!(standing.state, ApplicabilityState::NoLongerApplies);

    let history = history_for_link(&ledger, "cce1").unwrap();
    assert_eq!(history.len(), 2, "both records are kept");
    assert_eq!(history[0].recorded_at, history[1].recorded_at);
    assert_eq!(history[0].id, first[0].id, "history is in append order");
    assert_eq!(history[1].id, second[0].id, "history is in append order");
}

#[test]
fn the_same_second_pair_in_the_opposite_order_stands_on_applies() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = seeded(&dir.path().join("ledger.db"));
    let (base, edited) = f2_trees();
    let at = "2026-09-22T05:00:00Z";

    // The mirror: the edit is assessed first...
    let first = assess_tree_move(
        &ledger, "c1", "ws-base", "ws-edit", &base, &edited, None, at,
    )
    .unwrap();
    // ...then the tree returns to the evidence's own digests, in the same second.
    let second =
        assess_tree_move(&ledger, "c1", "ws-base", "ws-base", &base, &base, None, at).unwrap();
    assert_eq!(first[0].state, ApplicabilityState::NoLongerApplies);
    assert_eq!(second[0].state, ApplicabilityState::Applies);

    let standing = standing_for_change(&ledger, "c1").unwrap();
    let standing = standing
        .get("cce1")
        .expect("the link has a standing record");
    assert_eq!(
        standing.id, second[0].id,
        "the last appended record stands, not the lowest id"
    );
    assert_eq!(standing.state, ApplicabilityState::Applies);

    let history = history_for_link(&ledger, "cce1").unwrap();
    assert_eq!(history.len(), 2, "both records are kept");
    assert_eq!(history[0].recorded_at, history[1].recorded_at);
    assert_eq!(history[0].id, first[0].id, "history is in append order");
    assert_eq!(history[1].id, second[0].id, "history is in append order");
}

// ---------------------------------------------------------------------------
// NEVER-ASSESSED-NOT-READY-R3 — a backward clock. `recorded_at` is the
// caller's wall-clock read and nothing makes it non-decreasing between two
// runs. The later append must stand, and history must read in append order,
// even when the later append carries the EARLIER clock. `recorded_at` stays
// on each record as data.
// ---------------------------------------------------------------------------

#[test]
fn a_later_append_with_an_earlier_clock_still_stands() {
    let dir = tempfile::tempdir().unwrap();
    let ledger = seeded(&dir.path().join("ledger.db"));
    let (base, edited) = f2_trees();

    let first = assess_tree_move(
        &ledger,
        "c1",
        "ws-base",
        "ws-base",
        &base,
        &base,
        None,
        "2026-09-22T05:00:00Z",
    )
    .unwrap();
    // Appended second, stamped half an hour EARLIER.
    let second = assess_tree_move(
        &ledger,
        "c1",
        "ws-base",
        "ws-edit",
        &base,
        &edited,
        None,
        "2026-09-22T04:30:00Z",
    )
    .unwrap();
    assert_eq!(first[0].state, ApplicabilityState::Applies);
    assert_eq!(second[0].state, ApplicabilityState::NoLongerApplies);

    let standing = standing_for_change(&ledger, "c1").unwrap();
    let standing = standing
        .get("cce1")
        .expect("the link has a standing record");
    assert_eq!(
        standing.id, second[0].id,
        "the last appended record stands, whatever its clock says"
    );
    assert_eq!(standing.state, ApplicabilityState::NoLongerApplies);
    assert_eq!(
        standing.recorded_at, "2026-09-22T04:30:00Z",
        "recorded_at is kept as data, unaltered"
    );

    let history = history_for_link(&ledger, "cce1").unwrap();
    assert_eq!(history.len(), 2, "both records are kept");
    assert_eq!(history[0].id, first[0].id, "history is in append order");
    assert_eq!(history[1].id, second[0].id, "history is in append order");
    assert_eq!(history[0].recorded_at, "2026-09-22T05:00:00Z");
    assert_eq!(history[1].recorded_at, "2026-09-22T04:30:00Z");
}

#[test]
fn changed_paths_reports_additions_removals_and_edits() {
    let old = tree(&[("a", "1"), ("b", "1")]);
    let new = tree(&[("a", "1"), ("b", "2"), ("c", "1")]);
    let mut out = changed_paths(&old, &new);
    out.sort();
    assert_eq!(out, vec!["b".to_string(), "c".to_string()]);
}
