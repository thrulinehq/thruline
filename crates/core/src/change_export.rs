//! T-5-4 (`B10.16`, `Q18`) — can the records a Change's taken-away copy rests
//! on still be read, **now**, and if not, why.
//!
//! The export itself is rendered in the browser from the one derived
//! view-model (`ui/src/workspace/change-model.js`, `hq.packet`), and it derives
//! nothing. This module is the part only the engine can do: the served view is
//! a projection of the ledger, and three kinds of record the packet rests on
//! live *outside* it, where a file can be deleted, locked or rewritten behind
//! the ledger's back:
//!
//! | record | where it lives | what "readable" means |
//! |---|---|---|
//! | a decision's copy | `.thruline/decisions/<version>.json` under the worktree | the file opens, parses, and names the same act, version, actor, time and criteria version as the row |
//! | a piece of evidence a criterion link names | its own table (`evidence_bindings`, `speaker_findings`, `observations`) | the row the link points at exists |
//! | the conversation lines findings rest on | the raw transcript files every event of a confirmed session names | the file opens, and every recorded line is still there with the digest it was recorded with |
//!
//! WHAT IT DOES NOT DO. It writes nothing, anywhere: no ledger row, no file,
//! no cache. It never repairs, never falls back to a sealed mirror copy to
//! call a missing file present (the mirror answers `thruline verify`; a person
//! reading an export needs to know the original is gone), and never judges
//! the work. Every path it serves has been through the masker, and the
//! treatment is served beside it.
//!
//! WHAT IT RESTATES, AND WHY. The line-digest rule is `ingest::hash_lines`,
//! which is private to `ingest.rs` — a file this task does not own. It is
//! restated in [`line_digests`] with the same four rules (split on `\n`, strip
//! one `\r`, skip whitespace-only lines while still counting them, SHA-256 of
//! the rest), and `crates/core/tests/change_export.rs` pins the two together
//! by ingesting a transcript and checking every digest the ingest wrote.

use std::collections::BTreeMap;
use std::io::BufRead;
use std::path::Path;

use rusqlite::params;
use serde::Serialize;

use crate::chain::sha256_hex;
use crate::error::Result;
use crate::ledger::Ledger;
use crate::redact::{mask_recorded, MaskRecord, Treatment};

/// Bump when a served field is added or changed.
pub const CHANGE_EXPORT_SCHEMA_VERSION: u32 = 1;

/// Whether one record could be read now. A closed set; the browser holds one
/// sentence per word and says so when it meets one it does not know.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Standing {
    /// It opened, and it says what the ledger says it says.
    Readable,
    /// Nothing is at the place the record names.
    Missing,
    /// Something is there and this machine would not let it be opened.
    PermissionDenied,
    /// It opened, and it no longer says what was recorded.
    ContentChanged,
    /// The ledger points at a record the ledger does not hold.
    DanglingReference,
    /// The record says no copy was ever written (an approval whose worktree
    /// was not a folder when it was made).
    NotWritten,
    /// It could not be opened, for a reason this set has no finer word for.
    Unreadable,
}

impl Standing {
    pub fn is_readable(self) -> bool {
        self == Self::Readable
    }
}

/// One decision row and the copy of it under the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DecisionCopy {
    pub decision_id: String,
    /// `approved` or `withdrawn`, the row's own word.
    pub act: &'static str,
    /// The full version the act names. The screen's short form is the
    /// screen's job; the browser matches a row by prefix and cuts nothing.
    pub version: String,
    /// Where the copy is, masked. `None` for a withdrawal (none is written by
    /// design) and for an approval whose copy was never written.
    pub path: Option<String>,
    /// `None` for a withdrawal: there is no copy to read, and saying
    /// "readable" about nothing would be a pass for an absence.
    pub standing: Option<Standing>,
}

/// One piece of evidence a criterion link names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EvidenceRecord {
    /// `check_run`, `finding` or `observation`.
    pub kind: String,
    pub evidence_id: String,
    pub session_id: String,
    /// The accepted criteria (by their number) whose standing link names this
    /// evidence, so a gap is shown on the line it belongs to.
    pub criteria: Vec<i64>,
    /// The evidence's own name as its table holds it, masked; `None` when the
    /// row is not there to hold one.
    pub name: Option<String>,
    pub standing: Standing,
    /// For a run only: whether an execution record names it. `None` for
    /// anything that is not a run, or a run that is not there.
    pub execution_recorded: Option<bool>,
}

/// One transcript file one confirmed session's events were read from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TranscriptFile {
    pub session_id: String,
    /// Masked.
    pub file: String,
    /// How many recorded lines of this file the session's events name.
    pub lines: u64,
    /// Of those, how many are no longer in the file.
    pub lines_gone: u64,
    /// Of those, how many are in the file and differ from what was recorded.
    pub lines_changed: u64,
    pub standing: Standing,
}

/// Totals, counted here so no surface counts them itself.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Totals {
    pub records: u64,
    pub readable: u64,
    pub not_readable: u64,
}

/// Everything the export needs to know about records outside the served view.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExportRecords {
    pub schema_version: u32,
    pub change_id: String,
    /// When these were looked at. The answer is about this moment and no
    /// other; a file can vanish a second later.
    pub checked_at: String,
    /// The folder the Change's version was recorded in, masked; `None` when
    /// the record names none. Served so a copy that leaves paths out can find
    /// this one wherever it appears — a run's name carries its working folder.
    pub worktree: Option<String>,
    pub decisions: Vec<DecisionCopy>,
    pub evidence: Vec<EvidenceRecord>,
    pub transcripts: Vec<TranscriptFile>,
    pub totals: Totals,
    pub masking: Treatment,
}

/// Look at every record the Change's export rests on, now. `None` when the
/// Change does not exist.
pub fn records(
    ledger: &Ledger,
    change_id: &str,
    checked_at: &str,
) -> Result<Option<ExportRecords>> {
    let Some(change) = crate::change::read_change(ledger, change_id)? else {
        return Ok(None);
    };
    let mut record = MaskRecord::default();
    let mut mask = |text: &str| {
        let (out, r) = mask_recorded(text);
        record.merge(&r);
        out
    };

    let worktree = crate::change_decision::worktree_root(ledger, change_id)?
        .map(|p| mask(&p.to_string_lossy()));

    /* ── decisions ─────────────────────────────────────────────────────── */
    let mut decisions = Vec::new();
    for row in &change.decisions.history {
        let act = row.act.as_str();
        let (path, standing) = match row.act {
            crate::change_decision::DecisionAct::Withdrawn => (None, None),
            crate::change_decision::DecisionAct::Approved => match row.record_path.as_deref() {
                None => (None, Some(Standing::NotWritten)),
                Some(p) => (
                    Some(mask(p)),
                    Some(decision_copy_standing(Path::new(p), change_id, row)),
                ),
            },
        };
        decisions.push(DecisionCopy {
            decision_id: row.id.clone(),
            act,
            version: row.target_revision.clone(),
            path,
            standing,
        });
    }

    /* ── evidence the criterion links name ─────────────────────────────── */
    let executions = crate::run_execution::by_binding(ledger)?;
    let mut evidence = Vec::new();
    for (kind, evidence_id, session_id, criteria) in linked_evidence(ledger, change_id)? {
        let (name, standing) = match evidence_name(ledger, &kind, &evidence_id)? {
            Some(name) => (Some(mask(&name)), Standing::Readable),
            None => (None, Standing::DanglingReference),
        };
        let execution_recorded = (kind == "check_run" && standing.is_readable())
            .then(|| executions.contains_key(&evidence_id));
        evidence.push(EvidenceRecord {
            kind,
            evidence_id,
            session_id,
            criteria,
            name,
            standing,
            execution_recorded,
        });
    }

    /* ── the conversation lines of the confirmed work ──────────────────── */
    let mut transcripts = Vec::new();
    for session_id in confirmed_sessions(&change) {
        let mut by_file: BTreeMap<String, Vec<(u64, String)>> = BTreeMap::new();
        for link in ledger.session_chain(&session_id)? {
            by_file
                .entry(link.raw_file.clone())
                .or_default()
                .push((link.raw_line, link.raw_sha256.clone()));
        }
        for (file, lines) in by_file {
            let (standing, gone, changed) = match line_digests(Path::new(&file)) {
                Err(standing) => (standing, 0, 0),
                Ok(disk) => {
                    let mut gone = 0u64;
                    let mut changed = 0u64;
                    for (line, digest) in &lines {
                        match disk.get(line) {
                            None => gone += 1,
                            Some(found) if found != digest => changed += 1,
                            Some(_) => {}
                        }
                    }
                    let standing = if gone + changed == 0 {
                        Standing::Readable
                    } else {
                        Standing::ContentChanged
                    };
                    (standing, gone, changed)
                }
            };
            transcripts.push(TranscriptFile {
                session_id: session_id.clone(),
                file: mask(&file),
                lines: lines.len() as u64,
                lines_gone: gone,
                lines_changed: changed,
                standing,
            });
        }
    }

    let mut totals = Totals::default();
    let all = decisions
        .iter()
        .filter_map(|d| d.standing)
        .chain(evidence.iter().map(|e| e.standing))
        .chain(transcripts.iter().map(|t| t.standing));
    for standing in all {
        totals.records += 1;
        if standing.is_readable() {
            totals.readable += 1;
        } else {
            totals.not_readable += 1;
        }
    }

    Ok(Some(ExportRecords {
        schema_version: CHANGE_EXPORT_SCHEMA_VERSION,
        change_id: change.id.clone(),
        checked_at: checked_at.to_string(),
        worktree,
        decisions,
        evidence,
        transcripts,
        totals,
        masking: record.to_treatment(),
    }))
}

/// Read a decision copy and compare it with the row it copies.
fn decision_copy_standing(
    path: &Path,
    change_id: &str,
    row: &crate::change_decision::DecisionRow,
) -> Standing {
    /* DECISION-HISTORY-SHAPE-R3: the file is compared with the WHOLE document
     * the copy writer writes for this row — built by the same function, never
     * a hand-picked list of fields — so `exceptions`, and any field added to
     * the record later, is compared without anyone remembering to.
     * DECISION-HISTORY-SHAPE-R4: the comparison is order-sensitive, and stays
     * so, because both sides list `exceptions` in the ONE order
     * (`change_decision::in_canonical_order`): the file was written in it and
     * the row was read back in it, so an untouched copy is equal and a copy
     * whose exceptions were reordered by hand is not.
     * DECISION-FILE-AFTER-WITHDRAW: the file follows the latest act on its
     * version, so after a withdrawal (or a later approval of the same
     * version) an earlier approval's file holds a different row's copy and
     * reads `content_changed`; the copy of the act that stands reads
     * `readable`. A file that still says `"schema_version": 2` was written by
     * an older release and is compared with exactly what that release wrote
     * for this row, so an upgrade alone never makes an intact copy "changed". */
    let doc = match read_copy(path) {
        Ok(doc) => doc,
        Err(standing) => return standing,
    };
    let legacy = doc.as_ref().and_then(|d| d.get("schema_version"))
        == Some(&serde_json::json!(
            crate::change_decision::DECISION_RECORD_SCHEMA_VERSION_2
        ));
    let expected = if legacy {
        crate::change_decision::record_document_v2_for_row(change_id, row)
    } else {
        crate::change_decision::record_document_for_row(change_id, row)
    };
    same_document(doc, &expected)
}

/// Read a copy as JSON; `Err` is the standing of a copy that cannot be read
/// as one.
fn read_copy(path: &Path) -> std::result::Result<Option<serde_json::Value>, Standing> {
    let bytes = std::fs::read(path).map_err(|e| io_standing(&e))?;
    Ok(serde_json::from_slice::<serde_json::Value>(&bytes).ok())
}

/// A copy stands unchanged when, read as data, it is exactly `expected`:
/// whitespace and key order do not matter; any value, added key or removed
/// key does. A copy that is not JSON at all is changed.
fn same_document(doc: Option<serde_json::Value>, expected: &serde_json::Value) -> Standing {
    match doc {
        Some(doc) if doc == *expected => Standing::Readable,
        _ => Standing::ContentChanged,
    }
}

#[cfg(test)]
fn copy_standing_against(path: &Path, expected: &serde_json::Value) -> Standing {
    match read_copy(path) {
        Ok(doc) => same_document(doc, expected),
        Err(standing) => standing,
    }
}

fn io_standing(e: &std::io::Error) -> Standing {
    match e.kind() {
        std::io::ErrorKind::NotFound => Standing::Missing,
        std::io::ErrorKind::PermissionDenied => Standing::PermissionDenied,
        _ => Standing::Unreadable,
    }
}

/// Every piece of evidence a standing criterion link of this Change names:
/// the latest row per `(criterion, kind, evidence)`, dropping pairs whose
/// latest row takes the link away. Ordered by kind, then id, so the same
/// record always serves the same list; each carries the criteria it stands
/// linked to, in order.
type Linked = (String, String, String, Vec<i64>);

fn linked_evidence(ledger: &Ledger, change_id: &str) -> Result<Vec<Linked>> {
    let mut statement = ledger.conn().prepare(
        "SELECT criterion_index, evidence_kind, evidence_id, session_id, basis
           FROM change_criterion_evidence WHERE change_id=?1
          ORDER BY asserted_at, id",
    )?;
    let rows = statement.query_map(params![change_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    let mut latest: BTreeMap<(i64, String, String), (String, String)> = BTreeMap::new();
    for row in rows {
        let (criterion, kind, id, session, basis) = row?;
        latest.insert((criterion, kind, id), (session, basis));
    }
    let mut out: BTreeMap<(String, String), (String, Vec<i64>)> = BTreeMap::new();
    for ((criterion, kind, id), (session, basis)) in latest {
        if basis == "not-bearing" {
            continue;
        }
        let entry = out.entry((kind, id)).or_insert((session, Vec::new()));
        entry.1.push(criterion);
    }
    Ok(out
        .into_iter()
        .map(|((kind, id), (session, criteria))| (kind, id, session, criteria))
        .collect())
}

/// The named row a link points at, or `None` when the ledger holds no such row.
fn evidence_name(ledger: &Ledger, kind: &str, id: &str) -> Result<Option<String>> {
    let sql = match kind {
        "check_run" => "SELECT scope_key FROM evidence_bindings WHERE id=?1",
        "finding" => "SELECT claim_marker FROM speaker_findings WHERE id=?1",
        "observation" => "SELECT observation_type FROM observations WHERE id=?1",
        _ => return Ok(None),
    };
    let mut statement = ledger.conn().prepare(sql)?;
    let mut rows = statement.query(params![id])?;
    match rows.next()? {
        Some(row) => Ok(Some(row.get::<_, String>(0)?)),
        None => Ok(None),
    }
}

/// The sessions a person confirmed for this Change: the latest association
/// per session wins, exactly as `scopeOfWork` in `change-view.js` reads the
/// same served list.
fn confirmed_sessions(change: &crate::change::ChangeView) -> Vec<String> {
    let mut standing: BTreeMap<&str, crate::change::AssociationProvenance> = BTreeMap::new();
    for a in &change.associations {
        standing.insert(a.session_id.as_str(), a.provenance);
    }
    standing
        .into_iter()
        .filter(|(_, p)| *p == crate::change::AssociationProvenance::Confirmed)
        .map(|(s, _)| s.to_string())
        .collect()
}

/// `ingest::hash_lines`, restated (see the module header): line number →
/// SHA-256 of the line without its terminator, whitespace-only lines skipped
/// but counted. The error is the standing a person is told.
pub fn line_digests(path: &Path) -> std::result::Result<BTreeMap<u64, String>, Standing> {
    let file = std::fs::File::open(path).map_err(|e| io_standing(&e))?;
    let mut reader = std::io::BufReader::new(file);
    let mut out = BTreeMap::new();
    let mut line = 0u64;
    loop {
        let mut buf = Vec::new();
        let n = reader
            .read_until(b'\n', &mut buf)
            .map_err(|e| io_standing(&e))?;
        if n == 0 {
            break;
        }
        if buf.ends_with(b"\n") {
            buf.pop();
            if buf.ends_with(b"\r") {
                buf.pop();
            }
        }
        line += 1;
        if buf.iter().all(|b| b.is_ascii_whitespace()) {
            continue;
        }
        out.insert(line, sha256_hex(&buf));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change_decision::{record_document, DecisionAct, DecisionException};

    fn real() -> serde_json::Value {
        record_document(
            DecisionAct::Approved,
            "cccccccc-0000-4000-8000-00000000c001",
            "snap_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            1,
            "sam",
            "2026-09-22T11:00:00Z",
            &[DecisionException {
                waiver_id: "wv1".into(),
                obligation_id: "criterion:1".into(),
                kind: "waived".into(),
                actor: "sam".into(),
                authority: "accepted-profile".into(),
                rationale: "the payment sandbox is down all week".into(),
            }],
        )
    }

    /// A test-only builder: the real document plus one field the real
    /// copy writer does not write yet — standing in for a field someone
    /// adds to the record later.
    fn with_a_later_field() -> serde_json::Value {
        let mut doc = real();
        doc.as_object_mut()
            .map(|o| o.insert("reviewed_under".into(), serde_json::json!("profile-7")));
        doc
    }

    fn written(doc: &serde_json::Value) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        let path = dir.path().join("copy.json");
        let body = serde_json::to_vec_pretty(doc).unwrap_or_else(|e| panic!("json: {e}"));
        std::fs::write(&path, body).unwrap_or_else(|e| panic!("write: {e}"));
        (dir, path)
    }

    /// DECISION-HISTORY-SHAPE-R3, Done 2: a field added to the builder is
    /// compared without the check being touched. A file written by today's
    /// builder does not hold it, so against the extended builder it reads
    /// changed; a file that holds it reads unchanged against the extended
    /// builder and changed against today's.
    #[test]
    fn a_field_added_to_the_builder_is_compared_without_touching_the_check() {
        let (_d, today) = written(&real());
        assert_eq!(copy_standing_against(&today, &real()), Standing::Readable);
        assert_eq!(
            copy_standing_against(&today, &with_a_later_field()),
            Standing::ContentChanged,
            "a file missing the added field must not read as unchanged"
        );

        let (_e, later) = written(&with_a_later_field());
        assert_eq!(
            copy_standing_against(&later, &with_a_later_field()),
            Standing::Readable
        );
        assert_eq!(
            copy_standing_against(&later, &real()),
            Standing::ContentChanged,
            "a file holding a field the builder does not write is changed"
        );

        let mut other = with_a_later_field();
        other["reviewed_under"] = serde_json::json!("profile-8");
        let (_f, other) = written(&other);
        assert_eq!(
            copy_standing_against(&other, &with_a_later_field()),
            Standing::ContentChanged,
            "the added field's VALUE is compared, not only its presence"
        );
    }
}
