//! T-7-1 rehearsal: `thruline doctor` run from a folder that is not a git
//! repository exited 1 — `FAIL tree-fidelity … not a git repository` — with
//! every other line ok. Nothing was broken; there was no working tree to
//! check. It is now a `note`, which is printed and never counted as a failure.
//!
//! The two neighbours are pinned beside it so the fix cannot be "stop
//! checking": inside a repository the working-tree check still runs, and a
//! repository NAMED with `--repo` that is not one still fails, because that is
//! a check somebody asked for and did not get.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_thruline");

/// A machine with nothing on it, and doctor run from `cwd`. HOME and the
/// store are the test's own, so no line depends on the developer's setup.
fn doctor(scratch: &Path, cwd: &Path, extra: &[&str]) -> Output {
    Command::new(BIN)
        .arg("doctor")
        .arg("--exec-db")
        .arg(scratch.join("store/exec.db"))
        .arg("--spool")
        .arg(scratch.join("store/spool.jsonl"))
        .args(extra)
        .current_dir(cwd)
        .env("HOME", scratch.join("home"))
        .env("THRULINE_HOME", scratch.join("store"))
        .env_remove("GIT_DIR")
        .output()
        .expect("run thruline doctor")
}

/// A folder with no `.git` in it or above it — asserted, not assumed.
fn not_a_repository(scratch: &Path) -> PathBuf {
    let dir = scratch.join("somewhere");
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::create_dir_all(scratch.join("home")).expect("home");
    let dir = dir.canonicalize().expect("canonical");
    assert!(
        !dir.ancestors().any(|a| a.join(".git").exists()),
        "the scratch folder is inside a repository; this test cannot say anything"
    );
    dir
}

fn text(out: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn outside_a_repository_the_working_tree_check_is_a_note_and_doctor_exits_0() {
    let scratch = tempfile::tempdir().expect("tempdir");
    let here = not_a_repository(scratch.path());

    let out = doctor(scratch.path(), &here, &[]);
    let all = text(&out);
    assert_eq!(out.status.code(), Some(0), "{all}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let note = stdout
        .lines()
        .find(|l| l.starts_with("note tree-fidelity"))
        .unwrap_or_else(|| panic!("no note for the working-tree check:\n{all}"));
    assert!(note.contains("is not inside a git repository"), "{note}");
    assert!(note.contains("--repo <path>"), "{note}");
    assert!(
        !stdout.lines().any(|l| l.starts_with("FAIL")),
        "something failed on an empty machine:\n{all}"
    );
    assert!(
        stdout.contains(
            "Everything checked is intact. A `note` line is a check that did not apply here"
        ),
        "{all}"
    );
    // Positive control: the rows around it were printed, so this is doctor's
    // full report and not an early exit that happens to be 0.
    for key in ["git", "chain", "hooks", "redact.yml"] {
        assert!(
            stdout
                .lines()
                .any(|l| l.starts_with("ok  ") && l[5..].starts_with(key)),
            "no `{key}` row:\n{all}"
        );
    }

    // The same answer to a script: `ok`, and the note marked as one.
    let out = doctor(scratch.path(), &here, &["--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("doctor --json");
    assert_eq!(v["schema_version"], 2, "{v}");
    assert_eq!(v["ok"], true, "{v}");
    let findings = v["findings"].as_array().expect("findings");
    let notes: Vec<&serde_json::Value> = findings.iter().filter(|f| f["note"] == true).collect();
    assert_eq!(notes.len(), 1, "{v}");
    assert_eq!(notes[0]["key"], "tree-fidelity", "{v}");
    assert!(
        findings
            .iter()
            .all(|f| f["note"].is_boolean() && f["ok"] == true),
        "{v}"
    );
}

#[test]
fn inside_a_repository_the_working_tree_check_still_runs() {
    let scratch = tempfile::tempdir().expect("tempdir");
    let repo = not_a_repository(scratch.path()).join("project");
    std::fs::create_dir_all(&repo).expect("mkdir");
    let init = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&repo)
        .output()
        .expect("git init");
    assert!(init.status.success(), "{}", text(&init));

    let out = doctor(scratch.path(), &repo, &[]);
    let all = text(&out);
    assert_eq!(out.status.code(), Some(0), "{all}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.lines().any(|l| l.starts_with("ok   tree-fidelity")),
        "the working-tree check did not run inside a repository:\n{all}"
    );
    assert!(
        !stdout.lines().any(|l| l.starts_with("note")),
        "a note inside a repository:\n{all}"
    );
    assert!(stdout.contains("Everything checked is intact.\n"), "{all}");
}

#[test]
fn a_repository_named_with_repo_that_is_not_one_still_fails() {
    let scratch = tempfile::tempdir().expect("tempdir");
    let here = not_a_repository(scratch.path());

    let out = doctor(
        scratch.path(),
        &here,
        &["--repo", here.to_str().expect("utf-8")],
    );
    let all = text(&out);
    assert_eq!(out.status.code(), Some(1), "{all}");
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .any(|l| l.starts_with("FAIL tree-fidelity")),
        "{all}"
    );
}
