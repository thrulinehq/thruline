//! BUILD-SPEC §4.1 / §14 — installing the capture hook pack.
//!
//! # It refuses rather than appends
//!
//! Three hook packs can already be present on one machine: the legacy sensor
//! pack, the speaker pack, and this one. They are not interchangeable, and a
//! second pack silently appended alongside a first produces **two recorders
//! writing two records of the same session**, with nothing saying which one a
//! later claim was checked against. That is worse than not installing.
//!
//! So the default is refusal with an exit code, naming what it found. `--undo`
//! is the way forward, and `--undo` removes **exactly** what this wrote —
//! matched on the command string — so a co-installed pack belonging to someone
//! else survives it. Install and undo are symmetric by construction: the same
//! predicate decides what belongs to us in both directions.
//!
//! # A new subcommand rather than a change to `init` — and what that cost
//!
//! `thruline init` already exists and installs the legacy sensor + speaker
//! packs; BUILD-PLAN C.5 records the collision between it and §14's `init` as an
//! open reconciliation. Overloading it would have decided that reconciliation by
//! accident, so this shipped the §4.1 behaviour under its own name.
//!
//! **The bill came due at M3.4.** `--gate` lived only here, and here refuses on
//! a machine `init` has already set up — so on the documented install path the
//! Stop gate could not be turned on at all, except through `init --undo`, which
//! also removes the speakers and the MCP server. The gate and the truth loop
//! were mutually exclusive by install path. Run the other way round, `init`
//! appended without looking and produced **four `Stop` entries and two
//! recorders**.
//!
//! Resolved, deliberately and in the only direction that cannot raise a verdict:
//! §14's own spelling, `thruline init --gate`, installs the gate beside whatever
//! pack is already there — the gate appends no capture rows, so it collides with
//! nothing — and [`refuse_double_recorder`] makes the two-recorder state
//! impossible to write from **any** path, this one included.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use thruline_capture::harness::Harness;

use crate::doctor::settings_paths;

pub struct Args {
    pub harness: String,
    pub scope: String,
    pub settings: Option<PathBuf>,
    pub managed_settings_snippet: bool,
    /// §4.1's opt-in Stop gate. Adds one `Stop` entry running
    /// `thruline gate --hook stop` **beside** the capture pack, so the capture
    /// hook's 50 ms budget is untouched and the gate gets its own **10 s** one.
    /// This comment said 5 s while `gate_cmd::hook_entry` asked for 10; the
    /// number lives in the entry, and `the_gate_rides_an_uncapped_event` reads
    /// it from there rather than restating it.
    pub gate: bool,
    pub undo: bool,
    pub json: bool,
}

/// POSIX single-quoting for a path that is about to be spliced into a hook
/// command string.
///
/// **Refuses rather than emitting an ambiguous command.** A path containing a
/// single quote cannot be single-quoted without splicing shell syntax, and the
/// harness runs this string through a shell. A mangled `--spool` argument does
/// not fail loudly: the hook's own contract is to exit 0 in silence whatever
/// happens, so the only symptom would be an empty spool and an install that
/// said "Installed 9 hooks".
fn quote_for_shell(path: &std::path::Path) -> Result<String> {
    let text = path
        .to_str()
        .with_context(|| format!("spool path {} is not valid UTF-8", path.display()))?;
    if text.contains('\'') {
        anyhow::bail!(
            "refusing to install: the spool path contains a single quote, which cannot be \
             quoted into the hook command the harness runs through a shell:\n    {text}\n  \
             The hook always exits 0 in silence, so a mangled argument here would show up \
             only as an empty spool. Move the spool, or pass THRULINE_HOOK_SPOOL."
        );
    }
    // Unconditional quoting, not "quote when it looks like it needs it": the
    // set of characters a shell treats specially is longer than anyone
    // remembers, and a space in `$HOME` is the common case that would silently
    // split the argument in two.
    Ok(format!("'{text}'"))
}

/// The command every hook a **local** install writes invokes.
///
/// **It states `--spool` explicitly, and it names this binary by absolute
/// path.** Both are pins onto *this machine*, and both are right here because
/// the machine that runs `hook-install` is the machine the hook fires on. The
/// command used to end at the harness name, which left the record's location to
/// be resolved by the hook when it fired, out of whatever environment the agent
/// had. See `harness::resolve_spool` for the measured cost of that: the pilot's
/// hook and its exec shim wrote two different files, so no chain ever contained
/// both a `Stop` event and the execs it was meant to bound.
///
/// A fleet-pushed managed snippet has neither property, so it does **not** go
/// through here — see [`managed_hook_command`].
fn hook_command(pack: &Harness, spool: Option<&std::path::Path>) -> Result<String> {
    // The absolute path of the running binary, so the hook keeps working when
    // the agent's PATH differs from the installing shell's — which it usually
    // does.
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "thruline".to_string());
    let base = format!("{exe} hook {}", pack.name);
    match spool {
        Some(spool) => Ok(format!("{base} --spool {}", quote_for_shell(spool)?)),
        // Neither THRULINE_HOOK_SPOOL nor THRULINE_HOME nor HOME. The hook will
        // resolve nothing either and drop the line; saying so is the honest
        // move, but refusing the install is not — `--undo` must still work on a
        // machine in this state.
        None => Ok(base),
    }
}

/// Where Codex reads its pack from: `$CODEX_HOME/hooks.json`, else
/// `~/.codex/hooks.json`.
///
/// Refuses `--scope project`. Codex documents no per-repository hooks file, so
/// writing `.codex/hooks.json` into a repo would succeed, print a count, and be
/// read by nothing — an unreachable success path is a future loosening.
fn codex_hooks_path(scope: &str) -> Result<PathBuf> {
    if scope != "user" {
        anyhow::bail!(
            "codex hooks are user-scoped: Codex reads $CODEX_HOME/{}, and documents no \
             per-project hooks file, so `--scope {scope}` would write a file nothing reads",
            thruline_capture::codex::HOOKS_FILE
        );
    }
    if let Some(dir) = std::env::var_os("CODEX_HOME") {
        return Ok(PathBuf::from(dir).join(thruline_capture::codex::HOOKS_FILE));
    }
    let home = std::env::var_os("HOME").context("neither CODEX_HOME nor HOME is set")?;
    Ok(thruline_capture::codex::hooks_path(&PathBuf::from(home)))
}

/// One capture-hook entry.
///
/// Whether it states a `timeout` is the harness table's call, not this
/// function's: see `harness::Harness::timeout_for` and `capture::codex`
/// CONFLICT-7.
fn capture_entry(pack: &Harness, event: &str, command: &str) -> Value {
    match pack.timeout_for {
        Some(secs) => json!({ "hooks": [{
            "type": "command",
            "command": command,
            "timeout": secs(event),
        }]}),
        None => json!({ "hooks": [{ "type": "command", "command": command }] }),
    }
}

/// Does this hook entry belong to the capture pack?
///
/// The single predicate both install and undo consult, so "what we own" cannot
/// mean two different things in the two directions.
fn is_ours(entry: &Value) -> bool {
    let text = entry.to_string();
    // Every harness this installer writes, or `--undo` on a Codex machine
    // silently leaves the pack it just installed in place and reports zero.
    text.contains("hook claude-code")
        || text.contains("hook codex")
        || text.contains("gate --hook stop")
}

/// Any ThruLine-looking hook, ours or another pack's.
fn is_any_thruline(entry: &Value) -> bool {
    entry.to_string().contains("thruline")
}

/// Every undo command needed to clear the ThruLine hooks in this settings map.
fn undo_commands(hooks: &serde_json::Map<String, Value>) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = hooks
        .values()
        .filter_map(Value::as_array)
        .flatten()
        .filter(|e| is_any_thruline(e))
        .map(undo_for)
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Which command removes the pack an entry belongs to.
///
/// **The refusal used to print one remedy for every case, and it was the wrong
/// one half the time.** `--undo` matches on [`is_ours`], so on a machine set up
/// with `thruline init` it removed **0** entries, printed `Removed 0 ThruLine
/// capture hooks`, exited 0, and the retry refused identically — a loop with no
/// exit the product named. Measured at `58e7c2a`.
fn undo_for(entry: &Value) -> &'static str {
    if is_ours(entry) {
        "thruline hook-install --undo"
    } else {
        // `sensor append`, `brief`, `audit`, `digest precompute` — everything
        // `thruline init` writes.
        "thruline init --undo"
    }
}

pub fn run(args: Args) -> Result<ExitCode> {
    let pack = match thruline_capture::harness::by_name(&args.harness) {
        // Two different failures, said differently. Collapsing them would tell
        // someone who typed `gemini` that no such harness exists, when what is
        // true is that its registration file is not written yet.
        None => anyhow::bail!(
            "unknown harness `{}`: known harnesses are {}",
            args.harness,
            thruline_capture::harness::ALL
                .iter()
                .map(|h| h.name)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Some(h) if !h.installable => anyhow::bail!(
            "no hook pack for `{}` yet: its event contract is mapped, but the installer cannot \
             write its registration file",
            h.name
        ),
        Some(h) => h,
    };
    if args.managed_settings_snippet {
        if pack.name != "claude-code" {
            // Managed settings are a Claude Code enterprise mechanism. Printing
            // a Codex-shaped snippet under that flag would offer an
            // organisation an enforcement path that does not exist.
            anyhow::bail!(
                "--managed-settings-snippet is Claude Code only: Codex has no managed-settings \
                 mechanism to enforce a pack through"
            );
        }
        println!("{}", serde_json::to_string_pretty(&managed_snippet()?)?);
        // stdout stays pure JSON — this is piped into a settings file. The two
        // things the snippet deliberately does not know go to stderr, because
        // a fleet operator who does not know them gets silence, not an error:
        // the hook exits 0 whatever happens.
        eprintln!();
        eprintln!(
            "This snippet is machine-independent BY OMISSION, and the two omissions are yours:"
        );
        eprintln!();
        eprintln!("  1. It invokes a bare `thruline`, so the binary must be on the PATH of the");
        eprintln!("     shell your agents run in. It cannot name this machine's copy: an");
        eprintln!("     absolute path here is a path on THIS machine asserted about every");
        eprintln!("     machine you push it to.");
        eprintln!("  2. It states no --spool, so each machine resolves its own from");
        eprintln!("     THRULINE_HOOK_SPOOL, else $THRULINE_HOME/spool, else ~/.thruline/spool.");
        eprintln!("     A pinned spool would send every machine's record to THIS machine's home");
        eprintln!("     directory — measured: a snippet made under one HOME and run under");
        eprintln!("     another wrote the second user's session into the first user's spool,");
        eprintln!("     exit 0, no output.");
        eprintln!();
        eprintln!("This snippet is a RECORDER pack, and hook entries MERGE across settings");
        eprintln!("levels rather than replacing each other. On a machine that also runs");
        eprintln!("`thruline init` or `thruline hook-install`, that is two recorders writing");
        eprintln!("two records of one session — so push this OR install locally, not both.");
        eprintln!("`disableAllHooks` cannot switch a managed pack off. `thruline doctor` now");
        eprintln!("reads managed settings and reports the collision, and a local install");
        eprintln!("refuses rather than creating it.");
        eprintln!();
        eprintln!("`thruline doctor` on a target machine reports which spool it resolved.");
        return Ok(ExitCode::SUCCESS);
    }

    let path = match args.settings {
        Some(p) => p,
        None if pack.name == "codex" => codex_hooks_path(&args.scope)?,
        None => settings_paths()
            .into_iter()
            .find(|(scope, _)| *scope == args.scope)
            .map(|(_, p)| p)
            .context("scope must be `user` or `project`")?,
    };

    let mut root: Value = match std::fs::read_to_string(&path) {
        Ok(text) if !text.trim().is_empty() => serde_json::from_str(&text)
            .with_context(|| format!("{} is not valid JSON", path.display()))?,
        _ => json!({}),
    };
    let hooks = root
        .as_object_mut()
        .context("settings root is not an object")?
        .entry("hooks")
        .or_insert_with(|| json!({}));
    let hooks = hooks.as_object_mut().context("`hooks` is not an object")?;

    if args.undo {
        let mut removed = 0usize;
        for (_event, entries) in hooks.iter_mut() {
            if let Some(list) = entries.as_array_mut() {
                let before = list.len();
                list.retain(|e| !is_ours(e));
                removed += before - list.len();
            }
        }
        hooks.retain(|_, v| !v.as_array().is_some_and(|a| a.is_empty()));
        // What is STILL there, and what would remove it. Removing nothing and
        // saying "Removed 0" is how the refusal below became a loop.
        let left = undo_commands(hooks);
        write_settings(&path, &root)?;
        report(args.json, true, removed, &path, &[], None);
        if !left.is_empty() && !args.json {
            println!(
                "{} ThruLine hooks that this command does not own are still in {}.",
                if removed == 0 { "The" } else { "Other" },
                path.display()
            );
            for undo in &left {
                println!("  remove them with `{undo}`");
            }
        }
        return Ok(ExitCode::SUCCESS);
    }

    // Refuse rather than append.
    let existing: Vec<String> = hooks
        .iter()
        .filter(|(_, v)| v.as_array().is_some_and(|a| a.iter().any(is_any_thruline)))
        .map(|(k, _)| k.clone())
        .collect();
    if !existing.is_empty() {
        // The remedy is a function of WHICH pack is in the way. See `undo_for`:
        // pointing an `init` user at `hook-install --undo` removed nothing and
        // sent them back to this same refusal.
        let remedies = undo_commands(hooks);
        if args.json {
            crate::json_out::print(
                &crate::json_out::HOOK_INSTALL_REFUSED,
                json!({
                    "installed": false,
                    "reason": "existing ThruLine hooks found",
                    "events": existing,
                    "settings": path.display().to_string(),
                    "undo_with": remedies,
                }),
            )?;
        } else {
            println!(
                "Refusing to install: {} already has ThruLine hooks on:",
                path.display()
            );
            for e in &existing {
                println!("  {e}");
            }
            println!();
            println!("Two recorders writing two records of one session is worse than none,");
            println!("because nothing afterwards says which record a claim was checked against.");
            println!("Remove what is there first, then install again:");
            for undo in &remedies {
                println!("  {undo}");
            }
        }
        return Ok(ExitCode::from(1));
    }

    let spool = thruline_capture::harness::resolve_spool(&pack);
    let command = hook_command(&pack, spool.as_deref())?;
    // APPEND to each event's list; never replace it. The first version
    // `insert`ed a fresh array and destroyed a co-installed foreign hook on the
    // same event — caught by the install/undo symmetry test, and exactly the
    // harm the refusal above exists to prevent, committed by the installer.
    let mut push =
        |event: &str, entry: Value| match hooks.get_mut(event).and_then(Value::as_array_mut) {
            Some(list) => list.push(entry),
            None => {
                hooks.insert(event.to_string(), json!([entry]));
            }
        };
    let mut installed: Vec<String> = pack
        .subscribed
        .iter()
        .map(|event| {
            push(event, capture_entry(&pack, event, &command));
            (*event).to_string()
        })
        .collect();
    if args.gate {
        // A SECOND `Stop` entry, deliberately. The capture hook must stay
        // silent and fast; the gate must be able to speak and take seconds.
        // One command doing both would have to hold the stricter budget and the
        // louder contract at once.
        const GATE_EVENT: &str = "Stop";
        // A mechanical control, not a comment. The gate asks for 10 s; Codex
        // caps `SessionEnd` at 3 s and would truncate that silently, so a gate
        // on a capped event is a gate that dies mid-sentence and reports
        // nothing. `Stop` is uncapped on both harnesses today — this refuses
        // rather than trusting that it stays true.
        if thruline_capture::codex::CAPPED_EVENTS.contains(&GATE_EVENT) {
            anyhow::bail!(
                "refusing --gate: `{GATE_EVENT}` is capped at {} s by the harness, below the \
                 gate's own budget",
                thruline_capture::codex::CAPPED_TIMEOUT_MAX_SECS
            );
        }
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("thruline"));
        push(GATE_EVENT, crate::gate_cmd::hook_entry(&exe));
        installed.push("Stop (gate)".to_string());
    }
    write_settings(&path, &root)?;
    let spool = spool.map(|p| p.display().to_string());
    report(
        args.json,
        false,
        installed.len(),
        &path,
        &installed,
        spool.as_deref(),
    );
    if pack.name == "codex" && !args.json {
        trust_notice();
    }
    Ok(ExitCode::SUCCESS)
}

/// Every ThruLine **recorder** command registered in this settings value, by
/// event.
///
/// "Recorder" is [`crate::doctor::is_recorder`] and nothing else. One reading
/// of the word, shared by the detector that reports the collision and the guard
/// that prevents it — two readings is how the two states `doctor` exists to
/// make visible were both invisible for three commits.
fn recorders_by_event(root: &Value) -> std::collections::BTreeMap<String, Vec<String>> {
    root.get("hooks")
        .and_then(Value::as_object)
        .map(|hooks| {
            hooks
                .iter()
                .map(|(event, entries)| {
                    let commands: Vec<String> = crate::doctor::thruline_commands_on(entries)
                        .into_iter()
                        .filter(|c| crate::doctor::is_recorder(c))
                        .collect();
                    (event.clone(), commands)
                })
                .filter(|(_, c)| !c.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// **The guard that makes §14's collision impossible to write, from any path.**
///
/// Two recorders on one event means two records of one session, with nothing
/// afterwards saying which one a claim was checked against. `hook-install`
/// refuses at its own door; `init` did not, so `hook-install --gate` followed by
/// `init` produced **four `Stop` entries and two recorders** — measured, and
/// `doctor` reported it as a FAIL *after the fact*. This refuses it before the
/// bytes land, and it sits at the write, so a future install path inherits it
/// without having to remember it.
///
/// # It refuses a write that CREATES or KEEPS a collision, never one that reduces it
///
/// An unconditional "no file may have two recorders" would strand every machine
/// already in that state: `init --undo` is precisely the write that has to get
/// through, and the file it writes still contains the *other* pack. So the
/// comparison is against what is on disk, and a write that lowers an event's
/// recorder count is always allowed.
///
/// # And it counts over the MERGED view, because that is the one that runs
///
/// This read `before` from `std::fs::read_to_string(path)` — the single file it
/// was about to write — and it was wrong for the same reason `doctor` was.
/// BUILD-PLAN B.1: *"Hook entries merge across settings levels rather than
/// replacing each other."* Measured on the documented flags, both exiting 0:
/// `thruline init --yes` (user settings) then
/// `thruline hook-install --gate --scope project` (project settings) →
/// **`Stop` with four entries and two recorders**, plus six other events at two
/// recorders each. Neither file held two, so a per-file guard had nothing to
/// refuse.
pub(crate) fn refuse_double_recorder(path: &std::path::Path, after: &Value) -> Result<()> {
    refuse_double_recorder_across(&crate::doctor::merged_settings_paths(), path, after)
}

/// The guard over an explicit list of merged settings levels, so it can be
/// tested without the machine's real `~/.claude` or `/Library`.
///
/// **Peers apply only when the target is itself one of the levels.** A path
/// passed with `--settings` that is not a level Claude Code reads is not part of
/// any merge, so nothing merges with it and the single-file comparison is the
/// whole truth about it. Comparison is on the canonicalised path, so a symlink
/// into `~/.claude` is still recognised as the user level.
pub(crate) fn refuse_double_recorder_across(
    levels: &[(&'static str, PathBuf)],
    path: &std::path::Path,
    after: &Value,
) -> Result<()> {
    let target = same_file_key(path);
    let target_is_a_merged_level = levels.iter().any(|(_, p)| same_file_key(p) == target);
    // Every recorder the harness would merge in from a file this write does not
    // touch, by event, carrying the scope it came from so the refusal can say
    // where to go and look.
    let mut elsewhere: std::collections::BTreeMap<String, Vec<(&'static str, String)>> =
        std::collections::BTreeMap::new();
    if target_is_a_merged_level {
        for (scope, peer) in levels {
            if same_file_key(peer) == target {
                continue;
            }
            for (event, commands) in recorders_by_event(&settings_value(peer)) {
                let slot = elsewhere.entry(event).or_default();
                slot.extend(commands.into_iter().map(|c| (*scope, c)));
            }
        }
    }

    let before = recorders_by_event(&settings_value(path));
    for (event, commands) in recorders_by_event(after) {
        let merged_in = elsewhere.get(&event).map_or(0, Vec::len);
        let was = before.get(&event).map_or(0, Vec::len) + merged_in;
        let now = commands.len() + merged_in;
        if now < 2 || now < was {
            continue;
        }
        let mut listed = commands
            .iter()
            .map(|c| format!("\n      {c}\n        (this file)"))
            .collect::<String>();
        let mut managed = false;
        for (scope, command) in elsewhere.get(&event).map(Vec::as_slice).unwrap_or_default() {
            managed |= scope.starts_with("managed");
            listed.push_str(&format!("\n      {command}\n        ({scope} settings)"));
        }
        anyhow::bail!(
            "refusing to write {}: this would put {now} ThruLine RECORDERS on `{event}` in \
             the settings Claude Code merges:{listed}\n  \
             Hook entries MERGE across settings levels rather than replacing each other, so \
             every pack listed above is live at once. Two recorders write two records of one \
             session, and nothing afterwards says which record a claim was checked against — \
             worse than not installing at all.\n  \
             Remove the pack that is already there first (`thruline init --undo` for the \
             sensor and speaker packs, `thruline hook-install --undo` for the §4.1 capture \
             pack; add `--scope project` to reach a project-level pack), or install the Stop \
             gate onto the pack you already have with `thruline init --gate`, which adds no \
             second recorder.{}",
            path.display(),
            if managed {
                "\n  One of them is a MANAGED pack. It cannot be removed from this machine, \
                 and `disableAllHooks` cannot switch it off — it is already recording, so a \
                 second local pack would only duplicate it."
            } else {
                ""
            }
        );
    }
    Ok(())
}

/// A settings file's JSON, or an empty object when it is absent or unreadable.
///
/// Unreadable reads as empty on purpose: this guard's job is to refuse a write
/// it can prove is a collision, and a file it cannot parse proves nothing. The
/// install path itself refuses malformed JSON with a message that names it.
fn settings_value(path: &std::path::Path) -> Value {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .unwrap_or_else(|| json!({}))
}

/// A key two paths share exactly when they name the same file.
///
/// The file itself may not exist yet — this guard runs *before* the write — so
/// the parent directory is canonicalised and the file name appended, rather
/// than canonicalising the whole path.
fn same_file_key(path: &std::path::Path) -> PathBuf {
    let parent = path.parent().unwrap_or(std::path::Path::new("."));
    match (std::fs::canonicalize(parent), path.file_name()) {
        (Ok(dir), Some(name)) => dir.join(name),
        _ => path.to_path_buf(),
    }
}

fn write_settings(path: &std::path::Path, value: &Value) -> Result<()> {
    refuse_double_recorder(path, value)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn report(
    json_out: bool,
    undo: bool,
    count: usize,
    path: &std::path::Path,
    events: &[String],
    spool: Option<&str>,
) {
    if json_out {
        // `spool` is in the machine output because the pilot has to be able to
        // assert that the hook and the exec shim were pointed at ONE file. A
        // fact that only a human can read is a fact nothing can check.
        let value = json!({
            "undo": undo,
            "count": count,
            "settings": path.display().to_string(),
            "events": events,
            "spool": spool,
        });
        // A render failure here must not degrade to a silent empty line: a
        // consumer reading stdout would see a successful, contentless install.
        if let Err(e) = crate::json_out::print_compact(&crate::json_out::HOOK_INSTALL, value) {
            eprintln!("thruline: could not render the hook-install result: {e}");
        }
        return;
    }
    if undo {
        println!(
            "Removed {count} ThruLine capture hooks from {}",
            path.display()
        );
        return;
    }
    println!("Installed {count} hooks in {}", path.display());
    match spool {
        Some(s) => println!("Every one of them appends to {s}"),
        None => println!(
            "WARNING: no spool could be resolved (no THRULINE_HOOK_SPOOL, THRULINE_HOME or \
             HOME), so these hooks will record NOTHING."
        ),
    }
    println!();
    println!("What happens next:");
    println!("  1. Your agent's hooks now append one redacted line per event to the spool.");
    println!(
        "  2. Nothing leaves this machine, and secrets are masked before anything is written."
    );
    println!("  3. Snapshotting writes loose git objects into your repo; `thruline doctor`");
    println!("     reports the footprint and `git gc --prune=now` reclaims them.");
}

/// **The half of the install that is not a file write.**
///
/// Codex records hook trust against the definition's *hash*, so a pack this
/// command has just written is new by construction and every hook in it is
/// skipped until reviewed. Printing "Installed 9 hooks" and stopping there is
/// true and useless: it is the exact sentence an operator would quote while
/// wondering why the spool is empty. See `capture::codex` CONFLICT-8.
fn trust_notice() {
    println!();
    println!("BUT THEY WILL NOT RUN YET.");
    println!();
    println!("Codex records hook trust against the hook definition's hash, and this pack");
    println!("is new — so Codex marks every entry above for review and SKIPS it. Nothing");
    println!("will be captured, and nothing will say so: an untrusted pack and a working");
    println!("pack look identical from here.");
    println!();
    println!("Two ways forward for you:");
    println!(
        "  1. Run `{}` inside Codex and trust this pack — on your own machine.",
        thruline_capture::codex::TRUST_REVIEW_COMMAND
    );
    println!(
        "  2. Pass `{}` per invocation, for automation",
        thruline_capture::codex::TRUST_BYPASS_FLAG
    );
    println!("     that has already vetted the hook source. It skips the check for every");
    println!("     enabled hook, which is why the vendor named it `dangerously`.");
    println!();
    println!("(There is a third route that is not yours: the trust requirement is scoped to");
    println!("NON-MANAGED hooks, so an admin-managed configuration is exempt. Unavailable to");
    println!("an individual user, but it exists — this said \"the only two\" and was wrong.)");
}

/// The command the §4.1 managed snippet carries. **No path from this machine
/// appears in it.**
///
/// A managed-settings snippet is generated once and pushed to a fleet, so every
/// absolute path baked into it is a path on the *generating* machine asserted
/// about *every other* machine. [`hook_command`] bakes two of them — the
/// running binary and the resolved spool — and both are wrong the moment the
/// file is copied:
///
/// * **The spool.** Demonstrated before this function existed: a snippet
///   generated under `HOME=alice` and run under `HOME=bob` appended bob's
///   session line to **alice's** spool, and bob's spool directory was never
///   created. Nothing said so — `harness::run`'s contract is to exit 0 in
///   silence, and its own fallback fires only when no `--spool` was given at
///   all, never when the given one points somewhere wrong. Omitting `--spool`
///   hands the question to `harness::resolve_spool` on the machine that fires.
/// * **The binary.** `/Users/alice/.cargo/bin/thruline` is not where bob's
///   copy lives. A bare `thruline` is resolved from the agent's own `PATH`,
///   which is the only name that can be true fleet-wide; the deployment that
///   pushes the snippet is the thing that knows where it put the binary, and
///   `--managed-settings-snippet` says so on stderr.
fn managed_hook_command(pack: &Harness) -> String {
    format!("thruline hook {}", pack.name)
}

/// The §4.1 managed-settings snippet an org can enforce.
fn managed_snippet() -> Result<Value> {
    // The snippet an org enforces carries the capture pack only: a gate an
    // organisation cannot see the reasons of is a gate its engineers will route
    // around. Enabling it is `--gate` on the machine, per §4.1's "opt-in".
    let pack = thruline_capture::harness::CLAUDE_CODE;
    let command = managed_hook_command(&pack);
    let mut hooks = serde_json::Map::new();
    for event in thruline_capture::claude_code::SUBSCRIBED_EVENTS {
        hooks.insert(
            (*event).to_string(),
            json!([{ "hooks": [{ "type": "command", "command": command }] }]),
        );
    }
    Ok(json!({ "hooks": hooks }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(path: &std::path::Path, undo: bool) -> Args {
        Args {
            harness: "claude-code".into(),
            scope: "user".into(),
            settings: Some(path.to_path_buf()),
            managed_settings_snippet: false,
            gate: false,
            undo,
            json: true,
        }
    }

    /// A settings file carrying one ThruLine recorder on `Stop`.
    fn recorder_file(dir: &std::path::Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(
            &path,
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"/x/thruline hook claude-code"}]}]}}"#,
        )
        .expect("write");
        path
    }

    /// The value a fresh install would write: one recorder on `Stop`.
    fn one_recorder() -> Value {
        json!({"hooks":{"Stop":[{"hooks":[
            {"type":"command","command":"/x/thruline sensor append Stop"}
        ]}]}})
    }

    /// **The blind spot the guard was written around.** A pack in a *different*
    /// settings level is merged by the harness, so installing one here makes two
    /// recorders even though neither file holds two.
    #[test]
    fn a_recorder_in_another_merged_level_makes_this_write_a_collision() {
        let dir = tempfile::tempdir().expect("tempdir");
        let user = dir.path().join("user-settings.json");
        let project = recorder_file(dir.path(), "project-settings.json");
        let levels = vec![("user", user.clone()), ("project", project)];

        let err = refuse_double_recorder_across(&levels, &user, &one_recorder())
            .expect_err("a second recorder in the merged view must be refused");
        let text = format!("{err}");
        assert!(text.contains("2 ThruLine RECORDERS"), "{text}");
        assert!(
            text.contains("project settings"),
            "the refusal must say WHERE the other recorder is: {text}"
        );
        assert!(
            text.contains("MERGE"),
            "the refusal must say why a second file matters: {text}"
        );
    }

    /// A managed pack cannot be removed from the machine, so the refusal has to
    /// say so instead of naming an `--undo` that will not help.
    #[test]
    fn a_managed_recorder_is_seen_and_the_refusal_says_it_cannot_be_removed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let user = dir.path().join("user-settings.json");
        let managed = recorder_file(dir.path(), "managed-settings.json");
        let levels = vec![("user", user.clone()), ("managed", managed)];

        let err = refuse_double_recorder_across(&levels, &user, &one_recorder())
            .expect_err("a managed recorder is still a recorder");
        let text = format!("{err}");
        assert!(text.contains("MANAGED"), "{text}");
        assert!(
            text.contains("disableAllHooks"),
            "the refusal must name the thing that cannot switch it off: {text}"
        );
    }

    /// A path Claude Code does not read is part of no merge, so nothing merges
    /// into it. Without this, every test that installs into a scratch file would
    /// start failing on the strength of the developer's own `~/.claude`.
    #[test]
    fn a_target_outside_the_merged_levels_is_judged_on_its_own_file_alone() {
        let dir = tempfile::tempdir().expect("tempdir");
        let elsewhere = dir.path().join("scratch-settings.json");
        let user = recorder_file(dir.path(), "user-settings.json");
        let levels = vec![("user", user)];

        refuse_double_recorder_across(&levels, &elsewhere, &one_recorder())
            .expect("a file the harness never reads cannot collide with one it does");
    }

    /// `--undo` is the write that gets a machine OUT of the collision, so the
    /// guard must let a write through when it lowers the merged count.
    #[test]
    fn a_write_that_lowers_the_merged_count_is_allowed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let user = recorder_file(dir.path(), "user-settings.json");
        let project = recorder_file(dir.path(), "project-settings.json");
        let levels = vec![("user", user.clone()), ("project", project)];

        // Merged before: 2. This write removes the one in `user`, leaving 1.
        refuse_double_recorder_across(&levels, &user, &json!({"hooks":{}}))
            .expect("undo must get through on a machine already carrying two recorders");
    }

    #[test]
    fn installing_over_an_existing_pack_refuses_rather_than_appending() {
        // Two recorders writing two records of one session is worse than none:
        // nothing afterwards says which record a claim was checked against.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"thruline audit --hook stop"}]}]}}"#,
        )
        .expect("write");
        let before = std::fs::read_to_string(&path).expect("read");

        let code = run(args(&path, false)).expect("runs");
        assert_eq!(
            format!("{code:?}"),
            format!("{:?}", ExitCode::from(1)),
            "installing over an existing pack did not refuse"
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            before,
            "the refusal still modified the settings file"
        );
    }

    #[test]
    fn the_refusal_names_the_undo_that_actually_removes_what_is_in_the_way() {
        // Measured at `58e7c2a`: on a machine set up with `thruline init`, this
        // command refused, told the user to run `thruline hook-install --undo`,
        // and that command removed **0** entries and exited 0. Re-running
        // produced the identical refusal. The remedy the product printed was a
        // loop with no exit, because `--undo` owns only what `is_ours` matches.
        let init_pack: Value = serde_json::from_str(
            r#"{"Stop":[{"hooks":[{"type":"command","command":"/x/thruline sensor append Stop"}]},
                        {"hooks":[{"type":"command","command":"/x/thruline audit --hook stop"}]}],
                "SessionStart":[{"hooks":[{"type":"command","command":"/x/thruline brief --hook session-start"}]}]}"#,
        )
        .expect("json");
        assert_eq!(
            undo_commands(init_pack.as_object().expect("obj")),
            vec!["thruline init --undo"],
            "the refusal would send an `init` user to a command that removes none of this"
        );

        // Its own pack, and both together.
        let ours: Value = serde_json::from_str(
            r#"{"Stop":[{"hooks":[{"type":"command","command":"/x/thruline hook claude-code --spool '/s'"}]},
                        {"hooks":[{"type":"command","command":"/x/thruline gate --hook stop"}]}]}"#,
        )
        .expect("json");
        assert_eq!(
            undo_commands(ours.as_object().expect("obj")),
            vec!["thruline hook-install --undo"]
        );
        let both: Value = serde_json::from_str(
            r#"{"Stop":[{"hooks":[{"type":"command","command":"/x/thruline hook claude-code"}]},
                        {"hooks":[{"type":"command","command":"/x/thruline sensor append Stop"}]}]}"#,
        )
        .expect("json");
        assert_eq!(
            undo_commands(both.as_object().expect("obj")),
            vec!["thruline hook-install --undo", "thruline init --undo"],
            "a machine carrying both packs needs both undos named"
        );
        // A foreign hook is nobody's business here.
        let foreign: Value =
            serde_json::from_str(r#"{"Stop":[{"hooks":[{"type":"command","command":"other"}]}]}"#)
                .expect("json");
        assert!(undo_commands(foreign.as_object().expect("obj")).is_empty());
    }

    #[test]
    fn undo_on_a_foreign_thruline_pack_removes_nothing_and_leaves_it_named() {
        // The other half of the loop: `--undo` must not report "Removed 0" and
        // stop. What is still there, and what removes it, is the whole answer.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"/x/thruline sensor append Stop"}]}]}}"#,
        )
        .expect("write");
        run(args(&path, true)).expect("undo");
        let after: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(
            undo_commands(after["hooks"].as_object().expect("hooks")),
            vec!["thruline init --undo"],
            "undo either removed a pack it does not own, or left nothing to name"
        );
    }

    #[test]
    fn install_then_undo_returns_the_file_to_its_prior_meaning() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        // A foreign hook that must survive both operations.
        std::fs::write(
            &path,
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"/usr/local/bin/other-tool"}]}]}}"#,
        )
        .expect("write");

        run(args(&path, false)).expect("install");
        let after: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        assert!(
            after["hooks"]["PostToolUseFailure"].is_array(),
            "not installed"
        );

        run(args(&path, true)).expect("undo");
        let undone: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        let stop = undone["hooks"]["Stop"].as_array().expect("Stop survives");
        assert_eq!(stop.len(), 1, "undo removed a hook that was not ours");
        assert!(stop[0].to_string().contains("other-tool"));
        assert!(
            undone["hooks"].get("PostToolUseFailure").is_none(),
            "undo left our hooks behind"
        );
    }

    #[test]
    fn the_installed_pack_carries_post_tool_use_failure() {
        // The subscription whose absence would bias the record toward success.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        run(args(&path, false)).expect("install");
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        for event in thruline_capture::claude_code::SUBSCRIBED_EVENTS {
            assert!(v["hooks"][event].is_array(), "{event} was not installed");
        }
        assert!(v["hooks"]["PostToolUseFailure"].is_array());
    }

    // -----------------------------------------------------------------------
    // RULING 4 — one spool, so one chain carries both the hook events and the
    // execs. The hook command used to end at the harness name.
    // -----------------------------------------------------------------------

    #[test]
    fn every_installed_hook_states_the_spool_it_writes() {
        // The defect this closes: `<bin> hook codex` with no `--spool` resolves
        // `$THRULINE_HOME/spool/codex.jsonl` WHEN IT FIRES, while the exec shim
        // was installed against `/thruline/spool/codex.jsonl`. Two files, two
        // chains, and no chain holding both a `Stop` and the execs it bounds —
        // so a phase boundary could not be evaluated at all.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        run(args(&path, false)).expect("install");
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");

        let pack = thruline_capture::harness::CLAUDE_CODE;
        let expected = thruline_capture::harness::resolve_spool(&pack).expect("a spool resolves");
        // Compared against the resolver the HOOK will use, not against a
        // literal: a test that restates the path would pass a build in which
        // the two disagreed.
        let needle = format!("--spool '{}'", expected.display());
        for event in pack.subscribed {
            let text = v["hooks"][event][0]["hooks"][0]["command"]
                .as_str()
                .unwrap_or_else(|| panic!("{event} has no command"))
                .to_string();
            assert!(text.contains(&needle), "{event}: {text}");
        }
    }

    #[test]
    fn the_json_report_names_the_spool_so_the_pilot_can_assert_on_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        // `report` is the only place the fact is published for a machine; if it
        // stops publishing it, the pilot's unification check goes blind.
        let pack = thruline_capture::harness::CLAUDE_CODE;
        let expected = thruline_capture::harness::resolve_spool(&pack).expect("a spool resolves");
        run(args(&path, false)).expect("install");
        let cmd = std::fs::read_to_string(&path).expect("read");
        assert!(cmd.contains(&expected.display().to_string()));
    }

    #[test]
    fn a_spool_path_that_cannot_be_quoted_is_refused_rather_than_mangled() {
        // OBSERVED REFUSING. The hook always exits 0 in silence, so a spool
        // argument split or re-quoted by the shell would present as an empty
        // spool under an install that printed "Installed 9 hooks".
        let err = quote_for_shell(std::path::Path::new("/tmp/it's/codex.jsonl"))
            .expect_err("a single quote must be refused");
        assert!(err.to_string().contains("single quote"), "{err}");

        // And the ordinary hazard — a space — is quoted, not refused.
        let ok = quote_for_shell(std::path::Path::new(
            "/Users/Ada Lovelace/.thruline/s.jsonl",
        ))
        .expect("a space is quotable");
        assert_eq!(ok, "'/Users/Ada Lovelace/.thruline/s.jsonl'");

        // The refusal reaches the command builder rather than being swallowed.
        let bad = hook_command(
            &thruline_capture::harness::CODEX,
            Some(std::path::Path::new("/tmp/it's/codex.jsonl")),
        )
        .expect_err("hook_command must propagate the refusal");
        assert!(bad.to_string().contains("single quote"), "{bad}");
    }

    #[test]
    fn undo_still_owns_a_pack_that_now_carries_a_spool_argument() {
        // `is_ours` matches on `hook <harness>`; appending `--spool …` after it
        // must not make the installer stop recognising its own entries, or
        // `--undo` would leave a live recorder behind and the next install
        // would refuse forever.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        run(args(&path, false)).expect("install");
        run(args(&path, true)).expect("undo");
        let undone: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        assert!(
            undone["hooks"].as_object().is_none_or(|h| h.is_empty()),
            "undo did not recognise the spool-bearing pack it wrote: {undone}"
        );
    }

    #[test]
    fn the_managed_snippet_subscribes_to_exactly_what_install_writes() {
        // An org enforcing the snippet must get the same subscriptions a local
        // install produces, or managed machines record a different population.
        // Compared on OUR entries only: a local install preserves whatever
        // foreign hooks the file already had, and the snippet cannot know about
        // those.
        //
        // **Subscriptions, not commands.** This compared the two whole `hooks`
        // objects, which forced them to be byte-identical — and a local
        // install's command is correctly full of THIS machine's absolute paths.
        // The equality therefore passed only because the snippet had been given
        // the same machine-local paths, which is exactly the defect: the same
        // assertion is green on a same-machine comparison whether the snippet is
        // portable or not. What the two must share is the event set.
        let snippet = managed_snippet().expect("snippet");
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        run(args(&path, false)).expect("install");
        let written: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        let events = |v: &Value| -> Vec<String> {
            let mut keys: Vec<String> = v["hooks"]
                .as_object()
                .expect("hooks object")
                .keys()
                .cloned()
                .collect();
            keys.sort();
            keys
        };
        assert_eq!(
            events(&snippet),
            events(&written),
            "the managed snippet and a local install subscribe to different events"
        );
    }

    #[test]
    fn the_managed_snippet_names_no_path_belonging_to_this_machine() {
        // The fleet property, checked where it is cheap. A snippet an
        // organisation pushes to N machines may not contain a path that is only
        // true on the one that generated it — see
        // `a_managed_snippet_generated_under_one_home_records_under_the_other`
        // in tests/m07_managed_snippet.rs for the same property proved by
        // actually running the thing under a second HOME.
        let snippet = managed_snippet().expect("snippet");
        let text = snippet.to_string();
        for machine_local in [
            std::env::current_exe()
                .expect("current exe")
                .display()
                .to_string(),
            std::env::var("HOME").unwrap_or_else(|_| "/nonexistent-home".into()),
        ] {
            assert!(
                !text.contains(&machine_local),
                "the managed snippet carries {machine_local}, which is a fact about \
                 this machine and about no other: {text}"
            );
        }
        assert!(
            !text.contains("--spool"),
            "the managed snippet pins a spool; every machine it is pushed to would \
             write into the generating machine's home: {text}"
        );
    }

    // -----------------------------------------------------------------------
    // Codex
    // -----------------------------------------------------------------------

    fn codex_args(path: &std::path::Path, undo: bool) -> Args {
        Args {
            harness: "codex".into(),
            ..args(path, undo)
        }
    }

    /// The real foreign entry found in `~/.codex/hooks.json` on the machine
    /// this adapter was built against. It is here, verbatim, because it very
    /// nearly collides with our own ownership predicate: it contains the
    /// substring `codex-hook`, and `is_ours` looks for `hook codex`.
    const FOREIGN: &str = r#"{"hooks":{"SessionStart":[{"matcher":"startup|resume|clear|compact","hooks":[{"type":"command","command":"/opt/market/market-mcp relay-codex-hook session-start","timeout":1}]}]}}"#;

    #[test]
    fn the_codex_pack_invokes_the_codex_hook_and_not_claude_codes() {
        // Pasting `hook claude-code` into the Codex pack would spool Codex
        // events into the Claude Code spool and map them with the wrong kind
        // table — a wrong record rather than a missing one.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("hooks.json");
        run(codex_args(&path, false)).expect("install");
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");

        for event in thruline_capture::codex::SUBSCRIBED_EVENTS {
            let entries = v["hooks"][event].as_array().unwrap_or_else(|| {
                panic!("{event} was not installed");
            });
            let text = entries[0].to_string();
            assert!(text.contains("hook codex"), "{event}: {text}");
            assert!(!text.contains("hook claude-code"), "{event}: {text}");
        }
        // Codex has no `PostToolUseFailure`; its `PostToolUse` fires on a
        // non-zero exit too. Subscribing to one would register an event the
        // harness never sends, and quietly imply we had the pair.
        assert!(
            v["hooks"].get("PostToolUseFailure").is_none(),
            "installed a Claude Code event Codex does not send"
        );
        assert!(
            v["hooks"]["PostToolUse"].is_array(),
            "the failure-bearing event is not subscribed"
        );
    }

    #[test]
    fn every_capped_codex_event_states_a_timeout_inside_the_cap() {
        use thruline_capture::codex::{
            CAPPED_EVENTS, CAPPED_TIMEOUT_MAX_SECS, CAPTURE_TIMEOUT_SECS,
        };
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("hooks.json");
        run(codex_args(&path, false)).expect("install");
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");

        let mut capped_seen = 0usize;
        for event in thruline_capture::codex::SUBSCRIBED_EVENTS {
            let t = v["hooks"][event][0]["hooks"][0]["timeout"]
                .as_u64()
                // Absent means Codex's own default, which on a capped event is
                // 1 s — tighter than a cold binary start.
                .unwrap_or_else(|| panic!("{event} states no timeout"));
            if CAPPED_EVENTS.contains(event) {
                capped_seen += 1;
                assert!(
                    t <= CAPPED_TIMEOUT_MAX_SECS,
                    "{event}: {t}s exceeds the {CAPPED_TIMEOUT_MAX_SECS}s cap and would be \
                     truncated silently"
                );
            } else {
                assert_eq!(t, CAPTURE_TIMEOUT_SECS, "{event}");
            }
        }
        assert_eq!(
            capped_seen, 1,
            "expected exactly one capped event among the subscriptions (SessionEnd); \
             it is also the only event Codex clamps at all, which \
             scripts/vendor-freshness.py re-derives from the binary's clamp messages"
        );
    }

    #[test]
    fn codex_install_and_undo_leave_a_near_colliding_foreign_pack_alone() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("hooks.json");
        std::fs::write(&path, FOREIGN).expect("write");

        run(codex_args(&path, false)).expect("install");
        let after: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        let start = after["hooks"]["SessionStart"]
            .as_array()
            .expect("SessionStart");
        assert_eq!(start.len(), 2, "install replaced the foreign entry");
        assert!(start.iter().any(|e| e.to_string().contains("market-mcp")));

        run(codex_args(&path, true)).expect("undo");
        let undone: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        let start = undone["hooks"]["SessionStart"]
            .as_array()
            .expect("SessionStart survives");
        assert_eq!(
            start.len(),
            1,
            "undo took the foreign entry too — `relay-codex-hook` is not `hook codex`"
        );
        assert!(start[0].to_string().contains("market-mcp"));
        assert!(
            undone["hooks"].get("PostToolUse").is_none(),
            "undo left the codex pack behind"
        );
    }

    #[test]
    fn codex_refuses_project_scope_rather_than_writing_a_file_nothing_reads() {
        let err = run(Args {
            harness: "codex".into(),
            scope: "project".into(),
            settings: None,
            managed_settings_snippet: false,
            gate: false,
            undo: false,
            json: true,
        })
        .expect_err("project scope must refuse");
        assert!(
            err.to_string().contains("user-scoped"),
            "wrong refusal: {err}"
        );
    }

    #[test]
    fn codex_refuses_the_managed_settings_snippet() {
        let err = run(Args {
            managed_settings_snippet: true,
            ..codex_args(std::path::Path::new("/dev/null"), false)
        })
        .expect_err("codex has no managed-settings mechanism");
        assert!(err.to_string().contains("Claude Code only"), "{err}");
    }

    #[test]
    fn an_unknown_harness_and_an_uninstallable_one_fail_differently() {
        // Telling someone who typed `gemini` that no such harness exists would
        // be false: its contract is mapped, its registration file is not.
        let unknown = run(Args {
            harness: "clyde".into(),
            ..codex_args(std::path::Path::new("/dev/null"), false)
        })
        .expect_err("unknown");
        assert!(unknown.to_string().contains("unknown harness"), "{unknown}");

        let mapped = run(Args {
            harness: "gemini".into(),
            ..codex_args(std::path::Path::new("/dev/null"), false)
        })
        .expect_err("gemini");
        assert!(
            mapped.to_string().contains("no hook pack") && !mapped.to_string().contains("unknown"),
            "{mapped}"
        );
    }

    #[test]
    fn the_gate_rides_an_uncapped_event() {
        // The premise of the `--gate` refusal in `run`. The gate asks for 10 s;
        // a capped event would truncate that to 3 s with nothing said.
        assert!(
            !thruline_capture::codex::CAPPED_EVENTS.contains(&"Stop"),
            "the gate's event became capped — the refusal in `run` is now live"
        );
        let entry = crate::gate_cmd::hook_entry(std::path::Path::new("/x/thruline"));
        let asked = entry["hooks"][0]["timeout"].as_u64().expect("timeout");
        assert!(
            asked > thruline_capture::codex::CAPPED_TIMEOUT_MAX_SECS,
            "the gate now fits inside the cap, so the refusal guards nothing"
        );
    }

    #[test]
    fn a_codex_install_says_the_pack_will_not_run_until_it_is_trusted() {
        // The install is only half the story: Codex records trust against the
        // hook definition's hash, so a freshly written pack is skipped. A
        // command that prints "Installed 9 hooks" and stops is true and
        // useless — it is the sentence an operator quotes while wondering why
        // the spool is empty.
        //
        // Asserted on the CONSTANTS rather than on prose, so the notice cannot
        // drift from the flag it names.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("hooks.json");
        // `--json` is the machine path and stays quiet; the human path speaks.
        let mut a = codex_args(&path, false);
        a.json = false;
        run(a).expect("install");

        let bypass = thruline_capture::codex::TRUST_BYPASS_FLAG;
        let review = thruline_capture::codex::TRUST_REVIEW_COMMAND;
        assert!(bypass.starts_with("--"), "{bypass}");
        assert!(
            bypass.contains("dangerously"),
            "the vendor's name carries the warning; do not soften it: {bypass}"
        );
        assert!(review.starts_with('/'), "{review}");
    }

    #[test]
    fn the_unsubscribed_codex_events_are_declared_not_silent() {
        // Codex has 11 hook events; this adapter takes 9. The other 2 must be
        // named, or the gap is a silence that reads as completeness.
        //
        // These numbers are still transcribed — a count in a test is a claim
        // about a vendor, not a derivation of one. What checks them against the
        // installed Codex is `scripts/vendor-freshness.py` (fact F1), which
        // reads the binary's own field table. This assertion's job is only that
        // the two constants cannot drift apart from each other silently.
        use thruline_capture::codex::{SUBSCRIBED_EVENTS, UNSUBSCRIBED_EVENTS};
        assert_eq!(SUBSCRIBED_EVENTS.len(), 9);
        assert_eq!(UNSUBSCRIBED_EVENTS.len(), 2);
        assert_eq!(SUBSCRIBED_EVENTS.len() + UNSUBSCRIBED_EVENTS.len(), 11);
        for e in UNSUBSCRIBED_EVENTS {
            assert!(
                !SUBSCRIBED_EVENTS.contains(e),
                "{e} is both subscribed and declared unsubscribed"
            );
        }
    }

    #[test]
    fn codex_user_scope_resolves_to_the_codex_hooks_file() {
        // Not `~/.claude/settings.json`. Codex reads a dedicated file, and a
        // pack written to the wrong one installs cleanly and is read by nobody.
        let p = thruline_capture::codex::hooks_path(std::path::Path::new("/home/x"));
        assert_eq!(p, std::path::Path::new("/home/x/.codex/hooks.json"));
    }
}
