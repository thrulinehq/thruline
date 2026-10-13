//! A201 — the claim-loop verifier.
//!
//! `claim_completion` records submissions as `ready_for_verification` and its
//! response is forbidden from being a verdict. This module is the verdict's
//! only source: a deterministic pass over each submission's evidence
//! manifest, checked against the claiming session's own capture record and
//! the workspace, with the result appended to `claim_verdicts` — never
//! written onto the claim row, which is append-only and stays exactly as
//! submitted.
//!
//! The same three rules that bind the speaker bind this pass:
//!
//! 1. **"No proof found" is never "a lie found"** (A199, POSITIONING). A
//!    `no_proof_found` verdict states that the searched window held no match;
//!    the claim may be entirely true and merely unshown.
//! 2. **Every verdict carries its basis**: which session the evidence was
//!    read from and how it was identified, the scanned window (seq range),
//!    per-entry receipts, and the spill count the window was guarded against.
//! 3. **Ambiguity never accuses.** An unidentifiable claiming session, an
//!    entry the deterministic tier cannot check, or an incomplete evidence
//!    window (the A201 spill guard, and A202's resume-birth signal — a
//!    claiming session born from a resume holds pre-resume evidence under
//!    its prior session id) each degrade to `unknown` — silence with a
//!    recorded reason, not a softer accusation.
//!
//! ## How a claim finds its session
//!
//! `mcp_calls` stores no session. But the claiming agent's own client
//! delivered the tool call to the capture layer as a PostToolUse row whose
//! `tool_input` is byte-for-byte the call's argument object, so
//! `sha256(canonical(tool_input))` reproduces `mcp_calls.request_sha256`
//! exactly (the A126 identity rule: exact, never proximity). One matching
//! delivery — or several that agree — names the session; zero, or several
//! that disagree, is an abstention.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::speaker::{self, DigestLine, EvidenceIndex};

/// The findings chain scope for verdict rows.
const CHAIN_SCOPE: &str = "claim_verdict";

/// The three verdicts. Storage slugs; the display phrase lives in
/// [`ClaimVerdict::phrase`] so engine vocabulary never reaches a surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimVerdict {
    Supported,
    NoProofFound,
    Unknown,
}

impl ClaimVerdict {
    pub fn as_str(self) -> &'static str {
        match self {
            ClaimVerdict::Supported => "supported",
            ClaimVerdict::NoProofFound => "no_proof_found",
            ClaimVerdict::Unknown => "unknown",
        }
    }

    /// The first-level phrase (UX-DOCTRINE): plain words, no enum labels.
    pub fn phrase(self) -> &'static str {
        match self {
            ClaimVerdict::Supported => "proof found in the record",
            ClaimVerdict::NoProofFound => "no proof found in the record",
            ClaimVerdict::Unknown => "the record cannot decide",
        }
    }

    fn from_str(s: &str) -> Option<ClaimVerdict> {
        match s {
            "supported" => Some(ClaimVerdict::Supported),
            "no_proof_found" => Some(ClaimVerdict::NoProofFound),
            "unknown" => Some(ClaimVerdict::Unknown),
            _ => None,
        }
    }
}

/* ── the manifest, parsed generously ─────────────────────────────────── */

/// One entry from the claimant's evidence manifest, reduced to the three
/// deterministic classes the audit already checks — plus the honest fourth:
/// a thing this tier cannot check, which can only ever produce silence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestEntry {
    /// A test or build command claimed to have run green.
    Test { command: String },
    /// A commit claimed to exist. Checked class-level against the session's
    /// uninterrupted `git commit` invocations; the id is carried as the
    /// entry's name, not matched byte-for-byte (the record stores output
    /// tails, not refs).
    Commit { id: String },
    /// A file claimed changed, optionally at a specific content hash.
    File {
        path: String,
        sha256: Option<String>,
    },
    /// Named in the manifest, not checkable by this tier. Never accused.
    Unchecked { name: String },
}

/// Parse a submitted manifest object. Total and generous: every recognised
/// key contributes entries, every unrecognised key becomes an `Unchecked`
/// entry (named, never dropped — Rule 12), and a non-object manifest yields
/// nothing checkable.
pub fn parse_manifest(manifest: &Value) -> Vec<ManifestEntry> {
    let Some(map) = manifest.as_object() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (key, value) in map {
        match key.as_str() {
            "tests" | "commands_run" => {
                for v in strings_of(value) {
                    let probe = crate::identity::identity_key("Bash", &command_input(&v), None);
                    if crate::identity::is_test_invocation(&probe)
                        || crate::identity::is_build_invocation(&probe)
                    {
                        out.push(ManifestEntry::Test { command: v });
                    } else {
                        out.push(ManifestEntry::Unchecked {
                            name: format!("{key}: {v}"),
                        });
                    }
                }
            }
            "commit" | "commits" => {
                for v in strings_of(value) {
                    out.push(ManifestEntry::Commit { id: v });
                }
            }
            "files" | "files_changed" => {
                if let Some(items) = value.as_array() {
                    for item in items {
                        match item {
                            Value::String(p) => out.push(ManifestEntry::File {
                                path: p.clone(),
                                sha256: None,
                            }),
                            Value::Object(o) => {
                                if let Some(p) = o.get("path").and_then(Value::as_str) {
                                    out.push(ManifestEntry::File {
                                        path: p.to_string(),
                                        sha256: o
                                            .get("sha256")
                                            .and_then(Value::as_str)
                                            .map(str::to_lowercase),
                                    });
                                } else {
                                    out.push(ManifestEntry::Unchecked {
                                        name: format!("{key}: {item}"),
                                    });
                                }
                            }
                            other => out.push(ManifestEntry::Unchecked {
                                name: format!("{key}: {other}"),
                            }),
                        }
                    }
                } else if let Some(p) = value.as_str() {
                    out.push(ManifestEntry::File {
                        path: p.to_string(),
                        sha256: None,
                    });
                } else {
                    out.push(ManifestEntry::Unchecked {
                        name: format!("{key}: {value}"),
                    });
                }
            }
            _ => out.push(ManifestEntry::Unchecked {
                name: format!("{key}: {value}"),
            }),
        }
    }
    out
}

fn strings_of(value: &Value) -> Vec<String> {
    match value {
        Value::String(s) => vec![s.clone()],
        Value::Array(items) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn command_input(command: &str) -> String {
    serde_json::json!({ "command": command }).to_string()
}

/* ── evaluation ──────────────────────────────────────────────────────── */

/// How the claiming session was (or was not) identified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionBasis {
    /// Exactly one session's capture record delivered this call's argument
    /// hash. `seq`/`chain_hash` receipt the identifying delivery.
    Identified {
        session_id: String,
        seq: i64,
        chain_hash: String,
    },
    /// No chained delivery carries this call's argument hash — the claiming
    /// session may predate the capture hooks, or its delivery spilled.
    NotOnRecord,
    /// More than one session delivered the identical call. Unidentifiable
    /// from what is recorded; an abstention, never a guess.
    Ambiguous { sessions: usize },
}

/// One checked manifest entry, with its receipt. Serialized into the verdict
/// row's `detail_json` so `thruline why` can replay the check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryResult {
    /// `test` / `commit` / `file` / `unchecked`.
    pub class: String,
    /// The entry's own words (command, commit id, path).
    pub value: String,
    /// `supported` / `no_proof_found` / `unknown`.
    pub outcome: String,
    /// What the outcome rests on — a capture-row receipt, a workspace hash,
    /// or the reason nothing could be decided.
    pub basis: String,
}

/// The full evaluation of one claim, before storage.
#[derive(Debug, Clone)]
pub struct ClaimEvaluation {
    pub verdict: ClaimVerdict,
    pub reason: String,
    pub entries: Vec<EntryResult>,
    pub session: SessionBasis,
    pub rows_scanned: u64,
    pub seq_lo: Option<i64>,
    pub seq_hi: Option<i64>,
    pub spilled_deliveries: u64,
}

/// Evaluate one manifest against a session's evidence index (when the
/// session is known) and the workspace. Deterministic; never writes.
pub fn evaluate(
    manifest: Option<&Value>,
    session: SessionBasis,
    idx: Option<&EvidenceIndex>,
    cwd: Option<&str>,
) -> ClaimEvaluation {
    let entries = manifest.map(parse_manifest).unwrap_or_default();
    let spilled = idx.map(|i| i.spilled_deliveries).unwrap_or(0);
    // A202: the A201 spill guard generalized — every known gap in the
    // scanned window (spilled deliveries, a resume-born claiming session)
    // joins one phrase, and any gap turns per-entry absence into abstention.
    let gaps = idx.map(EvidenceIndex::window_gaps).unwrap_or_default();
    let window_gap = (!gaps.is_empty()).then(|| gaps.join("; "));
    let mut results = Vec::new();
    for entry in &entries {
        results.push(check_entry(entry, idx, cwd, window_gap.as_deref()));
    }

    let (verdict, reason) = fold(&results, &session, window_gap.as_deref());
    ClaimEvaluation {
        verdict,
        reason,
        entries: results,
        session,
        rows_scanned: idx.map(|i| i.rows_scanned).unwrap_or(0),
        seq_lo: idx.and_then(|i| i.seq_lo),
        seq_hi: idx.and_then(|i| i.seq_hi),
        spilled_deliveries: spilled,
    }
}

fn check_entry(
    entry: &ManifestEntry,
    idx: Option<&EvidenceIndex>,
    cwd: Option<&str>,
    window_gap: Option<&str>,
) -> EntryResult {
    // The absence phrase every unproved entry shares. "In the record" means
    // the identified session's chained deliveries — named so the receipt
    // states what was searched, not merely that a search happened.
    let no_proof = |class: &str, value: &str, looked_for: &str| EntryResult {
        class: class.to_string(),
        value: value.to_string(),
        outcome: "no_proof_found".to_string(),
        basis: format!("looked for {looked_for}; none in the session record"),
    };
    // The window-completeness guard (A201 spills, A202 resume birth),
    // applied per entry: over an incomplete window the absence cannot be
    // established, so the entry abstains and says why.
    let guard = |mut r: EntryResult| -> EntryResult {
        if r.outcome == "no_proof_found" {
            if let Some(gap) = window_gap {
                r.outcome = "unknown".to_string();
                r.basis =
                    format!("evidence window incomplete: {gap} — absence cannot be established");
            }
        }
        r
    };
    let unknown_no_session = |class: &str, value: &str| EntryResult {
        class: class.to_string(),
        value: value.to_string(),
        outcome: "unknown".to_string(),
        basis: "the claiming session's record was not identified, so absence cannot be \
                established"
            .to_string(),
    };

    match entry {
        ManifestEntry::Test { command } => match idx {
            Some(idx) => match idx.test_success.first() {
                Some((seq, chain)) => EntryResult {
                    class: "test".to_string(),
                    value: command.clone(),
                    outcome: "supported".to_string(),
                    basis: format!(
                        "a recognised test or build run reading as success at capture seq \
                         {seq} (chain {chain})"
                    ),
                },
                None => guard(no_proof("test", command, "a passing test or build run")),
            },
            None => unknown_no_session("test", command),
        },
        ManifestEntry::Commit { id } => match idx {
            Some(idx) => match idx.commit_success.first() {
                Some((seq, chain)) => EntryResult {
                    class: "commit".to_string(),
                    value: id.clone(),
                    outcome: "supported".to_string(),
                    basis: format!(
                        "an uninterrupted git commit at capture seq {seq} (chain {chain})"
                    ),
                },
                None => guard(no_proof("commit", id, "a git commit")),
            },
            None => unknown_no_session("commit", id),
        },
        ManifestEntry::File { path, sha256 } => {
            let lowered = path.to_lowercase();
            if let Some(idx) = idx {
                if let Some((seq, chain, touched)) = idx
                    .files_touched
                    .iter()
                    .find(|(_, _, p)| speaker::path_matches_artifact(p, &lowered))
                {
                    return EntryResult {
                        class: "file".to_string(),
                        value: path.clone(),
                        outcome: "supported".to_string(),
                        basis: format!(
                            "an edit touching {touched} at capture seq {seq} (chain {chain})"
                        ),
                    };
                }
            }
            // The workspace check needs no session: a manifest hash matching
            // the file on disk is proof regardless of which session made it.
            if let Some(expect) = sha256 {
                if let Some(actual) = workspace_sha256(path, cwd) {
                    if &actual == expect {
                        return EntryResult {
                            class: "file".to_string(),
                            value: path.clone(),
                            outcome: "supported".to_string(),
                            basis: format!(
                                "the workspace file matches the manifest hash \
                                            (sha256 {actual})"
                            ),
                        };
                    }
                }
            }
            match idx {
                Some(_) => guard(no_proof(
                    "file",
                    path,
                    "an edit touching what the entry names, or a workspace file matching \
                     its hash",
                )),
                None => unknown_no_session("file", path),
            }
        }
        ManifestEntry::Unchecked { name } => EntryResult {
            class: "unchecked".to_string(),
            value: name.clone(),
            outcome: "unknown".to_string(),
            basis: "nothing this deterministic tier can check".to_string(),
        },
    }
}

/// Hash a manifest file path against the workspace. Relative paths resolve
/// against the claiming session's cwd when one is on record; otherwise they
/// cannot be read and the check abstains.
fn workspace_sha256(path: &str, cwd: Option<&str>) -> Option<String> {
    let candidate = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        PathBuf::from(cwd?).join(path)
    };
    let bytes = std::fs::read(candidate).ok()?;
    Some(sha256_hex(&bytes))
}

fn fold(
    results: &[EntryResult],
    session: &SessionBasis,
    window_gap: Option<&str>,
) -> (ClaimVerdict, String) {
    if results.is_empty() {
        return (
            ClaimVerdict::Unknown,
            "the manifest names nothing checkable".to_string(),
        );
    }
    let unproved: Vec<&EntryResult> = results
        .iter()
        .filter(|r| r.outcome == "no_proof_found")
        .collect();
    if !unproved.is_empty() {
        let named: Vec<String> = unproved
            .iter()
            .map(|r| format!("{} {:?}", r.class, r.value))
            .collect();
        return (
            ClaimVerdict::NoProofFound,
            format!("no proof found in the record for {}", named.join(", ")),
        );
    }
    if results.iter().all(|r| r.outcome == "supported") {
        return (
            ClaimVerdict::Supported,
            format!(
                "every manifest entry matched evidence ({})",
                crate::voice::count_phrase(results.len() as u64, "entry", "entries")
            ),
        );
    }
    // A mix of supported and unknown, or all unknown: the record cannot
    // decide, and the reason names the strongest cause.
    let reason = if let Some(gap) = window_gap {
        format!("evidence window incomplete: {gap}")
    } else {
        match session {
            SessionBasis::NotOnRecord => {
                "the claiming session's record was not identified".to_string()
            }
            SessionBasis::Ambiguous { sessions } => format!(
                "{} delivered the identical call; the claiming session cannot be told apart",
                crate::voice::count_phrase(*sessions as u64, "session", "sessions")
            ),
            SessionBasis::Identified { .. } => {
                let unchecked = results.iter().filter(|r| r.outcome == "unknown").count();
                format!(
                    "{} not checkable by this tier",
                    crate::voice::count_phrase(unchecked as u64, "entry", "entries")
                )
            }
        }
    };
    (ClaimVerdict::Unknown, reason)
}

/* ── storage: the append-only verdict chain ──────────────────────────── */

/// Deterministic verdict identity: the same claim evaluated against the same
/// record state lands on the same id, so a re-run appends nothing. The
/// recording time is deliberately NOT part of the identity — a verdict is a
/// fact about the record, not an action taken at a time — and a changed
/// record produces a changed detail, hence a new row.
pub fn verdict_id(call_id: &str, evaluation: &ClaimEvaluation, detail_json: &str) -> String {
    let material = format!(
        "{call_id}\n{}\n{}\n{detail_json}",
        evaluation.verdict.as_str(),
        evaluation.reason
    );
    format!("cvrd_{}", sha256_hex(material.as_bytes()))
}

/// The typeable form of a claim (call) id: `tlc_` plus the first 12 hex
/// characters of the call id — the same prefix-resolution contract as every
/// other truth-loop id.
pub fn claim_display_id(call_id: &str) -> String {
    let cut: String = call_id.chars().take(12).collect();
    format!("tlc_{cut}")
}

/// The canonical content of one verdict row, recomputable from its own
/// columns — the same rule as `speaker_findings`, so chain verification
/// needs no stored blob.
struct VerdictContent<'a> {
    call_id: &'a str,
    verdict: &'a str,
    reason: &'a str,
    detail_json: &'a str,
    session_id: Option<&'a str>,
    rows_scanned: u64,
    seq_lo: Option<i64>,
    seq_hi: Option<i64>,
    spilled: u64,
}

impl VerdictContent<'_> {
    fn canonical(&self) -> String {
        format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            self.call_id,
            self.verdict,
            self.reason,
            self.detail_json,
            self.session_id.unwrap_or(""),
            self.rows_scanned,
            self.seq_lo.map(|v| v.to_string()).unwrap_or_default(),
            self.seq_hi.map(|v| v.to_string()).unwrap_or_default(),
            self.spilled,
        )
    }
}

/// One stored verdict row, as read back.
#[derive(Debug, Clone)]
pub struct VerdictRow {
    pub id: String,
    pub seq: i64,
    pub recorded_at: String,
    pub call_id: String,
    pub verdict: ClaimVerdict,
    pub reason: String,
    pub detail_json: String,
    pub session_id: Option<String>,
    pub evidence_rows_scanned: u64,
    pub sensor_seq_lo: Option<i64>,
    pub sensor_seq_hi: Option<i64>,
    pub spilled_deliveries: u64,
    pub chain_hash: String,
}

/// Append one evaluation. Idempotent by [`verdict_id`]; returns whether a
/// row was actually written.
pub fn record_verdict(
    ledger: &Ledger,
    call_id: &str,
    evaluation: &ClaimEvaluation,
    recorded_at: &str,
) -> Result<bool> {
    let detail_json = serde_json::to_string(&evaluation.entries)?;
    let id = verdict_id(call_id, evaluation, &detail_json);
    let conn = ledger.conn();
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let outcome = (|| -> Result<bool> {
        let exists: Option<i64> = conn
            .query_row("SELECT 1 FROM claim_verdicts WHERE id = ?1", [&id], |r| {
                r.get(0)
            })
            .optional()?;
        if exists.is_some() {
            return Ok(false);
        }
        let head: Option<(String, i64)> = conn
            .query_row(
                "SELECT chain_hash, seq FROM claim_verdicts ORDER BY seq DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (prev_hash, seq) = match head {
            Some((hash, last)) => (hash, last + 1),
            None => (genesis_prev_hash(CHAIN_SCOPE), 0),
        };
        let session_id = match &evaluation.session {
            SessionBasis::Identified { session_id, .. } => Some(session_id.as_str()),
            _ => None,
        };
        let content = VerdictContent {
            call_id,
            verdict: evaluation.verdict.as_str(),
            reason: &evaluation.reason,
            detail_json: &detail_json,
            session_id,
            rows_scanned: evaluation.rows_scanned,
            seq_lo: evaluation.seq_lo,
            seq_hi: evaluation.seq_hi,
            spilled: evaluation.spilled_deliveries,
        }
        .canonical();
        let content_sha256 = sha256_hex(content.as_bytes());
        let chain = chain_hash(&prev_hash, &content_sha256, CHAIN_SCOPE, seq);
        conn.execute(
            "INSERT INTO claim_verdicts(
                 id, seq, recorded_at, call_id, verdict, reason, detail_json,
                 session_id, evidence_rows_scanned, sensor_seq_lo, sensor_seq_hi,
                 spilled_deliveries, content_sha256, prev_hash, chain_hash)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            rusqlite::params![
                id,
                seq,
                recorded_at,
                call_id,
                evaluation.verdict.as_str(),
                evaluation.reason,
                detail_json,
                session_id,
                evaluation.rows_scanned as i64,
                evaluation.seq_lo,
                evaluation.seq_hi,
                evaluation.spilled_deliveries as i64,
                content_sha256,
                prev_hash,
                chain,
            ],
        )?;
        Ok(true)
    })();
    match outcome {
        Ok(appended) => {
            conn.execute_batch("COMMIT")?;
            Ok(appended)
        }
        Err(err) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(err)
        }
    }
}

/// Every stored verdict, oldest first.
pub fn verdict_rows(ledger: &Ledger) -> Result<Vec<VerdictRow>> {
    let conn = ledger.conn();
    let mut stmt = conn.prepare(
        "SELECT id, seq, recorded_at, call_id, verdict, reason, detail_json,
                session_id, evidence_rows_scanned, sensor_seq_lo, sensor_seq_hi,
                spilled_deliveries, chain_hash
         FROM claim_verdicts ORDER BY seq",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(VerdictRow {
            id: r.get(0)?,
            seq: r.get(1)?,
            recorded_at: r.get(2)?,
            call_id: r.get(3)?,
            verdict: ClaimVerdict::from_str(&r.get::<_, String>(4)?)
                .unwrap_or(ClaimVerdict::Unknown),
            reason: r.get(5)?,
            detail_json: r.get(6)?,
            session_id: r.get(7)?,
            evidence_rows_scanned: r.get::<_, i64>(8)?.max(0) as u64,
            sensor_seq_lo: r.get(9)?,
            sensor_seq_hi: r.get(10)?,
            spilled_deliveries: r.get::<_, i64>(11)?.max(0) as u64,
            chain_hash: r.get(12)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// The standing (latest) verdict per call, by seq.
pub fn latest_verdicts(ledger: &Ledger) -> Result<BTreeMap<String, VerdictRow>> {
    let mut out = BTreeMap::new();
    for row in verdict_rows(ledger)? {
        out.insert(row.call_id.clone(), row);
    }
    Ok(out)
}

/// Re-fold the verdict chain link by link; any mismatch names the seq.
pub fn verify_verdicts_chain(ledger: &Ledger) -> Result<u64> {
    let conn = ledger.conn();
    let mut stmt = conn.prepare(
        "SELECT seq, call_id, verdict, reason, detail_json, session_id,
                evidence_rows_scanned, sensor_seq_lo, sensor_seq_hi,
                spilled_deliveries, content_sha256, prev_hash, chain_hash
         FROM claim_verdicts ORDER BY seq",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            (
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, Option<i64>>(7)?,
                r.get::<_, Option<i64>>(8)?,
                r.get::<_, i64>(9)?,
            ),
            (
                r.get::<_, String>(10)?,
                r.get::<_, String>(11)?,
                r.get::<_, String>(12)?,
            ),
        ))
    })?;
    let mut prev = genesis_prev_hash(CHAIN_SCOPE);
    let mut checked = 0u64;
    for row in rows {
        let (seq, fields, tail) = row?;
        let (call_id, verdict, reason, detail, session_id, scanned, lo, hi, spilled) = fields;
        let (stored_sha, stored_prev, stored_chain) = tail;
        let content = VerdictContent {
            call_id: &call_id,
            verdict: &verdict,
            reason: &reason,
            detail_json: &detail,
            session_id: session_id.as_deref(),
            rows_scanned: scanned.max(0) as u64,
            seq_lo: lo,
            seq_hi: hi,
            spilled: spilled.max(0) as u64,
        }
        .canonical();
        let sha = sha256_hex(content.as_bytes());
        let expect_chain = chain_hash(&prev, &sha, CHAIN_SCOPE, seq);
        if sha != stored_sha || stored_prev != prev || stored_chain != expect_chain {
            return Err(CoreError::Integrity(format!(
                "claim verdict chain does not add up at seq {seq}"
            )));
        }
        prev = stored_chain;
        checked += 1;
    }
    Ok(checked)
}

/* ── the verifier pass ───────────────────────────────────────────────── */

/// One claim's worth of the pass's outcome, for the CLI to print.
#[derive(Debug, Clone)]
pub struct VerifiedClaim {
    pub call_id: String,
    pub agent: String,
    pub scope: String,
    pub obligation: Option<String>,
    pub accepted_at_unix_ms: i64,
    pub verdict: ClaimVerdict,
    pub reason: String,
    pub verdict_short_id: String,
    /// False when the identical verdict already stood and nothing was
    /// appended this run.
    pub newly_recorded: bool,
}

/// The deterministic verifier pass: every `ready_for_verification` claim,
/// each evaluated against its own session's record (when identifiable), the
/// workspace, and the A201 spill guard; verdicts appended, never mutated.
pub fn verify_claims(
    ledger: &Ledger,
    spill: &Path,
    recorded_at: &str,
) -> Result<Vec<VerifiedClaim>> {
    // Claims, oldest first, straight from the append-only record.
    struct PendingClaim {
        call_id: String,
        agent: String,
        scope: String,
        obligation: Option<String>,
        manifest_json: Option<String>,
        request_sha256: String,
        accepted_at_unix_ms: i64,
    }
    let claims: Vec<PendingClaim> = {
        let conn = ledger.conn();
        let mut stmt = conn.prepare(
            "SELECT call_id, agent, scope, obligation, manifest_json, request_sha256,
                    accepted_at_unix_ms
             FROM mcp_calls
             WHERE tool = 'claim_completion' AND outcome = 'ready_for_verification'
             ORDER BY accepted_seq",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(PendingClaim {
                call_id: r.get(0)?,
                agent: r.get(1)?,
                scope: r.get(2)?,
                obligation: r.get(3)?,
                manifest_json: r.get(4)?,
                request_sha256: r.get(5)?,
                accepted_at_unix_ms: r.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        out
    };

    // The session identifier: argument hash → the deliveries that carried it.
    let deliveries = speaker::mcp_claim_deliveries(ledger)?;
    let mut by_sha: BTreeMap<&str, Vec<&speaker::McpClaimDelivery>> = BTreeMap::new();
    for d in &deliveries {
        by_sha.entry(d.request_sha256.as_str()).or_default().push(d);
    }

    let mut report = Vec::new();
    for claim in claims {
        let manifest: Option<Value> = claim
            .manifest_json
            .as_deref()
            .and_then(|m| serde_json::from_str(m).ok());

        let session = match by_sha.get(claim.request_sha256.as_str()) {
            None => SessionBasis::NotOnRecord,
            Some(rows) => {
                let mut sessions: Vec<&str> = rows
                    .iter()
                    .filter_map(|d| d.session_id.as_deref())
                    .collect();
                sessions.sort_unstable();
                sessions.dedup();
                match sessions.as_slice() {
                    [] => SessionBasis::NotOnRecord,
                    [one] => match rows.iter().find(|d| d.session_id.as_deref() == Some(*one)) {
                        Some(receipt) => SessionBasis::Identified {
                            session_id: (*one).to_string(),
                            seq: receipt.seq,
                            chain_hash: receipt.chain_hash.clone(),
                        },
                        // Unreachable by construction (the session came from
                        // these very rows); abstention is still the only
                        // honest fallback (no expect() in core).
                        None => SessionBasis::NotOnRecord,
                    },
                    many => SessionBasis::Ambiguous {
                        sessions: many.len(),
                    },
                }
            }
        };

        let (idx, cwd) = match &session {
            SessionBasis::Identified { session_id, .. } => {
                let rows = speaker::session_post_tool_rows(ledger, session_id)?;
                let cwd = rows.iter().find_map(|r| r.cwd.clone());
                let mut idx = EvidenceIndex::build(&rows);
                idx.spilled_deliveries = crate::sensor::spilled_for_session(spill, session_id);
                // A202: a claiming session born from a resume holds
                // pre-resume evidence under its prior id — a window this
                // scan cannot complete, so absence over it degrades to
                // `unknown`. The verifier has no transcript to count
                // against, so the resume signal is its only completeness
                // testimony; an unreadable record reads as "not known
                // resumed" (fail-open, never fail-accuse).
                idx.resume_started =
                    speaker::session_born_from_resume(ledger, session_id).unwrap_or(false);
                (Some(idx), cwd)
            }
            _ => (None, None),
        };

        let evaluation = evaluate(manifest.as_ref(), session, idx.as_ref(), cwd.as_deref());
        let newly_recorded = record_verdict(ledger, &claim.call_id, &evaluation, recorded_at)?;
        let detail_json = serde_json::to_string(&evaluation.entries)?;
        report.push(VerifiedClaim {
            verdict_short_id: speaker::short_display_id(&verdict_id(
                &claim.call_id,
                &evaluation,
                &detail_json,
            )),
            call_id: claim.call_id,
            agent: claim.agent,
            scope: claim.scope,
            obligation: claim.obligation,
            accepted_at_unix_ms: claim.accepted_at_unix_ms,
            verdict: evaluation.verdict,
            reason: evaluation.reason,
            newly_recorded,
        });
    }
    Ok(report)
}

/* ── feeding the digest cache (A201: verdicts reach the next brief) ──── */

const CLAIM_TRUNCATE: usize = 140;

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

/// The claim loop's contribution to a project's brief: every
/// `ready_for_verification` submission scoped to this project whose standing
/// verdict is not `supported`, newest first, as digest lines. A claim with
/// no verdict yet reads as still-unchecked (a status, not an accusation);
/// one whose latest verdict is `no_proof_found` carries that verdict's own
/// words and id.
pub fn claim_digest_lines(
    ledger: &Ledger,
    project_key: &str,
    limit: usize,
) -> Result<Vec<DigestLine>> {
    let scope = format!("project:{project_key}");
    let claims: Vec<(String, Option<String>)> = {
        let conn = ledger.conn();
        let mut stmt = conn.prepare(
            "SELECT call_id, obligation FROM mcp_calls
             WHERE tool = 'claim_completion' AND outcome = 'ready_for_verification'
               AND scope = ?1
             ORDER BY accepted_seq DESC",
        )?;
        let rows = stmt.query_map([&scope], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        out
    };
    if claims.is_empty() {
        return Ok(Vec::new());
    }
    let latest = latest_verdicts(ledger)?;
    let mut lines = Vec::new();
    for (call_id, obligation) in claims {
        if lines.len() >= limit {
            break;
        }
        let claim_text = truncate(
            obligation.as_deref().unwrap_or("(no obligation text)"),
            CLAIM_TRUNCATE,
        );
        match latest.get(&call_id) {
            Some(v) if v.verdict == ClaimVerdict::Supported => continue,
            Some(v) if v.verdict == ClaimVerdict::NoProofFound => lines.push(DigestLine {
                kind: "claim_no_proof".to_string(),
                claim: claim_text,
                transcript_line: None,
                // The line's own prefix already carries the accusation shape;
                // the basis names what was searched for, without repeating it.
                expected: Some(
                    v.reason
                        .strip_prefix("no proof found in the record for ")
                        .map(|rest| format!("the manifest names {rest}; the record held no match"))
                        .unwrap_or_else(|| v.reason.clone()),
                ),
                why_id: Some(speaker::short_display_id(&v.id)),
            }),
            Some(v) => lines.push(DigestLine {
                kind: "claim_open".to_string(),
                claim: claim_text,
                transcript_line: None,
                expected: Some(format!("checked, and {}", v.reason)),
                why_id: Some(speaker::short_display_id(&v.id)),
            }),
            None => lines.push(DigestLine {
                kind: "claim_open".to_string(),
                claim: claim_text,
                transcript_line: None,
                expected: Some(
                    "submitted for verification and not yet checked — run \
                     `thruline verify-claims`"
                        .to_string(),
                ),
                why_id: Some(claim_display_id(&call_id)),
            }),
        }
    }
    Ok(lines)
}

/// Rewrite one project's cached digest claim lines to match the standing
/// verdicts — the stale-accusation guard for the claim loop. A verdict that
/// just landed must not leave last night's "still no proof recorded" line
/// waiting for the next brief; conversely a project with no cache but
/// unresolved claims gets a claims-only digest so the loop still speaks.
pub fn refresh_claim_lines(
    ledger: &Ledger,
    dir: &Path,
    project_key: &str,
    generated_at: &str,
) -> Result<()> {
    let fresh = claim_digest_lines(ledger, project_key, speaker::MAX_LINES)?;
    match speaker::read_cache_digest(dir, project_key) {
        Some(mut digest) => {
            let (turn_no_proof, rest): (Vec<DigestLine>, Vec<DigestLine>) = digest
                .lines
                .drain(..)
                .filter(|l| !l.kind.starts_with("claim_"))
                .partition(|l| l.kind == "no_proof");
            let mut lines = turn_no_proof;
            lines.extend(fresh);
            lines.extend(rest);
            lines.truncate(speaker::MAX_LINES);
            digest.lines = lines;
            speaker::write_cache(dir, &digest)
        }
        None => {
            if fresh.is_empty() {
                return Ok(());
            }
            let digest = speaker::Digest {
                schema: speaker::DIGEST_SCHEMA.to_string(),
                project_key: project_key.to_string(),
                session_id: String::new(),
                transcript_path: String::new(),
                generated_at: generated_at.to_string(),
                hook: "verify-claims".to_string(),
                lines: fresh,
                evidence_rows_scanned: 0,
                sensor_seq_lo: None,
                sensor_seq_hi: None,
                sensor_chain_head: None,
                spilled_deliveries: 0,
                transcript_tool_events: None,
                resume_started: false,
                evidence_window_note: None,
                unaccounted_rows: 0,
                abstentions: Vec::new(),
            };
            speaker::write_cache(dir, &digest)
        }
    }
}

/* ── thruline why: the receipt chain behind any truth-loop id ────────── */

/// What a truth-loop id resolved to. Ambiguity is a refusal, never a guess
/// (the same contract as `now::Resolution`).
#[derive(Debug)]
pub enum WhyResolution {
    /// The full id and the rendered receipt chain, terminal-ready.
    One {
        id: String,
        rendered: String,
    },
    /// Every id the prefix matched, so the human can retype a longer one.
    Ambiguous(Vec<String>),
    NotFound,
}

/// Resolve a truth-loop id — `spkf_` (audit finding), `spkp_` (open
/// promise), `cvrd_` (claim verdict), `tlc_` (claim call) — by exact prefix.
///
/// Returns `None` for a needle that wears none of those prefixes, so the
/// caller can fall through to the attention-item namespace. Findings answer
/// from the ledger first and from the digest sidecars second (a SessionEnd
/// digest is cached without being announced); promises answer from the
/// sidecars, which are their only home.
pub fn why_truth_loop(
    ledger: &Ledger,
    digest_dir: &Path,
    needle: &str,
) -> Result<Option<WhyResolution>> {
    let needle = needle.trim();
    let resolution = if needle.starts_with("spkf_") {
        why_finding(ledger, digest_dir, needle)?
    } else if needle.starts_with("spkp_") {
        why_promise(digest_dir, needle)
    } else if needle.starts_with("cvrd_") {
        why_verdict(ledger, needle)?
    } else if needle.starts_with("tlc_") {
        why_claim(ledger, needle)?
    } else {
        return Ok(None);
    };
    Ok(Some(resolution))
}

fn iso_of_ms(ms: i64) -> String {
    crate::ingest::iso8601_utc(ms.div_euclid(1000))
}

fn window_phrase(rows: u64, lo: Option<i64>, hi: Option<i64>) -> String {
    match (lo, hi) {
        (Some(lo), Some(hi)) => format!(
            "{} from the session's own capture record (capture seq {lo}–{hi})",
            crate::voice::count_phrase(rows, "tool event", "tool events")
        ),
        _ => "the session's capture record holds no tool events".to_string(),
    }
}

/* ── spkf: audit findings ────────────────────────────────────────────── */

struct FindingRow {
    id: String,
    seq: i64,
    recorded_at: String,
    hook: String,
    session_id: String,
    transcript_path: String,
    transcript_line: i64,
    claim_kind: String,
    claim_marker: String,
    claim_text: String,
    expected_evidence: String,
    evidence_rows_scanned: i64,
    sensor_seq_lo: Option<i64>,
    sensor_seq_hi: Option<i64>,
    chain_hash: String,
}

fn why_finding(ledger: &Ledger, digest_dir: &Path, needle: &str) -> Result<WhyResolution> {
    // The ledger's findings first: recorded history outranks a cache file.
    let rows: Vec<FindingRow> = {
        let conn = ledger.conn();
        let mut stmt = conn.prepare(
            "SELECT id, seq, recorded_at, hook, session_id, transcript_path,
                    transcript_line, claim_kind, claim_marker, claim_text,
                    expected_evidence, evidence_rows_scanned, sensor_seq_lo,
                    sensor_seq_hi, chain_hash
             FROM speaker_findings WHERE substr(id, 1, ?1) = ?2 ORDER BY seq",
        )?;
        let found = stmt.query_map(
            rusqlite::params![needle.chars().count() as i64, needle],
            |r| {
                Ok(FindingRow {
                    id: r.get(0)?,
                    seq: r.get(1)?,
                    recorded_at: r.get(2)?,
                    hook: r.get(3)?,
                    session_id: r.get(4)?,
                    transcript_path: r.get(5)?,
                    transcript_line: r.get(6)?,
                    claim_kind: r.get(7)?,
                    claim_marker: r.get(8)?,
                    claim_text: r.get(9)?,
                    expected_evidence: r.get(10)?,
                    evidence_rows_scanned: r.get(11)?,
                    sensor_seq_lo: r.get(12)?,
                    sensor_seq_hi: r.get(13)?,
                    chain_hash: r.get(14)?,
                })
            },
        )?;
        let mut out = Vec::new();
        for row in found {
            out.push(row?);
        }
        out
    };

    // The cached digests second, for lines never announced to the ledger.
    let mut cached: Vec<(String, speaker::Digest, DigestLine)> = Vec::new();
    for digest in speaker::all_cache_digests(digest_dir) {
        for line in &digest.lines {
            if line.kind != "no_proof" {
                continue;
            }
            let Some(full) = speaker::finding_id(&digest, line) else {
                continue;
            };
            if full.starts_with(needle) && !rows.iter().any(|r| r.id == full) {
                cached.push((full, digest.clone(), line.clone()));
            }
        }
    }

    let mut ids: Vec<String> = rows
        .iter()
        .map(|r| r.id.clone())
        .chain(cached.iter().map(|(id, _, _)| id.clone()))
        .collect();
    ids.sort();
    ids.dedup();
    match ids.len() {
        0 => Ok(WhyResolution::NotFound),
        1 => {
            if let Some(row) = rows.first() {
                Ok(WhyResolution::One {
                    id: row.id.clone(),
                    rendered: render_finding(row),
                })
            } else {
                let (id, digest, line) = &cached[0];
                Ok(WhyResolution::One {
                    id: id.clone(),
                    rendered: render_cached_no_proof(id, digest, line),
                })
            }
        }
        _ => Ok(WhyResolution::Ambiguous(ids)),
    }
}

fn render_finding(row: &FindingRow) -> String {
    let window = window_phrase(
        row.evidence_rows_scanned.max(0) as u64,
        row.sensor_seq_lo,
        row.sensor_seq_hi,
    );
    format!(
        "claimed, and no proof found in the record\n\
         \x20 \"{claim}\"\n\
         \x20 the claim may be entirely true — the record simply does not show it\n\
         \n\
         what happened\n\
         \x20 the session's final turn made this claim (extraction rule {marker}, kind {kind})\n\
         \x20 the record was searched for {expected}\n\
         \x20 the search window: {window}\n\
         \x20 nothing in that window matched\n\
         \n\
         the receipt\n\
         \x20 claim        {path}:{line}\n\
         \x20 session      {session}\n\
         \x20 audited      {at} ({hook} hook)\n\
         \x20 finding      {id}\n\
         \x20 chain        scope speaker, seq {seq}, hash {chain}\n",
        claim = row.claim_text,
        marker = row.claim_marker,
        kind = row.claim_kind,
        expected = row.expected_evidence,
        path = row.transcript_path,
        line = row.transcript_line,
        session = row.session_id,
        at = row.recorded_at,
        hook = row.hook,
        id = row.id,
        seq = row.seq,
        chain = row.chain_hash,
    )
}

fn render_cached_no_proof(id: &str, digest: &speaker::Digest, line: &DigestLine) -> String {
    let window = window_phrase(
        digest.evidence_rows_scanned,
        digest.sensor_seq_lo,
        digest.sensor_seq_hi,
    );
    let receipt_line = line
        .transcript_line
        .map(|n| n.to_string())
        .unwrap_or_else(|| "?".to_string());
    format!(
        "claimed, and no proof found in the record\n\
         \x20 \"{claim}\"\n\
         \x20 the claim may be entirely true — the record simply does not show it\n\
         \n\
         what happened\n\
         \x20 the record was searched for {expected}\n\
         \x20 the search window: {window}\n\
         \x20 nothing in that window matched\n\
         \x20 this line is cached for the next brief; the stop audit is what records it\n\
         \n\
         the receipt\n\
         \x20 claim        {path}:{line}\n\
         \x20 session      {session}\n\
         \x20 digest       {key}.json (written {at}, {hook} hook)\n\
         \x20 id           {id}\n",
        claim = line.claim,
        expected = line.expected.as_deref().unwrap_or("matching tool evidence"),
        path = digest.transcript_path,
        line = receipt_line,
        session = digest.session_id,
        key = digest.project_key,
        at = digest.generated_at,
        hook = digest.hook,
    )
}

/* ── spkp: open promises ─────────────────────────────────────────────── */

fn why_promise(digest_dir: &Path, needle: &str) -> WhyResolution {
    let mut matches: Vec<(String, speaker::Digest, DigestLine)> = Vec::new();
    for digest in speaker::all_cache_digests(digest_dir) {
        for line in &digest.lines {
            if line.kind != "open_promise" {
                continue;
            }
            let Some(line_no) = line.transcript_line else {
                continue;
            };
            let full = speaker::promise_id_parts(
                &digest.project_key,
                &digest.session_id,
                &digest.transcript_path,
                line_no,
                &line.claim,
            );
            if full.starts_with(needle) {
                matches.push((full, digest.clone(), line.clone()));
            }
        }
    }
    let mut ids: Vec<String> = matches.iter().map(|(id, _, _)| id.clone()).collect();
    ids.sort();
    ids.dedup();
    match ids.len() {
        0 => WhyResolution::NotFound,
        1 => {
            let (id, digest, line) = &matches[0];
            let receipt_line = line
                .transcript_line
                .map(|n| n.to_string())
                .unwrap_or_else(|| "?".to_string());
            WhyResolution::One {
                id: id.clone(),
                rendered: format!(
                    "left open when the session ended\n\
                     \x20 \"{claim}\"\n\
                     \n\
                     what happened\n\
                     \x20 the session's final turn proposed this, and the record shows no tool\n\
                     \x20 activity after it — a fact about where the record stops, not a\n\
                     \x20 judgment of the work\n\
                     \n\
                     the receipt\n\
                     \x20 promise      {path}:{line}\n\
                     \x20 session      {session}\n\
                     \x20 digest       {key}.json (written {at}, {hook} hook)\n\
                     \x20 id           {id}\n",
                    claim = line.claim,
                    path = digest.transcript_path,
                    line = receipt_line,
                    session = digest.session_id,
                    key = digest.project_key,
                    at = digest.generated_at,
                    hook = digest.hook,
                ),
            }
        }
        _ => WhyResolution::Ambiguous(ids),
    }
}

/* ── cvrd and tlc: verdicts and the claims they answer ───────────────── */

struct ClaimContext {
    agent: String,
    scope: String,
    obligation: Option<String>,
    accepted_at_unix_ms: i64,
    request_sha256: String,
}

fn claim_context(ledger: &Ledger, call_id: &str) -> Result<Option<ClaimContext>> {
    Ok(ledger
        .conn()
        .query_row(
            "SELECT agent, scope, obligation, accepted_at_unix_ms, request_sha256
             FROM mcp_calls WHERE call_id = ?1",
            [call_id],
            |r| {
                Ok(ClaimContext {
                    agent: r.get(0)?,
                    scope: r.get(1)?,
                    obligation: r.get(2)?,
                    accepted_at_unix_ms: r.get(3)?,
                    request_sha256: r.get(4)?,
                })
            },
        )
        .optional()?)
}

fn entry_outcome_phrase(outcome: &str) -> &'static str {
    match outcome {
        "supported" => "proof found",
        "no_proof_found" => "no proof found in the record",
        _ => "not decidable by this tier",
    }
}

fn render_verdict(ledger: &Ledger, row: &VerdictRow) -> Result<String> {
    let context = claim_context(ledger, &row.call_id)?;
    let obligation = context
        .as_ref()
        .and_then(|c| c.obligation.as_deref())
        .unwrap_or("(no obligation text)");
    let submitted = match &context {
        Some(c) => format!(
            "submitted by {} for {}, accepted {}",
            c.agent,
            c.scope,
            iso_of_ms(c.accepted_at_unix_ms)
        ),
        None => "the claim row was not found in mcp_calls".to_string(),
    };
    let session = match &row.session_id {
        Some(s) => format!("the claiming session: {s} (identified by the call's own argument hash in the capture record)"),
        None => "the claiming session could not be identified from the capture record".to_string(),
    };
    let mut entries = String::new();
    if let Ok(detail) = serde_json::from_str::<Vec<EntryResult>>(&row.detail_json) {
        for e in &detail {
            entries.push_str(&format!(
                "\x20   [{class}] {value}\n\x20       {phrase}: {basis}\n",
                class = e.class,
                value = e.value,
                phrase = entry_outcome_phrase(&e.outcome),
                basis = e.basis,
            ));
        }
    }
    if entries.is_empty() {
        entries.push_str("\x20   (the manifest held no entries)\n");
    }
    let window = window_phrase(
        row.evidence_rows_scanned,
        row.sensor_seq_lo,
        row.sensor_seq_hi,
    );
    let spilled = match row.spilled_deliveries {
        0 => String::new(),
        n => format!(
            "; {} never reached the record",
            crate::voice::count_phrase(n, "spilled delivery", "spilled deliveries")
        ),
    };
    // The verdict phrase and the reason often open with the same words; a
    // reason that already carries the phrase stands alone.
    let verdict_line = if row.reason.starts_with(row.verdict.phrase()) {
        row.reason.clone()
    } else {
        format!("{} — {}", row.verdict.phrase(), row.reason)
    };
    Ok(format!(
        "a claim submitted as finished, checked against the record\n\
         \x20 \"{obligation}\"\n\
         \x20 verdict: {verdict_line}\n\
         \n\
         what happened\n\
         \x20 {submitted}\n\
         \x20 {session}\n\
         \x20 the manifest, entry by entry:\n\
         {entries}\
         \x20 the search window: {window}{spilled}\n\
         \n\
         the receipt\n\
         \x20 claim call   {call_id} (mcp_calls, append-only)\n\
         \x20 verdict      {id}\n\
         \x20 recorded     {at}\n\
         \x20 chain        scope claim_verdict, seq {seq}, hash {chain}\n",
        call_id = row.call_id,
        id = row.id,
        at = row.recorded_at,
        seq = row.seq,
        chain = row.chain_hash,
    ))
}

fn why_verdict(ledger: &Ledger, needle: &str) -> Result<WhyResolution> {
    let rows = verdict_rows(ledger)?;
    let matches: Vec<&VerdictRow> = rows.iter().filter(|r| r.id.starts_with(needle)).collect();
    match matches.len() {
        0 => Ok(WhyResolution::NotFound),
        1 => Ok(WhyResolution::One {
            id: matches[0].id.clone(),
            rendered: render_verdict(ledger, matches[0])?,
        }),
        _ => Ok(WhyResolution::Ambiguous(
            matches.iter().map(|r| r.id.clone()).collect(),
        )),
    }
}

fn why_claim(ledger: &Ledger, needle: &str) -> Result<WhyResolution> {
    let hex = needle.trim_start_matches("tlc_");
    if hex.is_empty() {
        return Ok(WhyResolution::NotFound);
    }
    let call_ids: Vec<String> = {
        let conn = ledger.conn();
        let mut stmt = conn.prepare(
            "SELECT call_id FROM mcp_calls
             WHERE tool = 'claim_completion' AND substr(call_id, 1, ?1) = ?2
             ORDER BY accepted_seq",
        )?;
        let rows = stmt.query_map(rusqlite::params![hex.chars().count() as i64, hex], |r| {
            r.get::<_, String>(0)
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        out
    };
    match call_ids.len() {
        0 => Ok(WhyResolution::NotFound),
        1 => {
            let call_id = &call_ids[0];
            let Some(context) = claim_context(ledger, call_id)? else {
                return Ok(WhyResolution::NotFound);
            };
            let obligation = context
                .obligation
                .as_deref()
                .unwrap_or("(no obligation text)");
            let history: Vec<VerdictRow> = verdict_rows(ledger)?
                .into_iter()
                .filter(|v| &v.call_id == call_id)
                .collect();
            let standing = match history.last() {
                Some(v) => format!(
                    "standing verdict: {} — {} (verdict {}, recorded {})",
                    v.verdict.phrase(),
                    v.reason,
                    speaker::short_display_id(&v.id),
                    v.recorded_at
                ),
                None => "no verdict yet — run `thruline verify-claims` to evaluate it".to_string(),
            };
            let mut verdict_receipts = String::new();
            for v in &history {
                verdict_receipts.push_str(&format!(
                    "\x20 verdict      {} ({}, recorded {})\n",
                    v.id,
                    v.verdict.phrase(),
                    v.recorded_at
                ));
            }
            Ok(WhyResolution::One {
                id: claim_display_id(call_id),
                rendered: format!(
                    "a claim submitted as finished\n\
                     \x20 \"{obligation}\"\n\
                     \x20 {standing}\n\
                     \n\
                     what happened\n\
                     \x20 submitted by {agent} for {scope}, accepted {at}\n\
                     \x20 recorded as ready for verification — the response to the claimant said\n\
                     \x20 so and nothing more; the verdict belongs to the verifier\n\
                     \n\
                     the receipt\n\
                     \x20 claim call   {call_id} (mcp_calls, append-only)\n\
                     \x20 request      sha256 {sha}\n\
                     {verdicts}",
                    agent = context.agent,
                    scope = context.scope,
                    at = iso_of_ms(context.accepted_at_unix_ms),
                    sha = context.request_sha256,
                    verdicts = verdict_receipts,
                ),
            })
        }
        _ => Ok(WhyResolution::Ambiguous(
            call_ids.iter().map(|c| claim_display_id(c)).collect(),
        )),
    }
}
