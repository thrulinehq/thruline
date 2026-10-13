//! T-3-2b — the ordinary path, through the compiled binary and nothing else.
//!
//! What the core tests cannot show: that a person who types the three commands
//! in order ends up with a Change that covers folders, is bound to a version,
//! and has the run they made attached to what counts as done — with no SQL, no
//! hand-inserted row and no second verb. Every wall T-3-2 hit was invisible
//! from inside the engine and obvious the moment somebody typed the commands.
//!
//! A real temporary git repository, a real store under a temporary HOME, the
//! real binary. No real store is opened.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

fn git(cwd: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(home: &Path, cwd: &Path, db: &Path, args: &[&str]) -> Run {
    let out = Command::new(bin())
        .env("HOME", home)
        .env("THRULINE_HOME", home.join(".thruline"))
        .current_dir(cwd)
        .arg("--db")
        .arg(db)
        .args(args)
        .output()
        .expect("run thruline");
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// A repository with one commit, and a store whose one project points at it
/// and holds one recorded session that edited a file in it.
///
/// The project's root is the repository, because that is what a real
/// registration writes: the Change's own folder is where a check runs.
fn fixture(base: &Path) -> (PathBuf, PathBuf) {
    let repo = base.join("repo");
    std::fs::create_dir_all(repo.join("src/upload")).expect("mkdir");
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("src/upload/retry.rs"), "// retry\n").expect("write");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "seed"]);

    // A real transcript, because the captured `cwd` inside it is the only
    // thing that establishes this project's folder (T-3-2b). `projects.root_path`
    // for a Claude Code project is derived from the vendor's lossy directory
    // key and is deliberately not evidence of a root, so a fixture that wrote
    // only the row would be testing a path no real store can reach.
    std::fs::write(
        repo.join("s.jsonl"),
        format!("{{\"type\":\"user\",\"cwd\":\"{}\"}}\n", repo.display()),
    )
    .expect("transcript");

    let db = base.join("ledger.db");
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES('cccccccc-0000-4000-8000-00000000p001', ?1, 'claude_code', 'Synthetic',
                    '2026-01-01T00:00:00Z')",
            rusqlite::params![repo.display().to_string()],
        )
        .expect("project");
    ledger
        .conn()
        .execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES('cccccccc-0000-4000-8000-00000000s001',
                    'cccccccc-0000-4000-8000-00000000p001', ?1, '2026-01-02T09:30:00Z')",
            rusqlite::params![repo.join("s.jsonl").display().to_string()],
        )
        .expect("session");
    ledger
        .conn()
        .execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                 tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES('cccccccc-ev-1','cccccccc-0000-4000-8000-00000000p001',
                    'cccccccc-0000-4000-8000-00000000s001',1,'2026-01-02T09:31:00Z',
                    'tool_use','Edit', ?1, ?2, 1, 'x', '', '')",
            rusqlite::params![
                format!(
                    "{{\"file_path\":\"{}\"}}",
                    repo.join("src/upload/retry.rs").display()
                ),
                repo.join("s.jsonl").display().to_string()
            ],
        )
        .expect("event");
    (repo, db)
}

/// The id `change new` printed, which is the only place a person reads one.
fn change_id(out: &Run) -> String {
    out.stdout
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("chg_"))
        .unwrap_or_else(|| panic!("no Change id was printed: {}", out.stdout))
        .to_string()
}

/// The whole ordinary path: open a Change, confirm the work, accept what
/// counts as done and what it covers, run a check for it.
///
/// Every assertion below is on the store the commands left behind, read with
/// no help from any of them.
#[test]
fn the_ordinary_path_ends_with_a_run_attached_to_what_counts_as_done() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path());
    let home = dir.path();

    let opened = run(
        home,
        &repo,
        &db,
        &[
            "change",
            "new",
            "--title",
            "Retry failed uploads",
            "--purpose",
            "The test suite passes on the packages that changed.",
            "--requester",
            "sam",
        ],
    );
    assert_eq!(
        opened.code,
        0,
        "{opened:?}",
        opened = (&opened.stdout, &opened.stderr)
    );
    let change = change_id(&opened);

    // Opening a Change binds it to the folder as it stands: `applies` is never
    // null for a Change made on this build (wall 3).
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    assert!(
        thruline_core::examined_inputs::standing(
            &ledger,
            thruline_core::examined_inputs::SubjectKind::Intended,
            &change,
        )
        .expect("read")
        .is_some(),
        "opening a Change wrote nothing about what it is about"
    );
    drop(ledger);

    let confirmed = run(
        home,
        &repo,
        &db,
        &[
            "change",
            "associate",
            &change,
            "cccccccc-0000-4000-8000-00000000s001",
            "--confirm",
            "--actor",
            "sam",
            "--reason",
            "this is the retry work",
        ],
    );
    assert_eq!(confirmed.code, 0, "{}", confirmed.stderr);

    // Accepting writes the folders as well as the lines (wall 2), and says so.
    let accepted = run(
        home,
        &repo,
        &db,
        &[
            "change",
            "accept",
            &change,
            "--all",
            "--actor",
            "sam",
            "--rationale",
            "this is what I want checked",
        ],
    );
    assert_eq!(accepted.code, 0, "{}", accepted.stderr);
    assert!(
        accepted.stdout.contains("src/upload/**"),
        "accepting never said what the Change covers: {}",
        accepted.stdout
    );
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    let allowed: String = ledger
        .conn()
        .query_row(
            "SELECT allowed_scope_json FROM change_scope_revisions WHERE change_id=?1
              ORDER BY scope_version DESC LIMIT 1",
            rusqlite::params![change],
            |r| r.get(0),
        )
        .expect("scope");
    assert_eq!(allowed, "[\"src/upload/**\"]", "{allowed}");
    drop(ledger);

    // One check, run for this Change (wall 1). The command is the person's own
    // and its exit code passes through untouched.
    let checked = run(
        home,
        &repo,
        &db,
        &[
            "check",
            "--for-change",
            &change,
            "--actor",
            "sam",
            "--",
            "true",
        ],
    );
    assert_eq!(checked.code, 0, "{}", checked.stderr);
    assert!(
        checked.stderr.contains("recorded for Change"),
        "the wrapper never said where it recorded the run: {}",
        checked.stderr
    );

    // And the Change can see it, with no manual step: the run is a candidate,
    // its session is counted, and `applies` was asked of it.
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    let at = "2026-02-01T12:00:00Z";
    thruline_core::change_evidence::derive_links(&ledger, &change, at).expect("derive");
    let view = thruline_core::change_evidence::coverage(
        &ledger,
        &change,
        &thruline_core::change_propose::machine_environment(&ledger, &change).expect("env"),
    )
    .expect("coverage");
    assert_eq!(view.nothing_bound_slug, None, "{view:?}");
    assert!(
        !view.candidates.is_empty(),
        "the run the person just made reached nothing: {view:?}"
    );
    for criterion in &view.criteria {
        for item in &criterion.items {
            assert!(
                item.applies.is_some(),
                "the question was not put for {}",
                item.evidence_id
            );
        }
    }
}

/// A check run with no Change named is recorded and attached to nothing — the
/// behaviour every existing use of this verb depends on, unchanged.
#[test]
fn a_check_run_with_no_change_named_is_recorded_and_attached_to_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path());
    let out = run(dir.path(), &repo, &db, &["check", "--", "true"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        !out.stderr.contains("recorded for Change"),
        "a run for no Change claimed a Change: {}",
        out.stderr
    );
    let conn = rusqlite::Connection::open(&db).expect("open");
    let bindings: i64 = conn
        .query_row("SELECT COUNT(*) FROM evidence_bindings", [], |r| r.get(0))
        .expect("count");
    assert_eq!(bindings, 1, "the capture was not recorded");
    let checks: i64 = conn
        .query_row("SELECT COUNT(*) FROM change_check_sessions", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(checks, 0, "a run for no Change wrote a check session");
    let links: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM change_session_associations",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(links, 0, "a run for no Change linked itself to something");
}

/// A Change this store does not hold is refused BEFORE the command runs: the
/// person's command is not spent on a run that could not be recorded where
/// they asked for it.
#[test]
fn a_check_for_a_change_that_does_not_exist_refuses_before_the_command_runs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path());
    let marker = repo.join("the-command-ran");
    let out = run(
        dir.path(),
        &repo,
        &db,
        &[
            "check",
            "--for-change",
            "chg_nothing",
            "--",
            "touch",
            marker.to_str().expect("path"),
        ],
    );
    assert_eq!(out.code, 125, "{out:?}", out = (&out.stdout, &out.stderr));
    assert!(
        !marker.exists(),
        "the command ran after the wrapper refused to record it"
    );
    assert!(
        out.stderr.contains("no Change with id"),
        "the refusal did not say what was wrong: {}",
        out.stderr
    );
    let conn = rusqlite::Connection::open(&db).expect("open");
    let bindings: i64 = conn
        .query_row("SELECT COUNT(*) FROM evidence_bindings", [], |r| r.get(0))
        .expect("count");
    assert_eq!(bindings, 0, "a refused run was recorded anyway");
}
