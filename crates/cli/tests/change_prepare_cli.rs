//! T-3-2 — `thruline change prepare` at the surface a person touches: what
//! exactly will be examined, in Sam's words, and a second running copy of the
//! app that is named rather than assumed (`B4.6`, `B6.2`, `B6.3`).
//!
//! What this file is for that the core tests are not: the *words* and the
//! *machine*. The core tests hand the predicate a record; this one builds a
//! real git worktree, observes it, and reads back the sentences a person sees.
//!
//! Every store is a fixture under a temporary HOME. No real store is opened,
//! and no port this test did not itself open is ever probed.

use std::path::{Path, PathBuf};
use std::process::Command;

use rusqlite::params;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000p001";
const CHANGE: &str = "cccccccc-0000-4000-8000-00000000c001";

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

/// A real git worktree with one commit and one uncommitted edit, so the
/// observation has something true to say about both.
fn worktree(dir: &Path) -> PathBuf {
    let root = dir.join("project");
    std::fs::create_dir_all(root.join("src")).expect("mkdir");
    let git = |args: &[&str]| {
        Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
            .output()
            .expect("git");
    };
    git(&["init", "-q"]);
    std::fs::write(root.join("src/retry.rs"), "// one\n").expect("write");
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "one"]);
    // One edit nobody committed: the Prepare view must say it is included.
    std::fs::write(root.join("src/retry.rs"), "// one\n// two\n").expect("write");
    root
}

fn fixture(dir: &Path, root: &Path) -> PathBuf {
    let db = dir.join("ledger.db");
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open a fresh ledger");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES(?1, ?2, 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
            params![PROJECT, root.display().to_string()],
        )
        .expect("project");
    thruline_core::change::insert_change(
        &ledger,
        &thruline_core::change::NewChange {
            id: CHANGE.to_string(),
            project_id: PROJECT.to_string(),
            opened_at: "2026-01-02T09:00:00Z".to_string(),
            request: thruline_core::change::ChangeRequest {
                title: "Retry failed deliveries".to_string(),
                purpose: "Deliveries that fail once are lost.".to_string(),
                prompt_event_id: None,
                requester: "sam".to_string(),
                source: thruline_core::change::RequestSource::Person,
            },
        },
    )
    .expect("change");
    db
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(home: &Path, db: &Path, cwd: &Path, args: &[&str]) -> Run {
    let out = Command::new(bin())
        .env("HOME", home)
        .env("THRULINE_HOME", home.join(".thruline"))
        .current_dir(cwd)
        .arg("--db")
        .arg(db)
        .args(args)
        .output()
        .expect("run thruline");
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

#[test]
fn prepare_says_what_will_be_examined_in_sams_words() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = worktree(dir.path());
    let db = fixture(dir.path(), &root);

    let out = run(dir.path(), &db, &root, &["change", "prepare", CHANGE]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("The project folder:"),
        "stdout: {}",
        out.stdout
    );
    assert!(out.stdout.contains("Starting from:"), "{}", out.stdout);
    assert!(
        out.stdout.contains("Your unsaved changes are included:"),
        "an uncommitted edit must be named as included: {}",
        out.stdout
    );
    // Plain never says any of these. Whole words only: the output names the
    // temporary project folder, whose random name (`.tmpIdJ92R`) can contain
    // "pid" without anyone having said it.
    for word in ["snapshot id", "tree hash", "pid", "worktree", "predicate"] {
        assert!(
            !contains_whole_word(&out.stdout, word),
            "the plain register said {word:?}: {}",
            out.stdout
        );
    }
}

/// Case-insensitive match of `word` (which may be a phrase, and may carry a
/// plural `s`) that does not count a hit inside a longer alphanumeric run, so
/// `PID: 4` and `the pids` are found and `.tmpIdJ92R` is not.
fn contains_whole_word(haystack: &str, word: &str) -> bool {
    let haystack = haystack.to_lowercase();
    let word = word.to_lowercase();
    haystack.match_indices(&word).any(|(start, hit)| {
        let before = haystack[..start].chars().next_back();
        let rest = &haystack[start + hit.len()..];
        let rest = rest.strip_prefix('s').unwrap_or(rest);
        !before.is_some_and(char::is_alphanumeric)
            && !rest.chars().next().is_some_and(char::is_alphanumeric)
    })
}

#[test]
fn the_forbidden_word_check_matches_words_not_fragments() {
    // Inside a random token: not a hit.
    assert!(!contains_whole_word(
        "The project folder: /private/tmp/.tmpIdJ92R/project",
        "pid"
    ));
    // A real word, in any case or punctuation: a hit.
    assert!(contains_whole_word("the PID is 4", "pid"));
    assert!(contains_whole_word("pid: 4", "pid"));
    assert!(contains_whole_word("(pid)", "pid"));
    assert!(contains_whole_word("Snapshot ID recorded", "snapshot id"));
    // A plural is still the word; a longer word that merely contains it is not.
    assert!(contains_whole_word("two worktrees", "worktree"));
    assert!(!contains_whole_word("rapidly", "pid"));
    assert!(!contains_whole_word("tmpidj92r", "pid"));
}

#[test]
fn prepare_records_what_it_observed_and_json_carries_both_registers() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = worktree(dir.path());
    let db = fixture(dir.path(), &root);

    let out = run(
        dir.path(),
        &db,
        &root,
        &["change", "prepare", CHANGE, "--json"],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    // The envelope is flat: the shape's fields are top-level beside
    // `schema_version` and `shape`.
    let data: serde_json::Value = serde_json::from_str(&out.stdout).expect("json");
    assert_eq!(data["record"]["inserted"], serde_json::json!(true));
    let prepare = &data["prepare"];
    assert_eq!(
        prepare["inputs"]["base_revision"]["state"],
        serde_json::json!("known"),
        "a real worktree must produce a real starting point"
    );
    assert_eq!(
        prepare["inputs"]["uncommitted"]["state"],
        serde_json::json!("present")
    );
    assert_eq!(
        prepare["inputs"]["target"]["kind"],
        serde_json::json!("none"),
        "no port was named, so no running app is claimed"
    );
    let sentences = data["sentences"].as_array().expect("sentences");
    assert!(!sentences.is_empty());
    for pair in sentences {
        let plain = pair[0].as_str().unwrap_or_default();
        let technical = pair[1].as_str().unwrap_or_default();
        assert!(!plain.is_empty() && !technical.is_empty());
        assert_ne!(
            plain, technical,
            "both registers must be present and differ"
        );
        assert!(
            !plain.contains('{'),
            "an unfilled placeholder reached a person: {plain}"
        );
    }
}

/// A port nobody is listening on is recorded as nobody, not as somebody.
#[test]
fn a_port_with_nothing_on_it_says_so_and_invents_no_owner() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = worktree(dir.path());
    let db = fixture(dir.path(), &root);

    // A port this test opened and closed: nothing is listening on it now, and
    // nothing else on this machine is touched.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);

    let out = run(
        dir.path(),
        &db,
        &root,
        &[
            "change",
            "prepare",
            CHANGE,
            "--app-port",
            &port.to_string(),
            "--json",
        ],
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let value: serde_json::Value = serde_json::from_str(&out.stdout).expect("json");
    let target = &value["prepare"]["inputs"]["target"];
    assert_eq!(target["kind"], serde_json::json!("running_app"));
    assert_eq!(
        target["owner_exe"]["state"],
        serde_json::json!("unknown"),
        "nothing was listening, so no owner may be named"
    );
    assert_eq!(value["prepare"]["app_program"], serde_json::Value::Null);
}
