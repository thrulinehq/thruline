//! `thruline policy` end to end: BUILD-SPEC §9's gate, run locally.
//!
//! The assertions that matter here are the refusals. A policy gate's failure
//! modes are all in one direction — a gate that silently passes is worse than
//! no gate, because it is a green check somebody trusts.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_thruline");

fn tmp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("tl-policy-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

fn run(dir: &std::path::Path, args: &[&str]) -> (String, i32) {
    let out = Command::new(BIN)
        .arg("policy")
        .args(args)
        .arg("--repo")
        .arg(dir)
        .output()
        .expect("run thruline policy");
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (text, out.status.code().unwrap_or(-1))
}

#[test]
fn init_writes_the_documented_example_and_it_parses() {
    let dir = tmp("init");
    let (text, code) = run(&dir, &["--init"]);
    assert_eq!(code, 0, "{text}");
    let written = std::fs::read_to_string(dir.join(".thruline").join("policy.yml")).expect("read");
    assert!(written.contains("version: 1"), "{written}");
    assert!(
        written.contains("min_changed_line_coverage: 70"),
        "{written}"
    );
    // `init` parses what it wrote, so a broken template cannot ship silently.
    assert!(text.contains("Wrote"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Clobbering a policy file is the worst thing this command could do: it is the
/// thing standing between a merge and an unsupported claim.
#[test]
fn init_refuses_to_overwrite_an_existing_policy() {
    let dir = tmp("no-clobber");
    run(&dir, &["--init"]);
    std::fs::write(dir.join(".thruline").join("policy.yml"), "version: 1\n").expect("write");
    let (text, code) = run(&dir, &["--init"]);
    assert_eq!(code, 1, "{text}");
    assert_eq!(
        std::fs::read_to_string(dir.join(".thruline").join("policy.yml")).expect("read"),
        "version: 1\n",
        "the existing policy was overwritten"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A repository that never opted in must not be gated by a default nobody
/// wrote.
#[test]
fn no_policy_file_gates_nothing() {
    let dir = tmp("none");
    let (text, code) = run(&dir, &[]);
    assert_eq!(code, 0, "{text}");
    assert!(text.contains("nothing is gated"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A policy the parser does not understand stops the run. It must never fall
/// back to "gate nothing", which is how a typo turns a gate off.
#[test]
fn an_unparseable_policy_is_an_error_not_a_pass() {
    let dir = tmp("broken");
    std::fs::create_dir_all(dir.join(".thruline")).expect("mkdir");
    std::fs::write(
        dir.join(".thruline").join("policy.yml"),
        "version: 1\nrequire_reciept: true\n",
    )
    .expect("write");
    let (text, code) = run(&dir, &[]);
    assert_eq!(code, 1, "a broken policy passed the gate: {text}");
    assert!(text.contains("require_reciept"), "{text}");
    assert!(
        text.contains("line 2"),
        "the error does not name the line: {text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The subset's refusals reach the user with the construct named, not as a
/// parse error they have to guess at.
#[test]
fn a_construct_outside_the_subset_is_named_to_the_user() {
    let dir = tmp("subset");
    std::fs::create_dir_all(dir.join(".thruline")).expect("mkdir");
    std::fs::write(
        dir.join(".thruline").join("policy.yml"),
        "version: 1\nclaims:\n  tests_pass:\n    required_when:\n      paths:\n        - src/**\n",
    )
    .expect("write");
    let (text, code) = run(&dir, &[]);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("block sequence"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// The Action's contract with the binary.
// ---------------------------------------------------------------------------

/// `--conclusion` prints one word and nothing else, because the Action's shell
/// `case` matches on it exactly. A stray line here is a check run that reports
/// "unrecognised conclusion" to every user at once.
#[test]
fn the_conclusion_flag_prints_exactly_one_recognised_word() {
    let dir = tmp("conclusion");
    run(&dir, &["--init"]);
    let out = Command::new(BIN)
        .args(["policy", "--conclusion", "--repo"])
        .arg(&dir)
        .output()
        .expect("run");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    // With no execution store this exits 1 and prints nothing, which the
    // Action treats as a failed step rather than a conclusion. The contract
    // being asserted is that stdout is *never* something the `case` cannot
    // match: either empty, or exactly one of the three words.
    let trimmed = text.trim();
    assert!(
        trimmed.is_empty() || ["success", "failure", "neutral"].contains(&trimmed),
        "stdout was {text:?}"
    );
    assert!(text.lines().count() <= 1, "{text:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The three words the shell matches on are the three the binary can print.
/// Checked by reading both, so a fourth conclusion added in Rust fails here
/// rather than in someone's CI.
#[test]
fn the_action_script_matches_every_conclusion_the_binary_can_print() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root");
    let script = std::fs::read_to_string(root.join("packaging/action/action.yml")).expect("action");
    let rust = std::fs::read_to_string(root.join("crates/cli/src/policy_cmd.rs")).expect("cmd");
    let mut printable: Vec<&str> = Vec::new();
    for word in ["success", "failure", "neutral"] {
        if rust.contains(&format!("=> \"{word}\"")) {
            printable.push(word);
        }
    }
    assert_eq!(printable.len(), 3, "{printable:?}");
    for word in &printable {
        assert!(
            script.contains(word),
            "`{word}` is a conclusion the binary prints and the action script does not \
             handle; CI would report `unrecognised conclusion`"
        );
    }
    // The policy step must not parse structured output in shell — such a parser
    // has no tests, and the one place it would go wrong is a conclusion read as
    // the wrong one. Checked over the policy step only: the receipt step's own
    // `jq -r '.conclusion'` is covered by the M2 census and is not this rule's
    // business.
    let step = script
        .split("- name: Evaluate the policy")
        .nth(1)
        .and_then(|s| s.split("\n    - name:").next())
        .expect("the policy step is gone from the action");
    for parser in ["sed -n", "jq ", "awk "] {
        assert!(
            !step.contains(parser),
            "the policy step parses structured output with `{parser}`"
        );
    }
    assert!(step.contains("--conclusion"), "{step}");
}

/// The installer must verify before it executes. A supply-chain step that
/// trusts the download is not one.
#[test]
fn the_installer_checksums_before_it_installs() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root");
    let install =
        std::fs::read_to_string(root.join("packaging/action/install.sh")).expect("install.sh");
    let sum = install.find("shasum -a 256").expect("no checksum step");
    let run = install.find("install -m 0755").expect("no install step");
    assert!(
        sum < run,
        "the binary is installed before it is checksummed"
    );
    assert!(
        install.contains("set -euo pipefail"),
        "unset -e: a failed curl would continue"
    );
    // A floating version changes behaviour without a diff. Checked against the
    // ways it would actually appear — `releases/latest`, or a default — rather
    // than against the word, which also occurs in the comment explaining why it
    // is absent. (The first version of this assertion matched its own prose.)
    for floating in ["releases/latest", ":-latest", "=latest"] {
        assert!(
            !install.contains(floating),
            "the installer resolves a floating version via `{floating}`"
        );
    }
    // Positive control: the pinned inputs really are required, so the check
    // above is asserting something.
    assert!(
        install.contains("${TL_VERSION:?"),
        "the version is not required"
    );
    assert!(
        install.contains("${TL_CHECKSUM:?"),
        "the checksum is not required"
    );
}

// ---------------------------------------------------------------------------
// §14's sync verbs, and the shape of a decision that was made.
// ---------------------------------------------------------------------------

/// §18's answer is that the hosted sync server does not ship in v1. The verb
/// therefore **exists and refuses**, rather than being absent: a verb §14 lists
/// that is simply missing reads as an oversight, and the next person implements
/// it.
#[test]
fn sync_refuses_and_names_the_decision_rather_than_being_absent() {
    let out = Command::new(BIN)
        .args(["sync", "push", "--json"])
        .output()
        .expect("run thruline sync");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(
        out.status.code(),
        Some(1),
        "sync must refuse, not fail as a usage error and not succeed: {text}"
    );
    let v: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(v["ok"], false);
    let reason = v["reason"].as_str().unwrap_or_default();
    assert!(reason.contains("§18"), "{reason}");
    assert!(
        reason.contains("no server exists") || reason.contains("does not ship"),
        "{reason}"
    );
}

/// The refusal is one string, so the two builds cannot drift apart in what they
/// tell a user. Checked by reading, because only one build is compiled here.
#[test]
fn both_builds_of_sync_give_the_same_reason() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root");
    let src = std::fs::read_to_string(root.join("crates/cli/src/sync_cmd.rs")).expect("sync_cmd");
    assert_eq!(
        src.matches("const NOT_SHIPPED").count(),
        1,
        "the refusal text is defined more than once, so the two builds can drift"
    );
    assert_eq!(src.matches("#[cfg(feature = \"sync\")]").count(), 1);
    assert_eq!(src.matches("#[cfg(not(feature = \"sync\"))]").count(), 1);
    // Neither build may exit 0 for a verb that needs a transport there is none of.
    for build in src.split("pub fn run").skip(1) {
        let head = build.split("\n}\n").next().unwrap_or(build);
        if head.contains("\"status\" =>") {
            continue;
        }
        assert!(
            head.contains("ExitCode::from(1)"),
            "a build of `sync` can exit 0 without a transport"
        );
    }
}
