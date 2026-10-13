//! Read-only standing-check context. Only an exact verified bridge receipt
//! selects a check; proposal text is never promoted into an instruction.

use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::time::{Duration, Instant};

use rusqlite::{types::ValueRef, OptionalExtension};
use serde::Serialize;
use serde_json::Value;

use crate::chain::sha256_hex;
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::redact::Masked;

use super::authority::{bridge, PersonActKind};
use super::params::parse_params;
use super::storage;

const MAX_DECISIONS: usize = 1024;
const MAX_ROWS: usize = 16384;
const MAX_PROOF_LINE: usize = 1024 * 1024;
const READ_BUDGET: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Serialize)]
pub struct ContextDecision {
    pub project_id: String,
    pub decision_id: String,
    pub revision: u32,
    pub kind: String,
    /// Masked string leaves for serving; the hash covers the original bytes.
    pub params: Value,
    pub params_sha256: String,
    pub severity: String,
    pub applies_from_version: String,
    pub acceptance_id: String,
    pub receipt_id: String,
    pub event_id: String,
    pub tuple_sha256: String,
    pub raw_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextNote {
    pub project_id: String,
    pub decision_id: String,
    pub state: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DecisionContext {
    pub decisions: Vec<ContextDecision>,
    pub notes: Vec<ContextNote>,
    /// None preserves the historical packet binding when there are no rows.
    pub fingerprint: Option<String>,
}

/// Read across project sessions. No schema, authority, verdict or file writes.
pub fn read(ledger: &Ledger, project_ids: &[String]) -> Result<DecisionContext> {
    let deadline = Instant::now() + READ_BUDGET;
    let fingerprint = fingerprint_until(ledger, project_ids, deadline)?;
    let mut result = DecisionContext {
        decisions: Vec::new(),
        notes: Vec::new(),
        fingerprint,
    };
    for project in sorted_projects(project_ids) {
        for decision in decision_ids(ledger, &project)? {
            ensure_time(deadline)?;
            if verified_retirement(ledger, &decision, deadline)? {
                continue;
            }
            let mut query = ledger.conn().prepare(
                "SELECT a.id,r.rev,r.kind,r.params_schema_version,r.params_json,
                        a.severity,a.applies_from_version
                 FROM decision_acceptances a JOIN decision_revisions r
                   ON r.decision_id=a.decision_id AND r.rev=a.rev
                 WHERE a.decision_id=?1 ORDER BY a.rowid DESC LIMIT 1025",
            )?;
            let candidates = query
                .query_map([&decision], |row| {
                    Ok(Candidate {
                        act: row.get(0)?,
                        revision: row.get(1)?,
                        kind: row.get(2)?,
                        schema: row.get(3)?,
                        params_json: row.get(4)?,
                        severity: row.get(5)?,
                        from: row.get(6)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            if candidates.len() > MAX_DECISIONS {
                return Err(CoreError::Config(
                    "Decision context exceeds its acceptance bound".into(),
                ));
            }
            let has_pending = !candidates.is_empty();
            let mut selected = false;
            for candidate in candidates {
                ensure_time(deadline)?;
                if !bridge::verified_read_only(
                    ledger.conn(),
                    PersonActKind::Accept,
                    &candidate.act,
                    deadline.saturating_duration_since(Instant::now()),
                )? {
                    continue;
                }
                ensure_time(deadline)?;
                // Serve only the typed bytes in the confirmed tuple, never
                // subsequently edited proposal words or source descriptions.
                let check =
                    parse_params(&candidate.kind, candidate.schema, &candidate.params_json)?;
                let proof = proof(ledger, &candidate.act)?.ok_or_else(|| {
                    CoreError::Integrity("verified Decision receipt is absent".into())
                })?;
                result.decisions.push(ContextDecision {
                    project_id: project.clone(),
                    decision_id: decision.clone(),
                    revision: candidate.revision,
                    kind: candidate.kind,
                    params: mask_json(&serde_json::from_str(&check.json()?)?),
                    params_sha256: sha256_hex(candidate.params_json.as_bytes()),
                    severity: candidate.severity,
                    applies_from_version: candidate.from,
                    acceptance_id: candidate.act,
                    receipt_id: proof.0,
                    event_id: proof.1,
                    tuple_sha256: proof.2,
                    raw_sha256: proof.3,
                });
                selected = true;
                break;
            }
            if has_pending && !selected {
                result.notes.push(ContextNote {
                    project_id: project.clone(),
                    decision_id: decision,
                    state: "unverifiable".into(),
                    reason: "no exact bridge receipt verifies an acceptance".into(),
                });
            }
        }
    }
    if result.fingerprint != fingerprint_until(ledger, project_ids, deadline)? {
        return Err(CoreError::Integrity(
            "Decisions changed while context was being read; request it again".into(),
        ));
    }
    Ok(result)
}

/// Bind packet identity to project-scoped derived rows and exact receipt
/// lines and their current bridge qualification, including changes that do not
/// move the event-chain head.
pub fn fingerprint(ledger: &Ledger, project_ids: &[String]) -> Result<Option<String>> {
    fingerprint_until(ledger, project_ids, Instant::now() + READ_BUDGET)
}

fn fingerprint_until(
    ledger: &Ledger,
    project_ids: &[String],
    deadline: Instant,
) -> Result<Option<String>> {
    let mut bytes = b"thruline.decisions.context.v1\n".to_vec();
    let mut found = false;
    let mut rows = 0;
    for project in sorted_projects(project_ids) {
        if decision_ids(ledger, &project)?.is_empty()
            && super::rule_files::notes_for_project(ledger, &project)?
                .notes
                .is_empty()
        {
            continue;
        }
        found = true;
        append(&mut bytes, project.as_bytes());
        for (table, sql) in [
            ("decisions", "SELECT t.* FROM decisions t WHERE t.project_id=?1 ORDER BY t.id"),
            ("decision_revisions", "SELECT t.* FROM decision_revisions t JOIN decisions d ON d.id=t.decision_id WHERE d.project_id=?1 ORDER BY t.decision_id,t.rev"),
            ("decision_acceptances", "SELECT t.* FROM decision_acceptances t JOIN decisions d ON d.id=t.decision_id WHERE d.project_id=?1 ORDER BY t.decision_id,t.rowid"),
            ("decision_retirements", "SELECT t.* FROM decision_retirements t JOIN decisions d ON d.id=t.decision_id WHERE d.project_id=?1 ORDER BY t.decision_id,t.rowid"),
            ("decision_waivers", "SELECT t.* FROM decision_waivers t JOIN decisions d ON d.id=t.decision_id WHERE d.project_id=?1 ORDER BY t.decision_id,t.rowid"),
            ("decision_person_acts", "SELECT t.* FROM decision_person_acts t JOIN decisions d ON d.id=t.decision_id WHERE d.project_id=?1 ORDER BY t.decision_id,t.rowid"),
            ("decision_bridge_proofs", "SELECT t.* FROM decision_bridge_proofs t JOIN decision_person_acts p ON p.id=t.receipt_id JOIN decisions d ON d.id=p.decision_id WHERE d.project_id=?1 ORDER BY d.id,t.rowid"),
            ("decision_rule_file_notes", "SELECT t.* FROM decision_rule_file_notes t WHERE t.project_id=?1 ORDER BY t.id"),
        ] {
            ensure_time(deadline)?;
            append(&mut bytes, table.as_bytes());
            if !table_exists(ledger, table)? { append(&mut bytes, b"absent"); continue; }
            let mut query = ledger.conn().prepare(sql)?;
            let columns = query.column_count();
            for name in query.column_names() { append(&mut bytes, name.as_bytes()); }
            let mut records = query.query([&project])?;
            while let Some(row) = records.next()? {
                rows += 1;
                if rows > MAX_ROWS { return Err(CoreError::Config("Decision context fingerprint exceeds its row bound".into())); }
                ensure_time(deadline)?;
                append(&mut bytes, b"row");
                for column in 0..columns {
                    match row.get_ref(column)? {
                        ValueRef::Null => append(&mut bytes, b"null"),
                        ValueRef::Integer(value) => { append(&mut bytes, b"integer"); append(&mut bytes, &value.to_le_bytes()); }
                        ValueRef::Real(value) => { append(&mut bytes, b"real"); append(&mut bytes, &value.to_bits().to_le_bytes()); }
                        ValueRef::Text(value) => { ensure_field_bound(value)?; append(&mut bytes, b"text"); append(&mut bytes, value); }
                        ValueRef::Blob(value) => { ensure_field_bound(value)?; append(&mut bytes, b"blob"); append(&mut bytes, value); }
                    }
                    if bytes.len() > 16 * 1024 * 1024 {
                        return Err(CoreError::Config("Decision context fingerprint exceeds its sixteen MiB bound".into()));
                    }
                }
            }
        }
        if table_exists(ledger, "decision_person_acts")? {
            let mut query = ledger.conn().prepare("SELECT p.raw_file,p.raw_line,p.action,p.act_row_id FROM decision_person_acts p JOIN decisions d ON d.id=p.decision_id WHERE d.project_id=?1 ORDER BY d.id,p.rowid")?;
            let mut records = query.query([&project])?;
            while let Some(row) = records.next()? {
                ensure_time(deadline)?;
                let path: String = row.get(0)?;
                let line: i64 = row.get(1)?;
                let line = u64::try_from(line)
                    .map_err(|_| CoreError::Integrity("Decision receipt line is invalid".into()))?;
                append(&mut bytes, b"raw-confirmation-line");
                append(
                    &mut bytes,
                    receipt_line(Path::new(&path), line, deadline)?.as_bytes(),
                );
                // A receipt's validity also depends on normalized source,
                // the complete file, UID, private mode and canonical location.
                // Use the authority verifier instead of duplicating its policy.
                let action: String = row.get(2)?;
                let act: String = row.get(3)?;
                let kind = match action.as_str() {
                    "accept" => Some(PersonActKind::Accept),
                    "retire" => Some(PersonActKind::Retire),
                    "waive" => Some(PersonActKind::Waive),
                    _ => None,
                };
                ensure_time(deadline)?;
                let verified = match kind {
                    Some(kind) => bridge::verified_read_only(
                        ledger.conn(),
                        kind,
                        &act,
                        deadline.saturating_duration_since(Instant::now()),
                    )?,
                    None => false,
                };
                ensure_time(deadline)?;
                append(&mut bytes, b"bridge-qualification");
                append(
                    &mut bytes,
                    if verified {
                        b"verified"
                    } else {
                        b"unverifiable"
                    },
                );
                ensure_field_bound(&bytes)?;
            }
        }
    }
    Ok(found.then(|| sha256_hex(&bytes)))
}

fn receipt_line(path: &Path, line: u64, deadline: Instant) -> Result<String> {
    if line == 0 {
        return Ok("missing-line".into());
    }
    // NONBLOCK prevents a replaced FIFO/device from suspending the caller
    // before the cooperative deadline. NOFOLLOW pins a leaf descriptor rather
    // than following a changed receipt symlink.
    #[cfg(unix)]
    let file: File = {
        let descriptor = match rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        ) {
            Ok(descriptor) => descriptor,
            Err(_) => return Ok("missing-file".into()),
        };
        let Ok(stat) = rustix::fs::fstat(&descriptor) else {
            return Ok("missing-file".into());
        };
        if !rustix::fs::FileType::from_raw_mode(stat.st_mode).is_file() {
            return Ok("nonregular-file".into());
        }
        descriptor.into()
    };
    #[cfg(not(unix))]
    let file = {
        let Ok(named) = std::fs::symlink_metadata(path) else {
            return Ok("missing-file".into());
        };
        if named.file_type().is_symlink() || !named.is_file() {
            return Ok("nonregular-file".into());
        }
        match File::open(path) {
            Ok(file) => file,
            Err(_) => return Ok("missing-file".into()),
        }
    };
    let Ok(metadata) = file.metadata() else {
        return Ok("missing-file".into());
    };
    if !metadata.is_file() {
        return Ok("nonregular-file".into());
    }
    // A legacy transcript cannot expand one bridge-context read into an
    // unbounded prefix scan. Actual bridge receipts fit the smaller authority
    // verifier's complete-file bound.
    if metadata.len() > MAX_PROOF_LINE as u64 {
        return Ok("oversized-file".into());
    }
    let mut remaining = metadata.len();
    let mut reader = BufReader::new(file);
    let mut raw = Vec::new();
    for current in 1..=line {
        ensure_time(deadline)?;
        raw.clear();
        if remaining == 0 {
            return Ok("missing-line".into());
        }
        let count = (&mut reader)
            .take(remaining.min((MAX_PROOF_LINE + 1) as u64))
            .read_until(b'\n', &mut raw)
            .map_err(|error| CoreError::io(path, error))?;
        if count == 0 {
            return Ok("missing-line".into());
        }
        if count > MAX_PROOF_LINE {
            return Ok("oversized-line".into());
        }
        remaining = remaining.saturating_sub(count as u64);
        if current == line {
            let Ok(after) = reader.get_ref().metadata() else {
                return Ok("missing-file".into());
            };
            if after.len() != metadata.len() || after.modified().ok() != metadata.modified().ok() {
                return Ok("changed-file".into());
            }
            ensure_time(deadline)?;
            return Ok(sha256_hex(&raw));
        }
    }
    Ok("missing-line".into())
}

fn decision_ids(ledger: &Ledger, project: &str) -> Result<Vec<String>> {
    if !table_exists(ledger, "decisions")? {
        return Ok(Vec::new());
    }
    let mut query = ledger
        .conn()
        .prepare("SELECT id FROM decisions WHERE project_id=?1 ORDER BY id LIMIT 1025")?;
    let ids = query
        .query_map([project], |row| row.get(0))?
        .collect::<std::result::Result<Vec<String>, _>>()?;
    if ids.len() > MAX_DECISIONS {
        return Err(CoreError::Config(
            "Decision context exceeds its 1024 decision bound".into(),
        ));
    }
    Ok(ids)
}

fn verified_retirement(ledger: &Ledger, decision: &str, deadline: Instant) -> Result<bool> {
    ensure_time(deadline)?;
    let retired = storage::verified_retirement_for_decision_read_only(
        ledger,
        decision,
        deadline.saturating_duration_since(Instant::now()),
    )?
    .is_some();
    ensure_time(deadline)?;
    Ok(retired)
}

type Proof = (String, String, String, String);
fn proof(ledger: &Ledger, acceptance: &str) -> Result<Option<Proof>> {
    if !table_exists(ledger, "decision_bridge_proofs")? {
        return Ok(None);
    }
    Ok(ledger.conn().query_row("SELECT b.receipt_id,b.event_id,b.tuple_sha256,b.raw_sha256 FROM decision_bridge_proofs b JOIN decision_person_acts p ON p.id=b.receipt_id AND p.event_id=b.event_id WHERE p.action='accept' AND p.act_row_id=?1", [acceptance], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?)
}

struct Candidate {
    act: String,
    revision: u32,
    kind: String,
    schema: u32,
    params_json: String,
    severity: String,
    from: String,
}
fn sorted_projects(projects: &[String]) -> Vec<String> {
    let mut sorted = projects.to_vec();
    sorted.sort();
    sorted.dedup();
    sorted
}
fn table_exists(ledger: &Ledger, name: &str) -> Result<bool> {
    Ok(ledger.conn().query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [name],
        |row| row.get(0),
    )?)
}
fn ensure_time(deadline: Instant) -> Result<()> {
    if Instant::now() >= deadline {
        Err(CoreError::Config(
            "Decision context exceeded its read budget".into(),
        ))
    } else {
        Ok(())
    }
}
fn append(bytes: &mut Vec<u8>, part: &[u8]) {
    bytes.extend_from_slice(&(part.len() as u64).to_le_bytes());
    bytes.extend_from_slice(part);
}
fn ensure_field_bound(bytes: &[u8]) -> Result<()> {
    if bytes.len() > 16 * 1024 * 1024 {
        Err(CoreError::Config(
            "Decision context field exceeds its sixteen MiB bound".into(),
        ))
    } else {
        Ok(())
    }
}
fn mask_json(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(Masked::mask(text).into_text()),
        Value::Array(items) => Value::Array(items.iter().map(mask_json).collect()),
        Value::Object(items) => Value::Object(
            items
                .iter()
                .map(|(key, value)| (key.clone(), mask_json(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}
