//! `thruline pilot-budget` — the $3 ceiling, wired to the thing that runs.
//!
//! # Why this file exists
//!
//! `Budget`, `BudgetStop`, `Ledger::charge` and `check_first_task` were
//! referenced from exactly one place outside their own definitions: the test
//! file. `grep -c pilot crates/cli/src/main.rs` returned **0**. A guard that
//! only its own tests can reach is a guard the product does not have — the
//! ninth instance of that shape in this program.
//!
//! `scripts/pilot-run.sh` is the thing that actually spends money, and it is a
//! shell script. So the ceiling has to be reachable from a shell, with an exit
//! code the shell can branch on:
//!
//! ```text
//! thruline pilot-budget check  --ledger P --model M      # room for one more?
//! thruline pilot-budget charge --ledger P --model M --task T --input N ...
//! thruline pilot-budget show   --ledger P
//! ```
//!
//! **Exit 4 is a budget halt** and nothing else in this binary uses it, so the
//! runner cannot mistake a halt for an ordinary failure or for success.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{anyhow, Result};
use thruline_bench::pilot::{price_for, Budget, Ledger, Usage, UsageSource, PRICES, USD};

/// The exit code for a budget halt. **A halt is a result, not a failure**, but
/// it is emphatically not a success, and 4 is used by nothing else here.
pub const HALT: u8 = 4;

pub enum Action {
    /// Is there room for one more task at its worst permitted price?
    Check,
    /// Record what a task cost, refusing if it would cross the cap.
    Charge {
        task: String,
        input: u64,
        output: u64,
        cache_read: u64,
        cache_write_5m: u64,
        cache_write_1h: u64,
        source: String,
    },
    /// Print the running total.
    Show,
}

pub struct Args {
    pub ledger: PathBuf,
    pub model: Option<String>,
    pub action: Action,
}

fn usd(m: u64) -> String {
    format!("${:.6}", m as f64 / USD as f64)
}

fn resolve_price(model: Option<&str>) -> Result<thruline_bench::pilot::Price> {
    let model = model.ok_or_else(|| {
        anyhow!("--model is required: an unpinned model is an unpriced run, and the cost line would be an estimate wearing a measurement's clothes")
    })?;
    price_for(model).ok_or_else(|| {
        let known: Vec<&str> = PRICES.iter().map(|p| p.model).collect();
        anyhow!(
            "`{model}` is not in the pinned price table, so this run cannot be \
             priced. Refusing to guess a rate. Known: {}",
            known.join(", ")
        )
    })
}

pub fn run(args: Args) -> Result<ExitCode> {
    let budget = Budget::pilot_authorisation();
    let mut ledger = Ledger::open(&args.ledger)?;

    match args.action {
        Action::Show => {
            print_state(&ledger, budget);
            Ok(ExitCode::SUCCESS)
        }
        Action::Check => {
            // The model is resolved even though `check` does not price anything,
            // because a run that is about to start on an unpriced model must be
            // refused HERE, before the money is spent, rather than at the report
            // afterwards when refusing costs the whole task.
            let price = resolve_price(args.model.as_deref())?;
            print_state(&ledger, budget);
            match ledger.headroom_for_one_more(budget) {
                Ok(slack) => {
                    println!(
                        "pilot-budget: OK to run one more task on `{}` — {} of headroom \
                         beyond the worst-case task",
                        price.model,
                        usd(slack)
                    );
                    Ok(ExitCode::SUCCESS)
                }
                Err(stop) => {
                    eprintln!("pilot-budget: {stop}");
                    Ok(ExitCode::from(HALT))
                }
            }
        }
        Action::Charge {
            task,
            input,
            output,
            cache_read,
            cache_write_5m,
            cache_write_1h,
            source,
        } => {
            let price = resolve_price(args.model.as_deref())?;
            let source = UsageSource::named(&source).ok_or_else(|| {
                anyhow!(
                    "unknown usage source `{source}`. Expected one of: turn-completed, \
                     rollout-file, reconciled. An unrecognised provenance must never \
                     become the optimistic one by default"
                )
            })?;
            // Already-disjoint buckets: the reader that produced them did the
            // vendor-specific subtraction. See `Usage`'s doc comment for why
            // that is not this command's job.
            let usage = Usage::from_claude_code(
                input,
                output,
                cache_read,
                cache_write_5m,
                cache_write_1h,
                source,
            );
            if usage.billable_tokens() == 0 {
                return Err(anyhow!(
                    "refusing to charge `{task}` zero tokens. A paid task that reports \
                     no usage is a BROKEN READER, not a free task, and recording $0 \
                     for it makes the ledger understate every later check"
                ));
            }
            let cost = usage.cost(price);
            match ledger.charge(&task, usage, price, budget) {
                Ok(charged) => {
                    println!(
                        "pilot-budget: charged `{task}` {} on `{}` (source {})",
                        usd(charged),
                        price.model,
                        source.as_str()
                    );
                    print_state(&ledger, budget);
                    Ok(ExitCode::SUCCESS)
                }
                Err(stop) => {
                    eprintln!("pilot-budget: {stop}");
                    eprintln!(
                        "pilot-budget: the task cost {} and the ledger stands at {} \
                         over {} task(s).",
                        usd(cost),
                        usd(ledger.spent()),
                        ledger.tasks_charged()
                    );
                    Ok(ExitCode::from(HALT))
                }
            }
        }
    }
}

fn print_state(ledger: &Ledger, budget: Budget) {
    println!("pilot-budget: ledger      {}", ledger.path().display());
    println!(
        "pilot-budget: spent       {} over {} task(s)",
        usd(ledger.spent()),
        ledger.tasks_charged()
    );
    println!(
        "pilot-budget: run cap     {}   per-task cap {}",
        usd(budget.run_total),
        usd(budget.per_task)
    );
    // This line read `authorised $8.000000` for as long as the $8 figure sat in
    // the constructor: an OpenAI-era number reported against an Anthropic
    // credit, over a cap that was and is $3. The authorisation and the credit
    // are now two lines because they are two facts, and the reserved part says
    // out loud that it is not spendable here.
    println!(
        "pilot-budget: authorised  {} for this pilot   remaining {}",
        usd(budget.authorised),
        usd(budget.authorised_remaining(ledger.spent()))
    );
    println!(
        "pilot-budget: credit      {} total; {} RESERVED — not authorised for this run",
        usd(budget.credit),
        usd(budget.reserved())
    );
    if ledger.any_may_understate() {
        println!(
            "pilot-budget: CAUTION — at least one charge came from a source that omits \
             reasoning tokens. The total above is a FLOOR."
        );
    }
}
