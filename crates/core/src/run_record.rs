//! T-3-6-F3 (`Q2`, `B16.8`, `B16.10`) — two typed fields about one captured
//! run: **what it said when it failed**, and **what class of evidence it was
//! declared to be**.
//!
//! ## The hole this fills
//!
//! On case `Q2` the record held a failing `cargo test`, and every screen could
//! say that a line was not proven and none of them could say *what broke*. The
//! failing assertion's own words — "a second retry request while one is
//! pending must return the same job, not a new one", and the two job ids it
//! printed — existed only in the terminal the person had already closed.
//! [`crate::evidence_binding`] keeps stdout and stderr as a digest and a byte
//! count, never as content, so there is no derive-time reading of "the
//! recorded output" to have. The line is therefore read **while the bytes are
//! going past**, in the wrapper's own tee, exactly the way T-3-6-F1 reads the
//! test count, and stored as a column.
//!
//! The second field is the one `B16.10` needs. Nothing in this product records
//! where a finding came from, so nothing could label a seeded demonstration as
//! seeded, and `B16.10` — *do not present a seeded demonstration as field
//! accuracy* — had no surface to stand on. It is recorded here, **declared by
//! whoever ran the check** (`thruline check --evidence-class seeded`), never
//! guessed from a branch name and never inferred from a failure. A run with no
//! declaration carries no class, and the screens say that in those words
//! rather than calling it ordinary.
//!
//! ## Why a table of its own
//!
//! The same answer T-3-4 and T-3-6-F1 gave: the binding's id is the SHA-256 of
//! its canonical `EVB_V1` bytes, so a new field inside them renames every
//! binding on every machine. This is a small append-only table keyed by the
//! run, read back by [`crate::change_evidence`].
//!
//! ## What is NOT here
//!
//! No verdict moves because of either field. A failure line does not make a
//! run more or less failing — the run's exit code already decided that — and a
//! class does not make evidence count for more or less. Both are things a
//! person reads beside a finding that the record already holds.

use std::collections::BTreeMap;

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::ledger::Ledger;

/// The longest failure block this will keep, in bytes of UTF-8.
///
/// Four lines of libtest's own output is under 400 bytes in every shape we
/// have seen. The cap is finite so that a run which panics with a megabyte of
/// message cannot make the wrapper the problem, and a block cut at the cap is
/// cut on a character boundary.
pub const MAX_FAILURE_LINE: usize = 512;

/// How many lines after the panic header are kept.
///
/// libtest prints the message, then `left:` and `right:` for an equality
/// assertion. Four covers that with one to spare; the fifth line onward is a
/// backtrace note, which is not the assertion's own words.
const MAX_BLOCK_LINES: usize = 4;

/// Where a finding came from — the four classes of
/// `qualification/retry-delivery/evidence-classes.json`, which
/// `docs/PHASE1-DECISIONS.md` §2.4 decided and which are never merged.
///
/// This is **declared**, never derived. The product cannot tell by looking at
/// a run whether the defect it found was planted, and a guess printed beside a
/// finding is worse than no label at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceClass {
    /// Whatever actually happened, for real, on this machine's own work.
    NaturallyOccurring,
    /// A labelled defect, set up on purpose to demonstrate something.
    Seeded,
    /// A hand-written fixture. Never presented as a run that happened.
    Synthetic,
    /// A reader outside the thing under test, counting for itself.
    Independent,
}

impl EvidenceClass {
    /// The word the store holds, and the word the command line accepts.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NaturallyOccurring => "naturally_occurring",
            Self::Seeded => "seeded",
            Self::Synthetic => "synthetic",
            Self::Independent => "independent",
        }
    }

    /// The copy-file slug for the sentence a person reads beside a finding.
    pub fn slug(&self) -> &'static str {
        match self {
            Self::NaturallyOccurring => "evidence_class_naturally_occurring",
            Self::Seeded => "evidence_class_seeded",
            Self::Synthetic => "evidence_class_synthetic",
            Self::Independent => "evidence_class_independent",
        }
    }

    /// A word from outside, refused rather than coerced. An unrecognised class
    /// is not stored as one of the four and is not stored as "unknown": the
    /// caller is told, and nothing is written.
    pub fn parse(word: &str) -> Option<Self> {
        match word.trim() {
            "naturally_occurring" | "naturally-occurring" => Some(Self::NaturallyOccurring),
            "seeded" => Some(Self::Seeded),
            "synthetic" => Some(Self::Synthetic),
            "independent" => Some(Self::Independent),
            _ => None,
        }
    }

    /// Every class, in the order the decision document lists them — for the
    /// command line's own error message, so it can never drift from the enum.
    pub fn every() -> [Self; 4] {
        [
            Self::NaturallyOccurring,
            Self::Seeded,
            Self::Synthetic,
            Self::Independent,
        ]
    }
}

/// What one captured run left behind besides its digest.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecord {
    /// The failing assertion's own words, as the stream carried them. `None`
    /// when the stream held no failure block this scanner recognised — which
    /// is **not** "it passed" and is never rendered as one.
    pub failure_line: Option<String>,
    /// What this run was declared to be. `None` when nobody said.
    pub evidence_class: Option<EvidenceClass>,
}

impl RunRecord {
    /// Is there anything here worth a row? A record with neither field is not
    /// stored, so an ordinary passing run costs nothing.
    pub fn is_empty(&self) -> bool {
        self.failure_line.is_none() && self.evidence_class.is_none()
    }
}

/// Reads the first failure block out of a byte stream as it goes past.
///
/// Incremental and allocation-bounded, like [`crate::test_tally::Scanner`]:
/// the wrapper feeds it 64 KiB at a time and it holds at most one line plus
/// the block it has decided to keep. Only the FIRST block is kept — a suite
/// with nine failures has one thing a person reads first, and the run's own
/// output is where the other eight live.
#[derive(Debug, Default)]
pub struct FailureScanner {
    line: Vec<u8>,
    overlong: bool,
    /// True once a `panicked at …:` header has been seen and the lines after
    /// it are the block.
    capturing: bool,
    block: Vec<String>,
    done: bool,
}

/// The longest single line this will assemble before giving up on it.
const MAX_LINE: usize = 4096;

impl FailureScanner {
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
        if !self.overlong && !self.done {
            if let Ok(text) = std::str::from_utf8(&self.line) {
                let text = text.trim_end().to_string();
                self.consider(&text);
            }
        }
        self.line.clear();
        self.overlong = false;
    }

    fn consider(&mut self, text: &str) {
        if !self.capturing {
            if is_panic_header(text) {
                self.capturing = true;
            }
            return;
        }
        let trimmed = text.trim();
        // The block ends at the first blank line, at libtest's backtrace note,
        // or when it has had its four lines. Ending it is not a failure to
        // read: what was kept is the assertion's own words and the note is
        // ours, not the test's.
        if trimmed.is_empty() || trimmed.starts_with("note:") || self.block.len() >= MAX_BLOCK_LINES
        {
            self.done = !self.block.is_empty();
            self.capturing = false;
            return;
        }
        self.block.push(trimmed.to_string());
    }

    /// The failure block over everything fed so far, including a last line
    /// with no newline after it, capped at [`MAX_FAILURE_LINE`] bytes.
    pub fn finish(mut self) -> Option<String> {
        self.finish_line();
        if self.block.is_empty() {
            return None;
        }
        let joined = self.block.join("\n");
        Some(truncate_on_char_boundary(&joined, MAX_FAILURE_LINE))
    }
}

/// libtest's panic header: `thread '…' panicked at src/lib.rs:12:5:`.
///
/// Matched on the two tokens it always carries rather than on the whole shape,
/// so a harness that words the thread half differently still opens a block.
fn is_panic_header(text: &str) -> bool {
    text.contains("panicked at ") && text.trim_end().ends_with(':')
}

/// Cut at `max` bytes without splitting a character. Never appends an ellipsis:
/// a mark this file invented, sitting inside text attributed to the run, would
/// be this product putting a word in the record's mouth.
fn truncate_on_char_boundary(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

/// Append this run's record beside the binding it came from.
///
/// One row per binding, and the binding id is the primary key: the store
/// refuses a second reading of the same captured run rather than letting a
/// later one quietly replace the first. A record with nothing in it writes
/// nothing.
pub fn record(ledger: &Ledger, binding_id: &str, run: &RunRecord, at: &str) -> Result<()> {
    if run.is_empty() {
        return Ok(());
    }
    ledger.conn().execute(
        "INSERT INTO check_run_records(binding_id, failure_line, evidence_class, recorded_at)
         VALUES(?1,?2,?3,?4)",
        params![
            binding_id,
            run.failure_line.as_deref(),
            run.evidence_class.map(|c| c.as_str()),
            at
        ],
    )?;
    Ok(())
}

/// The record for one captured run, when the store holds one.
pub fn of_binding(ledger: &Ledger, binding_id: &str) -> Result<Option<RunRecord>> {
    Ok(ledger
        .conn()
        .query_row(
            "SELECT failure_line, evidence_class FROM check_run_records WHERE binding_id=?1",
            params![binding_id],
            |row| {
                Ok(RunRecord {
                    failure_line: row.get::<_, Option<String>>(0)?,
                    evidence_class: row
                        .get::<_, Option<String>>(1)?
                        .as_deref()
                        .and_then(EvidenceClass::parse),
                })
            },
        )
        .optional()?)
}

/// Every run record in the store, by binding id — read once for a whole
/// coverage pass rather than per row, the way execution state and tallies are.
pub fn by_binding(ledger: &Ledger) -> Result<BTreeMap<String, RunRecord>> {
    let mut statement = ledger
        .conn()
        .prepare("SELECT binding_id, failure_line, evidence_class FROM check_run_records")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            RunRecord {
                failure_line: row.get::<_, Option<String>>(1)?,
                evidence_class: row
                    .get::<_, Option<String>>(2)?
                    .as_deref()
                    .and_then(EvidenceClass::parse),
            },
        ))
    })?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (id, run) = row?;
        out.insert(id, run);
    }
    Ok(out)
}
