//! Streaming JSONL parser: raw transcript lines → normalized events (§8).
//!
//! Reads one line at a time through a [`BufReader`] — a session file can be
//! tens of megabytes and is never loaded whole. Nothing here panics: every
//! defect becomes a [`MalformedLine`] the caller counts and reports.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use serde_json::{Map, Value};

use crate::chain::sha256_hex;
use crate::error::{CoreError, Result};
use crate::model::{EventKind, MalformedLine, MalformedReason, NormalizedEvent};
use sha2::{Digest, Sha256};

/// One raw line that yielded at least zero events.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ParsedLine {
    pub line: u64,
    /// SHA-256 of the line bytes, excluding the terminator.
    pub sha256: String,
    /// `sessionId` when the line carries one. Never `session_id` — on
    /// `sessionKind:"bg"` files that is the *parent* session (§8).
    pub session_id: Option<String>,
    /// The line's `type`, verbatim, for the ingest report.
    pub raw_type: String,
    /// Events in block order. One line commonly fans out to several.
    pub events: Vec<NormalizedEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineOutcome {
    Parsed(ParsedLine),
    Malformed(MalformedLine),
}

/// Tallies for the ingest report. Counting is the point — see Rule 12.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParseStats {
    /// Every raw line in the file, including blank and malformed ones — the
    /// population the session digest covers.
    pub raw_lines_total: u64,
    /// SHA-256 over every raw line in order (§7 `sessions.raw_digest`). Closes
    /// M1's gap where a malformed line carried no tamper evidence.
    pub raw_digest: String,
    pub lines_read: u64,
    pub lines_malformed: u64,
    pub events: u64,
    pub events_by_kind: BTreeMap<String, u64>,
    /// `type` values outside the §8 mapping, with occurrence counts.
    pub unknown_types: BTreeMap<String, u64>,
    pub malformed: Vec<MalformedLine>,
}

impl ParseStats {
    fn record(&mut self, outcome: &LineOutcome) {
        self.lines_read += 1;
        match outcome {
            LineOutcome::Parsed(p) => {
                if !is_mapped_type(&p.raw_type) {
                    *self.unknown_types.entry(p.raw_type.clone()).or_insert(0) += 1;
                }
                for ev in &p.events {
                    self.events += 1;
                    *self
                        .events_by_kind
                        .entry(ev.kind.as_str().to_string())
                        .or_insert(0) += 1;
                }
            }
            LineOutcome::Malformed(m) => {
                self.lines_malformed += 1;
                self.malformed.push(m.clone());
            }
        }
    }
}

/// `type` values the §8 table maps to something other than the catch-all.
fn is_mapped_type(t: &str) -> bool {
    matches!(t, "user" | "assistant" | "summary")
}

/// Streaming reader over one session file.
pub struct SessionLines<R: BufRead> {
    reader: R,
    file: String,
    line: u64,
    finished: bool,
    digest: Sha256,
    raw_lines: u64,
}

impl SessionLines<BufReader<File>> {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let file = File::open(path).map_err(|e| CoreError::io(path, e))?;
        Ok(SessionLines {
            reader: BufReader::new(file),
            file: path.display().to_string(),
            line: 0,
            finished: false,
            digest: Sha256::new(),
            raw_lines: 0,
        })
    }
}

impl<R: BufRead> SessionLines<R> {
    pub fn from_reader(reader: R, file: impl Into<String>) -> Self {
        SessionLines {
            reader,
            file: file.into(),
            line: 0,
            finished: false,
            digest: Sha256::new(),
            raw_lines: 0,
        }
    }

    pub fn file(&self) -> &str {
        &self.file
    }

    /// Read every line, folding outcomes into `stats`.
    pub fn collect_with_stats(mut self, stats: &mut ParseStats) -> Result<Vec<ParsedLine>> {
        let mut out = Vec::new();
        for item in self.by_ref() {
            let outcome = item?;
            stats.record(&outcome);
            if let LineOutcome::Parsed(p) = outcome {
                out.push(p);
            }
        }
        // Ownership is retained through the loop so the whole-file digest can
        // be finalized here rather than guessed by the caller.
        stats.raw_lines_total = self.raw_lines;
        stats.raw_digest = crate::chain::to_hex(&self.digest.finalize());
        Ok(out)
    }
}

impl<R: BufRead> Iterator for SessionLines<R> {
    type Item = Result<LineOutcome>;

    fn next(&mut self) -> Option<Self::Item> {
        // Loops rather than recursing on skipped lines: a file of many
        // consecutive blank lines would otherwise blow the stack, and an
        // abort is not a way to "handle" malformed input.
        let (buf, line, terminated) = loop {
            if self.finished {
                return None;
            }
            let mut buf = Vec::new();
            match self.reader.read_until(b'\n', &mut buf) {
                Err(e) => {
                    self.finished = true;
                    return Some(Err(CoreError::io(&self.file, e)));
                }
                Ok(0) => {
                    self.finished = true;
                    return None;
                }
                Ok(_) => {}
            }

            // A line with no terminator can only be the last one in the file.
            let terminated = buf.ends_with(b"\n");
            if terminated {
                buf.pop();
                if buf.ends_with(b"\r") {
                    buf.pop();
                }
            } else {
                self.finished = true;
            }

            // Counted even when skipped, so `file:line` receipts stay true to
            // what an editor shows.
            self.line += 1;

            // Every raw line feeds the session digest — blank and malformed
            // included. Length-prefixed so line boundaries are unambiguous.
            self.raw_lines += 1;
            self.digest.update((buf.len() as u64).to_le_bytes());
            self.digest.update(&buf);

            // Blank and whitespace-only lines are separators, not defects.
            if buf.iter().all(|b| b.is_ascii_whitespace()) {
                continue;
            }
            break (buf, self.line, terminated);
        };

        let sha256 = sha256_hex(&buf);

        let text = match std::str::from_utf8(&buf) {
            Ok(t) => t,
            Err(_) => {
                return Some(Ok(LineOutcome::Malformed(MalformedLine {
                    file: self.file.clone(),
                    line,
                    reason: MalformedReason::InvalidUtf8,
                })))
            }
        };

        let value: Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(e) => {
                let reason = if terminated {
                    MalformedReason::InvalidJson(e.to_string())
                } else {
                    // Unterminated *and* unparseable: the writer was
                    // interrupted mid-line. Expected, not corruption.
                    MalformedReason::TruncatedFinalLine
                };
                return Some(Ok(LineOutcome::Malformed(MalformedLine {
                    file: self.file.clone(),
                    line,
                    reason,
                })));
            }
        };

        Some(Ok(LineOutcome::Parsed(normalize(line, sha256, &value))))
    }
}

/// Convenience: parse a whole file, streaming internally.
pub fn parse_file(path: impl AsRef<Path>, stats: &mut ParseStats) -> Result<Vec<ParsedLine>> {
    SessionLines::open(path)?.collect_with_stats(stats)
}

/// Convenience for tests and fixtures.
pub fn parse_reader<R: Read>(
    reader: R,
    file: impl Into<String>,
    stats: &mut ParseStats,
) -> Result<Vec<ParsedLine>> {
    SessionLines::from_reader(BufReader::new(reader), file).collect_with_stats(stats)
}

/* ── normalization (§8) ─────────────────────────────────────────────── */

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn normalize(line: u64, sha256: String, v: &Value) -> ParsedLine {
    let raw_type = str_field(v, "type").unwrap_or_else(|| "(missing)".to_string());
    // §8: `sessionId` is authoritative and equals the file stem. `session_id`
    // is the PARENT session on background sub-sessions — using it here would
    // misattribute events.
    let session_id = str_field(v, "sessionId");
    let src_uuid = str_field(v, "uuid");
    let parent_uuid = str_field(v, "parentUuid");
    let ts = str_field(v, "timestamp");
    let is_sidechain = v
        .get("isSidechain")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let finish = |mut ev: NormalizedEvent| -> NormalizedEvent {
        ev.src_uuid = src_uuid.clone();
        ev.parent_uuid = parent_uuid.clone();
        ev.ts = ts.clone();
        ev.is_sidechain = is_sidechain;
        ev.line_session_id = session_id.clone();
        ev
    };

    let events: Vec<NormalizedEvent> = match raw_type.as_str() {
        "user" => normalize_user(v).into_iter().map(finish).collect(),
        "assistant" => normalize_assistant(v).into_iter().map(finish).collect(),
        "summary" => {
            let mut ev = NormalizedEvent::new(EventKind::Summary);
            ev.text = str_field(v, "summary");
            vec![finish(ev)]
        }
        other => {
            // Catch-all. The raw line stays on disk; `text` carries the type
            // token so the report and later queries can group by it.
            let mut ev = NormalizedEvent::new(EventKind::Meta);
            ev.text = Some(other.to_string());
            vec![finish(ev)]
        }
    };

    ParsedLine {
        line,
        sha256,
        session_id,
        raw_type,
        events,
    }
}

fn normalize_user(v: &Value) -> Vec<NormalizedEvent> {
    let role = v
        .get("message")
        .and_then(|m| m.get("role"))
        .and_then(Value::as_str)
        .unwrap_or("user")
        .to_string();

    let content = match v.get("message").and_then(|m| m.get("content")) {
        Some(c) => c,
        None => return vec![NormalizedEvent::new(EventKind::Meta)],
    };

    // §8: `toolUseResult` is a top-level sibling of `message`, not a field
    // inside the block. It describes the line's first tool_result.
    let tool_use_result = v.get("toolUseResult");

    match content {
        Value::String(s) => {
            let mut ev = NormalizedEvent::new(EventKind::UserMsg);
            ev.role = Some(role);
            ev.text = Some(s.clone());
            vec![ev]
        }
        Value::Array(blocks) => {
            let mut out = Vec::with_capacity(blocks.len());
            let mut first_tool_result = true;
            for block in blocks {
                let btype = block.get("type").and_then(Value::as_str).unwrap_or("");
                let mut ev = match btype {
                    "tool_result" => {
                        let mut ev = NormalizedEvent::new(EventKind::ToolResult);
                        ev.tool_use_id = str_field(block, "tool_use_id");
                        ev.is_error = block
                            .get("is_error")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        // Lossless envelope: the block's `content` (string or
                        // array) plus the line's `toolUseResult` when present.
                        let mut env = Map::new();
                        env.insert(
                            "content".to_string(),
                            block.get("content").cloned().unwrap_or(Value::Null),
                        );
                        if first_tool_result {
                            if let Some(tur) = tool_use_result {
                                env.insert("toolUseResult".to_string(), tur.clone());
                            }
                        }
                        first_tool_result = false;
                        ev.tool_result = Some(Value::Object(env).to_string());
                        ev
                    }
                    "text" => {
                        let mut ev = NormalizedEvent::new(EventKind::UserMsg);
                        ev.text = str_field(block, "text");
                        ev
                    }
                    other => {
                        // e.g. `image`. Preserved as meta, counted, never dropped.
                        let mut ev = NormalizedEvent::new(EventKind::Meta);
                        ev.text = Some(format!("user.content[].{other}"));
                        ev
                    }
                };
                ev.role = Some(role.clone());
                out.push(ev);
            }
            out
        }
        _ => vec![NormalizedEvent::new(EventKind::Meta)],
    }
}

fn normalize_assistant(v: &Value) -> Vec<NormalizedEvent> {
    let role = v
        .get("message")
        .and_then(|m| m.get("role"))
        .and_then(Value::as_str)
        .unwrap_or("assistant")
        .to_string();

    let blocks = match v
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
    {
        Some(b) => b,
        None => return vec![NormalizedEvent::new(EventKind::Meta)],
    };

    let mut out = Vec::with_capacity(blocks.len());
    for block in blocks {
        let btype = block.get("type").and_then(Value::as_str).unwrap_or("");
        let mut ev = match btype {
            "text" => {
                let mut ev = NormalizedEvent::new(EventKind::AssistantText);
                ev.text = str_field(block, "text");
                ev
            }
            "thinking" => {
                // Ingested for ledger completeness; excluded from claim
                // extraction at M3. The `signature` is not retained.
                let mut ev = NormalizedEvent::new(EventKind::AssistantThinking);
                ev.text = str_field(block, "thinking");
                ev
            }
            "tool_use" => {
                let mut ev = NormalizedEvent::new(EventKind::ToolUse);
                ev.tool_name = str_field(block, "name");
                ev.tool_use_id = str_field(block, "id");
                ev.tool_input = block.get("input").map(|i| i.to_string());
                ev
            }
            other => {
                let mut ev = NormalizedEvent::new(EventKind::Meta);
                ev.text = Some(format!("assistant.content[].{other}"));
                ev
            }
        };
        ev.role = Some(role.clone());
        out.push(ev);
    }
    out
}
