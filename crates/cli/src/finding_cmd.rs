//! T-4-2 — `thruline change finding`: the terminal door to where one thing a
//! review found stands.
//!
//! It calls [`thruline_core::finding_standing::record`], the same function
//! `POST /api/changes/<id>/findings/<finding>/standing` calls, so a standing
//! changed from a terminal and one changed from the screen leave rows of the
//! same shape and are refused for the same reasons. The sentences it prints
//! are the copy file's, mirrored in `finding_standing.rs` and held equal by
//! `crates/core/tests/finding_standing_copy.rs`.
//!
//! A "resolved" mark is `--to awaiting_verification --via resolved_mark`. A
//! synthetic import, for testing only, is `--via synthetic_resolved_import`
//! and is stored and shown as exactly that; this product imports no real
//! provider comment threads. Neither can reach `fixed_with_evidence`.
//!
//! Exit 1 on a refusal, because "nothing was saved" is not success.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use thruline_core::finding_standing::{
    self, part_words, refusal_line, EvidenceKind, EvidenceRef, Outcome, Standing, StandingAct, Via,
};
use thruline_core::ledger::Ledger;

/// `thruline change finding`'s arguments, in their own struct so the
/// generated argument function stays small (`cli_surface_budget.rs`).
#[derive(Debug, clap::Args)]
pub struct FindingArgs {
    /// The Change the finding belongs to.
    #[arg(long)]
    pub change: String,
    /// The finding, as `<review>:<n>` (shown under "The record behind this").
    #[arg(long)]
    pub finding: String,
    /// Where it stands now: open, under_repair, awaiting_verification,
    /// fixed_with_evidence, disputed, informational or waived.
    #[arg(long)]
    pub to: String,
    /// How it arrived: person (default), resolved_mark, or
    /// synthetic_resolved_import (a labelled test import). A mark reaches
    /// awaiting_verification and nothing else.
    #[arg(long, default_value = "person")]
    pub via: String,
    /// Who decided. Defaults to the account this is running as.
    #[arg(long)]
    pub actor: Option<String>,
    /// Why you can decide this. Required to dispute, waive, or keep for
    /// information only.
    #[arg(long, default_value = "")]
    pub authority: String,
    /// The reason, in your own words. Required to dispute, waive, or keep for
    /// information only.
    #[arg(long, default_value = "")]
    pub reason: String,
    /// A later review of the current version that no longer raises it.
    #[arg(long, conflicts_with = "evidence_check")]
    pub evidence_review: Option<String>,
    /// A completed, passing check run of this Change that covers its file.
    #[arg(long)]
    pub evidence_check: Option<String>,
    #[arg(long)]
    pub json: bool,
}

pub fn run(db: PathBuf, args: FindingArgs) -> Result<ExitCode> {
    let ledger = Ledger::open(&db).with_context(|| format!("opening {}", db.display()))?;
    let to = Standing::parse(args.to.trim()).context("--to")?;
    let via = Via::parse(args.via.trim()).context("--via")?;
    // An `--actor` nobody can see is no `--actor`, exactly like an empty one
    // (ADV-review-input-1), and so is one that holds nothing a person can
    // read (ADV-review-input-4): the account this runs as is the name on the
    // row.
    let actor = args
        .actor
        .as_deref()
        .map(finding_standing::visible_text)
        .filter(|a| !a.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| match std::env::var("USER") {
            Ok(user) if !user.trim().is_empty() => user,
            _ => String::new(),
        });
    let evidence = match (&args.evidence_review, &args.evidence_check) {
        (Some(id), _) => Some(EvidenceRef {
            kind: EvidenceKind::Review,
            id: id.clone(),
        }),
        (None, Some(id)) => Some(EvidenceRef {
            kind: EvidenceKind::CheckRun,
            id: id.clone(),
        }),
        (None, None) => None,
    };
    let act = StandingAct {
        finding: args.finding.clone(),
        to,
        via,
        actor,
        authority: args.authority,
        reason: args.reason,
        evidence,
        seen: None,
        at: thruline_core::ingest::iso8601_utc_now(),
    };
    let outcome = finding_standing::record(&ledger, &args.change, &act)
        .context("recording where this finding stands")?;
    match &outcome {
        Outcome::Recorded { from, to, .. } => {
            if args.json {
                crate::json_out::print(
                    &crate::json_out::CHANGE_FINDING,
                    serde_json::json!({ "acted": outcome }),
                )?;
            } else {
                println!("{} → {}", from.label(), to.label());
                // FIXED-WORDING (R-2026-09-28-9): a fix says only what the
                // record shows, and lists beside it every other thing the
                // same re-check raised — the screen's words, from the same
                // read.
                if *to == Standing::FixedWithEvidence {
                    let view = finding_standing::read(&ledger, &args.change)
                        .context("reading where this finding stands")?;
                    if let Some(f) = view.findings.iter().find(|f| f.key == args.finding) {
                        for line in finding_standing::fixed_lines(f) {
                            println!("{line}");
                        }
                    }
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Outcome::Refused(refusal) => {
            if args.json {
                crate::json_out::print(
                    &crate::json_out::CHANGE_FINDING,
                    serde_json::json!({
                        "acted": outcome,
                        "plain": refusal_line(refusal.code),
                    }),
                )?;
            } else {
                println!("{}", refusal_line(refusal.code));
                // The parts left empty, or the parts over the limit: a
                // refusal names one list or the other, never both.
                for named in [&refusal.missing, &refusal.too_long] {
                    if !named.is_empty() {
                        let parts: Vec<&str> = named.iter().map(|p| part_words(p)).collect();
                        println!("  {}", parts.join(", "));
                    }
                }
            }
            Ok(ExitCode::FAILURE)
        }
    }
}
