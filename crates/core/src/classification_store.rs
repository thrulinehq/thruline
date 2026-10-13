//! Append-only storage for §5t language-tier classifications.
//!
//! This module deliberately knows nothing about an Ollama provider or claim
//! extraction. Its one job is to make annotation identity mechanical:
//! `(event, model, prompt hash)`. The layer is fixed to `classification`, the
//! hashes are computed here, and an existing key is returned unchanged.

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::Serialize;

use crate::chain::sha256_hex;
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;

pub const CLASSIFICATION_LAYER: &str = "classification";

#[derive(Debug, Clone, PartialEq)]
pub struct ClassificationAnnotation {
    pub id: String,
    pub event_id: String,
    pub model: String,
    pub prompt_sha256: String,
    pub output_json: String,
    pub output_sha256: String,
    pub confidence: f64,
    pub created_at: String,
}

/// Canonical material bound into a completed classification-run manifest.
/// The digest covers the entire active projection, not just claim ids, so a
/// changed kind or marker cannot silently inherit a prior run's authority.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(crate) struct ClaimProjectionRecord {
    pub id: String,
    pub event_id: String,
    pub text: String,
    pub span_start: Option<i64>,
    pub span_end: Option<i64>,
    pub extractor: String,
    pub kind: String,
    pub marker: Option<String>,
}

pub(crate) fn claim_projection_sha256(mut records: Vec<ClaimProjectionRecord>) -> Result<String> {
    records.sort();
    Ok(sha256_hex(&serde_json::to_vec(&records)?))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreDisposition {
    Inserted,
    Existing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoreResult {
    pub disposition: StoreDisposition,
    pub annotation: ClassificationAnnotation,
}

#[derive(Debug, Clone, Copy)]
pub struct NewClassification<'a> {
    pub event_id: &'a str,
    pub model: &'a str,
    /// Exact prompt sent to the classifier. Only its SHA-256 is persisted.
    pub prompt: &'a str,
    /// Strict JSON returned by the classifier, stored and hashed byte-for-byte.
    pub output_json: &'a str,
    pub confidence: f64,
    pub created_at: &'a str,
}

/// Look up a frozen classification before making a model call.
///
/// Callers pass the exact prompt rather than a caller-computed digest so an
/// incorrect hash can never fork the idempotency key.
pub fn find(
    ledger: &Ledger,
    event_id: &str,
    model: &str,
    prompt: &str,
) -> Result<Option<ClassificationAnnotation>> {
    let prompt_sha256 = sha256_hex(prompt.as_bytes());
    find_by_hash(ledger, event_id, model, &prompt_sha256)
}

/// Read one append-only classification by id and verify its frozen output at
/// the consumption boundary. Same-shaped rows from other annotation layers
/// are deliberately invisible here.
pub(crate) fn find_by_id(
    ledger: &Ledger,
    annotation_id: &str,
) -> Result<Option<ClassificationAnnotation>> {
    let row = ledger
        .conn()
        .query_row(
            "SELECT id, event_id, model, prompt_sha256, output_json, output_sha256,
                    confidence, created_at
             FROM annotations WHERE id = ?1 AND layer = 'classification'",
            params![annotation_id],
            annotation_from_row,
        )
        .optional()?;
    verify_output_hash(row)
}

/// Store the first output for this `(event, model, prompt)` input.
///
/// Replays return the original row even if a caller supplies a different
/// output, confidence, or timestamp. That is the idempotency boundary: a
/// model swap or prompt change gets a new row; the same frozen input never
/// rewrites history.
pub fn store(ledger: &Ledger, new: NewClassification<'_>) -> Result<StoreResult> {
    store_on(ledger.conn(), new)
}

/// Store a classification inside the caller's transaction. This is used for
/// the final run manifest so making a language projection authoritative and
/// replacing its rebuildable claims are one atomic commit.
pub(crate) fn store_in_transaction(
    tx: &Transaction<'_>,
    new: NewClassification<'_>,
) -> Result<StoreResult> {
    store_on(tx, new)
}

fn store_on(conn: &Connection, new: NewClassification<'_>) -> Result<StoreResult> {
    validate(&new)?;
    let prompt_sha256 = sha256_hex(new.prompt.as_bytes());
    if let Some(annotation) = find_by_hash_on(conn, new.event_id, new.model, &prompt_sha256)? {
        return Ok(StoreResult {
            disposition: StoreDisposition::Existing,
            annotation,
        });
    }

    let output_sha256 = sha256_hex(new.output_json.as_bytes());
    let id = annotation_id(new.event_id, new.model, &prompt_sha256);
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO annotations(id, event_id, layer, model, prompt_sha256,
             output_json, output_sha256, confidence, created_at)
         VALUES(?1,?2,'classification',?3,?4,?5,?6,?7,?8)",
        params![
            id,
            new.event_id,
            new.model,
            prompt_sha256,
            new.output_json,
            output_sha256,
            new.confidence,
            new.created_at,
        ],
    )?;

    let annotation =
        find_by_hash_on(conn, new.event_id, new.model, &prompt_sha256)?.ok_or_else(|| {
            CoreError::Integrity(format!(
                "classification annotation insert for event {:?}, model {:?} was ignored but no \
                 row exists at its input key",
                new.event_id, new.model
            ))
        })?;
    Ok(StoreResult {
        disposition: if inserted == 1 {
            StoreDisposition::Inserted
        } else {
            StoreDisposition::Existing
        },
        annotation,
    })
}

fn find_by_hash(
    ledger: &Ledger,
    event_id: &str,
    model: &str,
    prompt_sha256: &str,
) -> Result<Option<ClassificationAnnotation>> {
    find_by_hash_on(ledger.conn(), event_id, model, prompt_sha256)
}

fn find_by_hash_on(
    conn: &Connection,
    event_id: &str,
    model: &str,
    prompt_sha256: &str,
) -> Result<Option<ClassificationAnnotation>> {
    let row = conn
        .query_row(
            "SELECT id, event_id, model, prompt_sha256, output_json, output_sha256,
                    confidence, created_at
             FROM annotations
             WHERE layer = 'classification' AND event_id = ?1 AND model = ?2
               AND prompt_sha256 = ?3",
            params![event_id, model, prompt_sha256],
            annotation_from_row,
        )
        .optional()?;

    verify_output_hash(row)
}

fn annotation_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ClassificationAnnotation> {
    Ok(ClassificationAnnotation {
        id: r.get(0)?,
        event_id: r.get(1)?,
        model: r.get(2)?,
        prompt_sha256: r.get(3)?,
        output_json: r.get(4)?,
        output_sha256: r.get(5)?,
        confidence: r.get(6)?,
        created_at: r.get(7)?,
    })
}

fn verify_output_hash(
    row: Option<ClassificationAnnotation>,
) -> Result<Option<ClassificationAnnotation>> {
    if let Some(annotation) = row.as_ref() {
        let actual = sha256_hex(annotation.output_json.as_bytes());
        if actual != annotation.output_sha256 {
            return Err(CoreError::Integrity(format!(
                "classification annotation {} output hash mismatch: stored {}, computed {}",
                annotation.id, annotation.output_sha256, actual
            )));
        }
    }
    Ok(row)
}

fn validate(new: &NewClassification<'_>) -> Result<()> {
    if new.event_id.trim().is_empty() {
        return Err(CoreError::Config(
            "classification event_id must not be empty".to_string(),
        ));
    }
    if new.model.trim().is_empty() {
        return Err(CoreError::Config(
            "classification model must not be empty".to_string(),
        ));
    }
    if new.prompt.trim().is_empty() {
        return Err(CoreError::Config(
            "classification prompt must not be empty".to_string(),
        ));
    }
    let json: serde_json::Value = serde_json::from_str(new.output_json)?;
    if !json.is_object() {
        return Err(CoreError::Config(
            "classification output must be a JSON object".to_string(),
        ));
    }
    if !new.confidence.is_finite() || !(0.0..=1.0).contains(&new.confidence) {
        return Err(CoreError::Config(format!(
            "classification confidence {} is outside 0..=1",
            new.confidence
        )));
    }
    if new.created_at.trim().is_empty() {
        return Err(CoreError::Config(
            "classification created_at must not be empty".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn annotation_id(event_id: &str, model: &str, prompt_sha256: &str) -> String {
    // Length-prefix the variable fields so neither punctuation nor a model
    // name containing a delimiter can alias a different tuple.
    let material = format!(
        "{}:{}{}:{}{}:{}",
        event_id.len(),
        event_id,
        model.len(),
        model,
        prompt_sha256.len(),
        prompt_sha256
    );
    format!("a{}", &sha256_hex(material.as_bytes())[..31])
}
