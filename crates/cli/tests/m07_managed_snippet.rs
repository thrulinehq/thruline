//! BUILD-SPEC §4.1 — the managed-settings snippet, checked as a **fleet**
//! artefact rather than as a local one.
//!
//! # The test this file exists because of
//!
//! `hook_install::tests` already compared the snippet against a local install.
//! That comparison is structurally incapable of seeing the defect it was
//! standing in front of: both sides are generated on the same machine, in the
//! same process, out of the same `HOME`, so a snippet full of that machine's
//! absolute paths matches an install full of the same absolute paths and the
//! assertion is green. The snippet was carrying the generating machine's spool
//! path, and an organisation pushing it fleet-wide would have sent every
//! machine's record into one user's home directory.
//!
//! **Nothing downstream would have said so.** `harness::run` exits 0 and prints
//! nothing by contract, and its own spool fallback fires only when no `--spool`
//! was passed at all — never when the one that was passed points somewhere
//! wrong. The symptom is an empty spool on every machine but one.
//!
//! So the property is proved the only way it can be: generate under one `HOME`,
//! run under another, and read both spools.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn thruline() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

/// A directory holding a copy of the binary under the plain name `thruline`,
/// which is what the snippet's command invokes.
///
/// This is not a convenience: it is half of the property. A fleet snippet
/// cannot name the generating machine's binary either, so it says `thruline`
/// and leaves the deployment to put one on `PATH`. If that name did not
/// resolve, the run below would fail with 127 rather than writing anywhere,
/// and the spool assertions would pass for the wrong reason.
fn bin_dir(root: &Path) -> PathBuf {
    let dir = root.join("bin");
    std::fs::create_dir_all(&dir).expect("bin dir");
    let dest = dir.join("thruline");
    std::fs::copy(thruline(), &dest).expect("copy the binary under a bare name");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    dir
}

/// A command with every ambient spool decision removed, so what the test
/// measures is the snippet and not the machine running the suite.
fn clean(cmd: &mut Command) -> &mut Command {
    cmd.env_remove("THRULINE_HOOK_SPOOL")
        .env_remove("THRULINE_HOME")
        .env_remove("CLAUDE_PROJECT_DIR")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
}

fn snippet_command_for(event: &str, home: &Path) -> String {
    let out = clean(&mut Command::new(thruline()))
        .args([
            "hook-install",
            "--harness",
            "claude-code",
            "--managed-settings-snippet",
        ])
        .env("HOME", home)
        .output()
        .expect("managed-settings-snippet");
    assert!(
        out.status.success(),
        "snippet: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "snippet is not JSON: {e}\n{}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    value["hooks"][event][0]["hooks"][0]["command"]
        .as_str()
        .unwrap_or_else(|| panic!("no {event} command in {value}"))
        .to_string()
}

fn spool_lines(home: &Path) -> Vec<serde_json::Value> {
    let path = home
        .join(".thruline")
        .join("spool")
        .join("claude-code.jsonl");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("spool line is JSON"))
        .collect()
}

/// Run a snippet command string the way a harness does: through a shell.
fn fire(command: &str, home: &Path, bin: &Path, payload: &str) -> std::process::Output {
    let mut child = clean(&mut Command::new("sh"))
        .arg("-c")
        .arg(command)
        .env("HOME", home)
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the hook");
    {
        use std::io::Write;
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(payload.as_bytes())
            .expect("write payload");
    }
    child.wait_with_output().expect("hook output")
}

#[test]
fn a_managed_snippet_generated_under_one_home_records_under_the_other() {
    // The regression, stated as the org sees it: alice generates the snippet,
    // the fleet pushes it, bob runs a session. Bob's line must be in bob's
    // spool.
    let dir = tempfile::tempdir().expect("tempdir");
    let alice = dir.path().join("alice");
    let bob = dir.path().join("bob");
    std::fs::create_dir_all(&alice).expect("alice");
    std::fs::create_dir_all(&bob).expect("bob");
    let bin = bin_dir(dir.path());

    let command = snippet_command_for("Stop", &alice);
    let out = fire(
        &command,
        &bob,
        &bin,
        r#"{"session_id":"bob-session","hook_event_name":"Stop","cwd":"/tmp"}"#,
    );
    assert!(
        out.status.success(),
        "the hook must exit 0 whatever happens; it exited {:?}: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );

    let bobs = spool_lines(&bob);
    let alices = spool_lines(&alice);
    assert_eq!(
        alices.len(),
        0,
        "bob's session was recorded in ALICE's spool — the snippet carried the \
         generating machine's home: {alices:?}"
    );
    assert_eq!(
        bobs.len(),
        1,
        "bob's session reached no spool of bob's. alice's holds {}: {bobs:?}",
        alices.len()
    );
    assert_eq!(
        bobs[0]["session_id"], "bob-session",
        "the line in bob's spool is not bob's session: {}",
        bobs[0]
    );
}

#[test]
fn the_snippet_still_honours_the_running_machines_hook_spool() {
    // The other half of dropping `--spool`: a machine that has deliberately
    // moved its spool must still be obeyed. If the unpinned hook fell back to
    // `$HOME/.thruline/spool` only, such a machine would end up with the hook
    // writing one file and its exec shim another — two chains, and no chain
    // holding both a `Stop` and the execs it bounds. That is the failure
    // `--spool` was pinned for in the first place, and it must not come back
    // through this door.
    let dir = tempfile::tempdir().expect("tempdir");
    let alice = dir.path().join("alice");
    let bob = dir.path().join("bob");
    std::fs::create_dir_all(&alice).expect("alice");
    std::fs::create_dir_all(&bob).expect("bob");
    let bin = bin_dir(dir.path());
    let elsewhere = dir.path().join("elsewhere").join("codex-and-hooks.jsonl");

    let command = snippet_command_for("SessionStart", &alice);
    let out = clean(&mut Command::new("sh"))
        .arg("-c")
        .arg(&command)
        .env("HOME", &bob)
        .env("THRULINE_HOOK_SPOOL", &elsewhere)
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            use std::io::Write;
            c.stdin
                .as_mut()
                .expect("stdin")
                .write_all(br#"{"session_id":"moved","hook_event_name":"SessionStart"}"#)?;
            c.wait_with_output()
        })
        .expect("hook output");
    assert!(out.status.success());

    let text = std::fs::read_to_string(&elsewhere)
        .unwrap_or_else(|e| panic!("THRULINE_HOOK_SPOOL was ignored by the unpinned hook: {e}"));
    assert!(
        text.contains("moved"),
        "the moved spool holds no line: {text}"
    );
    assert_eq!(
        spool_lines(&bob).len(),
        0,
        "the line was written to the default spool as well as the named one"
    );
}

#[test]
fn a_local_install_still_pins_the_spool_it_resolved() {
    // The pin is not being retired — it is being scoped. On the machine that
    // ran the installer, the settings file must still say on its face where the
    // record goes, because that machine IS the machine the hook fires on.
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).expect("home");
    let settings = dir.path().join("settings.json");
    let out = clean(&mut Command::new(thruline()))
        .args(["hook-install", "--harness", "claude-code", "--settings"])
        .arg(&settings)
        .arg("--json")
        .env("HOME", &home)
        .output()
        .expect("hook-install");
    assert!(
        out.status.success(),
        "install: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = std::fs::read_to_string(&settings).expect("settings");
    assert!(
        text.contains("--spool"),
        "a local install stopped pinning its spool: {text}"
    );
    assert!(
        text.contains(&home.join(".thruline").display().to_string()),
        "a local install pinned a spool that is not this machine's: {text}"
    );
}
