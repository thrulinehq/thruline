//! The cheap inline sweep is a read-only diagnostic over an exact Git tree.

use std::path::Path;
use std::process::Command;

use rusqlite::params;
use serde_json::json;
use thruline_core::decisions::engine::CHECK_IMPL_VERSION;
use thruline_core::decisions::inline;
use thruline_core::decisions::storage::{self, VerdictInput};
use thruline_core::ledger::Ledger;

#[path = "support/person_action.rs"]
mod person_action;
use person_action::PersonRecorder;

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git");
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8(output.stdout).unwrap().trim().into()
}

#[test]
fn active_sweep_reuses_current_cache_and_excludes_uncached_e_without_writing() {
    let repo = tempfile::tempdir().expect("repo");
    git(repo.path(), &["init", "-q"]);
    std::fs::write(repo.path().join("kept.txt"), "present\n").unwrap();
    git(repo.path(), &["add", "."]);
    git(
        repo.path(),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "basis",
        ],
    );
    let version = git(repo.path(), &["rev-parse", "HEAD"]);
    let tree = git(repo.path(), &["rev-parse", "HEAD^{tree}"]);
    let git_tool = git(repo.path(), &["--version"]);
    let store = tempfile::tempdir().expect("store");
    let db = store.path().join("ledger.db");
    let ledger = Ledger::open(&db).expect("ledger");
    ledger.conn().execute(
        "INSERT INTO projects(id,root_path,source,created_at) VALUES('inline',?1,'codex','2026-10-07T00:00:00Z')",
        [repo.path().to_string_lossy().as_ref()],
    ).unwrap();
    let mut person = PersonRecorder::new(&ledger, "inline");
    for (id, kind, rule) in [
        (
            "a1",
            "text_absent",
            json!({"kind":"text_absent","terms":["trial"],"synonyms":[],"surfaces":["code"],"allowlist":[]}),
        ),
        (
            "e1",
            "command_passes",
            json!({"kind":"command_passes","command":"cargo test","is_test":true,"min_passed":1}),
        ),
        (
            "c1",
            "visual_match",
            json!({"kind":"visual_match","tolerance_pct":0.5,"screens":[{
                "name":"Inbox","route":"/#/design/Inbox","scenario":"design",
                "reference_path":"docs/hq-next/reference/Inbox.png",
                "reference_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "width":1512,"height":920,"full_page":false,"mask_rectangles":[]
            }]}),
        ),
    ] {
        ledger.conn().execute("INSERT INTO decisions(id,project_id,created_at) VALUES(?1,'inline','2026-10-07T00:00:00Z')", [id]).unwrap();
        ledger.conn().execute(
            "INSERT INTO decision_revisions(decision_id,rev,kind,params_schema_version,params_json,text,sources_json,author_actor,recorded_at)
             VALUES(?1,1,?2,1,?3,'fixture','[]','person','2026-10-07T00:00:00Z')",
            params![id,kind,rule.to_string()],
        ).unwrap();
        let act = format!("accept-{id}");
        ledger.conn().execute(
            "INSERT INTO decision_acceptances(id,decision_id,rev,actor,accepted_at,rationale,severity,applies_from_version)
             VALUES(?1,?2,1,'person','2026-10-07T00:01:00Z','fixture','warns',?3)",
            params![act,id,version],
        ).unwrap();
        person.bind_bridge_acceptance(&ledger, &act);
    }
    let cached_id = storage::record_verdict(
        &ledger,
        VerdictInput {
            decision_id: "a1".into(),
            revision: 1,
            version_id: version.clone(),
            tree_hash: tree.clone(),
            verdict: "broken".into(),
            reason: Some("stored exact-tree witness".into()),
            checkable_if: None,
            findings: json!([{"surface":"code","path":"kept.txt","line":1,"span":"trial"}]),
            surfaces: json!([]),
            check_impl_version: CHECK_IMPL_VERSION.into(),
            tool_versions: json!({"git":git_tool}),
            computed_at: "2026-10-07T00:02:00Z".into(),
            basis: json!({"checked_places":1}),
        },
    )
    .unwrap();
    storage::record_verdict(
        &ledger,
        VerdictInput {
            decision_id: "c1".into(),
            revision: 1,
            version_id: version.clone(),
            tree_hash: tree.clone(),
            verdict: "holds".into(),
            reason: Some("obsolete visual receipt".into()),
            checkable_if: None,
            findings: json!([]),
            surfaces: json!([]),
            check_impl_version: CHECK_IMPL_VERSION.into(),
            tool_versions: json!({"git":git_tool,"visual":{"browser":"old"}}),
            computed_at: "2026-10-07T00:02:00Z".into(),
            basis: json!({"checked_places":1}),
        },
    )
    .unwrap();
    let before: i64 = ledger
        .conn()
        .query_row("SELECT count(*) FROM decision_verdicts", [], |row| {
            row.get(0)
        })
        .unwrap();
    drop(ledger);
    let snapshot = inline::cached_read_only(&db, repo.path(), "inline", &version, false).unwrap();
    assert_eq!(snapshot.results.len(), 3);
    assert_eq!(snapshot.results[0].decision_id, "a1");
    assert!(snapshot.results[0].from_cache);
    let snapshot_c1 = snapshot
        .results
        .iter()
        .find(|row| row.decision_id == "c1")
        .unwrap();
    assert_eq!(snapshot_c1.state, "unverifiable");
    assert!(!snapshot_c1.from_cache);
    assert!(snapshot_c1
        .reason
        .as_deref()
        .unwrap()
        .contains("explicit full check"));
    let snapshot_e1 = snapshot
        .results
        .iter()
        .find(|row| row.decision_id == "e1")
        .unwrap();
    assert_eq!(snapshot_e1.state, "unverifiable");
    assert!(snapshot_e1.full_command.is_some());
    let sweep = inline::evaluate_read_only(&db, repo.path(), "inline", &version, false).unwrap();
    assert_eq!(sweep.results.len(), 3);
    let a1 = sweep
        .results
        .iter()
        .find(|row| row.decision_id == "a1")
        .unwrap();
    assert_eq!(a1.state, "broken");
    assert!(a1.from_cache);
    assert_eq!(a1.verdict_id.as_deref(), Some(cached_id.as_str()));
    let e1 = sweep
        .results
        .iter()
        .find(|row| row.decision_id == "e1")
        .unwrap();
    assert_eq!(e1.state, "unverifiable");
    assert!(!e1.from_cache);
    assert!(e1
        .reason
        .as_deref()
        .unwrap()
        .contains("explicit full check"));
    assert!(e1
        .full_command
        .as_deref()
        .unwrap()
        .contains("decisions check"));
    let c1 = sweep
        .results
        .iter()
        .find(|row| row.decision_id == "c1")
        .unwrap();
    assert_eq!(c1.state, "unverifiable");
    assert!(!c1.from_cache);
    assert!(c1
        .reason
        .as_deref()
        .unwrap()
        .contains("explicit full check"));
    let ledger = Ledger::open(&db).unwrap();
    let after: i64 = ledger
        .conn()
        .query_row("SELECT count(*) FROM decision_verdicts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(after, before);
}

#[test]
fn proposed_a1_b3_b4_diagnostic_names_the_exact_tree_without_writing_a_verdict() {
    let repo = tempfile::tempdir().expect("repo");
    git(repo.path(), &["init", "-q"]);
    std::fs::create_dir_all(repo.path().join("tests")).unwrap();
    std::fs::create_dir_all(repo.path().join(".github/workflows")).unwrap();
    std::fs::write(
        repo.path().join("tests/basic.rs"),
        "#[test] fn basic() { assert_eq!(2, 2); }\n",
    )
    .unwrap();
    std::fs::write(repo.path().join(".github/workflows/ci.yml"), "name: CI\non: push\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - name: test\n        run: cargo test\n").unwrap();
    git(repo.path(), &["add", "."]);
    git(
        repo.path(),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "basis",
        ],
    );
    let version = git(repo.path(), &["rev-parse", "HEAD"]);
    let tree = git(repo.path(), &["rev-parse", "HEAD^{tree}"]);
    let store = tempfile::tempdir().expect("store");
    let db = store.path().join("ledger.db");
    let ledger = Ledger::open(&db).expect("ledger");
    ledger.conn().execute(
        "INSERT INTO projects(id,root_path,source,created_at) VALUES('inline',?1,'codex','2026-10-07T00:00:00Z')",
        [repo.path().to_string_lossy().as_ref()],
    ).unwrap();
    let cases = [
        (
            "a1",
            "text_absent",
            json!({"kind":"text_absent","terms":["trial"],"synonyms":[],"surfaces":["code"],"allowlist":[]}),
        ),
        (
            "b3",
            "assertion_floor",
            json!({"kind":"assertion_floor","paths":["tests/**"],"minimum":1,"baseline_version":version}),
        ),
        (
            "b4",
            "ci_step_present",
            json!({"kind":"ci_step_present","workflow":".github/workflows/ci.yml","step":"test","threshold_pct":null}),
        ),
    ];
    for (id, kind, rule) in cases {
        ledger.conn().execute(
            "INSERT INTO decisions(id,project_id,created_at) VALUES(?1,'inline','2026-10-07T00:00:00Z')",
            [id],
        ).unwrap();
        ledger.conn().execute(
            "INSERT INTO decision_revisions(decision_id,rev,kind,params_schema_version,params_json,text,sources_json,author_actor,recorded_at)
             VALUES(?1,1,?2,1,?3,'fixture','[]','person','2026-10-07T00:00:00Z')",
            params![id, kind, rule.to_string()],
        ).unwrap();
    }
    let before: i64 = ledger
        .conn()
        .query_row("SELECT count(*) FROM decision_verdicts", [], |row| {
            row.get(0)
        })
        .unwrap();
    drop(ledger);
    let sweep = inline::evaluate_read_only(&db, repo.path(), "inline", &version, true)
        .expect("proposed diagnostic");
    assert_eq!(sweep.tree_hash.as_deref(), Some(tree.as_str()));
    assert!(sweep.diagnostic);
    assert_eq!(sweep.results.len(), 3);
    assert!(sweep
        .results
        .iter()
        .all(|row| row.advisory && row.verdict_id.is_none()));
    assert!(sweep
        .results
        .iter()
        .any(|row| row.kind == "text_absent" && row.state == "holds"));
    assert!(sweep
        .results
        .iter()
        .any(|row| row.kind == "assertion_floor"));
    let b4 = sweep
        .results
        .iter()
        .find(|row| row.kind == "ci_step_present")
        .unwrap();
    assert_eq!(b4.state, "unverifiable");
    assert!(b4.reason.as_deref().unwrap().contains("accepted baseline"));
    assert!(b4
        .full_command
        .as_deref()
        .unwrap()
        .contains("decisions check"));
    for named in ["HEAD", &version[..8]] {
        let normalized = inline::evaluate_read_only(&db, repo.path(), "inline", named, true)
            .expect("named version resolves inside read-only sweep");
        assert_eq!(normalized.version_id, version);
        assert_eq!(normalized.tree_hash.as_deref(), Some(tree.as_str()));
        assert!(normalized
            .results
            .iter()
            .any(|row| row.kind == "text_absent" && row.state == "holds"));
    }
    let ledger = Ledger::open(&db).unwrap();
    let after: i64 = ledger
        .conn()
        .query_row("SELECT count(*) FROM decision_verdicts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(after, before);
}

#[test]
fn full_check_command_keeps_a_leading_dash_project_id_as_one_option() {
    let version = "a".repeat(40);
    let command = inline::full_command_all("-member", Path::new("/tmp/repo"), &version);
    assert!(command.contains("--project='-member'"), "{command}");
    let one = inline::full_command("-member", Path::new("/tmp/repo"), &version, "-a1");
    assert!(one.contains("--project='-member'"), "{one}");
    assert!(one.contains("--id='-a1'"), "{one}");
}
