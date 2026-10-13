//! T-3-6-F1 (`Q1`) — a check that ran and passed is shown as a check that ran
//! and passed, through the compiled binary and nothing else.
//!
//! The defect this pins was invisible from inside the engine: two real test
//! runs were recorded for a Change, both exited 0, both were listed under
//! "Every run", and the accepted line they bore on still read "Something ran
//! for this, and it checked nothing." The store held the exit code and threw
//! the run's own count away, so nothing in it could tell a green suite from a
//! suite that ran zero tests.
//!
//! Everything below goes through the person's own verbs, against a real
//! temporary store under a temporary HOME. The "test runner" is a two-line
//! script named `pytest` on this test's own PATH: it is a bare test-runner
//! phrase, so the wrapper classifies the run as a test the same way it
//! classifies `cargo test`, and it prints exactly what libtest prints. No real
//! store is opened and nothing is compiled.

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

fn run(home: &Path, cwd: &Path, db: &Path, path_extra: &Path, args: &[&str]) -> Run {
    let path = format!(
        "{}:{}",
        path_extra.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new(bin())
        .env("HOME", home)
        .env("THRULINE_HOME", home.join(".thruline"))
        .env("PATH", path)
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

/// A fake test runner that prints what libtest prints and exits 0.
///
/// `body` is the `test result:` line it writes, or nothing at all — the second
/// shape is the run that exits 0 having checked nothing, which must keep
/// reading exactly as it always has.
fn runner(dir: &Path, body: &str) -> PathBuf {
    let bin_dir = dir.join("bin");
    std::fs::create_dir_all(&bin_dir).expect("mkdir");
    let script = bin_dir.join("pytest");
    std::fs::write(&script, format!("#!/bin/sh\n{body}\nexit 0\n")).expect("write runner");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    bin_dir
}

/// A repository with one commit and a store whose one project points at it,
/// holding one recorded session that asked for the work and edited a file.
fn fixture(base: &Path) -> (PathBuf, PathBuf) {
    let repo = base.join("repo");
    std::fs::create_dir_all(repo.join("src/upload")).expect("mkdir");
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("src/upload/retry.rs"), "// retry\n").expect("write");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "seed"]);
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
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, role, text,
                 raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES('cccccccc-ev-0','cccccccc-0000-4000-8000-00000000p001',
                    'cccccccc-0000-4000-8000-00000000s001',1,'2026-01-02T09:30:00Z',
                    'user_msg','user','Two retry requests for the same delivery do not
                     enqueue duplicate work.', ?1, 1, 'x', '', '')",
            rusqlite::params![repo.join("s.jsonl").display().to_string()],
        )
        .expect("ask");
    ledger
        .conn()
        .execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                 tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES('cccccccc-ev-1','cccccccc-0000-4000-8000-00000000p001',
                    'cccccccc-0000-4000-8000-00000000s001',2,'2026-01-02T09:31:00Z',
                    'tool_use','Edit', ?1, ?2, 2, 'x', '', '')",
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

fn change_id(out: &Run) -> String {
    out.stdout
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("chg_"))
        .unwrap_or_else(|| panic!("no Change id was printed: {}", out.stdout))
        .to_string()
}

/// Open a Change, confirm the work, accept one test-worded line, and hand back
/// everything a check needs.
fn up_to_accepted(home: &Path, repo: &Path, db: &Path, path_extra: &Path) -> String {
    let opened = run(
        home,
        repo,
        db,
        path_extra,
        &[
            "change",
            "new",
            "--title",
            "Retry failed uploads",
            "--purpose",
            "Two retry requests for the same delivery do not enqueue duplicate work.",
            "--requester",
            "sam",
        ],
    );
    assert_eq!(opened.code, 0, "{}", opened.stderr);
    let change = change_id(&opened);

    let confirmed = run(
        home,
        repo,
        db,
        path_extra,
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

    let proposed = run(home, repo, db, path_extra, &["change", "propose", &change]);
    assert_eq!(proposed.code, 0, "{}", proposed.stderr);

    // The accepted line is the person's own words, and it is test-worded, so
    // the rule's `declared` basis can reach a test-class run for it.
    let accepted = run(
        home,
        repo,
        db,
        path_extra,
        &[
            "change",
            "accept",
            &change,
            "--criterion",
            "1",
            "--edit",
            "1=The duplicate-retry test passes.",
            "--scope",
            "src/**",
            "--actor",
            "sam",
            "--rationale",
            "this is what I want checked",
        ],
    );
    assert_eq!(accepted.code, 0, "{}", accepted.stderr);
    change
}

/// What the coverage layer says about the one accepted line.
fn state_of(db: &Path, change: &str) -> (String, String) {
    let ledger = thruline_core::ledger::Ledger::open(db).expect("open");
    thruline_core::change_evidence::derive_links(&ledger, change, "2026-02-01T12:00:00Z")
        .expect("derive");
    let view = thruline_core::change_evidence::coverage(
        &ledger,
        change,
        &thruline_core::change_propose::ProfileEnvironment::default(),
    )
    .expect("coverage");
    let first = view.criteria.first().expect("one accepted line");
    let observed = first
        .items
        .first()
        .map(|i| format!("{:?}", i.observed))
        .unwrap_or_else(|| "no item".to_string());
    (format!("{:?}", first.state), observed)
}

/// **The defect, as a test.** A test-class run that exited 0 and printed a
/// count of passes is evidence for the line it covers.
#[test]
fn a_run_that_counted_passes_is_evidence_for_the_line_it_covers() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path());
    let home = dir.path();
    let bin_dir = runner(
        dir.path(),
        "echo 'test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out'",
    );

    let change = up_to_accepted(home, &repo, &db, &bin_dir);
    let checked = run(
        home,
        &repo,
        &db,
        &bin_dir,
        &[
            "check",
            "--for-change",
            &change,
            "--actor",
            "sam",
            "--",
            "pytest",
        ],
    );
    assert_eq!(checked.code, 0, "{}", checked.stderr);
    assert!(
        checked.stderr.contains("recorded for Change"),
        "the wrapper never said where it recorded the run: {}",
        checked.stderr
    );

    // The count is on record as a typed field, read off the stream the wrapper
    // captured — never off the exit code.
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    let tallies = thruline_core::test_tally::by_binding(&ledger).expect("tallies");
    let tally = tallies.values().next().copied().expect("one tally");
    assert_eq!(tally.passed, 3, "{tallies:?}");
    assert_eq!(tally.failed, 0);
    assert_eq!(tally.suites, 1);
    assert!(tally.counts_a_suite());
    drop(ledger);

    let (state, observed) = state_of(&db, &change);
    assert_eq!(state, "Evidence", "the accepted line still reads {state}");
    assert_eq!(
        observed, "Proven",
        "the run under it still reads {observed}"
    );
}

/// **The refusal that stays.** A zero exit with no count in it is still a run
/// that checked nothing, in those words. This is the half of `Q3` the fix is
/// not allowed to spend.
#[test]
fn a_zero_exit_with_no_count_still_checked_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path());
    let home = dir.path();
    let bin_dir = runner(dir.path(), "echo 'nothing to do here'");

    let change = up_to_accepted(home, &repo, &db, &bin_dir);
    let checked = run(
        home,
        &repo,
        &db,
        &bin_dir,
        &[
            "check",
            "--for-change",
            &change,
            "--actor",
            "sam",
            "--",
            "pytest",
        ],
    );
    assert_eq!(checked.code, 0, "{}", checked.stderr);

    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    assert!(
        thruline_core::test_tally::by_binding(&ledger)
            .expect("tallies")
            .is_empty(),
        "a run with no count in it must store no count"
    );
    drop(ledger);

    let (state, observed) = state_of(&db, &change);
    assert_eq!(
        state, "RanNothing",
        "an uncounted zero exit now reads {state}"
    );
    assert_eq!(observed, "Couldnt");
}

/// A suite that ran and passed nothing — every test filtered out — is not a
/// pass either. `0 passed; 0 failed` is a real libtest line and it settles
/// nothing.
#[test]
fn a_suite_that_ran_no_tests_is_not_a_pass() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path());
    let home = dir.path();
    let bin_dir = runner(
        dir.path(),
        "echo 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 12 filtered out'",
    );

    let change = up_to_accepted(home, &repo, &db, &bin_dir);
    let checked = run(
        home,
        &repo,
        &db,
        &bin_dir,
        &[
            "check",
            "--for-change",
            &change,
            "--actor",
            "sam",
            "--",
            "pytest",
        ],
    );
    assert_eq!(checked.code, 0, "{}", checked.stderr);

    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    let tallies = thruline_core::test_tally::by_binding(&ledger).expect("tallies");
    let tally = tallies.values().next().copied().expect("one tally");
    assert_eq!(tally.passed, 0);
    assert!(
        !tally.counts_a_suite(),
        "zero passes cannot count as a suite"
    );
    drop(ledger);

    let (state, observed) = state_of(&db, &change);
    assert_eq!(state, "RanNothing");
    assert_eq!(observed, "Couldnt");
}

/// **T-3-6-F3 (`Q2`, `B16.8`, `B16.10`), through the compiled binary.**
///
/// A failing test run, recorded with a declared evidence class, must reach the
/// served coverage document carrying BOTH the failing assertion's own words
/// and the class — because a person reads them beside the finding and nowhere
/// else. Nothing here is derived from a branch name: the class is the word the
/// person running the check declared, and `--evidence-class` is the only way
/// it gets into the store.
#[test]
fn a_failing_run_declared_seeded_serves_its_words_and_its_class() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path());
    let home = dir.path();
    // A test runner that fails exactly the way libtest fails, and exits 101.
    let bin_dir = dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir).expect("mkdir");
    let script = bin_dir.join("pytest");
    std::fs::write(
        &script,
        "#!/bin/sh\n\
         echo \"thread 'two_retry_requests' panicked at tests/duplicate_retry.rs:28:5:\"\n\
         echo 'assertion `left == right` failed: a second retry request while one is pending \
         must return the same job, not a new one'\n\
         echo '  left: \"job_0002\"'\n\
         echo ' right: \"job_0003\"'\n\
         echo 'note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace'\n\
         echo 'test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out'\n\
         exit 101\n",
    )
    .expect("write runner");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    let change = up_to_accepted(home, &repo, &db, &bin_dir);
    let checked = run(
        home,
        &repo,
        &db,
        &bin_dir,
        &[
            "check",
            "--for-change",
            &change,
            "--actor",
            "sam",
            "--evidence-class",
            "seeded",
            "--",
            "pytest",
        ],
    );
    assert_eq!(
        checked.code, 101,
        "the wrapper passes the exit code through"
    );

    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    thruline_core::change_evidence::derive_links(&ledger, &change, "2026-02-01T12:00:00Z")
        .expect("derive");
    let view = thruline_core::change_evidence::coverage(
        &ledger,
        &change,
        &thruline_core::change_propose::ProfileEnvironment::default(),
    )
    .expect("coverage");
    let item = view
        .criteria
        .iter()
        .flat_map(|c| c.items.iter())
        .find(|i| i.kind == thruline_core::change_evidence::EvidenceKind::CheckRun)
        .expect("the failing run is linked to the accepted line");

    let line = item
        .failure_line
        .as_deref()
        .expect("the failing assertion's own words are served");
    assert!(
        line.contains("must return the same job, not a new one"),
        "{line}"
    );
    assert!(
        line.contains("job_0002") && line.contains("job_0003"),
        "{line}"
    );
    assert!(!line.contains("RUST_BACKTRACE"), "{line}");
    assert_eq!(
        item.evidence_class,
        Some(thruline_core::run_record::EvidenceClass::Seeded),
        "B16.10: the declared class is served wherever the finding is shown"
    );
    assert_eq!(item.evidence_class_slug, Some("evidence_class_seeded"));
}

/// A class word this product cannot render is refused before anything runs,
/// never coerced to one of the four and never stored as itself.
#[test]
fn an_unknown_evidence_class_is_refused_and_nothing_runs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path());
    let home = dir.path();
    let bin_dir = runner(dir.path(), "echo 'test result: ok. 1 passed; 0 failed'");
    let change = up_to_accepted(home, &repo, &db, &bin_dir);
    let refused = run(
        home,
        &repo,
        &db,
        &bin_dir,
        &[
            "check",
            "--for-change",
            &change,
            "--actor",
            "sam",
            "--evidence-class",
            "ordinary",
            "--",
            "pytest",
        ],
    );
    assert_ne!(refused.code, 0, "{}", refused.stdout);
    assert!(
        refused.stderr.contains("is not a class we record"),
        "{}",
        refused.stderr
    );
    assert!(refused.stderr.contains("seeded"), "{}", refused.stderr);
}

/* ── C30 R-RUN: pytest's own summary is counted ───────────────────────── */

/// A `pytest` on this test's PATH that prints one real pytest capture
/// (`tests/fixtures/pytest/<name>.txt`, pytest 9.0.1's own output; see the
/// note in `check.rs`'s `pytest_tally_and_live_output`) and exits as pytest
/// did for it.
fn pytest_capture(dir: &Path, name: &str, exit: i32) -> PathBuf {
    let capture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pytest")
        .join(format!("{name}.txt"));
    assert!(capture.is_file(), "{}", capture.display());
    runner_exiting(dir, &format!("cat '{}'", capture.display()), exit)
}

/// [`runner`], exiting `exit`.
fn runner_exiting(dir: &Path, body: &str, exit: i32) -> PathBuf {
    let bin_dir = dir.join("bin");
    std::fs::create_dir_all(&bin_dir).expect("mkdir");
    let script = bin_dir.join("pytest");
    std::fs::write(&script, format!("#!/bin/sh\n{body}\nexit {exit}\n")).expect("write runner");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    bin_dir
}

fn checked_with(dir: &Path, bin_dir: &Path) -> (PathBuf, String, Run) {
    let (repo, db) = fixture(dir);
    let change = up_to_accepted(dir, &repo, &db, bin_dir);
    let checked = run(
        dir,
        &repo,
        &db,
        bin_dir,
        &[
            "check",
            "--for-change",
            &change,
            "--actor",
            "sam",
            "--",
            "pytest",
        ],
    );
    (db, change, checked)
}

/// **The pass form, for pytest.** A real passing pytest session is recorded
/// with pytest's own count, and is evidence for the line it covers — exactly
/// as libtest's count is.
#[test]
fn a_passing_pytest_run_is_counted_from_its_own_summary() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bin_dir = pytest_capture(dir.path(), "pass_default", 0);
    let (db, change, checked) = checked_with(dir.path(), &bin_dir);
    assert_eq!(checked.code, 0, "{}", checked.stderr);
    assert!(
        checked.stdout.contains("5 passed in"),
        "the run's own output is passed through untouched: {}",
        checked.stdout
    );

    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    let tallies = thruline_core::test_tally::by_binding(&ledger).expect("tallies");
    let tally = tallies.values().next().copied().expect("one tally");
    assert_eq!(
        (tally.passed, tally.failed, tally.ignored, tally.suites),
        (5, 0, 0, 1),
        "{tallies:?}"
    );
    assert!(tally.counts_a_suite());
    drop(ledger);

    let (state, observed) = state_of(&db, &change);
    assert_eq!(state, "Evidence", "the accepted line still reads {state}");
    assert_eq!(observed, "Proven");
}

/// **The fail form, for pytest.** A real failing session — one failure, one
/// setup error, one skip, an xfail, an xpass and a warning — is recorded as
/// three passed and two failed, with pytest's exit status passed through.
#[test]
fn a_failing_pytest_run_counts_its_failures_and_errors() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bin_dir = pytest_capture(dir.path(), "mixed_default", 1);
    let (db, _change, checked) = checked_with(dir.path(), &bin_dir);
    assert_eq!(checked.code, 1, "the wrapper passes the exit code through");

    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    let tallies = thruline_core::test_tally::by_binding(&ledger).expect("tallies");
    let tally = tallies.values().next().copied().expect("one tally");
    assert_eq!(
        (tally.passed, tally.failed, tally.ignored, tally.suites),
        (3, 2, 1, 1),
        "{tallies:?}"
    );
    assert!(!tally.counts_a_suite());
}

/// **Never invent a count.** `pytest -qq` prints no summary; the run is
/// recorded with no count at all — never zero — and checked nothing.
#[test]
fn a_pytest_run_that_printed_no_summary_stores_no_count() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bin_dir = pytest_capture(dir.path(), "pass_qq", 0);
    let (db, change, checked) = checked_with(dir.path(), &bin_dir);
    assert_eq!(checked.code, 0, "{}", checked.stderr);

    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    assert!(
        thruline_core::test_tally::by_binding(&ledger)
            .expect("tallies")
            .is_empty(),
        "a run with no summary in it must store no count"
    );
    drop(ledger);
    let (state, observed) = state_of(&db, &change);
    assert_eq!(state, "RanNothing");
    assert_eq!(observed, "Couldnt");
}
