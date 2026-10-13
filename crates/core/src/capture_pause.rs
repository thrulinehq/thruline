//! C34 R-LEDGER (7) — the pause gates' shared reading.
//!
//! A pause is recorded in `capture_holds` (`capture_hold`). This module is how
//! the six capture callers honour it WITHOUT touching the write path: the hook
//! delivery (`thruline sensor append`), the Stop audit, the HQ load, the
//! first-use register, the Codex live observer and `thruline ingest`. Every one
//! of them asks this module one question — may this be read? — and, when the
//! answer is no, leaves the thing alone and counts that it did. Nothing here
//! appends to the ledger, edits a mirror or opens a recording for anything but
//! its first lines, and `ingest.rs`, `sensor.rs`, `mirror.rs` and every
//! `change_*` writer are untouched by this card (C34 ruling Q6).
//!
//! THE RULE (C34 ruling Q5, design §6.2), the session-start rule:
//!
//! * While a folder is paused, nothing under it is read.
//! * After the pause is lifted, a recording whose SESSION STARTED inside the
//!   pause — its first timestamped transcript line, or its Codex
//!   `session_meta` — is never read. It is counted, as "started while paused".
//! * A session that started outside any pause is read in full, and the rows it
//!   timestamped inside a pause are MARKED `recorded_while_paused` when they
//!   are shown ([`RowMark`]) — a join at read time; nothing is written.
//!
//! NO PAUSE, NO EFFECT. With no pause or resume act in the record
//! ([`PauseGate::is_empty`]) every function here returns before it opens a
//! recording: the gates are byte-for-byte no-ops, and
//! `crates/cli/tests/capture_gates_noop.rs` is the proof.
//!
//! FAIL OPEN, WHERE LOSS IS FOR GOOD. [`paused_delivery`] answers `None` ("not
//! paused") on any error: a hook delivery lost to a failed read is lost
//! forever, and recording a delivery the person paused is the smaller harm. The
//! callers that can simply be run again ([`PauseGate::read`]) get the error.
//!
//! EVERY SKIP IS COUNTED. A load, a register and an ingest return
//! [`PauseCounts`]; a hook delivery and an audit, which have no one to say it
//! to, leave one line in a sidecar beside the ledger ([`note_skip`]) that
//! `thruline capture holds` and the capture document read back
//! ([`skip_count_for`]). The line names the paused folder and the time — never
//! a session, an event or a word of the paused work.

use std::collections::BTreeMap;
use std::io::{BufRead as _, Read as _, Write as _};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::capture_hold::{self, PauseWindow};
use crate::error::Result;
use crate::ledger::Ledger;

/// The sidecar the hook and the audit count their skips in, beside the ledger.
pub const SKIP_LOG: &str = "capture-pause-skips.jsonl";

/// How much of a recording is read to find where it started and what folder it
/// is in: its opening lines, never the rest.
const HEAD_BYTES: u64 = 512 * 1024;

/// What a recording's first lines say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Not under a pause, and not started in one: read it.
    Read,
    /// The folder is paused now: nothing under it is read.
    PausedByYou { folder: String },
    /// The folder is not paused now, but this session started while it was.
    StartedWhilePaused { folder: String },
}

struct Win {
    window: PauseWindow,
    /// The folder as the disk spells it (a symlink or `/private/var`).
    disk_folder: String,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
}

impl Win {
    fn covers(&self, path: &str, path_on_disk: &str) -> bool {
        capture_hold::path_is_inside(&self.window.folder, path)
            || capture_hold::path_is_inside(&self.disk_folder, path_on_disk)
    }

    fn open(&self) -> bool {
        self.window.to.is_none()
    }

    /// `t` is at or after the pause and, for a pause that has been lifted,
    /// before its resumption. A lifted pause whose times cannot be read spans
    /// nothing: no session is left unread on a guess.
    fn spans(&self, t: i64) -> bool {
        match (self.from_ms, self.open(), self.to_ms) {
            (Some(from), true, _) => t >= from,
            (Some(from), false, Some(to)) => t >= from && t < to,
            _ => false,
        }
    }
}

/// Every pause window the record holds, read once, ready to be asked about.
pub struct PauseGate {
    wins: Vec<Win>,
}

impl PauseGate {
    /// Read the record's pause windows. An `Err` is a record that could not be
    /// read; a caller that can be run again treats it as a refusal.
    pub fn read(ledger: &Ledger) -> Result<Self> {
        let wins = capture_hold::pause_windows(ledger)?
            .into_iter()
            .map(|window| {
                let disk_folder = capture_hold::on_disk(&window.folder);
                let from_ms = crate::hq::parse_ms(&window.from);
                let to_ms = window.to.as_deref().and_then(crate::hq::parse_ms);
                Win {
                    window,
                    disk_folder,
                    from_ms,
                    to_ms,
                }
            })
            .collect();
        Ok(Self { wins })
    }

    /// No pause or resume act has ever been recorded: every gate does nothing.
    pub fn is_empty(&self) -> bool {
        self.wins.is_empty()
    }

    /// Whether `cwd` is under a pause in force.
    pub fn paused_now(&self, cwd: &str) -> Option<&PauseWindow> {
        let on_disk = capture_hold::on_disk(cwd);
        self.wins
            .iter()
            .filter(|w| w.open() && w.covers(cwd, &on_disk))
            .max_by_key(|w| w.window.folder.len())
            .map(|w| &w.window)
    }

    /// May a recording in `cwd` that started at `started` be read? `started`
    /// is asked for only when a closed pause covers the folder.
    pub fn judge(&self, cwd: &str, started: impl FnOnce() -> Option<i64>) -> Verdict {
        if self.wins.is_empty() {
            return Verdict::Read;
        }
        let on_disk = capture_hold::on_disk(cwd);
        if let Some(w) = self
            .wins
            .iter()
            .filter(|w| w.open() && w.covers(cwd, &on_disk))
            .max_by_key(|w| w.window.folder.len())
        {
            return Verdict::PausedByYou {
                folder: w.window.folder.clone(),
            };
        }
        let closed: Vec<&Win> = self
            .wins
            .iter()
            .filter(|w| !w.open() && w.covers(cwd, &on_disk))
            .collect();
        if closed.is_empty() {
            return Verdict::Read;
        }
        match started() {
            Some(t) => match closed.iter().find(|w| w.spans(t)) {
                Some(w) => Verdict::StartedWhilePaused {
                    folder: w.window.folder.clone(),
                },
                None => Verdict::Read,
            },
            // A session whose start cannot be read is read: nothing is dropped
            // on a guess.
            None => Verdict::Read,
        }
    }

    /// [`PauseGate::judge`] for one recording on disk. The folder is the first
    /// `cwd` its opening lines state; a Claude Code folder that states none is
    /// taken by its name, exactly as the HQ load takes it.
    pub fn judge_file(&self, file: &Path) -> Verdict {
        if self.wins.is_empty() {
            return Verdict::Read;
        }
        let cwd = match crate::hq::recorded_cwd(file) {
            Some(c) => c,
            None => match self.by_folder_name(file) {
                Some(folder) => return Verdict::PausedByYou { folder },
                None => return Verdict::Read,
            },
        };
        self.judge(&cwd, || session_started_ms(file))
    }

    /// A Claude Code folder named exactly as a paused folder is mangled, for a
    /// recording that states no `cwd` at all.
    fn by_folder_name(&self, file: &Path) -> Option<String> {
        let name = file
            .parent()?
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())?;
        self.wins
            .iter()
            .filter(|w| w.open())
            .find(|w| crate::speaker::mangle_root(&w.window.folder) == name)
            .map(|w| w.window.folder.clone())
    }

    /// The pauses that cover a project's root, ready to mark rows by time.
    pub fn marks_for(&self, root: &str) -> RowMark {
        let on_disk = capture_hold::on_disk(root);
        RowMark {
            spans: self
                .wins
                .iter()
                .filter(|w| w.covers(root, &on_disk))
                .filter_map(|w| {
                    let from = w.from_ms?;
                    let to = if w.open() { None } else { Some(w.to_ms?) };
                    Some((from, to))
                })
                .collect(),
        }
    }
}

/// The stretches of a project's pauses, to mark a row against at read time.
#[derive(Debug, Clone, Default)]
pub struct RowMark {
    /// `(from, to)` in epoch milliseconds; `to` `None` while the pause stands.
    spans: Vec<(i64, Option<i64>)>,
}

impl RowMark {
    /// Nothing was ever paused for this project: no row is marked.
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// Whether a row timestamped `ts` was recorded inside a pause. A row with
    /// no readable timestamp is not marked.
    pub fn marked(&self, ts: Option<&str>) -> bool {
        let Some(t) = ts.and_then(crate::hq::parse_ms) else {
            return false;
        };
        self.spans
            .iter()
            .any(|&(from, to)| t >= from && to.is_none_or(|to| t < to))
    }
}

/// When a recording's session started: the first `timestamp` its opening lines
/// carry, at the top of a line (Claude Code) or in its `session_meta` payload
/// (Codex). `None` when the opening lines carry none.
pub fn session_started_ms(file: &Path) -> Option<i64> {
    let handle = std::fs::File::open(file).ok()?;
    if !handle.metadata().ok()?.is_file() {
        return None;
    }
    let mut reader = std::io::BufReader::new(handle.take(HEAD_BYTES));
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return None,
            Ok(_) => {}
        }
        let Ok(v) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };
        let ts = v
            .get("timestamp")
            .or_else(|| v.pointer("/payload/timestamp"))
            .and_then(Value::as_str);
        if let Some(ms) = ts.and_then(crate::hq::parse_ms) {
            return Some(ms);
        }
    }
}

/// What a gate left out, and why.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PauseCounts {
    /// Recordings left unread because their folder is paused now.
    pub paused_by_you: u64,
    /// Recordings left unread because their session started inside a pause.
    pub started_while_paused: u64,
    /// Per paused folder: `(paused_by_you, started_while_paused)`.
    #[serde(skip)]
    pub by_folder: BTreeMap<String, (u64, u64)>,
}

impl PauseCounts {
    /// Count one verdict. A `Read` counts nothing.
    pub fn add(&mut self, verdict: &Verdict) {
        match verdict {
            Verdict::Read => {}
            Verdict::PausedByYou { folder } => {
                self.paused_by_you += 1;
                self.by_folder.entry(folder.clone()).or_default().0 += 1;
            }
            Verdict::StartedWhilePaused { folder } => {
                self.started_while_paused += 1;
                self.by_folder.entry(folder.clone()).or_default().1 += 1;
            }
        }
    }

    pub fn total(&self) -> u64 {
        self.paused_by_you + self.started_while_paused
    }

    /// One plain line per paused folder that left something unread.
    pub fn lines(&self) -> Vec<String> {
        self.by_folder
            .iter()
            .map(|(folder, (paused, started))| {
                let mut parts = Vec::new();
                if *paused > 0 {
                    parts.push(format!("{paused} not read because the folder is paused"));
                }
                if *started > 0 {
                    parts.push(format!(
                        "{started} not read because the session started while recording was paused"
                    ));
                }
                format!("paused by you: {folder}: {}", parts.join("; "))
            })
            .collect()
    }
}

/// Keep the recordings a pause does not cover, and count the rest. With no
/// pause in the record every file is kept and none is opened.
pub fn filter_files(gate: &PauseGate, files: Vec<PathBuf>) -> (Vec<PathBuf>, PauseCounts) {
    let mut counts = PauseCounts::default();
    if gate.is_empty() {
        return (files, counts);
    }
    let mut kept = Vec::with_capacity(files.len());
    for file in files {
        let verdict = gate.judge_file(&file);
        counts.add(&verdict);
        if verdict == Verdict::Read {
            kept.push(file);
        }
    }
    (kept, counts)
}

/// Every Claude Code recording under a path `thruline ingest` was given: the
/// same files its discovery would hand to the reader.
pub fn claude_files(path: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for found in crate::ingest::discover(path)? {
        out.extend(found.sessions);
    }
    Ok(out)
}

/// Every Codex recording under a path: `*.jsonl` files, directories walked,
/// symlinks not followed, sorted (what Codex discovery takes).
pub fn codex_files(path: &Path) -> Vec<PathBuf> {
    fn walk(path: &Path, out: &mut Vec<PathBuf>) {
        let Ok(meta) = std::fs::symlink_metadata(path) else {
            return;
        };
        if meta.file_type().is_symlink() {
            return;
        }
        if meta.is_file() {
            if path.extension().is_some_and(|e| e == "jsonl") {
                out.push(path.to_path_buf());
            }
            return;
        }
        if !meta.is_dir() {
            return;
        }
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        let mut children: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
        children.sort();
        for child in children {
            walk(&child, out);
        }
    }
    let mut out = Vec::new();
    walk(path, &mut out);
    out.sort();
    out.dedup();
    out
}

/// The delivery's `cwd` and `transcript_path`, as a hook's payload states them.
fn delivery_fields(payload: &[u8]) -> (Option<String>, Option<String>) {
    let Ok(v) = serde_json::from_slice::<Value>(payload) else {
        return (None, None);
    };
    let field = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    (field("cwd"), field("transcript_path"))
}

/// Whether a hook delivery (or a Stop audit) must be left unrecorded because
/// its `cwd` is under a pause, or its session started inside one. Returns the
/// paused folder. **Every failure is "not paused"** (see the module doc).
pub fn paused_delivery(ledger: &Ledger, payload: &[u8]) -> Option<String> {
    let (cwd, transcript) = delivery_fields(payload);
    paused_session(ledger, cwd.as_deref(), transcript.as_deref())
}

/// [`paused_delivery`] from the fields already read.
pub fn paused_session(
    ledger: &Ledger,
    cwd: Option<&str>,
    transcript_path: Option<&str>,
) -> Option<String> {
    let cwd = cwd?;
    let gate = PauseGate::read(ledger).ok()?;
    if gate.is_empty() {
        return None;
    }
    match gate.judge(cwd, || {
        transcript_path.and_then(|t| session_started_ms(Path::new(t)))
    }) {
        Verdict::Read => None,
        Verdict::PausedByYou { folder } | Verdict::StartedWhilePaused { folder } => Some(folder),
    }
}

/// Which gate left a delivery out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipGate {
    HookDelivery,
    Audit,
}

impl SkipGate {
    fn word(self) -> &'static str {
        match self {
            Self::HookDelivery => "hook_delivery",
            Self::Audit => "audit",
        }
    }
}

/// Where the sidecar lives: beside the ledger, as the sensor's spill does.
pub fn skip_log_path(db: &Path) -> PathBuf {
    db.with_file_name(SKIP_LOG)
}

/// The one older file the sidecar keeps (see [`note_skip`]).
pub fn skip_log_rotation_path(db: &Path) -> PathBuf {
    db.with_file_name(format!("{SKIP_LOG}.1"))
}

/// The sidecar is rotated at this size (about 8,000 counted skips).
pub const SKIP_LOG_MAX_BYTES: u64 = 1024 * 1024;

/// Count one skipped delivery or audit. One short line, appended in one write;
/// best effort, and silent: a hook must never speak, and a failure to count is
/// the one thing this cannot tell the hook about. It names the folder the pause
/// is on and the time, nothing of the paused work.
pub fn note_skip(db: &Path, gate: SkipGate, folder: &str, at: &str) {
    let line = serde_json::json!({"at": at, "gate": gate.word(), "folder": folder});
    // Bounded: past SKIP_LOG_MAX_BYTES the file is renamed over its one rotation
    // (`.1`, atomic, so a hook that is appending at that moment loses nothing)
    // and counting starts a new file. At most two files are ever kept, so a pause
    // left on for months cannot grow this without bound, and what the capture
    // route reads each time stays small. The count then covers the retained part.
    let path = skip_log_path(db);
    if std::fs::metadata(&path).is_ok_and(|m| m.len() >= SKIP_LOG_MAX_BYTES) {
        let _ = std::fs::rename(&path, skip_log_rotation_path(db));
    }
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(skip_log_path(db))
        .and_then(|mut f| writeln!(f, "{line}"));
}

/// What the sidecar counted for one folder.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SkipCount {
    /// Hook deliveries not recorded while the folder was paused.
    pub hook_deliveries: u64,
    /// Stop audits not recorded while the folder was paused.
    pub audits: u64,
    pub last_at: Option<String>,
}

/// Read the sidecar back for one paused folder. A missing file counts zero; a
/// line that is not one of ours is ignored.
pub fn skip_count_for(db: &Path, folder: &str) -> SkipCount {
    let mut count = SkipCount::default();
    // the older file first, so `last_at` is the newest line read
    for path in [skip_log_rotation_path(db), skip_log_path(db)] {
        let Ok(file) = std::fs::File::open(path) else {
            continue;
        };
        count_lines(&mut count, folder, file);
    }
    count
}

fn count_lines(count: &mut SkipCount, folder: &str, file: std::fs::File) {
    for line in std::io::BufReader::new(file).lines().map_while(|l| l.ok()) {
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if v.get("folder").and_then(Value::as_str) != Some(folder) {
            continue;
        }
        match v.get("gate").and_then(Value::as_str) {
            Some("hook_delivery") => count.hook_deliveries += 1,
            Some("audit") => count.audits += 1,
            _ => continue,
        }
        count.last_at = v.get("at").and_then(Value::as_str).map(str::to_string);
    }
}
