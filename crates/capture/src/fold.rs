//! BUILD-SPEC §3.1 / §4.4 — folding the hook spool into the ledger.
//!
//! The hook writes one line and exits (§4.1's 50 ms budget). Something has to
//! turn those lines into a hash-chained event chain with snapshots, and that
//! something is here. It is the **only writer that touches SQLite**, which is
//! what keeps the hook path free of it.
//!
//! # Three properties, each a test rather than an argument
//!
//! **Idempotent.** `seq` is the line's ordinal position in the spool file, not
//! a counter this process keeps. That makes it a pure function of the spool, so
//! folding the same spool twice derives the same seq for the same line, and
//! `UNIQUE(session_id, seq)` turns the second attempt into a no-op. A fold that
//! numbered events as it went would produce different chains on replay.
//!
//! **Crash-consistent.** There is no checkpoint file to get out of step with the
//! database. A fold interrupted anywhere leaves the spool untouched — it is
//! append-only and this never writes to it — and leaves the ledger holding
//! whatever batches committed. The next fold skips what is already stored (by
//! `seq`, per session), chains onto the stored head, and continues. Proven by
//! `an_interrupted_fold_replays_to_the_same_chain`.
//!
//! **Loud about what it cannot represent.** A line that cannot be sealed is
//! written to a quarantine file and counted, never dropped. This is the
//! invariant that produced 1,442,374 `meta` rows in the legacy corpus — every
//! one of them an unrecognised JSONL field, preserved, counted and reported
//! rather than discarded — and it was right to. `doctor` surfaces the count.
//!
//! # Why a refused event is a quarantine and not a panic
//!
//! `Ledger::append_events` verifies every hash before writing, because `events`
//! is append-only and a mis-sealed row can never be repaired. The fold is the
//! only caller, so that refusal is the gate every spooled event passes. When it
//! fires, the batch is rejected — so the fold isolates the offending line,
//! quarantines it, and retries the rest. One malformed delivery must not stop a
//! session's whole record from landing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use thruline_core::spec::event::{
    CompactionPost, CompactionPre, Event, EventError, EventKind, FsChange, FsOp, OpenPayload,
    Payload, SnapshotPayload, Source, Stop, StopReason, ToolCall, ToolResult,
};
use thruline_core::spec::snapshot::{snapshot_tree, TreeSnapshot};
use thruline_ledger::{CheckRow, EventRow, Ledger, LedgerError, RepoId};

/// The `source` stamped on events folded from a Claude Code hook.
///
/// Also the **default** for a spool line that names no harness: lines written
/// before the `harness` field existed were all Claude Code's, and reading them
/// as such is a fact about when they were written rather than a guess.
pub const HOOK_SOURCE: &str = "claude-code-hook";

/// How far apart consecutive spool lines are numbered.
///
/// `seq` is the line's ordinal position in the spool — that is what makes it a
/// pure function of the evidence and the fold idempotent. But the fold also
/// *derives* events that have no line of their own: a snapshot after a
/// file-mutating tool. Numbering those `line_seq + 1` collided with the next
/// line's own ordinal, and the collision was silent in the worst possible way:
/// the next delivery hit the `seq <= last_seq` resume guard and was counted as
/// **already present**, so a real hook delivery vanished and the report called
/// it idempotence.
///
/// A stride leaves room for derived events without giving up the property:
/// `seq / STRIDE` is still the spool ordinal, resume still works, and a replay
/// still reproduces the chain byte for byte.
pub const SPOOL_SEQ_STRIDE: u64 = 4;

/// Bytes of captured output the check builder will read to parse a runner's
/// result. Beyond it the check is recorded as `unverifiable` with the reason,
/// rather than the fold loading an unbounded file into memory.
pub const CHECK_PARSE_LIMIT: u64 = 8 * 1024 * 1024;

/// Tools whose use can change files on disk, and therefore warrant a snapshot.
///
/// A snapshot after a read-only tool would be pure cost: the tree cannot have
/// moved, and §4.4's engine writes objects into the user's repository.
pub const MUTATING_TOOLS: &[&str] = &[
    "Bash",
    "Edit",
    "Write",
    "NotebookEdit",
    "MultiEdit",
    "Update",
    "Patch",
];

/// What a fold did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FoldReport {
    pub lines_read: u64,
    pub events_written: u64,
    /// Already present from an earlier fold. Idempotence, counted.
    pub events_skipped: u64,
    pub snapshots_taken: u64,
    pub sessions: u64,
    /// Which sessions this fold saw, so folding two spools can detect a
    /// session that appears in both.
    pub sessions_seen: std::collections::BTreeSet<String>,
    /// Sessions found in more than one spool. **A defect, reported loudly.**
    ///
    /// `seq` is a line's ordinal in *its* spool and the fold resumes at the
    /// session's chain tail, so two spools numbering one session put two
    /// schemes in one seq space: whichever folds second hits the resume guard
    /// and its lines are counted as already present — a real delivery vanishes
    /// and the report calls it idempotence. That is the exact failure the
    /// stride was introduced to fix, arriving through a second file instead of
    /// a second writer. Detected rather than prevented, because preventing it
    /// means renumbering, and renumbering costs the property that makes the
    /// chain head a function of the evidence.
    pub sessions_in_multiple_spools: Vec<String>,
    /// Snapshots due at a mutating-tool boundary that could not be taken.
    ///
    /// Counted rather than skipped. The first version discarded the error with
    /// `.ok()?`, so a repository the engine could not snapshot produced a chain
    /// with no snapshot events and nothing saying why — found by the end-to-end
    /// run, where `snapshots_taken` was 0 and every other number looked right.
    /// A missing snapshot means claims cannot be bound to a tree (§6.1), which
    /// is the difference between `unverifiable` and `supported`.
    pub snapshots_failed: Vec<(u64, String)>,
    /// `fs_change` events that were due and could not be built.
    pub fs_changes_failed: Vec<(u64, String)>,
    /// Lines that could not become an event, with the reason. Every one is in
    /// the quarantine file.
    pub quarantined: Vec<(u64, String)>,
    /// The quarantine file itself could not be written. Counted, because
    /// otherwise `doctor` reports "N deliveries held at <path>" for a path that
    /// holds nothing — a report about evidence that is not where it says.
    pub quarantine_write_error: Option<String>,
    /// `exec` events folded (§4.4's shim).
    pub exec_events: u64,
    /// §3.3 `Check` records derived from those execs.
    pub checks_written: u64,
    /// Execs whose output could not be read back to parse a check, with the
    /// reason. A check that could not be built is not the same as a command
    /// that ran no tests.
    pub checks_failed: Vec<(String, String)>,
}

impl FoldReport {
    /// Fold another harness's spool into this report.
    ///
    /// Every harness has its own spool and one fold per file; the numbers are
    /// per-run totals, so they add. `sessions` is the exception in principle —
    /// one session could appear in two spools — and is summed anyway, because
    /// the alternative is carrying the session-id set through a report whose
    /// only reader prints counts. Named, so it is a known approximation rather
    /// than an unnoticed one.
    pub fn merge(&mut self, other: FoldReport) {
        for session in &other.sessions_seen {
            if self.sessions_seen.contains(session)
                && !self.sessions_in_multiple_spools.contains(session)
            {
                self.sessions_in_multiple_spools.push(session.clone());
            }
        }
        self.sessions_seen
            .extend(other.sessions_seen.iter().cloned());
        self.sessions_in_multiple_spools
            .extend(other.sessions_in_multiple_spools.iter().cloned());
        self.sessions_in_multiple_spools.sort();
        self.sessions_in_multiple_spools.dedup();
        self.lines_read += other.lines_read;
        self.events_written += other.events_written;
        self.events_skipped += other.events_skipped;
        self.snapshots_taken += other.snapshots_taken;
        self.sessions += other.sessions;
        self.exec_events += other.exec_events;
        self.checks_written += other.checks_written;
        self.snapshots_failed.extend(other.snapshots_failed);
        self.fs_changes_failed.extend(other.fs_changes_failed);
        self.quarantined.extend(other.quarantined);
        self.checks_failed.extend(other.checks_failed);
        self.quarantine_write_error = self
            .quarantine_write_error
            .take()
            .or(other.quarantine_write_error);
    }
}

/// Where a line that cannot be sealed goes.
///
/// A sibling of the spool, so anything that can read one can find the other.
pub fn quarantine_path(spool: &Path) -> PathBuf {
    let mut name = spool
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "spool".to_string());
    name.push_str(".quarantine");
    spool.with_file_name(name)
}

/// Fold `spool` into `ledger` under `repo`.
///
/// `repo_root` is snapshotted at mutating-tool boundaries; pass `None` to fold
/// without snapshots (a spool from another machine, or a test).
pub fn fold(
    spool: &Path,
    ledger: &Ledger,
    repo: &RepoId,
    repo_root: Option<&Path>,
) -> Result<FoldReport, LedgerError> {
    let mut report = FoldReport::default();
    let text = match std::fs::read_to_string(spool) {
        Ok(t) => t,
        // No spool is not an error: nothing has happened yet.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(report),
        Err(source) => {
            return Err(LedgerError::Io {
                path: spool.to_path_buf(),
                source,
            })
        }
    };

    // Group by session, preserving each line's GLOBAL ordinal as its `seq`.
    // Global rather than per-session so the value is a pure function of the
    // spool: a partial fold cannot shift the numbering of anything that follows.
    let mut by_session: BTreeMap<String, Vec<(u64, Value)>> = BTreeMap::new();
    let mut quarantine: Vec<(u64, String, String)> = Vec::new();

    for (index, raw) in text.lines().enumerate() {
        if raw.trim().is_empty() {
            continue;
        }
        report.lines_read += 1;
        // The ORDINAL, not the seq. Everything a human reads — the quarantine
        // file, the report — speaks in line numbers; only the chain speaks in
        // `seq`, and `seq = ordinal * SPOOL_SEQ_STRIDE`.
        let seq = (index as u64) + 1;
        match serde_json::from_str::<Value>(raw) {
            Ok(line) => match line.get("session_id").and_then(Value::as_str) {
                Some(session) => by_session
                    .entry(session.to_string())
                    .or_default()
                    .push((seq, line)),
                None => quarantine.push((
                    seq,
                    "spool line has no session_id, so it belongs to no chain".to_string(),
                    raw.to_string(),
                )),
            },
            Err(e) => {
                quarantine.push((seq, format!("spool line is not JSON: {e}"), raw.to_string()))
            }
        }
    }

    for (session, lines) in by_session {
        report.sessions += 1;
        report.sessions_seen.insert(session.clone());
        fold_session(
            ledger,
            repo,
            repo_root,
            &session,
            &lines,
            &mut report,
            &mut quarantine,
        )?;
    }

    if !quarantine.is_empty() {
        report.quarantine_write_error = write_quarantine(spool, &quarantine);
        for (seq, reason, _) in &quarantine {
            report.quarantined.push((*seq, reason.clone()));
        }
        report.quarantined.sort();
    }
    Ok(report)
}

#[allow(clippy::too_many_arguments)]
fn fold_session(
    ledger: &Ledger,
    repo: &RepoId,
    repo_root: Option<&Path>,
    session: &str,
    lines: &[(u64, Value)],
    report: &mut FoldReport,
    quarantine: &mut Vec<(u64, String, String)>,
) -> Result<(), LedgerError> {
    let harness = "claude-code";
    ledger.upsert_session(session, repo, harness, None, false, None)?;

    // Resume point: what is already stored wins, so a replay is a no-op and an
    // interrupted fold continues rather than restarting.
    let (mut last_seq, mut prev) = match ledger.last_event(session)? {
        Some((seq, hash)) => (seq, hash),
        None => (0, thruline_core::spec::chain::genesis_prev_hash(session)),
    };

    let mut rows: Vec<EventRow> = Vec::new();
    for (ordinal, line) in lines {
        let seq = &ordinal.saturating_mul(SPOOL_SEQ_STRIDE);
        if *seq <= last_seq {
            report.events_skipped += 1;
            continue;
        }
        let ts = line
            .get("received_at")
            .and_then(Value::as_str)
            .unwrap_or("1970-01-01T00:00:00Z");
        let hook_event = line
            .get("hook_event")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let payload_value = line.get("payload").cloned().unwrap_or(Value::Null);
        let harness = harness_of(line);
        // A line the exec recorder wrote carries a ready-made §3.1
        // `{kind, payload}` pair and its own source; a hook line carries a
        // vendor event name this module maps. Two producers, one chain.
        let source = source_of(line);
        let built = match line.get("spec_payload") {
            Some(tagged) => build_spec_payload(tagged),
            None => build_payload(&harness, hook_event, &payload_value),
        };

        match built {
            Ok(payload) => {
                // A DERIVED id, not a fresh ULID. With random ids, re-folding
                // the same spool produces a different chain head — measured:
                // same event count, same last_seq, different hash — which would
                // make the head not a function of the evidence. The head is what
                // a receipt publishes; a rebuild that cannot reproduce it cannot
                // check it.
                let id = thruline_core::spec::event::EventId::derive(
                    format!(
                        "{session}|{seq}|{}",
                        line.get("raw_sha256")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                    )
                    .as_bytes(),
                );
                match Event::new(session, *seq, ts, source, payload, prev.clone())
                    .map(|e| e.with_id(id))
                    .and_then(Event::seal)
                {
                    Ok(event) => {
                        prev = event.hash().unwrap_or_default().to_string();
                        last_seq = *seq;
                        let applied = line
                            .get("redactions_applied")
                            .and_then(Value::as_u64)
                            .unwrap_or(0);
                        rows.push(EventRow {
                            redactions_applied: applied,
                            ..EventRow::bare(event)
                        });

                        // §6.1 needs to know WHICH paths changed, and C.7/S1
                        // requires that set to come from worktree observation
                        // rather than from a tree diff: a `.gitattributes`
                        // clean filter makes the worktree change while the tree
                        // hash does not, so a diff-derived change set is blind
                        // to exactly the write it exists to detect. The hook's
                        // `tool_input.file_path` IS that observation.
                        if let Some((path, op)) =
                            observed_change(&harness, hook_event, &payload_value)
                        {
                            match fs_change_event(session, last_seq, &prev, &path, op) {
                                Ok((event, seq)) => {
                                    prev = event.hash().unwrap_or_default().to_string();
                                    last_seq = seq;
                                    rows.push(EventRow::bare(event));
                                }
                                Err(why) => report.fs_changes_failed.push((*ordinal, why)),
                            }
                        }

                        // Snapshot after a tool that can have moved files.
                        if let Some(root) = repo_root {
                            if is_mutating(&harness, hook_event, &payload_value) {
                                match snapshot_event(root, session, last_seq, &prev) {
                                    Ok((snap_event, snap_seq)) => {
                                        prev = snap_event.hash().unwrap_or_default().to_string();
                                        last_seq = snap_seq;
                                        rows.push(EventRow::bare(snap_event));
                                    }
                                    // Counted, never silent: a chain with no
                                    // snapshot cannot bind a claim to a tree
                                    // (§6.1), which is the difference between
                                    // `unverifiable` and `supported`.
                                    Err(why) => report.snapshots_failed.push((*ordinal, why)),
                                }
                            }
                        }
                    }
                    Err(e) => quarantine.push((
                        *ordinal,
                        format!("could not seal a {hook_event} event: {e}"),
                        line.to_string(),
                    )),
                }
            }
            Err(reason) => quarantine.push((*ordinal, reason, line.to_string())),
        }
    }

    if rows.is_empty() {
        return Ok(());
    }
    // `append_events` verifies every hash before writing. If the batch is
    // refused, isolate the offender rather than losing the whole session.
    match ledger.append_events(repo, &rows) {
        Ok(written) => {
            report.events_written += written;
            report.events_skipped += rows.len() as u64 - written;
            record_checks(ledger, session, &rows, report)?;
        }
        Err(LedgerError::CorruptEvent { id, source }) => {
            let (written, kept) = retry_without(ledger, repo, &rows, &id, quarantine, source)?;
            report.events_written += written;
            record_checks(ledger, session, &rows[..kept], report)?;
        }
        Err(other) => return Err(other),
    }
    Ok(())
}

/// Which harness delivered this line.
///
/// Read from the line, not from the spool file name — a `--spool` flag or a
/// copy changes the second and not the first. A line with no `harness` field
/// predates it, and every such line came from Claude Code.
fn harness_of(line: &Value) -> crate::harness::Harness {
    line.get("harness")
        .and_then(Value::as_str)
        .and_then(crate::harness::by_name)
        .unwrap_or(crate::harness::CLAUDE_CODE)
}

/// The `source` a spool line declares, defaulting to its harness's.
fn source_of(line: &Value) -> Source {
    let Some(name) = line.get("source").and_then(Value::as_str) else {
        return Source::Other(harness_of(line).source.into());
    };
    match serde_json::from_value::<Source>(Value::String(name.to_string())) {
        Ok(source) => source,
        // Unreachable while `Source::Other` is untagged, and an unknown
        // producer must round-trip rather than be rejected: its event's hash
        // was taken over the name it wrote.
        Err(_) => Source::Other(name.to_string()),
    }
}

/// A §3.1 `{kind, payload}` pair produced by the exec recorder.
///
/// Deserialised through the same `Payload` enum every other path uses, so a
/// producer cannot introduce a payload shape the event model has not agreed to.
fn build_spec_payload(tagged: &Value) -> Result<Payload, String> {
    serde_json::from_value::<Payload>(tagged.clone()).map_err(|e| {
        format!(
            "a spool line carried a spec_payload this build cannot read: {e}; \
             quarantined rather than reshaped"
        )
    })
}

/// Derive §3.3 `Check` records from the `exec` events just written.
///
/// **The other half of M0's done-when.** Without this the chain has execs and
/// nothing that says what they proved; with it, `tests_pass` has something to
/// be checked against in M1. No verdict is computed here — §6.3 is M1, and a
/// check whose output could not be parsed is stored as `unverifiable`, never as
/// an empty `parsed` block that would read downstream as a clean run.
fn record_checks(
    ledger: &Ledger,
    session: &str,
    rows: &[EventRow],
    report: &mut FoldReport,
) -> Result<(), LedgerError> {
    for row in rows {
        // Counted HERE, over the events actually written, rather than at the
        // point the fold derives one. Counting only derived snapshots made
        // `snapshots_taken` read 0 for a session whose chain held two of them,
        // which is the same misleading zero the `.ok()?` produced — a number
        // that is right about the wrong population.
        if row.event.kind() == &EventKind::Snapshot {
            report.snapshots_taken += 1;
        }
        if row.event.kind() != &EventKind::Exec {
            continue;
        }
        report.exec_events += 1;
        let payload = row.event.payload();
        // A snapshot the RECORDER could not take is reported by the fold too.
        // It is recorded on the exec event rather than in this process, and a
        // failure nobody surfaces is a failure nobody acts on.
        if let Some(failures) = payload.get("snapshots_failed").and_then(Value::as_array) {
            for failure in failures {
                report.snapshots_failed.push((
                    row.event.seq(),
                    format!(
                        "{} snapshot for exec {}: {}",
                        failure.get("which").and_then(Value::as_str).unwrap_or("?"),
                        row.event.id(),
                        failure.get("why").and_then(Value::as_str).unwrap_or("?")
                    ),
                ));
            }
        }
        let argv: Vec<String> = payload
            .get("argv")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let exit_code = payload
            .get("exit_code")
            .and_then(Value::as_i64)
            .unwrap_or(-1);
        let (output, read_error) = captured_output(payload);
        if let Some(why) = read_error {
            report
                .checks_failed
                .push((row.event.id().to_string(), why.clone()));
        }
        let outcome = thruline_verify::check::check_from_exec(&argv, exit_code, &output);
        let string_of = |key: &str| {
            payload
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        // A test check, or a §6.4 build/lint/typecheck one. An exec that is
        // neither is not a check and is left alone — `make`, `git`, `echo`.
        let (category, runner, exit, parsed) = match outcome.draft() {
            Some(draft) => (
                draft.category.to_string(),
                draft.runner.as_str().to_string(),
                draft.exit_code,
                parsed_json(&draft.outcome),
            ),
            None => match thruline_verify::check::tool_check_from_exec(&argv, exit_code) {
                Some(tool) => (
                    tool.category.to_string(),
                    tool.tool.clone(),
                    tool.exit_code,
                    // A build check's evidence is its EXIT CODE and nothing
                    // else. `{}` would invite a reader to find zeros in it and
                    // conclude something, so the shape says what it is. The
                    // bypass flag rides here because `checks` has no column for
                    // it and a DDL change costs a migration on every store.
                    serde_json::json!({
                        "exit_only": true,
                        "bypass": tool.bypass,
                    })
                    .to_string(),
                ),
                None => continue,
            },
        };
        let row = CheckRow {
            id: check_id(row.event.id().as_str()),
            session_id: session.to_string(),
            exec_event_id: row.event.id().to_string(),
            category,
            runner,
            argv_hash: string_of("argv_hash"),
            cwd_hash: format!(
                "sha256:{}",
                thruline_core::chain::sha256_hex(string_of("cwd").as_bytes())
            ),
            env_hash: string_of("env_hash"),
            // Empty when no snapshot could be taken. An empty string can never
            // equal a 40-hex tree hash, so §6.1's binding cannot mistake an
            // unbound check for a bound one.
            input_tree_hash: payload
                .get("input_tree_hash")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            exit_code: exit,
            parsed,
        };
        // Counted, not fatal. A `?` here would abort the fold AFTER the events
        // committed, and the next fold would skip those events as already
        // stored — so a transient insert failure would lose the check
        // permanently while every other number looked right. That is the same
        // shape as the swallowed snapshot, one layer along.
        match ledger.insert_check(&row) {
            Ok(n) => report.checks_written += n,
            Err(e) => report
                .checks_failed
                .push((row.exec_event_id.clone(), format!("check not stored: {e}"))),
        }
    }
    Ok(())
}

/// `chk_` + a ULID derived from the exec event's id, so a re-fold produces the
/// same check id and the insert is a no-op rather than a duplicate.
fn check_id(exec_event_id: &str) -> String {
    let derived = thruline_core::spec::event::EventId::derive(exec_event_id.as_bytes());
    format!("chk_{}", derived.as_str().trim_start_matches("evt_"))
}

/// The §3.3 `parsed` block, or an explicit statement that there is none.
///
/// `{}` is never written for an unparseable run: a defaulted parsed block reads
/// as `{total: 0, failed: 0}`, which is a clean run. This is the same rule the
/// parser enforces in its return type, carried into storage.
fn parsed_json(outcome: &thruline_verify::RunnerOutcome) -> String {
    let value = match outcome {
        thruline_verify::RunnerOutcome::Parsed(p) => serde_json::json!({
            "tests_total": p.tests_total,
            "passed": p.passed,
            "failed": p.failed,
            "skipped": p.skipped,
            "xfail": p.xfail,
            "test_ids": p.test_ids,
            "duration_ms": p.duration_ms,
        }),
        thruline_verify::RunnerOutcome::Unverifiable { reason, hint } => serde_json::json!({
            "unverifiable": { "reason": reason, "hint": hint },
        }),
    };
    value.to_string()
}

/// Read back what the recorder stored for an exec, bounded.
///
/// Returns `(output, why_it_is_incomplete)`. The second half is the point: a
/// missing or oversized output file must reach the report, because a check
/// built from nothing is indistinguishable from a run that produced nothing.
fn captured_output(payload: &Value) -> (String, Option<String>) {
    let mut text = String::new();
    let mut problem: Option<String> = None;
    let mut budget = CHECK_PARSE_LIMIT;
    for key in ["stdout_store", "stderr_store"] {
        let Some(path) = payload.get(key).and_then(Value::as_str) else {
            continue;
        };
        let path = Path::new(path);
        match std::fs::metadata(path) {
            Ok(meta) if meta.len() > budget => {
                problem = Some(format!(
                    "{key} is {} bytes, past the {CHECK_PARSE_LIMIT}-byte parse limit; \
                     the run's result was not read",
                    meta.len()
                ));
                continue;
            }
            Ok(meta) => budget = budget.saturating_sub(meta.len()),
            Err(e) => {
                problem = Some(format!(
                    "{key} at {} could not be read: {e}",
                    path.display()
                ));
                continue;
            }
        }
        match std::fs::read_to_string(path) {
            Ok(body) => text.push_str(&body),
            Err(e) => {
                problem = Some(format!(
                    "{key} at {} could not be read: {e}",
                    path.display()
                ))
            }
        }
    }
    (text, problem)
}

/// Re-append a batch with one refused event removed.
///
/// The refused event is quarantined. Everything after it in the chain refers to
/// its hash, so those are quarantined too rather than written into a chain with
/// a hole — a chain that verifies is the product's whole claim, and half a
/// session that verifies is worth more than a whole one that does not.
fn retry_without(
    ledger: &Ledger,
    repo: &RepoId,
    rows: &[EventRow],
    refused_id: &str,
    quarantine: &mut Vec<(u64, String, String)>,
    reason: EventError,
) -> Result<(u64, usize), LedgerError> {
    let at = rows
        .iter()
        .position(|r| r.event.id().as_str() == refused_id)
        .unwrap_or(0);
    for row in &rows[at..] {
        quarantine.push((
            row.event.seq(),
            format!(
                "refused by the ledger's hash check ({reason}); everything after it \
                 in this batch chains onto it and is held back with it"
            ),
            row.event.id().to_string(),
        ));
    }
    if at == 0 {
        return Ok((0, 0));
    }
    Ok((ledger.append_events(repo, &rows[..at])?, at))
}

/// The path a tool delivery says it changed, and how.
///
/// Returns `None` when the delivery names no path — a `Bash` command can change
/// anything and says which nowhere, so nothing is invented for it. The exec
/// recorder's before/after snapshots cover that case instead.
fn observed_change(
    harness: &crate::harness::Harness,
    hook_event: &str,
    payload: &Value,
) -> Option<(String, FsOp)> {
    if !is_tool_post(harness, hook_event) {
        return None;
    }
    let tool = payload.get("tool_name").and_then(Value::as_str)?;
    if !MUTATING_TOOLS.contains(&tool) {
        return None;
    }
    let path = payload
        .get("tool_input")
        .and_then(|i| i.get("file_path"))
        .and_then(Value::as_str)
        // Gemini's `write_file` / `replace` name the path `file_path` too, but
        // its `run_shell_command` names none — same shape, same silence.
        .or_else(|| {
            payload
                .get("tool_input")
                .and_then(|i| i.get("absolute_path"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            payload
                .get("tool_response")
                .and_then(|r| r.get("filePath"))
                .and_then(Value::as_str)
        })?;
    // `Write` creates or overwrites; `Edit` and its siblings modify an existing
    // file. Inferred from the TOOL, and the inference is stamped on the event
    // rather than presented as an observation — the hook does not report
    // whether the file existed before.
    let op = match tool {
        "Write" | "NotebookEdit" => FsOp::Create,
        _ => FsOp::Modify,
    };
    Some((path.to_string(), op))
}

/// An `fs_change` event for one observed path.
fn fs_change_event(
    session: &str,
    after_seq: u64,
    prev: &str,
    path: &str,
    op: FsOp,
) -> Result<(Event, u64), String> {
    let seq = after_seq + 1;
    let mut extra: BTreeMap<String, Value> = BTreeMap::new();
    extra.insert(
        "op_inferred_from".into(),
        Value::String("tool_name; the hook does not report prior existence".into()),
    );
    extra.insert(
        "observed_by".into(),
        Value::String("claude-code-hook".into()),
    );
    let payload = FsChange {
        path: path.to_string(),
        op,
        // Not computed: the hook delivers after the write, so the prior bytes
        // are already gone. A fabricated digest would be worse than none.
        before_hash: None,
        after_hash: None,
        extra,
    };
    let id =
        thruline_core::spec::event::EventId::derive(format!("{session}|{seq}|{path}").as_bytes());
    let event = Event::new(
        session,
        seq,
        "1970-01-01T00:00:00Z",
        Source::Other(HOOK_SOURCE.into()),
        Payload::FsChange(payload),
        prev,
    )
    .map_err(|e| format!("fs_change event is not constructible: {e}"))?
    .with_id(id)
    .seal()
    .map_err(|e| format!("fs_change event does not seal: {e}"))?;
    Ok((event, seq))
}

fn is_mutating(harness: &crate::harness::Harness, hook_event: &str, payload: &Value) -> bool {
    if !is_tool_post(harness, hook_event) {
        return false;
    }
    payload
        .get("tool_name")
        .and_then(Value::as_str)
        .is_some_and(|t| MUTATING_TOOLS.contains(&t))
}

/// Is this the harness's *after a tool ran* event?
///
/// Asked through the harness's own map rather than by name: the three
/// harnesses call it `PostToolUse`, `PostToolUse`+`PostToolUseFailure`, and
/// `AfterTool`, and a name comparison here is the shape that made this
/// codebase's Claude Code adapter capture only passing runs until the vendor
/// doc was read.
fn is_tool_post(harness: &crate::harness::Harness, hook_event: &str) -> bool {
    (harness.kind_for)(hook_event) == Some("tool_result")
}

/// A `snapshot` event for the tree as it stands now.
fn snapshot_event(
    root: &Path,
    session: &str,
    after_seq: u64,
    prev: &str,
) -> Result<(Event, u64), String> {
    let snap: TreeSnapshot =
        snapshot_tree(root).map_err(|e| format!("snapshot of {} failed: {e}", root.display()))?;
    let seq = after_seq + 1;
    let mut extra: BTreeMap<String, Value> = BTreeMap::new();
    // **When this was measured, stated on the event.**
    //
    // The fold runs after the session, so a snapshot it takes is a snapshot of
    // *now* carrying the `seq` of a *past* boundary. Every fold-derived
    // snapshot in a session therefore holds the same tree hash, and reading one
    // as "the tree as it was after that tool ran" is a fabrication — the same
    // shape as binding a check to a tree measured later, which is the
    // laundering the clean-filter finding identified.
    //
    // The tree hash itself is true; the moment is not. So the moment is
    // labelled, and §6.7 takes its base tree from the session's `head_commit`
    // — a state git recorded, not one this process inferred.
    extra.insert("taken_at".into(), Value::String("fold".into()));
    extra.insert("contemporaneous".into(), Value::Bool(false));
    let payload = SnapshotPayload {
        tree_hash: snap.tree_hash,
        head_commit: snap.head_commit,
        dirty: snap.dirty,
        changed_paths_count: snap.changed_paths_count,
        extra,
    };
    let id = thruline_core::spec::event::EventId::derive(
        format!("{session}|{seq}|{}", payload.tree_hash).as_bytes(),
    );
    let event = Event::new(
        session,
        seq,
        "1970-01-01T00:00:00Z",
        Source::Git,
        Payload::Snapshot(payload),
        prev,
    )
    .map_err(|e| format!("snapshot event is not constructible: {e}"))?
    .with_id(id)
    .seal()
    .map_err(|e| format!("snapshot event does not seal: {e}"))?;
    Ok((event, seq))
}

/// Map one hook delivery onto a §3.1 payload.
///
/// An unrecognised hook event is an error, not a `note`: the fold must not
/// invent a classification for something it does not understand, and the
/// quarantine is where an unclassifiable delivery belongs so a human sees it.
fn build_payload(
    harness: &crate::harness::Harness,
    hook_event: &str,
    p: &Value,
) -> Result<Payload, String> {
    let s = |key: &str| {
        p.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let open = |keys: &[&str]| -> OpenPayload {
        keys.iter()
            .filter_map(|k| p.get(*k).map(|v| ((*k).to_string(), v.clone())))
            .collect()
    };

    // Gemini's names differ throughout and its outcome lives in a field rather
    // than in the event name, so it is mapped by kind rather than by name.
    if harness.name == "gemini" {
        return gemini_payload(harness, hook_event, p);
    }

    Ok(match hook_event {
        "SessionStart" => Payload::SessionStart(open(&["source", "cwd", "permission_mode"])),
        "UserPromptSubmit" => Payload::UserPrompt(open(&["prompt", "prompt_id", "cwd"])),
        "PreToolUse" => Payload::ToolCall(ToolCall {
            tool: s("tool_name"),
            input_hash: digest(&p.get("tool_input").cloned().unwrap_or(Value::Null)),
            input_redacted: compact(p.get("tool_input")),
            cwd: s("cwd"),
            extra: Default::default(),
        }),
        "PostToolUse" | "PostToolUseFailure" => Payload::ToolResult(ToolResult {
            tool: s("tool_name"),
            call_id: s("tool_use_id"),
            // The distinction the two events exist to carry. Subscribing to only
            // one of them would make every result look successful.
            //
            // **On Codex this is not what the event means**, and `ok_basis`
            // below is how a reader can tell. Codex's `PostToolUse` fires after
            // a non-zero Bash exit too, and the documented payload names no
            // outcome field — `tool_response` is "Tool-specific output", shape
            // unspecified. So `ok` is not derivable there and is not asserted
            // to be.
            ok: hook_event == "PostToolUse",
            output_hash: digest(
                &p.get("tool_response")
                    .or_else(|| p.get("error"))
                    .cloned()
                    .unwrap_or(Value::Null),
            ),
            output_excerpt: compact(p.get("tool_response").or_else(|| p.get("error"))),
            // Absent, not invented: the hook carries no exit code, and a
            // fabricated 0 would read downstream as observed evidence.
            exit_code: None,
            extra: ok_basis(if harness.name == "codex" {
                // Verified against the vendor doc 2026-09-02, not assumed.
                OK_NOT_STATED
            } else {
                "event_name"
            }),
        }),
        "PreCompact" => Payload::CompactionPre(CompactionPre {
            open_promises: Vec::new(),
            open_tasks: Vec::new(),
            recent_claims: Vec::new(),
            transcript_hash: digest(&Value::String(s("transcript_path"))),
            extra: Default::default(),
        }),
        "PostCompact" => Payload::CompactionPost(CompactionPost {
            transcript_hash: digest(&Value::String(s("compact_summary"))),
            extra: Default::default(),
        }),
        "SubagentStop" => Payload::SubagentStop(open(&["agent_id", "agent_type"])),
        "Stop" => Payload::Stop(Stop {
            // `decision` and `reason` are TOP level on Stop — verified against
            // the vendor doc after a summarised pass placed them inside
            // `hookSpecificOutput`.
            reason: match p.get("decision").and_then(Value::as_str) {
                Some("block") => StopReason::HookBlocked,
                _ => StopReason::AgentFinished,
            },
            last_assistant_message_hash: digest(&Value::String(s("last_assistant_message"))),
            last_assistant_message: s("last_assistant_message"),
            stop_hook_active: p.get("stop_hook_active").and_then(Value::as_bool),
            extra: Default::default(),
        }),
        "SessionEnd" => Payload::SessionEnd(open(&["reason", "cwd"])),
        other => {
            return Err(format!(
                "no §3.1 mapping for hook event {other:?} — quarantined rather \
                 than guessed at"
            ))
        }
    })
}

/// `ToolResult.ok` is only as good as what said so, and this records which.
///
/// `event_name` — the harness fired a distinct failure event, so the outcome
/// *is* the event. `tool_response.error` — the harness put it in a field.
/// [`OK_NOT_STATED`] — the harness fires one event for both outcomes and its
/// documented payload names neither. A reader that treats `ok == false` as a
/// failure must check this first, or an unstated outcome becomes an accusation
/// on one harness and a clean bill on another.
fn ok_basis(basis: &str) -> thruline_core::spec::event::Extra {
    let mut extra: BTreeMap<String, Value> = BTreeMap::new();
    extra.insert("ok_basis".into(), Value::String(basis.to_string()));
    extra
}

/// The harness reported an outcome nowhere. **Not the same as a failure.**
pub const OK_NOT_STATED: &str = "not_stated";

/// What a `tool_result` event says about the tool's outcome — **three states,
/// not two.**
///
/// # Why this type exists
///
/// §3.1 fixes `tool_result.ok` as a `bool`, and a bool has no room for "the
/// harness did not say". Claude Code fires a distinct failure event, so its
/// event *is* the outcome; Gemini puts the failure in `tool_response.error`;
/// **Codex fires one event for both and its documented payload names
/// neither.** The fold records which of those applied in `ok_basis`, and the
/// bool beside it is only as good as that basis.
///
/// A reader that takes `ok` without the basis therefore reads an *unstated*
/// Codex outcome as a success. That is verbatim the
/// `PostToolUse`-fires-only-on-success finding — the one whose whole point was
/// that a systematic bias toward "it passed" destroys a false-success detector
/// — reappearing one consumer downstream.
///
/// So the two fields are never read apart. [`tool_outcome`] is the only place
/// in the workspace that reads either, `NotStated` is a variant a `match` must
/// handle, and a census fails the build if a second reader appears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolOutcome {
    /// Something in the record said the tool succeeded.
    Succeeded,
    /// Something in the record said it failed.
    Failed,
    /// **Nothing said either way.** Not a success, and not an accusation.
    NotStated,
}

impl ToolOutcome {
    /// Did the record state a failure? `false` for [`ToolOutcome::NotStated`].
    ///
    /// The predicate a stop-reason classifier wants: "is there evidence this
    /// tool failed", which an absence of evidence does not satisfy.
    pub fn is_stated_failure(self) -> bool {
        self == ToolOutcome::Failed
    }

    /// A word for a human-readable timeline. `NotStated` renders as its own
    /// word rather than borrowing either outcome's.
    pub fn label(self) -> &'static str {
        match self {
            ToolOutcome::Succeeded => "ok",
            ToolOutcome::Failed => "FAILED",
            ToolOutcome::NotStated => "outcome not stated",
        }
    }
}

/// Read a `tool_result` payload's outcome — **the only reader of `ok` and
/// `ok_basis` in the workspace.**
///
/// A payload with no `ok` at all is [`ToolOutcome::NotStated`], for the same
/// reason: an absent field is not a passing one.
pub fn tool_outcome(payload: &Value) -> ToolOutcome {
    if payload.get("ok_basis").and_then(Value::as_str) == Some(OK_NOT_STATED) {
        return ToolOutcome::NotStated;
    }
    match payload.get("ok").and_then(Value::as_bool) {
        Some(true) => ToolOutcome::Succeeded,
        Some(false) => ToolOutcome::Failed,
        None => ToolOutcome::NotStated,
    }
}

/// Gemini's payloads (§4.3). Different names, and the outcome is a field.
fn gemini_payload(
    harness: &crate::harness::Harness,
    hook_event: &str,
    p: &Value,
) -> Result<Payload, String> {
    let s = |key: &str| {
        p.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let open = |keys: &[&str]| -> OpenPayload {
        keys.iter()
            .filter_map(|k| p.get(*k).map(|v| ((*k).to_string(), v.clone())))
            .collect()
    };
    let kind = (harness.kind_for)(hook_event).ok_or_else(|| {
        format!(
            "no §3.1 mapping for Gemini event {hook_event:?} — quarantined rather than guessed at"
        )
    })?;
    Ok(match kind {
        "session_start" => Payload::SessionStart(open(&["source", "cwd", "timestamp"])),
        "user_prompt" => Payload::UserPrompt(open(&["prompt", "cwd"])),
        "tool_call" => Payload::ToolCall(ToolCall {
            tool: s("tool_name"),
            input_hash: digest(&p.get("tool_input").cloned().unwrap_or(Value::Null)),
            input_redacted: compact(p.get("tool_input")),
            cwd: s("cwd"),
            extra: Default::default(),
        }),
        "tool_result" => {
            // **Refused, not coerced.** An envelope shape this adapter has not
            // seen would have to be read as a success, and then "the record did
            // not say" and "the tool succeeded" become the same value.
            let ok = crate::gemini::tool_ok(p).map_err(|refusal| {
                format!(
                    "Gemini AfterTool envelope is not the documented shape ({}) — \
                     quarantined rather than read as a success",
                    refusal.as_str()
                )
            })?;
            Payload::ToolResult(ToolResult {
                tool: s("tool_name"),
                // There is no `tool_use_id` on this harness, so Before/After
                // cannot be correlated by id. Empty is the honest value.
                call_id: String::new(),
                ok,
                output_hash: digest(&p.get("tool_response").cloned().unwrap_or(Value::Null)),
                output_excerpt: compact(p.get("tool_response")),
                exit_code: None,
                extra: ok_basis("tool_response.error"),
            })
        }
        "stop" => Payload::Stop(Stop {
            reason: match p.get("decision").and_then(Value::as_str) {
                Some("deny" | "block") => StopReason::HookBlocked,
                _ => StopReason::AgentFinished,
            },
            // `AfterAgent.prompt_response`, which is this harness's statement
            // of what it said. There is no `Stop` event to take it from.
            last_assistant_message_hash: digest(&Value::String(s("prompt_response"))),
            last_assistant_message: s("prompt_response"),
            stop_hook_active: p.get("stop_hook_active").and_then(Value::as_bool),
            extra: Default::default(),
        }),
        "compaction_pre" => Payload::CompactionPre(CompactionPre {
            open_promises: Vec::new(),
            open_tasks: Vec::new(),
            recent_claims: Vec::new(),
            transcript_hash: digest(&Value::String(s("transcript_path"))),
            // There is no `PostCompress` on this harness, so nothing will close
            // this boundary. Said on the event rather than left to be inferred
            // from an absence.
            extra: {
                let mut e: BTreeMap<String, Value> = BTreeMap::new();
                e.insert(
                    "no_post_side".into(),
                    Value::String(
                        "Gemini fires PreCompress only; the compaction's end is not observable"
                            .into(),
                    ),
                );
                e
            },
        }),
        "note" => Payload::Note(open(&["notification_type", "message", "details"])),
        "session_end" => Payload::SessionEnd(open(&["reason", "cwd"])),
        other => {
            return Err(format!(
                "Gemini event {hook_event:?} maps to kind {other:?}, which this fold \
                 does not build — quarantined rather than guessed at"
            ))
        }
    })
}

fn digest(v: &Value) -> String {
    let bytes = match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    format!(
        "sha256:{}",
        thruline_core::chain::sha256_hex(bytes.as_bytes())
    )
}

/// A bounded rendering of an already-redacted value.
///
/// Masks, THEN cuts — locally, in this function. The spool line was redacted
/// before it was written (§4.6), so the mask here is a fixpoint and finds
/// nothing. It runs anyway because `redact::TRUNCATION_SITES` verifies the
/// mask-then-cut invariant by proximity within one function, and a guarantee
/// that holds two frames up is invisible to it. This is the third new
/// truncation site in this milestone that the census caught before a human did;
/// making the invariant locally true is cheaper than arguing for an exemption
/// each time.
///
/// `truncate_respecting_markers` keeps the cut from landing inside a
/// `[masked: …]` marker (A209-C4/B2).
fn compact(v: Option<&Value>) -> String {
    let text = match v {
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    };
    let (remasked, _record) = thruline_core::redact::mask_recorded(&text);
    debug_assert_eq!(
        remasked, text,
        "re-masking spool text changed it — the hook's redaction was not a \
         fixpoint, which means a marker is being read as a secret"
    );
    thruline_core::redact::truncate_respecting_markers(&remasked, 2_000)
}

/// Write held-back lines, returning why it could not be done.
///
/// The failure used to be discarded with `let _ =`, which made the FoldReport's
/// `quarantined` list a claim about a file that might not exist. Same class as
/// the swallowed snapshot: the count was right and the evidence was not where
/// the count said it was.
fn write_quarantine(spool: &Path, entries: &[(u64, String, String)]) -> Option<String> {
    use std::io::Write;
    let path = quarantine_path(spool);
    let mut file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(f) => f,
        Err(e) => return Some(format!("{}: {e}", path.display())),
    };
    for (seq, reason, raw) in entries {
        let line = serde_json::json!({ "spool_seq": seq, "reason": reason, "line": raw });
        if let Err(e) = writeln!(file, "{line}") {
            return Some(format!("{}: {e}", path.display()));
        }
    }
    None
}

#[cfg(test)]
mod tests;
