//! Public Decisions JSON uses only synthetic, private fixtures here.
//! Confirmation is covered by pure output construction, never invoked.

use std::path::Path;
use std::process::Command;

use serde_json::Value;
use thruline_core::decisions::params::{CheckParams, PathPresentParams};
use thruline_core::decisions::storage::{self, DecisionSource, NewDecision, SourceKind};
use thruline_core::Ledger;

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn fixture() -> (
    tempfile::TempDir,
    Ledger,
    std::path::PathBuf,
    String,
    String,
) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("kept.txt"), "fixture\n").unwrap();
    std::fs::write(root.join("AGENTS.md"), "Keep kept.txt.\n").unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "AGENTS.md", "kept.txt"]);
    git(
        &root,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
    );
    let version = git(&root, &["rev-parse", "HEAD"]);
    let ledger = Ledger::open(temp.path().join("ledger.db")).unwrap();
    ledger.conn().execute(
        "INSERT INTO projects(id,root_path,source,created_at) VALUES('json-member',?1,'codex','2026-10-08T00:00:00Z')",
        [root.to_str().unwrap()],
    ).unwrap();
    let span = "Keep kept.txt".to_owned();
    let decision = storage::create(
        &ledger,
        NewDecision {
            project_id: "json-member".into(),
            source_anchor: "json-fixture".into(),
            span: span.clone(),
            sources: vec![DecisionSource {
                source_kind: SourceKind::RuleFile,
                event_id: None,
                file_anchor: Some("json-fixture".into()),
                raw_file: "AGENTS.md".into(),
                raw_line: 1,
                span_start: 0,
                span_end: span.len() as u64,
                span,
                actor: Some("fixture".into()),
                recorded_at: Some("2026-10-08T00:00:00Z".into()),
            }],
            author_actor: "fixture".into(),
            recorded_at: "2026-10-08T00:00:00Z".into(),
            params: CheckParams::PathPresent(PathPresentParams {
                path: "kept.txt".into(),
                sha256: None,
            }),
        },
    )
    .unwrap();
    (temp, ledger, root, version, decision)
}

fn cli(temp: &tempfile::TempDir, root: &Path, args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .arg("--db")
        .arg(temp.path().join("ledger.db"))
        .args(["decide", "--project=json-member", "--root"])
        .arg(root)
        .arg("--json")
        .args(args)
        .env("HOME", temp.path())
        .env("THRULINE_HOME", temp.path().join("state"))
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn fields(value: &Value) -> Vec<&str> {
    value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect()
}

fn no_person_proof_or_verdict(ledger: &Ledger) {
    for table in ["decision_person_acts", "decision_verdicts"] {
        let count: i64 = ledger
            .conn()
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "{table}");
    }
}

#[test]
fn preparing_an_inert_act_emits_the_registered_closed_document() {
    let (temp, ledger, root, version, decision) = fixture();
    let body = cli(
        &temp,
        &root,
        &[
            "prepare",
            "accept",
            &decision,
            "--actor",
            "fixture",
            "--because",
            "fixture choice",
            "--from-version",
            &version,
        ],
    );
    assert_eq!(body["shape"], "thruline.decide.prepare");
    assert_eq!(body["schema_version"], 1);
    assert_eq!(body["project_id"], "json-member");
    assert_eq!(body["decision_id"], decision);
    assert_eq!(body["action"], "accept");
    assert_eq!(body["state"], "pending_bridge");
    assert!(body["act_row_id"].as_str().is_some_and(|id| !id.is_empty()));
    assert_eq!(
        fields(&body),
        [
            "act_row_id",
            "action",
            "decision_id",
            "project_id",
            "schema_version",
            "shape",
            "state"
        ]
    );
    assert!(!root.join(".thruline/decisions.yml").exists());
    no_person_proof_or_verdict(&ledger);
}

#[test]
fn repairing_an_inert_derived_copy_emits_the_registered_closed_document() {
    let (temp, ledger, root, _, _) = fixture();
    let body = cli(&temp, &root, &["repair-derived"]);
    assert_eq!(body["shape"], "thruline.decide.repair-derived");
    assert_eq!(body["schema_version"], 1);
    assert_eq!(body["project_id"], "json-member");
    assert_eq!(body["verified"], true);
    assert_eq!(
        body["derived_file"],
        root.join(".thruline/decisions.yml").to_str().unwrap()
    );
    assert_eq!(
        fields(&body),
        [
            "derived_file",
            "project_id",
            "schema_version",
            "shape",
            "verified"
        ]
    );
    no_person_proof_or_verdict(&ledger);
}

#[test]
fn rule_import_emits_the_registered_document_without_person_authority() {
    let (temp, ledger, root, version, _) = fixture();
    let body = cli(&temp, &root, &["rules", "--version", &version]);
    assert_eq!(body["shape"], "thruline.decide.rules");
    assert_eq!(body["schema_version"], 1);
    assert_eq!(body["project_id"], "json-member");
    assert_eq!(body["version_id"], version);
    assert!(body["proposals"].is_array());
    assert!(body["notes"].is_array());
    assert_eq!(
        fields(&body),
        [
            "notes",
            "project_id",
            "proposals",
            "schema_version",
            "shape",
            "tree_hash",
            "version_id"
        ]
    );
    no_person_proof_or_verdict(&ledger);
}
