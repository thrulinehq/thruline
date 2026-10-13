//! `thruline run -- <agent command>` — BUILD-SPEC §4.4, first sentence.
//!
//! > *"Spawn the agent under a PTY; record the child process tree."*
//!
//! # What this buys, and it is not convenience
//!
//! §4.4 calls the generic recorder *"the harness-agnostic fallback and the
//! source of truth"*. This is its front door: no hooks, no vendor contract, no
//! settings file. Point it at any agent — Cursor, Copilot CLI, OpenCode, a
//! shell script — and the runners it invokes are recorded through the same
//! shim every other path uses.
//!
//! The PTY is a correctness question rather than a cosmetic one. Many agents
//! behave differently when they believe they are not on a terminal: they drop
//! progress rendering, change buffering, or refuse to run interactively.
//! Recording an agent under a pipe records **a different agent** from the one
//! the user runs, and a flight recorder whose presence changes the flight is
//! not a flight recorder.
//!
//! # It closes a standing disclosure
//!
//! `stdio` is on every exec event because a terminal changes bytes and §7 must
//! never read that difference as tampering. Until now nothing here produced
//! anything but `"pipe"`, so the capture-shape branch of `Comparison::of` was
//! **unreachable**. Under `thruline run` the shim's children get a real
//! terminal and record `"tty"`; the container records `"pipe"`; the branch is
//! exercised by a real pair of runs.
//!
//! # What is not built, said here rather than found later
//!
//! - **No signal forwarding.** Ctrl-C in raw mode reaches the child as a byte
//!   on the PTY, which is how a terminal delivers it; a `SIGINT` sent to *this*
//!   process is not relayed.
//! - **No `SIGWINCH` propagation.** The child's window size is set once, at
//!   spawn, from the parent's. Resizing the window mid-run does not reach it.
//! - **No controlling terminal.** See `capture::pty`: `setsid` + `TIOCSCTTY`
//!   need `unsafe`, which this workspace forbids. The child gets a terminal
//!   device, not a session leader's terminal.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};

use anyhow::{Context, Result};

pub struct Args {
    /// Everything after `--`.
    pub command: Vec<String>,
    /// Session id recorded execs are attributed to. Derived when absent.
    pub session: Option<String>,
    /// Which harness's spool this run joins. One session's lines must live in
    /// one spool.
    pub harness: String,
    pub repo: Option<PathBuf>,
    /// Force pipes even under a terminal, for scripting.
    pub no_pty: bool,
    /// Write the report here as JSON.
    ///
    /// **The only unambiguous way to read it.** This command is a pass-through:
    /// the child owns stdout and stderr, so a report printed to either shares
    /// the stream with whatever the agent wrote. A file has no such contention.
    pub report: Option<PathBuf>,
    pub json: bool,
}

pub fn run(args: Args) -> Result<ExitCode> {
    let Some((program, rest)) = args.command.split_first() else {
        anyhow::bail!("nothing to run: `thruline run -- <agent command>`");
    };
    let home = crate::shim_cmd::thruline_home()?;
    let harness = thruline_capture::harness::by_name(&args.harness)
        .with_context(|| format!("no harness named {:?}", args.harness))?;
    let repo = args
        .repo
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));

    // A derived id, not a random one: two runs of the same command at the same
    // instant in the same repository are the same run, and a re-run must not
    // silently create a second session for evidence that is already stored.
    let started = thruline_core::ingest::iso8601_utc_now();
    let owns_session = args.session.is_none();
    let session = args.session.unwrap_or_else(|| {
        format!(
            "run-{}",
            &thruline_core::chain::sha256_hex(
                format!(
                    "{started}|{}|{}",
                    repo.display(),
                    args.command.join("\u{0}")
                )
                .as_bytes()
            )[..32]
        )
    });

    // The shim is what records; this only arranges for it to be found. It is
    // installed pointing at the harness's spool, so this run's execs and any
    // hook deliveries for the same session share one ordinal space.
    let bin = std::env::current_exe().context("locating this binary")?;
    let mut opts = thruline_capture::shim::InstallOptions::under(&home, bin, started.clone());
    opts.spool = thruline_capture::harness::default_spool(&harness)
        .context("locating the harness spool; set THRULINE_HOME or HOME")?;
    opts.session_id = Some(session.clone());
    opts.repo_root = Some(repo.clone());
    let manifest =
        thruline_capture::shim::install(&opts).context("installing the exec recorder")?;
    if owns_session {
        lifecycle(
            &opts.spool,
            &session,
            &args.harness,
            "session_start",
            &started,
        );
    }

    let path = match std::env::var_os("PATH") {
        Some(existing) => {
            let mut joined = std::ffi::OsString::from(manifest.shim_dir.as_os_str());
            joined.push(":");
            joined.push(existing);
            joined
        }
        None => manifest.shim_dir.clone().into_os_string(),
    };

    let mut command = Command::new(program);
    command
        .args(rest)
        .current_dir(&repo)
        .env("PATH", path)
        // Both spellings: the shim reads the harness variable first and its own
        // second, and an agent launched this way may set neither.
        .env(thruline_capture::exec::SESSION_VAR, &session)
        .env(thruline_capture::exec::HARNESS_SESSION_VAR, &session);

    let want_pty = !args.no_pty && thruline_capture::pty::stdout_is_terminal();
    let (code, shape) = if want_pty {
        relay_on_pty(command)?
    } else {
        let status = command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| format!("running {program}"))?;
        (status.code().unwrap_or(1), thruline_capture::pty::PIPE)
    };

    // Only this wrapper's own session ends here. An explicitly joined harness
    // session may still be live, so never close it on a nested child exit.
    if owns_session {
        lifecycle(
            &opts.spool,
            &session,
            &args.harness,
            "session_end",
            &thruline_core::ingest::iso8601_utc_now(),
        );
    }
    // Fold what the run produced, so `explain` and `claims` work immediately.
    let folded = fold_quietly(&repo);

    let report = serde_json::json!({
        "session": session,
        "exit_code": code,
        "stdio": shape,
        "window": thruline_capture::pty::window(),
        "spool": opts.spool,
        "folded": folded,
    });
    if let Some(path) = &args.report {
        // The same document as `--json`, so it carries the same version. An
        // unversioned copy of a versioned shape is the drift this exists
        // against — and this file, not stdout, is what a consumer reads, since
        // stdout is interleaved with the child's own output.
        let versioned = crate::json_out::envelope(&crate::json_out::RUN, report.clone())?;
        std::fs::write(path, serde_json::to_string_pretty(&versioned)?)
            .with_context(|| format!("writing the report to {}", path.display()))?;
    }
    if args.json {
        // Interleaved with the child's own output, necessarily: they share the
        // stream. `--report` is the machine-readable path.
        crate::json_out::print(&crate::json_out::RUN, report.clone())?;
    } else {
        eprintln!("\nthruline: session {session} · child exited {code} · capture shape {shape}");
        eprintln!("  thruline explain --last     what happened");
        eprintln!("  thruline claims  --last     what was claimed, and what the record proves");
    }
    // The child's exit code, unchanged. This command's own opinion belongs on
    // stderr, not in `$?`.
    Ok(ExitCode::from(u8::try_from(code).unwrap_or(1)))
}

/// The wrapper observes its own lifecycle directly. It does not impersonate a
/// vendor hook or promise that every subprocess was captured. A failed write is
/// disclosed and leaves missing evidence; it cannot change the child's exit.
fn lifecycle(spool: &std::path::Path, session: &str, harness: &str, kind: &str, at: &str) {
    let payload =
        serde_json::json!({"kind":kind,"payload":{"reason":"thruline run wrapper lifecycle"}});
    let material = format!("{session}|{at}|{payload}");
    let line = serde_json::json!({"received_at":at,"source":"thruline","harness":harness,
        "session_id":session,"session_binding":"run-wrapper","spec_payload":payload,
        "raw_sha256":thruline_core::chain::sha256_hex(material.as_bytes())});
    if let Err(error) = thruline_capture::claude_code::append_line(spool, &format!("{line}\n")) {
        eprintln!(
            "thruline: wrapper lifecycle was not recorded: {}",
            thruline_capture::redaction::Redactor::new()
                .redact(&error.to_string())
                .bounded(240)
                .text()
        );
    }
}

/// Run the child on a pseudoterminal, relaying both directions.
#[cfg(unix)]
fn relay_on_pty(command: Command) -> Result<(i32, &'static str)> {
    let mut pty =
        thruline_capture::pty::spawn_on_pty(command).context("allocating a pseudoterminal")?;
    // Restored by `Drop`, including on an early return: a terminal left raw is
    // a terminal the user types into blind.
    let _raw = thruline_capture::pty::RawMode::enter();

    // Parent stdin → the child's terminal, on its own thread so a child that
    // never reads cannot block the output relay.
    let writer = pty.controller.try_clone().context("duplicating the pty")?;
    let feeder = std::thread::spawn(move || {
        let mut writer = writer;
        let mut buf = [0u8; 4096];
        let mut stdin = std::io::stdin().lock();
        loop {
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if writer.write_all(&buf[..n]).is_err() || writer.flush().is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut buf = [0u8; 64 * 1024];
    loop {
        match pty.controller.read(&mut buf) {
            // A controller read after the last user-side fd closes is EOF on
            // some platforms and `EIO` on others. Both mean the child is done
            // writing.
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let mut out = std::io::stdout().lock();
                let _ = out.write_all(&buf[..n]);
                let _ = out.flush();
            }
        }
    }
    let status = pty.child.wait().context("waiting for the child")?;
    // The feeder is blocked on the parent's stdin, which may never close. It is
    // detached deliberately: joining it would hang every run whose agent exits
    // while the user's terminal is still open.
    drop(feeder);
    Ok((status.code().unwrap_or(1), thruline_capture::pty::TTY))
}

#[cfg(not(unix))]
fn relay_on_pty(command: Command) -> Result<(i32, &'static str)> {
    // Tier 2 (§2): no pseudoterminal, and the shape recorded says so.
    let status = command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("running the child")?;
    Ok((status.code().unwrap_or(1), thruline_capture::pty::PIPE))
}

/// Fold the spool, reporting what it did rather than whether it errored.
///
/// A fold failure must not change this command's exit code: `$?` belongs to the
/// child, and a user who ran an agent successfully has not failed because
/// ThruLine could not write a row. The failure is printed.
fn fold_quietly(repo: &std::path::Path) -> serde_json::Value {
    match crate::shim_cmd::fold_report(repo) {
        Ok(report) => serde_json::json!({
            "events_written": report.events_written,
            "checks_written": report.checks_written,
            "quarantined": report.quarantined.len(),
        }),
        Err(e) => {
            eprintln!("thruline: the run was recorded but could not be folded: {e:#}");
            serde_json::json!({ "error": e.to_string() })
        }
    }
}
