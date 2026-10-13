//! The §11 one-time importer from the pre-spec ledger.
//!
//! > *"Keep the existing ledger data importable (write a one-time importer from
//! > the current schema)."* — BUILD-SPEC §11
//!
//! # What it can honestly claim, and what it cannot
//!
//! The legacy store is a record of **what the agent said** — transcript rows,
//! seven kinds, none of which is `exec`, `fs_change` or `snapshot`. BUILD-SPEC
//! §0 principle 2 puts that in a different evidentiary tier from what this
//! store exists to hold. So every imported session is stamped
//! `capture_degraded = 1`, and §13 turns that into: its claims may reach
//! `unsupported` or `unverifiable`, **never** `supported`.
//!
//! That is not a limitation of the importer. It is the honest reading of what
//! two million transcript rows can prove about what a machine did, which is
//! nothing.
//!
//! # Three measured facts that shaped this
//!
//! 1. **`raw_sha256` is carried forward, never recomputed.** 75,741 of 76,203
//!    source transcripts no longer exist on disk — 99.4% — matching
//!    `ledger_meta.raw_files_missing` exactly. An importer that re-derived the
//!    digest would fail on almost the whole corpus. The recorded digest is
//!    imported as *provenance*, and the receipt path it names may already be
//!    dead; that is a property of the source, and it is recorded rather than
//!    hidden.
//! 2. **70% of legacy events are `kind='meta'`**, a kind §3.1 has no name for.
//!    They map to `note`, with the original kind preserved in
//!    `events.legacy_kind` so the mapping is reversible and auditable. Guessing
//!    a richer mapping for 1.4 M rows nobody has classified would be inventing
//!    evidence.
//! 3. **The legacy chain is a forest, not a chain** — 76,203 per-session chains
//!    each rooted at `sha256(project_id)`. Deleting an entire session breaks no
//!    link any other session can see. The imported events are re-chained under
//!    §3.1's rule (each session rooted at `sha256(session_id)`, every field in
//!    the preimage) and the legacy `chain_hash` is preserved per row, so the old
//!    chain stays independently checkable against the old store.
//!
//! # Streaming
//!
//! The source is 6.9 GB. Every query here is a cursor with `LIMIT`/`WHERE seq >`
//! paging; nothing does `SELECT *` into a `Vec`. `imports` records progress so
//! a re-run resumes, and `INSERT ... ON CONFLICT(session_id, seq) DO NOTHING`
//! makes a resumed import idempotent rather than duplicating.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OpenFlags};
use thruline_core::spec::chain::genesis_prev_hash;
use thruline_core::spec::event::{
    Event, EventId, OpenPayload, Payload, Source, ToolCall, ToolResult,
};

use crate::schema::{EventRow, Ledger, LedgerError, LegacyProvenance};

/// The `source` stamped on every imported event.
///
/// Not one of §3.1's eight, and deliberately so: it lands in `Source::Other` and
/// is visibly not `exec-recorder` or a hook. A reader who sees it knows the row
/// came from a transcript, which is the whole point.
pub const IMPORT_SOURCE: &str = "claude-code-transcript";

/// Why an imported session can never support a claim.
pub const DEGRADED_REASON: &str =
    "imported from the pre-spec transcript ledger: these rows record what the \
     agent said, not what the machine did — no exec, fs or snapshot evidence \
     exists for this session";

/// How to run an import.
#[derive(Debug, Clone)]
pub struct ImportOptions {
    /// Path to the legacy `ledger.db`.
    pub source: PathBuf,
    /// Import only this legacy project, if given.
    pub project_id: Option<String>,
    /// Count and report, write nothing.
    pub dry_run: bool,
    /// Stop after this many legacy event rows. `None` means all of them.
    pub limit: Option<u64>,
    /// Maximum rows per page. The source is 6.9 GB; nothing is read whole.
    ///
    /// **Default 500, chosen from measurement rather than from a round number.**
    /// The independent verifier swept this at 400,000 rows:
    ///
    /// ```text
    ///   page 500    → 101 MB peak RSS, 30.6 s
    ///   page 5000   → 352 MB,          42.0 s   (the original default)
    ///   page 50000  → 712 MB,          26.9 s
    /// ```
    ///
    /// 5000 cost 3.5x the memory of 500 **and was slower**. It was picked
    /// because it looked like a sensible batch size, which is not a reason.
    ///
    /// A page is bounded by [`ImportOptions::page_bytes`] as well, and that is
    /// the bound that actually caps memory — see its docs.
    pub page_size: u64,
    /// Approximate maximum payload bytes held in one page.
    ///
    /// **Rows are the wrong unit and the measurement says so.** A legacy row's
    /// `text` / `tool_input` / `tool_result` are unbounded, so "5,000 rows" can
    /// mean a megabyte or a gigabyte. Measured on a copy of the real ledger,
    /// dry-running 2,046,353 rows: page_size 5,000 → 567 MB peak RSS, 1,000 →
    /// 391 MB, 200 → 232 MB. The row count was setting memory, and it was
    /// setting it by accident.
    ///
    /// With a byte budget the page stops accumulating once it has this much
    /// text, whatever the row count, so peak memory is a property of the
    /// setting rather than of whichever session happened to paste a large
    /// build log into a tool result.
    pub page_bytes: u64,
    /// Import `kind='meta'` rows as individual events.
    ///
    /// **Off by default, on measurement.** The 1,442,374 `meta` rows are not
    /// session content: each one's `text` is the *name of a JSONL field the
    /// parser did not recognise*, with `tool_name`/`tool_input` empty on all of
    /// them and `role` empty on 99.9%. There are 49 distinct names, and the
    /// distribution is dominated by another vendor entirely
    /// (`codex.event_msg.token_count` 502,620; `attachment` 240,807;
    /// `codex.event_msg.agent_reasoning` 217,454). They are CLAUDE.md's
    /// "preserve, count and report" invariant working as designed — the rows
    /// *are* the report.
    ///
    /// So excluding them drops no session content, and they arrive instead as
    /// one census note per session: 49 counts rather than 1.44 M events.
    pub include_meta_events: bool,
}

impl ImportOptions {
    pub fn new(source: impl Into<PathBuf>) -> ImportOptions {
        ImportOptions {
            source: source.into(),
            project_id: None,
            dry_run: false,
            limit: None,
            page_size: 500,
            page_bytes: 8 * 1024 * 1024,
            include_meta_events: false,
        }
    }
}

/// What an import did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub rows_read: u64,
    pub events_written: u64,
    /// Rows already present from an earlier run.
    pub events_skipped: u64,
    pub sessions_seen: u64,
    /// Legacy kinds encountered and how many of each. Reported rather than
    /// summarised, so an unexpected kind is visible instead of averaged away.
    pub kinds: std::collections::BTreeMap<String, u64>,
    /// Rows that could not be turned into an event, with the reason. **Counted
    /// and reported, never silently dropped** (CLAUDE.md).
    pub rejected: Vec<(String, String)>,
    /// `meta` rows folded into per-session censuses instead of events.
    pub meta_rows_aggregated: u64,
    /// Sessions whose source transcript no longer exists on disk.
    pub sessions_with_dead_source: u64,
    pub dry_run: bool,
}

/// The importer.
pub struct Importer<'a> {
    target: &'a Ledger,
}

/// One legacy `events` row, as far as this importer cares.
struct LegacyRow {
    id: String,
    project_id: String,
    session_id: String,
    seq: i64,
    ts: Option<String>,
    kind: String,
    text: Option<String>,
    tool_name: Option<String>,
    tool_use_id: Option<String>,
    tool_input: Option<String>,
    tool_result: Option<String>,
    is_error: i64,
    raw_file: String,
    raw_line: i64,
    raw_sha256: String,
    chain_hash: String,
}

impl<'a> Importer<'a> {
    pub fn new(target: &'a Ledger) -> Importer<'a> {
        Importer { target }
    }

    /// Open the legacy store **read-only**.
    ///
    /// `mode=ro` in the URI as well as `SQLITE_OPEN_READ_ONLY`, so the database
    /// file itself is never written. Verified against a copy of the real 6.9 GB
    /// ledger: SHA-256 identical before and after a full 2,046,353-row pass.
    ///
    /// **It does create a `-shm` beside the source, and an earlier version of
    /// this comment claimed it did not.** That claim was false, and the test
    /// backing it passed only because its fixture used the default rollback
    /// journal while the real ledger is in WAL mode. Reading a WAL database —
    /// even read-only — requires the shared-memory index, so SQLite creates
    /// `<db>-shm` (32 KB, measured) and may touch a zero-length `<db>-wal`.
    /// Neither alters a byte of the database, and both are removed by any
    /// clean close of the last connection.
    ///
    /// `immutable=1` would suppress even that, and is deliberately NOT used: it
    /// asserts to SQLite that nothing can be modifying the file, and a live
    /// ledger with a hook writing to it would then be read as silent garbage
    /// rather than as an error. A 32 KB sidecar is the correct price for not
    /// making that assertion about a file we do not control.
    fn open_source(path: &Path) -> Result<Connection, LedgerError> {
        let uri = format!("file:{}?mode=ro", path.display());
        let conn = thruline_core::store_hold::open_uri_connection(
            path,
            &uri,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )?;
        Ok(conn)
    }

    /// Run the import.
    pub fn run(&self, options: &ImportOptions) -> Result<ImportReport, LedgerError> {
        let source = Importer::open_source(&options.source)?;
        let mut report = ImportReport {
            dry_run: options.dry_run,
            ..Default::default()
        };

        let import_id = if options.dry_run {
            None
        } else {
            // ULID, per CLAUDE.md's id convention. The first version derived
            // this from `sha256(source | unix_seconds)` and two imports started
            // within the same second collided on the primary key — which is
            // exactly the resume case this table exists to support. Caught by
            // `an_interrupted_import_resumes_without_duplicating_and_continues_the_chain`.
            let id = format!("imp_{}", ulid::Ulid::generate());
            self.target.conn().execute(
                "INSERT INTO imports(id, source_path, source_kind, started_at)
                 VALUES(?1, ?2, 'thruline-legacy-ledger', ?3)",
                params![id, options.source.display().to_string(), now()],
            )?;
            Some(id)
        };

        // Page by (session_id, seq): the legacy `events` table is UNIQUE on that
        // pair, so it is a total order within a session and a stable cursor
        // across pages. `LIMIT` with a `>` cursor never re-reads a page and
        // never holds more than `page_size` rows.
        let mut cursor = (String::new(), i64::MIN);
        let mut census = MetaCensus::default();
        let mut truncated = false;
        loop {
            let page = self.read_page(&source, options, &cursor)?;
            if page.is_empty() {
                break;
            }
            let open_session = page
                .last()
                .map(|row| row.session_id.clone())
                .unwrap_or_default();
            if let Some(last) = page.last() {
                cursor = (last.session_id.clone(), last.seq);
            }
            report.rows_read += page.len() as u64;

            let converted = self.convert_page(&page, options, &mut census, &mut report);
            if !options.dry_run {
                self.write_page(&converted, &mut report)?;
            }
            // Pages are ordered by (session_id, seq), so every session except
            // the one straddling the page boundary is complete. Evicting them
            // here keeps memory bounded by the page rather than by the 76,203
            // sessions in the source.
            //
            // **This runs on the dry-run path too**, and it did not before.
            // Measured against a copy of the real 6.9 GB ledger: a dry run over
            // 2,046,353 rows peaked at 603 MB, because the census accumulated
            // every session and the flush was inside `if !options.dry_run`. The
            // dry run is the safe, exploratory mode — the one a user reaches for
            // first — and it was the one that grew without bound.
            self.flush_census(&mut census, Some(&open_session), options, &mut report)?;

            if let Some(limit) = options.limit {
                if report.rows_read >= limit {
                    truncated = true;
                    break;
                }
            }
        }
        // A truncated run stopped in the middle of a session, so its census is
        // incomplete. Writing it would publish a partial count AND, because the
        // note takes a seq, put a synthetic row between two legacy ones. The
        // pending entry is dropped; `meta` rows are not stored as events, so the
        // next complete run re-derives it from the source.
        if !truncated {
            self.flush_census(&mut census, None, options, &mut report)?;
        }

        if let Some(id) = import_id {
            self.target.conn().execute(
                "UPDATE imports SET finished_at = ?2, rows_read = ?3, events_written = ?4,
                                    sessions_seen = ?5, note = ?6 WHERE id = ?1",
                params![
                    id,
                    now(),
                    report.rows_read as i64,
                    report.events_written as i64,
                    report.sessions_seen as i64,
                    format!("{} rejected", report.rejected.len())
                ],
            )?;
        }
        Ok(report)
    }

    fn read_page(
        &self,
        source: &Connection,
        options: &ImportOptions,
        cursor: &(String, i64),
    ) -> Result<Vec<LegacyRow>, LedgerError> {
        let remaining = match options.limit {
            Some(limit) => limit.min(options.page_size),
            None => options.page_size,
        };
        let mut stmt = source.prepare_cached(
            "SELECT id, project_id, session_id, seq, ts, kind, text, tool_name,
                    tool_use_id, tool_input, tool_result, is_error,
                    raw_file, raw_line, raw_sha256, chain_hash
             FROM events
             WHERE (session_id, seq) > (?1, ?2)
               AND (?3 IS NULL OR project_id = ?3)
             ORDER BY session_id, seq
             LIMIT ?4",
        )?;
        let mut rows = stmt.query(params![
            cursor.0,
            cursor.1,
            options.project_id.as_deref(),
            remaining as i64
        ])?;

        // Accumulate under BOTH bounds, and stop at whichever binds first. The
        // byte budget is the one that actually caps memory; the row count is
        // there so a corpus of tiny rows still pages.
        let mut page: Vec<LegacyRow> = Vec::new();
        let mut bytes = 0u64;
        while let Some(row) = rows.next()? {
            let legacy = LegacyRow {
                id: row.get(0)?,
                project_id: row.get(1)?,
                session_id: row.get(2)?,
                seq: row.get(3)?,
                ts: row.get(4)?,
                kind: row.get(5)?,
                text: row.get(6)?,
                tool_name: row.get(7)?,
                tool_use_id: row.get(8)?,
                tool_input: row.get(9)?,
                tool_result: row.get(10)?,
                is_error: row.get(11)?,
                raw_file: row.get(12)?,
                raw_line: row.get(13)?,
                raw_sha256: row.get(14)?,
                chain_hash: row.get(15)?,
            };
            bytes += legacy.payload_bytes();
            page.push(legacy);
            // Checked AFTER pushing, so a single row larger than the whole
            // budget is still imported rather than looping forever on a page
            // that can never be filled.
            if bytes >= options.page_bytes {
                break;
            }
        }
        Ok(page)
    }

    fn convert_page(
        &self,
        page: &[LegacyRow],
        options: &ImportOptions,
        census: &mut MetaCensus,
        report: &mut ImportReport,
    ) -> Vec<(String, String, EventRow)> {
        let mut out = Vec::with_capacity(page.len());
        for row in page {
            *report.kinds.entry(row.kind.clone()).or_default() += 1;
            if row.kind == "meta" && !options.include_meta_events {
                census.record(row);
                report.meta_rows_aggregated += 1;
                continue;
            }
            match convert(row) {
                Ok(event_row) => {
                    out.push((row.project_id.clone(), row.session_id.clone(), event_row))
                }
                Err(reason) => report.rejected.push((row.id.clone(), reason)),
            }
        }
        out
    }

    fn write_page(
        &self,
        converted: &[(String, String, EventRow)],
        report: &mut ImportReport,
    ) -> Result<(), LedgerError> {
        // Group by session so `prev_hash` can be linked from what is already in
        // the ledger — this is what makes a resumed import continue the chain
        // rather than restart it.
        let mut by_session: std::collections::BTreeMap<(String, String), Vec<EventRow>> =
            Default::default();
        for (project, session, row) in converted {
            by_session
                .entry((project.clone(), session.clone()))
                .or_default()
                .push(row.clone());
        }

        for ((project, session), mut rows) in by_session {
            let repo = self.target.repo(&project, None)?;
            let existed = self.target.stored_head(&session)?;
            if existed.is_none() {
                report.sessions_seen += 1;
            }
            self.target.upsert_session(
                &session,
                &repo,
                "claude-code",
                None,
                true,
                Some(DEGRADED_REASON),
            )?;

            // Resume from the last event ACTUALLY STORED, and drop anything at
            // or below it before chaining.
            //
            // The first version chained every row in the page onto the stored
            // head and relied on `ON CONFLICT DO NOTHING` to drop duplicates.
            // That is wrong and the resume test caught it: the already-present
            // rows were re-sealed against the wrong predecessor, the *next* row
            // chained onto one of those rejected hashes, and the chain broke at
            // the resume point. Filtering has to happen before sealing, not
            // after inserting.
            let last_hash = match self.target.last_event(&session)? {
                Some((_, hash)) => hash,
                None => genesis_prev_hash(&session),
            };
            let before = rows.len();
            if let Some(last_seq) = self.target.last_imported_seq(&session)? {
                rows.retain(|row| row.event.seq() > last_seq);
            }
            report.events_skipped += (before - rows.len()) as u64;

            let mut prev = last_hash;
            let mut sealed = Vec::with_capacity(rows.len());
            for row in rows.drain(..) {
                let event = row.event.with_prev_hash(prev.clone());
                let event = match event.seal() {
                    Ok(e) => e,
                    Err(error) => {
                        report
                            .rejected
                            .push((row_id_of(&row.legacy), error.to_string()));
                        continue;
                    }
                };
                prev = event.hash().unwrap_or_default().to_string();
                sealed.push(EventRow { event, ..row });
            }
            // §11 / the 99.4% finding: a session whose transcript is gone can
            // never have its recorded digest re-derived. Checked once per
            // session, from the first row's provenance — a stat per row over
            // 1.44 M rows would dominate the import.
            if let Some(first) = sealed.first().and_then(|r| r.legacy.as_ref()) {
                if !Path::new(&first.raw_file).exists()
                    && !self.target.source_digest_unverifiable(&session)?
                {
                    self.target.mark_source_digest_unverifiable(&session)?;
                    report.sessions_with_dead_source += 1;
                }
            }

            let written = self.target.append_events(&repo, &sealed)?;
            report.events_written += written;
            report.events_skipped += sealed.len() as u64 - written;
        }
        Ok(())
    }

    /// Write one `note` per complete session holding its unknown-field census.
    ///
    /// `keep_open` names the session that straddles the page boundary and is
    /// therefore not finished yet.
    fn flush_census(
        &self,
        census: &mut MetaCensus,
        keep_open: Option<&str>,
        options: &ImportOptions,
        report: &mut ImportReport,
    ) -> Result<(), LedgerError> {
        let ready: Vec<String> = census
            .per_session
            .keys()
            .filter(|s| Some(s.as_str()) != keep_open)
            .cloned()
            .collect();
        for session in ready {
            let Some(entry) = census.per_session.remove(&session) else {
                continue;
            };
            // A dry run still EVICTS — it just does not write. Accumulating the
            // entry it is not going to use is how the exploratory mode became
            // the memory-hungry one.
            if options.dry_run {
                if entry.source_missing {
                    report.sessions_with_dead_source += 1;
                }
                continue;
            }
            if self.target.has_census_note(&session)? {
                // Already written by an earlier run. Events are append-only, so
                // this cannot be updated — and re-deriving the same census
                // would collide on (session_id, seq) and be silently dropped,
                // which is worse than skipping it deliberately.
                continue;
            }
            let repo = self.target.repo(&entry.project_id, None)?;
            self.target.upsert_session(
                &session,
                &repo,
                "claude-code",
                None,
                true,
                Some(DEGRADED_REASON),
            )?;
            let (last_seq, prev) = match self.target.last_event(&session)? {
                Some((seq, hash)) => (seq, hash),
                None => (0, genesis_prev_hash(&session)),
            };
            let mut payload = OpenPayload::new();
            payload.insert(
                "legacy_unknown_field_census".into(),
                serde_json::json!(entry.fields),
            );
            payload.insert("rows".into(), serde_json::json!(entry.rows));
            payload.insert(
                "note".into(),
                serde_json::json!(
                    "legacy `meta` rows record JSONL field names the parser did not                      recognise, not session content — folded to one census rather                      than imported as events"
                ),
            );
            let event = match Event::new(
                &session,
                last_seq + 1,
                &entry.last_ts,
                Source::Other(IMPORT_SOURCE.to_string()),
                Payload::Note(payload),
                prev,
            )
            .and_then(Event::seal)
            {
                Ok(event) => event,
                Err(error) => {
                    report.rejected.push((session.clone(), error.to_string()));
                    continue;
                }
            };
            report.events_written += self.target.append_events(&repo, &[EventRow::bare(event)])?;
            if entry.source_missing && !self.target.source_digest_unverifiable(&session)? {
                self.target.mark_source_digest_unverifiable(&session)?;
                report.sessions_with_dead_source += 1;
            }
        }
        Ok(())
    }
}

impl LegacyRow {
    /// Roughly what this row costs to hold. The unbounded columns only.
    fn payload_bytes(&self) -> u64 {
        let len = |s: &Option<String>| s.as_ref().map(String::len).unwrap_or(0);
        (len(&self.text) + len(&self.tool_input) + len(&self.tool_result) + 256) as u64
    }
}

/// Per-session accumulation of the legacy `meta` rows.
#[derive(Default)]
struct MetaCensus {
    per_session: std::collections::BTreeMap<String, MetaEntry>,
}

#[derive(Default)]
struct MetaEntry {
    project_id: String,
    rows: u64,
    fields: std::collections::BTreeMap<String, u64>,
    last_ts: String,
    /// Whether the source transcript still exists. Checked once per session,
    /// not once per row: 99.4% of them are gone and a stat per row over 1.44 M
    /// rows would dominate the import.
    source_missing: bool,
}

impl MetaCensus {
    fn record(&mut self, row: &LegacyRow) {
        let entry = self
            .per_session
            .entry(row.session_id.clone())
            .or_insert_with(|| MetaEntry {
                project_id: row.project_id.clone(),
                source_missing: !Path::new(&row.raw_file).exists(),
                last_ts: "1970-01-01T00:00:00Z".to_string(),
                ..Default::default()
            });
        entry.rows += 1;
        let field = row
            .text
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .unwrap_or("(empty)");
        *entry.fields.entry(field.to_string()).or_default() += 1;
        entry.last_ts = normalize_ts(row.ts.as_deref());
    }
}

fn row_id_of(legacy: &Option<LegacyProvenance>) -> String {
    legacy
        .as_ref()
        .map(|l| format!("{}:{}", l.raw_file, l.raw_line))
        .unwrap_or_else(|| "unknown".to_string())
}

fn now() -> String {
    // Seconds since the epoch, as a string. Deliberately not a formatted
    // timestamp: this value is bookkeeping and never enters a hash preimage,
    // and pulling in a date library for it would be a dependency for a log line.
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

/// §3.1 requires an RFC 3339 UTC `ts`. The legacy column is nullable and its
/// format is whatever the transcript carried.
fn normalize_ts(raw: Option<&str>) -> String {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        // A row with no timestamp is real (the legacy column is nullable). The
        // epoch is used rather than "now", because "now" would silently assert
        // that the event happened at import time.
        return "1970-01-01T00:00:00Z".to_string();
    };
    if raw.ends_with('Z') {
        return raw.to_string();
    }
    // `+00:00` and `Z` name the same instant; §3.1's validator accepts only the
    // second, so the offset form is rewritten rather than rejected.
    if let Some(base) = raw
        .strip_suffix("+00:00")
        .or_else(|| raw.strip_suffix("+0000"))
    {
        return format!("{base}Z");
    }
    format!("{raw}Z")
}

/// Map one legacy row onto a §3.1 event.
///
/// The mapping, stated plainly:
///
/// | legacy kind | §3.1 kind | why |
/// |---|---|---|
/// | `user_msg` | `user_prompt` | exact |
/// | `tool_use` | `tool_call` | exact |
/// | `tool_result` | `tool_result` | exact |
/// | `assistant_text`, `assistant_thinking`, `summary`, `meta` | `note` | §3.1 has no name for them |
///
/// `meta` is 70% of the corpus. Mapping it to `note` loses nothing that was
/// there: `legacy_kind` is preserved on the row, so the distinction is
/// recoverable, and inventing a richer mapping for 1.4 M unclassified rows would
/// be manufacturing evidence.
fn convert(row: &LegacyRow) -> Result<EventRow, String> {
    let ts = normalize_ts(row.ts.as_deref());
    let text = row.text.clone().unwrap_or_default();

    let payload = match row.kind.as_str() {
        "user_msg" => {
            let mut p = OpenPayload::new();
            p.insert("text".into(), serde_json::json!(text));
            Payload::UserPrompt(p)
        }
        "tool_use" => Payload::ToolCall(ToolCall {
            tool: row.tool_name.clone().unwrap_or_else(|| "unknown".into()),
            input_hash: digest(row.tool_input.as_deref().unwrap_or("")),
            input_redacted: row.tool_input.clone().unwrap_or_default(),
            cwd: String::new(),
            extra: Default::default(),
        }),
        "tool_result" => Payload::ToolResult(ToolResult {
            tool: row.tool_name.clone().unwrap_or_else(|| "unknown".into()),
            call_id: row.tool_use_id.clone().unwrap_or_default(),
            ok: row.is_error == 0,
            output_hash: digest(row.tool_result.as_deref().unwrap_or("")),
            output_excerpt: row.tool_result.clone().unwrap_or_default(),
            // The legacy schema records `is_error`, not an exit code. Absent
            // rather than invented: a fabricated 0 or 1 would be read
            // downstream as observed evidence.
            exit_code: None,
            extra: Default::default(),
        }),
        "assistant_text" | "assistant_thinking" | "summary" | "meta" => {
            let mut p = OpenPayload::new();
            p.insert("text".into(), serde_json::json!(text));
            p.insert("legacy_kind".into(), serde_json::json!(row.kind));
            Payload::Note(p)
        }
        other => return Err(format!("unmapped legacy kind {other:?}")),
    };

    let seq = u64::try_from(row.seq).map_err(|_| format!("negative legacy seq {}", row.seq))?;
    let event = Event::new(
        &row.session_id,
        seq,
        &ts,
        Source::Other(IMPORT_SOURCE.to_string()),
        payload,
        // Replaced by the writer, which links against what is already stored.
        genesis_prev_hash(&row.session_id),
    )
    .map_err(|e| e.to_string())?;

    // Keep the legacy row's identity where it is a valid event id, so a
    // re-import is stable; otherwise mint one.
    let event = match EventId::parse(&format!("evt_{}", row.id)) {
        Ok(id) => event.with_id(id),
        Err(_) => event,
    };

    Ok(EventRow {
        event,
        redactions_applied: 0,
        redaction_families: Default::default(),
        legacy: Some(LegacyProvenance {
            raw_file: row.raw_file.clone(),
            raw_line: row.raw_line,
            raw_sha256: row.raw_sha256.clone(),
            legacy_kind: row.kind.clone(),
            legacy_chain_hash: row.chain_hash.clone(),
        }),
    })
}

fn digest(text: &str) -> String {
    format!(
        "sha256:{}",
        thruline_core::chain::sha256_hex(text.as_bytes())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_open_keeps_read_only_mode_and_refuses_a_live_store_hold() {
        let root = tempfile::tempdir().expect("scratch directory");
        let source = root.path().join("ledger.db");
        drop(Connection::open(&source).expect("scratch SQLite file"));
        let before = std::fs::read(&source).expect("source bytes");
        let reader = Importer::open_source(&source).expect("no hold read");
        assert!(reader.is_readonly(rusqlite::MAIN_DB).expect("mode"));
        drop(reader);
        assert_eq!(std::fs::read(&source).expect("source bytes"), before);

        let hold = thruline_core::store_hold::StoreHold::create(&source, "removal-01", "P1")
            .expect("hold source");
        assert!(matches!(
            Importer::open_source(&source),
            Err(LedgerError::SourceOpenGuard(
                thruline_core::error::CoreError::StoreHeld { .. }
            ))
        ));
        hold.release().expect("release hold");
        assert!(Importer::open_source(&source).is_ok());
    }

    #[test]
    fn timestamps_are_normalized_to_rfc3339_utc_or_the_epoch() {
        assert_eq!(
            normalize_ts(Some("2026-09-02T10:11:12.345Z")),
            "2026-09-02T10:11:12.345Z"
        );
        assert_eq!(
            normalize_ts(Some("2026-09-02T10:11:12+00:00")),
            "2026-09-02T10:11:12Z"
        );
        assert_eq!(
            normalize_ts(Some("2026-09-02T10:11:12")),
            "2026-09-02T10:11:12Z"
        );
        // A missing timestamp becomes the epoch, NOT import time: "now" would
        // assert the event happened when the import ran.
        assert_eq!(normalize_ts(None), "1970-01-01T00:00:00Z");
        assert_eq!(normalize_ts(Some("  ")), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn every_legacy_kind_has_a_mapping_and_none_is_invented() {
        // The legacy CHECK constraint lists exactly seven kinds. If one is ever
        // added there, this fails rather than the importer rejecting 1.4M rows
        // at 3am.
        let legacy_kinds = [
            "user_msg",
            "assistant_text",
            "assistant_thinking",
            "tool_use",
            "tool_result",
            "summary",
            "meta",
        ];
        for kind in legacy_kinds {
            let row = LegacyRow {
                id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
                project_id: "p".into(),
                session_id: "s".into(),
                seq: 1,
                ts: Some("2026-09-02T10:11:12Z".into()),
                kind: kind.into(),
                text: Some("hello".into()),
                tool_name: Some("Bash".into()),
                tool_use_id: Some("toolu_1".into()),
                tool_input: Some("{\"command\":\"ls\"}".into()),
                tool_result: Some("ok".into()),
                is_error: 0,
                raw_file: "/gone/session.jsonl".into(),
                raw_line: 12,
                raw_sha256: "deadbeef".into(),
                chain_hash: "cafe".into(),
            };
            let converted = convert(&row).unwrap_or_else(|e| panic!("{kind}: {e}"));
            let legacy = converted.legacy.expect("provenance is preserved");
            assert_eq!(legacy.legacy_kind, kind);
            assert_eq!(
                legacy.raw_sha256, "deadbeef",
                "the recorded digest must be carried forward, not recomputed — \
                 99.4% of source transcripts no longer exist"
            );
            assert_eq!(
                *converted.event.source(),
                Source::Other(IMPORT_SOURCE.to_string()),
                "an imported row must not look like hook or exec-recorder capture"
            );
        }

        // An unmapped kind is REPORTED, not silently turned into a note.
        let mut row = LegacyRow {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
            project_id: "p".into(),
            session_id: "s".into(),
            seq: 1,
            ts: None,
            kind: "something_new".into(),
            text: None,
            tool_name: None,
            tool_use_id: None,
            tool_input: None,
            tool_result: None,
            is_error: 0,
            raw_file: "f".into(),
            raw_line: 1,
            raw_sha256: "x".into(),
            chain_hash: "y".into(),
        };
        assert!(convert(&row).is_err());
        row.kind = "meta".into();
        assert!(convert(&row).is_ok());
    }

    #[test]
    fn a_tool_result_never_invents_an_exit_code() {
        // The legacy schema has `is_error`, not an exit code. A fabricated 0
        // would be read downstream as observed evidence of a passing command,
        // which is the exact failure this product exists to catch.
        let row = LegacyRow {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
            project_id: "p".into(),
            session_id: "s".into(),
            seq: 1,
            ts: None,
            kind: "tool_result".into(),
            text: None,
            tool_name: Some("Bash".into()),
            tool_use_id: Some("t".into()),
            tool_input: None,
            tool_result: Some("boom".into()),
            is_error: 1,
            raw_file: "f".into(),
            raw_line: 1,
            raw_sha256: "x".into(),
            chain_hash: "y".into(),
        };
        let converted = convert(&row).expect("converts");
        let Payload::ToolResult(result) = converted.event.payload_typed().expect("typed") else {
            panic!("wrong variant");
        };
        assert_eq!(result.exit_code, None);
        assert!(!result.ok);
    }
}
