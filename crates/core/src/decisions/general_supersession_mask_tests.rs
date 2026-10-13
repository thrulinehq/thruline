use super::*;
use crate::decisions::params::{CheckParams, PathPresentParams};
use crate::decisions::storage::{self, DecisionSource, NewDecision, SourceKind};

fn review_fixture(words: &str) -> (Ledger, Value) {
    let ledger = Ledger::open_in_memory().expect("synthetic ledger");
    ledger.conn().execute("INSERT INTO projects(id,root_path,source,created_at) VALUES('review','/synthetic','codex','2026-10-08T00:00:00Z')", []).expect("synthetic project");
    let mut metadata = json!({"project_id":"review"});
    for (prefix, text) in [("predecessor", "Keep the old rule"), ("successor", words)] {
        let id = storage::create(
            &ledger,
            NewDecision {
                project_id: "review".into(),
                source_anchor: format!("synthetic-{prefix}"),
                span: text.into(),
                sources: vec![DecisionSource {
                    source_kind: SourceKind::RuleFile,
                    event_id: None,
                    file_anchor: Some(format!("synthetic-{prefix}")),
                    raw_file: "AGENTS.md".into(),
                    raw_line: 1,
                    span_start: 0,
                    span_end: text.len() as u64,
                    span: text.into(),
                    actor: None,
                    recorded_at: None,
                }],
                author_actor: "rule_file".into(),
                recorded_at: "2026-10-08T00:00:00Z".into(),
                params: CheckParams::PathPresent(PathPresentParams {
                    path: "policy/rules.yml".into(),
                    sha256: None,
                }),
            },
        )
        .expect("synthetic proposal");
        let bytes: String = ledger
            .conn()
            .query_row(
                "SELECT params_json FROM decision_revisions WHERE decision_id=?1 AND rev=1",
                [&id],
                |row| row.get(0),
            )
            .expect("stored parameters");
        metadata[format!("{prefix}_decision_id")] = json!(id);
        metadata[format!("{prefix}_revision")] = json!(1);
        metadata[format!("{prefix}_applies_from_version")] = json!("a".repeat(40));
        metadata[format!("{prefix}_params_sha256")] = json!(sha256_hex(bytes.as_bytes()));
    }
    (ledger, metadata)
}

#[test]
fn supersession_review_refuses_bare_credential_words_without_authority() {
    let (ledger, metadata) = review_fixture("Keep the new rule");
    assert!(
        review_details(&ledger, &metadata).is_ok(),
        "plain proposal review remains possible"
    );
    for prefix in [
        "sk-",
        "sk-ant-",
        "ghp_",
        "gho_",
        "ghu_",
        "ghs_",
        "ghr_",
        "github_pat_",
        "AKIA",
        "ASIA",
        "AIza",
        "eyJ",
        "xoxb-",
    ] {
        let (ledger, metadata) = review_fixture(&format!("Keep {prefix} in the new rule"));
        assert!(
            review_details(&ledger, &metadata).is_err(),
            "bare prefix {prefix} reached the review"
        );
        for table in [
            "decision_acceptances",
            "decision_person_acts",
            "decision_retirements",
        ] {
            assert_eq!(
                ledger
                    .conn()
                    .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| row
                        .get::<_, i64>(0))
                    .expect("authority count"),
                0
            );
        }
    }
}
