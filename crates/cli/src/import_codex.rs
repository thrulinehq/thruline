//! `thruline import-codex` — BUILD-SPEC §4.2's rollout path, wired.
//!
//! The reader existed and nothing consumed it, which is a reader with no
//! consumer. This is the consumer.
//!
//! **What it cannot do is the first thing it prints.** Every session it creates
//! is `capture_degraded`, because a rollout is the harness's own log rather
//! than execution evidence (§0.2), so no claim in an imported session can reach
//! `supported`. What it buys is Explain's history — a timeline, compaction
//! boundaries, the files a patch touched — for sessions the shim never saw.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use thruline_ledger::Ledger;

pub struct Args {
    pub sessions_root: Option<PathBuf>,
    pub session_file: Option<PathBuf>,
    pub exec_db: Option<PathBuf>,
    pub repo: Option<PathBuf>,
    pub limit: Option<usize>,
    /// Report what would be imported and write nothing.
    pub dry_run: bool,
    pub json: bool,
}

pub fn run(args: Args) -> Result<ExitCode> {
    let root = match args.sessions_root.or_else(|| {
        args.session_file
            .as_ref()
            .and_then(|p| p.parent().map(PathBuf::from))
    }) {
        Some(p) => p,
        None => thruline_capture::codex_rollout::default_sessions_root()
            .context("locating ~/.codex/sessions; set HOME or pass --sessions-root")?,
    };
    if !root.is_dir() {
        anyhow::bail!(
            "no Codex sessions directory at {}. §4.2 says `.codex/sessions/*.jsonl`; \
             the real layout is user-global and date-nested, \
             `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`",
            root.display()
        );
    }
    let repo = args
        .repo
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));

    if args.dry_run {
        // Reads the first line of each rollout and nothing else: the question
        // is "how many of these are mine", and answering it must not cost a
        // parse of years of history.
        let (mine, others) = survey(&root, &repo);
        if args.json {
            crate::json_out::print(
                &crate::json_out::IMPORT_CODEX_DRY_RUN,
                serde_json::json!({
                    "dry_run": true,
                    "sessions_root": root,
                    "repo": repo,
                    "rollouts_for_this_repo": mine,
                    "rollouts_for_other_repos": others,
                }),
            )?;
        } else {
            println!(
                "{mine} rollout(s) under {} name this repository; {others} name another.",
                root.display()
            );
            println!("Nothing was written. Re-run without --dry-run to import.");
        }
        return Ok(ExitCode::SUCCESS);
    }

    let db = match args.exec_db {
        Some(p) => p,
        None => Ledger::default_path().context("locating the execution store")?,
    };
    let ledger = Ledger::open(&db).with_context(|| format!("opening {}", db.display()))?;
    let key = repo.to_string_lossy().to_string();
    let repo_id = ledger
        .repo(&key, Some(&key))
        .context("registering the repository")?;

    let (report, source_read) = if let Some(path) = &args.session_file {
        let (report, source) =
            thruline_capture::codex_rollout::import_live_file(path, &ledger, &repo_id, &repo)
                .map_err(anyhow::Error::msg)?;
        (report, Some(source))
    } else {
        (
            thruline_capture::codex_rollout::import(&root, &ledger, &repo_id, &repo, args.limit)
                .context("importing Codex rollouts")?,
            None,
        )
    };

    // An import that could not place every line is not a clean import. Same
    // rule as the fold: what could not be done is reported, and the exit code
    // says so.
    let clean = report.refused.is_empty();
    if args.json {
        crate::json_out::print(
            &crate::json_out::IMPORT_CODEX,
            serde_json::json!({
                "ok": clean,
                "source_read": source_read,
                "capture_degraded": true,
                "degraded_reason": thruline_capture::codex_rollout::DEGRADED_REASON,
                "rollouts_seen": report.rollouts_seen,
                "rollouts_for_other_repos": report.rollouts_for_other_repos,
                "lines_read": report.lines_read,
                "events_written": report.events_written,
                "events_skipped": report.events_skipped,
                "sessions": report.sessions,
                "fs_changes": report.fs_changes,
                "compaction_boundaries": report.compaction_boundaries,
                "patches_failed": report.patches_failed,
                "unmapped": report.unmapped,
                "refused": report.refused,
            }),
        )?;
    } else {
        println!(
            "{} rollout(s) read · {} session(s) imported · {} for other repositories",
            report.rollouts_seen, report.sessions, report.rollouts_for_other_repos
        );
        println!(
            "{} event(s) written · {} already present · {} line(s) read",
            report.events_written, report.events_skipped, report.lines_read
        );
        println!(
            "{} file change(s) recovered · {} compaction boundary(ies) · {} failed patch(es)",
            report.fs_changes, report.compaction_boundaries, report.patches_failed
        );
        for (family, n) in &report.unmapped {
            println!("  unmapped  {n:>6}  {family}");
        }
        for (ordinal, why) in report.refused.iter().take(20) {
            println!("  REFUSED   line {ordinal}: {why}");
        }
        if report.refused.len() > 20 {
            println!("  ... and {} more", report.refused.len() - 20);
        }
        println!();
        // Said every time, not once in a manual. §13's ceiling is the most
        // important thing about this command.
        println!(
            "These sessions are marked capture_degraded: a rollout is the harness's own\n\
             log, not execution evidence (§0.2), so no claim in them can be supported.\n\
             They are history for `thruline explain`, and nothing more."
        );
    }
    Ok(if clean {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

/// `(rollouts naming this repo, rollouts naming another)`.
fn survey(root: &std::path::Path, repo: &std::path::Path) -> (u64, u64) {
    use std::io::BufRead;
    let repo = std::fs::canonicalize(repo).unwrap_or_else(|_| repo.to_path_buf());
    let (mut mine, mut others) = (0u64, 0u64);
    for path in thruline_capture::codex::rollouts(root) {
        let Ok(file) = std::fs::File::open(&path) else {
            continue;
        };
        let mut first = String::new();
        if std::io::BufReader::new(file).read_line(&mut first).is_err() {
            continue;
        }
        match thruline_capture::codex::recover(0, first.trim()) {
            Ok(thruline_capture::codex::Recovered {
                kind: thruline_capture::codex::RecoveredKind::SessionMeta { cwd, .. },
                ..
            }) => {
                let cwd = std::fs::canonicalize(&cwd).unwrap_or_else(|_| PathBuf::from(&cwd));
                if cwd.starts_with(&repo) {
                    mine += 1;
                } else {
                    others += 1;
                }
            }
            _ => others += 1,
        }
    }
    (mine, others)
}
