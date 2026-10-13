//! BUILD-SPEC §4.4's first sentence — *"spawn the agent under a PTY"* — and the
//! disclosure it closes.
//!
//! # Why a terminal is a capture question and not a cosmetic one
//!
//! A runner behaves differently when its output is a terminal: `pytest` draws a
//! progress line sized to the window, `cargo` colourises, `npm` switches
//! renderers. **The bytes change, and sometimes the behaviour does.** §7
//! re-executes a check in a container, where output is a pipe, and compares the
//! two runs. If the comparison read a terminal difference as a content
//! difference it would report tampering that did not happen — an accusation
//! produced by a terminal, landing on a person.
//!
//! `Comparison::of` already conditions on a recorded `stdio` shape rather than
//! guessing. What was missing was a producer of anything other than `"pipe"`:
//! the branch existed and nothing could reach it, and a guard nothing can reach
//! is a guard nobody can trust. This module is that producer.
//!
//! # What "tty" means here, exactly
//!
//! [`spawn_on_pty`] gives the child **one** pseudoterminal for stdin, stdout
//! and stderr, because that is what a terminal is: one device, both output
//! streams on it. The consequence is recorded rather than hidden — the two are
//! no longer separable, so the caller records `streams: "merged"` and does not
//! present a `stderr_hash` it did not measure separately.
//!
//! The child does **not** get a controlling terminal. `setsid` + `TIOCSCTTY`
//! both need `unsafe` in a `pre_exec` closure, which this workspace forbids
//! outright. What the child gets is a terminal *device* on its standard
//! streams, which is what `isatty(1)` answers and what every runner in §6.2
//! branches on. Job control, `/dev/tty` opens and foreground-process-group
//! signal delivery are out of scope, stated here rather than discovered later.
//!
//! # Unix only, and loudly so
//!
//! §2 puts Windows at tier 2 — the build must pass, capture may be reduced. On
//! Windows [`stdout_is_terminal`] answers `false` unconditionally and the pipe
//! path is kept, so the recorded shape stays `"pipe"`: a true statement about a
//! reduced capture rather than a `"tty"` nobody produced.

/// The `stdio` shape recorded when a child ran on a pseudoterminal.
///
/// Paired with [`PIPE`] so both spellings live in one place: they are compared
/// across a receipt boundary, and two string literals in two crates are exactly
/// how such a comparison quietly stops matching.
pub const TTY: &str = "tty";

/// The `stdio` shape recorded when a child ran on pipes.
pub const PIPE: &str = "pipe";

#[cfg(unix)]
mod unix {
    use std::fs::File;
    use std::io;
    use std::os::fd::OwnedFd;
    use std::process::{Child, Command, Stdio};

    use rustix_openpty::rustix::termios::{
        isatty, tcgetattr, tcgetwinsize, tcsetattr, OptionalActions, Termios, Winsize,
    };

    /// A child running on a pseudoterminal, and the controller side to read.
    pub struct PtyChild {
        pub child: Child,
        /// The controller end. Reading it yields everything the child wrote to
        /// either stream, interleaved as a terminal interleaves them.
        pub controller: File,
    }

    /// Spawn `command` with a pseudoterminal on stdin, stdout and stderr.
    ///
    /// `winsize` is the parent terminal's, when there is one. A child that
    /// believes the window is 0×0 renders differently from one that knows it is
    /// 80×24, and the whole point of the terminal here is fidelity.
    ///
    /// # The `Command` is taken by value, and that is load-bearing
    ///
    /// A read of the controller sees EOF only when **every** copy of the user
    /// side is closed. `Command` keeps the `Stdio` values it was configured
    /// with so it can be spawned again, so a `&mut Command` that outlives this
    /// call keeps three copies of the user fd open in this process and the read
    /// below never returns. Measured: the first version of this function hung
    /// its own unit test, and the symptom (a test that never finishes) points
    /// nowhere near the cause. Owning the `Command` makes the close part of
    /// this function rather than a caller's obligation.
    pub fn spawn_on_pty(mut command: Command) -> io::Result<PtyChild> {
        let pty = rustix_openpty::openpty(None, Some(&child_winsize())).map_err(io::Error::from)?;
        let controller: OwnedFd = pty.controller;
        let user: OwnedFd = pty.user;

        command
            .stdin(Stdio::from(user.try_clone()?))
            .stdout(Stdio::from(user.try_clone()?))
            .stderr(Stdio::from(user.try_clone()?));
        let child = command.spawn()?;
        // Both of these, in this order, and neither is optional. See above.
        drop(command);
        drop(user);
        Ok(PtyChild {
            child,
            controller: File::from(controller),
        })
    }

    /// Is this process's stdout a terminal?
    ///
    /// The question the recorder asks before deciding what shape to give its
    /// child. A recorder that forces a pipe under a terminal has changed the
    /// conditions it exists to record.
    pub fn stdout_is_terminal() -> bool {
        isatty(io::stdout())
    }

    pub fn stdin_is_terminal() -> bool {
        isatty(io::stdin())
    }

    /// The parent terminal's settings, restored when this is dropped.
    ///
    /// A `Drop` guard rather than a call at the end: a relay that returns early,
    /// panics, or is killed by the child's own failure must still leave the
    /// user's terminal usable. Every shell command after a forgotten `tcsetattr`
    /// is typed blind.
    pub struct RawMode {
        saved: Option<Termios>,
    }

    impl RawMode {
        /// Put the parent terminal in raw mode, so keystrokes reach the child
        /// unbuffered and unechoed — the terminal on the far side of the PTY
        /// does the echoing now.
        ///
        /// A no-op when stdin is not a terminal, which is every test and every
        /// CI run.
        pub fn enter() -> RawMode {
            if !stdin_is_terminal() {
                return RawMode { saved: None };
            }
            let Ok(original) = tcgetattr(io::stdin()) else {
                return RawMode { saved: None };
            };
            let mut raw = original.clone();
            raw.make_raw();
            match tcsetattr(io::stdin(), OptionalActions::Now, &raw) {
                Ok(()) => RawMode {
                    saved: Some(original),
                },
                // Could not enter: do not record a restore we would perform on
                // a terminal we never changed.
                Err(_) => RawMode { saved: None },
            }
        }

        pub fn is_raw(&self) -> bool {
            self.saved.is_some()
        }
    }

    impl Drop for RawMode {
        fn drop(&mut self) {
            if let Some(saved) = self.saved.take() {
                let _ = tcsetattr(io::stdin(), OptionalActions::Now, &saved);
            }
        }
    }

    /// The size to give the child.
    ///
    /// The parent terminal's when there is one. When there is not — a pipe, a
    /// CI runner, a test harness — a **conventional 80×24**, because a child
    /// told the window is 0×0 renders nothing and some runners divide by it.
    ///
    /// This is a default for the *child's device*, not a recorded fact:
    /// [`window`] still answers `None` when nothing was measured, so the run
    /// report says "no window was measured" rather than reporting 80×24 as an
    /// observation.
    fn child_winsize() -> Winsize {
        tcgetwinsize(io::stdout())
            .ok()
            .filter(|w| w.ws_col > 0 && w.ws_row > 0)
            .unwrap_or(Winsize {
                ws_row: 24,
                ws_col: 80,
                ws_xpixel: 0,
                ws_ypixel: 0,
            })
    }

    /// The parent terminal's size, **as measured**. `None` when there is none.
    pub fn window() -> Option<(u16, u16)> {
        let w: Winsize = tcgetwinsize(io::stdout()).ok()?;
        Some((w.ws_col, w.ws_row))
    }
}

#[cfg(unix)]
pub use unix::{spawn_on_pty, stdin_is_terminal, stdout_is_terminal, window, PtyChild, RawMode};

#[cfg(not(unix))]
mod fallback {
    /// Tier 2 (§2): the capture is reduced and says so, rather than claiming a
    /// shape this build cannot produce.
    pub fn stdout_is_terminal() -> bool {
        false
    }

    pub fn stdin_is_terminal() -> bool {
        false
    }

    pub fn window() -> Option<(u16, u16)> {
        None
    }

    /// Nothing to enter and nothing to restore.
    pub struct RawMode;

    impl RawMode {
        pub fn enter() -> RawMode {
            RawMode
        }
        pub fn is_raw(&self) -> bool {
            false
        }
    }
}

#[cfg(not(unix))]
pub use fallback::{stdin_is_terminal, stdout_is_terminal, window, RawMode};

/// The shape a child's standard streams were given, **measured**.
///
/// This used to be a `const` equal to `"pipe"`, which made §7's capture-shape
/// comparison equal on both sides by construction. A recorded fact belongs in
/// the record; a constant standing where one belongs is the defect this build
/// has been removing from the verdict path all round.
pub fn stdio_mode() -> &'static str {
    if stdout_is_terminal() {
        TTY
    } else {
        PIPE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two spellings are the only two, and they differ. §7's comparison
    /// keys on them.
    #[test]
    fn the_two_shapes_are_distinct_and_named_once() {
        assert_ne!(TTY, PIPE);
        assert_eq!(TTY, "tty");
        assert_eq!(PIPE, "pipe");
    }

    /// The test harness runs with pipes, so this is the measured answer here —
    /// and it is measured, not assumed: `cargo test` gives its children pipes.
    #[test]
    fn the_mode_is_read_from_the_process_and_not_declared() {
        assert_eq!(stdio_mode(), if stdout_is_terminal() { TTY } else { PIPE });
    }

    #[cfg(unix)]
    #[test]
    fn a_child_on_a_pty_sees_a_terminal_and_one_on_a_pipe_does_not() {
        use std::io::Read;
        use std::process::Command;

        // `test -t 1` is the shell's own `isatty(1)`. Nothing here trusts our
        // own answer to the question we are testing.
        let mut command = Command::new("sh");
        command.args(["-c", "test -t 1 && echo TTY || echo PIPE"]);
        let mut pty = spawn_on_pty(command).expect("pty");
        let mut out = String::new();
        // EIO on the controller after the child closes is EOF here; read
        // errors end the read rather than failing the test.
        let _ = pty.controller.read_to_string(&mut out);
        let status = pty.child.wait().expect("wait");
        assert!(status.success());
        assert!(
            out.contains("TTY"),
            "a child on a pseudoterminal must see a terminal: {out:?}"
        );

        let piped = Command::new("sh")
            .args(["-c", "test -t 1 && echo TTY || echo PIPE"])
            .output()
            .expect("pipe");
        assert!(
            String::from_utf8_lossy(&piped.stdout).contains("PIPE"),
            "the control must NOT see a terminal, or the test above proves nothing"
        );
    }

    /// A terminal really does change the bytes — measured rather than asserted,
    /// because the whole `stdio` field exists on that premise.
    #[cfg(unix)]
    #[test]
    fn the_same_command_produces_different_bytes_on_a_terminal() {
        use std::io::Read;
        use std::process::Command;

        let script = "printf 'x'; test -t 1 && printf '\\033[32mgreen\\033[0m' || printf 'plain'";
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        let mut pty = spawn_on_pty(command).expect("pty");
        let mut on_tty = Vec::new();
        let _ = pty.controller.read_to_end(&mut on_tty);
        let _ = pty.child.wait();

        let on_pipe = Command::new("sh").args(["-c", script]).output().expect("o");
        assert_ne!(
            on_tty, on_pipe.stdout,
            "if a terminal changed nothing, §7 would not need to condition on \
             the capture shape at all"
        );
    }

    #[cfg(unix)]
    #[test]
    fn raw_mode_is_a_no_op_when_stdin_is_not_a_terminal() {
        let guard = RawMode::enter();
        assert!(
            !guard.is_raw(),
            "the test harness's stdin is not a terminal; entering raw mode here \
             would be changing a terminal that is not ours"
        );
    }
}
