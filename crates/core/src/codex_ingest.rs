//! Codex discovery plus two-pass sealed-generation preparation.
//!
//! Parsing here is source-admission work only. Returned events gain authority
//! only if the caller commits them through the ledger's generation transaction.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::adapter::{
    display_name_from_root, lexical_absolute, stable_owner_key, stable_source_session_id,
    LogicalRoute, Probe, SessionPlan, TranscriptAdapter,
};
use crate::chain::sha256_hex;
use crate::codex::{
    normalize_codex_line, source_turn_identity, CodexEmissionRole, CodexLineContext,
};
use crate::error::{CoreError, Result};
use crate::generation_spool::GenerationSpool;
use crate::ledger::RawDisposition;
use crate::mirror::{MirrorStore, SealedMirror};
use crate::model::{RawLineRef, Source};

pub const MAX_CODEX_LINE_BYTES: usize = 8 * 1024 * 1024;

pub struct AcceptedCodexPrefix<'a> {
    pub payload_bytes: u64,
    pub raw_lines: u64,
    pub reject_malformed_suffix: bool,
    pub available_message_slots: &'a [(String, String)],
}

#[derive(Debug, Clone)]
pub struct CodexAdapter {
    owner_key_override: Option<String>,
}

impl CodexAdapter {
    pub fn new(owner_key_override: Option<String>) -> Self {
        Self { owner_key_override }
    }

    pub fn owner_key_for(&self, plan: &SessionPlan) -> Result<String> {
        match &self.owner_key_override {
            Some(key) if !key.trim().is_empty() => Ok(key.clone()),
            Some(_) => Err(CoreError::Config(
                "Codex owner identity override cannot be empty".to_string(),
            )),
            None => stable_owner_key(&plan.owner_root),
        }
    }

    pub fn inspect_sealed(
        &self,
        plan: &SessionPlan,
        sealed_payload: &Path,
    ) -> Result<LogicalRoute> {
        let owner_key = self.owner_key_for(plan)?;
        inspect_codex_file(sealed_payload, owner_key)
    }

    /// Resolve only the first authoritative physical owner for capture
    /// placement. Full routing and ambiguity checks run against sealed bytes.
    pub fn inspect_capture_route(&self, plan: &SessionPlan) -> Result<LogicalRoute> {
        let owner_key = self.owner_key_for(plan)?;
        inspect_codex_first_meta(&plan.path, owner_key)
    }

    pub fn count_sealed_lines(&self, mirror: &MirrorStore, sealed: &SealedMirror) -> Result<u64> {
        mirror.verify(sealed)?;
        let payload = mirror.resolve_payload(sealed)?;
        let mut reader = BoundedJsonl::open(&payload, 0, 0)?;
        let mut count = 0u64;
        while reader.next_line()?.is_some() {
            count += 1;
        }
        Ok(count)
    }

    pub fn prepare_generation(
        &self,
        mirror: &MirrorStore,
        sealed: &SealedMirror,
        route: &LogicalRoute,
        accepted: AcceptedCodexPrefix<'_>,
    ) -> Result<PreparedCodexGeneration> {
        self.prepare_generation_inner(mirror, sealed, route, accepted, None, None)
    }

    /// [`CodexAdapter::prepare_generation`] that gives up as soon as `stop`
    /// answers `true` (asked every 1,024 raw lines). C27: the load passes a
    /// check that the live source has moved since its capture — a candidate
    /// whose source moved fails the pre-commit check anyway, so mapping the
    /// rest of a 40 MB rollout is work that can only be thrown away.
    pub fn prepare_generation_watching(
        &self,
        mirror: &MirrorStore,
        sealed: &SealedMirror,
        route: &LogicalRoute,
        accepted: AcceptedCodexPrefix<'_>,
        stop: &dyn Fn() -> bool,
    ) -> Result<PreparedCodexGeneration> {
        self.prepare_generation_inner(mirror, sealed, route, accepted, None, Some(stop))
    }

    /// Deterministic admission fault used only by atomicity integration tests.
    /// No CLI, environment, configuration, or persisted state can select it.
    #[doc(hidden)]
    pub fn prepare_generation_with_line_failpoint(
        &self,
        mirror: &MirrorStore,
        sealed: &SealedMirror,
        route: &LogicalRoute,
        accepted: AcceptedCodexPrefix<'_>,
        fail_at_raw_line: u64,
    ) -> Result<PreparedCodexGeneration> {
        self.prepare_generation_inner(
            mirror,
            sealed,
            route,
            accepted,
            Some(fail_at_raw_line),
            None,
        )
    }

    fn prepare_generation_inner(
        &self,
        mirror: &MirrorStore,
        sealed: &SealedMirror,
        route: &LogicalRoute,
        accepted: AcceptedCodexPrefix<'_>,
        fail_at_raw_line: Option<u64>,
        stop: Option<&dyn Fn() -> bool>,
    ) -> Result<PreparedCodexGeneration> {
        let stopped = |line: u64| line.is_multiple_of(1024) && stop.is_some_and(|stop| stop());
        let accepted_prefix_bytes = accepted.payload_bytes;
        let accepted_prefix_lines = accepted.raw_lines;
        let reject_malformed_suffix = accepted.reject_malformed_suffix;
        mirror.verify(sealed)?;
        let payload_path = mirror.resolve_payload(sealed)?;
        let mut spool = GenerationSpool::create(mirror.root(), &sealed.generation_key)?;
        if accepted_prefix_bytes > 0
            && !prefix_ends_at_line_boundary(&payload_path, accepted_prefix_bytes)?
        {
            return Err(CoreError::Integrity(
                "accepted prefix does not end at a JSONL line boundary".to_string(),
            ));
        }

        let index_path = mirror
            .root()
            .join("indexes")
            .join(format!("{}.sqlite", sealed.generation_key));
        if let Some(parent) = index_path.parent() {
            create_private_directory(parent)?;
        }
        remove_private_sqlite_files(&index_path)?;
        create_private_file(&index_path)?;
        let index = Connection::open(&index_path)?;
        set_private_mode(&index_path, 0o600)?;
        index.execute_batch(
            "PRAGMA journal_mode=DELETE;
             DROP TABLE IF EXISTS mirror_slots;
             DROP TABLE IF EXISTS fallback_groups;
             DROP TABLE IF EXISTS native_items;
             DROP TABLE IF EXISTS replayed_lines;
             CREATE TABLE mirror_slots(
               ordinal INTEGER PRIMARY KEY AUTOINCREMENT,
               slot_key TEXT NOT NULL UNIQUE,
               mirror_key TEXT NOT NULL,
               consumed INTEGER NOT NULL DEFAULT 0);
             CREATE INDEX mirror_slots_match
               ON mirror_slots(mirror_key,consumed,ordinal);
             CREATE TABLE fallback_groups(
               mirror_key TEXT PRIMARY KEY,
               candidate_count INTEGER NOT NULL DEFAULT 0,
               eligible INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE native_items(
               native_key TEXT PRIMARY KEY,
               payload_sha256 TEXT NOT NULL,
               first_raw_line INTEGER NOT NULL);
             CREATE TABLE replayed_lines(raw_line INTEGER PRIMARY KEY);",
        )?;
        for (slot_key, mirror_key) in accepted.available_message_slots {
            index.execute(
                "INSERT INTO mirror_slots(slot_key,mirror_key) VALUES(?1,?2)",
                params![slot_key, mirror_key],
            )?;
        }

        // Fallback identities must not depend on which generation first
        // admitted a physical raw row. Otherwise incremental G1→G2 and a
        // one-shot G2 assign different src_uuid values to identical bytes.
        let file_id =
            stable_source_session_id(&route.owner_key, Source::Codex, &route.source_session_key);
        let mut first_pass = BoundedJsonl::open(&payload_path, 0, 0)?;
        let mut full_line_count = 0u64;
        let mut family_counts = BTreeMap::<String, u64>::new();
        let mut turn_scope = "pre-turn".to_string();
        let mut prefix_turn_scope = turn_scope.clone();
        let mut turn_boundary_line = None;
        let mut turn_boundary_sha256: Option<String> = None;
        let mut prefix_turn_boundary_line = None;
        let mut prefix_turn_boundary_sha256: Option<String> = None;
        while let Some(raw) = first_pass.next_line()? {
            full_line_count = raw.line;
            if stopped(raw.line) {
                return Err(CoreError::Integrity(
                    SOURCE_MOVED_DURING_MAPPING.to_string(),
                ));
            }
            if raw.over_cap {
                return Err(CoreError::Integrity(format!(
                    "Codex raw line {} exceeds the 8 MiB admission cap",
                    raw.line
                )));
            }
            if raw.bytes.iter().all(|b| b.is_ascii_whitespace()) {
                continue;
            }
            let value: Value = match serde_json::from_slice(&raw.bytes) {
                Ok(value) => value,
                Err(_) => {
                    if raw.line > accepted_prefix_lines {
                        *family_counts.entry("malformed".to_string()).or_insert(0) += 1;
                    }
                    continue;
                }
            };
            update_turn_scope(
                &value,
                raw.line,
                &raw.sha256,
                &mut turn_scope,
                &mut turn_boundary_line,
                &mut turn_boundary_sha256,
            );
            if raw.line <= accepted_prefix_lines {
                prefix_turn_scope.clone_from(&turn_scope);
                prefix_turn_boundary_line = turn_boundary_line;
                prefix_turn_boundary_sha256.clone_from(&turn_boundary_sha256);
            }
            let replayed = index_native_item(&index, &value, raw.line)?;
            let normalized = normalize_codex_line(
                CodexLineContext {
                    owner_session_id: &file_id,
                    source_session_id: &route.source_session_key,
                    file_id: &file_id,
                    turn_scope: &turn_scope,
                    turn_boundary_line,
                    turn_boundary_sha256: turn_boundary_sha256.as_deref(),
                    line: raw.line,
                    raw_sha256: &raw.sha256,
                },
                &value,
            )?;
            let family = family_name(&normalized.envelope_type, normalized.item_type.as_deref());
            let is_local = raw.line > accepted_prefix_lines;
            if is_local {
                *family_counts.entry(family).or_insert(0) += 1;
            }
            if replayed {
                continue;
            }
            if !is_local {
                continue;
            }
            for mapped in &normalized.events {
                match &mapped.emission {
                    CodexEmissionRole::CanonicalItem {
                        mirror_key: Some(key),
                    } => {
                        let slot_key = mapped.event.src_uuid.as_deref().ok_or_else(|| {
                            CoreError::Integrity(
                                "canonical Codex message has no stable slot identity".to_string(),
                            )
                        })?;
                        index.execute(
                            "INSERT INTO mirror_slots(slot_key,mirror_key) VALUES(?1,?2)",
                            params![slot_key, key],
                        )?;
                    }
                    CodexEmissionRole::FallbackCandidate { mirror_key } => {
                        index.execute(
                            "INSERT INTO fallback_groups(mirror_key,candidate_count)
                             VALUES(?1,1)
                             ON CONFLICT(mirror_key) DO UPDATE
                               SET candidate_count=candidate_count+1",
                            params![mirror_key],
                        )?;
                    }
                    _ => continue,
                }
            }
        }
        index.execute(
            "UPDATE fallback_groups
             SET eligible = CASE WHEN candidate_count = (
                   SELECT COUNT(*) FROM mirror_slots s
                   WHERE s.mirror_key=fallback_groups.mirror_key AND s.consumed=0)
                 AND candidate_count > 0 THEN 1 ELSE 0 END",
            [],
        )?;

        let mut malformed = 0u64;
        let mut unknown = 0u64;
        let mut suppressed_mirrors = 0u64;
        let mut suppressed_replays = 0u64;
        let mut fallback_messages = 0u64;
        let mut second_pass =
            BoundedJsonl::open(&payload_path, accepted_prefix_bytes, accepted_prefix_lines)?;
        let mut turn_scope = prefix_turn_scope;
        let mut turn_boundary_line = prefix_turn_boundary_line;
        let mut turn_boundary_sha256 = prefix_turn_boundary_sha256;
        while let Some(raw) = second_pass.next_line()? {
            if stopped(raw.line) {
                return Err(CoreError::Integrity(
                    SOURCE_MOVED_DURING_MAPPING.to_string(),
                ));
            }
            if fail_at_raw_line == Some(raw.line) {
                return Err(CoreError::Integrity(format!(
                    "injected Codex admission failure at raw line {}",
                    raw.line
                )));
            }
            if raw.over_cap {
                return Err(CoreError::Integrity(format!(
                    "Codex raw line {} exceeds the 8 MiB admission cap",
                    raw.line
                )));
            }
            if raw.bytes.iter().all(|b| b.is_ascii_whitespace()) {
                spool.push_disposition(raw.disposition(
                    "blank",
                    "blank",
                    None,
                    0,
                    source_turn_identity(&file_id, &turn_scope),
                    turn_boundary_line,
                    turn_boundary_sha256.clone(),
                ))?;
                continue;
            }
            let value: Value = match serde_json::from_slice(&raw.bytes) {
                Ok(value) => value,
                Err(_) if reject_malformed_suffix => {
                    return Err(CoreError::Integrity(format!(
                        "successor suffix contains malformed JSON at raw line {}",
                        raw.line
                    )));
                }
                Err(_) => {
                    malformed += 1;
                    spool.push_disposition(raw.disposition(
                        "malformed",
                        "malformed",
                        Some("invalid_json".to_string()),
                        0,
                        source_turn_identity(&file_id, &turn_scope),
                        turn_boundary_line,
                        turn_boundary_sha256.clone(),
                    ))?;
                    continue;
                }
            };
            update_turn_scope(
                &value,
                raw.line,
                &raw.sha256,
                &mut turn_scope,
                &mut turn_boundary_line,
                &mut turn_boundary_sha256,
            );
            let normalized = normalize_codex_line(
                CodexLineContext {
                    owner_session_id: &file_id,
                    source_session_id: &route.source_session_key,
                    file_id: &file_id,
                    turn_scope: &turn_scope,
                    turn_boundary_line,
                    turn_boundary_sha256: turn_boundary_sha256.as_deref(),
                    line: raw.line,
                    raw_sha256: &raw.sha256,
                },
                &value,
            )?;
            let family = family_name(&normalized.envelope_type, normalized.item_type.as_deref());
            let replayed: bool = index.query_row(
                "SELECT EXISTS(SELECT 1 FROM replayed_lines WHERE raw_line=?1)",
                params![raw.line as i64],
                |row| row.get(0),
            )?;
            if replayed {
                suppressed_replays += 1;
                spool.push_disposition(
                    raw.disposition(
                        &family,
                        "suppressed_replay",
                        Some("exact_native_item_replay".to_string()),
                        0,
                        normalized
                            .events
                            .first()
                            .and_then(|event| event.event.source_turn_id.clone()),
                        turn_boundary_line,
                        turn_boundary_sha256.clone(),
                    ),
                )?;
                continue;
            }
            let family_unknown =
                is_unknown_family(&normalized.envelope_type, normalized.item_type.as_deref());
            let mut emitted = 0u64;
            let mut any_metadata = false;
            let mut any_fallback = false;
            let mut any_suppressed = false;
            let mut matched_slot: Option<String> = None;
            for mapped in normalized.events {
                match mapped.emission {
                    CodexEmissionRole::FallbackCandidate { mirror_key } => {
                        let is_assistant =
                            mapped.event.kind == crate::model::EventKind::AssistantText;
                        if let Some(slot_key) = consume_mirror_slot(&index, &mirror_key)? {
                            if is_assistant {
                                suppressed_mirrors += 1;
                            }
                            any_suppressed = true;
                            matched_slot = Some(slot_key);
                            continue;
                        }
                        if is_assistant {
                            fallback_messages += 1;
                        }
                        any_fallback = true;
                    }
                    CodexEmissionRole::Metadata => any_metadata = true,
                    CodexEmissionRole::CanonicalItem { .. } => {}
                }
                let raw_ref = RawLineRef {
                    file: payload_path.display().to_string(),
                    line: raw.line,
                    sha256: raw.sha256.clone(),
                };
                spool.push_event(&raw_ref, &mapped.event)?;
                emitted += 1;
            }
            let disposition = if family_unknown {
                unknown += 1;
                "unknown"
            } else if emitted == 0 && any_suppressed {
                "suppressed_mirror"
            } else if any_fallback {
                "fallback"
            } else if any_metadata {
                "meta"
            } else {
                "event"
            };
            let reason = matched_slot.map(|slot| format!("matched_slot:{slot}"));
            spool.push_disposition(raw.disposition(
                &family,
                disposition,
                reason,
                emitted,
                source_turn_identity(&file_id, &turn_scope),
                turn_boundary_line,
                turn_boundary_sha256.clone(),
            ))?;
        }

        drop(index);
        remove_private_sqlite_files(&index_path)?;
        let spool = spool.finish()?;
        let disposition_count = crate::ledger::GenerationRowSource::disposition_count(&spool);
        let event_count = crate::ledger::GenerationRowSource::event_count(&spool);
        let spool_flushes = spool.flush_count();
        let max_spool_buffered_rows = spool.max_buffered_rows();
        let max_spool_buffered_bytes = spool.max_buffered_bytes();

        Ok(PreparedCodexGeneration {
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
        })
    }
}

impl TranscriptAdapter for CodexAdapter {
    fn source(&self) -> Source {
        Source::Codex
    }

    fn probe(&self, path: &Path) -> Result<Probe> {
        match probe_file_source(path)? {
            Some(Source::Codex) => Ok(Probe {
                source: Source::Codex,
                format_token: "codex-rollout-jsonl".to_string(),
            }),
            Some(Source::ClaudeCode) => Err(CoreError::Config(
                "source probe identified a Claude Code transcript".to_string(),
            )),
            None => Err(CoreError::Config(
                "source probe found no recognized Codex envelope".to_string(),
            )),
        }
    }

    fn discover(&self, roots: &[PathBuf]) -> Result<Vec<SessionPlan>> {
        let mut plans = Vec::new();
        for root in roots {
            let owner_root = codex_store_root(root).unwrap_or_else(|| {
                if root.is_file() {
                    root.parent().unwrap_or(Path::new(".")).to_path_buf()
                } else {
                    root.clone()
                }
            });
            discover_jsonl(root, &owner_root, &mut plans)?;
        }
        plans.sort_by(|a, b| a.path.cmp(&b.path));
        plans.dedup_by(|a, b| a.path == b.path);
        Ok(plans)
    }

    fn inspect(&self, plan: &SessionPlan) -> Result<LogicalRoute> {
        let owner_key = self.owner_key_for(plan)?;
        inspect_codex_file(&plan.path, owner_key)
    }
}

/// Identify an explicitly supplied file or directory without relying on its
/// pathname. This preserves legacy Claude roots while keeping `auto` closed
/// when the supplied material is empty, unknown, or contains both sources.
pub fn detect_source_path(path: &Path) -> Result<Source> {
    let adapter = CodexAdapter::new(None);
    let plans = adapter.discover(&[path.to_path_buf()])?;
    let mut saw_claude = false;
    let mut saw_codex = false;
    for plan in plans {
        match probe_file_source(&plan.path)? {
            Some(Source::ClaudeCode) => saw_claude = true,
            Some(Source::Codex) => saw_codex = true,
            None => {}
        }
        if saw_claude && saw_codex {
            return Err(CoreError::Config(
                "auto source detection found both Claude Code and Codex transcripts".to_string(),
            ));
        }
    }
    match (saw_claude, saw_codex) {
        (true, false) => Ok(Source::ClaudeCode),
        (false, true) => Ok(Source::Codex),
        _ => Err(CoreError::Config(
            "auto source detection found no recognized transcript envelope".to_string(),
        )),
    }
}

fn probe_file_source(path: &Path) -> Result<Option<Source>> {
    let mut reader = BoundedJsonl::open(path, 0, 0)?;
    while let Some(raw) = reader.next_line()? {
        if raw.over_cap || raw.bytes.iter().all(|b| b.is_ascii_whitespace()) {
            continue;
        }
        let value: Value = match serde_json::from_slice(&raw.bytes) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let top = value.get("type").and_then(Value::as_str).unwrap_or("");
        if is_codex_envelope(top) {
            return Ok(Some(Source::Codex));
        }
        if matches!(top, "user" | "assistant" | "summary") {
            return Ok(Some(Source::ClaudeCode));
        }
    }
    Ok(None)
}

fn codex_store_root(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|ancestor| ancestor.file_name().is_some_and(|name| name == ".codex"))
        .map(Path::to_path_buf)
}

/// The error [`CodexAdapter::prepare_generation_watching`] gives up with.
pub const SOURCE_MOVED_DURING_MAPPING: &str = "live source moved while its candidate was mapped";

pub struct PreparedCodexGeneration {
    pub full_line_count: u64,
    pub disposition_count: u64,
    pub event_count: u64,
    pub family_counts: BTreeMap<String, u64>,
    pub malformed: u64,
    pub unknown: u64,
    pub suppressed_mirrors: u64,
    pub suppressed_replays: u64,
    pub fallback_messages: u64,
    pub spool_flushes: u64,
    pub max_spool_buffered_rows: usize,
    pub max_spool_buffered_bytes: usize,
    pub spool: GenerationSpool,
}

fn inspect_codex_file(path: &Path, owner_key: String) -> Result<LogicalRoute> {
    let mut reader = BoundedJsonl::open(path, 0, 0)?;
    let mut saw_session_meta = false;
    let mut session_key: Option<String> = None;
    let mut project_root: Option<String> = None;
    let mut context_roots = BTreeSet::new();
    let mut started_at: Option<String> = None;
    while let Some(raw) = reader.next_line()? {
        if raw.over_cap {
            return Err(CoreError::Integrity(
                "Codex route inspection encountered an over-cap line".to_string(),
            ));
        }
        if raw.bytes.iter().all(|b| b.is_ascii_whitespace()) {
            continue;
        }
        let value: Value = match serde_json::from_slice(&raw.bytes) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let envelope_type = value.get("type").and_then(Value::as_str);
        if envelope_type == Some("turn_context") {
            if let Some(root) = value
                .get("payload")
                .and_then(|payload| payload.get("cwd"))
                .and_then(Value::as_str)
            {
                context_roots.insert(lexical_absolute(Path::new(root))?);
            }
            continue;
        }
        if envelope_type != Some("session_meta") {
            continue;
        }
        let payload = value.get("payload").unwrap_or(&Value::Null);
        let candidate_session = payload
            .get("id")
            .and_then(Value::as_str)
            .or_else(|| payload.get("session_id").and_then(Value::as_str));
        let candidate_root = payload.get("cwd").and_then(Value::as_str);
        if !saw_session_meta {
            saw_session_meta = true;
            session_key = Some(
                candidate_session
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        CoreError::Integrity(
                            "first Codex session_meta has no authoritative id".to_string(),
                        )
                    })?
                    .to_owned(),
            );
            let root = candidate_root.ok_or_else(|| {
                CoreError::Integrity("first Codex session_meta has no project cwd".to_string())
            })?;
            if !Path::new(root).is_absolute() {
                return Err(CoreError::Integrity(
                    "first Codex session_meta cwd is not absolute".to_string(),
                ));
            }
            project_root = Some(lexical_absolute(Path::new(root))?);
            started_at = value
                .get("timestamp")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| {
                    payload
                        .get("timestamp")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                });
        } else if let Some(root) = candidate_root {
            let normalized = lexical_absolute(Path::new(root))?;
            if project_root.as_deref() != Some(normalized.as_str()) {
                return Err(CoreError::Integrity(
                    "Codex physical file has ambiguous project routing".to_string(),
                ));
            }
        }
    }
    let source_session_key = session_key.ok_or_else(|| {
        CoreError::Integrity("Codex physical file has no authoritative session_meta id".to_string())
    })?;
    let normalized_root = project_root.ok_or_else(|| {
        CoreError::Integrity("Codex physical file has no authoritative project root".to_string())
    })?;
    if context_roots
        .iter()
        .any(|candidate| candidate != &normalized_root)
    {
        return Err(CoreError::Integrity(
            "Codex physical file has ambiguous project routing".to_string(),
        ));
    }
    Ok(LogicalRoute {
        owner_key,
        source_project_key: normalized_root.clone(),
        source_session_key,
        display_name: display_name_from_root(&normalized_root),
        normalized_root,
        started_at,
    })
}

fn inspect_codex_first_meta(path: &Path, owner_key: String) -> Result<LogicalRoute> {
    let mut reader = BoundedJsonl::open(path, 0, 0)?;
    while let Some(raw) = reader.next_line()? {
        if raw.over_cap {
            return Err(CoreError::Integrity(
                "Codex route inspection encountered an over-cap line".to_string(),
            ));
        }
        if raw.bytes.iter().all(|byte| byte.is_ascii_whitespace()) {
            continue;
        }
        let value: Value = match serde_json::from_slice(&raw.bytes) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if value.get("type").and_then(Value::as_str) != Some("session_meta") {
            continue;
        }
        let payload = value.get("payload").unwrap_or(&Value::Null);
        let source_session_key = payload
            .get("id")
            .and_then(Value::as_str)
            .or_else(|| payload.get("session_id").and_then(Value::as_str))
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                CoreError::Integrity("first Codex session_meta has no authoritative id".to_string())
            })?
            .to_string();
        let root = payload.get("cwd").and_then(Value::as_str).ok_or_else(|| {
            CoreError::Integrity("first Codex session_meta has no project cwd".to_string())
        })?;
        if !Path::new(root).is_absolute() {
            return Err(CoreError::Integrity(
                "first Codex session_meta cwd is not absolute".to_string(),
            ));
        }
        let normalized_root = lexical_absolute(Path::new(root))?;
        let started_at = value
            .get("timestamp")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                payload
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            });
        return Ok(LogicalRoute {
            owner_key,
            source_project_key: normalized_root.clone(),
            source_session_key,
            display_name: display_name_from_root(&normalized_root),
            normalized_root,
            started_at,
        });
    }
    Err(CoreError::Integrity(
        "Codex physical file has no authoritative session_meta id".to_string(),
    ))
}

fn discover_jsonl(root: &Path, owner_root: &Path, plans: &mut Vec<SessionPlan>) -> Result<()> {
    let metadata = fs::symlink_metadata(root).map_err(|e| CoreError::io(root, e))?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_file() {
        if root.extension().is_some_and(|ext| ext == "jsonl") {
            plans.push(SessionPlan {
                source: Source::Codex,
                path: root.to_path_buf(),
                owner_root: owner_root.to_path_buf(),
            });
        }
        return Ok(());
    }
    if !metadata.is_dir() {
        return Ok(());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(root).map_err(|e| CoreError::io(root, e))? {
        entries.push(entry.map_err(|e| CoreError::io(root, e))?.path());
    }
    entries.sort();
    for path in entries {
        discover_jsonl(&path, owner_root, plans)?;
    }
    Ok(())
}

fn is_codex_envelope(value: &str) -> bool {
    matches!(
        value,
        "session_meta"
            | "turn_context"
            | "response_item"
            | "event_msg"
            | "compacted"
            | "world_state"
            | "inter_agent_communication_metadata"
    )
}

fn family_name(top: &str, item: Option<&str>) -> String {
    item.map_or_else(|| top.to_string(), |item| format!("{top}/{item}"))
}

fn is_unknown_family(top: &str, item: Option<&str>) -> bool {
    match top {
        // These two measured families are structurally recognized but have no
        // semantic interpretation in M4.R2; they are the 13,493 raw-format
        // unknowns in the M4.R1 census and remain safe Meta context.
        "world_state" | "inter_agent_communication_metadata" => true,
        "session_meta" | "turn_context" | "compacted" => false,
        "response_item" => !matches!(
            item,
            Some(
                "agent_message"
                    | "custom_tool_call"
                    | "custom_tool_call_output"
                    | "function_call"
                    | "function_call_output"
                    | "image_generation_call"
                    | "message"
                    | "reasoning"
                    | "tool_search_call"
                    | "tool_search_output"
                    | "web_search_call"
            )
        ),
        "event_msg" => !matches!(
            item,
            Some(
                "agent_message"
                    | "agent_reasoning"
                    | "context_compacted"
                    | "dynamic_tool_call_request"
                    | "dynamic_tool_call_response"
                    | "error"
                    | "exec_command_end"
                    | "image_generation_end"
                    | "item_completed"
                    | "mcp_tool_call_end"
                    | "patch_apply_end"
                    | "sub_agent_activity"
                    | "task_complete"
                    | "task_started"
                    | "thread_goal_updated"
                    | "thread_name_updated"
                    | "thread_rolled_back"
                    | "thread_settings_applied"
                    | "token_count"
                    | "turn_aborted"
                    | "user_message"
                    | "view_image_tool_call"
                    | "web_search_end"
            )
        ),
        _ => true,
    }
}

fn canonical_payload_sha(value: &Value) -> Result<String> {
    fn canonical(value: &Value) -> Value {
        match value {
            Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
            Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort_unstable();
                let mut out = serde_json::Map::new();
                for key in keys {
                    if let Some(value) = map.get(key) {
                        out.insert(key.clone(), canonical(value));
                    }
                }
                Value::Object(out)
            }
            scalar => scalar.clone(),
        }
    }
    Ok(sha256_hex(
        serde_json::to_string(&canonical(value))?.as_bytes(),
    ))
}

fn update_turn_scope(
    value: &Value,
    raw_line: u64,
    raw_sha256: &str,
    turn_scope: &mut String,
    boundary_line: &mut Option<u64>,
    boundary_sha256: &mut Option<String>,
) {
    if value.get("type").and_then(Value::as_str) != Some("turn_context") {
        return;
    }
    let recognized = value
        .get("payload")
        .and_then(|payload| payload.get("turn_id"))
        .and_then(Value::as_str)
        .filter(|turn| !turn.trim().is_empty());
    *turn_scope = recognized.map_or_else(
        || format!("unidentified-turn-at-{raw_line}"),
        |turn| format!("turn:{turn}"),
    );
    *boundary_line = Some(raw_line);
    *boundary_sha256 = Some(raw_sha256.to_string());
}

fn index_native_item(index: &Connection, value: &Value, raw_line: u64) -> Result<bool> {
    if value.get("type").and_then(Value::as_str) != Some("response_item") {
        return Ok(false);
    }
    let Some(payload) = value.get("payload").filter(|payload| payload.is_object()) else {
        return Ok(false);
    };
    // `call_id` associates a tool call with its output; it is not the native
    // identity of both response items. Only an affirmative response-item id
    // can authorize exact replay suppression.
    let Some(native) = payload
        .get("id")
        .and_then(Value::as_str)
        .filter(|native| !native.is_empty())
    else {
        return Ok(false);
    };
    let native_key = native.to_string();
    let payload_sha = canonical_payload_sha(payload)?;
    let prior: Option<(String, i64)> = index
        .query_row(
            "SELECT payload_sha256,first_raw_line FROM native_items WHERE native_key=?1",
            params![native_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    match prior {
        None => {
            index.execute(
                "INSERT INTO native_items(native_key,payload_sha256,first_raw_line)
                 VALUES(?1,?2,?3)",
                params![native_key, payload_sha, raw_line as i64],
            )?;
            Ok(false)
        }
        Some((prior_sha, _)) if prior_sha == payload_sha => {
            index.execute(
                "INSERT INTO replayed_lines(raw_line) VALUES(?1)",
                params![raw_line as i64],
            )?;
            Ok(true)
        }
        Some(_) => Err(CoreError::Integrity(
            "Codex response item id repeats with divergent payload".to_string(),
        )),
    }
}

fn consume_mirror_slot(index: &Connection, mirror_key: &str) -> Result<Option<String>> {
    let eligible: bool = index
        .query_row(
            "SELECT eligible FROM fallback_groups WHERE mirror_key=?1",
            params![mirror_key],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(false);
    if !eligible {
        return Ok(None);
    }
    let slot: Option<String> = index
        .query_row(
            "SELECT slot_key FROM mirror_slots
             WHERE mirror_key=?1 AND consumed=0 ORDER BY ordinal LIMIT 1",
            params![mirror_key],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(slot) = &slot {
        let changed = index.execute(
            "UPDATE mirror_slots SET consumed=1 WHERE slot_key=?1 AND consumed=0",
            params![slot],
        )?;
        if changed != 1 {
            return Err(CoreError::Integrity(
                "Codex mirror slot consumption was not deterministic".to_string(),
            ));
        }
    }
    Ok(slot)
}

fn prefix_ends_at_line_boundary(path: &Path, bytes: u64) -> Result<bool> {
    if bytes == 0 {
        return Ok(true);
    }
    let mut file = File::open(path).map_err(|e| CoreError::io(path, e))?;
    file.seek(SeekFrom::Start(bytes - 1))
        .map_err(|e| CoreError::io(path, e))?;
    let mut byte = [0u8; 1];
    file.read_exact(&mut byte)
        .map_err(|e| CoreError::io(path, e))?;
    Ok(byte[0] == b'\n')
}

#[derive(Debug)]
struct RawJsonlLine {
    line: u64,
    byte_start: u64,
    byte_length: u64,
    sha256: String,
    bytes: Vec<u8>,
    over_cap: bool,
}

impl RawJsonlLine {
    #[allow(clippy::too_many_arguments)]
    fn disposition(
        &self,
        family: &str,
        disposition: &str,
        reason: Option<String>,
        emitted_event_count: u64,
        source_turn_id: Option<String>,
        turn_boundary_raw_line: Option<u64>,
        turn_boundary_raw_sha256: Option<String>,
    ) -> RawDisposition {
        RawDisposition {
            raw_line: self.line,
            raw_sha256: self.sha256.clone(),
            byte_start: self.byte_start,
            byte_length: self.byte_length,
            envelope_family: family.to_string(),
            disposition: disposition.to_string(),
            reason,
            source_turn_id,
            turn_boundary_raw_line,
            turn_boundary_raw_sha256,
            emitted_event_count,
        }
    }
}

struct BoundedJsonl {
    reader: BufReader<File>,
    path: PathBuf,
    line: u64,
    offset: u64,
}

impl BoundedJsonl {
    fn open(path: &Path, offset: u64, base_line: u64) -> Result<Self> {
        let mut file = OpenOptions::new()
            .read(true)
            .open(path)
            .map_err(|e| CoreError::io(path, e))?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| CoreError::io(path, e))?;
        Ok(Self {
            reader: BufReader::with_capacity(64 * 1024, file),
            path: path.to_path_buf(),
            line: base_line,
            offset,
        })
    }

    fn next_line(&mut self) -> Result<Option<RawJsonlLine>> {
        let start = self.offset;
        let mut bytes = Vec::new();
        let mut over_cap = false;
        loop {
            let available = self
                .reader
                .fill_buf()
                .map_err(|e| CoreError::io(&self.path, e))?;
            if available.is_empty() {
                if self.offset == start {
                    return Ok(None);
                }
                break;
            }
            let take = available
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(available.len(), |index| index + 1);
            let chunk = &available[..take];
            self.offset += take as u64;
            if !over_cap {
                // Retain room for CRLF framing, then enforce the content cap
                // after stripping the terminator. An unterminated MAX+1 row
                // must never be admitted as if its extra byte were framing.
                if bytes.len().saturating_add(chunk.len()) > MAX_CODEX_LINE_BYTES + 2 {
                    over_cap = true;
                    bytes.clear();
                } else {
                    bytes.extend_from_slice(chunk);
                }
            }
            let ended = chunk.last() == Some(&b'\n');
            self.reader.consume(take);
            if ended {
                break;
            }
        }
        self.line += 1;
        let byte_length = self.offset - start;
        if bytes.last() == Some(&b'\n') {
            bytes.pop();
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
        }
        if bytes.len() > MAX_CODEX_LINE_BYTES {
            over_cap = true;
            bytes.clear();
        }
        // Over-cap rows are fail-loud and can never enter a generation, so no
        // per-row receipt is accepted for bytes the bounded reader did not
        // retain. The complete candidate mirror still carries their exact hash.
        let sha256 = if over_cap {
            String::new()
        } else {
            sha256_hex(&bytes)
        };
        Ok(Some(RawJsonlLine {
            line: self.line,
            byte_start: start,
            byte_length,
            sha256,
            bytes: if over_cap { Vec::new() } else { bytes },
            over_cap,
        }))
    }
}

fn set_private_mode(path: &Path, mode: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .map_err(|e| CoreError::io(path, e))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
        return Err(CoreError::Config(
            "private Codex authority index permissions are unavailable".to_string(),
        ));
    }
    Ok(())
}

fn create_private_directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
                return Err(CoreError::Integrity(
                    "private adapter index path is not a non-symlink directory".to_string(),
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
                .create(path)
                .map_err(|error| CoreError::io(path, error))?;
            sync_parent(path)?;
        }
        Err(error) => return Err(CoreError::io(path, error)),
    }
    set_private_mode(path, 0o700)
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

fn remove_private_sqlite_files(path: &Path) -> Result<()> {
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

fn sync_parent(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let directory = File::open(parent).map_err(|error| CoreError::io(parent, error))?;
    directory
        .sync_all()
        .map_err(|error| CoreError::io(parent, error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn bounded_reader_preserves_offsets_hashes_and_final_line() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("rows.jsonl");
        fs::write(&path, b"one\r\ntwo").expect("write");
        let mut reader = BoundedJsonl::open(&path, 0, 0).expect("open");
        let one = reader.next_line().expect("read").expect("one");
        let two = reader.next_line().expect("read").expect("two");
        assert_eq!(one.bytes, b"one");
        assert_eq!(one.byte_start, 0);
        assert_eq!(one.byte_length, 5);
        assert_eq!(two.bytes, b"two");
        assert_eq!(two.byte_start, 5);
        assert_eq!(two.byte_length, 3);
        assert!(reader.next_line().expect("eof").is_none());
    }

    #[test]
    fn source_route_uses_first_session_meta_and_rejects_root_ambiguity() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("rollout.jsonl");
        let mut file = File::create(&path).expect("create");
        writeln!(
            file,
            r#"{{"timestamp":"t0","type":"session_meta","payload":{{"id":"s1","cwd":"/work/a"}}}}"#
        )
        .expect("first");
        writeln!(
            file,
            r#"{{"type":"session_meta","payload":{{"id":"lineage","cwd":"/work/a"}}}}"#
        )
        .expect("lineage");
        let route = inspect_codex_file(&path, "owner".to_string()).expect("route");
        assert_eq!(route.source_session_key, "s1");
        assert_eq!(route.normalized_root, "/work/a");

        writeln!(
            file,
            r#"{{"type":"session_meta","payload":{{"id":"other","cwd":"/work/b"}}}}"#
        )
        .expect("ambiguous");
        drop(file);
        assert!(inspect_codex_file(&path, "owner".to_string()).is_err());
    }

    #[test]
    fn first_session_meta_without_id_cannot_be_rebound_by_a_later_meta() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("rollout.jsonl");
        fs::write(
            &path,
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/work/a\"}}\n",
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"later\",\"cwd\":\"/work/a\"}}\n"
            ),
        )
        .expect("write");
        let error = inspect_codex_file(&path, "owner".to_string())
            .expect_err("later metadata must never become the physical owner");
        assert!(error.to_string().contains("no authoritative id"));
    }

    #[test]
    fn source_probe_skips_malformed_prefix_without_waiting_or_guessing() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("rollout.jsonl");
        fs::write(
            &path,
            concat!(
                "{malformed\n",
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"s\",\"cwd\":\"/work/a\"}}\n"
            ),
        )
        .expect("write");
        let probe = CodexAdapter::new(Some("owner".to_string()))
            .probe(&path)
            .expect("recognized envelope after malformed prefix");
        assert_eq!(probe.source, Source::Codex);
    }

    #[test]
    fn auto_source_detection_preserves_generic_claude_roots_and_rejects_mixed_roots() {
        let tmp = TempDir::new().expect("tempdir");
        let project = tmp.path().join("projects").join("legacy-project");
        fs::create_dir_all(&project).expect("project dir");
        fs::write(
            project.join("claude.jsonl"),
            "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":\"done\"}}\n",
        )
        .expect("claude fixture");
        assert_eq!(
            detect_source_path(&tmp.path().join("projects")).expect("legacy root"),
            Source::ClaudeCode
        );

        fs::write(
            project.join("codex.jsonl"),
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"s\",\"cwd\":\"/work\"}}\n",
        )
        .expect("codex fixture");
        let error = detect_source_path(&tmp.path().join("projects"))
            .expect_err("mixed source root must fail closed");
        assert!(error.to_string().contains("both Claude Code and Codex"));
    }

    #[test]
    fn bounded_reader_rejects_unterminated_max_plus_one_content_byte() {
        let tmp = TempDir::new().expect("tempdir");
        let exact = tmp.path().join("exact.jsonl");
        fs::write(&exact, vec![b'x'; MAX_CODEX_LINE_BYTES]).expect("write exact cap");
        let line = BoundedJsonl::open(&exact, 0, 0)
            .expect("open")
            .next_line()
            .expect("read")
            .expect("line");
        assert!(!line.over_cap);
        assert_eq!(line.bytes.len(), MAX_CODEX_LINE_BYTES);

        let over = tmp.path().join("over.jsonl");
        fs::write(&over, vec![b'x'; MAX_CODEX_LINE_BYTES + 1]).expect("write over cap");
        let line = BoundedJsonl::open(&over, 0, 0)
            .expect("open")
            .next_line()
            .expect("read")
            .expect("line");
        assert!(line.over_cap);
        assert!(line.bytes.is_empty());
    }

    #[test]
    fn session_and_archive_roots_share_one_codex_owner_scope() {
        let tmp = TempDir::new().expect("tempdir");
        let store = tmp.path().join(".codex");
        let sessions = store.join("sessions");
        let archive = store.join("archived_sessions");
        fs::create_dir_all(&sessions).expect("sessions");
        fs::create_dir_all(&archive).expect("archive");
        fs::write(sessions.join("one.jsonl"), "{}\n").expect("one");
        fs::write(archive.join("two.jsonl"), "{}\n").expect("two");
        let adapter = CodexAdapter::new(None);
        let plans = adapter
            .discover(&[sessions, archive])
            .expect("discover both stores");
        assert_eq!(plans.len(), 2);
        assert!(plans.iter().all(|plan| plan.owner_root == store));
    }
}
