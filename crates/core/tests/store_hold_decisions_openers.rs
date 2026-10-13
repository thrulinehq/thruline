//! C47 narrow opener exception: both Decisions readers honor the removal hold.

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
    self, AcceptanceInput, DecisionSource, NewDecision, SourceKind, VerdictInput,
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
}

#[test]
fn no_hold_decisions_readers_preserve_store_bytes_then_both_refuse_the_hold() {
    use thruline_core::{decisions::inline, error::CoreError, store_hold::StoreHold};
    let fixture = Fixture::new();
    fixture.broken("hold-source", "Keep the agreed rule");
    drop(fixture.ledger);
    let before = std::fs::read(&fixture.db).expect("closed store bytes");
    let line = speaker::cached_line_within(&fixture.db, &fixture.root, speaker_budget::ROOMY)
        .expect("known broken line");
    assert!(line.contains(&fixture.version[..8]));
    inline::cached_read_only(
        &fixture.db,
        &fixture.root,
        "project-speaker",
        &fixture.version,
        false,
    )
    .expect("same cached sweep without a hold");
    assert_eq!(std::fs::read(&fixture.db).unwrap(), before);
    let owner = StoreHold::create(&fixture.db, "removal-reader", "P1").expect("hold");
    assert!(
        speaker::cached_line_within(&fixture.db, &fixture.root, speaker_budget::ROOMY).is_none(),
        "held speaker must be silent"
    );
    assert!(matches!(
        inline::cached_read_only(
            &fixture.db,
            &fixture.root,
            "project-speaker",
            &fixture.version,
            false
        ),
        Err(CoreError::StoreHeld { .. })
    ));
    assert_eq!(std::fs::read(&fixture.db).unwrap(), before);
    owner.release().expect("release");
}
