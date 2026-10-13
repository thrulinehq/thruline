//! A92 additive half: identity persists, groups project, and nothing merges.

use thruline_core::ledger::Ledger;
use thruline_core::workspace::{group_by_repo, Basis, RootVia, WorkspaceIdentityRow};

fn row(project_id: &str, repo_id: Option<&str>, root: Option<&str>) -> WorkspaceIdentityRow {
    WorkspaceIdentityRow {
        project_id: project_id.to_string(),
        repo_id: repo_id.map(str::to_string),
        basis: if repo_id.is_some() {
            Basis::Git.as_str().to_string()
        } else {
            Basis::Unresolvable.as_str().to_string()
        },
        root_commits_json: "[]".to_string(),
        remote_fingerprints_json: "[]".to_string(),
        resolved_root: root.map(str::to_string),
        resolved_via: root.map(|_| RootVia::RecordedCwd.as_str().to_string()),
        sessions_sampled: 3,
        observed_at: "2026-08-04T00:00:00Z".to_string(),
    }
}

#[test]
fn one_repo_many_working_roots_groups_without_merging() {
    // The A88 shape: one repo entered at three working roots, each of which is
    // still its own project with its own chain.
    let rows = vec![
        row("-r-market", Some("repo-a"), Some("/r/market")),
        row("-r-market-docs-a", Some("repo-a"), Some("/r/market/docs/a")),
        row("-r-market-docs-b", Some("repo-a"), Some("/r/market/docs/b")),
        row("-r-other", Some("repo-b"), Some("/r/other")),
    ];
    let groups = group_by_repo(&rows);
    assert_eq!(groups.len(), 2, "two repos");

    let market = groups
        .iter()
        .find(|g| g.repo_id == "repo-a")
        .expect("repo-a present");
    assert_eq!(market.roots.len(), 3, "three working roots under one repo");

    // Every project id survives the projection — grouping must never collapse
    // them, because each carries its own chain genesis.
    let ids: Vec<&str> = market.roots.iter().map(|r| r.project_id.as_str()).collect();
    assert_eq!(ids, ["-r-market", "-r-market-docs-a", "-r-market-docs-b"]);

    // The subdirectory launch keeps its OWN working root, not the repo root.
    let sub = market
        .roots
        .iter()
        .find(|r| r.project_id == "-r-market-docs-a")
        .expect("subdirectory root present");
    assert_eq!(sub.working_root.as_deref(), Some("/r/market/docs/a"));
}

#[test]
fn abstaining_projects_belong_to_no_group() {
    let rows = vec![
        row("-resolved", Some("repo-a"), Some("/r/a")),
        row("-gone", None, None),
    ];
    let groups = group_by_repo(&rows);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].roots.len(), 1);
    assert_eq!(
        groups[0].roots[0].project_id, "-resolved",
        "a project we cannot place must not be placed anywhere — putting an \
         unresolvable project in a group would be a guess"
    );
}

#[test]
fn grouping_is_deterministic() {
    let a = vec![
        row("-b", Some("repo-2"), Some("/b")),
        row("-a", Some("repo-1"), Some("/a")),
    ];
    let b = vec![
        row("-a", Some("repo-1"), Some("/a")),
        row("-b", Some("repo-2"), Some("/b")),
    ];
    assert_eq!(
        group_by_repo(&a),
        group_by_repo(&b),
        "input order must not change the projection"
    );
}

#[test]
fn identity_round_trips_through_the_ledger() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    ledger
        .upsert_project(
            &thruline_core::model::ProjectRef {
                id: "-p".to_string(),
                root_path: "/p".to_string(),
                source: thruline_core::model::Source::ClaudeCode,
                display_name: Some("p".to_string()),
            },
            "2026-08-04T00:00:00Z",
        )
        .expect("project");

    let written = row("-p", Some("repo-a"), Some("/p"));
    ledger
        .record_workspace_identity(&written)
        .expect("record identity");
    let read = ledger.workspace_identities().expect("read back");
    assert_eq!(read, vec![written.clone()]);

    // Derived and re-runnable: a second backfill converges rather than
    // conflicting on the primary key.
    ledger
        .record_workspace_identity(&written)
        .expect("re-record is idempotent");
    assert_eq!(ledger.workspace_identities().expect("read").len(), 1);
}

#[test]
fn every_enum_string_satisfies_the_schema_check_domain() {
    // The schema pins `basis` and `resolved_via` as CHECK domains. If an enum
    // gains a variant whose string is not in the domain, the failure would
    // otherwise appear at runtime, mid-backfill, on someone's real ledger.
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    ledger
        .upsert_project(
            &thruline_core::model::ProjectRef {
                id: "-p".to_string(),
                root_path: "/p".to_string(),
                source: thruline_core::model::Source::ClaudeCode,
                display_name: None,
            },
            "2026-08-04T00:00:00Z",
        )
        .expect("project");

    for basis in [Basis::Git, Basis::Marker, Basis::Unresolvable] {
        for via in [
            Some(RootVia::RecordedCwd),
            Some(RootVia::Reconstructed),
            None,
        ] {
            let mut r = row("-p", Some("repo"), Some("/p"));
            r.basis = basis.as_str().to_string();
            r.resolved_via = via.map(|v| v.as_str().to_string());
            ledger.record_workspace_identity(&r).unwrap_or_else(|e| {
                panic!(
                    "{} / {:?} rejected by the CHECK domain: {e}",
                    basis.as_str(),
                    via
                )
            });
        }
    }
}
