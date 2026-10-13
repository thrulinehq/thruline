//! A199 Sessions 2–3 — the speaker layer of the Live Truth Loop.
//!
//! Sensors append and never speak (C1.2); a **speaker** does the opposite: its
//! whole job is to put receipt-backed words into a session, and it never
//! appends a capture row. This module is the deterministic core behind the
//! three speaker verbs:
//!
//! * `thruline digest precompute --hook session-end` — reads ONLY the ended
//!   session's `sensor_events` rows (indexed by session_id — never a ledger
//!   scan) plus its transcript, binds the final turn's completion claims to
//!   tool evidence, and writes a per-project digest cache of at most three
//!   receipt-carrying lines.
//! * `thruline brief --hook session-start` — reads that cache and injects it.
//! * `thruline audit --hook stop` — same binding for the turn that just ended;
//!   each completion claim with no proof in the record is appended to
//!   `speaker_findings` (append-only, hash-chained) and the cache is refreshed
//!   so a session that never reaches SessionEnd still leaves a brief behind.
//!
//! Three rules bind every line produced here:
//!
//! 1. **"No proof found" is never "a lie found"** (A199, POSITIONING). A claim
//!    without matching evidence in the record may be entirely true — the
//!    record simply does not show it. The rendered form is *"claimed, and no
//!    proof found in the record"*, and ambiguity produces silence, not a line.
//! 2. **Every line carries a receipt**: the transcript file and line the claim
//!    sits on, plus the scanned sensor window (seq range) the absence was
//!    established against. No receipt → the line may not exist.
//! 3. **O(session), never O(project)** (A199 finding 1). Nothing here invokes
//!    annotation or any whole-project pass; evidence comes from the one
//!    session's sensor rows, claims from the one transcript's final turn.
//!    Writes hold only short transactions (A199 finding 2).
//!
//! ## Evidence position — a recorded deviation from the session spec
//!
//! The spec sketch said "bind claim → PostToolUse events AFTER the claim".
//! For a *completion* claim that ordering is backwards: the ordinary turn
//! shape is work-then-report, so the proof of "all 33 tests pass" is the test
//! run *before* the sentence, and demanding later evidence would manufacture
//! a no-proof line for almost every honestly-reported turn — the exact
//! false-accusation failure NORTHSTAR principle 4 forbids. Completion claims
//! therefore bind against the whole session's tool record. Position still
//! governs *promises*: a proposal counts as "left open" only when no tool
//! activity follows it in the transcript, which is a statement of fact about
//! the record, not an accusation.

use std::path::{Path, PathBuf};

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use crate::error::{CoreError, Result};
use crate::extract::{ClaimExtractor, ClaimKind, Marker, MarkerExtractor};
use crate::ledger::Ledger;
use crate::model::EventKind;
use crate::parse::{parse_file, ParseStats};

/// The digest cache directory, beside the ledger: `~/.thruline/digest` for the
/// default ledger, redirected wholesale by `--db` (which is how tests stay off
/// the real machine state).
pub fn digest_dir(db: &Path) -> PathBuf {
    db.with_file_name("digest")
}

/// The findings chain scope. One machine-wide chain, like the sensor's: a
/// finding can exist before its project was ever ingested.
const CHAIN_SCOPE: &str = "speaker";

/* ── project identity of the cache ───────────────────────────────────── */

/// The per-project cache key.
///
/// The vendor's transcript layout is the authority when we have it: the
/// transcript's parent directory name IS the munged project identity ingest
/// uses as `projects.id`, so SessionEnd (which writes the cache) and
/// SessionStart (which reads it) land on the same key by construction.
/// Fallback: the same munging applied to `cwd` — the vendor maps every
/// non-alphanumeric byte to `-` (see `workspace::reconstruct_root`).
pub fn project_key(transcript_path: Option<&str>, cwd: Option<&str>) -> Option<String> {
    if let Some(tp) = transcript_path {
        let parent = Path::new(tp)
            .parent()
            .and_then(Path::file_name)
            .and_then(|n| n.to_str());
        if let Some(dir) = parent {
            if !dir.is_empty() && dir != "projects" {
                return Some(sanitize_key(dir));
            }
        }
    }
    let cwd = cwd.map(str::trim).filter(|c| !c.is_empty())?;
    Some(mangle_root(cwd))
}

/// The vendor's project-directory munging: every non-alphanumeric → `-`.
pub fn mangle_root(root: &str) -> String {
    root.trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// The key doubles as a file name; anything path-hostile becomes `-`.
fn sanitize_key(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/* ── the final turn of a transcript ──────────────────────────────────── */

/// One claim from the final turn, with the receipt that locates it.
#[derive(Debug, Clone)]
pub struct TurnClaim {
    pub text: String,
    pub kind: ClaimKind,
    pub marker: Marker,
    /// Lowercased artifact tokens from the extractor's lexical gate.
    pub artifacts: Vec<String>,
    /// 1-based JSONL line in the transcript — the receipt.
    pub line: u64,
}

/// What the final turn of a transcript asserts, and where its tools stopped.
#[derive(Debug, Clone, Default)]
pub struct FinalTurn {
    /// Work / marker / proposal claims from non-sidechain assistant text after
    /// the last user prompt. Findings and micro-intents are dropped here: the
    /// scope principle (§5b) — only material work may ever produce a line.
    pub claims: Vec<TurnClaim>,
    /// Line of the last non-sidechain user text — the turn boundary. `None`
    /// means no boundary was found and no claims were extracted.
    pub boundary_line: Option<u64>,
    /// Line of the last non-sidechain tool_use anywhere in the file. A
    /// proposal after this line has nothing following it in the record.
    pub last_tool_line: Option<u64>,
    /// A202: how many non-sidechain tool_use events the WHOLE transcript
    /// holds — the transcript's own testimony of the session's tool
    /// activity, which the window-completeness guard measures the scanned
    /// sensor window against.
    pub tool_use_count: u64,
}

/// Extract the final turn: the trailing window of assistant text after the
/// last user message that carries text.
///
/// The boundary is deliberately generous toward silence: meta user lines
/// (command output, injected reminders) also move it forward, which can only
/// shrink the claim window. A transcript with no user text yields no claims.
pub fn final_turn(transcript: &Path) -> Result<FinalTurn> {
    let mut stats = ParseStats::default();
    let lines = parse_file(transcript, &mut stats)?;
    Ok(final_turn_of(&lines))
}

/// [`final_turn`] over lines already parsed — the one final-turn rule, for a
/// caller that holds the lines rather than a path (`crate::agent_log`, which
/// hands it only the lines the ledger holds unchanged and only the assistant
/// text whose own `message.role` names the agent, to find where the final claim
/// is). Nothing here reads a file.
pub fn final_turn_of(lines: &[crate::parse::ParsedLine]) -> FinalTurn {
    let mut boundary_line: Option<u64> = None;
    let mut last_tool_line: Option<u64> = None;
    let mut tool_use_count = 0u64;
    for parsed in lines {
        for ev in &parsed.events {
            if ev.is_sidechain {
                continue;
            }
            match ev.kind {
                EventKind::UserMsg => {
                    if ev.text.as_deref().is_some_and(|t| !t.trim().is_empty()) {
                        boundary_line = Some(parsed.line);
                    }
                }
                EventKind::ToolUse => {
                    last_tool_line = Some(parsed.line);
                    tool_use_count += 1;
                }
                _ => {}
            }
        }
    }

    let Some(boundary) = boundary_line else {
        return FinalTurn {
            tool_use_count,
            ..FinalTurn::default()
        };
    };

    let extractor = MarkerExtractor;
    let mut claims = Vec::new();
    for parsed in lines {
        if parsed.line <= boundary {
            continue;
        }
        for ev in &parsed.events {
            if ev.is_sidechain || ev.kind != EventKind::AssistantText {
                continue;
            }
            let Some(text) = ev.text.as_deref() else {
                continue;
            };
            for claim in extractor.extract(text) {
                if !matches!(
                    claim.kind,
                    ClaimKind::Work | ClaimKind::Marker | ClaimKind::Proposal
                ) {
                    continue;
                }
                claims.push(TurnClaim {
                    text: claim.text,
                    kind: claim.kind,
                    marker: claim.marker,
                    artifacts: claim.artifacts,
                    line: parsed.line,
                });
            }
        }
    }
    FinalTurn {
        claims,
        boundary_line: Some(boundary),
        last_tool_line,
        tool_use_count,
    }
}

/* ── session evidence from the sensor record ─────────────────────────── */

/// One PostToolUse delivery, reduced to what binding needs.
#[derive(Debug, Clone)]
pub struct EvidenceRowLite {
    pub seq: i64,
    pub chain_hash: String,
    pub tool_name: String,
    /// The tool_input object, serialized — what `identity_key` expects.
    pub tool_input: String,
    /// stdout + stderr tail (or the raw response tail), for outcome reading.
    pub response_tail: String,
    pub interrupted: bool,
    pub cwd: Option<String>,
}

/// The ended session's PostToolUse rows, by the indexed session_id lookup —
/// this is the O(session) read A199 finding 1 requires; it never scans.
pub fn session_post_tool_rows(ledger: &Ledger, session_id: &str) -> Result<Vec<EvidenceRowLite>> {
    let conn = ledger.conn();
    let mut stmt = conn.prepare(
        "SELECT seq, chain_hash, payload FROM sensor_events
         WHERE session_id = ?1 AND hook_event = 'PostToolUse'
         ORDER BY seq",
    )?;
    let rows = stmt.query_map([session_id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (seq, chain, payload) = row?;
        let Ok(v) = serde_json::from_str::<Value>(&payload) else {
            // A malformed payload was still a delivery; it is counted by the
            // scan window (seq range) even though it can bind nothing.
            continue;
        };
        let Some(tool_name) = v.get("tool_name").and_then(Value::as_str) else {
            continue;
        };
        let tool_input = v
            .get("tool_input")
            .map(Value::to_string)
            .unwrap_or_else(|| "{}".to_string());
        let response = v.get("tool_response");
        let interrupted = response
            .and_then(|r| r.get("interrupted"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let response_tail = response_tail(response);
        out.push(EvidenceRowLite {
            seq,
            chain_hash: chain,
            tool_name: tool_name.to_string(),
            tool_input,
            response_tail,
            interrupted,
            cwd: v.get("cwd").and_then(Value::as_str).map(str::to_owned),
        });
    }
    Ok(out)
}

/// A202: whether this session's own record says it began as the resumption
/// of an earlier session — the FIRST SessionStart delivery recorded for this
/// id names `source: "resume"` (verified against live payloads: sources are
/// `startup` / `resume` / `compact`). The vendor mints a NEW session id on
/// resume and rewrites the transcript under it, so a resume-born session's
/// pre-resume evidence is recorded under the prior id and cannot be joined
/// deterministically (cross-id stitching is registered future work); the
/// binder treats such a window as incomplete. A later `resume` under an id
/// that first saw `startup` does NOT count: that id's own capture covers
/// its whole life, and the transcript-deficit check remains the guard.
///
/// Callers treat an error as `false` — unknown must not degrade, exactly as
/// a missing spill file is zero spills known (fail-open, never fail-accuse
/// — and here the failure direction is toward the pre-A202 behaviour, which
/// the deficit check still guards).
pub fn session_born_from_resume(ledger: &Ledger, session_id: &str) -> Result<bool> {
    let conn = ledger.conn();
    let first: Option<String> = conn
        .query_row(
            "SELECT payload FROM sensor_events
             WHERE session_id = ?1 AND hook_event = 'SessionStart'
             ORDER BY seq LIMIT 1",
            [session_id],
            |r| r.get(0),
        )
        .optional()?;
    let source = first
        .and_then(|p| serde_json::from_str::<Value>(&p).ok())
        .and_then(|v| v.get("source").and_then(Value::as_str).map(str::to_owned));
    Ok(source.as_deref() == Some("resume"))
}

/// One recorded PostToolUse delivery of an `mcp__thruline__claim_completion`
/// call — the A201 verifier's session identifier. The delivery's
/// `tool_input` IS the call's argument object, so hashing its canonical form
/// reproduces `mcp_calls.request_sha256` exactly (the A126 identity rule:
/// exact, never proximity).
#[derive(Debug, Clone)]
pub struct McpClaimDelivery {
    pub request_sha256: String,
    pub session_id: Option<String>,
    pub seq: i64,
    pub chain_hash: String,
}

/// Every chained delivery of a `claim_completion` tool call, machine-wide.
///
/// Lives here because this module is a licensed reader of the capture rows
/// (`c1_hooks_probe`'s containment gate); the verifier consumes the result
/// and never touches the table itself. The LIKE clause is a prefilter only —
/// the tool name is then checked exactly on the parsed payload.
pub fn mcp_claim_deliveries(ledger: &Ledger) -> Result<Vec<McpClaimDelivery>> {
    let conn = ledger.conn();
    let mut stmt = conn.prepare(
        "SELECT seq, chain_hash, session_id, payload FROM sensor_events
         WHERE hook_event = 'PostToolUse'
           AND payload LIKE '%mcp__thruline__claim_completion%'
         ORDER BY seq",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (seq, chain_hash, session_id, payload) = row?;
        let Ok(v) = serde_json::from_str::<Value>(&payload) else {
            continue;
        };
        if v.get("tool_name").and_then(Value::as_str) != Some("mcp__thruline__claim_completion") {
            continue;
        }
        let Some(input) = v.get("tool_input") else {
            continue;
        };
        let canonical = serde_json::to_string(input).unwrap_or_default();
        out.push(McpClaimDelivery {
            request_sha256: sha256_hex(canonical.as_bytes()),
            session_id,
            seq,
            chain_hash,
        });
    }
    Ok(out)
}

/// stdout+stderr (or the serialized response), reduced to a bounded tail.
fn response_tail(response: Option<&Value>) -> String {
    const TAIL: usize = 6_000;
    let text = match response {
        Some(Value::Object(map)) => {
            let stdout = map.get("stdout").and_then(Value::as_str).unwrap_or("");
            let stderr = map.get("stderr").and_then(Value::as_str).unwrap_or("");
            if stdout.is_empty() && stderr.is_empty() {
                Value::Object(map.clone()).to_string()
            } else {
                format!("{stdout}\n{stderr}")
            }
        }
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    };
    match text.char_indices().nth_back(TAIL.saturating_sub(1)) {
        Some((idx, _)) => text[idx..].to_string(),
        None => text,
    }
}

/// The session's evidence, indexed by class. Success reading is deliberately
/// generous: a generous match produces silence, and silence is the safe
/// direction to be wrong in (§5b).
#[derive(Debug, Clone, Default)]
pub struct EvidenceIndex {
    /// Every PostToolUse delivery for the session, parsable or not — the scan
    /// window a no-proof line cites.
    pub rows_scanned: u64,
    pub seq_lo: Option<i64>,
    pub seq_hi: Option<i64>,
    /// Chain hash of the last scanned row — the tamper-evidence anchor of the
    /// window.
    pub chain_head: Option<String>,
    /// A201 spill guard: deliveries for THIS session that never reached the
    /// chain (`sensor::spilled_for_session`). Any value above zero means the
    /// evidence window is incomplete, and [`bind`] refuses to accuse over an
    /// incomplete window — a spilled row could be the very proof a no-proof
    /// line would deny. Zero when unknown, which is the pre-A201 behaviour.
    pub spilled_deliveries: u64,
    /// A202 window-completeness guard: the transcript's own count of
    /// non-sidechain tool_use events ([`FinalTurn::tool_use_count`]), when a
    /// transcript was read. ANY deficit — the transcript holding more tool
    /// events than the window scanned — marks the window incomplete: one
    /// unseen event could be the very proof a no-proof line would deny.
    /// Surplus means nothing (sidechain tool calls deliver under the same
    /// session id, so scanned rows can legitimately exceed the transcript's
    /// main-thread count) and never fires the guard. `None` means no
    /// transcript testimony, which is the pre-A202 behaviour.
    pub transcript_tool_events: Option<u64>,
    /// A202: true when this session's own record says it began as the
    /// resumption of an earlier one (`session_born_from_resume`) — the
    /// vendor mints a NEW session id on resume, so evidence from before the
    /// resume is recorded under the prior id and this session's indexed scan
    /// can never reach it. Conservative: the window is treated as
    /// incomplete even when the counts happen to balance, because sidechain
    /// deliveries can mask a main-thread deficit. False when unknown.
    pub resume_started: bool,
    /// A203: rows in the window whose tool or command falls outside the
    /// recognized evidence vocabulary — an unrecognized toolchain (`npx
    /// tsc`, `deno check`, a bespoke harness), an agent or MCP call whose
    /// response can carry arbitrary tool output, a shell command that could
    /// write files or run hooks. The gauntlet's largest mechanism (M2) was
    /// proof sitting in exactly such rows while the matcher read only
    /// canonical cargo/npm signatures and concluded "definitively empty".
    /// Any value above zero means no class's absence can be established
    /// over this window, and [`bind`] degrades accusations to silence.
    pub unaccounted_rows: u64,
    /// A203, class-scoped. `unaccounted_rows` collapsed every unreadable row
    /// into one session-wide veto, so ONE `mkdir` anywhere silenced every
    /// claim in the session. Measured before this split: `unaccounted_rows > 0`
    /// in 24 of 24 real sessions on the author's machine (1 … 1968), which made
    /// [`Binding::NoProofFound`] structurally unreachable and the Stop-hook
    /// audit a dead feature that had never written a row.
    ///
    /// These count only the rows that could plausibly carry the proof of THAT
    /// class. A shell command could run anything, so it taints all three; an
    /// Edit row whose payload named no path can hide an edit but cannot hide a
    /// test run or a commit.
    pub unaccounted_test: u64,
    pub unaccounted_commit: u64,
    pub unaccounted_files: u64,
    /// Recognised test/build invocations whose output reads as success, as
    /// `(seq, chain_hash)`. Crate-visible for the A201 verifier, which checks
    /// manifest entries against the same three classes as the audit.
    pub(crate) test_success: Vec<(i64, String)>,
    /// Files touched by Edit/Write/NotebookEdit-family tools, as
    /// `(seq, chain_hash, lowercased path)`.
    pub(crate) files_touched: Vec<(i64, String, String)>,
    /// Uninterrupted `git commit` invocations, as `(seq, chain_hash)`.
    pub(crate) commit_success: Vec<(i64, String)>,
}

impl EvidenceIndex {
    pub fn build(rows: &[EvidenceRowLite]) -> Self {
        let mut idx = EvidenceIndex {
            rows_scanned: rows.len() as u64,
            seq_lo: rows.first().map(|r| r.seq),
            seq_hi: rows.last().map(|r| r.seq),
            chain_head: rows.last().map(|r| r.chain_hash.clone()),
            ..EvidenceIndex::default()
        };
        for row in rows {
            let tool = row.tool_name.to_ascii_lowercase();
            let mut accounted = false;
            if tool == "bash" {
                let key = crate::identity::identity_key(
                    &row.tool_name,
                    &row.tool_input,
                    row.cwd.as_deref(),
                );
                let recognised = crate::identity::is_test_invocation(&key)
                    || crate::identity::is_build_invocation(&key);
                let command = crate::derive::json_string_field(&row.tool_input, "command")
                    .unwrap_or_default();
                if recognised {
                    if !row.interrupted && outcome_is_success(&row.response_tail) {
                        idx.test_success.push((row.seq, row.chain_hash.clone()));
                    }
                    // A recognized run is accounted for when its outcome is
                    // legible either way (success feeds the class; a clear
                    // failure is a counted non-proof) or when it was visibly
                    // interrupted (an incomplete run cannot hide a completed
                    // proof). A recognized run whose output the reader
                    // cannot classify could have succeeded — unaccounted.
                    accounted = row.interrupted || outcome_is_legible(&row.response_tail);
                } else if command.contains("git commit") {
                    if !row.interrupted {
                        idx.commit_success.push((row.seq, row.chain_hash.clone()));
                    }
                    // Deliberately NOT accounted: commit and push hooks run
                    // arbitrary gates, and the gauntlet held a pre-push hook
                    // whose output WAS the test proof a no-proof line
                    // denied. The row still evidences the commit class.
                } else {
                    accounted = command_is_read_only(&command);
                }
            } else if matches!(
                tool.as_str(),
                "edit" | "write" | "notebookedit" | "multiedit"
            ) {
                for field in ["file_path", "notebook_path", "path"] {
                    if let Some(p) = crate::derive::json_string_field(&row.tool_input, field) {
                        idx.files_touched.push((
                            row.seq,
                            row.chain_hash.clone(),
                            p.to_ascii_lowercase(),
                        ));
                        // Accounted only when the path was indexable: an edit
                        // row whose payload names no path could be the very
                        // edit a file claim names, unmatchable.
                        accounted = true;
                        break;
                    }
                }
            } else {
                // Read-only tools can neither prove nor hide anything the v0
                // classes look for. Everything else — Task agents, MCP
                // servers, browser drivers, tools this code has never heard
                // of — can carry arbitrary work in its response, so its
                // presence makes "definitively empty" unestablishable.
                accounted = matches!(
                    tool.as_str(),
                    "read"
                        | "glob"
                        | "grep"
                        | "ls"
                        | "websearch"
                        | "webfetch"
                        | "todowrite"
                        | "todoread"
                        | "notebookread"
                        | "killshell"
                        | "exitplanmode"
                        // Agent-side reporting verbs. They return a value or
                        // hand the user a file; neither can execute a command,
                        // reach a repository, or write into the workspace, so
                        // neither can hide a test run, a commit or an edit.
                        // Deliberately NOT added: Task, Workflow and Skill,
                        // whose work happens elsewhere and may never be
                        // recorded under this session.
                        | "structuredoutput"
                        | "senduserfile"
                ) || tool.starts_with("mcp__thruline__");
            }
            if !accounted {
                idx.unaccounted_rows += 1;
                // What could this row have been hiding? A shell command could
                // have run a test, made a commit, or written a file, so it
                // taints all three. An edit-family row that named no path can
                // hide an edit and nothing else — it executes nothing. Every
                // other unrecognized tool (Task, Workflow, an MCP server, a
                // tool this code has never heard of) can carry arbitrary work
                // in its response and taints all three.
                let edit_family = matches!(
                    tool.as_str(),
                    "edit" | "write" | "notebookedit" | "multiedit"
                );
                if edit_family {
                    idx.unaccounted_files += 1;
                } else {
                    idx.unaccounted_test += 1;
                    idx.unaccounted_commit += 1;
                    idx.unaccounted_files += 1;
                }
            }
        }
        idx
    }

    /// A202 — every reason the scanned evidence window is known incomplete,
    /// in plain words and a fixed order: spilled deliveries (A201), the
    /// transcript deficit, the resume birth. Empty means complete as far as
    /// the record can tell; any entry means [`bind`] refuses to accuse.
    pub fn window_gaps(&self) -> Vec<String> {
        let mut gaps = Vec::new();
        if self.spilled_deliveries > 0 {
            gaps.push(crate::voice::count_phrase(
                self.spilled_deliveries,
                "spilled delivery",
                "spilled deliveries",
            ));
        }
        if let Some(transcript) = self.transcript_tool_events {
            if transcript > self.rows_scanned {
                gaps.push(format!(
                    "transcript holds {}, session record holds {}",
                    crate::voice::count_phrase(transcript, "tool event", "tool events"),
                    self.rows_scanned
                ));
            }
        }
        if self.resume_started {
            gaps.push(
                "the session began as a resume, and evidence from before it is recorded \
                 under the prior session id"
                    .to_string(),
            );
        }
        gaps
    }

    /// A202: true when the transcript↔record joint itself is broken — a
    /// transcript deficit or a resume birth. Unlike a spill (which hides
    /// only sensor deliveries and cannot change what the transcript shows),
    /// a capture gap compromises the session record's account of the turn
    /// as a whole, so it also degrades "left open" promise lines to
    /// silence, not just accusations.
    pub fn capture_gap(&self) -> bool {
        self.transcript_tool_events
            .is_some_and(|t| t > self.rows_scanned)
            || self.resume_started
    }
}

/// §5d's outcome reader, applied in the silence-safe direction: only a clear
/// failure or a clear absence withholds success; a partial count is enough.
fn outcome_is_success(tail: &str) -> bool {
    use crate::premise::Outcome;
    match crate::premise::classify_outcome(tail) {
        Outcome::Tests {
            failed: Some(0), ..
        } => true,
        Outcome::Tests {
            passed: Some(_),
            failed: None,
        } => true,
        Outcome::Tests { .. } => false,
        Outcome::Succeeded => true,
        Outcome::Failed => false,
        Outcome::Unknown => false,
    }
}

/// A203: the outcome reader could classify the run one way or the other.
/// A recognized invocation whose tail reads as nothing at all could have
/// been the very success a no-proof line would deny, so it counts against
/// the window's completeness rather than for either side.
fn outcome_is_legible(tail: &str) -> bool {
    use crate::premise::Outcome;
    !matches!(
        crate::premise::classify_outcome(tail),
        Outcome::Tests {
            passed: None,
            failed: None,
        } | Outcome::Unknown
    )
}

/// A203: a shell command that can neither run a gate nor write a file — the
/// closed set of read-only operations whose presence in the window says
/// nothing about any evidence class. Compound commands are split and every
/// segment must qualify; any output redirection disqualifies the whole
/// command (a `>` can write the very file an edit claim names).
fn command_is_read_only(command: &str) -> bool {
    if command.contains('>') {
        return false;
    }
    const READ_ONLY: [&str; 35] = [
        "ls", "cat", "head", "tail", "grep", "rg", "find", "pwd", "wc", "echo", "printf", "which",
        "file", "stat", "tree", "du", "df", "diff", "sort", "uniq", "date", "env", "true", "sleep",
        "basename", "dirname", "realpath", "shasum", "md5", "cut", "tr", "nl", "column", "cd",
        "type",
    ];
    const GIT_READ_ONLY: [&str; 10] = [
        "status",
        "log",
        "diff",
        "show",
        "branch",
        "rev-parse",
        "remote",
        "ls-files",
        "blame",
        "describe",
    ];
    let flattened = command
        .replace("&&", "\n")
        .replace("||", "\n")
        .replace([';', '|'], "\n");
    for segment in flattened.lines() {
        let mut toks = segment
            .split_whitespace()
            .skip_while(|t| t.contains('=') && !t.starts_with('='));
        let Some(head) = toks.next() else {
            continue;
        };
        let head = head.rsplit('/').next().unwrap_or(head);
        if head == "git" {
            let sub = toks.next().unwrap_or("");
            if !GIT_READ_ONLY.contains(&sub) {
                return false;
            }
        } else if !READ_ONLY.contains(&head) {
            return false;
        }
    }
    true
}

/* ── binding a claim to the record ───────────────────────────────────── */

/// The three v0 verdicts. Only `NoProofFound` ever produces a line; the other
/// two are silence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Binding {
    Evidenced,
    NoProofFound {
        /// What the record was searched for and did not hold — named so the
        /// line can say what "no proof" means here.
        expected: String,
    },
    /// The claim names nothing this deterministic tier can check. Ambiguity
    /// never accuses.
    Unknown,
}

/// Deterministic evidence classes v0, behind the G1 extraction gate (A203).
///
/// A claim is checked against every class the gate admits it to; ANY match
/// anywhere in the session is enough for silence. `NoProofFound` now needs
/// three things: at least one admitted class, a window with no completeness
/// gap (A201/A202), and — new at A203 — a window whose every row the
/// recognizer could account for. The first gauntlet's largest false-
/// accusation mechanism was proof sitting in scanned rows the matcher could
/// not read; outside the recognized vocabulary, absence cannot be
/// established and the verdict degrades to silence.
pub fn bind(claim: &TurnClaim, idx: &EvidenceIndex) -> Binding {
    bind_explained(claim, idx).0
}

/// [`bind`], with the reason a verdict degraded to silence — the receipt
/// [`compose`] records so a quieter brief can still say why it is quiet.
pub fn bind_explained(claim: &TurnClaim, idx: &EvidenceIndex) -> (Binding, Option<String>) {
    if !claim.kind.is_material_work() {
        return (Binding::Unknown, None);
    }
    let classes = match crate::speaker_gate::admit_completion(&claim.text, &claim.artifacts) {
        crate::speaker_gate::GateVerdict::Refused(reason) => {
            return (Binding::Unknown, Some(reason.to_string()))
        }
        crate::speaker_gate::GateVerdict::Admitted(classes) => classes,
    };

    let mut expected = Vec::new();
    if classes.test_run {
        if !idx.test_success.is_empty() {
            return (Binding::Evidenced, None);
        }
        expected.push("a passing test or build run");
    }
    if classes.commit {
        if !idx.commit_success.is_empty() {
            return (Binding::Evidenced, None);
        }
        expected.push("a git commit");
    }
    if !classes.file_artifacts.is_empty() {
        if classes.file_artifacts.iter().any(|a| {
            idx.files_touched
                .iter()
                .any(|(_, _, path)| path_matches_artifact(path, a))
        }) {
            return (Binding::Evidenced, None);
        }
        expected.push("an edit touching what the claim names");
    }
    // The gate refuses no-class claims, so at least one class is engaged
    // here and `expected` is never empty.

    // The A201 spill guard, generalized at A202 into the window-completeness
    // guard. A delivery that spilled off the chain, a transcript holding
    // main-thread tool events the scanned window never saw, or a session
    // born from a resume (whose pre-resume evidence lives under the prior
    // session id) each mean "no proof found" cannot be established — the
    // proof may sit exactly where the window does not reach. The verdict
    // degrades to silence, and the digest records why (fail-open must not
    // fail-accuse; a gap can only hide proof, never create it, so the
    // Evidenced returns above are untouched).
    let gaps = idx.window_gaps();
    if !gaps.is_empty() {
        return (
            Binding::Unknown,
            Some(format!("evidence window incomplete: {}", gaps.join("; "))),
        );
    }
    // A203: the window is complete, but rows sit in it that the recognizer
    // cannot read — an unrecognized toolchain, an agent's response, a
    // command that could have run a gate or written a file. Any one of them
    // could hold the proof, so "definitively empty" cannot be said.
    // A203, class-scoped. The veto used to read `idx.unaccounted_rows > 0`,
    // which is a claim about the WHOLE session: one `mkdir`, one `Task`, one
    // MCP call anywhere in it silenced every claim in it. Measured on 24 of 24
    // real sessions that count was never zero, so `NoProofFound` was
    // unreachable and this feature had never written a row.
    //
    // Only rows that could have carried the proof of a class THIS claim
    // engages can make that claim's absence unestablishable. A shell command
    // taints all three classes because it could have run anything; an edit row
    // that named no path taints only the file class.
    let blinded = [
        (classes.test_run, idx.unaccounted_test),
        (classes.commit, idx.unaccounted_commit),
        (!classes.file_artifacts.is_empty(), idx.unaccounted_files),
    ]
    .iter()
    .filter(|(engaged, _)| *engaged)
    .map(|(_, n)| *n)
    .max()
    .unwrap_or(0);
    if blinded > 0 {
        return (
            Binding::Unknown,
            Some(format!(
                "{} in the scanned window outside the recognized evidence vocabulary — \
                 absence cannot be established",
                crate::voice::count_phrase(blinded, "tool event", "tool events")
            )),
        );
    }
    (
        Binding::NoProofFound {
            expected: expected.join(", or "),
        },
        None,
    )
}

/// Path ↔ artifact matching, on the touched path's basename. Tokens under
/// four characters must match the basename exactly — "rs" must not match
/// every Rust file in the tree. Crate-visible for the A201 verifier, which
/// matches manifest file entries with the same rule.
pub(crate) fn path_matches_artifact(path_lower: &str, artifact_lower: &str) -> bool {
    if artifact_lower.contains('/') {
        return path_lower.contains(artifact_lower);
    }
    let base = path_lower.rsplit('/').next().unwrap_or(path_lower);
    if base == artifact_lower {
        return true;
    }
    if artifact_lower.len() >= 4 {
        return base.contains(artifact_lower) || artifact_lower.contains(base);
    }
    false
}

/* ── the digest ──────────────────────────────────────────────────────── */

/// One rendered line of the digest, with its receipt fields explicit so the
/// sidecar can be read by machines without re-parsing prose.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestLine {
    /// `no_proof` or `open_promise` from the session's own final turn;
    /// `claim_no_proof` or `claim_open` for `claim_completion` submissions
    /// carried in from the claim loop (A201).
    pub kind: String,
    /// The claim's own words, truncated for the token budget.
    pub claim: String,
    /// Receipt: 1-based line in `transcript_path`. Absent for claim-loop
    /// lines, whose receipt is the recorded call (`why_id` reaches it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_line: Option<u64>,
    /// For `no_proof`: what the scan looked for and did not find. For
    /// claim-loop lines: the standing basis in plain words.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    /// A201: the typeable id this line answers to — `thruline why <id>`.
    /// Content-derived (`spkf_`/`spkp_`/`cvrd_`/`tlc_` plus a hash prefix),
    /// so the same line always wears the same id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why_id: Option<String>,
}

/// The per-project digest cache content. The `.md` beside it is the rendered
/// form; this sidecar carries the receipts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Digest {
    pub schema: String,
    pub project_key: String,
    pub session_id: String,
    pub transcript_path: String,
    pub generated_at: String,
    /// Which hook produced it: `session-end` or `stop`.
    pub hook: String,
    pub lines: Vec<DigestLine>,
    /// The scanned sensor window the no-proof lines cite.
    pub evidence_rows_scanned: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensor_seq_lo: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensor_seq_hi: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensor_chain_head: Option<String>,
    /// A201: deliveries for this session that never reached the chain. Above
    /// zero, no-proof verdicts were degraded to silence and
    /// `evidence_window_note` says so.
    #[serde(default)]
    pub spilled_deliveries: u64,
    /// A202: the transcript's own count of non-sidechain tool_use events,
    /// when a transcript was read — the testimony the scanned window was
    /// measured against. Absent on pre-A202 sidecars.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_tool_events: Option<u64>,
    /// A202: true when the session's first recorded SessionStart names
    /// `source: "resume"` — pre-resume evidence lives under the prior
    /// session id, so the window was treated as incomplete.
    #[serde(default)]
    pub resume_started: bool,
    /// The reason recorded when the window-completeness guard degraded the
    /// window — e.g. "evidence window incomplete: 2 spilled deliveries" or
    /// "evidence window incomplete: transcript holds 648 tool events,
    /// session record holds 0".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_window_note: Option<String>,
    /// A203: rows in the scanned window outside the recognized evidence
    /// vocabulary. Above zero, no absence is establishable and every
    /// would-be no-proof verdict degraded to silence. Absent on pre-A203
    /// sidecars.
    #[serde(default)]
    pub unaccounted_rows: u64,
    /// A203: per-claim receipts for silence — "line N: <why this material
    /// claim produced no verdict>". At most [`MAX_ABSTENTIONS`]; the honest
    /// counterpart of a much quieter brief.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub abstentions: Vec<String>,
}

pub const DIGEST_SCHEMA: &str = "thruline_digest_v0";

/// At most this many lines ever render (the spec's bound).
pub const MAX_LINES: usize = 3;

/// A203: at most this many abstention receipts the sidecar keeps.
pub const MAX_ABSTENTIONS: usize = 12;

const CLAIM_TRUNCATE: usize = 140;

/// Where a digest speaks from — the identity fields of one composition. A
/// struct rather than five positional strings, for the same reason as
/// `mcp::Entry`: a future field must not be silently transposed with its
/// neighbour at a call site.
#[derive(Debug, Clone, Copy)]
pub struct DigestScope<'a> {
    pub project_key: &'a str,
    pub session_id: &'a str,
    pub transcript_path: &'a str,
    pub generated_at: &'a str,
    pub hook: &'a str,
}

/// Compose the digest from a bound final turn. Deterministic; at most
/// [`MAX_LINES`] lines: no-proof completion claims first (transcript order),
/// then `claim_lines` carried in from the claim loop (A201 — the verifier's
/// still-unresolved `claim_completion` submissions for this project), then
/// promises left open at the end of the turn. Nothing to say → empty
/// `lines`, and the cache writer removes the file rather than storing filler.
pub fn compose(
    scope: &DigestScope<'_>,
    turn: &FinalTurn,
    idx: &EvidenceIndex,
    claim_lines: &[DigestLine],
) -> Digest {
    let DigestScope {
        project_key,
        session_id,
        transcript_path,
        generated_at,
        hook,
    } = *scope;
    let mut lines = Vec::new();
    let mut abstentions = Vec::new();
    for claim in &turn.claims {
        if !claim.kind.is_material_work() {
            continue;
        }
        match bind_explained(claim, idx) {
            (Binding::NoProofFound { expected }, _) => {
                lines.push(DigestLine {
                    kind: "no_proof".to_string(),
                    why_id: Some(short_display_id(&finding_id_parts(
                        project_key,
                        session_id,
                        transcript_path,
                        claim.line,
                        &truncate(&claim.text, CLAIM_TRUNCATE),
                    ))),
                    claim: truncate(&claim.text, CLAIM_TRUNCATE),
                    transcript_line: Some(claim.line),
                    expected: Some(expected),
                });
            }
            // A203: silence with a receipt. The sidecar keeps the reason a
            // would-be line is absent, bounded so a wordy turn cannot bloat
            // the cache file.
            (Binding::Unknown, Some(reason)) if abstentions.len() < MAX_ABSTENTIONS => {
                abstentions.push(format!("line {}: {}", claim.line, reason));
            }
            _ => {}
        }
    }
    lines.extend(claim_lines.iter().cloned());
    // A202: a capture gap (transcript deficit or resume birth) compromises
    // the record's account of the turn itself, so even the factual "left
    // open" statement stands on ground the record cannot vouch for —
    // promise lines degrade to silence with the accusations. A spill alone
    // does NOT degrade promises (pinned at A201): promises bind against the
    // transcript only, and a spilled sensor delivery cannot change what the
    // transcript shows.
    let gapped = idx.capture_gap();
    for claim in &turn.claims {
        if gapped || claim.kind != ClaimKind::Proposal {
            continue;
        }
        // A203 task 3: only a forward-looking first-person commitment may
        // become a promise line. The gauntlet bound past-tense event
        // reports, "Now …" state descriptions, and an imperative addressed
        // to the user as open promises — all false; none survives this gate.
        if !crate::speaker_gate::admissible_promise(&claim.text, claim.marker) {
            continue;
        }
        // A promise is "left open" only when the record shows nothing after
        // it. Any later tool activity → silence: fulfilment quality is not
        // judgeable by this tier, and silence never wrongs anyone.
        let followed = turn.last_tool_line.is_some_and(|t| t > claim.line);
        if !followed {
            lines.push(DigestLine {
                kind: "open_promise".to_string(),
                why_id: Some(short_display_id(&promise_id_parts(
                    project_key,
                    session_id,
                    transcript_path,
                    claim.line,
                    &truncate(&claim.text, CLAIM_TRUNCATE),
                ))),
                claim: truncate(&claim.text, CLAIM_TRUNCATE),
                transcript_line: Some(claim.line),
                expected: None,
            });
        }
    }
    lines.truncate(MAX_LINES);
    Digest {
        schema: DIGEST_SCHEMA.to_string(),
        project_key: project_key.to_string(),
        session_id: session_id.to_string(),
        transcript_path: transcript_path.to_string(),
        generated_at: generated_at.to_string(),
        hook: hook.to_string(),
        lines,
        evidence_rows_scanned: idx.rows_scanned,
        sensor_seq_lo: idx.seq_lo,
        sensor_seq_hi: idx.seq_hi,
        sensor_chain_head: idx.chain_head.clone(),
        spilled_deliveries: idx.spilled_deliveries,
        transcript_tool_events: idx.transcript_tool_events,
        resume_started: idx.resume_started,
        // Recorded whenever the window is known incomplete, whether or not a
        // claim would have been accused: it is a fact about the scan basis,
        // and it is the receipt for why a would-be no-proof line is absent.
        evidence_window_note: {
            let gaps = idx.window_gaps();
            (!gaps.is_empty()).then(|| format!("evidence window incomplete: {}", gaps.join("; ")))
        },
        unaccounted_rows: idx.unaccounted_rows,
        abstentions,
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

/// Render the digest as the ≤3-line brief. The header names the transcript
/// (the file every line's receipt points into); each line carries its line
/// number and, for no-proof lines, the scanned sensor window — and, since
/// A201, the typeable id `thruline why <id>` answers for.
pub fn render_markdown(digest: &Digest) -> String {
    if digest.lines.is_empty() {
        return String::new();
    }
    let file = Path::new(&digest.transcript_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&digest.transcript_path);
    let window = match (digest.sensor_seq_lo, digest.sensor_seq_hi) {
        (Some(lo), Some(hi)) => format!(
            "scanned {} in the session record (sensor seq {lo}–{hi})",
            crate::voice::count_phrase(digest.evidence_rows_scanned, "tool event", "tool events"),
        ),
        _ => "the session record holds no tool events".to_string(),
    };
    let mut out = if digest.transcript_path.is_empty() {
        "THRULINE — what the record shows for this project\n".to_string()
    } else {
        format!(
            "THRULINE — what the previous session's own record shows ({})\n",
            digest.transcript_path
        )
    };
    for line in &digest.lines {
        let receipt = match line.transcript_line {
            Some(n) => format!(" · receipt {file}:{n}"),
            None => String::new(),
        };
        let why = match line.why_id.as_deref() {
            Some(id) => format!(" · why {id}"),
            None => String::new(),
        };
        match line.kind.as_str() {
            "no_proof" => {
                let expected = line.expected.as_deref().unwrap_or("matching tool evidence");
                out.push_str(&format!(
                    "- claimed, and no proof found in the record: \"{}\" — looked for {expected}; {window}{receipt}{why}\n",
                    line.claim
                ));
            }
            "claim_no_proof" => {
                let basis = line
                    .expected
                    .as_deref()
                    .unwrap_or("the record was searched and held no match");
                out.push_str(&format!(
                    "- claimed as finished, and no proof found in the record: \"{}\" — {basis}{receipt}{why}\n",
                    line.claim
                ));
            }
            "claim_open" => {
                let basis = line
                    .expected
                    .as_deref()
                    .unwrap_or("submitted for verification; no proof recorded yet");
                out.push_str(&format!(
                    "- claimed as finished, and still no proof recorded: \"{}\" — {basis}{receipt}{why}\n",
                    line.claim
                ));
            }
            _ => {
                out.push_str(&format!(
                    "- left open when the session ended: \"{}\"{receipt}{why}\n",
                    line.claim
                ));
            }
        }
    }
    out
}

/* ── the cache files ─────────────────────────────────────────────────── */

/// Write (or clear) the per-project cache: `<key>.md` rendered, `<key>.json`
/// receipts. An empty digest REMOVES both — absence is the honest form of
/// "nothing to say", and it is what keeps healthy runs invisible.
pub fn write_cache(dir: &Path, digest: &Digest) -> Result<()> {
    let md_path = dir.join(format!("{}.md", digest.project_key));
    let json_path = dir.join(format!("{}.json", digest.project_key));
    if digest.lines.is_empty() {
        let _ = std::fs::remove_file(&md_path);
        let _ = std::fs::remove_file(&json_path);
        return Ok(());
    }
    std::fs::create_dir_all(dir).map_err(|e| CoreError::io(dir, e))?;
    let md = render_markdown(digest);
    let json = serde_json::to_string_pretty(digest)?;
    atomic_write(&md_path, md.as_bytes())?;
    atomic_write(&json_path, json.as_bytes())?;
    Ok(())
}

/// Write-then-rename, so a brief racing this writer reads a whole file or the
/// previous one — never half of each.
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(|e| CoreError::io(&tmp, e))?;
    std::fs::rename(&tmp, path).map_err(|e| CoreError::io(path, e))
}

/// The rendered brief for a project, when one exists and says anything.
pub fn read_cache_markdown(dir: &Path, project_key: &str) -> Option<String> {
    let path = dir.join(format!("{}.md", sanitize_key(project_key)));
    let text = std::fs::read_to_string(path).ok()?;
    if text.trim().is_empty() {
        return None;
    }
    Some(text)
}

/// The receipts sidecar for a project, parsed. `None` when absent or
/// unreadable — callers treat that as "nothing cached", never as an error.
pub fn read_cache_digest(dir: &Path, project_key: &str) -> Option<Digest> {
    let path = dir.join(format!("{}.json", sanitize_key(project_key)));
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Every parseable digest sidecar in the cache directory — `thruline why`'s
/// fallback namespace for line ids that never reached the ledger (a
/// SessionEnd digest is cached without being announced).
pub fn all_cache_digests(dir: &Path) -> Vec<Digest> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        if let Some(digest) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
        {
            out.push(digest);
        }
    }
    out
}

/* ── the announce-to-ledger record (speaker_findings) ────────────────── */

/// Deterministic finding identity: the same claim on the same transcript line
/// audited twice is one finding, not two — Stop can fire more than once for a
/// turn, and an idempotent id is what keeps the append-only table honest.
/// The material is unchanged from A200, so ids already on a ledger stay
/// valid.
pub fn finding_id_parts(
    project_key: &str,
    session_id: &str,
    transcript_path: &str,
    transcript_line: u64,
    claim: &str,
) -> String {
    let material =
        format!("{project_key}\n{session_id}\n{transcript_path}\n{transcript_line}\n{claim}");
    format!("spkf_{}", sha256_hex(material.as_bytes()))
}

/// A left-open promise's line identity — same material shape as a finding,
/// distinct prefix, because a promise is not an accusation and never lands
/// in `speaker_findings`; `thruline why` answers for it from the digest
/// sidecar.
pub fn promise_id_parts(
    project_key: &str,
    session_id: &str,
    transcript_path: &str,
    transcript_line: u64,
    claim: &str,
) -> String {
    let material =
        format!("{project_key}\n{session_id}\n{transcript_path}\n{transcript_line}\n{claim}");
    format!("spkp_{}", sha256_hex(material.as_bytes()))
}

/// The typeable form of a content-derived id: the prefix plus the first 12
/// hex characters. `thruline why` resolves prefixes, so this is enough to
/// type while the full hash stays the stored identity.
pub fn short_display_id(full: &str) -> String {
    match full.split_once('_') {
        Some((prefix, hex)) => {
            let cut: String = hex.chars().take(12).collect();
            format!("{prefix}_{cut}")
        }
        None => full.chars().take(12).collect(),
    }
}

/// The finding identity for a digest line (see [`finding_id_parts`]).
/// `None` for a line with no transcript receipt — such a line is not a
/// session-turn finding and may not become one.
pub fn finding_id(digest: &Digest, line: &DigestLine) -> Option<String> {
    let transcript_line = line.transcript_line?;
    Some(finding_id_parts(
        &digest.project_key,
        &digest.session_id,
        &digest.transcript_path,
        transcript_line,
        &line.claim,
    ))
}

/// The fields each finding's chain link hashes — recomputable from the row's
/// own columns, so `verify_findings_chain` needs no stored blob.
struct FindingContent<'a> {
    hook: &'a str,
    session_id: &'a str,
    project_key: &'a str,
    transcript_path: &'a str,
    transcript_line: u64,
    claim_kind: &'a str,
    claim_marker: &'a str,
    claim_text: &'a str,
    expected: &'a str,
    rows_scanned: u64,
    seq_lo: Option<i64>,
    seq_hi: Option<i64>,
}

impl FindingContent<'_> {
    fn canonical(&self) -> String {
        format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            self.hook,
            self.session_id,
            self.project_key,
            self.transcript_path,
            self.transcript_line,
            self.claim_kind,
            self.claim_marker,
            self.claim_text,
            self.expected,
            self.rows_scanned,
            self.seq_lo.map(|v| v.to_string()).unwrap_or_default(),
            self.seq_hi.map(|v| v.to_string()).unwrap_or_default(),
        )
    }
}

/// Append the digest's no-proof lines to `speaker_findings`. Idempotent by
/// [`finding_id`]; holds one short IMMEDIATE transaction (A199 finding 2) and
/// gives up quietly on a busy ledger rather than queueing behind a long
/// writer. Returns how many rows were actually appended.
pub fn record_findings(
    ledger: &Ledger,
    digest: &Digest,
    turn: &FinalTurn,
    recorded_at: &str,
) -> Result<u64> {
    let no_proof: Vec<&DigestLine> = digest
        .lines
        .iter()
        .filter(|l| l.kind == "no_proof")
        .collect();
    if no_proof.is_empty() {
        return Ok(0);
    }
    let conn = ledger.conn();
    conn.busy_timeout(std::time::Duration::from_millis(800))?;
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let outcome = (|| -> Result<u64> {
        let mut appended = 0u64;
        for line in no_proof {
            // A no-proof line without a transcript receipt cannot exist by
            // rule 2; if one ever appeared it may not become a finding.
            let (Some(id), Some(line_no)) = (finding_id(digest, line), line.transcript_line) else {
                continue;
            };
            let exists: Option<i64> = conn
                .query_row("SELECT 1 FROM speaker_findings WHERE id = ?1", [&id], |r| {
                    r.get(0)
                })
                .optional()?;
            if exists.is_some() {
                continue;
            }
            // The claim's kind/marker, recovered from the turn by line+text so
            // the row states which rule extracted it.
            let (kind, marker) = turn
                .claims
                .iter()
                .find(|c| c.line == line_no && c.text.starts_with(line.claim.trim_end_matches('…')))
                .map(|c| (c.kind.as_str(), c.marker.as_str()))
                .unwrap_or(("work", "completion"));
            let head: Option<(String, i64)> = conn
                .query_row(
                    "SELECT chain_hash, seq FROM speaker_findings ORDER BY seq DESC LIMIT 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let (prev_hash, seq) = match head {
                Some((hash, last)) => (hash, last + 1),
                None => (genesis_prev_hash(CHAIN_SCOPE), 0),
            };
            let expected = line.expected.as_deref().unwrap_or("");
            let content = FindingContent {
                hook: &digest.hook,
                session_id: &digest.session_id,
                project_key: &digest.project_key,
                transcript_path: &digest.transcript_path,
                transcript_line: line_no,
                claim_kind: kind,
                claim_marker: marker,
                claim_text: &line.claim,
                expected,
                rows_scanned: digest.evidence_rows_scanned,
                seq_lo: digest.sensor_seq_lo,
                seq_hi: digest.sensor_seq_hi,
            }
            .canonical();
            let content_sha256 = sha256_hex(content.as_bytes());
            let chain = chain_hash(&prev_hash, &content_sha256, CHAIN_SCOPE, seq);
            conn.execute(
                "INSERT INTO speaker_findings(
                     id, seq, recorded_at, hook, session_id, project_key,
                     transcript_path, transcript_line, claim_kind, claim_marker,
                     claim_text, expected_evidence, evidence_rows_scanned,
                     sensor_seq_lo, sensor_seq_hi, content_sha256, prev_hash, chain_hash)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
                rusqlite::params![
                    id,
                    seq,
                    recorded_at,
                    digest.hook,
                    digest.session_id,
                    digest.project_key,
                    digest.transcript_path,
                    line_no as i64,
                    kind,
                    marker,
                    line.claim,
                    expected,
                    digest.evidence_rows_scanned as i64,
                    digest.sensor_seq_lo,
                    digest.sensor_seq_hi,
                    content_sha256,
                    prev_hash,
                    chain,
                ],
            )?;
            appended += 1;
        }
        Ok(appended)
    })();
    match outcome {
        Ok(n) => {
            conn.execute_batch("COMMIT")?;
            Ok(n)
        }
        Err(err) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(err)
        }
    }
}

/// Re-fold the findings chain link by link; any mismatch names the seq.
pub fn verify_findings_chain(ledger: &Ledger) -> Result<u64> {
    let conn = ledger.conn();
    let mut stmt = conn.prepare(
        "SELECT seq, hook, session_id, project_key, transcript_path, transcript_line,
                claim_kind, claim_marker, claim_text, expected_evidence,
                evidence_rows_scanned, sensor_seq_lo, sensor_seq_hi,
                content_sha256, prev_hash, chain_hash
         FROM speaker_findings ORDER BY seq",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            (
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
                r.get::<_, String>(9)?,
                r.get::<_, i64>(10)?,
            ),
            (
                r.get::<_, Option<i64>>(11)?,
                r.get::<_, Option<i64>>(12)?,
                r.get::<_, String>(13)?,
                r.get::<_, String>(14)?,
                r.get::<_, String>(15)?,
            ),
        ))
    })?;
    let mut prev = genesis_prev_hash(CHAIN_SCOPE);
    let mut checked = 0u64;
    for row in rows {
        let (seq, fields, tail) = row?;
        let (
            hook,
            session_id,
            project_key,
            transcript_path,
            line,
            kind,
            marker,
            claim,
            expected,
            scanned,
        ) = fields;
        let (seq_lo, seq_hi, stored_sha, stored_prev, stored_chain) = tail;
        let content = FindingContent {
            hook: &hook,
            session_id: &session_id,
            project_key: &project_key,
            transcript_path: &transcript_path,
            transcript_line: line.max(0) as u64,
            claim_kind: &kind,
            claim_marker: &marker,
            claim_text: &claim,
            expected: &expected,
            rows_scanned: scanned.max(0) as u64,
            seq_lo,
            seq_hi,
        }
        .canonical();
        let sha = sha256_hex(content.as_bytes());
        let expect_chain = chain_hash(&prev, &sha, CHAIN_SCOPE, seq);
        if sha != stored_sha || stored_prev != prev || stored_chain != expect_chain {
            return Err(CoreError::Integrity(format!(
                "speaker findings chain does not add up at seq {seq}"
            )));
        }
        prev = stored_chain;
        checked += 1;
    }
    Ok(checked)
}
