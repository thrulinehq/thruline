//! `thruline demo` shows a real refutation, not a rehearsed one.
//!
//! §15 asks for a first run that makes the value visible. The risk with any
//! demo is that it becomes a recording: the output stops depending on the code
//! and starts depending on a fixture, and then it keeps passing after the
//! product stops working. This test asserts the opposite property — the demo's
//! verdict is produced by the same verifiers M1's done-when exercises, over
//! evidence captured from a process that really ran during the command.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_thruline");

fn pytest_available() -> bool {
    Command::new("pytest")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn the_demo_refutes_the_agents_claim_and_names_the_promise() {
    if !pytest_available() {
        if std::env::var_os("THRULINE_TEST_NO_PYTEST").is_some() {
            eprintln!("SKIPPED: pytest is not installed; §15's demo did NOT run.");
            return;
        }
        panic!("pytest is not on PATH; `thruline demo` needs a real runner");
    }
    // The demo keeps its folder when it succeeds; keep it in this test's own
    // temporary directory rather than the machine's.
    let scratch = tempfile::tempdir().expect("tempdir");
    let out = Command::new(BIN)
        .args(["demo", "--json"])
        .env("TMPDIR", scratch.path())
        .output()
        .expect("run thruline demo");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        out.status.success(),
        "demo exited {:?}\n{text}\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));

    let claims = v["claims"].as_array().expect("claims");
    let tp = claims
        .iter()
        .find(|c| c["kind"] == "tests_pass")
        .unwrap_or_else(|| panic!("no `tests_pass` claim: {v:#}"));
    assert_eq!(
        tp["verdict"], "unsupported",
        "the demo did not refute the claim: {tp:#}"
    );
    let findings: Vec<String> = tp["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .filter_map(|f| f.as_str().map(str::to_string))
        .collect();
    // §16's two, by name. More is fine; fewer is the demo having stopped
    // demonstrating the thing it exists to demonstrate.
    for want in ["narrowed_run", "assertion_weakened"] {
        assert!(
            findings.iter().any(|f| f == want),
            "`{want}` is missing from {findings:?}"
        );
    }
    // §15 asks for an open promise as well as an unsupported claim.
    let promises = v["promises"].as_array().expect("promises");
    assert_eq!(promises.len(), 1, "{promises:?}");
    assert!(
        promises[0]
            .as_str()
            .is_some_and(|p| p.contains("regression test")),
        "{promises:?}"
    );
    // The runner is named, so a reader knows what actually executed.
    assert_eq!(v["runner"], "pytest");
}

/// The demo never touches the user's ledger — a first run that wrote to
/// `~/.thruline` would be a first run people are right to be wary of — and the
/// one thing it leaves is its own folder, in the temporary directory, holding
/// only what the demo made.
///
/// **Changed at T-7-1's rehearsal fixes, and said here rather than silently.**
/// This test used to assert the demo's repository was GONE after a default
/// run. That was the property that made the demo's own closing line — "Look
/// for yourself: THRULINE_HOME=<that folder>/home thruline explain --last" —
/// fail every time with "no execution store": it invited the reader into a
/// folder it had just deleted. The folder is now kept on success, named on
/// screen with how to remove it; the property about the user's own store is
/// unchanged.
#[test]
fn the_demo_keeps_only_its_own_folder_and_never_touches_the_users_store() {
    if !pytest_available() {
        return;
    }
    let scratch = tempfile::tempdir().expect("tempdir");
    let out = Command::new(BIN)
        .args(["demo", "--json"])
        .env("TMPDIR", scratch.path())
        .output()
        .expect("run");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let repo = std::path::PathBuf::from(v["repo"].as_str().expect("repo path"));
    assert!(repo.is_dir(), "the demo's repository is gone: {repo:?}");
    let root = repo.parent().expect("the demo's folder");
    assert_eq!(
        root.parent()
            .map(std::fs::canonicalize)
            .and_then(Result::ok),
        std::fs::canonicalize(scratch.path()).ok(),
        "the demo's folder is not in the temporary directory: {root:?}"
    );
    assert!(
        root.file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("thruline-demo.")),
        "the kept folder does not say what it is: {root:?}"
    );
    let mut inside: Vec<String> = std::fs::read_dir(root)
        .expect("read the demo's folder")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    inside.sort();
    assert_eq!(inside, ["home", "repo"], "the demo's folder holds more");

    let home = dirs_home().join(".thruline").join("exec.db");
    let before = std::fs::metadata(&home).ok().map(|m| m.len());
    let _ = Command::new(BIN)
        .args(["demo", "--json"])
        .env("TMPDIR", scratch.path())
        .output();
    let after = std::fs::metadata(&home).ok().map(|m| m.len());
    assert_eq!(
        before, after,
        "the demo wrote to the user's execution store"
    );
}

fn dirs_home() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/nonexistent"))
}

// ---------------------------------------------------------------------------
// T-7-1 rehearsal: a machine without what the demo needs
// ---------------------------------------------------------------------------

/// A `PATH` holding exactly the named programs, resolved from this test's own
/// `PATH`, so "pytest is missing" is true by construction rather than by
/// hoping the machine running the suite lacks it.
fn path_with_only(dir: &std::path::Path, programs: &[&str]) -> std::ffi::OsString {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).expect("bin dir");
    for program in programs {
        let found = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|p| p.join(program))
            .find(|p| p.is_file())
            .unwrap_or_else(|| panic!("`{program}` is not on this test's PATH"));
        std::os::unix::fs::symlink(&found, bin.join(program)).expect("symlink");
    }
    bin.into_os_string()
}

/// Run the demo in `dir` with only `programs` on PATH; nothing reaches HOME.
fn demo_with_only(dir: &std::path::Path, programs: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .args(["demo", "--dir"])
        .arg(dir.join("demo"))
        .env("PATH", path_with_only(dir, programs))
        .env("HOME", dir.join("home"))
        .env("THRULINE_HOME", dir.join("thruline-home"))
        .env("TMPDIR", dir)
        .output()
        .expect("run thruline demo")
}

/// The rehearsal's failure, exactly: under a PATH with no pytest the demo
/// printed "found none. Looked for: pytest" and exited 1, and nothing said how
/// to get it. It must name what is missing, say how to get it, and stop before
/// writing anything.
#[test]
fn without_pytest_the_demo_says_what_is_missing_and_how_to_get_it_before_writing_anything() {
    let temp = tempfile::tempdir().expect("tempdir");
    let out = demo_with_only(temp.path(), &["git"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert_eq!(out.status.code(), Some(1), "{stdout}\n{stderr}");
    // What is missing, by name — and only that: git is here.
    assert!(stderr.contains("it needs pytest on your PATH"), "{stderr}");
    assert!(!stderr.contains("To get git"), "{stderr}");
    // How to get it, and how to tell it worked.
    assert!(stderr.contains("To get pytest:"), "{stderr}");
    assert!(
        stderr.contains("pipx install pytest") && stderr.contains("python3 -m pip install pytest"),
        "{stderr}"
    );
    assert!(stderr.contains("`pytest --version`"), "{stderr}");
    // Why it will not do without, and that nothing happened.
    assert!(stderr.contains("will not simulate one"), "{stderr}");
    assert!(stderr.contains("Nothing was written."), "{stderr}");

    // Checked BEFORE starting: no repository was built, no store was made.
    assert!(stdout.is_empty(), "the demo started anyway: {stdout}");
    assert!(
        !temp.path().join("demo").exists(),
        "a demo folder was created"
    );
    assert!(
        !temp.path().join("thruline-home").exists(),
        "a store was created"
    );
}

/// Git is looked for up front too: without it the old demo died inside
/// `git init` with an error that named neither git nor how to get it.
#[test]
fn without_git_or_pytest_the_demo_names_both_before_writing_anything() {
    let temp = tempfile::tempdir().expect("tempdir");
    let out = demo_with_only(temp.path(), &[]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("it needs pytest and git on your PATH, and could not find them"),
        "{stderr}"
    );
    assert!(stderr.contains("To get pytest:"), "{stderr}");
    assert!(
        stderr.contains("To get git:") && stderr.contains("`git --version`"),
        "{stderr}"
    );
    assert!(
        !temp.path().join("demo").exists(),
        "a demo folder was created"
    );
}

/// Wherever a page tells a reader to run `thruline demo`, it must say the demo
/// needs pytest, and the install page must say how to check for it and get it.
///
/// This was the README's `## Install` (`thruline --version && thruline demo`).
/// The public README no longer runs the demo at all, so the facts moved with
/// the command: the book's quickstart is where a reader is told to type
/// `thruline demo`, and its install page carries the prerequisite table.
#[test]
fn the_readme_install_section_lists_pytest_for_the_demo() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let page = |name: &str| -> String {
        std::fs::read_to_string(root.join("docs/book/src").join(name))
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };

    // The page that tells the reader to run it must name what it needs.
    let quickstart = page("quickstart.md");
    assert!(
        quickstart.contains("thruline demo"),
        "the guard is aimed at the wrong page: quickstart.md no longer gives `thruline demo`"
    );
    assert!(
        quickstart.contains("it needs **pytest** on `PATH`"),
        "quickstart.md gives `thruline demo` without saying it needs pytest"
    );

    // The prerequisite table says how to check for it and how to get it.
    let install = page("install.md");
    let row = install
        .split(" | ")
        .find(|cell| cell.contains("only for `thruline demo`"))
        .unwrap_or_else(|| panic!("install.md has no row for `thruline demo`"));
    assert!(row.contains("`pytest --version`"), "{row}");
    assert!(row.contains("pipx install pytest"), "{row}");
    assert!(
        install.contains("**pytest** on `PATH`"),
        "install.md's table does not name pytest as the demo's prerequisite"
    );
    // The demo's own refusal offers these two ways to get pytest
    // (`without_git_or_pytest_the_demo_names_both_before_writing_anything`);
    // the table must offer the same two, or the page and the product disagree.
    assert!(
        row.contains("python3 -m pip install pytest"),
        "install.md and the demo's own message offer different ways to get pytest: {row}"
    );
}

/// The demo's closing lines are commands a person pastes, so they are run here
/// exactly as printed. At the rehearsal the first one exited 1 with "no
/// execution store", because the demo had already deleted the folder it named.
///
/// The temporary directory holds a space and a quote on purpose: one of the
/// printed lines is `rm -rf`, and a path that splits into two words there is
/// the difference between removing the demo and removing something else.
#[test]
fn the_closing_look_for_yourself_command_works_as_printed() {
    if !pytest_available() {
        if std::env::var_os("THRULINE_TEST_NO_PYTEST").is_some() {
            eprintln!("SKIPPED: pytest is not installed; §15's demo did NOT run.");
            return;
        }
        panic!("pytest is not on PATH; `thruline demo` needs a real runner");
    }
    let scratch = tempfile::tempdir().expect("tempdir");
    let tmpdir = scratch.path().join("demo's temp dir");
    let home = scratch.path().join("home");
    std::fs::create_dir_all(&tmpdir).expect("tmpdir");
    std::fs::create_dir_all(&home).expect("home");
    let out = Command::new(BIN)
        .arg("demo")
        .env("TMPDIR", &tmpdir)
        .env("HOME", &home)
        .env_remove("THRULINE_HOME")
        .output()
        .expect("run thruline demo");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let line_after = |marker: &str| -> String {
        let mut lines = stdout.lines();
        lines
            .find(|l| l.contains(marker))
            .unwrap_or_else(|| panic!("no `{marker}` in:\n{stdout}"));
        lines.next().expect("a command after it").trim().to_string()
    };
    let sh = |command: &str| {
        Command::new("sh")
            .args(["-c", command])
            .env("HOME", &home)
            .env_remove("THRULINE_HOME")
            .output()
            .expect("run sh")
    };

    // 1. "Look for yourself" — the command reads what the demo recorded.
    let look = line_after("Look for yourself:");
    assert!(look.contains("explain --last"), "{look}");
    let read = sh(&look);
    let said = String::from_utf8_lossy(&read.stdout);
    let complained = String::from_utf8_lossy(&read.stderr);
    assert!(
        !complained.contains("no execution store"),
        "the printed command names a folder that is gone: {look}\n{complained}"
    );
    // `explain` exits 3 when a claim is unsupported, and the demo says so.
    assert_eq!(read.status.code(), Some(3), "{look}\n{said}\n{complained}");
    assert!(stdout.contains("It exits with status 3"), "{stdout}");
    assert!(
        said.contains("d3m0d3m0-0000-4000-8000-000000000001"),
        "it read some other session: {said}"
    );
    assert!(
        said.contains("I'll add a regression test for the large-weight case"),
        "it did not read the demo's record: {said}"
    );

    // 2. The folder it names is the one it kept, and the printed removal
    //    removes exactly that. Resolved by the shell first, so the path the
    //    test checks is the path `rm` would receive.
    let remove = line_after("are done:");
    let target = remove
        .strip_prefix("rm -rf ")
        .unwrap_or_else(|| panic!("not a removal: {remove}"));
    let resolved = sh(&format!("printf %s {target}"));
    let resolved = std::path::PathBuf::from(String::from_utf8_lossy(&resolved.stdout).to_string());
    assert_eq!(
        resolved.parent().map(std::path::Path::to_path_buf),
        Some(tmpdir.clone()),
        "the removal is not aimed at a folder in the temporary directory: {remove}"
    );
    assert!(
        resolved.join("home").join("exec.db").is_file(),
        "{resolved:?}"
    );
    let removed = sh(&remove);
    assert!(
        removed.status.success(),
        "{remove}: {}",
        String::from_utf8_lossy(&removed.stderr)
    );
    assert!(!resolved.exists(), "the printed removal left {resolved:?}");
    assert!(
        tmpdir.is_dir(),
        "the removal took the temporary directory too"
    );
}
