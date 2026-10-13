#[path = "support/person_action.rs"]
mod person_action;

use person_action::PersonRecorder;
use std::path::Path;
use std::process::Command;

use thruline_core::decisions::checks::VerdictState;
use thruline_core::decisions::engine::check_project_version;
use thruline_core::decisions::params::{CheckParams, Surface, TextAbsentParams};
use thruline_core::decisions::storage::{
    self, AcceptanceInput, DecisionSource, NewDecision, SourceKind,
};
use thruline_core::ledger::Ledger;

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git runs");
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8(output.stdout)
        .expect("git text")
        .trim()
        .to_owned()
}

fn commit(repo: &Path, label: &str) -> String {
    git(repo, &["add", "-A"]);
    git(
        repo,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            label,
        ],
    );
    git(repo, &["rev-parse", "HEAD"])
}

#[test]
fn only_person_accepted_revisions_are_checked_cached_and_replayed() {
    let repo = tempfile::tempdir().expect("repo");
    git(repo.path(), &["init", "-q"]);
    std::fs::create_dir(repo.path().join("src")).expect("src");
    std::fs::write(
        repo.path().join("src/lib.rs"),
        "const OBSOLETE: bool = true;\n",
    )
    .expect("old source");
    let old = commit(repo.path(), "old");

    let ledger = Ledger::open(repo.path().join(".git/decision-ledger.db")).expect("ledger");
    ledger.conn().execute(
        "INSERT INTO projects(id,root_path,source,created_at) VALUES(?1,?2,'codex','2026-10-07T00:00:00Z')",
        rusqlite::params!["project-a", repo.path().display().to_string()],
    ).expect("project");
    let span = "No obsolete label anywhere";
    let decision_id = storage::create(
        &ledger,
        NewDecision {
            project_id: "project-a".into(),
            source_anchor: "event-1".into(),
            span: span.into(),
            sources: vec![DecisionSource {
                source_kind: SourceKind::WorkPrompt,
                event_id: Some("event-1".into()),
                file_anchor: None,
                raw_file: "session.jsonl".into(),
                raw_line: 5,
                span_start: 0,
                span_end: span.len() as u64,
                span: span.into(),
                actor: Some("founder".into()),
                recorded_at: Some("2026-10-07T00:00:00Z".into()),
            }],
            author_actor: "founder".into(),
            recorded_at: "2026-10-07T00:00:00Z".into(),
            params: CheckParams::TextAbsent(TextAbsentParams {
                test_allowlist: Vec::new(),
                json_field_allowlist: Vec::new(),
                conditional_path_allowlist: Vec::new(),
                span_allowlist: Vec::new(),
                terms: vec!["obsolete".into()],
                synonyms: vec![],
                surfaces: vec![Surface::Code],
                allowlist: vec![],
            }),
        },
    )
    .expect("proposal");

    let unaccepted = check_project_version(
        &ledger,
        "project-a",
        repo.path(),
        &old,
        "2026-10-07T00:01:00Z",
        false,
    )
    .expect("no accepted decisions");
    assert!(unaccepted.is_empty());

    let acceptance_id = storage::accept(
        &ledger,
        AcceptanceInput {
            decision_id: decision_id.clone(),
            revision: 1,
            actor: "founder".into(),
            at: "2026-10-07T00:02:00Z".into(),
            because: "Keep the label out of code".into(),
            severity: "blocks".into(),
            applies_from_version: old.clone(),
        },
    )
    .expect("person acceptance");
    let mut person = PersonRecorder::new(&ledger, "project-a");
    person.bind_acceptance(&ledger, &acceptance_id);

    let first = check_project_version(
        &ledger,
        "project-a",
        repo.path(),
        &old,
        "2026-10-07T00:03:00Z",
        false,
    )
    .expect("first computation");
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].state, VerdictState::Broken);
    assert!(!first[0].from_cache);
    assert_eq!(first[0].decision_id, decision_id);
    let tree = git(repo.path(), &["rev-parse", &format!("{old}^{{tree}}")]);
    assert_eq!(first[0].tree_hash.as_deref(), Some(tree.as_str()));
    assert_eq!(first[0].outcome.surfaces[0].finding_count, 1);
    let rows: i64 = ledger
        .conn()
        .query_row("SELECT count(*) FROM decision_verdicts", [], |row| {
            row.get(0)
        })
        .expect("row count");
    assert_eq!(rows, 1);

    let cached = check_project_version(
        &ledger,
        "project-a",
        repo.path(),
        &old,
        "2026-10-07T00:04:00Z",
        false,
    )
    .expect("cached computation");
    assert_eq!(cached[0].state, VerdictState::Broken);
    assert!(cached[0].from_cache);
    assert_eq!(cached[0].verdict_id, first[0].verdict_id);
    let replay = check_project_version(
        &ledger,
        "project-a",
        repo.path(),
        &old,
        "2026-10-07T00:05:00Z",
        true,
    )
    .expect("replay");
    assert_eq!(replay[0].state, VerdictState::Broken);
    assert_eq!(replay[0].outcome, first[0].outcome);

    std::fs::write(
        repo.path().join("src/lib.rs"),
        "const CURRENT: bool = true;\n",
    )
    .expect("new source");
    let new = commit(repo.path(), "new");
    let missing_replay = check_project_version(
        &ledger,
        "project-a",
        repo.path(),
        &new,
        "2026-10-07T00:05:30Z",
        true,
    )
    .expect("missing replay evidence is an outcome");
    assert_eq!(missing_replay[0].state, VerdictState::Unverifiable);
    assert!(missing_replay[0].verdict_id.is_none());
    let held = check_project_version(
        &ledger,
        "project-a",
        repo.path(),
        &new,
        "2026-10-07T00:06:00Z",
        false,
    )
    .expect("new computation");
    assert_eq!(held[0].state, VerdictState::Holds);
    assert_ne!(held[0].tree_hash, first[0].tree_hash);
}

#[test]
fn agent_added_nocap_allow_comment_does_not_amend_person_accepted_allowlist() {
    let repo = tempfile::tempdir().expect("repo");
    git(repo.path(), &["init", "-q"]);
    std::fs::create_dir(repo.path().join("src")).expect("src");
    std::fs::write(
        repo.path().join("src/licence.rs"),
        "pub const TRIAL_DAYS: i64 = 14;\n",
    )
    .expect("source");
    let baseline = commit(repo.path(), "trial source");

    let ledger = Ledger::open(repo.path().join(".git/decision-ledger.db")).expect("ledger");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id,root_path,source,created_at) VALUES(?1,?2,'codex','2026-10-07T00:00:00Z')",
            rusqlite::params!["project-a", repo.path().display().to_string()],
        )
        .expect("project");
    let span = "No trial in source";
    let decision_id = storage::create(
        &ledger,
        NewDecision {
            project_id: "project-a".into(),
            source_anchor: "event-1".into(),
            span: span.into(),
            sources: vec![DecisionSource {
                source_kind: SourceKind::WorkPrompt,
                event_id: Some("event-1".into()),
                file_anchor: None,
                raw_file: "session.jsonl".into(),
                raw_line: 5,
                span_start: 0,
                span_end: span.len() as u64,
                span: span.into(),
                actor: Some("founder".into()),
                recorded_at: Some("2026-10-07T00:00:00Z".into()),
            }],
            author_actor: "founder".into(),
            recorded_at: "2026-10-07T00:00:00Z".into(),
            params: CheckParams::TextAbsent(TextAbsentParams {
                test_allowlist: Vec::new(),
                json_field_allowlist: Vec::new(),
                conditional_path_allowlist: Vec::new(),
                span_allowlist: Vec::new(),
                terms: vec!["trial".into()],
                synonyms: vec![],
                surfaces: vec![Surface::Code],
                allowlist: vec![],
            }),
        },
    )
    .expect("proposal");
    let acceptance_id = storage::accept(
        &ledger,
        AcceptanceInput {
            decision_id: decision_id.clone(),
            revision: 1,
            actor: "founder".into(),
            at: "2026-10-07T00:02:00Z".into(),
            because: "Keep trial out of source".into(),
            severity: "blocks".into(),
            applies_from_version: baseline.clone(),
        },
    )
    .expect("person acceptance");
    let mut person = PersonRecorder::new(&ledger, "project-a");
    person.bind_acceptance(&ledger, &acceptance_id);

    let before = check_project_version(
        &ledger,
        "project-a",
        repo.path(),
        &baseline,
        "2026-10-07T00:03:00Z",
        false,
    )
    .expect("baseline check");
    assert_eq!(before[0].state, VerdictState::Broken);
    assert!(before[0]
        .outcome
        .findings
        .iter()
        .any(|finding| finding.path == "src/licence.rs" && finding.line == 1));

    std::fs::write(
        repo.path().join("src/licence.rs"),
        "// nocap-allow: trial\npub const TRIAL_DAYS: i64 = 14;\n",
    )
    .expect("agent comment");
    git(repo.path(), &["add", "-A"]);
    git(
        repo.path(),
        &[
            "-c",
            "user.name=Agent Fixture",
            "-c",
            "user.email=agent@example.invalid",
            "commit",
            "-qm",
            "agent adds an in-code allowlist comment",
        ],
    );
    let target = git(repo.path(), &["rev-parse", "HEAD"]);

    let after = check_project_version(
        &ledger,
        "project-a",
        repo.path(),
        &target,
        "2026-10-07T00:04:00Z",
        false,
    )
    .expect("target check");
    assert_eq!(after[0].state, VerdictState::Broken);
    assert!(after[0].outcome.findings.iter().any(|finding| {
        finding.path == "src/licence.rs" && finding.line == 2 && finding.span == "TRIAL"
    }));
    assert_eq!(
        after[0].outcome.surfaces[0].finding_count, 2,
        "the source comment is data, never an allowlist directive"
    );
    let accepted = storage::accepted_for_project(&ledger, "project-a").expect("accepted revision");
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].params["allowlist"], serde_json::json!([]));
}
