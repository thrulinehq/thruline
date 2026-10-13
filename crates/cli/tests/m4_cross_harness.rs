//! M4's done-when (BUILD-SPEC §17):
//!
//! > *the same session captured through each harness yields equivalent claims
//! > and verdicts*
//!
//! # The dishonest pass this file is mostly about
//!
//! "Equivalent" has an obvious way to be satisfied by blindness: **three
//! harnesses agree because all three are blind in the same place.** Equivalence
//! then means equally uninformative, and deleting a capture path from all three
//! adapters *passes* the done-when. So there are two arms.
//!
//! **Positive arm.** One scripted agent, three harnesses, same claims and same
//! verdicts. That is §17's sentence.
//!
//! **Negative arm, and it is the load-bearing one.** A session whose evidence
//! *should* differ per harness must actually differ. Claude Code fires
//! `PostToolUse` only on success and a separate `PostToolUseFailure`; Gemini
//! puts the failure in a field; Codex fires one event and its documented
//! payload states no outcome at all. So a session containing a **failing** tool
//! call cannot produce identical evidence across the three, and this file
//! asserts that it does not. Without this arm the positive arm is satisfiable
//! by a stub.
//!
//! **And a floor.** Each harness must contribute at least one event shape the
//! others do not, or the equivalence is being met by a lowest common
//! denominator that has quietly dropped everything harness-specific.
//!
//! # What is held constant
//!
//! The repository, the edits, the commands run through the shim, and the final
//! message. What varies is only the hook vocabulary — which is exactly the
//! thing the `capture` crate's harness boundary exists to absorb.

use std::collections::BTreeSet;
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

fn require_pytest() -> bool {
    if pytest_available() {
        return true;
    }
    if std::env::var_os("THRULINE_TEST_NO_PYTEST").is_some() {
        eprintln!("SKIPPED: pytest is not installed. M4's done-when did NOT run.");
        return false;
    }
    panic!("pytest is not on PATH; M4's done-when needs a real runner");
}

/// One scripted agent, run under one harness's hook vocabulary.
struct Harnessed {
    _dir: tempfile::TempDir,
    home: PathBuf,
    repo: PathBuf,
    shim: PathBuf,
    session: String,
    harness: &'static str,
}

impl Harnessed {
    fn new(harness: &'static str) -> Harnessed {
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
        for args in [vec!["add", "-A"], vec!["commit", "-qm", "baseline"]] {
            let out = Command::new("git")
                .current_dir(&repo)
                .args(&args)
                .output()
                .expect("git");
            assert!(out.status.success());
        }
        let shim = home.join("shim");
        // `--harness` puts the recorder's lines in the SAME spool as this
        // harness's hook deliveries. One session's lines must live in one
        // spool file: `seq` is a line's ordinal in its own spool and the fold
        // resumes at the session's chain tail, so two files numbering one
        // session make the second one folded look already present. Measured —
        // the Codex arm of this test lost its `session_start` and
        // `user_prompt` exactly that way before `--harness` existed.
        let out = Command::new(thruline())
            .args(["shim", "install", "--harness", harness])
            .env("THRULINE_HOME", &home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .current_dir(&repo)
            .output()
            .expect("shim install");
        assert!(out.status.success());
        Harnessed {
            _dir: dir,
            home,
            repo,
            shim,
            // One session id across all three, so the only variable is the
            // hook vocabulary.
            session: "cccccccc-dddd-4eee-8fff-000000000000".to_string(),
            harness,
        }
    }

    fn hook(&self, json: &str) {
        use std::io::Write;
        let mut child = Command::new(thruline())
            .args(["hook", self.harness])
            .env("THRULINE_HOME", &self.home)
            .current_dir(&self.repo)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("hook");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(json.as_bytes())
            .expect("write");
        let out = child.wait_with_output().expect("wait");
        // Every harness's contract says the same thing here, and Gemini's says
        // it hardest: "Silence is Mandatory".
        assert!(out.status.success(), "the hook must exit 0");
        assert!(out.stdout.is_empty(), "a capture hook prints nothing");
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
            "fold: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
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
                "claims --json ({}): {e}\nstdout: {}\nstderr: {}",
                self.harness,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        })
    }

    fn events(&self) -> Vec<serde_json::Value> {
        let ledger =
            thruline_ledger::Ledger::open(&self.home.join("exec.db")).expect("open ledger");
        ledger
            .session_events(&self.session)
            .expect("events")
            .iter()
            .map(|e| {
                serde_json::json!({
                    "kind": e.kind().as_str().as_str(),
                    "source": serde_json::to_value(e.source()).unwrap_or(serde_json::Value::Null),
                    "payload": e.payload(),
                })
            })
            .collect()
    }

    // --- the hook vocabulary, per harness --------------------------------

    fn session_start(&self) {
        let (event, extra) = match self.harness {
            "gemini" => (
                "SessionStart",
                r#","source":"startup","timestamp":"2026-09-02T10:00:00Z""#,
            ),
            _ => ("SessionStart", r#","source":"startup""#),
        };
        self.hook(&format!(
            r#"{{"hook_event_name":"{event}","session_id":"{}","transcript_path":"/t","cwd":"{}"{extra}}}"#,
            self.session,
            self.repo.display()
        ));
    }

    fn prompt(&self, text: &str) {
        let event = match self.harness {
            "gemini" => "BeforeAgent",
            _ => "UserPromptSubmit",
        };
        self.hook(&format!(
            r#"{{"hook_event_name":"{event}","session_id":"{}","transcript_path":"/t","cwd":"{}","prompt":"{text}"}}"#,
            self.session,
            self.repo.display()
        ));
    }

    /// A tool that changed a file, and succeeded.
    fn edited(&self, path: &str) {
        let json = match self.harness {
            "gemini" => format!(
                r#"{{"hook_event_name":"AfterTool","session_id":"{}","transcript_path":"/t","cwd":"{}","timestamp":"2026-09-02T10:00:01Z","tool_name":"Edit","tool_input":{{"file_path":"{path}"}},"tool_response":{{"llmContent":"ok","returnDisplay":"ok"}}}}"#,
                self.session,
                self.repo.display()
            ),
            _ => format!(
                r#"{{"hook_event_name":"PostToolUse","session_id":"{}","transcript_path":"/t","cwd":"{}","tool_name":"Edit","tool_input":{{"file_path":"{path}"}},"tool_response":{{"success":true}},"tool_use_id":"t_{path}"}}"#,
                self.session,
                self.repo.display()
            ),
        };
        self.hook(&json);
    }

    /// A tool that **failed**. This is the negative arm's whole payload: the
    /// three harnesses report it in three structurally different ways.
    fn tool_failed(&self, command: &str) {
        let json = match self.harness {
            "claude-code" => format!(
                r#"{{"hook_event_name":"PostToolUseFailure","session_id":"{}","transcript_path":"/t","cwd":"{}","tool_name":"Bash","tool_input":{{"command":"{command}"}},"error":"Exit code 1\nassert 1 == 2","is_interrupt":false,"tool_use_id":"t_fail"}}"#,
                self.session,
                self.repo.display()
            ),
            "codex" => format!(
                r#"{{"hook_event_name":"PostToolUse","session_id":"{}","transcript_path":"/t","cwd":"{}","turn_id":"turn1","tool_name":"Bash","tool_input":{{"command":"{command}"}},"tool_response":"Exit code 1\nassert 1 == 2","tool_use_id":"t_fail"}}"#,
                self.session,
                self.repo.display()
            ),
            _ => format!(
                r#"{{"hook_event_name":"AfterTool","session_id":"{}","transcript_path":"/t","cwd":"{}","timestamp":"2026-09-02T10:00:02Z","tool_name":"run_shell_command","tool_input":{{"command":"{command}"}},"tool_response":{{"llmContent":"assert 1 == 2","error":{{"message":"Command failed with exit code 1"}}}}}}"#,
                self.session,
                self.repo.display()
            ),
        };
        self.hook(&json);
    }

    fn stop(&self, message: &str) {
        let json = match self.harness {
            "gemini" => format!(
                r#"{{"hook_event_name":"AfterAgent","session_id":"{}","transcript_path":"/t","cwd":"{}","timestamp":"2026-09-02T10:00:03Z","prompt":"make it work","prompt_response":"{message}","stop_hook_active":false}}"#,
                self.session,
                self.repo.display()
            ),
            _ => format!(
                r#"{{"hook_event_name":"Stop","session_id":"{}","transcript_path":"/t","cwd":"{}","last_assistant_message":"{message}","stop_hook_active":false}}"#,
                self.session,
                self.repo.display()
            ),
        };
        self.hook(&json);
    }
}

/// The scripted agent. Identical under every harness.
fn run_the_agent(harness: &'static str, fail_a_tool: bool) -> Harnessed {
    let a = Harnessed::new(harness);
    a.session_start();
    a.prompt("add a handling fee");

    std::fs::write(
        a.repo.join("shipping.py"),
        "def rate(weight_kg):\n    return round(2.5 * weight_kg + 7.5, 2)\n",
    )
    .expect("edit");
    a.edited("shipping.py");
    std::fs::write(
        a.repo.join("test_shipping.py"),
        "from shipping import rate\n\n\
         def test_rate_small():\n    assert rate(2) == 12.5\n\n\
         def test_rate_large():\n    assert rate(10) == 32.5\n",
    )
    .expect("edit");
    a.edited("test_shipping.py");

    if fail_a_tool {
        a.tool_failed("pytest -q --maxfail=1");
    }

    // The evidence itself comes from the shim, which is harness-independent by
    // construction — that is §4.4's whole point, and it is why the verdicts can
    // agree while the hook records differ.
    let out = a.bash("pytest -q");
    assert!(
        out.status.success(),
        "the scripted run must pass: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    a.stop("All tests pass.");
    a.fold();
    a
}

fn claim_summary(json: &serde_json::Value) -> Vec<(String, String, String)> {
    json["claims"]
        .as_array()
        .expect("claims")
        .iter()
        .map(|c| {
            (
                c["kind"].as_str().unwrap_or("?").to_string(),
                c["text"].as_str().unwrap_or("?").to_string(),
                c["verdict"].as_str().unwrap_or("?").to_string(),
            )
        })
        .collect()
}

const HARNESSES: [&str; 3] = ["claude-code", "codex", "gemini"];

// ---------------------------------------------------------------------------
// The positive arm — §17's sentence
// ---------------------------------------------------------------------------

#[test]
fn the_same_session_through_each_harness_yields_the_same_claims_and_verdicts() {
    if !require_pytest() {
        return;
    }
    let runs: Vec<(&str, serde_json::Value)> = HARNESSES
        .iter()
        .map(|h| (*h, run_the_agent(h, false).claims_json()))
        .collect();

    let reference = claim_summary(&runs[0].1);
    assert!(
        !reference.is_empty(),
        "the reference run extracted no claims at all, so agreement below \
         would be agreement about nothing: {:#}",
        runs[0].1
    );
    assert!(
        reference.iter().any(|(kind, _, _)| kind == "tests_pass"),
        "the scripted agent said 'All tests pass.' and no tests_pass claim was \
         extracted: {reference:?}"
    );
    for (harness, json) in &runs[1..] {
        assert_eq!(
            claim_summary(json),
            reference,
            "{harness} disagreed with {} about what was claimed and proved:\n{json:#}",
            runs[0].0
        );
    }
}

// ---------------------------------------------------------------------------
// The negative arm — the one that makes the positive arm mean something
// ---------------------------------------------------------------------------

/// A **failing** tool call cannot be recorded identically by three harnesses
/// that report failure in three different ways, and this asserts it is not.
///
/// If this test ever passes by finding the three records identical, the
/// differential above is being satisfied by blindness rather than by capture:
/// three adapters that all dropped the failure would agree perfectly.
#[test]
fn a_failing_tool_call_is_not_recorded_identically_by_three_harnesses() {
    if !require_pytest() {
        return;
    }
    let mut shapes: Vec<(&str, serde_json::Value)> = Vec::new();
    for h in HARNESSES {
        let run = run_the_agent(h, true);
        let failures: Vec<serde_json::Value> = run
            .events()
            .into_iter()
            .filter(|e| e["kind"] == serde_json::json!("tool_result"))
            .filter(|e| {
                e["payload"]["tool"] == serde_json::json!("Bash")
                    || e["payload"]["tool"] == serde_json::json!("run_shell_command")
            })
            .map(|e| {
                serde_json::json!({
                    "ok": e["payload"]["ok"],
                    "ok_basis": e["payload"]["ok_basis"],
                })
            })
            .collect();
        assert_eq!(
            failures.len(),
            1,
            "{h} recorded {} tool results for the failing command, not 1",
            failures.len()
        );
        shapes.push((h, failures[0].clone()));
    }

    let distinct: BTreeSet<String> = shapes.iter().map(|(_, v)| v.to_string()).collect();
    assert!(
        distinct.len() > 1,
        "three harnesses that report failure in three different ways produced \
         ONE identical record: {shapes:?}. That is equivalence by blindness, \
         not by capture."
    );

    // And the readings are the specific ones the vendor contracts license.
    let by = |name: &str| -> serde_json::Value {
        shapes
            .iter()
            .find(|(h, _)| *h == name)
            .map(|(_, v)| v.clone())
            .expect("harness")
    };
    assert_eq!(
        by("claude-code"),
        serde_json::json!({ "ok": false, "ok_basis": "event_name" }),
        "Claude Code fires a distinct failure event, so the event IS the fact"
    );
    assert_eq!(
        by("gemini"),
        serde_json::json!({ "ok": false, "ok_basis": "tool_response.error" }),
        "Gemini puts the failure in a field"
    );
    assert_eq!(
        by("codex"),
        serde_json::json!({ "ok": true, "ok_basis": "not_stated" }),
        "Codex fires one event for both outcomes and its documented payload \
         names neither — an unstated outcome, recorded as unstated rather than \
         guessed in either direction"
    );
}

/// The floor: the harness stream is real, the shared evidence is shared, and
/// the three records are distinguishable **without looking at `source`**.
///
/// Equivalence met by a lowest common denominator — three adapters that kept
/// only what all three have, or that kept nothing and leaned on the shim —
/// passes the positive arm and fails here.
///
/// `source` is deliberately excluded from every comparison below. It is
/// stamped per harness, so including it would make every event trivially
/// unique and this test could not fail. That is the shape of a vacuous guard,
/// and this file exists because of one.
#[test]
fn the_equivalence_is_not_a_lowest_common_denominator() {
    if !require_pytest() {
        return;
    }
    let mut runs: Vec<(&str, Vec<serde_json::Value>)> = Vec::new();
    for h in HARNESSES {
        runs.push((h, run_the_agent(h, true).events()));
    }

    // (a) The hook stream exists at all. An adapter that dropped every hook
    //     delivery and leaned on the shim would still produce a session, and
    //     the three would still agree.
    for (h, events) in &runs {
        let hook_kinds: BTreeSet<&str> = events
            .iter()
            .filter(|e| e["source"].as_str().is_some_and(|s| s.ends_with("-hook")))
            .filter_map(|e| e["kind"].as_str())
            .collect();
        for required in ["session_start", "user_prompt", "tool_result", "stop"] {
            assert!(
                hook_kinds.contains(required),
                "{h} recorded no {required} from its hook stream: {hook_kinds:?}"
            );
        }
    }

    // (b) The harness-independent evidence really is shared. §4.4: the exec
    //     record is the source of truth, and it is why the verdicts can agree
    //     while the hook records differ.
    for (h, events) in &runs {
        assert!(
            events
                .iter()
                .any(|e| e["kind"] == serde_json::json!("exec")),
            "{h} produced no exec record, so its verdict rests on hooks alone"
        );
    }

    // (c) Three distinguishing facts, one per harness, each read off a vendor
    //     contract rather than invented. Compared as payload content, with
    //     `source` excluded.
    let fact = |h: &str, pred: &dyn Fn(&serde_json::Value) -> bool| -> bool {
        runs.iter()
            .find(|(name, _)| *name == h)
            .expect("harness")
            .1
            .iter()
            .any(pred)
    };
    let basis = |want: &'static str| {
        move |e: &serde_json::Value| e["payload"]["ok_basis"] == serde_json::json!(want)
    };

    // Claude Code: a distinct failure event, so the event IS the outcome.
    assert!(fact("claude-code", &basis("event_name")));
    assert!(!fact("codex", &basis("event_name")));
    assert!(!fact("gemini", &basis("event_name")));

    // Gemini: the outcome is a field, and there is no `PostCompress`, so a
    // compaction boundary carries a capability gap the others never carry.
    assert!(fact("gemini", &basis("tool_response.error")));
    assert!(!fact("claude-code", &basis("tool_response.error")));
    assert!(!fact("codex", &basis("tool_response.error")));

    // Codex: one event for both outcomes, and its documented payload names
    // neither. Recorded as unstated on exactly one harness.
    assert!(fact("codex", &basis("not_stated")));
    assert!(!fact("claude-code", &basis("not_stated")));
    assert!(!fact("gemini", &basis("not_stated")));

    // And Gemini alone has no correlation id to carry, because its contract
    // has none — an absence that is a fact, not a capture failure.
    let has_call_id = |e: &serde_json::Value| {
        e["kind"] == serde_json::json!("tool_result")
            && e["payload"]["call_id"]
                .as_str()
                .is_some_and(|s| !s.is_empty())
    };
    assert!(fact("claude-code", &has_call_id));
    assert!(fact("codex", &has_call_id));
    assert!(
        !fact("gemini", &has_call_id),
        "Gemini's AfterTool carries no tool_use_id; a non-empty one here was \
         invented"
    );
}

/// One session's lines in two spools is a **defect the fold names**, not a
/// silence.
///
/// `seq` is a line's ordinal in its own spool and the fold resumes at the
/// session's chain tail, so the second spool folded hits the resume guard and
/// its deliveries are counted as already present: a real delivery vanishes and
/// the report calls it idempotence. That is verbatim the failure the seq stride
/// was introduced to fix, arriving through a second file rather than a second
/// writer — and it is how the Codex arm above silently lost its `session_start`
/// before `shim install --harness` existed.
#[test]
fn a_session_split_across_two_spools_is_reported_rather_than_skipped() {
    let a = Harnessed::new("codex");
    // Deliberately mismatched: the shim on Claude Code's spool, the hooks on
    // Codex's.
    let out = Command::new(thruline())
        .args(["shim", "install", "--session", &a.session])
        .env("THRULINE_HOME", &a.home)
        .current_dir(&a.repo)
        .output()
        .expect("shim install");
    assert!(out.status.success());
    a.session_start();
    // One recorded exec, which lands in the Claude Code spool.
    let path = format!(
        "{}:{}",
        a.shim.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let _ = Command::new("sh")
        .args(["-c", "make -v >/dev/null 2>&1 || true"])
        .current_dir(&a.repo)
        .env("THRULINE_HOME", &a.home)
        .env("PATH", path)
        .env("CLAUDE_CODE_SESSION_ID", &a.session)
        .output()
        .expect("bash");

    let out = Command::new(thruline())
        .args(["fold", "--json"])
        .env("THRULINE_HOME", &a.home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(&a.repo)
        .output()
        .expect("fold");
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).expect("fold json");
    assert_eq!(
        report["sessions_in_multiple_spools"],
        serde_json::json!([a.session]),
        "the split was not detected: {report:#}"
    );
    assert_eq!(report["ok"], serde_json::json!(false));
    assert_eq!(
        out.status.code(),
        Some(1),
        "a fold that could not record everything must exit non-zero"
    );

    // The control: the same session with the shim on the RIGHT spool is clean.
    let b = Harnessed::new("codex");
    let out = Command::new(thruline())
        .args([
            "shim",
            "install",
            "--harness",
            "codex",
            "--session",
            &b.session,
        ])
        .env("THRULINE_HOME", &b.home)
        .current_dir(&b.repo)
        .output()
        .expect("shim install");
    assert!(out.status.success());
    b.session_start();
    let out = Command::new(thruline())
        .args(["fold", "--json"])
        .env("THRULINE_HOME", &b.home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(&b.repo)
        .output()
        .expect("fold");
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).expect("fold json");
    assert_eq!(
        report["sessions_in_multiple_spools"],
        serde_json::json!([]),
        "{report:#}"
    );
}
