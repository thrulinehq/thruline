//! **F4 and F5, end to end, through the receipt a reviewer actually reads.**
//!
//! Both were reproduced on `b5335c0` with the harness below.
//!
//! **F4** — a committed `.thruline/claims.yml` replaced the built-in table
//! wholesale, so a file nobody signed chose the words a reader is shown:
//!
//! ```text
//! without the file : 1 unverifiable claim · conclusion neutral
//! with    the file : 0 claims             · conclusion success  (badge: verified)
//! `claims.yml` in the signed payload: 0 occurrences
//! ```
//!
//! **F5** — the same green, reached without any file at all. Report completion
//! in ordinary English the substring table does not contain ("the change is in
//! place", "that should do it", "diff applied") and the claim count is zero, so
//! every gate has nothing to iterate over:
//!
//! ```text
//! "The change is in place. That should do it - diff applied."
//!   -> 0 claims · conclusion success · verified true
//! ```
//!
//! Zero claims and proven-good were the same green. The fix is NOT a bigger
//! phrase table — that is an arms race against English and it loses. It is that
//! **a session which changed files and made no recognised claim may not
//! conclude `success`**: the absence of a claim is not the absence of a
//! problem, and stopping is itself a completion assertion this build cannot
//! check.

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

/// One recorded session: a committed repository, an optional claims table, a
/// working-tree edit, and a final message.
fn record(claims_yml: Option<&str>, message: &str, edit: bool) -> Session {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).expect("repo");
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.email", "a@e.invalid"]);
    git(&repo, &["config", "user.name", "A"]);
    std::fs::write(repo.join("app.py"), "def rate(w):\n    return 2.5 * w\n").expect("write");
    if let Some(yaml) = claims_yml {
        std::fs::create_dir_all(repo.join(".thruline")).expect("mkdir");
        std::fs::write(repo.join(".thruline/claims.yml"), yaml).expect("write");
    }
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "first"]);

    let session = "77777777-8888-4999-8aaa-bbbbbbbbbbbb";
    let hook = |json: String| {
        use std::io::Write;
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
        r#"{{"hook_event_name":"SessionStart","session_id":"{session}","transcript_path":"/t","cwd":"{}","source":"startup"}}"#,
        repo.display()
    ));
    if edit {
        std::fs::write(repo.join("app.py"), "def rate(w):\n    return 3.0 * w\n").expect("edit");
        // The event a real Claude Code session emits for an edit. Without it
        // the ledger has no evidence a file changed, and "changed files" is the
        // condition F5's floor turns on — deliberately, since a floor that
        // fires without evidence would be pessimism rather than a finding.
        hook(format!(
            r#"{{"hook_event_name":"PostToolUse","session_id":"{session}","transcript_path":"/t","cwd":"{}","tool_name":"Edit","tool_input":{{"file_path":"{}"}},"tool_response":{{"success":true}}}}"#,
            repo.display(),
            repo.join("app.py").display()
        ));
    }
    hook(format!(
        r#"{{"hook_event_name":"Stop","session_id":"{session}","transcript_path":"/t","cwd":"{}","last_assistant_message":{},"stop_hook_active":false}}"#,
        repo.display(),
        serde_json::to_string(message).expect("json")
    ));

    let out = Command::new(thruline())
        .args(["fold"])
        .env("THRULINE_HOME", &home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(&repo)
        .output()
        .expect("fold");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let receipt = dir.path().join("receipt.json");
    let out = Command::new(thruline())
        .args(["receipt", "--out"])
        .arg(&receipt)
        .env("THRULINE_HOME", &home)
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

impl Session {
    /// The signed payload, decoded. What a reviewer's verifier reads.
    fn payload(&self) -> serde_json::Value {
        let env: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&self.receipt).expect("read"))
                .expect("envelope");
        let b64 = env["payload"].as_str().expect("payload");
        let bytes = base64_decode(b64);
        serde_json::from_slice(&bytes).expect("statement")
    }

    /// `verify-receipt --json`'s conclusion — the word the badge and the check
    /// run are drawn from.
    fn conclusion(&self) -> String {
        let out = Command::new(thruline())
            .args(["verify-receipt"])
            .arg(&self.receipt)
            .arg("--json")
            .env("THRULINE_HOME", &self.home)
            .current_dir(&self.repo)
            .output()
            .expect("verify-receipt");
        let v: serde_json::Value =
            serde_json::from_slice(&out.stdout).expect("verify-receipt --json");
        v["conclusion"].as_str().expect("conclusion").to_string()
    }

    fn claims_stdout(&self) -> String {
        let out = Command::new(thruline())
            .args(["claims"])
            .env("THRULINE_HOME", &self.home)
            .current_dir(&self.repo)
            .output()
            .expect("claims");
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    /// `thruline <verb> --json`, parsed. `/api/claims` and `/api/explain` are
    /// these bytes verbatim (`docs/UI-SERVER-CONTRACT.md`), so this is also
    /// what the app reads.
    fn json(&self, verb: &str) -> serde_json::Value {
        let out = Command::new(thruline())
            .args([verb, "--json"])
            .env("THRULINE_HOME", &self.home)
            .current_dir(&self.repo)
            .output()
            .unwrap_or_else(|e| panic!("{verb} --json: {e}"));
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "{verb} --json: {e}\nstdout: {}\nstderr: {}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        })
    }
}

fn base64_decode(s: &str) -> Vec<u8> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut idx = [255u8; 256];
    for (i, c) in T.iter().enumerate() {
        idx[*c as usize] = i as u8;
    }
    let mut acc = 0u32;
    let mut bits = 0u32;
    let mut out = Vec::new();
    for b in s.bytes() {
        if b == b'=' || b == b'\n' || b == b'\r' {
            continue;
        }
        let v = idx[b as usize];
        assert_ne!(v, 255, "not base64: {b}");
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

const EMPTYING_TABLE: &str = r#"version: 1
patterns:
  - kind: tests_pass
    scope: all
    phrases:
      - "zzzzz never said by anyone zzzzz"
"#;

/// F4. The committed file may add; it may not take the claim away, and it may
/// not stay out of the receipt.
#[test]
fn a_committed_claims_table_can_neither_empty_the_claim_table_nor_hide_itself() {
    let s = record(Some(EMPTYING_TABLE), "All tests pass.", true);
    let p = s.payload();
    let claims = p["predicate"]["claims"].as_array().expect("claims");
    assert!(
        claims.iter().any(|c| c["kind"] == "tests_pass"),
        "the repo file removed a default pattern: {claims:#?}"
    );
    assert_ne!(
        s.conclusion(),
        "success",
        "an unproved `all tests pass` concluded success"
    );

    let text = serde_json::to_string(&p).expect("payload text");
    assert!(
        text.contains("claims.yml"),
        "the receipt does not disclose that a repository table was in force"
    );
    assert!(
        text.contains("sha256:"),
        "the table is named but not pinned: no digest in the payload"
    );
    assert!(
        s.claims_stdout().contains("claims.yml"),
        "`thruline claims` does not say the table was overridden:\n{}",
        s.claims_stdout()
    );
}

/// F5. Ordinary English the table does not contain, over a real edit.
#[test]
fn a_session_that_changed_files_and_claimed_nothing_may_not_conclude_success() {
    let s = record(
        None,
        "The change is in place. That should do it - diff applied.",
        true,
    );
    let p = s.payload();
    let verdicts = p["predicate"]["verdicts"].as_array().expect("verdicts");
    assert!(
        !verdicts.is_empty(),
        "a session that changed files and claimed nothing produced nothing for a gate to \
         iterate over"
    );
    assert!(
        verdicts.iter().all(|v| v["verdict"] != "supported"),
        "nothing was proved, so nothing may be `supported`: {verdicts:#?}"
    );
    assert!(
        verdicts.iter().any(|v| v["verifier"]
            .as_str()
            .unwrap_or("")
            .starts_with("unclaimed")),
        "the floor is not attributed to a named check: {verdicts:#?}"
    );
    assert_ne!(
        s.conclusion(),
        "success",
        "zero claims and proven-good are still the same green"
    );
}

/// **F5 reaches the screen, not only the receipt.**
///
/// `thruline ui` renders `/api/explain`, whose body is `explain --json`
/// verbatim. `explain` used to derive its own verdicts inline from the
/// extraction and never applied the floor, so on the operator's store, 2026-09-08,
/// 7 of the 13 sessions carrying a `stop` reported one claim under
/// `thruline claims` and `claims_vs_proof: []` under `thruline explain` —
/// and the app said "no completion claim was made, so there is nothing to find
/// against" about a session that had changed 18 files.
///
/// The two verbs read the same record, so they may not disagree about what it
/// holds. This pins the agreement rather than the wording of either.
#[test]
fn explain_and_claims_report_the_same_floored_claim() {
    let s = record(
        None,
        "The change is in place. That should do it - diff applied.",
        true,
    );
    let claims = s.json("claims");
    let explain = s.json("explain");
    let listed = claims["claims"].as_array().expect("claims");
    let shown = explain["claims_vs_proof"].as_array().expect("explain");
    assert_eq!(
        shown.len(),
        listed.len(),
        "`explain` and `claims` disagree about how many claims this record holds — \
         claims: {listed:#?}\nexplain: {shown:#?}"
    );
    assert!(
        !shown.is_empty(),
        "the floor did not reach `explain` at all"
    );
    for (c, e) in listed.iter().zip(shown.iter()) {
        assert_eq!(c["kind"], e["kind"], "the two verbs named different kinds");
        assert_eq!(c["text"], e["text"], "the two verbs quoted different text");
        assert_eq!(
            c["verdict"], e["verdict"],
            "the two verbs reached different verdicts on the same claim"
        );
    }
}

/// The floor is not a blanket pessimism: a session that changed nothing has
/// nothing to be unproved about, and stays exactly as it was.
#[test]
fn a_session_that_changed_no_files_and_claimed_nothing_is_not_floored() {
    let s = record(None, "Nothing needed changing here.", false);
    let p = s.payload();
    assert!(
        p["predicate"]["claims"]
            .as_array()
            .expect("claims")
            .is_empty(),
        "a no-op session was given a claim it never made: {:#?}",
        p["predicate"]["claims"]
    );
}

/// The floor never displaces a claim that WAS recognised: it fires only on an
/// empty extraction, so it can add a verdict and can never replace one.
#[test]
fn a_recognised_claim_is_judged_by_its_own_verifier_and_not_by_the_floor() {
    let s = record(None, "All tests pass.", true);
    let p = s.payload();
    let verdicts = p["predicate"]["verdicts"].as_array().expect("verdicts");
    assert_eq!(verdicts.len(), 1, "{verdicts:#?}");
    assert!(
        verdicts[0]["verifier"]
            .as_str()
            .unwrap_or("")
            .starts_with("tests_pass"),
        "the floor displaced a real verifier: {verdicts:#?}"
    );
}
