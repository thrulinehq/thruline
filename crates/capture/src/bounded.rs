//! One explicitly requested local check, with bounded time and output.
//! Shares the recorder's redaction and output store. It is not a sandbox:
//! descendants can escape a process group, and external inputs are not closed.

#[cfg(unix)]
use std::io::Read;
use std::path::Path;
#[cfg(unix)]
use std::process::Stdio;
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(unix)]
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;
#[cfg(unix)]
use std::time::Instant;

use crate::redaction::Redactor;
#[cfg(unix)]
use crate::stream::CaptureSink;
use crate::stream::Captured;

pub const OUTPUT_LIMIT: u64 = 8 * 1024 * 1024;

pub struct Stream {
    pub captured: Captured,
    pub complete: bool,
}

pub struct Report {
    pub status: ExitStatus,
    pub timed_out: bool,
    pub cancelled: bool,
    pub stdout: Option<Stream>,
    pub stderr: Option<Stream>,
    pub resources: ResourceObservation,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ResourceObservation {
    pub elapsed_ms: u64,
    pub sampled_peak_rss_bytes: Option<u64>,
    pub samples: u64,
    pub scope: String,
}

#[cfg(unix)]
struct OwnedGroup(std::process::Child);
#[cfg(unix)]
impl std::ops::Deref for OwnedGroup {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
#[cfg(unix)]
impl std::ops::DerefMut for OwnedGroup {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
#[cfg(unix)]
impl Drop for OwnedGroup {
    fn drop(&mut self) {
        // Also runs on capture errors and normal parent exit. A child retaining
        // a pipe or continuing in the owned group must not outlive this capture.
        let _ = Command::new("/bin/kill")
            .args(["-KILL", "--", &format!("-{}", self.0.id())])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(unix)]
fn sample_rss(pid: u32) -> Option<u64> {
    // This is an observed lower bound, not an OS-enforced memory ceiling.
    // Only the launched process is measured; child processes are excluded.
    let mut child = Command::new("/bin/ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_millis(150);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let mut output = Vec::new();
    child
        .stdout
        .take()?
        .take(129)
        .read_to_end(&mut output)
        .ok()?;
    if !status.success() || output.len() > 128 {
        return None;
    }
    String::from_utf8(output)
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?
        .checked_mul(1024)
}

#[cfg(unix)]
fn drain(reader: impl Read + Send + 'static, sink: CaptureSink) -> Receiver<Stream> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = reader.take(OUTPUT_LIMIT + 1);
        let mut sink = sink;
        let mut bytes = [0; 16 * 1024];
        let mut complete = true;
        loop {
            match reader.read(&mut bytes) {
                Ok(0) => break,
                Ok(n) => sink.push(&bytes[..n]),
                Err(_) => {
                    complete = false;
                    break;
                }
            }
        }
        let captured = sink.finish();
        complete &= captured.bytes_raw <= OUTPUT_LIMIT;
        let _ = sender.send(Stream { captured, complete });
    });
    receiver
}

/// No shell interpretation, inherited stdin or retries. Each stream is capped
/// at 8 MiB. The return remains bounded if a detached descendant holds a pipe.
#[cfg(unix)]
pub fn run(
    command: Command,
    timeout: Duration,
    redactor: Redactor,
    store: &Path,
) -> std::io::Result<Report> {
    capture(
        command,
        timeout,
        redactor,
        store,
        &AtomicBool::new(false),
        None,
    )
}

/// Fixed shell prologue sets inherited CPU, file-size, descriptor and core
/// limits. Program and arguments are passed as argv, never interpolated into
/// shell text. This is resource bounding, not hostile-code isolation.
#[cfg(unix)]
pub fn run_controlled(
    command: Command,
    timeout: Duration,
    redactor: Redactor,
    store: &Path,
    cancel: &AtomicBool,
    input: Option<Vec<u8>>,
) -> std::io::Result<Report> {
    if cancel.load(Ordering::SeqCst) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "cancelled before launch",
        ));
    }
    let mut limited = Command::new("/bin/sh");
    limited.args(["-c", "ulimit -t \"$1\" && ulimit -f 65536 && ulimit -n 128 && ulimit -c 0 || exit 125; shift; exec \"$@\"", "thruline-bounded"])
        .arg(timeout.as_secs().max(1).to_string())
        .arg(command.get_program()).args(command.get_args());
    if let Some(cwd) = command.get_current_dir() {
        limited.current_dir(cwd);
    }
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            limited.env(key, value);
        } else {
            limited.env_remove(key);
        }
    }
    capture(limited, timeout, redactor, store, cancel, input)
}

#[cfg(unix)]
fn capture(
    mut command: Command,
    timeout: Duration,
    redactor: Redactor,
    store: &Path,
    cancel: &AtomicBool,
    input: Option<Vec<u8>>,
) -> std::io::Result<Report> {
    use std::io::Write;
    use std::os::unix::process::CommandExt;
    command
        .process_group(0)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = OwnedGroup(command.spawn()?);
    if let (Some(mut stdin), Some(bytes)) = (child.stdin.take(), input) {
        std::thread::spawn(move || {
            let _ = stdin.write_all(&bytes);
        });
    }
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("missing stdout pipe"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other("missing stderr pipe"))?;
    let out = drain(
        stdout,
        CaptureSink::new(redactor.clone(), Some(store), "out"),
    );
    let err = drain(stderr, CaptureSink::new(redactor, Some(store), "err"));
    let started = Instant::now();
    let mut timed_out = false;
    let mut cancelled = false;
    let mut peak = None;
    let mut samples = 0;
    let mut next_sample = Duration::ZERO;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= timeout || cancel.load(Ordering::SeqCst) {
            cancelled = cancel.load(Ordering::SeqCst);
            timed_out = !cancelled;
            // A group created for this child only; never signal the caller's
            // process group. Direct kill is a fallback if /bin/kill fails.
            let _ = Command::new("/bin/kill")
                .args(["-KILL", &format!("-{}", child.id())])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            let _ = child.kill();
            break child.wait()?;
        }
        if started.elapsed() >= next_sample {
            if let Some(bytes) = sample_rss(child.id()) {
                peak = Some(peak.unwrap_or(0).max(bytes));
                samples += 1;
            }
            next_sample = started.elapsed() + Duration::from_millis(200);
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    // One shared drain deadline: a surviving descendant cannot hold this
    // caller indefinitely, and missing EOF is never complete capture.
    let deadline = Instant::now() + Duration::from_millis(500);
    let stdout = out
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .ok();
    let stderr = err
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .ok();
    Ok(Report {
        status,
        timed_out,
        cancelled,
        stdout,
        stderr,
        resources: ResourceObservation { elapsed_ms: started.elapsed().as_millis() as u64,
            sampled_peak_rss_bytes: peak, samples,
            scope: "Sampled launched-process RSS, every 200 ms while observable; descendants and between-sample peaks are excluded. Missing samples mean unknown memory use. No hard RAM ceiling.".into() },
    })
}

#[cfg(not(unix))]
pub fn run_controlled(
    _command: Command,
    _timeout: Duration,
    _redactor: Redactor,
    _store: &Path,
    _cancel: &AtomicBool,
    _input: Option<Vec<u8>>,
) -> std::io::Result<Report> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "bounded local checks require Unix",
    ))
}

#[cfg(not(unix))]
pub fn run(
    _command: Command,
    _timeout: Duration,
    _redactor: Redactor,
    _store: &Path,
) -> std::io::Result<Report> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "bounded local checks require Unix process groups on this build",
    ))
}

#[cfg(all(test, unix))]
mod controlled_tests {
    use super::*;
    #[test]
    fn explicit_cancel_stops_capture_before_deadline() {
        let directory = tempfile::tempdir().expect("temporary store");
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let request = std::sync::Arc::clone(&cancel);
        let sender = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            request.store(true, Ordering::SeqCst);
        });
        let mut command = Command::new("/bin/sleep");
        command.arg("20");
        let start = Instant::now();
        let report = run_controlled(
            command,
            Duration::from_secs(10),
            Redactor::from_repo(directory.path()).expect("redactor"),
            directory.path(),
            &cancel,
            None,
        )
        .expect("capture");
        sender.join().expect("cancellation request");
        assert!(report.cancelled);
        assert!(!report.timed_out);
        assert!(start.elapsed() < Duration::from_secs(3));
    }
    #[test]
    fn normal_parent_exit_cleans_an_owned_descendant() {
        let directory = tempfile::tempdir().expect("temporary store");
        let pidfile = directory.path().join("child.pid");
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            "sleep 20 >/dev/null 2>&1 & echo $! > \"$1\"",
            "owned-child",
        ]);
        command.arg(&pidfile);
        let report = run_controlled(
            command,
            Duration::from_secs(5),
            Redactor::from_repo(directory.path()).expect("redactor"),
            directory.path(),
            &AtomicBool::new(false),
            None,
        )
        .expect("capture");
        assert_eq!(report.status.code(), Some(0));
        let pid = std::fs::read_to_string(pidfile).expect("observed child pid");
        let deadline = Instant::now() + Duration::from_secs(2);
        while Command::new("/bin/kill")
            .args(["-0", pid.trim()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("process observation")
            .success()
        {
            assert!(
                Instant::now() < deadline,
                "owned descendant survived completed capture: {pid}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
