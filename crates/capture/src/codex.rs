//! BUILD-SPEC §4.2 — the Codex CLI adapter.
//!
//! # Two Rule 7 conflicts with §4.2, surfaced rather than blended
//!
//! **[CONFLICT-4] Hooks are not experimental.** §4.2 says *"Hooks are
//! experimental and change frequently"* and makes a sessions-directory watcher
//! the primary path. The current vendor documentation carries no experimental
//! marking and documents a full lifecycle hook system, and this machine already
//! has a populated `~/.codex/hooks.json`. **The ordering inverts: hooks are
//! primary, the rollout reader is the recovery path.**
//!
//! **[CONFLICT-5] The sessions directory is not `.codex/sessions/*.jsonl`.**
//! Verified on disk: it is user-global and date-nested,
//! `~/.codex/sessions/YYYY/MM/DD/rollout-<ISO8601>-<uuid>.jsonl`, and a rollout
//! is bound to a repository by `session_meta.payload.cwd` — there is no
//! per-repository sessions directory to watch. Built as §4.2 is written, the
//! adapter would find nothing, anywhere, and report a clean absence.
//!
//! **[CONFLICT-6] An interrupted Codex session records no terminal state.**
//! **Amended — the reason was wrong, and the check found it.** This comment
//! used to say Codex *fires an `Interrupt` hook event* that §3.1 has no kind
//! for, and that we declined to map it. `scripts/vendor-freshness.py` derives
//! the hook-event set from the installed binary's own `HookEventsToml` field
//! table, and on codex-cli 0.146.0 that set is **11 names with no `Interrupt`
//! among them**. There is no such hook event to decline. The hole is real and
//! larger than described: a user abort produces **no subscribable event at
//! all**, so it cannot be recorded even by choosing to. `TurnAbortReason` lives
//! in the protocol, not in the hook surface. Closing this needs a §3.1 kind
//! *and* a vendor event to hang it on; today only the rollout reader can see it.
//!
//! **[CONFLICT-7] The 3.5 s speaker watchdog cannot hold on a capped event.**
//! `SessionEnd` defaults to a **1 s** timeout and is capped at **3 s**
//! ([`CAPPED_TIMEOUT_MAX_SECS`]); everything else defaults to 600 s. ThruLine's
//! speaker watchdog gives itself 3.5 s, so on that event Codex would kill the
//! process **before** our own bound fired, and the self-timeout that exists to
//! make the failure ours would never run. No speaker is installed on a capped
//! event today — the gate rides `Stop`, which is uncapped — and
//! [`CAPPED_EVENTS`] exists so that a future one cannot be added without
//! tripping a test rather than a user. **This too named `Interrupt` as a second
//! capped event; the binary emits exactly one clamp message, `clamping
//! SessionEnd hook timeout to …`, and there is no second.**
//!
//! # What the rollout reader is *for*
//!
//! Two record types that §3.1 has first-class kinds for sit unmapped in the
//! legacy corpus's `meta` pile, measured at **56,273** and **2,791** rows:
//!
//! - `event_msg`/`patch_apply_end` is a **file mutation** — §3.1 `fs_change`.
//!   Its `changes` map names every path with `add` / `update` / `delete` and an
//!   optional `move_path`.
//! - `event_msg`/`context_compacted` is a **compaction boundary** — §3.1
//!   `compaction_pre` / `compaction_post`.
//!
//! Recovering them is most of what this reader is for.
//!
//! # Three things the reader must not invent, each measured
//!
//! 1. **`success` decides whether a patch happened.** A `patch_apply_end` with
//!    `success: false` still carries a full `changes` map — measured on this
//!    machine, with `stderr: "Failed to write file …"`. A reader that mapped
//!    `changes` unconditionally would record file mutations that did not occur,
//!    which is §0.2 exactly backwards: the record says it failed and the
//!    payload describes what it would have done.
//! 2. **Ordering comes from the file, never from a counter.** Every event's
//!    position is its line ordinal in the rollout, so a re-read of the same
//!    rollout derives the same chain. A line whose ordinal cannot be
//!    established is **refused**, not defaulted — the `seq = u64::MAX` lesson,
//!    applied before it can be committed rather than after.
//! 3. **`compacted` and `context_compacted` are different rows.** They share a
//!    timestamp to the millisecond and both mean "a compaction happened", and
//!    it is tempting to treat them as one record. They are not: `compacted`
//!    carries `message` and `replacement_history` (the *post* state) and
//!    `context_compacted` carries nothing but its own type (the *boundary*).
//!    Counting them as the same rows double-counts compactions, and an equal-
//!    cardinality coincidence in one corpus is not evidence that they are.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// The hook events this adapter subscribes to.
///
/// Verified against the vendor documentation (BUILD-PLAN B.2, retrieved
/// 2026-09-02). **No `PostToolUseFailure`**, and its absence is a finding
/// rather than an oversight: Codex's `PostToolUse` fires after a non-zero
/// `Bash` exit too, so the pair Claude Code needs does not exist here.
///
/// **[CONFLICT-6b — amendment] Firing on failure is not the property that
/// matters.** The original claim here was that `PostToolUse` *is* the failure
/// event because it fires on a non-zero exit. That is verified and true, and it
/// is the wrong test. The property a verifier needs is that a failure can be
/// **distinguished**, and it cannot:
///
/// - The payload is `turn_id`, `tool_name`, `tool_use_id`, `tool_input`,
///   `tool_response` plus the common fields — **no exit code, no status, no
///   success flag** — and the schema is `additionalProperties: false`, so the
///   absence is closed rather than merely undocumented. In `codex-rs` the exit
///   status lives in `response_header()` and is not carried into the event.
/// - So every Codex `tool_result` enters §3.1 **outcome-blind**.
///
/// This is **worse than the Claude Code case**, not equivalent. There, failure
/// was *missing* — one of a pair of events never arrived, which is detectable
/// as an absence and was in fact detected. Here failure is *present and
/// indistinguishable*, which nothing downstream can notice.
///
/// It is also why [`crate::fold::OK_NOT_STATED`] is not caution but the only
/// honest value, and why `fold::tool_outcome` must keep returning
/// `ToolOutcome::NotStated` for these rows. A future reader tempted to infer an
/// outcome from `tool_response` would be inventing the record.
///
/// **And the fire-on-failure guarantee is `Bash`-only.** Codex gates the event
/// on `success_for_logging()`, which is hardcoded `true` for shell, but
/// `result.success()` for MCP tools and `false` for aborted ones — so **a
/// failed MCP call and an aborted tool fire no `PostToolUse` at all**. For
/// those, the Claude Code failure mode returns on top of this one: the event
/// is simply absent.
pub const SUBSCRIBED_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PreCompact",
    "PostCompact",
    "SubagentStop",
    "Stop",
    "SessionEnd",
];

/// Map a Codex hook event name onto a §3.1 event kind.
pub fn event_kind_for(hook_event: &str) -> Option<&'static str> {
    Some(match hook_event {
        "SessionStart" => "session_start",
        "UserPromptSubmit" => "user_prompt",
        "PreToolUse" => "tool_call",
        // One event for both outcomes here, unlike Claude Code — and it
        // carries nothing that says WHICH outcome. See CONFLICT-6b.
        "PostToolUse" => "tool_result",
        "PreCompact" => "compaction_pre",
        "PostCompact" => "compaction_post",
        "SubagentStop" => "subagent_stop",
        "Stop" => "stop",
        "SessionEnd" => "session_end",
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Hook registration — §4.1 as Codex spells it
// ---------------------------------------------------------------------------

/// Codex reads its hook pack from a **dedicated file**, not from a settings
/// file it shares with everything else.
///
/// Read off the disk rather than the prose (§0.2): `~/.codex/hooks.json` exists
/// on this machine with a *foreign* pack already in it, shaped
/// `{"hooks": {Event: [{"matcher": …, "hooks": [{"type", "command", "timeout"}]}]}}`.
/// The nesting is Claude Code's; the file name, the `timeout` field and the
/// caps below are not. That foreign pack is also why the installer appends and
/// never replaces: someone else's recorder is not ours to delete.
pub const HOOKS_FILE: &str = "hooks.json";

/// `<home>/.codex/hooks.json`. `$CODEX_HOME`, when set, *is* the `.codex` dir.
pub fn hooks_path(home: &Path) -> PathBuf {
    home.join(".codex").join(HOOKS_FILE)
}

/// **[CONFLICT-8] Installing a Codex hook pack does not make it run.**
///
/// Verified against the vendor documentation, which is unambiguous:
///
/// > *"Before a non-managed hook can run, Codex requires you to review and
/// > trust the exact hook definition. Codex records trust against the hook's
/// > current hash, so new or changed hooks are marked for review and skipped
/// > until trusted."*
///
/// A pack this installer has just written is, by construction, **new** — so
/// every hook in it is marked for review and **skipped**. `hook-install` can
/// report "Installed 9 hooks" truthfully and capture nothing, and the symptom
/// is identical to the tmux `PATH` overwrite: hooks present, nothing fires,
/// spool empty. Two independent causes, one symptom; fixing either alone still
/// yields zero.
///
/// The trust store is real and was located: `[hooks.state]` in
/// `$CODEX_HOME/config.toml`, carrying `trust_level` and `trusted_hash`. This
/// crate does not write it — computing the vendor's hash would mean inventing a
/// format from the outside, and a wrong hash is a silent failure rather than a
/// loud one.
///
/// So the escapes available to a **user** are two: [`TRUST_REVIEW_COMMAND`]
/// interactively, or [`TRUST_BYPASS_FLAG`] per invocation for automation that
/// has vetted the source itself. A **third** exists and is not the user's:
/// trust is scoped to *non-managed* hooks, so an admin-managed configuration is
/// exempt. An earlier version of this comment said "the only ones", which was
/// an overstatement.
///
/// **The installer's duty is to say all of this**, because an operator who is
/// not told discovers it as a coverage finding.
///
/// # Verified empirically, not only from the documentation
///
/// `scripts/hook-event-rig.sh` drives a real `codex exec` against a scripted
/// local endpoint, at zero API cost, and the control is unambiguous:
///
/// | run | spool |
/// |---|---|
/// | without [`TRUST_BYPASS_FLAG`] | **no spool file at all** (2 model requests still served) |
/// | with it | `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Stop`, `SessionEnd` |
pub const TRUST_REVIEW_COMMAND: &str = "/hooks";

/// The per-invocation escape, for automation that has vetted the hooks itself.
///
/// Named `dangerously` by the vendor, and the name is earned: it runs every
/// enabled hook without the persisted trust check. Correct for a sealed
/// benchmark container whose hook pack this repository wrote; wrong as a habit
/// on a developer's machine.
pub const TRUST_BYPASS_FLAG: &str = "--dangerously-bypass-hook-trust";

/// Codex's full event list (11), against the 9 in [`SUBSCRIBED_EVENTS`].
///
/// Declared so the gap is a stated number rather than a silence. Both are
/// omitted deliberately:
///
/// - `PermissionRequest` is an approval prompt, not an action. It records what
///   the agent *asked to do*, which §3.1 has no kind for and which would
///   double-count against the `PreToolUse` that follows an approval.
/// - `SubagentStart` has no §3.1 kind and no counterpart in the pair we already
///   capture: we take `SubagentStop` because it carries the outcome.
///
/// **This list said 12 and named `Interrupt` first, transcribed from prose.**
/// It is 11, and `Interrupt` is not among them — see CONFLICT-6.
/// `scripts/vendor-freshness.py` re-derives this set from the installed
/// binary's own field table, so the next divergence fails instead of sitting.
///
/// The union of this and [`SUBSCRIBED_EVENTS`] is a **closed** claim about the
/// vendor: adding an event to one without removing it from the other, or
/// missing one Codex has added, is what that check exists to catch.
pub const UNSUBSCRIBED_EVENTS: &[&str] = &["PermissionRequest", "SubagentStart"];

/// **The event Codex kills fastest** (CONFLICT-7).
///
/// Every other hook event defaults to 600 s. This one defaults to **1 s** and
/// is capped at [`CAPPED_TIMEOUT_MAX_SECS`] — the tightest budget of the three
/// harnesses, and the one place a hook that works everywhere else is killed
/// with nothing said. A pack that leaves the timeout unset here inherits 1 s,
/// which a cold binary start on a loaded machine does not reliably fit inside.
///
/// **This list also carried `Interrupt`.** The installed binary emits exactly
/// one clamp message — `clamping SessionEnd hook timeout to …` — and
/// `scripts/vendor-freshness.py` derives the set from those messages, so a
/// second capped event would appear here by measurement rather than by memory.
pub const CAPPED_EVENTS: &[&str] = &["SessionEnd"];

/// The largest timeout Codex honours on a [`CAPPED_EVENTS`] entry.
pub const CAPPED_TIMEOUT_MAX_SECS: u64 = 3;

/// What the capture hook asks for on an uncapped event.
///
/// §4.1's budget is 50 ms p95; this is headroom for a cold start, not an
/// expectation. It is deliberately nowhere near the 600 s default: a capture
/// hook permitted to hang for ten minutes is one that will eventually hang an
/// agent for ten minutes.
pub const CAPTURE_TIMEOUT_SECS: u64 = 5;

/// The timeout to register for one event — capped where Codex caps it.
pub fn timeout_secs_for(event: &str) -> u64 {
    if CAPPED_EVENTS.contains(&event) {
        CAPPED_TIMEOUT_MAX_SECS
    } else {
        CAPTURE_TIMEOUT_SECS
    }
}

/// The user-global, date-nested rollout directory (CONFLICT-5).
pub fn sessions_root(home: &Path) -> PathBuf {
    home.join(".codex").join("sessions")
}

/// Every rollout file under `root`, in path order.
///
/// Path order is date order, because the layout is `YYYY/MM/DD/` and the file
/// name begins with an ISO-8601 timestamp.
pub fn rollouts(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(root, &mut out);
    out.sort();
    out
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
        {
            out.push(path);
        }
    }
}

/// One recovered record, with the position the *file* gave it.
#[derive(Debug, Clone, PartialEq)]
pub struct Recovered {
    /// The line's ordinal in the rollout, 0-based. Not a counter this reader
    /// keeps: a re-read derives the same number for the same line.
    pub ordinal: u64,
    pub ts: String,
    pub kind: RecoveredKind,
    /// sha256 of the raw line, so a recovered event names the bytes it came
    /// from and a later reader can check the claim.
    pub raw_sha256: String,
}

/// What a rollout line turned out to be.
#[derive(Debug, Clone, PartialEq)]
pub enum RecoveredKind {
    SessionMeta {
        session_id: String,
        cwd: String,
    },
    /// One `patch_apply_end` that **succeeded**, exploded into its paths.
    FsChanges {
        call_id: String,
        changes: Vec<PathChange>,
    },
    /// A `patch_apply_end` that failed. Recorded, and it is not an `fs_change`:
    /// the record says the write did not happen.
    PatchFailed {
        call_id: String,
        stderr: String,
        paths: Vec<String>,
    },
    /// `event_msg`/`context_compacted`: the boundary, and nothing else.
    CompactionBoundary,
    /// The `compacted` envelope: the post-compaction state.
    CompactionPost {
        summary: String,
        replacement_hash: String,
    },
    AgentMessage {
        text: String,
        phase: Option<String>,
    },
    UserMessage {
        text: String,
    },
    /// An execution reported by the harness, never collector-owned Exec evidence.
    CommandReport {
        id: String,
        argv: Vec<String>,
        cwd: String,
        status: String,
        exit_code: Option<i64>,
        output: String,
    },
    /// A line this reader recognises the shape of but has no §3.1 kind for.
    /// Counted and named, never dropped — the invariant that produced the
    /// legacy corpus's 1.4 M preserved `meta` rows, and it was right to.
    Unmapped {
        family: String,
    },
}

/// One path a patch touched.
#[derive(Debug, Clone, PartialEq)]
pub struct PathChange {
    pub path: String,
    /// `create` | `modify` | `delete` | `rename`, in §3.1's vocabulary.
    pub op: &'static str,
    pub move_path: Option<String>,
}

/// Why a line could not be recovered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The line is not JSON, or not an object.
    Unparseable,
    /// The envelope has no `type`.
    NoEnvelopeType,
    /// **The line carries no timestamp.** Refused rather than stamped with
    /// "now": a recovered event's time is a recorded fact or it is absent, and
    /// an invented one would be indistinguishable from a real one afterwards.
    NoTimestamp,
}

/// Read one rollout line.
///
/// `ordinal` is supplied by the caller **from the file**, which is the only
/// place it can honestly come from.
pub fn recover(ordinal: u64, raw: &str) -> Result<Recovered, Refusal> {
    let value: Value = serde_json::from_str(raw).map_err(|_| Refusal::Unparseable)?;
    let object = value.as_object().ok_or(Refusal::Unparseable)?;
    let envelope = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or(Refusal::NoEnvelopeType)?;
    let ts = object
        .get("timestamp")
        .and_then(Value::as_str)
        .ok_or(Refusal::NoTimestamp)?
        .to_string();
    let payload = object.get("payload").cloned().unwrap_or(Value::Null);
    let raw_sha256 = thruline_core::chain::sha256_hex(raw.as_bytes());

    let kind = match envelope {
        "session_meta" => RecoveredKind::SessionMeta {
            session_id: string_at(&payload, "id"),
            cwd: string_at(&payload, "cwd"),
        },
        "compacted" => RecoveredKind::CompactionPost {
            summary: string_at(&payload, "message"),
            replacement_hash: thruline_core::chain::sha256_hex(
                payload
                    .get("replacement_history")
                    .map(std::string::ToString::to_string)
                    .unwrap_or_default()
                    .as_bytes(),
            ),
        },
        "event_msg" => event_msg(&payload),
        other => RecoveredKind::Unmapped {
            family: other.to_string(),
        },
    };
    Ok(Recovered {
        ordinal,
        ts,
        kind,
        raw_sha256,
    })
}

fn event_msg(payload: &Value) -> RecoveredKind {
    let event_type = payload.get("type").and_then(Value::as_str).unwrap_or("");
    match event_type {
        "item_completed" => completed_item(payload),
        "patch_apply_end" => {
            let call_id = string_at(payload, "call_id");
            let paths: Vec<PathChange> = payload
                .get("changes")
                .and_then(Value::as_object)
                .map(|m| {
                    m.iter()
                        .map(|(path, change)| PathChange {
                            path: path.clone(),
                            op: op_of(change),
                            move_path: change
                                .get("move_path")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                        })
                        .collect()
                })
                .unwrap_or_default();
            // The record decides, not the payload's ambition.
            if payload.get("success").and_then(Value::as_bool) == Some(true) {
                RecoveredKind::FsChanges {
                    call_id,
                    changes: paths,
                }
            } else {
                RecoveredKind::PatchFailed {
                    call_id,
                    stderr: string_at(payload, "stderr"),
                    paths: paths.into_iter().map(|c| c.path).collect(),
                }
            }
        }
        "context_compacted" => RecoveredKind::CompactionBoundary,
        "agent_message" => RecoveredKind::AgentMessage {
            text: message_text(payload),
            phase: payload.get("phase").map(|phase| {
                phase
                    .as_str()
                    .unwrap_or("unknown_nonstring_phase")
                    .to_owned()
            }),
        },
        "user_message" => RecoveredKind::UserMessage {
            text: message_text(payload),
        },
        other => RecoveredKind::Unmapped {
            family: format!("event_msg/{other}"),
        },
    }
}

// Codex CLI 0.153.4 records canonical completed items. Response-item mirrors
// stay unmapped so one message cannot manufacture duplicate completion claims.
fn completed_item(payload: &Value) -> RecoveredKind {
    let item = &payload["item"];
    let kind = item["type"].as_str().unwrap_or("missing_type");
    let unknown = || RecoveredKind::Unmapped {
        family: format!("event_msg/item_completed/{kind}"),
    };
    match kind {
        "UserMessage" | "AgentMessage" => {
            let Some(content) = item["content"].as_array() else {
                return unknown();
            };
            if content.is_empty()
                || content.iter().any(|c| {
                    !matches!(c["type"].as_str(), Some("text" | "Text")) || !c["text"].is_string()
                })
            {
                return unknown();
            }
            let text = content
                .iter()
                .filter_map(|c| c["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            if kind == "UserMessage" {
                RecoveredKind::UserMessage { text }
            } else {
                RecoveredKind::AgentMessage {
                    text,
                    phase: Some(item["phase"].as_str().unwrap_or("unknown_phase").to_owned()),
                }
            }
        }
        "CommandExecution" => {
            let Some(command) = item["command"].as_array() else {
                return unknown();
            };
            if command.is_empty() || command.iter().any(|a| !a.is_string()) {
                return unknown();
            }
            RecoveredKind::CommandReport {
                id: string_at(item, "id"),
                argv: command
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
                cwd: string_at(item, "cwd"),
                status: string_at(item, "status"),
                exit_code: item["exit_code"].as_i64(),
                output: string_at(item, "aggregated_output"),
            }
        }
        "FileChange" => {
            let Some(changes) = item["changes"].as_object() else {
                return unknown();
            };
            if changes
                .values()
                .any(|c| !matches!(c["type"].as_str(), Some("add" | "update" | "delete")))
            {
                return unknown();
            }
            let id = string_at(item, "id");
            if item["status"] != "completed" {
                return RecoveredKind::PatchFailed {
                    call_id: id,
                    stderr: string_at(item, "stderr"),
                    paths: changes.keys().cloned().collect(),
                };
            }
            RecoveredKind::FsChanges {
                call_id: id,
                changes: changes
                    .iter()
                    .map(|(path, c)| PathChange {
                        path: path.clone(),
                        op: op_of(c),
                        move_path: c["move_path"].as_str().map(str::to_owned),
                    })
                    .collect(),
            }
        }
        _ => unknown(),
    }
}

/// §3.1's `fs_change.op`, from Codex's own vocabulary.
fn op_of(change: &Value) -> &'static str {
    if change
        .get("move_path")
        .is_some_and(|v| !v.is_null() && v.as_str().is_some_and(|s| !s.is_empty()))
    {
        return "rename";
    }
    match change.get("type").and_then(Value::as_str) {
        Some("add") => "create",
        Some("delete") => "delete",
        // Anything else that reached here is a content change. `update` is the
        // only other value observed; an unrecognised one is a modification
        // rather than a silent drop.
        _ => "modify",
    }
}

fn message_text(payload: &Value) -> String {
    payload
        .get("message")
        .or_else(|| payload.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn string_at(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests;
