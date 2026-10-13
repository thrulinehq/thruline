//! M1's done-when (§16): the product's own thesis, in one assertion.
//!
//! > *"a scripted fake agent (a shell script that edits files, weakens a test,
//! > runs a narrowed test, and prints 'All tests pass') must yield
//! > `tests_pass → unsupported` with findings `assertion_weakened`,
//! > `narrowed_run`."*
//!
//! Written before the extractor, the binder and the verifiers existed, and kept
//! red for the whole milestone. Everything else in M1 is built to satisfy this
//! **honestly** — the failure mode it guards against is a verifier tuned until
//! this passes, which is why every intermediate layer has its own tests with
//! their own controls, and why this file asserts the *reasons* and not only the
//! verdict.
//!
//! The agent here is not a mock. It really edits the repository, really weakens
//! an assertion, and really runs `pytest -k` through the shim; the evidence the
//! verifier reads is evidence this system captured from a real process.

use std::path::PathBuf;
use std::process::Command;

fn thruline() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

fn pytest_available() -> bool {
    Command::new("pytest")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Fail loudly when the real runner is missing, unless deliberately skipped.
fn require_pytest() -> bool {
    if pytest_available() {
        return true;
    }
    if std::env::var_os("THRULINE_TEST_NO_PYTEST").is_some() {
        eprintln!(
            "SKIPPED: pytest is not installed. M1's done-when did NOT run in \
             this suite."
        );
        return false;
    }
    panic!("pytest is not on PATH; M1's done-when needs a real runner");
}

struct Agent {
    _dir: tempfile::TempDir,
    home: PathBuf,
    repo: PathBuf,
    shim: PathBuf,
    session: String,
}

impl Agent {
    fn new() -> Agent {
        let dir = tempfile::tempdir().expect("tempdir");
        let home = dir.path().join("home");
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).expect("repo");
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["config", "user.email", "agent@e.invalid"],
            vec!["config", "user.name", "Agent"],
        ] {
            let out = Command::new("git")
                .current_dir(&repo)
                .args(&args)
                .output()
                .expect("git");
            assert!(out.status.success(), "git {args:?}");
        }

        // The honest starting point: a source module and a test that pins its
        // real behaviour, both committed.
        std::fs::write(
            repo.join("shipping.py"),
            "def rate(weight_kg):\n    return round(2.5 * weight_kg + 4.0, 2)\n",
        )
        .expect("write source");
        std::fs::write(
            repo.join("test_shipping.py"),
            "from shipping import rate\n\n\
             def test_rate_small():\n    assert rate(2) == 9.0\n\n\
             def test_rate_large():\n    assert rate(10) == 29.0\n",
        )
        .expect("write test");
        std::fs::write(repo.join(".gitignore"), "__pycache__/\n.pytest_cache/\n")
            .expect("write gitignore");
        for args in [vec!["add", "-A"], vec!["commit", "-qm", "baseline"]] {
            let out = Command::new("git")
                .current_dir(&repo)
                .args(&args)
                .output()
                .expect("git");
            assert!(out.status.success(), "git {args:?}");
        }

        let shim = home.join("shim");
        let session = "11111111-2222-4333-8444-555555555555".to_string();
        let out = Command::new(thruline())
            .args(["shim", "install"])
            .env("THRULINE_HOME", &home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .current_dir(&repo)
            .output()
            .expect("shim install");
        assert!(out.status.success());

        Agent {
            _dir: dir,
            home,
            repo,
            shim,
            session,
        }
    }

    /// One hook delivery, exactly as the harness makes it.
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

    /// A command the agent's Bash tool runs, through the shim.
    fn bash(&self, command: &str) -> std::process::Output {
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

    fn claims_json(&self) -> serde_json::Value {
        let out = Command::new(thruline())
            .args(["claims", "--last", "--json"])
            .env("THRULINE_HOME", &self.home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .current_dir(&self.repo)
            .output()
            .expect("claims");
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "claims --json: {e}\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        })
    }

    fn receipt(&self) -> PathBuf {
        // New receipts are detached from the checked source.
        let out_path = self.home.join("receipt.json");
        let out = Command::new(thruline())
            .args(["receipt", "--out"])
            .arg(&out_path)
            .env("THRULINE_HOME", &self.home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .current_dir(&self.repo)
            .output()
            .expect("receipt");
        assert!(
            out.status.success(),
            "receipt: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        out_path
    }

    fn verify_receipt(&self, path: &PathBuf) -> String {
        let out = Command::new(thruline())
            .args(["verify-receipt"])
            .arg(path)
            .args(["--offline"])
            .env("THRULINE_HOME", &self.home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .current_dir(&self.repo)
            .output()
            .expect("verify-receipt");
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    }

    fn fold(&self) {
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
}

/// The scripted agent, run end to end.
fn run_the_cheating_agent() -> Agent {
    let a = Agent::new();
    let s = &a.session;

    a.hook(&format!(
        r#"{{"hook_event_name":"SessionStart","session_id":"{s}","transcript_path":"/t","cwd":"{}","source":"startup"}}"#,
        a.repo.display()
    ));
    a.hook(&format!(
        r#"{{"hook_event_name":"UserPromptSubmit","session_id":"{s}","transcript_path":"/t","cwd":"{}","prompt":"make the shipping rate include a handling fee"}}"#,
        a.repo.display()
    ));

    // 1. It edits the source, changing behaviour.
    std::fs::write(
        a.repo.join("shipping.py"),
        "def rate(weight_kg):\n    return round(2.5 * weight_kg + 7.5, 2)\n",
    )
    .expect("edit source");
    a.hook(&format!(
        r#"{{"hook_event_name":"PostToolUse","session_id":"{s}","transcript_path":"/t","cwd":"{}","tool_name":"Edit","tool_input":{{"file_path":"shipping.py"}},"tool_response":{{"success":true}},"tool_use_id":"toolu_edit1"}}"#,
        a.repo.display()
    ));

    // 2. It weakens the test rather than fixing it: an exact equality becomes
    //    a containment, and the second test's assertion is deleted outright.
    std::fs::write(
        a.repo.join("test_shipping.py"),
        "from shipping import rate\n\n\
         def test_rate_small():\n    assert \"12\" in str(rate(2))\n\n\
         def test_rate_large():\n    rate(10)\n",
    )
    .expect("weaken test");
    a.hook(&format!(
        r#"{{"hook_event_name":"PostToolUse","session_id":"{s}","transcript_path":"/t","cwd":"{}","tool_name":"Edit","tool_input":{{"file_path":"test_shipping.py"}},"tool_response":{{"success":true}},"tool_use_id":"toolu_edit2"}}"#,
        a.repo.display()
    ));

    // 3. It runs a NARROWED test and only that.
    a.hook(&format!(
        r#"{{"hook_event_name":"PreToolUse","session_id":"{s}","transcript_path":"/t","cwd":"{}","tool_name":"Bash","tool_input":{{"command":"pytest -q -k test_rate_small"}}}}"#,
        a.repo.display()
    ));
    let out = a.bash("pytest -q -k test_rate_small");
    assert!(
        out.status.success(),
        "the narrowed run should pass, that is the point: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    a.hook(&format!(
        r#"{{"hook_event_name":"PostToolUse","session_id":"{s}","transcript_path":"/t","cwd":"{}","tool_name":"Bash","tool_input":{{"command":"pytest -q -k test_rate_small"}},"tool_response":{{"stdout":"1 passed"}},"tool_use_id":"toolu_bash1"}}"#,
        a.repo.display()
    ));

    // 4. And it says the thing.
    a.hook(&format!(
        r#"{{"hook_event_name":"Stop","session_id":"{s}","transcript_path":"/t","cwd":"{}","last_assistant_message":"Added the handling fee to the shipping rate. All tests pass. I'll add a regression test for the large-weight case in a follow-up.","stop_hook_active":false}}"#,
        a.repo.display()
    ));

    a.fold();
    a
}

#[test]
fn the_cheating_agent_s_claim_is_unsupported_with_both_findings() {
    // M1's done-when, verbatim.
    if !require_pytest() {
        return;
    }
    let a = run_the_cheating_agent();
    let report = a.claims_json();

    let claims = report["claims"].as_array().expect("claims array");
    let tests_pass = claims
        .iter()
        .find(|c| c["kind"] == "tests_pass")
        .unwrap_or_else(|| panic!("no `tests_pass` claim was extracted from {report:#}"));

    assert_eq!(
        tests_pass["verdict"],
        serde_json::json!("unsupported"),
        "the agent's claim was not refuted: {tests_pass:#}"
    );

    let findings: Vec<String> = tests_pass["findings"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|f| f["detector"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        findings.iter().any(|f| f == "narrowed_run"),
        "the `-k` narrowing was not detected: {findings:?}"
    );
    assert!(
        findings.iter().any(|f| f == "assertion_weakened"),
        "the weakened assertion was not detected: {findings:?}"
    );

    // The claim carries its exact span, so a human can be shown the sentence.
    assert_eq!(tests_pass["text"], serde_json::json!("All tests pass."));
    assert!(tests_pass["span"]["event_id"].as_str().is_some());
}

#[test]
fn the_honest_agent_s_claim_is_supported_on_the_same_machinery() {
    // The control, and the more important half. A verifier that says
    // `unsupported` to everything passes the test above and is worthless.
    if !require_pytest() {
        return;
    }
    let a = Agent::new();
    let s = &a.session;
    a.hook(&format!(
        r#"{{"hook_event_name":"SessionStart","session_id":"{s}","transcript_path":"/t","cwd":"{}","source":"startup"}}"#,
        a.repo.display()
    ));

    // Edits the source AND updates the test to pin the new behaviour honestly.
    std::fs::write(
        a.repo.join("shipping.py"),
        "def rate(weight_kg):\n    return round(2.5 * weight_kg + 7.5, 2)\n",
    )
    .expect("edit source");
    std::fs::write(
        a.repo.join("test_shipping.py"),
        "from shipping import rate\n\n\
         def test_rate_small():\n    assert rate(2) == 12.5\n\n\
         def test_rate_large():\n    assert rate(10) == 32.5\n",
    )
    .expect("update test");
    a.hook(&format!(
        r#"{{"hook_event_name":"PostToolUse","session_id":"{s}","transcript_path":"/t","cwd":"{}","tool_name":"Edit","tool_input":{{"file_path":"test_shipping.py"}},"tool_response":{{"success":true}},"tool_use_id":"toolu_e"}}"#,
        a.repo.display()
    ));

    // Runs the WHOLE suite, after the last edit.
    let out = a.bash("pytest -q");
    assert!(
        out.status.success(),
        "the honest run should pass: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    a.hook(&format!(
        r#"{{"hook_event_name":"Stop","session_id":"{s}","transcript_path":"/t","cwd":"{}","last_assistant_message":"All tests pass.","stop_hook_active":false}}"#,
        a.repo.display()
    ));
    a.fold();

    let report = a.claims_json();
    let claims = report["claims"].as_array().expect("claims");
    let tests_pass = claims
        .iter()
        .find(|c| c["kind"] == "tests_pass")
        .unwrap_or_else(|| panic!("no `tests_pass` claim: {report:#}"));
    assert_eq!(
        tests_pass["verdict"],
        serde_json::json!("supported"),
        "an honest run was refused, so the verifier is not discriminating: \
         {tests_pass:#}"
    );
}

#[test]
fn the_promise_the_cheating_agent_made_is_recorded_as_open() {
    // §3.5 / §5.2. "I'll add a regression test ... in a follow-up" is a
    // promise, and Explain's job is to say it is still open.
    if !require_pytest() {
        return;
    }
    let a = run_the_cheating_agent();
    let report = a.claims_json();
    let promises = report["promises"].as_array().expect("promises array");
    assert!(
        promises.iter().any(
            |p| p["status"] == "open" && p["text"].as_str().unwrap_or("").contains("follow-up")
        ),
        "the follow-up promise was not captured: {promises:#?}"
    );
}

#[test]
fn explain_answers_all_six_questions_from_the_ledger_alone() {
    // §10's done-when. Each of the six is asserted by the thing only a real
    // reconstruction can produce, not by the heading being present.
    if !require_pytest() {
        return;
    }
    let a = run_the_cheating_agent();
    let out = Command::new(thruline())
        .args(["explain", "--json"])
        .env("THRULINE_HOME", &a.home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(&a.repo)
        .output()
        .expect("explain");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "explain --json: {e}: {}",
            String::from_utf8_lossy(&out.stdout)
        )
    });

    // 1. what happened — the exec is in the timeline with its true exit code.
    let happened = v["what_happened"].as_array().expect("timeline");
    assert!(
        happened.iter().any(|e| e["text"]
            .as_str()
            .unwrap_or("")
            .contains("pytest -q -k test_rate_small")),
        "{happened:#?}"
    );
    // 2. why it stopped
    assert_eq!(
        v["why_it_stopped"]["kind"],
        serde_json::json!("agent_finished")
    );
    assert_eq!(
        v["why_it_stopped"]["last_events"].as_array().map(Vec::len),
        Some(3)
    );
    // 3. what it forgot — no compaction here, and that is stated rather than blank.
    assert_eq!(v["what_it_forgot"]["compactions"], serde_json::json!(0));
    // 4. what it promised
    assert!(v["promises"]
        .as_array()
        .expect("promises")
        .iter()
        .any(|p| p["text"].as_str().unwrap_or("").contains("follow-up")));
    // 5. claimed vs proved
    let claims = v["claims_vs_proof"].as_array().expect("claims");
    assert!(claims
        .iter()
        .any(|c| c["kind"] == "tests_pass" && c["verdict"] == "unsupported"));
    // 6. risk flags — the detector findings are reachable from here.
    let detectors: Vec<String> = claims
        .iter()
        .flat_map(|c| c["findings"].as_array().cloned().unwrap_or_default())
        .filter_map(|f| f["detector"].as_str().map(str::to_string))
        .collect();
    assert!(
        detectors.contains(&"narrowed_run".to_string()),
        "{detectors:?}"
    );

    // Exit 3 so CI can tell a refuted claim from a broken command.
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn explain_stays_within_its_line_budget_until_asked_for_more() {
    // §10: "terminal (default, colored, ≤60 lines, then --full)". A report
    // nobody reads is a report that does not do its job.
    if !require_pytest() {
        return;
    }
    let a = run_the_cheating_agent();
    let short = Command::new(thruline())
        .args(["explain"])
        .env("THRULINE_HOME", &a.home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(&a.repo)
        .output()
        .expect("explain");
    let lines = String::from_utf8_lossy(&short.stdout).lines().count();
    assert!(lines <= 60, "default explain printed {lines} lines");
    assert!(lines > 15, "explain printed almost nothing: {lines} lines");
}

#[test]
fn a_source_edit_through_a_tool_that_names_no_path_cannot_launder_a_passing_run() {
    // §6.1 rule 2's laundering, reproduced end to end.
    //
    // A full `pytest -q` passes. The agent then breaks the source with `sed -i`
    // through the Bash tool — which emits NO `fs_change`, because only Edit and
    // Write name a path. Rule 2 reasoned purely over edit events and reported
    // "no non-test source file changed after it" while the tree demonstrably
    // had: rule 1 reasons over tree hashes, rule 2 reasoned over edit events,
    // and when they disagreed rule 2 won.
    //
    // This is the same §0.3 class as the `.gitattributes` filter by a different
    // route, and far more reachable — every agent has a Bash tool.
    if !require_pytest() {
        return;
    }
    let a = Agent::new();
    let s = &a.session;
    a.hook(&format!(
        r#"{{"hook_event_name":"SessionStart","session_id":"{s}","transcript_path":"/t","cwd":"{}","source":"startup"}}"#,
        a.repo.display()
    ));

    // A full, honest, passing run.
    let out = a.bash("pytest -q");
    assert!(
        out.status.success(),
        "the baseline suite should pass: {}",
        String::from_utf8_lossy(&out.stdout)
    );

    // Then the source is broken through a tool that names no path.
    let out = a.bash("sed -i.bak 's/2.5/9.9/' shipping.py && rm -f shipping.py.bak");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        std::fs::read_to_string(a.repo.join("shipping.py"))
            .expect("read source")
            .contains("9.9"),
        "the sed did not take effect, so this test proves nothing"
    );
    a.hook(&format!(
        r#"{{"hook_event_name":"PostToolUse","session_id":"{s}","transcript_path":"/t","cwd":"{}","tool_name":"Bash","tool_input":{{"command":"sed -i 's/2.5/9.9/' shipping.py"}},"tool_response":"ok","tool_use_id":"toolu_sed"}}"#,
        a.repo.display()
    ));
    a.hook(&format!(
        r#"{{"hook_event_name":"Stop","session_id":"{s}","transcript_path":"/t","cwd":"{}","last_assistant_message":"All tests pass.","stop_hook_active":false}}"#,
        a.repo.display()
    ));
    a.fold();

    let report = a.claims_json();
    let claim = report["claims"]
        .as_array()
        .expect("claims")
        .iter()
        .find(|c| c["kind"] == "tests_pass")
        .cloned()
        .unwrap_or_else(|| panic!("no tests_pass claim: {report:#}"));

    assert_ne!(
        claim["verdict"],
        serde_json::json!("supported"),
        "a passing run was laundered across a source change nothing observed: {claim:#}"
    );

    // And the binding is in the JSON structurally, not only as prose — §6.1
    // says to record WHICH rule applied, and rule 2 is a relaxation a reader
    // must be able to tell from rule 1.
    let binding = &claim["binding"];
    assert!(
        !binding.is_null(),
        "the binding serialised as null: {claim:#}"
    );
    assert!(
        binding.get("superseded_by_unobserved_change").is_some()
            || binding.get("unobservable").is_some(),
        "the binding did not name the unobserved change: {binding:#}"
    );
}

#[test]
fn a_test_only_change_after_a_check_still_binds_and_says_so_structurally() {
    // The control for the test above. Rule 2 must still EXIST: §6.1 grants it
    // deliberately, and a fix that turned every non-identical tree into a
    // refusal would pass the laundering test by deleting the rule.
    if !require_pytest() {
        return;
    }
    let a = Agent::new();
    let s = &a.session;
    a.hook(&format!(
        r#"{{"hook_event_name":"SessionStart","session_id":"{s}","transcript_path":"/t","cwd":"{}","source":"startup"}}"#,
        a.repo.display()
    ));
    let out = a.bash("pytest -q");
    assert!(out.status.success());

    // A comment added to the TEST file only: the tree changes, no source does.
    let path = a.repo.join("test_shipping.py");
    let text = std::fs::read_to_string(&path).expect("read");
    std::fs::write(&path, format!("{text}# a note\n")).expect("write");
    a.hook(&format!(
        r#"{{"hook_event_name":"PostToolUse","session_id":"{s}","transcript_path":"/t","cwd":"{}","tool_name":"Bash","tool_input":{{"command":"echo '# a note' >> test_shipping.py"}},"tool_response":"ok","tool_use_id":"toolu_note"}}"#,
        a.repo.display()
    ));
    a.hook(&format!(
        r#"{{"hook_event_name":"Stop","session_id":"{s}","transcript_path":"/t","cwd":"{}","last_assistant_message":"All tests pass.","stop_hook_active":false}}"#,
        a.repo.display()
    ));
    a.fold();

    let report = a.claims_json();
    let claim = report["claims"]
        .as_array()
        .expect("claims")
        .iter()
        .find(|c| c["kind"] == "tests_pass")
        .cloned()
        .expect("a tests_pass claim");
    assert_eq!(
        claim["verdict"],
        serde_json::json!("supported"),
        "rule 2 was deleted rather than fixed — a test-only change now refuses: {claim:#}"
    );
    let binding = &claim["binding"];
    assert!(
        binding.get("no_relevant_change_since").is_some(),
        "{binding:#}"
    );
    assert!(
        binding["no_relevant_change_since"]["paths_ignored"]
            .as_array()
            .is_some_and(|a| a.iter().any(|p| p.as_str() == Some("test_shipping.py"))),
        "the tree half of rule 2 did not record which path it discounted: {binding:#}"
    );
}

#[test]
fn a_build_claim_is_refuted_by_the_build_that_actually_ran() {
    // §6.4 end to end, which is the wiring the unit tests cannot reach: a
    // wrapped `cargo build` becomes a `build`-category check in the ledger,
    // `session_view` reads it back, and `tool_checks` judges the claim.
    //
    // The repository here is a Python one, so `cargo build` genuinely fails —
    // a real non-zero exit from a real process, not a fixture.
    let a = Agent::new();
    let s = &a.session;
    a.hook(&format!(
        r#"{{"hook_event_name":"SessionStart","session_id":"{s}","transcript_path":"/t","cwd":"{}","source":"startup"}}"#,
        a.repo.display()
    ));
    let out = a.bash("cargo build");
    assert!(
        !out.status.success(),
        "cargo build unexpectedly succeeded here, so this proves nothing"
    );
    a.hook(&format!(
        r#"{{"hook_event_name":"Stop","session_id":"{s}","transcript_path":"/t","cwd":"{}","last_assistant_message":"The build succeeds.","stop_hook_active":false}}"#,
        a.repo.display()
    ));
    a.fold();

    let report = a.claims_json();
    let claim = report["claims"]
        .as_array()
        .expect("claims")
        .iter()
        .find(|c| c["kind"] == "build_ok")
        .cloned()
        .unwrap_or_else(|| panic!("no build_ok claim was extracted: {report:#}"));
    assert_eq!(
        claim["verdict"],
        serde_json::json!("unsupported"),
        "a failing build did not refute a build claim: {claim:#}"
    );
    assert_eq!(claim["verifier"], serde_json::json!("build_ok@1"));
}

#[test]
fn a_claim_with_no_command_behind_it_is_unverifiable_and_names_what_is_missing() {
    // The other half, and the one §6 cares most about: no evidence is not
    // refutation. The agent says the lint is clean and never ran a linter.
    let a = Agent::new();
    let s = &a.session;
    a.hook(&format!(
        r#"{{"hook_event_name":"SessionStart","session_id":"{s}","transcript_path":"/t","cwd":"{}","source":"startup"}}"#,
        a.repo.display()
    ));
    a.hook(&format!(
        r#"{{"hook_event_name":"Stop","session_id":"{s}","transcript_path":"/t","cwd":"{}","last_assistant_message":"Lint clean. No secrets committed.","stop_hook_active":false}}"#,
        a.repo.display()
    ));
    a.fold();

    let report = a.claims_json();
    let claims = report["claims"].as_array().expect("claims");
    let lint = claims
        .iter()
        .find(|c| c["kind"] == "lint_ok")
        .unwrap_or_else(|| panic!("{report:#}"));
    assert_eq!(
        lint["verdict"],
        serde_json::json!("unverifiable"),
        "{lint:#}"
    );
    assert!(lint["reasons"][0]
        .as_str()
        .unwrap_or_default()
        .contains("no lint command was captured"));

    // And `secrets_absent` has its CONFLICT-8 ceiling even with a clean diff.
    let secrets = claims
        .iter()
        .find(|c| c["kind"] == "secrets_absent")
        .unwrap_or_else(|| panic!("{report:#}"));
    assert_eq!(secrets["verdict"], serde_json::json!("unverifiable"));
    assert!(secrets["reasons"][0]
        .as_str()
        .unwrap_or_default()
        .contains("never `supported`"));
}

#[test]
fn a_promise_is_open_until_a_later_session_touches_what_it_named() {
    // §3.5 / §11 / §14 — the cross-session half. Within one session every
    // promise is trailing, because the only prose read is the final message, so
    // fulfilment can only come from a later session.
    if !require_pytest() {
        return;
    }
    let a = run_the_cheating_agent();

    let promises = |args: &[&str]| -> serde_json::Value {
        let out = Command::new(thruline())
            .args(args)
            .env("THRULINE_HOME", &a.home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .current_dir(&a.repo)
            .output()
            .expect("promises");
        serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stdout)))
    };

    // Recording happens when `claims` runs, so the promise is in the store.
    let _ = a.claims_json();
    let listed = promises(&["promises", "--open", "--json"]);
    let open: Vec<&str> = listed["promises"]
        .as_array()
        .expect("promises")
        .iter()
        .filter_map(|p| p["text"].as_str())
        .collect();
    assert!(
        open.iter().any(|t| t.contains("regression test")),
        "the promise was not recorded as open: {open:?}"
    );

    // A LATER session touches the file the promise named.
    let s2 = "22222222-3333-4333-8444-555555555555";
    a.hook(&format!(
        r#"{{"hook_event_name":"SessionStart","session_id":"{s2}","transcript_path":"/t","cwd":"{}","source":"startup"}}"#,
        a.repo.display()
    ));
    a.hook(&format!(
        r#"{{"hook_event_name":"PostToolUse","session_id":"{s2}","transcript_path":"/t","cwd":"{}","tool_name":"Edit","tool_input":{{"file_path":"test_regression_large_weight.py"}},"tool_response":{{"success":true}},"tool_use_id":"t9"}}"#,
        a.repo.display()
    ));
    a.fold();

    let listed = promises(&["promises", "--all", "--json"]);
    let statuses: Vec<(&str, &str)> = listed["promises"]
        .as_array()
        .expect("promises")
        .iter()
        .filter_map(|p| Some((p["text"].as_str()?, p["status"].as_str()?)))
        .collect();
    assert!(
        statuses
            .iter()
            .any(|(t, s)| t.contains("regression test") && *s == "fulfilled"),
        "a later session did the thing and the promise stayed open: {statuses:?}"
    );
}

#[test]
fn a_promise_nothing_touched_stays_open_rather_than_being_assumed_done() {
    // The direction the heuristic errs in, asserted. A promise reported as
    // outstanding when it was quietly done is visible and correctable; one
    // reported as done when it was dropped is the failure this exists to catch.
    if !require_pytest() {
        return;
    }
    let a = run_the_cheating_agent();
    let _ = a.claims_json();

    // A later session that changes something unrelated.
    let s2 = "33333333-4444-4333-8444-555555555555";
    a.hook(&format!(
        r#"{{"hook_event_name":"PostToolUse","session_id":"{s2}","transcript_path":"/t","cwd":"{}","tool_name":"Edit","tool_input":{{"file_path":"unrelated_config.ini"}},"tool_response":{{"success":true}},"tool_use_id":"t9"}}"#,
        a.repo.display()
    ));
    a.fold();

    let out = Command::new(thruline())
        .args(["promises", "--all", "--json"])
        .env("THRULINE_HOME", &a.home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(&a.repo)
        .output()
        .expect("promises");
    let listed: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert!(
        listed["promises"]
            .as_array()
            .expect("promises")
            .iter()
            .any(
                |p| p["text"].as_str().unwrap_or("").contains("regression test")
                    && p["status"] == "open"
            ),
        "an unrelated edit closed a promise: {listed:#}"
    );
}

#[test]
fn a_committed_private_key_refutes_a_secrets_absent_claim() {
    // The most serious thing `secrets_absent` exists to catch, and the one the
    // conjunction dropped: a private key fires `private_key_block` and has no
    // prefix for `resembles_known_credential` to find, so requiring both left
    // it `unverifiable` rather than accused.
    let a = Agent::new();
    let s = &a.session;
    a.hook(&format!(
        r#"{{"hook_event_name":"SessionStart","session_id":"{s}","transcript_path":"/t","cwd":"{}","source":"startup"}}"#,
        a.repo.display()
    ));
    std::fs::write(
        a.repo.join("deploy_key.pem"),
        "-----BEGIN PRIVATE KEY-----\n\
         MC4CAQAwBQYDK2VwBCIEIL9k1sQFCJTZ1cVMcVQmpTfbUvC7Y0f5wY6wZ3vJqK9M\n\
         -----END PRIVATE KEY-----\n",
    )
    .expect("write key");
    a.hook(&format!(
        r#"{{"hook_event_name":"PostToolUse","session_id":"{s}","transcript_path":"/t","cwd":"{}","tool_name":"Write","tool_input":{{"file_path":"deploy_key.pem"}},"tool_response":{{"success":true}},"tool_use_id":"t1"}}"#,
        a.repo.display()
    ));
    a.hook(&format!(
        r#"{{"hook_event_name":"Stop","session_id":"{s}","transcript_path":"/t","cwd":"{}","last_assistant_message":"Added the deploy key. No secrets committed.","stop_hook_active":false}}"#,
        a.repo.display()
    ));
    a.fold();

    let report = a.claims_json();
    let claim = report["claims"]
        .as_array()
        .expect("claims")
        .iter()
        .find(|c| c["kind"] == "secrets_absent")
        .cloned()
        .unwrap_or_else(|| panic!("no secrets_absent claim: {report:#}"));
    assert_eq!(
        claim["verdict"],
        serde_json::json!("unsupported"),
        "a committed private key did not refute `no secrets committed`: {claim:#}"
    );
    assert!(claim["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .any(|f| f["detail"]
            .as_str()
            .unwrap_or("")
            .contains("private_key_block")));
}

#[test]
fn an_ordinary_change_does_not_accuse_the_developer_of_committing_a_secret() {
    // The other half. `secrets_absent`'s ceiling is `unverifiable`, so a false
    // positive cannot be corrected by anything downstream — it is the end of
    // the line for that claim.
    let a = Agent::new();
    let s = &a.session;
    a.hook(&format!(
        r#"{{"hook_event_name":"SessionStart","session_id":"{s}","transcript_path":"/t","cwd":"{}","source":"startup"}}"#,
        a.repo.display()
    ));
    // Values that LOOK credential-adjacent and are not: an env var name, a
    // documented placeholder, a git sha, a cert path.
    std::fs::write(
        a.repo.join("config.ini"),
        "[deploy]\n\
         token_env = GITHUB_TOKEN\n\
         example_key = <your-api-key-here>\n\
         pinned_commit = 3886ba5d1f0c8e2a9b7d4e6f1a2c3d4e5f60718\n\
         tls_cert_path = /etc/ssl/certs/server.pem\n",
    )
    .expect("write config");
    a.hook(&format!(
        r#"{{"hook_event_name":"PostToolUse","session_id":"{s}","transcript_path":"/t","cwd":"{}","tool_name":"Write","tool_input":{{"file_path":"config.ini"}},"tool_response":{{"success":true}},"tool_use_id":"t1"}}"#,
        a.repo.display()
    ));
    a.hook(&format!(
        r#"{{"hook_event_name":"Stop","session_id":"{s}","transcript_path":"/t","cwd":"{}","last_assistant_message":"No secrets committed.","stop_hook_active":false}}"#,
        a.repo.display()
    ));
    a.fold();

    let report = a.claims_json();
    let claim = report["claims"]
        .as_array()
        .expect("claims")
        .iter()
        .find(|c| c["kind"] == "secrets_absent")
        .cloned()
        .expect("a secrets_absent claim");
    assert_eq!(
        claim["verdict"],
        serde_json::json!("unverifiable"),
        "an ordinary config change accused the developer: {claim:#}"
    );
}

#[test]
fn a_placeholder_key_block_is_masked_and_does_not_accuse_end_to_end() {
    // A209-C12's disclosed over-mask, through the whole product. The masker
    // still masks it — that is the accepted redactor behaviour — and
    // `secrets_absent` must not turn it into an accusation.
    let a = Agent::new();
    let s = &a.session;
    a.hook(&format!(
        r#"{{"hook_event_name":"SessionStart","session_id":"{s}","transcript_path":"/t","cwd":"{}","source":"startup"}}"#,
        a.repo.display()
    ));
    std::fs::write(
        a.repo.join("deploy_key.pem.example"),
        "-----BEGIN PRIVATE KEY-----\nnot-a-real-key\n-----END PRIVATE KEY-----\n",
    )
    .expect("write");
    a.hook(&format!(
        r#"{{"hook_event_name":"PostToolUse","session_id":"{s}","transcript_path":"/t","cwd":"{}","tool_name":"Write","tool_input":{{"file_path":"deploy_key.pem.example"}},"tool_response":{{"success":true}},"tool_use_id":"t1"}}"#,
        a.repo.display()
    ));
    a.hook(&format!(
        r#"{{"hook_event_name":"Stop","session_id":"{s}","transcript_path":"/t","cwd":"{}","last_assistant_message":"Added a key template. No secrets committed.","stop_hook_active":false}}"#,
        a.repo.display()
    ));
    a.fold();

    let claim = a.claims_json()["claims"]
        .as_array()
        .expect("claims")
        .iter()
        .find(|c| c["kind"] == "secrets_absent")
        .cloned()
        .expect("a secrets_absent claim");
    assert_eq!(
        claim["verdict"],
        serde_json::json!("unverifiable"),
        "a placeholder key template falsely accused the developer: {claim:#}"
    );
}

/// IR-7: the receipt may not assert `supported` about a tree its check never ran on.
///
/// `thruline claims` derives the verdict against the **session's final
/// snapshot**. `thruline receipt` stores that verdict beside a `gitTree` taken
/// from the **working tree at receipt time**. Those are two different trees the
/// moment anything is written after the tests — and the acceptance walk writes
/// something every run, because step 13 is `policy --init`.
///
/// So the claim is derived about one tree and asserted about another, and
/// `verify-receipt` re-derives it against the second and contradicts it. Both
/// halves are internally right; the receipt is what puts a true sentence next
/// to the wrong subject.
///
/// This asserts the **consequence** — that the receipt's own verifier does not
/// contradict it — rather than any internal call's return value. A guard that
/// checked the derivation returned `Ok` is the guard this failure walks past.
#[test]
fn a_receipt_never_asserts_supported_about_a_tree_its_check_did_not_run_on() {
    if !require_pytest() {
        return;
    }
    let a = Agent::new();
    let s = &a.session;
    a.hook(&format!(
        r#"{{"hook_event_name":"SessionStart","session_id":"{s}","transcript_path":"/t","cwd":"{}","source":"startup"}}"#,
        a.repo.display()
    ));
    std::fs::write(
        a.repo.join("shipping.py"),
        "def rate(weight_kg):\n    return round(2.5 * weight_kg + 7.5, 2)\n",
    )
    .expect("edit source");
    std::fs::write(
        a.repo.join("test_shipping.py"),
        "from shipping import rate\n\n\
         def test_rate_small():\n    assert rate(2) == 12.5\n\n\
         def test_rate_large():\n    assert rate(10) == 32.5\n",
    )
    .expect("update test");
    a.hook(&format!(
        r#"{{"hook_event_name":"PostToolUse","session_id":"{s}","transcript_path":"/t","cwd":"{}","tool_name":"Edit","tool_input":{{"file_path":"test_shipping.py"}},"tool_response":{{"success":true}},"tool_use_id":"toolu_e"}}"#,
        a.repo.display()
    ));
    let out = a.bash("pytest -q");
    assert!(out.status.success(), "the honest run should pass");
    a.hook(&format!(
        r#"{{"hook_event_name":"Stop","session_id":"{s}","transcript_path":"/t","cwd":"{}","last_assistant_message":"All tests pass.","stop_hook_active":false}}"#,
        a.repo.display()
    ));
    a.fold();

    // The control. Without this the test could pass because nothing was
    // `supported` in the first place, which proves nothing at all.
    let report = a.claims_json();
    let tests_pass = report["claims"]
        .as_array()
        .expect("claims")
        .iter()
        .find(|c| c["kind"] == "tests_pass")
        .unwrap_or_else(|| panic!("no `tests_pass` claim: {report:#}"))
        .clone();
    assert_eq!(
        tests_pass["verdict"],
        serde_json::json!("supported"),
        "the control failed: nothing was `supported`, so this test proves nothing"
    );

    // What the walk's step 13 does, one step before the receipt is made.
    std::fs::create_dir_all(a.repo.join(".thruline")).expect("mkdir");
    std::fs::write(a.repo.join(".thruline").join("policy.yml"), "version: 1\n")
        .expect("write policy");

    let receipt = a.receipt();
    let text = a.verify_receipt(&receipt);
    assert!(
        !text.contains("FAIL verdict_derivation"),
        "the receipt asserts `supported` over a check that ran on a different \
         tree than the receipt is about, and its own verifier says so one \
         command later:\n\n{text}"
    );
}
