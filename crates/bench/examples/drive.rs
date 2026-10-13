//! Run `drive` before `thruline bench drive` exists.
//!
//! The CLI wiring is ~15 lines in `crates/cli`, which Track E does not own, and
//! it is requested as R1 in docs/tracks/E/INTERFACE-REQUESTS.md. Until it lands, this is how
//! an operator actually uses the thing:
//!
//! ```text
//! cargo run -p thruline-bench --example drive -- \
//!     --task <id> \
//!     --corpus <.../tb-src/original-tasks> \
//!     --root   <where to stage> \
//!     --ledger <.../holdout-spend.tsv> \
//!     [--run-id <id>] [--model <name>] [--harness claude-code|codex|terminus]
//! ```
//!
//! **It is an example rather than a `src/bin`, deliberately.** A binary target
//! would make `crates/bench/src/` an executable surface for
//! `tests/split_enforcement_census.rs`, which would turn six registered
//! exceptions stale — correctly, but as a side effect of a convenience script
//! rather than as the deliberate act of shipping a runner. The census entries
//! come out when the CLI dispatches, and not a moment before.
//!
//! Every decision, refusal and message belongs to `thruline_bench::drive`. This
//! file parses argv and prints; it decides nothing.

use std::path::PathBuf;
use std::process::ExitCode;

use thruline_bench::drive::{self, DriveError, DriveRequest};
use thruline_bench::pilot::Harness;

fn main() -> ExitCode {
    let mut task = None;
    let mut corpus = None;
    let mut root = None;
    let mut ledger = None;
    let mut run_id = None;
    let mut model = "claude-opus-5".to_string();
    let mut launched_by = String::new();
    let mut harness = Harness::ClaudeCode;

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = || args.next().unwrap_or_default();
        match a.as_str() {
            "--task" => task = Some(val()),
            "--corpus" => corpus = Some(PathBuf::from(val())),
            "--root" => root = Some(PathBuf::from(val())),
            "--ledger" => ledger = Some(PathBuf::from(val())),
            "--run-id" => run_id = Some(val()),
            "--model" => model = val(),
            "--launched-by" => launched_by = val(),
            "--harness" => {
                let name = val();
                match Harness::named(&name) {
                    Some(h) => harness = h,
                    None => {
                        eprintln!("unknown harness `{name}`");
                        return ExitCode::from(2);
                    }
                }
            }
            other => {
                eprintln!("unknown argument `{other}`. See the module docs.");
                return ExitCode::from(2);
            }
        }
    }

    let (Some(task), Some(corpus), Some(root), Some(ledger)) = (task, corpus, root, ledger) else {
        eprintln!(
            "--task, --corpus, --root, --ledger and --launched-by are all required.\n\
             \n\
             --ledger in particular has no default ON PURPOSE: it is the \
             append-only record of which runs have consumed which holdout, and \
             a default would let two operators spend the same holdout into two \
             different files and each see a clean first run."
        );
        return ExitCode::from(2);
    };

    // The run id defaults to the task, which is enough to make a second spend of
    // the same protocol refuse rather than silently succeed under a fresh name.
    let run_id = run_id.unwrap_or_else(|| format!("drive-{task}"));

    let req = DriveRequest {
        task,
        corpus,
        root,
        run_id,
        harness,
        model,
        launched_by,
        ledger,
    };

    match drive::prepare(&req) {
        Ok(prepared) => {
            print!("{}", prepared.operator_brief());
            println!(
                "Then record what happened. `drive::record` forces \
                 NoBindableTree unless the workspace\ncarried a real git tree \
                 before the agent's first action — so initialise the \
                 workspace\nas a repository with content in it, or the session \
                 produces nothing countable."
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            // The mapping published in docs/BENCH-DRIVE.md. Kept in one place so
            // the CLI can adopt it verbatim rather than invent a second one.
            ExitCode::from(match e {
                DriveError::UnknownTask { .. } => 2,
                // A bug here, not a user error: 3 is the "this build is wrong"
                // code, and an oracle leak is exactly that.
                DriveError::Uncommittable(_) | DriveError::OracleLeaked { .. } => 3,
                DriveError::NoLauncher => 2,
                DriveError::Io(_) => 74,
                DriveError::NotFromTune(_)
                | DriveError::Split(_)
                | DriveError::AlreadySpent { .. } => 1,
            })
        }
    }
}
