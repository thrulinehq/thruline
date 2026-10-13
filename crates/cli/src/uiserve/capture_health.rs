//! HQ Next — `GET /api/hq/projects/<id>/capture`: is this project being
//! recorded, read from facts on this machine (C30 R-ROUTES "Capture health").
//!
//! It answers four questions for one HQ project, each from one source:
//!
//!   - **Claude Code hooks** — the settings files Claude Code merges for this
//!     project (user, the project's `.claude/settings.json` and
//!     `settings.local.json`, managed), read by the same function as
//!     `thruline doctor`'s hooks check ([`crate::doctor::read_hook_levels`]),
//!     with each command's program resolved the way `doctor` resolves it
//!     ([`crate::doctor::resolve_program`]). So the route and the terminal
//!     cannot disagree about what is installed or whether it can run.
//!   - **Codex** — what this server's observer says, and nothing more.
//!   - **Git** — one bounded `git rev-parse --show-toplevel` in the project's
//!     root, under a 5 s deadline, with system and user git config refused.
//!   - **The record** — sessions, the first and latest recorded step, and how
//!     many deliveries reached the ledger through the hooks (`sensor_events`).
//!
//! It also serves `holds` (C30 R-LEDGER): this folder's pause and path
//! exclusions as the record holds them, the built-in defaults laid in. A pause
//! is honoured by the six capture callers (`holds.honoured` is `true`, C34 Q5);
//! an exclusion is applied when a view is served or exported
//! (`holds.exclusions_honoured` is `true`, with the sentence beside it). `paused_skips` counts the hook
//! deliveries and Stop audits a pause left unrecorded, and is absent while that
//! count is zero.
//!
//! A fact that could not be read is `null` with a reason beside it, never a
//! guess (C30 G3). The route writes nothing, appends nothing and adds no DDL.
//! Every string it serves has been through `redact::mask_recorded`, and the
//! merged record is served as `masking`.

use super::*;

use std::process::Command;
use std::time::Instant;

use thruline_core::hq as core_hq;
use thruline_core::ledger::Ledger;
use thruline_core::redact::{mask_recorded, MaskRecord};

use crate::doctor::{merged_settings_paths_for, read_hook_levels, SettingsState};

/// The shape of this document. Raised when a field changes meaning.
pub(super) const SCHEMA_VERSION: u32 = 1;

/// How long the one git read may take.
const GIT_DEADLINE: Duration = Duration::from_secs(5);

pub(super) fn owns(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("/api/hq/projects/") else {
        return false;
    };
    matches!(
        rest.split('/').collect::<Vec<_>>().as_slice(),
        [_, "capture"]
    )
}

fn json<T: Serialize>(stream: &mut TcpStream, status: u16, value: &T) -> Result<()> {
    respond(
        stream,
        status,
        "application/json; charset=utf-8",
        &serde_json::to_vec(value)?,
    )
}

fn refuse(stream: &mut TcpStream, status: u16, error: &str) -> Result<()> {
    json(stream, status, &serde_json::json!({ "error": error }))
}

pub(super) fn handle(
    state: &Arc<State>,
    method: &str,
    path: &str,
    query: &str,
    headers: &RequestHeaders,
    stream: &mut TcpStream,
) -> Result<()> {
    if !query.is_empty() {
        return refuse(stream, 400, "unknown_query_parameter");
    }
    if headers.malformed {
        return refuse(stream, 400, "malformed_request");
    }
    if method != "GET" {
        return refuse(stream, 405, "method_not_allowed");
    }
    let id = path
        .strip_prefix("/api/hq/projects/")
        .and_then(|rest| rest.strip_suffix("/capture"))
        .and_then(percent_decode)
        .filter(|id| !id.is_empty() && !id.contains('/'));
    let Some(id) = id else {
        return refuse(stream, 400, "malformed_request");
    };
    let ledger = match Ledger::open(&state.db) {
        Ok(l) => l,
        Err(thruline_core::error::CoreError::StoreHeld { .. }) => {
            return super::store_held(state, stream)
        }
        Err(problem) => {
            return json(
                stream,
                503,
                &serde_json::json!({"error": "store_unavailable", "detail": problem.to_string()}),
            )
        }
    };
    let project = match core_hq::find_project(&ledger, &id) {
        Ok(Some(p)) => p,
        Ok(None) => return refuse(stream, 404, "no_such_project"),
        Err(e) => {
            return json(
                stream,
                500,
                &serde_json::json!({"error": "read_failed", "detail": e.to_string()}),
            )
        }
    };
    let mut mask = Masker::default();
    // C30 R-LEDGER: what a person has paused or excluded for this folder, with
    // the built-in defaults laid in. Read from the record only.
    let (holds, holds_reason) =
        match thruline_core::capture_hold::holds_view(&ledger, &project.root) {
            Ok(view) => (
                Some(super::capture_holds::masked(view, |s| mask.text(s))),
                None,
            ),
            Err(e) => (
                None,
                Some(mask.text(&format!("the pauses and exclusions could not be read: {e}"))),
            ),
        };
    let doc = CaptureHealth {
        schema_version: SCHEMA_VERSION,
        checked_at: thruline_core::ingest::iso8601_utc_now(),
        project: ProjectRef {
            id: project.id.clone(),
            root: mask.text(&project.shown_root()),
        },
        claude_code_hooks: hooks(Path::new(&project.root), &mut mask),
        codex: codex(state, Path::new(&project.root), &mut mask),
        git: git(Path::new(&project.root), &mut mask),
        record: match record(&ledger, &state.db, &project, &mut mask) {
            Ok(record) => record,
            Err(thruline_core::error::CoreError::StoreHeld { .. }) => {
                return super::store_held(state, stream)
            }
            Err(problem) => return Err(problem.into()),
        },
        holds,
        holds_reason,
        paused_skips: {
            let left_out = thruline_core::capture_pause::skip_count_for(&state.db, &project.root);
            (left_out.hook_deliveries + left_out.audits > 0).then_some(left_out)
        },
        masking: mask.record,
    };
    let policy = match crate::exclusion_serve::policy(&ledger, &project.root) {
        Ok(policy) => policy,
        Err(_) => return refuse(stream, 503, "exclusion_policy_unavailable"),
    };
    let raw = serde_json::to_vec(&doc)?;
    if policy.is_empty() {
        return respond(stream, 200, "application/json; charset=utf-8", &raw);
    }
    let mut served: serde_json::Value = serde_json::from_slice(&raw)?;
    let left_out = match present_health(&mut served, &policy) {
        Ok(count) => count,
        Err(_) => return refuse(stream, 503, "exclusion_lookup_failed"),
    };
    if left_out == 0 {
        respond(stream, 200, "application/json; charset=utf-8", &raw)
    } else {
        json(stream, 200, &served)
    }
}

/// Settings must display the standing exclusion keys so a person can turn
/// them off again. Operational facts outside `holds` still use the current
/// presentation policy, including paths and hook commands.
fn present_health(
    doc: &mut serde_json::Value,
    policy: &thruline_core::capture_exclusion::ExclusionPolicy,
) -> Result<u64> {
    if policy.is_empty() {
        return Ok(0);
    }
    let holds = doc
        .as_object_mut()
        .context("capture health has no fields")?
        .remove("holds");
    let result = crate::exclusion_serve::checked_apply(doc, policy);
    if let Some(holds) = holds {
        doc["holds"] = holds;
    }
    result
}

/// Masks every served string and keeps the merged record.
#[derive(Default)]
struct Masker {
    record: MaskRecord,
}

impl Masker {
    fn text(&mut self, raw: &str) -> String {
        let (out, rec) = mask_recorded(raw);
        self.record.merge(&rec);
        out
    }
    fn opt(&mut self, raw: Option<&str>) -> Option<String> {
        raw.map(|r| self.text(r))
    }
}

#[derive(Serialize)]
struct CaptureHealth {
    schema_version: u32,
    /// When this was read, UTC.
    checked_at: String,
    project: ProjectRef,
    claude_code_hooks: Hooks,
    codex: Codex,
    git: Git,
    record: Record,
    /// C30 R-LEDGER: this folder's pause and path exclusions as the record
    /// holds them, the built-in defaults laid in. A pause is honoured
    /// (`honoured: true`); an exclusion is applied at serve and export time
    /// (`exclusions_honoured: true`), with the sentence beside them. `null`,
    /// with the reason, when the record could not be read.
    holds: Option<thruline_core::capture_hold::HoldsView>,
    holds_reason: Option<String>,
    /// C34 pause gate: the hook deliveries and Stop audits left unrecorded
    /// because this folder was paused, counted beside the ledger. Absent while
    /// nothing has been left out, so a project never paused serves what it did.
    #[serde(skip_serializing_if = "Option::is_none")]
    paused_skips: Option<thruline_core::capture_pause::SkipCount>,
    masking: MaskRecord,
}

#[derive(Serialize)]
struct ProjectRef {
    id: String,
    root: String,
}

/* ───────────────────────────── Claude Code hooks ───────────────────────────── */

#[derive(Serialize)]
struct Hooks {
    /// A ThruLine recorder (a command that appends capture rows) is on at
    /// least one event Claude Code reads for this project. `null` when none
    /// was found and a settings level could not be read.
    installed: Option<bool>,
    installed_reason: Option<String>,
    /// Every ThruLine command found names a program that exists. `false` as
    /// soon as one does not; `null` when nothing is installed or a level could
    /// not be read.
    alive: Option<bool>,
    alive_reason: Option<String>,
    /// The events carrying a recorder, sorted, each once.
    events: Vec<String>,
    /// The commands whose program is missing, counted.
    missing_programs: u64,
    /// This build's version, when every recorder names this very program.
    /// `null` when a recorder names another ThruLine program (its version is
    /// not read without running it) or nothing is installed.
    version: Option<String>,
    version_reason: Option<String>,
    /// Each settings level Claude Code merges for this project, in its order.
    levels: Vec<Level>,
}

#[derive(Serialize)]
struct Level {
    /// `user` | `project` | `project-local` | `managed` | `managed drop-in`.
    scope: &'static str,
    path: String,
    /// `read` | `absent` | `unreadable` | `not_json`.
    state: &'static str,
    reason: Option<String>,
    commands: Vec<HookCommandView>,
}

#[derive(Serialize)]
struct HookCommandView {
    event: String,
    command: String,
    /// `recorder` (appends capture rows) | `other` (speaker, gate, MCP …).
    kind: &'static str,
    /// The program it names is a file on this machine.
    program_exists: bool,
    /// The program it names, when that is not a file on this machine.
    missing_program: Option<String>,
    /// It names the very binary serving this page. `null` when the program
    /// does not resolve.
    this_program: Option<bool>,
}

fn hooks(root: &Path, mask: &mut Masker) -> Hooks {
    let here = std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok());
    let mut levels = Vec::new();
    let mut unreadable: Vec<String> = Vec::new();
    let mut recorder_events: BTreeSet<String> = BTreeSet::new();
    let mut commands_total = 0u64;
    let mut missing = 0u64;
    let mut missing_recorder = false;
    // Of the recorders whose program resolves: does each name this binary?
    let mut recorder_programs: Vec<Option<bool>> = Vec::new();
    for level in read_hook_levels(&merged_settings_paths_for(Some(root))) {
        let path = mask.text(&level.path.display().to_string());
        let (state, reason, commands) = match level.state {
            SettingsState::Absent => ("absent", None, Vec::new()),
            SettingsState::Unreadable(why) => {
                unreadable.push(format!("{}: {why}", level.scope));
                ("unreadable", Some(mask.text(&why)), Vec::new())
            }
            SettingsState::NotJson => {
                unreadable.push(format!("{}: not valid JSON", level.scope));
                (
                    "not_json",
                    Some("the file is not valid JSON".to_string()),
                    Vec::new(),
                )
            }
            SettingsState::Read(read) => {
                let mut out = Vec::new();
                for (event, cmds) in read {
                    for c in cmds {
                        commands_total += 1;
                        let this_program = c.program.as_ref().map(|p| {
                            p.canonicalize()
                                .ok()
                                .zip(here.as_ref())
                                .is_some_and(|(a, b)| &a == b)
                        });
                        if c.missing_program.is_some() {
                            missing += 1;
                            missing_recorder |= c.recorder;
                        }
                        if c.recorder {
                            recorder_events.insert(event.clone());
                            if c.missing_program.is_none() {
                                recorder_programs.push(this_program);
                            }
                        }
                        out.push(HookCommandView {
                            event: mask.text(&event),
                            command: mask.text(&c.command),
                            kind: if c.recorder { "recorder" } else { "other" },
                            program_exists: c.missing_program.is_none(),
                            missing_program: mask.opt(c.missing_program.as_deref()),
                            this_program,
                        });
                    }
                }
                ("read", None, out)
            }
        };
        levels.push(Level {
            scope: level.scope,
            path,
            state,
            reason,
            commands,
        });
    }
    let unreadable_reason = (!unreadable.is_empty()).then(|| {
        mask.text(&format!(
            "a settings file Claude Code reads for this project could not be read ({})",
            unreadable.join("; ")
        ))
    });
    let (installed, installed_reason) = if !recorder_events.is_empty() {
        (Some(true), None)
    } else if let Some(why) = &unreadable_reason {
        (None, Some(why.clone()))
    } else {
        (Some(false), None)
    };
    let (alive, alive_reason) = match installed {
        Some(true) if missing > 0 => (
            Some(false),
            Some(format!(
                "{missing} of {commands_total} ThruLine hook commands name a program that is not on this machine{}",
                if missing_recorder {
                    ", a recorder among them"
                } else {
                    ""
                }
            )),
        ),
        Some(true) => match &unreadable_reason {
            Some(why) => (None, Some(why.clone())),
            None => (Some(true), None),
        },
        Some(false) => (None, Some("no ThruLine recorder is installed".to_string())),
        None => (None, unreadable_reason.clone()),
    };
    let (version, version_reason) = if installed != Some(true) {
        (None, Some("no ThruLine recorder is installed".to_string()))
    } else if recorder_programs.is_empty() {
        (
            None,
            Some("no recorder names a program that is on this machine".to_string()),
        )
    } else if recorder_programs.iter().all(|p| *p == Some(true)) {
        (Some(env!("CARGO_PKG_VERSION").to_string()), None)
    } else {
        (
            None,
            Some(
                "a recorder names a different ThruLine program than the one serving this page; its version is not read without running it"
                    .to_string(),
            ),
        )
    };
    Hooks {
        installed,
        installed_reason,
        alive,
        alive_reason,
        events: recorder_events.into_iter().map(|e| mask.text(&e)).collect(),
        missing_programs: missing,
        version,
        version_reason,
        levels,
    }
}

/* ─────────────────────────────────── Codex ─────────────────────────────────── */

#[derive(Serialize)]
struct Codex {
    /// This server was started with an observation source (`--observe-codex`).
    configured: bool,
    /// The observer's own state word (`starting` | `watching` | `paused` |
    /// `disconnected` | `stopped`), as it says it; `null` when not configured.
    state: Option<String>,
    /// `state == "paused"`; `null` when not configured.
    paused: Option<bool>,
    /// The observer's folder is this project's root; `null` when not
    /// configured or either folder cannot be resolved.
    watches_this_project: Option<bool>,
    reason: Option<String>,
}

/// Whether the observer's folder is this project's root: `None` when either
/// folder cannot be resolved, never a guess. The capture-hold routes use the
/// same answer to decide whether a pause reaches the observer.
pub(super) fn observer_watches(observer: &observation::Observer, root: &Path) -> Option<bool> {
    let read = observer.read();
    let watched = read["project"]
        .as_str()
        .and_then(|p| Path::new(p).canonicalize().ok());
    watched.zip(root.canonicalize().ok()).map(|(a, b)| a == b)
}

fn codex(state: &State, root: &Path, mask: &mut Masker) -> Codex {
    let Some(observer) = &state.observer else {
        return Codex {
            configured: false,
            state: None,
            paused: None,
            watches_this_project: None,
            reason: Some("HQ was not started with an observation source".to_string()),
        };
    };
    let read = observer.read();
    let word = read["state"].as_str().map(str::to_string);
    let watches = observer_watches(observer, root);
    Codex {
        configured: true,
        paused: word.as_deref().map(|w| w == "paused"),
        state: mask.opt(word.as_deref()),
        reason: watches.is_none().then(|| {
            "the observed folder or this project's folder could not be resolved".to_string()
        }),
        watches_this_project: watches,
    }
}

/* ──────────────────────────────────── Git ──────────────────────────────────── */

#[derive(Serialize)]
struct Git {
    /// Git answered that the root is inside a working tree (`true`) or that
    /// it is not a repository (`false`); `null` when git could not answer.
    repository: Option<bool>,
    /// The working tree's top level, as git printed it.
    toplevel: Option<String>,
    reason: Option<String>,
}

fn git(root: &Path, mask: &mut Masker) -> Git {
    let unknown = |reason: String| Git {
        repository: None,
        toplevel: None,
        reason: Some(reason),
    };
    if !root.is_dir() {
        return unknown("the project's folder is not on this machine".to_string());
    }
    let mut command = Command::new("git");
    thruline_core::git_isolation::isolate_config(&mut command);
    command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C")
        .env("LANG", "C")
        // The answer is about the project's folder, not about any repository
        // this server's own environment points git at.
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--show-toplevel"]);
    let deadline = Instant::now() + GIT_DEADLINE;
    match thruline_core::bounded_child::output_until(&mut command, deadline) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            unknown("git is not installed on this machine".to_string())
        }
        Err(e) => unknown(mask.text(&format!("git could not be started: {e}"))),
        Ok(thruline_core::bounded_child::Outcome::TimedOut) => unknown(format!(
            "git did not answer within {} s",
            GIT_DEADLINE.as_secs()
        )),
        Ok(thruline_core::bounded_child::Outcome::Finished(out)) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if out.status.success() {
                let top = String::from_utf8_lossy(&out.stdout).trim_end().to_string();
                Git {
                    repository: Some(true),
                    toplevel: Some(mask.text(&top)),
                    reason: None,
                }
            } else if stderr.contains("not a git repository") {
                Git {
                    repository: Some(false),
                    toplevel: None,
                    reason: None,
                }
            } else {
                let first = stderr.lines().next().unwrap_or("").trim();
                unknown(mask.text(&format!("git could not answer: {first}")))
            }
        }
    }
}

/* ───────────────────────────────── The record ──────────────────────────────── */

#[derive(Serialize)]
struct Record {
    /// Sessions recorded for this project, by agent; `null` when the ledger
    /// could not be read (see `reason`).
    sessions: Option<SessionCounts>,
    /// The earliest and latest recorded step's time and agent.
    first_step: Option<Step>,
    latest_step: Option<Step>,
    /// Deliveries Claude Code's hooks put in the ledger (`sensor_events`) for
    /// this project: by its root or a folder inside it, or by one of its
    /// sessions. Zero means every step here came from reading the recordings.
    through_hooks: Option<ThroughHooks>,
    reason: Option<String>,
}

#[derive(Serialize, Default)]
struct SessionCounts {
    claude_code: u64,
    codex: u64,
}

#[derive(Serialize)]
struct Step {
    at: String,
    /// `claude_code` | `codex`.
    agent: String,
}

#[derive(Serialize)]
struct ThroughHooks {
    deliveries: u64,
    first_at: Option<String>,
    latest_at: Option<String>,
    latest_event: Option<String>,
}

fn record(
    ledger: &Ledger,
    db: &Path,
    project: &core_hq::HqProject,
    mask: &mut Masker,
) -> thruline_core::error::Result<Record> {
    match read_record(ledger, db, project, mask) {
        Ok(r) => Ok(r),
        Err(e @ thruline_core::error::CoreError::StoreHeld { .. }) => Err(e),
        Err(e) => Ok(Record {
            sessions: None,
            first_step: None,
            latest_step: None,
            through_hooks: None,
            reason: Some(mask.text(&format!("the record could not be read: {e}"))),
        }),
    }
}

fn read_record(
    ledger: &Ledger,
    db: &Path,
    project: &core_hq::HqProject,
    mask: &mut Masker,
) -> thruline_core::error::Result<Record> {
    let conn = ledger.conn();
    let mut sessions = SessionCounts::default();
    let mut first: Option<(String, String)> = None;
    let mut latest: Option<(String, String)> = None;
    // Each member's session count, for the key of the delivery tally below:
    // a session is never deleted or re-filed, so a member's set of sessions
    // has grown exactly when its count has.
    let mut member_sessions: Vec<(String, u64)> = Vec::new();
    for m in &project.members {
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE project_id = ?1",
            [&m.ledger_project_id],
            |r| r.get(0),
        )?;
        let n = n as u64;
        member_sessions.push((m.ledger_project_id.clone(), n));
        match m.source.as_str() {
            "codex" => sessions.codex += n,
            _ => sessions.claude_code += n,
        }
        // MIN and MAX asked apart: each is one descent of
        // `idx_events_project_ts`. Asked together SQLite walks every entry of
        // the member's range (2.3 s for the 465-session Codex member).
        let lo: Option<String> = conn.query_row(
            "SELECT MIN(ts) FROM events WHERE project_id = ?1",
            [&m.ledger_project_id],
            |r| r.get(0),
        )?;
        let hi: Option<String> = conn.query_row(
            "SELECT MAX(ts) FROM events WHERE project_id = ?1",
            [&m.ledger_project_id],
            |r| r.get(0),
        )?;
        if let Some(lo) = lo {
            if first.as_ref().is_none_or(|(at, _)| lo < *at) {
                first = Some((lo, m.source.clone()));
            }
        }
        if let Some(hi) = hi {
            if latest.as_ref().is_none_or(|(at, _)| hi > *at) {
                latest = Some((hi, m.source.clone()));
            }
        }
    }
    member_sessions.sort();
    member_sessions.dedup();

    // Which deliveries are this project's: a `cwd` at the root (or a folder
    // inside it) of the project or one of its members, or a session the
    // ledger files under one of its members.
    let mut roots: BTreeSet<String> = BTreeSet::new();
    roots.insert(project.root.trim_end_matches('/').to_string());
    for m in &project.members {
        roots.insert(m.root.trim_end_matches('/').to_string());
    }
    roots.retain(|r| !r.is_empty());
    let rule = Rule {
        roots: roots.into_iter().collect(),
        members: member_sessions,
    };
    let tally = through_hooks(ledger, db, &project.id, &rule)?;
    let through_hooks = ThroughHooks {
        deliveries: tally.deliveries,
        first_at: mask.opt(tally.first_at.as_deref()),
        latest_at: mask.opt(tally.latest_at.as_deref()),
        latest_event: mask.opt(tally.latest.as_ref().map(|(_, e)| e.as_str())),
    };
    let step = |s: Option<(String, String)>, mask: &mut Masker| {
        s.map(|(at, agent)| Step {
            at: mask.text(&at),
            agent,
        })
    };
    Ok(Record {
        sessions: Some(sessions),
        first_step: step(first, mask),
        latest_step: step(latest, mask),
        through_hooks: Some(through_hooks),
        reason: None,
    })
}

/* ──────────────────────────── Deliveries, tallied ──────────────────────────── */
//
// `sensor_events` has no `cwd` index, and none may be added (A193: no DDL),
// so knowing which deliveries are this project's means reading the table
// once. On the founder's store (289,724 rows, 5.4 KB average payload) that
// read, done as two row-returning queries kept in a map, cost 0.4–10 s a
// request. Now it is one aggregate pass, read in parallel shares
// ([`first_tally`]), done once per project per server, and kept in memory:
// `sensor_events` is append-only (its triggers refuse UPDATE and DELETE) and
// ordered by `seq`, so a tally made up to one `seq` stays exact for every row
// up to it, and a later request reads only the rows appended since. A
// project's tally is made again from the start when its rule changes (a
// folder, a member, or a member's session count). Nothing is written
// anywhere; a restart starts empty.

/// What decides which deliveries are this project's: the folders whose `cwd`
/// counts, and each member's ledger project with its session count.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Rule {
    roots: Vec<String>,
    members: Vec<(String, u64)>,
}

/// The deliveries one [`Rule`] matched among the rows up to `through`.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
struct Tally {
    deliveries: u64,
    first_at: Option<String>,
    latest_at: Option<String>,
    /// The matched delivery with the highest `seq`, and its event.
    latest: Option<(i64, String)>,
}

impl Tally {
    /// This tally followed by one over a later, disjoint range of `seq`.
    fn then(mut self, later: Tally) -> Tally {
        self.deliveries += later.deliveries;
        self.first_at = match (self.first_at, later.first_at) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        self.latest_at = match (self.latest_at, later.latest_at) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        self.latest = match (self.latest, later.latest) {
            (Some(a), Some(b)) => Some(if b.0 > a.0 { b } else { a }),
            (a, b) => a.or(b),
        };
        self
    }
}

/// One project's tally, and how far through `sensor_events` it reaches.
struct Kept {
    rule: Rule,
    /// The last `seq` the tally covers and that row's `chain_hash`: the chain
    /// makes the hash stand for every row before it, so a store replaced
    /// under the same path is never mistaken for the one the tally read.
    through: Option<(i64, String)>,
    tally: Tally,
}

/// Every tally this server has made, by store and HQ project. One entry per
/// project ever asked about; each is a few short strings.
static KEPT: Mutex<BTreeMap<(PathBuf, String), Kept>> = Mutex::new(BTreeMap::new());

fn through_hooks(
    ledger: &Ledger,
    db: &Path,
    project_id: &str,
    rule: &Rule,
) -> thruline_core::error::Result<Tally> {
    let head: Option<(i64, String)> = {
        let mut stmt = ledger
            .conn()
            .prepare("SELECT seq, chain_hash FROM sensor_events ORDER BY seq DESC LIMIT 1")?;
        let mut rows = stmt.query([])?;
        match rows.next()? {
            Some(r) => Some((r.get(0)?, r.get(1)?)),
            None => None,
        }
    };
    let key = (db.to_path_buf(), project_id.to_string());
    let kept = KEPT.lock().ok().and_then(|kept| {
        kept.get(&key)
            .filter(|k| &k.rule == rule)
            .map(|k| (k.through.clone(), k.tally.clone()))
    });
    let tally = match (kept, &head) {
        // Nothing appended since.
        (Some((through, tally)), _) if through == head => tally,
        // Rows appended since, after the very rows the tally read.
        (Some((Some((at, hash)), tally)), Some((upto, _)))
            if at < *upto && chain_hash_at(ledger, at)?.as_deref() == Some(hash.as_str()) =>
        {
            tally.then(tally_span(
                ledger,
                rule,
                Span::Since {
                    after: at,
                    upto: *upto,
                },
            )?)
        }
        // Nothing kept, the rule changed, or the store is not the one read.
        _ => match &head {
            Some((upto, _)) => first_tally(ledger, db, rule, *upto)?,
            None => Tally::default(),
        },
    };
    if let Ok(mut kept) = KEPT.lock() {
        kept.insert(
            key,
            Kept {
                rule: rule.clone(),
                through: head,
                tally: tally.clone(),
            },
        );
    }
    Ok(tally)
}

fn chain_hash_at(ledger: &Ledger, seq: i64) -> thruline_core::error::Result<Option<String>> {
    let mut stmt = ledger
        .conn()
        .prepare("SELECT chain_hash FROM sensor_events WHERE seq = ?1")?;
    let mut rows = stmt.query([seq])?;
    Ok(match rows.next()? {
        Some(r) => Some(r.get(0)?),
        None => None,
    })
}

/// One value bound to the tally's query.
enum Arg {
    Text(String),
    Int(i64),
}

/// Which rows one pass reads.
#[derive(Clone, Copy, Debug)]
enum Span {
    /// `after < seq <= upto`, walking the `seq` index: the rows appended
    /// since a tally was made.
    Since { after: i64, upto: i64 },
    /// `lo < rowid <= hi` (either end open) with `seq <= upto`, walking the
    /// table itself: one reader's share of a first tally. `+seq` keeps the
    /// planner off the `seq` index, which would cost a lookup per row.
    Rows {
        lo: Option<i64>,
        hi: Option<i64>,
        upto: i64,
    },
}

/// The first tally for a rule: every row up to `upto`, read once. The rows
/// are cut by `rowid` into shares read at the same time, each on its own
/// connection, because a cold read of the table is many scattered pages
/// (171k leaf pages, 667 MB, on the founder's store) and the disk answers
/// many reads at once far faster than one after another: 3.5 s for one
/// reader, 0.9 s for sixteen, on a fresh clone of that store.
///
/// The shares are exact together. They cover every `rowid` (the first has no
/// lower end and the last no upper one), each row is in exactly one, and the
/// `seq <= upto` bound means every reader counts the same rows whatever was
/// appended while it read: a row's `seq` is fixed when it is appended, and
/// every row up to `upto` was committed before `upto` was read.
fn first_tally(
    ledger: &Ledger,
    db: &Path,
    rule: &Rule,
    upto: i64,
) -> thruline_core::error::Result<Tally> {
    let max_rowid: i64 =
        ledger
            .conn()
            .query_row("SELECT IFNULL(MAX(rowid), 0) FROM sensor_events", [], |r| {
                r.get(0)
            })?;
    let readers = (max_rowid.max(1) as usize).min(core_hq::PARALLEL_READERS) as i64;
    if readers <= 1 {
        let all = Span::Rows {
            lo: None,
            hi: None,
            upto,
        };
        return tally_span(ledger, rule, all);
    }
    let shares: Vec<Span> = (0..readers)
        .map(|i| Span::Rows {
            lo: (i > 0).then(|| max_rowid * i / readers),
            hi: (i + 1 < readers).then(|| max_rowid * (i + 1) / readers),
            upto,
        })
        .collect();
    std::thread::scope(|scope| {
        let handles: Vec<_> = shares
            .into_iter()
            .map(|share| {
                scope.spawn(move || {
                    let reader = Ledger::open(db)?;
                    tally_span(&reader, rule, share)
                })
            })
            .collect();
        let mut total = Tally::default();
        for handle in handles {
            let part = handle.join().unwrap_or_else(|_| {
                Err(thruline_core::error::CoreError::Config(
                    "a reader of the hook deliveries stopped".to_string(),
                ))
            })?;
            total = total.then(part);
        }
        Ok(total)
    })
}

/// The deliveries `rule` matches among the rows `span` names, in one pass.
fn tally_span(ledger: &Ledger, rule: &Rule, span: Span) -> thruline_core::error::Result<Tally> {
    use Arg::{Int, Text};
    let conn = ledger.conn();
    let mut terms: Vec<String> = Vec::new();
    let mut args: Vec<Arg> = Vec::new();
    for root in &rule.roots {
        let inside = format!("{root}/");
        terms.push(format!(
            "cwd = ?{} OR substr(cwd, 1, ?{}) = ?{}",
            args.len() + 1,
            args.len() + 2,
            args.len() + 3
        ));
        args.push(Text(root.clone()));
        args.push(Int(inside.chars().count() as i64));
        args.push(Text(inside));
    }
    if !rule.members.is_empty() {
        let marks: Vec<String> = (0..rule.members.len())
            .map(|i| format!("?{}", args.len() + 1 + i))
            .collect();
        terms.push(format!(
            "session_id IN (SELECT id FROM sessions WHERE project_id IN ({}))",
            marks.join(",")
        ));
        for (id, _) in &rule.members {
            args.push(Text(id.clone()));
        }
    }
    if terms.is_empty() {
        return Ok(Tally::default());
    }
    let range = match span {
        Span::Since { after, upto } => {
            args.push(Int(after));
            args.push(Int(upto));
            format!("seq > ?{} AND seq <= ?{}", args.len() - 1, args.len())
        }
        Span::Rows { lo, hi, upto } => {
            args.push(Int(upto));
            let mut range = format!("+seq <= ?{}", args.len());
            if let Some(lo) = lo {
                args.push(Int(lo));
                range.push_str(&format!(" AND rowid > ?{}", args.len()));
            }
            if let Some(hi) = hi {
                args.push(Int(hi));
                range.push_str(&format!(" AND rowid <= ?{}", args.len()));
            }
            range
        }
    };
    let sql = format!(
        "SELECT COUNT(*), MIN(received_at), MAX(received_at), MAX(seq) \
         FROM sensor_events WHERE {range} AND ({})",
        terms.join(" OR ")
    );
    let mut stmt = conn.prepare(&sql)?;
    for (i, arg) in args.iter().enumerate() {
        match arg {
            Text(t) => stmt.raw_bind_parameter(i + 1, t)?,
            Int(n) => stmt.raw_bind_parameter(i + 1, n)?,
        }
    }
    let (count, first_at, latest_at, last): (i64, Option<String>, Option<String>, Option<i64>) = {
        let mut rows = stmt.raw_query();
        match rows.next()? {
            Some(r) => (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?),
            None => (0, None, None, None),
        }
    };
    let latest = match last {
        Some(seq) => Some((
            seq,
            conn.query_row(
                "SELECT hook_event FROM sensor_events WHERE seq = ?1",
                [seq],
                |r| r.get::<_, String>(0),
            )?,
        )),
        None => None,
    };
    Ok(Tally {
        deliveries: count as u64,
        first_at,
        latest_at,
        latest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use thruline_core::capture_hold::ExclusionView;

    #[test]
    fn capture_health_hides_a_matching_hook_but_keeps_the_exclusion_control() {
        let policy = thruline_core::capture_exclusion::ExclusionPolicy::from_exclusions(
            "/synthetic/shop",
            &[ExclusionView {
                key: ".claude/settings.json".into(),
                covers: ".claude/settings.json".into(),
                note: None,
                built_in: false,
                on: true,
                source: "recorded",
                basis: None,
                since: None,
                by: None,
            }],
        )
        .unwrap();
        let mut doc = serde_json::json!({
            "project": {"id": "shop", "root": "/synthetic/shop"},
            "claude_code_hooks": {"levels": [{
                "path": "/synthetic/shop/.claude/settings.json",
                "state": "read",
                "commands": [{"command": "cat .claude/settings.json", "kind": "other"}]
            }]},
            "holds": {"exclusions": [{"key": ".claude/settings.json", "on": true}]}
        });
        let standing = doc["holds"].clone();
        assert_eq!(present_health(&mut doc, &policy).unwrap(), 1);
        assert_eq!(doc["holds"], standing);
        assert_eq!(doc["project"]["id"], "shop");
        assert_eq!(doc["claude_code_hooks"]["levels"][0]["state"], "read");
        assert_eq!(doc["left_out_by_exclusion"], 1);
        let shown = doc["claude_code_hooks"].to_string();
        assert!(!shown.contains("/synthetic/shop/.claude/settings.json"));
        assert!(!shown.contains("cat .claude/settings.json"));
        assert!(shown.contains("left out by your exclusion"));
    }

    fn tally(n: u64, first: &str, last: &str, seq: i64, event: &str) -> Tally {
        Tally {
            deliveries: n,
            first_at: Some(first.to_string()),
            latest_at: Some(last.to_string()),
            latest: Some((seq, event.to_string())),
        }
    }

    #[test]
    fn a_tally_followed_by_a_later_one_is_their_union() {
        let a = tally(2, "2026-10-01T09:00:00Z", "2026-10-01T09:05:00Z", 4, "Stop");
        let b = tally(
            3,
            "2026-10-01T08:00:00Z",
            "2026-10-01T09:01:00Z",
            9,
            "SessionEnd",
        );
        let both = a.clone().then(b.clone());
        assert_eq!(both.deliveries, 5);
        assert_eq!(both.first_at.as_deref(), Some("2026-10-01T08:00:00Z"));
        assert_eq!(both.latest_at.as_deref(), Some("2026-10-01T09:05:00Z"));
        assert_eq!(both.latest, Some((9, "SessionEnd".to_string())));
        assert_eq!(a.clone().then(Tally::default()), a);
        assert_eq!(Tally::default().then(b.clone()), b);
    }
}
