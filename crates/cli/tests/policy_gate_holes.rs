//! Four holes Track R found in the §9 policy gate, each with the reproduction
//! that opened it.
//!
//! Every one of them fails in the same direction: the gate returns **success**
//! on a run it should have refused. That is the only direction that matters —
//! a gate that wrongly blocks gets fixed within the hour because somebody is
//! standing in front of it, and a gate that wrongly passes is a green check
//! somebody trusts.
//!
//! | | what R found | measured before the fix |
//! |---|---|---|
//! | **F8** | the PR is judged by the policy the PR just wrote | strict base → `failure`; the same PR rewrites `.thruline/policy.yml` → `success` |
//! | **F9** | an empty `changed_paths` skips every path rule | commit-before-testing → `changed_paths = Some([])` → every `required_when` rule silently skipped → `success` |
//! | **F10** | an empty directory satisfies `require_receipt` | `mkdir -p .thruline/receipts` as the only change → `failure` → `success` |
//! | **F11** | `max_stop_blocks` is dead on arrival | block ledger says 99 blocks, policy allows 0 → `success` |
//!
//! These are end-to-end against the real binary because all four lived in the
//! wiring — `crates/policy` evaluated correctly in every case, and
//! `policy_cmd.rs` handed it a subject that was not true.

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

/// `required_when: ["**"]` and `require_receipt: true` — the policy on the base
/// branch, which refuses this session.
const STRICT: &str = "version: 1\n\
require_receipt: true\n\
claims:\n\
\x20 tests_pass:\n\
\x20   required_when: { paths: [\"**\"] }\n\
\x20   require_reexecuted: true\n";

/// R's mutation, verbatim: `required_when` narrowed to a directory the change
/// does not touch, `require_receipt` turned off.
const LOOSE: &str = "version: 1\n\
require_receipt: false\n\
claims:\n\
\x20 tests_pass:\n\
\x20   required_when: { paths: [\"vendor/**\"] }\n\
\x20   require_reexecuted: true\n";

const SESSION: &str = "77777777-8888-4999-8aaa-bbbbbbbbbbbb";

struct Fixture {
    _dir: tempfile::TempDir,
    home: PathBuf,
    repo: PathBuf,
}

/// A repository with a committed policy and one recorded session.
///
/// `commit_the_edit` is F9's whole subject: an agent that commits **before**
/// the session's last captured event leaves the head tree and the final
/// snapshot tree as the same git object, so `git diff-tree` between them is
/// empty.
fn fixture(policy: Option<&str>, commit_the_edit: bool) -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(repo.join("src")).expect("repo");
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.email", "a@e.invalid"]);
    git(&repo, &["config", "user.name", "A"]);
    std::fs::write(repo.join("src").join("main.rs"), "fn main() {}\n").expect("write");
    if let Some(text) = policy {
        std::fs::create_dir_all(repo.join(".thruline")).expect("mkdir");
        std::fs::write(repo.join(".thruline").join("policy.yml"), text).expect("write");
    }
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "base"]);

    let f = Fixture {
        _dir: dir,
        home,
        repo,
    };
    f.hook(&format!(
        r#"{{"hook_event_name":"SessionStart","session_id":"{SESSION}","transcript_path":"/t","cwd":"{}","source":"startup"}}"#,
        f.repo.display()
    ));
    std::fs::write(
        f.repo.join("src").join("main.rs"),
        "fn main() { println!(\"x\"); }\n",
    )
    .expect("write");
    if commit_the_edit {
        git(&f.repo, &["add", "-A"]);
        git(&f.repo, &["commit", "-qm", "work"]);
    }
    // A mutating `PostToolUse` is what makes the fold take a tree snapshot, and
    // a snapshot is what makes `changed_paths` *determined* at all.
    f.hook(&format!(
        r#"{{"hook_event_name":"PostToolUse","session_id":"{SESSION}","transcript_path":"/t","cwd":"{}","tool_name":"Edit","tool_input":{{"file_path":"{}/src/main.rs"}},"tool_response":{{}}}}"#,
        f.repo.display(),
        f.repo.display()
    ));
    f.hook(&format!(
        r#"{{"hook_event_name":"Stop","session_id":"{SESSION}","transcript_path":"/t","cwd":"{}","last_assistant_message":"All tests pass.","stop_hook_active":false}}"#,
        f.repo.display()
    ));
    let out = f.cmd(&["fold"]).output().expect("fold");
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
            .current_dir(&self.repo);
        c
    }

    fn hook(&self, json: &str) {
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

    /// `thruline policy <args> --repo <repo>`; returns (stdout+stderr, code).
    fn policy(&self, args: &[&str]) -> (String, i32) {
        self.policy_env(args, &[])
    }

    fn policy_env(&self, args: &[&str], env: &[(&str, &str)]) -> (String, i32) {
        let mut c = self.cmd(&["policy"]);
        c.args(args).arg("--repo").arg(&self.repo);
        for (k, v) in env {
            c.env(k, v);
        }
        let out = c.output().expect("run thruline policy");
        let mut text = String::from_utf8_lossy(&out.stdout).to_string();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        (text, out.status.code().unwrap_or(-1))
    }

    fn write_policy(&self, text: &str) {
        std::fs::write(self.repo.join(".thruline").join("policy.yml"), text).expect("write");
    }

    fn issue_receipt(&self, to: &str) {
        let path = self.repo.join(to);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        let out = self
            .cmd(&["receipt", "--predicate-version", "0.1", "--out"])
            .arg(&path)
            .output()
            .expect("receipt");
        assert!(
            out.status.success(),
            "receipt: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

// ---------------------------------------------------------------------------
// F8 — the PR is judged by the policy the PR just wrote.
// ---------------------------------------------------------------------------

/// Measured before the fix at `b5335c0`: base policy → `failure` (exit 3); the
/// same PR rewrites `.thruline/policy.yml` and the very next run of the gate →
/// `success` (exit 0). The change under review disarmed the gate reviewing it.
#[test]
fn f8_a_pull_request_cannot_weaken_the_policy_that_judges_it() {
    let f = fixture(Some(STRICT), false);
    f.issue_receipt(".thruline/receipts/latest.json");

    let (base_text, base_code) = f.policy(&["--conclusion"]);
    assert_eq!(
        base_code, 3,
        "the base policy does not refuse this session, so the experiment proves nothing: {base_text}"
    );

    git(&f.repo, &["checkout", "-q", "-b", "pr"]);
    f.write_policy(LOOSE);
    git(&f.repo, &["add", "-A"]);
    git(&f.repo, &["commit", "-qm", "loosen the policy"]);

    let (text, code) = f.policy(&["--conclusion", "--base", "main"]);
    assert_eq!(
        code, 3,
        "the PR's own rewritten policy judged the PR: {text}"
    );
    assert_eq!(text.trim(), "failure", "{text}");
}

/// The Action does not have to remember the flag. `GITHUB_BASE_REF` is set on
/// every `pull_request` event, and it is adopted when `--base` is absent.
#[test]
fn f8_github_base_ref_is_adopted_when_no_flag_is_given() {
    let f = fixture(Some(STRICT), false);
    f.issue_receipt(".thruline/receipts/latest.json");
    git(&f.repo, &["checkout", "-q", "-b", "pr"]);
    f.write_policy(LOOSE);
    git(&f.repo, &["add", "-A"]);
    git(&f.repo, &["commit", "-qm", "loosen"]);

    let (text, code) = f.policy_env(&["--conclusion"], &[("GITHUB_BASE_REF", "main")]);
    assert_eq!(code, 3, "GITHUB_BASE_REF was ignored: {text}");
}

/// A base ref that cannot be resolved is a **refusal**, never a quiet fall back
/// to the head checkout. Falling back would restore F8 exactly, and would do it
/// on the one run where something had already gone wrong.
#[test]
fn f8_an_unresolvable_base_is_a_refusal_not_a_fallback_to_head() {
    let f = fixture(Some(STRICT), false);
    f.write_policy(LOOSE);
    let (text, code) = f.policy(&["--conclusion", "--base", "no-such-branch"]);
    assert_eq!(code, 1, "an unresolvable base did not refuse: {text}");
    assert!(
        !text.contains("success"),
        "a conclusion was printed from the head policy: {text}"
    );
    assert!(text.contains("no-such-branch"), "{text}");
}

/// The cost of the operator's ruling, pinned rather than left to be discovered:
/// a PR that **adds** the first policy is not gated by it, because the base
/// branch had not opted in. Read from the base branch, never the PR head.
#[test]
fn f8_a_policy_the_pr_adds_does_not_gate_that_pr() {
    let f = fixture(None, false);
    git(&f.repo, &["checkout", "-q", "-b", "pr"]);
    std::fs::create_dir_all(f.repo.join(".thruline")).expect("mkdir");
    f.write_policy(STRICT);
    git(&f.repo, &["add", "-A"]);
    git(&f.repo, &["commit", "-qm", "add a policy"]);

    let (text, code) = f.policy(&["--base", "main"]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("nothing is gated"), "{text}");
}

// ---------------------------------------------------------------------------
// F9 — an empty `changed_paths` skips every path rule.
// ---------------------------------------------------------------------------

/// An agent that commits before the session's last captured event leaves the
/// head tree and the final snapshot tree as **the same git object**, so
/// `git diff-tree` prints nothing and `changed_paths` is `Some(vec![])` —
/// *determined, and empty*. Measured before the fix: the identical session with
/// the edit left uncommitted → `failure` (exit 3); committed first → `success`
/// (exit 0), with every `required_when` rule silently skipped.
#[test]
fn f9_committing_before_the_session_ends_does_not_switch_the_path_rules_off() {
    let uncommitted = fixture(Some(STRICT), false);
    uncommitted.issue_receipt(".thruline/receipts/latest.json");
    let (text, code) = uncommitted.policy(&["--conclusion"]);
    assert_eq!(code, 3, "the control case does not fail: {text}");

    let committed = fixture(Some(STRICT), true);
    committed.issue_receipt(".thruline/receipts/latest.json");
    let (text, code) = committed.policy(&[]);
    assert_ne!(
        code, 0,
        "committing before the session ended turned every path rule off: {text}"
    );
    assert!(
        text.contains("required_when") || text.contains("require_reexecuted"),
        "the rule was skipped rather than evaluated: {text}"
    );
}

// ---------------------------------------------------------------------------
// F10 — an empty directory satisfies `require_receipt`.
// ---------------------------------------------------------------------------

/// Measured before the fix: `require_receipt: true` with no `.thruline/receipts`
/// → `failure` (exit 3); `mkdir -p .thruline/receipts` as the **only** change →
/// `success` (exit 0). A directory is not a receipt.
#[test]
fn f10_an_empty_receipts_directory_is_not_a_receipt() {
    let f = fixture(
        Some("version: 1\nrequire_receipt: true\nclaims: {}\n"),
        false,
    );

    let (text, code) = f.policy(&["--conclusion"]);
    assert_eq!(code, 3, "{text}");

    std::fs::create_dir_all(f.repo.join(".thruline").join("receipts")).expect("mkdir");
    let (text, code) = f.policy(&["--conclusion"]);
    assert_eq!(
        code, 3,
        "an empty directory satisfied require_receipt: {text}"
    );

    // Nor does a file that merely has the right name and extension.
    std::fs::write(
        f.repo
            .join(".thruline")
            .join("receipts")
            .join("latest.json"),
        "{}",
    )
    .expect("write");
    let (text, code) = f.policy(&["--conclusion"]);
    assert_eq!(
        code, 3,
        "an empty JSON object satisfied require_receipt: {text}"
    );

    // A receipt this binary actually issued does.
    f.issue_receipt(".thruline/receipts/latest.json");
    let (text, code) = f.policy(&["--conclusion"]);
    assert_eq!(
        code, 0,
        "a real receipt did not satisfy require_receipt: {text}"
    );
}

/// `require_receipt` means a receipt for **this** run. A receipt issued for
/// some other session is a receipt, and it is not this session's evidence.
#[test]
fn f10_a_receipt_for_another_session_does_not_satisfy_this_one() {
    let f = fixture(
        Some("version: 1\nrequire_receipt: true\nclaims: {}\n"),
        false,
    );
    f.issue_receipt(".thruline/receipts/latest.json");
    let (text, code) = f.policy(&["--conclusion"]);
    assert_eq!(code, 0, "{text}");

    // Rewrite the session id inside the receipt's payload. The envelope stays
    // well-formed and still carries a signature; it simply attests to a
    // different run.
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD;
    let path = f
        .repo
        .join(".thruline")
        .join("receipts")
        .join("latest.json");
    let mut envelope: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
    let payload = String::from_utf8(
        b64.decode(envelope["payload"].as_str().expect("payload"))
            .expect("base64"),
    )
    .expect("utf8");
    let swapped = payload.replace(SESSION, "00000000-1111-4222-8333-444444444444");
    assert_ne!(swapped, payload, "the session id is not in the receipt");
    envelope["payload"] = serde_json::Value::String(b64.encode(swapped));
    std::fs::write(&path, serde_json::to_string(&envelope).expect("ser")).expect("write");

    let (text, code) = f.policy(&["--conclusion"]);
    assert_eq!(
        code, 3,
        "a receipt for a different session satisfied require_receipt: {text}"
    );
}

// ---------------------------------------------------------------------------
// F11 — `max_stop_blocks` is dead on arrival.
// ---------------------------------------------------------------------------

const STOP_GATE: &str = "version: 1\n\
require_receipt: false\n\
claims: {}\n\
stop_gate:\n\
\x20 enabled: true\n\
\x20 max_stop_blocks: 0\n";

/// R's note on this one was *"**Attack.** None needed — the rule is dead on
/// arrival."* `stop_blocks: 0` was a literal in `policy_cmd.rs`, so the
/// comparison against `max_stop_blocks` could not fire for any input.
/// R's note on this one was *"**Attack.** None needed — the rule is dead on
/// arrival."* `stop_blocks: 0` was a literal in `policy_cmd.rs`, so the
/// comparison against `max_stop_blocks` could not fire for any input.
///
/// Since G2 the count comes from the `gate_blocks` table rather than a loose
/// JSON file, so the blocks below are written through the ledger's own
/// appender. Writing them by hand would mean restating the chain hash in a
/// test, which is the shape of assertion this project refuses: it proves
/// somebody typed the constant twice.
#[test]
fn f11_max_stop_blocks_reads_the_real_block_ledger() {
    let f = fixture(Some(STOP_GATE), false);

    // Zero recorded blocks is "could not be evaluated", not "the limit is
    // satisfied" — the second half of F11, asserted in its own test below.
    // Here the interesting case is a ledger that HAS blocks.
    let db = f.home.join("exec.db");
    let ledger = thruline_ledger::Ledger::open(&db).expect("open exec.db");
    for i in 0..2 {
        ledger
            .record_gate_block(SESSION, &format!("key-{i}"), "tests_pass")
            .expect("record a block");
    }

    let (text, code) = f.policy(&["--conclusion"]);
    assert_eq!(
        code, 3,
        "2 stop blocks against a limit of 0 passed the gate: {text}"
    );
}

/// No ledger is **not** a ledger of zero. A gate that cannot see the block
/// count says so — that is §9's neutral, exit 0 and nothing blocked — rather
/// than reporting a limit it never checked as satisfied.
#[test]
fn f11_an_unreadable_block_ledger_is_neutral_not_a_pass() {
    let f = fixture(Some(STOP_GATE), false);
    let (text, code) = f.policy(&["--conclusion"]);
    assert_eq!(code, 0, "{text}");
    assert_eq!(
        text.trim(),
        "neutral",
        "an absent block ledger read as a satisfied limit: {text}"
    );

    // And a store that exists with no rows for this session is the same answer,
    // which is the case the move into a table created: a file's absence used to
    // carry "the gate never wrote here", and an empty result set does not.
    let db = f.home.join("exec.db");
    let ledger = thruline_ledger::Ledger::open(&db).expect("open exec.db");
    drop(ledger);
    let (text, code) = f.policy(&["--conclusion"]);
    assert_eq!(code, 0, "{text}");
    assert_eq!(
        text.trim(),
        "neutral",
        "an empty block ledger read as a satisfied limit: {text}"
    );
}

#[test]
fn f11_a_disabled_stop_gate_reports_nothing() {
    let f = fixture(
        Some("version: 1\nrequire_receipt: false\nclaims: {}\nstop_gate:\n  enabled: false\n  max_stop_blocks: 0\n"),
        false,
    );
    let (text, code) = f.policy(&["--conclusion"]);
    assert_eq!(code, 0, "{text}");
    assert_eq!(text.trim(), "success", "{text}");
}

/// `packaging/action/action.yml` calls `thruline policy --policy "$TL_POLICY"`,
/// and the value a user writes there is a **repository-relative** path. Since
/// `GITHUB_BASE_REF` is now adopted without a flag, that combination is what
/// every pull-request run of the shipped Action takes — so it has to work, and
/// it has to read the base branch's copy rather than erroring or silently
/// falling back to the file the pull request wrote.
#[test]
fn a_relative_policy_path_resolves_against_the_repo_and_still_reads_the_base() {
    let f = fixture(Some(STRICT), false);
    f.issue_receipt(".thruline/receipts/latest.json");
    git(&f.repo, &["checkout", "-q", "-b", "pr"]);
    f.write_policy(LOOSE);
    git(&f.repo, &["add", "-A"]);
    git(&f.repo, &["commit", "-qm", "loosen"]);

    let (text, code) = f.policy(&[
        "--conclusion",
        "--policy",
        ".thruline/policy.yml",
        "--base",
        "main",
    ]);
    assert_eq!(code, 3, "{text}");
    assert_eq!(text.trim(), "failure", "{text}");
}
