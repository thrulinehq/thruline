//! C30 R-LEDGER — `thruline change second`: the terminal door to a second
//! approval, and to taking your own back (`--withdraw`).
//!
//! It calls [`thruline_core::change_decision_second::second`] and
//! [`thruline_core::change_decision_second::withdraw`], the same functions
//! `POST /api/changes/<id>/seconds` and `…/seconds/withdraw` call, so a second
//! put from a terminal and one put from the screen leave rows of the same shape
//! and are refused for the same reasons, in the same words. A second never makes
//! an approval stand and never moves a result, and it is free.
//!
//! A terminal has no screen that was showing a version, so `--seen-version`
//! defaults to the version the standing approval names, and the answer says
//! which version that was. Pass it to second only the version you were looking
//! at; a different one is refused as "moved", exactly as the screen's is.
//!
//! Exit 1 on a refusal, because "nothing was saved" is not success.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use thruline_core::change_decision::{self, Decide};
use thruline_core::change_decision_second::{self as seconds, Seconding, Withdrawing};
use thruline_core::ledger::Ledger;

/// `thruline change second`'s arguments, in their own struct so the generated
/// argument function stays small (`cli_surface_budget.rs`).
#[derive(Debug, clap::Args)]
pub struct SecondArgs {
    /// The Change whose approved version you are putting your name beside.
    pub change: String,
    /// Your name, as you want it recorded. Defaults to the account this is
    /// running as. ThruLine records the name you type; it does not check who is
    /// at the keyboard.
    #[arg(long)]
    pub actor: Option<String>,
    /// The version you were looking at. Defaults to the version the standing
    /// approval names.
    #[arg(long)]
    pub seen_version: Option<String>,
    /// Take back your own standing second approval instead of making one.
    #[arg(long)]
    pub withdraw: bool,
}

pub fn run(db: PathBuf, args: SecondArgs) -> Result<ExitCode> {
    let ledger = Ledger::open(&db).with_context(|| format!("opening {}", db.display()))?;
    let change = args.change.trim().to_string();
    let actor = match args.actor.as_deref().map(str::trim) {
        Some(given) if !given.is_empty() => given.to_string(),
        _ => crate::check::account_name(),
    };
    if !seconds::exists(&ledger, &change).context("looking for the Change")? {
        println!("No Change with id {change:?} is recorded in this store. Nothing was saved.");
        return Ok(ExitCode::FAILURE);
    }
    let at = thruline_core::ingest::iso8601_utc_now();
    if args.withdraw {
        return withdraw(&ledger, &change, &actor, &at);
    }
    let standing = change_decision::read(&ledger, &change)
        .context("reading the approval that stands")?
        .standing;
    let seen = match (&args.seen_version, &standing) {
        (Some(given), _) => given.trim().to_string(),
        (None, Some(approval)) => approval.target_revision.clone(),
        (None, None) => String::new(),
    };
    let decide = Decide {
        actor,
        at,
        seen_revision: seen,
        note: None,
    };
    match seconds::second(&ledger, &change, &decide).context("recording the second")? {
        Seconding::Recorded { second } => {
            let by = standing.map(|a| a.actor).unwrap_or_default();
            println!(
                "Recorded {} as a second person on version {} of {change}, which {by} approved. \
                 The approval still stands as it was; this adds a name beside it and changes no \
                 result. {}",
                seconds::masked_text(&second.actor),
                seconds::masked_text(&second.target_revision),
                seconds::NAME_NOT_CHECKED,
            );
            Ok(ExitCode::SUCCESS)
        }
        Seconding::NothingToSecond => {
            println!("{}", seconds::NOTHING_TO_SECOND);
            Ok(ExitCode::FAILURE)
        }
        Seconding::SamePerson { with } => {
            println!(
                "{}",
                seconds::same_person_sentence(&seconds::masked_text(&with))
            );
            Ok(ExitCode::FAILURE)
        }
        Seconding::Moved { seen, now, changed } => {
            println!(
                "{} (You named {}; the approval is for {}{}.)",
                seconds::MOVED,
                seconds::masked_text(&seen),
                seconds::masked_text(&now),
                if changed.is_empty() {
                    String::new()
                } else {
                    format!(
                        "; changed since: {}",
                        changed
                            .iter()
                            .map(|path| seconds::masked_text(path))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            );
            Ok(ExitCode::FAILURE)
        }
    }
}

fn withdraw(ledger: &Ledger, change: &str, actor: &str, at: &str) -> Result<ExitCode> {
    match seconds::withdraw(ledger, change, actor, at).context("taking the second back")? {
        Withdrawing::Recorded { withdrawal } => {
            println!(
                "Took back {}'s second approval on {change}. The approval itself still stands \
                 as it was. You may second it again.",
                seconds::masked_text(&withdrawal.actor)
            );
            Ok(ExitCode::SUCCESS)
        }
        Withdrawing::NoSecondToWithdraw => {
            println!(
                "{}",
                seconds::no_second_to_withdraw_sentence(&seconds::masked_text(actor))
            );
            Ok(ExitCode::FAILURE)
        }
    }
}
