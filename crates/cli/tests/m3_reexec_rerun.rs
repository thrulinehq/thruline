//! M3.3, and the M3.2 clause that had no test carrying it into a verdict.
//!
//! Both tests here drive the real binary against a real container and then ask
//! `thruline claims` — the reader that sits downstream of the store — what the
//! session's `tests_pass` claim came out as. They exist because the two facts
//! below were, at the time of writing, held by nothing that could fail:
//!
//! 1. **M3.3 was absent.** `policy.reexec.reruns` parsed, defaulted to 2, and
//!    fed the policy digest, and no shipped code read it. `reexec` had exactly
//!    one `sandbox.run` call site. "Flaky never counts as passing" held only
//!    because a stronger veto — a re-execution that *failed* is
//!    `reexecution_failed` — returned first for any failing check. That
//!    accident dies the moment a rerun turns a failure into a pass, which is
//!    precisely what reruns are for.
//!
//! 2. **M3.2's headline clause was guarded by a source-text grep.** Deleting
//!    the `record_comparison` call in `reexec_cmd.rs`, or making it write
//!    `{"outcome":"agrees"}` unconditionally, left 459 tests green: the only
//!    thing naming it was `m3_reexec_laundering.rs`'s
//!    `src.contains("fn record_comparison(")`, which both mutations leave
//!    intact. `reexecution_contradicts` — Fail, and therefore `unsupported` —
//!    was reachable only through a field the re-executor writes about itself,
//!    and no test carried a disagreement through the stored row into a verdict.
//!
//! # Why these need Docker and are `#[ignore]`d
//!
//! A fixture that hand-writes the `reexec` block into the store would pass
//! with `record_comparison` deleted, because the fixture would be supplying
//! what the product is supposed to write. The mutation has to be observable,
//! so the product has to be the writer, so a container has to run. Set
//! `THRULINE_TEST_PYTEST_IMAGE` to an image with `pytest` on its PATH and run
//! with `--ignored`.

use std::path::PathBuf;
use std::process::Command;

#[path = "support/qualified_binary.rs"]
mod qualified_binary;
use qualified_binary::thruline;

fn docker_available() -> bool {
    Command::new("docker")
        .args(["info", "--format", "{{.ServerVersion}}"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn pytest_image() -> String {
    std::env::var("THRULINE_TEST_PYTEST_IMAGE").expect(
        "set THRULINE_TEST_PYTEST_IMAGE to an image with pytest on its PATH; these tests \
         assert about a real container and must not invent one",
    )
}

/// A repository, a session, and one wrapped command through the PATH shim.
struct Session {
    _dir: tempfile::TempDir,
    home: PathBuf,
    repo: PathBuf,
    shim: PathBuf,
    session: String,
}

impl Session {
    /// `files` are written and committed; `ignored` are written and named in
    /// `.gitignore`, so they exist for the agent's run on the host and are
    /// absent from the tree the container materialises.
    fn record(session: &str, files: &[(&str, &str)], ignored: &[(&str, &str)]) -> Session {
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

        let mut gitignore = String::from("__pycache__/\n.pytest_cache/\n");
        for (name, _) in ignored {
            gitignore.push_str(name);
            gitignore.push('\n');
        }
        std::fs::write(repo.join(".gitignore"), gitignore).expect("w");
        std::fs::write(
            repo.join("pyproject.toml"),
            "[project]\nname='r'\nversion='0'\n",
        )
        .expect("w");
        std::fs::create_dir_all(repo.join(".thruline")).expect("mkdir");
        std::fs::write(
            repo.join(".thruline/reexec.yml"),
            format!("version: 1\nimage: {}\n", pytest_image()),
        )
        .expect("w");
        for (name, body) in files {
            std::fs::write(repo.join(name), body).expect("w");
        }
        for args in [vec!["add", "-A"], vec!["commit", "-qm", "baseline"]] {
            let out = Command::new("git")
                .current_dir(&repo)
                .args(&args)
                .output()
                .expect("git");
            assert!(out.status.success(), "git {args:?}");
        }
        // **After** the commit, so they are in the worktree and in no tree.
        for (name, body) in ignored {
            std::fs::write(repo.join(name), body).expect("w");
        }

        let s = Session {
            _dir: dir,
            home: home.clone(),
            repo,
            shim: home.join("shim"),
            session: session.to_string(),
        };
        let out = Command::new(thruline())
            .args(["shim", "install"])
            .env("THRULINE_HOME", &s.home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .current_dir(&s.repo)
            .output()
            .expect("shim install");
        assert!(out.status.success());
        s.hook(&format!(
            r#"{{"hook_event_name":"SessionStart","session_id":"{}","transcript_path":"/t","cwd":"{}","source":"startup"}}"#,
            s.session,
            s.repo.display()
        ));
        s
    }

    fn hook(&self, json: &str) {
        use std::io::Write;
        let mut child = Command::new(thruline())
            .args(["hook", "claude-code"])
            .env("THRULINE_HOME", &self.home)
            .current_dir(&self.repo)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .expect("hook");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(json.as_bytes())
            .expect("write");
        assert!(child.wait().expect("wait").success());
    }

    /// One command through the PATH shim — what produces the `exec` event and
    /// the snapshot that binds it.
    fn agent(&self, command: &str) -> std::process::Output {
        let path = format!(
            "{}:{}",
            self.shim.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        Command::new("sh")
            .args(["-c", command])
            .current_dir(&self.repo)
            .env("THRULINE_HOME", &self.home)
            .env("PATH", path)
            .env("CLAUDE_CODE_SESSION_ID", &self.session)
            .output()
            .expect("bash tool")
    }

    fn stop_and_fold(&self) {
        self.hook(&format!(
            r#"{{"hook_event_name":"Stop","session_id":"{}","transcript_path":"/t","cwd":"{}","last_assistant_message":"All tests pass.","stop_hook_active":false}}"#,
            self.session,
            self.repo.display()
        ));
        let out = Command::new(thruline())
            .args(["fold"])
            .env("THRULINE_HOME", &self.home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .current_dir(&self.repo)
            .output()
            .expect("fold");
        assert!(
            out.status.success(),
            "fold: {}",
            String::from_utf8_lossy(&out.stdout)
        );
    }

    fn run(&self, args: &[&str]) -> std::process::Output {
        Command::new(thruline())
            .args(args)
            .env("THRULINE_HOME", &self.home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .current_dir(&self.repo)
            .output()
            .expect("thruline")
    }

    fn reexec_report(&self) -> serde_json::Value {
        let out = self.run(&["reexec", "--json", "--timeout-secs", "600"]);
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "reexec --json produced no JSON report ({e}). stdout: {} stderr: {}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        })
    }

    /// `thruline claims`' verdict for `tests_pass`, and the detectors it fired.
    fn claims_verdict(&self) -> (String, Vec<String>) {
        let out = self.run(&["claims", "--json"]);
        let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "claims --json: {e}: {}",
                String::from_utf8_lossy(&out.stdout)
            )
        });
        let claims = value["claims"].as_array().cloned().unwrap_or_default();
        let claim = claims
            .iter()
            .find(|c| c["kind"] == "tests_pass")
            .unwrap_or_else(|| panic!("no tests_pass claim: {value:#}"));
        (
            claim["verdict"].as_str().unwrap_or("(none)").to_string(),
            claim["findings"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter_map(|f| f["detector"].as_str().map(str::to_string))
                .collect(),
        )
    }
}

/// A test that fails the first time it runs in a fresh workspace and passes
/// every time after — the ordinary shape of a state-dependent flake, and the
/// only shape a rerun policy can catch.
const FLAKY_TEST: &str = "\
import pathlib

MARK = pathlib.Path(__file__).parent / '.flaky_marker'


def test_state_dependent():
    if not MARK.exists():
        MARK.write_text('seen')
        raise AssertionError('no marker in this workspace yet')
";

// ---------------------------------------------------------------------------
// M3.3 — flaky never counts as passing.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "requires a running Docker daemon and THRULINE_TEST_PYTEST_IMAGE. Run with --ignored."]
fn a_flaky_reexecution_never_counts_as_passing() {
    assert!(docker_available(), "no Docker daemon");

    // The marker exists on the host, so the agent's own run passes and the
    // captured check is honest, bound, and green. It is gitignored, so it is in
    // no tree and the container starts without it: rerun 1 of 2 fails and
    // creates it, rerun 2 of 2 passes. Outcomes differ at the same tree in the
    // same image — flaky, by §6.3's definition.
    let s = Session::record(
        "a1a1a1a1-b2b2-4c3c-8d4d-e5e5e5e5e5e5",
        &[("test_state.py", FLAKY_TEST)],
        &[(".flaky_marker", "seen")],
    );
    let out = s.agent("pytest -q");
    assert!(
        out.status.success(),
        "the agent's own run must pass, or this fixture is about something else: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    s.stop_and_fold();

    let (before, _) = s.claims_verdict();
    assert_eq!(
        before, "supported",
        "the session is not `supported` before re-execution, so nothing below is \
         about the re-execution"
    );

    let report = s.reexec_report();
    assert!(report["refused"].is_null(), "{report:#}");

    // The two runs are in the record, in order, with their own exit codes.
    let runs = report["checks"][0]["runs"]
        .as_array()
        .unwrap_or_else(|| panic!("the re-execution recorded no per-run history: {report:#}"));
    assert_eq!(
        runs.len(),
        2,
        "`policy.reexec.reruns` defaults to 2 and the first run failed, so the command \
         must have been run twice: {report:#}"
    );
    assert_ne!(
        runs[0]["exit_code"], runs[1]["exit_code"],
        "the fixture did not produce a flake: {report:#}"
    );
    assert_eq!(
        report["checks"][0]["deterministic"], false,
        "two runs of the same command at the same tree in the same image disagreed and \
         the report calls it deterministic: {report:#}"
    );
    // The LAST run passed and agrees with the agent's run, so nothing about
    // exit codes stops this from reading as a confirmed pass. Only the
    // flakiness does.
    assert_eq!(
        report["checks"][0]["exit_code"], 0,
        "the recorded outcome is not the passing rerun, so the older accident (a \
         failing re-execution is vetoed by `reexecution_failed`) is what this test \
         would be measuring: {report:#}"
    );
    assert_ne!(
        report["exit"], 0,
        "`reexec` exited 0 on a flaky re-execution; a CI gate reads 0 as a pass: \
         {report:#}"
    );

    let (after, findings) = s.claims_verdict();
    assert_ne!(
        after, "supported",
        "a flaky re-execution left the claim `supported`. §6.3: flaky tests never \
         count as passing evidence. findings: {findings:?}"
    );
    assert!(
        findings.iter().any(|d| d == "reexecution_flaky"),
        "nothing in the record names the flakiness: {findings:?}"
    );
    assert!(
        !findings.iter().any(|d| d == "reexecution_failed"),
        "the verdict rests on `reexecution_failed`, which is the accident this test \
         exists to stop relying on: {findings:?}"
    );
}

// ---------------------------------------------------------------------------
// M3.2 — a disagreement, carried through the stored row into a verdict.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "requires a running Docker daemon and THRULINE_TEST_PYTEST_IMAGE. Run with --ignored."]
fn a_contradicting_reexecution_reaches_the_verdict_through_the_stored_row() {
    assert!(docker_available(), "no Docker daemon");

    // The agent's green run counted a test file git has never seen. The
    // container materialises the tree, so it runs two tests where the agent ran
    // three. Both exit 0 — so no exit-code veto anywhere on the path — and the
    // tallies disagree. That is §7's mismatch, and §7 says it is `unsupported`.
    let s = Session::record(
        "b1b1b1b1-c2c2-4d3d-8e4e-f5f5f5f5f5f5",
        &[(
            "test_core.py",
            "def test_one():\n    assert True\n\n\ndef test_two():\n    assert True\n",
        )],
        &[("test_extra.py", "def test_extra():\n    assert True\n")],
    );
    let out = s.agent("pytest -q");
    assert!(
        out.status.success(),
        "the agent's own run must pass: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    s.stop_and_fold();

    let (before, _) = s.claims_verdict();
    assert_eq!(
        before, "supported",
        "the session is not `supported` before re-execution, so nothing below is \
         about the re-execution"
    );

    let report = s.reexec_report();
    assert!(report["refused"].is_null(), "{report:#}");
    assert_eq!(
        report["checks"][0]["comparison"]["outcome"], "disagrees",
        "the fixture did not produce a disagreement, so this run tests nothing: \
         {report:#}"
    );
    assert_eq!(
        report["checks"][0]["exit_code"], 0,
        "the re-execution failed outright; then `reexecution_failed` carries the \
         verdict and `reexecution_contradicts` is untested again: {report:#}"
    );

    // **The assertion the source-text grep stood in for.** The comparison has
    // to survive the process that made it: written onto the `Check` row, read
    // back by `thruline claims`, and turned into a Fail finding.
    let (after, findings) = s.claims_verdict();
    assert_eq!(
        after, "unsupported",
        "a re-execution that contradicted the agent's own run left the claim \
         {before:?} → {after:?}. §7: mismatches are `unsupported`. findings: \
         {findings:?}"
    );
    assert!(
        findings.iter().any(|d| d == "reexecution_contradicts"),
        "the disagreement did not reach the verifier through the stored row — which \
         is what deleting `record_comparison`, or hard-coding \"agrees\" in it, \
         produces: {findings:?}"
    );
}

// ---------------------------------------------------------------------------
// M3.3 — §6.3's word is *outcomes*, and this is the half of it that was
// asserted in a doc comment and held by nothing.
// ---------------------------------------------------------------------------

/// Two runs that both FAIL, with the same exit code and different tallies.
///
/// The host has `.host_marker` (gitignored, so it is in no tree), which is what
/// makes the agent's own run pass and the captured check honest. Inside the
/// container that file is absent, so `test_environment` fails on both runs —
/// same exit code, twice. `.tally_marker` is created by whichever run goes
/// first, and its presence adds a second test to the module, so the two runs
/// collect different numbers of tests: `(1 total, 0 passed, 1 failed)` then
/// `(2, 1, 1)`.
const SAME_EXIT_DIFFERENT_TALLY: &str = "\
import pathlib

WS = pathlib.Path(__file__).parent
HOST = WS / '.host_marker'
SEEN = WS / '.tally_marker'

FIRST_HERE = not SEEN.exists()
if FIRST_HERE:
    SEEN.write_text('seen')


def test_environment():
    assert HOST.exists(), 'this workspace is not the one the agent ran in'


if not FIRST_HERE:

    def test_appears_only_after_the_first_run():
        pass
";

#[test]
#[ignore = "requires a running Docker daemon and THRULINE_TEST_PYTEST_IMAGE. Run with --ignored."]
fn two_runs_with_one_exit_code_and_two_tallies_are_flaky() {
    // `Attempt::outcome()` decides what "the same outcome" means, and its doc
    // comment says the tally counts as well as the exit code — §6.3 says
    // *outcomes*, not *exit codes*. Nothing held that half. Mutating the
    // function to `(self.result.exit_code, None)` left the whole workspace
    // green, because every flake fixture in the suite also changed its exit
    // code, so the exit code alone was always enough to tell the runs apart.
    //
    // A doc comment asserting a property no test holds is a claim. This is the
    // fixture that can only be caught by the tally: the exit codes AGREE.
    assert!(docker_available(), "no Docker daemon");

    let s = Session::record(
        "b2b2b2b2-c3c3-4d4d-8e5e-f6f6f6f6f6f6",
        &[("test_tally.py", SAME_EXIT_DIFFERENT_TALLY)],
        // BOTH markers are gitignored: an untracked file that is not ignored is
        // still in the snapshot, and `.tally_marker` reaching the tree made both
        // container runs collect two tests and the fixture witness nothing.
        &[(".host_marker", "here"), (".tally_marker", "seen")],
    );
    let out = s.agent("pytest -q");
    assert!(
        out.status.success(),
        "the agent's own run must pass, or this fixture is about something else: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    s.stop_and_fold();

    let report = s.reexec_report();
    assert!(report["refused"].is_null(), "{report:#}");
    let check = &report["checks"][0];
    let runs = check["runs"]
        .as_array()
        .unwrap_or_else(|| panic!("the re-execution recorded no per-run history: {report:#}"));
    assert_eq!(
        runs.len(),
        2,
        "the first run must have been re-run: {report:#}"
    );

    // The whole point of the fixture, asserted before anything rests on it.
    assert_eq!(
        runs[0]["exit_code"], runs[1]["exit_code"],
        "the exit codes differ, so this fixture would be caught by the exit code alone \
         and would witness nothing: {report:#}"
    );
    assert_ne!(
        runs[0]["parsed"], runs[1]["parsed"],
        "the two runs parsed identically, so there is no tally difference to detect: \
         {report:#}"
    );

    // And therefore: not deterministic, and flaky.
    assert_eq!(
        check["deterministic"], false,
        "two runs at the same tree in the same image reported different numbers of \
         tests and the report calls it deterministic — `Attempt::outcome()` is \
         comparing exit codes only, which is the thing its doc comment says it does \
         not do: {report:#}"
    );

    // # The seam this fixture exposes, stated rather than implied
    //
    // `deterministic` IS `Attempt::outcome()` equality, so the assertion above
    // is the function itself and the mutation reddens it. But the tally half
    // can only ever *decide* something when the two runs share an exit code,
    // and a rerun happens only after a failure — so both runs are non-zero, and
    // both `Report::exit_code` (`c.exit_code != 0`, before `!c.deterministic`)
    // and `tests_pass` (`reexecution_failed`, which returns before the
    // comparison outcome is read) have already refused on the exit code alone.
    //
    // So: the tally half is load-bearing for the RECORD and for nothing else
    // today. It cannot move a verdict, because everything that reaches it is
    // already fatal. That is the same shape as the swallowed config refusal —
    // a rule riding a stronger veto — and it is named here, in
    // `docs/STATUS.md`, and nowhere else does it hold. This assertion is
    // deliberately on `deterministic` and not on the claim's verdict: a verdict
    // assertion here would pass through `reexecution_failed` and witness
    // nothing.
    assert_ne!(
        report["exit"], 0,
        "`reexec` exited 0 on a flaky re-execution: {report:#}"
    );
}
