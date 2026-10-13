//! The entailment pass (§5 M3.4). Two passes, one scorer, no graph in memory.
//!
//! ## Why this holds no derivation state
//!
//! §5a's memory gate is ≤ 1.0 GB peak total RSS for `annotate`, and the way it
//! is met is architectural rather than incidental: **the scorer and derivation
//! are never co-resident.** Claims come straight out of `events`; candidate
//! evidence is queried per claim and rendered one row at a time. The graph is
//! not built here at all. `derive` consumes what this writes, afterwards, with
//! the model already unloaded.
//!
//! ## Two passes
//!
//! * **Backward** — a completed-work claim against action clusters near it.
//!   "Did what the tools actually did entail what was said?"
//! * **Forward** — a proposal against *later* action clusters project-wide,
//!   behind a cheap lexical gate, and only once the project has moved on. A
//!   proposal from the current session has not been dropped; it is pending.
//!
//! Both write `evidence_links` rows carrying the score, the verdict band, the
//! pass, and — load-bearing for M3.5 — the premise's **fidelity**.
//!
//! ## Idempotency
//!
//! Claim ids are `sha256(event ‖ extractor ‖ span ‖ text)`, not ULIDs. The
//! ULID convention yields to the stronger invariant: derived and annotated rows
//! must be **rebuildable byte-identically**, and a time-ordered random id can
//! never be. Re-running with the same extractor and model is a no-op; a changed
//! model appends, because `scorer` is part of the `evidence_links` key.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::{params, OptionalExtension};

use crate::chain::sha256_hex;
use crate::classification_store::{
    self, ClaimProjectionRecord, NewClassification, StoreDisposition,
};
use crate::config::Config;
use crate::derive::{evidence_detail_for, evidence_row_for};
use crate::error::Result;
use crate::extract::{
    sentence_candidates, Claim, ClaimExtractor, ClaimKind, MarkerExtractor, SentenceCandidate,
};
use crate::language::{
    Annotator as LanguageAnnotator, ClassificationOutcome, LanguageCategory, LanguageError,
    MemoryPressureProbe, MemoryPressureState, MemoryPressureThresholds, ShadowLabel, SpeechAct,
    SystemMemoryPressureProbe,
};
use crate::ledger::Ledger;
use crate::nli::{EntailmentScorer, Scores};
use crate::premise::{is_agent_record, render_tagged_for_claim, Fidelity};
use crate::raw_scope::{RawScope, RawScopeCache};

/// §5i: the per-line `cwd` a shell command actually ran in.
///
/// Claude Code records `cwd` on every raw JSONL line. It is the difference
/// between `cd …/atlas-proxy && npm test` (repo-root cwd) and a bare
/// `npm test` run from `…/atlas-proxy` — the same invocation, and A51's whole
/// failure. Read here rather than migrated into `events`, which is append-only.
///
/// Cached per file: a project's evidence is concentrated in a handful of
/// transcripts, and re-reading a 30 MB JSONL per candidate would dominate the
/// annotate budget.
fn row_scope(row: &crate::derive::EvidenceRow, cache: &mut RawScopeCache) -> RawScope {
    match (
        row.raw_file.as_deref(),
        row.raw_line,
        row.raw_sha256.as_deref(),
    ) {
        (Some(file), Some(line), Some(hash)) => cache.scope_for(file, line, hash),
        _ => RawScope::default(),
    }
}

/// The frozen §5i identity for one evidence row.
fn row_identity(row: &crate::derive::EvidenceRow, cache: &mut RawScopeCache) -> String {
    let scope = row_scope(row, cache);
    if !scope.trusted {
        return String::new();
    }
    crate::identity::identity_key(
        &row.tool_name,
        row.tool_input.as_deref().unwrap_or(""),
        scope.cwd.as_deref(),
    )
}

/// Verdict band for one (claim, evidence) pair. Names match §7's comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Entailed,
    NotEntailed,
    Uncertain,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Entailed => "entailed",
            Verdict::NotEntailed => "not_entailed",
            Verdict::Uncertain => "uncertain",
        }
    }
    pub fn band(score: f32, hi: f32, lo: f32) -> Self {
        if score >= hi {
            Verdict::Entailed
        } else if score <= lo {
            Verdict::NotEntailed
        } else {
            Verdict::Uncertain
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AnnotateReport {
    /// Extractor revision that produced the active derived claim set.
    pub extractor: String,
    pub language_model: Option<String>,
    pub classification_events: u64,
    pub classification_candidates: u64,
    pub classification_annotations_inserted: u64,
    pub classification_annotations_reused: u64,
    pub classification_accepted: u64,
    pub classification_demoted: u64,
    pub classification_uncertain: u64,
    pub classification_attempts: u64,
    pub classification_memory_pauses: u64,
    /// Candidates left `Uncertain` because memory pressure never cleared inside
    /// `language.memory_pause_timeout_secs`.
    ///
    /// **Counted, not swallowed.** A degraded run that nobody can see is the
    /// failure this project refuses; before the deadline existed the same
    /// condition was an unbounded wait, which nobody could see either.
    pub classification_memory_timeouts: u64,
    pub classification_ms: u128,
    pub events_scanned: u64,
    pub claims_written: u64,
    pub completed_claims: u64,
    pub proposal_claims: u64,
    /// Proposals the project has not yet moved past — never forward-scored.
    pub proposals_still_recent: u64,
    /// Findings: stored for A29, never scored and never accused (§5b).
    pub finding_claims: u64,
    /// Micro-intents: absorbed, never scored, never a ghost (§5b).
    pub micro_intent_claims: u64,
    pub backward_pairs: u64,
    pub forward_pairs: u64,
    pub links_written: u64,
    pub pairs_truncated: u64,
    /// Claims with no candidate evidence at all — M3.5's `unverified` path.
    pub claims_without_candidates: u64,
    /// §5j eligibility funnel (A52): proposals → action-defeated →
    /// output-annotated → eligible. Reported with every run.
    pub proposals_action_defeated: u64,
    pub proposals_output_annotated: u64,
    pub proposals_eligible: u64,
    pub proposals_unsearchable: u64,
    /// §5r unsupported funnel: completed-work/marker claims whose dominant
    /// local evidence families were all read, and per-family coverage misses.
    pub unsupported_coverage_eligible: u64,
    pub coverage_missing_by_family: BTreeMap<String, u64>,
    /// §5p family coverage, tabulated with every run.
    pub family_test: u64,
    pub family_build: u64,
    pub family_git: u64,
    pub family_agent: u64,
    pub family_none: u64,
    pub fallback_premises: u64,
    pub parsed_premises: u64,
    pub extract_ms: u128,
    /// Whole candidate pipeline: profile/retrieval, rendering, inference and
    /// SQLite writes. Kept for end-to-end budgeting; it is not scorer time.
    pub score_ms: u128,
    /// Pair-local input lookup + claim-focused rendering.
    pub pair_prepare_us: u128,
    /// Calls through `EntailmentScorer::score` only (tokenization + backend).
    pub inference_us: u128,
    /// Pair-local evidence-link writes only.
    pub link_write_us: u128,
}

/// Read-only §5r measurement over a frozen annotation layer.
#[derive(Debug, Clone, Default)]
pub struct CoverageDryRun {
    pub claims: u64,
    pub coverage_eligible: u64,
    pub would_be_unsupported: u64,
    pub now_unknown: u64,
    pub now_emitted: u64,
    pub project_family_profile: BTreeMap<String, u64>,
    pub missing_by_family: BTreeMap<String, u64>,
}

/// Deterministic claim id. Hex, not ULID — see the module docs.
fn claim_id(event_id: &str, extractor: &str, c: &Claim) -> String {
    let material = format!(
        "{event_id}\u{1f}{extractor}\u{1f}{}\u{1f}{}\u{1f}{}",
        c.span_start, c.span_end, c.text
    );
    format!("c{}", &sha256_hex(material.as_bytes())[..31])
}

fn projected_claim_records(
    extractor: &str,
    claims: &[(String, Claim)],
) -> Vec<ClaimProjectionRecord> {
    claims
        .iter()
        .map(|(event_id, claim)| ClaimProjectionRecord {
            id: claim_id(event_id, extractor, claim),
            event_id: event_id.clone(),
            text: claim.text.clone(),
            span_start: Some(claim.span_start as i64),
            span_end: Some(claim.span_end as i64),
            extractor: extractor.to_string(),
            kind: claim.kind.as_str().to_string(),
            marker: Some(claim.marker.as_str().to_string()),
        })
        .collect()
}

/// Canonical `assistant_text` events in derivation's order, deduplicated by
/// `src_uuid` (A13) so a resumed session's copied prefix is not re-claimed.
fn statement_events(
    ledger: &Ledger,
    project_id: &str,
) -> Result<Vec<(String, String, i64, String)>> {
    let mut stmt = ledger.conn().prepare(
        "SELECT e.id, e.session_id, e.seq, e.text, e.src_uuid
         FROM events e JOIN sessions s ON s.id = e.session_id
         WHERE e.project_id = ?1 AND e.kind = 'assistant_text' AND e.text IS NOT NULL
         ORDER BY (s.started_at IS NULL), s.started_at, e.session_id, e.seq",
    )?;
    let rows = stmt.query_map(params![project_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<String>>(4)?,
        ))
    })?;
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out = Vec::new();
    for r in rows {
        let (id, session_id, seq, text, uuid) = r?;
        if let Some(u) = uuid {
            if !seen.insert(u) {
                continue;
            }
        }
        out.push((id, session_id, seq, text));
    }
    Ok(out)
}

fn count_claim(r: &mut AnnotateReport, kind: ClaimKind) {
    r.claims_written += 1;
    match kind {
        ClaimKind::Work | ClaimKind::Marker => r.completed_claims += 1,
        ClaimKind::Proposal => r.proposal_claims += 1,
        ClaimKind::Finding => r.finding_claims += 1,
        ClaimKind::MicroIntent => r.micro_intent_claims += 1,
    }
}

/// Replace the project's rebuildable claim set after a complete extraction
/// pass. Classification annotations themselves are append-only; claims and
/// their scores are the active derived projection and may be rebuilt.
fn persist_claims(
    ledger: &Ledger,
    project_id: &str,
    extractor: &str,
    claims: &[(String, Claim)],
    r: &mut AnnotateReport,
) -> Result<()> {
    let existing_extractors: Vec<String> = {
        let mut stmt = ledger.conn().prepare(
            "SELECT DISTINCT c.extractor FROM claims c
             JOIN events e ON e.id = c.event_id
             WHERE e.project_id = ?1 ORDER BY c.extractor",
        )?;
        let rows = stmt
            .query_map(params![project_id], |row| row.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        rows
    };
    let changing_extractor = existing_extractors
        .iter()
        .any(|existing| existing != extractor);
    let mut projection_changed = changing_extractor;
    let tx = ledger.conn().unchecked_transaction()?;
    if changing_extractor {
        tx.execute(
            "DELETE FROM evidence_links WHERE claim_id IN (
                 SELECT c.id FROM claims c JOIN events e ON e.id = c.event_id
                 WHERE e.project_id = ?1)",
            params![project_id],
        )?;
        tx.execute(
            "DELETE FROM claim_searches WHERE claim_id IN (
                 SELECT c.id FROM claims c JOIN events e ON e.id = c.event_id
                 WHERE e.project_id = ?1)",
            params![project_id],
        )?;
        tx.execute(
            "DELETE FROM claims WHERE event_id IN (
                 SELECT id FROM events WHERE project_id = ?1)",
            params![project_id],
        )?;
    }
    {
        let mut ins = tx.prepare(
            "INSERT OR IGNORE INTO claims(id, event_id, text, span_start, span_end,
                 extractor, confidence, kind, marker)
             VALUES(?1,?2,?3,?4,?5,?6,NULL,?7,?8)",
        )?;
        for (event_id, c) in claims {
            let id = claim_id(event_id, extractor, c);
            projection_changed |= ins.execute(params![
                id,
                event_id,
                c.text,
                c.span_start as i64,
                c.span_end as i64,
                extractor,
                c.kind.as_str(),
                c.marker.as_str(),
            ])? > 0;
            count_claim(r, c.kind);
        }
    }
    if projection_changed {
        // A same-extractor append adds new claims without entering the
        // replacement block above; its old scores are still an incomplete
        // projection and must lose authority just like a taxonomy swap.
        if !changing_extractor {
            tx.execute(
                "DELETE FROM evidence_links WHERE claim_id IN (
                     SELECT c.id FROM claims c JOIN events e ON e.id = c.event_id
                     WHERE e.project_id = ?1)",
                params![project_id],
            )?;
            tx.execute(
                "DELETE FROM claim_searches WHERE claim_id IN (
                     SELECT c.id FROM claims c JOIN events e ON e.id = c.event_id
                     WHERE e.project_id = ?1)",
                params![project_id],
            )?;
        }
        tx.execute(
            "INSERT INTO entailment_projection_heads(project_id, scorer) VALUES(?1, '')
             ON CONFLICT(project_id) DO UPDATE SET scorer = ''",
            params![project_id],
        )?;
    }
    tx.execute(
        "INSERT INTO claim_projection_heads(project_id, extractor, manifest_annotation_id)
         VALUES(?1,?2,NULL)
         ON CONFLICT(project_id) DO UPDATE SET
           extractor = excluded.extractor,
           manifest_annotation_id = NULL",
        params![project_id, extractor],
    )?;
    tx.commit()?;
    r.extractor = extractor.to_string();
    Ok(())
}

/// Atomically make one completed language run authoritative. Per-event model
/// outputs may already have been appended, but the rebuildable claim
/// projection and its completion manifest cross the visibility boundary in a
/// single transaction. Replaying an already-complete run preserves existing
/// evidence links when the complete projected claim set is unchanged.
struct ProjectionManifest<'a> {
    anchor: &'a str,
    model_tag: &'a str,
    prompt: &'a str,
    json: &'a str,
    created_at: &'a str,
}

fn persist_language_projection(
    ledger: &Ledger,
    project_id: &str,
    extractor: &str,
    claims: &[(String, Claim)],
    manifest: ProjectionManifest<'_>,
    r: &mut AnnotateReport,
) -> Result<()> {
    let expected_hash =
        classification_store::claim_projection_sha256(projected_claim_records(extractor, claims))?;
    let current_hash = {
        let mut stmt = ledger.conn().prepare(
            "SELECT c.id, c.event_id, c.text, c.span_start, c.span_end,
                    c.extractor, c.kind, c.marker
             FROM claims c JOIN events e ON e.id = c.event_id
             WHERE e.project_id = ?1 ORDER BY c.id",
        )?;
        let records = stmt
            .query_map(params![project_id], |row| {
                Ok(ClaimProjectionRecord {
                    id: row.get(0)?,
                    event_id: row.get(1)?,
                    text: row.get(2)?,
                    span_start: row.get(3)?,
                    span_end: row.get(4)?,
                    extractor: row.get(5)?,
                    kind: row.get(6)?,
                    marker: row.get(7)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        classification_store::claim_projection_sha256(records)?
    };
    let current_extractor: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT extractor FROM claim_projection_heads WHERE project_id = ?1",
            params![project_id],
            |row| row.get(0),
        )
        .optional()?;

    let tx = ledger.conn().unchecked_transaction()?;
    if current_extractor.as_deref() != Some(extractor) || current_hash != expected_hash {
        tx.execute(
            "DELETE FROM evidence_links WHERE claim_id IN (
                 SELECT c.id FROM claims c JOIN events e ON e.id = c.event_id
                 WHERE e.project_id = ?1)",
            params![project_id],
        )?;
        tx.execute(
            "DELETE FROM claim_searches WHERE claim_id IN (
                 SELECT c.id FROM claims c JOIN events e ON e.id = c.event_id
                 WHERE e.project_id = ?1)",
            params![project_id],
        )?;
        tx.execute(
            "INSERT INTO entailment_projection_heads(project_id, scorer) VALUES(?1, '')
             ON CONFLICT(project_id) DO UPDATE SET scorer = ''",
            params![project_id],
        )?;
        tx.execute(
            "DELETE FROM claims WHERE event_id IN (
                 SELECT id FROM events WHERE project_id = ?1)",
            params![project_id],
        )?;
        let mut ins = tx.prepare(
            "INSERT INTO claims(id, event_id, text, span_start, span_end,
                 extractor, confidence, kind, marker)
             VALUES(?1,?2,?3,?4,?5,?6,NULL,?7,?8)",
        )?;
        for (event_id, claim) in claims {
            ins.execute(params![
                claim_id(event_id, extractor, claim),
                event_id,
                claim.text,
                claim.span_start as i64,
                claim.span_end as i64,
                extractor,
                claim.kind.as_str(),
                claim.marker.as_str(),
            ])?;
        }
    }

    let manifest = classification_store::store_in_transaction(
        &tx,
        NewClassification {
            event_id: manifest.anchor,
            model: manifest.model_tag,
            prompt: manifest.prompt,
            output_json: manifest.json,
            confidence: 1.0,
            created_at: manifest.created_at,
        },
    )?;
    tx.execute(
        "INSERT INTO claim_projection_heads(project_id, extractor, manifest_annotation_id)
         VALUES(?1,?2,?3)
         ON CONFLICT(project_id) DO UPDATE SET
           extractor = excluded.extractor,
           manifest_annotation_id = excluded.manifest_annotation_id",
        params![project_id, extractor, manifest.annotation.id],
    )?;
    tx.commit()?;

    for (_, claim) in claims {
        count_claim(r, claim.kind);
    }
    match manifest.disposition {
        StoreDisposition::Inserted => r.classification_annotations_inserted += 1,
        StoreDisposition::Existing => r.classification_annotations_reused += 1,
    }
    r.extractor = extractor.to_string();
    Ok(())
}

/// Pass 1: extract and persist claims. No model is loaded here.
fn write_claims(ledger: &Ledger, project_id: &str, r: &mut AnnotateReport) -> Result<()> {
    let events = statement_events(ledger, project_id)?;
    r.events_scanned = events.len() as u64;
    let ex = MarkerExtractor;
    let mut claims = Vec::new();
    for (event_id, _session, _seq, text) in &events {
        for c in ex.extract(text) {
            claims.push((event_id.clone(), c));
        }
    }
    persist_claims(ledger, project_id, ex.name(), &claims, r)
}

const LANGUAGE_EXTRACTOR_REV: &str = "language-v1";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredCandidateClassification {
    span_start: usize,
    span_end: usize,
    category: LanguageCategory,
    confidence: f32,
    speech_act: SpeechAct,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<ShadowLabel>,
    attempts: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredEventClassification {
    #[serde(rename = "type")]
    record_type: String,
    results: Vec<StoredCandidateClassification>,
}

/// One frozen per-event annotation consumed by a complete language run.
/// The run manifest carries this list so later export never guesses which
/// historical model outputs belonged to the active projection.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ClassificationRunMember {
    event_id: String,
    annotation_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredClassificationRun {
    #[serde(rename = "type")]
    record_type: String,
    complete: bool,
    truth_bearing: bool,
    prompt_revision: String,
    extractor: String,
    claim_set_sha256: String,
    events: u64,
    candidates: u64,
    accepted: u64,
    members: Vec<ClassificationRunMember>,
}

/// Safe, receipt-addressed view of the active optional language layer.
/// Numeric confidence, provider prose, raw prompts, and uncertain reasons are
/// deliberately absent: none of them is evidence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LanguageShadowProjection {
    pub truth_bearing: bool,
    pub scope: &'static str,
    pub run: LanguageShadowRun,
    pub coverage: LanguageShadowCoverage,
    pub items: Vec<LanguageShadowItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LanguageShadowRun {
    pub annotation_id: String,
    pub model: String,
    pub model_digest: String,
    pub prompt_revision: String,
    pub extractor: String,
    pub created_at: String,
    pub prompt_sha256: String,
    pub output_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LanguageShadowCoverage {
    pub events: u64,
    pub candidates: u64,
    pub classified: u64,
    pub uncertain: u64,
    pub labeled: u64,
    pub accepted_claims: u64,
    pub attempts: u64,
    pub categories: BTreeMap<String, u64>,
    pub speech_acts: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LanguageShadowItem {
    pub annotation_id: String,
    pub event_id: String,
    pub session_id: String,
    pub seq: i64,
    pub span: [usize; 2],
    pub category: String,
    pub speech_act: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub attempts: u8,
    pub prompt_sha256: String,
    pub output_sha256: String,
}

/// Exact minimal-context message sent to the language provider. Public so the
/// ignored, local-only M4 measurement harness exercises production prompt
/// construction instead of maintaining a look-alike.
pub fn language_candidate_message(candidates: &[SentenceCandidate], index: usize) -> String {
    let candidate = &candidates[index];
    let previous = index
        .checked_sub(1)
        .and_then(|i| candidates.get(i))
        .map(|c| c.text.as_str())
        .unwrap_or("(none)");
    let next = candidates
        .get(index + 1)
        .map(|c| c.text.as_str())
        .unwrap_or("(none)");
    format!(
        "Previous sentence: {previous}\nTarget sentence: {}\nNext sentence: {next}",
        candidate.context
    )
}

fn event_prompt_material(messages: &[String]) -> String {
    let joined = messages
        .iter()
        .enumerate()
        .map(|(i, message)| format!("candidate {i}\n{message}"))
        .collect::<Vec<_>>()
        .join("\n\u{1e}\n");
    crate::language::prompt_material(&joined)
}

fn language_event_inputs(text: &str) -> (Vec<SentenceCandidate>, Vec<String>, String) {
    let candidates = sentence_candidates(text);
    let messages: Vec<String> = (0..candidates.len())
        .map(|index| language_candidate_message(&candidates, index))
        .collect();
    let prompt = event_prompt_material(&messages);
    (candidates, messages, prompt)
}

fn append_classification_run_key(run_keys: &mut String, event_id: &str, prompt_sha256: &str) {
    run_keys.push_str(event_id);
    run_keys.push('|');
    run_keys.push_str(prompt_sha256);
    run_keys.push('\n');
}

fn classification_run_prompt(extractor: &str, claim_set_sha256: &str, run_keys: &str) -> String {
    format!(
        "{}\nextractor:{extractor}\nclaim-set:{claim_set_sha256}\nclassification-run:{}",
        crate::language::PROMPT_REVISION,
        sha256_hex(run_keys.as_bytes())
    )
}

fn language_extractor_identity(model_tag: &str, confidence_threshold: f32) -> String {
    format!(
        "{LANGUAGE_EXTRACTOR_REV}:{}:t{:08x}",
        &sha256_hex(format!("{model_tag}|{}", crate::language::PROMPT_REVISION).as_bytes())[..12],
        confidence_threshold.to_bits(),
    )
}

fn language_extractor_threshold(extractor: &str, model_tag: &str) -> Option<f32> {
    let prefix = format!(
        "{LANGUAGE_EXTRACTOR_REV}:{}:t",
        &sha256_hex(format!("{model_tag}|{}", crate::language::PROMPT_REVISION).as_bytes())[..12],
    );
    let bits = extractor.strip_prefix(&prefix)?;
    if bits.len() != 8
        || !bits
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return None;
    }
    let threshold = f32::from_bits(u32::from_str_radix(bits, 16).ok()?);
    (threshold.is_finite()
        && (0.0..=1.0).contains(&threshold)
        && language_extractor_identity(model_tag, threshold) == extractor)
        .then_some(threshold)
}

/// Test door for the model-name validity gate (A209-C): the gate had no
/// test of any kind before, and a gate nothing exercises is a gate nobody
/// notices loosening.
pub fn safe_language_model_name_for_test(value: &str) -> bool {
    safe_language_model_name(value)
}

fn safe_language_model_name(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
        && crate::redact::mask(value).2 == 0
        && !crate::redact::resembles_known_credential(value)
}

fn split_safe_language_model_identity(value: &str) -> Option<(&str, &str)> {
    let (model, digest) = value.rsplit_once("@sha256:")?;
    (safe_language_model_name(model) && !model.contains("@sha256:") && is_lower_sha256(digest))
        .then_some((model, digest))
}

fn validate_language_model_tag(value: &str) -> Result<()> {
    let valid = if value.contains("@sha256:") {
        split_safe_language_model_identity(value).is_some()
    } else {
        safe_language_model_name(value)
    };
    if !valid {
        return Err(crate::error::CoreError::Config(
            "language model identity must be a bounded safe tag, optionally followed by one lowercase SHA-256 digest"
                .to_string(),
        ));
    }
    Ok(())
}

fn canonical_language_timestamp(value: &str) -> bool {
    if value.len() < 20 || value.len() > 30 || !value.ends_with('Z') {
        return false;
    }
    let bytes = value.as_bytes();
    for (index, separator) in [(4, b'-'), (7, b'-'), (10, b'T'), (13, b':'), (16, b':')] {
        if bytes.get(index) != Some(&separator) {
            return false;
        }
    }
    if bytes
        .iter()
        .take(19)
        .enumerate()
        .any(|(index, byte)| ![4, 7, 10, 13, 16].contains(&index) && !byte.is_ascii_digit())
    {
        return false;
    }
    let number = |range: std::ops::Range<usize>| -> Option<u32> {
        std::str::from_utf8(bytes.get(range)?).ok()?.parse().ok()
    };
    let year = number(0..4).unwrap_or(0);
    let month = number(5..7).unwrap_or(0);
    let day = number(8..10).unwrap_or(0);
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    if day < 1
        || day > max_day
        || !number(11..13).is_some_and(|hour| hour <= 23)
        || !number(14..16).is_some_and(|minute| minute <= 59)
        || !number(17..19).is_some_and(|second| second <= 60)
    {
        return false;
    }
    let fraction = &value[19..value.len() - 1];
    fraction.is_empty()
        || (fraction.starts_with('.')
            && fraction.len() >= 2
            && fraction.len() <= 10
            && fraction[1..].bytes().all(|byte| byte.is_ascii_digit()))
}

fn uncertain_reason(reason: &crate::language::UncertainReason) -> String {
    match reason {
        crate::language::UncertainReason::Provider(message) => message.clone(),
        crate::language::UncertainReason::InvalidOutput(message) => message.clone(),
    }
}

/// One-way agreement gate between the deterministic extractor and language
/// classifier. The model may admit or demote an existing claim, never create
/// or re-type one.
pub fn language_category_agrees(kind: ClaimKind, category: LanguageCategory) -> bool {
    matches!(
        (kind, category),
        (
            ClaimKind::Work | ClaimKind::Marker,
            LanguageCategory::CompletedClaim
        ) | (ClaimKind::Proposal, LanguageCategory::Proposal)
            | (ClaimKind::Finding, LanguageCategory::Finding)
            | (ClaimKind::MicroIntent, LanguageCategory::MicroIntent)
    )
}

/// Refusal and deferral are useful shadow navigation metadata, but can only
/// make a heuristic proposal less assertive. They never create a dropped
/// verdict and never affect completed-work/finding/micro-intent candidates.
pub fn speech_act_allows_claim(kind: ClaimKind, speech_act: SpeechAct) -> bool {
    !matches!(
        (kind, speech_act),
        (
            ClaimKind::Proposal,
            SpeechAct::Refusal | SpeechAct::Deferral
        )
    )
}

const LANGUAGE_MEMORY_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

/// Delay between pressure samples. The product installs
/// [`ThreadMemoryPressurePoll`]; the hidden injection path exists solely so
/// tests can drive the unchanged wait policy without wall-clock sleeps.
#[doc(hidden)]
pub trait MemoryPressurePoll: Send + Sync {
    fn wait_before_retry(&self) -> std::result::Result<(), LanguageError>;
}

/// Fixed production polling behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct ThreadMemoryPressurePoll;

impl MemoryPressurePoll for ThreadMemoryPressurePoll {
    fn wait_before_retry(&self) -> std::result::Result<(), LanguageError> {
        std::thread::sleep(LANGUAGE_MEMORY_POLL_INTERVAL);
        Ok(())
    }
}

fn production_language_memory() -> (SystemMemoryPressureProbe, ThreadMemoryPressurePoll) {
    (SystemMemoryPressureProbe, ThreadMemoryPressurePoll)
}

/// What the pause loop decided.
///
/// `TimedOut` is the escape the loop used to lack. It is not an error: an
/// unavailable language tier already has a defined path — `LanguageConfig`'s
/// own documentation says annotation continues without it — and a candidate
/// nobody could classify is `Uncertain` with a stated reason, never a guess.
/// What it is not, any more, is a wait with no end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LanguageMemory {
    Ready,
    TimedOut { waited_secs: u64 },
}

/// Waits for memory pressure to clear, **with a deadline**.
///
/// # The bug this signature exists to fix
///
/// This was `while state == Paused { poll.wait_before_retry()?; state =
/// probe.sample(...)?; }` — no deadline, no iteration bound, no escape. If
/// available memory never rose above `resume_above_available_mb` it waited
/// forever. Measured: a full workspace test run blocked here for 13.5 minutes
/// on 7.7 seconds of CPU, while a second process built the same workspace —
/// which is an ordinary CI condition, not an exotic one. In CI it presents as a
/// hung runner rather than a failed one, which is the worse of the two.
///
/// The deadline is `cfg.language.memory_pause_timeout_secs`, validated non-zero
/// because a zero deadline is the same unbounded wait wearing a bound's name.
///
/// **One residual, stated rather than implied.** The deadline is checked
/// between retries, so it bounds *this* loop and not a `MemoryPressurePoll`
/// implementation that never returns. The production poll sleeps 250 ms and
/// returns; a poll that blocks forever is a broken poll, and no loop bound here
/// could rescue it.
fn wait_for_language_memory(
    cfg: &Config,
    r: &mut AnnotateReport,
    probe: &dyn MemoryPressureProbe,
    poll: &dyn MemoryPressurePoll,
) -> Result<LanguageMemory> {
    let mib = 1024_u64 * 1024;
    let thresholds = MemoryPressureThresholds::new(
        cfg.language.pause_below_available_mb.saturating_mul(mib),
        cfg.language.resume_above_available_mb.saturating_mul(mib),
    )
    .map_err(|e| crate::error::CoreError::Config(e.to_string()))?;
    let mut state = probe
        .sample(MemoryPressureState::Ready, thresholds)
        .map_err(|e| crate::error::CoreError::Config(e.to_string()))?;
    if state == MemoryPressureState::Ready {
        return Ok(LanguageMemory::Ready);
    }
    r.classification_memory_pauses += 1;
    tracing::warn!(
        pause_mb = cfg.language.pause_below_available_mb,
        resume_mb = cfg.language.resume_above_available_mb,
        timeout_secs = cfg.language.memory_pause_timeout_secs,
        "language queue paused for memory pressure"
    );
    let deadline = std::time::Duration::from_secs(cfg.language.memory_pause_timeout_secs);
    let started = std::time::Instant::now();
    while state == MemoryPressureState::Paused {
        if started.elapsed() >= deadline {
            let waited_secs = started.elapsed().as_secs();
            r.classification_memory_timeouts += 1;
            // Loud. A degraded run that nobody notices is the failure this
            // project refuses; the count travels in the report and the line
            // says what to change.
            tracing::error!(
                waited_secs,
                resume_mb = cfg.language.resume_above_available_mb,
                timeout_secs = cfg.language.memory_pause_timeout_secs,
                "language queue gave up waiting for memory; classifying this \
                 candidate as uncertain. Raise language.memory_pause_timeout_secs, \
                 lower language.resume_above_available_mb, or run with less \
                 competing for memory."
            );
            return Ok(LanguageMemory::TimedOut { waited_secs });
        }
        poll.wait_before_retry()
            .map_err(|e| crate::error::CoreError::Config(e.to_string()))?;
        state = probe
            .sample(state, thresholds)
            .map_err(|e| crate::error::CoreError::Config(e.to_string()))?;
    }
    tracing::info!("language queue resumed after memory pressure cleared");
    Ok(LanguageMemory::Ready)
}

fn parse_stored_event(
    raw: &str,
    candidates: &[SentenceCandidate],
) -> Result<StoredEventClassification> {
    let stored: StoredEventClassification = serde_json::from_str(raw)?;
    if stored.record_type != "event_classification" || stored.results.len() != candidates.len() {
        return Err(crate::error::CoreError::Integrity(
            "classification annotation does not match its event sentence frontier".to_string(),
        ));
    }
    for (result, candidate) in stored.results.iter().zip(candidates) {
        if !result.confidence.is_finite() || !(0.0..=1.0).contains(&result.confidence) {
            return Err(crate::error::CoreError::Integrity(
                "classification annotation contains confidence outside 0..=1".to_string(),
            ));
        }
        if result.attempts > 2
            || (result.attempts == 0
                && (result.category != LanguageCategory::Uncertain || result.reason.is_none()))
        {
            return Err(crate::error::CoreError::Integrity(
                "classification annotation contains an impossible provider attempt count"
                    .to_string(),
            ));
        }
        if result.category == LanguageCategory::Uncertain
            && (result.speech_act != SpeechAct::None || result.label.is_some())
        {
            return Err(crate::error::CoreError::Integrity(
                "uncertain classification annotation carries a speech act or label".to_string(),
            ));
        }
        if result.category != LanguageCategory::Uncertain && result.reason.is_some() {
            return Err(crate::error::CoreError::Integrity(
                "classified language annotation carries provider failure prose".to_string(),
            ));
        }
        if result.span_start != candidate.span_start || result.span_end != candidate.span_end {
            return Err(crate::error::CoreError::Integrity(
                "classification annotation span does not match immutable event text".to_string(),
            ));
        }
    }
    Ok(stored)
}

fn classification_admits_candidate(
    candidate: &SentenceCandidate,
    result: &StoredCandidateClassification,
    confidence_threshold: f32,
) -> bool {
    result.confidence >= confidence_threshold
        && candidate.heuristic.as_ref().is_some_and(|claim| {
            language_category_agrees(claim.kind, result.category)
                && speech_act_allows_claim(claim.kind, result.speech_act)
        })
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .chars()
            .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
}

/// Resolve the exact per-event annotations named by the active completed
/// language manifest. Historical rows not named by that manifest are inert.
/// Every returned item remains shadow-only and carries its event/span receipt.
pub fn active_language_shadow_projection(
    ledger: &Ledger,
    project_id: &str,
) -> Result<Option<LanguageShadowProjection>> {
    let active_head: Option<(String, Option<String>)> = ledger
        .conn()
        .query_row(
            "SELECT extractor, manifest_annotation_id FROM claim_projection_heads
             WHERE project_id=?1",
            params![project_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((active_extractor, manifest_id)) = active_head else {
        return Ok(None);
    };
    let Some(manifest_id) = manifest_id else {
        return Ok(None);
    };
    let manifest_annotation =
        classification_store::find_by_id(ledger, &manifest_id)?.ok_or_else(|| {
            crate::error::CoreError::Integrity(format!(
                "active language manifest {manifest_id:?} is not a classification annotation"
            ))
        })?;
    let manifest: StoredClassificationRun = serde_json::from_str(&manifest_annotation.output_json)?;
    if manifest.record_type != "classification_run"
        || !manifest.complete
        || manifest.truth_bearing
        || manifest.extractor != active_extractor
        || manifest.prompt_revision != crate::language::PROMPT_REVISION
        || !is_lower_sha256(&manifest.claim_set_sha256)
        || manifest.accepted > manifest.candidates
        || !is_lower_sha256(&manifest_annotation.prompt_sha256)
        || !is_lower_sha256(&manifest_annotation.output_sha256)
        || !canonical_language_timestamp(&manifest_annotation.created_at)
    {
        return Err(crate::error::CoreError::Integrity(format!(
            "active language manifest {manifest_id:?} is incomplete, truth-bearing, or malformed"
        )));
    }

    let (model, model_digest) = split_safe_language_model_identity(&manifest_annotation.model)
        .map(|(model, digest)| (model.to_string(), digest.to_string()))
        .ok_or_else(|| {
            crate::error::CoreError::Integrity(format!(
                "active language manifest {manifest_id:?} has no bounded immutable model identity"
            ))
        })?;
    let confidence_threshold =
        language_extractor_threshold(&manifest.extractor, &manifest_annotation.model).ok_or_else(
            || {
                crate::error::CoreError::Integrity(format!(
                    "active language manifest {manifest_id:?} has a malformed extractor identity"
                ))
            },
        )?;

    let expected_events = statement_events(ledger, project_id)?;
    let Some((expected_anchor, ..)) = expected_events.first() else {
        return Err(crate::error::CoreError::Integrity(format!(
            "active language manifest {manifest_id:?} has no canonical project anchor"
        )));
    };
    if manifest_annotation.event_id != *expected_anchor
        || manifest.events != manifest.members.len() as u64
        || expected_events.len() != manifest.members.len()
        || expected_events
            .iter()
            .map(|(event_id, ..)| event_id)
            .ne(manifest.members.iter().map(|member| &member.event_id))
    {
        return Err(crate::error::CoreError::Integrity(format!(
            "active language manifest {manifest_id:?} does not name the exact project event frontier"
        )));
    }

    // The manifest and rebuildable claim projection crossed the visibility
    // boundary in one transaction. Re-hash that exact active projection at
    // export time. The member loop below independently reconstructs the same
    // projection from its frozen classification rows; both sides must agree.
    let mut active_claim_records = {
        let mut stmt = ledger.conn().prepare(
            "SELECT c.id, c.event_id, c.text, c.span_start, c.span_end,
                    c.extractor, c.kind, c.marker
               FROM claims c JOIN events e ON e.id = c.event_id
              WHERE e.project_id = ?1 ORDER BY c.id",
        )?;
        let records = stmt
            .query_map(params![project_id], |row| {
                Ok(ClaimProjectionRecord {
                    id: row.get(0)?,
                    event_id: row.get(1)?,
                    text: row.get(2)?,
                    span_start: row.get(3)?,
                    span_end: row.get(4)?,
                    extractor: row.get(5)?,
                    kind: row.get(6)?,
                    marker: row.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        records
    };

    let mut member_ids = BTreeSet::new();
    let mut event_ids = BTreeSet::new();
    let mut admitted_claims = Vec::new();
    let mut run_keys = String::new();
    let mut items = Vec::new();
    let mut categories = BTreeMap::new();
    let mut speech_acts = BTreeMap::new();
    let mut classified = 0u64;
    let mut uncertain = 0u64;
    let mut labeled = 0u64;
    let mut attempts = 0u64;

    for (member, (_, session_id, seq, text)) in manifest.members.iter().zip(expected_events.iter())
    {
        if !event_ids.insert(member.event_id.as_str())
            || !member_ids.insert(member.annotation_id.as_str())
        {
            return Err(crate::error::CoreError::Integrity(format!(
                "active language manifest {manifest_id:?} contains duplicate membership"
            )));
        }
        let annotation = classification_store::find_by_id(ledger, &member.annotation_id)?
            .ok_or_else(|| {
                crate::error::CoreError::Integrity(format!(
                    "language manifest member {:?} is not a classification annotation",
                    member.annotation_id
                ))
            })?;
        if annotation.event_id != member.event_id || annotation.model != manifest_annotation.model {
            return Err(crate::error::CoreError::Integrity(format!(
                "language manifest member {:?} is bound to the wrong event or model",
                member.annotation_id
            )));
        }
        if !is_lower_sha256(&annotation.prompt_sha256)
            || !is_lower_sha256(&annotation.output_sha256)
            || !canonical_language_timestamp(&annotation.created_at)
        {
            return Err(crate::error::CoreError::Integrity(format!(
                "language manifest member {:?} has malformed provenance digests",
                member.annotation_id
            )));
        }
        let (candidates, _messages, prompt) = language_event_inputs(text);
        let expected_prompt_sha256 = sha256_hex(prompt.as_bytes());
        if annotation.prompt_sha256 != expected_prompt_sha256
            || annotation.id
                != classification_store::annotation_id(
                    &annotation.event_id,
                    &annotation.model,
                    &annotation.prompt_sha256,
                )
        {
            return Err(crate::error::CoreError::Integrity(format!(
                "language manifest member {:?} does not match its canonical input receipt",
                member.annotation_id
            )));
        }
        append_classification_run_key(&mut run_keys, &member.event_id, &expected_prompt_sha256);
        let stored = parse_stored_event(&annotation.output_json, &candidates)?;
        for (candidate, result) in candidates.iter().zip(&stored.results) {
            if classification_admits_candidate(candidate, result, confidence_threshold) {
                if let Some(claim) = candidate.heuristic.as_ref() {
                    admitted_claims.push((member.event_id.clone(), claim.clone()));
                }
            }
            let category = result.category.as_str().to_string();
            let speech_act = result.speech_act.as_str().to_string();
            *categories.entry(category.clone()).or_insert(0) += 1;
            *speech_acts.entry(speech_act.clone()).or_insert(0) += 1;
            if result.category == LanguageCategory::Uncertain {
                uncertain += 1;
            } else {
                classified += 1;
            }
            labeled += u64::from(result.label.is_some());
            attempts += u64::from(result.attempts);
            items.push(LanguageShadowItem {
                annotation_id: annotation.id.clone(),
                event_id: member.event_id.clone(),
                session_id: session_id.clone(),
                seq: *seq,
                span: [result.span_start, result.span_end],
                category,
                speech_act,
                label: result
                    .label
                    .as_ref()
                    .map(|label| label.as_str().to_string()),
                attempts: result.attempts,
                prompt_sha256: annotation.prompt_sha256.clone(),
                output_sha256: annotation.output_sha256.clone(),
            });
        }
    }
    if manifest.candidates != items.len() as u64 || manifest.candidates != classified + uncertain {
        return Err(crate::error::CoreError::Integrity(format!(
            "active language manifest {manifest_id:?} candidate coverage does not match its members"
        )));
    }

    let mut reconstructed_claim_records =
        projected_claim_records(&manifest.extractor, &admitted_claims);
    active_claim_records.sort();
    reconstructed_claim_records.sort();
    if manifest.accepted != reconstructed_claim_records.len() as u64
        || active_claim_records != reconstructed_claim_records
        || classification_store::claim_projection_sha256(reconstructed_claim_records.clone())?
            != manifest.claim_set_sha256
    {
        return Err(crate::error::CoreError::Integrity(format!(
            "active language manifest {manifest_id:?} does not match its classified claim projection"
        )));
    }

    let expected_run_prompt =
        classification_run_prompt(&manifest.extractor, &manifest.claim_set_sha256, &run_keys);
    if manifest_annotation.prompt_sha256 != sha256_hex(expected_run_prompt.as_bytes())
        || manifest_annotation.id
            != classification_store::annotation_id(
                &manifest_annotation.event_id,
                &manifest_annotation.model,
                &manifest_annotation.prompt_sha256,
            )
    {
        return Err(crate::error::CoreError::Integrity(format!(
            "active language manifest {manifest_id:?} does not match its canonical run receipt"
        )));
    }

    Ok(Some(LanguageShadowProjection {
        truth_bearing: false,
        scope: "at_export",
        run: LanguageShadowRun {
            annotation_id: manifest_annotation.id,
            model,
            model_digest,
            prompt_revision: manifest.prompt_revision,
            extractor: manifest.extractor,
            created_at: manifest_annotation.created_at,
            prompt_sha256: manifest_annotation.prompt_sha256,
            output_sha256: manifest_annotation.output_sha256,
        },
        coverage: LanguageShadowCoverage {
            events: manifest.events,
            candidates: manifest.candidates,
            classified,
            uncertain,
            labeled,
            accepted_claims: manifest.accepted,
            attempts,
            categories,
            speech_acts,
        },
        items,
    }))
}

/// M4.2 extraction pass. The model sees sentence language plus adjacent
/// sentence context only; evidence and every truth-verdict type are absent
/// from this API. A heuristic claim survives only on category agreement.
pub fn extract_language_claims(
    ledger: &Ledger,
    project_id: &str,
    cfg: &Config,
    annotator: &dyn LanguageAnnotator,
    model_tag: &str,
) -> Result<AnnotateReport> {
    let (probe, poll) = production_language_memory();
    extract_language_claims_with_memory(
        ledger, project_id, cfg, annotator, model_tag, &probe, &poll,
    )
}

/// Deterministic dependency-injection seam for integration tests. Normal
/// product construction always enters through [`extract_language_claims`],
/// which installs the real system probe and fixed 250 ms poll.
#[doc(hidden)]
pub fn extract_language_claims_with_memory(
    ledger: &Ledger,
    project_id: &str,
    cfg: &Config,
    annotator: &dyn LanguageAnnotator,
    model_tag: &str,
    probe: &dyn MemoryPressureProbe,
    poll: &dyn MemoryPressurePoll,
) -> Result<AnnotateReport> {
    cfg.validate()?;
    validate_language_model_tag(model_tag)?;
    let t0 = std::time::Instant::now();
    let events = statement_events(ledger, project_id)?;
    let mut r = AnnotateReport {
        events_scanned: events.len() as u64,
        language_model: Some(model_tag.to_string()),
        ..Default::default()
    };
    let created_at = crate::ingest::iso8601_utc_now();
    let mut active_claims: Vec<(String, Claim)> = Vec::new();
    let mut run_members: Vec<ClassificationRunMember> = Vec::new();
    let mut run_keys = String::new();

    for (event_id, _session, _seq, text) in &events {
        let (candidates, messages, prompt) = language_event_inputs(text);
        r.classification_candidates += candidates.len() as u64;
        append_classification_run_key(&mut run_keys, event_id, &sha256_hex(prompt.as_bytes()));

        let (stored, annotation_id) =
            match classification_store::find(ledger, event_id, model_tag, &prompt)? {
                Some(annotation) => {
                    r.classification_annotations_reused += 1;
                    (
                        parse_stored_event(&annotation.output_json, &candidates)?,
                        annotation.id,
                    )
                }
                None => {
                    let mut results = Vec::with_capacity(candidates.len());
                    for (candidate, message) in candidates.iter().zip(&messages) {
                        // The language tier is skipped for this candidate when
                        // memory never freed. `Uncertain` with a stated reason is
                        // the slot the design already has for "nobody could
                        // classify this" — `parse_stored_event` accepts exactly
                        // `attempts: 0` with `Uncertain` and a reason — so this
                        // needs no new state and cannot become a guessed
                        // category.
                        let pressure = wait_for_language_memory(cfg, &mut r, probe, poll)?;
                        if let LanguageMemory::TimedOut { waited_secs } = pressure {
                            results.push(StoredCandidateClassification {
                                span_start: candidate.span_start,
                                span_end: candidate.span_end,
                                category: LanguageCategory::Uncertain,
                                confidence: 0.0,
                                speech_act: SpeechAct::None,
                                label: None,
                                attempts: 0,
                                reason: Some(format!(
                                    "language queue gave up after {waited_secs}s waiting \
                                     for memory above {} MB",
                                    cfg.language.resume_above_available_mb
                                )),
                            });
                            continue;
                        }
                        let result = match annotator.classify(message) {
                            ClassificationOutcome::Classified { output, attempts } => {
                                r.classification_attempts += u64::from(attempts);
                                StoredCandidateClassification {
                                    span_start: candidate.span_start,
                                    span_end: candidate.span_end,
                                    category: output.category,
                                    confidence: output.confidence.get(),
                                    speech_act: output.speech_act,
                                    label: output.label,
                                    attempts,
                                    reason: None,
                                }
                            }
                            ClassificationOutcome::Uncertain { attempts, reason } => {
                                r.classification_attempts += u64::from(attempts);
                                StoredCandidateClassification {
                                    span_start: candidate.span_start,
                                    span_end: candidate.span_end,
                                    category: LanguageCategory::Uncertain,
                                    confidence: 0.0,
                                    speech_act: SpeechAct::None,
                                    label: None,
                                    attempts,
                                    reason: Some(uncertain_reason(&reason)),
                                }
                            }
                        };
                        results.push(result);
                    }
                    let event_record = StoredEventClassification {
                        record_type: "event_classification".to_string(),
                        results,
                    };
                    let output_json = serde_json::to_string(&event_record)?;
                    let confidence = if event_record.results.is_empty() {
                        1.0
                    } else {
                        event_record
                            .results
                            .iter()
                            .map(|result| f64::from(result.confidence))
                            .sum::<f64>()
                            / event_record.results.len() as f64
                    };
                    let result = classification_store::store(
                        ledger,
                        NewClassification {
                            event_id,
                            model: model_tag,
                            prompt: &prompt,
                            output_json: &output_json,
                            confidence,
                            created_at: &created_at,
                        },
                    )?;
                    match result.disposition {
                        StoreDisposition::Inserted => r.classification_annotations_inserted += 1,
                        StoreDisposition::Existing => r.classification_annotations_reused += 1,
                    }
                    (
                        parse_stored_event(&result.annotation.output_json, &candidates)?,
                        result.annotation.id,
                    )
                }
            };
        run_members.push(ClassificationRunMember {
            event_id: event_id.clone(),
            annotation_id,
        });

        for (candidate, result) in candidates.iter().zip(&stored.results) {
            let confident = result.confidence >= cfg.language.confidence_threshold;
            if result.category == LanguageCategory::Uncertain || !confident {
                r.classification_uncertain += 1;
            }
            if let Some(claim) = candidate.heuristic.as_ref() {
                if classification_admits_candidate(
                    candidate,
                    result,
                    cfg.language.confidence_threshold,
                ) {
                    active_claims.push((event_id.clone(), claim.clone()));
                    r.classification_accepted += 1;
                } else {
                    r.classification_demoted += 1;
                }
            }
        }
        r.classification_events += 1;
    }

    let extractor = language_extractor_identity(model_tag, cfg.language.confidence_threshold);

    if let Some((anchor, ..)) = events.first() {
        let claim_set_sha256 = classification_store::claim_projection_sha256(
            projected_claim_records(&extractor, &active_claims),
        )?;
        let run_prompt = classification_run_prompt(&extractor, &claim_set_sha256, &run_keys);
        let run_json = serde_json::to_string(&serde_json::json!({
            "type": "classification_run",
            "complete": true,
            "truth_bearing": false,
            "prompt_revision": crate::language::PROMPT_REVISION,
            "extractor": extractor,
            "claim_set_sha256": claim_set_sha256,
            "events": r.classification_events,
            "candidates": r.classification_candidates,
            "accepted": r.classification_accepted,
            "members": run_members,
        }))?;
        persist_language_projection(
            ledger,
            project_id,
            &extractor,
            &active_claims,
            ProjectionManifest {
                anchor,
                model_tag,
                prompt: &run_prompt,
                json: &run_json,
                created_at: &created_at,
            },
            &mut r,
        )?;
    } else {
        r.extractor = extractor;
    }
    r.classification_ms = t0.elapsed().as_millis();
    r.extract_ms = r.classification_ms;
    Ok(r)
}

/// One claim as the scoring pass reads it back.
struct StoredClaim {
    id: String,
    text: String,
    kind: String,
    session_id: String,
    seq: i64,
    /// Position of the claim's session in project chronology.
    session_rank: usize,
    /// §5m turn number: count of user messages at-or-before the claim.
    turn: i64,
}

fn session_turns(ledger: &Ledger, project_id: &str) -> Result<HashMap<String, Vec<i64>>> {
    let mut stmt = ledger.conn().prepare(
        "SELECT session_id, seq FROM events
         WHERE project_id = ?1 AND kind = 'user_msg'
         ORDER BY session_id, seq",
    )?;
    let rows = stmt.query_map(params![project_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
    })?;
    let mut turns: HashMap<String, Vec<i64>> = HashMap::new();
    for row in rows {
        let (session, seq) = row?;
        turns.entry(session).or_default().push(seq);
    }
    Ok(turns)
}

fn turn_at(turns: &HashMap<String, Vec<i64>>, session_id: &str, seq: i64) -> i64 {
    turns
        .get(session_id)
        .map(|v| v.partition_point(|q| *q <= seq) as i64)
        .unwrap_or(0)
}

fn load_claims(ledger: &Ledger, project_id: &str) -> Result<Vec<StoredClaim>> {
    // Session chronology, resolved once — three integers per session, not a
    // graph.
    let mut ranks = ledger.conn().prepare(
        "SELECT id FROM sessions WHERE project_id = ?1
         ORDER BY (started_at IS NULL), started_at, id",
    )?;
    let order: Vec<String> = ranks
        .query_map(params![project_id], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<_, _>>()?;
    drop(ranks);
    let turns = session_turns(ledger, project_id)?;

    let mut stmt = ledger.conn().prepare(
        "SELECT c.id, c.event_id, c.text, c.kind, e.session_id, e.seq
         FROM claims c JOIN events e ON e.id = c.event_id
         WHERE e.project_id = ?1
         ORDER BY c.id",
    )?;
    let rows = stmt.query_map(params![project_id], |r| {
        Ok(StoredClaim {
            id: r.get(0)?,
            text: r.get(2)?,
            kind: r.get(3)?,
            session_id: r.get(4)?,
            seq: r.get(5)?,
            session_rank: 0,
            turn: 0,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        let mut c = row?;
        c.session_rank = order.iter().position(|s| *s == c.session_id).unwrap_or(0);
        c.turn = turn_at(&turns, &c.session_id, c.seq);
        out.push(c);
    }
    Ok(out)
}

/// Events that can carry action proof.
///
/// Task notifications are user-message envelopes in Claude Code's JSONL, not
/// `tool_use` rows. Excluding them made completed subagent/workflow records
/// literally invisible to retrieval even though their result was captured in
/// full. The XML prefix is structural provenance, not a vocabulary guess.
const EVIDENCE_EVENT_PREDICATE: &str = "(e.kind = 'tool_use' OR
    (e.kind = 'user_msg' AND e.text LIKE '<task-notification>%'))";

/// §5r's evidence-profile vocabulary.
///
/// This is deliberately about record families, not verdicts. An unreadable
/// `cargo test` result is still a `test` record in the project profile; calling
/// it generic shell evidence would make the very family we failed to read
/// disappear from the coverage gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum ScanFamily {
    Test,
    Build,
    Git,
    Agent,
    FileChange,
    Shell,
    ReadSearch,
    Mcp,
    Other,
}

impl ScanFamily {
    fn as_str(self) -> &'static str {
        match self {
            ScanFamily::Test => "test",
            ScanFamily::Build => "build/lint",
            ScanFamily::Git => "git",
            ScanFamily::Agent => "agent/task",
            ScanFamily::FileChange => "edit/write",
            ScanFamily::Shell => "shell receipts",
            ScanFamily::ReadSearch => "read/search records",
            ScanFamily::Mcp => "MCP records",
            ScanFamily::Other => "other tool records",
        }
    }
}

#[derive(Default)]
struct EvidenceProfile {
    by_id: HashMap<String, ScanFamily>,
    turn_by_id: HashMap<String, i64>,
    counts: BTreeMap<ScanFamily, usize>,
    dominant: BTreeSet<ScanFamily>,
}

fn scan_family(row: &crate::derive::EvidenceRow, cache: &mut RawScopeCache) -> ScanFamily {
    if is_agent_record(row) {
        return ScanFamily::Agent;
    }
    let identity = row_identity(row, cache);
    let invocation = identity.rsplit(':').next().unwrap_or("");
    if invocation.starts_with("git ") {
        return ScanFamily::Git;
    }
    if crate::identity::is_test_invocation(&identity) {
        return ScanFamily::Test;
    }
    if crate::identity::is_build_invocation(&identity) {
        return ScanFamily::Build;
    }
    if matches!(
        row.tool_name.as_str(),
        "Edit" | "Write" | "NotebookEdit" | "MultiEdit" | "Update"
    ) {
        return ScanFamily::FileChange;
    }
    if matches!(
        row.tool_name.as_str(),
        "Read" | "Grep" | "Glob" | "LS" | "Search"
    ) {
        return ScanFamily::ReadSearch;
    }
    if row.tool_name.starts_with("mcp__") {
        return ScanFamily::Mcp;
    }
    if matches!(row.tool_name.as_str(), "Bash" | "Shell" | "Terminal") {
        return ScanFamily::Shell;
    }
    ScanFamily::Other
}

fn evidence_profile(
    ledger: &Ledger,
    project_id: &str,
    cache: &mut RawScopeCache,
) -> Result<EvidenceProfile> {
    let turns = session_turns(ledger, project_id)?;
    let mut stmt = ledger.conn().prepare(&format!(
        "SELECT e.id, e.session_id, e.seq FROM events e
         WHERE e.project_id = ?1 AND {EVIDENCE_EVENT_PREDICATE}
         ORDER BY e.session_id, e.seq"
    ))?;
    let ids: Vec<(String, String, i64)> = stmt
        .query_map(params![project_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<std::result::Result<_, _>>()?;
    drop(stmt);

    let mut profile = EvidenceProfile::default();
    for (id, session_id, seq) in ids {
        let Some(row) = evidence_row_for(ledger, &id)? else {
            continue;
        };
        let family = scan_family(&row, cache);
        profile
            .turn_by_id
            .insert(id.clone(), turn_at(&turns, &session_id, seq));
        profile.by_id.insert(id, family);
        *profile.counts.entry(family).or_default() += 1;
    }
    let total: usize = profile.counts.values().sum();
    if total > 0 {
        for (&family, &count) in &profile.counts {
            // Project-dominant means at least ten percent of captured evidence.
            // A locally dominant family below that threshold is added per
            // claim by `coverage_expectation`.
            if count.saturating_mul(10) >= total {
                profile.dominant.insert(family);
            }
        }
    }
    Ok(profile)
}

struct CoverageExpectation {
    required: BTreeMap<ScanFamily, BTreeSet<String>>,
}

impl CoverageExpectation {
    fn assess(&self, parsed: &BTreeSet<String>) -> (bool, Vec<ScanFamily>) {
        let missing: Vec<ScanFamily> = self
            .required
            .iter()
            .filter_map(|(&family, ids)| {
                (!ids.iter().all(|id| parsed.contains(id))).then_some(family)
            })
            .collect();
        (missing.is_empty(), missing)
    }
}

fn coverage_expectation(
    ledger: &Ledger,
    c: &StoredClaim,
    k: usize,
    profile: &EvidenceProfile,
) -> Result<CoverageExpectation> {
    let lo = c.seq - k as i64;
    let hi = c.seq + k as i64;
    let mut stmt = ledger.conn().prepare(&format!(
        "SELECT e.id FROM events e
         WHERE e.session_id = ?1 AND {EVIDENCE_EVENT_PREDICATE}
           AND e.seq BETWEEN ?2 AND ?3
         ORDER BY e.seq"
    ))?;
    let ids: Vec<String> = stmt
        .query_map(params![c.session_id, lo, hi], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    drop(stmt);

    let mut local: BTreeMap<ScanFamily, BTreeSet<String>> = BTreeMap::new();
    for id in ids {
        // Post-claim turns are context, not part of a comprehensive backward
        // scan (§5m). Same-turn evidence remains eligible regardless of order.
        if profile.turn_by_id.get(&id).copied().unwrap_or(0) > c.turn {
            continue;
        }
        let family = profile.by_id.get(&id).copied().unwrap_or(ScanFamily::Other);
        local.entry(family).or_default().insert(id);
    }
    let total: usize = local.values().map(BTreeSet::len).sum();
    let required = local
        .into_iter()
        .filter(|(family, ids)| {
            profile.dominant.contains(family) || (total > 0 && ids.len().saturating_mul(5) >= total)
        })
        .collect();
    Ok(CoverageExpectation { required })
}

/// Run the exact §5r family/candidate computation without scoring or writing.
///
/// Live projects are measurements, never development loops (Rule 13). This
/// helper makes the one permitted before/after measurement cheap and
/// read-only: candidate selection, claim-focused rendering, and the coverage
/// assessment are production code; the existing frozen verdict layer supplies
/// the old would-be-`unsupported` set.
pub fn coverage_dry_run(ledger: &Ledger, project_id: &str, cfg: &Config) -> Result<CoverageDryRun> {
    cfg.validate()?;
    let claims = load_claims(ledger, project_id)?;
    let mut sessions = ledger.conn().prepare(
        "SELECT id FROM sessions WHERE project_id = ?1
         ORDER BY (started_at IS NULL), started_at, id",
    )?;
    let session_order: Vec<String> = sessions
        .query_map(params![project_id], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    drop(sessions);

    let mut cwd_cache = RawScopeCache::default();
    let profile = evidence_profile(ledger, project_id, &mut cwd_cache)?;
    let would_be = crate::derive::would_be_unsupported_claims(ledger, project_id)?;
    let mut eligible_ids = BTreeSet::new();
    let mut report = CoverageDryRun {
        would_be_unsupported: would_be.len() as u64,
        project_family_profile: profile
            .counts
            .iter()
            .map(|(family, count)| (family.as_str().to_string(), *count as u64))
            .collect(),
        ..CoverageDryRun::default()
    };

    for c in claims
        .iter()
        .filter(|c| c.kind == ClaimKind::Work.as_str() || c.kind == ClaimKind::Marker.as_str())
    {
        report.claims += 1;
        let expectation = coverage_expectation(ledger, c, cfg.backward.k_events, &profile)?;
        let mut candidates = backward_candidates(
            ledger,
            c,
            cfg.backward.k_events,
            cfg.backward.max_candidates,
        )?;
        let arts = crate::extract::artifacts(&c.text);
        let terms = retrieval_tokens(&c.text, &arts);
        for id in backward_lexical_candidates(
            ledger,
            project_id,
            c,
            &terms,
            &session_order,
            cfg.backward.max_candidates,
        )? {
            if !candidates.contains(&id) {
                candidates.push(id);
            }
        }

        let mut parsed = BTreeSet::new();
        for id in candidates {
            let Some(row) = evidence_row_for(ledger, &id)? else {
                continue;
            };
            let detail = evidence_detail_for(ledger, &id)?;
            if render_tagged_for_claim(&row, &c.text, detail.as_deref()).fidelity
                == Fidelity::Parsed
            {
                parsed.insert(id);
            }
        }
        let (eligible, missing) = expectation.assess(&parsed);
        if eligible {
            report.coverage_eligible += 1;
            eligible_ids.insert(c.id.clone());
        } else {
            for family in missing {
                *report
                    .missing_by_family
                    .entry(family.as_str().to_string())
                    .or_default() += 1;
            }
        }
    }

    report.now_emitted = would_be.intersection(&eligible_ids).count() as u64;
    report.now_unknown = report.would_be_unsupported - report.now_emitted;
    Ok(report)
}

/// Backward candidates: `tool_use` events near the claim, in the same session.
///
/// §5 says "action clusters in same turn ± k events". Implemented as a seq
/// window rather than a turn walk — a turn is bounded by user messages, and at
/// k = 40 the window covers it without holding the turn structure in memory.
/// Nearest-first so the cap keeps the most relevant candidates.
fn backward_candidates(
    ledger: &Ledger,
    c: &StoredClaim,
    k: usize,
    cap: usize,
) -> Result<Vec<String>> {
    let mut stmt = ledger.conn().prepare(&format!(
        "SELECT e.id FROM events e
         WHERE e.session_id = ?1 AND {EVIDENCE_EVENT_PREDICATE}
           AND e.seq BETWEEN ?2 AND ?3
         ORDER BY ABS(e.seq - ?4), e.seq
         LIMIT ?5"
    ))?;
    let lo = c.seq - k as i64;
    let hi = c.seq + k as i64;
    let rows = stmt.query_map(params![c.session_id, lo, hi, c.seq, cap as i64], |r| {
        r.get::<_, String>(0)
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Earlier actions anywhere in the project that mention what the claim names.
///
/// The ±k window assumes a claim sits near the work it describes. Session
/// recaps break that assumption completely: "M1 is complete — all five
/// sub-checkpoints committed", "rustup was installed with `--no-modify-path`"
/// summarise a whole session from its end, hundreds of events after the work.
/// With only a local window, "no matching action found" is guaranteed for
/// those claims **whether or not they are true** — the product would be
/// accusing on the strength of where a sentence sits.
///
/// So a work claim gets the same lexically-gated search the forward pass
/// already gives proposals, pointed backwards. Bounded by the same cap, and
/// only ever *adding* candidates: evidence can only make an accusation less
/// likely, never more.
fn backward_lexical_candidates(
    ledger: &Ledger,
    project_id: &str,
    c: &StoredClaim,
    terms: &[String],
    session_order: &[String],
    cap: usize,
) -> Result<Vec<String>> {
    let toks = gate_tokens(terms);
    if toks.is_empty() {
        return Ok(Vec::new());
    }
    let likes: Vec<String> = toks.iter().map(|t| format!("%{t}%")).collect();
    let ors = (0..likes.len())
        .map(|i| searchable_action_clause_raw(i + 4))
        .collect::<Vec<_>>()
        .join(" OR ");

    let mut out: Vec<String> = Vec::new();
    // Earlier in the claim's own session first — nearest context wins.
    {
        let sql = format!(
            "SELECT e.id FROM events e
             WHERE e.project_id = ?1 AND e.session_id = ?2
               AND {EVIDENCE_EVENT_PREDICATE}
               AND e.seq < ?3 AND ({ors})
             ORDER BY e.seq DESC LIMIT {cap}"
        );
        let mut stmt = ledger.conn().prepare(&sql)?;
        let mut binds: Vec<&dyn rusqlite::ToSql> = vec![&project_id, &c.session_id, &c.seq];
        for l in &likes {
            binds.push(l);
        }
        let rows = stmt.query_map(binds.as_slice(), |r| r.get::<_, String>(0))?;
        for r in rows {
            out.push(r?);
        }
    }
    // Deliberately NOT extended across sessions.
    //
    // A recap summarises the session it closes, so same-session is where its
    // evidence is. Reaching into earlier sessions buys little and costs real
    // detection: measured on the M3.8c corpus, a project-wide backward search
    // dropped planted-fake recall from 8/8 to 6/8, because a fabricated claim
    // will usually find *some* unrelated earlier action mentioning one of its
    // tokens. Silence has to stay meaningful for the accusation to mean
    // anything, and that means bounding where we look.
    let _ = session_order;
    out.truncate(cap);
    Ok(out)
}

/// Has the project moved on from this proposal?
///
/// A proposal in the newest session, or with few later user turns, is
/// **pending**, not dropped. Only once the work has visibly continued past it
/// does silence start to mean something.
fn project_moved_on(
    ledger: &Ledger,
    project_id: &str,
    c: &StoredClaim,
    session_count: usize,
    min_turns: usize,
) -> Result<bool> {
    if c.session_rank + 1 < session_count {
        return Ok(true);
    }
    let later: i64 = ledger.conn().query_row(
        "SELECT COUNT(*) FROM events
         WHERE project_id = ?1 AND session_id = ?2 AND kind = 'user_msg' AND seq > ?3",
        params![project_id, c.session_id, c.seq],
        |r| r.get(0),
    )?;
    Ok(later >= min_turns as i64)
}

/// Forward candidates: later `tool_use` events project-wide, lexically gated.
///
/// The gate is what keeps this affordable. Without it every proposal would be
/// scored against every subsequent action in the project; with it, only actions
/// whose tool input mentions one of the claim's artifact tokens are considered.
/// Tokens are chosen deterministically (longest first, then lexicographic) so
/// the same claim always gates on the same terms.
fn forward_candidates(
    ledger: &Ledger,
    project_id: &str,
    c: &StoredClaim,
    terms: &[String],
    session_order: &[String],
    cfg: &Config,
) -> Result<Vec<String>> {
    let toks = gate_tokens(terms);
    if toks.len() < cfg.forward.min_lexical_overlap.max(1) {
        return Ok(Vec::new());
    }

    // Sessions strictly after the claim's, plus the tail of its own session.
    let later_sessions: Vec<&String> = session_order
        .iter()
        .skip(c.session_rank + 1)
        .collect::<Vec<_>>();

    let likes: Vec<String> = toks.iter().map(|t| format!("%{t}%")).collect();
    let ors = (0..likes.len())
        .map(|i| searchable_action_clause_raw(i + 4))
        .collect::<Vec<_>>()
        .join(" OR ");

    let mut out: Vec<String> = Vec::new();

    // Same session, later in it.
    {
        let sql = format!(
            "SELECT e.id FROM events e
             WHERE e.project_id = ?1 AND e.session_id = ?2
               AND {EVIDENCE_EVENT_PREDICATE}
               AND e.seq > ?3 AND ({ors})
             ORDER BY e.seq LIMIT {}",
            cfg.forward.max_candidates
        );
        let mut stmt = ledger.conn().prepare(&sql)?;
        let mut binds: Vec<&dyn rusqlite::ToSql> = vec![&project_id, &c.session_id, &c.seq];
        for l in &likes {
            binds.push(l);
        }
        let rows = stmt.query_map(binds.as_slice(), |r| r.get::<_, String>(0))?;
        for r in rows {
            out.push(r?);
        }
    }

    // Later sessions, in chronological order, until the cap is full.
    for s in later_sessions {
        if out.len() >= cfg.forward.max_candidates {
            break;
        }
        let remaining = cfg.forward.max_candidates - out.len();
        let sql = format!(
            "SELECT e.id FROM events e
             WHERE e.project_id = ?1 AND e.session_id = ?2
               AND {EVIDENCE_EVENT_PREDICATE}
               AND ({ors})
             ORDER BY e.seq LIMIT {remaining}"
        );
        let mut stmt = ledger.conn().prepare(&sql)?;
        // ?3 is unused in this variant but keeps the LIKE placeholders at ?4+.
        let unused = 0i64;
        let mut binds: Vec<&dyn rusqlite::ToSql> = vec![&project_id, s, &unused];
        for l in &likes {
            binds.push(l);
        }
        let rows = stmt.query_map(binds.as_slice(), |r| r.get::<_, String>(0))?;
        for r in rows {
            out.push(r?);
        }
    }
    out.truncate(cfg.forward.max_candidates);
    Ok(out)
}

/// Do the claim and the premise talk about the same thing at all?
///
/// This is the separator the scorer cannot provide. Measured in M3.6: a TRUE
/// claim the model merely fails to confirm and a FABRICATED claim with no
/// supporting action are numerically indistinguishable to it — both land at
/// neutral ≈ 0.99. What distinguishes them is whether the evidence is *about*
/// the claim. Zero shared content words means nothing recorded even mentions
/// what was claimed, which is the honest basis for "no matching action found".
///
/// The failure direction is deliberate: a fabricated claim that happens to
/// share a word with unrelated evidence abstains instead of warning. That
/// costs recall and protects precision, which is the trade this product is
/// built around.
pub fn topically_related(claim: &str, premise: &str) -> bool {
    relatedness(claim, premise) > 0.0
}

/// *How strongly* the premise is about the claim, in `0.0..=1.0`.
///
/// A32 made the bool above insufficient. "Is this related at all?" is the right
/// question for admitting evidence and the wrong one for choosing which single
/// piece of evidence an accusation rests on: a claim with no distinctive token
/// is weakly related to *every* nearby action, so a scan over 30 admitted
/// candidates finds a false conflict with probability `1 − (1−p)^30` ≈ 0.96 at
/// p = 0.10. Derivation uses this to rank, and accuses only on the top of the
/// ranking — so the exposure stops growing with the size of the session.
///
/// The two tiers never interleave, which is the point: an artifact match is
/// evidence that names the same thing, a word match is evidence that merely
/// shares vocabulary, and no amount of the latter may outrank the former.
///
/// * `0.6..=1.0` — the claim names an artifact and the premise contains it.
/// * `0.1..=0.5` — no artifact to match on; ordinary content words overlap.
/// * `0.0`       — unrelated, and inadmissible as evidence either way.
pub fn relatedness(claim: &str, premise: &str) -> f32 {
    // Distinctive tokens first: a backticked name, a path, an identifier or a
    // number the claim commits to. M3.7 adjudication showed why a plain
    // content-word overlap is not enough — real premises run to 300 characters
    // and share *some* ordinary word with almost any claim, so the test passed
    // vacuously and let the contradiction path accuse true statements.
    let claim_marks = crate::extract::artifacts(claim);
    if !claim_marks.is_empty() {
        let low = premise.to_lowercase();
        let hit = claim_marks
            .iter()
            .filter(|a| low.contains(a.as_str()))
            .count();
        if hit == 0 {
            return 0.0;
        }
        // Naming one of the claim's artifacts already clears the artifact tier;
        // naming more of them ranks higher still.
        return 0.6 + 0.4 * (hit as f32 / claim_marks.len() as f32);
    }
    const STOP: [&str; 30] = [
        "the", "and", "was", "were", "has", "have", "had", "for", "with", "that", "this", "from",
        "not", "all", "its", "his", "her", "our", "are", "but", "you", "any", "can", "out", "new",
        "now", "run", "ran", "one", "two",
    ];
    fn words(s: &str) -> BTreeSet<String> {
        // Structured MCP/task receipts commonly carry the only proof in JSON
        // keys (`flagResolved`, `rescue_done`). Split camelCase before
        // lowercasing so those affirmatively captured records are retrievable
        // by the ordinary words used in the claim. This expands recall only;
        // conviction still has the independent §5q identity/outcome gates.
        let mut split = String::with_capacity(s.len() + 8);
        let mut prev_lower_or_digit = false;
        for ch in s.chars() {
            if ch.is_ascii_uppercase() && prev_lower_or_digit {
                split.push(' ');
            }
            split.push(ch);
            prev_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        }
        split
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            // Numbers count from two digits up: "96 passing" against "All 96
            // tests passed" is exactly the kind of overlap that proves the
            // evidence is about the claim, and a length-3 floor threw it away.
            .filter(|w| {
                (w.len() >= 3 || (w.len() >= 2 && w.chars().all(|c| c.is_ascii_digit())))
                    && !STOP.contains(w)
            })
            .map(|w| w.to_string())
            .collect()
    }
    let (cw, pw) = (words(claim), words(premise));
    let shared = cw.intersection(&pw).count();
    if shared == 0 {
        return 0.0;
    }
    // Share of the CLAIM that the premise accounts for, not raw overlap: a
    // 300-character premise should not rank above a short one merely for being
    // long enough to contain more words.
    let covered = shared as f32 / cw.len().max(1) as f32;
    (0.1 + 0.4 * covered).min(0.5)
}

/// Gate tokens for a proposal, shared by the candidate search and the
/// name-match count so the two can never disagree about what was searched for.
fn gate_tokens(artifacts: &[String]) -> Vec<&String> {
    let mut toks: Vec<&String> = artifacts
        .iter()
        .filter(|t| t.len() >= 4 && t.len() <= 80)
        .collect();
    toks.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    toks.truncate(MAX_GATE_TOKENS);
    toks
}

/// Search text is wider than extraction identity. Round 3's five missed
/// fulfillments included ordinary targets (`gallery`, `archive`) that are not
/// code identifiers, while ten missed completion proofs frequently lived in
/// a correlated result rather than the tool input. Artifact names stay first;
/// a small stop-filtered content tail supplies the missing natural nouns.
fn retrieval_tokens(claim: &str, artifacts: &[String]) -> Vec<String> {
    const STOP: [&str; 51] = [
        "about",
        "after",
        "again",
        "against",
        "all",
        "also",
        "before",
        "both",
        "build",
        "built",
        "clean",
        "complete",
        "completed",
        "done",
        "failed",
        "failure",
        "final",
        "first",
        "green",
        "into",
        "latest",
        "later",
        "merged",
        "next",
        "nothing",
        "passed",
        "passing",
        "result",
        "results",
        "run",
        "running",
        "succeeded",
        "success",
        "tests",
        "then",
        "there",
        "these",
        "they",
        "this",
        "through",
        "verify",
        "verified",
        "with",
        "without",
        "would",
        "added",
        "attempt",
        "make",
        "render",
        "update",
        "using",
    ];
    let mut out: Vec<String> = artifacts
        .iter()
        .map(|s| s.to_ascii_lowercase())
        .filter(|s| (4..=80).contains(&s.len()))
        .collect();
    out.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    out.dedup();
    out.truncate(MAX_GATE_TOKENS);

    let low_claim = claim.to_ascii_lowercase();
    const ALIASES: [(&str, &str); 8] = [
        ("secret", "token"),
        ("marker", "pattern"),
        ("byte-identical", "digest"),
        ("corpora", "corpus"),
        ("preregistration", "prereg"),
        ("warm-up", "starting"),
        ("browser-harness", "browser"),
        ("placements", "placement"),
    ];
    for (source, alias) in ALIASES {
        if out.len() == MAX_GATE_TOKENS {
            break;
        }
        if low_claim.contains(source) && !out.iter().any(|t| t == alias) {
            out.push(alias.to_string());
        }
    }

    // A59 (the Fix-2 ghost): a short labeled identifier — a capitalised word
    // followed by a number ("Fix 2", "Bug 3", "Phase 1") — is often the ONLY
    // durable name a proposal carries, and its words individually sit under
    // the 4-char token floor. The ~12 follow-through edits for "Now mirroring
    // Fix 2 on iOS" all carried the literal comment "Fix 2"; no token
    // matched. The bigram is the token.
    {
        let mut prev: Option<&str> = None;
        for w in claim.split_whitespace() {
            let wt = w.trim_matches(|c: char| !c.is_alphanumeric());
            if let Some(p) = prev {
                if wt.chars().all(|c| c.is_ascii_digit())
                    && !wt.is_empty()
                    && p.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                    && p.len() >= 3
                {
                    let bigram = format!("{} {}", p.to_ascii_lowercase(), wt);
                    if out.len() < MAX_GATE_TOKENS && !out.contains(&bigram) {
                        out.push(bigram);
                    }
                }
            }
            prev = Some(wt);
        }
    }

    let mut words: Vec<String> = claim
        .to_ascii_lowercase()
        .split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| {
                !c.is_alphanumeric() && c != '_' && c != '/' && c != '.' && c != '-'
            })
            .trim_end_matches(['.', ',', ';', ':'])
            .trim_matches('`')
            .to_string()
        })
        .filter(|w| {
            (4..=80).contains(&w.len())
                && !w.chars().all(|c| c.is_ascii_digit())
                && !STOP.contains(&w.as_str())
        })
        .collect();
    words.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    words.dedup();
    for word in words {
        if out.len() == MAX_GATE_TOKENS {
            break;
        }
        if !out.iter().any(|t| t == &word) {
            out.push(word);
        }
    }
    out
}

/// One token may occur in the action request, tool name, or the bounded prefix
/// of its correlated result. The latter is where Task/TaskOutput and shell
/// summaries carry their proof. Reusing the same numbered placeholder keeps
/// the query and bind count deterministic.
/// §5j action-side text: what the assistant *did* — commands, edit/write
/// paths and content, tool names. All of it lives in `tool_input`.
fn action_side_clause(raw: usize, sq: usize) -> String {
    // The REPLACE chain is §5i's separator-squashing, ported to the §5j
    // action matcher (A59): `retry_budget.rs` in a proposal must find a
    // `RetryBudget` in an edit, exactly as `Pick-a-Scene` had to find
    // `pickAScene.ts`. `sq` binds the token with separators removed.
    format!(
        "(LOWER(COALESCE(e.tool_input,'')) LIKE ?{raw}
          OR LOWER(COALESCE(e.tool_name,'')) LIKE ?{raw}
          OR (e.kind = 'user_msg' AND e.text LIKE '<task-notification>%'
              AND LOWER(COALESCE(e.text,'')) LIKE ?{raw})
          OR REPLACE(REPLACE(REPLACE(REPLACE(LOWER(COALESCE(e.tool_input,'')),
               '_',''),'-',''),'.',''),' ','') LIKE ?{sq})"
    )
}

/// §5j output-side text: what later commands *printed*. A grep hit or a file
/// listing MENTIONS the target; it is not an action on it, and A52 measured
/// what happens when the two are conflated — 644 of 697 proposals "defeated"
/// by their own name scrolling past, and `dropped` unreachable.
fn output_side_clause(raw: usize, sq: usize) -> String {
    format!(
        "EXISTS (
            SELECT 1 FROM events tr
            WHERE tr.tool_use_id = e.tool_use_id AND tr.kind = 'tool_result'
              AND (LOWER(substr(COALESCE(tr.tool_result,''),1,8000)) LIKE ?{raw}
                   OR REPLACE(REPLACE(REPLACE(REPLACE(LOWER(substr(COALESCE(tr.tool_result,''),1,8000)),
                        '_',''),'-',''),'.',''),' ','') LIKE ?{sq})
        )"
    )
}

/// A45's original combined clause, kept for candidate RETRIEVAL only.
/// Retrieval being generous merely offers the scorer more evidence; the
/// defeater being generous kills `dropped` (§5j splits it).
/// Raw-only combined clause for candidate RETRIEVAL. Retrieval already errs
/// generous by design; the squashed variants exist for the §5j defeater,
/// where a spelling miss manufactures a ghost.
fn searchable_action_clause_raw(raw: usize) -> String {
    format!(
        "(LOWER(COALESCE(e.tool_input,'')) LIKE ?{raw}
          OR LOWER(COALESCE(e.tool_name,'')) LIKE ?{raw}
          OR (e.kind = 'user_msg' AND e.text LIKE '<task-notification>%'
              AND LOWER(COALESCE(e.text,'')) LIKE ?{raw})
          OR EXISTS (
              SELECT 1 FROM events tr
              WHERE tr.tool_use_id = e.tool_use_id AND tr.kind = 'tool_result'
                AND LOWER(substr(COALESCE(tr.tool_result,''),1,8000)) LIKE ?{raw}
          ))"
    )
}

fn defeat_terms(claim: &str, retrieval: &[String]) -> Vec<String> {
    let artifacts = crate::extract::artifacts(claim);
    if artifacts.is_empty() {
        return retrieval.to_vec();
    }
    let mut durable: Vec<String> = artifacts.iter().map(|a| a.to_ascii_lowercase()).collect();
    for artifact in &artifacts {
        if let Some(base) = artifact.rsplit('/').next() {
            if base.len() >= 4 && !durable.iter().any(|d| d == base) {
                durable.push(base.to_ascii_lowercase());
            }
        }
    }
    durable
}

/// How many later actions name this proposal's target — **uncapped**, and
/// independent of scoring.
///
/// §5c: "absence-of-proof verdicts must be defeated by cheaper positive
/// signals." A name match is exactly that. It does not prove the proposal was
/// fulfilled, so it cannot produce `supported`; but it does prove the project
/// touched the thing again, which is enough to make "nobody ever did this" a
/// false statement. One `COUNT(*)` buys the difference between a ghost and a
/// pending item.
///
/// Returns `(searched, matches)`. `searched = false` means no usable gate
/// token existed — we could not look, which must never be reported as having
/// looked and found nothing.
fn name_match_count(
    ledger: &Ledger,
    project_id: &str,
    c: &StoredClaim,
    terms: &[String],
    session_order: &[String],
) -> Result<(bool, u32, u32)> {
    // §5j defeat is stricter than retrieval. When a proposal names a durable
    // artifact, an unrelated later action sharing an ordinary verb ("implement",
    // "retry", "update") cannot disprove abandonment. Use the artifact and
    // its basename; fall back to natural-language terms only for proposals
    // whose durable target is itself a natural name (gallery, archive, Fix 2).
    let durable = defeat_terms(&c.text, terms);
    let toks = gate_tokens(&durable);
    if toks.is_empty() {
        return Ok((false, 0, 0));
    }
    let likes: Vec<String> = toks.iter().map(|t| format!("%{t}%")).collect();
    // Squashed variants (A59): the same tokens with separators removed, bound
    // after the raw set so each clause can test both spellings.
    let squashed: Vec<String> = toks
        .iter()
        .map(|t| {
            format!(
                "%{}%",
                t.chars()
                    .filter(|c| !matches!(c, '_' | '-' | '.' | ' '))
                    .collect::<String>()
            )
        })
        .collect();
    let n = likes.len();
    // §5j: action-side and output-side counted SEPARATELY. Only the first may
    // defeat `dropped`; the second annotates the receipt.
    let count_with = |clause: &dyn Fn(usize, usize) -> String| -> Result<u32> {
        let ors = (0..n)
            .map(|i| clause(i + 4, i + 4 + n))
            .collect::<Vec<_>>()
            .join(" OR ");
        let later: Vec<&String> = session_order.iter().skip(c.session_rank + 1).collect();
        let mut total: u32 = 0;
        {
            let sql = format!(
                "SELECT COUNT(*) FROM events e
                 WHERE e.project_id = ?1 AND e.session_id = ?2
                   AND {EVIDENCE_EVENT_PREDICATE}
                   AND e.seq > ?3 AND ({ors})"
            );
            let mut stmt = ledger.conn().prepare(&sql)?;
            let mut binds: Vec<&dyn rusqlite::ToSql> = vec![&project_id, &c.session_id, &c.seq];
            for l in &likes {
                binds.push(l);
            }
            for l in &squashed {
                binds.push(l);
            }
            total += stmt.query_row(binds.as_slice(), |r| r.get::<_, i64>(0))? as u32;
        }
        for s in later {
            let sql = format!(
                "SELECT COUNT(*) FROM events e
                 WHERE e.project_id = ?1 AND e.session_id = ?2
                   AND {EVIDENCE_EVENT_PREDICATE}
                   AND ({ors})"
            );
            let mut stmt = ledger.conn().prepare(&sql)?;
            let zero: i64 = 0;
            let mut binds: Vec<&dyn rusqlite::ToSql> = vec![&project_id, s, &zero];
            for l in &likes {
                binds.push(l);
            }
            for l in &squashed {
                binds.push(l);
            }
            total += stmt.query_row(binds.as_slice(), |r| r.get::<_, i64>(0))? as u32;
        }
        Ok(total)
    };
    let action = count_with(&action_side_clause)?;
    let output = count_with(&output_side_clause)?;
    Ok((true, action, output))
}

/// Cap on gate terms per claim. More than this and the `OR` chain stops being
/// a filter — every action mentions *some* common token.
const MAX_GATE_TOKENS: usize = 6;

/// Run both passes over one project.
///
/// `scorer` is the only thing here that holds significant memory, and nothing
/// else of size is alive while it runs.
pub fn annotate(
    ledger: &Ledger,
    project_id: &str,
    cfg: &Config,
    scorer: &mut dyn EntailmentScorer,
    scorer_tag: &str,
) -> Result<AnnotateReport> {
    let report = extract_heuristic_claims(ledger, project_id)?;
    score_claims(ledger, project_id, cfg, scorer, scorer_tag, report)
}

/// Deterministic Tier-0 extraction, also used when Ollama is absent.
pub fn extract_heuristic_claims(ledger: &Ledger, project_id: &str) -> Result<AnnotateReport> {
    let mut r = AnnotateReport::default();
    let t0 = std::time::Instant::now();
    write_claims(ledger, project_id, &mut r)?;
    r.extract_ms = t0.elapsed().as_millis();
    Ok(r)
}

/// Score the currently active derived claims. The classification model has
/// already been dropped before callers enter this function; no provider type,
/// model output, or language confidence is available to verdict logic.
pub fn score_claims(
    ledger: &Ledger,
    project_id: &str,
    cfg: &Config,
    scorer: &mut dyn EntailmentScorer,
    scorer_tag: &str,
    mut r: AnnotateReport,
) -> Result<AnnotateReport> {
    cfg.validate()?;

    let claims = load_claims(ledger, project_id)?;
    let mut sessions = ledger.conn().prepare(
        "SELECT id FROM sessions WHERE project_id = ?1
         ORDER BY (started_at IS NULL), started_at, id",
    )?;
    let session_order: Vec<String> = sessions
        .query_map(params![project_id], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<_, _>>()?;
    drop(sessions);

    let t1 = std::time::Instant::now();
    let mut cwd_cache = RawScopeCache::default();
    let profile = evidence_profile(ledger, project_id, &mut cwd_cache)?;
    let anchor: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT id FROM events WHERE project_id = ?1 ORDER BY session_id, seq LIMIT 1",
            params![project_id],
            |row| row.get(0),
        )
        .optional()?;
    let extractor = if r.extractor.is_empty() {
        MarkerExtractor.name().to_string()
    } else {
        r.extractor.clone()
    };
    let tx = ledger.conn().unchecked_transaction()?;
    // M4.4: one scorer tag is a complete rebuildable projection. Clear that
    // tag atomically before rewriting it so candidates that fell out of the
    // current search cannot survive with pre-scope metadata. Other scorer
    // tags remain append-only and independently inspectable.
    tx.execute(
        "DELETE FROM evidence_links
         WHERE scorer = ?2 AND claim_id IN (
             SELECT c.id FROM claims c JOIN events e ON e.id = c.event_id
             WHERE e.project_id = ?1)",
        params![project_id, scorer_tag],
    )?;
    tx.execute(
        "DELETE FROM claim_searches
         WHERE scorer = ?2 AND claim_id IN (
             SELECT c.id FROM claims c JOIN events e ON e.id = c.event_id
             WHERE e.project_id = ?1)",
        params![project_id, scorer_tag],
    )?;
    {
        let mut ins = tx.prepare(
            "INSERT OR REPLACE INTO evidence_links(claim_id, evidence_event_id,
                 entail_score, verdict, scorer, fidelity, pass, contradiction, related,
                 ev_family, ev_passed, ev_failed, ev_ok, match_strength, ev_identity,
                 ev_session_id, ev_seq, ev_cwd, ev_git_branch)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,
                    ?16,?17,?18,?19)",
        )?;
        let mut search_ins = tx.prepare(
            "INSERT OR REPLACE INTO claim_searches(claim_id, scorer, pass, searched, name_matches,
                 action_matches, output_matches, coverage_eligible, missing_families)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        )?;

        for c in &claims {
            // §5b: findings and micro-intents can never produce a verdict, so
            // scoring them would buy nothing but wall time. They are already
            // persisted in `claims` — A29's raw material — and stop here.
            if c.kind == ClaimKind::Finding.as_str() || c.kind == ClaimKind::MicroIntent.as_str() {
                continue;
            }
            let is_completed =
                c.kind == ClaimKind::Work.as_str() || c.kind == ClaimKind::Marker.as_str();
            let mut coverage = None;
            let (candidates, pass) = if is_completed {
                coverage = Some(coverage_expectation(
                    ledger,
                    c,
                    cfg.backward.k_events,
                    &profile,
                )?);
                let mut cands = backward_candidates(
                    ledger,
                    c,
                    cfg.backward.k_events,
                    cfg.backward.max_candidates,
                )?;
                // A recap names its work but sits far from it. Widen by name,
                // not by distance.
                let arts = crate::extract::artifacts(&c.text);
                let terms = retrieval_tokens(&c.text, &arts);
                for id in backward_lexical_candidates(
                    ledger,
                    project_id,
                    c,
                    &terms,
                    &session_order,
                    cfg.backward.max_candidates,
                )? {
                    if !cands.contains(&id) {
                        cands.push(id);
                    }
                }
                (cands, "backward")
            } else {
                // Proposals are only forward-scored once the project has moved
                // past them; otherwise they are simply still pending.
                if !project_moved_on(
                    ledger,
                    project_id,
                    c,
                    session_order.len(),
                    cfg.forward.min_subsequent_user_turns,
                )? {
                    r.proposals_still_recent += 1;
                    continue;
                }
                let arts = crate::extract::artifacts(&c.text);
                let terms = retrieval_tokens(&c.text, &arts);
                // A34: record the search itself, not just its results. Without
                // this row, derivation cannot tell "we looked and the work was
                // never done" from "we never looked" — and the two are
                // indistinguishable at the point where a 👻 gets rendered.
                // Measured cost of getting this wrong, in this very milestone:
                // a locked database made `annotate` a silent no-op and the
                // graph reported 535 dropped threads that did not exist.
                let (searched, action, output) =
                    name_match_count(ledger, project_id, c, &terms, &session_order)?;
                // §5j funnel accounting, reported with every run so vacuity
                // can never again be mistaken for validation (A52).
                if !searched {
                    r.proposals_unsearchable += 1;
                } else if action > 0 {
                    r.proposals_action_defeated += 1;
                } else {
                    r.proposals_eligible += 1;
                    if output > 0 {
                        r.proposals_output_annotated += 1;
                    }
                }
                search_ins.execute(params![
                    c.id,
                    scorer_tag,
                    "forward",
                    i64::from(searched),
                    i64::from(action + output),
                    i64::from(action),
                    i64::from(output),
                    0i64,
                    "",
                ])?;
                (
                    forward_candidates(ledger, project_id, c, &terms, &session_order, cfg)?,
                    "forward",
                )
            };

            let mut parsed_local = BTreeSet::new();
            for ev_id in &candidates {
                let prepare_started = std::time::Instant::now();
                let Some(row) = evidence_row_for(ledger, ev_id)? else {
                    continue;
                };
                let detail = evidence_detail_for(ledger, ev_id)?;
                let rendered = render_tagged_for_claim(&row, &c.text, detail.as_deref());
                // Freeze what the run reported, for the typed contract (A33).
                //
                // §5g: through `row_outcome`, which reads only the row's
                // ExecutedOutput span. The old call reached for `result_tail`
                // by hand — correct, but only by the caller's good manners. A
                // future caller could have passed `tool_input` and nothing in
                // the type system would have objected. Now nothing else is
                // reachable from here.
                let typed_outcome = if is_agent_record(&row) {
                    // Agent prose is captured evidence for support, but it is
                    // not a runner's direct output. It never receives typed
                    // arithmetic authority.
                    crate::premise::Outcome::Unknown
                } else {
                    crate::premise::row_outcome(&row)
                };
                let ev_out = crate::contract::EvidenceOutcome::from_outcome(&typed_outcome);
                match rendered.fidelity {
                    Fidelity::Parsed => {
                        r.parsed_premises += 1;
                        parsed_local.insert(ev_id.clone());
                    }
                    Fidelity::Fallback => r.fallback_premises += 1,
                }
                r.pair_prepare_us += prepare_started.elapsed().as_micros();
                let inference_started = std::time::Instant::now();
                let s: Scores = scorer.score(&rendered.text, &c.text)?;
                r.inference_us += inference_started.elapsed().as_micros();
                let verdict = Verdict::band(s.entailment, cfg.thresholds.hi, cfg.thresholds.lo);
                // A32: frozen here, not recomputed at derive time, so the rank
                // an accusation rested on stays reconstructible from the
                // ledger alone.
                let strength = relatedness(&c.text, &rendered.text);
                let raw_scope = row_scope(&row, &mut cwd_cache);
                let identity = if raw_scope.trusted {
                    crate::identity::identity_key(
                        &row.tool_name,
                        row.tool_input.as_deref().unwrap_or(""),
                        raw_scope.cwd.as_deref(),
                    )
                } else {
                    String::new()
                };
                // §5p family tabulation. `git`/`agent` are identity- and
                // tool-derived rather than outcome families.
                {
                    let inv = identity.rsplit(':').next().unwrap_or("");
                    if inv.starts_with("git ") {
                        r.family_git += 1;
                    } else if is_agent_record(&row) {
                        r.family_agent += 1;
                    } else {
                        match ev_out.family {
                            crate::contract::Family::Test => r.family_test += 1,
                            crate::contract::Family::Build => r.family_build += 1,
                            crate::contract::Family::None => r.family_none += 1,
                        }
                    }
                }
                let write_started = std::time::Instant::now();
                ins.execute(params![
                    c.id,
                    ev_id,
                    s.entailment as f64,
                    verdict.as_str(),
                    scorer_tag,
                    rendered.fidelity.as_str(),
                    pass,
                    s.contradiction as f64,
                    i64::from(strength > 0.0),
                    ev_out.family.as_str(),
                    ev_out.passed.map(i64::from),
                    ev_out.failed.map(i64::from),
                    ev_out.ok.map(i64::from),
                    strength as f64,
                    identity,
                    row.session_id,
                    row.seq,
                    raw_scope.cwd,
                    raw_scope.git_branch,
                ])?;
                r.link_write_us += write_started.elapsed().as_micros();
                r.links_written += 1;
                if pass == "backward" {
                    r.backward_pairs += 1;
                } else {
                    r.forward_pairs += 1;
                }
            }
            if candidates.is_empty() {
                r.claims_without_candidates += 1;
            }
            if let Some(expectation) = coverage {
                let (eligible, missing) = expectation.assess(&parsed_local);
                let missing_text = missing
                    .iter()
                    .map(|f| f.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                if eligible {
                    r.unsupported_coverage_eligible += 1;
                } else {
                    for family in &missing {
                        *r.coverage_missing_by_family
                            .entry(family.as_str().to_string())
                            .or_default() += 1;
                    }
                }
                search_ins.execute(params![
                    c.id,
                    scorer_tag,
                    "backward",
                    1i64,
                    0i64,
                    0i64,
                    0i64,
                    i64::from(eligible),
                    missing_text,
                ])?;
            }
        }
    }

    // The audit manifest, active scorer head, links, and search coverage cross
    // one transaction boundary. A timestamp is not an ordering primitive:
    // two complete runs can finish inside the same second, while their stable
    // annotation hashes have no chronological order.
    let summary = format!(
        "{{\"claims\":{},\"links\":{},\"backward\":{},\"forward\":{},\
          \"coverage_eligible\":{},\"hi\":{},\"lo\":{}}}",
        r.claims_written,
        r.links_written,
        r.backward_pairs,
        r.forward_pairs,
        r.unsupported_coverage_eligible,
        cfg.thresholds.hi,
        cfg.thresholds.lo
    );
    if let Some(anchor) = anchor {
        let prompt_hash = sha256_hex(format!("{extractor}|{scorer_tag}").as_bytes());
        let out_hash = sha256_hex(summary.as_bytes());
        let annotation_id = format!(
            "a{}",
            &sha256_hex(format!("{project_id}|{scorer_tag}").as_bytes())[..31]
        );
        tx.execute(
            "INSERT OR REPLACE INTO annotations(id, event_id, layer, model,
                 prompt_sha256, output_json, output_sha256, confidence, created_at)
             VALUES(?1,?2,'entailment',?3,?4,?5,?6,NULL,?7)",
            params![
                annotation_id,
                anchor,
                scorer_tag,
                prompt_hash,
                summary,
                out_hash,
                crate::ingest::iso8601_utc_now(),
            ],
        )?;
        tx.execute(
            "INSERT INTO entailment_projection_heads(project_id, scorer)
             VALUES(?1, ?2)
             ON CONFLICT(project_id) DO UPDATE SET scorer = excluded.scorer",
            params![project_id, scorer_tag],
        )?;
    }
    tx.commit()?;
    r.score_ms = t1.elapsed().as_millis();

    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_sentence_candidate() -> Vec<SentenceCandidate> {
        sentence_candidates("I completed `src/lib.rs`.")
    }

    #[test]
    fn frozen_language_event_rejects_unknown_envelope_fields() {
        let candidates = one_sentence_candidate();
        assert_eq!(candidates.len(), 1);
        let raw = serde_json::json!({
            "type": "event_classification",
            "results": [{
                "span_start": candidates[0].span_start,
                "span_end": candidates[0].span_end,
                "category": "completed_claim",
                "confidence": 0.99,
                "speech_act": "none",
                "label": "Source update",
                "attempts": 1
            }],
            "truth_bearing": true
        })
        .to_string();

        let error = parse_stored_event(&raw, &candidates).expect_err("unknown envelope key");
        assert!(error.to_string().contains("unknown field"), "{error}");
    }

    #[test]
    fn frozen_language_candidate_rejects_unknown_result_fields() {
        let candidates = one_sentence_candidate();
        assert_eq!(candidates.len(), 1);
        let raw = serde_json::json!({
            "type": "event_classification",
            "results": [{
                "span_start": candidates[0].span_start,
                "span_end": candidates[0].span_end,
                "category": "completed_claim",
                "confidence": 0.99,
                "speech_act": "none",
                "label": "Source update",
                "attempts": 1,
                "verdict": "supported"
            }]
        })
        .to_string();

        let error = parse_stored_event(&raw, &candidates).expect_err("unknown result key");
        assert!(error.to_string().contains("unknown field"), "{error}");
    }

    #[test]
    fn frozen_language_event_accepts_the_exact_closed_shape() {
        let candidates = one_sentence_candidate();
        assert_eq!(candidates.len(), 1);
        let raw = serde_json::json!({
            "type": "event_classification",
            "results": [{
                "span_start": candidates[0].span_start,
                "span_end": candidates[0].span_end,
                "category": "completed_claim",
                "confidence": 0.99,
                "speech_act": "none",
                "label": "Source update",
                "attempts": 1
            }]
        })
        .to_string();

        let parsed = parse_stored_event(&raw, &candidates).expect("closed event shape");
        assert_eq!(parsed.results.len(), 1);
        assert_eq!(parsed.results[0].category, LanguageCategory::CompletedClaim);
        assert_eq!(parsed.results[0].speech_act, SpeechAct::None);
        assert_eq!(
            parsed.results[0].label.as_ref().map(ShadowLabel::as_str),
            Some("Source update")
        );
    }

    #[test]
    fn refusal_and_deferral_only_demote_heuristic_proposals() {
        for speech_act in [SpeechAct::Refusal, SpeechAct::Deferral] {
            assert!(!speech_act_allows_claim(ClaimKind::Proposal, speech_act));
            assert!(speech_act_allows_claim(ClaimKind::Work, speech_act));
            assert!(speech_act_allows_claim(ClaimKind::Marker, speech_act));
            assert!(speech_act_allows_claim(ClaimKind::Finding, speech_act));
            assert!(speech_act_allows_claim(ClaimKind::MicroIntent, speech_act));
        }
        assert!(speech_act_allows_claim(
            ClaimKind::Proposal,
            SpeechAct::Decision
        ));
        assert!(speech_act_allows_claim(
            ClaimKind::Proposal,
            SpeechAct::Proposal
        ));
    }

    #[test]
    fn production_language_memory_wiring_is_system_probe_with_fixed_poll_interval() {
        let (probe, poll) = production_language_memory();
        let _: &dyn MemoryPressureProbe = &probe;
        let _: &dyn MemoryPressurePoll = &poll;
        assert_eq!(
            LANGUAGE_MEMORY_POLL_INTERVAL,
            std::time::Duration::from_millis(250)
        );
    }

    #[test]
    fn verdict_bands_are_exactly_the_configured_thresholds() {
        assert_eq!(Verdict::band(0.60, 0.60, 0.10), Verdict::Entailed);
        assert_eq!(Verdict::band(0.59, 0.60, 0.10), Verdict::Uncertain);
        assert_eq!(Verdict::band(0.10, 0.60, 0.10), Verdict::NotEntailed);
        assert_eq!(Verdict::band(0.11, 0.60, 0.10), Verdict::Uncertain);
    }

    #[test]
    fn claim_ids_are_deterministic_and_span_sensitive() {
        use crate::extract::{ClaimKind, Marker};
        let mk = |span: usize| Claim {
            text: "All 3 tests pass.".to_string(),
            span_start: span,
            span_end: span + 17,
            kind: ClaimKind::Marker,
            marker: Marker::Ratio,
            artifacts: vec![],
        };
        let a = claim_id("e1", "marker-v2", &mk(0));
        assert_eq!(a, claim_id("e1", "marker-v2", &mk(0)), "not deterministic");
        assert_ne!(a, claim_id("e1", "marker-v2", &mk(5)), "span ignored");
        assert_ne!(a, claim_id("e2", "marker-v2", &mk(0)), "event ignored");
        assert!(a.starts_with('c') && a.len() == 32);
    }

    #[test]
    fn retrieval_terms_cover_the_round_three_missed_names() {
        let gallery = retrieval_tokens(
            "Now update the gallery composition.",
            &crate::extract::artifacts("Now update the gallery composition."),
        );
        assert!(gallery.iter().any(|t| t == "gallery"));
        assert!(gallery.iter().any(|t| t == "composition"));

        let archive = retrieval_tokens(
            "Now attempt the release archive.",
            &crate::extract::artifacts("Now attempt the release archive."),
        );
        assert!(archive.iter().any(|t| t == "archive"));

        let installer = retrieval_tokens(
            "Toolchain installed with --no-profile-change.",
            &crate::extract::artifacts("Toolchain installed with --no-profile-change."),
        );
        assert!(installer.iter().any(|t| t == "--no-profile-change"));
    }

    #[test]
    fn a59_short_labeled_identifiers_become_bigram_tokens() {
        // The Fix-2 ghost: "Fix", "Bug", "Phase" + number are below the 4-char
        // token floor individually, and the bigram is often the only durable
        // name a proposal carries. The ~12 follow-through edits all said
        // literally "Fix 2"; no token matched; a false ghost shipped.
        let t = retrieval_tokens(
            "Now mirroring Fix 2 on iOS.",
            &crate::extract::artifacts("Now mirroring Fix 2 on iOS."),
        );
        assert!(
            t.iter().any(|x| x == "fix 2"),
            "bigram token missing from {t:?}"
        );
        let t = retrieval_tokens(
            "Close out Bug 3 in the tally path.",
            &crate::extract::artifacts("Close out Bug 3 in the tally path."),
        );
        assert!(
            t.iter().any(|x| x == "bug 3"),
            "bigram token missing from {t:?}"
        );
        // Lowercase prose numbers must NOT become bigrams — "chapter 2 of the
        // plan" is not an identifier.
        let t = retrieval_tokens(
            "as noted in part 2 of the plan",
            &crate::extract::artifacts("as noted in part 2 of the plan"),
        );
        assert!(
            !t.iter().any(|x| x == "part 2"),
            "lowercase bigram wrongly extracted: {t:?}"
        );
    }

    #[test]
    fn an_unrelated_shared_verb_cannot_defeat_a_named_proposal() {
        let claim = "I'll implement RetryBudget in `src/net/retry_budget.rs`.";
        let retrieval = retrieval_tokens(claim, &crate::extract::artifacts(claim));
        assert!(
            retrieval.iter().any(|t| t == "implement"),
            "the generous retrieval vocabulary changed: {retrieval:?}"
        );
        let defeat = defeat_terms(claim, &retrieval);
        assert!(
            !defeat.iter().any(|t| t == "implement"),
            "an ordinary verb gained ghost-defeater authority: {defeat:?}"
        );
        assert!(defeat.iter().any(|t| t == "retry_budget.rs"));
    }

    #[test]
    fn a_relative_path_still_defeats_the_full_named_path() {
        let claim = "I'll harden `src/net/backoff.rs` retry jitter.";
        let defeat = defeat_terms(claim, &retrieval_tokens(claim, &[]));
        assert!(
            defeat.iter().any(|t| t == "backoff.rs"),
            "artifact basename missing from ghost-defeater vocabulary: {defeat:?}"
        );
    }

    #[test]
    fn all_fifteen_round_three_missed_proof_patterns_have_a_retrieval_key() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/evidence-authority-dev.json"
        ))
        .expect("dev diagnostic JSON");
        let mut closed = Vec::new();
        for case in fixture["cases"].as_array().expect("cases") {
            let class = case["class"].as_str().expect("class");
            if class != "dropped" && class != "unsupported" {
                continue;
            }
            let claim = case["claim"].as_str().expect("claim");
            let evidence = case["evidence"]
                .as_array()
                .expect("evidence")
                .iter()
                .map(|v| v.as_str().expect("evidence text"))
                .collect::<Vec<_>>()
                .join("\n")
                .to_ascii_lowercase();
            let terms = retrieval_tokens(claim, &crate::extract::artifacts(claim));
            assert!(
                terms.iter().any(|term| evidence.contains(term)),
                "{} still has no retrieval key: {terms:?}",
                case["id"].as_str().expect("id")
            );
            closed.push(case["id"].as_str().expect("id"));
        }
        assert_eq!(
            closed.len(),
            15,
            "the Round-3 recall set must stay complete"
        );
    }
}
