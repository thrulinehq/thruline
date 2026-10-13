//! BUILD-PLAN M3.4 and BUILD-SPEC §17, one test per property, each of which
//! **goes red when its property is deleted**.
//!
//! # What this file used to be
//!
//! Seven tests, `ok. 7 passed; 0 failed`, and a mutation pass that deleted the
//! block decision, the watchdog, the per-claim ledger and the whole
//! `stop_hook_active` branch without turning a single one of them red. Three
//! causes, all of them still visible in the git history of this file:
//!
//! 1. **Three tests were `pytest_available()`-gated and returned early.** On
//!    this machine, with pytest installed, the old suite ran in **4.49 s**;
//!    under `PATH=/usr/bin:/bin` it ran in **2.00 s** and printed the same
//!    `7 passed`. The only signal that most of the slice had gone untested was
//!    the wall clock. A silently-skipping test is the Rule 12 failure this
//!    program exists to stop, so there is no skip in this file: the fixture
//!    ships its own runner and **asserts its own premises**.
//! 2. **The predicate guard asserted `assert_ne!` over two closures defined in
//!    the test file** and never invoked the gate at all. It could not fail for
//!    any state of `gate_cmd.rs`, and it did not. It is gone; `p2` drives a
//!    real `unverifiable` session through the binary instead.
//! 3. **§17's "exactly once" test could not isolate what produced the once.**
//!    Two mechanisms yield it — the per-claim ledger and `max_stop_blocks` —
//!    so deleting either left the other to carry the assertion. `p7` blocks a
//!    *different* claim afterwards, which the count still permits, so only the
//!    per-claim ledger can explain the allow.
//!
//! # The fixture is hermetic
//!
//! `pytest` here is a four-line shell script written into the fixture's own
//! `bin` directory, put on `PATH` ahead of everything, and wrapped by the shim
//! like any other runner. It fails on purpose, deterministically, in about a
//! millisecond, on a machine with no Python at all. Nothing in this file
//! consults the host for anything but `git` and `sh`.

use std::path::PathBuf;
use std::process::Command;

fn thruline() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

/// A `PATH` with nothing on it but the fixture's own directories and the two
/// system ones `git` and `sh` live in.
///
/// Deliberately **not** the inherited `PATH`: a host `pytest` reachable behind
/// ours would make this suite's meaning a function of the developer's machine,
/// which is the defect the old file shipped.
fn hermetic_path(dirs: &[&std::path::Path]) -> String {
    let mut parts: Vec<String> = dirs.iter().map(|d| d.display().to_string()).collect();
    parts.push("/usr/bin".to_string());
    parts.push("/bin".to_string());
    parts.join(":")
}

struct Agent {
    _dir: tempfile::TempDir,
    home: PathBuf,
    repo: PathBuf,
    shim: PathBuf,
    bin: PathBuf,
    session: String,
}

impl Agent {
    /// A repository, a fake runner, and a shim that wraps it.
    ///
    /// Every premise this fixture depends on is asserted here, loudly, so a
    /// broken fixture is a failure and never a quiet pass.
    fn new(session: &str) -> Agent {
        let dir = tempfile::tempdir().expect("tempdir");
        let home = dir.path().join("home");
        let repo = dir.path().join("repo");
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&repo).expect("repo");
        std::fs::create_dir_all(&bin).expect("bin");
        Agent::write_fake_pytest(&bin);

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
             def test_rate_small():\n    assert rate(2) == 9.0\n",
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
            .args(["shim", "install", "--json"])
            .env("THRULINE_HOME", &home)
            .env("PATH", hermetic_path(&[&bin]))
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .current_dir(&repo)
            .output()
            .expect("shim install");
        assert!(
            out.status.success(),
            "shim install failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        // The premise, stated. `WrapPolicy::Resolvable` writes a wrapper only
        // for a program it can resolve, so a fixture whose fake runner is not
        // on the install PATH would capture nothing and every verdict below
        // would be `unverifiable` — which several of these tests would read as
        // a pass.
        let report: serde_json::Value = serde_json::from_slice(&out.stdout).expect("shim json");
        assert!(
            report["wrappers"].get("pytest").is_some(),
            "the shim wrote no `pytest` wrapper, so nothing in this file observes a test run: \
             {report}"
        );

        Agent {
            _dir: dir,
            home,
            repo,
            shim,
            bin,
            session: session.to_string(),
        }
    }

    /// The runner. Four lines of `sh`, no Python, no host.
    ///
    /// It prints a real pytest terminal summary because §6.2's parser looks for
    /// one, and it exits 1 because §6.3 treats a captured test run that exited
    /// non-zero as evidence *against* a `tests_pass` claim.
    fn write_fake_pytest(bin: &std::path::Path) {
        let path = bin.join("pytest");
        std::fs::write(
            &path,
            "#!/bin/sh\n\
             echo 'F'\n\
             echo '=========================== short test summary info ============================'\n\
             echo 'FAILED test_shipping.py::test_rate_small - assert 0.0 == 9.0'\n\
             echo '========================= 1 failed, 1 passed in 0.01s =========================='\n\
             exit 1\n",
        )
        .expect("write pytest");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
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

    fn start(&self) {
        self.hook(&format!(
            r#"{{"hook_event_name":"SessionStart","session_id":"{}","transcript_path":"/t","cwd":"{}","source":"startup"}}"#,
            self.session,
            self.repo.display()
        ));
    }

    /// Run the fake suite through the shim, as the agent's own shell would.
    fn run_the_suite(&self) {
        let out = Command::new("sh")
            .args(["-c", "pytest -q"])
            .current_dir(&self.repo)
            .env("THRULINE_HOME", &self.home)
            .env("PATH", hermetic_path(&[&self.shim, &self.bin]))
            .env("CLAUDE_CODE_SESSION_ID", &self.session)
            .output()
            .expect("sh");
        assert_eq!(
            out.status.code(),
            Some(1),
            "the fixture's runner was supposed to fail: {} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// The execution store the gate now keeps its block ledger in.
    ///
    /// Opening it through the binary first is load-bearing, not tidiness:
    /// `rusqlite::Connection::open` CREATES an empty database at a path that
    /// does not exist, and an empty database has no `gate_blocks` table. A test
    /// that forged into one of those would be writing to a file the product
    /// never reads, and would pass while proving nothing.
    fn exec_db(&self) -> PathBuf {
        let db = self.home.join("exec.db");
        if !db.is_file() {
            // `fold` is what the gate itself runs first, and it is the command
            // that creates and migrates the store.
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
                String::from_utf8_lossy(&out.stderr)
            );
        }
        assert!(
            db.is_file(),
            "{} was not created by the binary, so anything written there is not the \
             store the gate reads",
            db.display()
        );
        db
    }

    /// How many blocks the store records for this session, read past the
    /// chain check — the raw row count, so a test can tell "the chain is
    /// broken" from "there are no rows".
    fn recorded_blocks(&self) -> i64 {
        let conn = rusqlite::Connection::open(self.exec_db()).expect("open exec.db");
        conn.query_row(
            "SELECT count(*) FROM gate_blocks WHERE session_id = ?1",
            rusqlite::params![self.session],
            |r| r.get(0),
        )
        .expect("count")
    }

    /// Write a row into the gate's block ledger **by hand**, which is what
    /// Track R's G2 did to the loose file. This is the attack, expressed
    /// against the store that replaced it.
    /// The real `hash` of this session's last recorded block, so a forgery can
    /// be made to *follow* the chain and exercise the content check instead of
    /// the link check.
    fn last_block_hash(&self) -> String {
        let conn = rusqlite::Connection::open(self.exec_db()).expect("open exec.db");
        conn.query_row(
            "SELECT hash FROM gate_blocks WHERE session_id = ?1 ORDER BY seq DESC LIMIT 1",
            rusqlite::params![self.session],
            |r| r.get(0),
        )
        .expect("a recorded block to follow")
    }

    /// Forge a row with a chosen `prev_hash` and `hash`, so each arm of the
    /// chain check can be reached on its own. Without this, a single forgery
    /// shape reddens only the first arm it happens to trip and the others are
    /// guards nothing tests.
    fn forge_block_with(&self, claim_key: &str, prev_hash: &str, hash: &str, seq: Option<i64>) {
        let conn = rusqlite::Connection::open(self.exec_db()).expect("open exec.db");
        let next: i64 = seq.unwrap_or_else(|| {
            conn.query_row(
                "SELECT coalesce(max(seq), 0) + 1 FROM gate_blocks WHERE session_id = ?1",
                rusqlite::params![self.session],
                |r| r.get(0),
            )
            .expect("next seq")
        });
        conn.execute(
            "INSERT INTO gate_blocks(session_id, seq, claim_key, claim_kind, at, prev_hash, hash)
             VALUES(?1, ?2, ?3, 'tests_pass', '2026-01-01 00:00:00', ?4, ?5)",
            rusqlite::params![self.session, next, claim_key, prev_hash, hash],
        )
        .expect("insert a forged block");
    }

    fn forge_block(&self, claim_key: &str) {
        let conn = rusqlite::Connection::open(self.exec_db()).expect("open exec.db");
        let next: i64 = conn
            .query_row(
                "SELECT coalesce(max(seq), 0) + 1 FROM gate_blocks WHERE session_id = ?1",
                rusqlite::params![self.session],
                |r| r.get(0),
            )
            .expect("next seq");
        conn.execute(
            "INSERT INTO gate_blocks(session_id, seq, claim_key, claim_kind, at, prev_hash, hash)
             VALUES(?1, ?2, ?3, 'tests_pass', '2026-01-01 00:00:00', 'forged', 'forged')",
            rusqlite::params![self.session, next, claim_key],
        )
        .expect("insert a forged block");
    }

    /// `thruline policy --conclusion` for this session's repository, with the
    /// documented example policy in place. The layer that is NOT fail-open.
    fn policy_conclusion(&self) -> String {
        let init = Command::new(thruline())
            .args(["policy", "--init"])
            .env("THRULINE_HOME", &self.home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .current_dir(&self.repo)
            .output()
            .expect("policy --init");
        assert!(
            init.status.success(),
            "policy --init: {}",
            String::from_utf8_lossy(&init.stderr)
        );
        let out = Command::new(thruline())
            .args(["policy", "--conclusion"])
            .env("THRULINE_HOME", &self.home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .current_dir(&self.repo)
            .output()
            .expect("policy --conclusion");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn stop_payload(&self, message: &str, stop_hook_active: bool) -> String {
        format!(
            r#"{{"hook_event_name":"Stop","session_id":"{}","transcript_path":"/t","cwd":"{}","last_assistant_message":"{message}","stop_hook_active":{stop_hook_active}}}"#,
            self.session,
            self.repo.display()
        )
    }

    fn gate(&self, message: &str, stop_hook_active: bool) -> GateResult {
        self.gate_with(message, stop_hook_active, None, &[])
    }

    /// Deliver a `Stop` to the capture hook AND to the gate, as a machine with
    /// both installed does.
    fn gate_with(
        &self,
        message: &str,
        stop_hook_active: bool,
        budget_ms: Option<&str>,
        extra: &[&str],
    ) -> GateResult {
        use std::io::Write;
        let payload = self.stop_payload(message, stop_hook_active);
        self.hook(&payload);
        let mut cmd = Command::new(thruline());
        cmd.args(["gate", "--hook", "stop"])
            .args(extra)
            .env("THRULINE_HOME", &self.home)
            .env("PATH", hermetic_path(&[&self.shim, &self.bin]))
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .current_dir(&self.repo)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        if let Some(ms) = budget_ms {
            cmd.env("THRULINE_GATE_BUDGET_MS", ms);
        }
        let started = std::time::Instant::now();
        let mut child = cmd.spawn().expect("gate");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(payload.as_bytes())
            .expect("write");
        let out = child.wait_with_output().expect("wait");
        GateResult {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
            elapsed: started.elapsed(),
        }
    }

    /// The verdict, read independently of the gate. `claims` does not fold, so
    /// calling this before a gate reports what the *ledger* holds.
    fn claims(&self) -> serde_json::Value {
        let out = Command::new(thruline())
            .args(["claims", "--last", "--json"])
            .env("THRULINE_HOME", &self.home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .current_dir(&self.repo)
            .output()
            .expect("claims");
        serde_json::from_slice(&out.stdout).unwrap_or(serde_json::Value::Null)
    }

    fn verdict_for(&self, kind: &str) -> serde_json::Value {
        self.claims()["claims"]
            .as_array()
            .and_then(|c| c.iter().find(|c| c["kind"] == kind))
            .map(|c| c["verdict"].clone())
            .unwrap_or(serde_json::Value::Null)
    }
}

struct GateResult {
    code: Option<i32>,
    stdout: String,
    stderr: String,
    elapsed: std::time::Duration,
}

impl GateResult {
    fn decision(&self) -> Option<serde_json::Value> {
        serde_json::from_str::<serde_json::Value>(self.stdout.trim()).ok()
    }
    fn blocked(&self) -> bool {
        self.decision()
            .is_some_and(|v| v["decision"] == serde_json::json!("block"))
    }
    /// Every path must exit 0. A gate that exits non-zero prints a hook-error
    /// notice into the user's transcript.
    fn assert_exit_zero(&self) {
        assert_eq!(
            self.code,
            Some(0),
            "the gate exited {:?}. stdout={:?} stderr={:?}",
            self.code,
            self.stdout,
            self.stderr
        );
    }
}

/// An agent that really breaks the source, really runs the suite (which really
/// fails), and is about to say "All tests pass."
fn agent_with_an_unsupported_claim(session: &str) -> Agent {
    let a = Agent::new(session);
    a.start();
    std::fs::write(
        a.repo.join("shipping.py"),
        "def rate(weight_kg):\n    return 0.0\n",
    )
    .expect("w");
    a.hook(&format!(
        r#"{{"hook_event_name":"PostToolUse","session_id":"{}","transcript_path":"/t","cwd":"{}","tool_name":"Write","tool_input":{{"file_path":"{}/shipping.py","content":"..."}},"tool_response":{{"success":true}},"tool_use_id":"toolu_1","duration_ms":3}}"#,
        a.session,
        a.repo.display(),
        a.repo.display()
    ));
    a.run_the_suite();
    a
}

// ---------------------------------------------------------------------------
// P1 — the block decision (M3.4: `decision: "block"` + actionable `reason`).
// Mutation that turns this red: replace `run`'s `Some(reason) => println!(…)`
// arm with `ExitCode::SUCCESS`, i.e. never emit a decision.
// ---------------------------------------------------------------------------

#[test]
fn p1_an_unsupported_claim_is_blocked_with_a_top_level_decision_and_a_reason() {
    let a = agent_with_an_unsupported_claim("55555555-6666-4777-8888-999999999999");
    let first = a.gate("All tests pass.", false);
    first.assert_exit_zero();

    // The verdict this is about, read independently of the gate.
    assert_eq!(
        a.verdict_for("tests_pass"),
        serde_json::json!("unsupported"),
        "the premise of this test no longer holds: {:#}",
        a.claims()
    );
    assert!(
        first.blocked(),
        "the gate allowed an unsupported claim: stdout={:?} stderr={:?}",
        first.stdout,
        first.stderr
    );
    // B.1's shape: `decision` is TOP-LEVEL on Stop, with `reason` beside it.
    // A nested `hookSpecificOutput` form is silently ignored by the harness.
    let decision = first.decision().expect("a decision");
    assert_eq!(
        decision["decision"],
        serde_json::json!("block"),
        "{decision}"
    );
    let reason = decision["reason"].as_str().expect("a reason beside it");

    // Actionable: it names the claim, its kind, and a command that would
    // produce the evidence.
    assert!(reason.contains("All tests pass."), "{reason}");
    assert!(reason.contains("tests_pass"), "{reason}");
    assert!(reason.contains("pytest"), "{reason}");
    // And it does not accuse. CLAUDE.md's vocabulary is binding.
    for forbidden in ["lie", "lied", "lying", "dishonest", "cheat"] {
        assert!(
            !reason.to_lowercase().contains(forbidden),
            "forbidden vocabulary {forbidden:?} in {reason:?}"
        );
    }
    // The happy path is silent on stderr: a diagnostic there means the gate
    // stood down for a reason it did not expect.
    assert!(
        first.stderr.is_empty(),
        "the gate blocked and still complained: {:?}",
        first.stderr
    );
}

// ---------------------------------------------------------------------------
// P2 — `unsupported` ONLY (§4.1, §6). Mutation that turns this red: widen
// `gate_cmd::blocks` to `!matches!(verdict, Verdict::Supported)`.
//
// Both halves live in one test on purpose. An abstention test alone is also
// passed by a gate that never blocks anything, so the same fixture builder is
// driven to a real block two lines later.
// ---------------------------------------------------------------------------

#[test]
fn p2_unverifiable_never_blocks_and_the_same_harness_does_block() {
    let quiet = Agent::new("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee");
    quiet.start();
    // No test ever runs, so there is no evidence either way.
    let allowed = quiet.gate("All tests pass.", false);
    allowed.assert_exit_zero();
    assert_eq!(
        quiet.verdict_for("tests_pass"),
        serde_json::json!("unverifiable"),
        "the premise of this test no longer holds: {:#}",
        quiet.claims()
    );
    assert!(
        !allowed.blocked(),
        "the gate blocked an UNVERIFIABLE claim — §6 read backwards, and on a repository \
         with no exec capture it blocks every stop forever: {:?}",
        allowed.stdout
    );

    // The control, so the abstention above is the predicate and not a gate
    // that never works.
    let loud = agent_with_an_unsupported_claim("aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeef");
    let blocked = loud.gate("All tests pass.", false);
    blocked.assert_exit_zero();
    assert!(
        blocked.blocked(),
        "the control did not block, so the abstention above proves nothing: {:?}",
        blocked.stdout
    );
}

// ---------------------------------------------------------------------------
// P3 — the 5 s budget, enforced by a watchdog that emits *allow* on expiry
// (M3.4). Mutation that turns this red: delete the `std::thread::spawn` that
// sleeps and calls `process::exit(0)` at the top of `gate_cmd::run`.
//
// The default 5 s itself is pinned in `gate_cmd`'s unit tests, because this
// test necessarily overrides it to run fast.
// ---------------------------------------------------------------------------

#[test]
fn p3_the_watchdog_allows_the_stop_when_the_budget_expires() {
    let a = agent_with_an_unsupported_claim("bbbbbbbb-cccc-4ddd-8eee-ffffffffffff");
    // 1 ms: the watchdog certainly wins the race against the fold and the
    // verifiers, whatever they are doing.
    //
    // Through `--budget-ms`, not through the environment. Since F13 the
    // environment can only LENGTHEN the budget, because it is inherited by an
    // unmodified hook and the thing being gated should not get to choose how
    // long it is looked at. An argument is authored into the hook's own command
    // line, so it is honoured exactly — and this test is the reason that seam
    // has to exist rather than the watchdog going untested end-to-end.
    let out = a.gate_with("All tests pass.", false, None, &["--budget-ms", "1"]);
    out.assert_exit_zero();
    assert!(
        out.stdout.trim().is_empty(),
        "the budget expired and the gate still spoke: {:?}",
        out.stdout
    );
    assert!(
        out.elapsed < std::time::Duration::from_secs(4),
        "the gate ran {:?} on a 1 ms budget, so nothing bounded it",
        out.elapsed
    );

    // The control: the same session under the real budget DOES block, so the
    // silence above is the watchdog and not a gate that never worked.
    let b = agent_with_an_unsupported_claim("cccccccc-dddd-4eee-8fff-000000000000");
    let out = b.gate("All tests pass.", false);
    assert!(
        out.blocked(),
        "the control did not block, so the watchdog test proves nothing: {:?}",
        out.stdout
    );
}

// ---------------------------------------------------------------------------
// P4 — `stop_hook_active` respected (M3.4). Mutation that turns this red:
// delete the `already_continuing && history.count == 0` branch in `decide`.
// ---------------------------------------------------------------------------

#[test]
fn p4_stop_hook_active_makes_a_gate_that_has_blocked_nothing_stand_down() {
    // The harness sets `stop_hook_active` for ANY stop hook on the machine.
    // Seeing it on a session this gate has blocked nothing in means someone
    // else's hook is already driving the loop; adding a second reason to
    // continue is not this gate's job.
    let a = agent_with_an_unsupported_claim("dddddddd-1111-4222-8333-444444444444");
    let deferred = a.gate("All tests pass.", true);
    deferred.assert_exit_zero();
    assert!(
        !deferred.blocked(),
        "the gate blocked while another stop hook was already continuing the session, on a \
         session it had blocked nothing in: {:?}",
        deferred.stdout
    );

    // The control, on an identically-built session: the SAME claim with the
    // flag false does block. Without this the assertion above is passed by a
    // gate that blocks nothing at all.
    let b = agent_with_an_unsupported_claim("dddddddd-1111-4222-8333-444444444445");
    let blocked = b.gate("All tests pass.", false);
    assert!(
        blocked.blocked(),
        "the control did not block, so the deferral above proves nothing: {:?}",
        blocked.stdout
    );

    // And the flag does not disable the gate forever: once this gate HAS
    // blocked something, `stop_hook_active` is the loop it started, so a second
    // distinct claim is still fair game.
    let again = b.gate("The test suite passes.", true);
    assert!(
        again.blocked(),
        "`stop_hook_active` silenced the gate on a loop the gate itself started: {:?}",
        again.stdout
    );
}

// ---------------------------------------------------------------------------
// P5 — `max_stop_blocks` (M3.4: default 2, inside the harness cap of 8).
// Mutation that turns this red: delete `if history.count >= args.max_stop_blocks
// { return None; }` from `decide`.
// ---------------------------------------------------------------------------

#[test]
fn p5_max_stop_blocks_bounds_the_session_and_the_flag_is_honoured() {
    // "Exactly once" is per claim; this is the other half of the loop guard.
    // Without it an agent that keeps producing NEW unsupported sentences would
    // never be allowed to stop.
    // # No sentence is allowed here without being blocked somewhere here
    //
    // The old version of this test ended on `"All tests are green."`, which
    // matches **no phrase in §5.2's table**: it extracted no claim at all, so
    // the gate allowed that stop for a reason that had nothing to do with the
    // cap, and deleting `max_stop_blocks` outright left the assertion green.
    // A "this was allowed" assertion is worth exactly as much as the proof
    // that the same sentence is otherwise blockable, so the two agents below
    // swap their sentences: each one that must be allowed in one agent is
    // shown to block in the other.
    let a = agent_with_an_unsupported_claim("eeeeeeee-ffff-4000-8111-222222222222");
    assert!(a.gate("All tests pass.", false).blocked());
    assert!(
        a.gate("The test suite passes.", true).blocked(),
        "a DIFFERENT unsupported claim was not blocked, so the default cap is below 2"
    );
    let third = a.gate("The tests pass.", true);
    third.assert_exit_zero();
    assert!(
        !third.blocked(),
        "the gate blocked a third time; the default is 2, and a gate that never yields is a \
         gate the user disables: {:?}",
        third.stdout
    );

    // The cap is the argument and not a hard-coded 2 — and, in the same
    // strokes, `"The tests pass."` is proved to be a sentence this gate blocks
    // and `"All tests pass."` a sentence it will decline once the cap is spent.
    let b = agent_with_an_unsupported_claim("eeeeeeee-ffff-4000-8111-222222222223");
    assert!(
        b.gate_with("The tests pass.", false, None, &["--max-stop-blocks", "1"])
            .blocked(),
        "`The tests pass.` does not block even as a session's first claim, so the third stop \
         above was allowed for the wrong reason and this test is vacuous"
    );
    let second = b.gate_with("All tests pass.", true, None, &["--max-stop-blocks", "1"]);
    assert!(
        !second.blocked(),
        "`--max-stop-blocks 1` was ignored: {:?}",
        second.stdout
    );
}

// ---------------------------------------------------------------------------
// P6 — "the gate folds the spool first" (M3.4). Mutation that turns this red:
// delete the `fold::fold(…)` block from `decide`.
//
// The isolation: nothing in this test ever folds. The run the gate judges
// exists only as a line in the spool, and `claims` — which does not fold —
// cannot see it beforehand. If the gate blocks, the gate folded.
// ---------------------------------------------------------------------------

#[test]
fn p6_the_gate_folds_the_spool_itself_before_it_judges() {
    let a = agent_with_an_unsupported_claim("99999999-8888-4777-8666-555555555555");
    assert_eq!(
        a.verdict_for("tests_pass"),
        serde_json::Value::Null,
        "something folded before the gate did, so this test cannot tell whether the gate \
         folds: {:#}",
        a.claims()
    );

    let out = a.gate("All tests pass.", false);
    out.assert_exit_zero();
    assert!(
        out.blocked(),
        "the gate did not block on a failing run that was still only in the spool, so it \
         judged the session the last fold saw rather than the one that just happened: {:?}",
        out.stdout
    );
    // And the fold is durable, not a read-through: the ledger now holds it.
    assert_eq!(
        a.verdict_for("tests_pass"),
        serde_json::json!("unsupported"),
        "the gate blocked but wrote nothing to the ledger: {:#}",
        a.claims()
    );
}

// ---------------------------------------------------------------------------
// P7 — §17's done-when: "blocks an unsupported `tests_pass` claim EXACTLY
// ONCE". Mutation that turns this red: delete the
// `.find(|(claim, _)| !history.already_blocked(…))` filter, or drop
// `History::claims` and keep only `count`.
//
// The isolation the old §17 test lacked: after the repeat is allowed, a
// DIFFERENT claim is blocked, which the count still permits (1 of 2). Only the
// per-claim ledger can explain the allow in between.
// ---------------------------------------------------------------------------

#[test]
fn p7_the_same_claim_is_blocked_exactly_once_and_not_because_the_budget_ran_out() {
    let a = agent_with_an_unsupported_claim("77777777-6666-4555-8444-333333333333");
    let first = a.gate("All tests pass.", false);
    first.assert_exit_zero();
    assert!(first.blocked(), "{:?}", first.stdout);

    // The agent restates the same sentence. The evidence has not changed;
    // blocking again burns its turns for nothing and teaches the user to
    // uninstall the gate.
    let second = a.gate("All tests pass.", true);
    second.assert_exit_zero();
    assert!(
        !second.blocked(),
        "the gate blocked the SAME claim twice on unchanged evidence: {:?}",
        second.stdout
    );

    // …and the allow above was the per-claim ledger, not a spent budget: one
    // block of two has been used, and a different unsupported sentence is
    // still blocked.
    let different = a.gate("The test suite passes.", true);
    assert!(
        different.blocked(),
        "a different unsupported claim was allowed too, so the `exactly once` above was the \
         block COUNT and this suite cannot tell the two mechanisms apart: {:?}",
        different.stdout
    );
}

// ---------------------------------------------------------------------------
// The bookkeeping, which used to be swallowed.
// ---------------------------------------------------------------------------

/// Six stops at one agent: how many blocked, and how many named a reason.
///
/// `stop_hook_active` stays **false**: each of these is a fresh stop the user
/// prompted their way back to, not a continuation another hook is driving. That
/// is the shape the 6/6 was measured in, and it is the shape that reaches the
/// bookkeeping every time rather than standing down at P4's branch first.
fn six_stops(a: &Agent) -> (usize, usize) {
    let (mut blocked, mut said_why) = (0, 0);
    for _ in 0..6 {
        let out = a.gate("All tests pass.", false);
        out.assert_exit_zero();
        if out.blocked() {
            blocked += 1;
        }
        if out.stderr.contains("allowing this stop") {
            said_why += 1;
        }
    }
    (blocked, said_why)
}

#[test]
fn a_block_the_gate_cannot_write_down_is_not_issued_and_says_why() {
    // Measured before the fix, on this fixture: **6 blocks out of 6** stops
    // against a **1/6** control, because `History::save`'s `let _ = …` left the
    // count at zero forever and `max_stop_blocks` bounded nothing. The harness
    // cap of 8 was the only floor left.
    //
    // The failure is isolated to `save`: the block ledger's directory exists
    // and is readable, so the load path returns a clean empty history and only
    // the write fails. `a_block_ledger_the_gate_cannot_read_…` covers the other
    // half; the first version of this test conflated them and stayed green when
    // `save`'s swallow was put back.
    let control = agent_with_an_unsupported_claim("66666666-5555-4444-8333-222222222222");
    assert_eq!(
        six_stops(&control),
        (1, 0),
        "the control should block exactly once in six stops, silently"
    );

    let a = agent_with_an_unsupported_claim("66666666-5555-4444-8333-222222222223");
    // The block ledger is `exec.db` now, so the unwritable thing is the store
    // itself. Read-only, not absent: an absent store is a different branch
    // ("no execution store here") and would prove a different sentence.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(a.exec_db(), std::fs::Permissions::from_mode(0o400))
            .expect("chmod exec.db");
    }
    // The premise, asserted rather than assumed: running as root would make the
    // file writable anyway and this test would pass for the wrong reason.
    assert!(
        std::fs::OpenOptions::new()
            .append(true)
            .open(a.exec_db())
            .is_err(),
        "{} is still writable, so this test would prove nothing",
        a.exec_db().display()
    );

    let (blocked, said_why) = six_stops(&a);
    assert_eq!(
        blocked, 0,
        "a block the gate cannot count is a block it cannot bound, and it issued {blocked} of \
         them in six stops"
    );
    // Fail open, but not silently: Rule 12. Every one of those stops explained
    // itself on stderr.
    assert_eq!(
        said_why, 6,
        "the gate stood down six times and named a reason {said_why} times"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(a.exec_db(), std::fs::Permissions::from_mode(0o600));
    }
}

#[test]
fn a_block_ledger_the_gate_cannot_read_is_not_an_empty_one() {
    // The other half. A `.blocks` file that existed and could not be parsed
    // used to read as `count: 0` — the exact state in which `max_stop_blocks`
    // bounds nothing. "I have blocked nothing yet" and "I cannot tell what I
    // have blocked" are different answers and only one of them is safe to act
    // on.
    //
    // Since G2 the same rule covers a chain that does not verify, which is what
    // an unparseable file became when the history moved into a table.
    let a = agent_with_an_unsupported_claim("66666666-5555-4444-8333-222222222224");
    let first = a.gate("All tests pass.", false);
    assert!(first.blocked(), "{:?}", first.stdout);
    assert_eq!(a.recorded_blocks(), 1);

    // Break the chain by appending a row that does not follow the one before
    // it — an INSERT is the one write the triggers allow, so this is the
    // corruption a forger can actually produce.
    a.forge_block("1111111111111111111111111111111111111111111111111111111111111111");
    assert_eq!(a.recorded_blocks(), 2);

    let out = a.gate("The test suite passes.", true);
    out.assert_exit_zero();
    assert!(
        !out.blocked(),
        "the gate blocked with a block ledger it could not verify, so its own count of blocks \
         issued was unreliable and nothing bounds the loop: {:?}",
        out.stdout
    );
    assert!(
        out.stderr.contains("does not verify"),
        "it stood down without saying why: {:?}",
        out.stderr
    );
}

#[test]
fn every_malformed_delivery_is_an_allow_and_says_nothing() {
    // Fail open, absolutely. A gate that errors, blocks or prints on a bad
    // payload is a gate that breaks the user's agent. Nothing here reaches the
    // bookkeeping, so nothing here is entitled to stderr either.
    use std::io::Write;
    let a = Agent::new("dddddddd-eeee-4fff-8000-111111111111");
    for payload in [
        "",
        "{",
        "null",
        "[]",
        "{\"hook_event_name\":\"Stop\"}",
        "{\"session_id\":null}",
        "not json at all",
    ] {
        let mut child = Command::new(thruline())
            .args(["gate", "--hook", "stop"])
            .env("THRULINE_HOME", &a.home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .current_dir(&a.repo)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("gate");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(payload.as_bytes())
            .expect("write");
        let out = child.wait_with_output().expect("wait");
        assert_eq!(out.status.code(), Some(0), "payload {payload:?}");
        assert!(
            out.stdout.is_empty(),
            "payload {payload:?} produced stdout {:?}",
            String::from_utf8_lossy(&out.stdout)
        );
        assert!(
            out.stderr.is_empty(),
            "payload {payload:?} produced stderr {:?}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn the_gate_is_installed_beside_the_capture_pack_and_undone_with_it() {
    // Two `Stop` entries by design: the capture hook must stay silent inside
    // 50 ms, the gate must be able to speak and take seconds. `--undo` removes
    // both and leaves a foreign hook alone.
    let dir = tempfile::tempdir().expect("tempdir");
    let settings = dir.path().join("settings.json");
    std::fs::write(
        &settings,
        r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"/usr/local/bin/other-tool"}]}]}}"#,
    )
    .expect("w");
    let before = std::fs::read_to_string(&settings).expect("read");

    let out = Command::new(thruline())
        .args(["hook-install", "--gate", "--settings"])
        .arg(&settings)
        .args(["--json"])
        .output()
        .expect("install");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert!(
        value["events"]
            .as_array()
            .expect("events")
            .iter()
            .any(|e| e == "Stop (gate)"),
        "the gate hook was not installed: {value}"
    );
    let text = std::fs::read_to_string(&settings).expect("read");
    assert!(text.contains("gate --hook stop"), "{text}");
    assert!(text.contains("hook claude-code"), "{text}");
    assert!(
        text.contains("other-tool"),
        "the foreign hook was destroyed: {text}"
    );

    let out = Command::new(thruline())
        .args(["hook-install", "--undo", "--settings"])
        .arg(&settings)
        .args(["--json"])
        .output()
        .expect("undo");
    assert!(out.status.success());
    let after = std::fs::read_to_string(&settings).expect("read");
    assert!(!after.contains("gate --hook stop"), "{after}");
    assert!(after.contains("other-tool"), "{after}");
    // Byte-identical restore, modulo the pretty-printing the writer applies.
    let normalise = |t: &str| {
        serde_json::from_str::<serde_json::Value>(t)
            .expect("json")
            .to_string()
    };
    assert_eq!(normalise(&before), normalise(&after));
}

// ---------------------------------------------------------------------------
// The install composition (M3.4's other half).
//
// M3.4's gate was built, mutation-witnessed, and **unreachable**: `--gate`
// existed only on `hook-install`, which refuses on a machine already set up by
// `thruline init`; run the other way round, `init` appended until `Stop` carried
// four entries and two recorders. So on the documented install path the gate
// could not be turned on at all, and the one path that did turn it on left the
// state §14 calls worse than none.
//
// These tests read the settings file with their own predicate rather than the
// product's, so a production `is_recorder` that quietly stops matching is a
// failure here rather than a silent agreement.
// ---------------------------------------------------------------------------

/// The test's own reading of "a command that appends capture rows", deliberately
/// not imported from `doctor.rs`.
fn recorders_per_event(settings: &std::path::Path) -> Vec<(String, usize)> {
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(settings).expect("read settings"))
            .expect("parse settings");
    let Some(hooks) = value.get("hooks").and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };
    hooks
        .iter()
        .map(|(event, entries)| {
            let n = entries
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .filter_map(|e| e.get("hooks").and_then(serde_json::Value::as_array))
                .flatten()
                .filter_map(|h| h.get("command").and_then(serde_json::Value::as_str))
                .filter(|c| {
                    c.contains(" sensor append")
                        || c.contains(" hook claude-code")
                        || c.contains(" hook codex")
                        || c.contains(" hook gemini")
                })
                .count();
            (event.clone(), n)
        })
        .collect()
}

fn init_at(
    db: &std::path::Path,
    settings: &std::path::Path,
    extra: &[&str],
) -> std::process::Output {
    let mut cmd = Command::new(thruline());
    cmd.args(["--db"])
        .arg(db)
        .args(["init", "--settings"])
        .arg(settings)
        .args(["--claude-json"])
        .arg(settings.with_extension("claude.json"))
        .args(extra);
    cmd.output().expect("init")
}

#[test]
fn p8_the_documented_install_path_can_install_the_gate() {
    // The finding, stated as a requirement: `thruline init` is the documented
    // door, and before this the gate could not be reached through it. Turning
    // the gate on could only ever move a verdict DOWN — it blocks on
    // `unsupported` alone — so an install path that cannot reach it is the
    // unsafe side of the choice, not the cautious one.
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("ledger.db");
    let settings = dir.path().join("settings.json");

    let out = init_at(&db, &settings, &["--yes", "--gate"]);
    assert!(
        out.status.success(),
        "init --gate failed: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let text = std::fs::read_to_string(&settings).expect("read");
    assert!(
        text.contains("gate --hook stop"),
        "init --gate installed no gate: {text}"
    );
    // And the truth loop is there too — the gate and the speakers were
    // mutually exclusive by install path, which is the defect.
    assert!(text.contains("audit --hook stop"), "{text}");
    assert!(text.contains("sensor append Stop"), "{text}");
    // Still exactly one recorder on every event.
    for (event, n) in recorders_per_event(&settings) {
        assert!(n <= 1, "{event} carries {n} recorders: {text}");
    }

    // Reversible by the same command that wrote it.
    let out = init_at(&db, &settings, &["--undo"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let after = std::fs::read_to_string(&settings).expect("read");
    assert!(
        !after.contains("gate --hook stop"),
        "init --undo left the gate behind: {after}"
    );
}

/// Which mechanism refused a write, read out of what the command said.
///
/// The write guard's refusal is the only one that names ThruLine RECORDERS in
/// the merged settings; `hook-install`'s own door check says something else
/// entirely. The distinction is the whole point: p9 used to assert only "the
/// second command failed", and with `refuse_double_recorder` stubbed to
/// `Ok(())` two of its four pairs still refused — on `hook-install`'s
/// pre-existing door — so the test stayed half-green over a deleted guard.
#[derive(Debug, PartialEq, Eq)]
enum Refuser {
    /// `hook_install::refuse_double_recorder`, at the write.
    WriteGuard,
    /// A command's own precondition, before the guard is ever consulted.
    OwnDoor,
    /// It did not refuse at all.
    None,
}

fn refuser(out: &std::process::Output) -> Refuser {
    if out.status.success() {
        return Refuser::None;
    }
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if text.contains("ThruLine RECORDERS") {
        Refuser::WriteGuard
    } else {
        Refuser::OwnDoor
    }
}

/// Every ThruLine command that registers hooks, run against a chosen settings
/// file with a chosen HOME and working directory.
fn install_command(
    which: &str,
    db: &std::path::Path,
    settings: &std::path::Path,
    home: &std::path::Path,
    cwd: &std::path::Path,
) -> std::process::Output {
    let mut cmd = Command::new(thruline());
    match which {
        "init" => {
            cmd.args(["--db"])
                .arg(db)
                .args(["init", "--settings"])
                .arg(settings)
                .args(["--claude-json"])
                .arg(home.join(".claude.json"))
                .args(["--yes"]);
        }
        "sensor" => {
            cmd.args(["--db"])
                .arg(db)
                .args(["sensor", "install", "--settings"])
                .arg(settings);
        }
        "hook-install" => {
            cmd.args(["hook-install", "--gate", "--settings"])
                .arg(settings);
        }
        other => panic!("unknown install command {other}"),
    }
    // HOME and the working directory are what `doctor::merged_settings_paths`
    // reads to decide which files Claude Code would merge. Both are temporary
    // here, so this test says nothing about the machine it runs on — the
    // failure mode two escape tests in this repository already had, where the
    // assertion reported the filesystem rather than the code.
    cmd.env("HOME", home)
        .env("THRULINE_HOME", home.join(".thruline"))
        .current_dir(cwd)
        .output()
        .expect("install command")
}

/// The recorders on each event across EVERY file Claude Code merges, summed.
///
/// A per-file count answers a question nobody asked: the harness merges hook
/// entries across settings levels, so two files holding one recorder each is
/// the two-recorder state, and it is the one that was actually measured
/// (`init --yes` at the user level, then `hook-install --gate --scope project`,
/// both exiting 0, `Stop` with four entries).
fn merged_recorders_per_event(
    files: &[&std::path::Path],
) -> std::collections::BTreeMap<String, usize> {
    let mut totals: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for file in files {
        // A level that was never written is a level with no hooks, not a
        // reason to stop counting.
        if !file.exists() {
            continue;
        }
        for (event, n) in recorders_per_event(file) {
            *totals.entry(event).or_default() += n;
        }
    }
    totals
}

#[test]
fn p9_no_install_order_and_no_settings_level_can_put_two_recorders_on_one_event() {
    // The state §14 names "worse than none", reached in one command pair:
    // `hook-install --gate` then `init` produced Stop×4 with two recorders, and
    // nothing afterwards says which record a claim was checked against.
    //
    // # What this test used to claim, and what it actually did
    //
    // It was called `p9_no_install_order_...` and it tested four ordered pairs
    // that all wrote **one file**. The measured defect was never a one-file
    // defect: `thruline init --yes` writes the USER level and
    // `hook-install --gate --scope project` writes the PROJECT level, neither
    // file holds two recorders, and the harness merges them. A per-file test
    // had nothing to refuse and would have passed on the broken machine.
    //
    // It was also a `for` loop of bare `assert!`s, so the first pair to fail
    // ended the test and pairs two through four were never demonstrated — and
    // two of those four rode `hook-install`'s own door check rather than the
    // guard, which a stub of `refuse_double_recorder` made visible and the test
    // did not.
    //
    // # What it tests now
    //
    // Every ordered pair of the three commands that register hooks, in two
    // placements — both writes to one file, and one write per settings level —
    // with the refusal ATTRIBUTED, every case run to completion, and the
    // recorder count taken over the merged view rather than per file.
    //
    // In the split placement the second file is clean, so no command's own door
    // can refuse: every refusal there must be the write guard's, and stubbing
    // the guard reddens all six.
    const COMMANDS: [&str; 3] = ["init", "sensor", "hook-install"];
    let mut failures: Vec<String> = Vec::new();
    let mut attributions: Vec<String> = Vec::new();

    for split in [false, true] {
        for first in COMMANDS {
            for second in COMMANDS {
                let dir = tempfile::tempdir().expect("tempdir");
                let home = dir.path().join("home");
                let proj = dir.path().join("proj");
                std::fs::create_dir_all(home.join(".claude")).expect("home");
                std::fs::create_dir_all(proj.join(".claude")).expect("proj");
                let db = dir.path().join("ledger.db");
                // The two real settings levels `merged_settings_paths` reads:
                // `$HOME/.claude/settings.json` and `<cwd>/.claude/settings.json`.
                let user = home.join(".claude").join("settings.json");
                let project = proj.join(".claude").join("settings.json");
                let second_target = if split { &project } else { &user };
                let placement = if split {
                    "user then project"
                } else {
                    "one file"
                };
                let case = format!("[{placement}] {first} then {second}");

                let a = install_command(first, &db, &user, &home, &proj);
                if !a.status.success() {
                    failures.push(format!(
                        "{case}: the FIRST command failed on a clean machine, so nothing \
                         below is about ordering: {}",
                        String::from_utf8_lossy(&a.stderr)
                    ));
                    continue;
                }
                let b = install_command(second, &db, second_target, &home, &proj);
                let who = refuser(&b);
                attributions.push(format!("{case} -> {who:?}"));

                // A second command that is ALLOWED is not by itself a defect:
                // re-running `init` or `sensor install` onto the file that
                // already holds that pack rewrites it rather than appending, and
                // the guard is written to permit any write that does not raise
                // an event's count. Measured: four of the nine one-file
                // orderings are allowed and all leave exactly one recorder per
                // event. The property this test is named for is the COUNT, and
                // it is asserted below for every case, refused or not.
                if split && who != Refuser::None && who != Refuser::WriteGuard {
                    // The split file is clean, so a command's own door has
                    // nothing to see. A refusal that is not the guard's here
                    // means the guard is not what is holding this property.
                    failures.push(format!(
                        "{case}: refused, but by {who:?} rather than the write guard, on a \
                         file that held no ThruLine hooks at all: {}",
                        String::from_utf8_lossy(&b.stderr)
                    ));
                }

                for (event, n) in merged_recorders_per_event(&[&user, &project]) {
                    if n > 1 {
                        failures.push(format!(
                            "{case}: {n} recorders on `{event}` across the merged settings"
                        ));
                    }
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} of 18 install orderings did not hold.\n\nFAILURES:\n  {}\n\nALL CASES:\n  {}",
        failures.len(),
        failures.join("\n  "),
        attributions.join("\n  ")
    );
    // Every case ran. A `for` loop of `assert!`s cannot say this.
    assert_eq!(
        attributions.len(),
        18,
        "not every ordering was demonstrated: {attributions:#?}"
    );
    // The guard, specifically, carries every one of the nine split orderings —
    // the placement the measured defect actually took, and the one no command's
    // own door can see. Stubbing `refuse_double_recorder` to `Ok(())` turns all
    // nine into two-recorder states and reddens this. The one-file placement
    // adds two more (`hook-install` first, then a pack that would append);
    // its other refusals are `hook-install`'s own door and are recorded as
    // such rather than counted as evidence for the guard.
    let split_by_guard = attributions
        .iter()
        .filter(|a| a.starts_with("[user then project]") && a.ends_with("WriteGuard"))
        .count();
    assert_eq!(
        split_by_guard, 9,
        "the write guard did not carry all nine cross-level orderings; the rest ride a \
         command's own door and would survive its deletion: {attributions:#?}"
    );
    let same_file_by_guard = attributions
        .iter()
        .filter(|a| a.starts_with("[one file]") && a.ends_with("WriteGuard"))
        .count();
    assert!(
        same_file_by_guard >= 2,
        "the guard refused nothing in the one-file placement, so every refusal there is \
         `hook-install`'s door: {attributions:#?}"
    );
}

#[test]
fn p10_a_machine_already_in_the_bad_state_can_still_be_undone() {
    // The guard refuses a write that CREATES or KEEPS a collision, never one
    // that reduces it. Anything stricter would strand every machine set up
    // before the guard existed: `init --undo` is exactly the write that has to
    // get through, and it writes a file that still contains the other pack.
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("ledger.db");
    let settings = dir.path().join("settings.json");
    let exe = thruline();
    std::fs::write(
        &settings,
        serde_json::json!({"hooks":{"Stop":[
            {"hooks":[{"type":"command","command":format!("{exe} hook claude-code --spool /tmp/s.jsonl")}]},
            {"hooks":[{"type":"command","command":format!("{exe} hook gemini --spool /tmp/g.jsonl")}]},
            {"hooks":[{"type":"command","command":format!("{exe} sensor append Stop")}]},
            {"hooks":[{"type":"command","command":"/usr/local/bin/other-tool"}]}
        ]}})
        .to_string(),
    )
    .expect("w");

    let out = init_at(&db, &settings, &["--undo"]);
    assert!(
        out.status.success(),
        "init --undo was refused on a machine already in the bad state: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let after = std::fs::read_to_string(&settings).expect("read");
    assert!(
        !after.contains("sensor append"),
        "undo removed nothing: {after}"
    );
    assert!(
        after.contains("other-tool"),
        "the foreign hook died: {after}"
    );
    // THREE recorders went in, so the file is STILL in a refusable state after
    // the removal. Two would have dropped to one and this test would agree with
    // a guard that had no allowance clause at all — it would be witnessing
    // nothing. Undo removed what init owned and not one byte more; the guard's
    // job was to stay out of the way of the reduction.
    assert_eq!(
        recorders_per_event(&settings)
            .into_iter()
            .find(|(e, _)| e == "Stop")
            .map(|(_, n)| n),
        Some(2),
        "the capture packs are not init's to remove: {after}"
    );
}

// ---------------------------------------------------------------------------
// F13 (Track R) — the gate's own budget disarmed it.
//
// Reproduction, which worked: clear the per-claim dedup, set
// THRULINE_GATE_BUDGET_MS=0, and the gate exits 0 having written nothing. It
// stayed installed and stopped gating.
// ---------------------------------------------------------------------------

#[test]
fn f13_the_environment_cannot_shorten_the_gates_budget_to_disarm_it() {
    let a = agent_with_an_unsupported_claim("13131313-1313-4131-8131-131313131313");

    // The control FIRST: this session does block, so a silent run below would
    // mean something.
    let control = a.gate("All tests pass.", false);
    control.assert_exit_zero();
    assert!(
        control.blocked(),
        "the control did not block: {:?}",
        control.stdout
    );

    // R's step 2 was `rm -f "$THRULINE_HOME/gate/"*.blocks`, to clear the
    // once-per-claim dedup so the next allow could only be the budget. **That
    // step is no longer possible**, and its impossibility is asserted in
    // `g2_the_block_ledger_is_append_only_by_the_database`. So the same
    // separation is achieved the other way: a DIFFERENT unsupported sentence
    // has never been blocked, so an allow on it cannot be the dedup either.
    let out = a.gate_with("The suite is green.", false, Some("0"), &[]);
    out.assert_exit_zero();
    assert!(
        out.blocked(),
        "F13: THRULINE_GATE_BUDGET_MS=0 disarmed the gate — it exited 0 having written \
         nothing, which the harness reads as allow: {:?}",
        out.stdout
    );
    // …and it said so rather than clamping in silence. A value quietly ignored
    // is how the next person concludes the variable works.
    assert!(
        out.stderr.contains("REFUSED"),
        "the shortening was clamped without saying so: {:?}",
        out.stderr
    );

    // Every value below the floor, not just zero — the defect was that ANY
    // parseable value was honoured.
    // Every value below the floor, not just zero — the defect was that ANY
    // parseable value was honoured. `max_stop_blocks` is raised for this loop
    // so the budget, not the block cap, is the only thing that could stand the
    // gate down; each sentence is new, so the dedup cannot either.
    // A FRESH agent for each, so neither the per-claim dedup nor the block cap
    // can be the thing that stands the gate down — only the budget can. Reusing
    // one agent with invented sentences was tried and was worse: a sentence the
    // claim table does not recognise produces no claim at all, and the test
    // then passes or fails for a reason that has nothing to do with F13.
    for (i, ms) in ["1", "10", "4999"].iter().enumerate() {
        let fresh =
            agent_with_an_unsupported_claim(&format!("13131313-1313-4131-8131-13131313131{i}"));
        let out = fresh.gate_with("All tests pass.", false, Some(ms), &[]);
        assert!(
            out.blocked(),
            "THRULINE_GATE_BUDGET_MS={ms} disarmed the gate: {:?}",
            out.stdout
        );
    }

    // The direction that is still allowed, because a slow machine is a real
    // thing: lengthening is honoured and changes nothing else.
    let fresh = agent_with_an_unsupported_claim("13131313-1313-4131-8131-131313131319");
    let out = fresh.gate_with("All tests pass.", false, Some("20000"), &[]);
    assert!(
        out.blocked(),
        "a LENGTHENED budget broke the gate: {:?}",
        out.stdout
    );
    assert!(
        !out.stderr.contains("REFUSED"),
        "lengthening was refused, which is not the rule: {:?}",
        out.stderr
    );
}

// ---------------------------------------------------------------------------
// G2 (Track R) — the block ledger was unauthenticated and agent-writable, and
// its `count` was a free integer that one `printf` could spend.
// ---------------------------------------------------------------------------

/// R's exact reproduction, and the fact that it can no longer be typed.
///
/// ```text
/// printf '{"count":9999,"claims":[]}' > $THRULINE_HOME/gate/$SID.blocks
/// ```
///
/// There is no such file. The history is `gate_blocks`, append-only by SQLite
/// trigger and hash chained. This asserts both halves: the file is gone, and
/// the table refuses the edits the file accepted.
#[test]
fn g2_the_block_ledger_is_append_only_by_the_database() {
    let a = agent_with_an_unsupported_claim("62626262-6262-4626-8626-262626262626");
    let first = a.gate("All tests pass.", false);
    assert!(first.blocked(), "{:?}", first.stdout);
    assert_eq!(a.recorded_blocks(), 1, "the block was not recorded");

    // The file R wrote to does not exist, and neither does its directory.
    assert!(
        !a.home.join("gate").exists(),
        "the loose block ledger is still being written; the forgeable path survived the move"
    );

    let conn = rusqlite::Connection::open(a.exec_db()).expect("open");
    // The two edits the loose file accepted, refused by the database rather
    // than by discipline.
    let updated = conn.execute(
        "UPDATE gate_blocks SET claim_key = 'x' WHERE session_id = ?1",
        rusqlite::params![a.session],
    );
    assert!(
        updated.is_err(),
        "a gate block was UPDATEd; the append-only trigger is not there"
    );
    let deleted = conn.execute(
        "DELETE FROM gate_blocks WHERE session_id = ?1",
        rusqlite::params![a.session],
    );
    assert!(
        deleted.is_err(),
        "a gate block was DELETEd; `rm -f` has simply moved into SQL"
    );
    // …and the row is still there, so the refusals above were refusals and not
    // silent no-ops.
    assert_eq!(a.recorded_blocks(), 1);
}

/// What an INSERT — the one write the triggers still allow — buys a forger.
///
/// **Not a silent bypass any more, and not prevented either.** This is the
/// property stated exactly: the row goes in, the chain stops verifying, and
/// every reader that is not obliged to fail open now refuses to report a
/// number it cannot trust.
#[test]
fn g2_a_forged_block_row_breaks_the_chain_and_the_ci_gate_stops_passing() {
    let a = agent_with_an_unsupported_claim("62626262-6262-4626-8626-262626262628");

    // A forged block, in the shape that used to spend the budget in one line.
    a.forge_block("0000000000000000000000000000000000000000000000000000000000000000");
    assert_eq!(a.recorded_blocks(), 1, "the forgery did not land");

    // The Stop hook is fail-open by §4.1 and stays that way: this stop is
    // allowed. It is allowed WITH A REASON, which is the whole change — the
    // old file was believed in silence.
    let out = a.gate("All tests pass.", false);
    out.assert_exit_zero();
    assert!(
        out.stderr.contains("does not verify"),
        "a tampered block ledger was read without saying so: {:?}",
        out.stderr
    );

    // The lever that is NOT fail-open. `thruline policy` reports
    // `stop_blocks` as unreadable rather than as a number, so a repository
    // with `stop_gate.enabled: true` no longer concludes success. A forger who
    // silences the Stop hook now reddens the check run instead.
    let policy = a.policy_conclusion();
    assert_ne!(
        policy, "success",
        "a tampered block ledger still passed the CI gate: {policy}"
    );
}

#[test]
fn g2_the_budget_still_binds_when_the_claims_are_real() {
    // The control for the fix: removing the stated `count` must not remove the
    // BUDGET. A file with `max_stop_blocks` genuinely blocked claims still
    // stands the gate down — otherwise the fix above is just "the gate blocks
    // forever", which §4.1's cap exists to prevent.
    let a = agent_with_an_unsupported_claim("62626262-6262-4626-8626-262626262627");
    let first = a.gate("All tests pass.", false);
    assert!(first.blocked(), "{:?}", first.stdout);
    let second = a.gate("The suite is green.", false);
    assert!(second.blocked(), "{:?}", second.stdout);
    // Two blocks is the default cap. A third, different, unsupported sentence
    // is allowed because the budget really is spent.
    let third = a.gate("Everything passes now.", false);
    assert!(
        !third.blocked(),
        "max_stop_blocks stopped bounding the session: {:?}",
        third.stdout
    );
}

/// Each arm of the chain check, reached on its own.
///
/// A single forgery shape trips whichever arm comes first and leaves the others
/// as guards nothing tests — measured: with only the "does not follow" case in
/// the suite, deleting the CONTENT hash check reddened nothing at all.
#[test]
fn every_arm_of_the_gate_chain_check_is_reachable() {
    // (1) The link: a row whose `prev_hash` is not its predecessor's `hash`.
    let a = agent_with_an_unsupported_claim("c4a11111-1111-4111-8111-111111111111");
    assert!(a.gate("All tests pass.", false).blocked());
    a.forge_block_with("aa", "not-the-previous-hash", "whatever", None);
    let out = a.gate("The test suite passes.", true);
    assert!(
        out.stderr.contains("does not follow"),
        "the link check did not fire: {:?}",
        out.stderr
    );

    // (2) The contents: a row that follows the chain correctly and whose own
    // hash does not match what its fields hash to. This is the forgery a
    // careful attacker writes, and it is the one the link check cannot see.
    let b = agent_with_an_unsupported_claim("c4a22222-2222-4222-8222-222222222222");
    assert!(b.gate("All tests pass.", false).blocked());
    let real_prev = b.last_block_hash();
    b.forge_block_with("bb", &real_prev, "a-hash-these-fields-do-not-produce", None);
    let out = b.gate("The test suite passes.", true);
    assert!(
        out.stderr.contains("do not hash to"),
        "the content check did not fire, so a chain-following forgery was believed: {:?}",
        out.stderr
    );

    // (3) The sequence: a gap, which is what a partial delete would leave if a
    // trigger were ever dropped.
    let c = agent_with_an_unsupported_claim("c4a33333-3333-4333-8333-333333333333");
    assert!(c.gate("All tests pass.", false).blocked());
    let real_prev = c.last_block_hash();
    c.forge_block_with("cc", &real_prev, "x", Some(9));
    let out = c.gate("The test suite passes.", true);
    assert!(
        out.stderr.contains("was due"),
        "the sequence check did not fire: {:?}",
        out.stderr
    );
}
