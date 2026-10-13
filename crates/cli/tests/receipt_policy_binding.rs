//! BUILD-SPEC §8 — the receipt is bound to the policy it was produced under,
//! and the word "Verified." is unreachable when something was unverifiable.
//!
//! # The two experiments this file runs, both of which failed before it existed
//!
//! **1. The opposite-policy mutation.** Issue a receipt in a repository with a
//! `.thruline/policy.yml`, then rewrite that policy to its literal opposite —
//! `require_receipt`, `require_reexecuted`, `allow_skips` and
//! `stop_gate.enabled` all flipped, `reexec.mode` changed — and verify the same
//! receipt again. Measured 2026-09-04 at `58e7c2a`: the two runs produced
//! **byte-identical output at exit 0**, because the only producer wrote
//! `policy_hash: String::new()` and the verifier's first match arm read `""` as
//! *not applicable*.
//!
//! **2. The content-filter case.** Issue a receipt in a repository with a
//! `.gitattributes` clean filter in scope. The snapshotter prints `WARN
//! snapshot taken under content filters: the tree hash may not name the bytes
//! on disk`; the finding was discarded, and `verify-receipt` printed
//! *"Verified. Every check the receipt asserts was confirmed here."* **eight
//! lines below that warning**, at exit 0.
//!
//! Both are run against the real binary, end to end, because both defects lived
//! in the wiring between components that were each individually tested.

// These existing cases pin v0.1 compatibility explicitly; session_v2 cases pin the new protocol.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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

/// A policy and its literal opposite. Every scalar the schema has, flipped.
const POLICY_A: &str = "version: 1\n\
require_receipt: true\n\
claims:\n\
\x20 tests_pass:\n\
\x20   require_reexecuted: true\n\
\x20   allow_skips: false\n\
promises:\n\
\x20 block_on_open: true\n\
\x20 comment_open: true\n\
stop_gate:\n\
\x20 enabled: true\n\
\x20 max_stop_blocks: 2\n\
reexec:\n\
\x20 mode: full\n\
\x20 reruns: 2\n\
\x20 image: auto\n";

const POLICY_OPPOSITE: &str = "version: 1\n\
require_receipt: false\n\
claims:\n\
\x20 tests_pass:\n\
\x20   require_reexecuted: false\n\
\x20   allow_skips: true\n\
promises:\n\
\x20 block_on_open: false\n\
\x20 comment_open: false\n\
stop_gate:\n\
\x20 enabled: false\n\
\x20 max_stop_blocks: 2\n\
reexec:\n\
\x20 mode: claimed\n\
\x20 reruns: 2\n\
\x20 image: auto\n";

struct Fixture {
    _dir: tempfile::TempDir,
    home: PathBuf,
    repo: PathBuf,
    receipt: PathBuf,
}

/// A repository, a recorded session, and a signed receipt.
fn fixture(session: &str, gitattributes: Option<&str>, policy: Option<&str>) -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).expect("repo");
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.email", "a@e.invalid"]);
    git(&repo, &["config", "user.name", "A"]);
    std::fs::write(repo.join("app.py"), "def rate(w):\n    return 2.5 * w\n").expect("write");
    if let Some(attrs) = gitattributes {
        std::fs::write(repo.join(".gitattributes"), attrs).expect("write");
    }
    if let Some(text) = policy {
        std::fs::create_dir_all(repo.join(".thruline")).expect("mkdir");
        std::fs::write(repo.join(".thruline").join("policy.yml"), text).expect("write");
    }
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "first"]);

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
    hook(format!(
        r#"{{"hook_event_name":"Stop","session_id":"{session}","transcript_path":"/t","cwd":"{}","last_assistant_message":"Added the rate function.","stop_hook_active":false}}"#,
        repo.display()
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

    Fixture {
        _dir: dir,
        home,
        repo,
        receipt,
    }
}

impl Fixture {
    fn verify(&self) -> Output {
        Command::new(thruline())
            .args(["verify-receipt"])
            .arg(&self.receipt)
            .args(["--offline"])
            .env("THRULINE_HOME", &self.home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .current_dir(&self.repo)
            .output()
            .expect("verify-receipt")
    }

    fn write_policy(&self, text: &str) {
        std::fs::write(self.repo.join(".thruline").join("policy.yml"), text).expect("write");
    }
}

#[test]
fn a_policy_rewritten_to_its_opposite_changes_the_verification() {
    let f = fixture("11111111-2222-4333-8444-555555555555", None, Some(POLICY_A));

    let before = f.verify();
    let before_text = String::from_utf8_lossy(&before.stdout).to_string();
    assert!(
        before_text.contains("ok   policy_hash"),
        "the receipt is not bound to the policy it was produced under: {before_text}"
    );

    f.write_policy(POLICY_OPPOSITE);
    let after = f.verify();
    let after_text = String::from_utf8_lossy(&after.stdout).to_string();

    // The experiment that exposed the defect: before this fix these two were
    // byte-identical, both at exit 0.
    assert_ne!(
        before_text, after_text,
        "a policy rewritten to its literal OPPOSITE produced identical output — \
         the receipt is bound to nothing"
    );
    assert_ne!(
        before.status.code(),
        after.status.code(),
        "the exit code did not move under an opposite policy"
    );
    assert_eq!(after.status.code(), Some(1), "{after_text}");
    assert!(after_text.contains("FAIL policy_hash"), "{after_text}");
    assert!(
        !after_text.contains("Verified. Every check"),
        "{after_text}"
    );

    // Restoring the original policy restores the pass, so the check is about
    // the policy and not about having been run twice.
    f.write_policy(POLICY_A);
    let restored = f.verify();
    assert_eq!(
        String::from_utf8_lossy(&restored.stdout),
        before_text,
        "the check does not depend only on the policy"
    );
}

#[test]
fn a_receipt_that_cannot_name_its_policy_is_refused_rather_than_excused() {
    // The empty string was the producer's value and the verifier read it as
    // "not applicable". Both an ABSENT field and an EMPTY one are refused now,
    // under their own reasons — hand-built receipts, because the producer can
    // no longer emit either.
    let f = fixture("22222222-3333-4444-8555-666666666666", None, Some(POLICY_A));
    let text = std::fs::read_to_string(&f.receipt).expect("receipt");
    let envelope: serde_json::Value = serde_json::from_str(&text).expect("json");
    let payload = envelope["payload"].as_str().expect("payload");

    for (what, mutate) in [
        (
            "absent",
            Box::new(|v: &mut serde_json::Value| {
                v["predicate"]["verifier"]
                    .as_object_mut()
                    .expect("verifier")
                    .remove("policy_hash");
            }) as Box<dyn Fn(&mut serde_json::Value)>,
        ),
        (
            "empty",
            Box::new(|v: &mut serde_json::Value| {
                v["predicate"]["verifier"]["policy_hash"] = serde_json::json!("");
            }),
        ),
    ] {
        use base64::Engine;
        let raw = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .expect("b64");
        let mut statement: serde_json::Value = serde_json::from_slice(&raw).expect("statement");
        mutate(&mut statement);
        let mut env = envelope.clone();
        env["payload"] = serde_json::json!(base64::engine::general_purpose::STANDARD
            .encode(serde_json::to_vec(&statement).expect("bytes")));
        let path = f.repo.join(format!("receipt-{what}.json"));
        std::fs::write(&path, serde_json::to_string(&env).expect("json")).expect("write");

        // The signature no longer covers the mutated payload, so this run also
        // fails on `signature` — that is fine and expected. What is asserted is
        // that `policy_hash` names the right defect, under its own words.
        let out = Command::new(thruline())
            .args(["verify-receipt"])
            .arg(&path)
            .args(["--offline"])
            .env("THRULINE_HOME", &f.home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .current_dir(&f.repo)
            .output()
            .expect("verify-receipt");
        let text = String::from_utf8_lossy(&out.stdout);
        assert_ne!(out.status.code(), Some(0), "{text}");
        assert!(!text.contains("Verified. Every check"), "{text}");
        assert!(text.contains("FAIL policy_hash"), "{what}: {text}");
        let expect = if what == "absent" {
            "states NO policy hash"
        } else {
            "EMPTY policy hash"
        };
        assert!(
            text.contains(expect),
            "{what} did not get its own reason: {text}"
        );
    }
}

#[test]
fn the_verified_sentence_is_absent_when_a_content_filter_is_in_scope() {
    // The §8.1 non-collapse case, end to end. The snapshotter warns; before
    // this fix the warning and the word "Verified." appeared in the same
    // terminal output, eight lines apart.
    let f = fixture(
        "33333333-4444-4555-8666-777777777777",
        Some("*.py filter=launder\n"),
        None,
    );

    // The receipt records the finding rather than discarding it.
    let text = std::fs::read_to_string(&f.receipt).expect("receipt");
    let envelope: serde_json::Value = serde_json::from_str(&text).expect("json");
    use base64::Engine;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(envelope["payload"].as_str().expect("payload"))
        .expect("b64");
    let statement: serde_json::Value = serde_json::from_slice(&raw).expect("statement");
    let capture = &statement["predicate"]["capture"];
    assert_eq!(
        capture["capture_degraded"], true,
        "a receipt issued under a clean filter says its capture was clean: {capture}"
    );
    assert!(
        capture["content_filters"]
            .as_str()
            .unwrap_or_default()
            .contains("launder"),
        "{capture}"
    );

    let out = f.verify();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        !text.contains("Verified. Every check"),
        "the verified sentence printed under a clean filter: {text}"
    );
    assert_ne!(out.status.code(), Some(0), "{text}");
    assert!(text.contains("skip capture_fidelity"), "{text}");

    // The control: the same session with no filter reaches the sentence, so
    // the guard is about the filter and not about the fixture.
    let clean = fixture("44444444-5555-4666-8777-888888888888", None, None);
    let out = clean.verify();
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(text.contains("Verified. Every check"), "{text}");
}
