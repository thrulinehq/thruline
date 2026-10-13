//! T-3-6-F1 (`Q1`, `B16.4`, `B16.6`) — how many tests a recorded run actually
//! ran, as a typed field.
//!
//! ## The hole this fills
//!
//! T-3-1 refused, deliberately, to read a zero exit code as a passing test
//! suite ([`crate::change_evidence::observed_of`]'s `RunClass::Test` arm): a
//! command that exits 0 proves that a command exited 0, and zero tests, every
//! test filtered out, and a full green suite are the same exit code. That
//! refusal is right about exit codes and was wrong about the whole record — a
//! `cargo test` run prints its own count, in its own typed shape, and the
//! wrapper that captured the run had those bytes in hand and threw the number
//! away. Sam ran two suites, both passed, and every screen said nothing had
//! run for either line.
//!
//! So: **the number is parsed once, at record time, from the stream as it is
//! captured, and stored as columns.** Never from the exit code, never from a
//! sentence, never re-derived on a screen.
//!
//! ## Record time, not derive time — because the bytes are gone afterwards
//!
//! [`crate::evidence_binding`] stores stdout and stderr as a **digest and a
//! byte count**, not as content: nothing on this machine keeps the output of a
//! check after the wrapper exits. There is therefore no derive-time reading of
//! "the recorded output" to have. The scan happens inside the wrapper's tee,
//! over the same bytes it is already hashing and passing through, and the
//! result is appended beside the binding.
//!
//! ## Why a table of its own and not a field of the binding
//!
//! The binding's id is the SHA-256 of its canonical `EVB_V1` bytes. Adding a
//! field to that document changes every binding id in existence, which is an
//! `EVB_V2` migration, not a card; adding a field *outside* the canonical
//! bytes would put a user-facing number in the document that the document's
//! own id does not cover. T-3-4 met the same choice for execution state and
//! answered it the same way — a small append-only table keyed by the run,
//! read back by [`crate::change_evidence`]. This is that pattern, again.
//!
//! ## What counts (the rule, stated once)
//!
//! [`TestTally::counts_a_suite`] is true when the record holds **at least one
//! passing test and no failing test**. Everything else is the old answer:
//!
//! * no parsable count at all → no row, and a zero-exit run keeps reading
//!   "Something ran for this, and it checked nothing."
//! * `0 passed` (everything filtered out, or an empty suite) → a run that
//!   checked nothing, said in those words.
//! * any failure → this never reaches the question: a failing suite exits
//!   non-zero and is `not proven` before the tally is consulted.
//!
//! And the tally never overrides the two vetoes above it: a cancelled or
//! interrupted execution (T-3-4) and evidence the freshness layer has
//! invalidated (T-S-2) still settle nothing, whatever they counted.

use std::collections::BTreeMap;

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::ledger::Ledger;

/// Bump when a served field of [`TestTally`] is added or changed.
pub const TEST_TALLY_SCHEMA_VERSION: u32 = 1;

/// What one captured run said it ran, summed over every suite in it.
///
/// A `cargo test` invocation can print several `test result:` lines — one per
/// test target — and the run is one run, so they are summed. `suites` is how
/// many such lines the stream carried, kept so that "three suites, nine tests"
/// is distinguishable from "one suite, nine tests" without re-reading bytes
/// that no longer exist.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestTally {
    pub passed: u64,
    pub failed: u64,
    pub ignored: u64,
    pub suites: u64,
}

impl TestTally {
    /// Two streams of one run, added. A `test result:` line appears in one
    /// stream or the other, never both, and a run is one run.
    pub fn plus(self, other: Self) -> Self {
        Self {
            passed: self.passed.saturating_add(other.passed),
            failed: self.failed.saturating_add(other.failed),
            ignored: self.ignored.saturating_add(other.ignored),
            suites: self.suites.saturating_add(other.suites),
        }
    }

    /// **The rule.** At least one test passed and none failed.
    pub fn counts_a_suite(&self) -> bool {
        self.suites > 0 && self.passed >= 1 && self.failed == 0
    }
}

/// Reads `test result:` lines out of a byte stream as it goes past.
///
/// Incremental and allocation-bounded: the wrapper feeds it 64 KiB at a time
/// and it holds at most one line. A line longer than [`MAX_LINE`] is not a
/// `test result:` line and is dropped rather than grown, so a run that prints
/// a gigabyte on one line cannot make the wrapper the problem.
#[derive(Debug, Default)]
pub struct Scanner {
    line: Vec<u8>,
    overlong: bool,
    tally: TestTally,
}

/// The longest line this will assemble before giving up on it. `cargo test`'s
/// result line is under 120 bytes; 4 KiB is generous and finite.
const MAX_LINE: usize = 4096;

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
                continue;
            }
            self.line.push(b);
        }
    }

    fn finish_line(&mut self) {
        if !self.overlong {
            if let Ok(text) = std::str::from_utf8(&self.line) {
                if let Some(counts) = parse_result_line(text) {
                    self.tally.passed += counts.passed;
                    self.tally.failed += counts.failed;
                    self.tally.ignored += counts.ignored;
                    self.tally.suites += 1;
                }
            }
        }
        self.line.clear();
        self.overlong = false;
    }

    /// The tally over everything fed so far, including a last line with no
    /// newline after it.
    pub fn finish(mut self) -> TestTally {
        self.finish_line();
        self.tally
    }
}

/// One `test result:` line, or nothing.
///
/// The shape, from libtest:
///
/// ```text
/// test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
/// test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out
/// ```
///
/// Both `passed` and `failed` must be present and be numbers, or this is not
/// a result line as far as the record is concerned — a half-read line is an
/// unknown, never a zero.
pub fn parse_result_line(line: &str) -> Option<TestTally> {
    let rest = line.split_once("test result:")?.1;
    let passed = labelled(rest, "passed")?;
    let failed = labelled(rest, "failed")?;
    let ignored = labelled(rest, "ignored").unwrap_or(0);
    Some(TestTally {
        passed,
        failed,
        ignored,
        suites: 1,
    })
}

/// The number immediately before `<word>` in `N word`.
fn labelled(rest: &str, word: &str) -> Option<u64> {
    for segment in rest.split(';') {
        let segment = segment.trim();
        let Some(number) = segment.strip_suffix(word) else {
            continue;
        };
        // The first segment is `ok. 3 passed`, so it is the LAST whitespace
        // token before the word that is the number, not the whole segment.
        let Some(number) = number.split_whitespace().next_back() else {
            continue;
        };
        if let Ok(n) = number.parse::<u64>() {
            return Some(n);
        }
    }
    None
}

/// Append this run's tally beside the binding it came from.
///
/// One row per binding, and the binding id is the primary key: the store
/// refuses a second tally for the same captured run rather than letting a
/// later reading quietly replace an earlier one.
pub fn record(ledger: &Ledger, binding_id: &str, tally: &TestTally, at: &str) -> Result<()> {
    ledger.conn().execute(
        "INSERT INTO check_run_tallies(binding_id, passed, failed, ignored, suites, recorded_at)
         VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            binding_id,
            tally.passed as i64,
            tally.failed as i64,
            tally.ignored as i64,
            tally.suites as i64,
            at
        ],
    )?;
    Ok(())
}

/// The tally for one captured run, when the record holds one.
pub fn of_binding(ledger: &Ledger, binding_id: &str) -> Result<Option<TestTally>> {
    Ok(ledger
        .conn()
        .query_row(
            "SELECT passed, failed, ignored, suites FROM check_run_tallies WHERE binding_id=?1",
            params![binding_id],
            |row| {
                Ok(TestTally {
                    passed: row.get::<_, i64>(0)?.max(0) as u64,
                    failed: row.get::<_, i64>(1)?.max(0) as u64,
                    ignored: row.get::<_, i64>(2)?.max(0) as u64,
                    suites: row.get::<_, i64>(3)?.max(0) as u64,
                })
            },
        )
        .optional()?)
}

/// Every tally in the store, by binding id — read once for a whole coverage
/// pass rather than per row, the same way execution state is.
pub fn by_binding(ledger: &Ledger) -> Result<BTreeMap<String, TestTally>> {
    let mut statement = ledger
        .conn()
        .prepare("SELECT binding_id, passed, failed, ignored, suites FROM check_run_tallies")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            TestTally {
                passed: row.get::<_, i64>(1)?.max(0) as u64,
                failed: row.get::<_, i64>(2)?.max(0) as u64,
                ignored: row.get::<_, i64>(3)?.max(0) as u64,
                suites: row.get::<_, i64>(4)?.max(0) as u64,
            },
        ))
    })?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (id, tally) = row?;
        out.insert(id, tally);
    }
    Ok(out)
}
