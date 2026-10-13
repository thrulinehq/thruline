//! C31 public A1 behavior: exact vendor key, dormant shipping condition, live proof.

#[path = "support/person_action.rs"]
mod person_action;

use person_action::PersonRecorder;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::Command;
use thruline_core::decisions::checks::{check_at, VerdictState};
use thruline_core::decisions::engine::check_project_version;
use thruline_core::decisions::inline;
use thruline_core::decisions::params::parse_params;
use thruline_core::decisions::storage::{
    self, AcceptanceInput, DecisionSource, NewDecision, SourceKind,
};
use thruline_core::ledger::Ledger;

const ARCHIVES: [&str; 4] = [
    "thruline-cli-aarch64-apple-darwin.tar.gz",
    "thruline-cli-x86_64-apple-darwin.tar.gz",
    "thruline-cli-aarch64-unknown-linux-musl.tar.gz",
    "thruline-cli-x86_64-unknown-linux-musl.tar.gz",
];

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn fixture(files: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-q"]);
    // Shipping proof binds the committed root and CLI package versions too.
    std::fs::write(
        repo.path().join("Cargo.toml"),
        "[workspace.package]\nversion='0.2.0'\n",
    )
    .unwrap();
    std::fs::create_dir_all(repo.path().join("crates/cli")).unwrap();
    std::fs::write(
        repo.path().join("crates/cli/Cargo.toml"),
        "[package]\nname='thruline-cli'\nversion.workspace=true\n",
    )
    .unwrap();
    for (name, text) in files {
        let path = repo.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    git(repo.path(), &["add", "-A"]);
    git(
        repo.path(),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "C31 fixture",
        ],
    );
    let version = git(repo.path(), &["rev-parse", "HEAD"]);
    (repo, version)
}

fn base() -> Value {
    let bundle: Value =
        serde_json::from_str(include_str!("fixtures/decisions/c31/selectors.json")).unwrap();
    bundle["base_params"].clone()
}

fn field() -> Value {
    let bundle: Value =
        serde_json::from_str(include_str!("fixtures/decisions/c31/selectors.json")).unwrap();
    bundle["paddle_scope"].clone()
}

fn condition() -> Value {
    json!({"pattern":"docs/**","release_version":"v0.2.0",
        "proof_path":"target/distrib/decisions-ship-proof.json"})
}

#[test]
fn empty_c31_fields_keep_historical_text_absent_params_bytes() {
    let old = r#"{"kind":"text_absent","terms":["trial"],"synonyms":[],"surfaces":["code"],"allowlist":[]}"#;
    let parsed = parse_params("text_absent", 1, old).unwrap();
    let normalized = serde_json::to_string(&parsed).unwrap();
    assert_eq!(
        normalized,
        r#"{"kind":"text_absent","terms":["trial"],"synonyms":[],"surfaces":["code"],"allowlist":[],"test_allowlist":[]}"#
    );
    assert!(!normalized.contains("json_field_allowlist"));
    assert!(!normalized.contains("conditional_path_allowlist"));
}

fn outcome(
    repo: &Path,
    version: &str,
    raw: Value,
) -> thruline_core::decisions::checks::CheckOutcome {
    let params = parse_params("text_absent", 1, &raw.to_string()).unwrap();
    check_at(repo, version, &params)
}

fn write_archives_and_proof(repo: &Path, version: &str) {
    let stage = repo.join("target/c31-staging/thruline-cli-v0.2.0");
    std::fs::create_dir_all(stage.join("bin")).unwrap();
    std::fs::write(stage.join("bin/thruline"), "synthetic binary").unwrap();
    let dist = repo.join("target/distrib");
    std::fs::create_dir_all(&dist).unwrap();
    let mut files = Vec::new();
    for name in ARCHIVES {
        let out = Command::new("tar")
            .arg("-czf")
            .arg(dist.join(name))
            .arg("-C")
            .arg(repo.join("target/c31-staging"))
            .arg("thruline-cli-v0.2.0")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let bytes = std::fs::read(dist.join(name)).unwrap();
        files.push(json!({"file_name":name,"sha256":Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect::<String>()}));
    }
    let proof = json!({"schema_version":1,"release_version":"v0.2.0",
        "checked_commit":version,"source":"private-release-dry-run","archives":files});
    std::fs::write(dist.join("decisions-ship-proof.json"), proof.to_string()).unwrap();
}

#[test]
fn paddle_scope_excludes_only_trial_in_exact_key() {
    let source = "{\n  \"data\": {\"items\": [{\"price\": {\n    \"trial_period\": \"trial in value\", \"trial_label\": \"trial\"\n  }}]}\n}\n";
    let (repo, version) = fixture(&[(
        "crates/licence-issue/fixtures/paddle-transaction_completed.json",
        source,
    )]);
    let mut raw = base();
    raw["json_field_allowlist"] = json!([field()]);
    let result = outcome(repo.path(), &version, raw);
    assert_eq!(result.state, VerdictState::Broken);
    assert_eq!(
        result.findings.len(),
        3,
        "value and sibling-key words stay visible"
    );
    assert!(result.findings.iter().all(|f| f.line == 3));
    assert!(result
        .not_looked_at
        .iter()
        .any(|r| r.source == "accepted_json_key_scope"
            && r.path == "crates/licence-issue/fixtures/paddle-transaction_completed.json"
            && r.start_line == Some(3)));
}

#[test]
fn missing_archive_proof_retains_broken_witness_and_names_missing_fact() {
    let (repo, version) = fixture(&[
        ("docs/history.md", "trial in docs\n"),
        ("src/gate.rs", "fn gate() { let _ = \"trial\"; }\n"),
    ]);
    let mut raw = base();
    raw["conditional_path_allowlist"] = json!([condition()]);
    let result = outcome(repo.path(), &version, raw);
    assert_eq!(result.state, VerdictState::Broken);
    assert_eq!(result.findings.len(), 2);
    assert!(result.findings.iter().any(|f| f.path == "docs/history.md"));
    assert!(result.reason.unwrap_or_default().contains("v0.2.0"));
}

#[test]
fn missing_archive_proof_without_other_findings_is_unverifiable() {
    let (repo, version) = fixture(&[("docs/history.md", "history\n")]);
    let mut raw = base();
    raw["conditional_path_allowlist"] = json!([condition()]);
    let result = outcome(repo.path(), &version, raw);
    assert_eq!(result.state, VerdictState::Unverifiable);
    assert!(result.reason.unwrap_or_default().contains("v0.2.0"));
}

#[test]
fn valid_archive_proof_excludes_only_the_named_docs_pattern() {
    let (repo, version) = fixture(&[
        ("docs/history.md", "trial in docs\n"),
        ("src/gate.rs", "fn gate() { let _ = \"trial\"; }\n"),
    ]);
    write_archives_and_proof(repo.path(), &version);
    let mut raw = base();
    raw["conditional_path_allowlist"] = json!([condition()]);
    let result = outcome(repo.path(), &version, raw);
    assert_eq!(result.state, VerdictState::Broken);
    assert_eq!(result.findings.len(), 1);
    assert_eq!(result.findings[0].path, "src/gate.rs");
    assert!(result
        .not_looked_at
        .iter()
        .any(|r| r.path == "docs/history.md"));
}

#[test]
fn proof_change_on_same_git_tree_bypasses_engine_and_inline_verdict_cache() {
    let (repo, version) = fixture(&[("docs/history.md", "trial in docs\n")]);
    let db_dir = tempfile::tempdir().unwrap();
    let db = db_dir.path().join("ledger.db");
    let ledger = Ledger::open(&db).unwrap();
    ledger.conn().execute(
        "INSERT INTO projects(id,root_path,source,created_at) VALUES('c31',?1,'codex','2026-10-07T00:00:00Z')",
        [repo.path().to_string_lossy().as_ref()],
    ).unwrap();
    let mut raw = base();
    raw["conditional_path_allowlist"] = json!([condition()]);
    let params = parse_params("text_absent", 1, &raw.to_string()).unwrap();
    let span = "Trial absent from shipped surfaces";
    let id = storage::create(
        &ledger,
        NewDecision {
            project_id: "c31".into(),
            source_anchor: "event-1".into(),
            span: span.into(),
            sources: vec![DecisionSource {
                source_kind: SourceKind::WorkPrompt,
                event_id: Some("event-1".into()),
                file_anchor: None,
                raw_file: "session.jsonl".into(),
                raw_line: 1,
                span_start: 0,
                span_end: span.len() as u64,
                span: span.into(),
                actor: Some("founder".into()),
                recorded_at: Some("2026-10-07T00:00:00Z".into()),
            }],
            author_actor: "fixture".into(),
            recorded_at: "2026-10-07T00:00:00Z".into(),
            params,
        },
    )
    .unwrap();
    let act = storage::accept(
        &ledger,
        AcceptanceInput {
            decision_id: id.clone(),
            revision: 1,
            actor: "founder".into(),
            at: "2026-10-07T00:01:00Z".into(),
            because: "Synthetic fixture only".into(),
            severity: "warns".into(),
            applies_from_version: version.clone(),
        },
    )
    .unwrap();
    let mut person = PersonRecorder::new(&ledger, "c31");
    person.bind_bridge_acceptance(&ledger, &act);

    let before = check_project_version(
        &ledger,
        "c31",
        repo.path(),
        &version,
        "2026-10-07T00:02:00Z",
        false,
    )
    .unwrap();
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].state, VerdictState::Broken);
    assert!(!before[0].from_cache);
    write_archives_and_proof(repo.path(), &version);
    let cached = inline::cached_read_only(&db, repo.path(), "c31", &version, false).unwrap();
    assert_eq!(cached.results.len(), 1);
    assert!(
        !cached.results[0].from_cache,
        "old Broken proof basis must not be reused"
    );
    let fresh = inline::evaluate_read_only(&db, repo.path(), "c31", &version, false).unwrap();
    assert_eq!(fresh.results.len(), 1);
    assert_eq!(fresh.results[0].state, "holds");
    assert!(!fresh.results[0].from_cache);
    let after = check_project_version(
        &ledger,
        "c31",
        repo.path(),
        &version,
        "2026-10-07T00:03:00Z",
        false,
    )
    .unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].state, VerdictState::Holds);
    assert!(!after[0].from_cache);
    assert_ne!(after[0].verdict_id, before[0].verdict_id);
}
