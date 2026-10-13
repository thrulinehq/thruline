//! C30 R-LEDGER — `thruline capture pause|resume|exclude|include|holds`: the
//! terminal door to a person's pause and path exclusions.
//!
//! Each verb calls [`thruline_core::capture_hold::record_for_person`] (and
//! `holds` calls [`thruline_core::capture_hold::holds_view`]), the same
//! functions `POST /api/hq/projects/<id>/capture/pause` and `…/exclusions` and
//! `GET …/capture` call, so a hold put from a terminal and one put from the
//! screen leave rows of the same shape and are refused for the same reasons, in
//! the same words.
//!
//! **A PAUSE IS HONOURED; AN EXCLUSION IS NOT YET** (C34 Q4, Q5, Q6). The six
//! capture callers read a recorded pause (`capture_pause`): the hook delivery,
//! the Stop audit, the HQ load, the first-use register, the Codex observer and
//! `thruline ingest`. An exclusion is recorded and is applied when ThruLine
//! shows or exports, in a later card. Every verb that writes says both, and
//! `holds` reports how many hook deliveries and audits a pause left out. From a
//! terminal there is no Codex observer to follow in memory, but a running
//! `thruline ui` reads the record again every cycle, so a pause from here
//! reaches it within seconds.
//!
//! `capture status` is the Assured Capture command's (it reports installation
//! readiness and needs its configuration), so this verb is `holds`.
//!
//! The project is named by its id or by the folder it lives in. The verbs print
//! its name and id and never the folder, and exit 1 on a refusal because
//! "nothing was saved" is not success.

use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result};
use thruline_core::capture_hold::{self as holds, Act, Hold, HoldsView, Recorded};
use thruline_core::hq::{self as core_hq, HqProject};
use thruline_core::ledger::Ledger;

/// The project a verb is about, in its own struct so the generated argument
/// function stays small (`cli_surface_budget.rs`).
#[derive(Debug, clap::Args)]
pub struct ProjectArg {
    /// The project: its id as HQ shows it, or the folder it lives in.
    #[arg(long)]
    pub project: String,
}

/// The project and the path an exclusion is about.
#[derive(Debug, clap::Args)]
pub struct ExcludeArgs {
    /// The project: its id as HQ shows it, or the folder it lives in.
    #[arg(long)]
    pub project: String,
    /// A built-in name (env, secrets, generated) or one path inside the
    /// project; `*` is the only wildcard. A folder called `env` is `env/`.
    pub path: String,
}

fn masked(raw: &str) -> String {
    thruline_core::redact::mask_recorded(raw).0
}

/// The project a person named, or `None` after saying so.
fn project(ledger: &Ledger, named: &str) -> Result<Option<HqProject>> {
    let named = named.trim();
    let projects = core_hq::hq_projects(ledger).context("reading the projects")?;
    if let Some(p) = projects.iter().find(|p| p.id == named) {
        return Ok(Some(p.clone()));
    }
    let plain = |s: &str| {
        let t = s.trim_end_matches('/');
        (if t.is_empty() { s } else { t }).to_string()
    };
    let want = plain(named);
    let want_real = Path::new(named).canonicalize().ok();
    let found = projects.into_iter().find(|p| {
        plain(&p.root) == want
            || want_real
                .as_deref()
                .zip(Path::new(&p.root).canonicalize().ok())
                .is_some_and(|(a, b)| a == b)
    });
    if found.is_none() {
        println!(
            "No project in this record matches {}. Name its id or the folder it lives in. \
             Nothing was saved.",
            masked(named)
        );
    }
    Ok(found)
}

fn open(db: &Path) -> Result<Ledger> {
    Ledger::open(db).with_context(|| format!("opening {}", db.display()))
}

fn say_where(found: &HqProject) {
    println!("{} (id {})", masked(&found.name), found.id);
}

/// Record one act and say what came of it.
fn record_act(db: &Path, named: &str, act: Act, pattern: Option<&str>) -> Result<ExitCode> {
    let ledger = open(db)?;
    let Some(found) = project(&ledger, named)? else {
        return Ok(ExitCode::FAILURE);
    };
    let actor = crate::check::account_name();
    let at = thruline_core::ingest::iso8601_utc_now();
    let hold = Hold {
        project_id: &found.id,
        project_folder: &found.root,
        actor: &actor,
        recorded_at: &at,
    };
    match holds::record_for_person(&ledger, act, &hold, pattern).context("recording the hold")? {
        Recorded::Appended(_) => {
            say_where(&found);
            println!(
                "{} {}",
                match (act, pattern) {
                    (Act::Paused, _) => "Recorded: paused.".to_string(),
                    (Act::Resumed, _) => "Recorded: resumed.".to_string(),
                    (Act::Excluded, Some(p)) => format!("Recorded: {} is excluded.", masked(p)),
                    (_, Some(p)) => format!("Recorded: {} is no longer excluded.", masked(p)),
                    (_, None) => "Recorded.".to_string(),
                },
                holds::HONOURED_NOTE
            );
            Ok(ExitCode::SUCCESS)
        }
        Recorded::Refused(why) => {
            println!("{}", why.sentence());
            Ok(ExitCode::FAILURE)
        }
    }
}

/// `thruline capture pause <PROJECT>`.
pub fn pause(db: &Path, args: ProjectArg) -> Result<ExitCode> {
    record_act(db, &args.project, Act::Paused, None)
}

/// `thruline capture resume <PROJECT>`.
pub fn resume(db: &Path, args: ProjectArg) -> Result<ExitCode> {
    record_act(db, &args.project, Act::Resumed, None)
}

/// An exclusion's path, checked the way the route checks it. `Err` carries the
/// sentence already printed.
fn checked(path: &str) -> std::result::Result<&str, ()> {
    let path = path.trim();
    match holds::pattern_problem(path) {
        Some(why) => {
            println!("{}", holds::bad_pattern_sentence(why));
            Err(())
        }
        None => Ok(path),
    }
}

/// `thruline capture exclude <PROJECT> <PATH>`.
pub fn exclude(db: &Path, args: ExcludeArgs) -> Result<ExitCode> {
    let Ok(path) = checked(&args.path) else {
        return Ok(ExitCode::FAILURE);
    };
    record_act(db, &args.project, Act::Excluded, Some(path))
}

/// `thruline capture include <PROJECT> <PATH>`.
pub fn include(db: &Path, args: ExcludeArgs) -> Result<ExitCode> {
    let Ok(path) = checked(&args.path) else {
        return Ok(ExitCode::FAILURE);
    };
    record_act(db, &args.project, Act::Included, Some(path))
}

/// What a folder's record says, in plain lines.
fn print_view(view: &HoldsView) {
    match (&view.paused_at, &view.paused_by) {
        (Some(at), Some(by)) => println!("paused       yes, since {at} by {}", masked(by)),
        _ => println!("paused       no"),
    }
    for (i, e) in view.exclusions.iter().enumerate() {
        let lead = if i == 0 {
            "excluded     "
        } else {
            "             "
        };
        let state = if e.on { "excluded" } else { "not excluded" };
        let why = match (&e.basis, &e.since, &e.by) {
            (Some(basis), _, _) => basis.to_string(),
            (None, Some(at), Some(by)) => format!("since {at} by {}", masked(by)),
            _ => String::new(),
        };
        println!("{lead}{:<14} {state:<13} {why}", masked(&e.key));
    }
}

/// `thruline capture holds <PROJECT>` — read-only.
pub fn show(db: &Path, args: ProjectArg) -> Result<ExitCode> {
    let ledger = open(db)?;
    let Some(found) = project(&ledger, &args.project)? else {
        return Ok(ExitCode::FAILURE);
    };
    let view = holds::holds_view(&ledger, &found.root).context("reading the holds")?;
    say_where(&found);
    print_view(&view);
    // C34 pause gate: what the hook and the Stop audit left unrecorded while
    // this folder was paused, counted beside the ledger. Silent while zero.
    let left_out = thruline_core::capture_pause::skip_count_for(db, &found.root);
    if left_out.hook_deliveries + left_out.audits > 0 {
        println!(
            "left out     {} hook delivery(ies) and {} Stop audit(s) were not recorded while paused{}",
            left_out.hook_deliveries,
            left_out.audits,
            left_out
                .last_at
                .as_deref()
                .map(|at| format!(" (last {at})"))
                .unwrap_or_default()
        );
    }
    println!("{}", view.note);
    Ok(ExitCode::SUCCESS)
}
