//! T-2-4 fix round 1 — `thruline change new`, the only way a person can open a
//! Change.
//!
//! # What this file exists against
//!
//! T-2-4 built the screen that shows one Change. Nothing in the product could
//! create one: `thruline change` had six verbs and none of them opened a
//! Change, and both the coder and the verifier had to write a scratch Rust
//! program that called `thruline_core::change::insert_change` directly to
//! produce a fixture for the card's own walkthrough. That is a capability no
//! person has. A screen whose subject nobody can create is a screen nobody can
//! reach, and the verifier recorded it as a product gap
//! (`docs/loop/reports/T-2-4-verifier-r0.md`, finding 3).
//!
//! Every store below is a fixture under the test's own temporary directory. No
//! real store is opened.

use std::path::{Path, PathBuf};
use std::process::Command;

use rusqlite::params;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000p001";
const OTHER: &str = "cccccccc-0000-4000-8000-00000000p002";

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

/// A ledger with `projects` rows and nothing else.
fn fixture(dir: &Path, projects: &[&str]) -> PathBuf {
    let db = dir.join("ledger.db");
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open a fresh ledger");
    for (i, id) in projects.iter().enumerate() {
        ledger
            .conn()
            .execute(
                "INSERT INTO projects(id, root_path, source, display_name, created_at)
                 VALUES(?1, ?2, 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
                params![id, format!("/tmp/synthetic-project-{i}")],
            )
            .expect("project");
    }
    db
}

struct Run {
    code: i32,
    out: String,
    err: String,
}

fn run(db: &Path, args: &[&str]) -> Run {
    let out = Command::new(bin())
        .arg("--db")
        .arg(db)
        .arg("change")
        .args(args)
        .output()
        .expect("running `thruline change`");
    Run {
        code: out.status.code().unwrap_or(-1),
        out: String::from_utf8_lossy(&out.stdout).into_owned(),
        err: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn changes(db: &Path) -> Vec<(String, String, String, String, String)> {
    let ledger = thruline_core::ledger::Ledger::open(db).expect("open");
    let mut stmt = ledger
        .conn()
        .prepare("SELECT id, project_id, title, requester, request_source FROM changes ORDER BY id")
        .expect("prepare");
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })
        .expect("query");
    rows.map(|r| r.expect("row")).collect()
}

#[test]
fn a_person_can_open_a_change_and_the_record_holds_exactly_what_they_said() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let db = fixture(dir.path(), &[PROJECT]);
    let r = run(
        &db,
        &[
            "new",
            "--title",
            "Retry failed uploads",
            "--requester",
            "Sam",
            "--purpose",
            "When an upload fails, the retry runs three times and then stops.",
        ],
    );
    assert_eq!(r.code, 0, "stderr: {}", r.err);

    let lines: Vec<&str> = r.out.lines().collect();
    assert_eq!(lines.len(), 2, "output was {:?}", r.out);
    assert_eq!(
        lines[0],
        thruline_core::voice::change_open_sentence("change_opened")
            .expect("the copy line")
            .0,
        "the verb wrote a sentence of its own instead of reading the copy file"
    );

    let rows = changes(&db);
    assert_eq!(rows.len(), 1);
    let (id, project, title, requester, source) = rows[0].clone();
    assert_eq!(lines[1], id, "the id printed is not the id written");
    assert_eq!(project, PROJECT);
    assert_eq!(title, "Retry failed uploads");
    assert_eq!(requester, "Sam");
    assert_eq!(
        source, "person",
        "a person typed this and the record says so"
    );

    // Opening a Change decides nothing else: no scope version, no acceptance,
    // no link, no implementation binding (`B3.4` and nothing more).
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    for table in [
        "change_scope_revisions",
        "change_scope_acceptances",
        "change_session_associations",
        "change_implementations",
    ] {
        let n: i64 = ledger
            .conn()
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap_or_else(|e| panic!("counting {table}: {e}"));
        assert_eq!(n, 0, "opening a Change wrote a row in {table}");
    }
}

/// The id the verb mints is an id the Change screen's address grammar accepts
/// (`change-workspace.jsx`: `#/changes/([A-Za-z0-9_-]{1,128})`). An id the
/// product cannot put in an address is an id nobody can open.
#[test]
fn the_id_it_mints_can_be_opened_on_the_screen_that_shows_a_change() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let db = fixture(dir.path(), &[PROJECT]);
    let r = run(
        &db,
        &[
            "new",
            "--title",
            "Retry failed uploads",
            "--requester",
            "Sam",
        ],
    );
    assert_eq!(r.code, 0, "stderr: {}", r.err);
    let id = r.out.lines().nth(1).expect("the id").to_string();
    assert!(
        !id.is_empty() && id.len() <= 128,
        "an id of {} characters cannot be an address",
        id.len()
    );
    assert!(
        id.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
        "{id:?} carries a character the screen's address grammar refuses"
    );

    // And the other verbs accept it, so the id is one product-wide fact.
    let proposed = run(&db, &["propose", &id]);
    assert_eq!(proposed.code, 0, "stderr: {}", proposed.err);
    let suggested = run(&db, &["suggest", &id]);
    assert_eq!(suggested.code, 0, "stderr: {}", suggested.err);
}

#[test]
fn without_a_title_or_without_a_name_nothing_is_written_and_the_reason_says_which() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let db = fixture(dir.path(), &[PROJECT]);

    let no_name = run(&db, &["new", "--title", "Retry failed uploads"]);
    assert_ne!(no_name.code, 0);
    assert!(
        no_name.err.contains(
            thruline_core::voice::change_open_sentence("change_open_needs_requester")
                .expect("the copy line")
                .0
        ),
        "stderr: {}",
        no_name.err
    );

    let no_title = run(&db, &["new", "--title", "   ", "--requester", "Sam"]);
    assert_ne!(no_title.code, 0);
    assert!(
        no_title.err.contains(
            thruline_core::voice::change_open_sentence("change_open_needs_title")
                .expect("the copy line")
                .0
        ),
        "stderr: {}",
        no_title.err
    );

    assert!(
        changes(&db).is_empty(),
        "a refusal wrote a row: the two halves of `fail loud` are the words AND the empty table"
    );
}

#[test]
fn a_project_nobody_has_work_under_is_refused_with_the_names_that_do_exist() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let db = fixture(dir.path(), &[PROJECT]);
    let r = run(
        &db,
        &[
            "new",
            "--title",
            "Retry",
            "--requester",
            "Sam",
            "--for",
            "not-a-project",
        ],
    );
    assert_ne!(r.code, 0);
    assert!(
        r.err.contains(
            thruline_core::voice::change_open_sentence("change_open_unknown_project")
                .expect("the copy line")
                .0
        ),
        "stderr: {}",
        r.err
    );
    assert!(
        r.err.contains(PROJECT),
        "the refusal did not name the work that IS here: {}",
        r.err
    );
    assert!(changes(&db).is_empty());
}

#[test]
fn with_one_project_it_needs_no_name_and_with_two_it_refuses_to_guess() {
    let one = tempfile::tempdir().expect("temporary directory");
    let db = fixture(one.path(), &[PROJECT]);
    let r = run(&db, &["new", "--title", "Retry", "--requester", "Sam"]);
    assert_eq!(r.code, 0, "stderr: {}", r.err);
    assert_eq!(changes(&db)[0].1, PROJECT);

    let two = tempfile::tempdir().expect("temporary directory");
    let db2 = fixture(two.path(), &[PROJECT, OTHER]);
    let ambiguous = run(&db2, &["new", "--title", "Retry", "--requester", "Sam"]);
    assert_ne!(ambiguous.code, 0);
    assert!(
        ambiguous.err.contains(
            thruline_core::voice::change_open_sentence("change_open_project_needed")
                .expect("the copy line")
                .0
        ),
        "stderr: {}",
        ambiguous.err
    );
    assert!(
        ambiguous.err.contains(PROJECT) && ambiguous.err.contains(OTHER),
        "the refusal did not say which names to choose between: {}",
        ambiguous.err
    );
    assert!(
        changes(&db2).is_empty(),
        "it guessed a project rather than asking"
    );

    let none = tempfile::tempdir().expect("temporary directory");
    let db3 = fixture(none.path(), &[]);
    let empty = run(&db3, &["new", "--title", "Retry", "--requester", "Sam"]);
    assert_ne!(empty.code, 0);
    assert!(
        empty.err.contains(
            thruline_core::voice::change_open_sentence("change_open_no_projects")
                .expect("the copy line")
                .0
        ),
        "stderr: {}",
        empty.err
    );
}

#[test]
fn the_json_shape_is_versioned_and_carries_both_registers() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let db = fixture(dir.path(), &[PROJECT]);
    let r = run(
        &db,
        &[
            "new",
            "--title",
            "Retry failed uploads",
            "--requester",
            "Sam",
            "--json",
        ],
    );
    assert_eq!(r.code, 0, "stderr: {}", r.err);
    let doc: serde_json::Value = serde_json::from_str(&r.out).expect("a JSON document");
    assert_eq!(doc["shape"], "thruline.change.new");
    assert_eq!(doc["schema_version"], 1);
    let body = &doc;
    assert_eq!(body["change"], changes(&db)[0].0);
    assert_eq!(body["project"], PROJECT);
    assert_eq!(body["title"], "Retry failed uploads");
    assert_eq!(body["requester"], "Sam");
    assert_eq!(body["source"], "person");
    assert_eq!(body["said"]["slug"], "change_opened");
    assert_ne!(
        body["said"]["plain"], body["said"]["technical"],
        "the technical register repeats the plain one, which is not a second level"
    );
}

/// `change new` is normally run from the repository the person selected. That
/// explicit root must win over Claude's lossy project-folder spelling.
#[test]
fn a_hyphenated_project_folder_binds_from_the_cli_working_directory() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let repo = dir.path().join("retry-delivery");
    std::fs::create_dir_all(&repo).expect("mkdir");
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    git(&["init", "-q"]);
    std::fs::write(repo.join("README.md"), "retry\n").expect("write");
    git(&["add", "."]);
    git(&["commit", "-qm", "seed"]);

    let db = dir.path().join("ledger.db");
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES(?1,?2,'claude_code','Retry','2026-01-01T00:00:00Z')",
            params![
                PROJECT,
                dir.path().join("retry/delivery").display().to_string()
            ],
        )
        .expect("project");

    let out = Command::new(bin())
        .current_dir(&repo)
        .arg("--db")
        .arg(&db)
        .args([
            "change",
            "new",
            "--root",
            repo.to_str().expect("utf8"),
            "--title",
            "Retry",
            "--requester",
            "Sam",
            "--json",
        ])
        .output()
        .expect("run change new");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert!(body["bound_inputs"].as_str().is_some(), "body: {body}");
    let change = body["change"].as_str().expect("change id");
    let canonical_repo = repo.canonicalize().expect("canonical repo");
    let record = thruline_core::examined_inputs::standing(
        &ledger,
        thruline_core::examined_inputs::SubjectKind::Intended,
        change,
    )
    .expect("standing")
    .expect("bound record");
    assert_eq!(
        record.worktree_path.value(),
        Some(canonical_repo.to_str().expect("utf8"))
    );
}

/// A process cwd is ambient state, not evidence that it belongs to the project
/// selected by `--for`. Without a captured root or `--root`, it must stay
/// unbound rather than attach an unrelated checkout.
#[test]
fn an_unrelated_cli_working_directory_does_not_bind_the_selected_project() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let unrelated = dir.path().join("unrelated-repository");
    std::fs::create_dir_all(&unrelated).expect("mkdir");
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(&unrelated)
            .args(args)
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    git(&["init", "-q"]);
    std::fs::write(unrelated.join("README.md"), "unrelated\n").expect("write");
    git(&["add", "."]);
    git(&["commit", "-qm", "seed"]);

    let db = dir.path().join("ledger.db");
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES(?1,?2,'claude_code','Retry','2026-01-01T00:00:00Z')",
            params![
                PROJECT,
                dir.path().join("retry/delivery").display().to_string()
            ],
        )
        .expect("project");

    let out = Command::new(bin())
        .current_dir(&unrelated)
        .arg("--db")
        .arg(&db)
        .args([
            "change",
            "new",
            "--for",
            PROJECT,
            "--title",
            "Retry",
            "--requester",
            "Sam",
            "--json",
        ])
        .output()
        .expect("run change new");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert!(body["bound_inputs"].is_null(), "body: {body}");
    let change = body["change"].as_str().expect("change id");
    assert!(
        thruline_core::examined_inputs::standing(
            &ledger,
            thruline_core::examined_inputs::SubjectKind::Intended,
            change,
        )
        .expect("standing")
        .is_none(),
        "the unrelated cwd was stored as this Change's worktree"
    );
}

/// A person can explicitly select a sibling worktree, and the bound identity
/// remains that worktree's identity instead of silently collapsing to the
/// primary checkout that shares its repository.
#[test]
fn an_explicit_sibling_worktree_keeps_its_own_workspace_identity() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let primary = dir.path().join("primary");
    let sibling = dir.path().join("sibling-worktree");
    std::fs::create_dir_all(&primary).expect("mkdir");
    let git = |cwd: &Path, args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    git(&primary, &["init", "-q", "-b", "main"]);
    std::fs::write(primary.join("README.md"), "primary\n").expect("write");
    git(&primary, &["add", "."]);
    git(&primary, &["commit", "-qm", "seed"]);
    git(
        &primary,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "sibling",
            sibling.to_str().expect("utf8"),
        ],
    );

    let db = dir.path().join("ledger.db");
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES(?1,?2,'claude_code','Retry','2026-01-01T00:00:00Z')",
            params![
                PROJECT,
                dir.path().join("retry/delivery").display().to_string()
            ],
        )
        .expect("project");

    let out = Command::new(bin())
        .current_dir(&sibling)
        .arg("--db")
        .arg(&db)
        .args([
            "change",
            "new",
            "--for",
            PROJECT,
            "--root",
            sibling.to_str().expect("utf8"),
            "--title",
            "Retry",
            "--requester",
            "Sam",
            "--json",
        ])
        .output()
        .expect("run change new");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let change = body["change"].as_str().expect("change id");
    let record = thruline_core::examined_inputs::standing(
        &ledger,
        thruline_core::examined_inputs::SubjectKind::Intended,
        change,
    )
    .expect("standing")
    .expect("bound record");
    let sibling_identity = match thruline_core::workspace_identity::observe(
        &sibling,
        thruline_core::workspace_identity::Scope::everything(),
    )
    .expect("sibling identity")
    {
        thruline_core::workspace_identity::Observation::Complete(fingerprint) => {
            fingerprint.identity().to_string()
        }
        other => panic!("incomplete sibling observation: {other:?}"),
    };
    let primary_identity = match thruline_core::workspace_identity::observe(
        &primary,
        thruline_core::workspace_identity::Scope::everything(),
    )
    .expect("primary identity")
    {
        thruline_core::workspace_identity::Observation::Complete(fingerprint) => {
            fingerprint.identity().to_string()
        }
        other => panic!("incomplete primary observation: {other:?}"),
    };
    assert_eq!(
        record.source_identity.value(),
        Some(sibling_identity.as_str())
    );
    assert_ne!(
        record.source_identity.value(),
        Some(primary_identity.as_str())
    );
    assert_eq!(
        record.worktree_path.value(),
        Some(
            sibling
                .canonicalize()
                .expect("canonical sibling")
                .to_str()
                .expect("utf8")
        )
    );
}
