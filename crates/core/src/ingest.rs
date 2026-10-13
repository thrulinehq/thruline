//! Discovery, idempotent ingest, and verification.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::adapter::{stable_source_project_id, stable_source_session_id, TranscriptAdapter};
use crate::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use crate::codex_ingest::{AcceptedCodexPrefix, CodexAdapter, PreparedCodexGeneration};
use crate::error::{CoreError, Result};
use crate::ledger::{
    AcceptedGeneration, CandidateAssessment, GenerationAcceptance, Ledger, RawDisposition,
    SourceProjectIdentity, SourceSessionIdentity,
};
use crate::mirror::{LineageDisposition, MirrorIdentity, MirrorStore, SealedMirror};
use crate::model::{MalformedLine, ProjectRef, RawLineRef, SessionRef, Source};
use crate::parse::{ParseStats, SessionLines};
use rusqlite::OptionalExtension;

/// A previously ingested line whose bytes no longer match the ledger.
///
/// Transcripts are append-only in practice, so this means the file was
/// rewritten. Reported loudly and never silently re-hashed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrityAlarm {
    pub file: String,
    pub line: u64,
    pub stored_sha256: String,
    pub found_sha256: String,
}

#[derive(Debug, Clone, Default)]
pub struct IngestReport {
    pub projects: Vec<String>,
    pub sessions: u64,
    pub lines_read: u64,
    pub lines_skipped_already_ingested: u64,
    pub new_events: u64,
    pub events_by_kind: BTreeMap<String, u64>,
    pub unknown_types: BTreeMap<String, u64>,
    pub malformed: Vec<MalformedLine>,
    pub integrity_alarms: Vec<IntegrityAlarm>,
    pub heads: BTreeMap<String, String>,
    pub elapsed_ms: u128,
    pub accepted_generations: u64,
    pub identical_observations: u64,
    pub rejected_candidates: u64,
    pub quarantined_envelopes: u64,
    pub dispositions: u64,
    pub suppressed_mirrors: u64,
    pub suppressed_replays: u64,
    pub fallback_messages: u64,
    pub adapter_malformed: u64,
    pub adapter_unknown: u64,
    pub spool_flushes: u64,
    pub max_spool_buffered_rows: u64,
    pub max_spool_buffered_bytes: u64,
    pub adapter_families: BTreeMap<String, u64>,
    pub candidate_statuses: BTreeMap<String, u64>,
    /// RB1.5 — claude_code seal-before-parse parity. Sealed = a new mirror
    /// generation was accepted; identical = the payload was already preserved;
    /// failures are loud, never silent (Rule 12).
    pub claude_sealed: u64,
    pub claude_seal_identical: u64,
    pub claude_seal_rejected: u64,
    pub claude_seal_failures: Vec<String>,
    /// A207 (MANDATE-v4 §13) — tool results whose content is a
    /// persisted-output reference to a side file: the transcript holds only
    /// an excerpt, so any search over these results is incomplete until the
    /// referenced bytes are ingested (open work, A207). Counted here so the
    /// incompleteness is disclosed, never silent.
    pub overflowed_tool_results: u64,
    /// Wall time per ingest phase, in nanoseconds, for the load-speed probe
    /// and `GET /api/hq/load`. Phases: `setup` (mirror open, lease, orphan
    /// scan), `seal` (capture: copy + hash passes), `seal_accept` (framing,
    /// lineage classification, generation acceptance), `parse`, `ledger`
    /// (session rows, line-hash compare, event append, provenance, head),
    /// `codex_prepare` (Codex mapping into the spool). Measurement only:
    /// nothing reads it to decide anything.
    pub phase_ns: BTreeMap<&'static str, u64>,
    /// C27: Codex generations whose mapping was made on a load's worker
    /// thread and used as made (the ledger still held no accepted generation
    /// for the session when its turn came).
    pub worker_mappings_used: u64,
    /// C27: Codex candidates refused before mapping because the live source
    /// had already changed since its capture (see `codex_stage2`).
    pub live_source_prechecks: u64,
}

impl IngestReport {
    fn time<T>(&mut self, phase: &'static str, started: std::time::Instant, out: T) -> T {
        *self.phase_ns.entry(phase).or_insert(0) += started.elapsed().as_nanos() as u64;
        out
    }
}

#[derive(Debug, Clone, Default)]
pub struct VerifyReport {
    pub projects: BTreeMap<String, ProjectVerdict>,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectVerdict {
    pub events_checked: u64,
    pub raw_matches: u64,
    /// RB1.5 — lines whose vendor file is gone (or rewritten) but whose bytes
    /// verify against the sealed mirror generation. The receipt survives the
    /// sweep through the mirror; counted separately so nobody mistakes a
    /// mirror pass for a live source.
    pub raw_matches_via_mirror: u64,
    /// Vendor files that are gone but fully covered by mirror generations.
    pub swept_files_preserved: Vec<String>,
    pub raw_mismatches: Vec<IntegrityAlarm>,
    pub chain_matches: u64,
    pub chain_mismatches: Vec<String>,
    pub missing_files: Vec<String>,
    /// Sessions whose whole-file digest or raw line count no longer matches.
    /// Catches rewrites of lines that produce no events — invisible to the
    /// per-event checks (§7 `sessions.raw_digest`).
    pub digest_mismatches: Vec<String>,
    pub digests_checked: u64,
    pub head_hash: String,
}

impl ProjectVerdict {
    pub fn ok(&self) -> bool {
        // A vendor file the mirror fully preserves is not a failure: the
        // receipts still re-read, through the seal (RB1.5). A missing file
        // WITHOUT mirror coverage still fails, loudly.
        self.raw_mismatches.is_empty()
            && self.chain_mismatches.is_empty()
            && self.missing_files.is_empty()
            && self.digest_mismatches.is_empty()
    }
}

impl VerifyReport {
    pub fn ok(&self) -> bool {
        self.projects.values().all(ProjectVerdict::ok)
    }
}

/// Default transcript root. Confirmed in M1 step 0 to be shared by the desktop
/// app and the terminal CLI (§8/A8).
pub fn default_roots() -> Vec<PathBuf> {
    match std::env::var_os("HOME") {
        Some(home) => vec![PathBuf::from(home).join(".claude/projects")],
        None => Vec::new(),
    }
}

pub fn default_codex_roots() -> Vec<PathBuf> {
    match std::env::var_os("HOME") {
        Some(home) => ["sessions", "archived_sessions"]
            .into_iter()
            .map(|name| PathBuf::from(&home).join(".codex").join(name))
            .filter(|path| path.exists())
            .collect(),
        None => Vec::new(),
    }
}

pub fn default_mirror_root() -> Result<PathBuf> {
    match std::env::var_os("HOME") {
        Some(home) => Ok(PathBuf::from(home).join(".thruline/mirrors")),
        None => Err(CoreError::Config("HOME is not set".to_string())),
    }
}

#[derive(Debug, Clone)]
pub struct CodexIngestOptions {
    pub mirror_root: PathBuf,
    pub owner_key: Option<String>,
}

pub fn default_db_path() -> Result<PathBuf> {
    match std::env::var_os("HOME") {
        Some(home) => Ok(PathBuf::from(home).join(".thruline/ledger.db")),
        None => Err(CoreError::Config("HOME is not set".to_string())),
    }
}

/// One project directory and its session files.
#[derive(Debug, Clone)]
pub struct Discovered {
    pub dir_name: String,
    pub dir: PathBuf,
    pub sessions: Vec<PathBuf>,
}

/// Accepts a transcript root, a single project directory, or one `.jsonl`.
pub fn discover(path: &Path) -> Result<Vec<Discovered>> {
    if path.is_file() {
        let dir = path.parent().unwrap_or(Path::new("."));
        return Ok(vec![Discovered {
            dir_name: dir_name_of(dir),
            dir: dir.to_path_buf(),
            sessions: vec![path.to_path_buf()],
        }]);
    }
    if !path.is_dir() {
        return Err(CoreError::Config(format!(
            "{} is neither a file nor a directory",
            path.display()
        )));
    }

    let own = jsonl_in(path)?;
    if !own.is_empty() {
        // The path IS a project directory.
        return Ok(vec![Discovered {
            dir_name: dir_name_of(path),
            dir: path.to_path_buf(),
            sessions: own,
        }]);
    }

    // Otherwise treat it as a root of project directories.
    let mut out = Vec::new();
    let entries = std::fs::read_dir(path).map_err(|e| CoreError::io(path, e))?;
    let mut dirs: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| CoreError::io(path, e))?;
        let p = entry.path();
        if p.is_dir() {
            dirs.push(p);
        }
    }
    dirs.sort();
    for d in dirs {
        let sessions = jsonl_in(&d)?;
        if !sessions.is_empty() {
            out.push(Discovered {
                dir_name: dir_name_of(&d),
                dir: d,
                sessions,
            });
        }
    }
    Ok(out)
}

fn jsonl_in(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| CoreError::io(dir, e))?;
    for entry in entries {
        let entry = entry.map_err(|e| CoreError::io(dir, e))?;
        let p = entry.path();
        if p.is_file() && p.extension().is_some_and(|e| e == "jsonl") {
            out.push(p);
        }
    }
    out.sort();
    Ok(out)
}

fn dir_name_of(dir: &Path) -> String {
    dir.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.display().to_string())
}

/// Session identity is **the file stem**, always.
///
/// Not the per-line `sessionId`: when a session is resumed, the new file opens
/// with a verbatim copy of the parent's transcript prefix, and those copied
/// lines keep the PARENT's `sessionId` (observed: `aaaaaaaa.jsonl` carries 810
/// lines stamped `bbbbbbbb` before 2,927 stamped with its own id). Trusting the
/// first `sessionId` in a file therefore binds the resumed file to its parent's
/// session and collides with the parent's own file — which surfaced as bogus
/// integrity alarms on a first ingest.
///
/// One file is one session. The per-line `sessionId` stays on disk and lets M2
/// recover the resume relationship.
fn session_id_of(file: &Path) -> String {
    file.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| file.display().to_string())
}

/// Ingest every session under `paths`, appending only what is new.
pub fn ingest(ledger: &mut Ledger, paths: &[PathBuf], mirror_root: &Path) -> Result<IngestReport> {
    let started = SystemTime::now();
    let mut report = IngestReport::default();

    // RB1.5 — mirror parity with the codex path: every source file is sealed
    // into the mirror store BEFORE any parse touches it. T7 measured why:
    // the vendor's ~30-day sweep destroys raw bytes, and for anything ingested
    // without a seal the ledger's parsed rows become the only copy (A150 P2).
    let t = std::time::Instant::now();
    let mirror = open_mirror(ledger, mirror_root)?;
    let mirror_lease = mirror.acquire_ingest_lease()?;
    recover_unaccepted(ledger, &mirror, &mirror_lease, Source::ClaudeCode)?;
    report.time("setup", t, ());

    ingest_claude_paths(ledger, &mirror, &mirror_lease, paths, &mut report)?;

    report.projects.sort();
    report.projects.dedup();
    report.elapsed_ms = started.elapsed().map(|d| d.as_millis()).unwrap_or(0);
    Ok(report)
}

fn open_mirror(ledger: &mut Ledger, mirror_root: &Path) -> Result<MirrorStore> {
    let mirror_authority = ledger.mirror_authority_key()?;
    MirrorStore::open_namespaced(mirror_root, &mirror_authority)
}

fn recover_unaccepted(
    ledger: &Ledger,
    mirror: &MirrorStore,
    lease: &crate::mirror::MirrorLease,
    source: Source,
) -> Result<()> {
    let accepted = ledger.accepted_generation_keys_for_source(source.as_str())?;
    mirror.recover_unaccepted_generations(lease, &accepted)?;
    Ok(())
}

fn ingest_claude_paths(
    ledger: &mut Ledger,
    mirror: &MirrorStore,
    mirror_lease: &crate::mirror::MirrorLease,
    paths: &[PathBuf],
    report: &mut IngestReport,
) -> Result<()> {
    let now = iso8601_utc_now();
    for path in paths {
        for project in discover(path)? {
            let project_id = project.dir_name.clone();
            ledger.upsert_project(
                &ProjectRef {
                    id: project_id.clone(),
                    root_path: demangle(&project.dir_name),
                    source: Source::ClaudeCode,
                    display_name: display_name(&project.dir_name),
                },
                &now,
            )?;
            report.projects.push(project_id.clone());

            for file in &project.sessions {
                report.sessions += 1;
                // Seal, then parse, append, and bind provenance: every event
                // — new this run or ingested before its session was ever
                // sealed — binds to the generation whose disposition rows
                // frame its line. The session's line digest is written last
                // (C27), so a digest that differs from the accepted payload's
                // says this whole sequence finished for those bytes.
                let staged = claude_stage1(mirror, mirror_lease.permit(), &project_id, file, false);
                claude_stage2(ledger, mirror, staged, report)?;
            }

            let t = std::time::Instant::now();
            let head = ledger.update_ledger_meta(&project_id, None)?;
            report.time("ledger", t, ());
            report.heads.insert(project_id, head);
        }
    }
    Ok(())
}

/// Files per mirror lease in a parallel load ([`LoadMirror::ingest_parallel`]).
pub const PARALLEL_CHUNK: usize = 32;
/// Source bytes staged ahead of the writer in a parallel load, at most.
pub const PARALLEL_BYTES_IN_FLIGHT: u64 = 256 * 1024 * 1024;

enum Staged {
    Claude(Box<ClaudeStaged>),
    Codex(Result<Vec<CodexStaged>>),
    /// Not staged on a worker: the writer runs it whole, at its turn.
    Serial,
}

impl Staged {
    /// Whether stage 2 will redo this file one at a time (a capture failed).
    fn needs_redo(&self) -> bool {
        match self {
            Staged::Claude(one) => one.captured.is_err(),
            Staged::Codex(Ok(list)) => list.iter().any(|one| one.captured.is_err()),
            Staged::Codex(Err(_)) => true,
            Staged::Serial => false,
        }
    }

    /// The same result with every worker-made Codex mapping thrown away.
    fn without_mappings(self) -> Staged {
        match self {
            Staged::Codex(Ok(list)) => Staged::Codex(Ok(list
                .into_iter()
                .map(|mut one| {
                    if let Some((_, Ok(prepared))) = one.fresh.take() {
                        let _ = prepared.spool.discard();
                    }
                    one
                })
                .collect())),
            other => other,
        }
    }

    /// Throw a stage-1 result away, removing any Codex spool it made.
    fn discard(self) {
        if let Staged::Codex(Ok(list)) = self {
            for staged in list {
                if let Some((_, Ok(prepared))) = staged.fresh {
                    let _ = prepared.spool.discard();
                }
            }
        }
    }
}

/// A Codex mapping made on a worker for a generation another recording of the
/// same chunk also reached is never used: its spool may have been replaced.
fn drop_contended(
    staged: Staged,
    claims: &std::sync::Mutex<std::collections::HashMap<String, bool>>,
) -> Staged {
    let Staged::Codex(Ok(list)) = staged else {
        return staged;
    };
    let list = list
        .into_iter()
        .map(|mut one| {
            let contended = match (&one.captured, claims.lock()) {
                (Ok(sealed), Ok(claims)) => {
                    claims.get(&sealed.generation_key).copied().unwrap_or(false)
                }
                _ => true,
            };
            if contended {
                if let Some((_, Ok(prepared))) = one.fresh.take() {
                    let _ = prepared.spool.discard();
                }
            }
            one
        })
        .collect();
    Staged::Codex(Ok(list))
}

/// How [`LoadMirror::unchanged_how`] decided a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unchanged {
    /// The file takes the full path.
    No,
    /// Its `stat` is exactly what the accepted generation's seal recorded:
    /// skipped without being opened.
    ByStat,
    /// Its `stat` differs from the seal, and one read of it proved its bytes
    /// are the accepted generation's (C27 (b)): opened once, not ingested.
    ByContent,
}

/// `(size, modification time, device, inode)`; `None` when the file cannot
/// be stat'd or the platform records no time, device or inode.
#[cfg(unix)]
fn stat_identity(file: &Path) -> Option<(u64, SystemTime, u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(file).ok()?;
    Some((meta.len(), meta.modified().ok()?, meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn stat_identity(_file: &Path) -> Option<(u64, SystemTime, u64, u64)> {
    None
}

/// Whether the full path would route the Codex recording at `file` to
/// `session_id`, judged without opening it.
///
/// The full path routes a recording by its owner key — derived from the path
/// alone (the nearest `.codex` folder, else the file's own folder, and that
/// folder's owner uid: [`CodexAdapter::owner_key_for`]) — and by the session
/// key in its first `session_meta`. A skip is only ever taken when the bytes
/// are the accepted generation's ([`Unchanged::ByStat`] by assumption,
/// [`Unchanged::ByContent`] by proof), so the session key is the one the
/// session was recorded under; what can differ is the owner, when a
/// recording is renamed into another owner's folder (found by its inode) or
/// was first ingested under an owner override. Any doubt answers `false`.
fn codex_routes_to(ledger: &Ledger, file: &Path, session_id: &str) -> bool {
    let adapter = CodexAdapter::new(None);
    let Some(owner) = adapter
        .discover(&[file.to_path_buf()])
        .ok()
        .and_then(|plans| plans.into_iter().next())
        .and_then(|plan| adapter.owner_key_for(&plan).ok())
    else {
        return false;
    };
    let key: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT source_session_key FROM source_sessions
             WHERE session_id = ?1 AND source = 'codex'",
            rusqlite::params![session_id],
            |r| r.get(0),
        )
        .optional()
        .ok()
        .flatten();
    key.is_some_and(|key| stable_source_session_id(&owner, Source::Codex, &key) == session_id)
}

/// Whether `Ledger::backfill_claude_event_provenance(session)` would insert
/// anything. Any doubt (a query that fails) answers `true`, so the file takes
/// the full path.
///
/// C29 R3 — asked on both skip branches ([`Unchanged::ByStat`] and
/// [`Unchanged::ByContent`]), because a skip must conclude exactly what a full
/// read concludes, and a full read always ends in that backfill. It is asked
/// in two steps:
///
/// 1. **The count screen.** The session's events, counted by the covering
///    `idx_events_session_ts` range, against the provenance rows bound
///    through the session's own generations, counted by the
///    `session_generations` autoindex and the covering
///    `idx_generation_provenance_generation` range. Equal counts answer
///    `false` without touching a single event row.
/// 2. **The exact question**, only when the counts differ: the SELECT of the
///    backfill's INSERT, asked as EXISTS. A session whose counts differ only
///    because some of its events have no disposition line in any of its
///    generations (the backfill could bind nothing) is still skipped.
///
/// The screen rests on one premise: **a provenance row always binds an event
/// of the same session as the generation it names.** Both writers keep it:
/// the backfill joins `g.session_id = e.session_id`, and the generation
/// acceptance writes each event and its provenance row for
/// `acceptance.session.id`'s own generation in one statement pair. With it,
/// and `event_id` the provenance key (one row per event), equal counts mean
/// every event of the session is bound, so the backfill has nothing to do.
/// `crates/core/tests/hq_load_content_skip.rs` pins both the premise on
/// stored rows and the census of writers.
fn claude_provenance_pending(ledger: &Ledger, session_id: &str) -> bool {
    let counts: Option<(i64, i64)> = ledger
        .conn()
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM events WHERE session_id = ?1),
               (SELECT COUNT(*)
                  FROM session_generations g
                  JOIN event_generation_provenance p ON p.generation_id = g.id
                 WHERE g.session_id = ?1)",
            rusqlite::params![session_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    if counts.is_some_and(|(events, bound)| events == bound) {
        return false;
    }
    ledger
        .conn()
        .query_row(
            "SELECT EXISTS(
               SELECT 1
               FROM events e
               JOIN session_generations g ON g.session_id = e.session_id
               JOIN generation_raw_dispositions d
                 ON d.generation_id = g.id AND d.raw_line = e.raw_line
               WHERE e.session_id = ?1
                 AND NOT EXISTS (
                   SELECT 1 FROM event_generation_provenance p WHERE p.event_id = e.id))",
            rusqlite::params![session_id],
            |r| r.get::<_, bool>(0),
        )
        .unwrap_or(true)
}

/// C27 — one load's mirror: opened, and its orphan scan run, once per load
/// instead of once per file.
///
/// `ingest` and `ingest_codex` each open the mirror, take the ingest lease and
/// walk every sealed generation in the store looking for orphans before they
/// read a single file. A load that called them once per file paid that walk
/// once per file — on a store of tens of thousands of generations, most of
/// the cost of a load that added nothing. A `LoadMirror` runs the walk once,
/// against the generations either source accepted, then takes the lease per
/// file (or per chunk, [`LoadMirror::ingest_parallel`]), so a hook that seals
/// between two files still can.
///
/// [`LoadMirror::unchanged`] is the other half: it says, from the ledger's
/// own rows and the accepted generation's seal, whether a file is byte-for-
/// byte what the ledger last read — without opening it.
pub struct LoadMirror {
    mirror: MirrorStore,
    /// Codex `source_locator` → ledger session, `None` when two sessions name
    /// the same path (ambiguous: the full path decides).
    codex_locators: std::collections::HashMap<String, Option<String>>,
    /// Codex `(device, inode)` → ledger session, read from the seals of the
    /// sessions' accepted generations — built the first time a Codex file's
    /// path is not a recorded locator (Codex moves finished rollouts into
    /// `archived_sessions/`; a rename keeps the inode and the bytes).
    codex_inodes: std::sync::OnceLock<std::collections::HashMap<(u64, u64), Option<String>>>,
    /// Whether this load's one orphan walk has run. It runs before the first
    /// capture, under that capture's lease — a load that captures nothing
    /// (every file unchanged) walks nothing.
    scanned: std::sync::atomic::AtomicBool,
    /// Accepted generation keys of both sources, read at open for the walk.
    accepted_keys: std::collections::BTreeSet<String>,
}

impl LoadMirror {
    pub fn open(ledger: &mut Ledger, mirror_root: &Path) -> Result<Self> {
        let mirror = open_mirror(ledger, mirror_root)?;
        // One walk, against every generation this ledger accepted from either
        // source. (`ingest` and `ingest_codex` each walk against their own
        // source's set only, so each marks every generation of the other
        // source as unreferenced — a private diagnostic marker with no
        // authority, written once and then re-read on every later walk: on
        // a store of 20,122 accepted Claude Code generations, every Codex
        // file's walk re-read 20,122 markers. The union marks only what this
        // ledger truly does not reference, and lists one directory entry for
        // the rest.)
        let mut accepted_keys =
            ledger.accepted_generation_keys_for_source(Source::ClaudeCode.as_str())?;
        accepted_keys.extend(ledger.accepted_generation_keys_for_source(Source::Codex.as_str())?);
        let mut codex_locators = std::collections::HashMap::new();
        let mut stmt = ledger.conn().prepare(
            "SELECT source_locator, session_id FROM source_sessions
             WHERE source = 'codex' AND source_locator IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (locator, session) = row?;
            codex_locators
                .entry(locator)
                .and_modify(|seen: &mut Option<String>| {
                    if seen.as_deref() != Some(session.as_str()) {
                        *seen = None;
                    }
                })
                .or_insert(Some(session));
        }
        drop(stmt);
        Ok(Self {
            mirror,
            codex_locators,
            codex_inodes: std::sync::OnceLock::new(),
            scanned: std::sync::atomic::AtomicBool::new(false),
            accepted_keys,
        })
    }

    /// The orphan walk, once per load, under a lease the caller holds.
    fn scan_once(&self, lease: &crate::mirror::MirrorLease) -> Result<()> {
        use std::sync::atomic::Ordering;
        if !self.scanned.swap(true, Ordering::SeqCst) {
            self.mirror
                .recover_unaccepted_generations(lease, &self.accepted_keys)?;
        }
        Ok(())
    }

    #[cfg(not(unix))]
    fn codex_session_by_inode(&self, _ledger: &Ledger, _file: &Path) -> Option<String> {
        None
    }

    #[cfg(unix)]
    fn codex_session_by_inode(&self, ledger: &Ledger, file: &Path) -> Option<String> {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::metadata(file).ok()?;
        let map = self.codex_inodes.get_or_init(|| {
            let mut map: std::collections::HashMap<(u64, u64), Option<String>> = Default::default();
            let sessions: Vec<String> = ledger
                .conn()
                .prepare(
                    "SELECT h.session_id FROM session_generation_heads h
                     JOIN source_sessions s ON s.session_id = h.session_id
                     WHERE s.source = 'codex'",
                )
                .and_then(|mut stmt| {
                    stmt.query_map([], |r| r.get::<_, String>(0))?
                        .collect::<std::result::Result<Vec<_>, _>>()
                })
                .unwrap_or_default();
            for session in sessions {
                let Ok(Some(generation)) = ledger.accepted_generation(&session) else {
                    continue;
                };
                let Some(seal) = self.mirror.read_seal(&sealed_from_generation(&generation)) else {
                    continue;
                };
                if let (Some(dev), Some(ino)) = (seal.source_device, seal.source_inode) {
                    map.entry((dev, ino))
                        .and_modify(|seen| {
                            if seen.as_deref() != Some(session.as_str()) {
                                *seen = None;
                            }
                        })
                        .or_insert(Some(session));
                }
            }
            map
        });
        map.get(&(meta.dev(), meta.ino())).cloned().flatten()
    }

    /// Whether `file` is exactly what the ledger last read and finished
    /// with, judged without opening it:
    ///
    /// * the session the file feeds has an accepted generation, and that
    ///   generation's seal recorded this file's current size, modification
    ///   time (to the nanosecond), device and inode
    ///   ([`MirrorStore::source_unchanged_since`]);
    /// * Claude Code only: the session row's line count is the generation's,
    ///   and its digest is the parse's line digest, not the payload digest
    ///   the generation's acceptance leaves there — so the parse, the event
    ///   append and the provenance binding all finished for those bytes
    ///   (`ingest_session` writes that digest last). Codex accepts the
    ///   generation and its events in one transaction, so its accepted
    ///   generation alone says so.
    /// * Claude Code only (C29 R3): no event of the session is left for
    ///   `backfill_claude_event_provenance` to bind
    ///   ([`claude_provenance_pending`]). A parse that finished before the
    ///   generation now holding its bytes was accepted leaves events unbound
    ///   while every field above matches.
    ///
    /// When this is `true`, a full ingest of the file would add nothing: the
    /// capture would be an identical observation and every line would already
    /// be in the ledger. Anything else — no row, no generation, a seal that
    /// cannot be read, any field that differs — is `false`, and the file takes
    /// the full path exactly as before.
    ///
    /// Codex: in both branches, the file must still route to the session it
    /// was found as — its owner key, from its path, is the session's
    /// ([`codex_routes_to`]). A recording renamed into another owner's folder
    /// is found by its inode, and the full path would accept it as a new
    /// session.
    ///
    /// C27 (b): when only the `stat` comparison fails, the file is read once
    /// and is still `true` if its bytes prove it unchanged
    /// ([`LoadMirror::unchanged_how`], [`Unchanged::ByContent`]). So `true`
    /// means "not ingested", and no longer always "not opened".
    pub fn unchanged(&self, ledger: &Ledger, file: &Path, codex: bool) -> bool {
        self.unchanged_how(ledger, file, codex) != Unchanged::No
    }

    /// [`LoadMirror::unchanged`], saying how it was decided.
    ///
    /// C27 (b): when the `stat` differs from the accepted seal, the file is
    /// read once — no copy, no parse, no mapping — and is still skipped when
    /// that read proves a full ingest would conclude "identical, nothing to
    /// add" ([`Unchanged::ByContent`]). On a real store most files a reload
    /// re-read every time had moved only in their device number (macOS
    /// renumbers a volume's `st_dev` across boots) or had their modification
    /// time touched with their bytes unchanged; the seal is content-addressed
    /// and never rewritten, so their `stat` could never match again.
    pub fn unchanged_how(&self, ledger: &Ledger, file: &Path, codex: bool) -> Unchanged {
        let shown = file.display().to_string();
        let session_id = if codex {
            match self.codex_locators.get(&shown) {
                Some(Some(session)) => session.clone(),
                Some(None) => return Unchanged::No,
                None => match self.codex_session_by_inode(ledger, file) {
                    Some(session) => session,
                    None => return Unchanged::No,
                },
            }
        } else {
            let session_id = session_id_of(file);
            let project = file.parent().map(dir_name_of).unwrap_or_default();
            let row: Option<(String, String, i64, Option<String>)> = ledger
                .conn()
                .query_row(
                    "SELECT project_id, file_path, raw_line_count, raw_digest
                     FROM sessions WHERE id = ?1",
                    rusqlite::params![session_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()
                .ok()
                .flatten();
            let Some((row_project, row_file, lines, digest)) = row else {
                return Unchanged::No;
            };
            let Ok(Some(generation)) = ledger.accepted_generation(&session_id) else {
                return Unchanged::No;
            };
            let parsed = digest
                .as_deref()
                .is_some_and(|d| d != generation.payload_sha256);
            if row_project != project
                || row_file != shown
                || lines.max(0) as u64 != generation.total_envelopes
                || !parsed
            {
                return Unchanged::No;
            }
            let sealed = sealed_from_generation(&generation);
            let same = if self.mirror.source_unchanged_since(file, &sealed) {
                Unchanged::ByStat
            } else {
                self.same_content(file, &generation, &sealed, digest.as_deref())
            };
            // The full path also binds each event to the generation line that
            // holds it (`backfill_claude_event_provenance`). A parse that
            // finished before the generation now holding its bytes was
            // accepted (a capture that failed on a live write, then a hook's
            // `seal_transcript`) left that to do, and the full path would do
            // it now. C29 R3: asked on both branches, the stat one included.
            if same != Unchanged::No && claude_provenance_pending(ledger, &session_id) {
                return Unchanged::No;
            }
            return same;
        };
        let Ok(Some(generation)) = ledger.accepted_generation(&session_id) else {
            return Unchanged::No;
        };
        if !codex_routes_to(ledger, file, &session_id) {
            return Unchanged::No;
        }
        let sealed = sealed_from_generation(&generation);
        if self.mirror.source_unchanged_since(file, &sealed) {
            return Unchanged::ByStat;
        }
        // A file of another size cannot hold the generation's bytes; decided
        // without opening it (a live rollout that grew takes the full path).
        if stat_identity(file).map(|s| s.0) != Some(generation.payload_bytes) {
            return Unchanged::No;
        }
        self.same_content(file, &generation, &sealed, None)
    }

    /// C27 (b) — whether `file`'s bytes, read once now, are exactly the
    /// accepted generation's: the same length and SHA-256 — the condition
    /// under which [`MirrorStore::classify`] answers `Identical` and ingest
    /// appends nothing. For Claude Code (`row_digest` given), the session
    /// row's line digest must also be these bytes' digest, so the parse the
    /// full path would run again over them has already run and finished
    /// (`ingest_session` writes that digest last) and would rewrite the row
    /// with the values it already holds.
    ///
    /// The seal must still read and check against its accepted digest, as
    /// the stat comparison requires. The file must not move while it is read:
    /// its size, modification time, device and inode are compared before and
    /// after. A size that is not the generation's is answered without
    /// reading. Any doubt answers [`Unchanged::No`].
    fn same_content(
        &self,
        file: &Path,
        generation: &AcceptedGeneration,
        sealed: &SealedMirror,
        row_digest: Option<&str>,
    ) -> Unchanged {
        if self.mirror.read_seal(sealed).is_none() {
            return Unchanged::No;
        }
        let Some(before) = stat_identity(file) else {
            return Unchanged::No;
        };
        if before.0 != generation.payload_bytes {
            return Unchanged::No;
        }
        let Ok(read) = file_digests(file) else {
            return Unchanged::No;
        };
        if stat_identity(file) != Some(before) {
            return Unchanged::No;
        }
        let same = read.bytes == generation.payload_bytes
            && read.payload_sha256 == generation.payload_sha256
            && read.raw_lines == generation.total_envelopes
            && row_digest.is_none_or(|digest| digest == read.raw_digest);
        if same {
            Unchanged::ByContent
        } else {
            Unchanged::No
        }
    }

    /// `ingest` for one Claude Code transcript, without the per-call orphan
    /// scan [`LoadMirror::open`] already ran.
    pub fn ingest_claude(&self, ledger: &mut Ledger, file: &Path) -> Result<IngestReport> {
        let started = SystemTime::now();
        let mut report = IngestReport::default();
        let t = std::time::Instant::now();
        let lease = self.mirror.acquire_ingest_lease()?;
        self.scan_once(&lease)?;
        report.time("setup", t, ());
        ingest_claude_paths(
            ledger,
            &self.mirror,
            &lease,
            &[file.to_path_buf()],
            &mut report,
        )?;
        report.projects.sort();
        report.projects.dedup();
        report.elapsed_ms = started.elapsed().map(|d| d.as_millis()).unwrap_or(0);
        Ok(report)
    }

    /// `ingest_codex` (no owner override) for one Codex rollout, without the
    /// per-call orphan scan [`LoadMirror::open`] already ran.
    pub fn ingest_codex(&self, ledger: &mut Ledger, file: &Path) -> Result<IngestReport> {
        let started = SystemTime::now();
        let mut report = IngestReport::default();
        let adapter = CodexAdapter::new(None);
        let t = std::time::Instant::now();
        let lease = self.mirror.acquire_ingest_lease()?;
        self.scan_once(&lease)?;
        report.time("setup", t, ());
        ingest_codex_paths(
            ledger,
            &adapter,
            &self.mirror,
            &lease,
            &[file.to_path_buf()],
            CodexSeams::default(),
            &mut report,
        )?;
        report.projects.sort();
        report.projects.dedup();
        report.elapsed_ms = started.elapsed().map(|d| d.as_millis()).unwrap_or(0);
        Ok(report)
    }

    /// Ingest `files` — each `(path, is_codex)`, every one needing work — with
    /// stage 1 (capture, framing, parse, and a Codex mapping made as for a
    /// session the ledger has never accepted) on `workers` threads and stage 2
    /// (everything that reads or writes the ledger) on this thread, strictly
    /// in the order given. `on_file(i, outcome)` is called for `files[i]`
    /// right after its commit, in order; returning `false` stops the load.
    ///
    /// The result is the result of ingesting the same files one at a time in
    /// the same order: stage 1 reads nothing from the ledger, and stage 2 uses
    /// a stage-1 Codex mapping only when the ledger still holds no accepted
    /// generation for that session (else it maps again, as the one-at-a-time
    /// path does). Any stage-1 capture failure is redone here, one file at a
    /// time, so a race between two workers can never become a refusal the
    /// one-at-a-time path would not have made.
    ///
    /// One mirror lease is held per chunk of [`PARALLEL_CHUNK`] files —
    /// captures and Codex spools in flight must not meet the recovery pass
    /// that taking a lease runs — and released between chunks, so a hook that
    /// seals a transcript waits at most one chunk. Stage-1 results in flight
    /// are bounded by [`PARALLEL_BYTES_IN_FLIGHT`] bytes of source.
    pub fn ingest_parallel(
        &self,
        ledger: &mut Ledger,
        files: &[(PathBuf, bool)],
        workers: usize,
        on_file: &mut dyn FnMut(usize, Result<IngestReport>) -> bool,
    ) -> bool {
        let workers = workers.max(1);
        let adapter = CodexAdapter::new(None);
        let mut base = 0usize;
        while base < files.len() {
            let chunk = &files[base..(base + PARALLEL_CHUNK).min(files.len())];
            let lease = match self
                .mirror
                .acquire_ingest_lease()
                .and_then(|lease| self.scan_once(&lease).map(|()| lease))
            {
                Ok(lease) => lease,
                Err(e) => {
                    // The whole chunk is refused for the same reason; report
                    // it once per file, in order, and go on to the next.
                    let msg = e.to_string();
                    let busy = e.is_busy();
                    for i in 0..chunk.len() {
                        let err = if busy {
                            CoreError::Sqlite(rusqlite::Error::SqliteFailure(
                                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
                                Some(msg.clone()),
                            ))
                        } else {
                            CoreError::Integrity(msg.clone())
                        };
                        if !on_file(base + i, Err(err)) {
                            return false;
                        }
                    }
                    base += chunk.len();
                    continue;
                }
            };
            let go_on =
                self.parallel_chunk(ledger, &adapter, &lease, chunk, base, workers, on_file);
            drop(lease);
            if !go_on {
                return false;
            }
            base += chunk.len();
        }
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn parallel_chunk(
        &self,
        ledger: &mut Ledger,
        adapter: &CodexAdapter,
        lease: &crate::mirror::MirrorLease,
        chunk: &[(PathBuf, bool)],
        base: usize,
        workers: usize,
        on_file: &mut dyn FnMut(usize, Result<IngestReport>) -> bool,
    ) -> bool {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{mpsc, Condvar, Mutex};
        let permit = lease.permit();
        let mirror = &self.mirror;
        let sizes: Vec<u64> = chunk
            .iter()
            .map(|(p, _)| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
            .collect();
        // Two Codex recordings can share a mirror generation — and so one
        // seal, which names the source file captured FIRST — only when they
        // hold the same bytes, so only when they are the same size. Those are
        // never captured on workers: the writer runs each whole, in order,
        // so the first in load order is the first captured, as one at a time.
        let mut codex_sizes: std::collections::HashMap<u64, usize> = Default::default();
        for (i, (_, codex)) in chunk.iter().enumerate() {
            if *codex {
                *codex_sizes.entry(sizes[i]).or_insert(0) += 1;
            }
        }
        let serial: Vec<bool> = chunk
            .iter()
            .enumerate()
            .map(|(i, (_, codex))| *codex && codex_sizes.get(&sizes[i]).copied().unwrap_or(0) > 1)
            .collect();
        // (next index to claim, source bytes staged and not yet committed)
        let gate = (Mutex::new((0usize, 0u64)), Condvar::new());
        let stop = AtomicBool::new(false);
        // generation key -> contended (a second recording reached it too).
        let claims: Mutex<std::collections::HashMap<String, bool>> = Mutex::new(Default::default());
        let claim = |key: &str| -> bool {
            let Ok(mut claims) = claims.lock() else {
                return false;
            };
            match claims.get_mut(key) {
                Some(contended) => {
                    *contended = true;
                    false
                }
                None => {
                    claims.insert(key.to_string(), false);
                    true
                }
            }
        };
        let mut go_on = true;
        let mut tainted = false;
        std::thread::scope(|scope| {
            let (tx, rx) = mpsc::channel::<(usize, Staged)>();
            for _ in 0..workers.min(chunk.len()) {
                let tx = tx.clone();
                let (gate, stop, sizes, claim, serial) = (&gate, &stop, &sizes, &claim, &serial);
                scope.spawn(move || loop {
                    let i = {
                        let Ok(mut g) = gate.0.lock() else { return };
                        loop {
                            if stop.load(Ordering::SeqCst) || g.0 >= chunk.len() {
                                return;
                            }
                            let size = sizes[g.0];
                            if g.1 == 0 || g.1 + size <= PARALLEL_BYTES_IN_FLIGHT {
                                break;
                            }
                            g = match gate.1.wait(g) {
                                Ok(g) => g,
                                Err(_) => return,
                            };
                        }
                        let i = g.0;
                        g.0 += 1;
                        g.1 += sizes[i];
                        i
                    };
                    let (file, codex) = &chunk[i];
                    let staged = if serial[i] {
                        Staged::Serial
                    } else if *codex {
                        let fresh = !self
                            .codex_locators
                            .contains_key(&file.display().to_string());
                        Staged::Codex(adapter.discover(std::slice::from_ref(file)).map(|plans| {
                            plans
                                .into_iter()
                                .map(|plan| {
                                    codex_stage1(adapter, mirror, permit, plan, fresh, claim)
                                })
                                .collect()
                        }))
                    } else {
                        let project_id = file.parent().map(dir_name_of).unwrap_or_default();
                        Staged::Claude(Box::new(claude_stage1(
                            mirror,
                            permit,
                            &project_id,
                            file,
                            true,
                        )))
                    };
                    if tx.send((i, staged)).is_err() {
                        return;
                    }
                });
            }
            drop(tx);
            let mut ready: BTreeMap<usize, Staged> = BTreeMap::new();
            let mut want = 0usize;
            for (i, staged) in rx.iter() {
                ready.insert(i, staged);
                while let Some(staged) = ready.remove(&want) {
                    let (file, codex) = &chunk[want];
                    let outcome = if go_on {
                        // After a file of this chunk was redone one at a time
                        // (a stage-1 capture failed, so its generation was
                        // never registered), no worker mapping is trusted.
                        let staged = if tainted {
                            staged.without_mappings()
                        } else {
                            drop_contended(staged, &claims)
                        };
                        if *codex && staged.needs_redo() {
                            tainted = true;
                        }
                        self.commit_staged(ledger, adapter, lease, file, *codex, staged)
                    } else {
                        staged.discard();
                        Ok(IngestReport::default())
                    };
                    if let Ok(mut g) = gate.0.lock() {
                        g.1 = g.1.saturating_sub(sizes[want]);
                    }
                    gate.1.notify_all();
                    if go_on && !on_file(base + want, outcome) {
                        go_on = false;
                        stop.store(true, Ordering::SeqCst);
                        gate.1.notify_all();
                    }
                    want += 1;
                }
            }
            for (_, staged) in std::mem::take(&mut ready) {
                staged.discard();
            }
        });
        go_on
    }

    fn commit_staged(
        &self,
        ledger: &mut Ledger,
        adapter: &CodexAdapter,
        lease: &crate::mirror::MirrorLease,
        file: &Path,
        codex: bool,
        staged: Staged,
    ) -> Result<IngestReport> {
        let started = SystemTime::now();
        let mut report = IngestReport::default();
        let one = [file.to_path_buf()];
        match staged {
            Staged::Claude(staged) if staged.captured.is_ok() => {
                let now = iso8601_utc_now();
                let project_id = staged.project_id.clone();
                ledger.upsert_project(
                    &ProjectRef {
                        id: project_id.clone(),
                        root_path: demangle(&project_id),
                        source: Source::ClaudeCode,
                        display_name: display_name(&project_id),
                    },
                    &now,
                )?;
                report.projects.push(project_id.clone());
                report.sessions += 1;
                claude_stage2(ledger, &self.mirror, *staged, &mut report)?;
                let t = std::time::Instant::now();
                let head = ledger.update_ledger_meta(&project_id, None)?;
                report.time("ledger", t, ());
                report.heads.insert(project_id, head);
            }
            Staged::Codex(Ok(list)) if list.iter().all(|s| s.captured.is_ok()) => {
                for staged in list {
                    codex_stage2(
                        ledger,
                        adapter,
                        &self.mirror,
                        staged,
                        CodexSeams::default(),
                        &mut report,
                    )?;
                }
            }
            // A stage-1 failure is redone here, one file at a time, exactly as
            // the one-at-a-time path would run it.
            other => {
                other.discard();
                if codex {
                    ingest_codex_paths(
                        ledger,
                        adapter,
                        &self.mirror,
                        lease,
                        &one,
                        CodexSeams::default(),
                        &mut report,
                    )?;
                } else {
                    ingest_claude_paths(ledger, &self.mirror, lease, &one, &mut report)?;
                }
            }
        }
        report.projects.sort();
        report.projects.dedup();
        report.elapsed_ms = started.elapsed().map(|d| d.as_millis()).unwrap_or(0);
        Ok(report)
    }
}

/// C1.3 (A170) — the compaction-insurance seal: one transcript, sealed into
/// the mirror the moment the vendor is about to lossy-summarize it. The
/// sensor's pre-compaction appender calls this with the payload's own
/// `transcript_path`; everything else is the RB1.5 machinery unchanged —
/// capture, lineage against the accepted generation, atomic acceptance.
/// Idempotent: an identical file is an identical observation, not a new
/// generation. The vendor session must never pay for a failure here, so the
/// caller treats any error as loud-best-effort.
pub fn seal_transcript(
    ledger: &mut Ledger,
    mirror_root: &Path,
    transcript: &Path,
) -> Result<IngestReport> {
    let now = iso8601_utc_now();
    let mut report = IngestReport::default();
    let project_id = transcript
        .parent()
        .and_then(|d| d.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| {
            CoreError::Config(format!(
                "transcript {} has no project directory",
                transcript.display()
            ))
        })?;
    ledger.upsert_project(
        &ProjectRef {
            id: project_id.clone(),
            root_path: demangle(&project_id),
            source: Source::ClaudeCode,
            display_name: display_name(&project_id),
        },
        &now,
    )?;
    let mirror_authority = ledger.mirror_authority_key()?;
    let mirror = MirrorStore::open_namespaced(mirror_root, &mirror_authority)?;
    let lease = mirror.acquire_ingest_lease()?;
    let accepted = ledger.accepted_generation_keys_for_source(Source::ClaudeCode.as_str())?;
    mirror.recover_unaccepted_generations(&lease, &accepted)?;
    report.sessions = 1;
    let staged = claude_stage1(&mirror, lease.permit(), &project_id, transcript, false);
    accept_claude_seal(
        ledger,
        &mirror,
        staged.captured,
        None,
        &project_id,
        transcript,
        &mut report,
    )?;
    Ok(report)
}

/// Ingest Codex rollout sessions through sealed generation authority.
pub fn ingest_codex(
    ledger: &mut Ledger,
    paths: &[PathBuf],
    options: &CodexIngestOptions,
) -> Result<IngestReport> {
    ingest_codex_inner(ledger, paths, options, CodexSeams::default())
}

/// Exercise a source-admission failure before any candidate rows can enter the
/// atomic ledger transaction. This seam is deliberately absent from CLI,
/// configuration, environment, and persisted product state.
#[doc(hidden)]
pub fn ingest_codex_with_prepare_failpoint(
    ledger: &mut Ledger,
    paths: &[PathBuf],
    options: &CodexIngestOptions,
    fail_at_raw_line: u64,
) -> Result<IngestReport> {
    ingest_codex_inner(
        ledger,
        paths,
        options,
        CodexSeams {
            fail_at_raw_line: Some(fail_at_raw_line),
            ..CodexSeams::default()
        },
    )
}

/// Test-only seam for the C27 live-source precheck: `after_capture` runs on
/// each source file after its capture and before the precheck (a test appends
/// to the file there, as a live writer would); the two flags turn off the
/// check before the mapping and the watch during it, so each path can be
/// compared with the path that has neither. Absent from
/// CLI, configuration, environment and persisted state, like the failpoint.
#[doc(hidden)]
pub fn ingest_codex_with_live_seam(
    ledger: &mut Ledger,
    paths: &[PathBuf],
    options: &CodexIngestOptions,
    after_capture: &dyn Fn(&Path),
    check_before_mapping: bool,
    watch_during_mapping: bool,
) -> Result<IngestReport> {
    ingest_codex_inner(
        ledger,
        paths,
        options,
        CodexSeams {
            fail_at_raw_line: None,
            after_capture: Some(after_capture),
            check_before_mapping,
            watch_during_mapping,
        },
    )
}

/// What the test seams change; the product always runs [`CodexSeams::default`].
#[derive(Clone, Copy)]
pub(crate) struct CodexSeams<'a> {
    fail_at_raw_line: Option<u64>,
    after_capture: Option<&'a dyn Fn(&Path)>,
    check_before_mapping: bool,
    watch_during_mapping: bool,
}

impl Default for CodexSeams<'_> {
    fn default() -> Self {
        CodexSeams {
            fail_at_raw_line: None,
            after_capture: None,
            check_before_mapping: true,
            watch_during_mapping: true,
        }
    }
}

fn ingest_codex_inner(
    ledger: &mut Ledger,
    paths: &[PathBuf],
    options: &CodexIngestOptions,
    seams: CodexSeams<'_>,
) -> Result<IngestReport> {
    let started = SystemTime::now();
    let mut report = IngestReport::default();
    let adapter = CodexAdapter::new(options.owner_key.clone());
    let t = std::time::Instant::now();
    let mirror = open_mirror(ledger, &options.mirror_root)?;
    let mirror_lease = mirror.acquire_ingest_lease()?;
    recover_unaccepted(ledger, &mirror, &mirror_lease, Source::Codex)?;
    report.time("setup", t, ());
    ingest_codex_paths(
        ledger,
        &adapter,
        &mirror,
        &mirror_lease,
        paths,
        seams,
        &mut report,
    )?;
    report.projects.sort();
    report.projects.dedup();
    report.elapsed_ms = started.elapsed().map(|d| d.as_millis()).unwrap_or(0);
    Ok(report)
}

fn ingest_codex_paths(
    ledger: &mut Ledger,
    adapter: &CodexAdapter,
    mirror: &MirrorStore,
    mirror_lease: &crate::mirror::MirrorLease,
    paths: &[PathBuf],
    seams: CodexSeams<'_>,
    report: &mut IngestReport,
) -> Result<()> {
    let t = std::time::Instant::now();
    let plans = adapter.discover(paths)?;
    report.time("setup", t, ());

    for plan in plans {
        let staged = codex_stage1(adapter, mirror, mirror_lease.permit(), plan, false, &|_| {
            false
        });
        codex_stage2(ledger, adapter, mirror, staged, seams, report)?;
    }
    Ok(())
}

/// C27 — a Codex recording's first stage: everything before the first ledger
/// read. Capture into the mirror, the sealed route, and — when `fresh` — the
/// lineage and mapping as for a session the ledger has never accepted. It
/// reads nothing from the ledger and writes nothing to it, so a load runs it
/// on worker threads; stage 2 ([`codex_stage2`]) does the rest, in order, on
/// the one thread that writes.
pub(crate) struct CodexStaged {
    plan: crate::adapter::SessionPlan,
    owner_key: Result<String>,
    unrouted_key: String,
    capture_route: Option<crate::adapter::LogicalRoute>,
    identity: MirrorIdentity,
    captured: Result<SealedMirror>,
    /// When captured: the payload path and its line count, or the error that
    /// stopped stage 1 there.
    sealed: Result<(PathBuf, u64)>,
    /// `Some(route)` when the sealed bytes route exactly as the capture did.
    sealed_route: Option<crate::adapter::LogicalRoute>,
    fresh: Option<(LineageDisposition, Result<PreparedCodexGeneration>)>,
    phase_ns: BTreeMap<&'static str, u64>,
}

pub(crate) fn codex_stage1(
    adapter: &CodexAdapter,
    mirror: &MirrorStore,
    permit: crate::mirror::CapturePermit<'_>,
    plan: crate::adapter::SessionPlan,
    fresh: bool,
    claim: &dyn Fn(&str) -> bool,
) -> CodexStaged {
    let mut phase_ns = BTreeMap::new();
    let t = std::time::Instant::now();
    let owner_key = adapter.owner_key_for(&plan);
    let unrouted_key = format!(
        "unrouted_{}",
        sha256_hex(plan.path.display().to_string().as_bytes())
    );
    // Capture placement may use only the first authoritative session_meta.
    // Full routing and ambiguity checks are repeated from sealed bytes.
    let capture_route = adapter.inspect_capture_route(&plan).ok();
    let fallback_owner = owner_key.as_ref().map(String::clone).unwrap_or_default();
    let identity = MirrorIdentity {
        owner_key: capture_route
            .as_ref()
            .map_or_else(|| fallback_owner.clone(), |route| route.owner_key.clone()),
        source_project_key: capture_route.as_ref().map_or_else(
            || unrouted_key.clone(),
            |route| route.source_project_key.clone(),
        ),
        source_session_key: capture_route.as_ref().map_or_else(
            || unrouted_key.clone(),
            |route| route.source_session_key.clone(),
        ),
    };
    let captured = if owner_key.is_ok() {
        mirror.capture_permitted(permit, &plan.path, &identity)
    } else {
        Err(CoreError::Integrity("owner key unavailable".to_string()))
    };
    *phase_ns.entry("seal").or_insert(0) += t.elapsed().as_nanos() as u64;
    let t = std::time::Instant::now();
    let mut staged_sealed: Result<(PathBuf, u64)> =
        Err(CoreError::Integrity("not captured".to_string()));
    let mut sealed_route = None;
    let mut fresh_out = None;
    if let Ok(sealed) = &captured {
        staged_sealed = mirror.resolve_payload(sealed).and_then(|payload| {
            let lines = adapter.count_sealed_lines(mirror, sealed)?;
            Ok((payload, lines))
        });
        if let Ok((payload_path, _)) = &staged_sealed {
            sealed_route = match adapter.inspect_sealed(&plan, payload_path) {
                Ok(route) if capture_route.as_ref() == Some(&route) => Some(route),
                _ => None,
            };
        }
        // A spool is named by its generation, so two recordings with the same
        // bytes for the same session must never map at once: only the first
        // to claim the generation maps here, and the writer maps the rest.
        // Every recording registers its generation, mapped here or not, so a
        // writer-side mapping of the same generation always marks it
        // contended.
        let claimed = claim(&sealed.generation_key);
        if fresh && claimed && !mirror.source_changed_since(&plan.path, sealed) {
            if let (Some(route), Ok(lineage)) = (&sealed_route, mirror.classify(sealed, None)) {
                *phase_ns.entry("seal_accept").or_insert(0) += t.elapsed().as_nanos() as u64;
                let t = std::time::Instant::now();
                let no_slots: Vec<(String, String)> = Vec::new();
                // A mapping whose source moves meanwhile gives up; the writer
                // then redoes it, and refuses the candidate there.
                let seal = mirror.read_seal(sealed);
                let moved = || {
                    seal.as_ref()
                        .is_some_and(|seal| MirrorStore::source_moved_from(&plan.path, seal))
                };
                let prepared = adapter.prepare_generation_watching(
                    mirror,
                    sealed,
                    route,
                    AcceptedCodexPrefix {
                        payload_bytes: 0,
                        raw_lines: 0,
                        reject_malformed_suffix: false,
                        available_message_slots: &no_slots,
                    },
                    &moved,
                );
                *phase_ns.entry("codex_prepare").or_insert(0) += t.elapsed().as_nanos() as u64;
                fresh_out = Some((lineage, prepared));
            }
        }
    }
    if fresh_out.is_none() {
        *phase_ns.entry("seal_accept").or_insert(0) += t.elapsed().as_nanos() as u64;
    }
    CodexStaged {
        plan,
        owner_key,
        unrouted_key,
        capture_route,
        identity,
        captured,
        sealed: staged_sealed,
        sealed_route,
        fresh: fresh_out,
        phase_ns,
    }
}

pub(crate) fn codex_stage2(
    ledger: &mut Ledger,
    adapter: &CodexAdapter,
    mirror: &MirrorStore,
    staged: CodexStaged,
    seams: CodexSeams<'_>,
    report: &mut IngestReport,
) -> Result<()> {
    let fail_at_raw_line = seams.fail_at_raw_line;
    let CodexStaged {
        plan,
        owner_key,
        unrouted_key,
        capture_route,
        identity,
        captured,
        sealed: staged_sealed,
        sealed_route,
        fresh,
        phase_ns,
    } = staged;
    for (k, v) in phase_ns {
        *report.phase_ns.entry(k).or_insert(0) += v;
    }
    report.sessions += 1;
    let owner_key = owner_key?;
    if let Some(after_capture) = seams.after_capture {
        after_capture(&plan.path);
    }
    let t = std::time::Instant::now();
    let sealed = match captured {
        Ok(sealed) => sealed,
        Err(_) => {
            record_codex_rejection(
                ledger,
                &identity.owner_key,
                &identity.source_project_key,
                &identity.source_session_key,
                None,
                None,
                None,
                None,
                "capture_failed",
                "capture_or_seal_failed",
            )?;
            note_rejection(report, "capture_failed");
            return Ok(());
        }
    };
    let (payload_path, candidate_lines) = staged_sealed?;
    let sealed_route = match sealed_route {
        Some(route) => route,
        None => {
            let diagnostic_route = capture_route.as_ref();
            let diagnostic_owner =
                diagnostic_route.map_or(owner_key.as_str(), |route| route.owner_key.as_str());
            let diagnostic_project = diagnostic_route.map_or(unrouted_key.as_str(), |route| {
                route.source_project_key.as_str()
            });
            let diagnostic_session = diagnostic_route.map_or(unrouted_key.as_str(), |route| {
                route.source_session_key.as_str()
            });
            let predecessor = if let Some(route) = diagnostic_route {
                let session_id = stable_source_session_id(
                    &route.owner_key,
                    Source::Codex,
                    &route.source_session_key,
                );
                ledger
                    .accepted_generation(&session_id)?
                    .map(|generation| generation.id)
            } else {
                None
            };
            record_codex_rejection(
                ledger,
                diagnostic_owner,
                diagnostic_project,
                diagnostic_session,
                predecessor.as_deref(),
                Some(&sealed),
                Some(candidate_lines),
                None,
                "continuity_unknown",
                "sealed_route_invalid_ambiguous_or_changed",
            )?;
            mirror.retain_rejected(&sealed)?;
            report.lines_read += candidate_lines;
            report.quarantined_envelopes += candidate_lines;
            note_rejection(report, "continuity_unknown");
            return Ok(());
        }
    };

    let project_id = stable_source_project_id(
        &sealed_route.owner_key,
        Source::Codex,
        &sealed_route.source_project_key,
    );
    let session_id = stable_source_session_id(
        &sealed_route.owner_key,
        Source::Codex,
        &sealed_route.source_session_key,
    );
    let accepted = ledger.accepted_generation(&session_id)?;
    let accepted_message_slots = ledger.available_codex_message_slots(&session_id)?;
    // C27: a stage-1 mapping made as for a session the ledger has never
    // accepted is used only when that is still exactly the case;
    // otherwise it is thrown away and the mapping is made here, against
    // what the ledger holds now.
    let mut fresh = fresh;
    // A mapping that failed on a worker is made again here: the failure
    // may belong to the race, not to the bytes.
    if matches!(fresh, Some((_, Err(_)))) {
        fresh = None;
    }
    if accepted.is_some() || !accepted_message_slots.is_empty() {
        if let Some((_, Ok(prepared))) = fresh.take() {
            prepared.spool.discard()?;
        }
    }
    let lineage = match &fresh {
        Some((lineage, _)) => *lineage,
        None => {
            let accepted_sealed = accepted.as_ref().map(sealed_from_generation);
            mirror.classify(&sealed, accepted_sealed.as_ref())?
        }
    };
    report.time("seal_accept", t, ());
    match lineage {
        LineageDisposition::Identical => {
            report.identical_observations += 1;
            report.projects.push(project_id);
            return Ok(());
        }
        LineageDisposition::Truncated | LineageDisposition::Divergent => {
            let status = "source_diverged";
            let reason = if lineage == LineageDisposition::Truncated {
                "candidate_is_shorter_than_accepted_generation"
            } else {
                "candidate_does_not_preserve_accepted_prefix"
            };
            record_codex_rejection(
                ledger,
                &sealed_route.owner_key,
                &sealed_route.source_project_key,
                &sealed_route.source_session_key,
                accepted.as_ref().map(|g| g.id.as_str()),
                Some(&sealed),
                Some(candidate_lines),
                None,
                status,
                reason,
            )?;
            mirror.retain_rejected(&sealed)?;
            report.lines_read += candidate_lines;
            report.quarantined_envelopes += candidate_lines;
            note_rejection(report, status);
            return Ok(());
        }
        LineageDisposition::Initial | LineageDisposition::PrefixSuccessor => {}
    }

    // C27: a live source that has already moved since its capture cannot pass
    // the final pre-commit check below (it compares exactly these fields, and
    // a moved size, time, device or inode does not move back), so the mapping
    // would be thrown away. Refuse now, with the refusal that check records,
    // and skip the work. Only a definite difference counts: a seal that cannot
    // be read, or a platform that records no time, takes the full path.
    let live_seal = if seams.check_before_mapping || seams.watch_during_mapping {
        mirror.read_seal(&sealed)
    } else {
        None
    };
    let moved = Cell::new(false);
    let source_moved = || {
        let now = live_seal
            .as_ref()
            .is_some_and(|seal| MirrorStore::source_moved_from(&plan.path, seal));
        if now {
            moved.set(true);
        }
        now
    };
    let live_refusal = |ledger: &Ledger, report: &mut IngestReport| -> Result<()> {
        let status = "capture_failed";
        record_codex_rejection(
            ledger,
            &sealed_route.owner_key,
            &sealed_route.source_project_key,
            &sealed_route.source_session_key,
            accepted.as_ref().map(|g| g.id.as_str()),
            Some(&sealed),
            Some(candidate_lines),
            Some(status),
            status,
            "live_source_changed_before_commit",
        )?;
        mirror.retain_rejected(&sealed)?;
        report.lines_read += candidate_lines;
        report.quarantined_envelopes += candidate_lines;
        note_rejection(report, status);
        report.live_source_prechecks += 1;
        Ok(())
    };
    if seams.check_before_mapping && source_moved() {
        if let Some((_, Ok(prepared))) = fresh {
            prepared.spool.discard()?;
        }
        return live_refusal(ledger, report);
    }

    let accepted_prefix_bytes = accepted.as_ref().map_or(0, |g| g.payload_bytes);
    let accepted_prefix_lines = accepted.as_ref().map_or(0, |g| g.total_envelopes);
    let accepted_prefix = AcceptedCodexPrefix {
        payload_bytes: accepted_prefix_bytes,
        raw_lines: accepted_prefix_lines,
        reject_malformed_suffix: accepted.is_some(),
        available_message_slots: &accepted_message_slots,
    };
    let t = std::time::Instant::now();
    let prepared_result = match (fresh, fail_at_raw_line) {
        (Some((_, prepared)), None) => {
            report.worker_mappings_used += 1;
            prepared
        }
        (_, Some(raw_line)) => adapter.prepare_generation_with_line_failpoint(
            mirror,
            &sealed,
            &sealed_route,
            accepted_prefix,
            raw_line,
        ),
        (None, None) => adapter.prepare_generation_watching(
            mirror,
            &sealed,
            &sealed_route,
            accepted_prefix,
            &|| seams.watch_during_mapping && source_moved(),
        ),
    };
    report.time("codex_prepare", t, ());
    let t = std::time::Instant::now();
    let prepared = match prepared_result {
        Ok(prepared) => prepared,
        // The mapping gave up because the source moved: the refusal the
        // pre-commit check would have recorded after it.
        Err(_) if moved.get() => return live_refusal(ledger, report),
        Err(_) => {
            record_codex_rejection(
                ledger,
                &sealed_route.owner_key,
                &sealed_route.source_project_key,
                &sealed_route.source_session_key,
                accepted.as_ref().map(|g| g.id.as_str()),
                Some(&sealed),
                Some(candidate_lines),
                None,
                "successor_rejected",
                "candidate_mapping_or_disposition_failed",
            )?;
            mirror.retain_rejected(&sealed)?;
            report.lines_read += candidate_lines;
            report.quarantined_envelopes += candidate_lines;
            note_rejection(report, "successor_rejected");
            return Ok(());
        }
    };

    let PreparedCodexGeneration {
        full_line_count,
        disposition_count,
        event_count,
        family_counts,
        malformed,
        unknown,
        suppressed_mirrors,
        suppressed_replays,
        fallback_messages,
        spool_flushes,
        max_spool_buffered_rows,
        max_spool_buffered_bytes,
        spool,
    } = prepared;
    let accepted_at = iso8601_utc_now();
    let project = ProjectRef {
        id: project_id.clone(),
        root_path: sealed_route.normalized_root.clone(),
        source: Source::Codex,
        display_name: sealed_route.display_name.clone(),
    };
    let session = SessionRef {
        id: session_id.clone(),
        project_id: project_id.clone(),
        file_path: payload_path.display().to_string(),
        started_at: sealed_route.started_at.clone(),
        raw_line_count: full_line_count,
        raw_digest: Some(sealed.payload_sha256.clone()),
    };
    let generation = AcceptedGeneration {
        id: format!("generation_{}", sealed.generation_key),
        project_id: project_id.clone(),
        session_id: session_id.clone(),
        predecessor_id: accepted.as_ref().map(|g| g.id.clone()),
        mirror_payload_path: sealed.relative_payload.clone(),
        mirror_seal_path: sealed.relative_seal.clone(),
        payload_sha256: sealed.payload_sha256.clone(),
        payload_bytes: sealed.payload_bytes,
        seal_sha256: sealed.seal_sha256.clone(),
        inherited_prefix_bytes: accepted_prefix_bytes,
        total_envelopes: full_line_count,
        local_envelopes: disposition_count,
        local_events: event_count,
        accepted_at: accepted_at.clone(),
    };
    let acceptance = GenerationAcceptance {
        project,
        project_identity: SourceProjectIdentity {
            project_id: project_id.clone(),
            owner_key: sealed_route.owner_key.clone(),
            source: Source::Codex.as_str().to_string(),
            source_project_key: sealed_route.source_project_key.clone(),
            normalized_root_hint: Some(sealed_route.normalized_root.clone()),
        },
        session,
        session_identity: SourceSessionIdentity {
            session_id: session_id.clone(),
            project_id: project_id.clone(),
            owner_key: sealed_route.owner_key.clone(),
            source: Source::Codex.as_str().to_string(),
            source_session_key: sealed_route.source_session_key.clone(),
            source_locator: Some(plan.path.display().to_string()),
        },
        generation,
        expected_predecessor: accepted.as_ref().map(|g| g.id.clone()),
        dispositions: Vec::new(),
        events: Vec::new(),
    };
    let live_source_changed = Cell::new(false);
    let committed = match ledger.accept_source_generation_from_rows_with_final_check(
        &acceptance,
        &spool,
        || match mirror.verify_live_candidate(&plan.path, &sealed) {
            Ok(()) => Ok(()),
            Err(error) => {
                live_source_changed.set(true);
                Err(error)
            }
        },
    ) {
        Ok(committed) => committed,
        Err(error) => {
            spool.discard()?;
            let (status, reason) = if live_source_changed.get() {
                ("capture_failed", "live_source_changed_before_commit")
            } else {
                ("transaction_failed", "atomic_generation_commit_failed")
            };
            record_codex_rejection(
                ledger,
                &sealed_route.owner_key,
                &sealed_route.source_project_key,
                &sealed_route.source_session_key,
                accepted.as_ref().map(|g| g.id.as_str()),
                Some(&sealed),
                Some(candidate_lines),
                Some(status),
                status,
                reason,
            )?;
            mirror.retain_rejected(&sealed)?;
            report.lines_read += candidate_lines;
            report.quarantined_envelopes += candidate_lines;
            note_rejection(report, status);
            if live_source_changed.get() {
                return Ok(());
            }
            return Err(error);
        }
    };
    spool.discard()?;

    report.projects.push(project_id.clone());
    report.accepted_generations += 1;
    report.lines_read += full_line_count;
    report.lines_skipped_already_ingested += accepted_prefix_lines;
    report.dispositions += disposition_count;
    report.suppressed_mirrors += suppressed_mirrors;
    report.suppressed_replays += suppressed_replays;
    report.fallback_messages += fallback_messages;
    report.adapter_malformed += malformed;
    report.adapter_unknown += unknown;
    report.spool_flushes += spool_flushes;
    report.max_spool_buffered_rows = report
        .max_spool_buffered_rows
        .max(max_spool_buffered_rows as u64);
    report.max_spool_buffered_bytes = report
        .max_spool_buffered_bytes
        .max(max_spool_buffered_bytes as u64);
    report.new_events += committed.event_count;
    for (family, count) in family_counts {
        *report.adapter_families.entry(family).or_insert(0) += count;
    }
    for (kind, count) in committed.events_by_kind {
        *report.events_by_kind.entry(kind).or_insert(0) += count;
    }
    let (head, _) = ledger.compute_project_head(&project_id)?;
    report.heads.insert(project_id, head);
    report.time("ledger", t, ());
    Ok(())
}

fn sealed_from_generation(generation: &AcceptedGeneration) -> SealedMirror {
    SealedMirror {
        generation_key: generation
            .id
            .strip_prefix("generation_")
            .unwrap_or(&generation.id)
            .to_string(),
        relative_payload: generation.mirror_payload_path.clone(),
        relative_seal: generation.mirror_seal_path.clone(),
        payload_sha256: generation.payload_sha256.clone(),
        payload_bytes: generation.payload_bytes,
        seal_sha256: generation.seal_sha256.clone(),
    }
}

#[allow(clippy::too_many_arguments)]
fn record_codex_rejection(
    ledger: &Ledger,
    owner_key: &str,
    project_key: &str,
    session_key: &str,
    predecessor: Option<&str>,
    sealed: Option<&SealedMirror>,
    raw_envelopes: Option<u64>,
    id_suffix: Option<&str>,
    status: &str,
    reason: &str,
) -> Result<()> {
    let material = format!(
        "{owner_key}:{project_key}:{session_key}:{}:{status}:{reason}:{}",
        predecessor.unwrap_or(""),
        sealed.map_or("", |candidate| candidate.payload_sha256.as_str())
    );
    let id = format!(
        "candidate_{}_{}",
        sha256_hex(material.as_bytes()),
        id_suffix.unwrap_or("assessment")
    );
    ledger.record_candidate_assessment(&CandidateAssessment {
        id,
        owner_key: owner_key.to_string(),
        source: Source::Codex.as_str().to_string(),
        source_project_key: project_key.to_string(),
        source_session_key: session_key.to_string(),
        predecessor_generation_id: predecessor.map(str::to_owned),
        candidate_sha256: sealed.map(|candidate| candidate.payload_sha256.clone()),
        candidate_bytes: sealed.map(|candidate| candidate.payload_bytes),
        raw_envelopes,
        status: status.to_string(),
        reason: reason.to_string(),
        observed_at: iso8601_utc_now(),
    })
}

fn note_rejection(report: &mut IngestReport, status: &str) {
    report.rejected_candidates += 1;
    *report
        .candidate_statuses
        .entry(status.to_string())
        .or_insert(0) += 1;
}

fn ingest_session(
    ledger: &mut Ledger,
    project_id: &str,
    file: &Path,
    parsed: Option<Result<Parsed>>,
    report: &mut IngestReport,
) -> Result<()> {
    let t = std::time::Instant::now();
    let (lines, stats) = match parsed {
        Some(parsed) => parsed,
        None => parse_session(file),
    }?;
    report.time("parse", t, ());
    let t = std::time::Instant::now();

    report.lines_read += stats.lines_read;
    for m in &stats.malformed {
        report.malformed.push(m.clone());
    }
    for (t, n) in &stats.unknown_types {
        *report.unknown_types.entry(t.clone()).or_insert(0) += n;
    }

    let session_id = session_id_of(file);
    let session = SessionRef {
        id: session_id.clone(),
        project_id: project_id.to_string(),
        file_path: file.display().to_string(),
        started_at: lines.iter().find_map(|l| {
            l.events
                .first()
                .and_then(|e| e.ts.clone())
                .filter(|s| !s.is_empty())
        }),
        raw_line_count: stats.raw_lines_total,
        raw_digest: Some(stats.raw_digest.clone()),
    };

    // C27: the session's line count and digest are the marker the load's
    // unchanged-file check reads, so they are written only after the events
    // and their provenance are in. Until then the row keeps what it had (the
    // seal's acceptance leaves the payload digest there, which the check
    // reads as "not yet parsed"); a new row starts as acceptance starts one.
    let prior: Option<(i64, Option<String>)> = ledger
        .conn()
        .query_row(
            "SELECT raw_line_count, raw_digest FROM sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (prior_lines, prior_digest) = prior.unwrap_or((0, None));
    ledger.upsert_session(&SessionRef {
        raw_line_count: prior_lines.max(0) as u64,
        raw_digest: prior_digest,
        ..session.clone()
    })?;

    let already = ledger.ingested_line_hashes(&session_id)?;
    let file_str = file.display().to_string();

    let mut pending: Vec<(RawLineRef, crate::model::NormalizedEvent)> = Vec::new();
    for line in &lines {
        match already.get(&line.line) {
            Some(stored) if *stored == line.sha256 => {
                report.lines_skipped_already_ingested += 1;
            }
            Some(stored) => {
                // A line we already ingested no longer hashes the same. The
                // file was rewritten. Never re-hash it into the ledger.
                report.integrity_alarms.push(IntegrityAlarm {
                    file: file_str.clone(),
                    line: line.line,
                    stored_sha256: stored.clone(),
                    found_sha256: line.sha256.clone(),
                });
            }
            None => {
                let raw = RawLineRef {
                    file: file_str.clone(),
                    line: line.line,
                    sha256: line.sha256.clone(),
                };
                for ev in &line.events {
                    pending.push((raw.clone(), ev.clone()));
                }
            }
        }
    }

    if !pending.is_empty() {
        let placed = ledger.append_session_events(project_id, &session_id, &pending)?;
        report.new_events += placed.len() as u64;
        for ev in &placed {
            *report
                .events_by_kind
                .entry(ev.event.kind.as_str().to_string())
                .or_insert(0) += 1;
            // A207: a tool result that is a persisted-output reference holds
            // only an excerpt — count it so the report can disclose that any
            // search over it is incomplete (§13).
            if let Some(envelope) = &ev.event.tool_result {
                if crate::overflow::in_tool_result_envelope(envelope).is_some() {
                    report.overflowed_tool_results += 1;
                }
            }
        }
    }
    ledger.backfill_claude_event_provenance(&session_id)?;
    ledger.upsert_session(&session)?;
    report.time("ledger", t, ());
    Ok(())
}

/// RB1.5 — seal one claude_code source file into the mirror store, before any
/// parse touches it. The codex pattern, claude-shaped: capture under the
/// ingest lease, classify against the accepted generation, accept prefix
/// growth atomically. Adapter v1 records NO per-line dispositions — the
/// generation carries `local_envelopes: 0` with the whole payload as inherited
/// prefix, which the disposition verifier accepts by construction; per-line
/// dispositions for claude are a later adapter revision, stated here rather
/// than implied. A capture failure never blocks parsing — preservation is
/// loud-best-effort, ingestion is the contract.
pub(crate) type FramedLines = std::result::Result<Vec<(u64, u64, String, String)>, usize>;
type Parsed = (Vec<crate::parse::ParsedLine>, ParseStats);

/// C27 — a Claude Code transcript's first stage: capture into the mirror and,
/// when `eager`, the framing of the sealed payload and the parse of the file.
/// It reads nothing from the ledger and writes nothing to it, so a load runs
/// it on worker threads; [`claude_stage2`] does the rest on the one thread
/// that writes. Not eager (`ingest`), the framing and the parse happen in
/// stage 2 exactly where they always have.
pub(crate) struct ClaudeStaged {
    project_id: String,
    file: PathBuf,
    captured: Result<SealedMirror>,
    framed: Option<Result<FramedLines>>,
    parsed: Option<Result<Parsed>>,
    phase_ns: BTreeMap<&'static str, u64>,
}

pub(crate) fn claude_stage1(
    mirror: &MirrorStore,
    permit: crate::mirror::CapturePermit<'_>,
    project_id: &str,
    file: &Path,
    eager: bool,
) -> ClaudeStaged {
    let mut phase_ns = BTreeMap::new();
    let session_id = session_id_of(file);
    let identity = MirrorIdentity {
        owner_key: "legacy-local".to_string(),
        source_project_key: project_id.to_string(),
        source_session_key: session_id,
    };
    let t = std::time::Instant::now();
    let captured = mirror.capture_permitted(permit, file, &identity);
    *phase_ns.entry("seal").or_insert(0) += t.elapsed().as_nanos() as u64;
    let (mut framed, mut parsed) = (None, None);
    if eager {
        let t = std::time::Instant::now();
        if let Ok(sealed) = &captured {
            framed = Some(frame_claude_payload(mirror, sealed));
        }
        *phase_ns.entry("seal_accept").or_insert(0) += t.elapsed().as_nanos() as u64;
        let t = std::time::Instant::now();
        parsed = Some(parse_session(file));
        *phase_ns.entry("parse").or_insert(0) += t.elapsed().as_nanos() as u64;
    }
    ClaudeStaged {
        project_id: project_id.to_string(),
        file: file.to_path_buf(),
        captured,
        framed,
        parsed,
        phase_ns,
    }
}

/// Stage 2: lineage, generation acceptance, then the parse's events and the
/// session row — in that order, as `ingest` always did.
pub(crate) fn claude_stage2(
    ledger: &mut Ledger,
    mirror: &MirrorStore,
    staged: ClaudeStaged,
    report: &mut IngestReport,
) -> Result<()> {
    let ClaudeStaged {
        project_id,
        file,
        captured,
        framed,
        parsed,
        phase_ns,
    } = staged;
    for (k, v) in phase_ns {
        *report.phase_ns.entry(k).or_insert(0) += v;
    }
    let t = std::time::Instant::now();
    let outcome = accept_claude_seal(ledger, mirror, captured, framed, &project_id, &file, report);
    report.time("seal_accept", t, outcome)?;
    ingest_session(ledger, &project_id, &file, parsed, report)
}

fn parse_session(file: &Path) -> Result<Parsed> {
    let mut stats = ParseStats::default();
    let lines = SessionLines::open(file)?.collect_with_stats(&mut stats)?;
    Ok((lines, stats))
}

/// Frame every line of a sealed payload: `(start, length, sha256, family)`,
/// or `Err(n)` when line `n` exceeds the mirror frame limit.
fn frame_claude_payload(mirror: &MirrorStore, sealed: &SealedMirror) -> Result<FramedLines> {
    let payload_path = mirror.resolve_payload(sealed)?;
    let payload = std::fs::read(&payload_path).map_err(|e| CoreError::io(&payload_path, e))?;
    let mut lines: Vec<(u64, u64, String, String)> = Vec::new(); // (start, len, sha, family)
    let mut cursor = 0usize;
    while cursor < payload.len() {
        let nl = payload[cursor..]
            .iter()
            .position(|b| *b == b'\n')
            .map(|i| cursor + i + 1)
            .unwrap_or(payload.len());
        let framed = &payload[cursor..nl];
        let mut content = framed;
        if content.last() == Some(&b'\n') {
            content = &content[..content.len() - 1];
            if content.last() == Some(&b'\r') {
                content = &content[..content.len() - 1];
            }
        }
        if framed.len() > crate::codex_ingest::MAX_CODEX_LINE_BYTES + 2 {
            return Ok(Err(lines.len() + 1));
        }
        let family = if content.iter().all(|b| b.is_ascii_whitespace()) {
            "blank".to_string()
        } else {
            match serde_json::from_slice::<serde_json::Value>(content) {
                Ok(value) => {
                    let (top, item) = crate::codex::codex_envelope_family(&value);
                    item.map_or(top.clone(), |item| format!("{top}/{item}"))
                }
                Err(_) => "malformed".to_string(),
            }
        };
        lines.push((
            cursor as u64,
            framed.len() as u64,
            sha256_hex(content),
            family,
        ));
        cursor = nl;
    }
    Ok(Ok(lines))
}

fn accept_claude_seal(
    ledger: &mut Ledger,
    mirror: &MirrorStore,
    captured: Result<SealedMirror>,
    framed: Option<Result<FramedLines>>,
    project_id: &str,
    file: &Path,
    report: &mut IngestReport,
) -> Result<()> {
    let session_id = session_id_of(file);
    let identity = MirrorIdentity {
        owner_key: "legacy-local".to_string(),
        source_project_key: project_id.to_string(),
        source_session_key: session_id.clone(),
    };
    let sealed = match captured {
        Ok(sealed) => sealed,
        Err(e) => {
            report
                .claude_seal_failures
                .push(format!("{}: capture failed: {e}", file.display()));
            return Ok(());
        }
    };
    let accepted = ledger.accepted_generation(&session_id)?;
    let accepted_sealed = accepted.as_ref().map(sealed_from_generation);
    match mirror.classify(&sealed, accepted_sealed.as_ref()) {
        Ok(LineageDisposition::Identical) => {
            report.claude_seal_identical += 1;
            return Ok(());
        }
        Ok(LineageDisposition::Initial | LineageDisposition::PrefixSuccessor) => {}
        Ok(LineageDisposition::Truncated | LineageDisposition::Divergent) => {
            // The vendor rewrote history under us. Preserve the candidate for
            // forensics; the per-line integrity alarm fires during parse.
            mirror.retain_rejected(&sealed)?;
            report.claude_seal_rejected += 1;
            return Ok(());
        }
        Err(e) => {
            report.claude_seal_failures.push(format!(
                "{}: lineage classification failed: {e}",
                file.display()
            ));
            return Ok(());
        }
    }
    // Real per-line disposition rows, from the sealed payload itself: the
    // verifier re-frames every line by byte range, re-hashes it, and
    // recomputes its envelope family — so the rows must be derived from the
    // sealed bytes, never from the live file. Claude adapter v1 assigns no
    // disposition SEMANTICS (that is codex's turn machinery); every row is
    // recorded as `unknown` with the reason stated, which the verifier
    // accepts and a later adapter revision can refine.
    let framed = match framed {
        Some(framed) => framed,
        None => frame_claude_payload(mirror, &sealed),
    }?;
    let lines = match framed {
        Ok(lines) => lines,
        Err(line) => {
            report.claude_seal_failures.push(format!(
                "{}: line {line} exceeds the mirror frame limit; bytes sealed but not accepted",
                file.display(),
            ));
            mirror.retain_rejected(&sealed)?;
            return Ok(());
        }
    };
    let line_count = lines.len() as u64;
    let prior_lines = accepted.as_ref().map_or(0, |g| g.total_envelopes);
    let prior_bytes = accepted.as_ref().map_or(0, |g| g.payload_bytes);
    if line_count < prior_lines {
        // classify() said prefix — a shorter line count here is a frame
        // disagreement worth refusing loudly rather than accepting.
        report.claude_seal_failures.push(format!(
            "{}: sealed line count regressed ({line_count} < {prior_lines})",
            file.display()
        ));
        mirror.retain_rejected(&sealed)?;
        return Ok(());
    }
    let dispositions: Vec<RawDisposition> = lines
        .iter()
        .enumerate()
        .skip(prior_lines as usize)
        .map(|(i, (start, len, sha, family))| RawDisposition {
            raw_line: i as u64 + 1,
            raw_sha256: sha.clone(),
            byte_start: *start,
            byte_length: *len,
            envelope_family: family.clone(),
            disposition: "unknown".to_string(),
            reason: Some("claude_adapter_v1_no_disposition_semantics".to_string()),
            source_turn_id: None,
            turn_boundary_raw_line: None,
            turn_boundary_raw_sha256: None,
            emitted_event_count: 0,
        })
        .collect();

    let accepted_at = iso8601_utc_now();
    let acceptance = GenerationAcceptance {
        project: ProjectRef {
            id: project_id.to_string(),
            root_path: demangle(project_id),
            source: Source::ClaudeCode,
            display_name: display_name(project_id),
        },
        project_identity: SourceProjectIdentity {
            project_id: project_id.to_string(),
            owner_key: identity.owner_key.clone(),
            source: Source::ClaudeCode.as_str().to_string(),
            source_project_key: project_id.to_string(),
            normalized_root_hint: Some(demangle(project_id)),
        },
        session: SessionRef {
            id: session_id.clone(),
            project_id: project_id.to_string(),
            // The vendor path stays the session's identity: claude receipts
            // anchor to raw JSONL, and the mirror is the survival copy.
            file_path: file.display().to_string(),
            started_at: None,
            raw_line_count: line_count,
            // The acceptance layer requires the payload sha here; the parse
            // that follows immediately re-upserts the session with the §7
            // line-dialect digest, which is what the digest verify reads.
            raw_digest: Some(sealed.payload_sha256.clone()),
        },
        session_identity: SourceSessionIdentity {
            session_id: session_id.clone(),
            project_id: project_id.to_string(),
            owner_key: identity.owner_key.clone(),
            source: Source::ClaudeCode.as_str().to_string(),
            source_session_key: session_id.clone(),
            source_locator: Some(file.display().to_string()),
        },
        generation: AcceptedGeneration {
            id: format!("generation_{}", sealed.generation_key),
            project_id: project_id.to_string(),
            session_id: session_id.clone(),
            predecessor_id: accepted.as_ref().map(|g| g.id.clone()),
            mirror_payload_path: sealed.relative_payload.clone(),
            mirror_seal_path: sealed.relative_seal.clone(),
            payload_sha256: sealed.payload_sha256.clone(),
            payload_bytes: sealed.payload_bytes,
            seal_sha256: sealed.seal_sha256.clone(),
            inherited_prefix_bytes: prior_bytes,
            total_envelopes: line_count,
            local_envelopes: line_count - prior_lines,
            local_events: 0,
            accepted_at,
        },
        expected_predecessor: accepted.as_ref().map(|g| g.id.clone()),
        dispositions,
        events: Vec::new(),
    };
    // Re-verify the live candidate immediately before accepting, then accept
    // atomically: a file the vendor rewrote mid-ingest never gains authority.
    match mirror
        .verify_live_candidate(file, &sealed)
        .and_then(|()| ledger.accept_source_generation(&acceptance))
    {
        Ok(_) => report.claude_sealed += 1,
        Err(e) => {
            mirror.retain_rejected(&sealed)?;
            report
                .claude_seal_failures
                .push(format!("{}: acceptance failed: {e}", file.display()));
        }
    }
    Ok(())
}

/// Re-read every raw file, recompute every hash and the whole chain.
pub fn verify(ledger: &Ledger, only_project: Option<&str>) -> Result<VerifyReport> {
    verify_inner(ledger, only_project, VerifyWriteback::Record, None)
}

/// Compute the same verdict as [`verify`] without changing ledger metadata.
///
/// Store replacement compares both files with this path so verification cannot
/// change any row that the replacement promises to preserve byte-for-byte.
/// The caller must still use a read-only ledger handle when it needs the
/// database connection itself to enforce an observational proof.
pub fn verify_observational(ledger: &Ledger, only_project: Option<&str>) -> Result<VerifyReport> {
    verify_inner(ledger, only_project, VerifyWriteback::Observe, None)
}

/// Observe using only this mirror base, without HOME, sibling-store or
/// embedded payload-path discovery. Removal proof must stay in its named store.
pub fn verify_observational_with_mirror_root(
    ledger: &Ledger,
    only_project: Option<&str>,
    mirror_base: &Path,
) -> Result<VerifyReport> {
    verify_inner(
        ledger,
        only_project,
        VerifyWriteback::Observe,
        Some(mirror_base),
    )
}

#[derive(Clone, Copy)]
enum VerifyWriteback {
    Record,
    Observe,
}

fn verify_inner(
    ledger: &Ledger,
    only_project: Option<&str>,
    writeback: VerifyWriteback,
    mirror_base: Option<&Path>,
) -> Result<VerifyReport> {
    let started = SystemTime::now();
    let mut report = VerifyReport::default();
    let now = iso8601_utc_now();

    for project_id in ledger.project_ids()? {
        if let Some(want) = only_project {
            if project_id != want {
                continue;
            }
        }
        let mut verdict = ProjectVerdict::default();

        for (session_id, _file_path) in ledger.sessions_of(&project_id)? {
            let stored = ledger.session_chain(&session_id)?;
            if stored.is_empty() {
                continue;
            }

            // Receipts bind each event to the immutable raw file that actually
            // produced it. A successor session path may point at a newer full
            // mirror; it never replaces a predecessor receipt.
            let mut raw_files: BTreeMap<String, Option<BTreeMap<u64, String>>> = BTreeMap::new();
            let explicit_receipts =
                explicit_codex_receipt_payloads(ledger, &session_id, mirror_base)?;

            // RB1.5 — the sealed generation is the survival copy: when the
            // vendor file is gone (or a line is), the accepted payload answers
            // for it, counted apart so a mirror pass never impersonates a live
            // source. Loaded lazily; a session never sealed simply has None.
            let mut mirror_lines: Option<Option<BTreeMap<u64, String>>> = None;
            let load_mirror = |ledger: &Ledger| -> Option<BTreeMap<u64, String>> {
                let generation = ledger.accepted_generation(&session_id).ok().flatten()?;
                let sealed = sealed_from_generation(&generation);
                candidate_mirror_stores(ledger, mirror_base)
                    .into_iter()
                    .find_map(|root| {
                        let store = MirrorStore::open_for_verification(root).ok()?;
                        store.verify(&sealed).ok()?;
                        let payload = store.resolve_payload(&sealed).ok()?;
                        hash_lines(&payload).ok()
                    })
            };

            let mut prev = genesis_prev_hash(&project_id);
            for link in &stored {
                verdict.events_checked += 1;
                if !raw_files.contains_key(&link.raw_file) {
                    let hashed = match &explicit_receipts {
                        Some(payloads) => payloads
                            .get(&link.raw_file)
                            .and_then(Option::as_ref)
                            .and_then(|payload| hash_lines(payload).ok()),
                        None => hash_lines(Path::new(&link.raw_file)).ok(),
                    };
                    if hashed.is_none() && !verdict.missing_files.contains(&link.raw_file) {
                        verdict.missing_files.push(link.raw_file.clone());
                    }
                    raw_files.insert(link.raw_file.clone(), hashed);
                }
                let fresh = raw_files.get(&link.raw_file).and_then(Option::as_ref);
                let disk = fresh.and_then(|lines| lines.get(&link.raw_line));
                match disk {
                    Some(found) if *found == link.raw_sha256 => verdict.raw_matches += 1,
                    // A line that DIFFERS on disk is a rewrite alarm, full
                    // stop — the mirror holding the original does not soften
                    // it, it proves it. Only ABSENT bytes (a swept file, a
                    // truncated tail) may be answered by the seal.
                    Some(found) => verdict.raw_mismatches.push(IntegrityAlarm {
                        file: link.raw_file.clone(),
                        line: link.raw_line,
                        stored_sha256: link.raw_sha256.clone(),
                        found_sha256: found.clone(),
                    }),
                    None => {
                        // A named Codex mirror must contain the generation
                        // that produced this receipt. Its successor cannot
                        // answer for a missing predecessor in an isolated proof.
                        let mirrored = if explicit_receipts.is_some() {
                            None
                        } else {
                            mirror_lines
                                .get_or_insert_with(|| load_mirror(ledger))
                                .as_ref()
                                .and_then(|lines| lines.get(&link.raw_line))
                        };
                        match mirrored {
                            Some(found) if *found == link.raw_sha256 => {
                                verdict.raw_matches_via_mirror += 1;
                            }
                            _ => verdict.raw_mismatches.push(IntegrityAlarm {
                                file: link.raw_file.clone(),
                                line: link.raw_line,
                                stored_sha256: link.raw_sha256.clone(),
                                found_sha256: "<line no longer present>".to_string(),
                            }),
                        }
                    }
                }

                let recomputed = chain_hash(&prev, &link.raw_sha256, &session_id, link.seq);
                if recomputed == link.chain_hash && prev == link.prev_hash {
                    verdict.chain_matches += 1;
                } else {
                    verdict.chain_mismatches.push(format!(
                        "{session_id} seq {}: stored {} / recomputed {}",
                        link.seq, link.chain_hash, recomputed
                    ));
                }
                prev = link.chain_hash.clone();
            }
        }

        // Whole-file digests: the only check that covers malformed lines.
        for sd in ledger.session_digests(&project_id)? {
            let (session_id, file_path, stored_count) =
                (sd.session_id, sd.file_path, sd.raw_line_count);
            let Some(stored_digest) = sd.raw_digest else {
                verdict
                    .digest_mismatches
                    .push(format!("{session_id}: no stored digest"));
                continue;
            };
            verdict.digests_checked += 1;
            let accepted = ledger.accepted_generation(&session_id)?;
            let mut explicit_head_payload = None;
            if let Some(head_generation) = &accepted {
                // Codex sessions store the payload path AS the session path, so
                // the root falls out by stripping; claude sessions keep the
                // vendor path (RB1.5) and resolve through the default
                // namespaced store instead. Both are read-only opens.
                let store = if mirror_base.is_some() {
                    candidate_mirror_stores(ledger, mirror_base)
                        .into_iter()
                        .find_map(|root| MirrorStore::open_for_verification(root).ok())
                        .ok_or_else(|| {
                            CoreError::Integrity(
                                "explicit mirror root is not verifiable".to_owned(),
                            )
                        })
                } else {
                    mirror_root_from_payload(
                        Path::new(&file_path),
                        &head_generation.mirror_payload_path,
                    )
                    .and_then(|root| MirrorStore::open_for_verification(&root))
                    .or_else(|first_err| {
                        candidate_mirror_stores(ledger, None)
                            .into_iter()
                            .find_map(|root| MirrorStore::open_for_verification(root).ok())
                            .ok_or(first_err)
                    })
                };
                match store {
                    Err(_) => verdict.digest_mismatches.push(format!(
                        "{session_id}: accepted mirror root is not verifiable"
                    )),
                    Ok(store) => {
                        if mirror_base.is_some()
                            && mirror_root_from_payload(
                                Path::new(&file_path),
                                &head_generation.mirror_payload_path,
                            )
                            .is_ok()
                        {
                            explicit_head_payload = store
                                .resolve_payload(&sealed_from_generation(head_generation))
                                .ok();
                        }
                        let lineage = ledger.accepted_generation_lineage(&session_id)?;
                        let mut prior: Option<SealedMirror> = None;
                        for generation in lineage {
                            let sealed = sealed_from_generation(&generation);
                            if store.verify(&sealed).is_err() {
                                verdict.digest_mismatches.push(format!(
                                    "{session_id}: generation {} failed seal verification",
                                    generation.id
                                ));
                            }
                            let disposition_payload_ok = store
                                .resolve_payload(&sealed)
                                .and_then(|payload| {
                                    verify_generation_disposition_bytes(
                                        ledger,
                                        &generation,
                                        &payload,
                                    )
                                })
                                .unwrap_or(false);
                            if !disposition_payload_ok {
                                verdict.digest_mismatches.push(format!(
                                    "{session_id}: generation {} disposition bytes failed verification",
                                    generation.id
                                ));
                            }
                            let expected_lineage = match &prior {
                                None => LineageDisposition::Initial,
                                Some(_) => LineageDisposition::PrefixSuccessor,
                            };
                            if store.classify(&sealed, prior.as_ref()).ok()
                                != Some(expected_lineage)
                            {
                                verdict.digest_mismatches.push(format!(
                                    "{session_id}: generation {} failed prefix-lineage verification",
                                    generation.id
                                ));
                            }
                            let disposition_count: i64 = ledger.conn().query_row(
                                "SELECT COUNT(*) FROM generation_raw_dispositions
                                 WHERE generation_id=?1",
                                [&generation.id],
                                |row| row.get(0),
                            )?;
                            let event_count: i64 = ledger.conn().query_row(
                                "SELECT COUNT(*) FROM event_generation_provenance
                                 WHERE generation_id=?1",
                                [&generation.id],
                                |row| row.get(0),
                            )?;
                            let turn_scope_breaks: i64 = ledger.conn().query_row(
                                "SELECT COUNT(*)
                                 FROM generation_raw_dispositions d
                                 JOIN session_generations dg ON dg.id=d.generation_id
                                 WHERE d.generation_id=?1
                                   AND d.turn_boundary_raw_line IS NOT NULL
                                   AND NOT EXISTS(
                                     SELECT 1
                                     FROM generation_raw_dispositions b
                                     JOIN session_generations bg ON bg.id=b.generation_id
                                     WHERE bg.session_id=dg.session_id
                                       AND b.raw_line=d.turn_boundary_raw_line
                                       AND b.raw_sha256=d.turn_boundary_raw_sha256)",
                                [&generation.id],
                                |row| row.get(0),
                            )?;
                            let message_slot_count: i64 = ledger.conn().query_row(
                                "SELECT COUNT(*) FROM codex_message_slots
                                 WHERE generation_id=?1",
                                [&generation.id],
                                |row| row.get(0),
                            )?;
                            let message_event_count: i64 = ledger.conn().query_row(
                                "SELECT COUNT(*) FROM event_generation_provenance p
                                 JOIN events e ON e.id=p.event_id
                                 WHERE p.generation_id=?1
                                   AND e.source_message_key IS NOT NULL",
                                [&generation.id],
                                |row| row.get(0),
                            )?;
                            let reconciliation_count: i64 = ledger.conn().query_row(
                                "SELECT COUNT(*) FROM codex_reconciliation_receipts
                                 WHERE generation_id=?1",
                                [&generation.id],
                                |row| row.get(0),
                            )?;
                            let suppressed_count: i64 = ledger.conn().query_row(
                                "SELECT COUNT(*) FROM generation_raw_dispositions
                                 WHERE generation_id=?1
                                   AND disposition='suppressed_mirror'",
                                [&generation.id],
                                |row| row.get(0),
                            )?;
                            // RB1.5 — vendor-specific accounting. The codex
                            // adapter knows its event count at acceptance
                            // (it parses from sealed bytes); the claude v1
                            // adapter seals BEFORE parse and records
                            // `local_events: 0`, binding events to the
                            // generation afterwards through provenance whose
                            // FK already pins each event to a framed line.
                            // Claude generations therefore hold the
                            // disposition arithmetic and the coverage check
                            // below, not the codex event equalities.
                            let is_claude = ledger
                                .conn()
                                .query_row(
                                    "SELECT source FROM source_sessions WHERE session_id=?1",
                                    [&session_id],
                                    |row| row.get::<_, String>(0),
                                )
                                .map(|s| s == "claude_code")
                                .unwrap_or(false);
                            let codex_accounting_broken = !is_claude
                                && (event_count.max(0) as u64 != generation.local_events
                                    || message_slot_count != message_event_count
                                    || reconciliation_count != suppressed_count);
                            if disposition_count.max(0) as u64 != generation.local_envelopes
                                || turn_scope_breaks != 0
                                || codex_accounting_broken
                            {
                                verdict.digest_mismatches.push(format!(
                                    "{session_id}: generation {} disposition/event accounting mismatch",
                                    generation.id
                                ));
                            }
                            prior = Some(sealed);
                        }
                        let provenance_count: i64 = ledger.conn().query_row(
                            "SELECT COUNT(*) FROM event_generation_provenance p
                             JOIN events e ON e.id=p.event_id WHERE e.session_id=?1",
                            [&session_id],
                            |row| row.get(0),
                        )?;
                        let accepted_event_count: i64 = ledger.conn().query_row(
                            "SELECT COUNT(*) FROM events WHERE session_id=?1",
                            [&session_id],
                            |row| row.get(0),
                        )?;
                        if provenance_count != accepted_event_count {
                            verdict.digest_mismatches.push(format!(
                                "{session_id}: accepted event/provenance coverage mismatch"
                            ));
                        }
                    }
                }
            }
            // RB1.5 — sessions.raw_digest speaks two dialects by vendor:
            // codex acceptance stores the payload byte sha; claude parse
            // stores the §7 length-prefixed line digest, generation or not.
            // The byte identity of a sealed claude source is still enforced —
            // by the generation compare just below — so nothing is lost by
            // comparing each dialect against its own writer.
            let claude_session = ledger
                .conn()
                .query_row(
                    "SELECT source FROM source_sessions WHERE session_id=?1",
                    [&session_id],
                    |row| row.get::<_, String>(0),
                )
                .map(|s| s == "claude_code")
                .unwrap_or(false);
            let payload_path = if mirror_base.is_some() && accepted.is_some() && !claude_session {
                explicit_head_payload.as_deref()
            } else {
                Some(Path::new(&file_path))
            };
            let found = payload_path.and_then(|payload| {
                if accepted.is_some() && !claude_session {
                    exact_file_identity(payload)
                        .map(|(lines, _, sha)| (lines, sha))
                        .ok()
                } else {
                    whole_file_digest(payload).ok()
                }
            });
            match found {
                None => {
                    if !verdict.missing_files.contains(&file_path) {
                        verdict.missing_files.push(file_path.clone());
                    }
                }
                Some((count, digest)) => {
                    if count != stored_count {
                        verdict.digest_mismatches.push(format!(
                            "{session_id}: raw_line_count stored {stored_count} / found {count}"
                        ));
                    } else if digest != stored_digest {
                        verdict.digest_mismatches.push(format!(
                            "{session_id}: raw_digest stored {stored_digest} / found {digest}"
                        ));
                    }
                    if let Some(generation) = &accepted {
                        match payload_path.and_then(|payload| exact_file_identity(payload).ok()) {
                            Some((_, bytes, sha))
                                if bytes == generation.payload_bytes
                                    && sha == generation.payload_sha256 => {}
                            Some((_, bytes, sha)) => verdict.digest_mismatches.push(format!(
                                "{session_id}: accepted mirror stored {}/{} / found {bytes}/{sha}",
                                generation.payload_bytes, generation.payload_sha256
                            )),
                            None => {}
                        }
                    }
                }
            }
        }

        // The split the UI could not make on its own (A147/A150): the chain's
        // own arithmetic is one fact; dead receipts are another. `verified_at`
        // keeps its strict meaning — everything passed.
        let chain_intact = verdict.chain_mismatches.is_empty();
        let missing = verdict.missing_files.len() as u64;
        // A missing vendor file with zero surviving mismatches was fully
        // answered by the mirror: reclassify it as swept-but-preserved so the
        // verdict can say what actually happened (RB1.5).
        let alarmed: std::collections::BTreeSet<&str> = verdict
            .raw_mismatches
            .iter()
            .map(|a| a.file.as_str())
            .collect();
        let (preserved, still_missing): (Vec<String>, Vec<String>) = verdict
            .missing_files
            .drain(..)
            .partition(|f| !alarmed.contains(f.as_str()));
        verdict.missing_files = still_missing;
        verdict.swept_files_preserved.extend(preserved);

        verdict.head_hash = match writeback {
            VerifyWriteback::Record => {
                let verified_at = verdict.ok().then_some(now.as_str());
                ledger.record_verification(&project_id, verified_at, chain_intact, missing)?
            }
            VerifyWriteback::Observe => ledger.compute_project_head(&project_id)?.0,
        };
        report.projects.insert(project_id, verdict);
    }

    report.elapsed_ms = started.elapsed().map(|d| d.as_millis()).unwrap_or(0);
    Ok(report)
}

/// In explicit-root mode a sealed Codex receipt names a generation, not a
/// permission to open the old absolute payload path. Bind every distinct
/// receipt path to its recorded provenance within the accepted lineage.
fn explicit_codex_receipt_payloads(
    ledger: &Ledger,
    session_id: &str,
    mirror_base: Option<&Path>,
) -> Result<Option<BTreeMap<String, Option<PathBuf>>>> {
    let Some(base) = mirror_base else {
        return Ok(None);
    };
    let source: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT source FROM source_sessions WHERE session_id=?1",
            [session_id],
            |row| row.get(0),
        )
        .optional()?;
    if source.as_deref() == Some("claude_code") {
        return Ok(None);
    }
    let lineage = ledger.accepted_generation_lineage(session_id)?;
    if lineage.is_empty() {
        return Ok(None);
    }
    let store = candidate_mirror_stores(ledger, Some(base))
        .into_iter()
        .find_map(|root| MirrorStore::open_for_verification(root).ok());
    let generations: BTreeMap<_, _> = lineage
        .into_iter()
        .map(|generation| (generation.id.clone(), generation))
        .collect();
    let mut statement = ledger.conn().prepare(
        "SELECT DISTINCT e.raw_file, p.generation_id
         FROM events e LEFT JOIN event_generation_provenance p ON p.event_id=e.id
         WHERE e.session_id=?1",
    )?;
    let rows = statement.query_map([session_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
    })?;
    let mut payloads = BTreeMap::new();
    for row in rows {
        let (recorded, generation_id) = row?;
        let resolved = generation_id
            .as_ref()
            .and_then(|id| generations.get(id))
            .and_then(|generation| {
                // This is a lexical receipt check only; never open its root.
                mirror_root_from_payload(Path::new(&recorded), &generation.mirror_payload_path)
                    .ok()?;
                store
                    .as_ref()?
                    .resolve_payload(&sealed_from_generation(generation))
                    .ok()
            });
        payloads
            .entry(recorded)
            .and_modify(|existing| {
                if *existing != resolved {
                    *existing = None;
                }
            })
            .or_insert(resolved);
    }
    Ok(Some(payloads))
}

/// The namespaced mirror stores this ledger could have sealed into: the
/// default root (production CLI) and the db-sibling root (tests, custom
/// `--mirror-root` beside the ledger). Verify tries each; sealing wrote one.
fn candidate_mirror_stores(ledger: &Ledger, explicit_base: Option<&Path>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let authority: Option<String> = ledger
        .conn()
        .query_row("SELECT authority_key FROM ledger_authority", [], |r| {
            r.get(0)
        })
        .ok();
    let Some(authority) = authority else {
        return out;
    };
    if let Some(base) = explicit_base {
        // Match MirrorStore's namespace component rule without its write path.
        if !authority.is_empty()
            && authority
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            out.push(base.join("ledgers").join(authority));
        }
        return out;
    }
    if let Ok(root) = default_mirror_root() {
        out.push(root.join("ledgers").join(&authority));
    }
    if let Some(db_dir) = ledger.conn().path().and_then(|p| Path::new(p).parent()) {
        out.push(db_dir.join("mirrors").join("ledgers").join(&authority));
    }
    out
}

fn mirror_root_from_payload(payload: &Path, relative: &str) -> Result<PathBuf> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(CoreError::Integrity(
            "accepted mirror receipt is not a contained relative path".to_string(),
        ));
    }
    let mut root = payload.to_path_buf();
    for _ in relative.components() {
        if !root.pop() {
            return Err(CoreError::Integrity(
                "accepted mirror receipt has no containing root".to_string(),
            ));
        }
    }
    if root.join(relative) != payload {
        return Err(CoreError::Integrity(
            "accepted mirror receipt does not resolve to the session payload".to_string(),
        ));
    }
    Ok(root)
}

/// Exact byte identity plus editor-style line count. Unlike the legacy
/// line-framed digest, this is the generation identity used by sealed mirrors.
fn exact_file_identity(path: &Path) -> Result<(u64, u64, String)> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| CoreError::io(path, e))?;
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut lines = 0u64;
    let mut saw_any = false;
    let mut last = 0u8;
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buffer).map_err(|e| CoreError::io(path, e))?;
        if n == 0 {
            break;
        }
        saw_any = true;
        last = buffer[n - 1];
        lines += buffer[..n].iter().filter(|b| **b == b'\n').count() as u64;
        bytes += n as u64;
        hash.update(&buffer[..n]);
    }
    if saw_any && last != b'\n' {
        lines += 1;
    }
    Ok((lines, bytes, crate::chain::to_hex(&hash.finalize())))
}

fn verify_generation_disposition_bytes(
    ledger: &Ledger,
    generation: &AcceptedGeneration,
    payload: &Path,
) -> Result<bool> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = std::fs::File::open(payload).map_err(|e| CoreError::io(payload, e))?;
    let mut statement = ledger.conn().prepare(
        "SELECT raw_line,raw_sha256,byte_start,byte_length,envelope_family,disposition
         FROM generation_raw_dispositions
         WHERE generation_id=?1 ORDER BY raw_line",
    )?;
    let mut rows = statement.query([&generation.id])?;
    let mut next_line = generation
        .total_envelopes
        .checked_sub(generation.local_envelopes)
        .and_then(|line| line.checked_add(1))
        .ok_or_else(|| CoreError::Integrity("generation line accounting underflow".to_string()))?;
    let mut next_byte = generation.inherited_prefix_bytes;
    let mut row_count = 0u64;
    file.seek(SeekFrom::Start(next_byte))
        .map_err(|e| CoreError::io(payload, e))?;

    while let Some(row) = rows.next()? {
        let raw_line = row.get::<_, i64>(0)?;
        let raw_sha256 = row.get::<_, String>(1)?;
        let byte_start = row.get::<_, i64>(2)?;
        let byte_length = row.get::<_, i64>(3)?;
        let envelope_family = row.get::<_, String>(4)?;
        let disposition = row.get::<_, String>(5)?;
        if raw_line < 0
            || byte_start < 0
            || byte_length <= 0
            || raw_line as u64 != next_line
            || byte_start as u64 != next_byte
            || byte_length as usize > crate::codex_ingest::MAX_CODEX_LINE_BYTES + 2
            || !matches!(
                disposition.as_str(),
                "blank"
                    | "malformed"
                    | "suppressed_replay"
                    | "unknown"
                    | "suppressed_mirror"
                    | "fallback"
                    | "meta"
                    | "event"
            )
        {
            return Ok(false);
        }
        let mut framed = vec![0u8; byte_length as usize];
        file.read_exact(&mut framed)
            .map_err(|e| CoreError::io(payload, e))?;
        if framed.last() == Some(&b'\n') {
            framed.pop();
            if framed.last() == Some(&b'\r') {
                framed.pop();
            }
        }
        if framed.iter().any(|byte| *byte == b'\n' || *byte == b'\r')
            || sha256_hex(&framed) != raw_sha256
        {
            return Ok(false);
        }
        let expected_family = if framed.iter().all(|byte| byte.is_ascii_whitespace()) {
            "blank".to_string()
        } else {
            match serde_json::from_slice::<serde_json::Value>(&framed) {
                Ok(value) => {
                    let (top, item) = crate::codex::codex_envelope_family(&value);
                    item.map_or(top.clone(), |item| format!("{top}/{item}"))
                }
                Err(_) => "malformed".to_string(),
            }
        };
        if expected_family != envelope_family {
            return Ok(false);
        }
        next_line = next_line
            .checked_add(1)
            .ok_or_else(|| CoreError::Integrity("generation line count overflow".to_string()))?;
        next_byte = next_byte
            .checked_add(byte_length as u64)
            .ok_or_else(|| CoreError::Integrity("generation byte count overflow".to_string()))?;
        row_count = row_count
            .checked_add(1)
            .ok_or_else(|| CoreError::Integrity("generation row count overflow".to_string()))?;
    }
    Ok(row_count == generation.local_envelopes
        && next_byte == generation.payload_bytes
        && next_line == generation.total_envelopes.saturating_add(1))
}

/// `(raw_line_count, digest)` over EVERY raw line in order, mirroring
/// [`crate::parse::SessionLines`] exactly so the values are comparable.
///
/// Public so a writer with no vendor transcript behind its session — a check
/// run recorded for a Change (`change_link::record_check_session`) — stores
/// exactly the digest this module recomputes (ADV-run-ledger-5).
pub fn whole_file_digest(path: &Path) -> Result<(u64, String)> {
    use sha2::{Digest, Sha256};
    use std::io::BufRead;
    let file = std::fs::File::open(path).map_err(|e| CoreError::io(path, e))?;
    let mut reader = std::io::BufReader::new(file);
    let mut h = Sha256::new();
    let mut count = 0u64;
    loop {
        let mut buf = Vec::new();
        let n = reader
            .read_until(b'\n', &mut buf)
            .map_err(|e| CoreError::io(path, e))?;
        if n == 0 {
            break;
        }
        if buf.ends_with(b"\n") {
            buf.pop();
            if buf.ends_with(b"\r") {
                buf.pop();
            }
        }
        count += 1;
        h.update((buf.len() as u64).to_le_bytes());
        h.update(&buf);
    }
    Ok((count, crate::chain::to_hex(&h.finalize())))
}

/// C27 (b) — what one pass over a recording's bytes proves about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDigests {
    /// Bytes read.
    pub bytes: u64,
    /// SHA-256 of exactly those bytes: what a mirror capture of them would
    /// record as its `payload_sha256`.
    pub payload_sha256: String,
    /// [`whole_file_digest`]'s line count and digest of the same bytes: what a
    /// parse of them would record as the session row's `raw_line_count` and
    /// `raw_digest`.
    pub raw_lines: u64,
    pub raw_digest: String,
}

/// C27 (b) — [`FileDigests`] for `path`, in one pass: the payload digest a
/// capture would seal and the line digest a parse would record. No JSON is
/// parsed. The line splitting is [`whole_file_digest`]'s, which is the
/// parser's (`parse::SessionLines`); a test pins all three together.
pub fn file_digests(path: &Path) -> Result<FileDigests> {
    use sha2::{Digest, Sha256};
    use std::io::BufRead;
    let file = std::fs::File::open(path).map_err(|e| CoreError::io(path, e))?;
    let mut reader = std::io::BufReader::with_capacity(1 << 20, file);
    let (mut whole, mut lines) = (Sha256::new(), Sha256::new());
    let (mut bytes, mut count) = (0u64, 0u64);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let n = reader
            .read_until(b'\n', &mut buf)
            .map_err(|e| CoreError::io(path, e))?;
        if n == 0 {
            break;
        }
        whole.update(&buf);
        bytes += n as u64;
        let mut content = buf.as_slice();
        if let Some(rest) = content.strip_suffix(b"\n") {
            content = rest.strip_suffix(b"\r").unwrap_or(rest);
        }
        count += 1;
        lines.update((content.len() as u64).to_le_bytes());
        lines.update(content);
    }
    Ok(FileDigests {
        bytes,
        payload_sha256: crate::chain::to_hex(&whole.finalize()),
        raw_lines: count,
        raw_digest: crate::chain::to_hex(&lines.finalize()),
    })
}

/// `raw_line -> sha256`, streaming. Mirrors the parser's line splitting so the
/// hashes are directly comparable.
fn hash_lines(path: &Path) -> Result<BTreeMap<u64, String>> {
    use std::io::BufRead;
    let file = std::fs::File::open(path).map_err(|e| CoreError::io(path, e))?;
    let mut reader = std::io::BufReader::new(file);
    let mut out = BTreeMap::new();
    let mut line = 0u64;
    loop {
        let mut buf = Vec::new();
        let n = reader
            .read_until(b'\n', &mut buf)
            .map_err(|e| CoreError::io(path, e))?;
        if n == 0 {
            break;
        }
        if buf.ends_with(b"\n") {
            buf.pop();
            if buf.ends_with(b"\r") {
                buf.pop();
            }
        }
        line += 1;
        if buf.iter().all(|b| b.is_ascii_whitespace()) {
            continue;
        }
        out.insert(line, sha256_hex(&buf));
    }
    Ok(out)
}

fn display_name(dir_name: &str) -> Option<String> {
    dir_name
        .rsplit('-')
        .find(|s| !s.is_empty())
        .map(str::to_owned)
}

/// `-Users-dev-thruline` → `/Users/dev/thruline`. Lossy where a real path
/// component contained `-`, which is why `id` stays the munged form.
pub(crate) fn demangle(dir_name: &str) -> String {
    if dir_name.starts_with('-') {
        dir_name.replacen('-', "/", 1).replace('-', "/")
    } else {
        dir_name.to_string()
    }
}

/// Minimal RFC-3339 UTC stamp — avoids a date dependency for a field that
/// never enters the chain.
pub fn iso8601_utc_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    iso8601_utc(secs)
}

/// The same stamp for a given instant. Split out so a stored millisecond
/// timestamp can be shown to a person as a date instead of as a bare integer.
pub fn iso8601_utc(secs: i64) -> String {
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}
