//! `thruline import-legacy` — BUILD-SPEC §11's one-time importer.
//!
//! Thin: every decision lives in `thruline_ledger::importer`, which is where the
//! tests are. This file's whole job is argument plumbing and a report a human
//! can read.
//!
//! **It does not read `--db`.** BUILD-PLAN C.2: `--db` names the legacy
//! transcript ledger and every one of the existing subcommands inherits it. One
//! flag that means two different databases depending on the subcommand is how a
//! receipt ends up pointing at the wrong chain, so the destination here is
//! `--exec-db`, defaulting to `~/.thruline/exec.db`.

use std::path::PathBuf;

use std::process::ExitCode;

use anyhow::{Context, Result};
use thruline_ledger::{ImportOptions, Importer, Ledger};

pub struct Args {
    pub source: Option<PathBuf>,
    pub exec_db: Option<PathBuf>,
    pub project: Option<String>,
    pub dry_run: bool,
    pub limit: Option<u64>,
    pub page_size: u64,
    pub page_bytes: u64,
    pub include_meta: bool,
    pub verify: bool,
    pub json: bool,
}

pub fn run(args: Args) -> Result<ExitCode> {
    let source = match args.source {
        Some(path) => path,
        None => legacy_default().context("locating the legacy ledger")?,
    };
    if !source.is_file() {
        anyhow::bail!("no legacy ledger at {}", source.display());
    }
    let target_path = match args.exec_db {
        Some(path) => path,
        None => Ledger::default_path().context("locating the execution store")?,
    };

    let ledger =
        Ledger::open(&target_path).with_context(|| format!("opening {}", target_path.display()))?;

    let mut options = ImportOptions::new(&source);
    options.project_id = args.project;
    options.dry_run = args.dry_run;
    options.limit = args.limit;
    options.page_size = args.page_size.max(1);
    options.page_bytes = args.page_bytes.max(1);
    options.include_meta_events = args.include_meta;

    let report = Importer::new(&ledger)
        .run(&options)
        .with_context(|| format!("importing {}", source.display()))?;

    // BUILD-PLAN M0.11 done-when (a) is "imports AND the resulting chain
    // verifies". An importer that cannot state the second half has delivered
    // half a feature, so the check lives here rather than in a one-off script.
    // Streaming, so verifying 76,203 sessions costs one event of memory.
    let mut verified = 0u64;
    let mut chain_failures: Vec<(String, String)> = Vec::new();
    if args.verify && !args.dry_run {
        ledger.for_each_session_id(|session| {
            match ledger.verify_session(session) {
                Ok(_) => verified += 1,
                Err(error) => chain_failures.push((session.to_string(), error.to_string())),
            }
            Ok(())
        })?;
    }

    if args.json {
        let value = serde_json::json!({
            "source": source.display().to_string(),
            "exec_db": target_path.display().to_string(),
            "dry_run": report.dry_run,
            "rows_read": report.rows_read,
            "events_written": report.events_written,
            "events_skipped": report.events_skipped,
            "sessions_seen": report.sessions_seen,
            "meta_rows_aggregated": report.meta_rows_aggregated,
            "sessions_with_dead_source": report.sessions_with_dead_source,
            "legacy_kinds": report.kinds,
            "rejected": report.rejected.len(),
            "rejected_sample": report.rejected.iter().take(20).collect::<Vec<_>>(),
            "chains_verified": verified,
            "chain_failures": chain_failures.len(),
            "chain_failure_sample": chain_failures.iter().take(20).collect::<Vec<_>>(),
        });
        crate::json_out::print(&crate::json_out::IMPORT_LEGACY, value)?;
        return Ok(exit_code(&report, &chain_failures));
    }

    if report.dry_run {
        println!("dry run — nothing was written");
    }
    println!("source     {}", source.display());
    println!("exec store {}", target_path.display());
    println!("rows read              {}", report.rows_read);
    println!("events written         {}", report.events_written);
    println!("events already present {}", report.events_skipped);
    println!("sessions               {}", report.sessions_seen);
    println!(
        "meta rows folded       {} (into one census per session)",
        report.meta_rows_aggregated
    );
    println!(
        "sessions whose source transcript is gone  {}",
        report.sessions_with_dead_source
    );
    if !report.kinds.is_empty() {
        println!("legacy kinds:");
        for (kind, count) in &report.kinds {
            println!("  {kind:<20} {count}");
        }
    }
    // Fail loud: a rejected row is not a rounding error, it is a row this
    // build could not represent, and a silent count is how that gets ignored.
    if !report.rejected.is_empty() {
        println!("rejected               {}", report.rejected.len());
        for (id, reason) in report.rejected.iter().take(10) {
            println!("  {id}: {reason}");
        }
        if report.rejected.len() > 10 {
            println!("  … and {} more", report.rejected.len() - 10);
        }
    }
    if args.verify && !args.dry_run {
        println!("chains verified        {verified}");
        if !chain_failures.is_empty() {
            println!("CHAINS THAT DO NOT VERIFY  {}", chain_failures.len());
            for (session, reason) in chain_failures.iter().take(10) {
                println!("  {session}: {reason}");
            }
        }
    }
    println!();
    println!("Every imported session is marked capture_degraded: these rows record what");
    println!("the agent said, not what the machine did. Their claims can reach");
    println!("`unverifiable` — never `supported`.");
    Ok(exit_code(&report, &chain_failures))
}

/// 0 when every row was represented and every chain verifies; 1 otherwise.
///
/// Rule 12: an import that could not represent part of its input has not
/// succeeded, and a script that only checks the exit code must not be told
/// otherwise. The rows that DID import are still written — this reports the
/// shortfall, it does not roll back evidence.
fn exit_code(
    report: &thruline_ledger::ImportReport,
    chain_failures: &[(String, String)],
) -> ExitCode {
    if report.rejected.is_empty() && chain_failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// `$THRULINE_HOME/ledger.db`, defaulting to `~/.thruline/ledger.db`.
fn legacy_default() -> Result<PathBuf> {
    let home = std::env::var_os("THRULINE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".thruline")))
        .context("no home directory: set THRULINE_HOME or pass --source")?;
    Ok(home.join("ledger.db"))
}
