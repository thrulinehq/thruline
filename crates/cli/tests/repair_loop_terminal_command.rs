//! C30 P0-4 — the terminal command HQ's "Check it again" sheet copies is a
//! command this binary runs, and records.
//!
//! The screen used to copy `thruline check --scope … -- <runner>`; `check` has
//! no `--scope`, so a person who pasted it got a usage error and nothing was
//! recorded. The screen now copies `thruline check --for-change <id> --
//! <runner> <scope…>` (`ui/src/hq/screens/Repair-Loop/terminal.js`).
//!
//! The examples live in one file both sides read:
//! `ui/src/hq/screens/Repair-Loop/terminal-command.json`. The UI's own test
//! (`repair-loop-run.test.js`) pins that `terminal.js` copies each `copies`
//! string exactly; this test RUNS each one — the exact string, with only
//! `{change}` replaced by a real Change id — through `/bin/sh`, the way a
//! terminal runs a pasted line, with a `thruline` on `PATH` that is this
//! build's binary. Why here and not in node: making a Change takes a store
//! with a project row, which these CLI tests already build with `rusqlite`
//! and the compiled binary (`CARGO_BIN_EXE_thruline`); a node test would need
//! a binary `npm test` does not build.
//!
//! Every store is under the test's own temporary HOME. No real store is opened.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

fn examples() -> Vec<serde_json::Value> {
    let text = include_str!("../../../ui/src/hq/screens/Repair-Loop/terminal-command.json");
    let doc: serde_json::Value = serde_json::from_str(text).expect("terminal-command.json parses");
    doc["examples"]
        .as_array()
        .expect("terminal-command.json has examples")
        .clone()
}

fn git(cwd: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// A repository holding `src/upload/retry.rs`, and the default store under a
/// temporary HOME (`$HOME/.thruline/ledger.db`, the store a pasted command
/// with no `--db` opens) with one project whose root is that repository.
fn fixture(home: &Path) -> (PathBuf, PathBuf) {
    let repo = home.join("repo");
    std::fs::create_dir_all(repo.join("src/upload")).expect("mkdir");
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("src/upload/retry.rs"), "// retry\n").expect("write");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "seed"]);
    std::fs::write(
        repo.join("s.jsonl"),
        format!("{{\"type\":\"user\",\"cwd\":\"{}\"}}\n", repo.display()),
    )
    .expect("transcript");

    std::fs::create_dir_all(home.join(".thruline")).expect("store folder");
    let db = home.join(".thruline/ledger.db");
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES('cccccccc-0000-4000-8000-00000000p001', ?1, 'claude_code', 'Synthetic',
                    '2026-01-01T00:00:00Z')",
            rusqlite::params![repo.display().to_string()],
        )
        .expect("project");
    ledger
        .conn()
        .execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES('cccccccc-0000-4000-8000-00000000s001',
                    'cccccccc-0000-4000-8000-00000000p001', ?1, '2026-01-02T09:30:00Z')",
            rusqlite::params![repo.join("s.jsonl").display().to_string()],
        )
        .expect("session");
    (repo, db)
}

struct Ran {
    code: i32,
    stdout: String,
    stderr: String,
}

/// One line, exactly as pasted into a terminal: `/bin/sh -c <line>` in the
/// repository, with this build's binary first on `PATH` as `thruline`.
fn paste(home: &Path, repo: &Path, line: &str) -> Ran {
    let bindir = home.join("bin");
    std::fs::create_dir_all(&bindir).expect("bin folder");
    let link = bindir.join("thruline");
    if !link.exists() {
        std::os::unix::fs::symlink(bin(), &link).expect("thruline on PATH");
    }
    let path = format!(
        "{}:{}",
        bindir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new("/bin/sh")
        .arg("-c")
        .arg(line)
        .current_dir(repo)
        .env("HOME", home)
        .env("THRULINE_HOME", home.join(".thruline"))
        .env("PATH", path)
        .output()
        .expect("sh runs");
    Ran {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn count(db: &Path, sql: &str, change: Option<&str>) -> i64 {
    let conn = rusqlite::Connection::open(db).expect("open");
    match change {
        Some(c) => conn.query_row(sql, rusqlite::params![c], |r| r.get(0)),
        None => conn.query_row(sql, [], |r| r.get(0)),
    }
    .expect("count")
}

const BINDINGS: &str = "SELECT COUNT(*) FROM evidence_bindings";
const FOR_CHANGE: &str = "SELECT COUNT(*) FROM change_check_sessions WHERE change_id = ?1";
const ANY_CHANGE: &str = "SELECT COUNT(*) FROM change_check_sessions";

#[test]
fn every_command_the_check_sheet_copies_runs_and_is_recorded() {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path();
    let (repo, db) = fixture(home);

    let opened = Command::new(bin())
        .env("HOME", home)
        .env("THRULINE_HOME", home.join(".thruline"))
        .current_dir(&repo)
        .arg("--db")
        .arg(&db)
        .args([
            "change",
            "new",
            "--title",
            "Retry failed uploads",
            "--purpose",
            "The upload retry is covered by a check.",
            "--requester",
            "sam",
        ])
        .output()
        .expect("change new");
    assert!(opened.status.success(), "{opened:?}");
    let stdout = String::from_utf8_lossy(&opened.stdout);
    let change = stdout
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("chg_"))
        .unwrap_or_else(|| panic!("no Change id was printed: {stdout}"))
        .to_string();

    let all = examples();
    assert!(all.len() >= 3, "terminal-command.json lost its examples");
    for example in all {
        let copies = example["copies"].as_str().expect("copies is a string");
        let tracked = example["change"].is_string();
        assert!(
            !copies.contains("--scope"),
            "the copied command names a flag `check` does not have: {copies}"
        );
        let line = copies.replace("{change}", &change);
        let before = (
            count(&db, BINDINGS, None),
            count(&db, FOR_CHANGE, Some(&change)),
            count(&db, ANY_CHANGE, None),
        );
        let ran = paste(home, &repo, &line);
        assert_eq!(
            ran.code, 0,
            "`{line}` did not run cleanly\nstdout: {}\nstderr: {}",
            ran.stdout, ran.stderr
        );
        assert_eq!(
            count(&db, BINDINGS, None),
            before.0 + 1,
            "`{line}` was not recorded as a check run: {}",
            ran.stderr
        );
        if tracked {
            assert!(
                ran.stderr
                    .contains(&format!("recorded for Change {change}")),
                "`{line}` never said it recorded the run for the Change: {}",
                ran.stderr
            );
            assert_eq!(
                count(&db, FOR_CHANGE, Some(&change)),
                before.1 + 1,
                "`{line}` was not attached to the Change"
            );
        } else {
            assert_eq!(
                count(&db, ANY_CHANGE, None),
                before.2,
                "`{line}` names no Change and must attach to none"
            );
        }
    }
}

/// The line the screen copied before C30, kept as the reason this file exists:
/// `check` refuses `--scope`, and nothing is recorded.
#[test]
fn the_old_scope_line_is_refused_and_records_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path();
    let (repo, db) = fixture(home);
    let ran = paste(
        home,
        &repo,
        "thruline check --scope src/upload -- test -d src/upload",
    );
    assert_ne!(ran.code, 0, "{}", ran.stderr);
    assert!(ran.stderr.contains("--scope"), "{}", ran.stderr);
    assert_eq!(count(&db, BINDINGS, None), 0);
}
