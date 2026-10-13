//! T-3-4 (`B7.1`, `B7.6`, `B6.8`, `B12.6`, `Q12`) — what happened to the
//! *execution* of a check, kept apart from what the check *observed*.
//!
//! ## Why this is a second dimension and not a sixth state word
//!
//! The record already says what a run observed: `proven`, `not_proven`,
//! `couldnt`, `loose`, `fog` ([`crate::change_evidence::Observed`]). It said
//! nothing about whether the run *reached the end*. A run whose last written
//! line was a pass and whose process was then killed looked, to every surface
//! reading only the observed half, exactly like a run that finished. That is
//! the failure this module exists to stop, and design record §3.8's "never one
//! badge" is the same rule seen from the screen: **execution state and
//! observed result are two fields and are never merged.**
//!
//! ## The states (`B7.1`), as a closed set
//!
//! | state | what the record holds |
//! |---|---|
//! | `not run` | no row at all. Absence is the encoding, deliberately: a run that has not happened has nothing to append. |
//! | `running` | a supervisor row — when it started, which process, which process group, which examined-inputs record. |
//! | `completed` | the process reached the end and its status was recorded. |
//! | `cancelled` | a person stopped it, with their name and the time. |
//! | `interrupted` | the process is gone and no completion was ever recorded: a crash, a kill, HQ shutting down or restarting under it. |
//! | `blocked` | it could not start, because something it needs is absent. |
//!
//! Transitions are **append-only rows** in `change_run_executions`, like every other
//! table in the Change layer: an earlier row is never rewritten to reflect a
//! later truth, only superseded by a newer row naming the same run. Reading
//! the state of a run means reading its latest row, and reading its history
//! means reading them all.
//!
//! ## Never a pass (`B7.6`)
//!
//! [`ExecutionState::settles`] is true for `completed` and for nothing else.
//! [`crate::change_evidence`] consults it before it forms an observed result,
//! so a cancelled, interrupted or blocked run cannot reach a criterion as
//! anything but a run that settled nothing — it is still listed, because a run
//! that was stopped half-way is a fact a person needs, and it is never counted.
//!
//! ## Where the processes are touched
//!
//! Nowhere here. This module is the **record**: it stores, reads and
//! reconciles. Signalling a process group, waiting a bounded time and
//! confirming nothing survived is the CLI's job, in the crate that already
//! spawns and signals ([`thruline-cli`'s `run_supervisor`]). Liveness is
//! injected into [`reconcile`] as a closure for exactly that reason, which is
//! also what makes "a dead pid becomes interrupted" and "a reused pid is never
//! adopted" testable without spawning anything.

use std::collections::BTreeMap;

use rusqlite::params;
use serde::Serialize;

use crate::error::{CoreError, Result};
use crate::ledger::Ledger;

/// Bump when a served field of [`ExecutionRecord`] is added or changed.
pub const RUN_EXECUTION_SCHEMA_VERSION: u32 = 1;

/// What happened to the execution of one check (`B7.1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionState {
    /// No row: nothing has been started for this run.
    NotRun,
    Running,
    Completed,
    Cancelled,
    Interrupted,
    Blocked,
}

impl ExecutionState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotRun => "not_run",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
            Self::Blocked => "blocked",
        }
    }

    /// The copy-file slug for the sentence. Looked up by a surface, never
    /// built here.
    pub fn slug(self) -> &'static str {
        match self {
            Self::NotRun => "execution_not_run",
            Self::Running => "execution_running",
            Self::Completed => "execution_completed",
            Self::Cancelled => "execution_cancelled",
            Self::Interrupted => "execution_interrupted",
            Self::Blocked => "execution_blocked",
        }
    }

    /// Parse a stored value. `not_run` is never stored — it is the absence of
    /// a row — so a store that holds it is a defect and says so.
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            "interrupted" => Ok(Self::Interrupted),
            "blocked" => Ok(Self::Blocked),
            other => Err(CoreError::Config(format!(
                "unknown execution state {other:?}"
            ))),
        }
    }

    /// **`B7.6`.** Can a run in this state satisfy a criterion at all?
    ///
    /// Exactly one state can, and it is not "the last line said ok". A run
    /// still going has not finished; a cancelled or interrupted one did not
    /// finish; a blocked one never started.
    pub fn settles(self) -> bool {
        matches!(self, Self::Completed)
    }

    /// Is this a state the run can still leave? Used by the reconciler and by
    /// nothing else, so "terminal" has one definition.
    pub fn is_open(self) -> bool {
        matches!(self, Self::Running)
    }
}

/// Why a run is in the state it is, as a closed set of slugs.
///
/// Sam never reads these words; they key a sentence in both registers
/// ([`crate::voice::EXECUTION_LINES`]). The Plain half of every one of them is
/// swept by `crates/core/tests/run_execution_copy.rs` for "pid", "process
/// group" and "SIGTERM", which are the three the card forbids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// A `running` row whose process was gone when HQ next started.
    HqRestarted,
    /// HQ was asked to stop and stopped the run first.
    HqShutDown,
    /// A person asked for it to stop.
    CancelledByPerson,
    /// A `running` row whose pid is alive and is somebody else's. Never
    /// adopted: the record says the run is gone and the pid was reused.
    PidReused,
    /// The run finished, and what it was examining had moved underneath it
    /// (`B6.8`). The result is kept; it is never a fresh pass.
    InputsChanged,
    /// Something the run needs is not on this machine.
    PrerequisiteAbsent,
    /// The child was killed by a signal and no cancellation record explains
    /// it: a crash, an external `kill`, the system reclaiming it. The wrapper
    /// saw the signal on its own child and says so, rather than calling a
    /// reaped corpse a completed check (verifier finding 1).
    ProcessSignalled,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HqRestarted => "hq_restarted",
            Self::HqShutDown => "hq_shut_down",
            Self::CancelledByPerson => "cancelled_by_person",
            Self::PidReused => "pid_reused",
            Self::InputsChanged => "inputs_changed",
            Self::PrerequisiteAbsent => "prerequisite_absent",
            Self::ProcessSignalled => "process_signalled",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::HqRestarted => "execution_reason_hq_restarted",
            Self::HqShutDown => "execution_reason_hq_shut_down",
            Self::CancelledByPerson => "execution_reason_cancelled_by_person",
            Self::PidReused => "execution_reason_pid_reused",
            Self::InputsChanged => "execution_reason_inputs_changed",
            Self::PrerequisiteAbsent => "execution_reason_prerequisite_absent",
            Self::ProcessSignalled => "execution_reason_process_signalled",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "hq_restarted" => Ok(Self::HqRestarted),
            "hq_shut_down" => Ok(Self::HqShutDown),
            "cancelled_by_person" => Ok(Self::CancelledByPerson),
            "pid_reused" => Ok(Self::PidReused),
            "inputs_changed" => Ok(Self::InputsChanged),
            "prerequisite_absent" => Ok(Self::PrerequisiteAbsent),
            "process_signalled" => Ok(Self::ProcessSignalled),
            other => Err(CoreError::Config(format!(
                "unknown execution reason {other:?}"
            ))),
        }
    }
}

/// **OFFLINE-BUILD-OUTCOME (D-051).** Did this run reach the end having
/// found that something it needs is not on this machine — an offline build
/// whose dependencies could not be fetched?
///
/// The wrapper decides that only from its own isolated dependency-fetch
/// step, before project code runs, and records that actual fetch attempt as
/// `completed` with the reason `prerequisite_absent`. This is the ONE reading
/// of that record, used by the run sheet's outcome and by the coverage layer's
/// observed result alike, so the two cannot disagree: such a run checked
/// nothing and settles nothing, whatever its exit code — never "failed", and
/// never a pass. (`blocked` with the same reason is a program that was never
/// found, and is read by its state, not here.)
pub fn dependencies_unavailable(state: ExecutionState, reason: Option<Reason>) -> bool {
    state == ExecutionState::Completed && reason == Some(Reason::PrerequisiteAbsent)
}

/// What a run was, when it started (`B12.6`).
///
/// `process_group` is stored beside `pid` because cleanup is a question about
/// the group, not about the one process: a check that spawned a test runner
/// that spawned compilers leaves a group behind, and "the pid is gone" would
/// be a true statement about the wrong thing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start {
    pub run_key: String,
    pub change_id: Option<String>,
    /// The command as the record will name it, for the screen.
    pub name: String,
    pub pid: Option<i64>,
    pub process_group: Option<i64>,
    pub started_at: String,
    /// The examined-inputs record this run froze at the start (T-3-2). It is
    /// what `B6.8`'s comparison at completion is made against.
    pub examined_inputs_id: Option<String>,
}

/// How a run ended, as the record will hold it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finish {
    pub run_key: String,
    pub state: ExecutionState,
    pub reason: Option<Reason>,
    pub actor: Option<String>,
    /// The binding the run produced, when it produced one. An interrupted run
    /// usually has none, which is exactly why the execution record cannot live
    /// on the binding.
    pub binding_id: Option<String>,
    /// **Verified, never assumed.** `Some(true)` means something looked and
    /// found no process left in the run's group; `Some(false)` means something
    /// looked and found survivors; `None` means nothing looked.
    pub cleanup_verified: Option<bool>,
    /// How many processes were still in the group when it was looked at.
    pub survivors: Option<i64>,
    pub at: String,
}

/// One stored transition, as it is read and served.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExecutionRecord {
    pub id: String,
    pub run_key: String,
    pub change_id: Option<String>,
    pub binding_id: Option<String>,
    pub name: String,
    pub state: ExecutionState,
    /// The copy-file slug for the state sentence.
    pub state_slug: &'static str,
    pub reason: Option<Reason>,
    /// The copy-file slug for the reason sentence. `None` exactly when
    /// `reason` is `None`.
    pub reason_slug: Option<&'static str>,
    pub actor: Option<String>,
    pub pid: Option<i64>,
    pub process_group: Option<i64>,
    pub started_at: Option<String>,
    pub examined_inputs_id: Option<String>,
    pub cleanup_verified: Option<bool>,
    pub survivors: Option<i64>,
    pub recorded_at: String,
    /// **`B7.6`, served so no surface has to re-derive it.** False for every
    /// state but `completed`.
    pub settles: bool,
}

/// A run the record still believes is going, as the reconciler meets it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Supervisor {
    pub run_key: String,
    pub change_id: Option<String>,
    pub name: String,
    pub pid: Option<i64>,
    pub process_group: Option<i64>,
    pub started_at: Option<String>,
}

/// What the world says about a supervised process, asked of the caller.
///
/// The three answers are deliberately not two. "Alive" and "gone" are not
/// enough, because the dangerous case is the third: the pid is alive and is
/// **not the process we started**, which every naive supervisor adopts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness {
    /// Our process, still going.
    Ours,
    /// Nothing is there.
    Gone,
    /// Something is there and it is not ours.
    Reused,
}

fn mint() -> String {
    format!("rx_{}", ulid::Ulid::generate())
}

/// Append a `running` row. Returns the row id.
pub fn start(ledger: &Ledger, s: &Start) -> Result<String> {
    // Serialize the phase handoff with cancellation. A terminal attempt can
    // never be reopened by a fetch completing after the person stopped it.
    // Acquire the write reservation before reading: concurrent fresh attempts
    // must not deadlock while upgrading deferred read transactions to writers.
    //
    // T-4-4-ATTEMPT-BUDGET-R2: a caller that already holds the write lock (the
    // attempt budget's `admit`, which must cover this row) is already inside
    // one; the row joins it and is kept when that caller commits.
    let transaction = if ledger.conn().is_autocommit() {
        Some(rusqlite::Transaction::new_unchecked(
            ledger.conn(),
            rusqlite::TransactionBehavior::Immediate,
        )?)
    } else {
        None
    };
    let state = state_of(ledger, &s.run_key)?;
    if state != ExecutionState::NotRun && state != ExecutionState::Running {
        return Err(CoreError::Config(
            "this attempt already ended; no new process can own it".into(),
        ));
    }
    let id = mint();
    ledger.conn().execute(
        "INSERT INTO change_run_executions(
             id, run_key, change_id, binding_id, name, state, reason, actor,
             pid, process_group, started_at, examined_inputs_id,
             cleanup_verified, survivors, recorded_at)
         VALUES(?1,?2,?3,NULL,?4,'running',NULL,NULL,?5,?6,?7,?8,NULL,NULL,?7)",
        params![
            id,
            s.run_key,
            s.change_id,
            s.name,
            s.pid,
            s.process_group,
            s.started_at,
            s.examined_inputs_id,
        ],
    )?;
    if let Some(transaction) = transaction {
        transaction.commit()?;
    }
    Ok(id)
}

/// Append a terminal row. Returns the row id.
///
/// The `running` row it supersedes is left exactly as it was — that is what
/// makes "it was running from 10:02, and at 10:09 it was found gone" readable
/// as the two facts it is.
pub fn finish(ledger: &Ledger, f: &Finish) -> Result<String> {
    match finish_or_refuse(ledger, f)? {
        Ok(id) => Ok(id),
        Err(already) => Err(CoreError::Config(format!(
            "this run already ended as `{}`; a second ending is refused",
            already.as_str()
        ))),
    }
}

/// **T-3-4 verifier finding 1 — a run can only end once.**
///
/// `cancel-run` signals the child, verifies its group and appends `cancelled`.
/// The wrapper is still sitting in `wait()`; a moment later it reaps the
/// corpse and — before this — appended `completed` on top. A check a person
/// had just stopped then read as a check that ran to the end, and the coverage
/// layer counted it. HQ shutdown did the same. The record contradicted itself
/// about what became of one thing, which is the failure this whole module
/// exists to prevent.
///
/// So the rule lives at the WRITER rather than in each caller: once a run's
/// latest row is terminal, no further ending may be appended. The refusal is
/// typed and loud, never a silent no-op (Rule 12) — but it is a VALUE here,
/// because the one caller that must carry on is the wrapper, for which
/// "somebody already wrote what became of this" is the correct outcome and not
/// a failure of its own.
///
/// Returns `Ok(Ok(id))` for the row it wrote, `Ok(Err(state))` naming the
/// ending the run already has.
pub fn finish_or_refuse(
    ledger: &Ledger,
    f: &Finish,
) -> Result<std::result::Result<String, ExecutionState>> {
    if f.state.is_open() {
        return Err(CoreError::Config(
            "`finish` was asked to write a state a run can still leave".into(),
        ));
    }
    if f.state == ExecutionState::NotRun {
        return Err(CoreError::Config(
            "`not run` is the absence of a row and is never stored".into(),
        ));
    }
    let standing = state_of(ledger, &f.run_key)?;
    if !standing.is_open() && standing != ExecutionState::NotRun {
        return Ok(Err(standing));
    }
    append_finish(ledger, f).map(Ok)
}

fn append_finish(ledger: &Ledger, f: &Finish) -> Result<String> {
    let prior = open_row(ledger, &f.run_key)?;
    let id = mint();
    ledger.conn().execute(
        "INSERT INTO change_run_executions(
             id, run_key, change_id, binding_id, name, state, reason, actor,
             pid, process_group, started_at, examined_inputs_id,
             cleanup_verified, survivors, recorded_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
        params![
            id,
            f.run_key,
            prior.as_ref().and_then(|p| p.change_id.clone()),
            f.binding_id,
            prior
                .as_ref()
                .map(|p| p.name.clone())
                .unwrap_or_else(|| f.run_key.clone()),
            f.state.as_str(),
            f.reason.map(|r| r.as_str()),
            f.actor,
            prior.as_ref().and_then(|p| p.pid),
            prior.as_ref().and_then(|p| p.process_group),
            prior.as_ref().and_then(|p| p.started_at.clone()),
            prior.as_ref().and_then(|p| p.examined_inputs_id.clone()),
            f.cleanup_verified,
            f.survivors,
            f.at,
        ],
    )?;
    Ok(id)
}

/// The latest row for one run, or `None` when nothing was ever started.
pub fn latest(ledger: &Ledger, run_key: &str) -> Result<Option<ExecutionRecord>> {
    let mut statement = ledger.conn().prepare(
        "SELECT id, run_key, change_id, binding_id, name, state, reason, actor,
                pid, process_group, started_at, examined_inputs_id,
                cleanup_verified, survivors, recorded_at
           FROM change_run_executions WHERE run_key=?1
          ORDER BY recorded_at DESC, rowid DESC LIMIT 1",
    )?;
    let mut rows = statement.query(params![run_key])?;
    match rows.next()? {
        Some(row) => Ok(Some(record_of(row)?)),
        None => Ok(None),
    }
}

/// The state of one run — `NotRun` when the record holds nothing.
pub fn state_of(ledger: &Ledger, run_key: &str) -> Result<ExecutionState> {
    Ok(latest(ledger, run_key)?
        .map(|r| r.state)
        .unwrap_or(ExecutionState::NotRun))
}

/// The latest row per run, keyed by the **binding id** the run produced.
///
/// This is the map [`crate::change_evidence`] consults: it meets evidence by
/// binding id and has to know, before it forms an observed result, whether the
/// execution behind it reached the end.
pub fn by_binding(ledger: &Ledger) -> Result<BTreeMap<String, ExecutionRecord>> {
    let mut out = BTreeMap::new();
    for record in all(ledger, None)? {
        if let Some(binding) = record.binding_id.clone() {
            out.insert(binding, record);
        }
    }
    Ok(out)
}

/// The latest row per run, for one Change or for the whole store.
///
/// Ordered oldest run first by the time the run started, so the screen's list
/// is the order things happened in and not the order ULIDs sorted in.
pub fn all(ledger: &Ledger, change_id: Option<&str>) -> Result<Vec<ExecutionRecord>> {
    let mut statement = ledger.conn().prepare(
        "SELECT id, run_key, change_id, binding_id, name, state, reason, actor,
                pid, process_group, started_at, examined_inputs_id,
                cleanup_verified, survivors, recorded_at
           FROM change_run_executions ORDER BY recorded_at ASC, rowid ASC",
    )?;
    let mut rows = statement.query([])?;
    let mut latest: BTreeMap<String, ExecutionRecord> = BTreeMap::new();
    while let Some(row) = rows.next()? {
        let record = record_of(row)?;
        latest.insert(record.run_key.clone(), record);
    }
    let mut out: Vec<ExecutionRecord> = latest
        .into_values()
        .filter(|r| match change_id {
            None => true,
            Some(id) => r.change_id.as_deref() == Some(id),
        })
        .collect();
    out.sort_by(|a, b| {
        (&a.started_at, &a.recorded_at, &a.run_key).cmp(&(
            &b.started_at,
            &b.recorded_at,
            &b.run_key,
        ))
    });
    Ok(out)
}

/// Every run the record still believes is going.
pub fn running(ledger: &Ledger) -> Result<Vec<Supervisor>> {
    Ok(all(ledger, None)?
        .into_iter()
        .filter(|r| r.state.is_open())
        .map(|r| Supervisor {
            run_key: r.run_key,
            change_id: r.change_id,
            name: r.name,
            pid: r.pid,
            process_group: r.process_group,
            started_at: r.started_at,
        })
        .collect())
}

/// What one pass of the reconciler did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reconciled {
    /// Runs whose process was gone: now `interrupted`.
    pub interrupted: Vec<String>,
    /// Runs whose pid is alive and is somebody else's: `interrupted` with
    /// `pid reused`, and **never adopted**.
    pub pid_reused: Vec<String>,
    /// Runs still genuinely going, left exactly as they were.
    pub still_running: Vec<String>,
}

/// **On HQ start (`Q12`).** Every `running` row whose process is gone becomes
/// `interrupted`; every one whose pid belongs to somebody else becomes
/// `interrupted` with `pid reused`. A run that really is ours and really is
/// going is left alone.
///
/// The liveness answer is the caller's, which is what keeps this function free
/// of process handling and lets a test state the three answers directly.
pub fn reconcile(
    ledger: &Ledger,
    at: &str,
    reason: Reason,
    mut liveness: impl FnMut(&Supervisor) -> Liveness,
) -> Result<Reconciled> {
    let mut out = Reconciled::default();
    for supervisor in running(ledger)? {
        match liveness(&supervisor) {
            Liveness::Ours => out.still_running.push(supervisor.run_key),
            Liveness::Gone => {
                finish(
                    ledger,
                    &Finish {
                        run_key: supervisor.run_key.clone(),
                        state: ExecutionState::Interrupted,
                        reason: Some(reason),
                        actor: None,
                        binding_id: None,
                        // The process is gone, which IS the cleanup answer,
                        // and it was looked at rather than assumed.
                        cleanup_verified: Some(true),
                        survivors: Some(0),
                        at: at.to_string(),
                    },
                )?;
                out.interrupted.push(supervisor.run_key);
            }
            Liveness::Reused => {
                finish(
                    ledger,
                    &Finish {
                        run_key: supervisor.run_key.clone(),
                        state: ExecutionState::Interrupted,
                        reason: Some(Reason::PidReused),
                        actor: None,
                        binding_id: None,
                        // Nothing of ours can be confirmed gone, because what
                        // is there is not ours to ask about.
                        cleanup_verified: None,
                        survivors: None,
                        at: at.to_string(),
                    },
                )?;
                out.pid_reused.push(supervisor.run_key);
            }
        }
    }
    Ok(out)
}

fn open_row(ledger: &Ledger, run_key: &str) -> Result<Option<ExecutionRecord>> {
    let mut statement = ledger.conn().prepare(
        "SELECT id, run_key, change_id, binding_id, name, state, reason, actor,
                pid, process_group, started_at, examined_inputs_id,
                cleanup_verified, survivors, recorded_at
           FROM change_run_executions WHERE run_key=?1 AND state='running'
          ORDER BY recorded_at DESC, rowid DESC LIMIT 1",
    )?;
    let mut rows = statement.query(params![run_key])?;
    match rows.next()? {
        Some(row) => Ok(Some(record_of(row)?)),
        None => Ok(None),
    }
}

fn record_of(row: &rusqlite::Row<'_>) -> Result<ExecutionRecord> {
    let state = ExecutionState::parse(&row.get::<_, String>(5)?)?;
    let reason = match row.get::<_, Option<String>>(6)? {
        Some(value) => Some(Reason::parse(&value)?),
        None => None,
    };
    Ok(ExecutionRecord {
        id: row.get(0)?,
        run_key: row.get(1)?,
        change_id: row.get(2)?,
        binding_id: row.get(3)?,
        name: row.get(4)?,
        state,
        state_slug: state.slug(),
        reason,
        reason_slug: reason.map(|r| r.slug()),
        actor: row.get(7)?,
        pid: row.get(8)?,
        process_group: row.get(9)?,
        started_at: row.get(10)?,
        examined_inputs_id: row.get(11)?,
        cleanup_verified: row.get(12)?,
        survivors: row.get(13)?,
        recorded_at: row.get(14)?,
        settles: state.settles(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ledger(dir: &tempfile::TempDir) -> Ledger {
        Ledger::open(dir.path().join("ledger.db")).expect("a fresh ledger opens")
    }

    fn started(l: &Ledger, key: &str) -> String {
        start(
            l,
            &Start {
                run_key: key.to_string(),
                change_id: Some("chg_1".into()),
                name: "cargo test".into(),
                pid: Some(4242),
                process_group: Some(4242),
                started_at: "2026-09-22T10:00:00Z".into(),
                examined_inputs_id: Some("ei_1".into()),
            },
        )
        .expect("a running row appends")
    }

    #[test]
    fn a_fetch_to_check_handoff_cannot_reopen_a_cancelled_attempt() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ledger = ledger(&dir);
        started(&ledger, "fetch_then_check");
        finish(
            &ledger,
            &Finish {
                run_key: "fetch_then_check".into(),
                state: ExecutionState::Cancelled,
                reason: Some(Reason::CancelledByPerson),
                actor: Some("sam".into()),
                binding_id: None,
                cleanup_verified: Some(true),
                survivors: Some(0),
                at: "2026-09-22T10:05:00Z".into(),
            },
        )
        .expect("cancel fetch");
        let resumed = start(
            &ledger,
            &Start {
                run_key: "fetch_then_check".into(),
                change_id: Some("chg_1".into()),
                name: "cargo test".into(),
                pid: Some(5252),
                process_group: Some(5252),
                started_at: "2026-09-22T10:06:00Z".into(),
                examined_inputs_id: Some("ei_1".into()),
            },
        );
        assert!(
            resumed.is_err(),
            "a finished attempt cannot acquire a new child"
        );
        assert_eq!(
            state_of(&ledger, "fetch_then_check").expect("state"),
            ExecutionState::Cancelled
        );
        assert!(running(&ledger).expect("supervisors").is_empty());
    }

    #[test]
    fn a_fetch_to_check_handoff_supersedes_the_fetch_process() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ledger = ledger(&dir);
        started(&ledger, "fetch_then_check");
        start(
            &ledger,
            &Start {
                run_key: "fetch_then_check".into(),
                change_id: Some("chg_1".into()),
                name: "cargo test".into(),
                pid: Some(5252),
                process_group: Some(5252),
                started_at: "2026-09-22T10:01:00Z".into(),
                examined_inputs_id: Some("ei_1".into()),
            },
        )
        .expect("handoff");
        let supervisors = running(&ledger).expect("supervisors");
        assert_eq!(supervisors.len(), 1);
        assert_eq!(supervisors[0].pid, Some(5252));
        assert_eq!(supervisors[0].process_group, Some(5252));
    }

    #[test]
    fn a_run_with_no_row_is_not_run_rather_than_anything_else() {
        let dir = tempfile::tempdir().expect("tempdir");
        let l = ledger(&dir);
        assert_eq!(
            state_of(&l, "nothing").expect("read"),
            ExecutionState::NotRun
        );
        assert!(!ExecutionState::NotRun.settles());
    }

    #[test]
    fn every_transition_is_readable_and_only_completed_settles() {
        let dir = tempfile::tempdir().expect("tempdir");
        let l = ledger(&dir);
        for (key, state, reason) in [
            ("r_done", ExecutionState::Completed, None),
            (
                "r_cancel",
                ExecutionState::Cancelled,
                Some(Reason::CancelledByPerson),
            ),
            (
                "r_int",
                ExecutionState::Interrupted,
                Some(Reason::HqRestarted),
            ),
            (
                "r_block",
                ExecutionState::Blocked,
                Some(Reason::PrerequisiteAbsent),
            ),
        ] {
            started(&l, key);
            assert_eq!(state_of(&l, key).expect("read"), ExecutionState::Running);
            finish(
                &l,
                &Finish {
                    run_key: key.to_string(),
                    state,
                    reason,
                    actor: Some("sam".into()),
                    binding_id: Some(format!("bind_{key}")),
                    cleanup_verified: Some(true),
                    survivors: Some(0),
                    at: "2026-09-22T10:05:00Z".into(),
                },
            )
            .expect("a terminal row appends");
            let read = latest(&l, key).expect("read").expect("a row");
            assert_eq!(read.state, state);
            assert_eq!(read.reason, reason);
            assert_eq!(read.settles, state == ExecutionState::Completed);
            // The supervisor's own fields are carried onto the terminal row,
            // so a person reading one row never has to join to know what ran.
            assert_eq!(read.pid, Some(4242));
            assert_eq!(read.process_group, Some(4242));
            assert_eq!(read.examined_inputs_id.as_deref(), Some("ei_1"));
        }
        assert!(ExecutionState::Completed.settles());
        for state in [
            ExecutionState::NotRun,
            ExecutionState::Running,
            ExecutionState::Cancelled,
            ExecutionState::Interrupted,
            ExecutionState::Blocked,
        ] {
            assert!(!state.settles(), "{} must never settle", state.as_str());
        }
    }

    #[test]
    fn the_running_row_is_not_rewritten_when_the_run_ends() {
        let dir = tempfile::tempdir().expect("tempdir");
        let l = ledger(&dir);
        started(&l, "r_1");
        finish(
            &l,
            &Finish {
                run_key: "r_1".into(),
                state: ExecutionState::Cancelled,
                reason: Some(Reason::CancelledByPerson),
                actor: Some("sam".into()),
                binding_id: None,
                cleanup_verified: Some(true),
                survivors: Some(0),
                at: "2026-09-22T10:05:00Z".into(),
            },
        )
        .expect("append");
        let rows: i64 = l
            .conn()
            .query_row(
                "SELECT count(*) FROM change_run_executions WHERE run_key='r_1'",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(rows, 2, "both moments stay readable");
        let still: i64 = l
            .conn()
            .query_row(
                "SELECT count(*) FROM change_run_executions WHERE run_key='r_1' AND state='running'",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(still, 1, "the running row is superseded, never edited");
    }

    #[test]
    fn a_dead_process_becomes_interrupted_on_restart() {
        let dir = tempfile::tempdir().expect("tempdir");
        let l = ledger(&dir);
        started(&l, "r_dead");
        let out = reconcile(&l, "2026-09-22T11:00:00Z", Reason::HqRestarted, |_| {
            Liveness::Gone
        })
        .expect("reconcile");
        assert_eq!(out.interrupted, vec!["r_dead".to_string()]);
        let read = latest(&l, "r_dead").expect("read").expect("a row");
        assert_eq!(read.state, ExecutionState::Interrupted);
        assert_eq!(read.reason, Some(Reason::HqRestarted));
        assert_eq!(read.cleanup_verified, Some(true));
        assert!(!read.settles);
    }

    #[test]
    fn a_reused_pid_is_interrupted_and_never_adopted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let l = ledger(&dir);
        started(&l, "r_reused");
        let out = reconcile(&l, "2026-09-22T11:00:00Z", Reason::HqRestarted, |_| {
            Liveness::Reused
        })
        .expect("reconcile");
        assert_eq!(out.pid_reused, vec!["r_reused".to_string()]);
        assert!(out.still_running.is_empty(), "nothing is adopted");
        let read = latest(&l, "r_reused").expect("read").expect("a row");
        assert_eq!(read.state, ExecutionState::Interrupted);
        assert_eq!(read.reason, Some(Reason::PidReused));
        assert_eq!(
            read.cleanup_verified, None,
            "nothing of ours could be confirmed gone, and the record says so \
             rather than claiming it was"
        );
    }

    #[test]
    fn a_run_that_is_really_ours_is_left_alone() {
        let dir = tempfile::tempdir().expect("tempdir");
        let l = ledger(&dir);
        started(&l, "r_live");
        let out = reconcile(&l, "2026-09-22T11:00:00Z", Reason::HqRestarted, |_| {
            Liveness::Ours
        })
        .expect("reconcile");
        assert_eq!(out.still_running, vec!["r_live".to_string()]);
        assert_eq!(
            state_of(&l, "r_live").expect("read"),
            ExecutionState::Running
        );
    }

    #[test]
    fn a_reconciled_run_is_not_reconciled_twice() {
        let dir = tempfile::tempdir().expect("tempdir");
        let l = ledger(&dir);
        started(&l, "r_once");
        for _ in 0..2 {
            reconcile(&l, "2026-09-22T11:00:00Z", Reason::HqRestarted, |_| {
                Liveness::Gone
            })
            .expect("reconcile");
        }
        let rows: i64 = l
            .conn()
            .query_row(
                "SELECT count(*) FROM change_run_executions WHERE state='interrupted'",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(rows, 1);
    }

    /// **Verifier finding 1.** A run ends once. The wrapper reaping a child
    /// that `cancel-run` already stopped must not turn a stopped check into a
    /// finished one, and the refusal is typed rather than silent.
    #[test]
    fn a_second_ending_is_refused_and_the_first_one_stands() {
        let dir = tempfile::tempdir().expect("tempdir");
        let l = ledger(&dir);
        started(&l, "r_1");
        finish(
            &l,
            &Finish {
                run_key: "r_1".into(),
                state: ExecutionState::Cancelled,
                reason: Some(Reason::CancelledByPerson),
                actor: Some("sam".into()),
                binding_id: None,
                cleanup_verified: Some(true),
                survivors: Some(0),
                at: "2026-09-22T10:05:00Z".into(),
            },
        )
        .expect("the first ending appends");

        let second = Finish {
            run_key: "r_1".into(),
            state: ExecutionState::Completed,
            reason: None,
            actor: None,
            binding_id: Some("bind_1".into()),
            cleanup_verified: None,
            survivors: None,
            at: "2026-09-22T10:05:04Z".into(),
        };
        // The value form names what the run already is, for the one caller
        // that has to carry on.
        assert_eq!(
            finish_or_refuse(&l, &second).expect("no error"),
            Err(ExecutionState::Cancelled)
        );
        // The loud form is an error, not a silent no-op (Rule 12).
        assert!(finish(&l, &second).is_err());

        let read = latest(&l, "r_1").expect("read").expect("a row");
        assert_eq!(
            read.state,
            ExecutionState::Cancelled,
            "the first ending stands"
        );
        assert_eq!(read.actor.as_deref(), Some("sam"));
        assert!(!read.settles);
        let terminal: i64 = l
            .conn()
            .query_row(
                "SELECT count(*) FROM change_run_executions
                  WHERE run_key='r_1' AND state <> 'running'",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(terminal, 1, "exactly one ending was ever written");
    }

    #[test]
    fn the_rows_are_append_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let l = ledger(&dir);
        let id = started(&l, "r_1");
        assert!(l
            .conn()
            .execute(
                "UPDATE change_run_executions SET state='completed' WHERE id=?1",
                params![id]
            )
            .is_err());
        assert!(l
            .conn()
            .execute("DELETE FROM change_run_executions WHERE id=?1", params![id])
            .is_err());
    }

    #[test]
    fn a_state_a_run_can_still_leave_is_refused_as_an_ending() {
        let dir = tempfile::tempdir().expect("tempdir");
        let l = ledger(&dir);
        started(&l, "r_1");
        for state in [ExecutionState::Running, ExecutionState::NotRun] {
            assert!(finish(
                &l,
                &Finish {
                    run_key: "r_1".into(),
                    state,
                    reason: None,
                    actor: None,
                    binding_id: None,
                    cleanup_verified: None,
                    survivors: None,
                    at: "2026-09-22T10:05:00Z".into(),
                },
            )
            .is_err());
        }
    }

    #[test]
    fn a_change_sees_only_its_own_runs_latest_first_moment_first() {
        let dir = tempfile::tempdir().expect("tempdir");
        let l = ledger(&dir);
        started(&l, "r_a");
        start(
            &l,
            &Start {
                run_key: "r_b".into(),
                change_id: Some("chg_2".into()),
                name: "npm test".into(),
                pid: None,
                process_group: None,
                started_at: "2026-09-22T09:00:00Z".into(),
                examined_inputs_id: None,
            },
        )
        .expect("append");
        let mine = all(&l, Some("chg_1")).expect("read");
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].run_key, "r_a");
        assert_eq!(all(&l, None).expect("read").len(), 2);
    }
}
