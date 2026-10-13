//! B5 can prove that a protected rule file is absent from a named Git tree
//! even when recorded edit authorship is incomplete. Presence alone cannot
//! prove that the actor rule holds.

#[path = "support/person_action.rs"]
mod person_action;

use std::path::Path;
use std::process::Command;

use person_action::PersonRecorder;
use rusqlite::params;
use serde_json::json;
use thruline_core::decisions::checks::VerdictState;
use thruline_core::decisions::engine;
use thruline_core::decisions::params::Surface;
use thruline_core::ledger::Ledger;

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("fixture git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git output is UTF-8")
        .trim()
        .to_owned()
}

struct Fixture {
    repo: tempfile::TempDir,
    ledger: Ledger,
    baseline: String,
    baseline_tree: String,
    removed: String,
    removed_tree: String,
    _person: PersonRecorder,
}

impl Fixture {
    fn new() -> Self {
        let repo = tempfile::tempdir().expect("scratch repository");
        git(repo.path(), &["init", "-q"]);
        git(repo.path(), &["config", "user.name", "Decisions Fixture"]);
        git(
            repo.path(),
            &["config", "user.email", "fixture@example.invalid"],
        );
        std::fs::write(repo.path().join("CLAUDE.md"), "Keep this rule.\n").expect("rule file");
        std::fs::write(repo.path().join("README.md"), "Fixture.\n").expect("readme");
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-qm", "rule present"]);
        let baseline = git(repo.path(), &["rev-parse", "HEAD"]);
        let baseline_tree = git(repo.path(), &["rev-parse", "HEAD^{tree}"]);
        git(repo.path(), &["rm", "CLAUDE.md"]);
        git(repo.path(), &["commit", "-qm", "rule removed"]);
        let removed = git(repo.path(), &["rev-parse", "HEAD"]);
        let removed_tree = git(repo.path(), &["rev-parse", "HEAD^{tree}"]);

        let ledger =
            Ledger::open(repo.path().join(".git/decision-ledger.db")).expect("scratch ledger");
        ledger.conn().execute(
            "INSERT INTO projects(id,root_path,source,created_at) VALUES('b5-project',?1,'codex','2026-10-07T00:00:00Z')",
            [repo.path().to_string_lossy().as_ref()],
        ).expect("project");
        ledger.conn().execute(
            "INSERT INTO decisions(id,project_id,created_at) VALUES('b5-decision','b5-project','2026-10-07T00:00:00Z')",
            [],
        ).expect("decision");
        ledger.conn().execute(
            "INSERT INTO decision_revisions(decision_id,rev,kind,params_schema_version,params_json,text,sources_json,author_actor,recorded_at)
             VALUES('b5-decision',1,'rule_file_present',1,?1,'Keep this rule.','[]','person','2026-10-07T00:00:00Z')",
            [json!({"kind":"rule_file_present","paths":["CLAUDE.md"],"person_edit_basis":"same_version"}).to_string()],
        ).expect("revision");
        ledger.conn().execute(
            "INSERT INTO decision_acceptances(id,decision_id,rev,actor,accepted_at,rationale,severity,applies_from_version)
             VALUES('b5-accept','b5-decision',1,'person','2026-10-07T00:01:00Z','protect the rule file','warns',?1)",
            params![baseline],
        ).expect("acceptance");
        let mut person = PersonRecorder::new(&ledger, "b5-project");
        person.bind_acceptance(&ledger, "b5-accept");
        Self {
            repo,
            ledger,
            baseline,
            baseline_tree,
            removed,
            removed_tree,
            _person: person,
        }
    }

    fn check(&self, version: &str) -> thruline_core::decisions::engine::RunResult {
        let results = engine::check_decision_version(
            &self.ledger,
            "b5-project",
            self.repo.path(),
            version,
            "2026-10-07T00:02:00Z",
            false,
            "b5-decision",
        )
        .expect("accepted B5 check");
        assert_eq!(results.len(), 1);
        results.into_iter().next().expect("one result")
    }
}

#[test]
fn accepted_rule_file_removed_on_exact_tree_is_broken_without_actor_census() {
    let fixture = Fixture::new();
    let result = fixture.check(&fixture.removed);
    assert_eq!(result.state, VerdictState::Broken);
    assert_eq!(
        result.tree_hash.as_deref(),
        Some(fixture.removed_tree.as_str())
    );
    assert_eq!(result.outcome.findings.len(), 1);
    assert_eq!(result.outcome.findings[0].path, "CLAUDE.md");
    assert_eq!(result.outcome.findings[0].surface, Surface::Docs);
}

#[test]
fn present_rule_file_does_not_prove_actor_provenance() {
    let fixture = Fixture::new();
    let result = fixture.check(&fixture.baseline);
    assert_eq!(result.state, VerdictState::Unverifiable);
    assert_eq!(
        result.tree_hash.as_deref(),
        Some(fixture.baseline_tree.as_str())
    );
    assert!(result.outcome.findings.is_empty());
}

#[test]
fn unresolved_version_cannot_prove_rule_file_absence() {
    let fixture = Fixture::new();
    let result = fixture.check(&"0".repeat(40));
    assert_eq!(result.state, VerdictState::Unverifiable);
    assert!(result.tree_hash.is_none());
    assert!(result.outcome.findings.is_empty());

    let unreadable = engine::check_decision_version(
        &fixture.ledger,
        "b5-project",
        &fixture.repo.path().join("unreadable"),
        &fixture.removed,
        "2026-10-07T00:03:00Z",
        false,
        "b5-decision",
    )
    .expect("unreadable repository returns a bounded result");
    assert_eq!(unreadable.len(), 1);
    assert_eq!(unreadable[0].state, VerdictState::Unverifiable);
    assert!(unreadable[0].tree_hash.is_none());
    assert!(unreadable[0].outcome.findings.is_empty());
}
