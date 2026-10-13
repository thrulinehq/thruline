//! BUILD-SPEC §4.4 / §14 — `thruline shim` and `thruline fold`.
//!
//! `shim install` writes the PATH shim directory and prints the one line a
//! shell needs to start using it. `shim exec` is what the wrappers invoke; it
//! is hidden from `--help` because a human never types it. `shim status` is the
//! integrity report, also folded into `thruline doctor`.
//!
//! `fold` turns the spool — hook deliveries and recorded execs alike — into the
//! hash-chained event store. It is a separate command rather than something the
//! hook does, for the reason §4.1's 50 ms budget gives: the hook writes one
//! line and exits, and everything that opens a database happens here.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use thruline_capture::exec::{self, RecordOptions, SessionBinding};
use thruline_capture::shim::{self, InstallOptions, ShimManifest};
use thruline_ledger::Ledger;

/// `$THRULINE_HOME`, or `~/.thruline`.
pub fn thruline_home() -> Result<PathBuf> {
    if let Some(home) = std::env::var_os("THRULINE_HOME") {
        return Ok(PathBuf::from(home));
    }
    let home = std::env::var_os("HOME").context("neither THRULINE_HOME nor HOME is set")?;
    Ok(PathBuf::from(home).join(".thruline"))
}

pub struct InstallArgs {
    pub dir: Option<PathBuf>,
    /// Which harness's spool recorded execs join.
    ///
    /// **One session's lines must live in one spool file.** The fold numbers a
    /// line by its ordinal in its spool and resumes at the session's chain
    /// tail, so hook deliveries in one file and execs in another put two
    /// numbering schemes in one seq space: whichever folds second hits the
    /// resume guard and its lines are counted as already present. Measured —
    /// a Codex session lost its `session_start` and `user_prompt` that way.
    pub harness: Option<String>,
    pub spool: Option<PathBuf>,
    pub session: Option<String>,
    pub repo: Option<PathBuf>,
    pub bin: Option<PathBuf>,
    /// `--wrap`: `resolvable` (default), `all`, or a comma-separated subset.
    /// See [`wrap_policy`].
    pub wrap: Option<String>,
    pub undo: bool,
    pub json: bool,
}

/// Parse `--wrap` into a [`WrapPolicy`], **refusing anything it cannot honour.**
///
/// # Why this exists at all
///
/// §4.4.4's amended predicate made [`WrapPolicy::Resolvable`] the default and
/// the only reachable policy: `WrapPolicy::All` and `WrapPolicy::Only` existed
/// in the library with no way for a caller to ask for them. The cost landed on
/// the suite. `crates/cli/tests/m08_exec_shim.rs` installs through this command
/// and then asserts things about wrappers for `make`, `npm`, `cargo` and
/// `pytest` — which under `Resolvable` exist only if the host happens to have
/// those programs. Measured at `9a7cfb6`: 28/28 on a developer's machine, and
/// **12 failures** under `env -i PATH=/usr/bin:/bin`. A test whose meaning is a
/// function of the host's PATH is not a test, and nothing in the output said
/// that was what had happened.
///
/// # It refuses a name it does not recognise
///
/// A typo — `--wrap pytst` — would otherwise install a shim wrapping nothing,
/// which is the state `ShimIntegrity::captures_nothing` exists to catch after
/// the fact. Catching it before the write is cheaper and says more.
pub fn wrap_policy(spec: &str) -> Result<shim::WrapPolicy> {
    match spec.trim() {
        "resolvable" => Ok(shim::WrapPolicy::Resolvable),
        "all" => Ok(shim::WrapPolicy::All),
        "" => anyhow::bail!(
            "--wrap needs a value: `resolvable`, `all`, or a comma-separated subset of {}",
            shim::WRAPPED.join(", ")
        ),
        list => {
            let names: Vec<String> = list
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
            let unknown: Vec<&str> = names
                .iter()
                .map(String::as_str)
                .filter(|n| !shim::WRAPPED.contains(n))
                .collect();
            if !unknown.is_empty() {
                anyhow::bail!(
                    "--wrap names {} which {} not a shim candidate. A wrapper is only ever \
                     written for one of: {}.\n  Wrapping a name outside that set would write \
                     an executable nothing parses the output of, and misspelling one would \
                     install a shim that wraps nothing and says it succeeded.",
                    unknown.join(", "),
                    if unknown.len() == 1 { "is" } else { "are" },
                    shim::WRAPPED.join(", ")
                );
            }
            if names.is_empty() {
                anyhow::bail!("--wrap was given no names");
            }
            Ok(shim::WrapPolicy::Only(names))
        }
    }
}

pub fn install(args: InstallArgs) -> Result<ExitCode> {
    let home = thruline_home()?;
    let dir = args.dir.clone().unwrap_or_else(|| home.join("shim"));

    if args.undo {
        let removed =
            shim::uninstall(&dir).with_context(|| format!("removing {}", dir.display()))?;
        if args.json {
            crate::json_out::print(
                &crate::json_out::SHIM_UNINSTALL,
                serde_json::json!({
                    "removed": removed.wrappers,
                    "modified_before_removal": removed.modified,
                    "left_alone": removed.left_alone,
                    "dir_removed": removed.dir_removed,
                }),
            )?;
        } else {
            println!(
                "removed {} wrapper(s) from {}",
                removed.wrappers.len(),
                dir.display()
            );
            if !removed.modified.is_empty() {
                // Never delete evidence of tampering without saying so.
                println!(
                    "NOTE: {} wrapper(s) had been edited since install and were removed: {}",
                    removed.modified.len(),
                    removed.modified.join(", ")
                );
            }
            if !removed.left_alone.is_empty() {
                println!(
                    "left alone (not written by ThruLine): {}",
                    removed.left_alone.join(", ")
                );
            }
        }
        return Ok(ExitCode::SUCCESS);
    }

    let opts = install_options(&home, dir, &args)?;

    let manifest = shim::install(&opts).context("writing the shim directory")?;
    if args.json {
        crate::json_out::print(
            &crate::json_out::SHIM_INSTALL,
            serde_json::to_value(&manifest)?,
        )?;
    } else {
        println!(
            "wrote {} wrappers to {}",
            manifest.wrappers.len(),
            manifest.shim_dir.display()
        );
        if !manifest.skipped.is_empty() {
            // Naming them is the point. A wrapper for a program that does not
            // exist answers `command -v` and changes what the session does, so
            // "not written" is a fact about the capture the user is owed.
            println!(
                "skipped {} (no wrapper written): {}",
                manifest.skipped.len(),
                manifest
                    .skipped
                    .iter()
                    .map(|(name, why)| format!("{name} ({why})"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        println!();
        println!("Put it first on PATH for the shell your agent runs in:");
        println!("  export PATH=\"{}:$PATH\"", manifest.shim_dir.display());
        println!();
        // §4.4 snapshots write git objects into the user's repository, and
        // BUILD-PLAN C.6 makes disclosing that an obligation rather than a
        // courtesy: a user learning it from `du` is the trust failure this
        // product exists to prevent.
        println!("What happens next:");
        // `shim::WRAPPED` is the candidate list, not the written one. Printing
        // it here would name wrappers this install did not write and promise
        // capture that will not happen.
        println!(
            "  1. Wrapped commands ({}) are recorded: argv, exit code, output digests.",
            manifest
                .wrappers
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
        println!("  2. A git tree snapshot is taken before and after each one. This writes");
        println!("     unreferenced objects into your repository's .git/objects; `thruline");
        println!("     doctor` reports the footprint and `git gc --prune=now` reclaims it.");
        println!("  3. `thruline fold` turns the spool into the event store; nothing leaves");
        println!("     this machine.");
    }
    Ok(ExitCode::SUCCESS)
}

/// The options `shim install` writes with: ThruLine's home, the shim folder in
/// it, this binary, the harness's spool, the wrap policy and the repository a
/// snapshot falls back to. One place, so HQ's install route
/// (`uiserve::install_route`) writes a shim byte for byte as this verb does.
pub(crate) fn install_options(
    home: &std::path::Path,
    dir: PathBuf,
    args: &InstallArgs,
) -> Result<InstallOptions> {
    let bin = match &args.bin {
        Some(p) => p.clone(),
        None => std::env::current_exe().context("locating this binary")?,
    };
    let mut opts = InstallOptions::under(home, bin, thruline_core::ingest::iso8601_utc_now());
    opts.dir = dir;
    if let Some(name) = &args.harness {
        let harness = thruline_capture::harness::by_name(name).with_context(|| {
            format!(
                "no harness named {name:?}. Known: {}",
                thruline_capture::harness::ALL
                    .iter()
                    .map(|h| h.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
        opts.spool = thruline_capture::harness::default_spool(&harness)
            .context("locating the harness spool; set THRULINE_HOME or HOME")?;
    }
    if let Some(spool) = &args.spool {
        opts.spool = spool.clone();
    }
    if let Some(spec) = &args.wrap {
        opts.policy = wrap_policy(spec)?;
    }
    opts.session_id = args.session.clone();
    opts.repo_root = match &args.repo {
        Some(r) => Some(r.clone()),
        None => std::env::current_dir().ok(),
    };
    Ok(opts)
}

pub fn status(dir: Option<PathBuf>, json: bool) -> Result<ExitCode> {
    let dir = match dir {
        Some(d) => d,
        None => thruline_home()?.join("shim"),
    };
    let integrity = shim::verify(&dir, None);
    if json {
        crate::json_out::print(
            &crate::json_out::SHIM_STATUS,
            serde_json::to_value(&integrity)?,
        )?;
    } else if !integrity.installed {
        println!("no shim installed at {}", dir.display());
    } else {
        println!("shim {}", dir.display());
        println!("  intact     {}", integrity.intact.len());
        if integrity.captures_nothing {
            // Named, not merely counted. "intact 0" is a number a reader
            // subtracts from an expectation they may not have; this is the
            // sentence.
            println!("  CAPTURES NOTHING — no wrapper is installed, so every command this");
            println!("             session runs is invisible to the recorder. A completion");
            println!("             claim about tests can only reach `unverifiable` here.");
            match manifest_at(&dir) {
                Some(m) if !m.skipped.is_empty() => {
                    println!(
                        "             {} candidate(s) were skipped at install: {}",
                        m.skipped.len(),
                        m.skipped.keys().cloned().collect::<Vec<_>>().join(", ")
                    );
                    if let Some(p) = &m.install_path {
                        println!("             against PATH {p}");
                    }
                    println!(
                        "             Install the tools you expect to be recorded, then \
                         `thruline shim install` again."
                    );
                }
                _ => println!("             Run `thruline shim install` again."),
            }
        }
        if !integrity.modified.is_empty() {
            println!("  MODIFIED   {}", integrity.modified.join(", "));
        }
        if !integrity.missing.is_empty() {
            println!("  MISSING    {}", integrity.missing.join(", "));
        }
        if !integrity.unexpected.is_empty() {
            println!("  unexpected {}", integrity.unexpected.join(", "));
        }
        if !integrity.forged.is_empty() {
            println!(
                "  FORGED     wrapper(s) ThruLine did not write, on PATH under its name: {}",
                integrity.forged.join(", ")
            );
        }
        if !integrity.uncovered.is_empty() {
            println!(
                "  uncovered  resolves now, skipped at install (reinstall to record it): {}",
                integrity.uncovered.join(", ")
            );
        }
        println!("  on PATH    {}", integrity.on_path);
        if !integrity.shadowed_by.is_empty() {
            println!(
                "  SHADOWED   earlier on PATH and reached first: {}",
                integrity.shadowed_by.join(", ")
            );
        }
    }
    Ok(if integrity.ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

/// `thruline shim exec` — what a wrapper invokes.
///
/// Returns the child's status. **Never** its own: a wrapper that reports its own
/// success in place of the command's failure destroys exactly the evidence this
/// product collects.
pub fn run_exec(program: String, args: Vec<String>) -> ExitCode {
    ExitCode::from(exec::record(RecordOptions::from_env(program, args)))
}

pub struct FoldArgs {
    pub spool: Option<PathBuf>,
    pub exec_db: Option<PathBuf>,
    pub repo: Option<PathBuf>,
    /// Bring the spool level with `sensor_events` before folding. See
    /// [`crate::sensor_bridge`] for why the two stores drift apart.
    pub from_sensor: bool,
    /// Where the sensor chain lives — the `--db` ledger, not the execution
    /// store. Only read when `from_sensor` is set.
    pub sensor_db: PathBuf,
    pub json: bool,
}

/// Fold every harness spool for `repo`, returning what it did.
///
/// The reporting half of [`fold`], factored out so `thruline run` can fold its
/// own session without the CLI's exit-code contract: that command's `$?`
/// belongs to the child it ran.
pub fn fold_report(repo: &std::path::Path) -> Result<thruline_capture::fold::FoldReport> {
    let spools: Vec<PathBuf> = thruline_capture::harness::ALL
        .iter()
        .filter_map(thruline_capture::harness::default_spool)
        .filter(|p| p.is_file())
        .collect();
    let db = Ledger::default_path().context("locating the execution store")?;
    let ledger = Ledger::open(&db).with_context(|| format!("opening {}", db.display()))?;
    let key = repo.to_string_lossy().to_string();
    let repo_id = ledger
        .repo(&key, Some(&key))
        .context("registering the repository")?;
    let mut merged = thruline_capture::fold::FoldReport::default();
    for spool in &spools {
        let next = thruline_capture::fold::fold(spool, &ledger, &repo_id, Some(repo))
            .with_context(|| format!("folding {}", spool.display()))?;
        merged.merge(next);
    }
    Ok(merged)
}

pub fn fold(args: FoldArgs) -> Result<ExitCode> {
    // BEFORE the spool is discovered, not after. The bridge may create the
    // spool on a machine that has only ever run the sensor pack, and discovery
    // below filters on `is_file()` — so a bridge that ran second would write a
    // file this run then declined to read. Reported separately from the fold's
    // own counters, so "the sensor held 400 deliveries the spool was missing"
    // and "the fold wrote 400 events" stay two distinct facts.
    //
    // The target is the named spool when one was named, and the Claude Code
    // spool otherwise: the sensor pack is Claude Code only, and appending its
    // deliveries to some other harness's file would mislabel every one.
    let bridged = if args.from_sensor {
        let target = match args.spool.clone() {
            Some(p) => p,
            None => thruline_capture::claude_code::default_spool()
                .context("locating the Claude Code spool; set THRULINE_HOME or HOME")?,
        };
        Some(
            crate::sensor_bridge::bridge(&args.sensor_db, &target)
                .context("bringing the spool level with the sensor")?,
        )
    } else {
        None
    };

    // Every harness spools to its own file, and every one of them folds into
    // the same chain. Naming one explicitly folds only that one; naming none
    // folds all that exist, because a session captured through Codex is not
    // less of a session than one captured through Claude Code.
    let spools: Vec<PathBuf> = match args.spool {
        Some(p) => vec![p],
        None => {
            let all: Vec<PathBuf> = thruline_capture::harness::ALL
                .iter()
                .filter_map(thruline_capture::harness::default_spool)
                .filter(|p| p.is_file())
                .collect();
            if all.is_empty() {
                // Keep naming the Claude Code path when nothing exists yet, so
                // the error says where the file was expected.
                vec![thruline_capture::claude_code::default_spool()
                    .context("locating the spool; set THRULINE_HOME or HOME")?]
            } else {
                all
            }
        }
    };
    let spool = spools[0].clone();

    let db = match args.exec_db {
        Some(p) => p,
        None => Ledger::default_path().context("locating the execution store")?,
    };
    let ledger = Ledger::open(&db).with_context(|| format!("opening {}", db.display()))?;
    let repo_root = args.repo.or_else(|| std::env::current_dir().ok());
    let key = repo_root
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let repo = ledger
        .repo(
            &key,
            repo_root
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .as_deref(),
        )
        .context("registering the repository")?;

    let mut report = thruline_capture::fold::fold(&spool, &ledger, &repo, repo_root.as_deref())
        .context("folding the spool")?;
    for extra in spools.iter().skip(1) {
        let next = thruline_capture::fold::fold(extra, &ledger, &repo, repo_root.as_deref())
            .with_context(|| format!("folding {}", extra.display()))?;
        report.merge(next);
    }

    // Everything that could not be done is reported, including the things that
    // are easy to leave out because they are not errors: a snapshot that was
    // due and failed, a check whose output could not be read, a quarantine file
    // that could not be written.
    let ok = report.snapshots_failed.is_empty()
        && report.quarantined.is_empty()
        && report.quarantine_write_error.is_none()
        && report.checks_failed.is_empty()
        && report.sessions_in_multiple_spools.is_empty();
    // Absent, not zeroed, when the bridge did not run: `"bridge": null` says
    // "not asked for", and a zeroed object would say "asked for, found nothing".
    let bridge_json = match &bridged {
        Some(b) => serde_json::json!({
            "deliveries_read": b.deliveries_read,
            "lines_appended": b.lines_appended,
            "unparseable_payloads": b.unparseable_payloads,
            "without_session": b.without_session,
            "watermark_before": b.watermark_before,
            "watermark_after": b.watermark_after,
            "watermark_write_error": b.watermark_write_error,
        }),
        None => serde_json::Value::Null,
    };
    if args.json {
        crate::json_out::print(
            &crate::json_out::FOLD,
            serde_json::json!({
                "ok": ok,
                "bridge": bridge_json,
                "lines_read": report.lines_read,
                "events_written": report.events_written,
                "events_skipped": report.events_skipped,
                "sessions": report.sessions,
                "snapshots_taken": report.snapshots_taken,
                "snapshots_failed": report.snapshots_failed,
                "exec_events": report.exec_events,
                "checks_written": report.checks_written,
                "checks_failed": report.checks_failed,
                "quarantined": report.quarantined,
                "quarantine_write_error": report.quarantine_write_error,
                "sessions_in_multiple_spools": report.sessions_in_multiple_spools,
            }),
        )?;
    } else {
        if let Some(b) = &bridged {
            println!(
                "bridge: {} sensor deliver(ies) read · {} appended to the spool · sensor seq {} → {}",
                b.deliveries_read, b.lines_appended, b.watermark_before, b.watermark_after
            );
            if b.unparseable_payloads > 0 {
                println!(
                    "HELD {} sensor payload(s) are not JSON; emitted with parse_ok=false and a \
                     null payload. The bytes remain in `sensor_events`, which is append-only.",
                    b.unparseable_payloads
                );
            }
            if b.without_session > 0 {
                println!(
                    "HELD {} sensor deliver(ies) carry no session_id and belong to no chain; \
                     the fold below quarantines them by line.",
                    b.without_session
                );
            }
            if let Some(why) = &b.watermark_write_error {
                println!(
                    "WARN the sensor watermark sidecar could not be written: {why}. The append \
                     succeeded and the next run recovers the position from the spool itself."
                );
            }
        }
        println!(
            "{} line(s) read · {} event(s) written · {} skipped · {} session(s)",
            report.lines_read, report.events_written, report.events_skipped, report.sessions
        );
        println!(
            "{} snapshot(s) · {} exec event(s) · {} check(s)",
            report.snapshots_taken, report.exec_events, report.checks_written
        );
        for (seq, why) in &report.snapshots_failed {
            println!("FAIL snapshot at spool line {seq}: {why}");
        }
        for (id, why) in &report.checks_failed {
            println!("FAIL check for {id}: {why}");
        }
        for (seq, why) in &report.quarantined {
            println!("HELD spool line {seq}: {why}");
        }
        for session in &report.sessions_in_multiple_spools {
            println!(
                "FAIL session {session} appears in more than one harness spool. `seq` is a \
                 line's ordinal in its own spool, so the second one folded hits the resume \
                 guard and its deliveries are counted as already present. Install the shim \
                 with `--harness <name>` so one session's lines share one spool."
            );
        }
        if let Some(why) = &report.quarantine_write_error {
            println!("FAIL the quarantine file could not be written: {why}");
        }
    }
    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

/// The installed shim's manifest, for `doctor`.
pub fn manifest() -> Option<ShimManifest> {
    manifest_at(&thruline_home().ok()?.join("shim"))
}

/// The manifest in a named shim directory, for callers that were given one.
pub fn manifest_at(dir: &std::path::Path) -> Option<ShimManifest> {
    ShimManifest::load(dir).ok()
}

/// Which session the recorder would attribute an exec to right now, and how.
/// Exposed for `doctor`, which reports it because a wrong attribution is not a
/// small error.
pub fn session_binding_hint(manifest: &ShimManifest) -> (Option<String>, SessionBinding) {
    if let Ok(id) = std::env::var(exec::SESSION_VAR) {
        if !id.is_empty() {
            return (Some(id), SessionBinding::Env);
        }
    }
    if let Ok(id) = std::env::var(exec::HARNESS_SESSION_VAR) {
        if !id.is_empty() {
            return (Some(id), SessionBinding::HarnessEnv);
        }
    }
    if let Some(id) = manifest.session_id.clone() {
        return (Some(id), SessionBinding::Manifest);
    }
    match exec::latest_spool_session(&manifest.spool) {
        Some(id) => (Some(id), SessionBinding::SpoolLatest),
        None => (None, SessionBinding::Unbound),
    }
}
