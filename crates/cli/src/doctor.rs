//! `thruline doctor` — BUILD-SPEC §13, and M0's stated done-when.
//!
//! > *"a real Claude Code session produces a complete event chain with
//! > snapshots and parsed test checks, and `thruline doctor` verifies chain
//! > integrity."*
//!
//! Chain integrity is the load-bearing check, and it is a **recomputation**,
//! not a read: `Ledger::verify_session` walks every event, re-derives its hash
//! from its canonical bytes, and re-links it. The `head_hash` column is a cache
//! maintained at append, so reporting it would be reporting the ledger's own
//! claim about itself.
//!
//! Everything else here exists because a fact the product knows and does not
//! say is a trust failure of exactly the kind the product is about:
//!
//! * **the git-object footprint**, with how to reclaim it and how to opt out —
//!   §4.4 snapshots write loose objects into the user's repository, and a user
//!   learning that from `du` would be the failure;
//! * **capture degradation** — a snapshot taken under a `.gitattributes` clean
//!   filter cannot be trusted to name the bytes on disk (§13), and claims bound
//!   to it must reach `unverifiable`, never `supported`;
//! * **which hook packs are installed**, because three can be present and they
//!   are not interchangeable;
//! * **spool health and quarantined lines**, because a quarantined delivery is
//!   evidence that did not make it into the record;
//! * **whether `.thruline/redact.yml` loaded**, because a config that silently
//!   failed to apply leaks exactly what the user asked to have masked.
//!
//! Exit code is 0 when everything it checked is intact, 1 otherwise — a script
//! must be able to ask. A check that did not apply where doctor ran — the
//! working-tree check, started outside any git repository — is printed as a
//! `note` and is not a failure (T-7-1 rehearsal).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use thruline_capture::redaction::RedactConfig;
use thruline_ledger::Ledger;

pub struct Args {
    pub exec_db: Option<PathBuf>,
    pub spool: Option<PathBuf>,
    pub repo: Option<PathBuf>,
    pub json: bool,
}

/// One reported fact.
struct Finding {
    key: &'static str,
    ok: bool,
    detail: String,
}

pub fn run(args: Args) -> Result<ExitCode> {
    let mut findings: Vec<Finding> = Vec::new();

    // --- git, without which the snapshot engine cannot run at all ----------
    let git = std::process::Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .arg("--version")
        .output();
    findings.push(match git {
        Ok(out) if out.status.success() => Finding {
            key: "git",
            ok: true,
            detail: String::from_utf8_lossy(&out.stdout).trim().to_string(),
        },
        _ => Finding {
            key: "git",
            ok: false,
            detail: "git is not on PATH; snapshots and tree binding cannot run".into(),
        },
    });

    // --- chain integrity, recomputed ---------------------------------------
    let db = match args.exec_db {
        Some(p) => p,
        None => Ledger::default_path().context("locating the execution store")?,
    };
    if db.is_file() {
        let ledger = Ledger::open(&db).with_context(|| format!("opening {}", db.display()))?;
        let mut sessions = 0u64;
        let mut verified = 0u64;
        let mut broken: Vec<String> = Vec::new();
        ledger.for_each_session_id(|s| {
            sessions += 1;
            match ledger.verify_session(s) {
                Ok(_) => verified += 1,
                // A session with no events is registered, not broken.
                Err(thruline_ledger::LedgerError::UnknownSession { .. }) => {}
                Err(e) => broken.push(format!("{s}: {e}")),
            }
            Ok(())
        })?;
        findings.push(Finding {
            key: "chain",
            ok: broken.is_empty(),
            detail: if broken.is_empty() {
                format!("{verified} of {sessions} sessions verify (recomputed, not read)")
            } else {
                format!(
                    "{} sessions DO NOT verify: {}",
                    broken.len(),
                    broken.join("; ")
                )
            },
        });
    } else {
        findings.push(Finding {
            key: "chain",
            ok: true,
            detail: format!("no execution store at {} yet", db.display()),
        });
    }

    // --- §13: a wrapped runner the recorder never saw -----------------------
    if db.is_file() {
        if let Ok(ledger) = Ledger::open(&db) {
            findings.push(bypass_finding(&ledger));
        }
    }

    // --- the spool, and anything quarantined out of it ---------------------
    let spool = args
        .spool
        .or_else(thruline_capture::claude_code::default_spool);
    match &spool {
        Some(path) if path.is_file() => {
            let lines = std::fs::read_to_string(path)
                .map(|t| t.lines().filter(|l| !l.trim().is_empty()).count())
                .unwrap_or(0);
            findings.push(Finding {
                key: "spool",
                ok: true,
                detail: format!("{lines} unfolded deliveries at {}", path.display()),
            });
            let q = thruline_capture::fold::quarantine_path(path);
            let held = std::fs::read_to_string(&q)
                .map(|t| t.lines().filter(|l| !l.trim().is_empty()).count())
                .unwrap_or(0);
            findings.push(Finding {
                key: "quarantine",
                ok: held == 0,
                detail: if held == 0 {
                    "no deliveries held back".into()
                } else {
                    format!(
                        "{held} deliveries could not be sealed and are held at {} — \
                         evidence that is NOT in the record",
                        q.display()
                    )
                },
            });
        }
        _ => findings.push(Finding {
            key: "spool",
            ok: true,
            detail: "no spool yet; no hook has fired".into(),
        }),
    }

    // --- hook packs ---------------------------------------------------------
    findings.push(hook_packs());

    // --- the §4.4 exec shim -------------------------------------------------
    findings.extend(shim_findings());

    // --- the repository: object footprint, capture degradation, redact.yml --
    let explicit = args.repo.is_some();
    let repo = args
        .repo
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let (repo_findings, notes) = repository_findings(&repo, explicit);
    findings.extend(repo_findings);

    // A note is a check that did not apply where doctor ran. It is printed,
    // and it is never counted against the exit code: nothing is broken.
    let ok = findings.iter().all(|f| f.ok);
    if args.json {
        let row = |f: &Finding, note: bool| serde_json::json!({"key": f.key, "ok": f.ok, "detail": f.detail, "note": note});
        let value = serde_json::json!({
            "ok": ok,
            "findings": findings
                .iter()
                .map(|f| row(f, false))
                .chain(notes.iter().map(|n| row(n, true)))
                .collect::<Vec<_>>(),
        });
        crate::json_out::print(&crate::json_out::DOCTOR, value)?;
    } else {
        for f in &findings {
            println!(
                "{} {:<12} {}",
                if f.ok { "ok  " } else { "FAIL" },
                f.key,
                f.detail
            );
        }
        for n in &notes {
            println!("note {:<12} {}", n.key, n.detail);
        }
        println!();
        println!(
            "{}",
            if !ok {
                "Something above is not intact. A FAIL on `chain` means the record \
                 cannot be trusted; a FAIL on `quarantine` means evidence is missing \
                 from it."
            } else if notes.is_empty() {
                "Everything checked is intact."
            } else {
                "Everything checked is intact. A `note` line is a check that did not \
                 apply here; it is not a failure and is not counted as one."
            }
        );
    }
    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

/// Did a tool run more test commands than the recorder captured?
///
/// §13's *"hooks removed or PATH shims bypassed"*, made checkable by M0.8: an
/// agent that runs `/opt/homebrew/bin/pytest` directly leaves a wrapper that is
/// intact, on `PATH` and never reached, which from the record's side is
/// identical to "no tests were run".
///
/// **Counted per session, not tested for emptiness.** Asking whether a session
/// had *no* execs let a single captured run hide every bypassed one after it —
/// `pytest` (captured, green) followed by `/usr/bin/pytest --ignore=failing`
/// (invisible) reported clean. Comparing counts catches that, and does not need
/// a per-command correlation key the vendor payload does not carry.
///
/// The commands are parsed by [`thruline_verify::split_commands`], which
/// refuses what it cannot expand, so `echo 'pytest passes'` is not a hit.
///
/// **What it cannot separate**, stated on the finding itself: a tool call that
/// was denied or failed before running also leaves a tool record and no exec.
/// Both readings mean the same thing for a verdict — there is no execution
/// evidence for that command — so the finding names both rather than guessing.
/// The coverage answer the attention headline is gated on.
///
/// Only findings that bear on whether the RECORD IS COMPLETE are consulted:
/// the shim (when nothing invokes it, a test command leaves no execution
/// evidence at all) and shim-bypass (a tool recorded a runner the recorder
/// never captured). Repository hygiene — `tree-fidelity`, `git-objects` — does
/// not make the record incomplete and must not be allowed to silence the calm
/// sentence, or the gate becomes noise and gets ignored.
///
/// Opens the execution store itself: it is a different database from the
/// transcript ledger `now` reads, and the two are not interchangeable.
///
/// **Cannot-tell is not Complete.** If the execution store cannot be located or
/// opened, this returns [`Coverage::Incomplete`]. An unopenable store is the
/// strongest possible statement that nothing was watched, and the one thing the
/// headline must never do is read calm over a record it could not check.
pub fn coverage(exec_db: Option<&Path>) -> thruline_core::now::Coverage {
    use thruline_core::now::Coverage;
    let db = match exec_db {
        Some(p) => p.to_path_buf(),
        None => match Ledger::default_path() {
            Ok(p) => p,
            Err(_) => return Coverage::Incomplete,
        },
    };
    let ledger = match Ledger::open(&db) {
        Ok(l) => l,
        Err(_) => return Coverage::Incomplete,
    };
    let mut complete = bypass_finding(&ledger).ok;
    for f in shim_findings() {
        complete &= f.ok;
    }
    if complete {
        Coverage::Complete
    } else {
        Coverage::Incomplete
    }
}

fn bypass_finding(ledger: &Ledger) -> Finding {
    let rows = match ledger.runner_evidence() {
        Ok(rows) => rows,
        Err(e) => {
            return Finding {
                key: "shim-bypass",
                ok: false,
                detail: format!("could not be checked: {e}"),
            }
        }
    };

    // session -> (runner commands a tool recorded, runner execs captured)
    let mut tallies: std::collections::BTreeMap<String, (Vec<String>, usize)> =
        std::collections::BTreeMap::new();
    for (session, kind, text) in rows {
        let entry = tallies.entry(session).or_default();
        let recognised: Vec<Vec<String>> = candidate_commands(&text)
            .into_iter()
            .filter(|argv| thruline_verify::Runner::from_argv(argv).is_some())
            .collect();
        if kind == "exec" {
            // An exec is one invocation whatever its argv parses to.
            if !recognised.is_empty() {
                entry.1 += 1;
            }
        } else {
            // A tool input can carry several: `pytest a && pytest b`.
            for argv in recognised {
                entry.0.push(argv.join(" "));
            }
        }
    }

    let mut hits: Vec<String> = Vec::new();
    for (session, (tool_commands, captured)) in tallies {
        if tool_commands.len() > captured {
            hits.push(format!(
                "{session}: {} recorded by a tool, {captured} captured ({})",
                tool_commands.len(),
                tool_commands.join(", ")
            ));
        }
    }
    Finding {
        key: "shim-bypass",
        ok: hits.is_empty(),
        detail: if hits.is_empty() {
            "every test runner a tool recorded was also captured as an exec".into()
        } else {
            format!(
                "{} session(s) recorded more test-runner invocations through a tool \
                 than the recorder captured. Either the shim was not on PATH for \
                 them, or the tool call never ran — both mean there is NO execution \
                 evidence for those commands, so a claim about them can reach \
                 `unverifiable` and never `supported`: {}",
                hits.len(),
                hits.join("; ")
            )
        },
    }
}

/// Every argv a recorded tool input or exec argv might have run.
///
/// The whole string is tried as a script — a Bash tool's input is one — and a
/// JSON rendering is unwrapped to its `command` field first. Both go through
/// the §13 parser, which refuses rather than guesses, so an unparseable payload
/// contributes nothing instead of contributing a wrong argv.
fn candidate_commands(text: &str) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = Vec::new();
    let script = match serde_json::from_str::<serde_json::Value>(text) {
        Ok(value) => value
            .get("command")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| text.to_string()),
        Err(_) => text.to_string(),
    };
    if let Ok(commands) = thruline_verify::split_commands(&script) {
        out.extend(commands.into_iter().map(|c| c.argv));
    }
    out
}

/// The PATH shim: installed, unmodified, reachable, and what it has recorded.
///
/// §13 lists *"hooks removed or PATH shims bypassed"* as a threat, so three
/// different failures have to be distinguishable here and none of them looks
/// like an error at the command line:
///
/// * **modified** — a wrapper whose bytes are not the ones ThruLine wrote. It
///   may be recording something else, or nothing.
/// * **not on PATH** — an intact shim nothing invokes records nothing, and the
///   session looks like one where no tests were run.
/// * **shadowed** — on PATH, but behind a directory that also provides the
///   program. The wrapper is never reached, and the symptom is identical to the
///   previous one from the record's point of view.
fn shim_findings() -> Vec<Finding> {
    let mut out = Vec::new();
    let Ok(home) = crate::shim_cmd::thruline_home() else {
        return out;
    };
    let dir = home.join("shim");
    let integrity = thruline_capture::shim::verify(&dir, None);
    if !integrity.installed {
        out.push(Finding {
            key: "shim",
            ok: true,
            detail: format!(
                "not installed at {}. Without it there are no `exec` events, so a \
                 completion claim about tests can reach `unverifiable` and never \
                 `supported`. `thruline shim install` writes it.",
                dir.display()
            ),
        });
        return out;
    }
    let mut problems: Vec<String> = Vec::new();
    if !integrity.modified.is_empty() {
        problems.push(format!("MODIFIED: {}", integrity.modified.join(", ")));
    }
    if !integrity.missing.is_empty() {
        problems.push(format!("MISSING: {}", integrity.missing.join(", ")));
    }
    if !integrity.on_path {
        problems.push(format!(
            "{} is NOT on PATH, so nothing invokes it",
            dir.display()
        ));
    }
    if !integrity.shadowed_by.is_empty() {
        problems.push(format!(
            "shadowed — these come first on PATH and are reached instead: {}",
            integrity.shadowed_by.join(", ")
        ));
    }
    // `forged` was carried by `ShimIntegrity` and reported by `shim status`,
    // and read here by nothing. A forgery field that `doctor` does not print is
    // not a guard — it is a field. `ShimIntegrity::ok()` already failed on it,
    // so the exit code was right and the sentence was missing.
    if !integrity.forged.is_empty() {
        problems.push(format!(
            "FORGED — file(s) in the shim directory under a wrapped name that ThruLine \
             did not write, answering `command -v` for a program that may not exist: {}",
            integrity.forged.join(", ")
        ));
    }
    if integrity.captures_nothing {
        problems.push(format!(
            "CAPTURES NOTHING — {} holds no wrapper, so no command is recorded and every \
             completion claim about tests stays `unverifiable`. Install the tools you \
             expect recorded, then `thruline shim install` again",
            dir.display()
        ));
    }
    if let Some(p) = &integrity.problem {
        problems.push(p.clone());
    }
    // `uncovered` is an advisory by construction — its own doc comment says a
    // tool installed after the shim is a reinstall, not a compromise, and it is
    // measured against whoever ran `doctor` rather than against the install's
    // PATH. So it is appended to the detail in BOTH branches rather than made a
    // problem: reported always, fatal never.
    let uncovered = if integrity.uncovered.is_empty() {
        String::new()
    } else {
        format!(
            ". Not covered (resolves now, skipped at install — reinstall to record it): {}",
            integrity.uncovered.join(", ")
        )
    };
    out.push(Finding {
        key: "shim",
        ok: problems.is_empty(),
        detail: if problems.is_empty() {
            format!(
                "{} wrappers intact and first on PATH ({}){uncovered}",
                integrity.intact.len(),
                dir.display()
            )
        } else {
            format!("{}{uncovered}", problems.join("; "))
        },
    });

    if let Some(manifest) = crate::shim_cmd::manifest() {
        let (session, binding) = crate::shim_cmd::session_binding_hint(&manifest);
        out.push(Finding {
            key: "exec-session",
            // Never a failure: an unbound exec is still recorded, under a
            // synthetic session. But the ROUTE is reported, because
            // `spool-latest` is a heuristic and evidence attributed to the
            // wrong session is not a small error.
            ok: true,
            detail: format!(
                "recorded commands attribute to {} (via {})",
                session.as_deref().unwrap_or("a synthetic unbound session"),
                binding.as_str()
            ),
        });
        let (count, bytes) = walk_objects(&manifest.output_store);
        out.push(Finding {
            key: "exec-output",
            ok: true,
            detail: format!(
                "{count} captured output file(s) / {:.1} MB at {}. Content-addressed \
                 and redacted before they were written; delete the directory to \
                 reclaim it, at the cost of being unable to re-parse those runs.",
                bytes as f64 / 1_048_576.0,
                manifest.output_store.display()
            ),
        });
    }
    out
}

/// Which hook packs are installed, across both settings scopes.
///
/// Three can be present on one machine — the legacy sensor pack, the speaker
/// pack, and the §4.1 capture hook — and they are not interchangeable.
///
/// # What this used to report, and why it was worse than nothing
///
/// It counted the **event keys** whose JSON contained the substring
/// `"thruline"`, and it hard-coded `ok: true`. Measured: a settings file whose
/// only entry was `/nonexistent/path/thruline hook claude-code` was certified
/// `hooks ok — user: 1 ThruLine hooks (Stop)`, and four entries under one
/// `Stop` key — two of them recorders — also read as `1`. So the two states
/// `doctor` exists to make visible were both invisible: a hook that cannot run,
/// and two recorders writing two records of one session.
///
/// It now counts entries, **stats the program each command names**, and says
/// `ok: false` when a hook cannot run or when one event carries more than one
/// recorder.
fn hook_packs() -> Finding {
    hook_packs_in(&merged_settings_paths())
}

/// The whole check, over a named list of settings files, so it can be tested
/// without reaching for the developer's own `~/.claude/settings.json`.
///
/// # It counts recorders across the WHOLE list, not inside each file
///
/// This loop used to compute its collision per path, which made the check blind
/// to the state it exists to report. BUILD-PLAN B.1, quoting the vendor doc:
/// *"Hook entries merge across settings levels rather than replacing each
/// other."* So one recorder in `~/.claude/settings.json` and one in
/// `.claude/settings.json` are **both live**, and the per-file count is 1 in
/// each — `ok`. Measured: `thruline init --yes` then
/// `thruline hook-install --gate --scope project` produced two recorders on
/// seven events, four `Stop` entries, and `doctor` reported `ok   hooks`.
fn hook_packs_in(paths: &[(&'static str, PathBuf)]) -> Finding {
    let mut found: Vec<String> = Vec::new();
    let mut problems: Vec<String> = Vec::new();
    // Event → the scope of every recorder on it, one entry per recorder, summed
    // over every settings level. This is the view the harness has.
    let mut merged_recorders: BTreeMap<String, Vec<&'static str>> = BTreeMap::new();
    for level in read_hook_levels(paths) {
        let scope = level.scope;
        let read = match level.state {
            // As before the read was factored out: a file that is absent or
            // cannot be read is passed over here. The HQ capture health route
            // serves the second as unknown, with its reason.
            SettingsState::Absent | SettingsState::Unreadable(_) => continue,
            SettingsState::NotJson => {
                problems.push(format!("{scope}: settings file is not valid JSON"));
                continue;
            }
            SettingsState::Read(read) => read,
        };
        let mut entries = 0usize;
        let mut events: Vec<String> = Vec::new();
        for (event, commands) in read {
            entries += commands.len();
            events.push(if commands.len() == 1 {
                event.clone()
            } else {
                format!("{event}×{}", commands.len())
            });
            for _ in commands.iter().filter(|c| c.recorder) {
                merged_recorders
                    .entry(event.clone())
                    .or_default()
                    .push(scope);
            }
            for command in &commands {
                if let Some(missing) = &command.missing_program {
                    problems.push(format!(
                        "{scope}: the `{event}` hook names `{missing}`, which is not an \
                         executable on this machine — that hook runs nothing and reports \
                         nothing"
                    ));
                }
            }
        }
        if entries > 0 {
            events.sort();
            found.push(format!(
                "{scope}: {entries} ThruLine hook entr{} ({})",
                if entries == 1 { "y" } else { "ies" },
                events.join(", ")
            ));
        }
    }
    // Two recorders on one event is §14's collision, and it is the one failure
    // here a user cannot see any other way: both append, and nothing afterwards
    // says which record a claim was checked against. Computed over the merged
    // view, so a collision that spans two settings levels is as visible as one
    // inside a single file.
    let mut collisions: Vec<String> = Vec::new();
    let mut spans_levels = false;
    for (event, scopes) in &merged_recorders {
        if scopes.len() < 2 {
            continue;
        }
        // Deduped in the order the levels were read, not sorted: "user +
        // project" is the order a reader will go looking in, and "project +
        // user" is the same fact stated backwards.
        let mut distinct: Vec<&str> = Vec::new();
        for scope in scopes {
            if !distinct.contains(scope) {
                distinct.push(scope);
            }
        }
        spans_levels |= distinct.len() > 1;
        collisions.push(format!(
            "{event}×{} ({})",
            scopes.len(),
            distinct.join(" + ")
        ));
    }
    if !collisions.is_empty() {
        problems.push(format!(
            "more than one ThruLine RECORDER on {}, so this session is being written to two \
             records and nothing afterwards says which one a claim was checked against{} — \
             `thruline hook-install --undo` and `thruline init --undo` each remove one pack \
             (add `--scope project` to reach a project-level pack)",
            collisions.join(", "),
            if spans_levels {
                ". Claude Code MERGES hook entries across settings levels rather than \
                 replacing them, so packs in two different files are both live"
            } else {
                ""
            }
        ));
    }
    let ok = problems.is_empty();
    let detail = match (found.is_empty(), problems.is_empty()) {
        (true, true) => "no ThruLine hooks installed".to_string(),
        (true, false) => problems.join("; "),
        (false, true) => found.join("; "),
        (false, false) => format!("{}. {}", found.join("; "), problems.join(". ")),
    };
    Finding {
        key: "hooks",
        ok,
        detail,
    }
}

/// One settings level as read: which scope, which file, and what it holds.
pub(crate) struct SettingsLevel {
    pub scope: &'static str,
    pub path: PathBuf,
    pub state: SettingsState,
}

/// What one settings file held, as far as ThruLine's hooks go.
pub(crate) enum SettingsState {
    /// Nothing is at that path.
    Absent,
    /// Something is at that path and could not be read; the reason.
    Unreadable(String),
    /// It was read and is not JSON.
    NotJson,
    /// It was read: each event, in the file's order, that carries at least
    /// one ThruLine command, with those commands.
    Read(Vec<(String, Vec<HookCommand>)>),
}

/// One ThruLine command under one event.
pub(crate) struct HookCommand {
    pub command: String,
    /// Does it append capture rows ([`is_recorder`])?
    pub recorder: bool,
    /// The file it would run ([`resolve_program`]); `None` when it does not
    /// resolve.
    pub program: Option<PathBuf>,
    /// The program it names when that is not an executable on this machine
    /// ([`resolve_program`]'s `Err`); `None` when it resolves.
    pub missing_program: Option<String>,
}

/// Read every settings level in `paths`. This is the one read `thruline
/// doctor`'s hooks check and HQ's capture health route
/// (`GET /api/hq/projects/<id>/capture`) both make, so the two cannot
/// disagree about what is installed or whether it can run.
pub(crate) fn read_hook_levels(paths: &[(&'static str, PathBuf)]) -> Vec<SettingsLevel> {
    paths
        .iter()
        .map(|(scope, path)| SettingsLevel {
            scope,
            path: path.clone(),
            state: read_hook_level(path),
        })
        .collect()
}

fn read_hook_level(path: &Path) -> SettingsState {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return SettingsState::Absent,
        Err(e) => return SettingsState::Unreadable(e.to_string()),
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return SettingsState::NotJson;
    };
    let Some(hooks) = value.get("hooks").and_then(serde_json::Value::as_object) else {
        return SettingsState::Read(Vec::new());
    };
    let mut read = Vec::new();
    for (event, value) in hooks {
        let commands: Vec<HookCommand> = thruline_commands_on(value)
            .into_iter()
            .map(|command| {
                let resolved = resolve_program(&command);
                HookCommand {
                    recorder: is_recorder(&command),
                    program: resolved.as_ref().ok().cloned(),
                    missing_program: resolved.err(),
                    command,
                }
            })
            .collect();
        if !commands.is_empty() {
            read.push((event.clone(), commands));
        }
    }
    SettingsState::Read(read)
}

/// Every ThruLine command under one event's entry list.
pub(crate) fn thruline_commands_on(event: &serde_json::Value) -> Vec<String> {
    event
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|entry| entry.get("hooks").and_then(serde_json::Value::as_array))
        .flatten()
        .filter_map(|h| h.get("command").and_then(serde_json::Value::as_str))
        .filter(|c| c.contains("thruline"))
        .map(str::to_string)
        .collect()
}

/// Does this command append capture rows?
///
/// Speakers (`brief`, `audit`, `digest`) and the Stop gate are deliberately not
/// recorders — A199's amendment (b) — so several of them on one event is
/// correct and only the writers collide.
pub(crate) fn is_recorder(command: &str) -> bool {
    command.contains(" hook claude-code")
        || command.contains(" hook codex")
        || command.contains(" hook gemini")
        || command.contains(" sensor append")
}

/// The program a hook command would actually run, when it cannot be run.
///
/// `None` means it resolves. The search is deliberately generous: the installed
/// command is `<absolute exe> hook claude-code --spool '<path>'`, and an
/// absolute path containing a space would otherwise be reported as missing on
/// the strength of its first word alone.
///
/// The product reads this as [`HookCommand::missing_program`], filled from
/// [`resolve_program`] in one call; this name stays for the tests that pin it.
#[cfg(test)]
fn unrunnable_program(command: &str) -> Option<String> {
    resolve_program(command).err()
}

/// The file a hook command would run (`Ok`), or the program it names when
/// that is not a file on this machine (`Err`). `doctor`'s "cannot run" is this
/// function's `Err`, so the capture health route, which also needs the file to
/// tell whether it is this ThruLine, resolves exactly as `doctor` does.
pub(crate) fn resolve_program(command: &str) -> std::result::Result<PathBuf, String> {
    let trimmed = command.trim();
    if let Some(rest) = trimmed.strip_prefix('\'') {
        if let Some(end) = rest.find('\'') {
            let quoted = &rest[..end];
            return if Path::new(quoted).is_file() {
                Ok(PathBuf::from(quoted))
            } else {
                Err(quoted.to_string())
            };
        }
    }
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    for take in (1..=words.len()).rev() {
        let candidate = PathBuf::from(words[..take].join(" "));
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    let first = words.first().copied().unwrap_or_default();
    // A bare name is resolved against PATH, as the harness's shell would.
    if !first.contains(std::path::MAIN_SEPARATOR) {
        if let Some(found) = on_path(first) {
            return Ok(found);
        }
    }
    Err(first.to_string())
}

fn on_path(program: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|dir| dir.join(program))
            .find(|candidate| candidate.is_file())
    })
}

/// The settings files `thruline` will **write** to, by scope name.
///
/// User and project, in that order. This is the `--scope` table, not the set
/// the harness reads: see [`merged_settings_paths`], which is strictly larger
/// and is what any *counting* must use.
pub fn settings_paths() -> Vec<(&'static str, PathBuf)> {
    let mut out = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        out.push((
            "user",
            PathBuf::from(home).join(".claude").join("settings.json"),
        ));
    }
    if let Ok(cwd) = std::env::current_dir() {
        out.push(("project", cwd.join(".claude").join("settings.json")));
    }
    out
}

/// Every settings file Claude Code **merges hook entries from**.
///
/// BUILD-PLAN B.1, quoting the vendor doc: *"Hook entries merge across settings
/// levels rather than replacing each other … and the `disableAllHooks` setting
/// can't disable managed hooks from outside managed settings."* Anything that
/// counts ThruLine hooks — the `doctor` check, the install-time guard — has to
/// read this list, because a per-file count of a merged configuration answers a
/// question nobody asked.
///
/// Two of these were absent, and each was a place a recorder could sit unseen:
///
/// * **`.claude/settings.local.json`** — the gitignored project level. Read by
///   the harness; missing from [`settings_paths`], so a pack parked there was
///   invisible to `doctor` and to the guard alike.
/// * **managed settings** — the enterprise level, which
///   `thruline hook-install --managed-settings-snippet` itself emits a
///   ten-event **recorder** pack for. Nothing in ThruLine read the file it goes
///   into, so a fleet-managed pack plus a local `init` was two recorders that
///   no ThruLine command could see, and `disableAllHooks` cannot switch the
///   managed one off.
///
/// The managed location is **derived from the installed harness, not
/// remembered** (A205): `getManagedFilePath()` in Claude Code 2.1.234 returns
/// `/Library/Application Support/ClaudeCode` on macOS,
/// `C:\Program Files\ClaudeCode` on Windows and `/etc/claude-code` elsewhere,
/// and the file is `managed-settings.json` inside it, with a `managed-settings.d`
/// drop-in directory beside it. Re-derive on a harness bump; a path that has
/// gone stale reads as "no managed pack", which is the failure direction that
/// stays quiet.
pub fn merged_settings_paths() -> Vec<(&'static str, PathBuf)> {
    merged_settings_paths_for(std::env::current_dir().ok().as_deref())
}

/// [`merged_settings_paths`] for the project at `project` instead of the
/// working directory, in the same order: user, project, project-local, then
/// managed. `None` leaves out the two project levels, as a working directory
/// that could not be read did before.
pub fn merged_settings_paths_for(project: Option<&Path>) -> Vec<(&'static str, PathBuf)> {
    let mut out = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        out.push((
            "user",
            PathBuf::from(home).join(".claude").join("settings.json"),
        ));
    }
    if let Some(dir) = project {
        out.push(("project", dir.join(".claude").join("settings.json")));
        out.push((
            "project-local",
            dir.join(".claude").join("settings.local.json"),
        ));
    }
    if let Some(dir) = managed_settings_dir() {
        out.push(("managed", dir.join("managed-settings.json")));
        // The drop-in directory is a second managed level, and leaving it
        // unscanned would move this blind spot one layer along rather than
        // close it. Sorted so the report is stable.
        if let Ok(entries) = std::fs::read_dir(dir.join("managed-settings.d")) {
            let mut drops: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "json"))
                .collect();
            drops.sort();
            out.extend(drops.into_iter().map(|p| ("managed drop-in", p)));
        }
    }
    out
}

/// Where Claude Code reads managed (enterprise) settings from on this platform.
fn managed_settings_dir() -> Option<PathBuf> {
    let dir = if cfg!(target_os = "macos") {
        "/Library/Application Support/ClaudeCode"
    } else if cfg!(target_os = "windows") {
        r"C:\Program Files\ClaudeCode"
    } else if cfg!(unix) {
        "/etc/claude-code"
    } else {
        return None;
    };
    Some(PathBuf::from(dir))
}

/// Is `path` inside a git working tree, by the one thing git itself looks for:
/// a `.git` (directory, or file for a worktree) here or in a parent.
///
/// Asked BEFORE snapshotting, so that "there is no repository here" can be told
/// apart from "there is one and it could not be snapshotted" — the snapshot's
/// own error folds both into one variant, and only the second is a failure.
/// With `GIT_DIR` set, git decides for itself, so this answers yes and lets the
/// snapshot speak.
fn inside_a_work_tree(path: &Path) -> bool {
    if std::env::var_os("GIT_DIR").is_some() {
        return true;
    }
    let Ok(path) = path.canonicalize() else {
        return true;
    };
    path.ancestors().any(|dir| dir.join(".git").exists())
}

/// The repository checks, and the notes for checks that did not apply.
///
/// T-7-1's rehearsal ran `thruline doctor` from a folder that is not a git
/// repository — an ordinary place to be, a home directory — and got `FAIL
/// tree-fidelity … not a git repository` and exit 1, with every other line ok.
/// Nothing was broken: there was no working tree to check. So when the folder
/// doctor was STARTED in is not inside a repository, the working-tree check is
/// reported as a note and does not fail the run. A repository NAMED with
/// `--repo` that is not one is still a failure: that is a check the person
/// asked for and did not get.
fn repository_findings(repo: &Path, explicit: bool) -> (Vec<Finding>, Vec<Finding>) {
    let mut out = Vec::new();
    let mut notes = Vec::new();

    // The git-object footprint the snapshot ruling requires be disclosed.
    let objects = repo.join(".git").join("objects");
    if objects.is_dir() {
        let (count, bytes) = walk_objects(&objects);
        out.push(Finding {
            key: "git-objects",
            ok: true,
            detail: format!(
                "{count} objects / {:.1} MB in .git/objects. Snapshotting writes \
                 loose objects here; they are unreferenced and `git gc --prune=now` \
                 reclaims them. Growth is bounded by distinct content the agent \
                 wrote, not by snapshot count.",
                bytes as f64 / 1_048_576.0
            ),
        });
    }

    // Capture degradation: a clean filter makes the tree hash stop naming the
    // bytes on disk, and §13 turns that into `unverifiable`.
    let snapshot = if !explicit && !inside_a_work_tree(repo) {
        notes.push(Finding {
            key: "tree-fidelity",
            ok: true,
            detail: format!(
                "not checked here: {} is not inside a git repository, so there is no \
                 working tree to snapshot. Run `thruline doctor` from inside your \
                 project, or pass `--repo <path>`.",
                repo.display()
            ),
        });
        None
    } else {
        Some(thruline_core::spec::snapshot::snapshot_tree(repo))
    };
    match snapshot {
        None => {}
        Some(Ok(snap)) => {
            let degraded = snap.content_filters.degrades_capture();
            out.push(Finding {
                key: "tree-fidelity",
                ok: !degraded,
                detail: if degraded {
                    format!(
                        "CAPTURE DEGRADED — {}. A tree hash taken here may not name \
                         the bytes on disk, so claims bound to it can reach \
                         `unverifiable` and never `supported`.",
                        snap.content_filters.describe()
                    )
                } else {
                    format!(
                        "{}; {} working-tree entries git cannot represent",
                        snap.content_filters.describe(),
                        snap.omitted.len()
                    )
                },
            });
        }
        Some(Err(e)) => out.push(Finding {
            key: "tree-fidelity",
            ok: false,
            detail: format!("could not snapshot this repository: {e}"),
        }),
    }

    // The redaction config: loaded, absent, or broken. A config that failed to
    // parse leaks exactly what the user asked to have masked, so "absent" and
    // "broken" must not look the same.
    let config = repo.join(RedactConfig::PATH);
    out.push(if config.is_file() {
        match RedactConfig::load(repo) {
            Ok(cfg) => Finding {
                key: "redact.yml",
                ok: true,
                detail: format!("loaded, {} custom rules", cfg.rules().len()),
            },
            Err(e) => Finding {
                key: "redact.yml",
                ok: false,
                detail: format!(
                    "present but DID NOT LOAD: {e}. The rules you wrote are not \
                     being applied."
                ),
            },
        }
    } else {
        Finding {
            key: "redact.yml",
            ok: true,
            detail: "none; built-in redaction only".into(),
        }
    });

    // The claim table, and which vocabulary decided what a reader was shown.
    //
    // **F4.** `.thruline/claims.yml` used to REPLACE the built-in table, so a
    // committed four-line file took a session from one unverifiable claim to
    // zero claims and a green badge, disclosing itself nowhere. It may now only
    // add; this reports the digest either way, so "which table judged me" is
    // answerable before a receipt is issued rather than after.
    out.push(match thruline_claims::Table::load(repo) {
        Ok(table) => Finding {
            key: "claims.yml",
            ok: true,
            detail: table.disclosure().unwrap_or_else(|| {
                format!(
                    "none; built-in claim table only ({}), {} patterns",
                    table.digest(),
                    table.patterns().len()
                )
            }),
        },
        Err(e) => Finding {
            key: "claims.yml",
            ok: false,
            detail: format!(
                "present but DID NOT LOAD: {e}. No claim is extracted while this stands, \
                 and a session with no claims is a session with nothing for a gate to check."
            ),
        },
    });

    (out, notes)
}

fn walk_objects(dir: &Path) -> (u64, u64) {
    let (mut count, mut bytes) = (0u64, 0u64);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (count, bytes);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let (c, b) = walk_objects(&path);
            count += c;
            bytes += b;
        } else if let Ok(meta) = entry.metadata() {
            count += 1;
            bytes += meta.len();
        }
    }
    (count, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A settings file with one `Stop` event carrying `commands`.
    fn settings(dir: &Path, commands: &[&str]) -> PathBuf {
        let entries: Vec<serde_json::Value> = commands
            .iter()
            .map(|c| serde_json::json!({"hooks":[{"type":"command","command": c}]}))
            .collect();
        let path = dir.join("settings.json");
        std::fs::write(
            &path,
            serde_json::json!({ "hooks": { "Stop": entries } }).to_string(),
        )
        .expect("write");
        path
    }

    fn check(path: PathBuf) -> Finding {
        hook_packs_in(&[("user", path)])
    }

    #[test]
    fn a_hook_naming_a_binary_that_does_not_exist_is_not_ok() {
        // Measured before this fix: `/nonexistent/path/thruline hook claude-code`
        // was reported as `hooks ok — user: 1 ThruLine hooks (Stop)`. `doctor`
        // never stat'd anything; it grepped the JSON for the substring
        // "thruline". A hook that cannot run records nothing and says nothing,
        // which is the one state a user cannot discover for themselves.
        let dir = tempfile::tempdir().expect("tempdir");
        let f = check(settings(
            dir.path(),
            &["/nonexistent/path/thruline hook claude-code --spool /tmp/s.jsonl"],
        ));
        assert!(!f.ok, "a hook that cannot run was certified: {}", f.detail);
        assert!(
            f.detail.contains("/nonexistent/path/thruline"),
            "the finding does not name the program: {}",
            f.detail
        );
    }

    #[test]
    fn a_hook_naming_a_real_binary_is_ok_and_counted_as_one_entry() {
        let dir = tempfile::tempdir().expect("tempdir");
        let exe = dir.path().join("thruline");
        std::fs::write(&exe, "#!/bin/sh\n").expect("w");
        let f = check(settings(
            dir.path(),
            &[&format!("{} hook claude-code", exe.display())],
        ));
        assert!(f.ok, "{}", f.detail);
        assert!(f.detail.contains("1 ThruLine hook entry"), "{}", f.detail);
    }

    #[test]
    fn entries_are_counted_not_event_keys() {
        // Four entries under one `Stop` key used to read as "1 ThruLine hooks
        // (Stop)" — the count was of event names, so `init` run after
        // `hook-install --gate` was indistinguishable from a clean install.
        let dir = tempfile::tempdir().expect("tempdir");
        let exe = dir.path().join("thruline");
        std::fs::write(&exe, "#!/bin/sh\n").expect("w");
        let e = exe.display();
        let f = check(settings(
            dir.path(),
            &[
                &format!("{e} hook claude-code --spool /tmp/s.jsonl"),
                &format!("{e} gate --hook stop"),
                &format!("{e} audit --hook stop"),
            ],
        ));
        assert!(
            f.detail.contains("3 ThruLine hook entries") && f.detail.contains("Stop×3"),
            "the entries under one event were collapsed into one: {}",
            f.detail
        );
        // One recorder, one gate, one speaker: not a collision.
        assert!(f.ok, "{}", f.detail);
    }

    #[test]
    fn two_recorders_on_one_event_is_a_finding_and_names_both_undos() {
        // §14's collision, reproduced exactly as `hook-install --gate` followed
        // by `init` produces it: four `Stop` entries, two of which append.
        // Nothing afterwards says which record a claim was checked against.
        let dir = tempfile::tempdir().expect("tempdir");
        let exe = dir.path().join("thruline");
        std::fs::write(&exe, "#!/bin/sh\n").expect("w");
        let e = exe.display();
        let f = check(settings(
            dir.path(),
            &[
                &format!("{e} hook claude-code --spool /tmp/s.jsonl"),
                &format!("{e} gate --hook stop"),
                &format!("{e} sensor append Stop --db /tmp/l.db"),
                &format!("{e} audit --hook stop"),
            ],
        ));
        assert!(!f.ok, "two recorders were certified: {}", f.detail);
        assert!(f.detail.contains("RECORDER on Stop×2"), "{}", f.detail);
        assert!(f.detail.contains("hook-install --undo"), "{}", f.detail);
        assert!(f.detail.contains("init --undo"), "{}", f.detail);
    }

    /// **The blind spot.** One recorder per file, two files, and the harness
    /// merges them: `doctor` counted inside its per-path loop and reported `ok`.
    #[test]
    fn a_collision_that_spans_two_settings_levels_is_reported() {
        let user_dir = tempfile::tempdir().expect("tempdir");
        let project_dir = tempfile::tempdir().expect("tempdir");
        let exe = user_dir.path().join("thruline");
        std::fs::write(&exe, "#!/usr/bin/env sh\n").expect("w");
        let e = exe.display();
        let f = hook_packs_in(&[
            (
                "user",
                settings(user_dir.path(), &[&format!("{e} sensor append Stop")]),
            ),
            (
                "project",
                settings(project_dir.path(), &[&format!("{e} hook claude-code")]),
            ),
        ]);
        assert!(
            !f.ok,
            "two recorders across two merged settings levels were certified: {}",
            f.detail
        );
        assert!(f.detail.contains("RECORDER on Stop×2"), "{}", f.detail);
        assert!(
            f.detail.contains("user + project"),
            "the finding must name the levels the collision spans: {}",
            f.detail
        );
        assert!(
            f.detail.contains("MERGES"),
            "the finding must say why two files are one problem: {}",
            f.detail
        );
    }

    /// One recorder in each of two levels is the collision above; one recorder
    /// total across two levels is a normal machine and must stay `ok`.
    #[test]
    fn one_recorder_and_one_speaker_across_two_levels_is_not_a_collision() {
        let user_dir = tempfile::tempdir().expect("tempdir");
        let project_dir = tempfile::tempdir().expect("tempdir");
        let exe = user_dir.path().join("thruline");
        std::fs::write(&exe, "#!/usr/bin/env sh\n").expect("w");
        let e = exe.display();
        let f = hook_packs_in(&[
            (
                "user",
                settings(user_dir.path(), &[&format!("{e} sensor append Stop")]),
            ),
            (
                "project",
                settings(project_dir.path(), &[&format!("{e} gate --hook stop")]),
            ),
        ]);
        assert!(f.ok, "{}", f.detail);
    }

    /// The two settings files that were missing from the list entirely.
    #[test]
    fn the_merged_view_covers_every_level_the_harness_reads() {
        let scopes: Vec<&str> = merged_settings_paths()
            .into_iter()
            .map(|(scope, _)| scope)
            .collect();
        for required in ["user", "project", "project-local", "managed"] {
            assert!(
                scopes.contains(&required),
                "`{required}` settings are merged by Claude Code and are absent from the \
                 list every ThruLine hook count is computed over: {scopes:?}"
            );
        }
        let names: Vec<String> = merged_settings_paths()
            .into_iter()
            .filter(|(scope, _)| *scope == "project-local")
            .map(|(_, p)| p.display().to_string())
            .collect();
        assert!(
            names
                .iter()
                .any(|p| p.ends_with(".claude/settings.local.json")),
            "the project-local level must be `.claude/settings.local.json`: {names:?}"
        );
    }

    #[test]
    fn a_program_path_containing_a_space_is_not_reported_missing() {
        // `$HOME` with a space in it is the common case, and the installer
        // writes the exe path unquoted. Reading only the first word would
        // condemn a perfectly good install.
        let dir = tempfile::tempdir().expect("tempdir");
        let holder = dir.path().join("Ada Lovelace");
        std::fs::create_dir_all(&holder).expect("mkdir");
        let exe = holder.join("thruline");
        std::fs::write(&exe, "#!/bin/sh\n").expect("w");
        assert_eq!(
            unrunnable_program(&format!("{} hook claude-code", exe.display())),
            None
        );
    }

    #[test]
    fn only_the_writers_count_as_recorders() {
        // A199 amendment (b): speakers put words into the session and append no
        // capture rows, so several of them on one event is correct.
        assert!(is_recorder("/x/thruline hook claude-code --spool /s"));
        assert!(is_recorder("/x/thruline sensor append Stop"));
        // `init --db X` pins the ledger between the binary and the verb: still
        // the one recorder it was, so the two-recorders guard still sees it.
        assert!(is_recorder(
            "/x/thruline --db '/l d/l.db' sensor append Stop"
        ));
        assert!(!is_recorder("/x/thruline --db '/l.db' audit --hook stop"));
        assert!(!is_recorder("/x/thruline --db '/l.db' gate --hook stop"));
        assert!(!is_recorder("/x/thruline gate --hook stop"));
        assert!(!is_recorder("/x/thruline audit --hook stop"));
        assert!(!is_recorder("/x/thruline brief --hook session-start"));
        assert!(!is_recorder(
            "/x/thruline digest precompute --hook session-end"
        ));
    }
}
