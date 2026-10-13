//! T-3-4 (`B7.1`, `B7.6`, `B12.6`, `Q12`) — stopping a running check from a
//! terminal, through the compiled binary and nothing else.
//!
//! What the in-process tests cannot show: that a person who types
//! `thruline change cancel-run --run <id>` gets the check stopped, is told so
//! in their own words, and ends up with a record that names them — and that
//! the check's whole process tree, grandchild included, is **verified gone**
//! rather than assumed gone.
//!
//! A real child in a process group of its own, a real store under a temporary
//! HOME, the real binary. No real store is opened.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

struct Run {
    code: i32,
    stdout: String,
}

fn run(home: &Path, db: &Path, args: &[&str]) -> Run {
    let out = Command::new(bin())
        .env("HOME", home)
        .env("THRULINE_HOME", home.join(".thruline"))
        .arg("--db")
        .arg(db)
        .args(args)
        .output()
        .expect("run thruline");
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
    }
}

/// How many processes are left in a group, read the way the card asks a
/// verifier to read it.
/// The wrapper refuses to record an execution against a workspace with no
/// repository identity, so a test that runs it needs a real one.
fn a_repository() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("work");
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "t@t"],
        vec!["config", "user.name", "t"],
    ] {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(&args)
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}");
    }
    std::fs::write(dir.path().join("a.txt"), "a\n").expect("write");
    for args in [vec!["add", "-A"], vec!["commit", "-qm", "one"]] {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(&args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}");
    }
    dir
}

fn survivors(pgid: u32) -> usize {
    let out = Command::new("pgrep")
        .arg("-g")
        .arg(pgid.to_string())
        .output()
        .expect("pgrep runs");
    match out.status.code() {
        Some(1) => 0,
        _ => String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count(),
    }
}

/// The supervisor row, read and nothing else.
///
/// Read-only on purpose (T-3-4-F3): a test that watches for a running check
/// must not be able to create a schema, take a write lock, or otherwise change
/// what the wrapper it is watching finds on disk.
fn running_run_key(db: &Path) -> Option<String> {
    let conn = rusqlite::Connection::open_with_flags(
        db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .ok()?;
    conn.query_row(
        "SELECT run_key FROM change_run_executions WHERE state='running' LIMIT 1",
        [],
        |r| r.get::<_, String>(0),
    )
    .ok()
}

#[cfg(unix)]
#[test]
fn a_person_stops_a_running_check_and_the_whole_tree_is_verified_gone() {
    use std::os::unix::process::CommandExt;

    let home = tempfile::tempdir().expect("home");
    let db = home.path().join("ledger.db");

    // A check with a child AND a grandchild: cleanup is a claim about the
    // tree, so the test makes a tree. The grandchild survives a signal aimed
    // at the leader alone, which is what makes the group the right unit.
    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg("sleep 45 & sleep 45")
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("a shell spawns");
    let pgid = child.id();

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && survivors(pgid) < 2 {
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(
        survivors(pgid) >= 2,
        "the child and the grandchild must both be in the group before it is stopped"
    );

    // The supervisor row this run would have written for itself. Inserted
    // through the engine's own writer, not by hand: a test that wrote the row
    // with SQL would be testing SQL.
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("ledger");
    let run_key = "cli:check:2026-09-22T10:00:00Z";
    thruline_core::run_execution::start(
        &ledger,
        &thruline_core::run_execution::Start {
            run_key: run_key.to_string(),
            change_id: None,
            name: "cargo test --workspace".into(),
            pid: Some(i64::from(pgid)),
            process_group: Some(i64::from(pgid)),
            started_at: "2026-09-22T10:00:00Z".into(),
            examined_inputs_id: None,
        },
    )
    .expect("the supervisor row appends");
    drop(ledger);

    let out = run(
        home.path(),
        &db,
        &["change", "cancel-run", "--run", run_key, "--actor", "sam"],
    );
    assert_eq!(out.code, 0, "{out:?}", out = out.stdout);
    assert!(
        out.stdout
            .contains("This check was stopped before it finished. Run it again."),
        "the person is told what happened in their own words: {}",
        out.stdout
    );
    assert!(
        out.stdout
            .contains("Nothing from this check was left running."),
        "cleanup is stated, not assumed: {}",
        out.stdout
    );
    // The three words the card forbids in this register.
    for forbidden in ["pid", "process group", "SIGTERM"] {
        assert!(
            !out.stdout.to_lowercase().contains(forbidden),
            "the verb said {forbidden:?} at a person: {}",
            out.stdout
        );
    }

    // The process table, read independently of anything the product said.
    assert_eq!(
        survivors(pgid),
        0,
        "the group must be empty, grandchild included"
    );
    let _ = child.wait();

    // The record: cancelled, by a named person, and it settles nothing.
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("reopen");
    let record = thruline_core::run_execution::latest(&ledger, run_key)
        .expect("read")
        .expect("a row");
    assert_eq!(
        record.state,
        thruline_core::run_execution::ExecutionState::Cancelled
    );
    assert_eq!(
        record.reason,
        Some(thruline_core::run_execution::Reason::CancelledByPerson)
    );
    assert_eq!(record.actor.as_deref(), Some("sam"));
    assert_eq!(record.cleanup_verified, Some(true));
    assert_eq!(record.survivors, Some(0));
    assert!(!record.settles);

    // The running row it supersedes is untouched: two moments, two rows.
    let rows: i64 = ledger
        .conn()
        .query_row(
            "SELECT count(*) FROM change_run_executions WHERE run_key=?1",
            [run_key],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(rows, 2);
}

/// A run nobody started, and a run that already ended, are two different
/// refusals — and neither is success.
#[test]
fn stopping_something_that_is_not_running_is_refused_in_words_and_exits_nonzero() {
    let home = tempfile::tempdir().expect("home");
    let db = home.path().join("ledger.db");
    drop(thruline_core::ledger::Ledger::open(&db).expect("ledger"));

    let out = run(
        home.path(),
        &db,
        &["change", "cancel-run", "--run", "nothing-by-that-name"],
    );
    assert_ne!(
        out.code, 0,
        "there was nothing to stop, which is not success"
    );
    assert!(
        out.stdout
            .contains("There is no check by that name in the record."),
        "{}",
        out.stdout
    );
}

/// **Verifier finding 1, through the binary.** A check that a person stops
/// must not become a check that finished when the wrapper reaps its corpse.
///
/// This is the live failure the verifier reproduced twice: `cancel-run` wrote
/// `cancelled`, the wrapper came out of `wait()` seconds later and appended
/// `completed` on top, and coverage then counted a stopped check as a pass.
#[cfg(unix)]
#[test]
fn a_cancelled_check_never_becomes_a_completed_one_when_the_wrapper_reaps_it() {
    let home = tempfile::tempdir().expect("home");
    let work = a_repository();
    let db = home.path().join("ledger.db");

    // A real `thruline check` around a long command, in the background. Its
    // stderr is kept, not thrown away: if the wrapper refuses, the refusal is
    // the answer to "why is there no running row", and a test that discards it
    // can only report a timeout.
    let wrapper_err_path = home.path().join("wrapper.err");
    let wrapper_err = std::fs::File::create(&wrapper_err_path).expect("wrapper stderr file");
    let mut wrapper = Command::new(bin())
        .env("HOME", home.path())
        .env("THRULINE_HOME", home.path().join(".thruline"))
        .current_dir(work.path())
        .arg("--db")
        .arg(&db)
        .args(["check", "--quiet", "--", "/bin/sh", "-c", "sleep 60 & wait"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(wrapper_err))
        .spawn()
        .expect("the wrapper spawns");

    // Wait for the wrapper to have opened its supervisor row.
    //
    // Two things about this wait are deliberate, and both come from T-3-4-F3.
    //
    // 1. It READS. Polling with `Ledger::open` made the test a second writer
    //    on a store the wrapper was still creating: on a fresh file that call
    //    creates the whole schema, and a wrapper that opens while another
    //    process is mid-creation sees a populated store without the current
    //    fingerprint and refuses with `schema upgrade pending` (A193), exits
    //    having run nothing, and no running row ever appears. A read-only
    //    connection cannot create, migrate or lock anything.
    // 2. The bound is measured, not guessed: spawn-to-row was 0.39-0.41 s
    //    idle and 0.48-0.92 s with a 14-core `cargo build --release` running
    //    (n = 15, this machine, 2026-09-23). 30 s is ~30x the loaded worst
    //    case; and a wrapper that has EXITED fails the test at once with its
    //    own words rather than burning the bound, because once it is gone the
    //    row is never coming.
    let deadline = Instant::now() + Duration::from_secs(30);
    let run_key = loop {
        if let Some(key) = running_run_key(&db) {
            break key;
        }
        if let Some(status) = wrapper.try_wait().expect("try_wait") {
            panic!(
                "the wrapper exited ({status}) before recording a running check; it said: {}",
                std::fs::read_to_string(&wrapper_err_path).unwrap_or_default()
            );
        }
        assert!(
            Instant::now() < deadline,
            "the wrapper never recorded a running check; it said: {}",
            std::fs::read_to_string(&wrapper_err_path).unwrap_or_default()
        );
        std::thread::sleep(Duration::from_millis(50));
    };

    let out = run(
        home.path(),
        &db,
        &["change", "cancel-run", "--run", &run_key, "--actor", "sam"],
    );
    assert_eq!(out.code, 0, "{}", out.stdout);

    // The wrapper now reaps a signalled child and writes its own ending.
    let status = {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            match wrapper.try_wait().expect("try_wait") {
                Some(status) => break status,
                None => {
                    assert!(Instant::now() < deadline, "the wrapper never exited");
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
    };
    assert!(!status.success(), "a killed child does not exit 0");

    // Exactly ONE ending, and it is the one the person made.
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("reopen");
    let record = thruline_core::run_execution::latest(&ledger, &run_key)
        .expect("read")
        .expect("a row");
    assert_eq!(
        record.state,
        thruline_core::run_execution::ExecutionState::Cancelled,
        "the person's cancellation stands"
    );
    assert_eq!(record.actor.as_deref(), Some("sam"));
    assert!(!record.settles, "a stopped check never settles anything");
    let endings: i64 = ledger
        .conn()
        .query_row(
            "SELECT count(*) FROM change_run_executions
              WHERE run_key=?1 AND state <> 'running'",
            [&run_key],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(endings, 1, "a run may end exactly once");
    let completed: i64 = ledger
        .conn()
        .query_row(
            "SELECT count(*) FROM change_run_executions
              WHERE run_key=?1 AND state='completed'",
            [&run_key],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(
        completed, 0,
        "no `completed` row may ever appear for this run"
    );
}

/// A child killed by something that left no cancellation record is
/// `interrupted`, never `completed` — the wrapper has the signal in hand.
#[cfg(unix)]
#[test]
fn a_signalled_child_with_no_cancellation_behind_it_is_interrupted() {
    let home = tempfile::tempdir().expect("home");
    let work = a_repository();
    let db = home.path().join("ledger.db");

    let out = Command::new(bin())
        .env("HOME", home.path())
        .env("THRULINE_HOME", home.path().join(".thruline"))
        .current_dir(work.path())
        .arg("--db")
        .arg(&db)
        // The child kills itself: a crash, as far as the wrapper can tell.
        .args(["check", "--quiet", "--", "/bin/sh", "-c", "kill -TERM $$"])
        .output()
        .expect("the wrapper runs");
    assert!(!out.status.success());

    let ledger = thruline_core::ledger::Ledger::open(&db).expect("ledger");
    let all = thruline_core::run_execution::all(&ledger, None).expect("read");
    assert_eq!(all.len(), 1, "one run was recorded");
    assert_eq!(
        all[0].state,
        thruline_core::run_execution::ExecutionState::Interrupted
    );
    assert_eq!(
        all[0].reason,
        Some(thruline_core::run_execution::Reason::ProcessSignalled)
    );
    assert!(!all[0].settles, "a signalled child settles nothing");
}
