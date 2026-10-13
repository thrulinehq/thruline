//! T-3-5 (`B7.6`, `B7.8`, card item 0) — readiness for a check a person
//! stopped, through the compiled binary and the served route.
//!
//! **Why this file exists.** Two rounds of T-3-5 asserted this behaviour
//! against a hand-built state: round 0 constructed the `CoverageState`
//! directly, round 1 constructed the ledger rows directly and called
//! `coverage()` in-process. Both were green while the product, driven by
//! `thruline check` and `thruline change cancel-run` and read through
//! `GET /api/changes/<id>/readiness`, was never exercised end to end by a
//! test. This file closes that gap: every row in the store here is written by
//! a product verb, and the sentence asserted is the one the server serves.
//!
//! The two shapes are the two ways a check stops early, and they reach the
//! execution record by different routes:
//!
//!  * **stopped by a person** — `cancel-run` appends `cancelled` while the
//!    wrapper is still inside `wait()`, so that row names no binding and the
//!    binding-keyed map cannot find it. The run's own key is the join.
//!  * **killed outright** — the child dies to a signal and the wrapper records
//!    `interrupted` against the binding it produced.
//!
//! Neither may ever read as "something ran, and it settled nothing": a check
//! that was started and stopped is a fact the person needs told.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
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

/// A repository with one commit, a store whose one project points at it, and
/// one recorded session that edited a file in it — the same shape
/// `check_for_change_freshness_cli.rs` uses.
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

/// `change new` → `associate --confirm` → `accept --all` → `prepare`, all
/// through the binary. The purpose is build-worded so the accepted criterion
/// and the build-classed run below meet on the ordinary derivation rule,
/// without a hand-made link.
fn an_accepted_change(home: &Path, repo: &Path, db: &Path) -> String {
    let opened = run(
        home,
        repo,
        db,
        &[
            "change",
            "new",
            "--title",
            "Retry failed uploads",
            "--purpose",
            "The build stays clean on the packages that changed.",
            "--requester",
            "sam",
        ],
    );
    assert_eq!(opened.code, 0, "{}", opened.stderr);
    let change = opened
        .stdout
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("chg_"))
        .unwrap_or_else(|| panic!("no Change id was printed: {}", opened.stdout))
        .to_string();

    for args in [
        vec![
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
        vec![
            "change",
            "accept",
            &change,
            "--all",
            "--actor",
            "sam",
            "--rationale",
            "this is what I want checked",
        ],
        vec!["change", "prepare", &change],
    ] {
        let out = run(home, repo, db, &args);
        assert_eq!(out.code, 0, "{args:?}: {}{}", out.stdout, out.stderr);
    }
    change
}

/// A `cargo` that never finishes, so a build-classed check can be caught
/// mid-flight. The class is read from the command TEXT, so this is a real
/// build-classed run by the product's own rule.
///
/// OFFLINE-KILL-GUARD-R3 (R-2026-09-28-26): the owned fetch takes cargo and
/// rustc only from `$CARGO_HOME/bin`, `$RUSTUP_HOME/toolchains` or a system
/// directory, so the shim lives in `<base>/cargo-home/bin` and the check is
/// started with `CARGO_HOME=<base>/cargo-home`. If the caller's rustc is a
/// rustup proxy, the same rustup binary is installed beside the shim (hard
/// link, or a copy across volumes) with `rustc` linked to it, as rustup lays
/// out its own `bin`; the toolchain it resolves is the caller's `$RUSTUP_HOME`.
fn a_cargo_that_hangs(base: &Path) -> PathBuf {
    let shim = base.join("cargo-home/bin");
    std::fs::create_dir_all(&shim).expect("shim dir");
    let path = std::env::var_os("PATH").unwrap_or_default();
    if let Some(rustup) = std::env::split_paths(&path)
        .map(|p| p.join("rustc"))
        .find(|p| p.is_file())
        .and_then(|p| std::fs::canonicalize(p).ok())
        .filter(|p| p.file_stem() == Some(std::ffi::OsStr::new("rustup")))
    {
        std::fs::hard_link(&rustup, shim.join("rustup"))
            .or_else(|_| std::fs::copy(&rustup, shim.join("rustup")).map(|_| ()))
            .expect("install rustup beside the shim");
        #[cfg(unix)]
        std::os::unix::fs::symlink("rustup", shim.join("rustc")).expect("rustc proxy");
    }
    let cargo = shim.join("cargo");
    std::fs::write(&cargo, "#!/bin/sh\nsleep 20\n").expect("write shim");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    shim
}

/// The wrapper this test started, reaped on every path out — including a
/// panic. A test that leaves a `thruline check` running poisons the next one,
/// and the store it is still writing to.
struct Check {
    child: Child,
}

impl Drop for Check {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Check {
    /// Wait for the wrapper to finish writing what it recorded.
    fn finished(&mut self) {
        let _ = self.child.wait();
    }
}

/// Start `thruline check --for-change … -- cargo build --lib` and return the
/// running wrapper plus the run key the store now holds for it.
fn a_check_in_flight(
    home: &Path,
    repo: &Path,
    db: &Path,
    change: &str,
    shim: &Path,
) -> (Check, String) {
    let path = format!(
        "{}:{}",
        shim.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let child = Command::new(bin())
        .env("HOME", home)
        .env("THRULINE_HOME", home.join(".thruline"))
        .env("PATH", path)
        .env("CARGO_HOME", shim.parent().expect("cargo home"))
        .current_dir(repo)
        .arg("--db")
        .arg(db)
        .args(["check", "--for-change", change, "--actor", "sam", "--"])
        .args(["cargo", "build", "--lib"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the check starts");
    // Owned by the reaping guard from the first instant, so no path out of
    // this function — including the assertion below — can leave it running.
    let check = Check { child };

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(ledger) = thruline_core::ledger::Ledger::open(db) {
            let key: Option<String> = ledger
                .conn()
                .query_row(
                    "SELECT run_key FROM change_run_executions WHERE state='running'
                      ORDER BY rowid DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .ok();
            if let Some(key) = key {
                return (check, key);
            }
        }
        assert!(
            Instant::now() < deadline,
            "the wrapper never recorded a running check"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// A running `thruline ui` on the loopback, killed when the test ends.
struct Server {
    child: Child,
    port: u16,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    fn start(home: &Path, repo: &Path, db: &Path) -> Server {
        let mut child = Command::new(bin())
            .env("HOME", home)
            .env("THRULINE_HOME", home.join(".thruline"))
            .current_dir(repo)
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
        let mut seen = String::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(line) => {
                    seen.push_str(&line);
                    seen.push('\n');
                    if let Some(rest) = line.trim().strip_prefix("ThruLine is at http://127.0.0.1:")
                    {
                        let port: u16 = rest
                            .trim()
                            .split('/')
                            .next()
                            .unwrap()
                            .parse()
                            .expect("port in the printed URL");
                        return Server { child, port };
                    }
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let mut err = String::new();
                    if let Some(mut e) = child.stderr.take() {
                        let _ = e.read_to_string(&mut err);
                    }
                    panic!("`thruline ui` never printed a loopback URL.\n{seen}\n{err}");
                }
            }
        }
    }

    fn get(&self, path: &str) -> String {
        let mut socket = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            self.port
        );
        socket.write_all(request.as_bytes()).expect("request");
        let mut raw = Vec::new();
        socket.read_to_end(&mut raw).expect("response");
        let split = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .expect("header terminator");
        String::from_utf8_lossy(&raw[split + 4..]).to_string()
    }
}

/// What the served readiness document must say about a check that stopped —
/// asserted on the JSON the route actually serves, not on a struct a test
/// built.
fn assert_the_stopped_check_is_named(body: &str) {
    assert!(
        body.contains("\"unsatisfied_slug\":\"readiness_stopped\""),
        "the stopped check must be named as stopped. Served readiness:\n{body}"
    );
    assert!(
        !body.contains("\"unsatisfied_slug\":\"readiness_settled_nothing\""),
        "a stopped check must never read as 'something ran and settled nothing'. \
         Served readiness:\n{body}"
    );
    assert!(
        body.contains("\"ready\":false"),
        "a Change with a stopped mandatory check is never ready:\n{body}"
    );
}

/// The card's first named case: a person stops a running check, and the screen
/// says so. Driven end to end — `check`, `cancel-run`, `GET /readiness`.
#[cfg(unix)]
#[test]
fn a_check_a_person_stopped_is_named_by_the_served_readiness() {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path();
    let (repo, db) = fixture(dir.path());
    let shim = a_cargo_that_hangs(dir.path());
    let change = an_accepted_change(home, &repo, &db);

    let (mut check, run_key) = a_check_in_flight(home, &repo, &db, &change, &shim);
    let cancelled = run(
        home,
        &repo,
        &db,
        &["change", "cancel-run", "--run", &run_key, "--actor", "sam"],
    );
    assert_eq!(
        cancelled.code, 0,
        "{}{}",
        cancelled.stdout, cancelled.stderr
    );
    check.finished();

    // The store's own account of it, first: one run, stopped by a person.
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("reopen");
    let record = thruline_core::run_execution::latest(&ledger, &run_key)
        .expect("read")
        .expect("a row");
    assert_eq!(
        record.state,
        thruline_core::run_execution::ExecutionState::Cancelled
    );
    // The half that made this invisible for two rounds: the row that names
    // what became of the run names no binding, so a binding-keyed lookup
    // cannot find it. If this ever stops being true the fallback below is
    // dead code and this test should be revisited rather than deleted.
    assert!(
        record.binding_id.is_none(),
        "a cancellation lands before the wrapper has a binding to name"
    );
    drop(ledger);

    let server = Server::start(home, &repo, &db);
    let body = server.get(&format!("/api/changes/{change}/readiness"));
    assert_the_stopped_check_is_named(&body);
}

/// The second shape: nobody cancelled anything, the check's child was killed
/// outright. The wrapper records `interrupted` itself, and readiness must name
/// that too — the person is owed the same fact.
#[cfg(unix)]
#[test]
fn a_check_killed_outright_is_named_by_the_served_readiness() {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path();
    let (repo, db) = fixture(dir.path());
    let shim = a_cargo_that_hangs(dir.path());
    let change = an_accepted_change(home, &repo, &db);

    let (mut check, run_key) = a_check_in_flight(home, &repo, &db, &change, &shim);
    let pid: i64 = {
        let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
        let record = thruline_core::run_execution::latest(&ledger, &run_key)
            .expect("read")
            .expect("a row");
        record.pid.expect("the running row records the child's pid")
    };
    let killed = Command::new("kill")
        .args(["-9", &pid.to_string()])
        .output()
        .expect("kill runs");
    assert!(killed.status.success(), "kill -9 {pid}: {killed:?}");
    check.finished();

    let ledger = thruline_core::ledger::Ledger::open(&db).expect("reopen");
    let record = thruline_core::run_execution::latest(&ledger, &run_key)
        .expect("read")
        .expect("a row");
    assert_eq!(
        record.state,
        thruline_core::run_execution::ExecutionState::Interrupted,
        "a child that died to a signal is interrupted, never completed"
    );
    drop(ledger);

    let server = Server::start(home, &repo, &db);
    let body = server.get(&format!("/api/changes/{change}/readiness"));
    assert_the_stopped_check_is_named(&body);
}
