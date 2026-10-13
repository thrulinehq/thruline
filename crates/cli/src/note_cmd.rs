//! C30 R-LEDGER — `thruline change note`: the terminal door to a note on a
//! Change.
//!
//! It calls [`thruline_core::change_note::add`], the same function
//! `POST /api/changes/<id>/notes` calls, so a note written from a terminal and
//! one written from the screen leave rows of the same shape and are refused for
//! the same reasons. A note is words: it never moves a verdict, a count,
//! readiness or which approval stands, and it is free.
//!
//! The Approve note has no door here: there is no approve verb, and the note
//! that goes with an approval is appended by the approval itself.
//!
//! Exit 1 on a refusal, because "nothing was saved" is not success.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use thruline_core::change_note::{self, MAX_NOTE_CHARS};
use thruline_core::ledger::Ledger;

/// `thruline change note`'s arguments, in their own struct so the generated
/// argument function stays small (`cli_surface_budget.rs`).
#[derive(Debug, clap::Args)]
pub struct NoteArgs {
    /// The Change this note is about.
    pub change: String,
    /// What you want to keep on the Change, in your own words (at most 2,000
    /// characters).
    #[arg(long)]
    pub body: String,
    /// Who is writing it. Defaults to the account this is running as.
    #[arg(long)]
    pub actor: Option<String>,
}

pub fn run(db: PathBuf, args: NoteArgs) -> Result<ExitCode> {
    let ledger = Ledger::open(&db).with_context(|| format!("opening {}", db.display()))?;
    let change = args.change.trim().to_string();
    let actor = match args.actor.as_deref().map(str::trim) {
        Some(given) if !given.is_empty() => given.to_string(),
        _ => crate::check::account_name(),
    };
    let at = thruline_core::ingest::iso8601_utc_now();
    match change_note::add(&ledger, &change, &actor, &args.body, &at).context("keeping the note")? {
        Ok(note) => {
            println!(
                "Kept a note on {change}, written by {} at {}. It is words only: it changes no \
                 result and no approval. (At most {MAX_NOTE_CHARS} characters.)",
                note.actor, note.recorded_at
            );
            Ok(ExitCode::SUCCESS)
        }
        Err(refusal) => {
            println!("{}", refusal.plain(&change));
            Ok(ExitCode::FAILURE)
        }
    }
}
