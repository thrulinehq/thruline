//! Terminal review preserves exact source-backed selector identities while
//! retaining the mask gate for every unrelated value.

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use thruline_core::chain::sha256_hex;
use thruline_core::decisions::authority::bridge;
use thruline_core::decisions::authority::PersonActKind;
use thruline_core::decisions::params::{CheckParams, Surface, TextAbsentParams};
use thruline_core::decisions::storage::{
    self, AcceptanceInput, DecisionSource, NewDecision, RetirementInput, SourceKind, WaiverInput,
};
use thruline_core::decisions::text_exclusions::TestScope;
use thruline_core::Ledger;

const NAME: &str = "r31_selected_guard_survives_literal_name_review";
const OPAQUE: &str = "f3Kq9ZbW2xLpR7vN4tJc8mHd5sYgA1eU6oI0";
const CANARY: &str = "ghp_TLCANARYaaZZ11bbYY22ccXX33ddWW44e";

fn git(root: &Path, args: &[&str]) -> String {
    let result = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_AUTHOR_DATE", "2026-10-07T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2026-10-07T00:00:00Z")
        .output()
        .expect("fixture Git");
    assert!(result.status.success(), "fixture Git command failed");
    String::from_utf8(result.stdout)
        .expect("fixture Git UTF-8")
        .trim()
        .into()
}

fn test_source(name: &str) -> String {
    // These committed declarations are parsed for identity, never executed by
    // the bridge. Keep their assertions substantive even in that fixture role.
    format!("#[test]\nfn {name}() {{ assert!(std::path::Path::new(\"tests/guards.rs\").is_file()); }}\n")
}

struct Case {
    repo: TempDir,
    ledger: Ledger,
    decision: String,
    act: String,
    oid: String,
}

impl Case {
    fn new(source: &str, name: &str, terms: Vec<String>, because: &str, span: &str) -> Self {
        Self::with_basis(source, name, terms, because, span, None)
    }

    fn with_basis(
        source: &str,
        name: &str,
        terms: Vec<String>,
        because: &str,
        span: &str,
        basis: Option<&str>,
    ) -> Self {
        let repo = TempDir::new().expect("own fixture directory");
        fs::create_dir(repo.path().join("tests")).expect("fixture tests directory");
        fs::write(repo.path().join("tests/guards.rs"), source).expect("fixture source");
        git(repo.path(), &["init", "-q"]);
        git(repo.path(), &["config", "user.name", "Recorded Person"]);
        git(
            repo.path(),
            &["config", "user.email", "person@example.invalid"],
        );
        git(repo.path(), &["add", "tests/guards.rs"]);
        git(repo.path(), &["commit", "-qm", "Guard fixture"]);
        let oid = git(repo.path(), &["rev-parse", "HEAD"]);
        let ledger = Ledger::open_in_memory().expect("scratch ledger");
        ledger
            .conn()
            .execute(
                "INSERT INTO projects(id,root_path,source,created_at) VALUES('review',?1,'codex','2026-10-07T00:00:00Z')",
                [repo.path().to_str().expect("fixture path")],
            )
            .expect("fixture project");
        let decision = storage::create(
            &ledger,
            NewDecision {
                project_id: "review".into(),
                source_anchor: "review-source".into(),
                span: span.into(),
                sources: vec![DecisionSource {
                    source_kind: SourceKind::RuleFile,
                    event_id: None,
                    file_anchor: Some("review-source".into()),
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
                params: CheckParams::TextAbsent(TextAbsentParams {
                    terms,
                    synonyms: vec![],
                    surfaces: vec![Surface::Code],
                    allowlist: vec![],
                    test_allowlist: vec![TestScope {
                        path: "tests/guards.rs".into(),
                        name_pattern: name.into(),
                    }],
                    json_field_allowlist: vec![],
                    conditional_path_allowlist: vec![],
                    span_allowlist: vec![],
                }),
            },
        )
        .expect("proposed selector decision");
        let act = storage::accept(
            &ledger,
            AcceptanceInput {
                decision_id: decision.clone(),
                revision: 1,
                actor: "person".into(),
                at: "2026-10-07T00:01:00Z".into(),
                because: because.into(),
                severity: "blocks".into(),
                applies_from_version: basis.unwrap_or(&oid).into(),
            },
        )
        .expect("inert acceptance");
        Self {
            repo,
            ledger,
            decision,
            act,
            oid,
        }
    }

    fn ordinary(source: &str, name: &str) -> Self {
        Self::new(
            source,
            name,
            vec!["denied".into()],
            "Review selected guard",
            "Keep the selected guard",
        )
    }

    fn counts(&self) -> Vec<i64> {
        [
            "sessions",
            "events",
            "decision_person_acts",
            "decision_acceptances",
            "decision_waivers",
            "decision_retirements",
        ]
        .iter()
        .map(|table| {
            self.ledger
                .conn()
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .expect("scratch counts")
        })
        .collect()
    }

    fn review(
        &self,
        action: PersonActKind,
        act: &str,
    ) -> thruline_core::error::Result<serde_json::Value> {
        let before = self.counts();
        let result = bridge::review_terminal_act(&self.ledger, action, act, None);
        assert_eq!(
            self.counts(),
            before,
            "review creates no person evidence or rows"
        );
        result
    }

    fn refused(&self) {
        let error = self
            .review(PersonActKind::Accept, &self.act)
            .expect_err("review stays closed");
        assert!(error.to_string().contains("masked values"));
    }
}

#[test]
fn registered_exact_accept_review_preserves_selector_and_original_digest() {
    let case = Case::ordinary(&test_source(NAME), NAME);
    let review = case
        .review(PersonActKind::Accept, &case.act)
        .expect("source-backed selector review");
    let stored: String = case
        .ledger
        .conn()
        .query_row(
            "SELECT params_json FROM decision_revisions WHERE decision_id=?1 AND rev=1",
            [&case.decision],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        review["decision"]["params"]["test_allowlist"][0]["name_pattern"],
        NAME
    );
    assert_eq!(
        review["decision"]["params"],
        serde_json::from_str::<serde_json::Value>(&stored).unwrap()
    );
    assert_eq!(
        review["decision"]["params_sha256"],
        sha256_hex(stored.as_bytes())
    );
    assert_eq!(
        review["confirmation"]["thruline_decision_act"]["params_sha256"],
        review["decision"]["params_sha256"]
    );
}

#[test]
fn registered_exact_waiver_review_uses_acceptance_source_version() {
    let case = Case::ordinary(&test_source(NAME), NAME);
    let act = storage::waive(
        &case.ledger,
        WaiverInput {
            decision_id: case.decision.clone(),
            version_id: "b".repeat(40),
            actor: "person".into(),
            at: "2026-10-07T00:02:00Z".into(),
            because: "Review one version".into(),
            expires_version: None,
            expires_at: None,
        },
    )
    .unwrap();
    let review = case
        .review(PersonActKind::Waive, &act)
        .expect("source-backed waiver review");
    assert_eq!(
        review["decision"]["params"]["test_allowlist"][0]["name_pattern"],
        NAME
    );
    assert_ne!(
        review["confirmation"]["thruline_decision_act"]["version_id"],
        case.oid
    );
}

#[test]
fn registered_exact_retire_review_uses_acceptance_source_version() {
    let case = Case::ordinary(&test_source(NAME), NAME);
    let act = storage::retire(
        &case.ledger,
        RetirementInput {
            decision_id: case.decision.clone(),
            actor: "person".into(),
            at: "2026-10-07T00:02:00Z".into(),
            because: "Review retiring this guard".into(),
            superseded_by: None,
        },
    )
    .unwrap();
    let review = case
        .review(PersonActKind::Retire, &act)
        .expect("source-backed retirement review");
    assert_eq!(
        review["decision"]["params"]["test_allowlist"][0]["name_pattern"],
        NAME
    );
}

#[test]
fn unregistered_selector_is_refused() {
    Case::ordinary(&test_source("ordinary_guard"), NAME).refused();
}

#[test]
fn production_function_is_refused() {
    Case::ordinary(&format!("fn {NAME}() {{}}\n"), NAME).refused();
}

#[test]
fn duplicate_test_identity_is_refused() {
    let source = format!(
        "mod first {{ {} }}\nmod second {{ {} }}\n",
        test_source(NAME),
        test_source(NAME)
    );
    Case::ordinary(&source, NAME).refused();
}

#[test]
fn absent_commit_is_refused() {
    Case::with_basis(
        &test_source(NAME),
        NAME,
        vec!["denied".into()],
        "Review selected guard",
        "Keep selected guard",
        Some(&"b".repeat(40)),
    )
    .refused();
}

#[test]
fn branch_selector_basis_is_refused() {
    Case::with_basis(
        &test_source(NAME),
        NAME,
        vec!["denied".into()],
        "Review selected guard",
        "Keep selected guard",
        Some("HEAD"),
    )
    .refused();
}

#[test]
fn malformed_source_is_refused() {
    Case::ordinary(&format!("#[test]\nfn {NAME}( {{"), NAME).refused();
}

#[test]
fn missing_repository_is_refused() {
    let case = Case::ordinary(&test_source(NAME), NAME);
    case.ledger
        .conn()
        .execute(
            "UPDATE projects SET root_path=?1 WHERE id='review'",
            [case.repo.path().join("absent").to_str().unwrap()],
        )
        .unwrap();
    case.refused();
}

#[test]
fn working_tree_function_cannot_replace_exact_commit_source() {
    let case = Case::ordinary(&test_source("ordinary_guard"), NAME);
    fs::write(case.repo.path().join("tests/guards.rs"), test_source(NAME)).unwrap();
    case.refused();
}

#[test]
fn known_credential_shape_is_refused_even_as_a_registered_test() {
    Case::ordinary(&test_source(CANARY), CANARY).refused();
}

#[test]
fn unrelated_parameter_entropy_is_refused() {
    Case::new(
        &test_source(NAME),
        NAME,
        vec![OPAQUE.into()],
        "Review selected guard",
        "Keep selected guard",
    )
    .refused();
}

#[test]
fn unrelated_confirmation_reason_entropy_is_refused() {
    Case::new(
        &test_source(NAME),
        NAME,
        vec!["denied".into()],
        OPAQUE,
        "Keep selected guard",
    )
    .refused();
}

#[test]
fn wildcard_cannot_exempt_unrelated_parameter_entropy() {
    Case::new(
        &test_source(NAME),
        &format!("{NAME}*"),
        vec![OPAQUE.into()],
        "Review selected guard",
        "Keep selected guard",
    )
    .refused();
}

#[test]
fn recorded_source_credential_mask_remains_intact() {
    let case = Case::new(
        &test_source(NAME),
        NAME,
        vec!["denied".into()],
        "Review selected guard",
        &format!("Keep selected guard {CANARY}"),
    );
    let review = case
        .review(PersonActKind::Accept, &case.act)
        .expect("already masked source review");
    let text = review["decision"]["text"].as_str().unwrap();
    assert!(!text.contains(CANARY));
    assert!(text.contains("[masked: github]"));
}

#[test]
fn recorded_source_value_entropy_mask_remains_intact() {
    let case = Case::new(
        &test_source(NAME),
        NAME,
        vec!["denied".into()],
        "Review selected guard",
        &format!("Keep selected guard \"{OPAQUE}\""),
    );
    let review = case
        .review(PersonActKind::Accept, &case.act)
        .expect("already masked source review");
    let text = review["decision"]["text"].as_str().unwrap();
    assert!(!text.contains(OPAQUE));
    assert!(text.contains("[masked: high_entropy]"));
}
