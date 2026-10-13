//! Folding a Codex rollout file into the §11 execution store — BUILD-SPEC
//! §4.2's secondary path, and the reason `crates/capture/src/codex.rs` exists.
//!
//! # What this recovers, and why it is worth a module
//!
//! Two record types that §3.1 has first-class kinds for sit unmapped in the
//! legacy corpus's `meta` pile, measured at **56,273** and **2,791** rows:
//! `event_msg`/`patch_apply_end` is a file mutation, and
//! `event_msg`/`context_compacted` is a compaction boundary. Until now
//! [`crate::codex::recover`] could name them and nothing folded them, which is
//! a reader with no consumer — the gap this closes.
//!
//! # Every imported session is `capture_degraded`, without exception
//!
//! A rollout is **the harness's own log**. §0.2 is explicit that the transcript
//! is a hint and the execution record is the truth, and nothing here is
//! execution evidence: `patch_apply_end` is Codex reporting what it wrote, not
//! the shim observing a process. §13's rule for degraded capture is that claims
//! reach `unverifiable` and never `supported`, and that is the ceiling for
//! every session this module creates.
//!
//! Stated as a consequence rather than a caveat: **importing a rollout cannot
//! make a claim supportable.** What it buys is Explain's history — a timeline,
//! a compaction boundary, a set of files that changed — for sessions the shim
//! never saw.
//!
//! # Ordering is the file's, and a line that cannot be placed is refused
//!
//! `seq` is the rollout line's ordinal times [`crate::fold::SPOOL_SEQ_STRIDE`],
//! exactly as the spool fold numbers its own lines: a pure function of the
//! evidence, so re-importing the same rollout derives the same chain and the
//! second import writes nothing. A line with no timestamp is **refused**, not
//! stamped with "now" — an invented time is indistinguishable from a recorded
//! one afterwards, which is the whole shape of the `seq = u64::MAX` defect this
//! build spent a round removing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use thruline_core::spec::event::{
    CompactionPost, CompactionPre, Event, EventId, EventKind, FsChange, FsOp, OpenPayload, Payload,
    Source, Stop, StopReason,
};
use thruline_ledger::{EventRow, Ledger, LedgerError, RepoId};

use crate::codex::{recover, PathChange, Recovered, RecoveredKind, Refusal};
use crate::fold::SPOOL_SEQ_STRIDE;

/// Why every session this module writes is degraded (§13).
pub const DEGRADED_REASON: &str =
    "imported from a Codex rollout file: the harness's own log, not execution \
     evidence (§0.2), so no claim in this session can be supported";

/// The harness name stamped on an imported session.
pub const HARNESS: &str = "codex";

/// What one import did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub rollouts_seen: u64,
    /// Rollouts skipped because their `session_meta.payload.cwd` named another
    /// repository. **Counted**, because "no sessions found" and "none of them
    /// were yours" are different answers.
    pub rollouts_for_other_repos: u64,
    pub lines_read: u64,
    pub events_written: u64,
    /// Already present from an earlier import. Idempotence, counted.
    pub events_skipped: u64,
    pub sessions: u64,
    pub fs_changes: u64,
    pub compaction_boundaries: u64,
    /// Patches the record says did **not** apply. Recorded as notes, never as
    /// `fs_change` — see [`crate::codex`].
    pub patches_failed: u64,
    /// Lines this build has no §3.1 kind for, by family, with counts. Never
    /// dropped silently: this is the invariant that preserved 1.4 M legacy
    /// rows, and it was right to.
    pub unmapped: BTreeMap<String, u64>,
    /// Lines that could not be placed at all, with the reason.
    pub refused: Vec<(u64, String)>,
}

/// Import every rollout under `sessions_root` whose `cwd` is inside `repo`.
///
/// `limit` bounds the number of rollout **files**, so a first run on a machine
/// with years of history is a decision rather than a surprise.
pub fn import(
    sessions_root: &Path,
    ledger: &Ledger,
    repo_id: &RepoId,
    repo: &Path,
    limit: Option<usize>,
) -> Result<ImportReport, LedgerError> {
    let mut report = ImportReport::default();
    let repo_key = canonical(repo);
    for rollout in crate::codex::rollouts(sessions_root) {
        if limit.is_some_and(|n| report.sessions as usize >= n) {
            break;
        }
        report.rollouts_seen += 1;
        match import_one(&rollout, ledger, repo_id, &repo_key, &mut report) {
            Ok(true) => report.sessions += 1,
            Ok(false) => report.rollouts_for_other_repos += 1,
            Err(e) => return Err(e),
        }
    }
    Ok(report)
}

/// Bounded automatic observation of a single growing log. Only complete lines
/// are admitted. The caller retains this exact read identity; imported reports
/// remain degraded and cannot independently prove execution.
pub fn import_live_file(
    path: &Path,
    ledger: &Ledger,
    repo_id: &RepoId,
    repo: &Path,
) -> Result<(ImportReport, Value), String> {
    use std::io::Read;
    let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("Observation requires a regular, non-symlink log".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(
            "Log exceeds the 16 MiB automatic observation limit; history remains preserved".into(),
        );
    }
    let complete = bytes
        .iter()
        .rposition(|b| *b == b'\n')
        .map(|p| p + 1)
        .unwrap_or(0);
    let trailing = bytes.len() - complete;
    // This cuts only the unparsed JSONL input at a whole-record boundary,
    // never a decoded field headed for display. Complete records still pass
    // bounded() below. The partial record remains in the source for a later
    // read; live_import_waits_for_complete_records_and_masks_before_storage
    // exercises a credential split across successive observations.
    bytes.truncate(complete);
    let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
    let source = serde_json::json!({"path":path,"complete_bytes":complete,"sha256":thruline_core::chain::sha256_hex(&bytes),"trailing_bytes_waiting":trailing});
    let mut report = ImportReport {
        rollouts_seen: 1,
        ..Default::default()
    };
    if complete > 0 {
        if import_text(path, text, ledger, repo_id, &canonical(repo), &mut report)
            .map_err(|e| e.to_string())?
        {
            report.sessions = 1;
        } else {
            report.rollouts_for_other_repos = 1;
        }
    }
    Ok((report, source))
}

/// `Ok(false)` when the rollout belongs to another repository.
fn import_one(
    path: &PathBuf,
    ledger: &Ledger,
    repo_id: &RepoId,
    repo_key: &str,
    report: &mut ImportReport,
) -> Result<bool, LedgerError> {
    let Ok(text) = std::fs::read_to_string(path) else {
        // Unreadable is not fatal for a whole import, and it is not silent
        // either: it is counted as a refusal against the file's own ordinal 0.
        report
            .refused
            .push((0, format!("{}: unreadable", path.display())));
        return Ok(false);
    };

    import_text(path, &text, ledger, repo_id, repo_key, report)
}

fn import_text(
    path: &Path,
    text: &str,
    ledger: &Ledger,
    repo_id: &RepoId,
    repo_key: &str,
    report: &mut ImportReport,
) -> Result<bool, LedgerError> {
    // Pass one: place the session. A rollout with no `session_meta` names no
    // session and no repository, so there is nothing to attribute it to.
    let mut session_id = None;
    let mut cwd = None;
    for (ordinal, line) in text.lines().enumerate() {
        if let Ok(Recovered {
            kind:
                RecoveredKind::SessionMeta {
                    session_id: id,
                    cwd: dir,
                },
            ..
        }) = recover(ordinal as u64, line)
        {
            session_id = Some(id);
            cwd = Some(dir);
            break;
        }
    }
    let (Some(session), Some(cwd)) = (session_id, cwd) else {
        report.refused.push((
            0,
            format!(
                "{}: no session_meta, so it names no session",
                path.display()
            ),
        ));
        return Ok(false);
    };
    if !within(&cwd, repo_key) {
        return Ok(false);
    }

    // §13, before a single event lands: the ceiling is set first, so a crash
    // mid-import cannot leave rows in a session that is not yet marked.
    ledger.upsert_session(
        &session,
        repo_id,
        HARNESS,
        None,
        true,
        Some(DEGRADED_REASON),
    )?;

    // Resume from what is stored, exactly as the spool fold does: a re-import
    // is a no-op and an interrupted one continues rather than restarting.
    let (last_seq, mut prev) = match ledger.last_event(&session)? {
        Some((seq, hash)) => (Some(seq), hash),
        None => (
            None,
            thruline_core::spec::chain::genesis_prev_hash(&session),
        ),
    };
    let mut rows: Vec<EventRow> = Vec::new();

    for (ordinal, line) in text.lines().enumerate() {
        report.lines_read += 1;
        let ordinal = ordinal as u64;
        let seq = ordinal.saturating_mul(SPOOL_SEQ_STRIDE);
        if last_seq.is_some_and(|last| seq <= last) {
            report.events_skipped += 1;
            continue;
        }
        let recovered = match recover(ordinal, line) {
            Ok(r) => r,
            Err(why) => {
                report.refused.push((ordinal, refusal_text(&why)));
                continue;
            }
        };
        let Some(payload) = payload_for(&recovered, report) else {
            continue;
        };
        let id = EventId::derive(format!("codex-rollout|{session}|{seq}").as_bytes());
        let event = Event::new(
            &session,
            seq,
            &recovered.ts,
            Source::CodexSessionFile,
            payload,
            &prev,
        )
        .and_then(|e| e.with_id(id).seal());
        match event {
            Ok(sealed) => {
                prev = sealed
                    .hash()
                    .map(str::to_string)
                    .unwrap_or_else(|| prev.clone());
                rows.push(EventRow::bare(sealed));
            }
            Err(e) => report.refused.push((ordinal, format!("{e}"))),
        }
    }

    report.events_written += ledger.append_events(repo_id, &rows)?;
    Ok(true)
}

/// Map one recovered record onto a §3.1 payload, or `None` to skip it.
///
/// `None` is only ever returned for a record this build has no kind for, and
/// every one of those is counted in `report.unmapped` first.
fn payload_for(recovered: &Recovered, report: &mut ImportReport) -> Option<Payload> {
    match &recovered.kind {
        RecoveredKind::SessionMeta { cwd, .. } => {
            let mut open: OpenPayload = BTreeMap::new();
            open.insert("cwd".into(), Value::String(cwd.clone()));
            open.insert("source".into(), Value::String("codex-rollout".into()));
            open.insert(
                "raw_sha256".into(),
                Value::String(recovered.raw_sha256.clone()),
            );
            Some(Payload::SessionStart(open))
        }
        RecoveredKind::FsChanges { call_id, changes } => {
            // One event carries the first path; the rest ride in `extra`. A
            // rollout line is ONE ordinal, and giving each path its own `seq`
            // would invent positions the file does not have — the same
            // invention this module refuses everywhere else.
            let first: &PathChange = changes.first()?;
            report.fs_changes += changes.len() as u64;
            let mut extra: BTreeMap<String, Value> = BTreeMap::new();
            extra.insert("call_id".into(), Value::String(call_id.clone()));
            extra.insert(
                "observed_by".into(),
                Value::String("codex-session-file".into()),
            );
            if changes.len() > 1 {
                extra.insert(
                    "also_changed".into(),
                    Value::Array(
                        changes
                            .iter()
                            .skip(1)
                            .map(|c| {
                                serde_json::json!({ "path": c.path, "op": c.op,
                                                    "move_path": c.move_path })
                            })
                            .collect(),
                    ),
                );
            }
            if let Some(to) = &first.move_path {
                extra.insert("move_path".into(), Value::String(to.clone()));
            }
            Some(Payload::FsChange(FsChange {
                path: first.path.clone(),
                op: op_of(first.op),
                // Not computed, and not invented: the rollout records the patch
                // after it applied, so the prior bytes are already gone.
                before_hash: None,
                after_hash: None,
                extra,
            }))
        }
        RecoveredKind::PatchFailed {
            call_id,
            stderr,
            paths,
        } => {
            report.patches_failed += 1;
            let mut open: OpenPayload = BTreeMap::new();
            open.insert("note".into(), Value::String("patch_apply_failed".into()));
            open.insert("call_id".into(), Value::String(call_id.clone()));
            open.insert("stderr".into(), Value::String(bounded(stderr)));
            open.insert("paths".into(), serde_json::json!(paths));
            // Deliberately a `note` and not an `fs_change`: the record says the
            // write did not happen, and the payload describing what it would
            // have done is not evidence that it did.
            Some(Payload::Note(open))
        }
        RecoveredKind::CompactionBoundary => {
            report.compaction_boundaries += 1;
            Some(Payload::CompactionPre(CompactionPre {
                // Empty because the record is empty. Codex's
                // `context_compacted` carries nothing but its own type — a test
                // in `codex::tests` pins that — so the open promises, open
                // tasks and recent claims §3.1 names are genuinely absent here
                // rather than merely uncollected.
                open_promises: Vec::new(),
                open_tasks: Vec::new(),
                recent_claims: Vec::new(),
                transcript_hash: format!("sha256:{}", recovered.raw_sha256),
                extra: {
                    let mut e: BTreeMap<String, Value> = BTreeMap::new();
                    e.insert(
                        "pre_state".into(),
                        Value::String(
                            "absent: Codex's context_compacted record carries only its type".into(),
                        ),
                    );
                    e
                },
            }))
        }
        RecoveredKind::CompactionPost {
            summary,
            replacement_hash,
        } => {
            let mut extra: BTreeMap<String, Value> = BTreeMap::new();
            extra.insert("summary".into(), Value::String(bounded(summary)));
            Some(Payload::CompactionPost(CompactionPost {
                transcript_hash: format!("sha256:{replacement_hash}"),
                extra,
            }))
        }
        RecoveredKind::AgentMessage { text, phase } => {
            // Current rollouts distinguish commentary from final answers.
            // A progress message (or an unknown phase) is not a completed turn.
            // Preserve the message without giving it a stop interpretation.
            if let Some(phase) = phase.as_deref().filter(|phase| *phase != "final_answer") {
                let mut note = BTreeMap::new();
                note.insert("note".into(), Value::String("agent_message".into()));
                note.insert("phase".into(), Value::String(phase.into()));
                note.insert("message".into(), Value::String(bounded(text)));
                return Some(Payload::Note(note));
            }
            // Retain the historical interpretation for old phase-less rollouts.
            // All rollout evidence remains degraded; none proves execution.
            Some(Payload::Stop(Stop {
                reason: StopReason::AgentFinished,
                last_assistant_message_hash: format!(
                    "sha256:{}",
                    thruline_core::chain::sha256_hex(text.as_bytes())
                ),
                last_assistant_message: bounded(text),
                stop_hook_active: None,
                extra: BTreeMap::new(),
            }))
        }
        RecoveredKind::UserMessage { text } => {
            let mut open: OpenPayload = BTreeMap::new();
            open.insert("prompt".into(), Value::String(bounded(text)));
            Some(Payload::UserPrompt(open))
        }
        RecoveredKind::CommandReport {
            id,
            argv,
            cwd,
            status,
            exit_code,
            output,
        } => {
            let mut note = BTreeMap::new();
            note.insert(
                "note".into(),
                Value::String("imported_command_report".into()),
            );
            note.insert(
                "observed_by".into(),
                Value::String("codex-session-file; not independently captured execution".into()),
            );
            note.insert("call_id".into(), Value::String(bounded(id)));
            note.insert(
                "command".into(),
                Value::String(bounded(&serde_json::to_string(argv).unwrap_or_default())),
            );
            note.insert("cwd".into(), Value::String(bounded(cwd)));
            note.insert("reported_status".into(), Value::String(bounded(status)));
            note.insert("reported_exit_code".into(), serde_json::json!(exit_code));
            note.insert("message".into(), Value::String(bounded(output)));
            Some(Payload::Note(note))
        }
        RecoveredKind::Unmapped { family } => {
            *report.unmapped.entry(family.clone()).or_insert(0) += 1;
            None
        }
    }
}

fn op_of(op: &str) -> FsOp {
    match op {
        "create" => FsOp::Create,
        "delete" => FsOp::Delete,
        "rename" => FsOp::Rename,
        _ => FsOp::Modify,
    }
}

fn refusal_text(why: &Refusal) -> String {
    match why {
        Refusal::Unparseable => "not a JSON object".to_string(),
        Refusal::NoEnvelopeType => "no envelope type".to_string(),
        Refusal::NoTimestamp => {
            "no timestamp — refused rather than stamped with the import's own clock".to_string()
        }
    }
}

/// Masks, then cuts. §4.6's ordering, locally true so the truncation census can
/// see it — the same reason `fold::compact` re-masks text that arrived masked.
///
/// Unlike the spool, a rollout arrives **unredacted**: it is the vendor's file,
/// written without ThruLine involved. So this mask is the first one these bytes
/// have ever seen, and it is load-bearing rather than idempotent.
fn bounded(text: &str) -> String {
    let masked = thruline_core::redact::mask_recorded(text).0;
    thruline_core::redact::truncate_respecting_markers(&masked, 4_000)
}

fn canonical(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

/// Is `cwd` inside the repository?
///
/// Compared on canonical paths and on **path components**, not on a string
/// prefix: `/repo` is not the parent of `/repository`, and a `starts_with` over
/// strings says it is.
fn within(cwd: &str, repo_key: &str) -> bool {
    let cwd = canonical(Path::new(cwd));
    Path::new(&cwd).starts_with(Path::new(repo_key))
}

/// The default sessions root for this machine (CONFLICT-5: user-global, not
/// per-repository).
pub fn default_sessions_root() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| crate::codex::sessions_root(Path::new(&h)))
}

/// Every §3.1 kind this module can produce, for `doctor` and for the tests.
pub const PRODUCED_KINDS: &[EventKind] = &[
    EventKind::SessionStart,
    EventKind::UserPrompt,
    EventKind::FsChange,
    EventKind::CompactionPre,
    EventKind::CompactionPost,
    EventKind::Stop,
    EventKind::Note,
];

#[cfg(test)]
mod tests;
