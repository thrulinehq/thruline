//! Record a finished `drive` session, without re-spending the ledger.
//!
//! ```text
//! cargo run -p thruline-bench --example record -- \
//!     --task <id> --corpus <…/original-tasks> \
//!     --launched-by <operator|coordinator|…> \
//!     --expect-protocol sha256:… \
//!     --tree <40-hex pre-action tree> \
//!     --state completed \
//!     --oracle-exit 0            # omit entirely if the oracle did not run
//!     --duration-ms 900000 \
//!     --executed-checks 0
//! ```
//!
//! # Why this is not `--prepared <path>`
//!
//! A session is prepared in one process and recorded in another, with a human
//! in between, and `Prepared` cannot survive that gap **by design**: it carries
//! the `RunManifest`, and the only way to obtain one is `HoldoutLedger::spend`.
//! Loading a `Prepared` from disk would mean either serialising a spend — a
//! second line in an append-only ledger for a run already made — or forging one.
//!
//! So the commitment is **re-derived** from the same request by
//! `drive::commitment_for`, which stages nothing and spends nothing, and
//! `--expect-protocol` is compared against it. The launcher pastes the hash
//! their brief printed; if the two disagree, something about the run differs
//! from what was committed to and this refuses rather than recording a row
//! under a protocol that did not govern it.
//!
//! # `--oracle-exit` is omitted, not zeroed
//!
//! `None` means the oracle did not run and is **not** a synonym for a non-zero
//! exit. Omit the flag in that case. Passing `0` says the task's own tests were
//! executed and passed, which is a different claim entirely.

use std::path::PathBuf;
use std::process::ExitCode;

use thruline_bench::class_two::TerminalState;
use thruline_bench::drive::{self, DriveError, DriveRequest, Observed};
use thruline_bench::pilot::Harness;

fn main() -> ExitCode {
    let mut task = None;
    let mut corpus = None;
    let mut launched_by = String::new();
    let mut expect = None;
    let mut tree = None;
    let mut state_s = "completed".to_string();
    let mut oracle_exit: Option<i32> = None;
    let mut duration_ms: u64 = 0;
    let mut executed_checks: usize = 0;
    let mut model = "claude-opus-5".to_string();
    let mut harness = Harness::ClaudeCode;

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = || args.next().unwrap_or_default();
        match a.as_str() {
            "--task" => task = Some(val()),
            "--corpus" => corpus = Some(PathBuf::from(val())),
            "--launched-by" => launched_by = val(),
            "--expect-protocol" => expect = Some(val()),
            "--tree" => tree = Some(val()),
            "--state" => state_s = val(),
            "--oracle-exit" => oracle_exit = val().parse().ok(),
            "--duration-ms" => duration_ms = val().parse().unwrap_or(0),
            "--executed-checks" => executed_checks = val().parse().unwrap_or(0),
            "--model" => model = val(),
            "--harness" => match Harness::named(&val()) {
                Some(h) => harness = h,
                None => return ExitCode::from(2),
            },
            other => {
                eprintln!("unknown argument `{other}`");
                return ExitCode::from(2);
            }
        }
    }

    let (Some(task), Some(corpus), Some(expect)) = (task, corpus, expect) else {
        eprintln!(
            "--task, --corpus, --launched-by and --expect-protocol are required.\n\
             --expect-protocol is the hash your brief printed; it is checked \
             against a re-derivation so a row cannot be recorded under a \
             protocol that did not govern the run."
        );
        return ExitCode::from(2);
    };

    // Terminal states a launcher can legitimately report. `NoBindableTree` and
    // `Missing` are NOT here: `record` and `ingest` derive those, and letting a
    // caller assert them would let a session declare its own precondition met.
    let state = match state_s.as_str() {
        "completed" => TerminalState::Completed,
        "agent-timeout" => TerminalState::AgentTimeout,
        "oracle-timeout" => TerminalState::OracleTimeout,
        "out-of-memory" => TerminalState::OutOfMemory,
        other => {
            eprintln!(
                "unknown --state `{other}`. One of: completed, agent-timeout, \
                 oracle-timeout, out-of-memory.\n\
                 `no-bindable-tree` and `missing` are deliberately not settable \
                 — they are derived from what you report, not asserted."
            );
            return ExitCode::from(2);
        }
    };

    let req = DriveRequest {
        task: task.clone(),
        corpus,
        // Neither is read by `commitment_for`; a record is not a preparation.
        root: PathBuf::from("/nonexistent"),
        run_id: String::new(),
        harness,
        model,
        launched_by,
        ledger: PathBuf::from("/nonexistent"),
    };

    let commitment = match drive::commitment_for(&req) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(match e {
                DriveError::UnknownTask { .. } | DriveError::NoLauncher => 2,
                DriveError::Uncommittable(_) | DriveError::OracleLeaked { .. } => 3,
                _ => 1,
            });
        }
    };

    if commitment.hash() != expect {
        eprintln!(
            "PROTOCOL MISMATCH — refusing to record.\n\
             \n  expected (yours): {expect}\n  re-derived:       {}\n\n\
             The run you are recording was committed under different terms than \
             the ones these arguments describe. Something differs: the task, the \
             launcher, the model, the harness, or the build. A row recorded here \
             would carry a protocol hash that did not govern the session, which \
             is the one thing `protocol_hash` exists to make impossible.",
            commitment.hash()
        );
        return ExitCode::from(1);
    }

    let row = drive::record_under(
        &task,
        &commitment,
        &Observed {
            state,
            oracle_exit,
            duration_ms,
            executed_checks,
            tree_before_first_action: tree,
        },
    );

    println!("task_id                  : {}", row.task_id);
    println!("state                    : {}", row.state.as_str());
    println!(
        "oracle_exit              : {}",
        row.oracle_exit
            .map(|c| c.to_string())
            .unwrap_or_else(|| "None (the oracle did not run)".into())
    );
    println!("executed_checks          : {}", row.executed_checks);
    println!("duration_ms              : {}", row.duration_ms);
    println!(
        "tree_before_first_action : {}",
        row.tree_before_first_action.as_deref().unwrap_or("None")
    );
    println!("protocol_hash            : {}", row.protocol_hash);

    if row.state == TerminalState::NoBindableTree {
        eprintln!(
            "\nNOTE: recorded as NoBindableTree. The tree you gave is not shaped \
             like a git tree hash, or was git's empty tree, so this session \
             supports no verdict and carries zero checks. That is the honest \
             outcome and not an error — but it is not a countable result either."
        );
    }
    ExitCode::SUCCESS
}
