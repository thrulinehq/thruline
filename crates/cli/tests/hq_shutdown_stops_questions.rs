//! D-049 r3 / ADV-run-security-5 — HQ's own shutdown ends a question the run
//! route is still putting to rustup, through the shipping binary.
//!
//! `uiserve.rs` calls `state.run_route.stop_questions()` after the accept loop
//! ends. The unit test `hq_stopping_ends_a_question_to_rustup_that_is_still_going`
//! calls that function directly, so removing the call from HQ's shutdown left
//! every test green (D-046). This test does not call it. It starts
//! `thruline ui`, lets the run sheet put a question to a rustup that never
//! answers, and stops HQ the way a person does — SIGINT, which is Ctrl+C. The
//! question runs in a process group of its own, so the signal does not reach
//! it. Only HQ's shutdown can end it before HQ exits. If that call is gone, the
//! question outlives HQ with parent 1.
//!
//! Everything is inside this test's temporary folder: the store, the project,
//! the home folder, rustup's homes and the stand-in rustup. No real store is
//! opened, and the machine's rustup is never started.

#![cfg(unix)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use thruline_core::ledger::Ledger;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000p001";
const CHANGE: &str = "cccccccc-0000-4000-8000-00000000c001";

/// `RUSTUP_DEADLINE` in `run_route.rs`. Past it the route ends the question
/// itself, so a stop that comes later cannot be told apart from the deadline.
const RUSTUP_DEADLINE: Duration = Duration::from_secs(4);

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

/// A real git repository with one commit, as the run route's fixtures use.
fn a_project(dir: &Path) -> PathBuf {
    let root = dir.join("project");
    std::fs::create_dir_all(&root).expect("project dir");
    std::fs::write(root.join("README.md"), "synthetic project\n").expect("file");
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "d049@example.invalid"],
        vec!["config", "user.name", "d049"],
        vec!["add", "."],
        vec!["commit", "-q", "-m", "one"],
    ] {
        let ok = Command::new("git")
            .args(&args)
            .current_dir(&root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("git runs")
            .success();
        assert!(ok, "git {args:?}");
    }
    root.canonicalize().expect("canonical root")
}

/// One Change whose accepted profile is `cargo-test`, bound to `root`.
fn a_change(db: &Path, root: &Path) {
    use thruline_core::change::{self, ChangeRequest, NewChange, RequestSource};
    let ledger = Ledger::open(db).expect("ledger");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES(?1, ?2, 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
            [PROJECT, &root.to_string_lossy()],
        )
        .expect("project");
    change::insert_change(
        &ledger,
        &NewChange {
            id: CHANGE.to_string(),
            project_id: PROJECT.to_string(),
            opened_at: "2026-01-02T09:00:00Z".to_string(),
            request: ChangeRequest {
                title: "Retry failed uploads".to_string(),
                purpose: "They do not retry.".to_string(),
                prompt_event_id: None,
                requester: "sam".to_string(),
                source: RequestSource::Person,
            },
        },
    )
    .expect("change");
    let version = change::append_scope(
        &ledger,
        CHANGE,
        "2026-01-02T09:30:00Z",
        &change::AcceptedScope {
            criteria: vec!["The tests pass.".to_string()],
            check_profile: vec!["cargo-test".to_string()],
            ..Default::default()
        },
    )
    .expect("scope");
    change::accept_scope(
        &ledger,
        CHANGE,
        version,
        &change::ScopeAcceptance {
            actor: "sam".into(),
            accepted_at: "2026-01-02T09:31:00Z".into(),
            rationale: String::new(),
        },
    )
    .expect("acceptance");
    thruline_core::examined_inputs::bind_intended_at(
        &ledger,
        CHANGE,
        "2026-01-02T09:32:00Z",
        Some(root),
    )
    .expect("bind")
    .expect("a record");
}

/// A folder holding a `rustup` that never answers and a `cargo` linked to it,
/// so the run route takes `cargo` for rustup's proxy and asks this `rustup`
/// which cargo a run would start. Each start writes its own pid and its
/// child's pid to `pids`, then waits a minute on the child.
fn a_rustup_that_never_answers(dir: &Path, pids: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir).expect("the rustup's folder");
    let rustup = dir.join("rustup");
    std::fs::write(
        &rustup,
        format!(
            "#!/bin/sh\necho $$ >> '{p}'\nsleep 60 &\necho $! >> '{p}'\nwait\n",
            p = pids.display()
        ),
    )
    .expect("the rustup script");
    std::fs::set_permissions(&rustup, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    std::os::unix::fs::symlink(&rustup, dir.join("cargo")).expect("cargo, a link to it");
    dir.to_path_buf()
}

fn pids_in(file: &Path) -> Vec<i32> {
    std::fs::read_to_string(file)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect()
}

fn alive(pid: i32) -> bool {
    rustix::process::Pid::from_raw(pid)
        .is_some_and(|pid| rustix::process::test_kill_process(pid).is_ok())
}

/// Polls `done` every 25 ms until it holds or `bound` passes.
fn wait_until(bound: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + bound;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Whatever this test started is ended when it ends, pass or fail: HQ, and
/// the process group of every question it recorded (its own processes only,
/// by the pids its own stand-in wrote).
struct Cleanup {
    hq: Option<Child>,
    pids: PathBuf,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Some(mut hq) = self.hq.take() {
            let _ = hq.kill();
            let _ = hq.wait();
        }
        for pid in pids_in(&self.pids) {
            if let Some(pid) = rustix::process::Pid::from_raw(pid) {
                let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
                let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
            }
        }
    }
}

/// Starts `thruline ui` on a free loopback port and returns it with that port.
fn start_hq(temp: &Path, db: &Path, rustup_dir: &Path) -> (Child, u16) {
    let home = temp.join("home");
    std::fs::create_dir_all(&home).expect("home");
    let rustup_home = temp.join("rustup-home");
    let cargo_home = temp.join("cargo-home");
    std::fs::create_dir_all(&rustup_home).expect("rustup home");
    std::fs::create_dir_all(&cargo_home).expect("cargo home");
    let path = std::env::join_paths([
        rustup_dir.to_path_buf(),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
    ])
    .expect("path");
    let mut child = Command::new(bin())
        .env("HOME", &home)
        .env("THRULINE_HOME", home.join(".thruline"))
        .env("PATH", path)
        .env("RUSTUP_HOME", &rustup_home)
        .env("CARGO_HOME", &cargo_home)
        .env_remove("RUSTUP_TOOLCHAIN")
        .current_dir(&home)
        .arg("--db")
        .arg(db)
        .args(["ui", "--no-open", "--port", "0"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("thruline ui");
    let stdout = child.stdout.take().expect("stdout");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { return };
            if tx.send(line).is_err() {
                return;
            }
        }
    });
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(line) => {
                if let Some(rest) = line.trim().strip_prefix("ThruLine is at http://127.0.0.1:") {
                    let port: u16 = rest
                        .trim()
                        .split('/')
                        .next()
                        .expect("port")
                        .parse()
                        .expect("port in the printed URL");
                    return (child, port);
                }
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                let mut err = String::new();
                if let Some(mut e) = child.stderr.take() {
                    let _ = e.read_to_string(&mut err);
                }
                panic!("`thruline ui` never printed a loopback URL.\n{err}");
            }
        }
    }
}

/// One GET, read to the end or until HQ goes away.
fn get(port: u16, path: &str) -> String {
    let Ok(mut socket) = TcpStream::connect(("127.0.0.1", port)) else {
        return String::new();
    };
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
    );
    if socket.write_all(request.as_bytes()).is_err() {
        return String::new();
    }
    let mut raw = Vec::new();
    let _ = socket.read_to_end(&mut raw);
    String::from_utf8_lossy(&raw).to_string()
}

#[test]
fn hqs_own_shutdown_ends_a_question_to_rustup_that_is_still_going() {
    let temp = tempfile::tempdir().expect("temp");
    let temp_path = temp.path().canonicalize().expect("canonical temp");
    let db = temp_path.join("ledger.db");
    let root = a_project(&temp_path);
    a_change(&db, &root);
    let pids = temp_path.join("RUSTUP-PIDS.txt");
    let rustup_dir = a_rustup_that_never_answers(&temp_path.join("slow-rustup"), &pids);

    let (hq, port) = start_hq(&temp_path, &db, &rustup_dir);
    let hq_pid = rustix::process::Pid::from_child(&hq);
    let mut cleanup = Cleanup {
        hq: Some(hq),
        pids: pids.clone(),
    };

    // The run sheet asks rustup which cargo a run here would start; this
    // rustup never answers, so the request is still going.
    let sheet = format!("/api/changes/{CHANGE}/run");
    let (answered, answer) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = answered.send(get(port, &sheet));
    });
    if !wait_until(Duration::from_secs(20), || pids_in(&pids).len() >= 2) {
        panic!(
            "the run sheet never asked rustup: {:?}; the sheet answered: {:?}",
            pids_in(&pids),
            answer.try_recv().ok()
        );
    }
    let asked_at = Instant::now();
    let asked = pids_in(&pids);
    assert!(
        asked.iter().all(|p| alive(*p)),
        "the question ended before HQ was stopped: {asked:?}"
    );

    // Ctrl+C: SIGINT to HQ's own process, not to the question's group.
    rustix::process::kill_process(hq_pid, rustix::process::Signal::INT).expect("SIGINT to HQ");
    let hq = cleanup.hq.as_mut().expect("HQ");
    let mut exited = None;
    assert!(
        wait_until(Duration::from_secs(30), || {
            exited = hq.try_wait().expect("HQ's status");
            exited.is_some()
        }),
        "HQ did not exit within 30 s of SIGINT"
    );
    let stopped_after = asked_at.elapsed();
    assert!(
        stopped_after < RUSTUP_DEADLINE,
        "could not run: HQ took {stopped_after:?} from the question starting to exiting, past the \
         route's own {RUSTUP_DEADLINE:?} deadline for it, so this run cannot tell HQ's shutdown \
         from that deadline"
    );
    cleanup.hq = None;

    // HQ has exited. Nothing of the question may be left behind it.
    assert!(
        wait_until(Duration::from_secs(5), || !asked.iter().any(|p| alive(*p))),
        "a question to rustup outlived HQ's shutdown (exit {exited:?}): {:?} still running",
        asked.iter().filter(|p| alive(**p)).collect::<Vec<_>>()
    );
    assert_eq!(
        pids_in(&pids),
        asked,
        "rustup was asked again after HQ was told to stop"
    );
}
