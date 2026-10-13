//! Exact Git rule-file proposals. A file import never accepts a decision.

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::chain::sha256_hex;
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::redact::{resembles_known_credential, Masked};

use super::grammar::{self, Actor, Proposal, Source};
use super::params::{parse_params, CheckParams, PARAMS_SCHEMA_VERSION};
use super::storage::{self, DecisionSource, NewDecision, SourceKind};

const ANCHOR_PREFIX: &str = "thruline.rule_file.v1:";
const MAX_FILE_BYTES: usize = 1024 * 1024;
const MAX_TREE_BYTES: usize = 8 * 1024 * 1024;
const MAX_RULE_FILES: usize = 1024;
const READ_BUDGET: Duration = Duration::from_secs(10);
const NOTES_TABLE: &str = "decision_rule_file_notes";
const NOTE_TABLE_DDL: &str = "CREATE TABLE IF NOT EXISTS decision_rule_file_notes(
 id TEXT PRIMARY KEY NOT NULL,
 project_id TEXT NOT NULL REFERENCES projects(id),
 source_anchor TEXT NOT NULL,
 code TEXT NOT NULL,
 span TEXT NOT NULL,
 source_json TEXT NOT NULL CHECK(json_valid(source_json)),
 anchor_json TEXT NOT NULL CHECK(json_valid(anchor_json)),
 reason TEXT NOT NULL,
 recorded_at TEXT NOT NULL,
 schema_version INTEGER NOT NULL CHECK(schema_version=1),
 UNIQUE(project_id,source_anchor,code))";
const NOTE_TRIGGERS: &[(&str, &str)] = &[
    ("decision_rule_file_notes_no_replace", "CREATE TRIGGER IF NOT EXISTS decision_rule_file_notes_no_replace BEFORE INSERT ON decision_rule_file_notes WHEN EXISTS(SELECT 1 FROM decision_rule_file_notes old WHERE old.id=NEW.id) BEGIN SELECT RAISE(ABORT,'rule file proposal notes are append-only'); END"),
    ("decision_rule_file_notes_no_update", "CREATE TRIGGER IF NOT EXISTS decision_rule_file_notes_no_update BEFORE UPDATE ON decision_rule_file_notes BEGIN SELECT RAISE(ABORT,'rule file proposal notes are append-only'); END"),
    ("decision_rule_file_notes_no_delete", "CREATE TRIGGER IF NOT EXISTS decision_rule_file_notes_no_delete BEFORE DELETE ON decision_rule_file_notes BEGIN SELECT RAISE(ABORT,'rule file proposal notes are append-only'); END"),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleFileAnchor {
    pub version_id: String,
    pub tree_hash: String,
    pub path: String,
    pub blob_oid: String,
    pub blob_sha256: String,
    pub raw_line: u64,
    pub span_start: u64,
    pub span_end: u64,
    pub span_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleFileProposal {
    pub decision_id: Option<String>,
    pub proposal_note_id: Option<String>,
    pub code: String,
    pub span: String,
    pub source: DecisionSource,
    pub anchor: RuleFileAnchor,
    pub params: Option<serde_json::Value>,
    pub created: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleFileNote {
    pub path: String,
    pub raw_line: u64,
    pub state: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleFileIngestReport {
    pub schema_version: u32,
    pub project_id: String,
    pub version_id: String,
    pub tree_hash: String,
    pub proposals: Vec<RuleFileProposal>,
    pub notes: Vec<RuleFileNote>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleSourceVerification {
    pub state: String,
    pub reason: Option<String>,
    pub anchor: Option<RuleFileAnchor>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleFileProposalNote {
    pub id: String,
    pub project_id: String,
    pub code: String,
    pub span: String,
    pub source: DecisionSource,
    pub anchor: RuleFileAnchor,
    pub state: String,
    pub reason: String,
    pub recorded_at: String,
    pub params: Option<serde_json::Value>,
    pub decision_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleFileNotesReport {
    pub schema_version: u32,
    pub state: String,
    pub notes: Vec<RuleFileProposalNote>,
}

/// Reading notes never installs a table or turns a span into a typed check.
pub fn notes_for_project(ledger: &Ledger, project_id: &str) -> Result<RuleFileNotesReport> {
    if !notes_exist(ledger)? {
        return Ok(RuleFileNotesReport {
            schema_version: 1,
            state: "not_recorded".into(),
            notes: Vec::new(),
        });
    }
    verify_notes_schema(ledger)?;
    let mut query = ledger.conn().prepare("SELECT id,project_id,source_anchor,code,span,source_json,anchor_json,reason,recorded_at,schema_version FROM decision_rule_file_notes WHERE project_id=?1 ORDER BY id LIMIT 1025")?;
    let rows = query
        .query_map([project_id], note_row)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if rows.len() > MAX_RULE_FILES {
        return Err(CoreError::Config(
            "rule proposal notes exceed their 1024 row bound".into(),
        ));
    }
    let notes = rows
        .into_iter()
        .map(decode_note)
        .collect::<Result<Vec<_>>>()?;
    Ok(RuleFileNotesReport {
        schema_version: 1,
        state: "recorded".into(),
        notes,
    })
}

/// Import tracked imperatives at one Git version as proposals only.
pub fn ingest(
    ledger: &Ledger,
    project_id: &str,
    root: &Path,
    version: &str,
    at: &str,
) -> Result<RuleFileIngestReport> {
    if project_id.trim().is_empty() || at.trim().is_empty() {
        return Err(CoreError::Config(
            "rule import needs a project and recorded time".into(),
        ));
    }
    let exists: bool = ledger.conn().query_row(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
        [project_id],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(CoreError::Config("rule import project is absent".into()));
    }
    if notes_exist(ledger)? {
        verify_notes_schema(ledger)?;
    }
    let deadline = Instant::now() + READ_BUDGET;
    let version_id = git_identity(root, version, "commit", deadline)?;
    let tree_hash = git_identity(root, &version_id, "tree", deadline)?;
    let listing = git(root, &["ls-tree", "-r", "-z", &tree_hash], deadline)?;
    if listing.len() > MAX_TREE_BYTES {
        return Err(CoreError::Config(
            "rule import tree exceeds its eight MiB bound".into(),
        ));
    }
    let mut report = RuleFileIngestReport {
        schema_version: 1,
        project_id: project_id.into(),
        version_id,
        tree_hash,
        proposals: Vec::new(),
        notes: Vec::new(),
    };
    let mut file_count = 0;
    for row in listing.split(|b| *b == 0).filter(|row| !row.is_empty()) {
        let (mode, oid, path) = tree_row(row)?;
        if !grammar::is_rule_file(&path) {
            continue;
        }
        if resembles_known_credential(&path) || Masked::mask(&path).text() != path {
            note(
                &mut report,
                "[masked rule path]",
                1,
                "unverifiable",
                "masked path bytes cannot identify an exact rule source",
            );
            continue;
        }
        file_count += 1;
        if file_count > MAX_RULE_FILES {
            return Err(CoreError::Config(
                "rule import exceeds its 1024 file bound".into(),
            ));
        }
        if !matches!(mode.as_str(), "100644" | "100755") {
            note(
                &mut report,
                &path,
                1,
                "unverifiable",
                "rule source must be a regular tracked Git blob",
            );
            continue;
        }
        let bytes = match read_blob(root, &oid, deadline) {
            Ok(bytes) => bytes,
            Err(error) => {
                note(&mut report, &path, 1, "unverifiable", &error.to_string());
                continue;
            }
        };
        let text = match std::str::from_utf8(&bytes) {
            Ok(text) => text,
            Err(_) => {
                note(
                    &mut report,
                    &path,
                    1,
                    "unverifiable",
                    "rule source blob is not UTF-8",
                );
                continue;
            }
        };
        for (base, block) in prose_blocks(text) {
            let line = line_at(text, base);
            if resembles_known_credential(block) || Masked::mask(block).text() != block {
                note(
                    &mut report,
                    &path,
                    line,
                    "unverifiable",
                    "masked source bytes cannot identify an exact rule span",
                );
                continue;
            }
            let reading = grammar::propose(&Source {
                kind: grammar::SourceKind::RuleFile,
                // Repository eligibility does not attest a person author.
                actor: Actor::Repository,
                actor_id: "rule_file",
                work: true,
                raw_file: &path,
                raw_line: line,
                event_id: None,
                text: block,
            })
            .map_err(|error| {
                CoreError::Config(format!("rule grammar refused source: {error:?}"))
            })?;
            for proposal in reading.proposals {
                let start = base
                    .checked_add(proposal.receipt.offset)
                    .ok_or_else(|| CoreError::Integrity("rule source span overflow".into()))?;
                let end = start
                    .checked_add(proposal.receipt.length)
                    .ok_or_else(|| CoreError::Integrity("rule source span overflow".into()))?;
                if text.get(start..end) != Some(proposal.text.as_str()) {
                    return Err(CoreError::Integrity(
                        "rule grammar span differs from exact Git bytes".into(),
                    ));
                }
                let mut anchor = RuleFileAnchor {
                    version_id: report.version_id.clone(),
                    tree_hash: report.tree_hash.clone(),
                    path: path.clone(),
                    blob_oid: oid.clone(),
                    blob_sha256: sha256_hex(&bytes),
                    raw_line: line_at(text, start),
                    span_start: start as u64,
                    span_end: end as u64,
                    span_sha256: sha256_hex(proposal.text.as_bytes()),
                };
                let file_anchor = format!("{ANCHOR_PREFIX}{}", serde_json::to_string(&anchor)?);
                let mut source = DecisionSource {
                    source_kind: SourceKind::RuleFile,
                    event_id: None,
                    file_anchor: Some(file_anchor.clone()),
                    raw_file: path.clone(),
                    raw_line: anchor.raw_line,
                    span_start: anchor.span_start,
                    span_end: anchor.span_end,
                    span: proposal.text.clone(),
                    actor: None,
                    recorded_at: None,
                };
                let params = proposed_params(&proposal);
                let mut proposal_note_id = None;
                let (decision_id, created) = match &params {
                    Some(params) => {
                        let id = storage::id_for_source(project_id, &file_anchor, params.kind());
                        let existing: Option<(String, String, u32, String, String, String)> = ledger.conn().query_row(
                            "SELECT d.project_id,r.kind,r.params_schema_version,r.text,r.sources_json,r.params_json
                             FROM decision_revisions r JOIN decisions d ON d.id=r.decision_id
                             WHERE r.decision_id=?1 AND r.rev=1",
                            [&id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
                        ).optional()?;
                        if let Some((saved_project, kind, schema, span, sources, saved_params)) =
                            existing
                        {
                            let saved_sources: Vec<DecisionSource> =
                                serde_json::from_str(&sources)?;
                            if saved_project != project_id
                                || kind != params.kind()
                                || schema != PARAMS_SCHEMA_VERSION
                                || span != proposal.text
                                || saved_sources != vec![source.clone()]
                                || saved_params != params.json()?
                            {
                                return Err(CoreError::Integrity(
                                    "existing rule proposal differs from its pinned source".into(),
                                ));
                            }
                            (Some(id), false)
                        } else if let Some((id, saved_source, saved_anchor)) =
                            previous_proposal(ledger, project_id, &anchor, &proposal.text, params)?
                        {
                            verify_inner(root, &saved_source)?;
                            source = saved_source;
                            anchor = saved_anchor;
                            (Some(id), false)
                        } else {
                            let created = storage::create(
                                ledger,
                                NewDecision {
                                    project_id: project_id.into(),
                                    source_anchor: file_anchor,
                                    span: proposal.text.clone(),
                                    sources: vec![source.clone()],
                                    author_actor: "rule_file".into(),
                                    recorded_at: at.into(),
                                    params: params.clone(),
                                },
                            )?;
                            (Some(created), true)
                        }
                    }
                    None => {
                        let (saved, created) = persist_note(
                            ledger,
                            project_id,
                            proposal.kind,
                            &proposal.text,
                            &source,
                            &anchor,
                            at,
                        )?;
                        verify_inner(root, &saved.source)?;
                        proposal_note_id = Some(saved.id);
                        source = saved.source;
                        anchor = saved.anchor;
                        note(
                            &mut report,
                            &path,
                            anchor.raw_line,
                            "needs_parameters",
                            "a person must supply the remaining typed check parameters",
                        );
                        (None, created)
                    }
                };
                report.proposals.push(RuleFileProposal {
                    decision_id,
                    proposal_note_id,
                    code: proposal.kind.into(),
                    span: proposal.text,
                    source,
                    anchor,
                    params: params
                        .map(|params| params.json())
                        .transpose()?
                        .map(|json| serde_json::from_str(&json))
                        .transpose()?,
                    created,
                });
            }
            for warning in reading.notes {
                note(
                    &mut report,
                    &path,
                    line,
                    "not_proposed",
                    &format!("{warning:?}"),
                );
            }
        }
    }
    Ok(report)
}

/// Re-read the exact pinned object and source span; missing evidence is explicit.
pub fn verify_source(root: &Path, source: &DecisionSource) -> Result<RuleSourceVerification> {
    let verified = verify_inner(root, source);
    Ok(match verified {
        Ok(anchor) => RuleSourceVerification {
            state: "verified".into(),
            reason: None,
            anchor: Some(anchor),
        },
        Err(error) => RuleSourceVerification {
            state: "unverifiable".into(),
            reason: Some(error.to_string()),
            anchor: None,
        },
    })
}

fn verify_inner(root: &Path, source: &DecisionSource) -> Result<RuleFileAnchor> {
    let fail = || CoreError::Integrity("rule source pin or exact span differs".into());
    let json = source
        .file_anchor
        .as_deref()
        .and_then(|anchor| anchor.strip_prefix(ANCHOR_PREFIX))
        .ok_or_else(fail)?;
    let anchor: RuleFileAnchor = serde_json::from_str(json)?;
    if source.source_kind != SourceKind::RuleFile
        || source.event_id.is_some()
        || source.actor.is_some()
        || source.recorded_at.is_some()
        || source.raw_file != anchor.path
        || !grammar::is_rule_file(&anchor.path)
        || source.raw_line != anchor.raw_line
        || source.span_start != anchor.span_start
        || source.span_end != anchor.span_end
        || anchor.span_end <= anchor.span_start
        || !oid_valid(&anchor.version_id)
        || !oid_valid(&anchor.tree_hash)
        || !oid_valid(&anchor.blob_oid)
        || !sha_valid(&anchor.blob_sha256)
        || !sha_valid(&anchor.span_sha256)
        || sha256_hex(source.span.as_bytes()) != anchor.span_sha256
    {
        return Err(fail());
    }
    let deadline = Instant::now() + READ_BUDGET;
    if git_identity(root, &anchor.version_id, "commit", deadline)? != anchor.version_id
        || git_identity(root, &anchor.version_id, "tree", deadline)? != anchor.tree_hash
    {
        return Err(fail());
    }
    let listing = git(
        root,
        &["ls-tree", "-z", &anchor.tree_hash, "--", &anchor.path],
        deadline,
    )?;
    let rows: Vec<_> = listing
        .split(|b| *b == 0)
        .filter(|row| !row.is_empty())
        .collect();
    if rows.len() != 1 {
        return Err(fail());
    }
    let (mode, oid, path) = tree_row(rows[0])?;
    if !matches!(mode.as_str(), "100644" | "100755")
        || oid != anchor.blob_oid
        || path != anchor.path
    {
        return Err(fail());
    }
    let bytes = read_blob(root, &oid, deadline)?;
    if sha256_hex(&bytes) != anchor.blob_sha256 {
        return Err(fail());
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| fail())?;
    let start = usize::try_from(anchor.span_start).map_err(|_| fail())?;
    let end = usize::try_from(anchor.span_end).map_err(|_| fail())?;
    if text.get(start..end) != Some(source.span.as_str()) || line_at(text, start) != anchor.raw_line
    {
        return Err(fail());
    }
    Ok(anchor)
}

fn proposed_params(proposal: &Proposal) -> Option<CheckParams> {
    let object = proposal
        .scope_hint
        .as_deref()
        .and_then(|suffix| proposal.object.strip_suffix(suffix))
        .unwrap_or(&proposal.object)
        .trim();
    let exact = object.trim_matches(['`', '"', '\'']);
    let surfaces = if proposal.scope_hint.as_deref().is_some_and(|hint| {
        matches!(
            hint.to_ascii_lowercase().as_str(),
            "from the ui" | "in the ui"
        )
    }) {
        serde_json::json!(["ui_copy"])
    } else {
        serde_json::json!(["code", "ui_copy", "docs", "tests", "ci", "other"])
    };
    let terms = proposal
        .terms
        .iter()
        .map(|term| {
            proposal
                .scope_hint
                .as_deref()
                .and_then(|suffix| term.strip_suffix(suffix))
                .unwrap_or(term)
                .trim()
                .trim_matches(['`', '"', '\''])
                .to_string()
        })
        .collect::<Vec<_>>();
    let value = match proposal.kind {
        "A1" => {
            serde_json::json!({"kind":"text_absent","terms":terms,"synonyms":[],"surfaces":surfaces,"allowlist":[],"test_allowlist":[]})
        }
        "A2" if terms.len() == 1
            && exact.chars().enumerate().all(|(index, ch)| {
                ch == '_'
                    || if index == 0 {
                        ch.is_alphabetic()
                    } else {
                        ch.is_alphanumeric()
                    }
            }) =>
        {
            serde_json::json!({"kind":"symbol_absent","symbol":exact,"language":null,"surfaces":surfaces,"allowlist":[]})
        }
        "B1" if !exact.contains(char::is_whitespace) => {
            serde_json::json!({"kind":"path_present","path":exact,"sha256":null})
        }
        "G2" => {
            serde_json::json!({"kind":"forbidden_words","words":terms,"surfaces":surfaces,"allowlist":[]})
        }
        _ => return None,
    };
    let kind = value.get("kind")?.as_str()?;
    parse_params(kind, PARAMS_SCHEMA_VERSION, &value.to_string()).ok()
}

fn note(report: &mut RuleFileIngestReport, path: &str, raw_line: u64, state: &str, reason: &str) {
    report.notes.push(RuleFileNote {
        path: path.into(),
        raw_line,
        state: state.into(),
        reason: Masked::mask(reason).into_text(),
    });
}

// Preserve contiguous prose for wrapped sentences, and exclude fenced examples.
fn prose_blocks(text: &str) -> Vec<(usize, &str)> {
    let mut blocks = Vec::new();
    let mut from = None;
    let mut offset = 0;
    let mut fence: Option<char> = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let marker = if trimmed.starts_with("```") {
            Some('`')
        } else if trimmed.starts_with("~~~") {
            Some('~')
        } else {
            None
        };
        if marker.is_some() {
            if let Some(start) = from.take() {
                blocks.push((start, &text[start..offset]));
            }
            if fence == marker {
                fence = None;
            } else if fence.is_none() {
                fence = marker;
            }
        } else if fence.is_none() {
            if trimmed.trim().is_empty() {
                if let Some(start) = from.take() {
                    blocks.push((start, &text[start..offset]));
                }
            } else if from.is_none() {
                from = Some(offset);
            }
        }
        offset += line.len();
    }
    if let Some(start) = from {
        blocks.push((start, &text[start..]));
    }
    blocks
}

fn line_at(text: &str, offset: usize) -> u64 {
    text.as_bytes()
        .get(..offset)
        .unwrap_or_default()
        .iter()
        .filter(|byte| **byte == b'\n')
        .count() as u64
        + 1
}

fn oid_valid(text: &str) -> bool {
    matches!(text.len(), 40 | 64)
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn sha_valid(text: &str) -> bool {
    text.len() == 64 && oid_valid(text)
}

fn tree_row(row: &[u8]) -> Result<(String, String, String)> {
    let fail = || CoreError::Integrity("rule source Git tree entry is invalid".into());
    let tab = row.iter().position(|b| *b == b'\t').ok_or_else(fail)?;
    let header = std::str::from_utf8(&row[..tab]).map_err(|_| fail())?;
    let fields: Vec<_> = header.split(' ').collect();
    if fields.len() != 3 || !matches!(fields[1], "blob" | "commit") || !oid_valid(fields[2]) {
        return Err(fail());
    }
    let path = std::str::from_utf8(&row[tab + 1..]).map_err(|_| fail())?;
    Ok((fields[0].into(), fields[2].into(), path.into()))
}

fn git_identity(root: &Path, version: &str, kind: &str, deadline: Instant) -> Result<String> {
    if version.trim().is_empty() || version.starts_with('-') || version.contains(['\0', '\n', '\r'])
    {
        return Err(CoreError::Config("rule source version is invalid".into()));
    }
    let spec = format!("{version}^{{{kind}}}");
    let bytes = git(
        root,
        &["rev-parse", "--verify", "--end-of-options", &spec],
        deadline,
    )?;
    let identity = std::str::from_utf8(&bytes)
        .map_err(|_| CoreError::Integrity("rule Git identity is not UTF-8".into()))?
        .trim();
    if !oid_valid(identity) {
        return Err(CoreError::Integrity("rule Git identity is invalid".into()));
    }
    Ok(identity.into())
}

fn read_blob(root: &Path, oid: &str, deadline: Instant) -> Result<Vec<u8>> {
    if !oid_valid(oid) {
        return Err(CoreError::Integrity(
            "rule Git blob identity is invalid".into(),
        ));
    }
    let size = git(root, &["cat-file", "-s", oid], deadline)?;
    let size = std::str::from_utf8(&size)
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .ok_or_else(|| CoreError::Integrity("rule Git blob size is unavailable".into()))?;
    if size > MAX_FILE_BYTES {
        return Err(CoreError::Config(
            "rule source exceeds its one MiB bound".into(),
        ));
    }
    let bytes = git(root, &["cat-file", "blob", oid], deadline)?;
    if bytes.len() != size {
        return Err(CoreError::Integrity("rule Git blob size differs".into()));
    }
    Ok(bytes)
}

fn git(root: &Path, args: &[&str], deadline: Instant) -> Result<Vec<u8>> {
    let mut command = Command::new("git");
    crate::git_isolation::isolate_config(&mut command)
        .args(args)
        .current_dir(root);
    match crate::bounded_child::output_until(&mut command, deadline) {
        Ok(crate::bounded_child::Outcome::Finished(output)) if output.status.success() => {
            Ok(output.stdout)
        }
        _ => Err(CoreError::Config(
            "exact rule Git evidence is unavailable or exceeded its read budget".into(),
        )),
    }
}

fn notes_exist(ledger: &Ledger) -> Result<bool> {
    Ok(ledger.conn().query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name=?1)",
        [NOTES_TABLE],
        |row| row.get(0),
    )?)
}

fn normalized_ddl(sql: &str) -> String {
    sql.chars()
        .filter(|ch| !ch.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .replace("ifnotexists", "")
}

fn verify_notes_schema(ledger: &Ledger) -> Result<()> {
    for (kind, name, ddl) in std::iter::once(("table", NOTES_TABLE, NOTE_TABLE_DDL)).chain(
        NOTE_TRIGGERS
            .iter()
            .map(|(name, ddl)| ("trigger", *name, *ddl)),
    ) {
        let saved: Option<String> = ledger
            .conn()
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type=?1 AND name=?2",
                rusqlite::params![kind, name],
                |row| row.get(0),
            )
            .optional()?;
        if saved.as_deref().map(normalized_ddl) != Some(normalized_ddl(ddl)) {
            return Err(CoreError::Integrity(
                "rule proposal note schema differs; no migration was performed".into(),
            ));
        }
    }
    Ok(())
}

fn install_notes_schema(ledger: &Ledger) -> Result<()> {
    if notes_exist(ledger)? {
        return verify_notes_schema(ledger);
    }
    for (name, _) in NOTE_TRIGGERS {
        let occupied: bool = ledger.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name=?1)",
            [name],
            |row| row.get(0),
        )?;
        if occupied {
            return Err(CoreError::Integrity(
                "rule proposal note trigger name is already occupied".into(),
            ));
        }
    }
    let tx = ledger.conn().unchecked_transaction()?;
    tx.execute_batch(NOTE_TABLE_DDL)?;
    for (_, ddl) in NOTE_TRIGGERS {
        tx.execute_batch(ddl)?;
    }
    verify_notes_schema(ledger)?;
    tx.commit()?;
    Ok(())
}

fn content_key(project: &str, anchor: &RuleFileAnchor, kind: &str) -> String {
    sha256_hex(
        format!(
            "thruline.rule.source.v1\0{project}\0{}\0{}\0{}\0{}\0{}\0{kind}",
            anchor.path, anchor.blob_sha256, anchor.span_start, anchor.span_end, anchor.span_sha256
        )
        .as_bytes(),
    )
}

#[derive(Debug)]
struct SavedNote {
    id: String,
    project: String,
    source_anchor: String,
    code: String,
    span: String,
    source_json: String,
    anchor_json: String,
    reason: String,
    at: String,
    schema: u32,
}
fn note_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SavedNote> {
    Ok(SavedNote {
        id: row.get(0)?,
        project: row.get(1)?,
        source_anchor: row.get(2)?,
        code: row.get(3)?,
        span: row.get(4)?,
        source_json: row.get(5)?,
        anchor_json: row.get(6)?,
        reason: row.get(7)?,
        at: row.get(8)?,
        schema: row.get(9)?,
    })
}

fn decode_note(row: SavedNote) -> Result<RuleFileProposalNote> {
    let source: DecisionSource = serde_json::from_str(&row.source_json)?;
    let anchor: RuleFileAnchor = serde_json::from_str(&row.anchor_json)?;
    let encoded = format!("{ANCHOR_PREFIX}{}", serde_json::to_string(&anchor)?);
    let expected = format!(
        "rule-note-{}",
        content_key(&row.project, &anchor, &row.code)
    );
    if row.schema != 1
        || row.id != expected
        || row.source_anchor != encoded
        || source.file_anchor.as_deref() != Some(encoded.as_str())
        || source.source_kind != SourceKind::RuleFile
        || source.event_id.is_some()
        || source.actor.is_some()
        || source.recorded_at.is_some()
        || source.span != row.span
        || source.raw_file != anchor.path
        || source.raw_line != anchor.raw_line
        || source.span_start != anchor.span_start
        || source.span_end != anchor.span_end
        || anchor.span_end <= anchor.span_start
        || anchor.span_end - anchor.span_start != row.span.len() as u64
        || anchor.raw_line == 0
        || !grammar::is_rule_file(&anchor.path)
        || !oid_valid(&anchor.version_id)
        || !oid_valid(&anchor.tree_hash)
        || !oid_valid(&anchor.blob_oid)
        || !sha_valid(&anchor.blob_sha256)
        || sha256_hex(row.span.as_bytes()) != anchor.span_sha256
        || Masked::mask(&row.span).text() != row.span
        || resembles_known_credential(&row.span)
        || Masked::mask(&source.raw_file).text() != source.raw_file
        || resembles_known_credential(&source.raw_file)
        || Masked::mask(&row.reason).text() != row.reason
        || resembles_known_credential(&row.reason)
    {
        return Err(CoreError::Integrity(
            "stored rule proposal note differs from its source pin".into(),
        ));
    }
    Ok(RuleFileProposalNote {
        id: row.id,
        project_id: row.project,
        code: row.code,
        span: row.span,
        source,
        anchor,
        state: "needs_parameters".into(),
        reason: row.reason,
        recorded_at: row.at,
        params: None,
        decision_id: None,
    })
}

#[cfg(test)]
#[path = "rule_files_mask_tests.rs"]
mod mask_tests;

fn persist_note(
    ledger: &Ledger,
    project: &str,
    code: &str,
    span: &str,
    source: &DecisionSource,
    anchor: &RuleFileAnchor,
    at: &str,
) -> Result<(RuleFileProposalNote, bool)> {
    install_notes_schema(ledger)?;
    let id = format!("rule-note-{}", content_key(project, anchor, code));
    let existing = ledger.conn().query_row("SELECT id,project_id,source_anchor,code,span,source_json,anchor_json,reason,recorded_at,schema_version FROM decision_rule_file_notes WHERE id=?1", [&id], note_row).optional()?;
    if let Some(existing) = existing {
        let saved = decode_note(existing)?;
        if saved.span != span
            || saved.project_id != project
            || saved.code != code
            || content_key(project, &saved.anchor, code) != content_key(project, anchor, code)
        {
            return Err(CoreError::Integrity(
                "existing rule proposal note differs from its source".into(),
            ));
        }
        return Ok((saved, false));
    }
    let source_anchor = source
        .file_anchor
        .as_deref()
        .ok_or_else(|| CoreError::Integrity("rule proposal note has no source anchor".into()))?;
    let reason = "a person must supply the remaining typed check parameters";
    ledger.conn().execute("INSERT INTO decision_rule_file_notes(id,project_id,source_anchor,code,span,source_json,anchor_json,reason,recorded_at,schema_version) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,1)", rusqlite::params![id,project,source_anchor,code,span,serde_json::to_string(source)?,serde_json::to_string(anchor)?,reason,at])?;
    Ok((
        RuleFileProposalNote {
            id,
            project_id: project.into(),
            code: code.into(),
            span: span.into(),
            source: source.clone(),
            anchor: anchor.clone(),
            state: "needs_parameters".into(),
            reason: reason.into(),
            recorded_at: at.into(),
            params: None,
            decision_id: None,
        },
        true,
    ))
}

type PreviousProposal = (String, DecisionSource, RuleFileAnchor);
fn previous_proposal(
    ledger: &Ledger,
    project: &str,
    anchor: &RuleFileAnchor,
    span: &str,
    params: &CheckParams,
) -> Result<Option<PreviousProposal>> {
    let mut query = ledger.conn().prepare("SELECT d.id,r.text,r.sources_json,r.params_json,r.params_schema_version FROM decisions d JOIN decision_revisions r ON r.decision_id=d.id WHERE d.project_id=?1 AND r.rev=1 AND r.kind=?2 ORDER BY d.id LIMIT 1025")?;
    let rows = query
        .query_map(rusqlite::params![project, params.kind()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, u32>(4)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if rows.len() > MAX_RULE_FILES {
        return Err(CoreError::Config(
            "rule deduplication exceeds its 1024 proposal bound".into(),
        ));
    }
    for (id, saved_span, sources, saved_params, schema) in rows {
        let sources: Vec<DecisionSource> = serde_json::from_str(&sources)?;
        let [source] = sources.as_slice() else {
            continue;
        };
        if source.source_kind != SourceKind::RuleFile {
            continue;
        }
        let Some(encoded) = source
            .file_anchor
            .as_deref()
            .and_then(|value| value.strip_prefix(ANCHOR_PREFIX))
        else {
            continue;
        };
        let saved_anchor: RuleFileAnchor = serde_json::from_str(encoded)?;
        if content_key(project, &saved_anchor, params.kind())
            != content_key(project, anchor, params.kind())
        {
            continue;
        }
        if saved_span != span
            || source.span != span
            || saved_params != params.json()?
            || schema != PARAMS_SCHEMA_VERSION
        {
            return Err(CoreError::Integrity(
                "existing rule proposal differs from its pinned typed bytes".into(),
            ));
        }
        return Ok(Some((id, source.clone(), saved_anchor)));
    }
    Ok(None)
}
