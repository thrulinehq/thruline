//! RECEIPT-V01-DEFAULT (founder ruling R-2026-09-28-21, launch D-084):
//! **`thruline receipt` with no flag issues predicate v0.1.**
//!
//! v0.2 stays reachable, but only by `--predicate-version 0.2`. What must not
//! move with the default is pinned here beside it:
//!
//! - the default statement's `predicateType` is the v0.1 URI and it satisfies
//!   the committed `spec/execution-check-v0.1.schema.json`;
//! - `.thruline/policy.yml`'s `require_receipt` — which reads v0.1 only — is
//!   satisfied by a receipt issued with no flag;
//! - the explicit flag still issues v0.2;
//! - `verify-receipt` still tells the two versions apart by itself, and every
//!   rendering of each still states that version's one conclusion.
//!
//! `thruline acceptance receipt` (v0.2 only, no flag) is pinned in
//! `l0_acceptance.rs`, beside the fixture that can issue one.

#[path = "../../attest/tests/schema_subset/mod.rs"]
mod schema_subset;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const V01: &str = "https://thrulinehq.com/attestation/execution-check/v0.1";
const V02: &str = "https://thrulinehq.com/attestation/execution-check/v0.2";
const SCHEMA_V01: &str = include_str!("../../../spec/execution-check-v0.1.schema.json");
const SESSION: &str = "cccccccc-5e5e-4a4a-8b8b-d0d0d0d0d0d0";

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

struct Fixture {
    dir: tempfile::TempDir,
    home: PathBuf,
    repo: PathBuf,
}

/// A repository with a committed policy requiring a receipt, and one recorded,
/// folded session that edited a file.
fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(repo.join("src")).expect("repo");
    std::fs::create_dir_all(repo.join(".thruline")).expect("mkdir");
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.email", "a@e.invalid"]);
    git(&repo, &["config", "user.name", "A"]);
    std::fs::write(repo.join("src").join("main.rs"), "fn main() {}\n").expect("write");
    std::fs::write(
        repo.join(".thruline").join("policy.yml"),
        "version: 1\nrequire_receipt: true\nclaims: {}\n",
    )
    .expect("write");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "base"]);

    let f = Fixture { dir, home, repo };
    f.hook(&format!(
        r#"{{"hook_event_name":"SessionStart","session_id":"{SESSION}","transcript_path":"/t","cwd":"{}","source":"startup"}}"#,
        f.repo.display()
    ));
    std::fs::write(
        f.repo.join("src").join("main.rs"),
        "fn main() { println!(\"x\"); }\n",
    )
    .expect("write");
    f.hook(&format!(
        r#"{{"hook_event_name":"PostToolUse","session_id":"{SESSION}","transcript_path":"/t","cwd":"{}","tool_name":"Edit","tool_input":{{"file_path":"{}/src/main.rs"}},"tool_response":{{}}}}"#,
        f.repo.display(),
        f.repo.display()
    ));
    f.hook(&format!(
        r#"{{"hook_event_name":"Stop","session_id":"{SESSION}","transcript_path":"/t","cwd":"{}","last_assistant_message":"Printed x from main.","stop_hook_active":false}}"#,
        f.repo.display()
    ));
    let out = f.run(&["fold"]);
    assert!(
        out.status.success(),
        "fold: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    f
}

impl Fixture {
    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(thruline());
        c.args(args)
            .env("THRULINE_HOME", &self.home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("GITHUB_BASE_REF")
            .env_remove("GITHUB_EVENT_NAME")
            .env_remove("THRULINE_POLICY_CONCLUSION")
            .current_dir(&self.repo);
        c
    }

    fn run(&self, args: &[&str]) -> Output {
        self.cmd(args).output().expect("thruline")
    }

    fn hook(&self, json: &str) {
        use std::io::Write;
        let mut child = self
            .cmd(&["hook", "claude-code"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
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

    /// `thruline receipt <extra> --out <path>`, asserted to succeed.
    fn issue(&self, extra: &[&str], path: &Path) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        let mut args = vec!["receipt"];
        args.extend_from_slice(extra);
        args.push("--out");
        args.push(path.to_str().expect("path"));
        let out = self.run(&args);
        assert!(
            out.status.success(),
            "receipt {extra:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn verify(&self, receipt: &Path, extra: &[&str]) -> Output {
        let mut args = vec!["verify-receipt", receipt.to_str().expect("path")];
        args.extend_from_slice(extra);
        self.run(&args)
    }
}

fn statement_of(envelope_text: &str) -> serde_json::Value {
    use base64::Engine as _;
    let envelope: serde_json::Value = serde_json::from_str(envelope_text).expect("envelope");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(envelope["payload"].as_str().expect("payload"))
        .expect("base64");
    serde_json::from_slice(&bytes).expect("statement")
}

#[test]
fn a_receipt_issued_with_no_flag_is_predicate_v01_and_satisfies_its_schema() {
    assert_eq!(
        thruline_attest::PREDICATE_TYPE,
        V01,
        "the crate's v0.1 constant moved"
    );
    let f = fixture();
    let out = f.run(&["receipt", "--json"]);
    assert!(
        out.status.success(),
        "receipt: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let statement = statement_of(&String::from_utf8_lossy(&out.stdout));
    assert_eq!(
        statement["predicateType"], V01,
        "`thruline receipt` with no flag did not issue predicate v0.1"
    );
    assert_eq!(statement["predicate"]["session"]["id"], SESSION);

    let schema: serde_json::Value = serde_json::from_str(SCHEMA_V01).expect("schema");
    let errs = schema_subset::validate(&schema, &schema, &statement);
    assert!(
        errs.is_empty(),
        "the default receipt does not satisfy spec/execution-check-v0.1.schema.json:\n  {}",
        errs.join("\n  ")
    );
}

#[test]
fn require_receipt_is_satisfied_by_a_receipt_issued_with_no_flag() {
    let f = fixture();
    let out = f.run(&["policy", "--conclusion", "--repo", f.repo.to_str().unwrap()]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "the policy passed with no receipt at all, so this test proves nothing: {}",
        String::from_utf8_lossy(&out.stdout)
    );

    f.issue(&[], &f.repo.join(".thruline/receipts/latest.json"));
    let out = f.run(&["policy", "--conclusion", "--repo", f.repo.to_str().unwrap()]);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "a receipt issued with no flag did not satisfy require_receipt: {text}"
    );
    assert_eq!(text.trim(), "success", "{text}");
}

#[test]
fn predicate_v02_is_still_issued_by_the_explicit_flag() {
    let f = fixture();
    let path = f.dir.path().join("outside").join("v02.json");
    f.issue(&["--predicate-version", "0.2"], &path);
    let statement = statement_of(&std::fs::read_to_string(&path).expect("read"));
    assert_eq!(statement["predicateType"], V02);
    assert_eq!(statement["predicate"]["kind"], "session_claims");

    let path = f.dir.path().join("outside").join("v01.json");
    f.issue(&["--predicate-version", "0.1"], &path);
    let statement = statement_of(&std::fs::read_to_string(&path).expect("read"));
    assert_eq!(statement["predicateType"], V01);

    let out = f.run(&["receipt", "--predicate-version", "0.3", "--json"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "a third version was accepted: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `verify-receipt` has no version flag: it reads the version from the
/// statement. Each reader is identified by what only it emits — the v0.2
/// session reader serves `signed_record` and `evidence_trust`, the v0.1 reader
/// does not — and every rendering states the same conclusion as `--json`.
#[test]
fn verify_receipt_detects_each_version_and_renders_its_one_conclusion() {
    let f = fixture();
    let v01 = f.dir.path().join("outside").join("default.json");
    f.issue(&[], &v01);
    let v02 = f.dir.path().join("outside").join("v02.json");
    f.issue(&["--predicate-version", "0.2"], &v02);

    for (path, is_v02) in [(&v01, false), (&v02, true)] {
        let out = f.verify(path, &["--json"]);
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "verify-receipt --json on {}: {e}\n{}",
                path.display(),
                String::from_utf8_lossy(&out.stderr)
            )
        });
        assert_eq!(
            json.get("signed_record").is_some(),
            is_v02,
            "verify-receipt read {} with the wrong reader: {json:#}",
            path.display()
        );
        assert_eq!(json.get("evidence_trust").is_some(), is_v02, "{json:#}");
        let conclusion = json["conclusion"].as_str().expect("conclusion").to_string();
        let code = out.status.code();

        let out = f.verify(
            path,
            &[
                "--format",
                "check-run",
                "--head-sha",
                "0".repeat(40).as_str(),
            ],
        );
        let check: serde_json::Value = serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|e| panic!("check-run: {e}\n{}", String::from_utf8_lossy(&out.stderr)));
        assert_eq!(check["conclusion"], conclusion.as_str(), "{check:#}");
        assert_eq!(
            out.status.code(),
            code,
            "check-run and --json exit differently"
        );

        for format in ["text", "md", "badge"] {
            let out = f.verify(path, &["--format", format]);
            assert!(
                !out.stdout.is_empty(),
                "--format {format} rendered nothing: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(
                out.status.code(),
                code,
                "--format {format} exits differently"
            );
        }
    }
}
