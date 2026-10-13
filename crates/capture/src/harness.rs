//! The harness-adapter boundary — BUILD-PLAN B.4.
//!
//! > *"the harness adapter boundary is a `trait HarnessAdapter` with one method
//! > that maps a `(harness, hook_event_name, serde_json::Value)` to zero or
//! > more `core::Event`s … Everything downstream of that trait must be
//! > harness-free."*
//!
//! It is a **descriptor**, not a trait, because a trait with one implementation
//! per harness and no behaviour to override is a vtable standing in for a
//! table (Rule 2). What varies between the three harnesses is data: which
//! events they emit, what those events are called, and which §3.1 kind each
//! maps to. What does not vary is everything else — read stdin, redact before
//! anything reaches disk, write one line, exit 0 in silence.
//!
//! # Why the harness is recorded on the line and not read off the filename
//!
//! Each harness spools to `spool/<harness>.jsonl`, so the filename does carry
//! it. The line carries it anyway: a fact that only exists in a path is a fact
//! that a rename, a `--spool` flag or a copy silently changes, and the fold
//! stamps `source` from it. One recorded field beats one naming convention.
//!
//! # The three contracts, and where they differ
//!
//! | | Claude Code | Codex | Gemini |
//! |---|---|---|---|
//! | tool-post event | `PostToolUse` **+ `PostToolUseFailure`** | `PostToolUse` (fires on non-zero, but **carries no outcome** — CONFLICT-6b) | `AfterTool` (failure in `tool_response.error`) |
//! | stop analogue | `Stop` | `Stop` | `AfterAgent` |
//! | compaction | `PreCompact` + `PostCompact` | `PreCompact` + `PostCompact` | `PreCompress` **only** |
//! | timeout unit | seconds | seconds | **milliseconds** |
//!
//! Each row is a way to be silently wrong, and each is verified against the
//! vendor documentation in the per-harness module rather than assumed here.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::redaction::Redactor;

/// One harness's hook contract, as data.
#[derive(Debug, Clone, Copy)]
pub struct Harness {
    /// `claude-code` | `codex` | `gemini`. Recorded on every spool line and
    /// used for the spool file name.
    pub name: &'static str,
    /// The events `thruline init` subscribes to, verified against the vendor
    /// documentation in the harness's own module.
    pub subscribed: &'static [&'static str],
    /// Hook event name → §3.1 event kind.
    pub kind_for: fn(&str) -> Option<&'static str>,
    /// The `source` stamped on folded events (§3.1).
    pub source: &'static str,
    /// **Can `thruline hook-install` write this harness's registration?**
    ///
    /// Subscribing to a contract and *installing* the subscription are two
    /// facts, and the second used to live as a literal `true` in the pilot's
    /// `provides_hook_events` while the installer bailed on the same harness.
    /// That is the sixth shape in the guard family — *a constant asserting a
    /// capability the code does not have* — and the fix is that the fact lives
    /// once, here, in the table the installer and the pilot both read.
    pub installable: bool,
    /// The timeout to register for one event, in the harness's own unit, when
    /// the harness needs one written down. `None` where the vendor default is
    /// safe to inherit.
    ///
    /// Codex is the only one of the three whose default on some events (**1 s**
    /// on `SessionEnd`, capped at 3 s) is tighter than a cold binary start, so
    /// it is the only one that must state a number. Gemini's unit is
    /// **milliseconds** — the row exists so that nobody later writes `5` here
    /// meaning seconds.
    pub timeout_for: Option<fn(&str) -> u64>,
}

/// Every harness this build knows.
pub const ALL: &[Harness] = &[CLAUDE_CODE, CODEX, GEMINI];

pub const CLAUDE_CODE: Harness = Harness {
    name: "claude-code",
    subscribed: crate::claude_code::SUBSCRIBED_EVENTS,
    kind_for: crate::claude_code::event_kind_for,
    source: "claude-code-hook",
    installable: true,
    // Claude Code's default is 60 s on every event, which the 50 ms capture
    // hook fits inside with three orders of magnitude to spare.
    timeout_for: None,
};

pub const CODEX: Harness = Harness {
    name: "codex",
    subscribed: crate::codex::SUBSCRIBED_EVENTS,
    kind_for: crate::codex::event_kind_for,
    source: "codex-hook",
    installable: true,
    timeout_for: Some(crate::codex::timeout_secs_for),
};

pub const GEMINI: Harness = Harness {
    name: "gemini",
    subscribed: crate::gemini::SUBSCRIBED_EVENTS,
    kind_for: crate::gemini::event_kind_for,
    source: "gemini-hook",
    // The event contract is mapped; the registration file is not written yet.
    installable: false,
    timeout_for: None,
};

/// Look one up by the name a user types after `thruline hook`.
pub fn by_name(name: &str) -> Option<Harness> {
    ALL.iter().copied().find(|h| h.name == name)
}

/// `$THRULINE_HOME/spool/<harness>.jsonl`.
pub fn default_spool(harness: &Harness) -> Option<PathBuf> {
    let home = std::env::var_os("THRULINE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".thruline")))?;
    Some(home.join("spool").join(format!("{}.jsonl", harness.name)))
}

/// **The one spool resolution, used at install time and at fire time.**
///
/// `THRULINE_HOOK_SPOOL` names the file directly, then [`default_spool`]'s
/// `$THRULINE_HOME`/`$HOME` chain. It is deliberately NOT `THRULINE_HOME`
/// alone: that variable also moves the ledger, the exec database and the
/// install log, so unifying the spool through it would relocate four things to
/// fix one.
///
/// # Why a per-machine install pins the answer into the command string
///
/// A `thruline hook-install` on *this* machine reads this once and writes the
/// answer into `--spool`, so the settings file says on its face where the
/// record goes instead of deferring the question to whatever environment the
/// agent happens to have. That is not a theoretical preference: in the
/// BUILD-SPEC pilot the container ran with `THRULINE_HOME=/thruline/home` while
/// the exec shim was installed with `--spool /thruline/spool/codex.jsonl`, so
/// the hook wrote one file and the shim wrote another. Two files, two hash
/// chains, and **no chain containing both a `Stop` event and the execs it is
/// supposed to bound** — a phase boundary that cannot be evaluated at all,
/// from an install that reported success.
///
/// # Why the §4.1 managed snippet must NOT pin it
///
/// The pin is right exactly because the installing machine is the running
/// machine. A managed-settings snippet is generated on one machine and pushed
/// to a fleet, so a pinned `--spool` there names **the generating machine's**
/// home directory on every machine that runs it. Measured: a snippet generated
/// under `HOME=alice` and run under `HOME=bob` wrote bob's session line into
/// alice's spool and left bob's spool directory non-existent — with the hook
/// exiting 0 and printing nothing, as its contract requires. So the snippet
/// omits `--spool`, and [`run`] resolves it **here**, on the machine that
/// fires, out of that machine's `THRULINE_HOOK_SPOOL`/`THRULINE_HOME`/`HOME`.
pub fn resolve_spool(harness: &Harness) -> Option<PathBuf> {
    match std::env::var_os("THRULINE_HOOK_SPOOL") {
        Some(p) if !p.is_empty() => Some(PathBuf::from(p)),
        _ => default_spool(harness),
    }
}

/// The repository's `.thruline/redact.yml` redactor, or the built-in rules.
///
/// Shared by every harness: §4.6 runs before anything reaches disk, and the
/// rules do not depend on which agent produced the text.
pub fn repo_redactor() -> Redactor {
    std::env::var_os("CLAUDE_PROJECT_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .and_then(|root| Redactor::from_repo(&root).ok())
        .unwrap_or_default()
}

/// The whole hook, end to end. **Always returns 0 and prints nothing.**
pub fn run(
    harness: &Harness,
    stdin: &mut impl Read,
    spool: Option<&Path>,
    received_at: &str,
    redactor: &Redactor,
) -> u8 {
    let mut raw = Vec::new();
    let mut limited = stdin.take((crate::claude_code::MAX_STDIN + 1) as u64);
    let _ = limited.read_to_end(&mut raw);
    let line = crate::claude_code::prepare_for(harness, &raw, received_at, redactor).render();
    // `resolve_spool`, not `default_spool`: an unpinned hook — which is what
    // the §4.1 managed snippet installs, deliberately — must still honour the
    // running machine's `THRULINE_HOOK_SPOOL`, or a fleet machine that moved
    // its spool gets two files and two chains.
    if let Some(path) = spool
        .map(Path::to_path_buf)
        .or_else(|| resolve_spool(harness))
    {
        // Fail open: an unwritable spool must not break the agent.
        let _ = crate::claude_code::append_line(&path, &line);
    }
    0
}

#[cfg(test)]
mod tests;
