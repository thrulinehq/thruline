//! D18 / F7 — a session whose `Stop` never arrived must not produce a green
//! receipt.
//!
//! Filed twice, independently: Track Q as D18 (CRITICAL,
//! `~/thruline-qa/QA-LOG.md:73-102`, artefact `artifacts/receipt-truncated.json`)
//! and Track R as F7. Measured by Q with the `1a52d60` binary:
//!
//! ```text
//! $ thruline explain --last      → "no_stop_recorded — the Stop hook never fired"  (exit 0)
//! $ thruline receipt --out r.json                                                  (exit 0)
//! $ thruline verify-receipt r.json → "Verified."   all 10 ok, 0 verdicts            (exit 0)
//! $ thruline policy              → "policy … — success"                             (exit 0)
//! ```
//!
//! Q's sentence is the one that decides the priority: *"What makes it worse
//! than tampering: it needs no malice. A crashed agent, a killed terminal, or a
//! hook that exceeded its timeout produces exactly this state, and the product's
//! answer is a green check."*
//!
//! The rule, as ruled: **where the lifecycle cannot be shown complete,
//! degrade.** Every case below is an end-to-end run of the real binary against
//! a real ledger and a real git repository — the surfaces Q reported on — not
//! a unit test of the predicate.

// These existing cases pin v0.1 compatibility explicitly; session_v2 cases pin the new protocol.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

fn thruline() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

struct Session {
    _dir: tempfile::TempDir,
    home: PathBuf,
    repo: PathBuf,
    receipt: PathBuf,
}

/// The lifecycle hooks a case delivers, in order, after `SessionStart` and the
/// tool activity. `&[]` is the truncated session.
fn record(tail: &[&str]) -> Session {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).expect("repo");
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.email", "a@e.invalid"]);
    git(&repo, &["config", "user.name", "A"]);
    std::fs::write(repo.join("app.py"), "def rate(w):\n    return 2.5 * w\n").expect("write");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "first"]);

    let session = "aaaa-d18-truncated";
    let hook = |json: String| {
        let mut child = Command::new(thruline())
            .args(["hook", "claude-code"])
            .env("THRULINE_HOME", &home)
            .current_dir(&repo)
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
    };
    let cwd = repo.display().to_string();
    hook(format!(
        r#"{{"hook_event_name":"SessionStart","session_id":"{session}","transcript_path":"/t","cwd":"{cwd}","source":"startup"}}"#
    ));
    // The work. This is what makes the session non-empty — a session with no
    // evidence-bearing event has nothing for the record to be incomplete about
    // and is deliberately not flagged.
    hook(format!(
        r#"{{"hook_event_name":"PreToolUse","session_id":"{session}","transcript_path":"/t","cwd":"{cwd}","tool_name":"Edit","tool_input":{{"file_path":"app.py"}}}}"#
    ));
    hook(format!(
        r#"{{"hook_event_name":"PostToolUse","session_id":"{session}","transcript_path":"/t","cwd":"{cwd}","tool_name":"Edit","tool_input":{{"file_path":"app.py"}},"tool_response":{{"ok":true}}}}"#
    ));
    for name in tail {
        hook(format!(
            r#"{{"hook_event_name":"{name}","session_id":"{session}","transcript_path":"/t","cwd":"{cwd}","last_assistant_message":"Added the rate function.","stop_hook_active":false,"reason":"clear"}}"#
        ));
    }

    let out = Command::new(thruline())
        .args(["fold"])
        .env("THRULINE_HOME", &home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(&repo)
        .output()
        .expect("fold");
    assert!(
        out.status.success(),
        "fold: {}",
        String::from_utf8_lossy(&out.stdout)
    );

    let receipt = dir.path().join("receipt.json");
    let out = Command::new(thruline())
        .args(["receipt", "--predicate-version", "0.1", "--out"])
        .arg(&receipt)
        .env("THRULINE_HOME", &home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(&repo)
        .output()
        .expect("receipt");
    assert!(
        out.status.success(),
        "receipt: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    Session {
        _dir: dir,
        home,
        repo,
        receipt,
    }
}

/// The receipt's own `predicate.capture`, decoded out of the DSSE payload —
/// the bytes a verifier on another machine reads, not an in-process view.
fn capture_block(s: &Session) -> serde_json::Value {
    use base64::Engine as _;
    let text = std::fs::read_to_string(&s.receipt).expect("read receipt");
    let env: serde_json::Value = serde_json::from_str(&text).expect("envelope");
    let payload = env["payload"].as_str().expect("payload");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .expect("base64");
    let statement: serde_json::Value = serde_json::from_slice(&bytes).expect("statement");
    statement["predicate"]["capture"].clone()
}

fn receipt_text(s: &Session) -> String {
    let bytes = std::fs::read(&s.receipt).expect("read");
    let text = String::from_utf8_lossy(&bytes).to_string();
    use base64::Engine as _;
    let env: serde_json::Value = serde_json::from_str(&text).expect("envelope");
    let payload = env["payload"].as_str().expect("payload");
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .expect("base64");
    String::from_utf8_lossy(&decoded).to_string()
}

fn verify(s: &Session) -> (i32, serde_json::Value) {
    let out = Command::new(thruline())
        .args(["verify-receipt"])
        .arg(&s.receipt)
        .arg("--json")
        .env("THRULINE_HOME", &s.home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(&s.repo)
        .output()
        .expect("verify");
    let value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: stdout={} stderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    });
    (out.status.code().unwrap_or(-1), value)
}

/// `thruline claims --last --json` over the store the receipt was cut from.
fn claims_json(s: &Session) -> serde_json::Value {
    let out = Command::new(thruline())
        .args(["claims", "--last", "--json"])
        .env("THRULINE_HOME", &s.home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(&s.repo)
        .output()
        .expect("claims");
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: stdout={} stderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

fn outcome_of(report: &serde_json::Value, name: &str) -> String {
    report["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("no check named {name}: {report:#}"))["outcome"]
        .as_str()
        .unwrap_or("?")
        .to_string()
}

// ---------------------------------------------------------------------------
// The control comes FIRST, deliberately. Every assertion below it is satisfied
// by a build that degrades everything, and this is the only thing that isn't.
// ---------------------------------------------------------------------------

#[test]
fn the_control_a_session_whose_stop_arrived_is_not_degraded() {
    let s = record(&["Stop"]);
    let capture = capture_block(&s);
    assert_eq!(
        capture["capture_degraded"],
        serde_json::json!(false),
        "a complete session was reported as degraded — the rule is over-firing, \
         and every other test in this file is now worthless: {capture:#}"
    );
    let (_, report) = verify(&s);
    assert_eq!(outcome_of(&report, "capture_fidelity"), "pass");
}

#[test]
fn a_session_end_without_a_stop_is_also_complete() {
    // `SessionEnd` is a terminal lifecycle hook. A session that ended cleanly
    // without a Stop is not truncated, and treating it as one would degrade
    // every `/clear` and every ordinary exit.
    let s = record(&["SessionEnd"]);
    assert_eq!(
        capture_block(&s)["capture_degraded"],
        serde_json::json!(false)
    );
}

// ---------------------------------------------------------------------------
// The defect
// ---------------------------------------------------------------------------

#[test]
fn a_session_whose_stop_never_arrived_is_degraded_on_the_face_of_the_receipt() {
    let s = record(&[]);
    let capture = capture_block(&s);
    assert_eq!(
        capture["capture_degraded"],
        serde_json::json!(true),
        "D18/F7: the Stop hook never fired and the receipt says capture was \
         fine: {capture:#}"
    );
}

#[test]
fn the_receipt_says_why_and_a_reader_can_find_the_word() {
    // Q's finding is as much about the WORDS as the flag. A receipt that is
    // `capture_degraded: true` and still contains no occurrence of the cause
    // would satisfy the test above and fix nothing.
    let text = receipt_text(&record(&[]));
    assert!(
        text.contains("hook_events_missing"),
        "the receipt carries no cause a reader can search for: {text}"
    );
}

#[test]
fn the_gate_stops_being_green() {
    let (code, report) = verify(&record(&[]));
    assert_eq!(
        outcome_of(&report, "capture_fidelity"),
        "skipped",
        "{report:#}"
    );
    assert_eq!(
        report["status"], "partial",
        "a truncated session still verified: {report:#}"
    );
    assert_eq!(report["conclusion"], "neutral", "{report:#}");
    assert_eq!(report["verified"], serde_json::json!(false), "{report:#}");
    assert_eq!(
        code, 2,
        "exit code did not move off success for a truncated session: {report:#}"
    );
}

// ---------------------------------------------------------------------------
// R's F7 attack: the hook fired, and then the session carried on
// ---------------------------------------------------------------------------

#[test]
fn a_stop_that_precedes_the_last_activity_does_not_count_as_complete() {
    // R's F7: the agent delivers SessionStart, runs its failing suite through
    // the shim, then neuters the Stop hook and keeps working. "A hook fired"
    // and "the hook fired last" are different facts, and only the second is
    // worth anything. Built by delivering Stop and then MORE tool activity.
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).expect("repo");
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.email", "a@e.invalid"]);
    git(&repo, &["config", "user.name", "A"]);
    std::fs::write(repo.join("app.py"), "x = 1\n").expect("write");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "first"]);

    let session = "aaaa-d18-after-stop";
    let cwd = repo.display().to_string();
    let hook = |json: String| {
        let mut child = Command::new(thruline())
            .args(["hook", "claude-code"])
            .env("THRULINE_HOME", &home)
            .current_dir(&repo)
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
    };
    hook(format!(
        r#"{{"hook_event_name":"SessionStart","session_id":"{session}","transcript_path":"/t","cwd":"{cwd}","source":"startup"}}"#
    ));
    hook(format!(
        r#"{{"hook_event_name":"Stop","session_id":"{session}","transcript_path":"/t","cwd":"{cwd}","last_assistant_message":"Done.","stop_hook_active":false}}"#
    ));
    // …and then it keeps going.
    hook(format!(
        r#"{{"hook_event_name":"PreToolUse","session_id":"{session}","transcript_path":"/t","cwd":"{cwd}","tool_name":"Edit","tool_input":{{"file_path":"app.py"}}}}"#
    ));

    let out = Command::new(thruline())
        .args(["fold"])
        .env("THRULINE_HOME", &home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(&repo)
        .output()
        .expect("fold");
    assert!(out.status.success());

    let receipt = dir.path().join("r.json");
    let out = Command::new(thruline())
        .args(["receipt", "--predicate-version", "0.1", "--out"])
        .arg(&receipt)
        .env("THRULINE_HOME", &home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(&repo)
        .output()
        .expect("receipt");
    assert!(
        out.status.success(),
        "receipt: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let s = Session {
        _dir: dir,
        home,
        repo,
        receipt,
    };
    assert_eq!(
        capture_block(&s)["capture_degraded"],
        serde_json::json!(true),
        "a Stop delivered BEFORE the last recorded work was accepted as a \
         complete lifecycle: {:#}",
        capture_block(&s)
    );
}

// ---------------------------------------------------------------------------
// Commit G p4-fog: the claims projection serves the flag, so a reader with no
// receipt can tell Fog from Recorded. The HQ panel reads `/api/claims`,
// which is `claims --json`'s bytes, and is handed no receipt.
// ---------------------------------------------------------------------------

#[test]
fn claims_json_serves_the_receipts_capture_flag_for_a_complete_session() {
    let s = record(&["Stop"]);
    let claims = claims_json(&s);
    assert_eq!(
        claims["capture_degraded"],
        serde_json::json!(false),
        "a complete session: {claims:#}"
    );
    assert_eq!(
        claims["capture_degraded"],
        capture_block(&s)["capture_degraded"],
        "claims --json and the receipt disagree about the same session"
    );
}

#[test]
fn claims_json_serves_the_receipts_capture_flag_when_the_stop_never_arrived() {
    let s = record(&[]);
    let claims = claims_json(&s);
    assert_eq!(
        claims["capture_degraded"],
        serde_json::json!(true),
        "D18/F7's degradation is on the receipt but not on the claims a reader \
         opens: {claims:#}"
    );
    assert_eq!(
        claims["capture_degraded"],
        capture_block(&s)["capture_degraded"],
        "claims --json and the receipt disagree about the same session"
    );
}
