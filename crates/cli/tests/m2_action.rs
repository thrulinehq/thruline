//! M2.6 — the GitHub Action, the check run, the PR table and the badge (§9).
//!
//! # The mechanical control, and the finding behind it
//!
//! A `--json` consumer using the standard idiom `all(c.outcome != "fail")` was
//! measured reading **exit 2 as success**: a partially verified receipt
//! contains no failing check, so the naive predicate says yes while the process
//! says "partial". A check run reporting "verified" for a partially verified
//! receipt is §6's `unverifiable` → `supported` collapse on the most public
//! surface the product has.
//!
//! The fix is that the conclusion is emitted, not derived. This file is the
//! control that keeps it that way: it walks `packaging/action/action.yml` and
//! fails if the Action ever reads `.checks`, `.outcome` or an exit code to
//! decide an outcome — a written instruction not to would not survive the next
//! edit.
//!
//! # Everything else here runs the real binary
//!
//! The renderings are produced by `thruline verify-receipt --format …` against
//! a real receipt on a real repository, and the check run's conclusion is
//! compared with the process's own exit code.

use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "support/qualified_binary.rs"]
mod qualified_binary;
use qualified_binary::thruline;

/// Is this line a YAML key rather than shell text?
///
/// Deliberately strict: `MSG="$MSG Add 'with: {fetch-depth: 0}'…"` contains a
/// colon and is not a key, and a loose test that accepted it is what made the
/// first version of the block-scalar guard vacuous — proven by breaking the
/// file and watching the guard stay green.
fn looks_like_a_yaml_key(line: &str) -> bool {
    let t = line.trim_start();
    let t = t.strip_prefix("- ").unwrap_or(t);
    let Some(colon) = t.find(':') else {
        return false;
    };
    let (name, rest) = t.split_at(colon);
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        && (rest.starts_with(": ") || rest.trim_end() == ":")
}

fn action_yaml() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging/action/action.yml");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
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
    _dir: tempfile::TempDir,
    home: PathBuf,
    repo: PathBuf,
    receipt: PathBuf,
}

/// A repository with a session and a signed receipt.
fn fixture() -> Fixture {
    fixture_version("0.1")
}

fn fixture_version(version: &str) -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).expect("repo");
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.email", "a@e.invalid"]);
    git(&repo, &["config", "user.name", "A"]);
    std::fs::write(repo.join("app.py"), "def rate(w):\n    return 2.5 * w\n").expect("w");
    std::fs::write(repo.join(".gitignore"), "__pycache__/\n").expect("w");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "first"]);

    let session = "77777777-8888-4999-8aaa-bbbbbbbbbbbb";
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
        r#"{{"hook_event_name":"Stop","session_id":"{session}","transcript_path":"/t","cwd":"{}","last_assistant_message":"All tests pass.","stop_hook_active":false}}"#,
        repo.display()
    ));
    let out = Command::new(thruline())
        .args(["fold"])
        .env("THRULINE_HOME", &home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(&repo)
        .output()
        .expect("fold");
    assert!(out.status.success());

    let receipt = dir.path().join("receipt.json");
    let out = Command::new(thruline())
        .args(["receipt", "--predicate-version", version, "--out"])
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
    fn verify(&self, format: &str, extra: &[&str]) -> std::process::Output {
        let mut args: Vec<String> = vec![
            "verify-receipt".into(),
            self.receipt.to_string_lossy().to_string(),
            "--offline".into(),
            "--format".into(),
            format.into(),
        ];
        args.extend(extra.iter().map(|s| (*s).to_string()));
        Command::new(thruline())
            .args(&args)
            .env("THRULINE_HOME", &self.home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("GITHUB_SHA")
            .current_dir(&self.repo)
            .output()
            .expect("verify")
    }
}

// ---------------------------------------------------------------------------
// The mechanical control over the Action itself.
// ---------------------------------------------------------------------------

#[test]
fn the_action_consumes_the_conclusion_and_never_re_derives_one() {
    let yaml = action_yaml();

    // It must read the conclusion the verifier emitted.
    assert!(
        yaml.contains("--format check-run"),
        "the action no longer asks for the check-run rendering"
    );
    assert!(
        yaml.contains(r#"jq -r '.conclusion' check-run.json"#),
        "the action no longer reads `.conclusion` from the verifier's own output"
    );

    // And it must NOT reconstruct one. Each of these is a way the collapse
    // re-enters: reading the checks array, testing outcomes, or branching on
    // the exit code.
    //
    // The walk is over the RUNNING lines only. Comments in this file explain
    // the forbidden idioms by name, and a census that could not tell a comment
    // from a command would force the explanation out of the file it is about.
    let running: String = yaml
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in [
        ".checks",
        ".outcome",
        "outcome !=",
        "outcome ==",
        "!= \"fail\"",
        "select(.outcome",
        "$?",
        "map(.outcome",
    ] {
        assert!(
            !running.contains(forbidden),
            "packaging/action/action.yml runs {forbidden:?}. The Action must consume \
             the conclusion `thruline verify-receipt --format check-run` emits and never \
             derive one: `all(c.outcome != \"fail\")` reads a partially verified receipt \
             as verified, which is §6's non-collapse rule undone on the most public \
             surface the product has."
        );
    }

    // The shallow-clone warning, because `actions/checkout` defaults to 1 and
    // the objects a receipt names are not in a shallow clone.
    assert!(
        yaml.contains("--is-shallow-repository"),
        "the action does not detect a shallow clone"
    );
    assert!(
        yaml.contains("fetch-depth: 0"),
        "the action never names the setting that fixes a shallow clone"
    );

    // And it is valid YAML-ish enough that the required keys are present.
    for key in ["runs:", "using: 'composite'", "check-runs", "gh pr comment"] {
        assert!(yaml.contains(key), "the action is missing {key:?}");
    }

    // A structural check for the one YAML defect this file actually hit: a
    // shell line inside a `run: |` block indented LESS than the block, which
    // ends the block scalar early and makes the whole document unparseable —
    // with no Rust test noticing, because every substring assertion above
    // still passes on a broken file.
    let mut block_indent: Option<usize> = None;
    for (i, line) in yaml.lines().enumerate() {
        let indent = line.len() - line.trim_start().len();
        if let Some(want) = block_indent {
            if line.trim().is_empty() {
                continue;
            }
            if indent < want {
                // The block ended. A key at this level is fine; anything else
                // is shell text that fell out of its block.
                assert!(
                    looks_like_a_yaml_key(line),
                    "line {} fell out of a `run:` block — it is indented {indent} where the \
                     block is {want}, which ends the YAML block scalar early and makes the \
                     document unparseable: {line:?}",
                    i + 1
                );
                block_indent = None;
            }
            continue;
        }
        let trimmed = line.trim_end();
        if trimmed.ends_with("run: |") || trimmed.ends_with("description: >-") {
            block_indent = Some(indent + 2);
        }
    }

    // The documented usage says `fetch-depth: 0` too, so a user copying the
    // snippet gets a green check run rather than a neutral one.
    let readme = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging/action/README.md"),
    )
    .expect("README");
    assert!(readme.contains("fetch-depth: 0"), "{readme}");
}

// ---------------------------------------------------------------------------
// The renderings, from the real binary.
// ---------------------------------------------------------------------------

#[test]
fn the_check_run_conclusion_agrees_with_the_process_exit_code() {
    // **`fixture()` is deliberately NOT used here any more.** Its agent says
    // "All tests pass." with no test check in the ledger, so its only verdict
    // is `unverifiable` — and this test used to assert that receipt was a
    // green check run at exit 0. That is §6's collapse, standing in the
    // product's own Action fixture: `unverifiable` rendered as verified.
    // The green path needs a receipt whose claim was actually decided.
    let f = fixture_with_message("Added the rate function.");
    let out = f.verify("check-run", &["--head-sha", "cafebabe"]);
    let payload: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("a check-run payload");
    assert_eq!(payload["name"], "thruline/receipt");
    assert_eq!(payload["status"], "completed");
    assert_eq!(payload["head_sha"], "cafebabe");

    // This receipt is verified on the machine that issued it: the ledger is
    // present, so nothing is skipped.
    assert_eq!(out.status.code(), Some(0), "{payload:#}");
    assert_eq!(payload["conclusion"], "success");

    // And the same receipt with the ledger out of reach is PARTIAL — which the
    // naive idiom would have read as verified, because it contains no failing
    // check.
    let elsewhere = tempfile::tempdir().expect("tempdir");
    let out = Command::new(thruline())
        .args([
            "verify-receipt",
            &f.receipt.to_string_lossy(),
            "--offline",
            "--format",
            "check-run",
            "--exec-db",
            &elsewhere.path().join("nothing.db").to_string_lossy(),
        ])
        .env("THRULINE_HOME", &f.home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(&f.repo)
        .output()
        .expect("verify");
    let payload: serde_json::Value = serde_json::from_slice(&out.stdout).expect("payload");
    assert_eq!(out.status.code(), Some(2), "{payload:#}");
    assert_eq!(
        payload["conclusion"], "neutral",
        "a partially verified receipt reported a conclusion other than neutral: {payload:#}"
    );
    assert_ne!(payload["conclusion"], "success");
    assert!(
        payload["output"]["title"]
            .as_str()
            .is_some_and(|t| t.contains("PARTIALLY")),
        "{payload:#}"
    );
}

#[test]
fn the_pr_table_names_every_check_and_every_claim() {
    let f = fixture();
    let out = f.verify("md", &[]);
    let md = String::from_utf8_lossy(&out.stdout);
    assert!(md.contains("### ThruLine"), "{md}");
    assert!(md.contains("| check | outcome | detail |"), "{md}");
    for name in [
        "signature",
        "payload_type",
        "statement_schema",
        "subject_tree",
    ] {
        assert!(
            md.contains(&format!("`{name}`")),
            "{name} missing from {md}"
        );
    }
    // The claim the agent made is in the table, with its verdict — and inside a
    // code span, so an agent that wrote a link or an image into its final
    // message cannot make the PR comment fetch anything on a reader's behalf.
    assert!(md.contains("`All tests pass.`"), "{md}");
    assert!(
        md.contains("unverifiable") || md.contains("unsupported") || md.contains("supported"),
        "no verdict in {md}"
    );
}

#[test]
fn the_badge_has_three_states_and_only_one_says_verified() {
    // Same reason as above: a receipt whose only verdict is `unverifiable`
    // must not be able to produce a `verified` badge, so the green state is
    // exercised with a receipt whose claim was decided.
    let f = fixture_with_message("Added the rate function.");
    let out = f.verify("badge", &[]);
    let svg = String::from_utf8_lossy(&out.stdout);
    assert!(svg.starts_with("<svg"), "{svg}");
    assert_eq!(out.status.code(), Some(0));
    assert!(svg.contains(">verified<"), "{svg}");

    // Partial, from the same receipt with the ledger out of reach.
    let elsewhere = tempfile::tempdir().expect("tempdir");
    let out = Command::new(thruline())
        .args([
            "verify-receipt",
            &f.receipt.to_string_lossy(),
            "--offline",
            "--format",
            "badge",
            "--exec-db",
            &elsewhere.path().join("nothing.db").to_string_lossy(),
        ])
        .env("THRULINE_HOME", &f.home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(&f.repo)
        .output()
        .expect("verify");
    let svg = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        !svg.contains(">verified<"),
        "a partially verified receipt produced a `verified` badge: {svg}"
    );
    assert!(svg.contains(">partial<"), "{svg}");

    // Self-contained: a README that renders this fetches nothing, so it cannot
    // report who read it and when (§0.8).
    for forbidden in ["<script", "<image", "@import", "xlink:href"] {
        assert!(!svg.contains(forbidden), "{svg}");
    }
}

#[test]
fn an_unknown_format_is_an_error_rather_than_a_default() {
    let f = fixture();
    let out = f.verify("html", &[]);
    assert_ne!(out.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("unknown --format"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_shallow_clone_is_neutral_and_says_how_to_fix_it() {
    // The finding: `actions/checkout` defaults to `fetch-depth: 1`, so the
    // Action as commonly configured would report a red check run on entirely
    // honest receipts. End to end here, with a real shallow clone.
    let f = fixture();
    // Move the repository on, so the commit and tree the receipt names are
    // BEHIND the tip. A depth-1 clone then genuinely lacks them, which is the
    // state `actions/checkout`'s default produces on any PR with history.
    std::fs::write(f.repo.join("app.py"), "def rate(w):\n    return 3.0 * w\n").expect("w");
    git(&f.repo, &["add", "-A"]);
    git(&f.repo, &["commit", "-qm", "second"]);

    let dir = tempfile::tempdir().expect("tempdir");
    let shallow = dir.path().join("shallow");
    let out = Command::new("git")
        .args(["clone", "-q", "--depth", "1"])
        .arg(format!("file://{}", f.repo.display()))
        .arg(&shallow)
        .output()
        .expect("clone");
    if !out.status.success() {
        eprintln!(
            "SKIPPED: this git refuses a local shallow clone ({}) — the unit-level \
             coverage is in crates/attest/tests/adversary.rs",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return;
    }
    assert_eq!(
        Command::new("git")
            .current_dir(&shallow)
            .args(["rev-parse", "--is-shallow-repository"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default(),
        "true",
        "the clone is not shallow, so this test proves nothing"
    );

    // A home with only the public key, as a reviewer has.
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).expect("home");
    std::fs::copy(f.home.join("signing.pub"), home.join("signing.pub")).expect("key");

    let out = Command::new(thruline())
        .args([
            "verify-receipt",
            &f.receipt.to_string_lossy(),
            "--offline",
            "--format",
            "check-run",
        ])
        .env("THRULINE_HOME", &home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(&shallow)
        .output()
        .expect("verify");
    let payload: serde_json::Value = serde_json::from_slice(&out.stdout).expect("payload");
    assert_ne!(
        payload["conclusion"], "failure",
        "a shallow clone failed an honest receipt — the exact damage this guards: {payload:#}"
    );
    assert_ne!(payload["conclusion"], "success");
    let text = payload["output"]["text"].as_str().unwrap_or_default();
    let summary = payload["output"]["summary"].as_str().unwrap_or_default();
    assert!(
        text.contains("fetch-depth: 0") || summary.contains("fetch-depth: 0"),
        "the reviewer is not told what to change: {payload:#}"
    );
    assert!(
        text.contains("skipped"),
        "the subject checks were not skipped, so the shallow branch never ran: {payload:#}"
    );

    // **The control.** A FULL clone of the same moved-on repository has the
    // objects and must report `success` — so the neutral above is the shallow
    // depth and not a receipt this repository genuinely cannot back.
    let full = dir.path().join("full");
    let out = Command::new("git")
        .args(["clone", "-q"])
        .arg(format!("file://{}", f.repo.display()))
        .arg(&full)
        .output()
        .expect("clone");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = Command::new(thruline())
        .args([
            "verify-receipt",
            &f.receipt.to_string_lossy(),
            "--offline",
            "--format",
            "check-run",
        ])
        .env("THRULINE_HOME", &home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(&full)
        .output()
        .expect("verify");
    let payload: serde_json::Value = serde_json::from_slice(&out.stdout).expect("payload");
    let text = payload["output"]["text"].as_str().unwrap_or_default();
    assert!(
        text.contains("`subject_commit` | ok"),
        "the full clone did not resolve the commit, so the shallow comparison is not \
         about depth: {payload:#}"
    );
    assert!(
        !text.contains("fetch-depth"),
        "a full clone was told to deepen: {payload:#}"
    );
}

#[test]
fn the_receipt_records_the_git_environment_it_was_produced_under() {
    // BUILD-PLAN C.7's S3 and §8.1's `capture.git_env_digest`. A meaningful
    // part of C.7's attack surface — `.git/info/exclude`, out-of-tree
    // attributes, sparse checkout, a filesystem monitor — is OUTSIDE the tree,
    // so no diff-based detector can reach it. The digest is how those
    // conditions reach a reviewer holding only the receipt.
    let f = fixture();
    let statement = {
        let out = Command::new(thruline())
            .args(["receipt", "--predicate-version", "0.1", "--json"])
            .env("THRULINE_HOME", &f.home)
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .current_dir(&f.repo)
            .output()
            .expect("receipt");
        let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
        let payload = envelope["payload"].as_str().expect("payload");
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .expect("base64");
        serde_json::from_slice::<serde_json::Value>(&bytes).expect("statement")
    };
    let digest = statement["predicate"]["capture"]["git_env_digest"]
        .as_str()
        .unwrap_or_else(|| panic!("no git_env_digest: {statement:#}"));
    assert!(digest.starts_with("sha256:"), "{digest}");

    // And it MOVES when a condition changes — including one that leaves no
    // diff at all, which is the case the field exists for.
    std::fs::create_dir_all(f.repo.join(".git/info")).expect("mkdir");
    std::fs::write(f.repo.join(".git/info/exclude"), "hidden.py\n").expect("w");
    let out = Command::new(thruline())
        .args(["receipt", "--predicate-version", "0.1", "--json"])
        .env("THRULINE_HOME", &f.home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(&f.repo)
        .output()
        .expect("receipt");
    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(envelope["payload"].as_str().expect("payload"))
        .expect("base64");
    let after: serde_json::Value = serde_json::from_slice(&bytes).expect("statement");
    assert_ne!(
        after["predicate"]["capture"]["git_env_digest"].as_str(),
        Some(digest),
        "a change to .git/info/exclude — which leaves NO diff — did not move the digest"
    );
}

#[test]
fn an_agents_final_message_cannot_put_markup_in_the_pr_comment() {
    // End to end, through the real binary: the agent's sentence carries an
    // image reference, and the rendered table must contain it as TEXT. An image
    // in a PR comment is a request made by whoever opens the pull request —
    // it tells a third party who read it and when, which is exactly what §0.8
    // forbids and what `explain --html` already escapes against.
    // The claim's text is the SENTENCE the phrase was found in, so the hostile
    // markup has to sit inside that sentence to reach the table at all — which
    // is also the shape an agent would use.
    // No full stops in the URL: the extractor's sentence splitter would cut it,
    // which is an accident of that splitter and not a defence to rely on.
    let f =
        fixture_with_message("After ![](http://localhost:9/pixel) the refactor, all tests pass");
    let out = f.verify("md", &[]);
    let md = String::from_utf8_lossy(&out.stdout);
    assert!(
        md.contains("localhost:9/pixel"),
        "the claim is missing entirely, so this proves nothing: {md}"
    );
    // Every line carrying the hostile text is a table row whose cell is a code
    // span, so nothing in it is parsed as markdown.
    for line in md.lines().filter(|l| l.contains("localhost:9/pixel")) {
        let cell = line
            .split('|')
            .find(|c| c.contains("localhost:9/pixel"))
            .unwrap_or(line)
            .trim();
        assert!(
            cell.starts_with('`') && cell.ends_with('`'),
            "agent text is not inside a code span: {line}"
        );
    }
}

/// The same fixture, with the agent's final message under the caller's control.
fn fixture_with_message(message: &str) -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).expect("repo");
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.email", "a@e.invalid"]);
    git(&repo, &["config", "user.name", "A"]);
    std::fs::write(repo.join("app.py"), "x = 1\n").expect("w");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-qm", "first"]);

    let session = "88888888-9999-4aaa-8bbb-cccccccccccc";
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
        r#"{{"hook_event_name":"Stop","session_id":"{session}","transcript_path":"/t","cwd":"{}","last_assistant_message":{},"stop_hook_active":false}}"#,
        repo.display(),
        serde_json::Value::String(message.to_string())
    ));
    let out = Command::new(thruline())
        .args(["fold"])
        .env("THRULINE_HOME", &home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(&repo)
        .output()
        .expect("fold");
    assert!(out.status.success());
    let receipt = dir.path().join("receipt.json");
    let out = Command::new(thruline())
        .args(["receipt", "--predicate-version", "0.1", "--out"])
        .arg(&receipt)
        .env("THRULINE_HOME", &home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(&repo)
        .output()
        .expect("receipt");
    assert!(out.status.success());
    Fixture {
        _dir: dir,
        home,
        repo,
        receipt,
    }
}

/// The receipt this suite's `fixture()` produces — an agent claiming *"All
/// tests pass."* with **no test check anywhere in the ledger** — must never
/// render as verified.
///
/// It did. Measured 2026-09-04: `verify-receipt` exited 0, the check run said
/// `success`, the badge said `verified`, and the text output printed
/// *"Verified. Every check the receipt asserts was confirmed here."* over a
/// predicate whose single verdict read
/// `unverifiable — no test check was captured in this session`.
///
/// Two tests in this file asserted that exit 0. They were the bug, written
/// down as an expectation.
#[test]
fn a_receipt_whose_only_verdict_is_unverifiable_is_never_green() {
    let f = fixture();

    let out = f.verify("text", &[]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        !text.contains("Verified. Every check"),
        "the verified sentence printed over an unverifiable verdict: {text}"
    );
    assert_ne!(out.status.code(), Some(0), "{text}");

    let out = f.verify("check-run", &["--head-sha", "cafebabe"]);
    let payload: serde_json::Value = serde_json::from_slice(&out.stdout).expect("check-run");
    assert_eq!(payload["conclusion"], "neutral", "{payload:#}");

    let out = f.verify("badge", &[]);
    let svg = String::from_utf8_lossy(&out.stdout);
    assert!(!svg.contains(">verified<"), "{svg}");
}

// ---------------------------------------------------------------------------
// The operator's ruling of 2026-09-04, end to end through the real binary.
//
// The defect, measured at 10dae44: a receipt whose only claim was decided
// `unsupported` produced `conclusion: "success"` and a green `verified` badge,
// because the check run posted the INTEGRITY axis and nothing else. The
// receipt was genuine; the claim inside it was not. `success` on a pull
// request does not read as "the signature verified".
//
// The unit-level half is `crates/attest/tests/ruling_check_run.rs`. This half
// runs the shipped binary, because the last four defects in this area were
// each found by running something that had only ever been read.
// ---------------------------------------------------------------------------

/// The same fixture receipt, re-signed with one verdict word replaced.
///
/// Re-signed with the fixture's own key, so the receipt is **valid**: this is
/// an honest receipt about a session in which a claim went unproved, not a
/// forgery. That is the whole point — the integrity axis stays green.
fn receipt_with_verdict(f: &Fixture, verdict: &str) -> PathBuf {
    use base64::Engine;
    let env: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&f.receipt).expect("read")).expect("json");
    let mut statement: serde_json::Value = serde_json::from_slice(
        &base64::engine::general_purpose::STANDARD
            .decode(env["payload"].as_str().expect("payload"))
            .expect("b64"),
    )
    .expect("statement");
    let verdicts = statement["predicate"]["verdicts"]
        .as_array_mut()
        .expect("verdicts");
    assert!(
        !verdicts.is_empty(),
        "the fixture receipt carries no verdict, so this test would prove nothing"
    );
    for v in verdicts.iter_mut() {
        v["verdict"] = serde_json::Value::String(verdict.to_string());
    }
    // **This helper forges. It is named for what it renders, and it is worth
    // saying plainly what it now is.**
    //
    // It flips a verdict word on a real receipt and re-signs with the fixture's
    // own key — the F2 attack exactly. `verify` re-derives every `supported`
    // verdict from the evidence beside it, and this session recorded no check,
    // so a flip to `supported` is now reported as un-re-derivable and can never
    // be green again. That is correct, and it means the **green control** in
    // these tests can no longer come from here: it comes from `f.receipt`, the
    // fixture's own unmodified receipt, whose session makes no claim at all.
    //
    // An earlier version of this helper injected a synthetic passing check so
    // the flip would re-derive. That was building the forgery a better forgery
    // — and `check_provenance` then caught it, because the ledger has no such
    // row. The fixture was made honest instead.
    let payload = serde_json::to_vec(&statement).expect("serialise");
    let key = thruline_attest::key::load_signing(&f.home.join("signing.key")).expect("key");
    let envelope = thruline_attest::dsse::sign(
        &payload,
        thruline_attest::statement::PAYLOAD_TYPE,
        &key,
        Some(thruline_attest::dsse::key_id(&key.verifying_key())),
    );
    let out = f.repo.join(format!("receipt-{verdict}.json"));
    std::fs::write(&out, serde_json::to_string_pretty(&envelope).expect("json")).expect("write");
    out
}

fn verify_receipt(
    f: &Fixture,
    receipt: &Path,
    format: &str,
    policy: Option<&str>,
) -> std::process::Output {
    let mut cmd = Command::new(thruline());
    cmd.args([
        "verify-receipt",
        &receipt.to_string_lossy(),
        "--offline",
        "--format",
        format,
    ])
    .env("THRULINE_HOME", &f.home)
    .env_remove("CLAUDE_CODE_SESSION_ID")
    .env_remove("GITHUB_SHA")
    .env_remove("THRULINE_POLICY_CONCLUSION")
    .current_dir(&f.repo);
    if let Some(p) = policy {
        cmd.env("THRULINE_POLICY_CONCLUSION", p);
    }
    cmd.output().expect("verify")
}

#[test]
fn ruling_1_verify_stays_integrity_only_over_an_unsupported_claim() {
    // The ruling confirms this: `verify` means "the receipt's checks reproduce
    // here", and an honestly recorded `unsupported` verdict reproduces
    // perfectly. Its EXIT CODE must not move — a gate reads `conclusion`.
    let f = fixture();
    let r = receipt_with_verdict(&f, "unsupported");
    let out = verify_receipt(&f, &r, "json", None);
    let j: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(
        out.status.code(),
        Some(0),
        "`unsupported` began blocking `verify`, which the ruling forbids: {j:#}"
    );
    assert_eq!(j["exit"], 0, "{j:#}");
    assert_eq!(j["integrity"]["status"], "verified", "{j:#}");
    assert_eq!(j["integrity"]["exit"], 0, "{j:#}");
}

#[test]
fn ruling_2_and_4_an_unsupported_claim_is_neutral_with_its_table_and_an_amber_badge() {
    let f = fixture();
    let r = receipt_with_verdict(&f, "unsupported");

    // 2: never `success`.
    let out = verify_receipt(&f, &r, "check-run", None);
    let payload: serde_json::Value = serde_json::from_slice(&out.stdout).expect("payload");
    assert_ne!(
        payload["conclusion"], "success",
        "the Action would post `success` over an unsupported claim: {payload:#}"
    );
    // 4: neutral, with the verdict table rendered.
    assert_eq!(payload["conclusion"], "neutral", "{payload:#}");
    let text = payload["output"]["text"].as_str().unwrap_or_default();
    assert!(text.contains("| claim | verdict | why |"), "{payload:#}");
    assert!(text.contains("**unsupported**"), "{payload:#}");
    assert!(
        payload["output"]["title"]
            .as_str()
            .is_some_and(|t| t.contains("NOT supported")),
        "the title still speaks only of the receipt: {payload:#}"
    );
    assert!(
        payload["output"]["summary"]
            .as_str()
            .is_some_and(|s| s.contains("NOT supported")),
        "{payload:#}"
    );

    // The badge is a rendering of the SAME conclusion, and it is observed —
    // a badge that only ever shows green is a decoration.
    let out = verify_receipt(&f, &r, "badge", None);
    let svg = String::from_utf8_lossy(&out.stdout);
    assert!(
        !svg.contains(">verified<"),
        "an unsupported claim produced a `verified` badge: {svg}"
    );
    assert!(svg.contains(">partial<"), "{svg}");
    assert!(svg.contains("#bf8700"), "the badge is not amber: {svg}");

    // **The green control, and it had to move.** It used to be
    // `receipt_with_verdict(&f, "supported")` — the SAME forged receipt with a
    // different word in it — which proved only that the renderer reads the
    // word. `verify` now re-derives every `supported` verdict from the evidence
    // beside it, and this fixture's session recorded no check, so that forgery
    // is reported as un-re-derivable and is neutral too. The control is now a
    // receipt from a session that makes NO claim, which is genuinely green —
    // `fixture()`'s own agent says "All tests pass." over a session with no
    // test check, so its honest verdict is `unverifiable` and it was never
    // green either.
    let g = fixture_with_message("Added the rate function.");
    let payload: serde_json::Value =
        serde_json::from_slice(&verify_receipt(&g, &g.receipt, "check-run", None).stdout)
            .expect("json");
    assert_eq!(payload["conclusion"], "success", "{payload:#}");
    let green = verify_receipt(&g, &g.receipt, "badge", None);
    let svg = String::from_utf8_lossy(&green.stdout);
    assert!(svg.contains(">verified<"), "{svg}");
    assert!(svg.contains("#2ea043"), "{svg}");

    // And the forged flip is NOT green, which is the finding that moved the
    // control. Asserted here so the move is a guard rather than a retreat.
    let forged = receipt_with_verdict(&f, "supported");
    let payload: serde_json::Value =
        serde_json::from_slice(&verify_receipt(&f, &forged, "check-run", None).stdout)
            .expect("json");
    assert_ne!(
        payload["conclusion"], "success",
        "a verdict flipped to `supported` and re-signed rendered green: {payload:#}"
    );
}

#[test]
fn ruling_3_and_5_failure_comes_only_from_policy() {
    let f = fixture();
    let ok = receipt_with_verdict(&f, "supported");
    let bad = receipt_with_verdict(&f, "unsupported");

    // 5: with no policy, NOTHING about the claims can reach `failure`.
    for receipt in [&ok, &bad] {
        let payload: serde_json::Value =
            serde_json::from_slice(&verify_receipt(&f, receipt, "check-run", None).stdout)
                .expect("json");
        assert_ne!(payload["conclusion"], "failure", "{payload:#}");
    }

    // 3: with a policy present, the conclusion comes from the policy — and the
    // red badge with it.
    let payload: serde_json::Value =
        serde_json::from_slice(&verify_receipt(&f, &ok, "check-run", Some("failure")).stdout)
            .expect("json");
    assert_eq!(payload["conclusion"], "failure", "{payload:#}");
    assert!(
        payload["output"]["title"]
            .as_str()
            .is_some_and(|t| t.contains("Policy FAILED")),
        "{payload:#}"
    );
    let red = verify_receipt(&f, &ok, "badge", Some("failure"));
    let svg = String::from_utf8_lossy(&red.stdout);
    assert!(svg.contains(">not verified<"), "{svg}");
    assert!(svg.contains("#cf222e"), "{svg}");

    // A passing policy cannot LIFT an unsupported claim: the combination is
    // worst-of, never last-writer-wins.
    let payload: serde_json::Value =
        serde_json::from_slice(&verify_receipt(&f, &bad, "check-run", Some("success")).stdout)
            .expect("json");
    assert_eq!(
        payload["conclusion"], "neutral",
        "a passing policy greened an unsupported claim: {payload:#}"
    );

    // A word the binary does not understand is REFUSED, not ignored. A gate
    // that reads an unrecognised conclusion as `fine` passes when it breaks.
    let out = verify_receipt(&f, &ok, "check-run", Some("passed"));
    assert_ne!(out.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("not one of success, neutral or failure"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn ruling_6_verify_json_states_the_verdict_distribution() {
    let f = fixture();
    let bad = receipt_with_verdict(&f, "unsupported");
    let j: serde_json::Value =
        serde_json::from_slice(&verify_receipt(&f, &bad, "json", None).stdout).expect("json");

    // The answer, stated. Not derivable from `checks`, and not inferable from
    // the absence of a failing one.
    assert_eq!(j["conclusion"], "neutral", "{j:#}");
    assert_eq!(j["verified"], serde_json::json!(false), "{j:#}");
    assert_eq!(j["status"], "partial", "{j:#}");

    // The distribution, complete and partitioned.
    let c = &j["claims"];
    assert!(c.is_object(), "no top-level verdict summary: {j:#}");
    let n = |k: &str| {
        c[k].as_u64()
            .unwrap_or_else(|| panic!("{k} missing: {j:#}"))
    };
    assert_eq!(
        n("supported") + n("unsupported") + n("unverifiable") + n("other"),
        n("total"),
        "the counts do not partition the total: {j:#}"
    );
    assert!(n("total") > 0, "{j:#}");
    assert_eq!(n("unsupported"), n("total"), "{j:#}");
    assert_eq!(c["all_supported"], serde_json::json!(false), "{j:#}");
    assert!(
        c["unsupported_claim_ids"]
            .as_array()
            .is_some_and(|a| a.len() as u64 == n("unsupported")),
        "the unsupported claims are counted but not named: {j:#}"
    );

    // The axes are named, so a reader can see WHICH one produced the answer.
    assert_eq!(j["integrity"]["status"], "verified", "{j:#}");
    assert_eq!(j["policy"]["evaluated"], serde_json::json!(false), "{j:#}");
    assert!(
        j["checks"].as_array().is_some_and(|a| !a.is_empty()),
        "{j:#}"
    );

    // **The naive idiom, computed here, so the divergence is visible rather
    // than asserted.** `all(c.outcome != "fail")` — the reading this whole
    // surface has been undone by twice — still says yes. The stated
    // conclusion does not, and `claims.all_supported` says why in one word.
    let naive_says_ok = j["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .all(|c| c["outcome"] != "fail");
    assert!(
        naive_says_ok,
        "the premise of the finding no longer holds: {j:#}"
    );

    // **The green control, so the fields are not simply always pessimistic.**
    // The fixture's own receipt, not a verdict flipped to `supported` — see the
    // note on `receipt_with_verdict`: that forgery is no longer green, and
    // `all_supported` over a receipt with zero claims is correctly `false`,
    // because a receipt with nothing in it has proved nothing.
    let g = fixture_with_message("Added the rate function.");
    let j: serde_json::Value =
        serde_json::from_slice(&verify_receipt(&g, &g.receipt, "json", None).stdout).expect("json");
    assert_eq!(j["conclusion"], "success", "{j:#}");
    assert_eq!(j["verified"], serde_json::json!(true), "{j:#}");
    assert_eq!(j["claims"]["total"], serde_json::json!(0), "{j:#}");
    assert_eq!(
        j["claims"]["all_supported"],
        serde_json::json!(false),
        "a receipt with no claims has not proved anything: {j:#}"
    );
}

/// **The Action's own state harness, run.**
///
/// `packaging/action/selftest.py` executes the step bodies out of `action.yml`
/// over really-signed receipts and watches every conclusion and every badge
/// state the Action can produce. Until now the only thing that ran it was
/// `.github/workflows/receipt.yml`, **which has never executed on GitHub** —
/// and the harness was found RED at `10dae44`: its scenario 1 still expected
/// `conclusion: success` over a receipt whose only verdict was `unverifiable`,
/// an expectation `crates/cli/tests/m2_action.rs` had already abandoned three
/// commits earlier. A guard nobody runs is a guard that goes stale silently,
/// which is this program's most-repeated finding.
///
/// So `cargo test` runs it too, on any machine that has the two tools it needs.
/// The skip is printed and names what was not checked; it never asserts.
#[test]
fn the_action_state_harness_runs_and_every_state_is_observed() {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging/action/selftest.py");
    assert!(script.is_file(), "{} is missing", script.display());

    let have = |argv: &[&str]| {
        Command::new(argv[0])
            .args(&argv[1..])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    // `pytest` is not optional decoration: without a real runner the harness's
    // cheating-session scenarios cannot run, and it reports that as a FAILURE
    // rather than a skip.
    if !have(&["python3", "-c", "import yaml"]) || !have(&["pytest", "--version"]) {
        eprintln!(
            "SKIPPED: {} needs python3 with PyYAML and a `pytest` on PATH. NOT CHECKED here: \
             the Action's step bodies, the four conclusion states, the four badge states, \
             the shallow refusal, the badge-vs-conclusion guard, and §9's policy gate. \
             `.github/workflows/receipt.yml` installs both and runs it on ubuntu and macOS.",
            script.display()
        );
        return;
    }

    // This older Action matrix asserts v0.1's neutral/empty-claim and
    // receipt-follow-up behavior. Pin its producer without changing C's harness.
    // The new protocol is exercised by v2_session_cli_* and session_v2 tests.
    let compatibility = tempfile::tempdir().unwrap();
    let wrapper = compatibility.path().join("thruline-v01");
    let quoted = format!("'{}'", thruline().replace('\'', "'\\''"));
    std::fs::write(&wrapper, format!("#!/bin/sh\nif [ \"$1\" = receipt ]; then shift; exec {quoted} receipt --predicate-version 0.1 \"$@\"; fi\nexec {quoted} \"$@\"\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let out = Command::new("python3")
        .arg(&script)
        .arg("--thruline")
        .arg(&wrapper)
        .output()
        .expect("selftest.py");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{log}");
    // The harness must have *observed* the states, not merely exited 0 — a
    // scenario that stopped running would still leave a clean exit if every
    // remaining expectation happened to hold.
    for state in [
        "verified              success",
        "unverifiable          neutral",
        "tampered              failure",
        "unsupported-no-policy neutral",
        "cheating-session      failure",
    ] {
        let (scenario, conclusion) = state.split_once(' ').expect("pair");
        let conclusion = conclusion.trim();
        let row = log
            .lines()
            .find(|l| l.trim_start().starts_with(scenario))
            .unwrap_or_else(|| panic!("no observed row for {scenario}:\n{log}"));
        assert!(
            row.contains(conclusion),
            "{scenario} did not observe {conclusion}: {row}"
        );
    }
    // Every badge state, observed.
    for badge in [
        "verified #2ea043",
        "partial #bf8700",
        "not verified #cf222e",
        "(no badge)",
    ] {
        assert!(
            log.contains(badge),
            "no run observed the badge {badge:?}:\n{log}"
        );
    }
}

#[test]
fn v2_session_cli_preserves_origin_ground_and_same_rendered_conclusion() {
    let f = fixture_version("0.2");
    let output = f.verify("json", &[]);
    let body: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&output.stderr)));
    assert_eq!(body["schema_version"], 2);
    assert_eq!(body["shape"], "thruline.verify-receipt.session");
    assert_eq!(body["claim_origins"][0]["extractor"], "rules");
    assert_eq!(body["verified"], false);
    assert_eq!(output.status.code(), Some(2));
    let signed = &body["signed_record"]["predicate"];
    assert_eq!(signed["claim_origins"], body["claim_origins"]);
    assert!(signed["claims"][0]["text"]
        .as_str()
        .unwrap()
        .contains("original text omitted"));
    let head = String::from_utf8(
        Command::new("git")
            .current_dir(&f.repo)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    let check = f.verify("check-run", &["--head-sha", &head]);
    let check: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    assert_eq!(check["conclusion"], body["conclusion"]);
    assert_eq!(check["output"]["title"], body["cause_title"]);
    let badge = f.verify("badge", &[]);
    assert!(String::from_utf8(badge.stdout)
        .unwrap()
        .contains(body["badge_label"].as_str().unwrap()));
    let md = f.verify("md", &[]);
    assert!(String::from_utf8(md.stdout)
        .unwrap()
        .contains(body["cause_title"].as_str().unwrap()));
}
