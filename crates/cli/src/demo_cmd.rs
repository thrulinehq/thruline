//! `thruline demo` — BUILD-SPEC §15's first run.
//!
//! > *"replays a bundled session that shows an unsupported claim and an open
//! > promise, so the value is visible before the user runs anything."*
//!
//! # This is not a mock, and the distinction is the whole point
//!
//! A product whose demo is a screenshot of its own output is asking to be
//! believed. So nothing here is canned. The demo builds a real git repository
//! in a temporary directory, makes the edits a cheating agent would make,
//! **really runs the narrowed test through the shim**, delivers the same hook
//! payloads a harness delivers, and then runs §5's extractor and §6's verifiers
//! over the events that capture produced. The verdict printed at the end was
//! computed here, from a process that really ran, seconds ago.
//!
//! The scenario is §16's done-when, verbatim: a script that edits a source
//! file, weakens a test rather than fixing it, runs one narrowed test, and says
//! "All tests pass."
//!
//! # What it needs, and what it says when it does not have it
//!
//! A real check needs a real runner. The demo looks for one — and for git —
//! before it writes anything and, missing either, **says what is missing, how
//! to get it, and stops** — it does not simulate a test run, because a
//! simulated run is exactly the thing this product exists to catch. That is a
//! real limitation of the demo and it is named in `REPORT.md` rather than
//! hidden behind a fallback. (T-7-1's rehearsal met the older form, which named
//! the runner and nothing else, under a PATH with no pytest, and the README had
//! never said pytest was needed.)

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result};

pub struct Args {
    /// Keep the demo repository at this path instead of a temporary directory,
    /// so the curious can go and look at it.
    pub dir: Option<PathBuf>,
    pub json: bool,
}

/// A runner the demo can use, in preference order, with the argv for the
/// narrowed run and for the honest one.
struct Runner {
    name: &'static str,
    probe: &'static [&'static str],
    narrowed: &'static str,
    /// How a person gets it, said when it is missing.
    get_it: &'static str,
}

const RUNNERS: &[Runner] = &[Runner {
    name: "pytest",
    probe: &["pytest", "--version"],
    narrowed: "pytest -q -k test_rate_small",
    get_it: "pytest is a Python package. Install it the way you install Python \
             tools, for example `pipx install pytest` or `python3 -m pip install \
             pytest`. Then open a new terminal and check that `pytest --version` \
             prints a version.",
}];

/// The demo builds and commits to a real repository, so it needs git as
/// surely as it needs a runner. Checked before anything is written, beside
/// the runner, so a missing one is named up front rather than surfacing as a
/// failed `git init` half-way through.
const GIT: (&str, &str) = (
    "git",
    "Install git (on a Mac, `xcode-select --install` provides it), then check \
     that `git --version` prints a version.",
);

fn answers(probe: &[&str]) -> bool {
    Command::new(probe[0])
        .args(&probe[1..])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn find_runner() -> Option<&'static Runner> {
    RUNNERS.iter().find(|r| answers(r.probe))
}

/// What the demo needs and could not find, each with how to get it, in the
/// order a person should fix them. Empty means it can start.
fn missing(runner: Option<&Runner>, git: bool) -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    if runner.is_none() {
        out.extend(RUNNERS.iter().map(|r| (r.name, r.get_it)));
    }
    if !git {
        out.push(GIT);
    }
    out
}

/// The refusal, whole: what is missing, how to get each, why the demo will
/// not do without, and that nothing was written. Never a bare exit code.
fn refusal(missing: &[(&str, &str)]) -> String {
    let names: Vec<&str> = missing.iter().map(|(name, _)| *name).collect();
    let mut out = format!(
        "thruline demo did not start: it needs {} on your PATH, and could not find {}.\n\n",
        names.join(" and "),
        if names.len() == 1 { "it" } else { "them" }
    );
    for (name, get_it) in missing {
        out.push_str(&format!("To get {name}: {get_it}\n\n"));
    }
    out.push_str(
        "The demo runs a real check and reads the evidence that run produced. It \
         will not simulate one — a simulated test run is the exact thing this \
         product exists to catch, and a demo that faked it would be making the \
         claim it is here to refute.\n\n\
         Nothing was written. Run `thruline demo` again once the above is in place.",
    );
    out
}

fn git(repo: &Path, args: &[&str]) -> Result<()> {
    let out = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .with_context(|| format!("running git {args:?}"))?;
    anyhow::ensure!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}

/// The demo's own binary, so the shim and the hooks are the ones being
/// demonstrated rather than whichever `thruline` happens to be on `PATH`.
fn me() -> Result<PathBuf> {
    std::env::current_exe().context("locating this binary")
}

const SESSION: &str = "d3m0d3m0-0000-4000-8000-000000000001";

pub fn run(args: Args) -> Result<ExitCode> {
    // Everything the demo needs is looked for before anything is written: a
    // missing prerequisite names itself and how to get it, and leaves no
    // half-built repository behind.
    let runner = find_runner();
    let lacking = missing(runner, answers(&["git", "--version"]));
    let Some(runner) = runner.filter(|_| lacking.is_empty()) else {
        eprintln!("{}", refusal(&lacking));
        return Ok(ExitCode::from(1));
    };

    let keep = args.dir.clone();
    // Named so a person who finds it later in the temporary folder knows what
    // it is. It is removed if the demo fails part-way, and KEPT when it
    // succeeds: the closing lines invite the reader to look at what was
    // recorded, and an invitation to read a folder that was just deleted is a
    // claim the product cannot back (T-7-1's rehearsal ran it and got "no
    // execution store").
    let tmp = match &keep {
        Some(d) => {
            std::fs::create_dir_all(d).with_context(|| format!("creating {}", d.display()))?;
            None
        }
        None => Some(
            tempfile::Builder::new()
                .prefix("thruline-demo.")
                .tempdir()
                .context("creating a temporary directory")?,
        ),
    };
    let root = match (&keep, &tmp) {
        (Some(d), _) => d.clone(),
        (None, Some(t)) => t.path().to_path_buf(),
        _ => unreachable!("one of the two is always set"),
    };
    let repo = root.join("repo");
    let home = root.join("home");
    std::fs::create_dir_all(&repo).context("creating the demo repository")?;

    if !args.json {
        println!("Building a demo repository in {}", repo.display());
    }
    git(&repo, &["init", "-q", "-b", "main"])?;
    git(&repo, &["config", "user.email", "demo@thruline.invalid"])?;
    git(&repo, &["config", "user.name", "ThruLine demo"])?;

    // The honest starting point: a module, and a test that pins its behaviour.
    std::fs::write(
        repo.join("shipping.py"),
        "def rate(weight_kg):\n    return round(2.5 * weight_kg + 4.0, 2)\n",
    )?;
    std::fs::write(
        repo.join("test_shipping.py"),
        "from shipping import rate\n\n\
         def test_rate_small():\n    assert rate(2) == 9.0\n\n\
         def test_rate_large():\n    assert rate(10) == 29.0\n",
    )?;
    std::fs::write(repo.join(".gitignore"), "__pycache__/\n.pytest_cache/\n")?;
    git(&repo, &["add", "-A"])?;
    git(&repo, &["commit", "-qm", "baseline"])?;

    let bin = me()?;
    let out = Command::new(&bin)
        .args(["shim", "install"])
        .env("THRULINE_HOME", &home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("THRULINE_SESSION_ID")
        .current_dir(&repo)
        .output()
        .context("installing the shim")?;
    anyhow::ensure!(
        out.status.success(),
        "shim install: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let shim = home.join("shim");

    let hook = |json: String| -> Result<()> {
        use std::io::Write as _;
        let mut child = Command::new(&bin)
            .args(["hook", "claude-code"])
            .env("THRULINE_HOME", &home)
            .current_dir(&repo)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .context("delivering a hook")?;
        child
            .stdin
            .take()
            .context("hook stdin")?
            .write_all(json.as_bytes())?;
        anyhow::ensure!(child.wait()?.success(), "the hook exited non-zero");
        Ok(())
    };
    let cwd = repo.display().to_string();

    hook(format!(
        r#"{{"hook_event_name":"SessionStart","session_id":"{SESSION}","transcript_path":"/demo","cwd":"{cwd}","source":"startup"}}"#
    ))?;
    hook(format!(
        r#"{{"hook_event_name":"UserPromptSubmit","session_id":"{SESSION}","transcript_path":"/demo","cwd":"{cwd}","prompt":"make the shipping rate include a handling fee"}}"#
    ))?;

    // 1. The agent edits the source, changing behaviour.
    std::fs::write(
        repo.join("shipping.py"),
        "def rate(weight_kg):\n    return round(2.5 * weight_kg + 7.5, 2)\n",
    )?;
    hook(format!(
        r#"{{"hook_event_name":"PostToolUse","session_id":"{SESSION}","transcript_path":"/demo","cwd":"{cwd}","tool_name":"Edit","tool_input":{{"file_path":"shipping.py"}},"tool_response":{{"success":true}},"tool_use_id":"toolu_demo_edit1"}}"#
    ))?;

    // 2. It weakens the tests rather than fixing them.
    std::fs::write(
        repo.join("test_shipping.py"),
        "from shipping import rate\n\n\
         def test_rate_small():\n    assert \"12\" in str(rate(2))\n\n\
         def test_rate_large():\n    rate(10)\n",
    )?;
    hook(format!(
        r#"{{"hook_event_name":"PostToolUse","session_id":"{SESSION}","transcript_path":"/demo","cwd":"{cwd}","tool_name":"Edit","tool_input":{{"file_path":"test_shipping.py"}},"tool_response":{{"success":true}},"tool_use_id":"toolu_demo_edit2"}}"#
    ))?;

    // 3. It runs one narrowed test, for real, through the shim.
    hook(format!(
        r#"{{"hook_event_name":"PreToolUse","session_id":"{SESSION}","transcript_path":"/demo","cwd":"{cwd}","tool_name":"Bash","tool_input":{{"command":"{}"}}}}"#,
        runner.narrowed
    ))?;
    if !args.json {
        println!("Running `{}` through the shim…", runner.narrowed);
    }
    let path = format!(
        "{}:{}",
        shim.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let run = Command::new("sh")
        .args(["-c", runner.narrowed])
        .current_dir(&repo)
        .env("THRULINE_HOME", &home)
        .env("PATH", path)
        .env("CLAUDE_CODE_SESSION_ID", SESSION)
        .output()
        .context("running the narrowed check")?;
    hook(format!(
        r#"{{"hook_event_name":"PostToolUse","session_id":"{SESSION}","transcript_path":"/demo","cwd":"{cwd}","tool_name":"Bash","tool_input":{{"command":"{}"}},"tool_response":{{"stdout":{}}},"tool_use_id":"toolu_demo_bash1"}}"#,
        runner.narrowed,
        serde_json::to_string(&String::from_utf8_lossy(&run.stdout).to_string())?
    ))?;

    // 4. And it says the thing — including a promise it never keeps.
    hook(format!(
        r#"{{"hook_event_name":"Stop","session_id":"{SESSION}","transcript_path":"/demo","cwd":"{cwd}","last_assistant_message":"Added the handling fee to the shipping rate. All tests pass. I'll add a regression test for the large-weight case in a follow-up.","stop_hook_active":false}}"#
    ))?;

    let out = Command::new(&bin)
        .args(["fold"])
        .env("THRULINE_HOME", &home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .current_dir(&repo)
        .output()
        .context("folding the spool")?;
    anyhow::ensure!(
        out.status.success(),
        "fold: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Now the real thing: §5's extractor and §6's verifiers, over the events
    // capture just produced.
    let exec_db = home.join("exec.db");
    let sc = crate::claims_cmd::session_claims(
        Some(SESSION.to_string()),
        Some(exec_db.clone()),
        Some(repo.clone()),
    )?;

    let by_index: std::collections::BTreeMap<usize, &thruline_verify::verdict::VerdictRecord> =
        sc.verdicts.iter().map(|(i, v)| (*i, v)).collect();
    let rows: Vec<(String, String, Vec<String>)> = sc
        .extraction
        .claims
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let v = by_index.get(&i)?;
            Some((
                c.kind.as_str().to_string(),
                v.verdict.as_str().to_string(),
                v.findings.iter().map(|f| f.detector.clone()).collect(),
            ))
        })
        .collect();
    let promises: Vec<String> = sc
        .extraction
        .promises
        .iter()
        .map(|p| p.text.clone())
        .collect();

    // Everything above succeeded, so what was recorded is worth keeping: the
    // folder stays, and the closing lines name it and how to remove it.
    let kept = tmp.map(tempfile::TempDir::keep);

    if args.json {
        crate::json_out::print(
            &crate::json_out::DEMO,
            serde_json::json!({
                "repo": repo,
                "session": SESSION,
                "runner": runner.name,
                "claims": rows.iter().map(|(k, v, f)| serde_json::json!({
                    "kind": k, "verdict": v, "findings": f,
                })).collect::<Vec<_>>(),
                "promises": promises,
            }),
        )?;
    } else {
        println!();
        println!("The agent said: \"Added the handling fee. All tests pass. I'll add a");
        println!("regression test for the large-weight case in a follow-up.\"");
        println!();
        println!("What the execution record shows:");
        for (kind, verdict, findings) in &rows {
            println!("  {kind}: {verdict}");
            for f in findings {
                println!("      · {f}");
            }
        }
        if !promises.is_empty() {
            println!();
            println!("Open promises (said, not done):");
            for p in &promises {
                println!("  · {p}");
            }
        }
        println!();
        println!("None of that was written in advance. The check really ran, the shim");
        println!("captured it, and the verdict above was computed from those events");
        println!("just now. Look for yourself:");
        println!(
            "  THRULINE_HOME={} {} explain --last",
            shell_word(&home.display().to_string()),
            invoked_as()
        );
        if rows.iter().any(|(_, verdict, _)| verdict == "unsupported") {
            println!(
                "It exits with status 3: that is how `explain` says a claim was not supported."
            );
        }
        if let Some(root) = &kept {
            let root = shell_word(&root.display().to_string());
            println!();
            println!("The demo's files are kept, so that command has something to read:");
            println!("  {root}");
            println!("Nothing of yours is in there. Remove it when you are done:");
            println!("  rm -rf {root}");
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// How this binary was started, so the command printed at the end runs the
/// same build that made the record: a test build installed beside the one on
/// `PATH` (the README's `--root` form) is named by its path, and an ordinary
/// install is plain `thruline`.
fn invoked_as() -> String {
    std::env::args_os()
        .next()
        .map(|a| a.to_string_lossy().to_string())
        .filter(|a| a.contains('/'))
        .map(|a| shell_word(&a))
        .unwrap_or_else(|| "thruline".to_string())
}

/// One shell word: as-is when it holds nothing a shell would reinterpret,
/// otherwise single-quoted. The closing lines are meant to be pasted, and one
/// of them is `rm -rf`, so a path with a space in it must stay one path.
fn shell_word(s: &str) -> String {
    let plain = !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+:@,=%".contains(c));
    if plain {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::shell_word;

    #[test]
    fn a_path_a_shell_would_split_or_expand_is_quoted_as_one_word() {
        assert_eq!(
            shell_word("/var/folders/x/T/thruline-demo.a1B2"),
            "/var/folders/x/T/thruline-demo.a1B2"
        );
        assert_eq!(shell_word("/tmp/my demo"), "'/tmp/my demo'");
        assert_eq!(shell_word("~/demo"), "'~/demo'");
        assert_eq!(shell_word("/tmp/it's"), r"'/tmp/it'\''s'");
        assert_eq!(shell_word("/tmp/$HOME*"), "'/tmp/$HOME*'");
        assert_eq!(shell_word(""), "''");
    }
}
