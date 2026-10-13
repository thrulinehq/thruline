//! `thruline promises` — BUILD-SPEC §14 and §11's cross-session query.
//!
//! > *Queries needed by CLI: open promises across sessions for a repo.*
//!
//! # Fulfilment is cross-session, and it is a heuristic that says so
//!
//! Within one session every promise is trailing: the only prose ThruLine reads
//! is the final message, so nothing follows a promise inside the session that
//! made it. Fulfilment therefore needs *later sessions*, which is what this
//! command looks at.
//!
//! A promise is marked **fulfilled** when a later session in the same
//! repository changed a path whose name carries the promise's distinctive
//! words. That is a heuristic, and the direction it errs in is chosen: a
//! promise nothing matches stays **open**, so the failure mode is a promise
//! reported as outstanding when it was quietly done — visible and correctable —
//! rather than one reported as done when it was dropped, which is the failure
//! the feature exists to prevent.
//!
//! Nothing here reaches a verdict. §10 item 4 is a narrative surface.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use thruline_ledger::Ledger;

pub struct Args {
    pub open_only: bool,
    pub all: bool,
    pub repo: Option<PathBuf>,
    pub exec_db: Option<PathBuf>,
    pub json: bool,
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
    let repo_root = args
        .repo
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let key = repo_root.to_string_lossy().to_string();
    let repo = ledger.repo(&key, Some(&key))?;

    // Revisit every open promise against what later sessions actually changed.
    let promises = ledger.promises_for_repo(&repo)?;
    for p in &promises {
        if p.status != "open" {
            continue;
        }
        let words = distinctive_words(&p.text);
        if words.is_empty() {
            continue;
        }
        for (later_session, event, path) in
            ledger.paths_changed_after_session(&repo, &p.session_id)?
        {
            let lower = path.to_ascii_lowercase();
            if words.iter().any(|w| lower.contains(w.as_str())) {
                ledger.close_promise(
                    &p.id,
                    "fulfilled",
                    &event,
                    &format!("session {later_session} changed {path}"),
                )?;
                break;
            }
        }
    }

    let promises = ledger.promises_for_repo(&repo)?;
    let show_all = args.all && !args.open_only;
    let shown: Vec<_> = promises
        .iter()
        .filter(|p| show_all || p.status == "open")
        .collect();

    if args.json {
        crate::json_out::print(
            &crate::json_out::PROMISES,
            serde_json::json!({
                "repo": key,
                "promises": shown.iter().map(|p| {
                    serde_json::json!({
                        "id": p.id, "session": p.session_id, "text": p.text,
                        "kind": p.kind, "status": p.status,
                    })
                }).collect::<Vec<_>>(),
            }),
        )?;
    } else if shown.is_empty() {
        println!(
            "no {}promises recorded for {key}",
            if show_all { "" } else { "open " }
        );
    } else {
        for p in &shown {
            println!("  [{:<9}] {:<10} {}", p.status, p.kind, p.text);
            println!("               opened in session {}", p.session_id);
        }
        println!();
        println!(
            "{} promise(s). A promise nothing later touched stays OPEN — that is a \
             report of what was said and not done, not proof that it was abandoned.",
            shown.len()
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// Words from a promise that could name a file.
///
/// Stop-words and short tokens removed, because "the", "add" and "test" match
/// nearly every path and would fulfil every promise at once.
fn distinctive_words(text: &str) -> BTreeSet<String> {
    const STOP: &[&str] = &[
        "the", "and", "for", "with", "will", "this", "that", "later", "follow", "up", "add",
        "adding", "added", "test", "tests", "in", "a", "an", "to", "of", "i", "we", "should",
        "would", "need", "next", "step", "todo", "future", "separate", "pull", "request", "case",
        "cases", "handling", "support", "make", "sure", "also",
    ];
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.len() >= 5 && !STOP.contains(w))
        .map(str::to_string)
        .collect()
}
