//! Authored fixture rules through the production importer; no person acceptance.

use std::fs;
use std::path::Path;
use std::process::Command;

use thruline_core::decisions::{rule_files, storage};
use thruline_core::Ledger;

fn git(root: &Path, argv: &[&str]) -> String {
    let result = Command::new("git")
        .args(argv)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "git {argv:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim().into()
}

fn fixture() -> (tempfile::TempDir, tempfile::TempDir, Ledger, String) {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-q"]);
    fs::write(
        repo.path().join("CLAUDE.md"),
        "# Rules\n- Remove obsolete_marker everywhere.\n- Keep retained.txt.\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("AGENTS.md"),
        "Never say deprecated_word.\n",
    )
    .unwrap();
    fs::write(repo.path().join("notes.md"), "Remove unrelated_marker.\n").unwrap();
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
            "rules",
        ],
    );
    let version = git(repo.path(), &["rev-parse", "HEAD"]);
    let store = tempfile::tempdir().unwrap();
    let ledger = Ledger::open(store.path().join("ledger.db")).unwrap();
    ledger.conn().execute("INSERT INTO projects(id,root_path,source,created_at) VALUES('rules',?1,'codex','2026-10-08T00:00:00Z')", [repo.path().to_string_lossy().as_ref()]).unwrap();
    (repo, store, ledger, version)
}

#[test]
fn tracked_exact_version_rule_spans_are_proposals_and_never_acceptances() {
    let (repo, _store, ledger, version) = fixture();
    fs::write(
        repo.path().join("CLAUDE.md"),
        "Remove a_different_worktree_marker.\n",
    )
    .unwrap();
    fs::write(
        repo.path().join(".cursorrules"),
        "Remove untracked_marker.\n",
    )
    .unwrap();
    let report = rule_files::ingest(
        &ledger,
        "rules",
        repo.path(),
        &version,
        "2026-10-08T00:01:00Z",
    )
    .unwrap();
    assert_eq!(report.version_id, version);
    assert_eq!(
        report.proposals.len(),
        3,
        "every supported tracked imperative remains visible"
    );
    for proposal in &report.proposals {
        assert!(proposal.anchor.path == "CLAUDE.md" || proposal.anchor.path == "AGENTS.md");
        assert_eq!(proposal.source.source_kind, storage::SourceKind::RuleFile);
        assert_eq!(proposal.anchor.version_id, version);
        assert_eq!(
            proposal.anchor.span_sha256,
            thruline_core::chain::sha256_hex(proposal.span.as_bytes())
        );
        assert_eq!(
            rule_files::verify_source(repo.path(), &proposal.source)
                .unwrap()
                .state,
            "verified"
        );
        assert!(!proposal.span.contains("a_different_worktree_marker"));
        assert!(!proposal.span.contains("untracked_marker"));
    }
    let absent = report.proposals.iter().find(|p| p.code == "A1").unwrap();
    assert_eq!(absent.source.raw_line, 2);
    assert_eq!(absent.span, "Remove obsolete_marker everywhere");
    for table in [
        "decision_acceptances",
        "decision_person_acts",
        "decision_verdicts",
        "decision_retirements",
    ] {
        assert_eq!(
            ledger
                .conn()
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0,
            "{table}"
        );
    }
}

#[test]
fn reimport_deduplicates_and_changed_blob_gets_a_new_source_identity() {
    let (repo, _store, ledger, version) = fixture();
    let first = rule_files::ingest(
        &ledger,
        "rules",
        repo.path(),
        &version,
        "2026-10-08T00:01:00Z",
    )
    .unwrap();
    assert!(!first.proposals.is_empty());
    let next = rule_files::ingest(
        &ledger,
        "rules",
        repo.path(),
        &version,
        "2026-10-08T00:02:00Z",
    )
    .unwrap();
    assert_eq!(
        first
            .proposals
            .iter()
            .map(|p| &p.decision_id)
            .collect::<Vec<_>>(),
        next.proposals
            .iter()
            .map(|p| &p.decision_id)
            .collect::<Vec<_>>()
    );
    assert!(next.proposals.iter().all(|p| !p.created));
    fs::write(
        repo.path().join("AGENTS.md"),
        "Never say newly_deprecated_word.\n",
    )
    .unwrap();
    git(repo.path(), &["add", "AGENTS.md"]);
    git(
        repo.path(),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "changed rule",
        ],
    );
    let changed = git(repo.path(), &["rev-parse", "HEAD"]);
    let changed = rule_files::ingest(
        &ledger,
        "rules",
        repo.path(),
        &changed,
        "2026-10-08T00:03:00Z",
    )
    .unwrap();
    let old = first
        .proposals
        .iter()
        .find(|p| p.anchor.path == "AGENTS.md")
        .unwrap();
    let new = changed
        .proposals
        .iter()
        .find(|p| p.anchor.path == "AGENTS.md")
        .unwrap();
    assert_ne!(old.decision_id, new.decision_id);
    assert_eq!(
        rule_files::verify_source(repo.path(), &old.source)
            .unwrap()
            .state,
        "verified"
    );
}

#[test]
fn forged_span_pin_and_missing_git_evidence_are_unverifiable() {
    let (repo, _store, ledger, version) = fixture();
    let report = rule_files::ingest(
        &ledger,
        "rules",
        repo.path(),
        &version,
        "2026-10-08T00:01:00Z",
    )
    .unwrap();
    let mut source = report.proposals.first().unwrap().source.clone();
    source.span = "Remove words_not_in_source".into();
    assert_eq!(
        rule_files::verify_source(repo.path(), &source)
            .unwrap()
            .state,
        "unverifiable"
    );
    source = report.proposals.first().unwrap().source.clone();
    source.file_anchor = None;
    assert_eq!(
        rule_files::verify_source(repo.path(), &source)
            .unwrap()
            .state,
        "unverifiable"
    );
    let no_git = tempfile::tempdir().unwrap();
    assert_eq!(
        rule_files::verify_source(no_git.path(), &report.proposals[0].source)
            .unwrap()
            .state,
        "unverifiable"
    );
}

fn commit_all(root: &Path) -> String {
    git(root, &["add", "-A"]);
    git(
        root,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "rule fixture",
        ],
    );
    git(root, &["rev-parse", "HEAD"])
}

#[test]
fn equivalent_rule_files_exclude_derived_copy_symlinks_and_fenced_examples() {
    let (repo, _store, ledger, _) = fixture();
    fs::create_dir_all(repo.path().join(".cursor/rules")).unwrap();
    fs::create_dir_all(repo.path().join(".thruline")).unwrap();
    fs::write(
        repo.path().join(".cursorrules"),
        "Remove cursor_marker.\nRemove `retired_symbol`.\n",
    )
    .unwrap();
    fs::write(
        repo.path().join(".cursor/rules/core.mdc"),
        "Keep nested.txt.\n",
    )
    .unwrap();
    fs::write(
        repo.path().join(".thruline/decisions.yml"),
        "Remove derived_marker.\n",
    )
    .unwrap();
    fs::write(
        repo.path().join("GEMINI.md"),
        "```text\nRemove example_marker.\n```\n\nOnly touch human_scope.\n",
    )
    .unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("notes.md", repo.path().join(".windsurfrules")).unwrap();
    let version = commit_all(repo.path());
    // A normal unrelated Git submodule must not stop reading rule blobs.
    git(
        repo.path(),
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{version},vendored"),
        ],
    );
    git(
        repo.path(),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "unrelated submodule",
        ],
    );
    let version = git(repo.path(), &["rev-parse", "HEAD"]);
    let report = rule_files::ingest(
        &ledger,
        "rules",
        repo.path(),
        &version,
        "2026-10-08T00:05:00Z",
    )
    .unwrap();
    assert!(report
        .proposals
        .iter()
        .any(|p| p.source.raw_file == ".cursorrules" && p.decision_id.is_some()));
    assert!(report.proposals.iter().any(|p| p.code == "A2"
        && p.params
            .as_ref()
            .is_some_and(|params| params["symbol"] == "retired_symbol")));
    assert!(report
        .proposals
        .iter()
        .any(|p| p.source.raw_file == ".cursor/rules/core.mdc" && p.decision_id.is_some()));
    assert!(!report
        .proposals
        .iter()
        .any(|p| p.span.contains("example_marker")
            || p.span.contains("derived_marker")
            || p.source.raw_file == ".windsurfrules"));
    let incomplete = report
        .proposals
        .iter()
        .find(|p| p.span == "Only touch human_scope")
        .unwrap();
    assert!(incomplete.params.is_none() && incomplete.decision_id.is_none());
    assert!(report
        .notes
        .iter()
        .any(|p| p.path == "GEMINI.md" && p.state == "needs_parameters"));
    #[cfg(unix)]
    assert!(report
        .notes
        .iter()
        .any(|p| p.path == ".windsurfrules" && p.state == "unverifiable"));
}

#[test]
fn wrapped_utf8_source_has_exact_global_offsets_and_all_named_terms() {
    let (repo, _store, ledger, _) = fixture();
    fs::write(
        repo.path().join("CLAUDE.md"),
        "# Café\n\n- Remove first_marker and second_marker\n  everywhere.\n",
    )
    .unwrap();
    let version = commit_all(repo.path());
    let report = rule_files::ingest(
        &ledger,
        "rules",
        repo.path(),
        &version,
        "2026-10-08T00:05:00Z",
    )
    .unwrap();
    let rule = report
        .proposals
        .iter()
        .find(|p| p.source.raw_file == "CLAUDE.md")
        .unwrap();
    assert_eq!(rule.source.raw_line, 3);
    assert_eq!(rule.anchor.span_start, "# Café\n\n- ".len() as u64);
    assert_eq!(
        rule.params.as_ref().unwrap()["terms"],
        serde_json::json!(["first_marker", "second_marker"])
    );
    assert_eq!(
        rule_files::verify_source(repo.path(), &rule.source)
            .unwrap()
            .state,
        "verified"
    );
    assert!(rule_files::ingest(
        &ledger,
        "rules",
        repo.path(),
        "missing-version",
        "2026-10-08T00:05:00Z"
    )
    .is_err());
}

#[test]
fn changed_stored_proposal_bytes_are_not_silently_deduplicated() {
    let (repo, _store, ledger, version) = fixture();
    let report = rule_files::ingest(
        &ledger,
        "rules",
        repo.path(),
        &version,
        "2026-10-08T00:01:00Z",
    )
    .unwrap();
    let id = report.proposals[0].decision_id.as_deref().unwrap();
    ledger
        .conn()
        .execute_batch("DROP TRIGGER decision_revisions_no_update")
        .unwrap();
    ledger.conn().execute("UPDATE decision_revisions SET text='words outside the pinned span' WHERE decision_id=?1", [id]).unwrap();
    assert!(rule_files::ingest(
        &ledger,
        "rules",
        repo.path(),
        &version,
        "2026-10-08T00:05:00Z"
    )
    .is_err());
}

#[test]
fn parameter_notes_survive_import_and_stay_append_only_with_exact_sources() {
    let (repo, store, ledger, _) = fixture();
    fs::write(repo.path().join("CLAUDE.md"), "Only touch human_scope.\n").unwrap();
    let version = commit_all(repo.path());
    let before = rule_files::notes_for_project(&ledger, "rules").unwrap();
    assert_eq!(before.state, "not_recorded");
    let absent: i64 = ledger
        .conn()
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='decision_rule_file_notes'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(absent, 0, "read does not create schema");
    let report = rule_files::ingest(
        &ledger,
        "rules",
        repo.path(),
        &version,
        "2026-10-08T00:05:00Z",
    )
    .unwrap();
    let proposal = report
        .proposals
        .iter()
        .find(|proposal| proposal.code == "D1")
        .unwrap();
    let note_id = proposal
        .proposal_note_id
        .as_ref()
        .expect("durable exact source note ID");
    let stored = rule_files::notes_for_project(&ledger, "rules").unwrap();
    assert_eq!(stored.state, "recorded");
    let saved = stored
        .notes
        .iter()
        .find(|note| &note.id == note_id)
        .unwrap();
    assert_eq!(saved.state, "needs_parameters");
    assert_eq!(saved.code, "D1");
    assert_eq!(saved.span, "Only touch human_scope");
    assert!(saved.params.is_none() && saved.decision_id.is_none());
    assert_eq!(
        rule_files::verify_source(repo.path(), &saved.source)
            .unwrap()
            .state,
        "verified"
    );
    let repeat = rule_files::ingest(
        &ledger,
        "rules",
        repo.path(),
        &version,
        "2026-10-08T00:06:00Z",
    )
    .unwrap();
    assert_eq!(
        repeat
            .proposals
            .iter()
            .find(|proposal| proposal.code == "D1")
            .unwrap()
            .proposal_note_id
            .as_ref(),
        Some(note_id)
    );
    assert_eq!(
        rule_files::notes_for_project(&ledger, "rules")
            .unwrap()
            .notes
            .len(),
        stored.notes.len()
    );
    assert!(ledger
        .conn()
        .execute(
            "UPDATE decision_rule_file_notes SET span='changed' WHERE id=?1",
            [note_id]
        )
        .is_err());
    assert!(ledger
        .conn()
        .execute(
            "DELETE FROM decision_rule_file_notes WHERE id=?1",
            [note_id]
        )
        .is_err());
    let count: i64 = ledger
        .conn()
        .query_row("SELECT count(*) FROM decision_acceptances", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
    let reopened = Ledger::open(store.path().join("ledger.db"))
        .expect("additive note schema does not require a ledger upgrade");
    assert_eq!(
        rule_files::notes_for_project(&reopened, "rules")
            .unwrap()
            .notes
            .len(),
        stored.notes.len()
    );
    let again = rule_files::ingest(
        &reopened,
        "rules",
        repo.path(),
        &version,
        "2026-10-08T00:07:00Z",
    )
    .unwrap();
    assert_eq!(
        again
            .proposals
            .iter()
            .find(|proposal| proposal.code == "D1")
            .unwrap()
            .proposal_note_id
            .as_ref(),
        Some(note_id)
    );
}

#[test]
fn unchanged_blob_new_commit_keeps_typed_and_note_identity_and_initial_receipt() {
    let (repo, _store, ledger, _) = fixture();
    fs::write(
        repo.path().join("CLAUDE.md"),
        "Only touch human_scope.\nKeep retained.txt.\n",
    )
    .unwrap();
    let original = commit_all(repo.path());
    let before = rule_files::ingest(
        &ledger,
        "rules",
        repo.path(),
        &original,
        "2026-10-08T00:05:00Z",
    )
    .unwrap();
    fs::write(repo.path().join("unrelated.txt"), "new unrelated bytes\n").unwrap();
    let next = commit_all(repo.path());
    assert_ne!(next, original);
    let after =
        rule_files::ingest(&ledger, "rules", repo.path(), &next, "2026-10-08T00:06:00Z").unwrap();
    assert_eq!(
        before
            .proposals
            .iter()
            .map(|p| (&p.decision_id, &p.proposal_note_id))
            .collect::<Vec<_>>(),
        after
            .proposals
            .iter()
            .map(|p| (&p.decision_id, &p.proposal_note_id))
            .collect::<Vec<_>>()
    );
    assert!(after
        .proposals
        .iter()
        .all(|proposal| !proposal.created && proposal.anchor.version_id == original));
    assert!(after
        .proposals
        .iter()
        .all(
            |proposal| rule_files::verify_source(repo.path(), &proposal.source)
                .unwrap()
                .state
                == "verified"
        ));
}

#[test]
fn foreign_note_schema_is_refused_without_migration_or_source_mutation() {
    let (repo, _store, ledger, _) = fixture();
    fs::write(repo.path().join("CLAUDE.md"), "Only touch human_scope.\n").unwrap();
    let version = commit_all(repo.path());
    ledger.conn().execute_batch("CREATE TABLE decision_rule_file_notes(id TEXT PRIMARY KEY, wrong TEXT); INSERT INTO decision_rule_file_notes VALUES('untouched','bytes');").unwrap();
    assert!(rule_files::notes_for_project(&ledger, "rules").is_err());
    assert!(rule_files::ingest(
        &ledger,
        "rules",
        repo.path(),
        &version,
        "2026-10-08T00:05:00Z"
    )
    .is_err());
    let original: String = ledger
        .conn()
        .query_row(
            "SELECT wrong FROM decision_rule_file_notes WHERE id='untouched'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(original, "bytes");
}

#[test]
fn rule_import_refuses_bare_credential_source_and_path_before_proposal_writes() {
    for prefix in [
        "sk-",
        "sk-ant-",
        "ghp_",
        "github_pat_",
        "AKIA",
        "ASIA",
        "AIza",
        "eyJ",
        "xoxb-",
    ] {
        let (repo, _store, ledger, _) = fixture();
        fs::write(
            repo.path().join("CLAUDE.md"),
            format!("Keep policy/{prefix}.txt.\n"),
        )
        .unwrap();
        let directory = repo.path().join(prefix);
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("AGENTS.md"), "Keep private-scope.txt.\n").unwrap();
        let version = commit_all(repo.path());
        let before: i64 = ledger
            .conn()
            .query_row("SELECT count(*) FROM decision_revisions", [], |row| {
                row.get(0)
            })
            .unwrap();
        let report = rule_files::ingest(
            &ledger,
            "rules",
            repo.path(),
            &version,
            "2026-10-08T00:08:00Z",
        )
        .unwrap();
        assert!(
            !report
                .proposals
                .iter()
                .any(|proposal| proposal.source.raw_file == "CLAUDE.md"
                    || proposal.source.raw_file.starts_with(prefix)),
            "bare prefix {prefix} was proposed: {report:?}"
        );
        assert!(report
            .notes
            .iter()
            .any(|note| note.path == "CLAUDE.md" && note.state == "unverifiable"));
        assert!(report
            .notes
            .iter()
            .any(|note| note.path == "[masked rule path]" && note.state == "unverifiable"));
        // The fixture's independent plain AGENTS.md still imports, proving
        // this guard did not simply disable the importer.
        assert!(report
            .proposals
            .iter()
            .any(|proposal| proposal.source.raw_file == "AGENTS.md"));
        let after: i64 = ledger
            .conn()
            .query_row("SELECT count(*) FROM decision_revisions", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(
            after - before,
            1,
            "only the independent plain rule may be written"
        );
        assert_eq!(
            ledger
                .conn()
                .query_row("SELECT count(*) FROM decision_acceptances", [], |row| row
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            0
        );
    }
}
