//! T-4-4-ATTEMPT-BUDGET (R-2026-09-28-43; Q14 C8) — `thruline change
//! attempts`: the terminal door to a Change's attempt budget.
//!
//! With `--budget <n> --actor <who>` it records how many repair attempts a
//! named person allows (one appended row,
//! [`thruline_core::attempt_budget::set`]). Without `--budget` it asks for the
//! next attempt: it prints every attempt, the budget and one sentence, from
//! the one function [`thruline_core::attempt_budget::sentences`], and exits 1
//! once every allowed attempt is made.
//!
//! It runs nothing. The attempt itself is somebody's change to the code;
//! ThruLine counts it from the runs `thruline check --for-change` records, and
//! that wrapper asks [`thruline_core::attempt_budget::admits`] before it spawns
//! anything, so a spent budget stops the run there too.
//!
//! "Unresolved" is said only when the readiness answer the Change's screen is
//! served ([`crate::uiserve::served_readiness`]) is not Ready. It is asked for
//! only once the budget is spent, and never re-derived here.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use thruline_core::attempt_budget::{self, Refusal};
use thruline_core::ledger::Ledger;

/// `thruline change attempts`'s arguments, in their own struct so the
/// generated argument function stays small (`cli_surface_budget.rs`).
#[derive(Debug, clap::Args)]
#[command(after_help = attempt_budget::OWN_FILES_LIMIT)]
pub struct AttemptsArgs {
    /// The Change this is about.
    pub change: String,
    /// Allow this many repair attempts (1–99), counted from the checks run
    /// after now. Without it, lists every attempt and says whether another may
    /// be made.
    #[arg(long, value_name = "N")]
    pub budget: Option<u32>,
    /// Who is setting the budget. Required with --budget.
    #[arg(long)]
    pub actor: Option<String>,
}

fn print_all(lines: &[String]) {
    for line in lines {
        println!("{line}");
    }
}

pub fn run(db: PathBuf, args: AttemptsArgs) -> Result<ExitCode> {
    let ledger = Ledger::open(&db).with_context(|| format!("opening {}", db.display()))?;
    let change = args.change.trim().to_string();

    // A Change the store does not hold is said as such, whether setting or
    // asking — never answered as "no budget is set" (Rule 12).
    if thruline_core::change::read_change(&ledger, &change)
        .context("reading the Change")?
        .is_none()
    {
        println!("{}", Refusal::NoSuchChange.plain(&change));
        return Ok(ExitCode::FAILURE);
    }

    if let Some(n) = args.budget {
        let actor = args.actor.as_deref().unwrap_or_default();
        let at = thruline_core::ingest::iso8601_utc_now();
        return match attempt_budget::set(&ledger, &change, n, actor, &at)
            .context("recording the attempt budget")?
        {
            Ok(_) => {
                let status = attempt_budget::status(&ledger, &change)
                    .context("reading the attempt budget back")?;
                print_all(&attempt_budget::sentences(&change, status.as_ref(), None));
                Ok(ExitCode::SUCCESS)
            }
            Err(refusal) => {
                println!("{}", refusal.plain(&change));
                Ok(ExitCode::FAILURE)
            }
        };
    }

    let status = attempt_budget::status(&ledger, &change).context("reading the attempt budget")?;
    let spent = status.as_ref().is_some_and(|s| s.spent);
    let ready = if spent {
        match crate::uiserve::served_readiness(&ledger, &change) {
            Ok((_, _, readiness)) => Some(readiness.ready),
            Err(e) => {
                // Said, not hidden (Rule 12): without the answer, the sentence
                // below says neither "unresolved" nor "Ready".
                eprintln!(
                    "thruline change attempts: the readiness answer could not be read: {e:#}"
                );
                None
            }
        }
    } else {
        None
    };
    print_all(&attempt_budget::sentences(&change, status.as_ref(), ready));
    Ok(if spent {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}
