//! BUILD-SPEC §4.4 — the PATH shim, exercised end to end through the real
//! binary against real runners.
//!
//! Nothing here mocks a runner. BUILD-PLAN F's anti-evidence column for M0.8 is
//! explicit that a mocked runner and the shim's own log prove nothing, so every
//! assertion below is either against a program's true observable behaviour
//! (its exit status, its output measured independently) or against the spool
//! the shim wrote, read back by a separate process.
//!
//! `pytest` is used where a real test runner is needed. If it is not installed
//! the test **fails loudly rather than skipping silently** — a green suite that
//! quietly did not run its central case is the failure this product is about —
//! except where the harness explicitly opts out with `THRULINE_TEST_NO_PYTEST`,
//! which prints why.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

fn thruline() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

struct Fixture {
    _dir: tempfile::TempDir,
    home: PathBuf,
    repo: PathBuf,
    shim: PathBuf,
}

/// The wrappers this suite's *mechanics* tests need to exist regardless of what
/// the host has installed.
///
/// Deliberately not `all`: writing all 15 puts wrappers for `uv`, `python3` and
/// friends on the fixture's PATH, which changes what a `run-tests`-shaped child
/// resolves and would make these tests measure something other than the
/// wrapper.
const MECHANICS: &str = "cargo,make,npm,pnpm,pytest";

impl Fixture {
    /// A fixture whose wrappers exist because this suite asked for them.
    ///
    /// **Which policy the install uses is part of what each test is asserting,
    /// so it is stated rather than inherited.** Under the default
    /// `WrapPolicy::Resolvable` a wrapper exists only where the host happens to
    /// have the program, so a test that asserts *"a modified `cargo` wrapper is
    /// reported"* quietly becomes *"an install with no `cargo` wrapper reports
    /// nothing"* on a machine without cargo — and passes or fails for reasons
    /// that have nothing to do with the property. Measured at `9a7cfb6`: this
    /// file passed 28/28 on a developer's machine and produced **12 failures**
    /// under `env -i PATH=/usr/bin:/bin`, with nothing in the output naming the
    /// host's PATH as the variable.
    ///
    /// Tests that are *about* the policy use [`Fixture::resolvable`].
    fn new() -> Fixture {
        Fixture::with_wrap(Some(MECHANICS))
    }

    /// A fixture installed under the shipped default, `WrapPolicy::Resolvable`.
    /// For the tests whose subject IS that predicate.
    fn resolvable() -> Fixture {
        Fixture::with_wrap(None)
    }

    fn with_wrap(wrap: Option<&str>) -> Fixture {
        let dir = tempfile::tempdir().expect("tempdir");
        let home = dir.path().join("home");
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).expect("repo dir");
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.email", "t@e.invalid"],
            vec!["config", "user.name", "T"],
        ] {
            let out = Command::new("git")
                .current_dir(&repo)
                .args(&args)
                .output()
                .expect("git");
            assert!(out.status.success(), "git {args:?}");
        }
        std::fs::write(repo.join(".gitignore"), "__pycache__/\n.pytest_cache/\n").expect("write");
        let out = Command::new("git")
            .current_dir(&repo)
            .args(["add", "-A"])
            .output()
            .expect("git add");
        assert!(out.status.success());
        let out = Command::new("git")
            .current_dir(&repo)
            .args(["commit", "-qm", "first"])
            .output()
            .expect("git commit");
        assert!(out.status.success());

        let shim = home.join("shim");
        let mut cmd = Command::new(thruline());
        cmd.args(["shim", "install", "--session", "sess-test"]);
        if let Some(spec) = wrap {
            cmd.args(["--wrap", spec]);
        }
        let out = cmd
            .env("THRULINE_HOME", &home)
            .current_dir(&repo)
            .output()
            .expect("shim install");
        assert!(
            out.status.success(),
            "shim install: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        Fixture {
            _dir: dir,
            home,
            repo,
            shim,
        }
    }

    /// Install a real program at `fakebin/<name>` so a wrapped name resolves
    /// to it. Used where the property under test is the wrapper's transparency
    /// rather than a runner's output — the child is a real process with a real
    /// exit status, which is what those assertions are about.
    ///
    /// `make` cannot serve here: it maps a recipe's exit 1 to its own exit 2,
    /// so measuring pass-through through `make` measures `make`.
    fn fake_program(&self, name: &str, body: &str) -> PathBuf {
        let bin = self.home.join("fakebin");
        std::fs::create_dir_all(&bin).expect("fakebin");
        let path = bin.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write fake");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        path
    }

    /// Run a wrapped program with `fakebin` behind the shim on PATH.
    fn run_with_fakebin(&self, program: &str, args: &[&str]) -> std::process::Output {
        let path = format!(
            "{}:{}:{}",
            self.shim.display(),
            self.home.join("fakebin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        Command::new(self.shim.join(program))
            .args(args)
            .current_dir(&self.repo)
            .env("THRULINE_HOME", &self.home)
            // The suite runs INSIDE a Claude Code session, so this is set in
            // the inherited environment and would otherwise win the binding
            // for every fixture. A test must control its own attribution.
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .env("PATH", path)
            .output()
            .unwrap_or_else(|e| panic!("running {program}: {e}"))
    }

    /// Run a command with the shim first on PATH, from the repo.
    fn run(&self, program: &str, args: &[&str]) -> std::process::Output {
        let path = format!(
            "{}:{}",
            self.shim.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        Command::new(self.shim.join(program))
            .args(args)
            .current_dir(&self.repo)
            .env("THRULINE_HOME", &self.home)
            // The suite runs INSIDE a Claude Code session, so this is set in
            // the inherited environment and would otherwise win the binding
            // for every fixture. A test must control its own attribution.
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .env("PATH", path)
            .output()
            .unwrap_or_else(|e| panic!("running {program}: {e}"))
    }

    fn spool(&self) -> Vec<serde_json::Value> {
        let path = self.home.join("spool").join("claude-code.jsonl");
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Vec::new();
        };
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).expect("spool line is JSON"))
            .collect()
    }

    /// Every `exec` payload the recorder wrote, in order.
    fn execs(&self) -> Vec<serde_json::Value> {
        self.spool()
            .into_iter()
            .filter(|v| v["spec_payload"]["kind"] == "exec")
            .map(|v| v["spec_payload"]["payload"].clone())
            .collect()
    }

    fn fold(&self) -> serde_json::Value {
        let out = Command::new(thruline())
            .args(["fold", "--json"])
            .current_dir(&self.repo)
            .env("THRULINE_HOME", &self.home)
            // The suite runs INSIDE a Claude Code session, so this is set in
            // the inherited environment and would otherwise win the binding
            // for every fixture. A test must control its own attribution.
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .output()
            .expect("fold");
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "fold --json: {e}\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        })
    }
}

fn pytest_available() -> bool {
    Command::new("pytest")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Fail loudly when the real runner is missing, unless the operator opted out.
fn require_pytest() -> bool {
    if pytest_available() {
        return true;
    }
    if std::env::var_os("THRULINE_TEST_NO_PYTEST").is_some() {
        eprintln!(
            "SKIPPED: pytest is not installed and THRULINE_TEST_NO_PYTEST is set. \
             The §4.4 shim's central case did NOT run in this suite."
        );
        return false;
    }
    panic!(
        "pytest is not on PATH. M0.8's evidence requires a real runner; set \
         THRULINE_TEST_NO_PYTEST=1 to skip it deliberately and see the skip \
         printed, rather than having it pass quietly."
    );
}

// ---------------------------------------------------------------------------
// Transparency: the property whose loss would destroy the evidence
// ---------------------------------------------------------------------------

#[test]
fn the_shim_returns_the_child_s_exit_code_unchanged_across_the_whole_range() {
    // A wrapper that eats a non-zero exit destroys exactly the evidence this
    // product collects — the same shape as `PostToolUse` firing only on
    // success. Checked across the range rather than at 0 and 1, because a
    // truncating, clamping or sign-extending bug shows up in the middle.
    let f = Fixture::new();
    f.fake_program("pnpm", "exit \"$1\"");
    for code in [0u8, 1, 2, 3, 42, 77, 126, 127, 128, 200, 254, 255] {
        let out = f.run_with_fakebin("pnpm", &[&code.to_string()]);
        assert_eq!(
            out.status.code(),
            Some(i32::from(code)),
            "the shim changed exit code {code}"
        );
        assert_eq!(
            out.status.signal_or_none(),
            None,
            "exit {code} was reported as a signal death"
        );
    }
    // And every one of them is in the record with the code it really had —
    // a recorder that only sees green runs is worse than none.
    let codes: Vec<i64> = f
        .execs()
        .iter()
        .filter(|e| e["argv"][0] == "pnpm")
        .filter_map(|e| e["exit_code"].as_i64())
        .collect();
    assert_eq!(
        codes,
        vec![0, 1, 2, 3, 42, 77, 126, 127, 128, 200, 254, 255]
    );
}

/// `ExitStatus::signal()` behind a name that compiles on both tiers.
trait SignalOrNone {
    fn signal_or_none(&self) -> Option<i32>;
}
impl SignalOrNone for std::process::ExitStatus {
    #[cfg(unix)]
    fn signal_or_none(&self) -> Option<i32> {
        use std::os::unix::process::ExitStatusExt;
        self.signal()
    }
    #[cfg(not(unix))]
    fn signal_or_none(&self) -> Option<i32> {
        None
    }
}

#[test]
fn a_child_killed_by_a_signal_is_reported_as_the_shell_reports_it_and_recorded() {
    // 128 + signal is what a POSIX shell puts in `$?`. It is NOT identical to
    // re-raising the signal — a parent using `waitpid` sees WIFEXITED — and the
    // event records `signal` so the difference is legible rather than lost.
    let f = Fixture::new();
    f.fake_program("pnpm", "kill -TERM $$");
    let out = f.run_with_fakebin("pnpm", &[]);
    assert_eq!(
        out.status.code(),
        Some(128 + 15),
        "a signal death did not surface as 128 + SIGTERM"
    );
    let exec = f
        .execs()
        .into_iter()
        .find(|e| e["argv"][0] == "pnpm")
        .expect("the exec was recorded");
    assert_eq!(exec["signal"], serde_json::json!(15));
    assert_eq!(exec["exit_code"], serde_json::json!(143));
    assert!(
        exec["exit_code_source"]
            .as_str()
            .unwrap_or_default()
            .contains("128 + signal"),
        "the synthesised exit code is not labelled as the wrapper's"
    );
    // The difference this trades away, asserted so it is a known one: the
    // wrapper EXITS with 143, it does not die of SIGTERM.
    assert_eq!(
        out.status.signal_or_none(),
        None,
        "the wrapper re-raised the signal, which `unsafe_code = \"forbid\"` \
         should make impossible"
    );
}

#[test]
fn stdout_and_stderr_reach_the_caller_unmodified_and_on_the_right_descriptors() {
    let f = Fixture::new();
    std::fs::write(
        f.repo.join("Makefile"),
        "talk:\n\t@sh -c 'printf out-only; printf err-only >&2'\n",
    )
    .expect("write makefile");
    let out = f.run("make", &["talk"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout), "out-only");
    assert_eq!(String::from_utf8_lossy(&out.stderr), "err-only");
}

#[test]
fn a_program_that_does_not_exist_behaves_as_it_would_without_the_shim() {
    // 127 is what a shell reports for "not found". A wrapper that returned 0
    // here would make a missing tool look like a successful run.
    //
    // Under `WrapPolicy::Resolvable` a wrapper exists only where a real
    // program of that name resolved at install time, so this test has to
    // make one and then take it away. That is not a workaround for the
    // policy — it is the only shape in which this property is still
    // reachable: present at install, gone at run. Where the program was
    // absent all along there is no wrapper, and "behaves as it would
    // without the shim" is then true by construction rather than by 127.
    //
    // **`resolvable`, because the predicate is the subject here.** `gradle` is
    // outside `MECHANICS` either way, but stating the policy is the point: this
    // is the one test in the file that would be measuring something else if the
    // wrapper were written unconditionally.
    let f = Fixture::resolvable();
    f.fake_program("gradle", "exit 0");
    let install_path = format!(
        "{}:{}",
        f.home.join("fakebin").display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new(thruline())
        .args(["shim", "install", "--session", "sess-test"])
        .env("THRULINE_HOME", &f.home)
        .env("PATH", &install_path)
        .current_dir(&f.repo)
        .output()
        .expect("re-install with gradle resolvable");
    assert!(
        out.status.success(),
        "re-install: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        f.shim.join("gradle").exists(),
        "the wrapper this test is about was not written even though gradle resolved"
    );
    std::fs::remove_file(f.home.join("fakebin").join("gradle")).expect("take gradle away");

    let path = format!("{}:/nonexistent-dir-for-this-test", f.shim.display());
    let out = Command::new(f.shim.join("gradle"))
        .arg("build")
        .current_dir(&f.repo)
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .env("PATH", path)
        .output()
        .expect("run");
    assert_eq!(out.status.code(), Some(127));
    let exec = f
        .execs()
        .into_iter()
        .find(|e| e["argv"][0] == "gradle")
        .expect("the attempt was still recorded");
    assert_eq!(
        exec["program_resolved"],
        serde_json::Value::Null,
        "a path was invented for a program that was not found"
    );
}

// ---------------------------------------------------------------------------
// Recursion: the control that is a mechanism, not an instruction
// ---------------------------------------------------------------------------

#[test]
fn a_shim_copied_elsewhere_on_path_cannot_make_the_recorder_re_enter_itself() {
    // The path comparison alone is defeated by a copy. The content marker is
    // not, and this is the test that says so: a second directory holding a copy
    // of the wrapper, ahead of the real program on PATH, must not produce an
    // infinite regress — it must resolve past both copies.
    let f = Fixture::new();
    let decoy = f.home.join("decoy");
    std::fs::create_dir_all(&decoy).expect("decoy dir");
    let wrapper = std::fs::read(f.shim.join("make")).expect("read wrapper");
    std::fs::write(decoy.join("make"), &wrapper).expect("copy wrapper");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(decoy.join("make"), std::fs::Permissions::from_mode(0o755))
            .expect("chmod");
    }
    std::fs::write(f.repo.join("Makefile"), "ok:\n\t@printf reached\n").expect("write makefile");

    let path = format!(
        "{}:{}:{}",
        f.shim.display(),
        decoy.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new(f.shim.join("make"))
        .arg("ok")
        .current_dir(&f.repo)
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .env("PATH", path)
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "reached");
}

// ---------------------------------------------------------------------------
// The evidence itself
// ---------------------------------------------------------------------------

#[test]
fn a_real_pytest_run_produces_an_exec_event_snapshots_and_a_parsed_check() {
    if !require_pytest() {
        return;
    }
    let f = Fixture::new();
    std::fs::write(
        f.repo.join("test_sample.py"),
        "def test_one():\n    assert 1 + 1 == 2\n\ndef test_two():\n    assert 'a' in 'abc'\n",
    )
    .expect("write test");

    let out = f.run("pytest", &["-q"]);
    assert!(
        out.status.success(),
        "pytest failed: {}",
        String::from_utf8_lossy(&out.stdout)
    );

    // The digest is over the redacted stream. With no secrets in it, redaction
    // is the identity, so the digest must equal an INDEPENDENTLY computed
    // sha256 of the bytes the caller saw — the shim's own log is not evidence
    // for itself.
    let exec = f.execs().pop().expect("an exec was recorded");
    let independent = format!("sha256:{}", thruline_core::chain::sha256_hex(&out.stdout));
    assert_eq!(
        exec["stdout_hash"],
        serde_json::json!(independent),
        "the recorded digest does not match the output the caller received"
    );
    assert_eq!(exec["exit_code"], serde_json::json!(0));
    assert_eq!(exec["argv"], serde_json::json!(["pytest", "-q"]));
    assert!(exec["input_tree_hash"]
        .as_str()
        .is_some_and(|h| h.len() == 40));
    assert!(exec["output_tree_hash"]
        .as_str()
        .is_some_and(|h| h.len() == 40));
    assert!(
        exec["pid"].as_u64().unwrap_or(0) > 0 && exec["parent"].as_u64().unwrap_or(0) > 0,
        "the process tree was not recorded"
    );

    // And the whole thing folds into a chain with a check carrying real
    // numbers — M0's done-when, minus the hook half.
    let report = f.fold();
    assert_eq!(report["exec_events"], serde_json::json!(1));
    assert_eq!(report["checks_written"], serde_json::json!(1));
    assert_eq!(report["ok"], serde_json::json!(true));
    assert!(report["snapshots_taken"].as_u64().unwrap_or(0) >= 1);

    let doctor = Command::new(thruline())
        .args(["doctor", "--json"])
        .current_dir(&f.repo)
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .env(
            "PATH",
            format!(
                "{}:{}",
                f.shim.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .output()
        .expect("doctor");
    let value: serde_json::Value = serde_json::from_slice(&doctor.stdout).expect("doctor --json");
    let chain = value["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .find(|f| f["key"] == "chain")
        .cloned()
        .expect("a chain finding");
    assert_eq!(chain["ok"], serde_json::json!(true), "{chain}");
    assert!(
        chain["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("verify (recomputed, not read)"),
        "{chain}"
    );
}

#[test]
fn a_failing_run_is_captured_with_its_failure_intact() {
    // The direction that matters. A recorder that only sees green runs is worse
    // than none.
    if !require_pytest() {
        return;
    }
    let f = Fixture::new();
    std::fs::write(
        f.repo.join("test_sample.py"),
        "def test_one():\n    assert 1 + 1 == 2\n\ndef test_bad():\n    assert 'z' in 'abc'\n",
    )
    .expect("write test");
    let out = f.run("pytest", &["-q"]);
    assert_eq!(out.status.code(), Some(1));

    f.fold();
    let db = f.home.join("exec.db");
    let ledger = thruline_ledger::Ledger::open(&db).expect("open store");
    let checks = ledger
        .checks_for_session("sess-test")
        .expect("checks")
        .into_iter()
        .collect::<Vec<_>>();
    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0].exit_code, 1);
    let parsed: serde_json::Value = serde_json::from_str(&checks[0].parsed).expect("parsed JSON");
    assert_eq!(parsed["failed"], serde_json::json!(1));
    assert_eq!(parsed["passed"], serde_json::json!(1));
}

#[test]
fn a_runner_reached_through_a_nested_shell_is_captured_at_the_exec_level() {
    // BUILD-PLAN M0.8 done-when (b). The shim is not pattern-matching the
    // command string: `sh` resolves `pytest` through PATH and hits the wrapper.
    if !require_pytest() {
        return;
    }
    let f = Fixture::new();
    std::fs::write(
        f.repo.join("test_sample.py"),
        "def test_one():\n    assert True\n",
    )
    .expect("write test");
    let path = format!(
        "{}:{}",
        f.shim.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new("sh")
        .args(["-c", "pytest -q && echo ok"])
        .current_dir(&f.repo)
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .env("PATH", path)
        .output()
        .expect("sh");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("ok"));

    let execs = f.execs();
    assert_eq!(execs.len(), 1, "the nested pytest was not captured");
    assert_eq!(execs[0]["argv"], serde_json::json!(["pytest", "-q"]));
}

#[test]
fn a_nested_runner_records_its_parent_so_the_process_tree_reconstructs() {
    // `make` → `pytest`: the inner exec's `parent` is the outer exec's `pid`.
    // That is what "record child processes" (§13) means without a tracer.
    if !require_pytest() {
        return;
    }
    let f = Fixture::new();
    std::fs::write(
        f.repo.join("test_sample.py"),
        "def test_one():\n    assert True\n",
    )
    .expect("write test");
    std::fs::write(f.repo.join("Makefile"), "test:\n\t@pytest -q\n").expect("write makefile");
    let out = f.run("make", &["test"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let execs = f.execs();
    let by_program: BTreeMap<String, serde_json::Value> = execs
        .iter()
        .map(|e| (e["argv"][0].as_str().unwrap_or("?").to_string(), e.clone()))
        .collect();
    let make = by_program.get("make").expect("the make exec");
    let pytest = by_program.get("pytest").expect("the nested pytest exec");
    assert_eq!(
        pytest["parent"], make["pid"],
        "the nested run's parent is not the outer run's pid, so the tree does \
         not reconstruct"
    );
    assert_eq!(pytest["depth"], serde_json::json!(1));
    assert_eq!(make["depth"], serde_json::json!(0));
}

// ---------------------------------------------------------------------------
// Redaction before hashing, on the path where a credential most plausibly is
// ---------------------------------------------------------------------------

#[test]
fn a_credential_in_output_argv_or_environment_reaches_no_file_the_shim_writes() {
    // Four canaries, three routes in. Measured by ABSENCE across every artifact
    // the shim produces — spool, output store, quarantine, ledger — rather than
    // by counting `[masked: …]` markers, because a marker-counting metric once
    // reported 17 of 17 masked when 4 were safe.
    let f = Fixture::new();
    let aws = "AKIAIOSFODNN7EXAMPLE";
    let aws_secret = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
    let gh = "ghp_16C7e42F292c6912E7710c838347Ae178B4a";
    let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxIn0.dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    std::fs::write(
        f.repo.join("Makefile"),
        format!(
            "leak:\n\t@printf 'Authorization: Bearer {jwt}\\naws_secret_access_key={aws_secret}\\ntoken={gh}\\n'\n"
        ),
    )
    .expect("write makefile");

    let path = format!(
        "{}:{}",
        f.shim.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new(f.shim.join("make"))
        .args(["leak", &format!("AWS_ACCESS_KEY_ID={aws}")])
        .current_dir(&f.repo)
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .env("PATH", path)
        .env("AWS_SECRET_ACCESS_KEY", aws_secret)
        .env("GITHUB_TOKEN", gh)
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The caller's own terminal is NOT redacted — that is deliberate, and the
    // control for the assertions below: the canaries really were produced.
    let seen = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(seen.contains(gh), "the canary never reached the output");
    assert!(
        seen.contains(aws_secret),
        "the aws-secret canary never reached the output, so its absence below \
         would prove nothing"
    );

    f.fold();
    let mut checked = 0usize;
    for path in files_under(&f.home) {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        checked += 1;
        for (name, canary) in [
            ("aws key id", aws),
            ("aws secret", aws_secret),
            ("github token", gh),
            ("jwt", jwt),
        ] {
            assert!(
                !leaks(&text, canary),
                "the {name} canary survived into {}",
                path.display()
            );
        }
    }
    assert!(checked > 3, "only {checked} files were scanned");
}

/// Sliding window over the whole canary, not a `contains` of the first line:
/// A209-C11 measured two regressions that were at the body's end.
fn leaks(hay: &str, needle: &str) -> bool {
    const WINDOW: usize = 16;
    let chars: Vec<char> = needle.chars().collect();
    if chars.len() < WINDOW {
        return hay.contains(needle);
    }
    (0..=chars.len() - WINDOW).any(|i| {
        let w: String = chars[i..i + WINDOW].iter().collect();
        hay.contains(&w)
    })
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(files_under(&path));
        } else {
            out.push(path);
        }
    }
    out
}

#[test]
fn the_environment_digest_is_over_the_redacted_environment() {
    // The mechanical proof of the ordering, and it costs something real: two
    // environments differing only in a masked value hash the SAME. Stated as a
    // test so the cost is visible and the ordering cannot regress silently.
    let f = Fixture::new();
    std::fs::write(f.repo.join("Makefile"), "noop:\n\t@true\n").expect("write makefile");
    let path = format!(
        "{}:{}",
        f.shim.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut hashes = Vec::new();
    for secret in [
        "ghp_16C7e42F292c6912E7710c838347Ae178B4a",
        "ghp_ZZZZe42F292c6912E7710c838347Ae178B4a",
    ] {
        let out = Command::new(f.shim.join("make"))
            .arg("noop")
            .current_dir(&f.repo)
            .env("THRULINE_HOME", &f.home)
            // The suite runs INSIDE a Claude Code session, so this is set in
            // the inherited environment and would otherwise win the binding
            // for every fixture. A test must control its own attribution.
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .env("PATH", &path)
            .env("GITHUB_TOKEN", secret)
            .output()
            .expect("run");
        assert!(out.status.success());
        hashes.push(
            f.execs()
                .pop()
                .expect("an exec")
                .get("env_hash")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
        );
    }
    assert_eq!(
        hashes[0], hashes[1],
        "two different secrets produced two different env digests, so the raw \
         value entered the hash"
    );
    assert!(hashes[0].starts_with("sha256:"));
}

// ---------------------------------------------------------------------------
// Bounds
// ---------------------------------------------------------------------------

#[test]
fn a_hundred_megabytes_of_stdout_is_digested_without_being_held() {
    // BUILD-PLAN M0.8(c). The digest is checked against an independently
    // computed one rather than against the recorder's own report — with no
    // secrets in the stream, redaction is the identity, so the two must agree
    // exactly.
    let f = Fixture::new();
    std::fs::write(
        f.repo.join("Makefile"),
        // 100 MiB: 1 MiB of a repeating line, a hundred times.
        "flood:\n\t@sh -c 'line=$$(printf \"%01048575d\" 0); i=0; while [ $$i -lt 100 ]; \
         do printf \"%s\\n\" \"$$line\"; i=$$((i+1)); done'\n",
    )
    .expect("write makefile");
    let out = f.run("make", &["flood"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        out.stdout.len(),
        100 * 1024 * 1024,
        "the flood was not 100 MiB"
    );

    let exec = f.execs().pop().expect("an exec");
    assert_eq!(
        exec["stdout_hash"],
        serde_json::json!(format!(
            "sha256:{}",
            thruline_core::chain::sha256_hex(&out.stdout)
        )),
        "the streamed digest does not match the bytes the caller received"
    );
    assert_eq!(
        exec["stdout_bytes"],
        serde_json::json!(100u64 * 1024 * 1024)
    );
    // The event stays small: §3.1 says bounded excerpts, and an event carrying
    // 100 MB would be an event nothing can read.
    let excerpt = exec["stdout_excerpt"].as_str().unwrap_or_default();
    assert!(
        excerpt.len() <= thruline_capture::stream::EXCERPT_BOUND + 64,
        "the excerpt is {} bytes",
        excerpt.len()
    );
}

// ---------------------------------------------------------------------------
// Install, integrity, undo
// ---------------------------------------------------------------------------

#[test]
fn a_modified_wrapper_is_reported_by_doctor_and_by_status() {
    // §13: "hooks removed or PATH shims bypassed". A wrapper that records
    // something else, or nothing, must not look like an intact install.
    let f = Fixture::new();
    let out = Command::new(thruline())
        .args(["shim", "status", "--json"])
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .env(
            "PATH",
            format!(
                "{}:{}",
                f.shim.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .output()
        .expect("status");
    assert!(out.status.success(), "a fresh install is not ok");

    std::fs::write(f.shim.join("cargo"), "#!/bin/sh\nexec /usr/bin/true\n").expect("tamper");
    let out = Command::new(thruline())
        .args(["shim", "status", "--json"])
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .output()
        .expect("status");
    assert!(
        !out.status.success(),
        "a tampered wrapper reported as intact"
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(value["modified"], serde_json::json!(["cargo"]));
}

#[test]
fn a_shim_that_is_not_on_path_is_reported_even_though_every_file_is_intact() {
    // The failure that looks identical to "no tests were run" from the
    // record's side, and the only place it can be caught is here.
    let f = Fixture::new();
    let out = Command::new(thruline())
        .args(["shim", "status", "--json"])
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("status");
    assert!(!out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(value["on_path"], serde_json::json!(false));
    assert!(value["modified"].as_array().is_some_and(|a| a.is_empty()));
}

#[test]
fn a_shim_that_wraps_nothing_is_reported_by_status_and_by_doctor() {
    // **The loud-to-silent inversion the amended predicate opened.** Under
    // `WrapPolicy::All` an install always wrote 15 wrappers, so `intact == 0`
    // meant `missing == 15` and this state could not be reached wearing a green
    // check. Under `Resolvable` an install where no candidate resolves writes
    // zero wrappers and records zero, so `missing` is empty as well — and the
    // measured result at `9a7cfb6` was `shim status` printing `intact 0 / on
    // PATH true`, exit 0, and `doctor` reporting `{"key":"shim","ok":true}`.
    //
    // No tampering is needed to produce it. It is what a container whose `uv`
    // provisioning silently failed looks like.
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    let empty = dir.path().join("emptybin");
    std::fs::create_dir_all(&empty).expect("emptybin");
    let shim = home.join("shim");

    let out = Command::new(thruline())
        .args(["shim", "install"])
        .env("THRULINE_HOME", &home)
        .env("PATH", empty.display().to_string())
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .current_dir(dir.path())
        .output()
        .expect("shim install");
    assert!(
        out.status.success(),
        "install must still succeed — it wrote what the predicate allowed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // With the shim FIRST on PATH, so `on_path` is true and the only thing
    // wrong is that there is nothing in it.
    let path = format!("{}:{}", shim.display(), empty.display());
    let status = Command::new(thruline())
        .args(["shim", "status", "--json"])
        .env("THRULINE_HOME", &home)
        .env("PATH", &path)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .output()
        .expect("status");
    let value: serde_json::Value = serde_json::from_slice(&status.stdout).expect("json");
    assert_eq!(value["intact"], serde_json::json!([]), "{value}");
    assert_eq!(value["missing"], serde_json::json!([]), "{value}");
    assert_eq!(value["on_path"], serde_json::json!(true), "{value}");
    assert_eq!(
        value["captures_nothing"],
        serde_json::json!(true),
        "a shim wrapping nothing did not say so: {value}"
    );
    assert!(
        !status.status.success(),
        "`shim status` exited 0 for a capture layer that captures nothing: {value}"
    );

    let doctor = Command::new(thruline())
        .args(["doctor", "--json"])
        .env("THRULINE_HOME", &home)
        .env("PATH", &path)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .current_dir(dir.path())
        .output()
        .expect("doctor");
    let report: serde_json::Value = serde_json::from_slice(&doctor.stdout).expect("doctor json");
    let shim_finding = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .find(|f| f["key"] == "shim")
        .cloned()
        .unwrap_or_else(|| panic!("no shim finding in {report}"));
    assert_eq!(
        shim_finding["ok"],
        serde_json::json!(false),
        "doctor called a shim wrapping nothing healthy: {shim_finding}"
    );
    assert!(
        shim_finding["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("CAPTURES NOTHING"),
        "doctor did not name the degradation: {shim_finding}"
    );
}

#[test]
fn doctor_reports_a_forged_wrapper_by_name() {
    // `ShimIntegrity::forged` failed `ok()` and was printed by `shim status`,
    // and `doctor` — the command a user actually runs — read it out of the
    // struct and said nothing about it, reporting only that some unnamed
    // problem existed. A forgery field nothing names is not a guard.
    let f = Fixture::new();
    // `go` is a candidate this install declined to write, so a file appearing
    // under that name inside OUR directory is a forgery by the manifest's own
    // definition — no editing of a real wrapper required.
    std::fs::write(f.shim.join("go"), "#!/bin/sh\nexit 0\n").expect("plant");
    let out = Command::new(thruline())
        .args(["doctor", "--json"])
        .env("THRULINE_HOME", &f.home)
        .env("PATH", format!("{}:/usr/bin:/bin", f.shim.display()))
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .current_dir(&f.repo)
        .output()
        .expect("doctor");
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).expect("doctor json");
    let finding = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .find(|f| f["key"] == "shim")
        .cloned()
        .unwrap_or_else(|| panic!("no shim finding in {report}"));
    let detail = finding["detail"].as_str().unwrap_or_default();
    assert_eq!(finding["ok"], serde_json::json!(false), "{finding}");
    assert!(
        detail.contains("FORGED") && detail.contains("go"),
        "doctor did not name the forged wrapper: {finding}"
    );
}

#[test]
fn doctor_names_a_candidate_that_resolves_now_but_was_skipped_at_install() {
    // `ShimIntegrity::uncovered` was also read by nothing in `doctor`. It is an
    // ADVISORY by its own contract — a tool installed after the shim is a
    // reinstall, not a compromise — so it must be printed without failing the
    // check. Printing nothing is the option that is not available: the user's
    // `uv` invocations are invisible and only this line says so.
    //
    // Deterministic by construction: `uv` is skipped because `--wrap cargo`
    // excludes it, then a real `uv` is planted on PATH. Nothing here depends on
    // what the host has.
    let f = Fixture::with_wrap(Some("cargo"));
    let late = f.home.join("late");
    std::fs::create_dir_all(&late).expect("late dir");
    let uv = late.join("uv");
    std::fs::write(&uv, "#!/bin/sh\nexit 0\n").expect("plant uv");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&uv, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let out = Command::new(thruline())
        .args(["doctor", "--json"])
        .env("THRULINE_HOME", &f.home)
        .env(
            "PATH",
            format!("{}:{}:/usr/bin:/bin", f.shim.display(), late.display()),
        )
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .current_dir(&f.repo)
        .output()
        .expect("doctor");
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).expect("doctor json");
    let finding = report["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .find(|f| f["key"] == "shim")
        .cloned()
        .unwrap_or_else(|| panic!("no shim finding in {report}"));
    let detail = finding["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("Not covered") && detail.contains("uv"),
        "doctor said nothing about a resolvable program it is not recording: {finding}"
    );
    assert_eq!(
        finding["ok"],
        serde_json::json!(true),
        "an uncovered candidate was made a failure; its own contract says advisory: {finding}"
    );
}

#[test]
fn wrap_refuses_a_name_that_is_not_a_candidate() {
    // The typo that would otherwise install a shim wrapping nothing and print
    // "wrote 0 wrappers" — the state the test above exists to catch after the
    // fact. Refusing before the write says more and costs less.
    let dir = tempfile::tempdir().expect("tempdir");
    let out = Command::new(thruline())
        .args(["shim", "install", "--wrap", "pytst"])
        .env("THRULINE_HOME", dir.path().join("home"))
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .current_dir(dir.path())
        .output()
        .expect("shim install");
    assert!(!out.status.success(), "a misspelt --wrap was accepted");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("pytst") && err.contains("pytest"),
        "the refusal named neither the typo nor the candidates: {err}"
    );
    assert!(
        !dir.path().join("home").join("shim").exists(),
        "a shim directory was written despite the refusal"
    );
}

#[test]
fn a_shim_shadowed_by_an_earlier_path_entry_is_reported() {
    // On PATH, intact, and never reached. Indistinguishable from "not
    // installed" in the record.
    let f = Fixture::new();
    let earlier = f.home.join("earlier");
    std::fs::create_dir_all(&earlier).expect("dir");
    std::fs::write(earlier.join("cargo"), "#!/bin/sh\nexit 0\n").expect("decoy");
    let out = Command::new(thruline())
        .args(["shim", "status", "--json"])
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .env(
            "PATH",
            format!("{}:{}", earlier.display(), f.shim.display()),
        )
        .output()
        .expect("status");
    assert!(!out.status.success());
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert!(
        value["shadowed_by"].as_array().is_some_and(|a| a
            .iter()
            .any(|s| s.as_str().unwrap_or("").ends_with("/cargo"))),
        "{value}"
    );
}

#[test]
fn install_refuses_a_directory_holding_files_it_did_not_write() {
    // `--dir ~/bin` is a plausible mistake and it is full of the user's own
    // scripts.
    let f = Fixture::new();
    let occupied = f.home.join("bin");
    std::fs::create_dir_all(&occupied).expect("dir");
    std::fs::write(occupied.join("my-script"), "#!/bin/sh\necho mine\n").expect("write");
    let out = Command::new(thruline())
        .args(["shim", "install", "--dir"])
        .arg(&occupied)
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .output()
        .expect("install");
    assert!(
        !out.status.success(),
        "it installed into a foreign directory"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("my-script"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        occupied.join("my-script").is_file() && !occupied.join("cargo").exists(),
        "the refusal was not zero-write"
    );
}

#[test]
fn uninstall_removes_what_it_wrote_and_leaves_what_it_did_not() {
    let f = Fixture::new();
    std::fs::write(f.shim.join("not-ours"), "#!/bin/sh\n").expect("write");
    let out = Command::new(thruline())
        .args(["shim", "uninstall", "--json"])
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .output()
        .expect("uninstall");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(value["left_alone"], serde_json::json!(["not-ours"]));
    assert!(!f.shim.join("cargo").exists());
    assert!(f.shim.join("not-ours").is_file());
}

#[test]
fn uninstall_names_a_wrapper_that_had_been_edited_rather_than_erasing_it_quietly() {
    let f = Fixture::new();
    std::fs::write(f.shim.join("npm"), "#!/bin/sh\nexit 0\n").expect("tamper");
    let out = Command::new(thruline())
        .args(["shim", "uninstall", "--json"])
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .output()
        .expect("uninstall");
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(value["modified_before_removal"], serde_json::json!(["npm"]));
}

#[test]
fn the_wrapper_runs_the_real_program_when_the_recorder_is_missing() {
    // Fail open. If the binary is deleted or replaced by one for another
    // architecture, every `cargo` and `npm` on the machine must keep working.
    let f = Fixture::new();
    let out = Command::new(thruline())
        .args(["shim", "install", "--bin", "/nonexistent/thruline"])
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .current_dir(&f.repo)
        .output()
        .expect("install");
    assert!(out.status.success());
    std::fs::write(f.repo.join("Makefile"), "ok:\n\t@printf reached\n").expect("write makefile");
    let out = f.run("make", &["ok"]);
    assert!(
        out.status.success(),
        "the wrapper broke the command instead of failing open: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "reached");
    assert!(
        f.execs().is_empty(),
        "an exec was recorded without a recorder"
    );
}

#[test]
fn an_exec_recorded_with_no_repository_is_still_recorded_and_says_it_is_unbound() {
    // A snapshot that cannot be taken must be counted, never dropped — and it
    // must not cost the exec event, because "a command ran and we cannot bind
    // it to a tree" is a fact worth having.
    let f = Fixture::new();
    let outside = f.home.join("not-a-repo");
    std::fs::create_dir_all(&outside).expect("dir");
    std::fs::write(outside.join("Makefile"), "ok:\n\t@printf reached\n").expect("write makefile");
    let path = format!(
        "{}:{}",
        f.shim.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new(f.shim.join("make"))
        .arg("ok")
        .current_dir(&outside)
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .env("PATH", path)
        .output()
        .expect("run");
    assert!(out.status.success());
    let exec = f.execs().pop().expect("an exec was recorded");
    assert_eq!(exec["input_tree_hash"], serde_json::Value::Null);
    assert_eq!(exec["output_tree_hash"], serde_json::Value::Null);
    // Not a *failure*: there was no repository, which is different from a
    // repository that could not be snapshotted.
    assert!(exec.get("snapshots_failed").is_none(), "{exec}");
}

// ---------------------------------------------------------------------------
// §13: the bypass the shim exists to make visible
// ---------------------------------------------------------------------------

#[test]
fn a_tool_that_ran_a_test_runner_with_no_exec_event_is_reported_as_a_bypass() {
    // The threat §13 names: the agent can read `.claude/settings.json`, knows
    // ThruLine is there, and runs `/opt/anaconda3/bin/pytest` directly. The
    // wrapper is intact, on PATH, and never reached — which from the record's
    // side is identical to "no tests were run". The only thing that can tell
    // them apart is the tool's own account of what it ran, used *only* to say
    // the record is incomplete.
    let f = Fixture::new();
    let spool = f.home.join("spool").join("claude-code.jsonl");
    let deliver = |json: &str| {
        let mut child = Command::new(thruline())
            .args(["hook", "claude-code"])
            .env("THRULINE_HOME", &f.home)
            // The suite runs INSIDE a Claude Code session, so this is set in
            // the inherited environment and would otherwise win the binding
            // for every fixture. A test must control its own attribution.
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .current_dir(&f.repo)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .expect("hook");
        {
            use std::io::Write;
            let mut stdin = child.stdin.take().expect("stdin");
            stdin.write_all(json.as_bytes()).expect("write");
        }
        assert!(child.wait().expect("wait").success());
    };
    deliver(
        r#"{"hook_event_name":"SessionStart","session_id":"bypassed","transcript_path":"/t","cwd":"/r","source":"startup"}"#,
    );
    deliver(
        r#"{"hook_event_name":"PreToolUse","session_id":"bypassed","transcript_path":"/t","cwd":"/r","tool_name":"Bash","tool_input":{"command":"pytest -q"}}"#,
    );
    assert!(spool.is_file());
    f.fold();

    let out = Command::new(thruline())
        .args(["doctor", "--json"])
        .current_dir(&f.repo)
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .output()
        .expect("doctor");
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let finding = value["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .find(|x| x["key"] == "shim-bypass")
        .cloned()
        .expect("a shim-bypass finding");
    assert_eq!(finding["ok"], serde_json::json!(false), "{finding}");
    assert!(
        finding["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("bypassed"),
        "{finding}"
    );
    assert!(!out.status.success(), "doctor did not fail on a bypass");
}

#[test]
fn a_tool_that_merely_mentions_a_runner_is_not_a_bypass() {
    // The precision half, and the reason the commands are parsed rather than
    // matched. `echo 'pytest passes'` ran no tests, and reporting it would make
    // the control noise a user learns to ignore.
    let f = Fixture::new();
    let deliver = |json: &str| {
        let mut child = Command::new(thruline())
            .args(["hook", "claude-code"])
            .env("THRULINE_HOME", &f.home)
            // The suite runs INSIDE a Claude Code session, so this is set in
            // the inherited environment and would otherwise win the binding
            // for every fixture. A test must control its own attribution.
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .current_dir(&f.repo)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .expect("hook");
        {
            use std::io::Write;
            let mut stdin = child.stdin.take().expect("stdin");
            stdin.write_all(json.as_bytes()).expect("write");
        }
        assert!(child.wait().expect("wait").success());
    };
    deliver(
        r#"{"hook_event_name":"PreToolUse","session_id":"honest","transcript_path":"/t","cwd":"/r","tool_name":"Bash","tool_input":{"command":"echo 'pytest passes'"}}"#,
    );
    deliver(
        r#"{"hook_event_name":"PreToolUse","session_id":"honest","transcript_path":"/t","cwd":"/r","tool_name":"Read","tool_input":{"file_path":"/r/test_pytest.py"}}"#,
    );
    f.fold();

    let out = Command::new(thruline())
        .args(["doctor", "--json"])
        .current_dir(&f.repo)
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .output()
        .expect("doctor");
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let finding = value["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .find(|x| x["key"] == "shim-bypass")
        .cloned()
        .expect("a shim-bypass finding");
    assert_eq!(
        finding["ok"],
        serde_json::json!(true),
        "an echo was reported as a bypassed test run: {finding}"
    );
}

// ---------------------------------------------------------------------------
// Re-entry: the failure that hangs a real shell
// ---------------------------------------------------------------------------

/// A third-party shim of the pyenv / asdf / rbenv / ccache shape: it carries
/// neither our marker nor our path, and it **re-dispatches the bare program
/// name through `PATH`** — straight back into our wrapper.
///
/// The counter exists only so this test terminates. A real one has no such
/// stop, which is why the unguarded case is a hang rather than a loop of six.
fn third_party_shim(dir: &Path, counter: &Path, real: &str) {
    std::fs::create_dir_all(dir).expect("third-party dir");
    let path = dir.join("pytest");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\n\
             n=$(cat '{counter}' 2>/dev/null || echo 0); n=$((n+1)); \
             printf '%s' \"$n\" > '{counter}'\n\
             if [ \"$n\" -ge 6 ]; then exec '{real}' \"$@\"; fi\n\
             exec pytest \"$@\"\n",
            counter = counter.display(),
        ),
    )
    .expect("write third-party shim");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    std::fs::write(counter, "0").expect("counter");
}

#[test]
fn a_third_party_shim_that_redispatches_by_name_cannot_make_us_re_enter() {
    // The HIGH finding, with its own negative control in the same test.
    //
    // Our wrapper resolves past our own directory and past anything carrying
    // our marker — a third-party shim carries neither, so the recorder runs
    // it, it re-dispatches `pytest` through PATH, and PATH finds our wrapper
    // again. Unguarded that is unbounded: measured elsewhere as a 25 s hang
    // with the program never running.
    //
    // The guard is `THRULINE_SHIM_ACTIVE`: a wrapper whose own program is
    // already being recorded fails open, drops our directory from PATH, and
    // execs. We leave the cycle after exactly one entry.
    if !require_pytest() {
        return;
    }
    let real = String::from_utf8_lossy(
        &Command::new("sh")
            .args(["-c", "command -v pytest"])
            .output()
            .expect("locate pytest")
            .stdout,
    )
    .trim()
    .to_string();
    assert!(!real.is_empty(), "pytest is not resolvable");

    let f = Fixture::new();
    std::fs::write(
        f.repo.join("test_sample.py"),
        "def test_one():\n    assert True\n",
    )
    .expect("write test");
    let third = f.home.join("third");
    let counter = f.home.join("counter");
    third_party_shim(&third, &counter, &real);

    let path = format!("{}:{}:/usr/bin:/bin", f.shim.display(), third.display());
    let out = Command::new("sh")
        .args(["-c", "pytest -q"])
        .current_dir(&f.repo)
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .env("PATH", &path)
        .output()
        .expect("run pytest through both shims");

    assert!(
        out.status.success(),
        "the program never ran: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let third_entries: u32 = std::fs::read_to_string(&counter)
        .expect("counter")
        .trim()
        .parse()
        .expect("counter is a number");
    assert!(
        third_entries >= 2,
        "the third-party shim did not re-dispatch, so this test proves nothing \
         about the cycle ({third_entries} entries)"
    );
    assert_eq!(
        f.execs().len(),
        1,
        "we re-entered the cycle: one recorded exec is the guard working, one \
         per third-party entry is the hang"
    );

    // --- negative control: the same fixture with the guard removed ---------
    //
    // Without it our wrapper is entered once per cycle. Written by hand rather
    // than by disabling the guard through a flag, because a switch that turns
    // the guard off is a switch somebody can leave off.
    std::fs::remove_file(f.home.join("spool").join("claude-code.jsonl")).expect("reset spool");
    std::fs::write(&counter, "0").expect("reset counter");
    std::fs::write(
        f.shim.join("pytest"),
        format!(
            "#!/bin/sh\nTHRULINE_SHIM_DIR='{}'\nexport THRULINE_SHIM_DIR\n\
             exec '{}' shim exec --program 'pytest' -- \"$@\"\n",
            f.shim.display(),
            thruline()
        ),
    )
    .expect("write unguarded wrapper");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            f.shim.join("pytest"),
            std::fs::Permissions::from_mode(0o755),
        )
        .expect("chmod");
    }
    let out = Command::new("sh")
        .args(["-c", "pytest -q"])
        .current_dir(&f.repo)
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .env("PATH", &path)
        .output()
        .expect("run through the unguarded wrapper");
    assert!(out.status.success());
    assert_eq!(
        f.execs().len(),
        third_entries as usize,
        "the unguarded wrapper did NOT re-enter once per cycle, so the guarded \
         result above is not evidence that the guard is what stopped it"
    );
}

#[test]
fn the_guard_does_not_cost_the_nested_runner_capture_it_exists_to_protect() {
    // The reason the guard is keyed on the PROGRAM and not on depth alone. A
    // blanket "fail open above depth 1" would silence `make` -> `pytest`, which
    // is most of what §6.2's parsers are there to read.
    if !require_pytest() {
        return;
    }
    let f = Fixture::new();
    std::fs::write(
        f.repo.join("test_sample.py"),
        "def test_one():\n    assert True\n",
    )
    .expect("write test");
    std::fs::write(f.repo.join("Makefile"), "test:\n\t@pytest -q\n").expect("write makefile");
    let out = f.run("make", &["test"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let programs: Vec<String> = f
        .execs()
        .iter()
        .map(|e| e["argv"][0].as_str().unwrap_or("?").to_string())
        .collect();
    assert!(
        programs.contains(&"make".to_string()) && programs.contains(&"pytest".to_string()),
        "the nested runner was lost to the re-entry guard: {programs:?}"
    );
}

// ---------------------------------------------------------------------------
// Session attribution: the route that is correct rather than probable
// ---------------------------------------------------------------------------

#[test]
fn the_harness_session_id_wins_over_the_spool_s_most_recent_session() {
    // The misbind, as a regression test. `spool-latest` attributes an exec to
    // whichever session wrote to the spool most recently — and the window is
    // every spool line, not just a session start, so exposure scales with the
    // OTHER session's tool-call rate. Measured elsewhere at 49.7% with two
    // equally active sessions.
    //
    // Here session `other` writes the most recent spool line and session
    // `mine` runs the command. Under `spool-latest` the exec lands in `other`.
    let f = Fixture::new();
    std::fs::write(f.repo.join("Makefile"), "ok:\n\t@printf reached\n").expect("write makefile");

    let mut child = Command::new(thruline())
        .args(["hook", "claude-code"])
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .current_dir(&f.repo)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("hook");
    {
        use std::io::Write;
        let mut stdin = child.stdin.take().expect("stdin");
        stdin
            .write_all(
                br#"{"hook_event_name":"UserPromptSubmit","session_id":"other","transcript_path":"/t","cwd":"/r","prompt":"hi"}"#,
            )
            .expect("write");
    }
    assert!(child.wait().expect("wait").success());

    let path = format!(
        "{}:{}",
        f.shim.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new(f.shim.join("make"))
        .arg("ok")
        .current_dir(&f.repo)
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .env("PATH", &path)
        .env("CLAUDE_CODE_SESSION_ID", "mine")
        .output()
        .expect("run");
    assert!(out.status.success());

    let line = f
        .spool()
        .into_iter()
        .find(|v| v["spec_payload"]["kind"] == "exec")
        .expect("an exec line");
    assert_eq!(
        line["session_id"],
        serde_json::json!("mine"),
        "the exec was attributed to the session that merely wrote last"
    );
    assert_eq!(line["session_binding"], serde_json::json!("harness-env"));
}

#[test]
fn without_the_harness_id_a_default_install_binds_an_exec_to_a_session_that_never_ran_it() {
    // The control for the test above, and the finding itself. Without it the
    // assertion up there would be about the fixture rather than about the fix.
    //
    // Two facts make this the DEFAULT path rather than a fallback:
    // `InstallOptions::under` leaves `session_id` unset and nothing passes
    // `--session`, so a default install has no manifest route; and the window
    // is every spool line, so any tool call by another session captures the
    // next exec.
    let f = Fixture::new();
    std::fs::write(f.repo.join("Makefile"), "ok:\n\t@printf reached\n").expect("write makefile");

    // A default install: no `--session`.
    let out = Command::new(thruline())
        .args(["shim", "install"])
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .current_dir(&f.repo)
        .output()
        .expect("reinstall with no session");
    assert!(out.status.success());
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(f.shim.join("thruline-shim.json")).expect("manifest"),
    )
    .expect("manifest json");
    assert!(
        manifest.get("session_id").is_none(),
        "a default install carries a session id, so `spool-latest` is not the \
         only route a default install takes"
    );

    // Session `other` writes ONE ordinary spool line — not a session start.
    let mut child = Command::new(thruline())
        .args(["hook", "claude-code"])
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .current_dir(&f.repo)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("hook");
    {
        use std::io::Write;
        let mut stdin = child.stdin.take().expect("stdin");
        stdin
            .write_all(
                br#"{"hook_event_name":"PostToolUse","session_id":"other","transcript_path":"/t","cwd":"/r","tool_name":"Read","tool_response":"x"}"#,
            )
            .expect("write");
    }
    assert!(child.wait().expect("wait").success());

    let path = format!(
        "{}:{}",
        f.shim.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new(f.shim.join("make"))
        .arg("ok")
        .current_dir(&f.repo)
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .env("PATH", &path)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .output()
        .expect("run");
    assert!(out.status.success());

    let line = f
        .spool()
        .into_iter()
        .find(|v| v["spec_payload"]["kind"] == "exec")
        .expect("an exec line");
    assert_eq!(line["session_binding"], serde_json::json!("spool-latest"));
    assert_eq!(
        line["session_id"],
        serde_json::json!("other"),
        "the misbind did not reproduce, so the fix above is untested"
    );
}

#[test]
fn an_explicit_override_still_beats_the_harness() {
    let f = Fixture::new();
    std::fs::write(f.repo.join("Makefile"), "ok:\n\t@printf reached\n").expect("write makefile");
    let path = format!(
        "{}:{}",
        f.shim.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new(f.shim.join("make"))
        .arg("ok")
        .current_dir(&f.repo)
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .env("PATH", &path)
        .env("CLAUDE_CODE_SESSION_ID", "harness")
        .env("THRULINE_SESSION_ID", "explicit")
        .output()
        .expect("run");
    assert!(out.status.success());
    let line = f
        .spool()
        .into_iter()
        .find(|v| v["spec_payload"]["kind"] == "exec")
        .expect("an exec line");
    assert_eq!(line["session_id"], serde_json::json!("explicit"));
    assert_eq!(line["session_binding"], serde_json::json!("env"));
}

#[test]
fn a_captured_run_does_not_hide_a_bypassed_one_in_the_same_session() {
    // The finding: asking whether a session had NO execs let one green
    // captured run hide every bypassed command after it. This session has a
    // real captured `pytest` AND a tool record of a second `pytest` that never
    // reached the recorder — the exact `pytest` then
    // `/usr/bin/pytest --ignore=failing` shape.
    if !require_pytest() {
        return;
    }
    let f = Fixture::new();
    std::fs::write(
        f.repo.join("test_sample.py"),
        "def test_one():\n    assert True\n",
    )
    .expect("write test");

    let deliver = |json: &str| {
        let mut child = Command::new(thruline())
            .args(["hook", "claude-code"])
            .env("THRULINE_HOME", &f.home)
            // The suite runs INSIDE a Claude Code session, so this is set in
            // the inherited environment and would otherwise win the binding
            // for every fixture. A test must control its own attribution.
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .current_dir(&f.repo)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .expect("hook");
        {
            use std::io::Write;
            let mut stdin = child.stdin.take().expect("stdin");
            stdin.write_all(json.as_bytes()).expect("write");
        }
        assert!(child.wait().expect("wait").success());
    };

    // Tool call 1 — and it really runs, through the shim.
    deliver(
        r#"{"hook_event_name":"PreToolUse","session_id":"sess-test","transcript_path":"/t","cwd":"/r","tool_name":"Bash","tool_input":{"command":"pytest -q"}}"#,
    );
    let out = f.run("pytest", &["-q"]);
    assert!(out.status.success());

    // Tool call 2 — the bypass: an absolute path that never touches the shim.
    deliver(
        r#"{"hook_event_name":"PreToolUse","session_id":"sess-test","transcript_path":"/t","cwd":"/r","tool_name":"Bash","tool_input":{"command":"/usr/local/bin/pytest --ignore=failing"}}"#,
    );

    f.fold();
    let out = Command::new(thruline())
        .args(["doctor", "--json"])
        .current_dir(&f.repo)
        .env("THRULINE_HOME", &f.home)
        // The suite runs INSIDE a Claude Code session, so this is set in
        // the inherited environment and would otherwise win the binding
        // for every fixture. A test must control its own attribution.
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .output()
        .expect("doctor");
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let finding = value["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .find(|x| x["key"] == "shim-bypass")
        .cloned()
        .expect("a shim-bypass finding");
    assert_eq!(
        finding["ok"],
        serde_json::json!(false),
        "a bypassed command hid behind a captured one: {finding}"
    );
    let detail = finding["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("2 recorded by a tool, 1 captured"),
        "{detail}"
    );
}
