//! Where, in a linked conversation's own log, the agent said what it finished
//! on: a file and a line, and none of the words.
//!
//! A Change can link work ThruLine was not recording. The record holds nothing
//! of what the agent said there, but the transcript the agent's own tool wrote
//! usually does, and a person can open it. So the Change says the work was not
//! recorded, says the log holds it, and points at the line. It does not show
//! the words: showing them needs a filter that proves no credential is among
//! them, and each round of adversarial testing found a layout of one that the
//! filter tried here admitted. The founder retired the quote at launch. Nothing
//! of the log's text leaves this module, so there is nothing for a filter to
//! miss.
//!
//! The module reads and writes nothing else:
//!
//! 1. **Only lines ThruLine holds.** The session's `events` rows name the file,
//!    line and SHA-256 of every transcript line the ledger ingested. The file
//!    is read back, and a line counts only when its bytes still hash to what
//!    the ledger recorded. A line the ledger does not hold, or that changed
//!    since, contributes nothing.
//! 2. **Only the agent, by the log's own field.** Assistant text counts only
//!    when its line's own JSON says `"type": "assistant"` and
//!    `"message": {"role": "assistant"}`. The parser defaults a missing
//!    `message.role` to `"assistant"`, so the ledger's `role` column is not
//!    that field and is never read here. A line whose field is missing, or
//!    whose two fields disagree, is not the agent's; the speaker is never
//!    inferred from wording, position or tool calls.
//! 3. **The final claim, by the existing rule.** The kept lines go through
//!    [`crate::speaker::final_turn_of`], the rule the speaker hooks use, and
//!    the place is the line of the last claim in the final turn.
//! 4. **Bounded work.** The place is asked for on every request for a Change,
//!    so asking must neither wait on a path nor read a log twice for nothing.
//!    - The recorded path is opened only when it names an ordinary file. A
//!      named pipe, a socket, a directory or a device is not opened: a pipe
//!      with no writer would hold the request for ever, and a device such as
//!      `/dev/zero` has no end. Its conversation has no place and the reason
//!      is [`NotLocated::LogUnreadable`]. The path is looked at by name, and
//!      the file that is then opened is looked at again without waiting for
//!      it, so a name swapped between the two looks is still refused.
//!    - What one read of a log worked out is remembered, in this process only
//!      and by the log's path (`LogMemo`), and is used again only while two
//!      things still hold: the file is the same file (device, inode, size,
//!      modification time and change time), and the ledger holds the same of
//!      the conversation (how many events, the last one's sequence number and
//!      its chain hash, which commits to the hash of every line before it).
//!      Either moving reads the log again and the answer is worked out anew.
//!      The change time is set by the system and cannot be put back by
//!      touching the file, so a log rewritten in place to its old size and
//!      modification time is still read again.
//!    - The memory holds at most [`MEMO_CAPACITY`] logs, the least recently
//!      used out first, and each entry is the path, the file's identity, what
//!      the ledger held and the answer: never a byte of the log. A restart
//!      forgets it, and the first look after one is a full read.
//!
//! What this is not: a finding, a claim that was checked, or an input to
//! readiness, coverage or approval. Nothing in the engine reads it.

use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use rusqlite::params;
use serde::Serialize;

use crate::chain::sha256_hex;
use crate::change::{mask_text, AssociationProvenance, ChangeView};
use crate::error::Result;
use crate::ledger::Ledger;
use crate::model::EventKind;
use crate::parse::{parse_reader, ParseStats, ParsedLine};
use crate::redact::{MaskRecord, Treatment};

/// Where the agent's final claim is in its own log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LogReceipt {
    /// The log file, as the ledger recorded it (masked).
    pub file: String,
    /// The 1-based line of that file.
    pub line: u64,
}

/// Why a confirmed conversation has no place. Each is a plain fact about the
/// record; none is a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NotLocated {
    /// The ledger holds no line of this conversation's log.
    NotHeld,
    /// The log file could not be read back: it is gone, or cannot be opened,
    /// or the path the ledger recorded no longer names an ordinary file (a
    /// named pipe, a socket, a directory, a device). Nothing is read from such
    /// a path.
    LogUnreadable,
    /// No line the ledger holds still reads as it did when it was recorded.
    LogChanged,
    /// The final turn holds no claim in a line whose own role field names the
    /// agent.
    NoAgentClaim,
}

/// What [`locate`] found for one conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Located {
    At(LogReceipt),
    Not(NotLocated),
}

/// One confirmed conversation of the Change, and its place or why there is
/// none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentLogEntry {
    pub session_id: String,
    pub receipt: Option<LogReceipt>,
    pub not_located: Option<NotLocated>,
}

/// What `GET /api/changes/<id>` serves beside the Change as `agent_log`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentLogView {
    /// One entry per confirmed conversation, by session id.
    pub sessions: Vec<AgentLogEntry>,
    /// What the masker did to the file names above.
    pub masking: Treatment,
}

/// The sessions a Change's standing associations confirm, in id order.
fn confirmed_sessions(change: &ChangeView) -> Vec<String> {
    let mut standing: BTreeMap<&str, AssociationProvenance> = BTreeMap::new();
    for a in &change.associations {
        standing.insert(a.session_id.as_str(), a.provenance);
    }
    standing
        .into_iter()
        .filter(|(_, p)| *p == AssociationProvenance::Confirmed)
        .map(|(s, _)| s.to_string())
        .collect()
}

/// The place, or why there is none, for every confirmed conversation.
pub fn for_change(ledger: &Ledger, change: &ChangeView) -> Result<AgentLogView> {
    let mut record = MaskRecord::default();
    let mut sessions = Vec::new();
    for session_id in confirmed_sessions(change) {
        let (receipt, not_located) = match locate(ledger, &session_id, &mut record)? {
            Located::At(receipt) => (Some(receipt), None),
            Located::Not(why) => (None, Some(why)),
        };
        sessions.push(AgentLogEntry {
            session_id,
            receipt,
            not_located,
        });
    }
    Ok(AgentLogView {
        sessions,
        masking: record.to_treatment(),
    })
}

/// The line of the agent's final claim in one conversation's log, from the log
/// lines the ledger holds.
pub fn locate(ledger: &Ledger, session_id: &str, record: &mut MaskRecord) -> Result<Located> {
    // What the ledger holds of this conversation, cheaply. The file of the
    // last event is the log whose end is meant.
    let Some(held) = held_summary(ledger, session_id)? else {
        return Ok(Located::Not(NotLocated::NotHeld));
    };

    // A caller for this path owns the cold computation until it has published
    // the answer. The memo itself stays unlocked during I/O; work in other
    // stripes can proceed. A later caller for this path checks the memo only
    // after the first has had a chance to remember its answer.
    let _read_guard = read_guard(&held.file);

    // Only an ordinary file is ever opened, and only the file itself is read.
    let Some((mut log, identity)) = open_ordinary(Path::new(&held.file)) else {
        return Ok(Located::Not(NotLocated::LogUnreadable));
    };

    let recalled = memo().recall(&held, &identity);
    let found = match recalled {
        Some(found) => found,
        None => {
            // Both were taken before the read, so a change after them is seen
            // next time and never hidden by this answer.
            let lines_held = held_lines(ledger, session_id, &held.file)?;
            let mut bytes = Vec::new();
            if log.read_to_end(&mut bytes).is_err() {
                return Ok(Located::Not(NotLocated::LogUnreadable));
            }
            let found = place_in(&bytes, &held.file, &lines_held);
            memo().remember(&held, identity, found);
            found
        }
    };
    Ok(match found {
        Found::At(line) => Located::At(LogReceipt {
            file: mask_text(&held.file, record),
            line,
        }),
        Found::Not(why) => Located::Not(why),
    })
}

/// What one read of a log worked out: the line of the final claim, or why
/// there is none. Never a word of the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Found {
    At(u64),
    Not(NotLocated),
}

/// The place in `bytes`, the whole of the log file `file`, from the lines the
/// ledger holds (`lines_held`: line number to SHA-256).
fn place_in(bytes: &[u8], file: &str, lines_held: &BTreeMap<u64, String>) -> Found {
    let raw = raw_lines(bytes);

    // The same parser ingest used, over the same bytes; a line is kept only
    // when the ledger holds it and it still hashes to what was recorded.
    let mut stats = ParseStats::default();
    let Ok(parsed) = parse_reader(bytes, file, &mut stats) else {
        return Found::Not(NotLocated::LogUnreadable);
    };
    let mut kept: Vec<ParsedLine> = Vec::new();
    for mut line in parsed {
        if lines_held.get(&line.line) != Some(&line.sha256) {
            continue;
        }
        let agent = raw
            .get(&line.line)
            .filter(|b| sha256_hex(b) == line.sha256)
            .is_some_and(|b| role_marks_agent(b));
        if !agent {
            // Not the agent's by its own field. Everything else on the line
            // stays, so the turn boundary is the one `final_turn` would find.
            line.events.retain(|e| e.kind != EventKind::AssistantText);
        }
        kept.push(line);
    }
    if kept.is_empty() {
        return Found::Not(NotLocated::LogChanged);
    }

    let turn = crate::speaker::final_turn_of(&kept);
    match turn.claims.last() {
        Some(last) => Found::At(last.line),
        None => Found::Not(NotLocated::NoAgentClaim),
    }
}

/// True only when the line's own JSON says the agent spoke it: top-level
/// `type` is `"assistant"` and `message.role` is `"assistant"`. Missing,
/// malformed or disagreeing fields are not the agent.
fn role_marks_agent(line: &[u8]) -> bool {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(line) else {
        return false;
    };
    let kind = v.get("type").and_then(serde_json::Value::as_str);
    let role = v
        .get("message")
        .and_then(|m| m.get("role"))
        .and_then(serde_json::Value::as_str);
    kind == Some("assistant") && role == Some("assistant")
}

/// Line number → bytes without the terminator, numbered the way ingest
/// numbers them (`change_export::line_digests`): 1-based, whitespace-only
/// lines counted and skipped.
fn raw_lines(bytes: &[u8]) -> BTreeMap<u64, Vec<u8>> {
    let mut out = BTreeMap::new();
    for (i, chunk) in bytes.split_inclusive(|b| *b == b'\n').enumerate() {
        let mut line = chunk;
        if let Some(rest) = line.strip_suffix(b"\n") {
            line = rest;
            if let Some(rest) = line.strip_suffix(b"\r") {
                line = rest;
            }
        }
        if line.iter().all(|b| b.is_ascii_whitespace()) {
            continue;
        }
        out.insert(i as u64 + 1, line.to_vec());
    }
    out
}

/* ── what the ledger holds of one conversation ───────────────────────── */

/// What the ledger holds of one conversation, without fetching a line of it.
///
/// `events` rows are append-only (SQLite triggers refuse an update or a
/// delete), so a count and the highest sequence number name the set of rows
/// exactly, and the last row's `chain_hash` commits to the `raw_sha256` of
/// every row before it in the chain. Together they say whether the lines the
/// ledger holds are the lines it held the last time, at the cost of an index
/// scan rather than a read of every row's bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Held {
    session: String,
    events: i64,
    last_seq: i64,
    chain: String,
    /// The log of the last event: the file whose end is meant.
    file: String,
}

/// [`Held`] for `session_id`, or `None` when the ledger holds no event of it.
fn held_summary(ledger: &Ledger, session_id: &str) -> Result<Option<Held>> {
    let conn = ledger.conn();
    let (events, last_seq): (i64, Option<i64>) = conn.query_row(
        "SELECT COUNT(*), MAX(seq) FROM events WHERE session_id=?1",
        params![session_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let Some(last_seq) = last_seq else {
        return Ok(None);
    };
    let (file, chain): (String, String) = conn.query_row(
        "SELECT raw_file, chain_hash FROM events WHERE session_id=?1 AND seq=?2",
        params![session_id, last_seq],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(Some(Held {
        session: session_id.to_string(),
        events,
        last_seq,
        chain,
        file,
    }))
}

/// Every line of `file` the ledger holds for this conversation, line number to
/// the SHA-256 recorded for it. Read only when the log is.
fn held_lines(ledger: &Ledger, session_id: &str, file: &str) -> Result<BTreeMap<u64, String>> {
    let mut statement = ledger.conn().prepare(
        "SELECT raw_line, raw_sha256 FROM events
          WHERE session_id=?1 AND raw_file=?2 ORDER BY seq",
    )?;
    let rows = statement.query_map(params![session_id, file], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut lines = BTreeMap::new();
    for row in rows {
        let (line, sha) = row?;
        lines.insert(line.max(0) as u64, sha);
    }
    Ok(lines)
}

/* ── opening a log only if it is an ordinary file ────────────────────── */

/// Which file a path named, at the moment it was opened. Two looks that give
/// the same identity are looks at the same bytes unless a program put them
/// back under the system's clock, which it cannot do to the change time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Identity {
    device: u64,
    inode: u64,
    size: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}

impl Identity {
    #[cfg(unix)]
    fn of(meta: &std::fs::Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;
        Identity {
            device: meta.dev(),
            inode: meta.ino(),
            size: meta.len(),
            modified: (meta.mtime(), meta.mtime_nsec()),
            changed: (meta.ctime(), meta.ctime_nsec()),
        }
    }

    #[cfg(not(unix))]
    fn of(meta: &std::fs::Metadata) -> Self {
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or((0, 0), |d| {
                (
                    i64::try_from(d.as_secs()).unwrap_or(0),
                    i64::from(d.subsec_nanos()),
                )
            });
        Identity {
            device: 0,
            inode: 0,
            size: meta.len(),
            modified,
            changed: (0, 0),
        }
    }
}

/// `path` opened for reading, with the identity of the file opened, only when
/// it names an ordinary file. `None` for anything else, and for anything that
/// cannot be opened.
fn open_ordinary(path: &Path) -> Option<(File, Identity)> {
    // By name first: a pipe, a device or a socket is not opened at all.
    if !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let file = open_without_waiting(path).ok()?;
    // Then by the file actually opened, which a swap of the name between the
    // two looks cannot change.
    let meta = file.metadata().ok()?;
    meta.is_file().then(|| (file, Identity::of(&meta)))
}

/// Opening a named pipe for reading waits for a writer; with `O_NONBLOCK` it
/// does not. An ordinary file ignores the flag.
#[cfg(unix)]
fn open_without_waiting(path: &Path) -> std::io::Result<File> {
    use rustix::fs::{open, Mode, OFlags};
    let fd = open(
        path,
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOCTTY | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    Ok(File::from(fd))
}

#[cfg(not(unix))]
fn open_without_waiting(path: &Path) -> std::io::Result<File> {
    File::open(path)
}

/* ── what one read of a log worked out, remembered ───────────────────── */

/// How many logs the memory holds at once; past it, the one looked at longest
/// ago is forgotten. A Change links a handful of conversations, so this is
/// more than a person has open, and each entry is a few hundred bytes.
pub const MEMO_CAPACITY: usize = 64;

/// Fixed synchronization storage for cold reads. Two different paths can
/// share a stripe and wait for each other, but no number of distinct paths
/// can grow an in-flight map. Neither a failed read nor a panic leaves a
/// permanent in-flight entry; releasing the guard lets the next look retry.
const READ_LOCK_STRIPES: usize = 64;
static READ_LOCKS: [Mutex<()>; READ_LOCK_STRIPES] = [const { Mutex::new(()) }; READ_LOCK_STRIPES];

fn read_guard(path: &str) -> MutexGuard<'static, ()> {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    let stripe = (hasher.finish() as usize) % READ_LOCK_STRIPES;
    READ_LOCKS[stripe]
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// One log's remembered answer, and what it was worked out from.
struct Remembered {
    held: Held,
    identity: Identity,
    found: Found,
    /// How many times this log's bytes have been read while it was remembered.
    reads: u64,
    /// When it was last used, by `LogMemo`'s count of looks.
    used: u64,
}

/// The answers of recent looks, by log path. In this process only, at most
/// [`MEMO_CAPACITY`] of them, and never a byte of a log.
struct LogMemo {
    tick: u64,
    entries: BTreeMap<String, Remembered>,
}

impl LogMemo {
    const fn new() -> Self {
        LogMemo {
            tick: 0,
            entries: BTreeMap::new(),
        }
    }

    /// The remembered answer for this log, when it was worked out from the
    /// same ledger rows and the same file.
    fn recall(&mut self, held: &Held, identity: &Identity) -> Option<Found> {
        self.tick += 1;
        let entry = self.entries.get_mut(&held.file)?;
        if entry.held != *held || entry.identity != *identity {
            return None;
        }
        entry.used = self.tick;
        Some(entry.found)
    }

    /// Keep `found` as the answer for this log after a read of it, forgetting
    /// the log looked at longest ago if the memory is full.
    fn remember(&mut self, held: &Held, identity: Identity, found: Found) {
        self.tick += 1;
        if let Some(entry) = self.entries.get_mut(&held.file) {
            entry.held = held.clone();
            entry.identity = identity;
            entry.found = found;
            entry.reads += 1;
            entry.used = self.tick;
            return;
        }
        if self.entries.len() >= MEMO_CAPACITY {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.used)
                .map(|(file, _)| file.clone());
            if let Some(oldest) = oldest {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(
            held.file.clone(),
            Remembered {
                held: held.clone(),
                identity,
                found,
                reads: 1,
                used: self.tick,
            },
        );
    }

    fn reads_of(&self, file: &str) -> u64 {
        self.entries.get(file).map_or(0, |e| e.reads)
    }
}

static MEMO: Mutex<LogMemo> = Mutex::new(LogMemo::new());

/// The memory, locked. A lock poisoned by a panic elsewhere still guards a
/// well-formed map: no step of [`LogMemo`] can stop half way.
fn memo() -> MutexGuard<'static, LogMemo> {
    MEMO.lock().unwrap_or_else(PoisonError::into_inner)
}

/// How many times this log's bytes have been read in this process while the
/// memory held it: 0 for a path never read, and for one that is not an
/// ordinary file. For the tests that hold the locator to reading a log when it
/// changed and not otherwise.
pub fn reads_of(log: &Path) -> u64 {
    memo().reads_of(log.to_string_lossy().as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(file: &str, events: i64) -> Held {
        Held {
            session: "cccccccc-0000-4000-8000-00000000d0d0".to_string(),
            events,
            last_seq: events,
            chain: format!("chain-{events}"),
            file: file.to_string(),
        }
    }

    fn identity(size: u64) -> Identity {
        Identity {
            device: 1,
            inode: 2,
            size,
            modified: (3, 4),
            changed: (5, 6),
        }
    }

    const FOUND: Found = Found::At(4);

    #[test]
    fn the_memory_never_holds_more_than_its_capacity() {
        let mut memo = LogMemo::new();
        for n in 0..(MEMO_CAPACITY + 25) {
            memo.remember(&held(&format!("/log/{n}.jsonl"), 4), identity(10), FOUND);
            assert!(memo.entries.len() <= MEMO_CAPACITY);
        }
        assert_eq!(memo.entries.len(), MEMO_CAPACITY);
    }

    #[test]
    fn the_log_looked_at_longest_ago_is_the_one_forgotten() {
        let mut memo = LogMemo::new();
        for n in 0..MEMO_CAPACITY {
            memo.remember(&held(&format!("/log/{n}.jsonl"), 4), identity(10), FOUND);
        }
        // Looking at the first again makes the second the oldest.
        assert_eq!(
            memo.recall(&held("/log/0.jsonl", 4), &identity(10)),
            Some(FOUND)
        );
        memo.remember(&held("/log/new.jsonl", 4), identity(10), FOUND);
        assert_eq!(memo.entries.len(), MEMO_CAPACITY);
        assert!(memo.entries.contains_key("/log/0.jsonl"), "recently used");
        assert!(memo.entries.contains_key("/log/new.jsonl"), "just added");
        assert!(!memo.entries.contains_key("/log/1.jsonl"), "oldest");
    }

    #[test]
    fn an_answer_is_used_again_only_for_the_same_file_and_the_same_ledger_rows() {
        let mut memo = LogMemo::new();
        memo.remember(&held("/log/a.jsonl", 4), identity(10), FOUND);
        assert_eq!(
            memo.recall(&held("/log/a.jsonl", 4), &identity(10)),
            Some(FOUND)
        );
        // The file moved: any one part of its identity.
        for other in [
            Identity {
                device: 9,
                ..identity(10)
            },
            Identity {
                inode: 9,
                ..identity(10)
            },
            identity(11),
            Identity {
                modified: (3, 5),
                ..identity(10)
            },
            Identity {
                changed: (5, 7),
                ..identity(10)
            },
        ] {
            assert_eq!(memo.recall(&held("/log/a.jsonl", 4), &other), None);
        }
        // The ledger took in more, or other, rows.
        assert_eq!(
            memo.recall(&held("/log/a.jsonl", 5), &identity(10)),
            None,
            "more events"
        );
        let mut rewritten_chain = held("/log/a.jsonl", 4);
        rewritten_chain.chain = "another".to_string();
        assert_eq!(memo.recall(&rewritten_chain, &identity(10)), None);
        let mut another_session = held("/log/a.jsonl", 4);
        another_session.session = "cccccccc-0000-4000-8000-00000000d0d1".to_string();
        assert_eq!(memo.recall(&another_session, &identity(10)), None);
    }

    /// A named pipe is opened without waiting for a writer, and what is opened
    /// is not an ordinary file. The look through the handle in `open_ordinary`
    /// depends on both: a name swapped for a pipe between the look by name and
    /// the open must not hold the request.
    #[cfg(unix)]
    #[test]
    fn a_pipe_is_opened_without_waiting_and_is_not_an_ordinary_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let pipe = dir.path().join("pipe");
        let made = std::process::Command::new("mkfifo")
            .arg(&pipe)
            .status()
            .expect("mkfifo");
        assert!(made.success(), "mkfifo failed");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let looked = open_without_waiting(&pipe).and_then(|f| f.metadata());
            let _ = tx.send(looked.map(|m| m.is_file()));
        });
        let looked = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("opening a named pipe waited for a writer");
        assert!(
            !looked.expect("open"),
            "a named pipe read as an ordinary file"
        );
    }

    #[test]
    fn reads_are_counted_per_log_and_survive_a_refresh_of_its_answer() {
        let mut memo = LogMemo::new();
        assert_eq!(memo.reads_of("/log/a.jsonl"), 0);
        memo.remember(&held("/log/a.jsonl", 4), identity(10), FOUND);
        memo.remember(&held("/log/a.jsonl", 5), identity(11), FOUND);
        memo.remember(&held("/log/b.jsonl", 4), identity(10), FOUND);
        assert_eq!(memo.reads_of("/log/a.jsonl"), 2);
        assert_eq!(memo.reads_of("/log/b.jsonl"), 1);
    }
}
