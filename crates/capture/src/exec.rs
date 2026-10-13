//! BUILD-SPEC §4.4 — the generic exec recorder behind the PATH shim.
//!
//! One wrapped command produces up to four spool lines, in this order:
//!
//! | line | kind | why it exists |
//! |---|---|---|
//! | 1 | `note` (`exec_begin`) | so an interrupted command is a *counted* gap rather than a silent one |
//! | 2 | `snapshot` | the tree the command ran against (§6.1 binding) |
//! | 3 | `exec` | argv, exit code, output digests, bounded excerpts (§3.1) |
//! | 4 | `snapshot` | the tree it left behind |
//!
//! # Transparency is the first requirement, not the last
//!
//! A wrapper that changes what its caller observes is worse than no wrapper.
//! In particular a wrapper that **eats a non-zero exit code destroys exactly
//! the evidence this product exists to collect** — it is the same shape as the
//! `PostToolUse` finding in BUILD-PLAN B.1, where subscribing only to the
//! success event would have captured only the test runs that passed. So:
//!
//! - the child's exit code is returned unchanged;
//! - stdin is inherited, so an interactive runner still reads the terminal;
//! - stdout and stderr are written through **raw and unmodified**, on their own
//!   descriptors, in arrival order per stream — redaction applies to what
//!   ThruLine *stores*, never to what the user sees on their own terminal;
//! - a child killed by a signal makes the wrapper exit `128 + signal`, which is
//!   the status a POSIX shell reports for signal death, and the signal number is
//!   recorded on the event.
//!
//! **The one difference a caller can observe** is that the child's stdout and
//! stderr are pipes rather than the terminal, so a runner that colourises only
//! on a TTY will not. Removing that difference needs a PTY, which is §4.4's
//! `thruline run` (M4.5). It is stated here rather than left to be discovered:
//! it also means `isatty`-gated progress output is absent from what is stored.
//!
//! # What the recorder is not
//!
//! It is not a sandbox and not a policy point. It never blocks, never edits a
//! command, never refuses to run one. If any part of recording fails — no git
//! repository, an unwritable spool, an output store that cannot be created —
//! the command still runs and the failure is **counted onto the event**, per
//! Rule 12 and BUILD-PLAN C.7: a snapshot that cannot be taken makes claims
//! `unverifiable`, and that is only true if somebody wrote down that it could
//! not be taken.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;
use thruline_core::spec::event::{Exec, OpenPayload, Payload, SnapshotPayload};
use thruline_core::spec::snapshot::{snapshot_tree, SnapshotError, TreeSnapshot};

use crate::redaction::Redactor;
use crate::shim::ShimManifest;
use crate::stream::{CaptureSink, Captured};

/// Marker every generated wrapper carries. Used to recognise our own scripts
/// while resolving the real program, so the recorder can never re-enter itself
/// even if `PATH` was rewritten or the shim directory was copied elsewhere.
///
/// A **content** check rather than a path comparison, deliberately: a path
/// comparison is defeated by a symlink, a copy, or a second shim directory, and
/// the failure mode it guards against is an unbounded fork bomb on the user's
/// machine.
pub const WRAPPER_MARKER: &str = "ThruLine exec shim v";

/// How deep a nest of wrapped programs may go before the recorder stops
/// recording and simply runs the command. `make` → `npm` → `pytest` is depth 3;
/// anything past this is a loop nobody intended.
pub const MAX_DEPTH: u32 = 16;

/// Env var carrying the nesting depth to child recorders.
pub const DEPTH_VAR: &str = "THRULINE_EXEC_DEPTH";

/// Env var naming the programs currently being recorded in this ancestry.
///
/// Set on every child. The **wrapper** reads it and fails open when its own
/// program is already in the set, which is what breaks a cycle through a
/// third-party shim (pyenv, asdf, rbenv, ccache) that re-dispatches the bare
/// program name through `PATH`. See [`crate::shim::ACTIVE_VAR`] for why the
/// guard is keyed on the program rather than on depth alone.
pub const ACTIVE_VAR: &str = "THRULINE_SHIM_ACTIVE";

/// Env var that overrides the session an exec is attributed to.
pub const SESSION_VAR: &str = "THRULINE_SESSION_ID";

/// The harness's own session id, exported into the shell it runs tools in.
///
/// **This is what makes attribution correct rather than probable.** Verified in
/// a live Claude Code tool shell: present, 36 characters, UUID-v4 shaped, the
/// same shape as a `sessions.id` row, and inherited by nested shells by
/// construction — so a command run five processes deep still carries it.
///
/// Before this was read, a default install had exactly one route open:
/// `InstallOptions::under` leaves `session_id` `None`, nothing passes
/// `--session`, and `THRULINE_SESSION_ID` was set by nobody. **Every exec on a
/// default install took `spool-latest`**, which attributes an exec to whichever
/// session wrote to the spool most recently — measured at **49.7% misbind**
/// with two equally active sessions: 662 execs, 329 attributed to a session
/// that never ran them.
pub const HARNESS_SESSION_VAR: &str = "CLAUDE_CODE_SESSION_ID";

/// How the child's stdout and stderr were connected.
///
/// **Pinned on the event because it changes the bytes.** A runner that
/// colourises or draws progress only on a TTY produces different output here
/// than it does unwrapped — pytest's terminal width and progress line among
/// them. That costs nothing today, and it is a trap for §7: when M3 re-executes
/// a check and compares `stdout_hash`, a TTY/non-TTY difference would surface
/// as a mismatch that is not one, i.e. as evidence of tampering where there is
/// none. Recording the mode is what lets that comparison be conditioned on it
/// rather than fooled by it.
///
/// **Measured, not declared.** This used to be `const STDIO_MODE = "pipe"`, and
/// under §7's comparison that made both sides equal by construction: the
/// capture-shape branch could not be reached however the run was made. Under
/// `thruline run` (§4.4) the recorder itself is on a terminal, so its children
/// are too, and the recorded shape is `"tty"`.
pub fn stdio_mode() -> &'static str {
    crate::pty::stdio_mode()
}

/// The debounce §4.4 asks for: a *post* snapshot identical to the *pre* one is
/// not emitted as a second event when the command took less than this.
///
/// It suppresses a duplicate **event**, never a measurement — both snapshots
/// are always taken, and the exec carries both tree hashes, so §6.1 binding is
/// unaffected by whether the second event was written. Debouncing the
/// measurement instead would bind a check to a tree it did not run on, which is
/// the one error this whole subsystem exists to prevent.
pub const SNAPSHOT_DEBOUNCE_MS: u128 = 200;

/// The session an exec was attributed to, and how.
///
/// Recorded on the event because attributing evidence to the wrong session is
/// not a small error, and the three routes are not equally trustworthy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionBinding {
    /// `$THRULINE_SESSION_ID` — the user or an integration said so explicitly.
    Env,
    /// `$CLAUDE_CODE_SESSION_ID` — the harness said so, in the shell it runs
    /// tools in. The only route that is correct rather than probable under
    /// concurrency.
    HarnessEnv,
    /// The shim manifest, written by `thruline shim install --session`.
    Manifest,
    /// The most recent `session_id` in the spool. A heuristic: correct for one
    /// agent session at a time on one machine, and wrong if two harnesses share
    /// a spool. Named on the event so a reader can discount it.
    SpoolLatest,
    /// Nothing said. The exec is recorded under a synthetic per-day session so
    /// it is still in the record rather than discarded.
    Unbound,
}

impl SessionBinding {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionBinding::Env => "env",
            SessionBinding::HarnessEnv => "harness-env",
            SessionBinding::Manifest => "manifest",
            SessionBinding::SpoolLatest => "spool-latest",
            SessionBinding::Unbound => "unbound",
        }
    }
}

/// Everything the recorder needs. Built by the CLI from the manifest.
#[derive(Debug, Clone)]
pub struct RecordOptions {
    /// The program name as invoked (`pytest`), not a path.
    pub program: String,
    pub args: Vec<String>,
    pub shim_dir: Option<PathBuf>,
    pub spool: Option<PathBuf>,
    pub output_store: Option<PathBuf>,
    pub session_id: Option<String>,
    pub session_binding: SessionBinding,
    pub repo_root: Option<PathBuf>,
}

impl RecordOptions {
    /// Resolve everything from the environment the wrapper set up.
    ///
    /// A missing or unreadable manifest is not fatal: the command must still
    /// run, and a recording with no spool is simply not recorded. That is the
    /// fail-open half of the contract.
    pub fn from_env(program: String, args: Vec<String>) -> RecordOptions {
        let shim_dir = std::env::var_os("THRULINE_SHIM_DIR").map(PathBuf::from);
        let manifest = shim_dir.as_deref().and_then(|d| ShimManifest::load(d).ok());
        let (session_id, binding) = resolve_session(manifest.as_ref());
        RecordOptions {
            program,
            args,
            spool: manifest.as_ref().map(|m| m.spool.clone()),
            output_store: manifest.as_ref().map(|m| m.output_store.clone()),
            repo_root: manifest.as_ref().and_then(|m| m.repo_root.clone()),
            session_id,
            session_binding: binding,
            shim_dir,
        }
    }
}

/// The explicit override, then the harness's own id, then the manifest, then —
/// only if all three are silent — the spool's most recent session.
///
/// The order is by how much each route can be wrong. The first two are
/// statements by something that knows; the manifest is a statement made at
/// install time; `spool-latest` is a guess whose error rate is the other
/// sessions' tool-call rate.
fn resolve_session(manifest: Option<&ShimManifest>) -> (Option<String>, SessionBinding) {
    if let Some(id) = std::env::var(SESSION_VAR).ok().filter(|s| !s.is_empty()) {
        return (Some(id), SessionBinding::Env);
    }
    if let Some(id) = std::env::var(HARNESS_SESSION_VAR)
        .ok()
        .filter(|s| !s.is_empty())
    {
        return (Some(id), SessionBinding::HarnessEnv);
    }
    if let Some(id) = manifest.and_then(|m| m.session_id.clone()) {
        return (Some(id), SessionBinding::Manifest);
    }
    if let Some(spool) = manifest.map(|m| m.spool.clone()) {
        if let Some(id) = latest_spool_session(&spool) {
            return (Some(id), SessionBinding::SpoolLatest);
        }
    }
    (None, SessionBinding::Unbound)
}

/// The last `session_id` written to the spool.
///
/// Reads the tail rather than the whole file: a spool is append-only and can be
/// large, and only its end is relevant.
pub fn latest_spool_session(spool: &Path) -> Option<String> {
    use std::io::{Seek, SeekFrom};
    let mut file = std::fs::File::open(spool).ok()?;
    let len = file.metadata().ok()?.len();
    let want = 64 * 1024u64;
    let from = len.saturating_sub(want);
    file.seek(SeekFrom::Start(from)).ok()?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf);
    for line in text.lines().rev() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if let Some(id) = value.get("session_id").and_then(Value::as_str) {
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }
    }
    None
}

/// What one recorded exec turned into. Returned for tests and for `--json`;
/// the authoritative record is the spool.
#[derive(Debug, Clone)]
pub struct ExecReport {
    pub exec_id: String,
    pub session_id: String,
    pub exit_code: i32,
    pub signal: Option<i32>,
    pub resolved: Option<PathBuf>,
    pub input_tree_hash: Option<String>,
    pub output_tree_hash: Option<String>,
    /// Snapshots that were due and could not be taken, with the reason.
    /// **Counted, never dropped** — this is the field whose absence made
    /// `snapshots_taken` read 0 while every other number looked right.
    pub snapshots_failed: Vec<(&'static str, String)>,
    pub post_snapshot_debounced: bool,
    pub spool_error: Option<String>,
    pub stdout: Captured,
    pub stderr: Captured,
    pub lines_spooled: u64,
    /// How the child's standard streams were connected: `pipe` or `tty`.
    /// §7's comparison conditions on this rather than assuming it.
    pub stdio: &'static str,
    /// True when a pseudoterminal carried both output streams, so `stderr` is
    /// an absence rather than a measurement.
    pub streams_merged: bool,
}

/// Record and run one wrapped command. Returns the status this process should
/// exit with.
pub fn record(opts: RecordOptions) -> u8 {
    let depth: u32 = std::env::var(DEPTH_VAR)
        .ok()
        .and_then(|d| d.parse().ok())
        .unwrap_or(0);

    let redactor = repo_redactor();
    let started = thruline_core::ingest::iso8601_utc_now();
    let exec_id = thruline_core::chain::sha256_hex(
        format!(
            "{}|{}|{}|{}",
            started,
            std::process::id(),
            opts.program,
            opts.args.join("\u{1}")
        )
        .as_bytes(),
    )[..32]
        .to_string();

    let session = opts
        .session_id
        .clone()
        .unwrap_or_else(|| unbound_session(&started));

    let mut report = ExecReport {
        exec_id: exec_id.clone(),
        session_id: session.clone(),
        exit_code: 0,
        signal: None,
        resolved: None,
        input_tree_hash: None,
        output_tree_hash: None,
        snapshots_failed: Vec::new(),
        post_snapshot_debounced: false,
        spool_error: None,
        stdout: empty_capture(),
        stderr: empty_capture(),
        lines_spooled: 0,
        // Filled in from the run below, never from a default that reads as a
        // measurement.
        stdio: crate::pty::PIPE,
        streams_merged: false,
    };

    // The full argv as recorded: program name as invoked, then its arguments.
    let mut argv = vec![opts.program.clone()];
    argv.extend(opts.args.iter().cloned());
    let redacted_argv = redact_argv(&redactor, &argv);

    // 1. `exec_begin`, written before anything that can block, so a command
    //    interrupted during its own snapshot still leaves a trace.
    let mut begin: OpenPayload = BTreeMap::new();
    begin.insert("note".into(), Value::String("exec_begin".into()));
    begin.insert("exec_id".into(), Value::String(exec_id.clone()));
    begin.insert("argv".into(), serde_json::json!(redacted_argv));
    begin.insert("depth".into(), serde_json::json!(depth));
    spool(
        &mut report,
        &opts,
        &session,
        "exec-recorder",
        &started,
        Payload::Note(begin),
    );

    // 2. The tree the command is about to run against.
    let repo_root = pick_repo_root(&opts);
    let pre = take_snapshot(repo_root.as_deref());
    let pre_at = std::time::Instant::now();
    match &pre {
        Ok(Some(snap)) => {
            report.input_tree_hash = Some(snap.tree_hash.clone());
            spool(
                &mut report,
                &opts,
                &session,
                "git",
                &started,
                snapshot_payload(snap),
            );
        }
        Ok(None) => {}
        Err(why) => report.snapshots_failed.push(("input", why.clone())),
    }

    // 3. Run it.
    let real = resolve_program(&opts.program, opts.shim_dir.as_deref());
    report.resolved = real.clone();
    let (status, stdout, stderr) = match &real {
        Some(path) if depth < MAX_DEPTH => {
            let (status, out, err, shape, merged) =
                run_child(path, &opts.args, depth, &redactor, &opts);
            report.stdio = shape;
            report.streams_merged = merged;
            (status, out, err)
        }
        Some(path) => {
            // Past the depth cap: run it, record nothing new. Recording here
            // would be the loop feeding itself.
            let code = passthrough(path, &opts.args, depth, &active_with(&opts.program));
            (ChildStatus::Exited(code), empty_capture(), empty_capture())
        }
        None => {
            let _ = writeln!(
                std::io::stderr(),
                "thruline: {}: not found on PATH outside the shim directory",
                opts.program
            );
            (ChildStatus::Exited(127), empty_capture(), empty_capture())
        }
    };
    let ended = thruline_core::ingest::iso8601_utc_now();

    // 4. The tree it left behind.
    let post = take_snapshot(repo_root.as_deref());
    let elapsed = pre_at.elapsed().as_millis();
    let mut post_payload = None;
    match &post {
        Ok(Some(snap)) => {
            report.output_tree_hash = Some(snap.tree_hash.clone());
            let same = report.input_tree_hash.as_deref() == Some(snap.tree_hash.as_str());
            if same && elapsed < SNAPSHOT_DEBOUNCE_MS {
                report.post_snapshot_debounced = true;
            } else {
                post_payload = Some(snapshot_payload(snap));
            }
        }
        Ok(None) => {}
        Err(why) => report.snapshots_failed.push(("output", why.clone())),
    }

    let (code, signal) = status.parts();
    report.exit_code = code;
    report.signal = signal;

    // 5. The exec itself.
    let exec = Exec {
        argv_hash: format!(
            "sha256:{}",
            thruline_core::chain::sha256_hex(redacted_argv.join("\u{1}").as_bytes())
        ),
        argv: redacted_argv.clone(),
        cwd: std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        env_hash: env_hash(&redactor),
        started: started.clone(),
        ended: ended.clone(),
        exit_code: i64::from(code),
        stdout_hash: stdout.hash.clone(),
        stderr_hash: stderr.hash.clone(),
        stdout_excerpt: stdout.excerpt.clone(),
        stderr_excerpt: stderr.excerpt.clone(),
        pid: status.pid(),
        parent: parent_pid(),
        extra: exec_extra(&report, &stdout, &stderr, &argv, depth, signal, &real),
    };
    report.stdout = stdout;
    report.stderr = stderr;
    spool(
        &mut report,
        &opts,
        &session,
        "exec-recorder",
        &ended,
        Payload::Exec(exec),
    );

    if let Some(payload) = post_payload {
        spool(&mut report, &opts, &session, "git", &ended, payload);
    }

    status.exit_status()
}

/// Fields beyond §3.1's minimum. Every one is either provenance for the event
/// or a *counted* degradation; nothing here is decorative.
fn exec_extra(
    report: &ExecReport,
    stdout: &Captured,
    stderr: &Captured,
    argv: &[String],
    depth: u32,
    signal: Option<i32>,
    resolved: &Option<PathBuf>,
) -> BTreeMap<String, Value> {
    let mut extra: BTreeMap<String, Value> = BTreeMap::new();
    extra.insert("exec_id".into(), Value::String(report.exec_id.clone()));
    extra.insert("depth".into(), serde_json::json!(depth));
    extra.insert(
        "program_resolved".into(),
        match resolved {
            Some(p) => Value::String(p.to_string_lossy().to_string()),
            // Null, not an empty string: "PATH held nothing but the shim" is a
            // fact, and an empty string reads as a path.
            None => Value::Null,
        },
    );
    if let Some(sig) = signal {
        // Present exactly when the child died by a signal. `exit_code` is then
        // the wrapper's own `128 + signal`, which is what a POSIX shell reports
        // — labelled, so it is not mistaken for the child's own status.
        extra.insert("signal".into(), serde_json::json!(sig));
        extra.insert(
            "exit_code_source".into(),
            Value::String("wrapper: 128 + signal".into()),
        );
    }
    match &report.input_tree_hash {
        Some(h) => extra.insert("input_tree_hash".into(), Value::String(h.clone())),
        None => extra.insert("input_tree_hash".into(), Value::Null),
    };
    match &report.output_tree_hash {
        Some(h) => extra.insert("output_tree_hash".into(), Value::String(h.clone())),
        None => extra.insert("output_tree_hash".into(), Value::Null),
    };
    if !report.snapshots_failed.is_empty() {
        extra.insert(
            "snapshots_failed".into(),
            serde_json::json!(report
                .snapshots_failed
                .iter()
                .map(|(which, why)| serde_json::json!({"which": which, "why": why}))
                .collect::<Vec<_>>()),
        );
    }
    if report.post_snapshot_debounced {
        extra.insert("post_snapshot_debounced".into(), Value::Bool(true));
    }
    extra.insert("stdio".into(), Value::String(report.stdio.into()));
    if report.streams_merged {
        // A terminal is ONE device: stdout and stderr are no longer separable,
        // so `stderr_hash` is the digest of nothing rather than a measurement.
        // Said on the event, because a reader who took it for a measurement
        // would conclude the command wrote nothing to stderr.
        extra.insert(
            "streams".into(),
            Value::String("merged: one pseudoterminal carries both".into()),
        );
    }
    extra.insert("stdout_bytes".into(), serde_json::json!(stdout.bytes_raw));
    extra.insert("stderr_bytes".into(), serde_json::json!(stderr.bytes_raw));
    if let Some(p) = &stdout.stored_path {
        extra.insert(
            "stdout_store".into(),
            Value::String(p.to_string_lossy().to_string()),
        );
    }
    if let Some(p) = &stderr.stored_path {
        extra.insert(
            "stderr_store".into(),
            Value::String(p.to_string_lossy().to_string()),
        );
    }
    let mut notes: Vec<String> = Vec::new();
    for (which, c) in [("stdout", stdout), ("stderr", stderr)] {
        if let Some(e) = &c.store_error {
            notes.push(format!("{which}: full output not stored: {e}"));
        }
        if c.invalid_utf8 {
            notes.push(format!("{which}: not valid UTF-8; decoded lossily"));
        }
        if c.block_cap_hit {
            notes.push(format!(
                "{which}: a line or unterminated key block exceeded the redaction \
                 block cap, so one block boundary fell mid-line"
            ));
        }
    }
    if !notes.is_empty() {
        extra.insert("capture_notes".into(), serde_json::json!(notes));
    }
    // §13: the parsed form of a shell payload, so a nested runner is visible
    // without pattern-matching the command string.
    match thruline_verify::shell_payload(argv) {
        thruline_verify::ShellPayload::Parsed(commands) if !commands.is_empty() => {
            extra.insert(
                "shell_commands".into(),
                serde_json::json!(commands.iter().map(|c| c.argv.clone()).collect::<Vec<_>>()),
            );
        }
        thruline_verify::ShellPayload::Unparsed { reason, .. } => {
            extra.insert("shell_payload_unparsed".into(), Value::String(reason));
        }
        _ => {}
    }
    extra
}

fn snapshot_payload(snap: &TreeSnapshot) -> Payload {
    let mut extra: BTreeMap<String, Value> = BTreeMap::new();
    // Taken at the exec boundary itself, so unlike a fold-derived snapshot this
    // one really does describe the tree at the moment its `seq` names.
    extra.insert("taken_at".into(), Value::String("exec".into()));
    extra.insert("contemporaneous".into(), Value::Bool(true));
    if snap.content_filters.degrades_capture() {
        extra.insert(
            "content_filters".into(),
            Value::String(snap.content_filters.describe()),
        );
        extra.insert("capture_degraded".into(), Value::Bool(true));
    }
    if !snap.omitted.is_empty() {
        extra.insert(
            "omitted_entries".into(),
            serde_json::json!(snap.omitted.len()),
        );
    }
    Payload::Snapshot(SnapshotPayload {
        tree_hash: snap.tree_hash.clone(),
        head_commit: snap.head_commit.clone(),
        dirty: snap.dirty,
        changed_paths_count: snap.changed_paths_count,
        extra,
    })
}

/// `Ok(None)` means there was nowhere to snapshot; `Err` means there was and it
/// failed, which is a different fact and is counted separately.
fn take_snapshot(root: Option<&Path>) -> Result<Option<TreeSnapshot>, String> {
    let Some(root) = root else {
        return Ok(None);
    };
    match snapshot_tree(root) {
        Ok(snap) => Ok(Some(snap)),
        // Not a repository at all: not a failure, just nothing to bind to.
        Err(SnapshotError::NotARepository { .. }) => Ok(None),
        Err(e) => Err(format!("snapshot of {} failed: {e}", root.display())),
    }
}

/// The working directory if it is inside a repository, else the manifest's.
///
/// Working directory first on purpose: a shim installed at user scope is used
/// across many repositories, and a manifest-fixed root would bind every one of
/// their execs to whichever repository happened to be current at install time.
fn pick_repo_root(opts: &RecordOptions) -> Option<PathBuf> {
    if let Ok(cwd) = std::env::current_dir() {
        return Some(cwd);
    }
    opts.repo_root.clone()
}

/// Redact an argv, letting each element name the one after it.
///
/// Masking elements independently loses the only context argv has. `--password
/// s3cr3t-opaque-value` is two elements: the value matches no shape, so it
/// survived, while the identical text on one line (`password=s3cr3t-…`) is
/// masked by the `.env` rule. Same secret, same evidence, two answers — decided
/// by whether a shell split it.
///
/// So a flag whose name says "a credential follows" makes the next element one,
/// through [`crate::redaction::key_implies_credential`] — **the same predicate
/// the JSON walk and the `.env` rule ask**, so a third caller cannot disagree
/// with the first two. `--token-file` and `--key-path` name a *property* and
/// are excluded by that predicate, not by a list here.
///
/// An element that names no credential and matches no shape still survives, and
/// that is correct: argv is evidence, and an opaque string nothing identifies is
/// not distinguishable from an argument.
pub fn redact_argv(redactor: &Redactor, argv: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(argv.len());
    let mut named_by_previous = false;
    for arg in argv {
        if named_by_previous && !arg.starts_with('-') {
            out.push("[masked: argv_value]".to_string());
            named_by_previous = false;
            continue;
        }
        named_by_previous = arg
            .strip_prefix("--")
            .or_else(|| arg.strip_prefix('-'))
            .filter(|name| !name.contains('='))
            .is_some_and(crate::redaction::key_implies_credential);
        out.push(redactor.redact(arg).into_text());
    }
    out
}

/// A digest over the **redacted** environment.
///
/// Redacting before hashing costs the ability to tell two environments apart
/// when they differ only in a masked value — stated plainly because it is a
/// real loss. It buys two things worth more: §4.6's ordering holds on this path
/// too, and the receipt does not ship a hash that a holder could brute-force a
/// low-entropy secret out of.
fn env_hash(redactor: &Redactor) -> String {
    let mut vars: Vec<String> = std::env::vars_os()
        .map(|(k, v)| format!("{}={}", k.to_string_lossy(), v.to_string_lossy()))
        .collect();
    vars.sort();
    let rendered = vars.join("\n");
    let redacted = redactor.redact(&rendered);
    format!(
        "sha256:{}",
        thruline_core::chain::sha256_hex(redacted.text().as_bytes())
    )
}

/// How the child ended.
enum ChildStatus {
    Exited(i32),
    Signalled { signal: i32, pid: u32 },
    ExitedChild { code: i32, pid: u32 },
}

impl ChildStatus {
    fn parts(&self) -> (i32, Option<i32>) {
        match self {
            ChildStatus::Exited(c) => (*c, None),
            ChildStatus::ExitedChild { code, .. } => (*code, None),
            ChildStatus::Signalled { signal, .. } => (128 + *signal, Some(*signal)),
        }
    }
    fn pid(&self) -> u32 {
        match self {
            ChildStatus::Exited(_) => 0,
            ChildStatus::ExitedChild { pid, .. } | ChildStatus::Signalled { pid, .. } => *pid,
        }
    }
    /// The status this process exits with.
    ///
    /// For a signal death this is `128 + signal`, the value a POSIX shell
    /// reports in `$?`. It is **not** identical to re-raising the signal: a
    /// parent using `waitpid` sees `WIFEXITED` rather than `WIFSIGNALED`.
    /// Re-raising needs `libc::raise`, and `unsafe_code = "forbid"` is a
    /// workspace lint this file will not weaken for a distinction almost
    /// nothing observes. Recorded here so it is a known difference and not a
    /// discovered one.
    fn exit_status(&self) -> u8 {
        let (code, _) = self.parts();
        u8::try_from(code & 0xff).unwrap_or(1)
    }
}

/// Spawn the real program, tee its streams, and wait.
/// Run the child, and say how its streams were connected.
///
/// Returns `(status, stdout, stderr, stdio_shape, streams_merged)`.
///
/// **The shape follows the recorder's own.** A recorder on a terminal that
/// handed its child a pipe would be recording a run that never happened: a
/// runner branches on `isatty`, so the pipe changes the bytes and can change
/// the behaviour. Under `thruline run` (§4.4) this process is on a terminal and
/// the child gets one too.
fn run_child(
    program: &Path,
    args: &[String],
    depth: u32,
    redactor: &Redactor,
    opts: &RecordOptions,
) -> (ChildStatus, Captured, Captured, &'static str, bool) {
    #[cfg(unix)]
    if crate::pty::stdout_is_terminal() {
        if let Some(result) = run_child_on_pty(program, args, depth, redactor, opts) {
            return result;
        }
        // A pseudoterminal could not be allocated. Falling back to pipes is the
        // right move — a recorded run beats no run — and the recorded shape
        // says `pipe`, which is then true.
    }
    let mut command = Command::new(program);
    command
        .args(args)
        .env(DEPTH_VAR, (depth + 1).to_string())
        .env(ACTIVE_VAR, active_with(&opts.program))
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(
                std::io::stderr(),
                "thruline: {}: {e}",
                program.to_string_lossy()
            );
            return (
                ChildStatus::Exited(126),
                empty_capture(),
                empty_capture(),
                crate::pty::PIPE,
                false,
            );
        }
    };
    let pid = child.id();

    let store = opts.output_store.clone();
    let out_handle = child.stdout.take().map(|pipe| {
        let sink = CaptureSink::new(redactor.clone(), store.as_deref(), "out");
        std::thread::spawn(move || pump(pipe, sink, false))
    });
    let err_handle = child.stderr.take().map(|pipe| {
        let sink = CaptureSink::new(redactor.clone(), store.as_deref(), "err");
        std::thread::spawn(move || pump(pipe, sink, true))
    });

    let status = child.wait();
    let stdout = out_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_else(empty_capture);
    let stderr = err_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_else(empty_capture);

    let status = match status {
        Ok(s) => child_status(&s, pid),
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "thruline: wait failed: {e}");
            ChildStatus::Exited(1)
        }
    };
    (status, stdout, stderr, crate::pty::PIPE, false)
}

/// The same, with the child on a pseudoterminal.
///
/// `None` when one could not be allocated, so the caller falls back to pipes
/// rather than failing the command the user asked for.
#[cfg(unix)]
fn run_child_on_pty(
    program: &Path,
    args: &[String],
    depth: u32,
    redactor: &Redactor,
    opts: &RecordOptions,
) -> Option<(ChildStatus, Captured, Captured, &'static str, bool)> {
    let mut command = Command::new(program);
    command
        .args(args)
        .env(DEPTH_VAR, (depth + 1).to_string())
        .env(ACTIVE_VAR, active_with(&opts.program));
    let mut pty = crate::pty::spawn_on_pty(command).ok()?;
    let pid = pty.child.id();
    let sink = CaptureSink::new(redactor.clone(), opts.output_store.as_deref(), "out");
    // Read to EOF FIRST, then reap. A child that fills the pty buffer while
    // nothing drains it deadlocks, and the deadlock looks like a hung test.
    let controller = std::mem::replace(&mut pty.controller, {
        // Placeholder so `pty` can be moved from; never read.
        std::fs::File::open("/dev/null").ok()?
    });
    let stdout = pump(controller, sink, false);
    let status = pty.child.wait();
    let status = match status {
        Ok(s) => child_status(&s, pid),
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "thruline: wait failed: {e}");
            ChildStatus::Exited(1)
        }
    };
    // `stderr` is an ABSENCE here, not a measurement: one device carried both.
    // `exec_extra` stamps `streams: merged` beside it so nothing reads the
    // empty digest as "the command wrote nothing to stderr".
    Some((status, stdout, empty_capture(), crate::pty::TTY, true))
}

#[cfg(unix)]
fn child_status(status: &std::process::ExitStatus, pid: u32) -> ChildStatus {
    use std::os::unix::process::ExitStatusExt;
    match (status.code(), status.signal()) {
        (Some(code), _) => ChildStatus::ExitedChild { code, pid },
        (None, Some(signal)) => ChildStatus::Signalled { signal, pid },
        // Neither: nothing observable to report, and inventing 0 would report a
        // success that was never seen.
        (None, None) => ChildStatus::ExitedChild { code: 1, pid },
    }
}

#[cfg(not(unix))]
fn child_status(status: &std::process::ExitStatus, pid: u32) -> ChildStatus {
    ChildStatus::ExitedChild {
        code: status.code().unwrap_or(1),
        pid,
    }
}

/// Read one pipe to exhaustion: raw bytes straight through to our own
/// descriptor, the same bytes into the sink.
fn pump<R: Read>(mut pipe: R, mut sink: CaptureSink, is_stderr: bool) -> Captured {
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match pipe.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let chunk = &buf[..n];
                // Raw and unmodified: this is the user's own terminal.
                if is_stderr {
                    let mut out = std::io::stderr().lock();
                    let _ = out.write_all(chunk);
                    let _ = out.flush();
                } else {
                    let mut out = std::io::stdout().lock();
                    let _ = out.write_all(chunk);
                    let _ = out.flush();
                }
                sink.push(chunk);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    sink.finish()
}

/// The active-program set this process should pass to its child.
///
/// Idempotent: a program already in the set is not added twice, so a long chain
/// cannot grow the variable without bound.
pub fn active_with(program: &str) -> String {
    let current = std::env::var(ACTIVE_VAR).unwrap_or_default();
    let already = current
        .split(':')
        .any(|entry| !entry.is_empty() && entry == program);
    if already {
        return current;
    }
    if current.is_empty() {
        program.to_string()
    } else {
        format!("{current}:{program}")
    }
}

/// Run a program with no recording at all (past the depth cap).
///
/// Carries the guard variables forward. Without them this was the *inside* of
/// the hang: past the cap the recorder ran the resolved program, and in a
/// third-party-shim cycle the resolved program is the shim itself, which
/// re-dispatched back into the wrapper at an unchanged depth — an infinite loop
/// whose only difference from the recorded one was that it recorded nothing.
fn passthrough(program: &Path, args: &[String], depth: u32, active: &str) -> i32 {
    match Command::new(program)
        .args(args)
        .env(DEPTH_VAR, (depth + 1).to_string())
        .env(ACTIVE_VAR, active)
        .status()
    {
        Ok(s) => s.code().unwrap_or(1),
        Err(_) => 126,
    }
}

/// Find the real program on `PATH`, refusing any candidate that is one of our
/// own wrappers.
///
/// The shim directory is skipped by path *and* every candidate is checked for
/// [`WRAPPER_MARKER`]. The second check is the one that matters: a path
/// comparison is defeated by a symlink, a copy of the shim directory, or a
/// second install, and the failure it prevents is an unbounded fork bomb.
pub fn resolve_program(program: &str, shim_dir: Option<&Path>) -> Option<PathBuf> {
    resolve_program_on(program, shim_dir, None)
}

/// [`resolve_program`] against a named `PATH` instead of the process's.
///
/// **This is the same walk, not a second one.** `resolve_program` delegates
/// here with `None`, so there is exactly one implementation of "which program
/// does this name reach" in the codebase and it is the one that actually runs
/// the child. That matters because [`crate::shim::install`] now asks the same
/// question to decide whether to write a wrapper at all: a second, agreeing-
/// looking PATH walk would be a second opinion, and the install-time answer
/// and the run-time answer are only guaranteed to match while they share a
/// body.
///
/// The explicit `path_var` exists so the predicate can be *tested* without
/// mutating the process environment — `std::env::set_var` is global to a test
/// binary whose other tests spawn `sh` and `git` by name.
pub fn resolve_program_on(
    program: &str,
    shim_dir: Option<&Path>,
    path_var: Option<&OsStr>,
) -> Option<PathBuf> {
    if program.contains('/') {
        let path = PathBuf::from(program);
        return if is_our_wrapper(&path) {
            None
        } else {
            Some(path)
        };
    }
    let from_env;
    let path_var = match path_var {
        Some(p) => p,
        None => {
            from_env = std::env::var_os("PATH")?;
            from_env.as_os_str()
        }
    };
    for dir in std::env::split_paths(path_var) {
        if shim_dir.is_some_and(|s| same_dir(s, &dir)) {
            continue;
        }
        let candidate = dir.join(program);
        if !is_executable(&candidate) {
            continue;
        }
        if is_our_wrapper(&candidate) {
            continue;
        }
        return Some(candidate);
    }
    None
}

fn same_dir(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Does this file carry the wrapper marker in its first few hundred bytes?
///
/// Public because `shim::verify` asks it of files it did **not** write: a
/// wrapper-shaped file in the shim directory that is absent from the manifest
/// is a forgery, and recognising it must use the same test the resolver uses
/// to refuse a candidate.
pub fn is_our_wrapper(path: &Path) -> bool {
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 512];
    let Ok(n) = file.read(&mut head) else {
        return false;
    };
    String::from_utf8_lossy(&head[..n]).contains(WRAPPER_MARKER)
}

/// Append one spool line. A spool that cannot be written is recorded on the
/// report and the command still runs.
fn spool(
    report: &mut ExecReport,
    opts: &RecordOptions,
    session: &str,
    source: &str,
    at: &str,
    payload: Payload,
) {
    let Some(path) = opts.spool.clone() else {
        return;
    };
    let tagged = match serde_json::to_value(&payload) {
        Ok(v) => v,
        Err(e) => {
            report.spool_error = Some(format!("payload is not serialisable: {e}"));
            return;
        }
    };
    let material = format!("{session}|{at}|{tagged}");
    let line = serde_json::json!({
        "received_at": at,
        "source": source,
        "session_id": session,
        "session_binding": opts.session_binding.as_str(),
        "spec_payload": tagged,
        "raw_sha256": thruline_core::chain::sha256_hex(material.as_bytes()),
    });
    let rendered = match serde_json::to_string(&line) {
        Ok(mut s) => {
            s.push('\n');
            s
        }
        Err(e) => {
            report.spool_error = Some(format!("spool line is not serialisable: {e}"));
            return;
        }
    };
    match crate::claude_code::append_line(&path, &rendered) {
        Ok(()) => report.lines_spooled += 1,
        Err(e) => report.spool_error = Some(format!("{}: {e}", path.display())),
    }
}

/// A session id for an exec nothing bound to a harness session.
///
/// Per day and per repository-less machine, so unattributed execs collect
/// somewhere findable rather than being discarded (Rule 12) and rather than
/// being merged into a real session's chain, where they would be evidence
/// attributed to a session that never ran them.
fn unbound_session(now: &str) -> String {
    let day = now.split('T').next().unwrap_or("unknown");
    format!("exec-unbound-{day}")
}

/// The pid of whatever invoked this shim.
///
/// This, with the child's pid, is what reconstructs a process tree: a nested
/// runner's `parent` is the outer runner's `pid`, so `make` → `pytest` is
/// visible without any tracing facility.
#[cfg(unix)]
fn parent_pid() -> u32 {
    std::os::unix::process::parent_id()
}

/// Windows is tier 2 (§2, build must pass; capture may be reduced). `0` is
/// "not observed" rather than a fabricated pid.
#[cfg(not(unix))]
fn parent_pid() -> u32 {
    0
}

fn empty_capture() -> Captured {
    Captured {
        hash: format!("sha256:{}", thruline_core::chain::sha256_hex(b"")),
        excerpt: String::new(),
        bytes_raw: 0,
        bytes_redacted: 0,
        redactions_applied: 0,
        redaction_families: Vec::new(),
        stored_path: None,
        store_error: None,
        invalid_utf8: false,
        block_cap_hit: false,
    }
}

/// `.thruline/redact.yml` from the working directory, falling back to built-in
/// rules — the same choice, for the same reason, as the hook path.
fn repo_redactor() -> Redactor {
    std::env::current_dir()
        .ok()
        .and_then(|root| Redactor::from_repo(&root).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
