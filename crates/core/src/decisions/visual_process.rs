//! Bound child capture output and stream closed diagnostic progress lines.
use super::{visual_progress, VisualProgressObserver};
use std::io::Read;
use std::process::{Child, Command, Output, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
struct Guard(Child);
impl Drop for Guard {
    fn drop(&mut self) {
        stop(&mut self.0);
    }
}
fn stop(child: &mut Child) {
    #[cfg(unix)]
    if let Ok(group) = i32::try_from(child.id()) {
        let _ = crate::bounded_child::signal_group(group, crate::bounded_child::kill_group_now);
    }
    let _ = child.kill();
    let _ = child.wait();
}
pub fn capture_process(
    command: &mut Command,
    total: usize,
    observer: Option<VisualProgressObserver>,
    timeout: Duration,
) -> Result<Output, &'static str> {
    if total == 0 || total > 64 {
        return Err("capture progress total is invalid");
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = Guard(command.spawn().map_err(|_| "capture runtime unavailable")?);
    let stdout = child.0.stdout.take().ok_or("capture output unavailable")?;
    let stderr = child
        .0
        .stderr
        .take()
        .ok_or("capture progress unavailable")?;
    let bad = Arc::new(AtomicBool::new(false));
    let stdout_bad = bad.clone();
    let out = std::thread::spawn(move || read_bounded(stdout, 8 * 1024 * 1024, &stdout_bad, None));
    let stderr_bad = bad.clone();
    let err = std::thread::spawn(move || {
        let mut progress = 0;
        let mut pending = Vec::new();
        let mut report = |chunk: &[u8]| {
            for byte in chunk {
                if *byte == b'\n' {
                    if let Ok(line) = std::str::from_utf8(&pending) {
                        if let Some(rest) = line.trim_end_matches('\r').strip_prefix("Captured ") {
                            let next = rest.split_once(" of ").and_then(|(a, b)| {
                                Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?))
                            });
                            if let Some((n, expected)) =
                                next.filter(|(n, t)| *t == total && *n == progress + 1 && *n <= *t)
                            {
                                progress = n;
                                visual_progress::emit_to(
                                    observer.as_ref(),
                                    n,
                                    expected,
                                    format!("{n} of {expected} screens"),
                                );
                            } else {
                                stderr_bad.store(true, Ordering::Relaxed);
                            }
                        }
                    } else {
                        stderr_bad.store(true, Ordering::Relaxed)
                    }
                    pending.clear();
                } else {
                    pending.push(*byte);
                    if pending.len() > 4096 {
                        stderr_bad.store(true, Ordering::Relaxed);
                        pending.clear();
                    }
                }
            }
        };
        let bytes = read_bounded(stderr, 128 * 1024, &stderr_bad, Some(&mut report));
        (bytes, progress)
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        if bad.load(Ordering::Relaxed) {
            stop(&mut child.0);
            let _ = out.join();
            let _ = err.join();
            return Err("capture output or progress exceeded its bound");
        };
        match child.0.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                stop(&mut child.0);
                let _ = out.join();
                let _ = err.join();
                return Err("capture exceeded its deadline");
            }
        }
    };
    // Stop any descendant that kept a pipe open after the parent exited.
    stop(&mut child.0);
    let stdout = out.join().map_err(|_| "capture output reader failed")?;
    let (stderr, progress) = err.join().map_err(|_| "capture progress reader failed")?;
    if bad.load(Ordering::Relaxed) {
        return Err("capture output or progress exceeded its bound");
    };
    if status.success() && progress != total {
        return Err("capture progress did not reach all screens");
    };
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}
type ChunkObserver<'a> = &'a mut dyn FnMut(&[u8]);
fn read_bounded(
    mut reader: impl Read,
    limit: usize,
    bad: &AtomicBool,
    mut callback: Option<ChunkObserver<'_>>,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if bytes.len() + n > limit {
                    bad.store(true, Ordering::Relaxed);
                    break;
                };
                if let Some(cb) = callback.as_deref_mut() {
                    cb(&buf[..n])
                };
                bytes.extend_from_slice(&buf[..n]);
            }
            Err(_) => {
                bad.store(true, Ordering::Relaxed);
                break;
            }
        }
    }
    bytes
}
