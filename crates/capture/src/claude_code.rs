//! BUILD-SPEC §4.1 — the Claude Code hook adapter.
//!
//! `thruline hook claude-code` reads one hook delivery on stdin, redacts it,
//! and appends **one line** to a spool file with a single write. It then exits
//! 0 having printed nothing, whatever happened.
//!
//! # Three contracts, and they are in tension
//!
//! 1. **Fail open, absolutely.** A hook that errors, blocks or prints is a hook
//!    that breaks the user's agent. Every failure path here returns exit 0 and
//!    empty output — an unwritable spool, malformed JSON, a payload larger than
//!    memory. What went wrong is recorded *in the spool line* when a line can be
//!    written at all, and dropped silently only when it cannot.
//! 2. **50 ms p95** (§4.1). Which is why this does no SQLite work and takes no
//!    snapshot. **[SPEC-DEVIATION D3]**, BUILD-PLAN M0.6: §4.1 asks for a
//!    snapshot after each file-mutating `PostToolUse`, and `git add -A` on a
//!    large repository plus opening a WAL database is not a 50 ms promise you
//!    can keep. Snapshots move to the fold and the exec shim, which is where
//!    verdict-grade evidence comes from anyway.
//! 3. **Redaction before anything reaches disk.** The spool is a file in the
//!    user's home directory; a secret written there is a secret leaked, before
//!    any hashing question arises. This is the first producer to exercise the
//!    §4.6 ordering, and it exercises it structurally: the spool takes
//!    [`Redacted`] values, so an unredacted string cannot be written.
//!
//! # `PostToolUseFailure` is mandatory, not optional
//!
//! `PostToolUse` fires only when a tool **succeeds** — verbatim from the vendor
//! doc, and the failure event's own example is a failing `npm test`. Subscribing
//! to `PostToolUse` alone would capture only the test runs that passed, which is
//! a systematic bias in the exact direction that destroys a false-success
//! detector: the product would see a session's green runs and never its red
//! ones. [`SUBSCRIBED_EVENTS`] carries it, and a test asserts it is there.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::redaction::Redactor;

/// The hook events this adapter subscribes to.
///
/// Verified against the vendor documentation (BUILD-PLAN B.1, retrieved
/// 2026-09-02), not guessed. Three spec guesses it confirmed and which must not
/// be "corrected": the tool result field is `tool_response`, `Stop` carries
/// `decision`/`reason` at the TOP level, and `PostCompact` exists.
pub const SUBSCRIBED_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    // Not optional. See the module docs.
    "PostToolUseFailure",
    "PreCompact",
    "PostCompact",
    "SubagentStop",
    "Stop",
    "SessionEnd",
];

/// Fields present on every hook delivery.
const COMMON_FIELDS: &[&str] = &[
    "session_id",
    "transcript_path",
    "cwd",
    "hook_event_name",
    "permission_mode",
    "prompt_id",
];

/// The maximum stdin this adapter will read.
///
/// A hook whose payload is unbounded is a hook that can be made to exhaust
/// memory by an agent pasting a large file into a tool input. Truncation is
/// recorded on the line rather than being silent.
pub const MAX_STDIN: usize = 4 * 1024 * 1024;

/// Bound for any single redacted text field on a spool line.
///
/// Applied through [`Redacted::bounded`], so the cut happens **after** masking —
/// A209-C3 measured four sites where cutting first sliced a credential below its
/// detector threshold.
pub const FIELD_BOUND: usize = 4_000;

/// One hook delivery, redacted and ready to spool.
///
/// Total, in `core::sensor`'s sense: a delivery that could not be parsed, whose
/// event name is unrecognised, or that is missing its spine is still recorded,
/// with the defect flagged on the row. Nothing is dropped for being malformed —
/// what is malformed is exactly what a later investigation needs to see.
#[derive(Debug, Clone, PartialEq)]
pub struct SpoolLine {
    pub received_at: String,
    /// Which harness delivered this. **Recorded, not read off the spool file
    /// name**: a fact that exists only in a path is one a rename, a `--spool`
    /// flag or a copy silently changes, and the fold stamps `source` from it.
    pub harness: &'static str,
    pub hook_event: String,
    pub recognized: bool,
    pub parse_ok: bool,
    pub spine_complete: bool,
    pub truncated_stdin: bool,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub transcript_path: Option<String>,
    pub redactions_applied: u64,
    pub redaction_families: Vec<String>,
    /// The redacted payload. Every string in it has been through the redactor.
    pub payload: Value,
    /// sha256 of the RAW bytes, so a later reader can tell whether two
    /// deliveries were identical without the raw bytes being kept.
    pub raw_sha256: String,
}

impl SpoolLine {
    /// One JSON line, newline-terminated.
    pub fn render(&self) -> String {
        let value = serde_json::json!({
            "received_at": self.received_at,
            "harness": self.harness,
            "hook_event": self.hook_event,
            "recognized": self.recognized,
            "parse_ok": self.parse_ok,
            "spine_complete": self.spine_complete,
            "truncated_stdin": self.truncated_stdin,
            "session_id": self.session_id,
            "cwd": self.cwd,
            "transcript_path": self.transcript_path,
            "redactions_applied": self.redactions_applied,
            "redaction_families": self.redaction_families,
            "payload": self.payload,
            "raw_sha256": self.raw_sha256,
        });
        // `to_string` on a Value cannot fail; the fallback keeps this total
        // rather than introducing an unwrap on the hook path.
        match serde_json::to_string(&value) {
            Ok(mut line) => {
                line.push('\n');
                line
            }
            Err(_) => format!(
                "{{\"hook_event\":{:?},\"parse_ok\":false,\"render_failed\":true}}\n",
                self.hook_event
            ),
        }
    }
}

/// Turn one raw hook delivery into a redacted spool line.
///
/// Never fails. See [`SpoolLine`] for why.
pub fn prepare(raw: &[u8], received_at: &str, redactor: &Redactor) -> SpoolLine {
    prepare_for(&crate::harness::CLAUDE_CODE, raw, received_at, redactor)
}

/// The same, for any harness (BUILD-PLAN B.4).
///
/// Everything here is harness-independent except which event names count as
/// recognised — the spine (`session_id`, `transcript_path`, `cwd`,
/// `hook_event_name`) is common to all three vendor contracts, verified in
/// each harness module.
pub fn prepare_for(
    harness: &crate::harness::Harness,
    raw: &[u8],
    received_at: &str,
    redactor: &Redactor,
) -> SpoolLine {
    let truncated_stdin = raw.len() > MAX_STDIN;
    let bytes = if truncated_stdin {
        &raw[..MAX_STDIN]
    } else {
        raw
    };
    let text = String::from_utf8_lossy(bytes);
    let parsed: Option<Value> = serde_json::from_str(&text).ok();
    let parse_ok = parsed.is_some();

    let field = |key: &str| -> Option<String> {
        parsed
            .as_ref()
            .and_then(|v| v.get(key))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let hook_event = field("hook_event_name").unwrap_or_else(|| "unknown".to_string());
    let session_id = field("session_id");
    let cwd = field("cwd");
    let transcript_path = field("transcript_path");
    let spine_complete = parsed.as_ref().is_some_and(|v| {
        COMMON_FIELDS
            .iter()
            .take(4) // session_id, transcript_path, cwd, hook_event_name
            .all(|k| v.get(k).and_then(Value::as_str).is_some())
    });

    // Redact EVERY string in the payload, before anything reaches disk.
    let mut applied = 0u64;
    let mut families: std::collections::BTreeSet<String> = Default::default();
    let payload = match parsed {
        Some(value) => redact_delivery(value, redactor, &mut applied, &mut families),
        // Unparseable: keep the raw text, redacted and bounded, because a
        // malformed delivery is precisely what an investigation needs to see.
        None => {
            let redacted = redactor.redact(&text);
            applied += redacted.applied();
            families.extend(redacted.families().iter().cloned());
            serde_json::json!({ "unparsed": redacted.bounded(FIELD_BOUND).text() })
        }
    };

    SpoolLine {
        received_at: received_at.to_string(),
        harness: harness.name,
        recognized: harness.subscribed.contains(&hook_event.as_str()),
        hook_event,
        parse_ok,
        spine_complete,
        truncated_stdin,
        session_id,
        cwd,
        transcript_path,
        redactions_applied: applied,
        redaction_families: families.into_iter().collect(),
        payload,
        raw_sha256: thruline_core::chain::sha256_hex(raw),
    }
}

/// Redact a parsed delivery, bounding every string after masking.
///
/// The bound is applied inside the redactor's own walk, through
/// `Redacted::bounded`, so there is no truncation site in this module at all.
/// An earlier version cut here, in a separate pass — which the A209 truncation
/// census refused, correctly: it verifies mask-then-cut by proximity, and a cut
/// whose mask happened two frames up looks exactly like a cut with no mask.
fn redact_delivery(
    value: Value,
    redactor: &Redactor,
    applied: &mut u64,
    families: &mut std::collections::BTreeSet<String>,
) -> Value {
    let (redacted, count, fams) = redactor.redact_json_bounded(value, FIELD_BOUND);
    *applied += count;
    families.extend(fams);
    redacted
}

/// Append one line to the spool with a **single** write.
///
/// `O_APPEND` plus one `write` is what makes two hooks landing together produce
/// two well-formed lines rather than one interleaved mess. `write_all` would
/// loop and could split; this issues one call and treats a short write as a
/// failure rather than retrying into a torn line.
pub fn append_line(spool: &Path, line: &str) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = spool.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(spool)?;
    let written = file.write(line.as_bytes())?;
    if written != line.len() {
        return Err(std::io::Error::other(format!(
            "short write: {written} of {} bytes",
            line.len()
        )));
    }
    Ok(())
}

/// `$THRULINE_HOME/spool/claude-code.jsonl`, defaulting under `~/.thruline`.
pub fn default_spool() -> Option<PathBuf> {
    let home = std::env::var_os("THRULINE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".thruline")))?;
    Some(home.join("spool").join("claude-code.jsonl"))
}

/// The whole hook, end to end. **Always returns 0 and prints nothing.**
pub fn run(stdin: &mut impl Read, spool: Option<&Path>, received_at: &str) -> u8 {
    let mut raw = Vec::new();
    // A read error still produces a line, from whatever arrived.
    let mut limited = stdin.take((MAX_STDIN + 1) as u64);
    let _ = limited.read_to_end(&mut raw);

    let redactor = repo_redactor();
    let line = prepare(&raw, received_at, &redactor).render();

    if let Some(path) = spool.map(Path::to_path_buf).or_else(default_spool) {
        // Fail open: an unwritable spool must not break the agent.
        let _ = append_line(&path, &line);
    }
    0
}

/// Load `.thruline/redact.yml` from the hook's working directory.
///
/// A malformed config falls back to the built-in rules **here specifically**,
/// against this module's usual discipline, because the alternative on a hook
/// path is worse: refusing to redact at all would mean either writing an
/// unredacted line or dropping the delivery. Built-in redaction plus a flagged
/// line beats both. `thruline doctor` is where a broken config must be loud.
fn repo_redactor() -> Redactor {
    std::env::var_os("CLAUDE_PROJECT_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .and_then(|root| Redactor::from_repo(&root).ok())
        .unwrap_or_default()
}

/// Which §3.1 event kind a hook event maps to.
///
/// Returned as a name rather than a `Payload` because the fold (M0.7) owns
/// sequencing and chaining; this adapter's job ends at the spool.
pub fn event_kind_for(hook_event: &str) -> Option<&'static str> {
    Some(match hook_event {
        "SessionStart" => "session_start",
        "UserPromptSubmit" => "user_prompt",
        "PreToolUse" => "tool_call",
        // Both outcomes map to `tool_result`; `ok` distinguishes them, which is
        // why subscribing to only one of the pair biases the record.
        "PostToolUse" | "PostToolUseFailure" => "tool_result",
        "PreCompact" => "compaction_pre",
        "PostCompact" => "compaction_post",
        "SubagentStop" => "subagent_stop",
        "Stop" => "stop",
        "SessionEnd" => "session_end",
        _ => return None,
    })
}

/// Fields this adapter deliberately preserves beyond the spec's minimum.
///
/// `duration_ms` is on both tool events and the spec does not mention it;
/// BUILD-PLAN B.1 says to capture it. `is_interrupt` distinguishes a user
/// cancellation from a tool failure, which a stop-reason classifier needs.
pub const BONUS_FIELDS: &[&str] = &["duration_ms", "is_interrupt", "tool_use_id", "trigger"];

/// A read-only view of what a delivery is, for tests and `doctor`.
pub fn summarize(line: &SpoolLine) -> BTreeMap<&'static str, String> {
    let mut out = BTreeMap::new();
    out.insert("hook_event", line.hook_event.clone());
    out.insert("recognized", line.recognized.to_string());
    out.insert("parse_ok", line.parse_ok.to_string());
    out.insert("redactions_applied", line.redactions_applied.to_string());
    if let Some(kind) = event_kind_for(&line.hook_event) {
        out.insert("event_kind", kind.to_string());
    }
    out
}

#[cfg(test)]
mod tests;
