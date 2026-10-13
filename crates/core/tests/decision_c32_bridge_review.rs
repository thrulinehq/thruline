//! Exact support names keep the same source-backed terminal review boundary.
use serde_json::{json, Value};
use std::{fs, path::Path, process::Command};
use thruline_core::decisions::{
    authority::{bridge, PersonActKind},
    params::parse_params,
    storage::{self, AcceptanceInput, DecisionSource, NewDecision, SourceKind},
};
use thruline_core::{chain::sha256_hex, Ledger};
const NAME: &str = "r31_selected_guard_survives_literal_name_review";
const OPAQUE: &str = "f3Kq9ZbW2xLpR7vN4tJc8mHd5sYgA1eU6oI0";
fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap().trim().into()
}
struct Case {
    repo: tempfile::TempDir,
    ledger: Ledger,
    act: String,
    params: Value,
    params_sha: String,
}
impl Case {
    fn new(source: &str, name: &str, term: &str, wrong_basis: bool) -> Self {
        let repo = tempfile::tempdir().unwrap();
        fs::create_dir(repo.path().join("tests")).unwrap();
        fs::write(repo.path().join("tests/guards.rs"), source).unwrap();
        git(repo.path(), &["init", "-q"]);
        git(repo.path(), &["config", "user.name", "Fixture"]);
        git(
            repo.path(),
            &["config", "user.email", "fixture@example.invalid"],
        );
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-qm", "Guard support fixture"]);
        let oid = git(repo.path(), &["rev-parse", "HEAD"]);
        let selector = json!({"path":"tests/guards.rs","name_pattern":name});
        let raw = json!({"kind":"text_absent","terms":[term],"synonyms":[],"surfaces":["tests"],
    "allowlist":[],"test_allowlist":[selector.clone()], "span_allowlist":[{
    "path":"tests/guards.rs","source_blob_sha256":sha256_hex(source.as_bytes()),
    "start_line":3,"end_line":3,"start_column":0,"end_column":source.lines().nth(2).unwrap().len(),
    "purpose":"test_support","supports":[selector]}]});
        let params = parse_params("text_absent", 1, &raw.to_string()).unwrap();
        let params_sha = sha256_hex(params.json().unwrap().as_bytes());
        let normalized: Value = serde_json::from_str(&params.json().unwrap()).unwrap();
        let ledger = Ledger::open_in_memory().unwrap();
        bridge::install_schema_for_write(&ledger).unwrap();
        ledger.conn().execute("INSERT INTO projects(id,root_path,source,created_at) VALUES('c32-review',?1,'codex','2026-10-07T00:00:00Z')",[repo.path().to_str().unwrap()]).unwrap();
        let span = "Keep the selected absence guard";
        let id = storage::create(
            &ledger,
            NewDecision {
                project_id: "c32-review".into(),
                source_anchor: "guard-review".into(),
                span: span.into(),
                sources: vec![DecisionSource {
                    source_kind: SourceKind::RuleFile,
                    event_id: None,
                    file_anchor: Some("guard-review".into()),
                    raw_file: "AGENTS.md".into(),
                    raw_line: 1,
                    span_start: 0,
                    span_end: span.len() as u64,
                    span: span.into(),
                    actor: Some("person".into()),
                    recorded_at: Some("2026-10-07T00:00:00Z".into()),
                }],
                author_actor: "person".into(),
                recorded_at: "2026-10-07T00:00:00Z".into(),
                params,
            },
        )
        .unwrap();
        let act = storage::accept(
            &ledger,
            AcceptanceInput {
                decision_id: id,
                revision: 1,
                actor: "person".into(),
                at: "2026-10-07T00:01:00Z".into(),
                because: "Review the exact support scope".into(),
                severity: "blocks".into(),
                applies_from_version: if wrong_basis { "0".repeat(40) } else { oid },
            },
        )
        .unwrap();
        Self {
            repo,
            ledger,
            act,
            params: normalized,
            params_sha,
        }
    }
    fn review(&self) -> thruline_core::error::Result<Value> {
        let before = self.counts();
        let result =
            bridge::review_terminal_act(&self.ledger, PersonActKind::Accept, &self.act, None);
        assert_eq!(self.counts(), before);
        result
    }
    fn counts(&self) -> Vec<i64> {
        [
            "decisions",
            "decision_revisions",
            "decision_acceptances",
            "decision_person_acts",
            "decision_bridge_proofs",
            "events",
            "sessions",
        ]
        .iter()
        .map(|t| {
            self.ledger
                .conn()
                .query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))
                .unwrap()
        })
        .collect()
    }
    fn valid() -> Self {
        Self::new(&format!("#[test]\nfn {NAME}() {{ assert_eq!(2 + 2, 4); }}\nfn support() {{ let _ = \"denied\"; }}\n"),NAME,"denied",false)
    }
}
#[test]
fn a_nested_exact_support_identity_is_reviewed_without_changing_bytes() {
    assert_ne!(
        thruline_core::redact::Masked::mask(&serde_json::to_string(NAME).unwrap()).text(),
        serde_json::to_string(NAME).unwrap()
    );
    let case = Case::valid();
    let review = case.review().unwrap();
    assert_eq!(review["decision"]["params"], case.params);
    assert_eq!(
        review["confirmation"]["thruline_decision_act"]["params_sha256"],
        case.params_sha
    );
    assert_eq!(
        review["decision"]["params"]["span_allowlist"][0]["supports"][0]["name_pattern"],
        NAME
    );
}
#[test]
fn dirty_worktree_does_not_replace_the_accepted_support_identity() {
    let case = Case::valid();
    fs::write(
        case.repo.path().join("tests/guards.rs"),
        "dirty replacement",
    )
    .unwrap();
    assert_eq!(case.review().unwrap()["decision"]["params"], case.params);
}
#[test]
fn unrelated_opaque_term_keeps_the_full_mask_gate() {
    let case=Case::new(&format!("#[test]\nfn {NAME}() {{ assert_eq!(2 + 2, 4); }}\nfn support() {{ let _ = \"denied\"; }}\n"),NAME,OPAQUE,false);
    assert!(case.review().is_err());
}
#[test]
fn absent_commit_cannot_unmask_nested_support_names() {
    let case=Case::new(&format!("#[test]\nfn {NAME}() {{ assert_eq!(2 + 2, 4); }}\nfn support() {{ let _ = \"denied\"; }}\n"),NAME,"denied",true);
    assert!(case.review().is_err());
}
#[test]
fn duplicate_declarations_cannot_unmask_nested_support_names() {
    let source=format!("#[test]\nfn {NAME}() {{ assert_eq!(2 + 2, 4); }}\n#[test]\nfn {NAME}() {{ assert_eq!(3 + 3, 6); }}\n");
    let case = Case::new(&source, NAME, "denied", false);
    assert!(case.review().is_err());
}
