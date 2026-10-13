//! C44: source availability and recorded-prefix integrity are distinct facts.
//!
//! The generation/chain verifier below preserves the existing accounting
//! contract. Source comparison treats exactly recorded byte S as EOF.

use crate::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use crate::error::{CoreError, Result};
use crate::ingest::{default_mirror_root, iso8601_utc_now};
use crate::ledger::{AcceptedGeneration, Ledger};
use crate::mirror::{LineageDisposition, MirrorStore, SealedMirror};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const AGENT_SOURCE_CHANGED: &str =
    "The agent's own session file changed after ThruLine recorded it.";

#[derive(Debug, Clone, Serialize)]
pub struct IntegrityAlarm {
    pub file: String,
    pub line: u64,
    pub stored_sha256: String,
    pub found_sha256: String,
}

/// Exact event-receipt coverage for one unavailable session/file pair.
/// The minimum and maximum are observed bounds, not a contiguous line range.
#[derive(Debug, Clone, Serialize)]
pub struct UnavailableRawReceiptCoverage {
    pub session_id: String,
    pub file: String,
    pub event_receipt_count: u64,
    pub min_recorded_line: u64,
    pub max_recorded_line: u64,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct VerifyReport {
    pub projects: BTreeMap<String, ProjectVerdict>,
    pub elapsed_ms: u128,
    pub complete: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ProjectVerdict {
    pub complete: bool,
    pub events_checked: u64,
    pub raw_matches: u64,
    pub raw_matches_via_mirror: u64,
    pub swept_files_preserved: Vec<String>,
    pub raw_mismatches: Vec<IntegrityAlarm>,
    pub unavailable_raw_receipts: Vec<UnavailableRawReceiptCoverage>,
    pub chain_matches: u64,
    pub chain_mismatches: Vec<String>,
    pub missing_files: Vec<String>,
    pub digest_mismatches: Vec<String>,
    pub digests_checked: u64,
    pub head_hash: String,
    pub sources: Vec<SourceReceipt>,
    pub unverifiable: Vec<String>,
}

impl ProjectVerdict {
    pub fn exit_code(&self) -> u8 {
        if !self.raw_mismatches.is_empty()
            || !self.chain_mismatches.is_empty()
            || !self.digest_mismatches.is_empty()
            || self.sources.iter().any(|s| {
                s.state == SourceState::PrefixMismatch || s.sealed_copy == SealedCopyState::Corrupt
            })
        {
            1
        } else if !self.complete
            || !self.unverifiable.is_empty()
            || !self.unavailable_raw_receipts.is_empty()
            || !self.missing_files.is_empty()
            || self.sources.iter().any(|s| {
                matches!(s.state, SourceState::SourceGone | SourceState::Unverifiable)
                    || matches!(
                        s.sealed_copy,
                        SealedCopyState::Missing | SealedCopyState::Unverifiable
                    )
            })
        {
            2
        } else {
            0
        }
    }
    pub fn ok(&self) -> bool {
        self.exit_code() == 0
    }

    fn qualify_changed_source_reasons(&mut self, source_checks: &BTreeMap<String, SourceCheck>) {
        let record_intact = self.complete
            && self.chain_matches == self.events_checked
            && self.chain_mismatches.is_empty();
        for source in &mut self.sources {
            if source.state != SourceState::PrefixMismatch {
                continue;
            }
            let original_intact = source_checks
                .get(&source.session_id)
                .is_some_and(|check| check.original_record_intact);
            source.reason = Some(
                if record_intact && original_intact && source.sealed_copy == SealedCopyState::Intact
                {
                    format!(
                    "{AGENT_SOURCE_CHANGED} ThruLine's record of the original is intact, and its sealed copy is intact."
                )
                } else {
                    AGENT_SOURCE_CHANGED.to_string()
                },
            );
        }
    }

    fn record_unavailable_raw_receipt(
        &mut self,
        session_id: &str,
        file: &str,
        line: u64,
        indices: &mut BTreeMap<String, usize>,
    ) -> Result<()> {
        let index = if let Some(index) = indices.get(file) {
            *index
        } else {
            let index = self.unavailable_raw_receipts.len();
            self.unavailable_raw_receipts
                .push(UnavailableRawReceiptCoverage {
                    session_id: session_id.to_string(),
                    file: file.to_string(),
                    event_receipt_count: 0,
                    min_recorded_line: line,
                    max_recorded_line: line,
                    reason:
                        "recorded raw receipt cannot be answered from source or a verified seal",
                });
            indices.insert(file.to_string(), index);
            index
        };
        let group = self
            .unavailable_raw_receipts
            .get_mut(index)
            .ok_or_else(|| {
                CoreError::Config("unavailable raw receipt coverage index is invalid".into())
            })?;
        group.event_receipt_count = group.event_receipt_count.checked_add(1).ok_or_else(|| {
            CoreError::Config("unavailable raw receipt coverage count overflow".into())
        })?;
        group.min_recorded_line = group.min_recorded_line.min(line);
        group.max_recorded_line = group.max_recorded_line.max(line);
        Ok(())
    }
}
impl VerifyReport {
    pub fn exit_code(&self) -> u8 {
        if self.projects.values().any(|p| p.exit_code() == 1) {
            1
        } else if !self.complete || self.projects.values().any(|p| p.exit_code() == 2) {
            2
        } else {
            0
        }
    }
    pub fn ok(&self) -> bool {
        self.exit_code() == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SourceState {
    Intact,
    GrewAfterRecording,
    SourceGone,
    PrefixMismatch,
    Unverifiable,
}
impl SourceState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Intact => "INTACT",
            Self::GrewAfterRecording => "GREW AFTER RECORDING",
            Self::SourceGone => "SOURCE GONE",
            Self::PrefixMismatch => "PREFIX MISMATCH",
            Self::Unverifiable => "UNVERIFIABLE",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SealedCopyState {
    Intact,
    Missing,
    Corrupt,
    Unverifiable,
    NotRecorded,
}
#[derive(Debug, Clone, Serialize)]
pub struct SourceReceipt {
    pub session_id: String,
    pub path: Option<String>,
    pub recorded_bytes: Option<u64>,
    pub recorded_sha256: Option<String>,
    pub prefix_sha256: Option<String>,
    pub current_bytes: Option<u64>,
    pub unrecorded_bytes: Option<u64>,
    pub state: SourceState,
    pub sealed_copy: SealedCopyState,
    pub reason: Option<String>,
}

struct SourceCheck {
    receipt: SourceReceipt,
    frame: Option<Frame>,
    mirror_frame: Option<Frame>,
    original_record_intact: bool,
}

struct Frame {
    lines: BTreeMap<u64, String>,
    line_count: u64,
    line_digest: String,
    byte_sha256: String,
    bytes_read: u64,
    current_bytes: u64,
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

// Absence, permissions and unsafe paths do not prove tampering. These two
// errors are the mirror verifier's closed, byte/receipt mismatch conditions.
fn known_seal_corruption(error: &CoreError) -> bool {
    matches!(error, CoreError::Integrity(message) if matches!(message.as_str(),
        "mirror seal digest does not match its accepted receipt"
        | "sealed mirror payload does not match its accepted receipt"))
}

fn generation_stores(
    ledger: &Ledger,
    sd: &crate::ledger::SessionDigest,
    generation: &AcceptedGeneration,
) -> Vec<PathBuf> {
    let mut roots = candidate_mirror_stores(ledger);
    if let Ok(root) =
        mirror_root_from_payload(Path::new(&sd.file_path), &generation.mirror_payload_path)
    {
        // Only Codex's session path is an accepted payload. A vendor path
        // cannot manufacture an additional mirror root.
        if roots.iter().all(|existing| existing != &root)
            && sd.file_path.ends_with(&generation.mirror_payload_path)
        {
            roots.push(root);
        }
    }
    roots
}

fn source_check(
    ledger: &Ledger,
    project: &str,
    sd: &crate::ledger::SessionDigest,
) -> Result<SourceCheck> {
    use rusqlite::OptionalExtension;
    let generation = ledger.accepted_generation(&sd.session_id)?;
    let identity: Option<(String, String, Option<String>)> = ledger
        .conn()
        .query_row(
            "SELECT project_id,source,source_locator FROM source_sessions WHERE session_id=?1",
            [&sd.session_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let path = match (&generation, &identity) {
        (Some(_), Some((owner, source, Some(locator))))
            if owner == project
                && matches!(source.as_str(), "claude_code" | "codex")
                && Path::new(locator).is_absolute() =>
        {
            Some(locator.clone())
        }
        (None, _) => Some(sd.file_path.clone()),
        _ => None,
    };
    let mut checked = SourceCheck {
        receipt: SourceReceipt {
            session_id: sd.session_id.clone(),
            path,
            recorded_bytes: generation.as_ref().map(|g| g.payload_bytes),
            recorded_sha256: generation.as_ref().map(|g| g.payload_sha256.clone()),
            prefix_sha256: None,
            current_bytes: None,
            unrecorded_bytes: None,
            state: SourceState::Unverifiable,
            sealed_copy: if generation.is_some() {
                SealedCopyState::Missing
            } else {
                SealedCopyState::NotRecorded
            },
            reason: None,
        },
        frame: None,
        mirror_frame: None,
        original_record_intact: false,
    };
    if let Some(generation) = &generation {
        if generation.session_id != sd.session_id || generation.project_id != project {
            checked.receipt.sealed_copy = SealedCopyState::Corrupt;
            checked.receipt.reason = Some("accepted generation identity mismatch".into());
            return Ok(checked);
        }
        let sealed = sealed_from_generation(generation);
        for root in generation_stores(ledger, sd, generation) {
            let Ok(store) = MirrorStore::open_for_verification(root) else {
                continue;
            };
            match store.verify(&sealed) {
                Ok(_) => {
                    match store
                        .resolve_payload(&sealed)
                        .and_then(|path| read_frame(&path, Some(generation.payload_bytes), None))
                    {
                        Ok(frame)
                            if frame.bytes_read == generation.payload_bytes
                                && frame.byte_sha256 == generation.payload_sha256 =>
                        {
                            checked.receipt.sealed_copy = SealedCopyState::Intact;
                            checked.mirror_frame = Some(frame);
                        }
                        Ok(_) => checked.receipt.sealed_copy = SealedCopyState::Corrupt,
                        Err(_) => checked.receipt.sealed_copy = SealedCopyState::Unverifiable,
                    }
                    break;
                }
                Err(error) if known_seal_corruption(&error) => {
                    checked.receipt.sealed_copy = SealedCopyState::Corrupt;
                    break;
                }
                Err(CoreError::Io { source, .. })
                    if source.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => checked.receipt.sealed_copy = SealedCopyState::Unverifiable,
            }
        }
    }
    let Some(path) = checked.receipt.path.as_deref() else {
        checked.receipt.reason =
            Some("recorded original source locator is absent or unsupported".into());
        return Ok(checked);
    };
    let byte_bound = generation.as_ref().map(|g| g.payload_bytes);
    // Legacy rows have no byte-S receipt. Their recorded line digest remains
    // testable; a line-count bound keeps later appended rows outside that check.
    let line_bound = if generation.is_none() {
        Some(sd.raw_line_count)
    } else {
        None
    };
    match read_frame(Path::new(path), byte_bound, line_bound) {
        Ok(frame) => {
            checked.receipt.current_bytes = Some(frame.current_bytes);
            checked.receipt.prefix_sha256 = Some(frame.byte_sha256.clone());
            match &generation {
                Some(g) if frame.bytes_read != g.payload_bytes || frame.byte_sha256 != g.payload_sha256 => {
                    checked.receipt.state = SourceState::PrefixMismatch;
                    checked.receipt.reason = Some(AGENT_SOURCE_CHANGED.into());
                }
                Some(g) if frame.current_bytes > g.payload_bytes => {
                    checked.receipt.state = SourceState::GrewAfterRecording;
                    checked.receipt.unrecorded_bytes = Some(frame.current_bytes - g.payload_bytes);
                }
                Some(_) => checked.receipt.state = SourceState::Intact,
                None => checked.receipt.reason = Some("legacy recording has no accepted byte-S prefix receipt; recorded line hashes are checked".into()),
            }
            checked.frame = Some(frame);
        }
        Err(CoreError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            checked.receipt.state = SourceState::SourceGone;
            checked.receipt.reason = Some("recorded original source no longer exists".into());
        }
        Err(_) => {
            checked.receipt.reason =
                Some("original source is not a stable, readable regular-file prefix".into())
        }
    }
    Ok(checked)
}

fn recorded_mirror_frame(ledger: &Ledger, session: &str, raw_path: &Path) -> Option<Frame> {
    let sd = ledger
        .conn()
        .query_row(
            "SELECT id,file_path,raw_line_count,raw_digest FROM sessions WHERE id=?1",
            [session],
            |r| {
                Ok(crate::ledger::SessionDigest {
                    session_id: r.get(0)?,
                    file_path: r.get(1)?,
                    raw_line_count: r.get::<_, i64>(2)?.max(0) as u64,
                    raw_digest: r.get(3)?,
                })
            },
        )
        .ok()?;
    for generation in ledger.accepted_generation_lineage(session).ok()? {
        let sealed = sealed_from_generation(&generation);
        for root in generation_stores(ledger, &sd, &generation) {
            let Ok(store) = MirrorStore::open_for_verification(root) else {
                continue;
            };
            let Ok(payload) = store.resolve_payload(&sealed) else {
                continue;
            };
            if payload == raw_path && store.verify(&sealed).is_ok() {
                return read_frame(&payload, Some(generation.payload_bytes), None)
                    .ok()
                    .filter(|f| {
                        f.bytes_read == generation.payload_bytes
                            && f.byte_sha256 == generation.payload_sha256
                    });
            }
        }
    }
    None
}

/// Read exactly S as EOF (or the legacy N-line prefix), never the live tail.
/// Descriptor-relative no-follow opens reject FIFO/symlink substitutions;
/// two prefix hashes qualify the bytes while allowing benign later appends.
fn read_frame(path: &Path, byte_bound: Option<u64>, line_bound: Option<u64>) -> Result<Frame> {
    use sha2::{Digest, Sha256};
    use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
    let mut opened = open_regular(path)?;
    let initial = opened.file.metadata().map_err(|e| CoreError::io(path, e))?;
    let limit = byte_bound.unwrap_or(initial.len());
    let mut reader = BufReader::new((&mut opened.file).take(limit));
    let mut lines = BTreeMap::new();
    let mut line_count = 0u64;
    let mut bytes_read = 0u64;
    let mut byte_hash = Sha256::new();
    let mut line_hash = Sha256::new();
    while line_bound.is_none_or(|bound| line_count < bound) {
        let mut bytes = Vec::new();
        let n = reader
            .by_ref()
            .take((crate::codex_ingest::MAX_CODEX_LINE_BYTES + 2) as u64)
            .read_until(b'\n', &mut bytes)
            .map_err(|e| CoreError::io(path, e))?;
        if n == 0 {
            break;
        }
        if bytes.len() > crate::codex_ingest::MAX_CODEX_LINE_BYTES + 1 {
            return Err(CoreError::Config(
                "recorded raw line exceeds the supported read bound".into(),
            ));
        }
        bytes_read = bytes_read
            .checked_add(n as u64)
            .ok_or_else(|| CoreError::Config("recorded byte count overflow".into()))?;
        byte_hash.update(&bytes);
        if bytes.last() == Some(&b'\n') {
            bytes.pop();
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
        }
        line_count = line_count
            .checked_add(1)
            .ok_or_else(|| CoreError::Config("recorded line count overflow".into()))?;
        line_hash.update((bytes.len() as u64).to_le_bytes());
        line_hash.update(&bytes);
        lines.insert(line_count, sha256_hex(&bytes));
    }
    let byte_sha256 = crate::chain::to_hex(&byte_hash.finalize());
    drop(reader);
    opened
        .file
        .seek(SeekFrom::Start(0))
        .map_err(|e| CoreError::io(path, e))?;
    let mut reread = (&mut opened.file).take(bytes_read);
    let mut second_hash = Sha256::new();
    let mut second_bytes = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = reread
            .read(&mut buffer)
            .map_err(|e| CoreError::io(path, e))?;
        if n == 0 {
            break;
        }
        second_hash.update(&buffer[..n]);
        second_bytes += n as u64;
    }
    let after = opened.file.metadata().map_err(|e| CoreError::io(path, e))?;
    if second_bytes != bytes_read
        || crate::chain::to_hex(&second_hash.finalize()) != byte_sha256
        || !same_identity(&initial, &after)
        || after.len() < bytes_read
        || !opened.named_identity_matches()?
    {
        return Err(CoreError::Config(
            "recorded source prefix changed while it was being read".into(),
        ));
    }
    Ok(Frame {
        lines,
        line_count,
        line_digest: crate::chain::to_hex(&line_hash.finalize()),
        byte_sha256,
        bytes_read,
        current_bytes: after.len(),
    })
}

struct RegularFile {
    file: std::fs::File,
    path: PathBuf,
    parents: Vec<(PathBuf, std::fs::File)>,
}

impl RegularFile {
    fn named_identity_matches(&self) -> Result<bool> {
        let named =
            std::fs::symlink_metadata(&self.path).map_err(|e| CoreError::io(&self.path, e))?;
        let file = self
            .file
            .metadata()
            .map_err(|e| CoreError::io(&self.path, e))?;
        if !named.is_file() || !same_identity(&named, &file) {
            return Ok(false);
        }
        for (path, held) in &self.parents {
            let named = std::fs::symlink_metadata(path).map_err(|e| CoreError::io(path, e))?;
            let descriptor = held.metadata().map_err(|e| CoreError::io(path, e))?;
            if !named.is_dir() || !same_identity(&named, &descriptor) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

fn same_identity(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        a.dev() == b.dev() && a.ino() == b.ino() && a.mode() == b.mode() && a.uid() == b.uid()
    }
    #[cfg(not(unix))]
    {
        a.is_file() == b.is_file() && a.created().ok() == b.created().ok()
    }
}

fn open_regular(path: &Path) -> Result<RegularFile> {
    use std::path::Component;
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| CoreError::io(path, e))?
            .join(path)
    };
    if path
        .components()
        .any(|c| !matches!(c, Component::RootDir | Component::Normal(_)))
    {
        return Err(CoreError::Config(
            "recorded source path is not a closed absolute file path".into(),
        ));
    }
    #[cfg(unix)]
    {
        use rustix::fs::{open, openat, Mode, OFlags};
        let directory_flags =
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let mut current: std::fs::File = open("/", directory_flags, Mode::empty())
            .map_err(|e| CoreError::io(&path, e.into()))?
            .into();
        let mut parents = Vec::new();
        let mut prefix = PathBuf::from("/");
        let components: Vec<_> = path
            .components()
            .filter_map(|c| match c {
                Component::Normal(v) => Some(v),
                _ => None,
            })
            .collect();
        let Some((filename, directories)) = components.split_last() else {
            return Err(CoreError::Config(
                "recorded source path names no file".into(),
            ));
        };
        for directory in directories {
            let next: std::fs::File = openat(&current, *directory, directory_flags, Mode::empty())
                .map_err(|e| CoreError::io(&path, e.into()))?
                .into();
            parents.push((prefix.clone(), current));
            current = next;
            prefix.push(directory);
        }
        let descriptor = openat(
            &current,
            *filename,
            OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| CoreError::io(&path, e.into()))?;
        let stat = rustix::fs::fstat(&descriptor).map_err(|e| CoreError::io(&path, e.into()))?;
        if !rustix::fs::FileType::from_raw_mode(stat.st_mode).is_file() {
            return Err(CoreError::Config(
                "recorded source is not a regular file".into(),
            ));
        }
        parents.push((prefix, current));
        let result = RegularFile {
            file: descriptor.into(),
            path,
            parents,
        };
        if !result.named_identity_matches()? {
            return Err(CoreError::Config(
                "recorded source path changed before read".into(),
            ));
        }
        Ok(result)
    }
    #[cfg(not(unix))]
    {
        let metadata = std::fs::symlink_metadata(&path).map_err(|e| CoreError::io(&path, e))?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(CoreError::Config(
                "recorded source is not a regular file".into(),
            ));
        }
        let file = std::fs::File::open(&path).map_err(|e| CoreError::io(&path, e))?;
        let result = RegularFile {
            file,
            path,
            parents: Vec::new(),
        };
        if !result.named_identity_matches()? {
            return Err(CoreError::Config(
                "recorded source path changed before read".into(),
            ));
        }
        Ok(result)
    }
}

#[derive(Debug)]
pub struct VerifyFailure {
    pub report: VerifyReport,
    pub error: CoreError,
}

/// Compatibility entry for callers that only consume completed reports.
pub fn verify(ledger: &Ledger, only_project: Option<&str>) -> Result<VerifyReport> {
    verify_with_report(ledger, only_project).map_err(|failure| failure.error)
}

/// A failed read retains checks already performed, including known mismatches.
/// Completion means the requested checks ran to their end, not that all
/// sources were available or all integrity checks passed.
pub fn verify_with_report(
    ledger: &Ledger,
    only_project: Option<&str>,
) -> std::result::Result<VerifyReport, VerifyFailure> {
    let started = std::time::Instant::now();
    let mut report = VerifyReport::default();
    let outcome = verify_into(ledger, only_project, &mut report);
    report.elapsed_ms = started.elapsed().as_millis();
    match outcome {
        Ok(()) => {
            report.complete = true;
            Ok(report)
        }
        Err(error) => Err(VerifyFailure { report, error }),
    }
}

fn verify_into(
    ledger: &Ledger,
    only_project: Option<&str>,
    report: &mut VerifyReport,
) -> Result<()> {
    use rusqlite::OptionalExtension;

    let now = iso8601_utc_now();

    for project_id in ledger.project_ids()? {
        if let Some(want) = only_project {
            if project_id != want {
                continue;
            }
        }
        let verdict = report.projects.entry(project_id.clone()).or_default();
        let mut source_checks = BTreeMap::new();
        let mut resolved_by_mirror = std::collections::BTreeSet::new();
        let mut unanswered_files = std::collections::BTreeSet::new();
        let session_digests: BTreeMap<_, _> = ledger
            .session_digests(&project_id)?
            .into_iter()
            .map(|sd| (sd.session_id.clone(), sd))
            .collect();

        for (session_id, _file_path) in ledger.sessions_of(&project_id)? {
            let mut original_receipts_intact = true;
            if let Some(sd) = session_digests.get(&session_id) {
                let checked = source_check(ledger, &project_id, sd)?;
                verdict.sources.push(checked.receipt.clone());
                source_checks.insert(session_id.clone(), checked);
            }
            let stored = ledger.session_chain(&session_id)?;

            // Receipts bind each event to the immutable raw file that actually
            // produced it. A successor session path may point at a newer full
            // mirror; it never replaces a predecessor receipt.
            let mut raw_files: BTreeMap<String, Option<BTreeMap<u64, String>>> = BTreeMap::new();
            let mut unavailable_indices = BTreeMap::new();

            // RB1.5 — the sealed generation is the survival copy: when the
            // vendor file is gone (or a line is), the accepted payload answers
            // for it, counted apart so a mirror pass never impersonates a live
            // source. Loaded lazily; a session never sealed simply has None.
            let mut mirror_lines: Option<Option<BTreeMap<u64, String>>> = None;
            let load_mirror = |ledger: &Ledger| -> Option<BTreeMap<u64, String>> {
                let generation = ledger.accepted_generation(&session_id).ok().flatten()?;
                let sealed = sealed_from_generation(&generation);
                candidate_mirror_stores(ledger)
                    .into_iter()
                    .find_map(|root| {
                        let store = MirrorStore::open_for_verification(root).ok()?;
                        store.verify(&sealed).ok()?;
                        let payload = store.resolve_payload(&sealed).ok()?;
                        read_frame(&payload, Some(generation.payload_bytes), None)
                            .ok()
                            .filter(|f| {
                                f.bytes_read == generation.payload_bytes
                                    && f.byte_sha256 == generation.payload_sha256
                            })
                            .map(|f| f.lines)
                    })
            };

            let mut prev = genesis_prev_hash(&project_id);
            for link in &stored {
                verdict.events_checked += 1;
                if !raw_files.contains_key(&link.raw_file) {
                    let hashed = match source_checks.get(&session_id) {
                        Some(check)
                            if check.receipt.path.as_deref() == Some(link.raw_file.as_str()) =>
                        {
                            check.frame.as_ref().map(|f| f.lines.clone())
                        }
                        _ => recorded_mirror_frame(ledger, &session_id, Path::new(&link.raw_file))
                            .map(|f| f.lines),
                    };
                    if hashed.is_none() && !verdict.missing_files.contains(&link.raw_file) {
                        verdict.missing_files.push(link.raw_file.clone());
                    }
                    raw_files.insert(link.raw_file.clone(), hashed);
                }
                let fresh = raw_files.get(&link.raw_file).and_then(Option::as_ref);
                let disk = fresh.and_then(|lines| lines.get(&link.raw_line));
                if let Some(check) = source_checks.get(&session_id) {
                    if check.receipt.state == SourceState::PrefixMismatch {
                        // Qualify every original receipt, including one that
                        // happens to match today's changed source. For a raw
                        // mirror path, `fresh` already came from the exact
                        // verified recorded_mirror_frame path and generation.
                        let original =
                            if check.receipt.path.as_deref() == Some(link.raw_file.as_str()) {
                                check.mirror_frame.as_ref().map(|frame| &frame.lines)
                            } else {
                                fresh
                            };
                        original_receipts_intact &= original
                            .and_then(|lines| lines.get(&link.raw_line))
                            == Some(&link.raw_sha256);
                    }
                }
                match disk {
                    Some(found) if *found == link.raw_sha256 => verdict.raw_matches += 1,
                    // A legacy final line has no exact EOF witness: a later
                    // append can change its physical line without changing
                    // recorded bytes. Retain that observed discrepancy as U.
                    // Earlier lines have a later recorded line, hence a real
                    // newline boundary; accepted S frames also have exact EOF.
                    Some(found)
                        if source_checks
                            .get(&session_id)
                            .is_some_and(|c| c.receipt.recorded_bytes.is_none())
                            && session_digests
                                .get(&session_id)
                                .is_some_and(|sd| link.raw_line >= sd.raw_line_count) =>
                    {
                        verdict.unverifiable.push(format!(
                            "{session_id}: raw receipt discrepancy {}:{} stored {} / found {}; no independent byte-boundary witness for the final recorded line",
                            link.raw_file, link.raw_line, link.raw_sha256, found));
                    }
                    Some(found) => {
                        verdict.raw_mismatches.push(IntegrityAlarm {
                            file: link.raw_file.clone(),
                            line: link.raw_line,
                            stored_sha256: link.raw_sha256.clone(),
                            found_sha256: found.clone(),
                        });
                    }
                    None => {
                        let mirrored = mirror_lines
                            .get_or_insert_with(|| load_mirror(ledger))
                            .as_ref()
                            .and_then(|lines| lines.get(&link.raw_line));
                        match mirrored {
                            Some(found) if *found == link.raw_sha256 => {
                                verdict.raw_matches_via_mirror += 1;
                                resolved_by_mirror.insert(link.raw_file.clone());
                            }
                            _ => {
                                original_receipts_intact = false;
                                unanswered_files.insert(link.raw_file.clone());
                                // Update the public coverage immediately so an
                                // independently aborted verification retains
                                // the exact receipts already checked.
                                verdict.record_unavailable_raw_receipt(
                                    &session_id,
                                    &link.raw_file,
                                    link.raw_line,
                                    &mut unavailable_indices,
                                )?;
                            }
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
            for index in unavailable_indices.into_values() {
                let group = verdict.unavailable_raw_receipts.get(index).ok_or_else(|| {
                    CoreError::Config("unavailable raw receipt coverage index is invalid".into())
                })?;
                verdict.unverifiable.push(format!(
                    "{session_id}: {} raw event receipts in {} cannot be read from source or a verified seal; recorded line minimum {}, maximum {} (bounds do not imply contiguous coverage)",
                    group.event_receipt_count, group.file, group.min_recorded_line, group.max_recorded_line
                ));
            }
            // Only one session's raw-line map is retained at a time. Digest
            // and byte receipts are small and remain for the accounting pass.
            if let Some(check) = source_checks.get_mut(&session_id) {
                check.original_record_intact = original_receipts_intact;
                if let Some(frame) = &mut check.frame {
                    frame.lines.clear();
                }
                if let Some(frame) = &mut check.mirror_frame {
                    frame.lines.clear();
                }
            }
        }

        // Whole-file digests: the only check that covers malformed lines.
        for sd in session_digests.into_values() {
            let (session_id, file_path, stored_count) =
                (sd.session_id, sd.file_path, sd.raw_line_count);
            let stored_digest = sd.raw_digest;
            let mut original_record_intact = source_checks
                .get(&session_id)
                .is_some_and(|check| check.original_record_intact);
            if stored_digest.is_none() {
                verdict
                    .unverifiable
                    .push(format!("{session_id}: no stored digest"));
            } else {
                verdict.digests_checked += 1;
            }
            // Missing legacy digest bytes do not remove independent accepted
            // generation seal, lineage, disposition or accounting evidence.
            let accepted = ledger.accepted_generation(&session_id)?;
            if let Some(head_generation) = &accepted {
                let independent_digest_failures = verdict.digest_mismatches.len();
                let independent_unavailable = verdict.unverifiable.len();
                let lineage = ledger.accepted_generation_lineage(&session_id)?;
                for generation in &lineage {
                    verify_generation_accounting(ledger, generation, &session_id, verdict)?;
                }
                verify_generation_coverage(ledger, &session_id, verdict)?;
                // Codex sessions store the payload path AS the session path, so
                // the root falls out by stripping; claude sessions keep the
                // vendor path (RB1.5) and resolve through the default
                // namespaced store instead. Both are read-only opens.
                let store = mirror_root_from_payload(
                    Path::new(&file_path),
                    &head_generation.mirror_payload_path,
                )
                .and_then(|root| MirrorStore::open_for_verification(&root))
                .or_else(|first_err| {
                    candidate_mirror_stores(ledger)
                        .into_iter()
                        .find_map(|root| MirrorStore::open_for_verification(root).ok())
                        .ok_or(first_err)
                });
                match store {
                    Err(_) => verdict.unverifiable.push(format!(
                        "{session_id}: accepted mirror root is not verifiable"
                    )),
                    Ok(store) => {
                        let mut prior: Option<SealedMirror> = None;
                        for generation in lineage {
                            let sealed = sealed_from_generation(&generation);
                            if let Err(error) = store.verify(&sealed) {
                                let message = format!(
                                    "{session_id}: generation {} failed seal verification",
                                    generation.id
                                );
                                if known_seal_corruption(&error) {
                                    verdict.digest_mismatches.push(message);
                                } else {
                                    verdict.unverifiable.push(message);
                                }
                            }
                            match store.resolve_payload(&sealed).and_then(|payload|
                                verify_generation_disposition_bytes(ledger, &generation, &payload)) {
                                Ok(true) => {},
                                Ok(false) => verdict.digest_mismatches.push(format!(
                                    "{session_id}: generation {} disposition bytes failed verification", generation.id)),
                                Err(CoreError::Integrity(_)) => verdict.digest_mismatches.push(format!(
                                    "{session_id}: generation {} disposition byte accounting is contradictory", generation.id)),
                                Err(_) => verdict.unverifiable.push(format!(
                                    "{session_id}: generation {} disposition bytes are unavailable", generation.id)),
                            }
                            let expected_lineage = match &prior {
                                None => LineageDisposition::Initial,
                                Some(_) => LineageDisposition::PrefixSuccessor,
                            };
                            match store.classify(&sealed, prior.as_ref()) {
                                Ok(found) if found == expected_lineage => {},
                                Ok(_) => verdict.digest_mismatches.push(format!(
                                    "{session_id}: generation {} failed prefix-lineage verification", generation.id)),
                                Err(_) => verdict.unverifiable.push(format!(
                                    "{session_id}: generation {} prefix-lineage is unavailable", generation.id)),
                            }
                            prior = Some(sealed);
                        }
                    }
                }
                original_record_intact &= verdict.digest_mismatches.len()
                    == independent_digest_failures
                    && verdict.unverifiable.len() == independent_unavailable;
            }
            // Recheck the stored line dialect at the recorded EOF, through
            // source bytes or the independently verified accepted payload.
            let check = source_checks.get(&session_id);
            let frame = check.and_then(|c| c.frame.as_ref().or(c.mirror_frame.as_ref()));
            if let Some(frame) = frame {
                if frame.line_count != stored_count {
                    verdict.digest_mismatches.push(format!(
                        "{session_id}: recorded raw_line_count stored {stored_count} / found {}",
                        frame.line_count
                    ));
                }
                let source: Option<String> = ledger
                    .conn()
                    .query_row(
                        "SELECT source FROM source_sessions WHERE session_id=?1",
                        [&session_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?;
                // Claude accepts the payload byte digest atomically before a
                // separate parse replaces it with the line digest. Both are
                // supported receipts; recognizing the acceptance format does
                // not establish that parsing completed. Require its exact
                // head identity and an independently checked recorded frame.
                let accepted_claude_bytes = source.as_deref() == Some("claude_code")
                    && accepted.as_ref().is_some_and(|generation| {
                        generation.project_id == project_id
                            && generation.session_id == session_id
                            && stored_digest.as_deref() == Some(generation.payload_sha256.as_str())
                            && frame.bytes_read == generation.payload_bytes
                            && frame.byte_sha256 == generation.payload_sha256
                    });
                if check.is_some_and(|check| check.receipt.state == SourceState::PrefixMismatch) {
                    // Qualify the original separately from today's changed
                    // source. A seal alone cannot excuse corrupt session
                    // metadata or an unanswered original event receipt.
                    original_record_intact &= accepted
                        .as_ref()
                        .zip(check.and_then(|check| check.mirror_frame.as_ref()))
                        .is_some_and(|(generation, original)| {
                            generation.project_id == project_id
                                && generation.session_id == session_id
                                && original.bytes_read == generation.payload_bytes
                                && original.byte_sha256 == generation.payload_sha256
                                && original.line_count == stored_count
                                && match source.as_deref() {
                                    Some("codex") => {
                                        stored_digest.as_deref()
                                            == Some(original.byte_sha256.as_str())
                                    }
                                    Some("claude_code") => {
                                        stored_digest.as_deref()
                                            == Some(original.byte_sha256.as_str())
                                            || stored_digest.as_deref()
                                                == Some(original.line_digest.as_str())
                                    }
                                    _ => false,
                                }
                        });
                }
                let found = if accepted.is_some()
                    && (source.as_deref() == Some("codex") || accepted_claude_bytes)
                {
                    &frame.byte_sha256
                } else {
                    &frame.line_digest
                };
                if let Some(stored_digest) = stored_digest.filter(|stored| stored != found) {
                    let discrepancy = format!(
                        "{session_id}: recorded raw_digest stored {stored_digest} / found {found}"
                    );
                    if accepted.is_some() {
                        verdict.digest_mismatches.push(discrepancy);
                    } else {
                        verdict.unverifiable.push(format!(
                        "{discrepancy}; no independent byte-boundary witness for the final recorded line"));
                    }
                }
            } else {
                verdict.unverifiable.push(format!(
                    "{session_id}: recorded digest bytes are unavailable"
                ));
            }
            if let Some(check) = source_checks.get_mut(&session_id) {
                check.original_record_intact = original_record_intact;
            }
        }

        // The split the UI could not make on its own (A147/A150): the chain's
        // own arithmetic is one fact; dead receipts are another. `verified_at`
        // keeps its strict meaning — everything passed.
        let chain_intact = verdict.chain_mismatches.is_empty();
        let missing = verdict.missing_files.len() as u64;
        // Preservation requires every missing raw receipt to have been
        // answered by a verified mirror, not just an absence of mismatches.
        let alarmed: std::collections::BTreeSet<&str> = verdict
            .raw_mismatches
            .iter()
            .map(|a| a.file.as_str())
            .collect();
        let (preserved, still_missing): (Vec<String>, Vec<String>) =
            verdict.missing_files.drain(..).partition(|f| {
                !alarmed.contains(f.as_str())
                    && resolved_by_mirror.contains(f)
                    && !unanswered_files.contains(f)
            });
        verdict.missing_files = still_missing;
        verdict.swept_files_preserved.extend(preserved);

        verdict.complete = true;
        let head = if verdict.ok() {
            ledger.record_verification(&project_id, Some(&now), chain_intact, missing)
        } else {
            ledger.record_verification(&project_id, None, chain_intact, missing)
        };
        verdict.head_hash = match head {
            Ok(head) => head,
            Err(error) => {
                verdict.complete = false;
                return Err(error);
            }
        };
        verdict.qualify_changed_source_reasons(&source_checks);
    }

    if only_project.is_some() && report.projects.is_empty() {
        return Err(CoreError::Config(
            "selected project has no recorded ledger project".into(),
        ));
    }
    Ok(())
}

/// The namespaced mirror stores this ledger could have sealed into: the
/// default root (production CLI) and the db-sibling root (tests, custom
/// `--mirror-root` beside the ledger). Verify tries each; sealing wrote one.
fn candidate_mirror_stores(ledger: &Ledger) -> Vec<PathBuf> {
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

// These checks depend only on recorded metadata. Unavailable source, seal
// bytes or mirror directories cannot hide a proved accounting contradiction.
fn verify_generation_accounting(
    ledger: &Ledger,
    generation: &AcceptedGeneration,
    session_id: &str,
    verdict: &mut ProjectVerdict,
) -> Result<()> {
    if generation.total_envelopes < generation.local_envelopes
        || generation.payload_bytes < generation.inherited_prefix_bytes
    {
        verdict.digest_mismatches.push(format!(
            "{session_id}: generation {} metadata accounting is contradictory",
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
    Ok(())
}

fn verify_generation_coverage(
    ledger: &Ledger,
    session_id: &str,
    verdict: &mut ProjectVerdict,
) -> Result<()> {
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
    Ok(())
}

fn verify_generation_disposition_bytes(
    ledger: &Ledger,
    generation: &AcceptedGeneration,
    payload: &Path,
) -> Result<bool> {
    use std::io::{Read, Seek, SeekFrom};

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
    let mut file = std::fs::File::open(payload).map_err(|e| CoreError::io(payload, e))?;
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
