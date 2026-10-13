//! Bounded, private staging for one candidate generation's normalized rows.
//!
//! The spool is only transaction input. It has no ledger or truth authority,
//! and is removed after either acceptance or deterministic rejection cleanup.

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

use crate::error::{CoreError, Result};
use crate::ledger::{GenerationRowExpectation, GenerationRowSource, RawDisposition};
use crate::model::{NormalizedEvent, RawLineRef};

const MAX_BUFFERED_ROWS: usize = 1_024;
const MAX_BUFFERED_BYTES: usize = 16 * 1024 * 1024;

enum PendingRow {
    Disposition(RawDisposition),
    Event(Vec<u8>, Vec<u8>),
}

pub struct GenerationSpool {
    path: PathBuf,
    conn: Connection,
    pending: Vec<PendingRow>,
    pending_bytes: usize,
    disposition_count: u64,
    event_count: u64,
    flush_count: u64,
    max_buffered_rows: usize,
    max_buffered_bytes: usize,
    finished: bool,
}

impl GenerationSpool {
    pub fn create(mirror_root: &Path, generation_key: &str) -> Result<Self> {
        if generation_key.is_empty()
            || !generation_key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        {
            return Err(CoreError::Integrity(
                "generation spool key is not path-safe".to_string(),
            ));
        }
        let dir = mirror_root.join("spools");
        match fs::symlink_metadata(&dir) {
            Ok(metadata) => {
                if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
                    return Err(CoreError::Integrity(
                        "generation spool directory is not a private non-symlink directory"
                            .to_string(),
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let mut builder = fs::DirBuilder::new();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    builder.mode(0o700);
                }
                builder
                    .create(&dir)
                    .map_err(|error| CoreError::io(&dir, error))?;
                sync_parent(&dir)?;
            }
            Err(error) => return Err(CoreError::io(&dir, error)),
        }
        set_private_mode(&dir, 0o700)?;
        let path = dir.join(format!("{generation_key}.sqlite"));
        remove_spool_files(&path)?;

        create_private_file(&path)?;
        let conn = Connection::open(&path)?;
        set_private_mode(&path, 0o600)?;
        conn.execute_batch(
            "PRAGMA journal_mode=DELETE;
             PRAGMA synchronous=FULL;
             PRAGMA temp_store=FILE;
             CREATE TABLE dispositions(
               ordinal INTEGER PRIMARY KEY,
               raw_line INTEGER NOT NULL,
               raw_sha256 TEXT NOT NULL,
               byte_start INTEGER NOT NULL,
               byte_length INTEGER NOT NULL,
               envelope_family TEXT NOT NULL,
               disposition TEXT NOT NULL,
               reason TEXT,
               source_turn_id TEXT,
               turn_boundary_raw_line INTEGER,
               turn_boundary_raw_sha256 TEXT,
               emitted_event_count INTEGER NOT NULL
             );
             CREATE TABLE events(
               ordinal INTEGER PRIMARY KEY,
               raw_json BLOB NOT NULL,
               event_json BLOB NOT NULL
             );",
        )?;
        Ok(Self {
            path,
            conn,
            pending: Vec::new(),
            pending_bytes: 0,
            disposition_count: 0,
            event_count: 0,
            flush_count: 0,
            max_buffered_rows: 0,
            max_buffered_bytes: 0,
            finished: false,
        })
    }

    pub fn push_disposition(&mut self, disposition: RawDisposition) -> Result<()> {
        let estimate = disposition.raw_sha256.len()
            + disposition.envelope_family.len()
            + disposition.disposition.len()
            + disposition.reason.as_deref().map_or(0, str::len)
            + disposition.source_turn_id.as_deref().map_or(0, str::len)
            + disposition
                .turn_boundary_raw_sha256
                .as_deref()
                .map_or(0, str::len)
            + 64;
        self.push_pending(PendingRow::Disposition(disposition), estimate)?;
        self.disposition_count += 1;
        Ok(())
    }

    pub fn push_event(&mut self, raw: &RawLineRef, event: &NormalizedEvent) -> Result<()> {
        let raw_json = serde_json::to_vec(raw)?;
        let event_json = serde_json::to_vec(event)?;
        let bytes = raw_json.len().saturating_add(event_json.len());
        self.push_pending(PendingRow::Event(raw_json, event_json), bytes)?;
        self.event_count += 1;
        Ok(())
    }

    pub fn finish(mut self) -> Result<Self> {
        self.flush()?;
        self.conn
            .execute_batch("PRAGMA optimize; PRAGMA query_only=ON;")?;
        self.finished = true;
        Ok(self)
    }

    pub fn flush_count(&self) -> u64 {
        self.flush_count
    }

    pub fn max_buffered_rows(&self) -> usize {
        self.max_buffered_rows
    }

    pub fn max_buffered_bytes(&self) -> usize {
        self.max_buffered_bytes
    }

    pub fn discard(self) -> Result<()> {
        let path = self.path.clone();
        drop(self.conn);
        remove_spool_files(&path)
    }

    fn push_pending(&mut self, row: PendingRow, bytes: usize) -> Result<()> {
        if bytes > MAX_BUFFERED_BYTES {
            return Err(CoreError::Integrity(
                "one normalized spool row exceeds the 16 MiB buffer cap".to_string(),
            ));
        }
        if !self.pending.is_empty()
            && (self.pending.len() >= MAX_BUFFERED_ROWS
                || self.pending_bytes.saturating_add(bytes) > MAX_BUFFERED_BYTES)
        {
            self.flush()?;
        }
        self.pending.push(row);
        self.pending_bytes = self.pending_bytes.saturating_add(bytes);
        self.max_buffered_rows = self.max_buffered_rows.max(self.pending.len());
        self.max_buffered_bytes = self.max_buffered_bytes.max(self.pending_bytes);
        if self.pending.len() >= MAX_BUFFERED_ROWS || self.pending_bytes >= MAX_BUFFERED_BYTES {
            self.flush()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        {
            let mut disposition_statement = tx.prepare(
                "INSERT INTO dispositions(
                   raw_line,raw_sha256,byte_start,byte_length,envelope_family,
                   disposition,reason,source_turn_id,turn_boundary_raw_line,
                   turn_boundary_raw_sha256,emitted_event_count)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            )?;
            let mut event_statement =
                tx.prepare("INSERT INTO events(raw_json,event_json) VALUES(?1,?2)")?;
            for row in self.pending.drain(..) {
                match row {
                    PendingRow::Disposition(disposition) => {
                        disposition_statement.execute(params![
                            disposition.raw_line as i64,
                            disposition.raw_sha256,
                            disposition.byte_start as i64,
                            disposition.byte_length as i64,
                            disposition.envelope_family,
                            disposition.disposition,
                            disposition.reason,
                            disposition.source_turn_id,
                            disposition.turn_boundary_raw_line.map(|line| line as i64),
                            disposition.turn_boundary_raw_sha256,
                            disposition.emitted_event_count as i64,
                        ])?;
                    }
                    PendingRow::Event(raw_json, event_json) => {
                        event_statement.execute(params![raw_json, event_json])?;
                    }
                }
            }
        }
        tx.commit()?;
        self.pending_bytes = 0;
        self.flush_count += 1;
        Ok(())
    }
}

impl GenerationRowSource for GenerationSpool {
    fn disposition_count(&self) -> u64 {
        self.disposition_count
    }

    fn event_count(&self) -> u64 {
        self.event_count
    }

    fn visit_dispositions(
        &self,
        visitor: &mut dyn FnMut(RawDisposition) -> Result<()>,
    ) -> Result<()> {
        if !self.finished {
            return Err(CoreError::Integrity(
                "generation spool was visited before it was finalized".to_string(),
            ));
        }
        let mut statement = self.conn.prepare(
            "SELECT raw_line,raw_sha256,byte_start,byte_length,envelope_family,
                    disposition,reason,source_turn_id,turn_boundary_raw_line,
                    turn_boundary_raw_sha256,emitted_event_count
             FROM dispositions ORDER BY ordinal",
        )?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            visitor(RawDisposition {
                raw_line: row.get::<_, i64>(0)?.max(0) as u64,
                raw_sha256: row.get(1)?,
                byte_start: row.get::<_, i64>(2)?.max(0) as u64,
                byte_length: row.get::<_, i64>(3)?.max(0) as u64,
                envelope_family: row.get(4)?,
                disposition: row.get(5)?,
                reason: row.get(6)?,
                source_turn_id: row.get(7)?,
                turn_boundary_raw_line: row
                    .get::<_, Option<i64>>(8)?
                    .map(|line| line.max(0) as u64),
                turn_boundary_raw_sha256: row.get(9)?,
                emitted_event_count: row.get::<_, i64>(10)?.max(0) as u64,
            })?;
        }
        Ok(())
    }

    fn visit_events(
        &self,
        visitor: &mut dyn FnMut(RawLineRef, NormalizedEvent) -> Result<()>,
    ) -> Result<()> {
        if !self.finished {
            return Err(CoreError::Integrity(
                "generation spool was visited before it was finalized".to_string(),
            ));
        }
        let mut statement = self
            .conn
            .prepare("SELECT raw_json,event_json FROM events ORDER BY ordinal")?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let raw_json: Vec<u8> = row.get(0)?;
            let event_json: Vec<u8> = row.get(1)?;
            visitor(
                serde_json::from_slice(&raw_json)?,
                serde_json::from_slice(&event_json)?,
            )?;
        }
        Ok(())
    }

    fn validate_accounting(&self, expected: &GenerationRowExpectation<'_>) -> Result<()> {
        if !self.finished {
            return Err(CoreError::Integrity(
                "generation spool was validated before it was finalized".to_string(),
            ));
        }

        let mut disposition_statement = self.conn.prepare(
            "SELECT raw_line,raw_sha256,byte_start,byte_length,
                    source_turn_id,turn_boundary_raw_line,
                    turn_boundary_raw_sha256,emitted_event_count
             FROM dispositions ORDER BY ordinal",
        )?;
        let mut disposition_rows = disposition_statement.query([])?;
        let mut event_statement = self
            .conn
            .prepare("SELECT raw_json,event_json FROM events ORDER BY ordinal")?;
        let mut event_rows = event_statement.query([])?;
        let mut next_event = next_spooled_event(&mut event_rows)?;
        let mut next_line = expected.first_raw_line;
        let mut next_byte = expected.first_byte_start;

        while let Some(row) = disposition_rows.next()? {
            let raw_line = row.get::<_, i64>(0)?;
            let raw_sha256 = row.get::<_, String>(1)?;
            let byte_start = row.get::<_, i64>(2)?;
            let byte_length = row.get::<_, i64>(3)?;
            let source_turn_id = row.get::<_, Option<String>>(4)?;
            let boundary_line = row.get::<_, Option<i64>>(5)?;
            let boundary_sha256 = row.get::<_, Option<String>>(6)?;
            let emitted_event_count = row.get::<_, i64>(7)?;
            let boundary_pair_valid = match (boundary_line, boundary_sha256.as_deref()) {
                (None, None) => source_turn_id.is_none(),
                (Some(line), Some(sha)) => {
                    line > 0
                        && line <= raw_line
                        && sha.len() == 64
                        && source_turn_id.as_ref().is_none_or(|turn| !turn.is_empty())
                }
                _ => false,
            };
            if raw_line < 0
                || byte_start < 0
                || byte_length <= 0
                || emitted_event_count < 0
                || raw_line as u64 != next_line
                || byte_start as u64 != next_byte
                || raw_sha256.len() != 64
                || !boundary_pair_valid
            {
                return Err(CoreError::Integrity(
                    "generation disposition does not cover the exact ordered suffix".to_string(),
                ));
            }

            let mut emitted = 0u64;
            while let Some((raw, event)) = next_event.as_ref() {
                if raw.line < next_line {
                    return Err(CoreError::Integrity(
                        "generation events are not source-ordered".to_string(),
                    ));
                }
                if raw.line != next_line {
                    break;
                }
                if raw.file != expected.mirror_file || raw.sha256 != raw_sha256 {
                    return Err(CoreError::Integrity(
                        "generation event provenance disagrees with its disposition".to_string(),
                    ));
                }
                if event.source_turn_id != source_turn_id
                    || event.source_turn_boundary_line != boundary_line.map(|line| line as u64)
                    || event.source_turn_boundary_sha256 != boundary_sha256
                {
                    return Err(CoreError::Integrity(
                        "generation event turn scope disagrees with its raw disposition"
                            .to_string(),
                    ));
                }
                emitted = emitted.checked_add(1).ok_or_else(|| {
                    CoreError::Integrity("generation event count overflow".to_string())
                })?;
                next_event = next_spooled_event(&mut event_rows)?;
            }
            if emitted != emitted_event_count as u64 {
                return Err(CoreError::Integrity(format!(
                    "generation disposition event count disagrees at raw line {next_line}"
                )));
            }
            next_line = next_line.checked_add(1).ok_or_else(|| {
                CoreError::Integrity("generation raw-line range overflows".to_string())
            })?;
            next_byte = next_byte
                .checked_add(byte_length as u64)
                .ok_or_else(|| CoreError::Integrity("raw byte offset overflow".to_string()))?;
        }

        if next_event.is_some() || next_byte != expected.payload_end {
            return Err(CoreError::Integrity(
                "generation rows do not account for the complete mirrored suffix".to_string(),
            ));
        }
        Ok(())
    }
}

fn next_spooled_event(
    rows: &mut rusqlite::Rows<'_>,
) -> Result<Option<(RawLineRef, NormalizedEvent)>> {
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let raw_json: Vec<u8> = row.get(0)?;
    let event_json: Vec<u8> = row.get(1)?;
    Ok(Some((
        serde_json::from_slice(&raw_json)?,
        serde_json::from_slice(&event_json)?,
    )))
}

fn remove_spool_files(path: &Path) -> Result<()> {
    for candidate in [
        path.to_path_buf(),
        PathBuf::from(format!("{}-journal", path.display())),
        PathBuf::from(format!("{}-wal", path.display())),
        PathBuf::from(format!("{}-shm", path.display())),
    ] {
        match fs::remove_file(&candidate) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(CoreError::io(&candidate, error)),
        }
    }
    sync_parent(path)
}

fn create_private_file(path: &Path) -> Result<()> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options
        .open(path)
        .map_err(|error| CoreError::io(path, error))?;
    file.sync_all()
        .map_err(|error| CoreError::io(path, error))?;
    sync_parent(path)
}

fn sync_parent(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let directory = File::open(parent).map_err(|error| CoreError::io(parent, error))?;
    directory
        .sync_all()
        .map_err(|error| CoreError::io(parent, error))
}

#[cfg(unix)]
fn set_private_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|error| CoreError::io(path, error))
}

#[cfg(not(unix))]
fn set_private_mode(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::model::EventKind;

    #[test]
    fn spool_is_bounded_ordered_and_discarded() {
        let tmp = tempdir().expect("tempdir");
        let mut spool = GenerationSpool::create(tmp.path(), "generation_test").expect("spool");
        for line in 1..=1_100u64 {
            let raw = RawLineRef {
                file: "sealed/payload".to_string(),
                line,
                sha256: format!("{line:064x}"),
            };
            spool
                .push_event(&raw, &NormalizedEvent::new(EventKind::Meta))
                .expect("event");
            spool
                .push_disposition(RawDisposition {
                    raw_line: line,
                    raw_sha256: raw.sha256,
                    byte_start: line,
                    byte_length: 1,
                    envelope_family: "test".to_string(),
                    disposition: "event".to_string(),
                    reason: None,
                    source_turn_id: None,
                    turn_boundary_raw_line: None,
                    turn_boundary_raw_sha256: None,
                    emitted_event_count: 1,
                })
                .expect("disposition");
        }
        let spool = spool.finish().expect("finish");
        assert!(spool.flush_count() >= 2);
        assert!(spool.max_buffered_rows() <= MAX_BUFFERED_ROWS);
        assert!(spool.max_buffered_bytes() <= MAX_BUFFERED_BYTES);
        let mut lines = Vec::new();
        spool
            .visit_events(&mut |raw, _| {
                lines.push(raw.line);
                Ok(())
            })
            .expect("visit");
        assert_eq!(lines, (1..=1_100).collect::<Vec<_>>());
        let path = spool.path.clone();
        spool.discard().expect("discard");
        assert!(!path.exists());
    }

    #[test]
    #[cfg(unix)]
    fn spool_directory_symlink_is_rejected_before_any_row_is_written() {
        use std::os::unix::fs::symlink;

        let tmp = tempdir().expect("tempdir");
        let outside = tempdir().expect("outside tempdir");
        symlink(outside.path(), tmp.path().join("spools")).expect("create spool symlink");
        let error = GenerationSpool::create(tmp.path(), "generation_test")
            .err()
            .expect("symlinked spool directory must fail closed");
        assert!(error.to_string().contains("non-symlink"));
        assert!(
            std::fs::read_dir(outside.path())
                .expect("outside directory")
                .next()
                .is_none(),
            "no private spool bytes may cross the mirror boundary"
        );
    }
}
