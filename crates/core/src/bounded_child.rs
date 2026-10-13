//! A child process with a time limit (SNAPSHOT-CHILD-TIMEOUT, founder ruling
//! D-051).
//!
//! `Command::output()` waits as long as the child takes. On the path that
//! writes a Change from the first screen, that wait holds a browser's request
//! open on `git` and `ps`, so a child that stalls stalls the request
//! (ADV-t71-1: four tests met their client's 60 s read with nothing sent).
//!
//! [`output_until`] runs one child in a process group of its own, with a
//! deadline:
//!
//! * A child that finishes in time answers exactly what `Command::output()`
//!   would have answered.
//! * A child that does not finish is killed together with its whole group, so
//!   anything it started goes with it. It is reaped, and the answer is
//!   [`Outcome::TimedOut`]. That answer carries **no output**: a partial read is
//!   not a read. The caller records the absence and never a value.
//!
//! HQ's shutdown guard stops these owned groups on SIGINT/SIGTERM and Rust
//! unwind. A stopped read returns `io::ErrorKind::Interrupted`, never partial
//! output or a fabricated deadline expiry. A guard cannot run after SIGKILL.
//! Normal completion also stops descendants before releasing the leader's pid.
//!
//! No `unsafe`. The group is signalled through `rustix`'s safe wrapper, as
//! `run_supervisor::stop_group` does in the CLI.

use std::process::{Child, Command, Output};
use std::time::{Duration, Instant};

use crate::error::CoreError;

/// How long one read of this machine may take — one `git` call through
/// `capture::git_output`, or one whole workspace snapshot — before it is
/// recorded as not read.
///
/// 20 s is a liveness bound, not a speed bound. An idle `git status` or
/// `ps -axo` here takes milliseconds. The limit is reached only by a child that
/// is not going to answer in any time a person holding a request open would
/// wait, and it sits well under the 60 s the first-use client waits.
pub const TIME_LIMIT: Duration = Duration::from_secs(20);

/// After the group is killed, how long the call waits to reap the child before
/// it answers anyway. `SIGKILL` cannot be caught, so this is reached only by a
/// child the kernel is holding in an uninterruptible wait. The waiting thread
/// still reaps it when the kernel lets go.
const REAP_BOUND: Duration = Duration::from_secs(2);

/// What a bounded child produced.
#[derive(Debug)]
pub enum Outcome {
    /// It finished within the limit. This is what `Command::output()` returns.
    Finished(Output),
    /// It did not finish within the limit, or the limit had already passed and
    /// it was never started. Nothing it wrote is kept.
    TimedOut,
}

/// Run `command` to completion or until `deadline`, whichever comes first.
///
/// The child's stdin is null and its stdout and stderr are captured, as with
/// `Command::output()`. A deadline that has already passed starts nothing:
/// a read that the limit left no time for was not read.
pub fn output_until(command: &mut Command, deadline: Instant) -> std::io::Result<Outcome> {
    if Instant::now() >= deadline {
        return Ok(Outcome::TimedOut);
    }
    run(command, deadline)
}

/// The error a caller returns when a read it cannot do without timed out.
/// It is an `Io` error of kind `TimedOut`, so [`is_timeout`] can tell it from a
/// read that failed. `what` names the read, e.g. `git status --porcelain -z`.
pub fn timed_out(what: &str, limit: Duration) -> CoreError {
    CoreError::Io {
        path: what.to_string(),
        source: std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("did not finish within {} s", limit.as_secs()),
        ),
    }
}

/// Whether `err` is a read that did not finish in time, as opposed to one that
/// finished and failed.
pub fn is_timeout(err: &CoreError) -> bool {
    matches!(err, CoreError::Io { source, .. } if source.kind() == std::io::ErrorKind::TimedOut)
}

/// HQ has one lifecycle per process. Closing is deliberately irreversible:
/// a request racing with shutdown must never reopen the right to spawn.
/// The guard also closes on an early return or Rust unwind, not on SIGKILL.
pub struct ShutdownGuard {
    active: bool,
}

pub fn shutdown_guard() -> ShutdownGuard {
    ShutdownGuard { active: true }
}

impl ShutdownGuard {
    /// Stop owned reads and wait a bounded time for their owners to reap them.
    /// False means the bound expired; it is never a claim of verified cleanup.
    pub fn stop(&mut self) -> bool {
        if !self.active {
            return true;
        }
        self.active = false;
        #[cfg(unix)]
        {
            {
                let mut registry = registry();
                registry.closing = true;
                for &raw in &registry.children {
                    kill_group(raw);
                }
            }
            let until = Instant::now() + REAP_BOUND;
            loop {
                if registry().children.is_empty() {
                    return true;
                }
                if Instant::now() >= until {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        #[cfg(not(unix))]
        true
    }
}

impl Drop for ShutdownGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = self.stop();
        }
    }
}

#[cfg(unix)]
#[derive(Default)]
struct Registry {
    closing: bool,
    children: std::collections::BTreeSet<u32>,
}

#[cfg(unix)]
fn registry() -> std::sync::MutexGuard<'static, Registry> {
    static REGISTRY: std::sync::LazyLock<std::sync::Mutex<Registry>> =
        std::sync::LazyLock::new(|| std::sync::Mutex::new(Registry::default()));
    // Poison does not relinquish process ownership. Recover it to clean up.
    REGISTRY
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(unix)]
fn interrupted() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::Interrupted,
        "HQ stopped while reading the workspace",
    )
}

#[cfg(unix)]
fn process_id(raw: u32) -> std::io::Result<rustix::process::Pid> {
    i32::try_from(raw)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
        .ok_or_else(|| std::io::Error::other("child pid is not representable"))
}

/// Why a process-group signal was not sent.
#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupSignalError {
    /// The target is 0, 1 or negative: to `kill(2)` those mean "my own group",
    /// "init" or "every process I may signal", never one owned child's group.
    NotAGroup(i32),
    /// The target is the caller's own process group, i.e. HQ itself.
    OwnGroup(i32),
    /// The target was allowed and the signal call failed.
    Failed(rustix::io::Errno),
}

/// The one door every process-group signal in the product goes through
/// (SNAP-PGID-GUARD here, RUN-STOP-PGID-GUARD for the run supervisor and the
/// run route). `signaller` is the seam: production passes [`kill_group_now`]
/// or [`term_group_now`], a test passes a recording fake so no real signal
/// is ever delivered.
#[cfg(unix)]
pub fn signal_group(
    group: i32,
    signaller: impl FnOnce(i32) -> rustix::io::Result<()>,
) -> Result<(), GroupSignalError> {
    if group <= 1 {
        return Err(GroupSignalError::NotAGroup(group));
    }
    if group == rustix::process::getpgrp().as_raw_nonzero().get() {
        return Err(GroupSignalError::OwnGroup(group));
    }
    signaller(group).map_err(GroupSignalError::Failed)
}

/// The real signaller: `SIGKILL` to one process group. Reached only through
/// [`signal_group`], which has already refused every forbidden target; the
/// `Pid` conversion refuses a non-positive value again rather than trust it.
#[cfg(unix)]
pub fn kill_group_now(group: i32) -> rustix::io::Result<()> {
    send_to_group(group, rustix::process::Signal::KILL)
}

/// `SIGTERM` to one process group: the polite first step of a stop. Reached
/// only through [`signal_group`], exactly as [`kill_group_now`] is.
#[cfg(unix)]
pub fn term_group_now(group: i32) -> rustix::io::Result<()> {
    send_to_group(group, rustix::process::Signal::TERM)
}

#[cfg(unix)]
fn send_to_group(group: i32, signal: rustix::process::Signal) -> rustix::io::Result<()> {
    let pid = if group > 1 {
        rustix::process::Pid::from_raw(group)
    } else {
        None
    };
    let pid = pid.ok_or(rustix::io::Errno::INVAL)?;
    rustix::process::kill_process_group(pid, signal)
}

/// OFFLINE-KILL-GUARD (founder ruling R-2026-09-28-7): the one door for a
/// kill owned by another core module. It takes the spawned `Child` by type, so
/// no pid can arrive from a row, a payload or a user, and it signals that
/// child's process group ONLY through [`signal_group`], which refuses pgid
/// <= 1 and HQ's own group before any syscall. The caller must have spawned the
/// child as its own group leader (`process_group(0)`) and keep it unreaped
/// until after this call, so the group id cannot have been reused. A refusal
/// is returned as an error, never reported as a stop.
#[cfg(unix)]
pub(crate) fn kill_owned_group(child: &mut Child) -> std::io::Result<()> {
    let group = i32::try_from(child.id())
        .map_err(|_| std::io::Error::other("child pid is not representable"))?;
    signal_group(group, kill_group_now).map_err(|refusal| match refusal {
        GroupSignalError::Failed(errno) => errno.into(),
        other => std::io::Error::other(format!("group signal refused: {other:?}")),
    })
}

/// Off unix there is no process group to own, and this product does not ship
/// there: nothing is signalled, and the caller is told so.
#[cfg(not(unix))]
pub(crate) fn kill_owned_group(child: &mut Child) -> std::io::Result<()> {
    let _ = child;
    Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
}

/// Caller holds the registry lock. An unreaped Child reserves every listed
/// pid, so this can never signal a new process that reused an old child's id.
#[cfg(unix)]
fn kill_group(raw: u32) {
    // A refusal or failure is not a stop: nothing here reports one as such.
    if let Ok(group) = i32::try_from(raw) {
        let _ = signal_group(group, kill_group_now);
    }
}

/// Observe exit without reaping: a leader can exit while a descendant still
/// holds an output pipe. Its pid stays reserved until the group is released.
#[cfg(unix)]
fn exited(raw: u32) -> std::io::Result<bool> {
    use rustix::process::{waitid, WaitId, WaitIdOptions};
    loop {
        match waitid(
            WaitId::Pid(process_id(raw)?),
            WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG,
        ) {
            Ok(status) => return Ok(status.is_some()),
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(error.into()),
        }
    }
}

#[cfg(unix)]
struct OwnedChild {
    child: Option<std::process::Child>,
    raw: u32,
}

#[cfg(unix)]
impl OwnedChild {
    fn spawn(command: &mut Command) -> std::io::Result<Self> {
        use std::os::unix::process::CommandExt;
        use std::process::Stdio;
        let mut registry = registry();
        if registry.closing {
            return Err(interrupted());
        }
        let child = command
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let raw = child.id();
        registry.children.insert(raw);
        Ok(Self {
            child: Some(child),
            raw,
        })
    }

    /// Only after waitid observed exit. Reap and unregister atomically with
    /// respect to shutdown's group signal; no stale pid can enter the registry.
    fn reap(&mut self) -> std::io::Result<std::process::ExitStatus> {
        let mut registry = registry();
        // Even with both pipes closed, an owned descendant may still run.
        // The unreaped leader reserves the group id until this signal.
        kill_group(self.raw);
        let status = self
            .child
            .as_mut()
            .ok_or_else(|| std::io::Error::other("child already reaped"))?
            .wait();
        registry.children.remove(&self.raw);
        self.child = None;
        if registry.closing {
            Err(interrupted())
        } else {
            status
        }
    }
}

#[cfg(unix)]
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.child.is_none() {
            return;
        }
        {
            let _registry = registry();
            kill_group(self.raw);
        }
        let until = Instant::now() + REAP_BOUND;
        while Instant::now() < until {
            if exited(self.raw).unwrap_or(false) {
                let _ = self.reap();
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        // An uninterruptible child must not hold its caller forever. Its
        // reaper retains BOTH the Child and registration until it can reap;
        // unlike the previous detached wait(), it never exposes a reused pid.
        if let Some(mut child) = self.child.take() {
            let raw = self.raw;
            std::thread::spawn(move || {
                while !exited(raw).unwrap_or(false) {
                    std::thread::sleep(Duration::from_millis(10));
                }
                let mut registry = registry();
                let _ = child.wait();
                registry.children.remove(&raw);
            });
        }
    }
}

enum Event {
    Stdout(std::io::Result<Vec<u8>>),
    Stderr(std::io::Result<Vec<u8>>),
}

#[cfg(unix)]
fn run(command: &mut Command, deadline: Instant) -> std::io::Result<Outcome> {
    use std::io::Read;
    use std::sync::mpsc;

    let mut owner = OwnedChild::spawn(command)?;
    let (tx, rx) = mpsc::channel::<Event>();
    let child = owner
        .child
        .as_mut()
        .ok_or_else(|| std::io::Error::other("spawned child was lost"))?;
    let (mut stdout, mut stderr) = (None, None);
    if let Some(mut pipe) = child.stdout.take() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = tx.send(Event::Stdout(pipe.read_to_end(&mut bytes).map(|_| bytes)));
        });
    } else {
        stdout = Some(Vec::new());
    }
    if let Some(mut pipe) = child.stderr.take() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = tx.send(Event::Stderr(pipe.read_to_end(&mut bytes).map(|_| bytes)));
        });
    } else {
        stderr = Some(Vec::new());
    }
    drop(tx);
    loop {
        if registry().closing {
            return Err(interrupted());
        }
        if Instant::now() >= deadline {
            return Ok(Outcome::TimedOut);
        }
        if stdout.is_some() && stderr.is_some() && exited(owner.raw)? {
            return Ok(Outcome::Finished(Output {
                status: owner.reap()?,
                stdout: stdout.take().unwrap_or_default(),
                stderr: stderr.take().unwrap_or_default(),
            }));
        }
        let left = deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(10));
        match rx.recv_timeout(left) {
            Ok(Event::Stdout(bytes)) => stdout = Some(bytes?),
            Ok(Event::Stderr(bytes)) => stderr = Some(bytes?),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            // The pipes can close before the leader exits. Poll exit without
            // spinning, and still observe shutdown and the shared deadline.
            Err(mpsc::RecvTimeoutError::Disconnected) => std::thread::sleep(left),
        }
    }
}

/// Off unix there is no process group to own, and this product does not ship
/// there. The call keeps `Command::output()`'s behaviour rather than
/// pretending to a bound it cannot enforce.
#[cfg(not(unix))]
fn run(command: &mut Command, _deadline: Instant) -> std::io::Result<Outcome> {
    command.output().map(Outcome::Finished)
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    fn alive(pid: &str) -> bool {
        Command::new("kill")
            .args(["-0", pid])
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// The child starts a grandchild in its own group and waits on it. At the
    /// limit both are killed; the call answers `TimedOut` promptly; neither
    /// process is left behind.
    #[test]
    fn a_child_and_what_it_started_are_killed_and_reaped_at_the_limit() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pids = dir.path().join("pids");
        let started = Instant::now();
        let outcome = output_until(
            Command::new("/bin/sh")
                .args([
                    "-c",
                    "echo $$ >> \"$1\"; sleep 30 & echo $! >> \"$1\"; wait",
                ])
                .arg("sh")
                .arg(&pids),
            Instant::now() + Duration::from_secs(5),
        )
        .expect("spawn");
        let took = started.elapsed();
        assert!(matches!(outcome, Outcome::TimedOut), "{outcome:?}");
        assert!(took < Duration::from_secs(10), "answered after {took:?}");

        let text = std::fs::read_to_string(&pids).expect("pids");
        let pids: Vec<&str> = text.lines().collect();
        assert_eq!(pids.len(), 2, "{text:?}");
        let until = Instant::now() + Duration::from_secs(5);
        while pids.iter().any(|p| alive(p)) {
            assert!(Instant::now() < until, "left behind: {pids:?}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// A child that finishes in time answers exactly what `Command::output()`
    /// answers: status, stdout and stderr.
    #[test]
    fn a_child_that_finishes_answers_exactly_what_output_would() {
        let script = "printf 'out\\n'; printf 'err' >&2; exit 3";
        let plain = Command::new("/bin/sh")
            .args(["-c", script])
            .output()
            .expect("output");
        let Outcome::Finished(bounded) = output_until(
            Command::new("/bin/sh").args(["-c", script]),
            Instant::now() + Duration::from_secs(10),
        )
        .expect("spawn") else {
            panic!("a child that exits at once timed out");
        };
        assert_eq!(bounded.status.code(), plain.status.code());
        assert_eq!(bounded.stdout, plain.stdout);
        assert_eq!(bounded.stderr, plain.stderr);
    }

    /// A deadline that has already passed starts nothing.
    #[test]
    fn a_deadline_already_passed_starts_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join("ran");
        let outcome = output_until(
            Command::new("/bin/sh")
                .args(["-c", "touch \"$1\""])
                .arg("sh")
                .arg(&marker),
            Instant::now(),
        )
        .expect("no spawn");
        assert!(matches!(outcome, Outcome::TimedOut));
        assert!(
            !marker.exists(),
            "a read the limit left no time for was run"
        );
    }

    /// A leader may exit while its descendant keeps the output pipes open.
    /// The group must remain owned until those pipes finish or the bound fires.
    #[test]
    fn a_dead_leader_does_not_release_its_pipe_holding_descendant() {
        struct Cleanup(Option<std::path::PathBuf>);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let Some(path) = &self.0 else { return };
                if let Some(raw) = std::fs::read_to_string(path)
                    .ok()
                    .and_then(|text| text.lines().next().and_then(|n| n.parse::<u32>().ok()))
                {
                    kill_group(raw);
                }
            }
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join("pids");
        let mut cleanup = Cleanup(Some(marker.clone()));
        let outcome = output_until(
            Command::new("/bin/sh")
                .args([
                    "-c",
                    "echo $$ > \"$1\"; sleep 30 & echo $! >> \"$1\"; exit 0",
                ])
                .arg("sh")
                .arg(&marker),
            Instant::now() + Duration::from_millis(500),
        )
        .expect("spawn");
        assert!(matches!(outcome, Outcome::TimedOut), "{outcome:?}");
        let text = std::fs::read_to_string(&marker).expect("pids");
        let pids: Vec<_> = text.lines().collect();
        assert_eq!(pids.len(), 2, "{text}");
        let until = Instant::now() + Duration::from_secs(5);
        while pids.iter().any(|p| alive(p)) {
            assert!(
                Instant::now() < until,
                "left behind after leader exit: {text}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        cleanup.0 = None;
    }

    #[test]
    fn a_finished_leader_does_not_abandon_a_descendant_with_closed_pipes() {
        struct Cleanup(Option<std::path::PathBuf>);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                if let Some(path) = &self.0 {
                    if let Some(raw) = std::fs::read_to_string(path)
                        .ok()
                        .and_then(|text| text.lines().next().and_then(|n| n.parse::<i32>().ok()))
                    {
                        let _ = signal_group(raw, kill_group_now);
                    }
                }
            }
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let marker = dir.path().join("pids");
        let mut cleanup = Cleanup(Some(marker.clone()));
        let Outcome::Finished(output) = output_until(Command::new("/bin/sh")
            .args(["-c", "echo $$ > \"$1\"; sleep 30 </dev/null >/dev/null 2>&1 & echo $! >> \"$1\"; printf out; printf err >&2; exit 3"])
            .arg("sh").arg(&marker), Instant::now() + Duration::from_secs(5)).expect("spawn") else {
            panic!("a finished leader was reported as timed out");
        };
        assert_eq!(output.status.code(), Some(3));
        assert_eq!(output.stdout, b"out");
        assert_eq!(output.stderr, b"err");
        let text = std::fs::read_to_string(&marker).expect("pids");
        let pids: Vec<_> = text.lines().collect();
        assert_eq!(pids.len(), 2, "{text}");
        let group =
            rustix::process::Pid::from_raw(pids[0].parse().expect("group")).expect("group id");
        let until = Instant::now() + Duration::from_secs(3);
        while rustix::process::test_kill_process_group(group) != Err(rustix::io::Errno::SRCH)
            || pids.iter().any(|p| alive(p))
        {
            assert!(
                Instant::now() < until,
                "finished read abandoned its group: {text}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        cleanup.0 = None;
    }

    /// SNAP-PGID-GUARD: the kill path refuses every group <= 1 and HQ's own
    /// group before any signal call. Exercised through a recording fake, so
    /// neither this test nor a mutant of the guard can deliver a real signal.
    #[test]
    fn the_group_signal_refuses_forbidden_targets_before_the_signaller() {
        let own = rustix::process::getpgrp().as_raw_nonzero().get();
        let forbidden = [
            (0, GroupSignalError::NotAGroup(0)),
            (1, GroupSignalError::NotAGroup(1)),
            (-1, GroupSignalError::NotAGroup(-1)),
            (i32::MIN, GroupSignalError::NotAGroup(i32::MIN)),
            (own, GroupSignalError::OwnGroup(own)),
        ];
        for (group, refusal) in forbidden {
            let mut calls = Vec::new();
            let result = signal_group(group, |g| {
                calls.push(g);
                Ok(())
            });
            assert!(calls.is_empty(), "signaller reached for group {group}");
            assert_eq!(result, Err(refusal), "group {group}");
        }

        // A group this test spawned itself: signalled exactly once, with it.
        // `cat` exits when its stdin closes, so cleanup needs no signal.
        let mut child = {
            use std::os::unix::process::CommandExt;
            Command::new("/bin/cat")
                .process_group(0)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .spawn()
                .expect("spawn cat")
        };
        let group = i32::try_from(child.id()).expect("pid fits i32");
        let mut calls = Vec::new();
        let result = signal_group(group, |g| {
            calls.push(g);
            Ok(())
        });
        drop(child.stdin.take());
        let _ = child.wait();
        assert_eq!(result, Ok(()));
        assert_eq!(calls, vec![group]);
    }

    #[test]
    fn a_timeout_error_is_told_apart_from_a_failure() {
        assert!(is_timeout(&timed_out("ps", TIME_LIMIT)));
        assert!(!is_timeout(&CoreError::Config("git status failed".into())));
        assert!(!is_timeout(&CoreError::io(
            "git",
            std::io::Error::from(std::io::ErrorKind::NotFound)
        )));
    }
}
