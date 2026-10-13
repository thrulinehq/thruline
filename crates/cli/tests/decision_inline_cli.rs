//! The inline CLI supervises a separate read-only worker and stores no verdict.

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use rusqlite::params;
use serde_json::{json, Value};
use thruline_core::decisions::inline;
use thruline_core::ledger::Ledger;

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().into()
}

#[test]
fn inline_diagnostic_is_bounded_and_does_not_append_a_verdict() {
    let repo = tempfile::tempdir().unwrap();
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
    let store = tempfile::tempdir().unwrap();
    let db = store.path().join("ledger.db");
    let ledger = Ledger::open(&db).unwrap();
    ledger.conn().execute(
        "INSERT INTO projects(id,root_path,source,created_at) VALUES('inline',?1,'codex','2026-10-07T00:00:00Z')",
        [repo.path().to_string_lossy().as_ref()],
    ).unwrap();
    ledger.conn().execute("INSERT INTO decisions(id,project_id,created_at) VALUES('a1','inline','2026-10-07T00:00:00Z')", []).unwrap();
    ledger.conn().execute(
        "INSERT INTO decision_revisions(decision_id,rev,kind,params_schema_version,params_json,text,sources_json,author_actor,recorded_at)
         VALUES('a1',1,'text_absent',1,?1,'fixture','[]','person','2026-10-07T00:00:00Z')",
        params![json!({"kind":"text_absent","terms":["trial"],"synonyms":[],"surfaces":["code"],"allowlist":[]}).to_string()],
    ).unwrap();
    drop(ledger);
    let start = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .arg("--db")
        .arg(&db)
        .args(["decisions", "--project", "inline", "--root"])
        .arg(repo.path())
        .args([
            "--json",
            "inline",
            "--version",
            "HEAD",
            "--diagnose-proposals",
        ])
        .env("HOME", store.path())
        .env("THRULINE_HOME", store.path().join("state"))
        .output()
        .unwrap();
    assert!(start.elapsed() < Duration::from_secs(20));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["shape"], "thruline.decide.inline");
    assert_eq!(body["schema_version"], 1);
    assert_eq!(
        body.as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "budget_ms",
            "diagnostic",
            "elapsed_ms",
            "project_id",
            "results",
            "schema_version",
            "shape",
            "tree_hash",
            "version_id"
        ]
    );
    assert_eq!(body["diagnostic"], true);
    assert_eq!(body["version_id"], version);
    assert_eq!(body["results"][0]["kind"], "text_absent");
    assert_eq!(body["results"][0]["state"], "holds");
    assert_eq!(body["results"][0]["advisory"], true);
    let ledger = Ledger::open(&db).unwrap();
    let rows: i64 = ledger
        .conn()
        .query_row("SELECT count(*) FROM decision_verdicts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(rows, 0);
}

#[test]
fn one_supervised_sweep_covers_two_ledger_members() {
    let repo = tempfile::tempdir().unwrap();
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
    let store = tempfile::tempdir().unwrap();
    let db = store.path().join("ledger.db");
    let ledger = Ledger::open(&db).unwrap();
    for id in ["-codex", "-claude"] {
        ledger.conn().execute("INSERT INTO projects(id,root_path,source,created_at) VALUES(?1,?2,'codex','2026-10-07T00:00:00Z')", params![id,repo.path().to_string_lossy()]).unwrap();
        let decision = format!("{id}-a1");
        ledger.conn().execute("INSERT INTO decisions(id,project_id,created_at) VALUES(?1,?2,'2026-10-07T00:00:00Z')", params![decision,id]).unwrap();
        ledger.conn().execute("INSERT INTO decision_revisions(decision_id,rev,kind,params_schema_version,params_json,text,sources_json,author_actor,recorded_at) VALUES(?1,1,'text_absent',1,?2,'fixture','[]','person','2026-10-07T00:00:00Z')", params![decision,json!({"kind":"text_absent","terms":["trial"],"synonyms":[],"surfaces":["code"],"allowlist":[]}).to_string()]).unwrap();
    }
    drop(ledger);
    let output = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .arg("--db")
        .arg(&db)
        .args(["decisions", "--project=-codex", "--root"])
        .arg(repo.path())
        .args([
            "--json",
            "inline",
            "--version",
            &version[..8],
            "--diagnose-proposals",
            "--project-member=-claude",
        ])
        .env("HOME", store.path())
        .env("THRULINE_HOME", store.path().join("state"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let body: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(body["shape"], "thruline.decide.inline");
    assert_eq!(body["schema_version"], 1);
    assert_eq!(
        body.as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "budget_ms",
            "diagnostic",
            "elapsed_ms",
            "projects",
            "schema_version",
            "shape",
            "state",
            "version_id"
        ]
    );
    assert_eq!(body["projects"].as_array().unwrap().len(), 2);
    for member in body["projects"].as_array().unwrap() {
        assert_eq!(member["schema_version"], 1);
        assert!(member.get("shape").is_none());
    }
    assert_eq!(body["version_id"], version);
    assert_eq!(body["projects"][0]["project_id"], "-codex");
    assert_eq!(body["projects"][1]["project_id"], "-claude");
    assert_eq!(body["projects"][0]["results"][0]["state"], "holds");
    assert_eq!(body["projects"][1]["results"][0]["state"], "holds");
    let ledger = Ledger::open(&db).unwrap();
    assert_eq!(
        ledger
            .conn()
            .query_row("SELECT count(*) FROM decision_verdicts", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn generated_full_command_with_leading_dash_ids_parses_as_help_only() {
    let binary = env!("CARGO_BIN_EXE_thruline");
    let binary = format!("'{}'", binary.replace('\'', "'\\''"));
    let version = "a".repeat(40);
    for generated in [
        inline::full_command("-member", Path::new("/tmp/repo"), &version, "-a1"),
        inline::full_command_all("-member", Path::new("/tmp/repo"), &version),
    ] {
        let script = format!("{} --help", generated.replacen("thruline", &binary, 1));
        let output = Command::new("/bin/sh")
            .arg("-c")
            .arg(script)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("Usage:"));
    }
}
