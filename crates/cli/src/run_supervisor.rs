//! T-3-4 (`B12.6`, `Q12`) — the process half of an examination's execution
//! state: stopping a run's process group, and **proving** nothing survived it.
//!
//! This is the CLI's side of [`thruline_core::run_execution`]. The engine holds
//! the record and refuses to guess; this file is the only place in the product
//! that signals or inspects a process on behalf of a check, and it lives here
//! because this is the crate that already spawns children and already installs
//! signal handlers (`integration_cmd::worker`, `docs/REUSE-MAP.md` §1.5).
//!
//! ## Cleanup is verified, never assumed (`B12.6`)
//!
//! A check that runs `cargo test` spawns a test runner which spawns compilers.
//! "The child's pid is gone" is a true statement about the wrong thing. Every
//! run is spawned into a **process group of its own** (`process_group(0)` at
//! the spawn, in `check.rs`), so the whole tree has one name, one signal
//! stops all of it, and one question — *is anything left in this group?* —
//! answers cleanup for the tree rather than for the leader.
//!
//! [`stop_group`] returns [`Cleanup`], which is three-valued on purpose:
//! `Verified` (looked, found nothing), `Survivors(n)` (looked, found some) and
//! `Unknown` (could not look). A boolean would make "we did not check"
//! indistinguishable from "we checked and it was clean" — the exact assumption
//! the requirement forbids — and the record's `cleanup_verified` column is
//! three-valued for the same reason.
//!
//! ## A reused pid is never adopted
//!
//! [`liveness`] asks two questions, not one. The pid may exist and belong to
//! somebody else: pids are recycled, and a supervisor that only asks "does the
//! pid exist" adopts a stranger's process and reports a dead run as still
//! going. The second question is whether that pid is still the **leader of the
//! group we recorded**. Our children are group leaders by construction
//! (`process_group(0)` makes pid and pgid equal), and a recycled pid is
//! overwhelmingly not one; when the answer disagrees the run is
//! [`Liveness::Reused`], which the reconciler records as interrupted and never
//! as running.
//!
//! No `unsafe`: signalling is `rustix`'s safe wrapper, and both readings of the
//! process table are `ps`/`pgrep`, which is also what a person auditing the
//! claim would type.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use thruline_core::run_execution::Liveness;

/// How long HQ waits for a run's process group to go away before it says so.
///
/// Bounded and short: shutdown must not hang on a wedged child, and a record
/// that says "we waited and it did not go" is more useful than a product that
/// never exits.
pub(crate) const STOP_BOUND: Duration = Duration::from_secs(5);

/// What was found when the run's process group was looked at afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cleanup {
    /// Looked, and nothing of this run remains.
    Verified,
    /// Looked, and this many processes remain. Nothing further is signalled:
    /// the product says what it found rather than escalating on its own.
    Survivors(u32),
    /// Could not look. Absence of an observation, not an observation of
    /// absence.
    Unknown,
}

impl Cleanup {
    /// The pair the record stores: `cleanup_verified` and `survivors`.
    pub(crate) fn as_record(self) -> (Option<bool>, Option<i64>) {
        match self {
            Self::Verified => (Some(true), Some(0)),
            Self::Survivors(n) => (Some(false), Some(i64::from(n))),
            Self::Unknown => (None, None),
        }
    }
}

/// Is the process group named by `pgid` still holding anything?
///
/// `pgrep -g <pgid>` is the reading, and it is the same command the card asks a
/// verifier to paste. Our own process is excluded: HQ may share a group with
/// nothing here, but a test harness can, and counting ourselves as a survivor
/// would be a false negative on every run.
pub(crate) fn group_survivors(pgid: i64) -> Cleanup {
    let output = Command::new("pgrep")
        .arg("-g")
        .arg(pgid.to_string())
        .stdin(Stdio::null())
        .output();
    let Ok(output) = output else {
        return Cleanup::Unknown;
    };
    // pgrep exits 1 with no output when nothing matched; anything above that is
    // pgrep itself failing, which is not an answer about the group.
    match output.status.code() {
        Some(0) => {}
        Some(1) => return Cleanup::Verified,
        _ => return Cleanup::Unknown,
    }
    let me = std::process::id();
    let count = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .filter(|pid| *pid != me)
        .count();
    match u32::try_from(count) {
        Ok(0) => Cleanup::Verified,
        Ok(n) => Cleanup::Survivors(n),
        Err(_) => Cleanup::Unknown,
    }
}

/// Stop a run's process group and then **look**.
///
/// The sequence is the ordinary one and its order matters: ask politely
/// (`SIGTERM`), wait a bounded time for the whole group to go, and only then
/// insist (`SIGKILL`) — a test runner that is given no chance to tear down
/// leaves worse debris than one that is. Whatever happens, the last step is a
/// reading of the process table, and that reading is what the record stores.
///
/// Returns `Unknown` when the pgid is not a value a process group can have,
/// which is the honest answer for a run recorded before this dimension existed.
///
/// **Every signal goes through the guarded door (RUN-STOP-PGID-GUARD).** The
/// pgid is a RECORDED number, and `kill_process_group` sends `kill(-pgid)`: a
/// recorded 1 would be `kill(-1)`, every process the user may signal. The
/// guard refuses 0, 1, negative and HQ's own group before any signal, and a
/// refused stop reads `Unknown`, never `Verified`.
#[cfg(unix)]
pub(crate) fn stop_group(pgid: i64, bound: Duration) -> Cleanup {
    use thruline_core::bounded_child::{kill_group_now, term_group_now};

    stop_group_with(pgid, bound, |group, signal| match signal {
        StopSignal::Term => term_group_now(group),
        StopSignal::Kill => kill_group_now(group),
    })
}

/// The two signals a stop sends, in order.
#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StopSignal {
    Term,
    Kill,
}

/// [`stop_group`] with its signaller as a seam: production passes the real
/// one, a test passes a recording fake so no real signal is ever delivered.
#[cfg(unix)]
fn stop_group_with(
    pgid: i64,
    bound: Duration,
    mut signaller: impl FnMut(i32, StopSignal) -> rustix::io::Result<()>,
) -> Cleanup {
    use thruline_core::bounded_child::{signal_group, GroupSignalError};

    let Ok(raw) = i32::try_from(pgid) else {
        return Cleanup::Unknown;
    };
    // A refusal is not a stop: nothing was signalled, so nothing is verified.
    // An ESRCH (`Failed`) means the group is already gone, which is not a
    // failure — the reading below is what decides, either way.
    match signal_group(raw, |group| signaller(group, StopSignal::Term)) {
        Ok(()) | Err(GroupSignalError::Failed(_)) => {}
        Err(GroupSignalError::NotAGroup(_) | GroupSignalError::OwnGroup(_)) => {
            return Cleanup::Unknown;
        }
    }
    let deadline = Instant::now() + bound;
    loop {
        match group_survivors(pgid) {
            Cleanup::Verified => return Cleanup::Verified,
            Cleanup::Unknown => return Cleanup::Unknown,
            Cleanup::Survivors(_) => {}
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = signal_group(raw, |group| signaller(group, StopSignal::Kill));
    // One more bounded look after the insistent signal, so the record states
    // what was true at the end rather than what was true before it.
    let deadline = Instant::now() + Duration::from_millis(500);
    loop {
        match group_survivors(pgid) {
            Cleanup::Verified => return Cleanup::Verified,
            Cleanup::Unknown => return Cleanup::Unknown,
            Cleanup::Survivors(n) => {
                if Instant::now() >= deadline {
                    return Cleanup::Survivors(n);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(not(unix))]
pub(crate) fn stop_group(_pgid: i64, _bound: Duration) -> Cleanup {
    Cleanup::Unknown
}

/// What the process table says about a supervised run (`Q12`, pid reuse).
///
/// `ps -p <pid> -o pgid=` is the whole reading. Three outcomes, and the third
/// is the one that matters: a live pid whose process group is not the one we
/// recorded is not our run, and saying "still running" about it would be the
/// product adopting a stranger.
pub(crate) fn liveness(pid: Option<i64>, pgid: Option<i64>) -> Liveness {
    let Some(pid) = pid else {
        // Nothing was recorded to ask about. The run cannot be shown as going.
        return Liveness::Gone;
    };
    let output = Command::new("ps")
        .arg("-p")
        .arg(pid.to_string())
        .arg("-o")
        .arg("pgid=")
        .stdin(Stdio::null())
        .output();
    let Ok(output) = output else {
        return Liveness::Reused;
    };
    if !output.status.success() {
        return Liveness::Gone;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let Some(found) = text
        .lines()
        .filter_map(|l| l.trim().parse::<i64>().ok())
        .next()
    else {
        return Liveness::Gone;
    };
    match pgid {
        // No group was recorded, so the second question cannot be asked and
        // the first one's answer is not enough to call it ours.
        None => Liveness::Reused,
        Some(group) if group == found => Liveness::Ours,
        Some(_) => Liveness::Reused,
    }
}

/// What one cancellation did, as both doors report it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Cancelled {
    pub(crate) run_key: String,
    pub(crate) name: String,
    pub(crate) cleanup: Cleanup,
}

/// Why a cancellation could not happen. Each is a different sentence, because
/// "there is no such run" and "that run already finished" are different facts
/// and a person acting on the wrong one wastes their time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancelRefusal {
    NoSuchRun,
    NotRunning,
}

impl CancelRefusal {
    pub(crate) fn slug(self) -> &'static str {
        match self {
            Self::NoSuchRun => "no_such_run",
            Self::NotRunning => "run_not_running",
        }
    }

    /// Sam's words. Neither half names a process or a signal.
    pub(crate) fn plain(self) -> &'static str {
        match self {
            Self::NoSuchRun => "There is no check by that name in the record.",
            Self::NotRunning => "That check is not running, so there is nothing to stop.",
        }
    }
}

/// **The one cancellation act, shared by both doors (`B12.6`, `Q12`).**
///
/// The CLI verb and the HTTP route call this and nothing else, so a run
/// stopped from a terminal and a run stopped from the screen leave records
/// that are the same shape and were made the same way. The order is the
/// requirement's: signal the group, wait a bounded time, **look**, and append
/// what was found — including when what was found is survivors.
///
/// A run that is not open is refused rather than re-cancelled: appending a
/// second ending to a run that already ended would be the record contradicting
/// itself about what became of one thing.
pub(crate) fn cancel(
    ledger: &thruline_core::ledger::Ledger,
    run_key: &str,
    actor: &str,
    reason: thruline_core::run_execution::Reason,
) -> anyhow::Result<std::result::Result<Cancelled, CancelRefusal>> {
    let Some(record) = thruline_core::run_execution::latest(ledger, run_key)? else {
        return Ok(Err(CancelRefusal::NoSuchRun));
    };
    if !record.state.is_open() {
        return Ok(Err(CancelRefusal::NotRunning));
    }
    let cleanup = match record.process_group {
        Some(pgid) => stop_group(pgid, STOP_BOUND),
        // Nothing was recorded to signal. The record says so rather than
        // claiming a clean stop it never verified.
        None => Cleanup::Unknown,
    };
    let (verified, survivors) = cleanup.as_record();
    thruline_core::run_execution::finish(
        ledger,
        &thruline_core::run_execution::Finish {
            run_key: run_key.to_string(),
            state: thruline_core::run_execution::ExecutionState::Cancelled,
            reason: Some(reason),
            actor: Some(actor.to_string()),
            binding_id: None,
            cleanup_verified: verified,
            survivors,
            at: thruline_core::ingest::iso8601_utc_now(),
        },
    )?;
    Ok(Ok(Cancelled {
        run_key: run_key.to_string(),
        name: record.name,
        cleanup,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A group with a child AND a grandchild: cleanup is a claim about the
    /// tree, so the test makes a tree (card acceptance 2).
    #[cfg(unix)]
    #[test]
    fn stopping_a_group_takes_the_grandchild_with_it_and_says_so() {
        use std::os::unix::process::CommandExt;

        let mut child = Command::new("/bin/sh")
            .arg("-c")
            // The grandchild outlives a signal sent to the child alone; only a
            // signal to the GROUP reaches it, which is the thing being proven.
            .arg("sleep 60 & sleep 60")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("a shell spawns");
        let pgid = i64::from(child.id());

        // Give the shell a moment to have actually forked the grandchild,
        // bounded so the test cannot hang.
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if matches!(group_survivors(pgid), Cleanup::Survivors(n) if n >= 2) {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(
            matches!(group_survivors(pgid), Cleanup::Survivors(n) if n >= 2),
            "the child and the grandchild must both be in the group before it is stopped"
        );

        assert_eq!(stop_group(pgid, Duration::from_secs(3)), Cleanup::Verified);
        assert_eq!(
            group_survivors(pgid),
            Cleanup::Verified,
            "nothing in the group may remain, grandchild included"
        );
        let _ = child.wait();
    }

    #[cfg(unix)]
    #[test]
    fn a_live_process_in_its_recorded_group_is_ours_and_a_stranger_is_never_adopted() {
        use std::os::unix::process::CommandExt;

        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg("sleep 30")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("a shell spawns");
        let pid = i64::from(child.id());

        assert_eq!(liveness(Some(pid), Some(pid)), Liveness::Ours);
        // The same live pid, recorded against a group it is not the leader of:
        // this is pid reuse as the supervisor meets it, and it is refused.
        assert_eq!(liveness(Some(pid), Some(pid + 1)), Liveness::Reused);
        // A live pid with no recorded group cannot be confirmed as ours.
        assert_eq!(liveness(Some(pid), None), Liveness::Reused);

        let _ = stop_group(pid, Duration::from_secs(3));
        let _ = child.wait();
        assert_eq!(
            liveness(Some(pid), Some(pid)),
            Liveness::Gone,
            "once the process is gone the record must say gone"
        );
    }

    /// RUN-STOP-PGID-GUARD: a recorded pgid that is not one owned group — 0,
    /// 1, negative, or HQ's own — is refused before any signal, and the
    /// cleanup reads Unknown, never Verified. Exercised through a recording
    /// fake only: `kill(-1)` from a mutant of this guard would reach every
    /// process the user owns, so no real signal is ever in play here.
    #[cfg(unix)]
    #[test]
    fn a_recorded_pgid_that_is_not_one_owned_group_is_refused_before_any_signal() {
        let own = i64::from(rustix::process::getpgrp().as_raw_nonzero().get());
        for pgid in [0, 1, -1, -2, i64::from(i32::MIN), i64::MIN, own] {
            let mut calls = Vec::new();
            let cleanup = stop_group_with(pgid, Duration::ZERO, |group, signal| {
                calls.push((group, signal));
                Ok(())
            });
            assert!(
                calls.is_empty(),
                "a signal was sent for recorded pgid {pgid}: {calls:?}"
            );
            assert_eq!(cleanup, Cleanup::Unknown, "recorded pgid {pgid}");
        }
    }

    /// The seam is the real path: a group this test owns reaches the fake,
    /// TERM first and then KILL, each with exactly that group. The fake
    /// delivers nothing, so the group is still there and the reading says so.
    /// `cat` ends when its stdin closes, so cleanup needs no signal.
    #[cfg(unix)]
    #[test]
    fn a_group_this_test_owns_reaches_the_signaller_term_then_kill() {
        use std::os::unix::process::CommandExt;

        let mut child = Command::new("/bin/cat")
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("cat spawns");
        let group = i32::try_from(child.id()).expect("a pid fits i32");
        let mut calls = Vec::new();
        let cleanup = stop_group_with(i64::from(group), Duration::ZERO, |g, signal| {
            calls.push((g, signal));
            Ok(())
        });
        drop(child.stdin.take());
        let _ = child.wait();
        assert_eq!(
            calls,
            vec![(group, StopSignal::Term), (group, StopSignal::Kill)]
        );
        assert_eq!(cleanup, Cleanup::Survivors(1));
    }

    #[test]
    fn a_run_with_no_recorded_process_is_gone_rather_than_running() {
        assert_eq!(liveness(None, Some(1)), Liveness::Gone);
    }

    #[test]
    fn the_three_cleanup_answers_reach_the_record_as_three_different_rows() {
        assert_eq!(Cleanup::Verified.as_record(), (Some(true), Some(0)));
        assert_eq!(Cleanup::Survivors(2).as_record(), (Some(false), Some(2)));
        assert_eq!(
            Cleanup::Unknown.as_record(),
            (None, None),
            "\"nothing looked\" must never be stored as \"looked and it was clean\""
        );
    }
}
