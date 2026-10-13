//! Exact parameter documents can propose a revision; they cannot grant authority.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Serialize;

use crate::chain::sha256_hex;
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::redact::Masked;

use super::params::parse_params;
use super::storage::{self, DecisionSource, RevisionInput, RevisionParent, SourceKind};

pub struct ProposalInput<'a> {
    pub project_id: &'a str,
    pub decision_id: &'a str,
    pub parent_revision: u32,
    pub parent_params_sha256: &'a str,
    pub parent_sources_sha256: &'a str,
    pub params_bytes: &'a [u8],
    pub params_sha256: &'a str,
    pub at: &'a str,
}

#[derive(Debug, Serialize)]
pub struct ProposalReceipt {
    pub schema_version: u32,
    pub decision_id: String,
    pub parent_revision: u32,
    pub revision: u32,
    pub kind: String,
    pub input_sha256: String,
    pub params_sha256: String,
    pub sources_sha256: String,
    pub state: &'static str,
    pub person_act_recorded: bool,
}

fn refused(reason: &str) -> CoreError {
    CoreError::Config(reason.into())
}

pub fn append(ledger: &Ledger, repo: &Path, input: ProposalInput<'_>) -> Result<ProposalReceipt> {
    if input.parent_revision == 0
        || input.params_bytes.is_empty()
        || input.params_bytes.len() > 1024 * 1024
        || ![
            input.parent_params_sha256,
            input.parent_sources_sha256,
            input.params_sha256,
        ]
        .iter()
        .all(|hash| {
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        || sha256_hex(input.params_bytes) != input.params_sha256
    {
        return Err(refused(
            "revision proposal needs exact SHA-256 pins and a bounded parameter document",
        ));
    }
    let (kind, schema, parent, span, raw_sources): (String, u32, String, String, String) =
        ledger.conn().query_row(
            "SELECT r.kind,r.params_schema_version,r.params_json,r.text,r.sources_json
         FROM decision_revisions r JOIN decisions d ON d.id=r.decision_id
         WHERE d.project_id=?1 AND d.id=?2 AND r.rev=?3",
            rusqlite::params![input.project_id, input.decision_id, input.parent_revision],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )?;
    if sha256_hex(parent.as_bytes()) != input.parent_params_sha256
        || sha256_hex(raw_sources.as_bytes()) != input.parent_sources_sha256
    {
        return Err(refused("revision parent parameters or source pins differ"));
    }
    parse_params(&kind, schema, &parent)?;
    let document = std::str::from_utf8(input.params_bytes)
        .map_err(|_| refused("revision parameters must be UTF-8 JSON"))?;
    let params = parse_params(&kind, schema, document)?;
    let normalized = params.json()?;
    let sources: Vec<DecisionSource> = serde_json::from_str(&raw_sources)?;
    if sources.is_empty() || sources.len() > 64 || !sources.iter().any(|s| s.span == span) {
        return Err(refused(
            "revision source span is absent or exceeds the source bound",
        ));
    }
    let mut verified = BTreeSet::new();
    for source in &sources {
        match source.source_kind {
            SourceKind::WorkPrompt => {
                let event_id = source
                    .event_id
                    .as_deref()
                    .ok_or_else(|| refused("work prompt needs its native event"))?;
                if verified.insert(event_id)
                    && !super::authority::verified_user_event(ledger, event_id)?
                {
                    return Err(refused(
                        "revision source is not an intact canonical person message",
                    ));
                }
                let (project, raw_file, raw_line, text): (String, String, i64, String) =
                    ledger.conn().query_row(
                        "SELECT project_id,raw_file,raw_line,text FROM events WHERE id=?1",
                        [event_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                    )?;
                let text = Masked::mask(&text).into_text();
                let start = usize::try_from(source.span_start).ok();
                let end = usize::try_from(source.span_end).ok();
                if project != input.project_id
                    || raw_file != source.raw_file
                    || u64::try_from(raw_line).ok() != Some(source.raw_line)
                    || source.span_end <= source.span_start
                    || start.zip(end).and_then(|(a, b)| text.get(a..b))
                        != Some(source.span.as_str())
                {
                    return Err(refused(
                        "revision source does not match its native project, receipt and exact span",
                    ));
                }
            }
            SourceKind::RuleFile => {
                if super::rule_files::verify_source(repo, source)?.state != "verified" {
                    return Err(refused(
                        "revision rule source does not verify at its pinned Git version",
                    ));
                }
            }
            _ => {
                return Err(refused(
                    "revision source family has no qualified staging adapter",
                ))
            }
        }
    }
    let revision = storage::append_revision_if_parent(
        ledger,
        RevisionInput {
            decision_id: input.decision_id.into(),
            span,
            sources,
            author_actor: "parameter-proposal".into(),
            recorded_at: input.at.into(),
            params,
        },
        RevisionParent {
            revision: input.parent_revision,
            params_sha256: input.parent_params_sha256,
            sources_sha256: input.parent_sources_sha256,
        },
    )?;
    Ok(ProposalReceipt {
        schema_version: 1,
        decision_id: input.decision_id.into(),
        parent_revision: input.parent_revision,
        revision,
        kind,
        input_sha256: input.params_sha256.into(),
        params_sha256: sha256_hex(normalized.as_bytes()),
        sources_sha256: input.parent_sources_sha256.into(),
        state: "proposed",
        person_act_recorded: false,
    })
}
