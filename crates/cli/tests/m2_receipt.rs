//! M2's done-when: **a receipt signed on one machine verifies on another with
//! only the repo and the receipt.**
//!
//! "Another machine" here is a fresh `git clone` plus a fresh `THRULINE_HOME`
//! containing nothing but the public key — **no ledger, no spool, no shim, no
//! exec-output store**. That is the state a reviewer is actually in, and it is
//! the state the done-when is about.

// These existing cases pin v0.1 compatibility explicitly; session_v2 cases pin the new protocol.
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

struct MachineA {
    _dir: tempfile::TempDir,
    home: PathBuf,
    repo: PathBuf,
    receipt: PathBuf,
}

/// Record a session and issue a signed receipt.
fn machine_a() -> MachineA {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home-a");
    let repo = dir.path().join("repo-a");
    std::fs::create_dir_all(&repo).expect("repo");
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.email", "a@e.invalid"]);
    git(&repo, &["config", "user.name", "A"]);
    std::fs::write(repo.join("app.py"), "def rate(w):\n    return 2.5 * w\n").expect("write");
    std::fs::write(repo.join(".gitignore"), "__pycache__/\n").expect("write");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "first"]);

    let session = "aaaa-m2-done-when";
    let hook = |json: &str| {
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
    hook(&format!(
        r#"{{"hook_event_name":"SessionStart","session_id":"{session}","transcript_path":"/t","cwd":"{}","source":"startup"}}"#,
        repo.display()
    ));
    hook(&format!(
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
    assert!(receipt.is_file());

    MachineA {
        _dir: dir,
        home,
        repo,
        receipt,
    }
}

/// A clone plus a home containing only the public key.
fn machine_b(a: &MachineA, dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let home = dir.join("home-b");
    let repo = dir.join("repo-b");
    std::fs::create_dir_all(&home).expect("home-b");
    let out = Command::new("git")
        .args(["clone", "-q"])
        .arg(&a.repo)
        .arg(&repo)
        .output()
        .expect("clone");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::copy(a.home.join("signing.pub"), home.join("signing.pub")).expect("copy key");
    let receipt = dir.join("receipt-b.json");
    std::fs::copy(&a.receipt, &receipt).expect("copy receipt");
    // Nothing else crosses: no ledger, no spool, no exec-output store.
    assert!(!home.join("exec.db").exists());
    (home, repo, receipt)
}

fn verify(home: &Path, repo: &Path, receipt: &Path, extra: &[&str]) -> (i32, serde_json::Value) {
    let mut args: Vec<&str> = vec!["verify-receipt"];
    let r = receipt.to_string_lossy().to_string();
    args.push(&r);
    args.extend_from_slice(extra);
    args.push("--json");
    let out = Command::new(thruline())
        .args(&args)
        .env("THRULINE_HOME", home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(repo)
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

fn outcomes(report: &serde_json::Value) -> Vec<(String, String)> {
    report["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .map(|c| {
            (
                c["name"].as_str().unwrap_or("?").to_string(),
                c["outcome"].as_str().unwrap_or("?").to_string(),
            )
        })
        .collect()
}

#[test]
fn a_receipt_signed_on_one_machine_verifies_on_another_with_only_the_repo_and_the_receipt() {
    let a = machine_a();
    let dir = tempfile::tempdir().expect("tempdir");
    let (home, repo, receipt) = machine_b(&a, dir.path());

    let (code, report) = verify(&home, &repo, &receipt, &["--offline"]);
    let got = outcomes(&report);

    // The portable core: all confirmed on B by RECOMPUTATION, not by reading
    // the statement.
    for name in [
        "signature",
        "payload_type",
        "statement_schema",
        "subject_tree",
        "subject_commit",
    ] {
        assert_eq!(
            got.iter().find(|(n, _)| n == name).map(|(_, o)| o.as_str()),
            Some("pass"),
            "{name} did not pass on the second machine: {got:?}"
        );
    }
    // And the one thing B genuinely cannot check is reported as unchecked, not
    // rounded up.
    assert_eq!(
        got.iter()
            .find(|(n, _)| n == "chain_head")
            .map(|(_, o)| o.as_str()),
        Some("skipped"),
        "{got:?}"
    );
    assert_eq!(
        code, 2,
        "a partially verified receipt exited {code}; 0 would be success because \
         something was never checked"
    );
}

#[test]
fn the_same_receipt_verifies_completely_on_the_machine_that_issued_it() {
    // The control for the test above: exit 2 must be about B's missing ledger,
    // not about the receipt being unverifiable anywhere.
    let a = machine_a();
    let (code, report) = verify(&a.home, &a.repo, &a.receipt, &["--offline"]);
    let got = outcomes(&report);
    assert!(
        got.iter()
            .all(|(_, o)| o == "pass" || o == "not_applicable"),
        "the issuing machine could not fully verify its own receipt: {got:?}"
    );
    assert_eq!(code, 0, "{got:?}");
}

#[test]
fn a_receipt_carried_to_an_unrelated_repository_is_refused() {
    // Replay. The subject is recomputed locally, so a receipt about another
    // repository's tree fails rather than passing quietly.
    let a = machine_a();
    let dir = tempfile::tempdir().expect("tempdir");
    let other = dir.path().join("other");
    std::fs::create_dir_all(&other).expect("dir");
    git(&other, &["init", "-q", "-b", "main"]);
    git(&other, &["config", "user.email", "o@e.invalid"]);
    git(&other, &["config", "user.name", "O"]);
    std::fs::write(other.join("unrelated.txt"), "nothing to do with it\n").expect("write");
    git(&other, &["add", "-A"]);
    git(&other, &["commit", "-qm", "unrelated"]);

    let home = dir.path().join("home-c");
    std::fs::create_dir_all(&home).expect("home");
    std::fs::copy(a.home.join("signing.pub"), home.join("signing.pub")).expect("key");

    let (code, report) = verify(&home, &other, &a.receipt, &["--offline"]);
    let got = outcomes(&report);
    assert_eq!(
        got.iter()
            .find(|(n, _)| n == "subject_tree")
            .map(|(_, o)| o.as_str()),
        Some("fail"),
        "a receipt about another repository verified here: {got:?}"
    );
    assert_eq!(code, 1);
}

#[test]
fn a_tampered_receipt_is_refused_even_though_only_one_byte_changed() {
    let a = machine_a();
    let dir = tempfile::tempdir().expect("tempdir");
    let (home, repo, receipt) = machine_b(&a, dir.path());

    // Flip a verdict inside the signed payload, and re-encode. The signature
    // now covers different bytes.
    let env: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&receipt).expect("read")).expect("json");
    use base64::Engine as _;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(env["payload"].as_str().expect("payload"))
        .expect("b64");
    let mut statement: serde_json::Value = serde_json::from_slice(&raw).expect("statement");
    statement["predicate"]["session"]["event_count"] = serde_json::json!(9999);
    let mut tampered = env.clone();
    tampered["payload"] = serde_json::json!(base64::engine::general_purpose::STANDARD
        .encode(serde_json::to_vec(&statement).expect("ser")));
    std::fs::write(&receipt, serde_json::to_string(&tampered).expect("ser")).expect("write");

    let (code, report) = verify(&home, &repo, &receipt, &["--offline"]);
    let got = outcomes(&report);
    assert_eq!(
        got.iter()
            .find(|(n, _)| n == "signature")
            .map(|(_, o)| o.as_str()),
        Some("fail"),
        "{got:?}"
    );
    assert_eq!(code, 1);
}

#[test]
fn verification_without_the_public_key_is_not_a_pass() {
    // The failure this whole subsystem is built against, at the CLI: a `verify`
    // that returns success because it never checked.
    let a = machine_a();
    let dir = tempfile::tempdir().expect("tempdir");
    let (home, repo, receipt) = machine_b(&a, dir.path());
    std::fs::remove_file(home.join("signing.pub")).expect("remove key");

    let (code, report) = verify(&home, &repo, &receipt, &["--offline"]);
    let got = outcomes(&report);
    assert_eq!(
        got.iter()
            .find(|(n, _)| n == "signature")
            .map(|(_, o)| o.as_str()),
        Some("skipped"),
        "{got:?}"
    );
    assert_ne!(code, 0, "an unverified signature exited 0");
}

#[test]
fn the_receipt_carries_a_commit_trailer_that_names_its_own_bytes() {
    let a = machine_a();
    let out = Command::new(thruline())
        .args(["receipt", "--trailer"])
        .env("THRULINE_HOME", &a.home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(&a.repo)
        .output()
        .expect("receipt");
    let text = String::from_utf8_lossy(&out.stdout);
    let trailer = text
        .lines()
        .find(|l| l.starts_with("ThruLine-Receipt:"))
        .unwrap_or_else(|| panic!("no trailer: {text}"));
    let digest = trailer.split_whitespace().nth(1).expect("digest");
    assert!(digest.starts_with("sha256:"));
    // And nothing amended a commit: §8.1 says never rewrite one the user did
    // not ask to amend, and this path has no code that could.
    let log = Command::new("git")
        .current_dir(&a.repo)
        .args(["log", "--format=%s"])
        .output()
        .expect("log");
    assert_eq!(String::from_utf8_lossy(&log.stdout).trim(), "first");
}

#[test]
fn the_json_conclusion_agrees_with_the_process_exit_code() {
    // The finding: a consumer using the standard idiom over `checks` reads
    // exit-2 as success. The JSON now states the conclusion, and this asserts
    // the two channels cannot disagree — a `jq` reader and a `$?` reader are
    // looking at the same receipt.
    let a = machine_a();
    let dir = tempfile::tempdir().expect("tempdir");
    let (home, repo, receipt) = machine_b(&a, dir.path());

    // Partial on B.
    let (code, report) = verify(&home, &repo, &receipt, &["--offline"]);
    assert_eq!(report["verified"], serde_json::json!(false));
    assert_eq!(report["status"], serde_json::json!("partial"));
    assert_eq!(report["exit"], serde_json::json!(code));
    assert_eq!(code, 2);
    // And the naive idiom really would have got it wrong.
    let naive = report["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .all(|c| c["outcome"] != "fail");
    assert!(naive, "the premise of the finding no longer holds");

    // Full on A.
    let (code, report) = verify(&a.home, &a.repo, &a.receipt, &["--offline"]);
    assert_eq!(report["verified"], serde_json::json!(true));
    assert_eq!(report["status"], serde_json::json!("verified"));
    assert_eq!(report["exit"], serde_json::json!(code));
    assert_eq!(code, 0);
}

#[test]
fn a_tree_buried_far_back_in_history_still_resolves() {
    // The bound that bit was not the one written down: `--max-count=500`
    // excluded 0% of this repository, while resolving only 200 trees excluded
    // 53.2% — and cost 1,907 ms doing it. Asking the object store about the
    // hash the receipt names is unbounded and costs about 10 ms.
    //
    // So: issue a receipt, then bury its tree under enough history that a
    // per-commit walk with any small window would miss it, and verify again.
    let a = machine_a();

    // The receipt as issued, against the tree at that moment.
    let (code, _) = verify(&a.home, &a.repo, &a.receipt, &["--offline"]);
    assert_eq!(code, 0, "the receipt did not verify when it was fresh");

    for i in 0..40 {
        std::fs::write(a.repo.join(format!("f{i}.txt")), format!("{i}\n")).expect("write");
        git(&a.repo, &["add", "-A"]);
        git(&a.repo, &["commit", "-qm", &format!("c{i}")]);
    }

    // The working tree has moved on, so the receipt's tree is now historical.
    // It is still an object this repository has, and that is the question.
    let (code, report) = verify(&a.home, &a.repo, &a.receipt, &["--offline"]);
    let got = outcomes(&report);
    assert_eq!(
        got.iter()
            .find(|(n, _)| n == "subject_tree")
            .map(|(_, o)| o.as_str()),
        Some("pass"),
        "a tree 40 commits back was reported absent: {got:?}"
    );
    assert_eq!(
        got.iter()
            .find(|(n, _)| n == "subject_commit")
            .map(|(_, o)| o.as_str()),
        Some("pass"),
        "{got:?}"
    );
    // **And the conclusion is 2, not 0, and that is the point of the change
    // that made it so.**
    //
    // The two assertions above are what this test is about: an object 40
    // commits back is still an object this repository has, and asking the store
    // directly finds it. But *having the object* is not *being the code under
    // review*: `head_binding` now says so, and it skips here because the head
    // moved 40 commits past the tree this receipt describes. Exit 2 is
    // "something could not be checked", which is exactly true — a receipt about
    // a tree 40 commits back does not describe this head, and the previous 0
    // said it did.
    assert_eq!(code, 2, "{got:?}");
    assert_eq!(
        got.iter()
            .find(|(n, _)| n == "head_binding")
            .map(|(_, o)| o.as_str()),
        Some("skipped"),
        "{got:?}"
    );
}

#[test]
fn a_receipt_naming_an_object_this_repository_does_not_have_is_refused() {
    // The control for the test above: "unbounded" must not mean "always
    // resolves".
    let a = machine_a();
    let receipt: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&a.receipt).expect("read")).expect("json");
    use base64::Engine as _;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(receipt["payload"].as_str().expect("payload"))
        .expect("b64");
    let mut statement: serde_json::Value = serde_json::from_slice(&raw).expect("statement");
    statement["subject"][0]["digest"]["gitTree"] =
        serde_json::json!("1111111111111111111111111111111111111111");
    let mut tampered = receipt.clone();
    tampered["payload"] = serde_json::json!(base64::engine::general_purpose::STANDARD
        .encode(serde_json::to_vec(&statement).expect("ser")));

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bogus.json");
    std::fs::write(&path, serde_json::to_string(&tampered).expect("ser")).expect("write");
    let (_, report) = verify(&a.home, &a.repo, &path, &["--offline"]);
    let got = outcomes(&report);
    // The signature fails too (the payload changed), which is correct — the
    // point here is that the bogus object does not resolve.
    assert_ne!(
        got.iter()
            .find(|(n, _)| n == "subject_tree")
            .map(|(_, o)| o.as_str()),
        Some("pass"),
        "a hash no object store has resolved: {got:?}"
    );
}
