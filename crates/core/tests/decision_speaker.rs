//! The Stop speaker reads cached decision verdicts only and names a proved tree.

#[path = "support/person_action.rs"]
mod person_action;
#[path = "support/speaker_budget.rs"]
mod speaker_budget;

use person_action::PersonRecorder;
use std::cell::RefCell;

use std::path::Path;
use std::process::Command;

use serde_json::json;
use thruline_core::decisions::engine::CHECK_IMPL_VERSION;
use thruline_core::decisions::params::{CheckParams, PathPresentParams};
use thruline_core::decisions::speaker;
use thruline_core::decisions::storage::{
    self, AcceptanceInput, DecisionSource, NewDecision, RevisionInput, SourceKind, VerdictInput,
};
use thruline_core::ledger::Ledger;

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git command");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("text")
        .trim()
        .into()
}

struct Fixture {
    _temp: tempfile::TempDir,
    root: std::path::PathBuf,
    db: std::path::PathBuf,
    ledger: Ledger,
    person: RefCell<PersonRecorder>,
    version: String,
    tree: String,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("scratch");
        let root = temp.path().join("repo");
        std::fs::create_dir_all(&root).expect("repo");
        git(&root, &["init", "-q"]);
        std::fs::write(root.join("kept.txt"), "present\n").expect("source file");
        git(&root, &["add", "kept.txt"]);
        git(
            &root,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-qm",
                "base",
            ],
        );
        let version = git(&root, &["rev-parse", "HEAD"]);
        let tree = git(&root, &["rev-parse", "HEAD^{tree}"]);
        let db = temp.path().join("ledger.db");
        let ledger = Ledger::open(&db).expect("ledger");
        ledger
            .conn()
            .execute(
                "INSERT INTO projects(id,root_path,source,created_at) VALUES(?1,?2,'codex','2026-10-07T00:00:00Z')",
                ("project-speaker", root.to_str().expect("root text")),
            )
            .expect("project");
        let person = PersonRecorder::new(&ledger, "project-speaker");
        Self {
            _temp: temp,
            root,
            db,
            ledger,
            person: RefCell::new(person),
            version,
            tree,
        }
    }

    fn broken(&self, source: &str, span: &str) -> String {
        self.broken_with_checked_places(source, span, 1)
    }

    fn broken_with_checked_places(&self, source: &str, span: &str, checked_places: u64) -> String {
        self.broken_with_impl(source, span, checked_places, CHECK_IMPL_VERSION)
    }

    fn broken_with_impl(
        &self,
        source: &str,
        span: &str,
        checked_places: u64,
        implementation: &str,
    ) -> String {
        let id = storage::create(
            &self.ledger,
            NewDecision {
                project_id: "project-speaker".into(),
                source_anchor: source.into(),
                span: span.into(),
                sources: vec![DecisionSource {
                    source_kind: SourceKind::RuleFile,
                    event_id: None,
                    file_anchor: Some(source.into()),
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
                params: CheckParams::PathPresent(PathPresentParams {
                    path: "kept.txt".into(),
                    sha256: None,
                }),
            },
        )
        .expect("proposal");
        let acceptance_id = storage::accept(
            &self.ledger,
            AcceptanceInput {
                decision_id: id.clone(),
                revision: 1,
                actor: "person".into(),
                at: "2026-10-07T00:01:00Z".into(),
                because: "kept rule".into(),
                severity: "blocks".into(),
                applies_from_version: self.version.clone(),
            },
        )
        .expect("acceptance");
        self.person
            .borrow_mut()
            .bind_bridge_acceptance(&self.ledger, &acceptance_id);
        storage::record_verdict(
            &self.ledger,
            VerdictInput {
                decision_id: id.clone(),
                revision: 1,
                version_id: self.version.clone(),
                tree_hash: self.tree.clone(),
                verdict: "broken".into(),
                reason: Some("missing rule".into()),
                checkable_if: None,
                findings: json!([{"surface":"code","path":"kept.txt","line":1,"span":"missing"}]),
                surfaces: json!([]),
                check_impl_version: implementation.into(),
                tool_versions: json!({"git":"fixture"}),
                computed_at: "2026-10-07T00:02:00Z".into(),
                basis: json!({"checked_places":checked_places}),
            },
        )
        .expect("verdict");
        id
    }

    fn verdict_count(&self) -> i64 {
        self.ledger
            .conn()
            .query_row("SELECT count(*) FROM decision_verdicts", [], |row| {
                row.get(0)
            })
            .expect("count")
    }
}

#[test]
fn obsolete_checker_receipt_cannot_make_the_speaker_claim_broken() {
    let fixture = Fixture::new();
    fixture.broken_with_impl(
        "obsolete-source",
        "Keep the old rule",
        1,
        "obsolete.fixture",
    );
    let before = fixture.verdict_count();
    assert!(
        speaker::cached_line_within(&fixture.db, &fixture.root, speaker_budget::ROOMY).is_none()
    );
    assert_eq!(fixture.verdict_count(), before);
}

#[test]
fn speaker_counts_offending_findings_not_examined_places() {
    let fixture = Fixture::new();
    fixture.broken_with_checked_places("count-source", "Keep the agreed rule", 17);
    let line = speaker::cached_line_within(&fixture.db, &fixture.root, speaker_budget::ROOMY)
        .expect("broken line");
    assert!(line.contains("(1 place)"), "{line}");
    assert!(!line.contains("17 places"), "{line}");
}

#[test]
fn one_cached_broken_line_names_exact_commit_and_writes_no_verdict() {
    let fixture = Fixture::new();
    assert!(
        speaker::cached_line_within(&fixture.db, &fixture.root, speaker_budget::ROOMY).is_none()
    );
    let id = fixture.broken("source-1", "Keep the agreed rule");
    let second_id = fixture.broken("source-2", "Keep the second rule");
    let before = fixture.verdict_count();
    let line = speaker::cached_line_within(&fixture.db, &fixture.root, speaker_budget::ROOMY)
        .expect("one line");
    assert!(line.contains("Decision broken on"), "{line}");
    assert!(line.contains(&fixture.version[..8]), "{line}");
    assert!(
        line.contains(&format!("why {id}")) || line.contains(&format!("why {second_id}")),
        "{line}"
    );
    assert!(!line.contains("this version"), "{line}");
    assert_eq!(line.lines().count(), 1);
    assert_eq!(
        fixture.verdict_count(),
        before,
        "speaker must not run checks"
    );
}

#[test]
fn speaker_is_silent_when_recorded_person_source_changes() {
    let fixture = Fixture::new();
    fixture.broken("tamper-source", "Keep the agreed rule");
    assert!(
        speaker::cached_line_within(&fixture.db, &fixture.root, speaker_budget::ROOMY).is_some()
    );
    let source = fixture.person.borrow();
    std::fs::write(source.raw_file(), "tampered\n").expect("tamper synthetic source");
    assert!(
        speaker::cached_line_within(&fixture.db, &fixture.root, speaker_budget::ROOMY).is_none()
    );
}

#[test]
fn newer_verified_revision_without_a_verdict_suppresses_older_broken_line() {
    let fixture = Fixture::new();
    let id = fixture.broken("older-source", "Keep the older rule");
    let revision = storage::append_revision(
        &fixture.ledger,
        RevisionInput {
            decision_id: id.clone(),
            span: "Keep the new rule".into(),
            sources: vec![DecisionSource {
                source_kind: SourceKind::RuleFile,
                event_id: None,
                file_anchor: Some("newer-source".into()),
                raw_file: "AGENTS.md".into(),
                raw_line: 2,
                span_start: 0,
                span_end: 17,
                span: "Keep the new rule".into(),
                actor: Some("person".into()),
                recorded_at: Some("2026-10-07T00:03:00Z".into()),
            }],
            author_actor: "person".into(),
            recorded_at: "2026-10-07T00:03:00Z".into(),
            params: CheckParams::PathPresent(PathPresentParams {
                path: "kept.txt".into(),
                sha256: None,
            }),
        },
    )
    .expect("revision");
    let act = storage::accept(
        &fixture.ledger,
        AcceptanceInput {
            decision_id: id,
            revision,
            actor: "person".into(),
            at: "2026-10-07T00:04:00Z".into(),
            because: "new words".into(),
            severity: "blocks".into(),
            applies_from_version: fixture.version.clone(),
        },
    )
    .expect("acceptance");
    fixture
        .person
        .borrow_mut()
        .bind_bridge_acceptance(&fixture.ledger, &act);
    assert!(
        speaker::cached_line_within(&fixture.db, &fixture.root, speaker_budget::ROOMY).is_none()
    );
}

#[test]
fn dirty_or_newer_head_suppresses_old_cached_verdict() {
    let fixture = Fixture::new();
    fixture.broken("source-1", "Keep the agreed rule");
    std::fs::write(fixture.root.join("kept.txt"), "edited\n").expect("dirty edit");
    assert!(
        speaker::cached_line_within(&fixture.db, &fixture.root, speaker_budget::ROOMY).is_none()
    );
    git(&fixture.root, &["add", "kept.txt"]);
    git(
        &fixture.root,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-qm",
            "next version",
        ],
    );
    assert!(
        speaker::cached_line_within(&fixture.db, &fixture.root, speaker_budget::ROOMY).is_none()
    );
}
