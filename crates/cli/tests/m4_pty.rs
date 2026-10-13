//! `thruline run` under a real pseudoterminal — BUILD-SPEC §4.4 — and the
//! disclosure it closes.
//!
//! # The disclosure
//!
//! `stdio` is recorded on every exec event because a terminal changes the bytes
//! a runner produces, and §7 re-executes in a container where output is a pipe.
//! If the comparison read a terminal difference as a content difference it
//! would report tampering that did not happen — **a false accusation produced
//! by a terminal, landing on a person**. `Comparison::of` conditions on the
//! recorded shape rather than guessing, and `REPORT.md` said that branch was
//! "unit-tested and not exercised end to end, because nothing in this build
//! produces a TTY".
//!
//! Something does now. These tests drive the whole chain with a real terminal:
//! `thruline run` is spawned **on** a pseudoterminal, so it gives the agent
//! one, so the shim gives `pytest` one, so the exec event records
//! `stdio: "tty"`.
//!
//! # Two arms, and the second is what keeps the first honest
//!
//! 1. A piped run and a TTY run of the same command on the same tree must
//!    **not** be reported as a disagreement. `Comparison::of` must normalise or
//!    say the capture shape differs — never "output mismatch → unsupported".
//! 2. A **genuine** content difference under an identical capture shape must
//!    still be reported. Otherwise the normalisation has simply switched the
//!    detector off, which is the cheapest possible way to satisfy arm 1.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use thruline_reexec::{Comparison, RunFacts};

#[path = "support/qualified_binary.rs"]
mod qualified_binary;
use qualified_binary::thruline;

fn pytest_available() -> bool {
    Command::new("pytest")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn require_pytest() -> bool {
    if pytest_available() {
        return true;
    }
    if std::env::var_os("THRULINE_TEST_NO_PYTEST").is_some() {
        eprintln!("SKIPPED: pytest is not installed. The PTY end-to-end did NOT run.");
        return false;
    }
    panic!("pytest is not on PATH; §4.4's PTY path needs a real runner");
}

fn docker_available() -> bool {
    Command::new("docker")
        .args(["version", "--format", "{{.Server.Version}}"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// A scratch repository with a passing suite.
fn scratch_repo() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).expect("repo");
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["config", "user.email", "a@e.invalid"],
        vec!["config", "user.name", "A"],
    ] {
        let out = Command::new("git")
            .current_dir(&repo)
            .args(&args)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}");
    }
    std::fs::write(
        repo.join("shipping.py"),
        "def rate(weight_kg):\n    return round(2.5 * weight_kg + 4.0, 2)\n",
    )
    .expect("w");
    std::fs::write(
        repo.join("test_shipping.py"),
        "from shipping import rate\n\n\
         def test_rate_small():\n    assert rate(2) == 9.0\n\n\
         def test_rate_large():\n    assert rate(10) == 29.0\n",
    )
    .expect("w");
    std::fs::write(repo.join(".gitignore"), "__pycache__/\n.pytest_cache/\n").expect("w");
    // §7's ecosystem detection reads a lockfile. Without one the re-execution
    // refuses rather than guessing an image, which is correct and is not what
    // the Docker arm below is testing.
    std::fs::write(repo.join("requirements.txt"), "pytest==9.0.1\n").expect("w");
    for args in [vec!["add", "-A"], vec!["commit", "-qm", "baseline"]] {
        let out = Command::new("git")
            .current_dir(&repo)
            .args(&args)
            .output()
            .expect("git");
        assert!(out.status.success());
    }
    (dir, home, repo)
}

/// Run `thruline run` **on a pseudoterminal**, so it sees a terminal and gives
/// its child one.
///
/// This is the whole trick: `cargo test` gives its children pipes, so a
/// `thruline run` launched normally would take the pipe path and prove nothing.
fn run_under_a_terminal(home: &Path, repo: &Path, command: &[&str]) -> (String, serde_json::Value) {
    // The report goes to a FILE, not to the terminal it shares with the child.
    let report_path = home.join("run-report.json");
    let mut cmd = Command::new(thruline());
    cmd.arg("run")
        .arg("--report")
        .arg(&report_path)
        .arg("--")
        .args(command)
        .current_dir(repo)
        .env("THRULINE_HOME", home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID");
    let mut pty = thruline_capture::pty::spawn_on_pty(cmd).expect("pseudoterminal");
    let mut out = Vec::new();
    // EOF on a controller is `Ok(0)` on some platforms and `EIO` on others.
    let _ = pty.controller.read_to_end(&mut out);
    let status = pty.child.wait().expect("wait");
    assert!(
        status.success(),
        "thruline run exited {:?}: {}",
        status.code(),
        String::from_utf8_lossy(&out)
    );
    let text = String::from_utf8_lossy(&out).replace('\r', "");
    let json: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&report_path).unwrap_or_else(|e| panic!("no report file: {e}\n{text}")),
    )
    .expect("the report is JSON");
    (text, json)
}

/// Every `exec` event of a session, as the ledger holds it.
fn exec_events(home: &Path, session: &str) -> Vec<serde_json::Value> {
    let ledger = thruline_ledger::Ledger::open(&home.join("exec.db")).expect("ledger");
    ledger
        .session_events(session)
        .expect("events")
        .iter()
        .filter(|e| e.kind().as_str().as_str() == "exec")
        .map(|e| e.payload().clone())
        .collect()
}

// ---------------------------------------------------------------------------
// The producer: a real terminal, recorded as one
// ---------------------------------------------------------------------------

/// `thruline run` on a terminal records `stdio: "tty"`, and the same command
/// off one records `"pipe"`. Both are measurements.
#[test]
fn a_run_on_a_terminal_records_a_terminal_and_one_off_it_does_not() {
    if !require_pytest() {
        return;
    }
    let (_dir, home, repo) = scratch_repo();
    let (_text, report) = run_under_a_terminal(&home, &repo, &["sh", "-c", "pytest -q"]);
    assert_eq!(
        report["stdio"],
        serde_json::json!("tty"),
        "thruline run was on a pseudoterminal and did not record one: {report:#}"
    );
    let session = report["session"].as_str().expect("session").to_string();
    let execs = exec_events(&home, &session);
    assert_eq!(execs.len(), 1, "expected one recorded exec: {execs:#?}");
    assert_eq!(
        execs[0]["stdio"],
        serde_json::json!("tty"),
        "the terminal did not reach the recorded exec: {:#}",
        execs[0]
    );
    // One device carried both streams, and the event says so rather than
    // presenting an empty `stderr_hash` as a measurement.
    assert!(execs[0]["streams"]
        .as_str()
        .is_some_and(|s| s.starts_with("merged")));
    // And the terminal really did change the bytes: pytest colourises only on
    // a terminal. This is the premise the whole `stdio` field rests on, so it
    // is measured here rather than assumed.
    let excerpt = execs[0]["stdout_excerpt"].as_str().unwrap_or("");
    assert!(
        excerpt.contains('\u{1b}'),
        "a terminal that produced no escape sequences would make the capture \
         shape irrelevant, and this test vacuous: {excerpt:?}"
    );

    // The control, off a terminal.
    let (_dir2, home2, repo2) = scratch_repo();
    let report_path = home2.join("run-report.json");
    let out = Command::new(thruline())
        .args(["run", "--report"])
        .arg(&report_path)
        .args(["--", "sh", "-c", "pytest -q"])
        .current_dir(&repo2)
        .env("THRULINE_HOME", &home2)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .output()
        .expect("run");
    assert!(out.status.success());
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&report_path).expect("report")).expect("json");
    assert_eq!(report["stdio"], serde_json::json!("pipe"));
    let session = report["session"].as_str().expect("session").to_string();
    let execs = exec_events(&home2, &session);
    assert_eq!(execs[0]["stdio"], serde_json::json!("pipe"));
    assert!(
        execs[0]["streams"].is_null(),
        "pipes keep the two streams separable, so nothing should say merged"
    );
}

/// The child's exit code is this command's exit code, unchanged.
#[test]
fn the_childs_exit_code_is_the_runs_exit_code() {
    let (_dir, home, repo) = scratch_repo();
    let out = Command::new(thruline())
        .args(["run", "--", "sh", "-c", "exit 7"])
        .current_dir(&repo)
        .env("THRULINE_HOME", &home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .output()
        .expect("run");
    assert_eq!(
        out.status.code(),
        Some(7),
        "a wrapper that reports its own success in place of the command's \
         failure destroys the evidence this product collects"
    );
}

// ---------------------------------------------------------------------------
// The comparison, on facts this product produced
// ---------------------------------------------------------------------------

/// **Arm 1.** A TTY run and a piped run of the same command on the same tree
/// are not a disagreement.
///
/// The recorded `stdio` on the left is real — it came out of the ledger above,
/// written by `thruline run` on a pseudoterminal. The right is the container's
/// shape (§7 always pipes).
#[test]
fn a_terminal_difference_is_never_reported_as_a_content_difference() {
    if !require_pytest() {
        return;
    }
    let (_dir, home, repo) = scratch_repo();
    let (_text, report) = run_under_a_terminal(&home, &repo, &["sh", "-c", "pytest -q"]);
    let session = report["session"].as_str().expect("session").to_string();
    let execs = exec_events(&home, &session);
    let recorded_shape = execs[0]["stdio"].as_str().expect("stdio").to_string();
    assert_eq!(recorded_shape, "tty");

    let agent = RunFacts {
        exit_code: 0,
        tally: Some((2, 2, 0)),
        stdio: recorded_shape,
    };
    let container = RunFacts {
        exit_code: 0,
        tally: Some((2, 2, 0)),
        stdio: "pipe".to_string(),
    };
    let verdict = Comparison::of(&agent, &container);
    match &verdict {
        Comparison::NotComparable { detail } => {
            assert!(
                detail.contains("tty") && detail.contains("pipe"),
                "the reason must name the shapes, or a reader cannot tell this \
                 from a parse failure: {detail}"
            );
        }
        other => panic!(
            "a terminal difference was reported as {other:?} — an accusation \
             produced by a terminal"
        ),
    }
    assert!(
        !matches!(verdict, Comparison::Disagrees { .. }),
        "never 'output mismatch → unsupported'"
    );
}

/// **Arm 2.** The normalisation did not switch the detector off.
///
/// A genuine content difference under an identical capture shape must still be
/// reported. Without this, arm 1 is satisfied by a comparison that agrees with
/// everything.
#[test]
fn a_real_difference_under_one_capture_shape_is_still_reported() {
    let same_shape = |tally: (u64, u64, u64)| RunFacts {
        exit_code: 0,
        tally: Some(tally),
        stdio: "pipe".to_string(),
    };
    assert!(matches!(
        Comparison::of(&same_shape((2, 2, 0)), &same_shape((2, 1, 1))),
        Comparison::Disagrees { .. }
    ));
    // And a differing exit code is a disagreement *even across* shapes: an
    // exit code is not a function of the terminal.
    let tty = RunFacts {
        exit_code: 0,
        tally: Some((2, 2, 0)),
        stdio: "tty".to_string(),
    };
    let piped_failure = RunFacts {
        exit_code: 1,
        tally: Some((2, 1, 1)),
        stdio: "pipe".to_string(),
    };
    assert!(matches!(
        Comparison::of(&tty, &piped_failure),
        Comparison::Disagrees { .. }
    ));
    // The honest agreement still agrees, or every one of the above is trivial.
    assert!(matches!(
        Comparison::of(&same_shape((2, 2, 0)), &same_shape((2, 2, 0))),
        Comparison::Agrees { .. }
    ));
}

// ---------------------------------------------------------------------------
// The container half, end to end. Needs a Docker daemon.
// ---------------------------------------------------------------------------

/// A session recorded on a terminal, re-executed in a container that pipes.
///
/// The whole chain, with nothing synthetic: `thruline run` on a pseudoterminal
/// → `stdio: "tty"` on the exec event → `thruline reexec` in a container →
/// `Comparison::of` across the two recorded shapes.
///
/// **Exit 2, not 3.** `verify-receipt` and `reexec` draw the same three-way
/// distinction: 0 ran and agreed, 2 ran and could not be compared, 3 refuted.
/// A capture-shape difference is the middle one, and a gate reading 3 here
/// would be failing a build over a terminal.
#[test]
#[ignore = "requires a running Docker daemon. Run with --ignored."]
fn a_terminal_session_re_executed_in_a_container_is_not_comparable_and_says_so() {
    assert!(
        docker_available(),
        "this test was asked for explicitly and there is no Docker daemon — a \
         skip here would report the disclosure as closed without closing it"
    );
    assert!(
        pytest_available(),
        "the agent's own run needs a real pytest"
    );
    let (_dir, home, repo) = scratch_repo();
    let (_text, report) = run_under_a_terminal(&home, &repo, &["sh", "-c", "pytest -q"]);
    assert_eq!(report["stdio"], serde_json::json!("tty"));
    let session = report["session"].as_str().expect("session").to_string();
    assert_eq!(exec_events(&home, &session)[0]["stdio"], "tty");

    let out = Command::new(thruline())
        .args(["reexec", "--json", "--session", &session])
        .current_dir(&repo)
        .env("THRULINE_HOME", &home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .output()
        .expect("reexec");
    let reexec: serde_json::Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}\n{}", String::from_utf8_lossy(&out.stdout)));
    assert!(
        reexec["refused"].is_null(),
        "the re-execution was refused: {reexec:#}"
    );
    let checks = reexec["checks"].as_array().expect("checks");
    assert_eq!(checks.len(), 1, "{reexec:#}");
    let comparison = &checks[0]["comparison"];
    assert_eq!(
        comparison["outcome"],
        serde_json::json!("not_comparable"),
        "a terminal difference must withhold the comparison, not refute the \
         claim: {comparison:#}"
    );
    let detail = comparison["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("tty") && detail.contains("pipe"),
        "the reason must name the shapes: {detail}"
    );
    assert_eq!(
        out.status.code(),
        Some(2),
        "ran and could not be compared is exit 2; 3 would fail a build over a \
         terminal. Report was {reexec:#}"
    );
    eprintln!("PTY→container comparison: {detail}");
}

// ---------------------------------------------------------------------------
// The flight recorder, launched for real
// ---------------------------------------------------------------------------

/// `thruline tui` starts, draws, and quits on `q`.
///
/// A smoke test, and it says so: the model's decisions are tested exhaustively
/// in `crates/tui`, and what this covers is the part that cannot be — that the
/// event loop starts, paints something, and gives the terminal back. The
/// terminal is real, from `capture::pty`, because a TUI on a pipe draws
/// nothing and would pass a weaker version of this test.
///
/// It also guards a defect the first version had: the loop redrew every 200 ms
/// whether or not anything had changed, which pinned a core on an idle screen
/// and flooded the terminal fast enough that a keystroke never landed. The
/// bounded read below fails if that comes back.
#[test]
fn the_flight_recorder_starts_draws_and_gives_the_terminal_back() {
    if !require_pytest() {
        return;
    }
    use std::io::Write;
    let (_dir, home, repo) = scratch_repo();
    let (_text, report) = run_under_a_terminal(&home, &repo, &["sh", "-c", "pytest -q"]);
    let session = report["session"].as_str().expect("session").to_string();

    let mut cmd = Command::new(thruline());
    cmd.args(["tui", "--session", &session])
        .current_dir(&repo)
        .env("THRULINE_HOME", &home)
        .env("TERM", "xterm-256color")
        .env_remove("CLAUDE_CODE_SESSION_ID");
    let mut pty = thruline_capture::pty::spawn_on_pty(cmd).expect("pseudoterminal");

    // Read on a background thread so a full pipe cannot wedge the write below.
    let mut reader = pty.controller.try_clone().expect("dup");
    let collector = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => buf.extend_from_slice(&chunk[..n]),
            }
        }
        buf
    });
    std::thread::sleep(std::time::Duration::from_millis(1500));
    pty.controller.write_all(b"q").expect("send q");
    pty.controller.flush().ok();

    let status = pty.child.wait().expect("wait");
    drop(pty);
    let drawn = collector.join().unwrap_or_default();
    assert!(
        status.success(),
        "the flight recorder exited {:?}",
        status.code()
    );
    let text = String::from_utf8_lossy(&drawn);
    assert!(
        text.contains("thruline"),
        "nothing was drawn ({} bytes)",
        drawn.len()
    );
    assert!(
        text.contains('┌'),
        "the panes were not drawn ({} bytes)",
        drawn.len()
    );
}

#[test]
fn wrapper_closes_only_the_session_it_created() {
    for joined in [false, true] {
        let (_dir, home, repo) = scratch_repo();
        let report = home.join("lifecycle-report.json");
        let mut cmd = Command::new(thruline());
        cmd.args(["run", "--no-pty", "--report"]).arg(&report);
        if joined {
            cmd.args(["--session", "still-live-harness-session"]);
        }
        let out = cmd
            .args(["--", "sh", "-c", "python3 -c 'print(1)'"])
            .current_dir(&repo)
            .env("THRULINE_HOME", &home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .output()
            .expect("run");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let reading: serde_json::Value =
            serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
        let ledger = thruline_ledger::Ledger::open_read_only(&home.join("exec.db")).unwrap();
        let events = ledger
            .session_events(reading["session"].as_str().unwrap())
            .unwrap();
        let ended = events
            .iter()
            .filter(|e| e.kind().as_str() == "session_end")
            .count();
        assert_eq!(
            ended,
            usize::from(!joined),
            "a joined session must remain open"
        );
        ledger
            .verify_session(reading["session"].as_str().unwrap())
            .unwrap();
    }
}
