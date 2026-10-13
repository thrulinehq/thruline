//! Historical output scanner retained with its regression corpus.
//!
//! R-2026-09-27-14 (D) supersedes output-based environmental classification:
//! project code can print every string this scanner recognizes. The check
//! wrapper no longer consults it. Only failure of the isolated product-owned
//! dependency fetch in [`crate::cargo_fetch`] authorizes Couldn't check;
//! after that fetch succeeds, a failing project command is Failed.

/// The longest single line this will assemble before giving up on it. The
/// recognised lines are all under 200 bytes.
const MAX_LINE: usize = 4096;

/// What one stream said about the build's environment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Reading {
    /// A line of cargo's own offline-fetch failure was seen.
    pub fetch_failed: bool,
    /// Compilation or a terminal failure of project code was seen.
    pub compiled: bool,
    /// A line could not be read in full as UTF-8, so it could hide a failure.
    pub unreadable: bool,
}

impl Reading {
    /// Two streams of one run, taken together.
    pub fn plus(self, other: Self) -> Self {
        Self {
            fetch_failed: self.fetch_failed || other.fetch_failed,
            compiled: self.compiled || other.compiled,
            unreadable: self.unreadable || other.unreadable,
        }
    }
}

/// Reads a byte stream as it goes past, holding at most one line.
#[derive(Debug, Default)]
pub struct Scanner {
    line: Vec<u8>,
    overlong: bool,
    reading: Reading,
}

impl Scanner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed the next chunk of the stream.
    pub fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            if b == b'\n' {
                self.finish_line();
                continue;
            }
            if self.overlong {
                continue;
            }
            if self.line.len() >= MAX_LINE {
                self.line.clear();
                self.overlong = true;
                self.reading.unreadable = true;
                continue;
            }
            self.line.push(b);
        }
    }

    fn finish_line(&mut self) {
        if !self.overlong {
            if let Ok(text) = std::str::from_utf8(&self.line) {
                if is_fetch_failure(text) {
                    self.reading.fetch_failed = true;
                }
                if is_compilation(text) {
                    self.reading.compiled = true;
                }
            } else {
                self.reading.unreadable = true;
            }
        }
        self.line.clear();
        self.overlong = false;
    }

    /// The reading over everything fed so far, including a last line with no
    /// newline after it.
    pub fn finish(mut self) -> Reading {
        self.finish_line();
        self.reading
    }
}

/// Cargo's own words for "offline, and a dependency is not here".
///
/// * The resolver could not find a dependency, and says it was offline:
///   `note: offline mode (via `--offline`) can sometimes cause surprising
///   resolution failures` (printed by cargo 1.97.1 on this machine).
/// * A git dependency could not be checked out: `can't checkout from '<url>':
///   you are in the offline mode (--offline)` (printed by cargo 1.97.1 on
///   this machine).
/// * A download was refused: `attempting to make an HTTP request, but
///   --offline was specified` (in cargo 1.97.1's string table; `--frozen`
///   takes the flag's place when that was the switch).
fn is_fetch_failure(line: &str) -> bool {
    (line.contains("offline mode (via `")
        && line.contains("can sometimes cause surprising resolution failures"))
        || line.contains("you are in the offline mode (")
        || (line.contains("attempting to make an HTTP request, but ")
            && line.contains(" was specified"))
}

/// Cargo's compilation diagnostics and terminal project-failure summaries.
/// The summaries remain when progress is quiet or cached, including custom
/// test harnesses and build scripts that print misleading resolver words.
/// Extra project-printed failure words can only keep the conservative failed
/// outcome; they can never turn a project failure into an environment claim.
fn is_compilation(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("Compiling ")
        || trimmed.starts_with("error[")
        || line.contains("could not compile `")
        || line.contains("test failed")
        || line.contains("failed to run custom build command")
}

/// **The one decision.** Did this run fail because something it needs is not
/// on this machine, rather than because of the project's code?
///
/// True only when every one of these holds:
///
/// * it exited, non-zero (a signal, or no exit code at all, is not this);
/// * both streams were captured whole and every scanned line was readable
///   (an unreadable or overlong line could hide a terminal failure);
/// * the output carried a recognised offline-fetch failure;
/// * nothing in it says compilation began or a compiler error was printed;
/// * no test failed and no failure block was read.
pub fn dependencies_unavailable(
    exit_code: Option<i64>,
    term_signal: Option<i64>,
    streams_whole: bool,
    reading: Reading,
    tests_failed: u64,
    has_failure_line: bool,
) -> bool {
    matches!(exit_code, Some(code) if code != 0)
        && term_signal.is_none()
        && streams_whole
        && reading.fetch_failed
        && !reading.compiled
        && !reading.unreadable
        && tests_failed == 0
        && !has_failure_line
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What cargo 1.97.1 printed, offline, for a project needing `serde` with
    /// an empty `CARGO_HOME`. The path is replaced; the lines are verbatim.
    const MISSING_DEPENDENCY: &str = "error: no matching package named `serde` found\n\
location searched: crates.io index\n\
required by package `offline_fixture v0.1.0 (/work/offline_fixture)`\n\
note: offline mode (via `--offline`) can sometimes cause surprising resolution failures\n\
help: if this error is too confusing you may wish to retry without `--offline`\n";

    /// A git dependency, offline, verbatim but for the path.
    const GIT_DEPENDENCY: &str =
        "error: failed to get `foo` as a dependency of package `gitdep v0.1.0 (/work/gitdep)`\n\
\n\
Caused by:\n\
  failed to load source for dependency `foo`\n\
\n\
Caused by:\n\
  unable to update https://example.invalid/foo.git\n\
\n\
Caused by:\n\
  can't checkout from 'https://example.invalid/foo.git': you are in the offline mode (--offline)\n";

    /// A type error in the project's own code, no dependencies, verbatim but
    /// for the path.
    const TYPE_ERROR: &str = "   Compiling typeerr v0.1.0 (/work/typeerr)\n\
error[E0308]: mismatched types\n\
 --> src/lib.rs:1:21\n\
  |\n\
1 | pub fn f() -> u32 { \"no\" }\n\
  |               ---   ^^^^ expected `u32`, found `&str`\n\
\n\
For more information about this error, try `rustc --explain E0308`.\n\
error: could not compile `typeerr` (lib) due to 1 previous error\n";

    fn read(text: &str) -> Reading {
        let mut s = Scanner::new();
        s.feed(text.as_bytes());
        s.finish()
    }

    fn decide(reading: Reading) -> bool {
        dependencies_unavailable(Some(101), None, true, reading, 0, false)
    }

    #[test]
    fn a_missing_dependency_offline_is_environmental() {
        let r = read(MISSING_DEPENDENCY);
        assert!(r.fetch_failed && !r.compiled, "{r:?}");
        assert!(decide(r));
    }

    #[test]
    fn a_git_dependency_offline_is_environmental() {
        assert!(decide(read(GIT_DEPENDENCY)));
    }

    #[test]
    fn a_refused_download_is_environmental() {
        let line = "  attempting to make an HTTP request, but --offline was specified\n";
        assert!(decide(read(line)));
        let frozen = "  attempting to make an HTTP request, but --frozen was specified\n";
        assert!(decide(read(frozen)));
    }

    #[test]
    fn a_compile_error_in_the_projects_own_code_is_not() {
        let r = read(TYPE_ERROR);
        assert!(!r.fetch_failed && r.compiled, "{r:?}");
        assert!(!decide(r));
    }

    /// The conservative half of the mixed case: a fetch failure AND a sign of
    /// compilation in one output is not environmental. It stays failed.
    #[test]
    fn a_fetch_failure_beside_a_compile_error_stays_failed() {
        let both = format!("{TYPE_ERROR}{MISSING_DEPENDENCY}");
        let r = read(&both);
        assert!(r.fetch_failed && r.compiled);
        assert!(!decide(r));
        // Split across the two streams, the same answer.
        let split = read(TYPE_ERROR).plus(read(MISSING_DEPENDENCY));
        assert!(!decide(split));
    }

    #[test]
    fn an_unrecognised_failure_is_not_environmental() {
        for text in [
            "error: could not find `Cargo.toml` in `/work` or any parent directory\n",
            "error: failed to download from `https://example.invalid/x`\n",
            "offline\n",
            "",
        ] {
            assert!(!decide(read(text)), "{text:?}");
        }
    }

    #[test]
    fn every_other_condition_keeps_the_old_answer() {
        let r = read(MISSING_DEPENDENCY);
        // It passed: nothing to reclassify.
        assert!(!dependencies_unavailable(Some(0), None, true, r, 0, false));
        // No exit code, or a signal: not a finished build.
        assert!(!dependencies_unavailable(None, None, true, r, 0, false));
        assert!(!dependencies_unavailable(
            Some(101),
            Some(9),
            true,
            r,
            0,
            false
        ));
        // A stream not captured whole could have lost the compile line.
        assert!(!dependencies_unavailable(
            Some(101),
            None,
            false,
            r,
            0,
            false
        ));
        // A failed test, or a failure block, is the code speaking.
        assert!(!dependencies_unavailable(
            Some(101),
            None,
            true,
            r,
            1,
            false
        ));
        assert!(!dependencies_unavailable(Some(101), None, true, r, 0, true));
    }

    #[test]
    fn a_line_split_across_chunks_is_still_read() {
        let mut s = Scanner::new();
        let bytes = MISSING_DEPENDENCY.as_bytes();
        for chunk in bytes.chunks(7) {
            s.feed(chunk);
        }
        assert!(decide(s.finish()));
    }

    #[test]
    fn a_last_line_with_no_newline_is_read() {
        let text = "note: offline mode (via `--offline`) can sometimes cause surprising resolution failures";
        assert!(decide(read(text)));
    }

    #[test]
    fn terminal_cargo_failures_cannot_be_overridden_by_resolver_words() {
        for failure in [
            "error: test failed, to rerun pass `--test custom`",
            "error: failed to run custom build command for `synthetic v0.1.0`",
            "error: doctest failed, to rerun pass `--doc`",
        ] {
            for text in [
                format!("{MISSING_DEPENDENCY}{failure}\n"),
                format!("{failure}\n{MISSING_DEPENDENCY}"),
            ] {
                assert!(!decide(read(&text)), "{text}");
                let mut scanner = Scanner::new();
                for chunk in text.as_bytes().chunks(3) {
                    scanner.feed(chunk);
                }
                assert!(!decide(scanner.finish()), "chunked: {text}");
            }
        }
    }

    #[test]
    fn an_overlong_line_cannot_hide_the_terminal_cargo_failure() {
        let mut scanner = Scanner::new();
        scanner.feed(MISSING_DEPENDENCY.as_bytes());
        scanner.feed(&[b'x'; MAX_LINE + 1]);
        scanner.feed(b"error: test failed, to rerun pass `--test custom`\n");
        assert!(!decide(scanner.finish()));
    }

    #[test]
    fn invalid_utf8_cannot_hide_the_terminal_cargo_failure() {
        let mut scanner = Scanner::new();
        scanner.feed(MISSING_DEPENDENCY.as_bytes());
        scanner.feed(b"\xfferror: test failed, to rerun pass `--test custom`\n");
        assert!(!decide(scanner.finish()));
    }

    #[test]
    fn an_unreadable_stream_cannot_supply_an_environmental_verdict() {
        let mut scanner = Scanner::new();
        scanner.feed(&[b'x'; MAX_LINE + 1]);
        scanner.feed(b"\n");
        assert!(!decide(read(MISSING_DEPENDENCY).plus(scanner.finish())));
    }
}
