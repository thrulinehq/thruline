//! `thruline tui` — BUILD-SPEC §10's flight recorder.
//!
//! This module is the *bridge*, and it is thin on purpose. It builds
//! [`thruline_tui::Flight`] from the ledger and hands it over; the TUI crate
//! decides nothing about verdicts and cannot, because it depends on no
//! ThruLine crate at all.
//!
//! **The claims and verdicts come from `claims_cmd::judge`** — the same call
//! `thruline claims`, `thruline explain` and `thruline receipt` all make. A TUI
//! that recomputed them could show a reader a verdict the receipt disagrees
//! with, which is the product contradicting its own evidence.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use serde_json::Value;
use thruline_ledger::Ledger;
use thruline_tui::{ClaimRow, Flight, PromiseRow, Row, StopSummary};

pub struct Args {
    pub session: Option<String>,
    pub last: bool,
    pub exec_db: Option<PathBuf>,
    pub repo: Option<PathBuf>,
}

pub fn run(args: Args) -> Result<ExitCode> {
    let db = match args.exec_db {
        Some(p) => p,
        None => Ledger::default_path().context("locating the execution store")?,
    };
    if !db.is_file() {
        anyhow::bail!("no execution store at {}", db.display());
    }
    let ledger = Ledger::open(&db).with_context(|| format!("opening {}", db.display()))?;
    let _ = args.last;
    let session = match args.session {
        Some(s) => s,
        None => crate::claims_cmd::last_session(&ledger)?.context("no session in the store")?,
    };
    let repo = args
        .repo
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let events = ledger.session_events(&session)?;
    let view = crate::claims_cmd::session_view(&session, &events, &ledger, &repo)?;
    let judged = crate::claims_cmd::judge(&session, &events, &view, &repo)?;

    // The timeline comes from `crates/explain`, so the TUI and `thruline
    // explain --full` cannot describe the same event differently.
    let timeline = crate::explain_cmd::timeline_of(&events);
    let risk_at: std::collections::BTreeMap<u64, String> = timeline
        .risks
        .iter()
        .filter_map(|r| r.risk.clone().map(|risk| (r.seq, risk)))
        .collect();
    let rows: Vec<Row> = timeline
        .entries
        .iter()
        .map(|entry| Row {
            seq: entry.seq,
            kind: entry.kind.clone(),
            text: entry.text.clone(),
            detail: events
                .iter()
                .find(|e| e.seq() == entry.seq)
                .map(detail_of)
                .unwrap_or_default(),
            risk: risk_at.get(&entry.seq).cloned(),
        })
        .collect();
    let stop_seq = events
        .iter()
        .rev()
        .find(|e| e.kind().as_str().as_str() == "stop")
        .map(thruline_core::spec::event::Event::seq);
    let stop_reason = events
        .iter()
        .rev()
        .find(|e| e.kind().as_str().as_str() == "stop")
        .and_then(|e| e.payload().get("reason").and_then(Value::as_str))
        .unwrap_or("unknown")
        .to_string();

    let flight = Flight::new(
        session.clone(),
        rows,
        judged
            .iter()
            .map(|(claim, record)| ClaimRow {
                kind: claim.kind.as_str().to_string(),
                text: claim.text.clone(),
                // Carried, never recomputed.
                verdict: record.verdict.as_str().to_string(),
                reasons: record.reasons.clone(),
                findings: record
                    .findings
                    .iter()
                    .map(|f| format!("{} — {}", f.detector, f.detail))
                    .collect(),
            })
            .collect(),
        ledger
            .repo(&repo.to_string_lossy(), Some(&repo.to_string_lossy()))
            .and_then(|id| ledger.promises_for_repo(&id))
            .unwrap_or_default()
            .into_iter()
            .filter(|p| p.session_id == session)
            .map(|p| PromiseRow {
                text: p.text,
                status: p.status,
            })
            .collect(),
        StopSummary {
            reason: stop_reason,
            detail: if stop_seq.is_some() {
                "the session recorded a stop".to_string()
            } else {
                "no stop event was recorded".to_string()
            },
            seq: stop_seq,
        },
    );
    thruline_tui::screen::run(flight).context("running the flight recorder")?;
    Ok(ExitCode::SUCCESS)
}

/// Everything the detail pane shows about one event.
///
/// Payload fields, bounded, in a stable order — no interpretation. A detail
/// pane that summarised would be a second renderer disagreeing with the first.
fn detail_of(e: &thruline_core::spec::event::Event) -> Vec<(String, String)> {
    let mut out = vec![
        ("id".to_string(), e.id().as_str().to_string()),
        ("seq".to_string(), e.seq().to_string()),
        ("ts".to_string(), e.ts().to_string()),
        (
            "source".to_string(),
            serde_json::to_value(e.source())
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default(),
        ),
    ];
    if let Some(map) = e.payload().as_object() {
        for (k, v) in map {
            let text = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            out.push((k.clone(), truncate(&text, 400)));
        }
    }
    out
}

/// A bounded rendering of an already-redacted payload field.
///
/// **Masks, then cuts — locally, in this function.** The value arrived
/// redacted: §4.6 runs before anything reaches the spool, so the mask here is
/// a fixpoint and finds nothing. It runs anyway because
/// `redact::TRUNCATION_SITES` verifies the mask-then-cut invariant *by
/// proximity within one function*, and a guarantee that holds two frames up is
/// invisible to it. A209-C3 measured four sites where cutting first sliced a
/// credential below its detector threshold; making the invariant locally true
/// is cheaper than arguing for an exemption each time.
///
/// The census caught this one before a human did — a truncation site in a new
/// crate's bridge, on the day it was written.
fn truncate(s: &str, n: usize) -> String {
    let remasked = thruline_core::redact::mask_recorded(s).0;
    thruline_core::redact::truncate_respecting_markers(&remasked, n)
}
