//! A207 (MANDATE-v4 §13) — `thruline check` and `thruline evidence`.
//!
//! `check` is the one door through which ThruLine ever sees a command run:
//! the user runs their own command *through* it (`thruline check -- cargo
//! test --workspace`). ThruLine never runs anything on its own — this verb
//! first fetches dependencies for supported Cargo checks in an isolated, offline
//! environment that runs no project code. If fetching succeeds, it executes
//! the argv the user typed after `--`, passes stdout,
//! stderr, and the exit code through untouched, and independently records
//! what happened: pre/post workspace identity, stream digests, the real
//! exit status, and the EVB_V1 binding with its COMMAND_STARTED /
//! COMMAND_FINISHED / EVIDENCE_BOUND observations.
//!
//! ## Exit-code semantics (documented contract)
//!
//! | situation                                        | exit code |
//! |--------------------------------------------------|-----------|
//! | the child exited with code N                     | N (unchanged, always) |
//! | the child died to signal S                       | 128 + S (shell convention; the signal is recorded) |
//! | schema upgrade pending (pre-spawn)               | 3 — the child never ran, nothing was paid (A193) |
//! | any other wrapper failure BEFORE the spawn       | 125 — the child never ran, nothing is recorded |
//! | usage error (clap, pre-spawn)                    | 2 |
//! | wrapper trouble AFTER the spawn                  | the child's code; the defect goes to stderr and, where it precedes the store, into the binding's hash-covered `wrapper_defects` |
//!
//! A wrapper failure never fabricates a child exit code: 125 is only ever
//! "the child did not run", and a post-spawn defect never displaces the
//! child's real status. The wrapper's own chatter goes to stderr after the
//! child has exited (`--quiet` suppresses it entirely, leaving both streams
//! byte-identical to the child's).
//!
//! `--shell` runs the command through `bash -o pipefail -c`, and pipefail
//! cannot be disabled: pipeline exit status must not be lost through shell
//! wrappers (§13/§32). Direct mode spawns the argv with no shell at all.
//!
//! `evidence` is the read path: for each binding it re-observes the
//! workspace under the binding's own frozen scope and reports the §9 state
//! verbatim — with detections appended to the observation chain, and both
//! moments (bound-at, evaluated-at) always shown.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use thruline_core::evidence_binding::{
    self as evb, establishes_all_tests, ArtifactDigest, EvidenceBinding, EvidenceState, ExecMode,
    Freshness, IdentityEdge, StreamCapture, WsidScopeDecl,
};
use thruline_core::ledger::Ledger;
use thruline_core::workspace_identity::{self as wsid, Observation, Scope};

/// Pre-spawn wrapper failure: the child never ran. Distinct from every
/// child code by contract, not by arithmetic — the stderr line and the
/// absence of any recorded execution are the distinguishers.
const WRAPPER_FAILURE: u8 = 125;

pub(crate) struct CheckArgs {
    pub shell: bool,
    pub quiet: bool,
    pub artifacts: Vec<PathBuf>,
    /// T-3-2 (`B6.3`): this run examined the application listening on this
    /// port. The port owner, its executable and the database it has open are
    /// read with `lsof` for THIS PORT ONLY, after the command has finished.
    /// Nothing scans, and no port the caller did not name is ever read.
    pub app_port: Option<u16>,
    /// Config and test files whose bytes are part of what was examined
    /// (`B6.2`). Digested together; the names are recorded beside the digest.
    pub config_files: Vec<String>,
    /// T-3-2b: this run was made for this Change. The capture is recorded
    /// under a session of its own and confirmed onto that Change, which is the
    /// only way a recorded execution can be read under one. `None` records the
    /// execution exactly as before and attaches it to nothing.
    pub for_change: Option<String>,
    /// Who is running it. Only read when `for_change` is given.
    pub actor: Option<String>,
    /// T-3-6-F3 (`B16.10`) — what class of evidence this run is, DECLARED by
    /// the person running it. `None` records no class at all, and every
    /// surface says the record does not say rather than calling it ordinary.
    pub evidence_class: Option<thruline_core::run_record::EvidenceClass>,
    pub command: Vec<String>,
}

/// Who a check run is attributed to when the person did not say.
///
/// Attaching a capture to a Change is a person's act and the store refuses it
/// without a name, so this verb must have one. The account this command is
/// running as is the closest true answer — this is a terminal, and the person
/// who typed the command is the person logged in — and when even that is not
/// set the sentence says "you" rather than inventing an identity.
fn invoking_user(given: Option<&str>) -> String {
    if let Some(actor) = given.map(str::trim).filter(|a| !a.is_empty()) {
        return actor.to_string();
    }
    match std::env::var("USER") {
        Ok(user) if !user.trim().is_empty() => user,
        _ => "you".to_string(),
    }
}

/// One stream's tee outcome: what the digest established, plus a
/// passthrough defect where writing through failed (the capture itself can
/// still be complete — we keep reading and hashing).
struct TeeOutcome {
    capture: StreamCapture,
    passthrough_defect: Option<String>,
    /// T-3-6-F1 (`Q1`) — what this stream said it ran, counted as it went
    /// past. The bytes are not kept anywhere (the binding holds a digest and
    /// a length), so this is the only moment the count can be read; it is
    /// read from the stream's own `test result:` line and never from the exit
    /// code. A stream that carried no such line yields a tally of no suites,
    /// which is stored as nothing at all.
    tally: thruline_core::test_tally::TestTally,
    /// T-3-6-F3 (`Q2`) — the failing assertion's own words, read from the same
    /// bytes at the same moment and for the same reason: the record keeps a
    /// digest of this stream, not the stream, so this is the only moment the
    /// words can be read. `None` is "the stream carried no failure block",
    /// which is never rendered as "it passed".
    failure_line: Option<String>,
}

fn tee(mut src: impl Read, mut dst: impl Write) -> TeeOutcome {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let mut total: u64 = 0;
    let mut buf = vec![0u8; 64 * 1024];
    let mut writing = true;
    let mut passthrough_defect = None;
    let mut scanner = thruline_core::test_tally::Scanner::new();
    // C30 R-RUN: pytest's own summary line, read from the same bytes at the
    // same moment as libtest's (the bytes are not kept afterwards).
    let mut pytest = PytestScanner::default();
    let mut failures = thruline_core::run_record::FailureScanner::new();
    loop {
        match src.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                hasher.update(&buf[..n]);
                scanner.feed(&buf[..n]);
                pytest.feed(&buf[..n]);
                failures.feed(&buf[..n]);
                total += n as u64;
                if writing {
                    if let Err(e) = dst.write_all(&buf[..n]).and_then(|()| dst.flush()) {
                        writing = false;
                        passthrough_defect = Some(format!(
                            "passthrough interrupted after {total} bytes ({e}) — the child's \
                             stream was still read and digested in full"
                        ));
                    }
                }
            }
            Err(e) => {
                return TeeOutcome {
                    capture: StreamCapture::Truncated {
                        reason: format!("read failed after {total} bytes: {e}"),
                    },
                    passthrough_defect,
                    // A truncated capture counts nothing. The record does not
                    // hold what the command said, so it cannot hold how many
                    // tests it ran either.
                    tally: thruline_core::test_tally::TestTally::default(),
                    // A truncated capture holds no failure line either: the
                    // record does not hold what the command said.
                    failure_line: None,
                };
            }
        }
    }
    let sha256 = {
        let digest = hasher.finalize();
        let mut s = String::with_capacity(64);
        for b in digest {
            use std::fmt::Write as _;
            let _ = write!(s, "{b:02x}");
        }
        s
    };
    TeeOutcome {
        capture: StreamCapture::Captured {
            sha256,
            bytes: total,
        },
        passthrough_defect,
        // A stream that carried neither form yields no suites, which is
        // stored as nothing at all — never as a count of zero.
        tally: scanner.finish().plus(pytest.finish()),
        failure_line: failures.finish(),
    }
}

/* ── C30 R-RUN: pytest's own count, beside libtest's ────────────────────── */

/// pytest's session summary, read out of a byte stream as it goes past —
/// the pytest counterpart of [`thruline_core::test_tally::Scanner`], with the
/// same bound on how much of one line it will hold.
///
/// pytest prints its summary once, as the last line of a session, so the
/// count is the **last** summary line the stream carried (as HQ's reading of
/// a recorded pytest run takes it, `thruline_core::hq::tally`). A summary
/// printed earlier — a test that runs pytest itself and shows that output when
/// it fails — is not this run's count, and is not added to it.
#[derive(Debug, Default)]
struct PytestScanner {
    line: Vec<u8>,
    overlong: bool,
    last: Option<thruline_core::test_tally::TestTally>,
}

/// The longest line the pytest scanner assembles; a summary line is far
/// shorter, even with every colour escape `--color=yes` adds.
const PYTEST_MAX_LINE: usize = 4096;

impl PytestScanner {
    fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            if b == b'\n' {
                self.finish_line();
                continue;
            }
            if self.overlong {
                continue;
            }
            if self.line.len() >= PYTEST_MAX_LINE {
                self.line.clear();
                self.overlong = true;
                continue;
            }
            self.line.push(b);
        }
    }

    fn finish_line(&mut self) {
        if !self.overlong {
            if let Ok(text) = std::str::from_utf8(&self.line) {
                if let Some(tally) = parse_pytest_summary(text) {
                    self.last = Some(tally);
                }
            }
        }
        self.line.clear();
        self.overlong = false;
    }

    /// The last summary the stream carried, as one suite; or a tally of no
    /// suites when it carried none.
    fn finish(mut self) -> thruline_core::test_tally::TestTally {
        self.finish_line();
        self.last.unwrap_or_default()
    }
}

/// One pytest session summary line, or nothing.
///
/// The shapes, from pytest's own `terminal.py` (`summary_stats`):
///
/// ```text
/// ========================= 1 failed, 4 passed, 1 skipped in 0.30s ==========================
/// 5 passed in 0.12s                                    (-q: no rule)
/// ============================ no tests ran in 0.00s =============================
/// = 1 failed, 3 passed, 1 skipped, 1 xfailed, 1 xpassed, 1 warning, 1 error in 0.01s =
/// ===================== 1 passed, 3 subtests passed in 65.21s (0:01:05) ===============
/// ```
///
/// Every part must be `N <outcome>` with an outcome pytest names, and the
/// line must end in pytest's duration; anything else is not a summary and
/// yields nothing, never a zero. What lands where:
///
/// * `passed` → passed.
/// * `failed`, and `error`/`errors` → failed. An error is a test that did not
///   pass (its setup or teardown broke, or its module could not be collected),
///   and pytest exits non-zero for it, as HQ's reading of a recorded run
///   already treats it.
/// * `skipped` → ignored, the libtest word for a test declared not to run.
/// * `deselected`, `xfailed`, `xpassed`, `warning(s)`, `rerun` and pytest's
///   `subtests …` counts are recognised and counted nowhere: none of them is a
///   test that passed or a test that failed by pytest's own exit status (a
///   strict xpass is already printed as `failed`).
/// * `no tests ran` → a summary of zero passed and zero failed, which is what
///   pytest said; a run whose output names no summary (`-qq`) has no count.
pub(crate) fn parse_pytest_summary(line: &str) -> Option<thruline_core::test_tally::TestTally> {
    let plain = printable(line);
    let text = plain.trim();
    // `write_sep("=", …)` puts a rule on both sides; `-q` writes the line bare.
    let body = if text.starts_with('=') {
        let inner = text.trim_start_matches('=');
        let both = inner.trim_end_matches('=');
        if both.len() == inner.len() {
            return None;
        }
        both.strip_prefix(' ')?.strip_suffix(' ')?
    } else if text.ends_with('=') {
        return None;
    } else {
        text
    };
    let (counts, duration) = body.rsplit_once(" in ")?;
    if !is_pytest_duration(duration) {
        return None;
    }
    let mut tally = thruline_core::test_tally::TestTally {
        suites: 1,
        ..Default::default()
    };
    if counts == "no tests ran" {
        return Some(tally);
    }
    let mut seen: Vec<&str> = Vec::new();
    for part in counts.split(", ") {
        let (n, outcome) = part.split_once(' ')?;
        if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) || seen.contains(&outcome) {
            return None;
        }
        seen.push(outcome);
        let n: u64 = n.parse().ok()?;
        match outcome {
            "passed" => tally.passed = n,
            "failed" | "error" | "errors" => tally.failed = tally.failed.saturating_add(n),
            "skipped" => tally.ignored = n,
            "deselected" | "xfailed" | "xpassed" | "warning" | "warnings" | "rerun"
            | "subtests passed" | "subtests failed" | "subtests skipped" => {}
            _ => return None,
        }
    }
    Some(tally)
}

/// pytest's session duration: `0.12s`; past a minute `65.21s (0:01:05)`, past
/// a day `90061.50s (1 day, 1:01:01)` (`format_session_duration`); and
/// `0.12 seconds`, the wording before pytest 6.
fn is_pytest_duration(duration: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let seconds = |s: &str| match s.split_once('.') {
        Some((whole, fraction)) => digits(whole) && digits(fraction),
        None => digits(s),
    };
    if let Some(n) = duration.strip_suffix(" seconds") {
        return seconds(n);
    }
    let Some((n, rest)) = duration.split_once('s') else {
        return false;
    };
    if !seconds(n) {
        return false;
    }
    if rest.is_empty() {
        return true;
    }
    let Some(clock) = rest.strip_prefix(" (").and_then(|r| r.strip_suffix(')')) else {
        return false;
    };
    let clock = match clock.split_once(", ") {
        Some((days, clock)) => match days.split_once(' ') {
            Some((n, "day" | "days")) if digits(n) => clock,
            _ => return false,
        },
        None => clock,
    };
    let parts: Vec<&str> = clock.split(':').collect();
    parts.len() == 3 && parts.iter().all(|p| digits(p))
}

/// A line as a terminal would leave it: decoded, with a closing `\r`
/// dropped, only what follows the last `\r` kept (a line redrawn in place
/// shows its last drawing), colour and other escape sequences removed, and
/// every other control character but a tab removed.
fn printable(raw: &str) -> String {
    let line = raw.strip_suffix('\r').unwrap_or(raw);
    let line = line.rsplit('\r').next().unwrap_or(line);
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.next() {
                // CSI: parameters, then one final byte in `@`..=`~`.
                Some('[') => {
                    for n in chars.by_ref() {
                        if ('@'..='~').contains(&n) {
                            break;
                        }
                    }
                }
                // OSC: up to BEL, or ESC `\`.
                Some(']') => {
                    while let Some(n) = chars.next() {
                        if n == '\u{7}' {
                            break;
                        }
                        if n == '\u{1b}' {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                // Any other escape is two characters.
                _ => {}
            }
            continue;
        }
        if c == '\t' || !c.is_control() {
            out.push(c);
        }
    }
    out
}

/* ── C30 R-RUN: what a check started from the run sheet has printed so far ── */

/// The most lines of a running check's output the run sheet is served.
pub(crate) const LIVE_LINES: usize = 100;
/// The most bytes those lines may hold together.
pub(crate) const LIVE_BYTES: usize = 16 * 1024;
/// The longest one line is kept; the rest of a longer line is cut off.
pub(crate) const LIVE_LINE_BYTES: usize = 1024;
/// What the tee keeps of one line before the sheet masks it: more than the sheet
/// serves of a line ([`LIVE_LINE_BYTES`]), so a secret that starts near the start of a
/// line is whole when it is masked, and the part a cut could split is never served.
pub(crate) const LIVE_TAIL_LINE_BYTES: usize = LIVE_BYTES / 2;

/// The newest part of what one run has printed so far, held in memory for
/// the run sheet while the run goes (C30 R-RUN).
///
/// **It is not the record.** The record of a run is what it has always been —
/// a digest and a length per stream, the count, the failure line — and this
/// changes none of it: the wrapper hashes and counts exactly the bytes it did
/// before, and this is where a detached run's passthrough goes instead of
/// nowhere. It is bounded ([`LIVE_LINES`], [`LIVE_BYTES`],
/// [`LIVE_LINE_BYTES`]), the newest kept, and it ends with the run: the run
/// route drops it when the run's job ends, and nothing writes it anywhere.
#[derive(Debug, Default)]
pub(crate) struct LiveOutput {
    tail: std::sync::Mutex<LiveTail>,
}

#[derive(Debug, Default)]
struct LiveTail {
    /// Whole lines, oldest first.
    lines: std::collections::VecDeque<String>,
    /// The bytes `lines` holds together.
    bytes: usize,
    /// Each stream's line still being printed (stdout, stderr), and whether
    /// it has grown past what one line may keep.
    open: [(Vec<u8>, bool); 2],
    /// Something printed is not in `lines`: an older line dropped, or a line
    /// cut short.
    cut: bool,
}

/// What the run sheet is served of a [`LiveOutput`]: the newest lines, oldest
/// first, and whether anything printed is not among them as printed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LiveLines {
    pub lines: Vec<String>,
    pub cut: bool,
}

/// Which stream a write came from: stdout's lines and stderr's lines are
/// assembled apart, so one stream's half-printed line never takes the other's.
const STDOUT: usize = 0;
const STDERR: usize = 1;

impl LiveOutput {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Test seam: the next bytes the check's stdout printed.
    #[cfg(test)]
    pub(crate) fn feed_for_test(&self, bytes: &[u8]) {
        self.take(STDOUT, bytes);
    }

    /// The next bytes one stream printed.
    fn take(&self, stream: usize, bytes: &[u8]) {
        let mut tail = self
            .tail
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for &b in bytes {
            if b == b'\n' {
                let (line, overlong) = std::mem::take(&mut tail.open[stream]);
                let (text, cut) = kept_line(&line, overlong);
                tail.cut |= cut;
                tail.bytes += text.len();
                tail.lines.push_back(text);
                tail.trim();
                continue;
            }
            // A few bytes past the bound, so the cut can fall on a whole
            // character; everything after that is dropped as it arrives.
            let (line, overlong) = &mut tail.open[stream];
            if line.len() < LIVE_TAIL_LINE_BYTES + 4 {
                line.push(b);
            } else {
                *overlong = true;
            }
        }
    }

    /// The newest finished lines, oldest first. A line still being printed is not
    /// served until it ends: it may end in half a token, which a masker cannot
    /// recognise.
    pub(crate) fn read(&self) -> LiveLines {
        let tail = self
            .tail
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut lines: std::collections::VecDeque<String> = tail.lines.clone();
        let mut bytes = tail.bytes;
        let mut cut = tail.cut;
        while lines.len() > LIVE_LINES || bytes > LIVE_BYTES {
            match lines.pop_front() {
                Some(dropped) => {
                    bytes -= dropped.len();
                    cut = true;
                }
                None => break,
            }
        }
        LiveLines {
            lines: lines.into(),
            cut,
        }
    }
}

impl LiveTail {
    /// Drop the oldest lines until the tail is within both bounds.
    fn trim(&mut self) {
        while self.lines.len() > LIVE_LINES || self.bytes > LIVE_BYTES {
            match self.lines.pop_front() {
                Some(dropped) => {
                    self.bytes -= dropped.len();
                    self.cut = true;
                }
                None => break,
            }
        }
    }
}

/// One line as the tail keeps it — [`printable`], and no longer than
/// [`LIVE_TAIL_LINE_BYTES`], cut at a character — and whether it was cut.
///
/// This cut is not what a person sees: the sheet masks the kept text and only then
/// cuts each line to [`LIVE_LINE_BYTES`] (`run_route.rs` `output_json`), so the text
/// around this cut is never served.
fn kept_line(raw: &[u8], overlong: bool) -> (String, bool) {
    let mut text = printable(&String::from_utf8_lossy(raw));
    let mut cut = overlong;
    if text.len() > LIVE_TAIL_LINE_BYTES {
        let mut at = LIVE_TAIL_LINE_BYTES;
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        text.truncate(at);
        cut = true;
    }
    (text, cut)
}

/// Where the tee passes one stream through when the run is detached and a
/// [`LiveOutput`] is listening.
struct LiveWriter {
    live: std::sync::Arc<LiveOutput>,
    stream: usize,
}

impl Write for LiveWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.live.take(self.stream, buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn refuse(reason: &str) -> std::process::ExitCode {
    eprintln!(
        "thruline check: {reason}\n\
         the command did NOT run — wrapper failure before the spawn (exit {WRAPPER_FAILURE}); \
         nothing was recorded"
    );
    std::process::ExitCode::from(WRAPPER_FAILURE)
}

/* ── P6-5: the run route's door into this same wrapper ──────────────────── */

/// P6-5 (design record §3.7) — one check HQ may start from the screen, and the
/// **only** argv it runs.
///
/// The run route resolves a check id from the Change's accepted check profile
/// to one of these rows, server-side. The browser sends the id and nothing
/// else: no command, no argument, no path, no environment variable. A profile
/// id with no row here cannot be started from the screen at all — the sheet
/// says so, and the terminal door (`thruline check -- …`) is untouched.
///
/// Nothing in this table installs, elevates, or runs a script out of the
/// repository: every row is a test runner the profile table already names
/// (`change_propose::PROFILE_TABLE`), invoked with fixed arguments, and a test
/// fences the program names against installers and shells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RunnableCheck {
    /// The accepted check profile's id for this check.
    pub id: &'static str,
    /// The program, looked up on the chosen environment's search path.
    pub program: &'static str,
    /// Fixed arguments. Never extended by a request.
    pub args: &'static [&'static str],
}

impl RunnableCheck {
    /// The argv exactly as the wrapper will run it.
    pub(crate) fn argv(&self) -> Vec<String> {
        std::iter::once(self.program)
            .chain(self.args.iter().copied())
            .map(str::to_string)
            .collect()
    }
}

// Before any runner other than cargo-test joins this list, docs/loop/tasks/POSTLAUNCH-PROJECT-LOCAL-TOOLS.md must land (founder ruling D-049).
pub(crate) const RUNNABLE_CHECKS: &[RunnableCheck] = &[RunnableCheck {
    id: "cargo-test",
    program: "cargo",
    args: &["test"],
}];

/// ADV-run-ledger-3 — the switch that keeps each allow-listed program off the
/// network: `(program, variable, value)`.
///
/// The run sheet promises "It does not install anything and does not touch
/// the network", and seals that sentence into the permission file. Plain
/// `cargo test` fetches any dependency missing from the local cache, so every
/// run the sheet starts is given every switch here ([`Launch::offline`]); a
/// dependency that is not already on this machine then fails the build
/// instead of being downloaded. A test fences [`RUNNABLE_CHECKS`] against
/// this table, so no program can be made startable from the sheet without a
/// switch. The switch governs the program's own fetching; code in the project
/// that opens a connection itself is the project's, and no variable stops it.
///
/// Supported Cargo commands from the terminal also receive these switches
/// after the product-owned offline fetch. Other terminal commands retain
/// their inherited environment.
pub(crate) const OFFLINE_SWITCHES: &[(&str, &str, &str)] =
    &[("cargo", "CARGO_NET_OFFLINE", "true")];

/// The allow-listed row for one profile id, or `None`.
pub(crate) fn runnable_check(id: &str) -> Option<&'static RunnableCheck> {
    RUNNABLE_CHECKS.iter().find(|c| c.id == id)
}

/// The name the execution record gives a run of `command` in `cwd` — the same
/// key [`run_check_at`] writes, computed by the same function, so the run
/// route can find the latest run of one check without a second naming rule.
pub(crate) fn run_name(command: &[String], cwd: &Path) -> String {
    evb::classify_command(&command.join(" "), &cwd.display().to_string()).key
}

/// Who a run is attributed to when the person did not say — the account HQ
/// is running as. The run route uses the same answer the terminal door does.
pub(crate) fn account_name() -> String {
    invoking_user(None)
}

/// How one run is launched. [`Launch::terminal`] is the `thruline check` verb
/// exactly as it always was; the run route builds the other shape.
///
/// The differences are the whole of what a screen needs and nothing more:
/// the directory is the project's recorded folder rather than the shell's,
/// the search path is the chosen environment's, the run key is minted by the
/// caller so a Cancel control can name the run before it finishes, and the
/// child's streams are digested and counted exactly as before but not passed
/// through to a terminal nobody is looking at.
pub(crate) struct Launch {
    /// `None` reads the current directory, after the ledger opened — the
    /// terminal door's order, kept so a pending upgrade still refuses first.
    pub cwd: Option<PathBuf>,
    /// The `PATH` the child sees. `None` inherits this process's.
    pub path_env: Option<std::ffi::OsString>,
    /// The toolchain rustup's proxy must start, as the run route's guard
    /// checked it (ADV-run-security-7): set as `RUSTUP_TOOLCHAIN` for the
    /// child, after everything else, so a toolchain file rewritten between the
    /// check and the start cannot hand the run to another. `None` sets nothing.
    pub rustup_toolchain: Option<std::ffi::OsString>,
    /// The run key (and session id) to record under. `None` mints the
    /// terminal door's fresh `cli:check:<time>:<ulid>`.
    pub run_key: Option<String>,
    /// Stdin closed, both streams digested and counted but not passed through.
    pub detached: bool,
    /// Where a pre-spawn refusal's reason is left for the caller to read.
    pub refusal: Option<std::sync::Arc<std::sync::Mutex<Option<String>>>>,
    /// Every [`OFFLINE_SWITCHES`] variable is set on run-sheet children.
    /// Supported Cargo checks also set them after their product-owned fetch.
    pub offline: bool,
    /// C30 R-RUN: where a detached run's two streams are passed through, so
    /// the run sheet can show what it has printed while it runs. `None`
    /// passes them nowhere, as before. Never read for a terminal run, whose
    /// streams go to the terminal.
    pub live: Option<std::sync::Arc<LiveOutput>>,
    /// Test-only: variables set (`Some`) or removed (`None`) for the child,
    /// over this process's own — so a test can give a run a rustup home of its
    /// own without touching this process's environment or the machine's.
    #[cfg(test)]
    pub env: Vec<(std::ffi::OsString, Option<std::ffi::OsString>)>,
}

impl Launch {
    /// The `thruline check` verb, unchanged.
    pub(crate) fn terminal() -> Self {
        Self {
            cwd: None,
            path_env: None,
            rustup_toolchain: None,
            run_key: None,
            detached: false,
            refusal: None,
            offline: false,
            live: None,
            #[cfg(test)]
            env: Vec::new(),
        }
    }

    fn refuse(&self, reason: &str) -> std::process::ExitCode {
        if let Some(sink) = &self.refusal {
            if let Ok(mut slot) = sink.lock() {
                *slot = Some(reason.to_string());
            }
        }
        refuse(reason)
    }
}

#[cfg(test)]
thread_local! {
    /// Test-only seam (T-4-4-ATTEMPT-BUDGET-R2): run once, on this thread, at
    /// the moment a check of a Change has been told it may start and before
    /// its process and its execution record exist — the window a second
    /// caller of the same Change would race in. A test drives the
    /// interleaving through it, so the race is reproduced by order, never by
    /// timing.
    pub(crate) static AFTER_ADMIT: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        std::cell::RefCell::new(None);
}

fn tee_to_sink_or_terminal(
    src: impl Read,
    detached: bool,
    stdout: bool,
    live: Option<std::sync::Arc<LiveOutput>>,
) -> TeeOutcome {
    match (detached, stdout, live) {
        // C30 R-RUN: a run the sheet started passes its streams to the sheet's
        // tail instead of nowhere. What is hashed and counted is unchanged.
        (true, _, Some(live)) => tee(
            src,
            LiveWriter {
                live,
                stream: if stdout { STDOUT } else { STDERR },
            },
        ),
        (true, _, None) => tee(src, std::io::sink()),
        (false, true, _) => tee(src, std::io::stdout().lock()),
        (false, false, _) => tee(src, std::io::stderr().lock()),
    }
}

/// OFFLINE-KILL-GUARD-R4 (P-2026-09-28-4): the receipt field on a check's
/// COMMAND_STARTED and COMMAND_FINISHED observations that says, in words, that
/// ThruLine's own dependency-fetch step did not run and why. The run sheet
/// serves it as `last.fetch_skipped`.
pub(crate) const FETCH_SKIPPED_FIELD: &str = "dependency_fetch_skipped";

pub(crate) fn run_check(db: &Path, args: CheckArgs) -> Result<std::process::ExitCode> {
    run_check_at(db, args, Launch::terminal())
}

pub(crate) fn run_check_at(
    db: &Path,
    args: CheckArgs,
    launch: Launch,
) -> Result<std::process::ExitCode> {
    // Open the ledger BEFORE the spawn: a pending schema migration must
    // refuse with exit 3 having paid nothing and run nothing (A193) — the
    // error is propagated so main's handler prints the one-line refusal.
    let ledger = match Ledger::open(db) {
        Ok(l) => l,
        Err(e) => {
            if matches!(e, thruline_core::error::CoreError::UpgradePending { .. }) {
                return Err(e.into());
            }
            return Ok(launch.refuse(&format!("the ledger at {} did not open: {e}", db.display())));
        }
    };

    let cwd = match &launch.cwd {
        Some(d) => d.clone(),
        None => match std::env::current_dir() {
            Ok(d) => d,
            Err(e) => {
                return Ok(launch.refuse(&format!("the current directory is unreadable: {e}")))
            }
        },
    };

    // Pre-command observation — the frozen expected side. An incomplete
    // observation is recorded as such, never upgraded; a workspace with no
    // repository identity cannot be recorded at all, so the wrapper refuses
    // rather than running unrecorded (§7: no placeholder identities).
    let pre_obs = match wsid::observe(&cwd, Scope::everything()) {
        Ok(o) => o,
        Err(e) => {
            return Ok(launch.refuse(&format!(
                "the workspace could not be observed ({e}) — run from inside a git \
                 worktree, or run the command without the wrapper"
            )))
        }
    };
    let (repo_id, root) = match &pre_obs {
        Observation::Complete(fp) => (fp.doc().repo_id.clone(), fp.doc().root.clone()),
        Observation::Incomplete { repo_id, root, .. } => (repo_id.clone(), root.clone()),
    };
    let Some(repository_identity) = repo_id else {
        return Ok(launch.refuse(
            "no stable repository identity could be derived for this workspace — \
             the execution could not be recorded against a real identity",
        ));
    };

    // The Change this run is for, resolved BEFORE the spawn (T-3-2b). A name
    // this store does not hold is a wrapper failure with nothing paid: running
    // the command and only then discovering the capture cannot be recorded
    // where the person asked for it is the worst of both.
    let for_change: Option<(String, String)> = match &args.for_change {
        None => None,
        Some(change_id) => match thruline_core::change::read_change(&ledger, change_id) {
            Ok(Some(view)) => Some((change_id.clone(), view.project_id)),
            _ => {
                return Ok(launch.refuse(&format!(
                    "no Change with id {change_id:?} is recorded in this store \u{2014} \
                         `thruline change new` opens one, and `thruline change results \
                         <id>` names the ones that exist"
                )))
            }
        },
    };

    let mut defects: Vec<String> = Vec::new();
    let cwd_text = cwd.display().to_string();
    let command_text = args.command.join(" ");
    let requested_scope = evb::classify_command(&command_text, &cwd_text);

    let (requested_argv, requested_mode) = if args.shell {
        (
            vec![
                "/bin/bash".to_string(),
                "-o".to_string(),
                "pipefail".to_string(),
                "-c".to_string(),
                command_text.clone(),
            ],
            ExecMode::Shell,
        )
    } else {
        (args.command.clone(), ExecMode::Direct)
    };

    // OFFLINE-KILL-GUARD-R4 (P-2026-09-28-4): with no allow-listed toolchain
    // the owned fetch is skipped, not the check. The check then runs as the
    // person's own command, a failure reads failed, and the record says why
    // ThruLine's own fetch step did not run.
    let (fetch, fetch_skipped) = match thruline_core::cargo_fetch::prepare_owned(
        &requested_argv,
        &cwd,
        Path::new(&root),
        launch.path_env.as_deref(),
        launch.rustup_toolchain.as_deref(),
    ) {
        Ok(thruline_core::cargo_fetch::Prepared::Plan(plan)) => (Some(plan), None),
        Ok(thruline_core::cargo_fetch::Prepared::Unsupported) => (None, None),
        Ok(thruline_core::cargo_fetch::Prepared::Skipped(why)) => (
            None,
            Some(format!("ThruLine's own fetch step did not run: {why}")),
        ),
        Err(error) => {
            return Ok(launch.refuse(&format!("dependency fetch could not be prepared: {error}")))
        }
    };
    if let Some(said) = &fetch_skipped {
        if !args.quiet && !launch.detached {
            eprintln!("[thruline check] {said}");
        }
    }
    let started_at = thruline_core::ingest::iso8601_utc_now();
    let source_session = launch
        .run_key
        .clone()
        .unwrap_or_else(|| format!("cli:check:{started_at}:{}", ulid::Ulid::generate()));

    // T-3-4 (`B6.8`): what the tree looked like at the spawn. The comparison
    // at completion is against THIS, not against whatever the tree became —
    // that is the whole of "do not attach a fresh pass to unexamined inputs".
    let inputs_at_spawn = thruline_core::examined_inputs::record_id(
        &thruline_core::examined_inputs::observe_source(Path::new(&root), None),
    );

    // T-4-4-ATTEMPT-BUDGET (R-2026-09-28-43): once every repair attempt a
    // person allowed this Change has been made, a run on a version no check of
    // it has examined is a further attempt nobody allowed, and it is refused
    // here, before anything spawns and before any row is written — for the
    // terminal door and the run route alike. The version is `inputs_at_spawn`,
    // which is content, not the snapshot the Change shows. A version already
    // examined runs as before; a Change with no budget is not limited. A budget
    // that cannot be read refuses too: starting a run the record might forbid
    // is the wrong way to fail.
    //
    // T-4-4-ATTEMPT-BUDGET-R2: the "yes" and the `running` row it allows are
    // one act. `admit` takes the store's write lock before it reads and the
    // lock is held (`admission`) until the first phase's row is written below,
    // so a second caller — another terminal, the run route, another process —
    // reads the record either before this caller asked or after its attempt is
    // there. A caller that cannot get the lock in time is refused, fail closed.
    let mut admission: Option<thruline_core::attempt_budget::Hold<'_>> = None;
    if let Some((change_id, _)) = &for_change {
        match thruline_core::attempt_budget::admit(&ledger, change_id, &inputs_at_spawn) {
            Ok(thruline_core::attempt_budget::Admission::Admitted(hold)) => admission = Some(hold),
            Ok(thruline_core::attempt_budget::Admission::Refused(stop)) => {
                return Ok(launch.refuse(&stop))
            }
            Err(e) => {
                return Ok(launch.refuse(&format!(
                    "the attempt budget of Change {change_id} could not be read ({e}), so \
                     nothing was started"
                )))
            }
        }
    }
    #[cfg(test)]
    if for_change.is_some() {
        if let Some(between) = AFTER_ADMIT.with(|slot| slot.borrow_mut().take()) {
            between();
        }
    }

    let mut fetching = fetch.is_some();
    let (
        argv,
        seed,
        out_tee,
        err_tee,
        exit_code,
        term_signal,
        final_code,
        dependencies_unavailable,
        phase_metadata,
    ) = loop {
        let argv = if fetching {
            fetch
                .as_ref()
                .context("fetch plan disappeared")?
                .argv
                .clone()
        } else {
            requested_argv.clone()
        };
        let mode = if fetching {
            ExecMode::Direct
        } else {
            requested_mode
        };
        let actual_cwd = if fetching {
            fetch.as_ref().context("fetch plan disappeared")?.cwd()
        } else {
            &cwd
        };
        let scope = if fetching {
            evb::classify_command(&argv.join(" "), &actual_cwd.display().to_string())
        } else {
            requested_scope.clone()
        };
        let metadata = if fetching {
            fetch
                .as_ref()
                .context("fetch plan disappeared")?
                .metadata(&requested_argv)
        } else {
            let mut metadata = serde_json::Map::new();
            if let Some(said) = &fetch_skipped {
                metadata.insert(
                    FETCH_SKIPPED_FIELD.to_string(),
                    serde_json::Value::String(said.clone()),
                );
            }
            metadata
        };
        let started_at = thruline_core::ingest::iso8601_utc_now();
        if !fetching
            && fetch.is_some()
            && thruline_core::run_execution::latest(&ledger, &source_session)?
                .is_some_and(|record| !record.state.is_open())
        {
            return Ok(launch
                .refuse("the dependency-fetch attempt was stopped before the check could start"));
        }
        // The spawn: directly, no shell interposition in direct mode. A spawn
        // failure is pre-spawn — the child never existed.
        //
        // T-3-4 (`B12.6`): the child gets a PROCESS GROUP OF ITS OWN. A check runs
        // a test runner which runs compilers; with one group per run, the whole
        // tree has one name, one signal reaches all of it, and "is anything left?"
        // is a question about the tree rather than about the leader.
        let mut spawn = if fetching {
            fetch.as_ref().context("fetch plan disappeared")?.command()
        } else {
            let mut command = Command::new(&argv[0]);
            command.args(&argv[1..]).current_dir(&cwd);
            command
        };
        spawn
            .stdin(if launch.detached {
                Stdio::null()
            } else {
                Stdio::inherit()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if !fetching {
            if let Some(path) = &launch.path_env {
                spawn.env("PATH", path);
            }
            if launch.offline || fetch.is_some() {
                for (_, variable, value) in OFFLINE_SWITCHES {
                    spawn.env(variable, value);
                }
            }
            #[cfg(test)]
            for (name, value) in &launch.env {
                match value {
                    Some(value) => spawn.env(name, value),
                    None => spawn.env_remove(name),
                };
            }
            // ADV-run-security-7 — last, so nothing above can take it back.
            if let Some(toolchain) = &launch.rustup_toolchain {
                spawn.env("RUSTUP_TOOLCHAIN", toolchain);
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            spawn.process_group(0);
        }
        let mut child = match spawn.spawn() {
            Ok(c) => c,
            Err(e) => return Ok(launch.refuse(&format!("{} did not spawn: {e}", argv[0]))),
        };
        let pid = child.id();

        // The supervisor row (`B7.1`): the record now says a run is going, with
        // what it is, which process group owns it and what it froze. If this
        // process never reaches the completion below — a crash, a kill, HQ
        // restarting under it — the row stays `running` and the reconciler turns
        // it into `interrupted`. That is the whole mechanism, and it is why the
        // row is written BEFORE anything can go wrong rather than after.
        let opened = thruline_core::run_execution::start(
            &ledger,
            &thruline_core::run_execution::Start {
                run_key: source_session.clone(),
                change_id: for_change.as_ref().map(|(id, _)| id.clone()),
                name: requested_scope.key.clone(),
                pid: Some(i64::from(pid)),
                process_group: Some(i64::from(pid)),
                started_at: started_at.clone(),
                examined_inputs_id: Some(inputs_at_spawn.clone()),
            },
        )
        // T-4-4-ATTEMPT-BUDGET-R2: the first phase's row is the attempt; it is
        // kept, and the admission lock released, only together.
        .and_then(|id| match admission.take() {
            Some(hold) => hold.commit().map(|()| id),
            None => Ok(id),
        });
        if let Err(e) = opened {
            // A cancellation may win between fetch completion and check spawn.
            // Never leave the newly spawned phase without a supervisor owner.
            let cleanup = crate::run_supervisor::stop_group(
                i64::from(pid),
                crate::run_supervisor::STOP_BOUND,
            );
            let _ = child.wait();
            return Ok(launch.refuse(&format!(
                "the execution record was not opened: {e}; cleanup: {cleanup:?}"
            )));
        }

        // A seed binding for the started observation: everything known now.
        let seed = EvidenceBinding {
            argv: argv.clone(),
            mode,
            cwd: actual_cwd.display().to_string(),
            root: root.clone(),
            repository_identity: repository_identity.clone(),
            scope: scope.clone(),
            wsid_scope: WsidScopeDecl::everything(),
            pre: IdentityEdge::from_observation(&pre_obs),
            post: IdentityEdge::incomplete("the command is still running".into()),
            exit_code: None,
            term_signal: None,
            stdout: StreamCapture::Truncated {
                reason: "the command is still running".into(),
            },
            stderr: StreamCapture::Truncated {
                reason: "the command is still running".into(),
            },
            artifacts: vec![],
            wrapper_defects: vec![],
            started_at: started_at.clone(),
            bound_at: started_at.clone(),
            source_adapter: "thruline".into(),
            source_session: source_session.clone(),
        };
        if let Err(e) = evb::record_command_started_with_metadata(&ledger, &seed, pid, &metadata) {
            let d = format!("COMMAND_STARTED was not recorded: {e}");
            eprintln!("thruline check: wrapper defect (the command keeps running): {d}");
            defects.push(d);
        }

        // Tee both streams on their own threads: no reordering within a
        // stream, no buffering deadlock, bytes passed through unmodified.
        let child_out = child.stdout.take();
        let child_err = child.stderr.take();
        let detached = launch.detached;
        let (live_out, live_err) = (launch.live.clone(), launch.live.clone());
        let (out_tee, err_tee, status) = std::thread::scope(|s| {
            let out_handle = s.spawn(move || match child_out {
                Some(src) => tee_to_sink_or_terminal(src, detached, true, live_out),
                None => TeeOutcome {
                    capture: StreamCapture::Truncated {
                        reason: "stdout pipe was not available".into(),
                    },
                    passthrough_defect: None,
                    tally: thruline_core::test_tally::TestTally::default(),
                    failure_line: None,
                },
            });
            let err_handle = s.spawn(move || match child_err {
                Some(src) => tee_to_sink_or_terminal(src, detached, false, live_err),
                None => TeeOutcome {
                    capture: StreamCapture::Truncated {
                        reason: "stderr pipe was not available".into(),
                    },
                    passthrough_defect: None,
                    tally: thruline_core::test_tally::TestTally::default(),
                    failure_line: None,
                },
            });
            // Keep observing cancellation during both phases, including the small
            // ownership handoff window. A terminal row always stops the new group.
            let status = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break Ok(status),
                    Err(error) => break Err(error),
                    Ok(None) => {}
                }
                if thruline_core::run_execution::latest(&ledger, &source_session)
                    .ok()
                    .flatten()
                    .is_some_and(|record| !record.state.is_open())
                {
                    crate::run_supervisor::stop_group(
                        i64::from(pid),
                        crate::run_supervisor::STOP_BOUND,
                    );
                    break child.wait();
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            };
            let out = out_handle.join().unwrap_or_else(|_| TeeOutcome {
                capture: StreamCapture::Truncated {
                    reason: "the stdout tee thread panicked".into(),
                },
                passthrough_defect: None,
                tally: thruline_core::test_tally::TestTally::default(),
                failure_line: None,
            });
            let err = err_handle.join().unwrap_or_else(|_| TeeOutcome {
                capture: StreamCapture::Truncated {
                    reason: "the stderr tee thread panicked".into(),
                },
                passthrough_defect: None,
                tally: thruline_core::test_tally::TestTally::default(),
                failure_line: None,
            });
            (out, err, status)
        });
        for d in [&out_tee.passthrough_defect, &err_tee.passthrough_defect]
            .into_iter()
            .flatten()
        {
            defects.push(d.clone());
        }

        // The child's real status — the one thing this wrapper must never
        // alter, whatever else goes wrong from here on.
        let (exit_code, term_signal, final_code) = match &status {
            Ok(st) => {
                let code = st.code().map(i64::from);
                #[cfg(unix)]
                let signal = {
                    use std::os::unix::process::ExitStatusExt;
                    st.signal().map(i64::from)
                };
                #[cfg(not(unix))]
                let signal: Option<i64> = None;
                let final_code = match (code, signal) {
                    (Some(c), _) => (c & 0xff) as u8,
                    (None, Some(s)) => (128 + (s & 0x7f)) as u8,
                    (None, None) => 1,
                };
                (code, signal, final_code)
            }
            Err(e) => {
                // Waiting failed after a real spawn — a wrapper defect. The
                // child ran; its code is unknowable, which exit 1 states
                // loudly rather than inventing a success.
                let d = format!("waiting for the child failed: {e}");
                eprintln!("thruline check: wrapper defect: {d}");
                defects.push(d);
                (None, None, 1u8)
            }
        };
        if fetching
            && exit_code == Some(0)
            && term_signal.is_none()
            && thruline_core::run_execution::latest(&ledger, &source_session)?
                .is_some_and(|record| record.state.is_open())
        {
            record_successful_fetch(
                &ledger,
                &seed,
                &metadata,
                &out_tee.capture,
                &err_tee.capture,
            )?;
            fetching = false;
            continue;
        }
        let dependency_failure =
            fetching && exit_code.is_some_and(|code| code != 0) && term_signal.is_none();
        break (
            argv,
            seed,
            out_tee,
            err_tee,
            exit_code,
            term_signal,
            final_code,
            dependency_failure,
            metadata,
        );
    };
    let exit_display = final_code;

    // Post-command observation. A failure here is a wrapper-path defect
    // recorded in the binding — never a change to the child's outcome.
    let bound_at = thruline_core::ingest::iso8601_utc_now();
    let post = match wsid::observe(&cwd, Scope::everything()) {
        Ok(o) => IdentityEdge::from_observation(&o),
        Err(e) => {
            let d = format!("post-command observation failed: {e}");
            defects.push(d.clone());
            IdentityEdge::incomplete(d)
        }
    };

    // Declared artifacts, digested as they stand after the run.
    let artifacts: Vec<ArtifactDigest> = args
        .artifacts
        .iter()
        .map(|p| {
            let rel = p.display().to_string();
            let full = if p.is_absolute() {
                p.clone()
            } else {
                cwd.join(p)
            };
            ArtifactDigest {
                path: rel,
                sha256: std::fs::read(&full)
                    .ok()
                    .map(|bytes| thruline_core::chain::sha256_hex(&bytes)),
            }
        })
        .collect();

    // T-3-6-F1 (`Q1`) — what this run counted, over both its streams. Read
    // while the bytes were going past, because they are not kept: the binding
    // holds a digest and a length, never the output. `suites == 0` means the
    // stream carried no `test result:` line, and nothing is stored for it —
    // an absent count reads exactly as it did before this field existed.
    let run_tally = out_tee.tally.plus(err_tee.tally);

    // R-2026-09-27-14 (D): only failure of the product's isolated fetch
    // authorizes Couldn't check. Project streams have no authority here.
    let binding = EvidenceBinding {
        pre: IdentityEdge::from_observation(&pre_obs),
        post,
        exit_code,
        term_signal,
        stdout: out_tee.capture,
        stderr: err_tee.capture,
        artifacts,
        wrapper_defects: defects,
        bound_at,
        ..seed
    };

    // T-3-4 (`B6.8`) — concurrency-safe finalisation. The tree as it stands
    // now, against the tree this run was started against. If they differ, the
    // files were edited while the command was running, and the result does not
    // describe what is on disk. It is recorded, and it is recorded as that:
    // `completed` with the reason `inputs changed`, which `settles()` still
    // allows and which the applicability layer reads as "no longer applies
    // from birth". It is never attached as a fresh pass.
    let inputs_at_finish = thruline_core::examined_inputs::record_id(
        &thruline_core::examined_inputs::observe_source(Path::new(&root), None),
    );
    let inputs_moved = inputs_at_finish != inputs_at_spawn;

    // Store and observe. Failures here are post-spawn: loud on stderr,
    // never a different exit code.
    let mut binding_line = String::new();
    match evb::store(&ledger, &binding) {
        Ok(outcome) => {
            let stored = outcome.stored();
            // The count, beside the binding it came from, before anything
            // reads coverage. A failure is a post-spawn wrapper defect: loud
            // on stderr, never a different exit code, and never a silent
            // absence — a run whose count was lost must read as a run with no
            // count, which is what an absent row already means.
            if run_tally.suites > 0 {
                if let Err(e) = thruline_core::test_tally::record(
                    &ledger,
                    &stored.id,
                    &run_tally,
                    &thruline_core::ingest::iso8601_utc_now(),
                ) {
                    eprintln!(
                        "thruline check: wrapper defect: the run's test count was not \
                         recorded: {e}"
                    );
                }
            }
            // T-3-6-F3 (`Q2`, `B16.10`) — what this run said when it failed,
            // and what it was declared to be, beside the binding they belong
            // to. Same failure policy as the count above: loud on stderr,
            // never a different exit code, and an absent row reads as the
            // record holding no line, which is what it is.
            let run_record = thruline_core::run_record::RunRecord {
                failure_line: out_tee
                    .failure_line
                    .clone()
                    .or_else(|| err_tee.failure_line.clone()),
                evidence_class: args.evidence_class,
            };
            if let Err(e) = thruline_core::run_record::record(
                &ledger,
                &stored.id,
                &run_record,
                &thruline_core::ingest::iso8601_utc_now(),
            ) {
                eprintln!(
                    "thruline check: wrapper defect: what the run said when it failed was \
                     not recorded: {e}"
                );
            }
            record_examined_inputs(
                &ledger,
                &cwd,
                &stored.id,
                &args,
                &argv,
                launch.path_env.as_deref(),
            );
            record_check_session(
                &ledger,
                &for_change,
                &stored.id,
                &source_session,
                &cwd,
                &args,
            );
            // Close the execution record against the binding it produced, so
            // the coverage layer can find it.
            //
            // **T-3-4 verifier finding 1 — a reaped child is not a finished
            // check.** Two guards, because one was not enough:
            //
            //  * A child that died to a SIGNAL never reads `completed`. The
            //    wrapper knows — it has `status.signal()` in hand and prints
            //    exit 143 — and before this it recorded `completed` anyway. A
            //    signalled child with no cancellation record behind it is
            //    `interrupted` with the reason `process signalled`.
            //  * Whatever this wrapper decides, the WRITER refuses a second
            //    ending. `cancel-run` and HQ shutdown append `cancelled`
            //    while this process is still inside `wait()`; the refusal is
            //    read as a value here (`finish_or_refuse`) because "somebody
            //    already wrote what became of this run" is the right answer,
            //    not a defect of ours. It is still said out loud.
            let signalled = term_signal.is_some();
            let ending = if signalled {
                thruline_core::run_execution::Finish {
                    run_key: source_session.clone(),
                    state: thruline_core::run_execution::ExecutionState::Interrupted,
                    reason: Some(thruline_core::run_execution::Reason::ProcessSignalled),
                    actor: None,
                    binding_id: Some(stored.id.clone()),
                    cleanup_verified: None,
                    survivors: None,
                    at: thruline_core::ingest::iso8601_utc_now(),
                }
            } else {
                thruline_core::run_execution::Finish {
                    run_key: source_session.clone(),
                    state: thruline_core::run_execution::ExecutionState::Completed,
                    reason: if dependencies_unavailable {
                        // It reached the end, and what it needed was not on
                        // this machine. It checked nothing and settles
                        // nothing; it is not the project's code failing.
                        Some(thruline_core::run_execution::Reason::PrerequisiteAbsent)
                    } else if inputs_moved {
                        Some(thruline_core::run_execution::Reason::InputsChanged)
                    } else {
                        None
                    },
                    actor: None,
                    binding_id: Some(stored.id.clone()),
                    // The wrapper waited for its own child, which is the
                    // reading: a group whose leader has been reaped and whose
                    // pipes closed holds nothing this run owns.
                    cleanup_verified: None,
                    survivors: None,
                    at: thruline_core::ingest::iso8601_utc_now(),
                }
            };
            match thruline_core::run_execution::finish_or_refuse(&ledger, &ending) {
                Ok(Ok(_)) => {}
                Ok(Err(already)) => {
                    if !args.quiet {
                        eprintln!(
                            "[thruline check] this run was already recorded as {} before the \
                             command exited; that record stands.",
                            already.as_str()
                        );
                    }
                }
                Err(e) => eprintln!(
                    "thruline check: wrapper defect: the execution record was not closed: {e}"
                ),
            }

            // **`B6.8` — the result does not describe what is on disk.** When
            // the tree moved under a run that did reach the end, the freshness
            // layer is told at the moment of completion, so the evidence is
            // `no longer applies` FROM BIRTH rather than from the next time
            // something happens to compare snapshots.
            if inputs_moved && !signalled {
                if let Some((change_id, _)) = &for_change {
                    if let Err(e) = thruline_core::evidence_applicability::mark_inputs_changed(
                        &ledger,
                        change_id,
                        &stored.id,
                        &inputs_at_spawn,
                        &inputs_at_finish,
                        &thruline_core::ingest::iso8601_utc_now(),
                    ) {
                        eprintln!(
                            "thruline check: wrapper defect: the inputs moved during this run \
                             and the record of that was not written: {e}"
                        );
                    }
                }
            }
            match evb::record_command_bound_with_metadata(&ledger, stored, &phase_metadata) {
                Ok((finished_id, bound_id)) => {
                    binding_line = format!(
                        "binding {}…  observations {}… {}…",
                        &stored.id[..16],
                        &finished_id[..12],
                        &bound_id[..12]
                    );
                }
                Err(e) => {
                    eprintln!(
                        "thruline check: wrapper defect: the binding row exists but its \
                         observations were not appended: {e}"
                    );
                }
            }
        }
        Err(e) => {
            eprintln!("thruline check: wrapper defect: the binding was not stored: {e}");
        }
    }

    if !args.quiet {
        let state = evb::state_at_binding(&binding);
        let outcome = match (binding.exit_code, binding.term_signal) {
            (Some(c), _) => format!("exit {c}"),
            (None, Some(s)) => format!("killed by signal {s} (exit {exit_display})"),
            (None, None) => "outcome unreadable".to_string(),
        };
        eprintln!(
            "\n[thruline check] {outcome} · scope {} ({}) · {} at binding\n\
             [thruline check] {}",
            binding.scope.key,
            binding.scope.class.as_str(),
            state.as_str(),
            if binding_line.is_empty() {
                "the execution was NOT recorded — see the defects above".to_string()
            } else {
                binding_line
            }
        );
    }

    Ok(std::process::ExitCode::from(final_code))
}

/// A successful fetch is an observation, never an approval candidate.
fn record_successful_fetch(
    ledger: &Ledger,
    seed: &EvidenceBinding,
    metadata: &serde_json::Map<String, serde_json::Value>,
    stdout: &StreamCapture,
    stderr: &StreamCapture,
) -> Result<()> {
    use thruline_core::observation::{
        self, Completeness, NewObservation, ObservationType, ReceiptTerminal, SourceReceipt,
    };
    let mut receipt = SourceReceipt::new(ReceiptTerminal::ProcessExecution {
        command: evb::rendered_command(&seed.argv),
        exit_code: Some(0),
    });
    receipt.extra.extend(metadata.clone());
    receipt
        .extra
        .insert("stdout".into(), serde_json::to_value(stdout)?);
    receipt
        .extra
        .insert("stderr".into(), serde_json::to_value(stderr)?);
    receipt
        .extra
        .insert("started_at".into(), seed.started_at.clone().into());
    observation::append(
        ledger,
        &NewObservation {
            observation_type: ObservationType::CommandFinished,
            source_adapter: &seed.source_adapter,
            source_session: &seed.source_session,
            repository_identity: &seed.repository_identity,
            workspace_identity: seed.pre.identity.as_deref(),
            observed_at: &thruline_core::ingest::iso8601_utc_now(),
            completeness: if seed.pre.identity.is_some() {
                Completeness::Complete
            } else {
                Completeness::Incomplete
            },
            receipt: &receipt,
        },
    )?;
    Ok(())
}

/// T-3-2b — record this capture under a session of its own, and confirm that
/// session onto the Change the person ran it for.
///
/// **Why the wrapper writes a session row at all.** Every piece of evidence
/// reaches a Change through a session and the link layer's own table. This
/// verb stamped its binding with a `source_session` that was not a row in
/// `sessions`, so the join found nothing and the one verb that records a real
/// execution produced evidence no Change could see. The row is written with
/// the id the binding already carries, so the binding is not rewritten and
/// nothing is stamped twice. The link itself is made by the link layer
/// (`change_link::record_check_session`) and by nothing here: this file knows
/// the verb, not the schema.
///
/// It is written only when a Change was named. A check run for no Change is
/// recorded exactly as before, stays readable through `thruline evidence`, and
/// is attached to nothing.
///
/// A failure here is a post-spawn wrapper defect: loud on stderr, never a
/// different exit code, and never a silent absence.
fn record_check_session(
    ledger: &Ledger,
    for_change: &Option<(String, String)>,
    binding_id: &str,
    session_id: &str,
    cwd: &Path,
    args: &CheckArgs,
) {
    let Some((change_id, project_id)) = for_change else {
        return;
    };
    let actor = invoking_user(args.actor.as_deref());
    let new = thruline_core::change_link::NewCheckSession {
        session_id: session_id.to_string(),
        project_id: project_id.clone(),
        binding_id: binding_id.to_string(),
        change_id: change_id.clone(),
        ran_in: cwd.display().to_string(),
        actor,
        recorded_at: thruline_core::ingest::iso8601_utc_now(),
    };
    match thruline_core::change_link::record_check_session(ledger, &new) {
        Ok(_) => {
            if !args.quiet {
                eprintln!(
                    "[thruline check] recorded for Change {change_id} — it will show \
                     up beside what counts as done"
                );
            }
        }
        Err(e) => eprintln!(
            "thruline check: wrapper defect: the run was recorded but not attached to              Change {change_id}: {e}"
        ),
    }

    // T-S-2: a `--for-change` run is one of the two moments (with `change
    // prepare`) a person can move an existing Change's tree version a second
    // time. Re-binding the Change's own `intended` record through
    // `bind_intended_at` is what makes `evidence_applicability::on_intended_bound`
    // (the freshness trigger) run — the same path `change new` uses at
    // creation, asked again here so a check run against a moved tree
    // re-assesses every criterion's freshness, not only the one this run's
    // own evidence bears on. A failure here is a wrapper defect, loud and
    // non-fatal, exactly like the two calls above it.
    // T-3-3-F1 fix 2: the freshness assessment can only invalidate a
    // `change_criterion_evidence` link that already exists, and those links
    // were derived on coverage *read* only — so a Change nobody opened
    // between two checks had nothing to invalidate and the edit was silent.
    // Deriving them here, at check-write time, makes the link exist before
    // the binding below assesses it. `derive_links` is idempotent and a pure
    // function of what is already stored, so it stays replay-deterministic;
    // it runs BEFORE `bind_intended_at` for exactly that ordering reason.
    // A failure is a wrapper defect: loud, non-fatal.
    if let Err(e) = thruline_core::change_evidence::derive_links(
        ledger,
        change_id,
        &thruline_core::ingest::iso8601_utc_now(),
    ) {
        eprintln!(
            "thruline check: wrapper defect: evidence was not linked to Change {change_id}: {e}"
        );
    }

    match thruline_core::examined_inputs::bind_intended_at(
        ledger,
        change_id,
        &thruline_core::ingest::iso8601_utc_now(),
        Some(cwd),
    ) {
        Ok(_) => {}
        Err(e) => eprintln!(
            "thruline check: wrapper defect: freshness was not reassessed for Change {change_id}: {e}"
        ),
    }
}

/// P6-5 — the tool's version, asked of the program THIS run found.
///
/// The terminal door inherits this process's search path, so asking by name
/// asks the same program the run used — unchanged. The run route can run a
/// check in another environment (its own search path); asking by name there
/// would ask whichever program HQ happens to see, and record the version of
/// the wrong tool — or record it absent while it ran. So the program is
/// looked up on the run's own path, asked by that full path, and recorded
/// under its plain name, exactly as the terminal door records it.
pub(crate) fn probe_tool_where_it_ran(
    program: &str,
    base: &str,
    path_env: Option<&std::ffi::OsStr>,
) -> thruline_core::examined_inputs::ToolInput {
    use thruline_core::examined_inputs as exi;
    let Some(path) = path_env.filter(|_| !program.contains('/')) else {
        return exi::probe_tool(base);
    };
    let found = std::env::split_paths(path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file());
    match found {
        Some(full) => exi::ToolInput {
            name: base.to_string(),
            ..exi::probe_tool(&full.display().to_string())
        },
        None => exi::ToolInput {
            name: base.to_string(),
            version: exi::Input::Unknown,
            present: false,
        },
    }
}

/// T-3-2 — write the examined-inputs record for a captured execution.
///
/// **Every check run gets one.** It is written immediately after the binding
/// row, from the same workspace this command ran in, and from probes that ran
/// here: the workspace identity (the tree hash that includes uncommitted
/// changes), one stored snapshot for the lockfiles and the dirty-diff digest,
/// the program's own `--version`, and — only when the caller named a port —
/// the owner of that listening socket.
///
/// A failure here is a post-spawn wrapper defect: loud on stderr, never a
/// different exit code, and never a fabricated input. A field the probes could
/// not answer is stored as unknown.
fn record_examined_inputs(
    ledger: &Ledger,
    cwd: &Path,
    binding_id: &str,
    args: &CheckArgs,
    argv: &[String],
    path_env: Option<&std::ffi::OsStr>,
) {
    use thruline_core::examined_inputs as exi;

    let at = thruline_core::ingest::iso8601_utc_now();
    let (mut inputs, snapshot) = exi::observe_source_with_snapshot(cwd, &at);
    if let Some(snap) = &snapshot {
        if let Err(e) = thruline_core::snapshot::store(ledger, snap) {
            eprintln!(
                "thruline check: wrapper defect: the workspace snapshot was not stored ({e}) — \
                 the examined-inputs record names no snapshot"
            );
            inputs.snapshot_id = exi::Input::Unknown;
        }
    }
    inputs.subject_kind = exi::SubjectKind::CheckRun;
    inputs.subject_id = binding_id.to_string();
    let (digest, names) = exi::config_digest(cwd, &args.config_files);
    inputs.config_sha256 = digest;
    inputs.config_files = names;
    if let Some(program) = argv.first() {
        let base = program.rsplit('/').next().unwrap_or(program);
        inputs.tools = vec![probe_tool_where_it_ran(program, base, path_env)];
    }
    if let Some(port) = args.app_port {
        inputs.target = exi::probe_running_app(port);
    }
    match exi::record(ledger, &inputs, &at) {
        Ok(outcome) => {
            if !args.quiet {
                eprintln!(
                    "[thruline check] examined inputs {}…{}",
                    &outcome.id[..16.min(outcome.id.len())],
                    if outcome.inserted {
                        ""
                    } else {
                        " (already recorded)"
                    }
                );
            }
        }
        Err(e) => eprintln!(
            "thruline check: wrapper defect: the examined-inputs record was not written: {e}"
        ),
    }
}

/* ── the evidence read path ─────────────────────────────────────────── */

pub(crate) struct EvidenceArgs {
    pub id: Option<String>,
    pub all: bool,
    pub root: Option<PathBuf>,
    pub json: bool,
}

fn freshness_json(f: &Freshness) -> serde_json::Value {
    match f {
        Freshness::CurrentStateConfirmed => serde_json::json!({ "state": f.state().as_str() }),
        Freshness::Stale { bound, current } => serde_json::json!({
            "state": f.state().as_str(), "bound": bound, "current": current,
        }),
        Freshness::InvalidatedByLaterChange { superseded_by } => serde_json::json!({
            "state": f.state().as_str(), "superseded_by": superseded_by,
        }),
        Freshness::Unknown { reason } | Freshness::IncompleteSearch { reason } => {
            serde_json::json!({ "state": f.state().as_str(), "reason": reason })
        }
    }
}

fn print_evaluation_human(e: &thruline_core::evidence_binding::Evaluation) {
    match &e.freshness {
        Freshness::CurrentStateConfirmed => {
            println!(
                "  evaluated {} → CURRENT_STATE_CONFIRMED — the workspace identity now \
                 equals the bound identity",
                e.evaluated_at
            );
        }
        Freshness::Stale { bound, current } => {
            println!(
                "  evaluated {} → STALE — the workspace changed after this evidence\n\
                 \x20   bound   {}\n\x20   current {}",
                e.evaluated_at, bound, current
            );
        }
        Freshness::InvalidatedByLaterChange { superseded_by } => {
            println!(
                "  evaluated {} → INVALIDATED_BY_LATER_CHANGE — a later binding of this \
                 scope covers the current state\n\x20   superseded by binding {}…",
                e.evaluated_at,
                &superseded_by[..16.min(superseded_by.len())]
            );
        }
        Freshness::Unknown { reason } => {
            println!("  evaluated {} → UNKNOWN — {reason}", e.evaluated_at);
        }
        Freshness::IncompleteSearch { reason } => {
            println!(
                "  evaluated {} → INCOMPLETE_SEARCH — {reason}",
                e.evaluated_at
            );
        }
    }
    for obs in &e.appended_observations {
        println!("    recorded: observation {}…", &obs[..16.min(obs.len())]);
    }
}

fn binding_header(stored: &thruline_core::evidence_binding::StoredBinding) -> String {
    let b = &stored.binding;
    let outcome = match (b.exit_code, b.term_signal) {
        (Some(c), _) => format!("exit {c}"),
        (None, Some(s)) => format!("signal {s}"),
        (None, None) => "outcome unreadable".to_string(),
    };
    let all_tests = if establishes_all_tests(&b.scope) {
        " · can establish an all-tests claim"
    } else if b.scope.class == evb::ScopeClass::Test {
        " · a narrow suite: cannot establish an all-tests claim"
    } else {
        ""
    };
    format!(
        "scope {} ({}{all_tests})\n  binding {}…  {}  bound {}",
        b.scope.key,
        b.scope.class.as_str(),
        &stored.id[..16],
        outcome,
        b.bound_at
    )
}

pub(crate) fn run_evidence(db: &Path, args: EvidenceArgs) -> Result<std::process::ExitCode> {
    let ledger = Ledger::open(db).context("opening the ledger")?;
    let evaluated_at = thruline_core::ingest::iso8601_utc_now();

    if let Some(id) = &args.id {
        let Some(stored) = evb::load(&ledger, id).context("loading the binding")? else {
            // Under `--json` this used to print an English sentence on stdout
            // and exit 1 — unparseable for every consumer that pinned the
            // surface. The sentence is preserved, as a field.
            let reason = format!(
                "no binding with id {id} is recorded — nothing to evaluate \
                 (run `thruline check -- <command>` to bind one)"
            );
            if args.json {
                crate::json_out::print(
                    &crate::json_out::EVIDENCE_ONE,
                    serde_json::json!({ "found": false, "id": id, "reason": reason }),
                )?;
            } else {
                println!("{reason}");
            }
            return Ok(std::process::ExitCode::FAILURE);
        };
        let story =
            evb::scope_story(&ledger, &stored, &evaluated_at).context("evaluating the binding")?;
        if args.json {
            let b = &stored.binding;
            crate::json_out::print(
                &crate::json_out::EVIDENCE_ONE,
                serde_json::json!({
                    "found": true,
                    "binding": stored,
                    "state": story.state.as_str(),
                    "freshness": freshness_json(&story.latest.freshness),
                    "bound_at": b.bound_at,
                    "evaluated_at": evaluated_at,
                    "prior_detections": story.prior_detections,
                    "first_detection_at": story.first_detection_at,
                }),
            )?;
        } else {
            println!("{}", binding_header(&stored));
            print_story(&story);
        }
        return Ok(std::process::ExitCode::SUCCESS);
    }

    let root = match &args.root {
        Some(r) => r.clone(),
        None => std::env::current_dir().context("resolving the current directory")?,
    };
    let repo = thruline_core::workspace::repo_identity_at(&root)
        .context("deriving the repository identity")?;
    let Some(repo_id) = repo.repo_id else {
        let reason = format!(
            "no stable repository identity could be derived at {} — bindings are \
             looked up by repository, so there is nothing to report here",
            root.display()
        );
        if args.json {
            crate::json_out::print(
                &crate::json_out::EVIDENCE_LIST,
                serde_json::json!({
                    "repository_identity": serde_json::Value::Null,
                    "evaluated_at": evaluated_at,
                    "bindings": [],
                    "reason": reason,
                }),
            )?;
        } else {
            println!("{reason}");
        }
        return Ok(std::process::ExitCode::SUCCESS);
    };

    let bindings = if args.all {
        evb::all_for_repo(&ledger, &repo_id).context("reading the bindings")?
    } else {
        evb::latest_per_scope(&ledger, &repo_id).context("reading the bindings")?
    };
    if bindings.is_empty() {
        let reason = "no command evidence is bound to this repository yet — run \
                      your check through `thruline check -- <command>` to bind one";
        if args.json {
            crate::json_out::print(
                &crate::json_out::EVIDENCE_LIST,
                serde_json::json!({
                    "repository_identity": repo_id,
                    "evaluated_at": evaluated_at,
                    "bindings": [],
                    "reason": reason,
                }),
            )?;
        } else {
            println!("{reason}");
        }
        return Ok(std::process::ExitCode::SUCCESS);
    }

    if args.json {
        let mut rows = Vec::new();
        for stored in &bindings {
            let story = evb::scope_story(&ledger, stored, &evaluated_at).context("evaluating")?;
            rows.push(serde_json::json!({
                "binding_id": stored.id,
                "scope_key": stored.binding.scope.key,
                "scope_class": stored.binding.scope.class.as_str(),
                "establishes_all_tests": establishes_all_tests(&stored.binding.scope),
                "exit_code": stored.binding.exit_code,
                "term_signal": stored.binding.term_signal,
                "bound_at": stored.binding.bound_at,
                "evaluated_at": evaluated_at,
                "state": story.state.as_str(),
                "freshness": freshness_json(&story.latest.freshness),
                "prior_detections": story.prior_detections,
                "first_detection_at": story.first_detection_at,
                "appended_observations": story.latest.appended_observations,
            }));
        }
        crate::json_out::print(
            &crate::json_out::EVIDENCE_LIST,
            serde_json::json!({
                "repository_identity": repo_id,
                "evaluated_at": evaluated_at,
                "bindings": rows,
            }),
        )?;
        return Ok(std::process::ExitCode::SUCCESS);
    }

    println!(
        "EVIDENCE — command evidence bound to exact workspace state (repo {}…)",
        &repo_id[..16.min(repo_id.len())]
    );
    for stored in &bindings {
        println!();
        println!("{}", binding_header(stored));
        let story = evb::scope_story(&ledger, stored, &evaluated_at).context("evaluating")?;
        print_story(&story);
    }
    Ok(std::process::ExitCode::SUCCESS)
}

fn print_story(story: &thruline_core::evidence_binding::ScopeStory) {
    if story.state == EvidenceState::SubsequentlyConfirmed {
        println!(
            "  evaluated {} → SUBSEQUENTLY_CONFIRMED — this scope's evidence was \
             recorded stale or invalidated {} time(s), first at {}; the current \
             workspace identity is bound and equal now. The earlier record stands.",
            story.latest.evaluated_at,
            story.prior_detections,
            story
                .first_detection_at
                .as_deref()
                .unwrap_or("(moment unrecorded)")
        );
        for obs in &story.latest.appended_observations {
            println!("    recorded: observation {}…", &obs[..16.min(obs.len())]);
        }
    } else {
        print_evaluation_human(&story.latest);
    }
}

/// T-4-4-ATTEMPT-BUDGET: the run route's door into this wrapper meets the same
/// stop as the terminal, and its reason reaches the sheet through `refusal`.
#[cfg(all(test, unix))]
mod attempt_budget_stop {
    use super::*;

    fn git(cwd: &Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}: {out:?}");
    }

    #[test]
    fn a_spent_budget_refuses_the_run_route_before_the_spawn_and_says_why() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).expect("mkdir");
        git(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("a.txt"), "a\n").expect("write");
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "seed"]);
        let db = dir.path().join("ledger.db");
        let ledger = Ledger::open(&db).expect("open");
        ledger
            .conn()
            .execute_batch(&format!(
                "INSERT INTO projects(id,root_path,source,display_name,created_at)
                   VALUES('cccccccc-0000-4000-8000-0000000000c4','{}','claude_code','P',
                          '2026-01-01T00:00:00Z');
                 INSERT INTO changes(id,project_id,opened_at,title,purpose,requester,
                     request_source)
                   VALUES('chg_route','cccccccc-0000-4000-8000-0000000000c4',
                          '2026-01-01T00:00:00Z','Retry','Kept','sam','person');
                 INSERT INTO change_run_executions(id,run_key,change_id,name,state,pid,
                     process_group,started_at,examined_inputs_id,cleanup_verified,survivors,
                     recorded_at)
                   VALUES('rx_1','run-1','chg_route','check','completed',1,1,
                          '2026-01-02T00:00:00Z','exi_elsewhere',1,0,'2026-01-02T00:00:00Z');",
                repo.display()
            ))
            .expect("seed");
        // A budget of one, set after that run; one new version is then
        // examined, which spends it.
        thruline_core::attempt_budget::set(&ledger, "chg_route", 1, "sam", "2026-01-02T00:01:00Z")
            .expect("store")
            .expect("set");
        ledger
            .conn()
            .execute_batch(
                "INSERT INTO change_run_executions(id,run_key,change_id,name,state,pid,
                     process_group,started_at,examined_inputs_id,cleanup_verified,survivors,
                     recorded_at)
                   VALUES('rx_2','run-2','chg_route','check','completed',1,1,
                          '2026-01-02T00:02:00Z','exi_attempt_one',1,0,'2026-01-02T00:02:00Z');",
            )
            .expect("attempt one");
        drop(ledger);

        let marker = dir.path().join("ran");
        let refusal = std::sync::Arc::new(std::sync::Mutex::new(None));
        let code = run_check_at(
            &db,
            CheckArgs {
                shell: false,
                quiet: true,
                artifacts: Vec::new(),
                app_port: None,
                config_files: Vec::new(),
                for_change: Some("chg_route".to_string()),
                actor: None,
                evidence_class: None,
                command: vec!["touch".to_string(), marker.display().to_string()],
            },
            Launch {
                cwd: Some(repo.clone()),
                path_env: None,
                rustup_toolchain: None,
                run_key: Some("run-route-3".to_string()),
                detached: true,
                refusal: Some(std::sync::Arc::clone(&refusal)),
                offline: true,
                live: None,
                env: Vec::new(),
            },
        )
        .expect("the wrapper answers");
        assert_eq!(code, std::process::ExitCode::from(WRAPPER_FAILURE));
        assert!(!marker.exists(), "the command was spawned");
        let reason = refusal.lock().expect("lock").clone().expect("a reason");
        assert!(
            reason.contains("sam allowed 1 repair attempt")
                && reason.contains("thruline change attempts chg_route --budget"),
            "{reason}"
        );
        let ledger = Ledger::open(&db).expect("open");
        assert!(thruline_core::run_execution::latest(&ledger, "run-route-3")
            .expect("read")
            .is_none());
    }

    // ── T-4-4-ATTEMPT-BUDGET-R2 ─────────────────────────────────────────

    const PROJECT: &str = "cccccccc-0000-4000-8000-0000000000c5";

    fn repo_with(root: &Path, name: &str, body: &str) -> PathBuf {
        let repo = root.join(name);
        std::fs::create_dir_all(&repo).expect("mkdir");
        git(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("lib.rs"), body).expect("write");
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "seed"]);
        repo
    }

    /// One project, one Change, one run from before any budget (the check that
    /// showed the problem), then a budget of `budget` set by sam.
    fn seed_with_budget(db: &Path, repo: &Path, budget: u32) {
        let ledger = Ledger::open(db).expect("open");
        ledger
            .conn()
            .execute_batch(&format!(
                "INSERT INTO projects(id,root_path,source,display_name,created_at)
                   VALUES('{PROJECT}','{}','claude_code','P','2026-01-01T00:00:00Z');
                 INSERT INTO changes(id,project_id,opened_at,title,purpose,requester,
                     request_source)
                   VALUES('chg_r2','{PROJECT}','2026-01-01T00:00:00Z','Retry','Kept','sam',
                          'person');
                 INSERT INTO change_run_executions(id,run_key,change_id,name,state,pid,
                     process_group,started_at,examined_inputs_id,cleanup_verified,survivors,
                     recorded_at)
                   VALUES('rx_before','run-before','chg_r2','check','completed',1,1,
                          '2026-01-02T00:00:00Z','exi_before',1,0,'2026-01-02T00:00:00Z');",
                repo.display()
            ))
            .expect("seed");
        thruline_core::attempt_budget::set(
            &ledger,
            "chg_r2",
            budget,
            "sam",
            "2026-01-02T00:01:00Z",
        )
        .expect("store")
        .expect("set");
    }

    fn check_of(command: Vec<String>) -> CheckArgs {
        CheckArgs {
            shell: false,
            quiet: true,
            artifacts: Vec::new(),
            app_port: None,
            config_files: Vec::new(),
            for_change: Some("chg_r2".to_string()),
            actor: None,
            evidence_class: None,
            command,
        }
    }

    type Said = std::sync::Arc<std::sync::Mutex<Option<String>>>;

    fn door(repo: &Path, run_key: &str, refusal: &Said) -> Launch {
        Launch {
            cwd: Some(repo.to_path_buf()),
            path_env: None,
            rustup_toolchain: None,
            run_key: Some(run_key.to_string()),
            detached: true,
            refusal: Some(std::sync::Arc::clone(refusal)),
            offline: true,
            live: None,
            env: Vec::new(),
        }
    }

    fn touch(marker: &Path) -> Vec<String> {
        vec!["touch".to_string(), marker.display().to_string()]
    }

    fn used(db: &Path) -> u32 {
        let ledger = Ledger::open(db).expect("open");
        thruline_core::attempt_budget::status(&ledger, "chg_r2")
            .expect("read")
            .expect("a budget")
            .used
    }

    /// The adversary's first break, by order rather than by timing: one
    /// attempt is left; caller A is told it may start, and before A's process
    /// or record exists, caller B — a new version, through the same door —
    /// asks. Exactly one of them may run, and the record never holds more
    /// attempts than the person allowed.
    #[test]
    fn two_callers_racing_for_the_last_attempt_admit_exactly_one() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo_a = repo_with(dir.path(), "a", "fn a() {}\n");
        let repo_b = repo_with(dir.path(), "b", "fn b() {}\n");
        let db = dir.path().join("ledger.db");
        seed_with_budget(&db, &repo_a, 1);

        let marker_a = dir.path().join("ran-a");
        let marker_b = dir.path().join("ran-b");
        let said_a: Said = std::sync::Arc::new(std::sync::Mutex::new(None));
        let said_b: Said = std::sync::Arc::new(std::sync::Mutex::new(None));
        let code_b = std::sync::Arc::new(std::sync::Mutex::new(None));
        {
            let (db, repo_b, marker_b, said_b, code_b) = (
                db.clone(),
                repo_b.clone(),
                marker_b.clone(),
                std::sync::Arc::clone(&said_b),
                std::sync::Arc::clone(&code_b),
            );
            AFTER_ADMIT.with(|slot| {
                *slot.borrow_mut() = Some(Box::new(move || {
                    let code = run_check_at(
                        &db,
                        check_of(touch(&marker_b)),
                        door(&repo_b, "run-b", &said_b),
                    )
                    .expect("B's wrapper answers");
                    *code_b.lock().expect("lock") = Some(code);
                }))
            });
        }
        let code_a = run_check_at(
            &db,
            check_of(touch(&marker_a)),
            door(&repo_a, "run-a", &said_a),
        )
        .expect("A's wrapper answers");
        assert!(
            AFTER_ADMIT.with(|slot| slot.borrow().is_none()),
            "the interleaving was not driven: A never reached its admission"
        );

        assert_eq!(code_a, std::process::ExitCode::SUCCESS, "{:?}", said_a);
        assert!(marker_a.exists(), "A was admitted and must have run");
        assert_eq!(
            *code_b.lock().expect("lock"),
            Some(std::process::ExitCode::from(WRAPPER_FAILURE)),
            "B raced past the stop: the last attempt was admitted twice"
        );
        assert!(!marker_b.exists(), "B's command was spawned");
        let reason = said_b.lock().expect("lock").clone().expect("B says why");
        assert!(reason.contains("no check was started"), "{reason}");
        let ledger = Ledger::open(&db).expect("open");
        assert!(
            thruline_core::run_execution::latest(&ledger, "run-b")
                .expect("read")
                .is_none(),
            "a refused caller wrote an execution row"
        );
        drop(ledger);
        assert_eq!(used(&db), 1, "more attempts recorded than the one allowed");

        // A caller arriving after A's record exists sees A's attempt as used.
        let said_c: Said = std::sync::Arc::new(std::sync::Mutex::new(None));
        let code_c = run_check_at(
            &db,
            check_of(touch(&marker_b)),
            door(&repo_b, "run-c", &said_c),
        )
        .expect("C's wrapper answers");
        assert_eq!(code_c, std::process::ExitCode::from(WRAPPER_FAILURE));
        assert!(!marker_b.exists());
        let reason = said_c.lock().expect("lock").clone().expect("C says why");
        assert!(reason.contains("sam allowed 1 repair attempt"), "{reason}");
        assert_eq!(used(&db), 1);
    }

    /// The adversary's second break: a file of exactly the kind ThruLine
    /// itself writes into the project folder (`.thruline/decisions/*.json`,
    /// written when a person approves a version) changes nothing a person
    /// changed, and must not spend an attempt.
    ///
    /// PARKED (T-4-4-ATTEMPT-BUDGET-R2 item 3): the only per-run version the
    /// record holds is `examined_inputs_id`, a hash over the whole folder that
    /// includes `.thruline/`; the counter cannot see which files made two
    /// hashes differ, and no second per-run value may be stored without DDL.
    /// The fix is an exclusion in `examined_inputs` / `workspace_identity`,
    /// which readiness and applicability also read — its own card,
    /// EXAMINED-INPUTS-OWN-FILES (P-2026-09-29-3). Until it lands the limit is
    /// stated wherever a person reads the count (T-4-4-ATTEMPT-BUDGET-R3). Run
    /// with `--ignored` to see it red.
    #[test]
    #[ignore = "PARKED: needs card EXAMINED-INPUTS-OWN-FILES (examined_inputs excludes .thruline/); the limit is stated meanwhile (P-2026-09-29-3)"]
    fn a_file_thruline_itself_writes_does_not_spend_an_attempt() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = repo_with(dir.path(), "repo", "fn a() {}\n");
        let db = dir.path().join("ledger.db");
        seed_with_budget(&db, &repo, 3);
        let said: Said = std::sync::Arc::new(std::sync::Mutex::new(None));

        let first = run_check_at(
            &db,
            check_of(vec!["true".to_string()]),
            door(&repo, "run-1", &said),
        )
        .expect("answers");
        assert_eq!(first, std::process::ExitCode::SUCCESS, "{said:?}");
        assert_eq!(used(&db), 1);

        std::fs::create_dir_all(repo.join(".thruline/decisions")).expect("mkdir");
        std::fs::write(
            repo.join(".thruline/decisions/version.json"),
            "{\"schema\":1}\n",
        )
        .expect("write");
        let second = run_check_at(
            &db,
            check_of(vec!["true".to_string()]),
            door(&repo, "run-2", &said),
        )
        .expect("answers");
        assert_eq!(second, std::process::ExitCode::SUCCESS, "{said:?}");
        assert_eq!(
            used(&db),
            1,
            "a file ThruLine itself writes spent one of the person's attempts"
        );
    }

    /// The other side of the same rule: a change a person made to the source
    /// is a new attempt.
    #[test]
    fn a_real_source_change_is_still_a_new_attempt() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = repo_with(dir.path(), "repo", "fn a() {}\n");
        let db = dir.path().join("ledger.db");
        seed_with_budget(&db, &repo, 3);
        let said: Said = std::sync::Arc::new(std::sync::Mutex::new(None));

        run_check_at(
            &db,
            check_of(vec!["true".to_string()]),
            door(&repo, "run-1", &said),
        )
        .expect("answers");
        assert_eq!(used(&db), 1);
        std::fs::write(repo.join("lib.rs"), "fn a() { let _ = 1; }\n").expect("edit");
        run_check_at(
            &db,
            check_of(vec!["true".to_string()]),
            door(&repo, "run-2", &said),
        )
        .expect("answers");
        assert_eq!(used(&db), 2, "a source change was not counted: {said:?}");
    }
}

/// C30 R-RUN — pytest's count, read by the wrapper from pytest's own summary
/// line; and the tail a detached run passes its streams to.
///
/// The `crates/cli/tests/fixtures/pytest/*.txt` files are **real pytest
/// output**: pytest 9.0.1 on Python 3.13, run on small projects written for
/// the purpose, stdout and stderr together, every byte pytest's own except the
/// folder the projects sat in (now `/work/samples`) and Python's install path
/// (now `/usr/lib/python3`). Each `.meta` beside it records the arguments and
/// the exit code. The forms no installed pytest prints on demand — a session
/// over a minute, over a day, and the `in N seconds` wording of older pytest —
/// are written by hand from pytest's `format_session_duration`, and are marked
/// so where they are used.
#[cfg(test)]
mod pytest_tally_and_live_output {
    use super::*;
    use thruline_core::test_tally::TestTally;

    fn fixture(name: &str) -> Vec<u8> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/pytest")
            .join(format!("{name}.txt"));
        std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    /// The whole stream through the scanner the tee uses, in chunks of
    /// `chunk` bytes, so a summary split across two reads is read whole.
    fn scanned(bytes: &[u8], chunk: usize) -> TestTally {
        let mut scanner = PytestScanner::default();
        for piece in bytes.chunks(chunk.max(1)) {
            scanner.feed(piece);
        }
        scanner.finish()
    }

    fn tally(passed: u64, failed: u64, ignored: u64) -> TestTally {
        TestTally {
            passed,
            failed,
            ignored,
            suites: 1,
        }
    }

    /// Every captured form, read whole and in awkward chunks, against what
    /// pytest's own summary says. `None`: pytest printed no session summary,
    /// and no count may be stored.
    #[test]
    fn every_real_pytest_summary_reads_as_pytest_counted_it() {
        let cases: &[(&str, Option<TestTally>)] = &[
            ("pass_default", Some(tally(5, 0, 0))),
            ("pass_q", Some(tally(5, 0, 0))),
            ("pass_v", Some(tally(5, 0, 0))),
            // `-qq` prints no summary at all: nothing is invented.
            ("pass_qq", None),
            // 1 failed + 1 error; 3 passed; 1 skipped. xfailed, xpassed and the
            // warning are recognised and counted nowhere.
            ("mixed_default", Some(tally(3, 2, 1))),
            ("mixed_q", Some(tally(3, 2, 1))),
            ("mixed_rA", Some(tally(3, 2, 1))),
            ("color_forced", Some(tally(3, 2, 1))),
            ("exitfirst", Some(tally(2, 1, 0))),
            // A module that could not be collected: `1 error`, exit 2.
            ("collect_error", Some(tally(0, 1, 0))),
            ("collect_error_q", Some(tally(0, 1, 0))),
            // pytest said it ran nothing, and that is what is stored.
            ("no_tests", Some(tally(0, 0, 0))),
            ("no_tests_q", Some(tally(0, 0, 0))),
            ("deselected_all", Some(tally(0, 0, 0))),
            ("deselected_some", Some(tally(2, 0, 0))),
            ("skip_only", Some(tally(0, 0, 1))),
            // A strict xpass is printed as `failed` by pytest itself.
            ("strict_xpass", Some(tally(1, 1, 0))),
            // pytest 9's subtests: a failing subtest and its test are both
            // `failed`; `subtests passed` is a count of subtests, not tests.
            ("subtests", Some(tally(0, 2, 0))),
            ("subtests_q", Some(tally(0, 2, 0))),
            ("subtests_pass_v", Some(tally(1, 0, 0))),
            // `--collect-only` ran nothing; "5 tests collected" is no result.
            ("collect_only", None),
            ("collect_only_q", None),
        ];
        for (name, expected) in cases {
            let bytes = fixture(name);
            for chunk in [1, 7, 64 * 1024] {
                let got = scanned(&bytes, chunk);
                match expected {
                    Some(t) => assert_eq!(got, *t, "{name} in chunks of {chunk}"),
                    None => assert_eq!(got.suites, 0, "{name}: a count was invented: {got:?}"),
                }
            }
        }
    }

    #[test]
    fn the_forms_no_installed_pytest_prints_on_demand_are_read_too() {
        // Hand-written from pytest's `format_session_duration`: past a minute
        // the clock follows the seconds, past a day the days come first.
        assert_eq!(
            parse_pytest_summary("====== 3 passed in 65.21s (0:01:05) ======"),
            Some(tally(3, 0, 0))
        );
        assert_eq!(
            parse_pytest_summary("= 1 failed, 2 passed in 90061.50s (1 day, 1:01:01) ="),
            Some(tally(2, 1, 0))
        );
        assert_eq!(
            parse_pytest_summary("2 passed in 172800.00s (2 days, 0:00:00)"),
            Some(tally(2, 0, 0))
        );
        // Hand-written: the wording of pytest before 6.0.
        assert_eq!(
            parse_pytest_summary("========== 4 passed, 1 skipped in 0.12 seconds =========="),
            Some(tally(4, 0, 1))
        );
        // The card's own examples.
        assert_eq!(
            parse_pytest_summary("== 2 failed, 3 passed in 1.02s =="),
            Some(tally(3, 2, 0))
        );
        assert_eq!(
            parse_pytest_summary("1 failed, 4 passed, 1 skipped in 0.3s"),
            Some(tally(4, 1, 1))
        );
        assert_eq!(
            parse_pytest_summary("5 passed in 0.12s"),
            Some(tally(5, 0, 0))
        );
        assert_eq!(
            parse_pytest_summary("3 passed, 2 errors in 0.40s\r"),
            Some(tally(3, 2, 0))
        );
    }

    /// Lines that end in a duration, or name a count, and are not pytest's
    /// summary. Each must yield nothing — never a zero.
    #[test]
    fn a_line_that_is_not_pytests_summary_is_no_count() {
        for line in [
            "test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s",
            "    Finished `test` profile [unoptimized] target(s) in 0.05s",
            "finished the job in 3s",
            "5 apples in 2s",
            "3 passed",
            "3 passed in",
            "3 passed in soon",
            "3 passed in 1.2",
            "3 passed in 1.2s (later)",
            "3 passed in 1.2s (1:2)",
            "= 3 passed in 0.10s",
            "3 passed in 0.10s =",
            "FAILED t.py::x - 3 passed in 0.10s",
            "1 failed, 1 failed in 0.10s",
            "-3 passed in 0.10s",
            "three passed in 0.10s",
            "3  passed in 0.10s",
            "3 passed,4 failed in 0.10s",
            "5 tests collected in 0.01s",
            "no tests collected in 0.01s",
            "Tests:       1 failed, 3 passed, 4 total",
            "",
        ] {
            assert_eq!(parse_pytest_summary(line), None, "{line:?}");
        }
    }

    /// A summary printed earlier in the stream — a test that runs pytest and
    /// shows that output when it fails — is not this run's count. The last
    /// summary is.
    #[test]
    fn the_last_summary_in_a_stream_is_the_runs_count() {
        let stream = b"---- Captured stdout call ----\n\
                       ========== 7 passed in 0.01s ==========\n\
                       FAILED test_plugin.py::test_inner - assert 0\n\
                       ========== 1 failed in 0.20s ==========\n";
        assert_eq!(scanned(stream, 5), tally(0, 1, 0));
    }

    /// Through the tee itself: libtest's lines and pytest's summary in one
    /// stream are both this run's, and both are kept; a stream with neither
    /// stores no count.
    #[test]
    fn the_tee_counts_pytest_beside_libtest_and_invents_nothing() {
        let both = b"test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n\
                     ======= 3 passed, 1 failed in 0.40s =======\n";
        let out = tee(&both[..], std::io::sink());
        assert_eq!(
            out.tally,
            TestTally {
                passed: 5,
                failed: 1,
                ignored: 0,
                suites: 2
            }
        );
        let pytest = tee(&fixture("mixed_default")[..], std::io::sink());
        assert_eq!(pytest.tally, tally(3, 2, 1));
        let none = tee(&fixture("pass_qq")[..], std::io::sink());
        assert_eq!(
            none.tally.suites, 0,
            "a count was invented: {:?}",
            none.tally
        );
    }

    /* ── the tail ──────────────────────────────────────────────────────── */

    #[test]
    fn the_tail_holds_whole_lines_across_writes_and_never_mixes_the_two_streams() {
        let live = LiveOutput::new();
        live.take(STDOUT, b"compiling");
        live.take(STDERR, b"warn");
        live.take(STDOUT, b" one\nrunning ");
        live.take(STDERR, b"ing: two\n");
        assert_eq!(
            live.read(),
            LiveLines {
                lines: vec!["compiling one".to_string(), "warning: two".to_string()],
                cut: false,
            },
            // A line still being printed is not served until it ends: it may end in
            // half a token, which a masker cannot recognise.
        );
        live.take(STDOUT, b"3 tests\n");
        assert_eq!(
            live.read().lines,
            vec!["compiling one", "warning: two", "running 3 tests"]
        );
    }

    #[test]
    fn the_tail_keeps_the_newest_lines_and_says_when_it_dropped_any() {
        let live = LiveOutput::new();
        for i in 1..=LIVE_LINES + 25 {
            live.take(STDOUT, format!("line {i}\n").as_bytes());
        }
        let read = live.read();
        assert_eq!(read.lines.len(), LIVE_LINES);
        assert_eq!(read.lines.first().map(String::as_str), Some("line 26"));
        assert_eq!(
            read.lines.last().map(String::as_str),
            Some(format!("line {}", LIVE_LINES + 25).as_str())
        );
        assert!(
            read.cut,
            "older lines were dropped and the tail did not say so"
        );

        // The byte bound holds too, newest kept.
        let wide = LiveOutput::new();
        let row = "w".repeat(LIVE_LINE_BYTES - 10);
        for i in 0..40 {
            wide.take(STDOUT, format!("{i:03} {row}\n").as_bytes());
        }
        let read = wide.read();
        let bytes: usize = read.lines.iter().map(String::len).sum();
        assert!(bytes <= LIVE_BYTES, "{bytes} bytes served");
        assert!(read.lines.len() < 40);
        assert!(read.lines.last().is_some_and(|l| l.starts_with("039 ")));
        assert!(read.cut);

        // A tail that dropped nothing says so.
        let short = LiveOutput::new();
        short.take(STDOUT, b"a\nb\n");
        assert!(!short.read().cut);
    }

    #[test]
    fn an_overlong_line_is_cut_at_a_character_and_said() {
        let live = LiveOutput::new();
        // A two-byte character straddles the cut.
        let mut long = "x".repeat(LIVE_TAIL_LINE_BYTES - 1);
        long.push('é');
        long.push_str(&"y".repeat(20_000));
        live.take(STDOUT, long.as_bytes());
        // Still being printed: not served yet.
        assert!(live.read().lines.is_empty());
        live.take(STDOUT, b"\nnext\n");
        let read = live.read();
        assert_eq!(read.lines.len(), 2);
        assert_eq!(read.lines[0], "x".repeat(LIVE_TAIL_LINE_BYTES - 1));
        assert!(!read.lines[0].contains('\u{fffd}'));
        assert_eq!(read.lines[1], "next");
        assert!(read.cut);

        // A line whose kept bytes are all colour codes is short once they are
        // removed — and the words after them were still dropped, which is said.
        let coloured = LiveOutput::new();
        coloured.take(STDOUT, "\x1b[0m".repeat(LIVE_TAIL_LINE_BYTES).as_bytes());
        coloured.take(STDOUT, b"the words after the colour codes\n");
        let read = coloured.read();
        assert_eq!(read.lines, vec![String::new()]);
        assert!(read.cut, "words were dropped and the tail did not say so");
    }

    #[test]
    fn colour_and_redrawn_lines_are_shown_as_a_terminal_would_leave_them() {
        let live = LiveOutput::new();
        live.take(STDOUT, b"\x1b[32m5 passed\x1b[0m in 0.1s\r\n");
        live.take(STDOUT, b"progress 10%\rprogress 90%\rdone\n");
        live.take(STDOUT, b"\x1b]0;title\x07tab\there\x08\n");
        assert_eq!(
            live.read().lines,
            vec!["5 passed in 0.1s", "done", "tab\there"]
        );
    }

    /* ── the record is unchanged by the tail ───────────────────────────── */

    fn git(cwd: &Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}: {out:?}");
    }

    /// What the record holds of one run: each stream's capture and the count.
    fn recorded(db: &Path, run_key: &str) -> (String, String, Option<TestTally>) {
        let ledger = Ledger::open(db).expect("open");
        let record = thruline_core::run_execution::latest(&ledger, run_key)
            .expect("read")
            .expect("a record");
        let binding_id = record.binding_id.expect("a binding");
        let stored = evb::load(&ledger, &binding_id)
            .expect("read")
            .expect("the binding");
        (
            serde_json::to_string(&stored.binding.stdout).expect("json"),
            serde_json::to_string(&stored.binding.stderr).expect("json"),
            thruline_core::test_tally::of_binding(&ledger, &binding_id).expect("read"),
        )
    }

    /// **Never change what is recorded.** The same run, detached, with the
    /// tail listening and without it: both streams' digests and lengths and the
    /// count are identical; the tail holds what it printed.
    #[cfg(unix)]
    #[test]
    fn a_run_with_the_tail_listening_records_exactly_what_it_records_without() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).expect("mkdir");
        git(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("a.txt"), "a\n").expect("write");
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "seed"]);
        let db = dir.path().join("ledger.db");
        let script = "printf 'collected 5 items\\n\\n'; printf 'warn: slow\\n' 1>&2; \
                      printf '===== 5 passed in 0.01s =====\\n'";
        let launch = |run_key: &str, live: Option<std::sync::Arc<LiveOutput>>| Launch {
            cwd: Some(repo.clone()),
            path_env: None,
            rustup_toolchain: None,
            run_key: Some(run_key.to_string()),
            detached: true,
            refusal: None,
            offline: false,
            live,
            env: Vec::new(),
        };
        let args = || CheckArgs {
            shell: false,
            quiet: true,
            artifacts: Vec::new(),
            app_port: None,
            config_files: Vec::new(),
            for_change: None,
            actor: None,
            evidence_class: None,
            command: vec!["sh".to_string(), "-c".to_string(), script.to_string()],
        };
        let without = run_check_at(&db, args(), launch("run-without", None)).expect("runs");
        let live = std::sync::Arc::new(LiveOutput::new());
        let with = run_check_at(
            &db,
            args(),
            launch("run-with", Some(std::sync::Arc::clone(&live))),
        )
        .expect("runs");
        assert_eq!(without, std::process::ExitCode::SUCCESS);
        assert_eq!(with, std::process::ExitCode::SUCCESS);
        let a = recorded(&db, "run-without");
        let b = recorded(&db, "run-with");
        assert_eq!(a, b, "the tail changed what the record holds");
        assert_eq!(a.2, Some(tally(5, 0, 0)), "pytest's count is recorded");
        let mut lines = live.read().lines;
        lines.sort();
        assert_eq!(
            lines,
            vec![
                "",
                "===== 5 passed in 0.01s =====",
                "collected 5 items",
                "warn: slow"
            ]
        );
    }
}
