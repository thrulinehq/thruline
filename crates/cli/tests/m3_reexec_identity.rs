//! A re-executed check's identity — BUILD-SPEC §6.1 and §6.3, over §7's output.
//!
//! # The defect these tests were written against
//!
//! `session_view` looked a check's position and command up by its
//! `exec_event_id`. A **re-executed** check has no `exec` event — deliberately:
//! writing one into the append-only chain ate hook deliveries — so the lookup
//! missed, and the miss produced two invented values:
//!
//! - `seq = u64::MAX`, which sorts after every edit, so §6.1 rule 2's *observed*
//!   half skipped the entire edit record and called the silence agreement;
//! - `argv = []`, which carries no narrowing flag, so §6.3's narrowing detector
//!   reported nothing about a `pytest -k` re-execution — **the strongest
//!   evidence class the product produces, presenting as a clean full-suite
//!   pass**, beside `reexecuted: true` in the receipt.
//!
//! Both facts are on the `Check` row already, because §8.1's container image
//! digest is kept there. These tests read them back through the real
//! `thruline claims`, and each one is paired with a **control** that fails when
//! the sentinel is reintroduced — see the module-level note on each.
//!
//! # The fixture cannot drift from the product
//!
//! The re-executed row's `parsed` column is built by
//! [`thruline_reexec::parsed_block`] — *the same function `thruline reexec`
//! calls*. A hand-written JSON fixture would drift toward whatever the reader
//! expects; this one cannot, because there is one producer of the shape.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use thruline_ledger::{CheckRow, Ledger};
use thruline_reexec::{ExecResult, ImageSource, Prepared};

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

fn require_pytest() -> bool {
    if pytest_available() {
        return true;
    }
    if std::env::var_os("THRULINE_TEST_NO_PYTEST").is_some() {
        eprintln!("SKIPPED: pytest is not installed.");
        return false;
    }
    panic!("pytest is not on PATH; this file needs a real runner");
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
        // `vendor/` is ignored: a write there changes behaviour (Python imports
        // `sitecustomize` from anything on the path) and changes NO tree.
        std::fs::write(
            repo.join(".gitignore"),
            "__pycache__/\n.pytest_cache/\nvendor/\n",
        )
        .expect("w");
        for args in [vec!["add", "-A"], vec!["commit", "-qm", "baseline"]] {
            let out = Command::new("git")
                .current_dir(&repo)
                .args(&args)
                .output()
                .expect("git");
            assert!(out.status.success());
        }
        let shim = home.join("shim");
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
            session: "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee".to_string(),
        }
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
            .expect("bash")
    }

    fn edited(&self, path: &str) {
        self.hook(&format!(
            r#"{{"hook_event_name":"PostToolUse","session_id":"{}","transcript_path":"/t","cwd":"{}","tool_name":"Edit","tool_input":{{"file_path":"{path}"}},"tool_response":{{"success":true}},"tool_use_id":"toolu_{path}"}}"#,
            self.session,
            self.repo.display()
        ));
    }

    fn stop(&self, message: &str) {
        self.hook(&format!(
            r#"{{"hook_event_name":"Stop","session_id":"{}","transcript_path":"/t","cwd":"{}","last_assistant_message":"{message}","stop_hook_active":false}}"#,
            self.session,
            self.repo.display()
        ));
    }

    fn fold(&self) {
        let out = Command::new(thruline())
            .args(["fold"])
            .env("THRULINE_HOME", &self.home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .current_dir(&self.repo)
            .output()
            .expect("fold");
        assert!(out.status.success());
    }

    fn ledger(&self) -> Ledger {
        Ledger::open(&self.home.join("exec.db")).expect("open ledger")
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

    /// Record a re-execution of `argv` at `tree`, exactly as `thruline reexec`
    /// records one: no `exec` event, the evidence on the `Check` row, and the
    /// `parsed` block built by the product's own writer.
    fn record_re_execution(&self, of_check: &str, argv: &[&str], tree: &str, tally: (u64, u64)) {
        let argv: Vec<String> = argv.iter().map(|s| (*s).to_string()).collect();
        let prepared = Prepared {
            image: "python:3.13.7-slim-bookworm".to_string(),
            image_digest: "sha256:df4f1e3a519256c244298c2464bdee096b63f86e095b55a47619b48d0a82b909"
                .to_string(),
            repo_digest: None,
            image_source: ImageSource::EcosystemDefault,
            workdir: PathBuf::from("/w"),
            materialized: thruline_reexec::materialize::Materialized::default(),
            network: false,
        };
        let result = ExecResult {
            exit_code: 0,
            stdout: Vec::new(),
            stderr: Vec::new(),
            duration_ms: 42,
            stdio: "pipe",
            timed_out: false,
        };
        let parsed = thruline_reexec::parsed_block(
            &serde_json::json!({
                "tests_total": tally.0,
                "passed": tally.1,
                "failed": 0,
                "skipped": 0,
                "xfail": 0,
            }),
            &prepared,
            "claimed",
            of_check,
            &argv,
            &result,
        );
        let row = CheckRow {
            id: format!("chk_reexec_{}", of_check.trim_start_matches("chk_")),
            session_id: self.session.clone(),
            // The empty id is the product's own record: no `exec` event exists.
            exec_event_id: String::new(),
            category: "test".to_string(),
            runner: "pytest".to_string(),
            argv_hash: "sha256:0".to_string(),
            cwd_hash: "sha256:0".to_string(),
            env_hash: "sha256:0".to_string(),
            input_tree_hash: tree.to_string(),
            exit_code: 0,
            parsed: parsed.to_string(),
        };
        assert_eq!(
            self.ledger().insert_check(&row).expect("insert"),
            1,
            "the re-executed check row was not written"
        );
    }
}

/// The captured test checks of the session, by id → (tree, argv-is-recorded).
fn captured_test_checks(a: &Agent) -> BTreeMap<String, String> {
    a.ledger()
        .checks_for_session(&a.session)
        .expect("checks")
        .into_iter()
        .filter(|c| c.category == "test" && !c.exec_event_id.is_empty())
        .map(|c| (c.id, c.input_tree_hash))
        .collect()
}

/// The tree hash of the session's last `snapshot` event.
fn last_snapshot_tree(a: &Agent) -> String {
    a.ledger()
        .session_events(&a.session)
        .expect("events")
        .iter()
        .filter(|e| e.kind().as_str().as_str() == "snapshot")
        .filter_map(|e| {
            e.payload()
                .get("tree_hash")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .next_back()
        .expect("a snapshot")
}

fn tests_pass_claim(v: &serde_json::Value) -> &serde_json::Value {
    v["claims"]
        .as_array()
        .expect("claims array")
        .iter()
        .find(|c| c["kind"] == serde_json::json!("tests_pass"))
        .unwrap_or_else(|| panic!("no tests_pass claim in {v:#}"))
}

fn detectors(claim: &serde_json::Value) -> Vec<String> {
    claim["findings"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|f| f["detector"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// 1. §6.3 narrowing, on a re-execution
// ---------------------------------------------------------------------------

/// A narrowed **re-execution** is detected as narrowed.
///
/// The session: the agent ran a narrowed suite, then edited the source, so its
/// own run is superseded and the re-execution at the final tree is the only
/// bound test check. That is §7's whole purpose, and it is exactly the shape in
/// which the missing `argv` was decisive: the chosen check was the re-executed
/// one, and it looked like a full run.
#[test]
fn a_narrowed_re_execution_is_detected_as_narrowed() {
    if !require_pytest() {
        return;
    }
    let a = Agent::new();
    a.hook(&format!(
        r#"{{"hook_event_name":"SessionStart","session_id":"{}","transcript_path":"/t","cwd":"{}","source":"startup"}}"#,
        a.session,
        a.repo.display()
    ));
    // The agent's own run: narrowed, and it will be superseded by the edit.
    let out = a.bash("pytest -q -k test_rate_small");
    assert!(out.status.success(), "pytest -k should pass");
    a.fold();
    let captured = captured_test_checks(&a);
    assert_eq!(
        captured.len(),
        1,
        "expected one captured check: {captured:?}"
    );
    let (of_check, tree_at_check) = captured.iter().next().expect("one");

    // Then it edits the source, so the tree moves on and its own run no longer
    // binds. The re-execution is at that FINAL tree.
    std::fs::write(
        a.repo.join("shipping.py"),
        "def rate(weight_kg):\n    return round(2.5 * weight_kg + 7.5, 2)\n",
    )
    .expect("edit");
    a.edited("shipping.py");
    a.stop("All tests pass.");
    a.fold();

    let final_tree = last_snapshot_tree(&a);
    assert_ne!(
        &final_tree, tree_at_check,
        "the edit must move the tree, or the captured run would still bind"
    );

    a.record_re_execution(
        of_check,
        &["pytest", "-q", "-k", "test_rate_small"],
        &final_tree,
        (1, 1),
    );

    let claim = a.claims_json();
    let claim = tests_pass_claim(&claim);
    let found = detectors(claim);
    assert!(
        found.iter().any(|d| d == "narrowed_run"),
        "a narrowed re-execution must raise `narrowed_run`; findings were {found:?} \
         and the claim was {claim:#}"
    );
    assert_eq!(
        claim["verdict"],
        serde_json::json!("unsupported"),
        "a `scope: all` claim backed only by a narrowed re-execution must not be \
         supported: {claim:#}"
    );
    // The evidence really is the re-executed row, not the agent's own run —
    // otherwise this test would pass for the wrong reason.
    let evidence: Vec<String> = claim["evidence"]
        .as_array()
        .expect("evidence")
        .iter()
        .filter_map(|e| e.as_str().map(str::to_string))
        .collect();
    assert!(
        evidence.iter().any(|e| e.starts_with("chk_reexec_")),
        "the chosen check should be the re-executed one; evidence was {evidence:?}"
    );
}

// ---------------------------------------------------------------------------
// 2. §6.1 ordering, on a re-execution
// ---------------------------------------------------------------------------

/// A re-executed check is ordered against the **edit record**, like any other.
///
/// The laundering route this closes needs both halves of §6.1 rule 2 to miss at
/// once, and `seq = u64::MAX` did exactly that:
///
/// - the *observed* half skipped every edit, because everything sorts before
///   `u64::MAX`;
/// - the *tree* half cannot see a write that changes no tree — and a write to a
///   **gitignored** path changes none.
///
/// So an Edit-tool write to `vendor/sitecustomize.py` — which Python imports,
/// and which can make a suite pass — was invisible to both. It needs no
/// `.gitattributes`, so `capture_degraded` does not defend against it either.
///
/// The re-execution here is the session's only test check, which is what
/// `reexec --mode full` produces: the agent ran nothing, and §7 ran the
/// repository's canonical command.
#[test]
fn a_re_executed_check_is_ordered_against_a_write_no_tree_can_see() {
    let a = Agent::new();
    a.hook(&format!(
        r#"{{"hook_event_name":"SessionStart","session_id":"{}","transcript_path":"/t","cwd":"{}","source":"startup"}}"#,
        a.session,
        a.repo.display()
    ));
    // A first edit, so there is a snapshot to anchor the re-execution to.
    std::fs::write(
        a.repo.join("shipping.py"),
        "def rate(weight_kg):\n    return round(2.5 * weight_kg + 7.5, 2)\n",
    )
    .expect("edit");
    a.edited("shipping.py");
    a.fold();
    let tree_at_reexec = last_snapshot_tree(&a);

    // §7 runs the canonical command at that tree. No captured check exists.
    a.record_re_execution("", &["pytest", "-q"], &tree_at_reexec, (2, 2));
    assert!(
        captured_test_checks(&a).is_empty(),
        "this fixture must isolate the re-executed check"
    );

    // ...and only THEN the agent writes the ignored file — no tree can see it —
    // and touches a test file, so the tree does move and §6.1 rule 1 does not
    // short-circuit the question.
    std::fs::create_dir_all(a.repo.join("vendor")).expect("mkdir");
    std::fs::write(
        a.repo.join("vendor/sitecustomize.py"),
        "import shipping\nshipping.rate = lambda w: 9.0\n",
    )
    .expect("w");
    a.edited("vendor/sitecustomize.py");
    std::fs::write(
        a.repo.join("test_shipping.py"),
        "from shipping import rate\n\n\
         def test_rate_small():\n    assert rate(2) == 9.0\n",
    )
    .expect("w");
    a.edited("test_shipping.py");
    a.stop("All tests pass.");
    a.fold();

    let json = a.claims_json();
    let claim = tests_pass_claim(&json);
    assert_ne!(
        tree_at_reexec,
        last_snapshot_tree(&a),
        "the test-file edit must move the tree, or rule 1 short-circuits and this \
         fixture proves nothing"
    );
    // The binding names BOTH facts, and it has to: this row is a re-execution
    // (so §6.3 will not let it carry the claim — see
    // `crates/verify/tests/reexec_cannot_promote.rs`) **and** §6.1 would have
    // called it superseded anyway. Recording only the first would drop the
    // ignored-path write, which is the finding this fixture exists for.
    let binding = claim["binding"].clone();
    let underlying = binding["reexecuted"]["underlying"].clone();
    assert!(
        underlying.get("superseded_by_edit").is_some(),
        "the ignored-path write happened after the re-execution and must supersede \
         it; the binding was {binding:#} and the claim {claim:#}"
    );
    assert_eq!(
        underlying["superseded_by_edit"]["path"],
        serde_json::json!("vendor/sitecustomize.py"),
        "the superseding path must be the one no tree can see: {binding:#}"
    );
    // And the supersession is a `fail` finding that NAMES the path, not only a
    // shape in the binding — a reader of the findings list must see it too.
    let superseded = claim["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .find(|f| f["detector"] == serde_json::json!("reexecution_superseded"))
        .unwrap_or_else(|| panic!("no `reexecution_superseded` finding: {claim:#}"));
    assert_eq!(superseded["severity"], serde_json::json!("fail"));
    assert!(
        superseded["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("vendor/sitecustomize.py"),
        "the finding does not name the write it is about: {superseded:#}"
    );
    assert_eq!(
        claim["verdict"],
        serde_json::json!("unsupported"),
        "{claim:#}"
    );
}

// ---------------------------------------------------------------------------
// 3. the sentinel is gone from the type, not only from one call site
// ---------------------------------------------------------------------------

/// `session_view` must not reintroduce a stand-in for a missing fact.
///
/// A source census, because the defect was a *value*, not a shape: `u64::MAX`
/// and an empty `argv` both compile perfectly and both read as agreement.
#[test]
fn the_view_builder_names_no_sentinel_position() {
    let src = include_str!("../src/claims_cmd.rs");
    let offending: Vec<&str> = src
        .lines()
        .filter(|l| {
            let code = l.split("//").next().unwrap_or(l);
            code.contains("u64::MAX") || code.contains("i64::MAX")
        })
        .collect();
    assert!(
        offending.is_empty(),
        "a sentinel position is back in the view builder: {offending:?}"
    );
}
