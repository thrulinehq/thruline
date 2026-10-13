//! `thruline sessions` — the list a person picks a session id out of.
//!
//! # The defect this exists against
//!
//! `explain` and `claims` open exactly one session, and given no `--session`
//! they open whichever id sorts last (`claims_cmd::last_session`). On a real
//! corpus that is the lexicographic maximum uuid: on the operator's store,
//! 44,107 sessions in, it is a two-event session with nothing in it, so
//! `thruline ui` opened on an empty screen while the thirteen sessions that
//! carry a `stop` event — the only ones anything can be extracted from — sat
//! unreachable because nothing enumerated them.
//!
//! This command does not change what "the last session" means for anybody. It
//! adds a way to **choose**, which is a different thing and is why the default
//! is left alone.
//!
//! # What a row is allowed to say
//!
//! Only what the record carries. `stop_events` is the gate on everything
//! downstream and `final_messages` is what the extractor reads; neither is a
//! promise that a claim comes out, because `extract` may legitimately return
//! none. A field named `has_claim` would be a claim about a claim, and it would
//! sometimes be false.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use thruline_ledger::{Ledger, SessionSummary};

pub struct Args {
    pub exec_db: Option<PathBuf>,
    pub limit: u32,
    pub per_project: bool,
    pub json: bool,
}

pub fn run(args: Args) -> Result<ExitCode> {
    let db = match args.exec_db {
        Some(p) => p,
        None => Ledger::default_path().context("locating the execution store")?,
    };
    // No `is_file` refusal, unlike `claims` and `explain`. An empty store is a
    // truthful answer to "which sessions are there" — zero of them — where a
    // session id that was asked for and is not there is not.
    let ledger = Ledger::open(&db).with_context(|| format!("opening {}", db.display()))?;
    let total = ledger.session_total().context("counting sessions")?;
    let rows = if args.per_project {
        ledger.session_summaries_by_project(args.limit)
    } else {
        ledger.session_summaries(args.limit)
    }
    .context("ranking sessions")?;

    if args.json {
        crate::json_out::print(
            &crate::json_out::SESSIONS,
            serde_json::json!({
                "store": db.display().to_string(),
                "total": total,
                "returned": rows.len(),
                "limit": args.limit,
                "limit_scope": if args.per_project { "project" } else { "store" },
                "sessions": rows.iter().map(row_json).collect::<Vec<_>>(),
            }),
        )?;
    } else {
        print_text(
            &db.display().to_string(),
            total,
            args.limit,
            args.per_project,
            &rows,
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// One row of the `--json` shape. Hand-written rather than derived, so that a
/// field added to `SessionSummary` for some other reader does not silently
/// enter a shape four teams pin.
fn row_json(s: &SessionSummary) -> serde_json::Value {
    serde_json::json!({
        "id": s.id,
        "repo_key": s.repo_key,
        "repo_root": s.repo_root,
        "harness": s.harness,
        "event_count": s.event_count,
        "user_prompts": s.user_prompts,
        "stop_events": s.stop_events,
        "file_change_events": s.file_change_events,
        "final_messages": s.final_messages,
        "first_event_at": s.first_event_at,
        "last_event_at": s.last_event_at,
        "head_hash": s.head_hash,
        "capture_degraded": s.capture_degraded,
    })
}

/// Every count carries a noun and the truncation is stated, never implied.
fn print_text(store: &str, total: u64, limit: u32, per_project: bool, rows: &[SessionSummary]) {
    if total == 0 {
        println!("no sessions in {store}");
        println!("Nothing has been captured here yet. Run `thruline doctor` to see whether the hooks are installed.");
        return;
    }
    let shown = rows.len();
    if per_project {
        println!(
            "{total} sessions in {store} · showing {shown}, up to {limit} per recorded project"
        );
    } else if (shown as u64) < total {
        println!("{total} sessions in {store} · showing the {shown} most recently active (of a {limit}-row limit)");
    } else {
        println!("{total} sessions in {store} · showing all {shown}");
    }
    println!();
    for s in rows {
        let label = s.repo_root.as_deref().unwrap_or(&s.repo_key);
        let when = s.last_event_at.as_deref().unwrap_or("no recorded time");
        println!(
            "  {}  {label}\n    {} · {} · {} · {} · last recorded event {when}",
            s.id,
            counted(s.event_count, "event", "events"),
            counted(s.stop_events, "stop", "stops"),
            counted(s.file_change_events, "file change", "file changes"),
            counted(s.user_prompts, "prompt", "prompts"),
        );
    }
    println!();
    println!("Run `thruline explain --session <id>` to open one.");
}

/// A count with its noun, agreeing in number. "1 stops" reads as a rendering
/// bug, and a reader who distrusts the rendering distrusts the number.
fn counted(n: u64, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}
