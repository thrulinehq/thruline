//! CLI handlers for person decisions and exact-version checks.
//!
//! Person acts append through the core storage API. A derived repository file
//! is regenerated after each successful act; it is never accepted as input.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek, SeekFrom, Write};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::json_out;
use anyhow::{bail, Context, Result};
use clap::{Args as ClapArgs, Subcommand};
use serde::Deserialize;
use serde_json::{json, Value};
use thruline_core::chain::sha256_hex;
use thruline_core::decisions::authority::bridge;
use thruline_core::decisions::authority::{self, PersonActKind};
use thruline_core::decisions::checks::VerdictState;
use thruline_core::decisions::engine::{self, RunResult};
use thruline_core::decisions::grammar;
use thruline_core::decisions::inline;
use thruline_core::decisions::kinds_k::{self, KCoverage};
use thruline_core::decisions::params::{parse_params, CheckParams, Surface, TextAbsentParams};
use thruline_core::decisions::storage::{
    self, AcceptanceInput, DecisionSource, NewDecision, RetirementInput, RevisionInput,
    SourceKind as StoredSourceKind, WaiverInput,
};
use thruline_core::decisions::why::{self, ChainLabel};
use thruline_core::ledger::Ledger;
use thruline_core::redact::Masked;
use thruline_policy::decisions::{
    Accepted as GateAccepted, DecisionState, GateAction, Receipt, ReceiptVerdict,
    Subject as GateSubject,
};
use thruline_policy::{Conclusion, Policy};

pub enum Verb {
    List,
    Propose {
        event_id: String,
    },
    Accept {
        decision_id: String,
        event_id: String,
        /// None accepts the latest proposed revision.
        revision: Option<u32>,
        actor: String,
        at: String,
        because: String,
        severity: String,
        applies_from_version: String,
        terms: Vec<String>,
        allowlist: Vec<String>,
        surfaces: Vec<Surface>,
    },
    Retire {
        decision_id: String,
        event_id: String,
        actor: String,
        at: String,
        because: String,
        superseded_by: Option<String>,
    },
    Check {
        version_id: String,
        decision_id: Option<String>,
        at: String,
        replay: bool,
    },
}

pub struct Args {
    pub db: PathBuf,
    pub project_id: String,
    pub project_root: PathBuf,
    pub verb: Verb,
    pub json: bool,
}

pub struct Outcome {
    pub exit_code: u8,
    pub body: Value,
    pub summary: String,
}

/// Command-line arguments shared by `decide` and `decisions`.
#[derive(Debug, ClapArgs)]
pub struct ProjectCliOptions {
    /// Project id from the local ledger. If omitted, find the project for this directory.
    #[arg(long, global = true)]
    pub project: Option<String>,
    /// Repository root; useful for checking a scratch copy of a recorded project.
    #[arg(long, global = true)]
    pub root: Option<PathBuf>,
    #[arg(long, global = true)]
    pub json: bool,
}

fn shell_arg(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn terminal_confirm_command(project: &str, root: &Path, action: &str, act_id: &str) -> String {
    format!(
        "thruline decide --project={} --root={} confirm {} {}",
        shell_arg(project),
        shell_arg(&root.to_string_lossy()),
        shell_arg(action),
        shell_arg(act_id),
    )
}

fn terminal_repair_command(project: &str, root: &Path) -> String {
    format!(
        "thruline decide --project={} --root={} repair-derived",
        shell_arg(project),
        shell_arg(&root.to_string_lossy()),
    )
}

/// Build one top-level clap variant for both documented command spellings.
/// The nested families keep their own derive frames, which matters on the
/// default 2 MiB test-thread stack used by the full CLI parser.
#[derive(Debug, ClapArgs)]
pub struct DecisionRootCliArgs {
    #[command(flatten)]
    pub project: ProjectCliOptions,
    #[command(subcommand)]
    pub verb: DecisionRootCliVerb,
}

#[derive(Debug, Subcommand)]
pub enum DecisionRootCliVerb {
    #[command(flatten)]
    Person(DecideCliVerb),
    #[command(flatten)]
    Checks(DecisionsCliVerb),
}

#[derive(Debug, ClapArgs)]
pub struct DecideCliArgs {
    #[command(flatten)]
    pub project: ProjectCliOptions,
    #[command(subcommand)]
    pub verb: DecideCliVerb,
}

#[derive(Debug, Subcommand)]
pub enum DecideCliVerb {
    /// List proposed, accepted and retired decisions.
    List,
    /// Import tracked repository rules as proposals at one immutable version.
    Rules {
        #[arg(long, default_value = "HEAD")]
        version: String,
    },
    /// Append complete check parameters as an inert, source-verified proposal.
    ReviseParams {
        decision_id: String,
        #[arg(long)]
        parent_revision: u32,
        #[arg(long)]
        parent_params_sha256: String,
        #[arg(long)]
        parent_sources_sha256: String,
        #[arg(long)]
        params_file: PathBuf,
        #[arg(long)]
        params_sha256: String,
    },
    /// Offer checkable spans from a recorded person work prompt.
    Propose {
        #[arg(long)]
        event_id: String,
        /// Source family. Only recorded prompts are supported by this command yet.
        #[arg(long, value_parser = ["prompts"])]
        from: Option<String>,
    },
    /// Accept one revision as a standing decision.
    Accept {
        decision_id: String,
        /// Recorded user confirmation with the exact acceptance tuple.
        #[arg(long)]
        event_id: String,
        #[arg(long)]
        revision: Option<u32>,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        because: String,
        #[arg(long, default_value = "blocks", value_parser = ["blocks", "warns"])]
        severity: String,
        /// Named version where this acceptance begins; defaults to the current commit.
        #[arg(long)]
        from_version: Option<String>,
        /// Replace the proposed A1 terms with words chosen by the person.
        #[arg(long = "term")]
        terms: Vec<String>,
        /// Allow accepted paths to contain the term.
        #[arg(long = "allow")]
        allowlist: Vec<String>,
        /// Restrict A1 to named surfaces; omitting this checks all proposed surfaces.
        #[arg(long = "surface", value_parser = ["code", "ui_copy", "docs", "tests", "ci", "other"])]
        surfaces: Vec<String>,
    },
    /// Retire an accepted decision, retaining its earlier verdicts.
    Retire {
        decision_id: String,
        /// Recorded user confirmation with the exact retirement tuple.
        #[arg(long)]
        event_id: String,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        because: String,
        #[arg(long)]
        superseded_by: Option<String>,
    },
    /// Confirm a pending person act at this terminal after macOS user presence.
    Confirm {
        #[arg(value_parser = ["accept", "retire", "waive", "supersede"])]
        action: String,
        act_row_id: String,
        #[arg(long)]
        predecessor_revision: Option<u32>,
        #[arg(long)]
        screen: Option<String>,
    },
    /// Rebuild the local derived copy from confirmed ledger decisions.
    RepairDerived,
    /// Prepare an inert act for exact interactive confirmation.
    Prepare {
        #[arg(value_parser = ["accept", "retire", "waive"])]
        action: String,
        decision_id: String,
        #[arg(long)]
        revision: Option<u32>,
        /// Display label only; the bridge checks the OS account separately.
        #[arg(long)]
        actor: String,
        #[arg(long)]
        because: String,
        #[arg(long, value_parser = ["blocks", "warns"])]
        severity: Option<String>,
        #[arg(long)]
        from_version: Option<String>,
        #[arg(long)]
        version_id: Option<String>,
        #[arg(long)]
        expires_version: Option<String>,
        #[arg(long)]
        expires_at: Option<String>,
        #[arg(long)]
        superseded_by: Option<String>,
    },
}

#[derive(Debug, ClapArgs)]
pub struct DecisionsCliArgs {
    #[command(flatten)]
    pub project: ProjectCliOptions,
    #[command(subcommand)]
    pub verb: DecisionsCliVerb,
}

#[derive(Debug, Subcommand)]
pub enum DecisionsCliVerb {
    /// Show the eighteen proposed Family K advisory defaults for first use.
    Defaults,
    /// Run a bounded read-only sweep of cheap checks on one exact version.
    Inline {
        #[arg(long)]
        version: String,
        /// Inspect latest proposal revisions as advisory diagnostics only.
        #[arg(long)]
        diagnose_proposals: bool,
        /// Internal supervised child path; no person act or verdict write.
        #[arg(long, hide = true)]
        worker: bool,
        /// Internal multi-member request from the local HQ route.
        #[arg(long, hide = true)]
        project_member: Vec<String>,
    },
    /// Check accepted decisions against one exact project version.
    Check {
        /// Git commit or other version id. Defaults to the current commit.
        #[arg(long)]
        version: Option<String>,
        /// Explicitly check all accepted decisions (the default).
        #[arg(long, conflicts_with = "id")]
        all: bool,
        /// Check only this accepted decision.
        #[arg(long)]
        id: Option<String>,
        /// Recompute against a stored exact-tree verdict and compare it without appending.
        #[arg(long)]
        replay: bool,
        /// Save a ledger-backed receipt for the exact checked version.
        #[arg(long, conflicts_with_all = ["id", "replay"])]
        receipt_out: Option<PathBuf>,
    },
    /// Apply the base branch's decisions policy to the current Git HEAD.
    Gate {
        /// Base ref whose policy governs this PR; defaults to GITHUB_BASE_REF.
        #[arg(long)]
        base: Option<String>,
        /// Receipt emitted by `decisions check --receipt-out`.
        #[arg(long)]
        receipt: Option<PathBuf>,
        /// Optional repository-relative policy path.
        #[arg(long)]
        policy: Option<PathBuf>,
    },
}

pub fn execute(ledger: &Ledger, args: &Args) -> Result<Outcome> {
    match &args.verb {
        Verb::List => list(ledger, &args.project_id),
        Verb::Propose { event_id } => propose_event(ledger, &args.project_id, event_id),
        Verb::Accept {
            decision_id,
            event_id,
            revision,
            actor,
            at,
            because,
            severity,
            applies_from_version,
            terms,
            allowlist,
            surfaces,
        } => {
            require_project_decision(ledger, &args.project_id, decision_id)?;
            if actor.trim().is_empty() || because.trim().is_empty() {
                bail!("acceptance requires a person actor and a reason");
            }
            if !matches!(severity.as_str(), "blocks" | "warns")
                || applies_from_version.trim().is_empty()
            {
                bail!("acceptance requires a valid severity and version");
            }
            anyhow::ensure!(
                authority::verified_user_event(ledger, event_id)?,
                "acceptance requires a recorded user confirmation event"
            );
            let selected_revision = match revision {
                Some(revision) => *revision,
                None => latest_revision(ledger, decision_id)?,
            };
            let revision = if terms.is_empty() && allowlist.is_empty() && surfaces.is_empty() {
                selected_revision
            } else {
                append_person_params_revision(
                    ledger,
                    PersonParamsRevision {
                        decision_id,
                        selected_revision,
                        actor,
                        at,
                        terms,
                        allowlist,
                        surfaces,
                    },
                )?
            };
            let acceptance_id = storage::accept(
                ledger,
                AcceptanceInput {
                    decision_id: decision_id.clone(),
                    revision,
                    actor: actor.clone(),
                    at: at.clone(),
                    because: because.clone(),
                    severity: severity.clone(),
                    applies_from_version: applies_from_version.clone(),
                },
            )?;
            authority::record(ledger, PersonActKind::Accept, &acceptance_id, event_id)
                .context("acceptance is pending: user confirmation does not match the exact act")?;
            let path = storage::write_derived_file(ledger, &args.project_id, &args.project_root)?;
            Ok(Outcome {
                exit_code: 0,
                body: json!({
                    "schema_version":1,
                    "decision_id":decision_id,
                    "revision":revision,
                    "acceptance_id":acceptance_id,
                    "lifecycle":"pending_bridge",
                    "derived_file":path,
                }),
                summary: format!("Decision {decision_id} revision {revision} has a historical source receipt; interactive confirmation is still pending."),
            })
        }
        Verb::Retire {
            decision_id,
            event_id,
            actor,
            at,
            because,
            superseded_by,
        } => {
            require_project_decision(ledger, &args.project_id, decision_id)?;
            anyhow::ensure!(
                authority::verified_user_event(ledger, event_id)?,
                "retirement requires a recorded user confirmation event"
            );
            if let Some(next_id) = superseded_by {
                require_project_decision(ledger, &args.project_id, next_id)?;
            }
            let retirement_id = storage::retire(
                ledger,
                RetirementInput {
                    decision_id: decision_id.clone(),
                    actor: actor.clone(),
                    at: at.clone(),
                    because: because.clone(),
                    superseded_by: superseded_by.clone(),
                },
            )?;
            authority::record(ledger, PersonActKind::Retire, &retirement_id, event_id)
                .context("retirement is pending: user confirmation does not match the exact act")?;
            let path = storage::write_derived_file(ledger, &args.project_id, &args.project_root)?;
            Ok(Outcome {
                exit_code: 0,
                body: json!({
                    "schema_version":1,
                    "decision_id":decision_id,
                    "retirement_id":retirement_id,
                    "lifecycle":"pending_bridge",
                    "derived_file":path,
                }),
                summary: format!("Decision {decision_id} retirement has a historical source receipt; interactive confirmation is still pending."),
            })
        }
        Verb::Check {
            version_id,
            decision_id,
            at,
            replay,
        } => check(
            ledger,
            &args.project_id,
            &args.project_root,
            version_id,
            decision_id.as_deref(),
            at,
            *replay,
        ),
    }
}

fn list(ledger: &Ledger, project_id: &str) -> Result<Outcome> {
    let mut statement = ledger.conn().prepare(
        "SELECT d.id,r.rev,r.kind,r.params_schema_version,r.params_json,r.text,r.sources_json
         FROM decisions d
         JOIN decision_revisions r ON r.decision_id=d.id
           AND r.rev=(SELECT max(next.rev) FROM decision_revisions next WHERE next.decision_id=d.id)
         WHERE d.project_id=?1 ORDER BY d.id",
    )?;
    let rows = statement.query_map([project_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, u32>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, u32>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;
    let mut decisions = Vec::new();
    let mut lines = Vec::new();
    for row in rows {
        let (
            id,
            latest_proposed_revision,
            mut kind,
            mut schema_version,
            mut params_json,
            mut span,
            sources_json,
        ) = row?;
        let latest_proposal_pins = json!({
            "revision":latest_proposed_revision,
            "params_sha256":sha256_hex(params_json.as_bytes()),
            "sources_sha256":sha256_hex(sources_json.as_bytes()),
        });
        let accepted = storage::verified_acceptance_for_decision(ledger, &id)?;
        let retirement = storage::verified_retirement_for_decision(ledger, &id)?;
        let accepted_revision = accepted.as_ref().map(|row| row.revision);
        let severity = accepted.as_ref().map(|row| row.severity.clone());
        let revision = accepted_revision.unwrap_or(latest_proposed_revision);
        if accepted_revision.is_some() {
            (kind, schema_version, params_json, span) = ledger.conn().query_row(
                "SELECT kind,params_schema_version,params_json,text FROM decision_revisions WHERE decision_id=?1 AND rev=?2",
                (&id, revision), |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
        }
        let params = parse_params(&kind, schema_version, &params_json)?;
        let lifecycle = if let Some(retirement) = &retirement {
            if retirement.superseded_by.is_some() {
                "superseded"
            } else {
                "retired"
            }
        } else if accepted_revision.is_some() {
            "accepted"
        } else {
            "proposed"
        };
        let span = Masked::mask(&span).into_text();
        lines.push(format!("{lifecycle} {id}: {span}"));
        decisions.push(json!({
            "id":id,
            "revision":revision,
            "latest_proposed_revision":latest_proposed_revision,
            "latest_proposal_pins":latest_proposal_pins,
            "kind":kind,
            "params_schema_version":schema_version,
            "params":mask_json(&serde_json::to_value(params)?),
            "params_sha256":sha256_hex(params_json.as_bytes()),
            "span":span,
            "lifecycle":lifecycle,
            "accepted_revision":accepted_revision,
            "severity":severity,
        }));
    }
    Ok(Outcome {
        exit_code: 0,
        body: json!({"schema_version":1,"project_id":project_id,"decisions":decisions}),
        summary: if lines.is_empty() {
            "No decisions recorded for this project.".into()
        } else {
            lines.join("\n")
        },
    })
}

fn mask_json(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(Masked::mask(text).into_text()),
        Value::Array(items) => Value::Array(items.iter().map(mask_json).collect()),
        Value::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), mask_json(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn require_project_decision(ledger: &Ledger, project_id: &str, decision_id: &str) -> Result<()> {
    let belongs: bool = ledger.conn().query_row(
        "SELECT EXISTS(SELECT 1 FROM decisions WHERE id=?1 AND project_id=?2)",
        (decision_id, project_id),
        |row| row.get(0),
    )?;
    if !belongs {
        bail!("decision does not belong to this project");
    }
    Ok(())
}

fn latest_revision(ledger: &Ledger, decision_id: &str) -> Result<u32> {
    ledger
        .conn()
        .query_row(
            "SELECT max(rev) FROM decision_revisions WHERE decision_id=?1",
            [decision_id],
            |row| row.get::<_, Option<u32>>(0),
        )?
        .context("decision has no revision")
}

fn propose_event(ledger: &Ledger, project_id: &str, event_id: &str) -> Result<Outcome> {
    let (event_project, kind, role, text, raw_file, raw_line, recorded_at): (
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        i64,
        Option<String>,
    ) = ledger
        .conn()
        .query_row(
            "SELECT project_id,kind,role,text,raw_file,raw_line,ts FROM events WHERE id=?1",
            [event_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .context("recorded event was not found")?;
    if event_project != project_id || kind != "user_msg" {
        bail!("only a person message in this project can propose a decision");
    }
    anyhow::ensure!(
        authority::verified_user_event(ledger, event_id)?,
        "proposal needs a recorded canonical user message with intact raw source and chain"
    );
    let raw_line = u64::try_from(raw_line).context("person source line must be positive")?;
    if role
        .as_deref()
        .is_some_and(|role| !matches!(role, "user" | "human" | "person"))
    {
        bail!("recorded event is not a person message");
    }
    let text = Masked::mask(&text.context("person event has no text")?).into_text();
    let actor_id = role.as_deref().unwrap_or("person");
    let source = grammar::Source {
        kind: grammar::SourceKind::WorkPrompt,
        actor: grammar::Actor::Person,
        actor_id,
        work: true,
        raw_file: &raw_file,
        raw_line,
        event_id: Some(event_id),
        text: &text,
    };
    let reading = grammar::propose(&source)
        .map_err(|reason| anyhow::anyhow!("person source cannot be proposed: {reason:?}"))?;
    let mut proposals = Vec::new();
    let mut unparameterized = Vec::new();
    for proposal in reading.proposals {
        // Other grammar families remain visible, but need a closed parameter
        // builder before they can become accept-ready ledger proposals.
        if proposal.kind != "A1" {
            unparameterized.push(json!({
                "code":proposal.kind,
                "span":proposal.text,
                "reason":"kind needs person-fixed parameters before it can be recorded",
            }));
            continue;
        }
        let start = proposal.receipt.offset;
        let end = start
            .checked_add(proposal.receipt.length)
            .context("proposal span offset overflowed")?;
        if text.get(start..end) != Some(proposal.text.as_str()) {
            bail!("proposal is not an exact span of the recorded person message");
        }
        let anchor = sha256_hex(format!("{event_id}\0{start}\0{end}").as_bytes());
        let params = CheckParams::TextAbsent(TextAbsentParams {
            test_allowlist: Vec::new(),
            json_field_allowlist: Vec::new(),
            conditional_path_allowlist: Vec::new(),
            span_allowlist: Vec::new(),
            terms: proposal.terms.clone(),
            synonyms: Vec::new(),
            surfaces: vec![
                Surface::Code,
                Surface::UiCopy,
                Surface::Docs,
                Surface::Tests,
                Surface::Ci,
                Surface::Other,
            ],
            allowlist: Vec::new(),
        });
        let id = storage::id_for_source(project_id, &anchor, params.kind());
        let exists: bool = ledger.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM decisions WHERE id=?1 AND project_id=?2)",
            (&id, project_id),
            |row| row.get(0),
        )?;
        if !exists {
            let start = u64::try_from(start).context("proposal offset is too large")?;
            let end = u64::try_from(end).context("proposal end is too large")?;
            let created = storage::create(
                ledger,
                NewDecision {
                    project_id: project_id.into(),
                    source_anchor: anchor.clone(),
                    span: proposal.text.clone(),
                    sources: vec![DecisionSource {
                        source_kind: StoredSourceKind::WorkPrompt,
                        event_id: Some(event_id.into()),
                        file_anchor: Some(anchor),
                        raw_file: raw_file.clone(),
                        raw_line,
                        span_start: start,
                        span_end: end,
                        span: proposal.text.clone(),
                        actor: Some(actor_id.into()),
                        recorded_at: recorded_at.clone(),
                    }],
                    author_actor: actor_id.into(),
                    recorded_at: recorded_at
                        .clone()
                        .unwrap_or_else(thruline_core::ingest::iso8601_utc_now),
                    params,
                },
            )?;
            if created != id {
                bail!("stored proposal identity did not match its source span");
            }
        }
        proposals.push(json!({
            "id":id,
            "kind":"text_absent",
            "code":proposal.kind,
            "span":proposal.text,
            "terms":proposal.terms,
            "source":{"event_id":event_id,"raw_file":Masked::mask(&raw_file).into_text(),"raw_line":raw_line,
                "span_start":start,"span_end":end},
            "lifecycle":"proposed",
        }));
    }
    let count = proposals.len();
    Ok(Outcome {
        exit_code: 0,
        body: json!({
            "schema_version":1,
            "project_id":project_id,
            "event_id":event_id,
            "proposals":proposals,
            "unparameterized":unparameterized,
            "notes":reading.notes,
        }),
        summary: format!("{count} checkable proposal(s) from the recorded person message."),
    })
}

struct PersonParamsRevision<'a> {
    decision_id: &'a str,
    selected_revision: u32,
    actor: &'a str,
    at: &'a str,
    terms: &'a [String],
    allowlist: &'a [String],
    surfaces: &'a [Surface],
}

fn append_person_params_revision(ledger: &Ledger, input: PersonParamsRevision<'_>) -> Result<u32> {
    let (kind, schema_version, raw_params, span, raw_sources): (
        String,
        u32,
        String,
        String,
        String,
    ) = ledger
        .conn()
        .query_row(
            "SELECT kind,params_schema_version,params_json,text,sources_json
                 FROM decision_revisions WHERE decision_id=?1 AND rev=?2",
            (input.decision_id, input.selected_revision),
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .context("selected decision revision was not found")?;
    let mut params = parse_params(&kind, schema_version, &raw_params)?;
    let CheckParams::TextAbsent(ref mut accepted) = params else {
        bail!("term, surface and allow overrides require a text_absent decision");
    };
    if !input.terms.is_empty() {
        accepted.terms = input.terms.to_vec();
    }
    if !input.allowlist.is_empty() {
        accepted.allowlist = input.allowlist.to_vec();
    }
    if !input.surfaces.is_empty() {
        accepted.surfaces = input.surfaces.to_vec();
    }
    let sources: Vec<DecisionSource> = serde_json::from_str(&raw_sources)?;
    Ok(storage::append_revision(
        ledger,
        RevisionInput {
            decision_id: input.decision_id.into(),
            span,
            sources,
            author_actor: input.actor.into(),
            recorded_at: input.at.into(),
            params,
        },
    )?)
}

fn check(
    ledger: &Ledger,
    project_id: &str,
    project_root: &std::path::Path,
    version_id: &str,
    decision_id: Option<&str>,
    at: &str,
    replay: bool,
) -> Result<Outcome> {
    let results = if let Some(decision_id) = decision_id {
        engine::check_decision_version(
            ledger,
            project_id,
            project_root,
            version_id,
            at,
            replay,
            decision_id,
        )?
    } else {
        engine::check_project_version(ledger, project_id, project_root, version_id, at, replay)?
    };
    let exit_code = if results
        .iter()
        .any(|result| result.state == VerdictState::Broken)
    {
        3
    } else if results.is_empty()
        || results
            .iter()
            .any(|result| result.state == VerdictState::Unverifiable)
    {
        4
    } else {
        0
    };
    let verdicts = results
        .iter()
        .map(|result| recorded_verdict_json(ledger, version_id, result))
        .collect::<Result<Vec<_>>>()?;
    let checked = results.len();
    let summary = check_summary(ledger, project_id, version_id, &results)?;
    Ok(Outcome {
        exit_code,
        body: json!({
            "schema_version":1,
            "project_id":project_id,
            "version_id":version_id,
            "verdicts":verdicts,
            "checked":checked,
            "replay":replay,
        }),
        summary,
    })
}

fn check_summary(
    ledger: &Ledger,
    project_id: &str,
    version_id: &str,
    results: &[RunResult],
) -> Result<String> {
    let spans = storage::accepted_for_project(ledger, project_id)?
        .into_iter()
        .map(|decision| (decision.id, decision.span))
        .collect::<BTreeMap<_, _>>();
    let version = bounded(&Masked::mask(version_id).into_text(), 80);
    let mut lines = Vec::new();
    for result in results.iter().take(32) {
        let span = spans
            .get(&result.decision_id)
            .map_or(result.decision_id.as_str(), String::as_str);
        let span = bounded(&Masked::mask(span).into_text(), 160);
        match result.state {
            VerdictState::Broken => {
                let first = result
                    .outcome
                    .findings
                    .first()
                    .map(|finding| format!("{}:{}", bounded(&finding.path, 160), finding.line))
                    .unwrap_or_else(|| "location unavailable".into());
                lines.push(format!(
                    "Decision broken on {version}: \"{span}\" — {} place(s): {first}.",
                    result.outcome.findings.len()
                ));
            }
            VerdictState::Holds => {
                lines.push(format!("Decision holds on {version}: \"{span}\"."));
            }
            VerdictState::Unverifiable => {
                let reason = result
                    .outcome
                    .reason
                    .as_deref()
                    .unwrap_or("evidence is unavailable");
                lines.push(format!(
                    "Can't check \"{span}\" on {version}: {}.",
                    bounded(&Masked::mask(reason).into_text(), 160)
                ));
            }
            VerdictState::NotApplicable => {
                lines.push(format!("Decision not applicable on {version}: \"{span}\"."));
            }
        }
        if !result.outcome.surfaces.is_empty() {
            lines.push("surface | state | findings".into());
            for surface in result.outcome.surfaces.iter().take(12) {
                lines.push(format!(
                    "{} | {} | {}",
                    surface.surface.as_str(),
                    engine::state_name(surface.state),
                    surface.finding_count
                ));
            }
        }
        if let Some(summary) =
            thruline_core::decisions::text_exclusions::summary(&result.outcome.not_looked_at)
        {
            lines.push(summary);
        }
    }
    if results.len() > 32 {
        lines.push(format!("… and {} more decisions.", results.len() - 32));
    }
    if lines.is_empty() {
        lines.push("No accepted decisions were available to check.".into());
    }
    Ok(lines.join("\n"))
}

fn bounded(text: &str, max_chars: usize) -> String {
    let masked = Masked::mask(text).into_text();
    let mut chars = masked.chars();
    let mut result = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        result.push('…');
    }
    result
}

fn verdict_json(version_id: &str, result: &RunResult) -> Value {
    json!({
        "decision_id":result.decision_id,
        "revision":result.revision,
        "version_id":version_id,
        "tree_hash":result.tree_hash,
        "state":engine::state_name(result.state),
        "reason":result.outcome.reason,
        "checkable_if":(result.state == VerdictState::Unverifiable).then_some(
            "Provide the named version and required recorded evidence, then check again"
        ),
        "findings":result.outcome.findings,
        "surfaces":result.outcome.surfaces,
        "visual":result.outcome.visual,
        "not_looked_at":result.outcome.not_looked_at,
        "finding_count":result.outcome.findings.len(),
        "from_cache":result.from_cache,
        "verdict_id":result.verdict_id,
    })
}

fn recorded_verdict_json(ledger: &Ledger, version: &str, result: &RunResult) -> Result<Value> {
    let mut value = verdict_json(version, result);
    value["record_projection"] = Value::Null;
    if let Some(id) = &result.verdict_id {
        let mut statement = ledger.conn().prepare(
            "SELECT basis_json,checkable_if FROM decision_verdicts WHERE id=?1 AND decision_id=?2 AND rev=?3",
        )?;
        let mut rows = statement.query((id, &result.decision_id, result.revision))?;
        let row: Option<(String, Option<String>)> = match rows.next()? {
            Some(row) => Some((row.get(0)?, row.get(1)?)),
            None => None,
        };
        if let Some((basis, checkable_if)) = row {
            let basis: Value = serde_json::from_str(&basis)?;
            value["record_projection"] = mask_json(&basis["record_projection"]);
            value["checkable_if"] =
                serde_json::to_value(checkable_if.map(|text| Masked::mask(&text).into_text()))?;
        }
    }
    Ok(value)
}

pub fn run(args: Args) -> Result<ExitCode> {
    let shape = match args.verb {
        Verb::List => &json_out::DECIDE_LIST,
        Verb::Propose { .. } => &json_out::DECIDE_PROPOSE,
        Verb::Accept { .. } => &json_out::DECIDE_ACCEPT,
        Verb::Retire { .. } => &json_out::DECIDE_RETIRE,
        Verb::Check { .. } => &json_out::DECIDE_CHECK,
    };
    let ledger = Ledger::open(&args.db)?;
    let outcome = execute(&ledger, &args)?;
    print_outcome(outcome, args.json, shape)
}

pub fn run_root_cli(db: PathBuf, cli: DecisionRootCliArgs) -> Result<ExitCode> {
    match cli.verb {
        DecisionRootCliVerb::Person(verb) => run_cli(
            db,
            DecideCliArgs {
                project: cli.project,
                verb,
            },
        ),
        DecisionRootCliVerb::Checks(verb) => run_decisions_cli(
            db,
            DecisionsCliArgs {
                project: cli.project,
                verb,
            },
        ),
    }
}

/// Refresh the repository projection only after the exact bridge act verifies.
/// This is separate from the OS prompt so isolated tests can exercise the
/// post-confirmation write without manufacturing a production presence proof.
pub(crate) fn finalize_confirmed_act(
    ledger: &Ledger,
    project_id: &str,
    project_root: &Path,
    action: PersonActKind,
    act_row_id: &str,
) -> Result<PathBuf> {
    anyhow::ensure!(
        bridge::verified(ledger, action, act_row_id)?,
        "person confirmation receipt does not verify"
    );
    let owns_act: bool = ledger.conn().query_row(
        "SELECT EXISTS(SELECT 1 FROM decision_person_acts
         WHERE action=?1 AND act_row_id=?2 AND project_id=?3)",
        [action.as_str(), act_row_id, project_id],
        |row| row.get(0),
    )?;
    anyhow::ensure!(owns_act, "confirmed act belongs to another project");
    let path = storage::write_derived_file(ledger, project_id, project_root)?;
    anyhow::ensure!(
        storage::verify_derived_file(ledger, project_id, project_root)?,
        "confirmed decision copy did not verify"
    );
    Ok(path)
}

/// Entry point for the singular `thruline decide` person-act commands.
pub fn run_cli(db: PathBuf, cli: DecideCliArgs) -> Result<ExitCode> {
    let ledger = Ledger::open(&db)?;
    let (project_id, project_root) = resolve_project(&ledger, &cli.project)?;
    if let DecideCliVerb::ReviseParams {
        decision_id,
        parent_revision,
        parent_params_sha256,
        parent_sources_sha256,
        params_file,
        params_sha256,
    } = &cli.verb
    {
        let bytes = read_parameter_document(params_file)?;
        let receipt = thruline_core::decisions::revision_proposal::append(
            &ledger,
            &project_root,
            thruline_core::decisions::revision_proposal::ProposalInput {
                project_id: &project_id,
                decision_id,
                parent_revision: *parent_revision,
                parent_params_sha256,
                parent_sources_sha256,
                params_bytes: &bytes,
                params_sha256,
                at: &thruline_core::ingest::iso8601_utc_now(),
            },
        )?;
        if cli.project.json {
            print_compact_decision_json(
                &json_out::DECIDE_REVISE_PARAMS,
                mask_json(&serde_json::to_value(&receipt)?),
            )?;
        } else {
            println!(
                "Proposed revision {} with parameters {}. Person acceptance is pending.",
                receipt.revision, receipt.params_sha256
            );
        }
        return Ok(ExitCode::SUCCESS);
    }
    if let DecideCliVerb::Rules { version } = &cli.verb {
        let report = thruline_core::decisions::rule_files::ingest(
            &ledger,
            &project_id,
            &project_root,
            version,
            &thruline_core::ingest::iso8601_utc_now(),
        )?;
        if cli.project.json {
            print_compact_decision_json(
                &json_out::DECIDE_RULES,
                mask_json(&serde_json::to_value(&report)?),
            )?;
        } else {
            println!(
                "{} rule proposal(s), {} span(s) needing parameters on {}. Person acceptance is pending.",
                report.proposals.len(), report.notes.len(), report.version_id
            );
        }
        return Ok(ExitCode::SUCCESS);
    }
    if matches!(&cli.verb, DecideCliVerb::RepairDerived) {
        let path = storage::write_derived_file(&ledger, &project_id, &project_root)?;
        anyhow::ensure!(
            storage::verify_derived_file(&ledger, &project_id, &project_root)?,
            "repaired decision copy did not verify"
        );
        if cli.project.json {
            print_compact_decision_json(
                &json_out::DECIDE_REPAIR_DERIVED,
                json!({"schema_version":1,"project_id":project_id,
                    "derived_file":path,"verified":true}),
            )?;
        } else {
            println!("Repaired derived decision copy: {}", path.display());
        }
        return Ok(ExitCode::SUCCESS);
    }
    if let DecideCliVerb::Confirm {
        action,
        act_row_id,
        predecessor_revision,
        screen,
    } = &cli.verb
    {
        let person_action = match action.as_str() {
            "accept" => PersonActKind::Accept,
            "retire" => PersonActKind::Retire,
            "waive" => PersonActKind::Waive,
            "supersede" => PersonActKind::Accept,
            _ => bail!("unknown person action"),
        };
        let row_project: String = match person_action {
            PersonActKind::Accept => ledger.conn().query_row(
                "SELECT d.project_id FROM decision_acceptances a JOIN decisions d ON d.id=a.decision_id WHERE a.id=?1",
                [act_row_id], |row| row.get(0),
            )?,
            PersonActKind::Retire => ledger.conn().query_row(
                "SELECT d.project_id FROM decision_retirements r JOIN decisions d ON d.id=r.decision_id WHERE r.id=?1",
                [act_row_id], |row| row.get(0),
            )?,
            PersonActKind::Waive => ledger.conn().query_row(
                "SELECT d.project_id FROM decision_waivers w JOIN decisions d ON d.id=w.decision_id WHERE w.id=?1",
                [act_row_id], |row| row.get(0),
            )?,
        };
        anyhow::ensure!(
            row_project == project_id,
            "person act belongs to another project"
        );
        drop(ledger);
        let mut writable = Ledger::open(&db)?;
        let receipt_id = if action == "supersede" {
            bridge::confirm_terminal_supersession(
                &mut writable,
                act_row_id,
                bridge::SupersessionInput {
                    predecessor_revision: predecessor_revision
                        .context("supersede requires --predecessor-revision")?,
                    screen: screen.clone().context("supersede requires --screen")?,
                },
            )?
        } else {
            anyhow::ensure!(
                predecessor_revision.is_none() && screen.is_none(),
                "predecessor revision and screen apply only to supersede"
            );
            bridge::confirm_terminal(&mut writable, person_action, act_row_id)?
        };
        let derived_file = finalize_confirmed_act(
            &writable,
            &project_id,
            &project_root,
            person_action,
            act_row_id,
        )
        .with_context(|| {
            format!(
                "person confirmation receipt {receipt_id} was recorded, but the derived decision copy could not be refreshed; run `{}`",
                terminal_repair_command(&project_id, &project_root)
            )
        })?;
        if cli.project.json {
            print_compact_decision_json(
                &json_out::DECIDE_CONFIRM,
                confirmation_json(&project_id, action, act_row_id, &receipt_id, &derived_file),
            )?;
        } else {
            println!("Person confirmation recorded: {receipt_id}");
        }
        return Ok(ExitCode::SUCCESS);
    }
    if let DecideCliVerb::Prepare {
        action,
        decision_id,
        revision,
        actor,
        because,
        severity,
        from_version,
        version_id,
        expires_version,
        expires_at,
        superseded_by,
    } = &cli.verb
    {
        require_project_decision(&ledger, &project_id, decision_id)?;
        anyhow::ensure!(
            !actor.trim().is_empty() && !because.trim().is_empty(),
            "prepare requires a display actor and a reason"
        );
        let now = thruline_core::ingest::iso8601_utc_now();
        let act_id = match action.as_str() {
            "accept" => {
                anyhow::ensure!(
                    version_id.is_none()
                        && expires_version.is_none()
                        && expires_at.is_none()
                        && superseded_by.is_none(),
                    "accept has irrelevant action options"
                );
                storage::accept(
                    &ledger,
                    AcceptanceInput {
                        decision_id: decision_id.clone(),
                        revision: revision.unwrap_or(latest_revision(&ledger, decision_id)?),
                        actor: actor.clone(),
                        at: now,
                        because: because.clone(),
                        severity: severity.clone().unwrap_or_else(|| "blocks".into()),
                        applies_from_version: match from_version {
                            Some(version) if !version.trim().is_empty() => version.clone(),
                            Some(_) => bail!("from_version must not be empty"),
                            None => git_head(&project_root)?,
                        },
                    },
                )?
            }
            "retire" => {
                anyhow::ensure!(
                    revision.is_none()
                        && severity.is_none()
                        && from_version.is_none()
                        && version_id.is_none()
                        && expires_version.is_none()
                        && expires_at.is_none(),
                    "retire has irrelevant action options"
                );
                if let Some(next_id) = superseded_by {
                    require_project_decision(&ledger, &project_id, next_id)?;
                }
                storage::retire(
                    &ledger,
                    RetirementInput {
                        decision_id: decision_id.clone(),
                        actor: actor.clone(),
                        at: now,
                        because: because.clone(),
                        superseded_by: superseded_by.clone(),
                    },
                )?
            }
            "waive" => {
                anyhow::ensure!(
                    revision.is_none()
                        && severity.is_none()
                        && from_version.is_none()
                        && superseded_by.is_none(),
                    "waive has irrelevant action options"
                );
                storage::waive(
                    &ledger,
                    WaiverInput {
                        decision_id: decision_id.clone(),
                        version_id: version_id.clone().context("waive requires --version-id")?,
                        actor: actor.clone(),
                        at: now,
                        because: because.clone(),
                        expires_version: expires_version.clone(),
                        expires_at: expires_at.clone(),
                    },
                )?
            }
            _ => bail!("unknown person action"),
        };
        if cli.project.json {
            print_compact_decision_json(
                &json_out::DECIDE_PREPARE,
                json!({"schema_version":1,"project_id":project_id,
                "decision_id":decision_id,"action":action,"act_row_id":act_id,
                "state":"pending_bridge"}),
            )?;
        } else {
            println!(
                "Pending {action} act {act_id}. Confirm in a person-owned terminal with `{}`.",
                terminal_confirm_command(&project_id, &project_root, action, &act_id)
            );
        }
        return Ok(ExitCode::SUCCESS);
    }
    let now = thruline_core::ingest::iso8601_utc_now();
    let verb = match cli.verb {
        DecideCliVerb::List => Verb::List,
        DecideCliVerb::Propose { event_id, .. } => Verb::Propose { event_id },
        DecideCliVerb::Accept {
            decision_id,
            event_id,
            revision,
            actor,
            because,
            severity,
            from_version,
            terms,
            allowlist,
            surfaces,
        } => Verb::Accept {
            decision_id,
            event_id,
            revision,
            actor,
            at: now,
            because,
            severity,
            applies_from_version: match from_version {
                Some(version) if !version.trim().is_empty() => version,
                Some(_) => bail!("from_version must not be empty"),
                None => git_head(&project_root)?,
            },
            terms,
            allowlist,
            surfaces: surfaces
                .iter()
                .map(|surface| match surface.as_str() {
                    "code" => Ok(Surface::Code),
                    "ui_copy" => Ok(Surface::UiCopy),
                    "docs" => Ok(Surface::Docs),
                    "tests" => Ok(Surface::Tests),
                    "ci" => Ok(Surface::Ci),
                    "other" => Ok(Surface::Other),
                    _ => bail!("unknown decision surface"),
                })
                .collect::<Result<Vec<_>>>()?,
        },
        DecideCliVerb::Retire {
            decision_id,
            event_id,
            actor,
            because,
            superseded_by,
        } => Verb::Retire {
            decision_id,
            event_id,
            actor,
            at: now,
            because,
            superseded_by,
        },
        DecideCliVerb::Confirm { .. } => unreachable!("handled above"),
        DecideCliVerb::RepairDerived | DecideCliVerb::Rules { .. } => unreachable!("handled above"),
        DecideCliVerb::Prepare { .. } | DecideCliVerb::ReviseParams { .. } => {
            unreachable!("handled above")
        }
    };
    let args = Args {
        db,
        project_id,
        project_root,
        verb,
        json: cli.project.json,
    };
    drop(ledger);
    run(args)
}

const INLINE_BUDGET: Duration = Duration::from_secs(20);
const INLINE_OUTPUT_LIMIT: u64 = 8 << 20;

struct SupervisedInline {
    output: Vec<u8>,
    timed_out: bool,
    elapsed: Duration,
}

fn signal_inline_group(child: &Child) {
    #[cfg(unix)]
    if let Ok(group) = i32::try_from(child.id()) {
        let _ = thruline_core::bounded_child::signal_group(
            group,
            thruline_core::bounded_child::kill_group_now,
        );
    }
}

fn stop_inline_child(child: &mut Child) {
    signal_inline_group(child);
    let _ = child.kill();
    // The leader is reaped after its entire group has been killed. The
    // capture is a file, so grandchildren cannot hold a pipe open here.
    let _ = child.wait();
}

#[cfg(unix)]
fn inline_child_exited_unreaped(child: &Child) -> Result<bool> {
    use rustix::process::{waitid, Pid, WaitId, WaitIdOptions};

    let raw = i32::try_from(child.id()).context("inline worker PID is too large")?;
    let pid = Pid::from_raw(raw).context("inline worker PID is invalid")?;
    loop {
        match waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG,
        ) {
            Ok(status) => return Ok(status.is_some()),
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(error.into()),
        }
    }
}

fn supervise_inline(command: &mut Command, budget: Duration) -> Result<SupervisedInline> {
    let mut capture = tempfile::tempfile().context("creating bounded inline capture")?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(capture.try_clone()?))
        .stderr(Stdio::null());
    #[cfg(unix)]
    command.process_group(0);
    let started = Instant::now();
    let mut child: Child = command
        .spawn()
        .context("starting read-only inline worker")?;
    let (status, timed_out) = loop {
        let capture_len = match capture.metadata() {
            Ok(metadata) => metadata.len(),
            Err(error) => {
                stop_inline_child(&mut child);
                return Err(error).context("measuring inline worker capture");
            }
        };
        if capture_len > INLINE_OUTPUT_LIMIT {
            stop_inline_child(&mut child);
            bail!("inline worker output bound exceeded");
        }
        #[cfg(unix)]
        match inline_child_exited_unreaped(&child) {
            Ok(true) => {
                // waitid(NOWAIT) leaves the leader unreaped, reserving its
                // process group ID until every descendant has been stopped.
                signal_inline_group(&child);
                break (Some(child.wait()?), false);
            }
            Ok(false) => {}
            Err(error) => {
                stop_inline_child(&mut child);
                return Err(error);
            }
        }
        #[cfg(not(unix))]
        if let Some(status) = child.try_wait()? {
            break (Some(status), false);
        }
        if started.elapsed() >= budget {
            break (None, true);
        }
        thread::sleep(Duration::from_millis(5));
    };
    if timed_out {
        stop_inline_child(&mut child);
    } else {
        anyhow::ensure!(
            status.is_some_and(|state| state.success()),
            "inline worker failed"
        );
    }
    anyhow::ensure!(
        capture.metadata()?.len() <= INLINE_OUTPUT_LIMIT,
        "inline worker output bound exceeded"
    );
    capture.seek(SeekFrom::Start(0))?;
    let mut output = Vec::new();
    (&mut capture)
        .take(INLINE_OUTPUT_LIMIT + 1)
        .read_to_end(&mut output)?;
    anyhow::ensure!(
        output.len() as u64 <= INLINE_OUTPUT_LIMIT,
        "inline worker output bound exceeded"
    );
    Ok(SupervisedInline {
        output,
        timed_out,
        elapsed: started.elapsed(),
    })
}

/// Sweep all ledger members of one local HQ project within one child deadline.
/// The worker publishes cache-only snapshots first; a timeout retains those
/// exact receipts and marks every unfinished procedure as budget-limited.
pub(crate) fn inline_sweep_projects(
    db: &Path,
    projects: &[String],
    repo: &Path,
    version: &str,
    diagnostic: bool,
) -> Result<Value> {
    anyhow::ensure!(!projects.is_empty(), "inline needs at least one project");
    anyhow::ensure!(
        projects.iter().all(|id| !id.is_empty())
            && projects.iter().collect::<BTreeSet<_>>().len() == projects.len(),
        "inline project members must be distinct nonempty ids"
    );
    anyhow::ensure!(!version.trim().is_empty(), "inline needs an exact version");
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--db")
        .arg(db)
        .arg("decisions")
        .arg(format!("--project={}", projects[0]))
        .arg("--root")
        .arg(repo)
        .args(["--json", "inline", "--version", version, "--worker"]);
    if diagnostic {
        command.arg("--diagnose-proposals");
    }
    for project in projects.iter().skip(1) {
        command.arg(format!("--project-member={project}"));
    }
    let supervised = supervise_inline(&mut command, INLINE_BUDGET)?;
    decode_inline_output(supervised, projects, repo, version, diagnostic)
}

fn decode_inline_output(
    supervised: SupervisedInline,
    projects: &[String],
    repo: &Path,
    version: &str,
    diagnostic: bool,
) -> Result<Value> {
    let mut cached = BTreeMap::<String, inline::InlineSweep>::new();
    let mut complete: Option<Vec<inline::InlineSweep>> = None;
    let mut announced_version: Option<String> = None;
    for line in supervised.output.split(|byte| *byte == b'\n') {
        let Ok(event) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        match event["event"].as_str() {
            Some("version") => {
                if let Some(exact) = event["version_id"].as_str() {
                    if inline_version_matches(version, exact) {
                        announced_version = Some(exact.into());
                    }
                }
            }
            Some("cache") => {
                let project: inline::InlineSweep =
                    serde_json::from_value(event["project"].clone())?;
                if projects.contains(&project.project_id)
                    && inline_version_matches(version, &project.version_id)
                {
                    cached.insert(project.project_id.clone(), project);
                }
            }
            Some("complete") => {
                let projects_value: Vec<inline::InlineSweep> =
                    serde_json::from_value(event["projects"].clone())?;
                complete = Some(projects_value);
            }
            _ => {}
        }
    }
    let elapsed_ms = u64::try_from(supervised.elapsed.as_millis()).unwrap_or(u64::MAX);
    if !supervised.timed_out {
        let completed = complete.context("inline worker omitted its complete result")?;
        let exact_version = &completed
            .first()
            .context("inline worker returned no project")?
            .version_id;
        anyhow::ensure!(
            inline_version_matches(version, exact_version)
                && announced_version
                    .as_deref()
                    .is_none_or(|known| known == exact_version)
                && completed.len() == projects.len()
                && completed
                    .iter()
                    .zip(projects)
                    .all(|(row, id)| &row.project_id == id && &row.version_id == exact_version),
            "inline worker returned different project members or version"
        );
        return Ok(json!({
            "schema_version":1,"state":"complete","version_id":exact_version,
            "diagnostic":diagnostic,"budget_ms":20_000,"elapsed_ms":elapsed_ms,
            "projects":completed,
        }));
    }
    let exact_version = announced_version
        .or_else(|| {
            cached
                .values()
                .next()
                .map(|project| project.version_id.clone())
        })
        .unwrap_or_else(|| version.into());
    let mut retained = projects
        .iter()
        .map(|project| {
            cached
                .remove(project)
                .unwrap_or_else(|| inline::InlineSweep {
                    schema_version: 1,
                    project_id: project.clone(),
                    version_id: exact_version.clone(),
                    tree_hash: None,
                    diagnostic,
                    results: Vec::new(),
                })
        })
        .collect::<Vec<_>>();
    for project in &mut retained {
        for row in &mut project.results {
            if row.reason.as_deref() == Some("inline check pending within the 20-second budget") {
                row.reason = Some("20-second inline budget exhausted before this check".into());
            }
        }
    }
    let full_commands = projects
        .iter()
        .map(|project| inline::full_command_all(project, repo, &exact_version))
        .collect::<Vec<_>>();
    Ok(json!({
        "schema_version":1,"state":"unverifiable","reason":"20-second inline budget exhausted",
        "version_id":exact_version,"diagnostic":diagnostic,"budget_ms":20_000,
        "elapsed_ms":elapsed_ms,"projects":retained,"full_commands":full_commands,
    }))
}

fn inline_version_matches(requested: &str, exact: &str) -> bool {
    if requested == exact {
        return true;
    }
    matches!(exact.len(), 40 | 64)
        && exact.bytes().all(|byte| byte.is_ascii_hexdigit())
        && (requested == "HEAD"
            || ((7..=64).contains(&requested.len())
                && requested.bytes().all(|byte| byte.is_ascii_hexdigit())
                && exact
                    .to_ascii_lowercase()
                    .starts_with(&requested.to_ascii_lowercase())))
}

fn inline_sweep(
    db: &Path,
    project: &str,
    repo: &Path,
    version: &str,
    diagnostic: bool,
) -> Result<Value> {
    let combined = inline_sweep_projects(db, &[project.to_owned()], repo, version, diagnostic)?;
    let mut single = combined["projects"][0].clone();
    if combined["state"] == "unverifiable" {
        single["state"] = Value::String("unverifiable".into());
        single["reason"] = combined["reason"].clone();
        single["full_command"] = combined["full_commands"][0].clone();
    }
    single["budget_ms"] = combined["budget_ms"].clone();
    single["elapsed_ms"] = combined["elapsed_ms"].clone();
    Ok(single)
}

/// Entry point for `thruline decisions check`.
pub fn run_decisions_cli(db: PathBuf, cli: DecisionsCliArgs) -> Result<ExitCode> {
    if let DecisionsCliVerb::Inline {
        version,
        diagnose_proposals,
        worker,
        project_member,
    } = &cli.verb
    {
        let project_id = cli
            .project
            .project
            .as_deref()
            .context("inline requires --project")?;
        let project_root = cli
            .project
            .root
            .as_deref()
            .context("inline requires --root")?;
        anyhow::ensure!(
            !version.trim().is_empty(),
            "inline requires an exact --version"
        );
        let members = std::iter::once(project_id.to_owned())
            .chain(project_member.iter().cloned())
            .collect::<Vec<_>>();
        if *worker {
            let exact_version = inline::resolve_exact_commit(project_root, version)
                .unwrap_or_else(|| version.to_owned());
            println!("{}", json!({"event":"version","version_id":exact_version}));
            std::io::stdout().flush()?;
            for member in &members {
                let snapshot = inline::cached_read_only(
                    &db,
                    project_root,
                    member,
                    &exact_version,
                    *diagnose_proposals,
                )?;
                println!("{}", json!({"event":"cache","project":snapshot}));
                std::io::stdout().flush()?;
            }
            let projects = members
                .iter()
                .map(|member| {
                    inline::evaluate_read_only(
                        &db,
                        project_root,
                        member,
                        &exact_version,
                        *diagnose_proposals,
                    )
                })
                .collect::<thruline_core::error::Result<Vec<_>>>()?;
            println!("{}", json!({"event":"complete","projects":projects}));
            std::io::stdout().flush()?;
            return Ok(ExitCode::SUCCESS);
        }
        let result = if project_member.is_empty() {
            inline_sweep(&db, project_id, project_root, version, *diagnose_proposals)?
        } else {
            inline_sweep_projects(&db, &members, project_root, version, *diagnose_proposals)?
        };
        if cli.project.json {
            print_compact_decision_json(&json_out::DECIDE_INLINE, result)?;
        } else {
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        return Ok(ExitCode::SUCCESS);
    }
    let ledger = Ledger::open(&db)?;
    let (project_id, project_root) = resolve_project(&ledger, &cli.project)?;
    let (verb, receipt_out) = match cli.verb {
        DecisionsCliVerb::Defaults => {
            return print_outcome(
                Outcome {
                    exit_code: 0,
                    body: defaults_document(&project_id),
                    summary: "18 proposed advisory defaults; none is accepted or checked.".into(),
                },
                cli.project.json,
                &json_out::DECIDE_DEFAULTS,
            );
        }
        DecisionsCliVerb::Check {
            version,
            id,
            replay,
            receipt_out,
            ..
        } => (
            Verb::Check {
                version_id: match version {
                    Some(version) if !version.trim().is_empty() => version,
                    Some(_) => bail!("version must not be empty"),
                    None => git_head(&project_root)?,
                },
                decision_id: id,
                at: thruline_core::ingest::iso8601_utc_now(),
                replay,
            },
            receipt_out,
        ),
        DecisionsCliVerb::Gate {
            base,
            receipt,
            policy,
        } => {
            return run_decision_gate(
                &ledger,
                &project_id,
                &project_root,
                base.as_deref(),
                receipt.as_deref(),
                policy.as_deref(),
                cli.project.json,
            );
        }
        DecisionsCliVerb::Inline { .. } => unreachable!("handled before opening the ledger"),
    };
    let args = Args {
        db,
        project_id,
        project_root,
        verb,
        json: cli.project.json,
    };
    let outcome = execute(&ledger, &args)?;
    if let Some(path) = receipt_out {
        let receipt = receipt_for_check(&ledger, &args.project_id, &args.project_root, &outcome)?;
        write_receipt(&path, &receipt)?;
    }
    print_outcome(outcome, args.json, &json_out::DECIDE_CHECK)
}

/// The catalog is the same read-only projection for CLI and local HTTP.
pub(crate) fn defaults_document(project_id: &str) -> Value {
    let defaults = kinds_k::proposed_defaults()
        .into_iter()
        .map(|proposal| {
            let coverage = match proposal.coverage {
                KCoverage::Unimplemented => "unimplemented",
                KCoverage::Partial => "partial",
                KCoverage::Complete => "complete",
            };
            json!({
                "code": proposal.code,
                "rule": proposal.rule,
                "producing_checks": proposal.producing_checks,
                "coverage": coverage,
                "coverage_note": proposal.coverage_note,
                "state": "proposed_advisory",
                "accepted_params": null,
                "verdict": null,
            })
        })
        .collect::<Vec<_>>();
    json!({"schema_version": 1, "project_id": project_id, "defaults": defaults})
}

fn print_outcome(outcome: Outcome, json: bool, shape: &json_out::Shape) -> Result<ExitCode> {
    if json {
        json_out::print(shape, decision_json_body(shape, outcome.body)?)?;
    } else {
        println!("{}", outcome.summary);
    }
    Ok(ExitCode::from(outcome.exit_code))
}

fn decision_json_body(shape: &json_out::Shape, body: Value) -> Result<Value> {
    let Value::Object(mut body) = body else {
        bail!("decision JSON body must be an object");
    };
    anyhow::ensure!(
        body.remove("schema_version") == Some(Value::from(shape.version)),
        "decision JSON body schema version differs from its registered shape"
    );
    Ok(Value::Object(body))
}

fn print_compact_decision_json(shape: &json_out::Shape, body: Value) -> Result<()> {
    json_out::print_compact(shape, decision_json_body(shape, body)?)
}

fn confirmation_json(
    project_id: &str,
    action: &str,
    act_row_id: &str,
    receipt_id: &str,
    derived_file: &Path,
) -> Value {
    json!({"schema_version":1,"receipt_id":receipt_id,"project_id":project_id,
        "action":action,"act_row_id":act_row_id,"derived_file":derived_file})
}

fn resolve_project(ledger: &Ledger, options: &ProjectCliOptions) -> Result<(String, PathBuf)> {
    let roots = ledger.project_roots()?;
    if let Some(id) = &options.project {
        let (_, recorded_root) = roots
            .into_iter()
            .find(|(candidate, _)| candidate == id)
            .context("project id was not found in the local ledger")?;
        return Ok((
            id.clone(),
            options
                .root
                .clone()
                .unwrap_or_else(|| PathBuf::from(recorded_root)),
        ));
    }
    let search_root = options.root.clone().unwrap_or(std::env::current_dir()?);
    let search_root = search_root
        .canonicalize()
        .with_context(|| format!("reading {}", search_root.display()))?;
    roots
        .into_iter()
        .filter_map(|(id, root)| {
            let recorded = PathBuf::from(root).canonicalize().ok()?;
            search_root.starts_with(&recorded).then_some((id, recorded))
        })
        .max_by_key(|(_, root)| root.components().count())
        .context("no project for this directory; pass --project and --root for a scratch copy")
}

fn git_head(root: &Path) -> Result<String> {
    let output = thruline_core::git_isolation::isolate_config(&mut Command::new("git"))
        .args(["rev-parse", "--verify", "--end-of-options", "HEAD^{commit}"])
        .current_dir(root)
        .output()
        .with_context(|| format!("reading Git version in {}", root.display()))?;
    if !output.status.success() {
        bail!("project HEAD is not a readable commit");
    }
    let oid = std::str::from_utf8(&output.stdout)?.trim();
    if oid.len() < 40 || !oid.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("project HEAD did not resolve to a commit id");
    }
    Ok(oid.into())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DerivedGateCopy {
    schema_version: u32,
    derived_copy: bool,
    project_id: String,
    member_project_ids: Vec<String>,
    content_sha256: String,
    decisions: Vec<Value>,
}

fn derived_for_gate(
    ledger: &Ledger,
    project_id: &str,
    repo: &Path,
) -> Result<(DerivedGateCopy, Vec<GateAccepted>)> {
    let path = repo.join(storage::DERIVED_FILE);
    let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    anyhow::ensure!(bytes.len() <= 8 << 20, "derived decision copy is too large");
    let copy: DerivedGateCopy =
        serde_json::from_slice(&bytes).context("derived decision copy is invalid")?;
    anyhow::ensure!(
        copy.schema_version == storage::DERIVED_SCHEMA_VERSION,
        "unsupported derived decision schema"
    );
    anyhow::ensure!(copy.derived_copy, "decision file is not a derived copy");
    anyhow::ensure!(
        copy.member_project_ids
            .iter()
            .any(|member| member == project_id),
        "project is absent from the derived decision copy"
    );
    anyhow::ensure!(
        storage::verify_derived_file_for_projects(
            ledger,
            &copy.project_id,
            &copy.member_project_ids,
            repo,
        )?,
        "derived decision copy differs from its ledger source"
    );
    anyhow::ensure!(
        std::fs::read(&path)? == bytes,
        "derived decision copy changed during gate evaluation"
    );
    let now = thruline_core::ingest::iso8601_utc_now();
    let head = git_object(repo, "HEAD^{commit}")?;
    let mut accepted = Vec::new();
    for member in &copy.member_project_ids {
        for decision in storage::accepted_for_project(ledger, member)? {
            accepted.push(GateAccepted {
                id: decision.id.clone(),
                revision: decision.revision,
                severity: decision.severity,
                waived: active_waiver(ledger, &decision.id, &head, &now)?,
            });
        }
    }
    anyhow::ensure!(
        accepted.len() == copy.decisions.len(),
        "derived decision copy has an incomplete decision set"
    );
    Ok((copy, accepted))
}

fn active_waiver(ledger: &Ledger, id: &str, version: &str, now: &str) -> Result<bool> {
    let mut statement = ledger.conn().prepare(
        "SELECT id,waived_at,expires_version,expires_at FROM decision_waivers
         WHERE decision_id=?1 AND version_id=?2 ORDER BY rowid DESC",
    )?;
    let rows = statement.query_map((id, version), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
        ))
    })?;
    for row in rows {
        let (act_id, waived_at, expires_version, expires_at) = row?;
        if waived_at.as_str() > now
            || expires_version.as_deref() == Some(version)
            || expires_at.as_deref().is_some_and(|expiry| expiry <= now)
            || !bridge::verified(ledger, PersonActKind::Waive, &act_id)?
        {
            continue;
        }
        return Ok(true);
    }
    Ok(false)
}

fn receipt_for_check(
    ledger: &Ledger,
    project_id: &str,
    repo: &Path,
    outcome: &Outcome,
) -> Result<Receipt> {
    let (derived, accepted) = derived_for_gate(ledger, project_id, repo)?;
    let version = outcome.body["version_id"]
        .as_str()
        .context("check has no exact version")?;
    anyhow::ensure!(
        version == git_head(repo)?,
        "receipt must be for the current exact Git HEAD"
    );
    let tree_hash = git_object(repo, "HEAD^{tree}")?;
    let results = outcome.body["verdicts"]
        .as_array()
        .context("check has no verdict array")?;
    let mut verdicts = Vec::new();
    for result in results {
        anyhow::ensure!(
            result["tree_hash"] == tree_hash,
            "check has no exact tree evidence"
        );
        verdicts.push(ReceiptVerdict {
            decision_id: result["decision_id"]
                .as_str()
                .context("decision id is missing")?
                .into(),
            revision: u32::try_from(result["revision"].as_u64().context("revision is missing")?)?,
            state: serde_json::from_value(result["state"].clone())?,
            verdict_id: result["verdict_id"]
                .as_str()
                .context("stored verdict id is missing")?
                .into(),
        });
    }
    anyhow::ensure!(
        verdicts.len() == accepted.len(),
        "check did not cover every derived decision"
    );
    let receipt = Receipt {
        schema_version: 1,
        project_id: derived.project_id,
        version_id: version.into(),
        tree_hash,
        derived_content_sha256: derived.content_sha256,
        verdicts,
    };
    anyhow::ensure!(
        verify_receipt_rows(ledger, &receipt)?,
        "check receipt does not match the decision ledger"
    );
    Ok(receipt)
}

fn write_receipt(path: &Path, receipt: &Receipt) -> Result<()> {
    let parent = path.parent().context("receipt output has no parent")?;
    std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    if path
        .symlink_metadata()
        .is_ok_and(|meta| meta.file_type().is_symlink())
    {
        bail!("decision receipt output is a symlink");
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    use std::io::Write;
    serde_json::to_writer_pretty(&mut temporary, receipt)?;
    temporary.write_all(b"\n")?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn read_parameter_document(path: &Path) -> Result<Vec<u8>> {
    #[cfg(unix)]
    let file: std::fs::File = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?
    .into();
    #[cfg(not(unix))]
    let file = {
        anyhow::ensure!(
            !path.symlink_metadata()?.file_type().is_symlink(),
            "parameter document must not be a symlink"
        );
        std::fs::File::open(path)?
    };
    anyhow::ensure!(
        file.metadata()?.is_file(),
        "parameter document must be a regular file"
    );
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(
        !bytes.is_empty() && bytes.len() <= 1024 * 1024,
        "parameter document must contain at most 1 MiB"
    );
    Ok(bytes)
}

fn read_receipt(path: &Path) -> Result<Option<Receipt>> {
    if path
        .symlink_metadata()
        .is_ok_and(|meta| meta.file_type().is_symlink())
    {
        bail!("decision receipt is a symlink");
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("reading {}", path.display())),
    };
    anyhow::ensure!(bytes.len() <= 8 << 20, "decision receipt is too large");
    Ok(Some(
        serde_json::from_slice(&bytes).context("decision receipt is invalid")?,
    ))
}

fn verify_receipt_rows(ledger: &Ledger, receipt: &Receipt) -> Result<bool> {
    for result in &receipt.verdicts {
        let mut statement = ledger.conn().prepare(
            "SELECT id,verdict,tree_hash FROM decision_verdicts
             WHERE decision_id=?1 AND rev=?2 AND version_id=?3 ORDER BY rowid DESC LIMIT 1",
        )?;
        let mut rows =
            statement.query((&result.decision_id, result.revision, &receipt.version_id))?;
        let Some(row) = rows.next()? else {
            return Ok(false);
        };
        let (id, state, tree_hash): (String, String, String) =
            (row.get(0)?, row.get(1)?, row.get(2)?);
        let expected: DecisionState = serde_json::from_value(Value::String(state))?;
        if id != result.verdict_id || expected != result.state || tree_hash != receipt.tree_hash {
            return Ok(false);
        }
    }
    Ok(true)
}

fn git_object(repo: &Path, object: &str) -> Result<String> {
    let output = Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(["rev-parse", "--verify", object])
        .current_dir(repo)
        .output()
        .context("resolving Git object")?;
    anyhow::ensure!(output.status.success(), "Git object could not be resolved");
    let oid = std::str::from_utf8(&output.stdout)?.trim();
    anyhow::ensure!(
        oid.len() == 40 && oid.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Git object did not resolve to a full SHA-1 OID"
    );
    Ok(oid.into())
}

fn run_decision_gate(
    ledger: &Ledger,
    project_id: &str,
    repo: &Path,
    base: Option<&str>,
    receipt_path: Option<&Path>,
    policy_path: Option<&Path>,
    json_output: bool,
) -> Result<ExitCode> {
    if !bridge::gates_active(ledger, project_id)? {
        return print_outcome(
            Outcome {
                exit_code: 0,
                body: json!({"schema_version":1,"conclusion":"success",
                    "reason":"person confirmation bridge is pending; decision gate is inactive"}),
                summary: "Person confirmation bridge is pending; decision gate is inactive.".into(),
            },
            json_output,
            &json_out::DECIDE_GATE,
        );
    }
    let policy_rel = policy_path.unwrap_or_else(|| Path::new(".thruline/policy.yml"));
    anyhow::ensure!(
        !policy_rel.is_absolute()
            && policy_rel
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_))),
        "decision gate policy path must stay inside the repository"
    );
    let base = decision_base_ref(base)?;
    let Some(policy_text) = decision_policy_text(repo, policy_rel, base.as_deref())? else {
        return print_outcome(
            Outcome {
                exit_code: 0,
                body: json!({"schema_version":1,"conclusion":"success","reason":"no policy file; nothing is gated"}),
                summary: "No decision policy file; nothing is gated.".into(),
            },
            json_output,
            &json_out::DECIDE_GATE,
        );
    };
    let policy = Policy::parse(&policy_text).context("decision gate policy is invalid")?;
    let Some(settings) = policy.decisions else {
        return print_outcome(
            Outcome {
                exit_code: 0,
                body: json!({"schema_version":1,"conclusion":"success","reason":"no decisions policy block"}),
                summary: "No decisions policy block; nothing is gated.".into(),
            },
            json_output,
            &json_out::DECIDE_GATE,
        );
    };
    if settings.require_hold == thruline_policy::decisions::RequireHold::None {
        return print_outcome(
            Outcome {
                exit_code: 0,
                body: json!({"schema_version":1,"conclusion":"success","reason":"decision gate is disabled by policy"}),
                summary: "Decision gate is disabled by policy.".into(),
            },
            json_output,
            &json_out::DECIDE_GATE,
        );
    }
    let (derived, accepted) = derived_for_gate(ledger, project_id, repo)?;
    if derived
        .member_project_ids
        .iter()
        .any(|member| !bridge::gates_active(ledger, member).unwrap_or(false))
    {
        return print_outcome(
            Outcome {
                exit_code: 0,
                body: json!({"schema_version":1,"conclusion":"success",
                    "reason":"a member project still needs person confirmation; decision gate is inactive"}),
                summary:
                    "A member project still needs person confirmation; decision gate is inactive."
                        .into(),
            },
            json_output,
            &json_out::DECIDE_GATE,
        );
    }
    let head = git_object(repo, "HEAD^{commit}")?;
    let tree = git_object(repo, "HEAD^{tree}")?;
    let path = receipt_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| repo.join(".thruline/decisions-receipt.json"));
    let mut subject = GateSubject {
        project_id: derived.project_id,
        version_id: head,
        tree_hash: tree,
        derived_content_sha256: derived.content_sha256,
        accepted,
        receipt: read_receipt(&path)?,
    };
    let mut judged = settings.evaluate(&subject);
    if judged.action == GateAction::Recompute {
        let fresh = recompute_group_receipt(ledger, repo, &derived.member_project_ids, &subject)?;
        write_receipt(&path, &fresh)?;
        subject.receipt = Some(fresh);
        judged = settings.evaluate(&subject);
    }
    if let Some(receipt) = &subject.receipt {
        if receipt.schema_version == 1
            && receipt.project_id == subject.project_id
            && receipt.version_id == subject.version_id
            && receipt.tree_hash == subject.tree_hash
            && receipt.derived_content_sha256 == subject.derived_content_sha256
            && !verify_receipt_rows(ledger, receipt)?
        {
            return print_outcome(
                Outcome {
                    exit_code: 3,
                    body: json!({"schema_version":1,"conclusion":"failure","reasons":[{"rule":"decisions.receipt_integrity","detail":"decision receipt differs from the local ledger"}]}),
                    summary: "Decision receipt differs from the local ledger.".into(),
                },
                json_output,
                &json_out::DECIDE_GATE,
            );
        }
    }
    anyhow::ensure!(
        git_object(repo, "HEAD^{commit}")? == subject.version_id
            && git_object(repo, "HEAD^{tree}")? == subject.tree_hash,
        "PR head changed during decision gate evaluation"
    );
    let conclusion = match judged.conclusion {
        Conclusion::Success => "success",
        Conclusion::Failure => "failure",
        Conclusion::Neutral => "neutral",
    };
    let exit_code = if judged.conclusion == Conclusion::Failure {
        3
    } else {
        0
    };
    let summary = if judged.reasons.is_empty() {
        format!("Decision policy {conclusion} on {}.", subject.version_id)
    } else {
        judged
            .reasons
            .iter()
            .map(|reason| reason.detail.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    };
    print_outcome(
        Outcome {
            exit_code,
            body: json!({
                "schema_version":1,
                "project_id":subject.project_id,
                "version_id":subject.version_id,
                "tree_hash":subject.tree_hash,
                "conclusion":conclusion,
                "reasons":judged.reasons.iter().map(|reason| json!({
                    "claim":reason.claim,
                    "rule":reason.rule,
                    "detail":reason.detail,
                    "conclusion":match reason.conclusion {
                        Conclusion::Success => "success",
                        Conclusion::Failure => "failure",
                        Conclusion::Neutral => "neutral",
                    },
                })).collect::<Vec<_>>(),
            }),
            summary,
        },
        json_output,
        &json_out::DECIDE_GATE,
    )
}

fn recompute_group_receipt(
    ledger: &Ledger,
    repo: &Path,
    members: &[String],
    subject: &GateSubject,
) -> Result<Receipt> {
    let now = thruline_core::ingest::iso8601_utc_now();
    let mut verdicts = Vec::new();
    for member in members {
        for result in
            engine::check_project_version(ledger, member, repo, &subject.version_id, &now, false)?
        {
            anyhow::ensure!(
                result.tree_hash.as_deref() == Some(&subject.tree_hash),
                "recomputed decision tree differs from PR head"
            );
            verdicts.push(ReceiptVerdict {
                decision_id: result.decision_id,
                revision: result.revision,
                state: serde_json::from_value(Value::String(
                    engine::state_name(result.state).into(),
                ))?,
                verdict_id: result
                    .verdict_id
                    .context("recomputed verdict has no stored receipt")?,
            });
        }
    }
    Ok(Receipt {
        schema_version: 1,
        project_id: subject.project_id.clone(),
        version_id: subject.version_id.clone(),
        tree_hash: subject.tree_hash.clone(),
        derived_content_sha256: subject.derived_content_sha256.clone(),
        verdicts,
    })
}

fn decision_base_ref(explicit: Option<&str>) -> Result<Option<String>> {
    let base = explicit.map(str::to_string).or_else(|| {
        std::env::var("GITHUB_BASE_REF")
            .ok()
            .filter(|value| !value.trim().is_empty())
    });
    if let Some(base) = &base {
        anyhow::ensure!(
            !base.is_empty()
                && !base.starts_with('-')
                && base
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"/._-".contains(&byte))
                && !base.contains(".."),
            "base ref is invalid"
        );
    } else if matches!(
        std::env::var("GITHUB_EVENT_NAME").as_deref(),
        Ok("pull_request" | "pull_request_target")
    ) {
        bail!("pull request base ref is unavailable; refusing to read policy from PR head");
    }
    Ok(base)
}

fn decision_policy_text(
    repo: &Path,
    relative: &Path,
    base: Option<&str>,
) -> Result<Option<String>> {
    let Some(base) = base else {
        return match std::fs::read_to_string(repo.join(relative)) {
            Ok(text) => Ok(Some(text)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).context("reading local decision policy"),
        };
    };
    let oid = git_object(repo, &format!("{base}^{{commit}}"))?;
    let relative = relative.to_str().context("policy path is not UTF-8")?;
    let listing = git_output(
        repo,
        &["ls-tree", "-r", "--name-only", &oid, "--", relative],
    )?;
    if !listing.lines().any(|line| line == relative) {
        return Ok(None);
    }
    let text = git_output(repo, &["show", &format!("{oid}:{relative}")])?;
    Ok(Some(text))
}

fn git_output(repo: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(args)
        .current_dir(repo)
        .output()
        .context("reading Git policy object")?;
    anyhow::ensure!(
        output.status.success(),
        "Git policy object could not be read"
    );
    Ok(String::from_utf8(output.stdout)?)
}

/// Resolve a Decisions id after the existing `why` namespaces decline it.
/// Returns `None` for unrelated ids so older `why` behavior remains intact.
pub fn try_why(ledger: &Ledger, item: &str, json: bool) -> Result<Option<ExitCode>> {
    let (is_decision, is_verdict): (bool, bool) = ledger.conn().query_row(
        "SELECT EXISTS(SELECT 1 FROM decisions WHERE id=?1),
                EXISTS(SELECT 1 FROM decision_verdicts WHERE id=?1)",
        [item],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if !is_decision && !is_verdict {
        return Ok(None);
    }
    let (version, tree) = if is_verdict {
        ledger.conn().query_row(
            "SELECT version_id,tree_hash FROM decision_verdicts WHERE id=?1",
            [item],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?
    } else {
        let mut statement = ledger.conn().prepare(
            "SELECT version_id,tree_hash FROM decision_verdicts
             WHERE decision_id=?1 ORDER BY rowid DESC LIMIT 1",
        )?;
        let mut rows = statement.query([item])?;
        if let Some(row) = rows.next()? {
            (row.get::<_, String>(0)?, row.get::<_, String>(1)?)
        } else {
            let mut accepted = ledger.conn().prepare(
                "SELECT a.applies_from_version,p.root_path
                 FROM decision_acceptances a
                 JOIN decisions d ON d.id=a.decision_id
                 JOIN projects p ON p.id=d.project_id
                 WHERE a.decision_id=?1 ORDER BY a.rowid DESC LIMIT 1",
            )?;
            let mut rows = accepted.query([item])?;
            let Some(row) = rows.next()? else {
                eprintln!("decision {item} is proposed; a person has not accepted it");
                return Ok(Some(ExitCode::from(4)));
            };
            let version: String = row.get(0)?;
            let root: String = row.get(1)?;
            let Some(tree) = git_tree(Path::new(&root), &version) else {
                eprintln!("decision {item} has no readable exact-version tree yet");
                return Ok(Some(ExitCode::from(4)));
            };
            (version, tree)
        }
    };
    let Some(proof) = why::proof_chain(ledger, item, &version, &tree)? else {
        eprintln!("decision {item} has no accepted proof chain");
        return Ok(Some(ExitCode::from(4)));
    };
    if json {
        println!("{}", serde_json::to_string(&proof)?);
    } else {
        for step in &proof.chain {
            let glyph = match step.number {
                1 => "①",
                2 => "②",
                3 => "③",
                4 => "④",
                5 => "⑤",
                _ => "⑥",
            };
            let label = match step.label {
                ChainLabel::Decided => "DECIDED",
                ChainLabel::Check => "CHECK",
                ChainLabel::RanOn => "RAN ON",
                ChainLabel::Found => "FOUND",
                ChainLabel::Since => "SINCE",
                ChainLabel::So => "SO",
            };
            println!("{glyph} {label} — {}", step.sentence);
        }
    }
    Ok(Some(ExitCode::SUCCESS))
}

fn git_tree(root: &Path, version: &str) -> Option<String> {
    let output = thruline_core::git_isolation::isolate_config(&mut Command::new("git"))
        .args(["rev-parse", "--verify", "--end-of-options"])
        .arg(format!("{version}^{{tree}}"))
        .current_dir(root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let oid = std::str::from_utf8(&output.stdout).ok()?.trim();
    (oid.len() >= 40 && oid.chars().all(|ch| ch.is_ascii_hexdigit())).then(|| oid.into())
}

#[cfg(test)]
mod visual_json_tests {
    use super::*;
    use thruline_core::decisions::checks::{CheckOutcome, VerdictState};
    use thruline_core::decisions::checks_visual::VisualScreenResult;

    #[test]
    fn check_json_keeps_typed_visual_rows() -> anyhow::Result<()> {
        let visual: VisualScreenResult = serde_json::from_value(json!({
            "screen":"Home","route":"#/design/home","actual_route":"#/home",
            "actual_scenario":"fixed-record","state":"broken","difference_pct":78.4,
            "different_pixels":784,"checked_pixels":1000,
            "reference_width":100,"reference_height":100,
            "capture_width":100,"capture_height":100,
            "compared_width":100,"compared_height":100,
            "diff_image_path":"/tmp/diff.png","diff_sha256":"a".repeat(64),
            "reference_sha256":"b".repeat(64),"version_id":"c".repeat(40),
            "commit_oid":"c".repeat(40),"tree_hash":"d".repeat(40),
            "renderer_sha256":"e".repeat(64),"reason":"pixel difference"
        }))?;
        let result = RunResult {
            decision_id: "decision-c1".into(),
            revision: 1,
            state: VerdictState::Broken,
            tree_hash: Some("d".repeat(40)),
            from_cache: false,
            verdict_id: Some("verdict-c1".into()),
            outcome: CheckOutcome {
                state: VerdictState::Broken,
                tree_hash: Some("d".repeat(40)),
                reason: Some("pixel difference".into()),
                checked_places: 1,
                findings: Vec::new(),
                surfaces: Vec::new(),
                not_looked_at: Vec::new(),
                visual: vec![visual],
            },
        };
        let projected = verdict_json(&"c".repeat(40), &result);
        assert_eq!(projected["visual"][0]["screen"], "Home");
        assert_eq!(projected["visual"][0]["difference_pct"], 78.4);
        assert_eq!(projected["visual"][0]["diff_sha256"], "a".repeat(64));
        assert_eq!(projected["visual"][0]["actual_route"], "#/home");
        Ok(())
    }
}

#[cfg(test)]
mod json_contract_tests {
    use super::*;

    #[test]
    fn confirmation_output_keeps_its_closed_payload_without_executing_confirmation() -> Result<()> {
        let path = Path::new("/tmp/decision-json-fixture/decisions.yml");
        let original = confirmation_json("member", "supersede", "act", "receipt", path);
        let document = json_out::envelope(
            &json_out::DECIDE_CONFIRM,
            decision_json_body(&json_out::DECIDE_CONFIRM, original.clone())?,
        )?;
        assert_eq!(document["schema_version"], 1);
        assert_eq!(document["shape"], "thruline.decide.confirm");
        let mut unwrapped = document.as_object().expect("object").clone();
        unwrapped.remove("shape");
        assert_eq!(Value::Object(unwrapped), original);
        assert_eq!(
            document
                .as_object()
                .expect("object")
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            [
                "act_row_id",
                "action",
                "derived_file",
                "project_id",
                "receipt_id",
                "schema_version",
                "shape"
            ],
        );
        Ok(())
    }

    #[test]
    fn decisions_reject_missing_or_conflicting_public_schema_and_preserve_nested_versions(
    ) -> Result<()> {
        let shape = &json_out::DECIDE_INLINE;
        for body in [
            json!([]),
            json!({}),
            json!({"schema_version": "1"}),
            json!({"schema_version": 2}),
        ] {
            assert!(decision_json_body(shape, body).is_err());
        }
        let conflicting = decision_json_body(shape, json!({"schema_version":1,"shape":"foreign"}))?;
        assert!(json_out::envelope(shape, conflicting).is_err());
        let original = json!({"schema_version":1,"project_id":"member","results":[{"schema_version":7,"state":"unverifiable"}]});
        let document = json_out::envelope(shape, decision_json_body(shape, original.clone())?)?;
        assert_eq!(document["results"], original["results"]);
        assert_eq!(document["schema_version"], 1);
        assert_eq!(document["shape"], "thruline.decide.inline");
        Ok(())
    }
}

#[cfg(test)]
mod inline_budget_tests {
    use super::*;

    #[test]
    fn budget_keeps_cached_a1_when_later_uncached_work_stalls() -> anyhow::Result<()> {
        let cached = inline::InlineSweep {
            schema_version: 1,
            project_id: "member".into(),
            version_id: "a".repeat(40),
            tree_hash: Some("b".repeat(40)),
            diagnostic: false,
            results: vec![
                inline::InlineRow {
                    decision_id: "fast-a1".into(),
                    revision: 1,
                    kind: "text_absent".into(),
                    state: "broken".into(),
                    reason: Some("stored exact-tree witness".into()),
                    checked_places: 1,
                    from_cache: true,
                    advisory: false,
                    verdict_id: Some("stored-a1".into()),
                    full_command: None,
                },
                inline::InlineRow {
                    decision_id: "slow-a1".into(),
                    revision: 1,
                    kind: "text_absent".into(),
                    state: "unverifiable".into(),
                    reason: Some("inline check pending within the 20-second budget".into()),
                    checked_places: 0,
                    from_cache: false,
                    advisory: false,
                    verdict_id: None,
                    full_command: Some("thruline decisions check --id slow-a1".into()),
                },
            ],
        };
        let event = json!({"event":"cache","project":cached}).to_string();
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            "printf '%s\\n' \"$1\"; sleep 2",
            "inline-test",
            &event,
        ]);
        let supervised = supervise_inline(&mut command, Duration::from_millis(60))?;
        assert!(supervised.timed_out);
        assert!(supervised.elapsed < Duration::from_secs(1));
        let version = "a".repeat(40);
        let body = decode_inline_output(
            supervised,
            &["member".into()],
            Path::new("/tmp/inline-fixture"),
            &version,
            false,
        )?;
        assert_eq!(body["state"], "unverifiable");
        assert_eq!(body["projects"][0]["results"][0]["decision_id"], "fast-a1");
        assert_eq!(body["projects"][0]["results"][0]["verdict_id"], "stored-a1");
        assert_eq!(body["projects"][0]["results"][1]["decision_id"], "slow-a1");
        assert_eq!(body["projects"][0]["results"][1]["state"], "unverifiable");
        assert!(body["projects"][0]["results"][1]["reason"]
            .as_str()
            .unwrap()
            .contains("budget exhausted"));
        assert!(body["projects"][0]["results"][1]["full_command"]
            .as_str()
            .unwrap()
            .contains("--id slow-a1"));
        assert!(body["full_commands"][0].as_str().unwrap().contains("--all"));
        Ok(())
    }

    #[test]
    fn person_followup_commands_bind_a_leading_dash_project() {
        let root = Path::new("/tmp/decisions scratch");
        let confirm = terminal_confirm_command("-member", root, "accept", "act-1");
        assert!(confirm.contains("--project='-member'"), "{confirm}");
        assert!(
            confirm.contains("--root='/tmp/decisions scratch'"),
            "{confirm}"
        );
        let repair = terminal_repair_command("-member", root);
        assert!(repair.contains("--project='-member'"), "{repair}");
        assert!(
            repair.contains("--root='/tmp/decisions scratch'"),
            "{repair}"
        );
    }

    #[test]
    fn unbounded_worker_output_is_killed_before_the_time_budget() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "yes x"]);
        let started = Instant::now();
        let error = supervise_inline(&mut command, Duration::from_secs(2))
            .err()
            .expect("large worker output must be refused");
        assert!(error.to_string().contains("output bound"), "{error:#}");
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[cfg(unix)]
    #[test]
    fn successful_worker_does_not_leave_a_background_writer() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let marker = dir.path().join("orphan-wrote");
        let mut command = Command::new("/bin/sh");
        command
            .args([
                "-c",
                "(sleep 0.1; printf orphan > \"$1\") & exit 0",
                "inline-test",
            ])
            .arg(&marker);
        let result = supervise_inline(&mut command, Duration::from_secs(1))?;
        assert!(!result.timed_out);
        thread::sleep(Duration::from_millis(200));
        assert!(!marker.exists(), "worker descendant outlived the sweep");
        Ok(())
    }
}
