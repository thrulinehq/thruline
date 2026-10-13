//! The laundering path, end to end.
//!
//! `crates/cli/tests/m3_reexec.rs` proves the *declared* gate refuses. This
//! file is about the three ways the gate was silent anyway, each reproduced by
//! running the binary before it was fixed:
//!
//! 1. **The upgrade.** `thruline reexec` turned a correctly `unsupported`
//!    session into a `supported` one and deleted the reason. Covered at the
//!    verifier in `crates/verify/tests/reexec_cannot_promote.rs`, where the
//!    pre-fix fixture is kept executable, and end to end here when a container
//!    runtime is present.
//! 2. **The fidelity gate keyed off the wrong thing.** `capture_degraded` has
//!    exactly one producer — the exec recorder, which stamps it when a filter
//!    driver was in scope *at an exec boundary*. Install `.gitattributes` one
//!    exec later and every recorded snapshot is clean, so the gate said
//!    nothing and `reexec` went on to materialise a tree that does not name the
//!    bytes on disk. Measured: identical filter, refused when installed before
//!    the last exec, silent when installed after it.
//! 3. **A sentinel in the contract.** `--mode full` serialised
//!    `"original_exit_code": -9223372036854775808`.
//!
//! Every test here is written so that removing the thing it guards makes it
//! fail — the fidelity test re-runs the same session with the `.gitattributes`
//! deleted and asserts the refusal changes.

use std::path::{Path, PathBuf};
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

/// A repository, a session that ran one wrapped command through the shim, and
/// a second wrapped command after an unobserved source edit — so the session's
/// own test check is bound to a tree that is not the final one.
struct Session {
    _dir: tempfile::TempDir,
    home: PathBuf,
    repo: PathBuf,
    shim: PathBuf,
    session: String,
}

impl Session {
    fn record() -> Session {
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
        std::fs::write(repo.join(".gitignore"), "__pycache__/\n").expect("w");
        let out = Command::new("git")
            .current_dir(&repo)
            .args(["add", "-A"])
            .output()
            .expect("git");
        assert!(out.status.success());
        let out = Command::new("git")
            .current_dir(&repo)
            .args(["commit", "-qm", "baseline"])
            .output()
            .expect("git");
        assert!(out.status.success());

        let s = Session {
            _dir: dir,
            home: home.clone(),
            repo,
            shim: home.join("shim"),
            session: "55555555-6666-4777-8888-999999999999".to_string(),
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

    /// One command through the PATH shim, which is what produces the `exec`
    /// event and the snapshot that binds it.
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

    fn reexec_report(&self, extra: &[&str]) -> serde_json::Value {
        let mut args = vec!["reexec", "--json", "--timeout-secs", "600"];
        args.extend_from_slice(extra);
        let out = self.run(&args);
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "reexec --json produced no JSON report ({e}). stdout: {} stderr: {}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        })
    }
}

// ---------------------------------------------------------------------------
// The fidelity gate, keyed on the tree that is about to be materialised.
// ---------------------------------------------------------------------------

#[test]
fn a_filter_installed_after_the_last_exec_is_still_refused() {
    // The bypass, reproduced: `capture_degraded` is written by the exec
    // recorder when a filter driver is in scope AT AN EXEC BOUNDARY, so an
    // agent that writes `.gitattributes` after its last wrapped command leaves
    // every recorded snapshot clean. Nothing in the record flags it, and
    // `reexec` used to go ahead and materialise a tree whose contents the
    // filter controls — re-running honest code to confirm a claim about
    // cheating code, which is the laundering the gate exists to refuse.
    let s = Session::record();
    // One wrapped command: the exec event and the snapshot behind it.
    s.agent("python3 -c 'import shipping'");
    // ...and only THEN the filter.
    std::fs::write(s.repo.join(".gitattributes"), "*.py filter=hide\n").expect("w");
    s.stop_and_fold();

    let report = s.reexec_report(&[]);
    assert_eq!(
        report["refused"]["reason"], "tree_fidelity_broken",
        "a filter installed after the last exec was not refused: {report:#}"
    );
    assert!(
        report["refused"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("re-execution time"),
        "the refusal does not say the filter was found at re-execution time rather \
         than in the record, which is the whole point of the check: {report:#}"
    );
    assert!(
        report["checks"].as_array().is_none_or(|a| a.is_empty()),
        "a refused re-execution ran something: {report:#}"
    );

    // **Remove the precondition and the refusal must change.** Same session,
    // same store, same everything except the file the gate is about. Without
    // this half the test would pass on a gate that refuses unconditionally,
    // which is the failure mode this program has already shipped once.
    std::fs::remove_file(s.repo.join(".gitattributes")).expect("rm");
    let report = s.reexec_report(&[]);
    assert_ne!(
        report["refused"]["reason"], "tree_fidelity_broken",
        "the gate refuses for tree fidelity with no filter present, so the assertion \
         above proves nothing: {report:#}"
    );
}

// ---------------------------------------------------------------------------
// The upgrade itself, end to end. Needs a container runtime.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "requires a running Docker daemon and an image carrying pytest. Run with --ignored."]
fn reexec_does_not_upgrade_a_superseded_session() {
    assert!(docker_available(), "no Docker daemon");
    let image = std::env::var("THRULINE_TEST_PYTEST_IMAGE").expect(
        "set THRULINE_TEST_PYTEST_IMAGE to an image with pytest on its PATH; this test \
         asserts about a real container and must not invent one",
    );

    let s = Session::record();
    std::fs::write(
        s.repo.join("test_shipping.py"),
        "from shipping import rate\n\n\
         def test_rate_small():\n    assert rate(2) == 9.0\n\n\
         def test_rate_large():\n    assert rate(10) == 29.0\n",
    )
    .expect("w");
    std::fs::write(
        s.repo.join("pyproject.toml"),
        "[project]\nname='r'\nversion='0'\n",
    )
    .expect("w");
    std::fs::create_dir_all(s.repo.join(".thruline")).expect("mkdir");
    std::fs::write(
        s.repo.join(".thruline/reexec.yml"),
        format!("version: 1\nimage: {image}\n"),
    )
    .expect("w");
    for args in [vec!["add", "-A"], vec!["commit", "-qm", "tests"]] {
        assert!(Command::new("git")
            .current_dir(&s.repo)
            .args(&args)
            .output()
            .expect("git")
            .status
            .success());
    }

    // The agent's own test run, captured.
    let out = s.agent("pytest -q");
    assert!(
        out.status.success(),
        "the baseline suite must pass: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    // Then a source edit through a tool that emits no `fs_change`, and one more
    // wrapped command so a snapshot of the NEW tree exists. The test check is
    // now bound to a tree that is not the final one.
    std::fs::write(
        s.repo.join("shipping.py"),
        "def rate(weight_kg):\n    return round(2.5 * weight_kg + 4.0, 2)\n\n\
         def surcharge(kind):\n    return {'air': 12.0}[kind]\n",
    )
    .expect("w");
    s.agent("python3 -c 'import shipping'");
    s.stop_and_fold();

    let before = claims_verdict(&s);
    assert_eq!(
        before.0, "unsupported",
        "the fixture does not reproduce the correctly-unsupported state: {:#?}",
        before
    );
    assert!(
        before.1.iter().any(|d| d == "tests_ran_before_final_edit"),
        "{before:#?}"
    );

    let report = s.reexec_report(&[]);
    assert!(report["refused"].is_null(), "{report:#}");
    assert_eq!(
        report["checks"][0]["comparison"]["outcome"], "agrees",
        "the re-execution did not agree, so this run does not test the promoting \
         case: {report:#}"
    );

    let after = claims_verdict(&s);
    assert_eq!(
        after.0, "unsupported",
        "`thruline reexec` upgraded a correctly-unsupported session: {after:#?}"
    );
    assert!(
        after.1.iter().any(|d| d == "tests_ran_before_final_edit"),
        "the supersession finding was dropped when the re-execution arrived: {after:#?}"
    );
    assert!(
        after.1.iter().any(|d| d == "reexecution_did_not_promote"),
        "nothing in the record says a re-execution ran and changed nothing: {after:#?}"
    );
}

/// `thruline claims`' verdict for `tests_pass`, and the detectors it fired.
fn claims_verdict(s: &Session) -> (String, Vec<String>) {
    let out = s.run(&["claims", "--json"]);
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

// ---------------------------------------------------------------------------
// The sentinel, and the refusal that does not exist.
// ---------------------------------------------------------------------------

#[test]
fn no_sentinel_can_reach_the_reexec_json_contract() {
    // `--mode full` serialised `"original_exit_code": -9223372036854775808`:
    // `i64::MIN`, chosen so it could not be confused with a real exit code, and
    // then written straight into the contract a CI consumer reads. The type no
    // longer has room for it — "the agent did not run this" is `None` and the
    // field is omitted — so the census below is about keeping it that way.
    let src =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/reexec_cmd.rs"))
            .expect("reexec_cmd.rs");
    assert!(
        src.contains("pub original_exit_code: Option<i64>,"),
        "`original_exit_code` is not an `Option` again, so there is a value standing \
         where a recorded one belongs"
    );
    for forbidden in ["i64::MIN", "NOT_RUN_BY_THE_AGENT"] {
        assert!(
            !src.contains(forbidden),
            "reexec_cmd.rs reaches {forbidden:?} again"
        );
    }
    // **What used to be here, and why it is gone.** This test also asserted
    // `src.contains("fn record_comparison(")` — a source-text grep standing in
    // for M3.2's headline clause. It could not fail: deleting the *call* to
    // `record_comparison`, or making it write `{"outcome":"agrees"}`
    // unconditionally, leaves the function definition intact, leaves 459 tests
    // green, and makes `reexecution_contradicts` unreachable. Both mutations
    // are now red against
    // `crates/cli/tests/m3_reexec_rerun.rs::a_contradicting_reexecution_reaches_the_verdict_through_the_stored_row`,
    // which carries a real disagreement through the stored row into a verdict.
    // A grep for a function name is not a test for what the function does.
}

#[test]
fn no_object_write_does_not_exist_and_is_not_the_fidelity_refusal_wearing_a_hat() {
    // BUILD-PLAN M3.2 says re-exec "also refuses under `--no-object-write`,
    // with a distinct reason". The flag does not exist: `Refusal` has four
    // variants and none of them is it, and `tree_fidelity_broken` is the only
    // string either would print. **Two refusals printing the same thing are one
    // refusal**, so this test pins the flag as unimplemented rather than
    // letting it be implemented by aliasing the fidelity refusal.
    let s = Session::record();
    let out = s.run(&["reexec", "--no-object-write"]);
    assert!(
        !out.status.success(),
        "`--no-object-write` now parses. It must come with a Refusal variant of its \
         own — a tree that cannot be materialised at all is a different fact from a \
         tree whose contents a filter controls — and this test must be replaced by \
         one that asserts the two print differently."
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("unexpected argument"),
        "`--no-object-write` failed for some reason other than not existing: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Recorded, so the plan's sentence is falsifiable from the repository
    // rather than from memory.
    let changelog =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../CHANGELOG.md"))
            .expect("CHANGELOG.md");
    assert!(
        changelog.contains("`--no-object-write` does not exist"),
        "the CHANGELOG no longer records `--no-object-write` as unimplemented, so \
         BUILD-PLAN M3.2's sentence about it is unqualified prose again"
    );
}
