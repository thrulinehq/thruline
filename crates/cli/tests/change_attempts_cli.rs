//! T-4-4-ATTEMPT-BUDGET (R-2026-09-28-43; Q14 C8) — a person sets how many
//! repair attempts a Change is allowed; ThruLine counts them from recorded runs
//! and stops the next run when they are spent. Through the compiled binary.
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

#[derive(Debug)]
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
/// through a recorded session's own `cwd` (the fixture `check_for_change_cli.rs`
/// uses, for the reason it gives).
fn fixture(base: &Path) -> (PathBuf, PathBuf) {
    let repo = base.join("repo");
    std::fs::create_dir_all(repo.join("src")).expect("mkdir");
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("src/lib.rs"), "// retry\n").expect("write");
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
             VALUES('cccccccc-0000-4000-8000-0000000000a4', ?1, 'claude_code', 'Synthetic',
                    '2026-01-01T00:00:00Z')",
            rusqlite::params![repo.display().to_string()],
        )
        .expect("project");
    ledger
        .conn()
        .execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES('cccccccc-0000-4000-8000-0000000000b4',
                    'cccccccc-0000-4000-8000-0000000000a4', ?1, '2026-01-02T09:30:00Z')",
            rusqlite::params![repo.join("s.jsonl").display().to_string()],
        )
        .expect("session");
    ledger
        .conn()
        .execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                 tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES('cccccccc-ev-t44','cccccccc-0000-4000-8000-0000000000a4',
                    'cccccccc-0000-4000-8000-0000000000b4',1,'2026-01-02T09:31:00Z',
                    'tool_use','Edit', ?1, ?2, 1, 'x', '', '')",
            rusqlite::params![
                format!(
                    "{{\"file_path\":\"{}\"}}",
                    repo.join("src/lib.rs").display()
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

fn open_change(home: &Path, repo: &Path, db: &Path) -> String {
    let opened = run(
        home,
        repo,
        db,
        &[
            "change",
            "new",
            "--title",
            "Retry a failed delivery",
            "--purpose",
            "Two retry requests for the same delivery do not enqueue duplicate work.",
            "--requester",
            "sam",
        ],
    );
    assert_eq!(opened.code, 0, "{opened:?}");
    change_id(&opened)
}

/// A repair attempt that does not fix anything: one more line, committed.
fn repair_attempt(repo: &Path, n: u32) {
    let path = repo.join("src/lib.rs");
    let mut text = std::fs::read_to_string(&path).expect("read");
    text.push_str(&format!("// repair attempt {n}: does not fix it\n"));
    std::fs::write(&path, text).expect("write");
    git(repo, &["commit", "-qam", &format!("repair attempt {n}")]);
}

fn execution_rows(db: &Path, change: &str) -> i64 {
    let conn = rusqlite::Connection::open(db).expect("open");
    conn.query_row(
        "SELECT COUNT(*) FROM change_run_executions WHERE change_id=?1",
        rusqlite::params![change],
        |r| r.get(0),
    )
    .expect("count")
}

fn budget_rows(db: &Path) -> i64 {
    let conn = rusqlite::Connection::open(db).expect("open");
    conn.query_row("SELECT COUNT(*) FROM change_attempt_budgets", [], |r| {
        r.get(0)
    })
    .expect("count")
}

/// The version the newest run of this Change examined, as the record froze it.
fn last_examined(db: &Path, change: &str) -> String {
    let conn = rusqlite::Connection::open(db).expect("open");
    conn.query_row(
        "SELECT examined_inputs_id FROM change_run_executions WHERE change_id=?1
          ORDER BY rowid DESC LIMIT 1",
        rusqlite::params![change],
        |r| r.get(0),
    )
    .expect("a run")
}

fn failing_check(home: &Path, repo: &Path, db: &Path, change: &str) {
    let out = run(
        home,
        repo,
        db,
        &[
            "check",
            "--for-change",
            change,
            "--actor",
            "sam",
            "--",
            "false",
        ],
    );
    assert_eq!(out.code, 1, "the child's own code passes through: {out:?}");
}

/// What the product says about this Change — the Change itself (state words,
/// the decisions and approval), its coverage and its readiness — read with the
/// same functions the served routes call, as JSON text.
fn what_it_claims(db: &Path, change: &str) -> String {
    let ledger = thruline_core::ledger::Ledger::open(db).expect("open");
    let view = thruline_core::change::read_change(&ledger, change)
        .expect("read")
        .expect("the Change");
    let environment =
        thruline_core::change_propose::machine_environment(&ledger, change).expect("env");
    let coverage =
        thruline_core::change_evidence::coverage(&ledger, change, &environment).expect("cov");
    let readiness =
        thruline_core::change_readiness::readiness(&ledger, change, &coverage).expect("ready");
    serde_json::to_string(&serde_json::json!({
        "change": view,
        "coverage": coverage,
        "readiness": readiness,
    }))
    .expect("json")
}

fn the_shown_version(db: &Path, change: &str) -> Option<String> {
    let ledger = thruline_core::ledger::Ledger::open(db).expect("open");
    let environment =
        thruline_core::change_propose::machine_environment(&ledger, change).expect("env");
    let coverage =
        thruline_core::change_evidence::coverage(&ledger, change, &environment).expect("cov");
    thruline_core::change_readiness::readiness(&ledger, change, &coverage)
        .expect("ready")
        .target_version
}

#[test]
fn change_help_lists_an_attempts_verb_with_a_budget() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = Command::new(bin())
        .env("HOME", dir.path())
        .env("THRULINE_HOME", dir.path().join(".thruline"))
        .args(["change", "--help"])
        .output()
        .expect("run");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.lines()
            .any(|l| l.trim_start().starts_with("attempts ")),
        "{text}"
    );
    let out = Command::new(bin())
        .env("HOME", dir.path())
        .env("THRULINE_HOME", dir.path().join(".thruline"))
        .args(["change", "attempts", "--help"])
        .output()
        .expect("run");
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("--budget") && text.contains("--actor"),
        "{text}"
    );
}

/// A Change id the store does not hold is said, loudly, whether a budget is
/// being set or asked about — never answered as "no budget is set".
#[test]
fn an_unknown_change_is_refused_whether_setting_or_asking() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path());
    for args in [
        vec!["change", "attempts", "chg_nothing"],
        vec![
            "change",
            "attempts",
            "chg_nothing",
            "--budget",
            "2",
            "--actor",
            "sam",
        ],
    ] {
        let out = run(dir.path(), &repo, &db, &args);
        assert_eq!(out.code, 1, "{args:?}: {out:?}");
        assert!(
            out.stdout.contains("No Change with id"),
            "{args:?}: {out:?}"
        );
        assert!(!out.stdout.contains("No attempt budget"), "{out:?}");
    }
    assert_eq!(budget_rows(&db), 0);
}

/// Setting a budget is a human act and needs a name.
#[test]
fn a_budget_without_a_name_or_out_of_range_is_refused_and_writes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path());
    let home = dir.path();
    let change = open_change(home, &repo, &db);
    for args in [
        vec!["change", "attempts", &change, "--budget", "2"],
        vec![
            "change", "attempts", &change, "--budget", "0", "--actor", "sam",
        ],
        vec![
            "change", "attempts", &change, "--budget", "100", "--actor", "sam",
        ],
    ] {
        let out = run(home, &repo, &db, &args);
        assert_eq!(out.code, 1, "{args:?}: {out:?}");
        assert!(out.stdout.contains("No budget was saved"), "{out:?}");
    }
    assert_eq!(budget_rows(&db), 0);
}

#[test]
fn a_spent_budget_stops_the_next_new_version_before_anything_spawns() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path());
    let home = dir.path();
    let change = open_change(home, &repo, &db);

    // No budget: nothing limits, and the verb says so.
    let none = run(home, &repo, &db, &["change", "attempts", &change]);
    assert_eq!(none.code, 0, "{none:?}");
    assert!(none.stdout.contains("No attempt budget is set"), "{none:?}");

    // The check that shows the problem, then the budget.
    failing_check(home, &repo, &db, &change);
    let before_budget = what_it_claims(&db, &change);
    let set = run(
        home,
        &repo,
        &db,
        &[
            "change", "attempts", &change, "--budget", "3", "--actor", "sam",
        ],
    );
    assert_eq!(set.code, 0, "{set:?}");
    assert!(
        set.stdout.contains("sam allowed 3 repair attempts"),
        "{set:?}"
    );
    assert_eq!(budget_rows(&db), 1);
    assert_eq!(
        what_it_claims(&db, &change),
        before_budget,
        "setting a budget changes nothing the product claims about the Change"
    );

    // Attempt 1, then a re-run on unchanged content (PW-F11): the Change's
    // shown version may move on each run, but the examined content did not,
    // so it is the same attempt.
    repair_attempt(&repo, 1);
    failing_check(home, &repo, &db, &change);
    let first_examined = last_examined(&db, &change);
    let first_shown = the_shown_version(&db, &change);
    failing_check(home, &repo, &db, &change);
    assert_eq!(
        last_examined(&db, &change),
        first_examined,
        "unchanged content is the same version by content"
    );
    let shown_after_rerun = the_shown_version(&db, &change);
    // At 55b80e99 the shown version moves on every run (observed:
    // 01M3NA9K… then 01M3NA9M…); the count must not care either way, so this
    // says which case the run met rather than pinning PW-F11's defect.
    eprintln!(
        "shown version: {first_shown:?} then {shown_after_rerun:?} (moved: {})",
        first_shown != shown_after_rerun
    );
    let asked = run(home, &repo, &db, &["change", "attempts", &change]);
    assert_eq!(asked.code, 0, "{asked:?}");
    assert!(asked.stdout.contains("1 made, 2 left"), "{asked:?}");
    assert!(
        asked
            .stdout
            .lines()
            .any(|l| l.starts_with("Attempt 1 ") && l.contains(": 2 runs:")),
        "the re-run is listed under the same attempt: {asked:?}"
    );

    // Attempts 2 and 3.
    for n in [2, 3] {
        repair_attempt(&repo, n);
        failing_check(home, &repo, &db, &change);
    }

    let stopped = run(
        home,
        &repo,
        &db,
        &["change", "attempts", &change, "--actor", "sam"],
    );
    assert_eq!(
        stopped.code, 1,
        "a spent budget is not success: {stopped:?}"
    );
    for n in 1..=3 {
        assert!(
            stopped.stdout.contains(&format!("Attempt {n} ")),
            "every attempt is listed: {stopped:?}"
        );
    }
    assert!(!stopped.stdout.contains("Attempt 4"), "{stopped:?}");
    let last = stopped.stdout.lines().last().unwrap_or_default();
    assert!(
        last.contains("All 3 repair attempts sam allowed have been made")
            && last.contains("unresolved"),
        "one sentence, and unresolved because readiness is not Ready: {stopped:?}"
    );
    let spent_claims = what_it_claims(&db, &change);

    // A commit nobody has checked is not an attempt, and the status reads no
    // git: it is byte-identical before and after the commit.
    repair_attempt(&repo, 4);
    let after_commit = run(home, &repo, &db, &["change", "attempts", &change]);
    assert_eq!(after_commit.stdout, stopped.stdout);
    assert_eq!(after_commit.code, 1);

    // The fourth new version: refused before the spawn, no row, exit 125.
    let rows_before = execution_rows(&db, &change);
    let marker = dir.path().join("the-command-ran");
    let refused = run(
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
            "touch",
            marker.to_str().expect("path"),
        ],
    );
    assert_eq!(refused.code, 125, "{refused:?}");
    assert!(
        !marker.exists(),
        "the command ran after the budget was spent"
    );
    assert_eq!(
        execution_rows(&db, &change),
        rows_before,
        "no execution row was written"
    );
    assert!(
        refused.stderr.contains("sam allowed 3 repair attempts")
            && refused.stderr.contains("3 have been made")
            && refused
                .stderr
                .contains(&format!("thruline change attempts {change} --budget")),
        "the refusal says what was allowed, what was used and what a person can do: {refused:?}"
    );
    assert_eq!(
        what_it_claims(&db, &change),
        spent_claims,
        "the stop changes nothing the product claims"
    );

    // A person allows one more: the new version may run again, and setting it
    // claimed nothing.
    let more = run(
        home,
        &repo,
        &db,
        &[
            "change", "attempts", &change, "--budget", "1", "--actor", "kim",
        ],
    );
    assert_eq!(more.code, 0, "{more:?}");
    assert_eq!(what_it_claims(&db, &change), spent_claims);
    let again = run(
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
            "touch",
            marker.to_str().expect("path"),
        ],
    );
    assert_eq!(again.code, 0, "{again:?}");
    assert!(marker.exists());

    // Every run is kept: the first, attempt 1 twice, attempts 2 and 3, and the
    // one after kim's budget. Two rows per run.
    let runs: i64 = rusqlite::Connection::open(&db)
        .expect("open")
        .query_row(
            "SELECT COUNT(DISTINCT run_key) FROM change_run_executions WHERE change_id=?1",
            rusqlite::params![change],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(runs, 6);
}

/// A193: a store written by the base binary is refused by every verb until
/// `thruline upgrade` runs, and the upgrade creates the table.
#[test]
fn every_verb_exits_3_until_the_upgrade_runs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("ledger.db");
    let base = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/tests/fixtures/schema-t-4-4-base.sql"),
    )
    .expect("the base schema fixture");
    let conn = rusqlite::Connection::open(&db).expect("open");
    conn.execute_batch(&base).expect("base schema");
    conn.execute_batch(
        "INSERT INTO projects(id,root_path,source,display_name,created_at)
           VALUES('p-old','/old/project','claude_code','Old','2026-01-01T00:00:00Z');
         INSERT INTO changes(id,project_id,opened_at,title,purpose,requester,request_source)
           VALUES('chg-old','p-old','2026-01-01T00:00:00Z','Old','Kept','sam','person');",
    )
    .expect("rows");
    conn.execute(
        "INSERT INTO schema_migrations(version) VALUES(?1)",
        rusqlite::params![format!(
            "schema_sha256_{}",
            thruline_core::chain::sha256_hex(base.as_bytes())
        )],
    )
    .expect("fingerprint");
    drop(conn);
    let has_table = || -> i64 {
        rusqlite::Connection::open(&db)
            .expect("open")
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='change_attempt_budgets'",
                [],
                |r| r.get(0),
            )
            .expect("count")
    };
    let home = dir.path();
    for args in [
        vec!["change", "attempts", "chg-old"],
        vec![
            "change", "attempts", "chg-old", "--budget", "2", "--actor", "sam",
        ],
        vec!["check", "--for-change", "chg-old", "--", "true"],
    ] {
        let out = run(home, dir.path(), &db, &args);
        assert_eq!(out.code, 3, "{args:?}: {out:?}");
        assert!(out.stderr.contains("schema upgrade pending"), "{out:?}");
        assert_eq!(has_table(), 0, "{args:?} paid for the migration");
    }
    let up = run(home, dir.path(), &db, &["upgrade"]);
    assert_eq!(up.code, 0, "{up:?}");
    assert_eq!(has_table(), 1);
    let set = run(
        home,
        dir.path(),
        &db,
        &[
            "change", "attempts", "chg-old", "--budget", "2", "--actor", "sam",
        ],
    );
    assert_eq!(set.code, 0, "{set:?}");
    assert_eq!(budget_rows(&db), 1);
}

/// T-4-4-ATTEMPT-BUDGET-R3 (P-2026-09-29-3) — the own-files limit, as literal
/// text: a change only to ThruLine's own files spends an attempt until card
/// EXAMINED-INPUTS-OWN-FILES lands, and every door that shows the count says so.
const OWN_FILES_LIMIT: &str = "A change made only to ThruLine's own files in the project \
(under .thruline/) currently counts as a new attempt.";

/// The words, with the help's line breaks and indentation folded to spaces.
fn says_the_limit(text: &str) -> bool {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .contains(OWN_FILES_LIMIT)
}

/// Each door a person reads the count through states the limit: the verb's
/// --help, the verb with attempts left, the verb once they are spent, and the
/// refusal `thruline check --for-change` prints before it spawns nothing.
#[test]
fn every_door_that_shows_the_count_states_the_own_files_limit() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path());
    let home = dir.path();

    let help = run(home, &repo, &db, &["change", "attempts", "--help"]);
    assert_eq!(help.code, 0, "{help:?}");
    assert!(says_the_limit(&help.stdout), "--help: {help:?}");

    let change = open_change(home, &repo, &db);
    let set = run(
        home,
        &repo,
        &db,
        &[
            "change", "attempts", &change, "--budget", "1", "--actor", "sam",
        ],
    );
    assert_eq!(set.code, 0, "{set:?}");
    assert!(says_the_limit(&set.stdout), "setting: {set:?}");
    let left = run(home, &repo, &db, &["change", "attempts", &change]);
    assert_eq!(left.code, 0, "{left:?}");
    assert!(says_the_limit(&left.stdout), "attempts left: {left:?}");

    repair_attempt(&repo, 1);
    failing_check(home, &repo, &db, &change);
    let spent = run(home, &repo, &db, &["change", "attempts", &change]);
    assert_eq!(spent.code, 1, "{spent:?}");
    assert!(says_the_limit(&spent.stdout), "spent: {spent:?}");
    assert!(
        !says_the_limit(spent.stdout.lines().last().unwrap_or_default()),
        "the last line stays the one that says whether another may be made: {spent:?}"
    );

    repair_attempt(&repo, 2);
    let rows_before = execution_rows(&db, &change);
    let refused = run(
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
    assert_eq!(refused.code, 125, "{refused:?}");
    assert_eq!(execution_rows(&db, &change), rows_before, "no row");
    assert!(says_the_limit(&refused.stderr), "refusal: {refused:?}");
}

/// T-4-4-ATTEMPT-BUDGET-R4 — the adversary's "stop after this one", through
/// the compiled binary. A check on a new version is in flight (its `running`
/// row is in the store, written the way the wrapper writes it, so no clock is
/// raced) when a person lowers the budget from 20 to 1; the check then ends.
/// That check is the one attempt: the count says so, and the next check on a
/// new version is refused before it spawns.
#[test]
fn a_budget_lowered_while_a_check_runs_counts_that_check_and_stops_the_next() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path());
    let home = dir.path();
    let change = open_change(home, &repo, &db);
    let set20 = run(
        home,
        &repo,
        &db,
        &[
            "change", "attempts", &change, "--budget", "20", "--actor", "walker",
        ],
    );
    assert_eq!(set20.code, 0, "{set20:?}");

    let row = |id: &str, state: &str| {
        rusqlite::Connection::open(&db)
            .expect("open")
            .execute(
                "INSERT INTO change_run_executions(id,run_key,change_id,name,state,pid,
                     process_group,started_at,examined_inputs_id,recorded_at)
                 VALUES(?1,'run-in-flight',?2,'check',?3,1,1,'2026-01-02T00:00:00Z',
                        'exi_in_flight','2026-01-02T00:00:00Z')",
                rusqlite::params![id, change, state],
            )
            .expect("execution row");
    };
    row("rx_in_flight", "running");
    let set1 = run(
        home,
        &repo,
        &db,
        &[
            "change",
            "attempts",
            &change,
            "--budget",
            "1",
            "--actor",
            "supervisor",
        ],
    );
    assert_eq!(set1.code, 0, "{set1:?}");
    row("rx_in_flight_end", "completed");

    let asked = run(home, &repo, &db, &["change", "attempts", &change]);
    assert_eq!(asked.code, 1, "one allowed, one made: spent: {asked:?}");
    assert!(
        asked
            .stdout
            .lines()
            .any(|l| l.starts_with("Attempt 1 ") && l.contains("exi_in_flight")),
        "the check that was running is attempt 1: {asked:?}"
    );
    assert!(
        asked
            .stdout
            .contains("The one repair attempt supervisor allowed has been made"),
        "{asked:?}"
    );

    let rows_before = execution_rows(&db, &change);
    let marker = dir.path().join("the-command-ran");
    let refused = run(
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
            "touch",
            marker.to_str().expect("path"),
        ],
    );
    assert_eq!(refused.code, 125, "{refused:?}");
    assert!(!marker.exists(), "a second check ran past a budget of 1");
    assert_eq!(execution_rows(&db, &change), rows_before);
    assert!(
        refused
            .stderr
            .contains("supervisor allowed 1 repair attempt"),
        "{refused:?}"
    );
}
