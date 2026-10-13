//! HQ Next — First run's Install (C30 R-ROUTES "Install": hooks, MCP, speaker
//! and shim, exactly as the person ticked them; the click is the consent; it
//! runs only on their project).
//!
//!   - `GET  /api/hq/projects/<id>/install` — the plan: for each of the four
//!     parts, the one file it would write, the entries it would add, where it
//!     is already installed, or why it would be refused; and a `plan` digest.
//!     Writes nothing.
//!   - `POST /api/hq/projects/<id>/install` — `{hooks, mcp, speaker,
//!     run_capture, plan}` and nothing else. Installs exactly the ticked parts
//!     through the existing installers, and only when `plan` is still the plan
//!     (so the click applies what the page showed). Same-origin AND this
//!     launch's run token, checked by the run route's own two guards before the
//!     body is read or the store is opened.
//!
//! **Which file each part writes** (and nothing else):
//!
//!   - Claude Code hooks → `<project>/.claude/settings.local.json`, the
//!     project-local level Claude Code reads for this folder only (each hook
//!     names this machine's ThruLine program, so it belongs in the personal,
//!     unshared file, not `.claude/settings.json`). Entries: the `thruline
//!     sensor install` pack, through `sensor_install_into` — `thruline init`'s
//!     own merge.
//!   - Speaker hooks → the same file, through `init_speaker_install_into`.
//!   - MCP server → `<project>/.mcp.json`, Claude Code's project-level server
//!     list, through `init_mcp_install_into` (the `--db` it names is this HQ's
//!     store). Claude Code asks once before it uses a server from that file.
//!   - Run capture → the shim folder in ThruLine's own home
//!     (`$THRULINE_HOME/shim`, else `~/.thruline/shim`), through `shim install`'s
//!     own options (`shim_cmd::install_options`). It is not the agent's or the
//!     project's file, and it records nothing until the person adds the one
//!     line it needs to their shell profile — which this route never writes.
//!
//! **Never written:** `~/.claude/settings.json`, `~/.claude.json`, a project's
//! shared `.claude/settings.json`, a shell profile, the ledger. The route
//! appends nothing to the record (the installers it calls append nothing
//! either) and adds no DDL.
//!
//! **Refused rather than written:** a file or folder on the way that is a
//! link (a rename replaces a name and never follows one, and the route refuses
//! before that); a file that is not JSON or not Claude Code's shape; hooks that
//! would put a second ThruLine recorder on an event Claude Code merges for this
//! folder (`hook_install::refuse_double_recorder_across`, over this project's
//! levels); hooks and speakers when this HQ reads a store other than the one
//! they write to when they fire (the default store, `~/.thruline/ledger.db`);
//! an `.mcp.json` `thruline` server ThruLine did not write; a shim folder that
//! holds files ThruLine did not write.
//!
//! A part already installed anywhere Claude Code reads for this folder is left
//! as it is and said to be. So a second click writes nothing and says so.

use super::*;

use std::sync::TryLockError;

use serde_json::{json, Value};

use thruline_capture::shim::{self, ShimManifest};
use thruline_core::hq as core_hq;
use thruline_core::ledger::Ledger;
use thruline_core::redact::{mask_recorded, MaskRecord};

use crate::doctor::merged_settings_paths_for;
use crate::hook_install::refuse_double_recorder_across;

/// The shape of both documents. Raised when a field changes meaning.
pub(super) const SCHEMA_VERSION: u32 = 1;

/// The one file the hooks and the speaker hooks are written to, under the
/// project's folder.
pub(super) const HOOKS_FILE: &str = ".claude/settings.local.json";

/// The one file the MCP server is written to, under the project's folder.
pub(super) const MCP_FILE: &str = ".mcp.json";

/// How the shim folder is named on screen: never the home folder's path.
const SHIM_SHOWN: &str = "<ThruLine home>/shim";

/// The terminal command that removes what this route writes in the project.
pub(super) const UNDO_PROJECT: &str =
    "thruline init --undo --settings .claude/settings.local.json --claude-json .mcp.json";

const PREFIX: &str = "/api/hq/projects/";
const TAIL: &str = "/install";
/// C46 §1.2(2): the one part this page can take back, Settings' speaker
/// switch (`POST /api/hq/projects/<id>/uninstall`, write-address row 30).
const UNINSTALL_TAIL: &str = "/uninstall";

/// One install (or removal) at a time in this HQ. A second press while one is
/// writing is answered 409 at once; nothing waits.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

pub(super) fn owns(p: &str) -> bool {
    raw_id(p).is_some() || raw_id_at(p, UNINSTALL_TAIL).is_some()
}

fn raw_id(path: &str) -> Option<&str> {
    raw_id_at(path, TAIL)
}

fn raw_id_at<'a>(path: &'a str, tail: &str) -> Option<&'a str> {
    let id = path.strip_prefix(PREFIX)?.strip_suffix(tail)?;
    (!id.is_empty() && !id.contains('/')).then_some(id)
}

fn json_answer(stream: &mut TcpStream, status: u16, value: &Value) -> Result<()> {
    respond(
        stream,
        status,
        "application/json; charset=utf-8",
        &serde_json::to_vec(value)?,
    )
}

fn refuse(stream: &mut TcpStream, status: u16, error: &str) -> Result<()> {
    json_answer(stream, status, &json!({ "error": error }))
}

/// The body: exactly the four choices and the plan they were made against.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    /// Locked on in the design ("Needed to record"); `false` is refused.
    hooks: bool,
    mcp: bool,
    speaker: bool,
    run_capture: bool,
    /// The `plan` of the GET the page showed.
    plan: String,
}

/// The removal's body: the one part, and the plan the page showed.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoveRequest {
    /// `speaker`, and nothing else in this version.
    part: String,
    /// The `plan` of the install GET the page showed.
    plan: String,
}

pub(super) fn handle(
    state: &Arc<State>,
    method: &str,
    path: &str,
    query: &str,
    headers: &RequestHeaders,
    reader: &mut BufReader<TcpStream>,
    stream: &mut TcpStream,
) -> Result<()> {
    if !query.is_empty() {
        return refuse(stream, 400, "unknown_query_parameter");
    }
    let removal = raw_id_at(path, UNINSTALL_TAIL).is_some();
    let id = raw_id_at(path, if removal { UNINSTALL_TAIL } else { TAIL })
        .and_then(percent_decode)
        .filter(|id| !id.is_empty() && !id.contains('/'));
    let Some(id) = id else {
        return refuse(stream, 400, "malformed_request");
    };
    if removal {
        if method != "POST" {
            return refuse(stream, 405, "method_not_allowed");
        }
        // The same two guards as an install, before the body is read and
        // before the store is opened.
        if !super::run_route::origin_is_ours(state, headers) {
            return refuse(stream, 403, "same_origin_required");
        }
        if !super::run_route::token_is_ours(state, headers) {
            return refuse(stream, 403, "run_token_required");
        }
        let body = match read_interpret_body(reader, headers) {
            Ok(body) => body,
            Err(BodyProblem::TooLarge) => return refuse(stream, 413, "request_too_large"),
            Err(BodyProblem::BadRequest) => return refuse(stream, 400, "malformed_request"),
        };
        let request: RemoveRequest = match serde_json::from_slice(&body) {
            Ok(r) => r,
            Err(_) => return refuse(stream, 400, "invalid_uninstall_request"),
        };
        if request.part != "speaker" {
            return refuse(stream, 400, "unknown_part");
        }
        return remove_speaker(state, &id, &request, stream);
    }
    match method {
        "GET" => {
            if headers.malformed
                || !headers.content_lengths.is_empty()
                || !headers.content_types.is_empty()
                || headers.transfer_encoding
            {
                return refuse(stream, 400, "malformed_request");
            }
            let project = match project_of(state, &id) {
                Ok(p) => p,
                Err((status, body)) => return json_answer(stream, status, &body),
            };
            let policy = (|| -> Result<_> {
                let ledger = Ledger::open(&state.db)?;
                crate::exclusion_serve::policy(&ledger, &project.root)
            })();
            let policy = match policy {
                Ok(policy) => policy,
                Err(problem) if super::error_is_store_held(&problem) => {
                    return super::store_held(state, stream)
                }
                Err(_) => return refuse(stream, 503, "exclusion_policy_unavailable"),
            };
            let mut mask = Masker::default();
            let survey = survey(state, &project, &mut mask);
            let doc = plan_document(&project, &survey, &mut mask);
            let raw = serde_json::to_vec(&doc)?;
            match present_plan(&raw, &policy) {
                Ok(served) => respond(stream, 200, "application/json; charset=utf-8", &served),
                Err(_) => refuse(stream, 503, "exclusion_policy_unavailable"),
            }
        }
        "POST" => {
            // The run route's own guards, before the body is read and before
            // the store is opened: one Origin, this server's; one token, this
            // launch's.
            if !super::run_route::origin_is_ours(state, headers) {
                return refuse(stream, 403, "same_origin_required");
            }
            if !super::run_route::token_is_ours(state, headers) {
                return refuse(stream, 403, "run_token_required");
            }
            let body = match read_interpret_body(reader, headers) {
                Ok(body) => body,
                Err(BodyProblem::TooLarge) => return refuse(stream, 413, "request_too_large"),
                Err(BodyProblem::BadRequest) => return refuse(stream, 400, "malformed_request"),
            };
            let request: Request = match serde_json::from_slice(&body) {
                Ok(r) => r,
                Err(_) => return refuse(stream, 400, "invalid_install_request"),
            };
            if !request.hooks {
                return refuse(stream, 400, "hooks_required");
            }
            apply(state, &id, &request, stream)
        }
        _ => refuse(stream, 405, "method_not_allowed"),
    }
}

fn present_plan<'a>(
    raw: &'a [u8],
    policy: &thruline_core::capture_exclusion::ExclusionPolicy,
) -> Result<std::borrow::Cow<'a, [u8]>> {
    crate::exclusion_serve::bytes(raw, policy)
}

fn exclusion_policy(
    state: &State,
    project: &core_hq::HqProject,
) -> Result<thruline_core::capture_exclusion::ExclusionPolicy> {
    let ledger = Ledger::open(&state.db)?;
    crate::exclusion_serve::policy(&ledger, &project.root)
}

fn presented_answer(
    stream: &mut TcpStream,
    status: u16,
    value: &Value,
    policy: &thruline_core::capture_exclusion::ExclusionPolicy,
) -> Result<()> {
    let raw = serde_json::to_vec(value)?;
    match present_plan(&raw, policy) {
        Ok(served) => respond(stream, status, "application/json; charset=utf-8", &served),
        Err(_) => refuse(stream, 503, "exclusion_policy_unavailable"),
    }
}

#[cfg(test)]
mod exclusion_plan_tests {
    use super::*;
    use thruline_core::capture_hold::ExclusionView;

    fn policy_for(key: &str) -> thruline_core::capture_exclusion::ExclusionPolicy {
        thruline_core::capture_exclusion::ExclusionPolicy::from_exclusions(
            "/synthetic/project",
            &[ExclusionView {
                key: key.into(),
                covers: key.into(),
                note: None,
                built_in: false,
                on: true,
                source: "recorded",
                basis: None,
                since: None,
                by: None,
            }],
        )
        .expect("synthetic policy")
    }

    #[test]
    fn an_excluded_install_target_hides_its_plan_entries_but_keeps_the_plan_id() {
        let raw = br#"{"plan":"synthetic-digest","parts":{"hooks":{"file":".claude/settings.local.json","state":"will_add","adds":["PRIVATE_HOOK_ENTRY"]}}}"#;
        assert!(
            matches!(present_plan(raw, &policy_for("other")), Ok(std::borrow::Cow::Borrowed(bytes)) if bytes == raw)
        );
        let served =
            present_plan(raw, &policy_for(".claude/settings.local.json")).expect("excluded plan");
        let doc: Value = serde_json::from_slice(&served).expect("served JSON");
        assert_eq!(doc["plan"], "synthetic-digest");
        assert_eq!(
            doc["parts"]["hooks"]["file"],
            thruline_core::capture_exclusion::marker(".claude/settings.local.json")
        );
        assert_eq!(doc["parts"]["hooks"]["state"], "will_add");
        assert_eq!(doc["left_out_by_exclusion"], 1);
        assert!(!String::from_utf8_lossy(&served).contains("PRIVATE_HOOK_ENTRY"));
    }

    #[test]
    fn an_excluded_install_target_hides_its_written_entries_and_after_plan() {
        let raw = br#"{"changed":true,"parts":{"hooks":{"outcome":"wrote","file":".claude/settings.local.json","entries":["PRIVATE_HOOK_ENTRY"]}},"after":{"parts":{"hooks":{"file":".claude/settings.local.json","adds":["PRIVATE_AFTER_ENTRY"]}}}}"#;
        let served = present_plan(raw, &policy_for(".claude/settings.local.json"))
            .expect("excluded install result");
        let doc: Value = serde_json::from_slice(&served).expect("served JSON");
        assert_eq!(doc["changed"], true);
        assert_eq!(doc["parts"]["hooks"]["outcome"], "wrote");
        assert_eq!(doc["left_out_by_exclusion"], 2);
        let served = String::from_utf8_lossy(&served);
        assert!(!served.contains("PRIVATE_HOOK_ENTRY"));
        assert!(!served.contains("PRIVATE_AFTER_ENTRY"));
    }
}

fn project_of(state: &State, id: &str) -> std::result::Result<core_hq::HqProject, (u16, Value)> {
    let ledger = Ledger::open(&state.db).map_err(|problem| {
        if matches!(&problem, thruline_core::error::CoreError::StoreHeld { .. }) {
            super::note_store_held(&state.store_held_refusals);
            return (
                503,
                json!({
                    "error": "store_held",
                    "message": "ThruLine is removing a project's records; try again when it finishes"
                }),
            );
        }
        (
            503,
            json!({"error": "store_unavailable", "detail": problem.to_string()}),
        )
    })?;
    match core_hq::find_project(&ledger, id) {
        Ok(Some(p)) => Ok(p),
        Ok(None) => Err((404, json!({"error": "no_such_project"}))),
        Err(e) => Err((
            500,
            json!({"error": "read_failed", "detail": e.to_string()}),
        )),
    }
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
}

/* ───────────────────────────────── the survey ───────────────────────────────── */

/// Why a part is not written, in a code and a sentence.
#[derive(Debug, Clone, Serialize)]
struct Refusal {
    code: &'static str,
    plain: String,
}

impl Refusal {
    fn new(code: &'static str, plain: impl Into<String>) -> Refusal {
        Refusal {
            code,
            plain: plain.into(),
        }
    }
}

/// Where a part stands before anything is written.
#[derive(Debug, Clone)]
enum Standing {
    /// The click would add these entries.
    WillAdd(Vec<String>),
    /// Every entry is already in a file Claude Code reads for this folder
    /// (or, for run capture, in ThruLine's home): these levels.
    Installed(Vec<String>),
    Refused(Refusal),
}

/// One file the route may write, as read before anything is written.
#[derive(Debug, Clone)]
struct Target {
    path: PathBuf,
    /// The file's JSON (`{}` when it is absent) and its bytes' digest
    /// (`absent` when there is no file), or why it cannot be used.
    read: std::result::Result<(Value, String), Refusal>,
}

#[derive(Debug, Clone)]
struct RunCapture {
    standing: Standing,
    dir: Option<PathBuf>,
    /// The wrappers an intact shim holds.
    wrappers: Vec<String>,
    path_line: Option<String>,
    on_path: Option<bool>,
}

struct Survey {
    root: PathBuf,
    db: PathBuf,
    levels: Vec<(&'static str, PathBuf)>,
    settings: Target,
    mcp_file: Target,
    hooks: Standing,
    speaker: Standing,
    mcp: Standing,
    run: RunCapture,
    undo_project: bool,
    undo_user: bool,
    plan: String,
}

/// A key two paths share exactly when they name the same file, whether or
/// not the file exists yet.
fn same_place(a: &Path, b: &Path) -> bool {
    let key = |p: &Path| {
        let parent = p.parent().unwrap_or(Path::new("."));
        match (std::fs::canonicalize(parent), p.file_name()) {
            (Ok(dir), Some(name)) => dir.join(name),
            _ => p.to_path_buf(),
        }
    };
    key(a) == key(b)
}

fn digest(bytes: &[u8]) -> String {
    thruline_core::chain::sha256_hex(bytes)
}

/// Read one of the two files the route may write, refusing a link anywhere
/// between the project's folder and the file.
fn read_target(root: &Path, rel: &str) -> Target {
    let path = root.join(rel);
    let shown = rel.to_string();
    let parts: Vec<&str> = rel.split('/').collect();
    let mut at = root.to_path_buf();
    for dir in &parts[..parts.len().saturating_sub(1)] {
        at.push(dir);
        match std::fs::symlink_metadata(&at) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Target {
                    path,
                    read: Err(Refusal::new(
                        "symlink",
                        format!(
                            "The {dir} folder in this project is a link to somewhere else, so ThruLine left it alone. Nothing was added."
                        ),
                    )),
                }
            }
            Ok(meta) if !meta.is_dir() => {
                return Target {
                    path,
                    read: Err(Refusal::new(
                        "not_a_folder",
                        format!("{dir} in this project is not a folder, so ThruLine left it alone. Nothing was added."),
                    )),
                }
            }
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {
                return Target {
                    path,
                    read: Ok((json!({}), "absent".to_string())),
                }
            }
            Err(e) => {
                return Target {
                    path,
                    read: Err(Refusal::new(
                        "unreadable",
                        format!("{dir} in this project could not be read ({e}), so ThruLine left it alone. Nothing was added."),
                    )),
                }
            }
        }
    }
    let read = match std::fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(Refusal::new(
            "symlink",
            format!("{shown} is a link to somewhere else, so ThruLine left it alone. Nothing was added."),
        )),
        Ok(meta) if !meta.is_file() => Err(Refusal::new(
            "not_a_file",
            format!("{shown} is not a regular file, so ThruLine left it alone. Nothing was added."),
        )),
        Ok(_) => match std::fs::read(&path) {
            Err(e) => Err(Refusal::new(
                "unreadable",
                format!("{shown} could not be read ({e}), so ThruLine left it alone. Nothing was added."),
            )),
            Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
                Ok(value) if value.is_object() => Ok((value, digest(&bytes))),
                _ => Err(Refusal::new(
                    "not_json",
                    format!("{shown} is not a JSON object, so ThruLine left it alone. Nothing was added."),
                )),
            },
        },
        Err(e) if e.kind() == ErrorKind::NotFound => Ok((json!({}), "absent".to_string())),
        Err(e) => Err(Refusal::new(
            "unreadable",
            format!("{shown} could not be read ({e}), so ThruLine left it alone. Nothing was added."),
        )),
    };
    Target { path, read }
}

/// One settings level Claude Code merges for this folder: its JSON (`{}`
/// when absent), or `None` when it exists and could not be read as JSON.
fn read_level(path: &Path) -> Option<Value> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str::<Value>(&text).ok(),
        Err(e) if e.kind() == ErrorKind::NotFound => Some(json!({})),
        Err(_) => None,
    }
}

/// `scope` names joined for a sentence: "user", "user and managed".
fn scopes_phrase(scopes: &[String]) -> String {
    match scopes {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn survey(state: &State, project: &core_hq::HqProject, mask: &mut Masker) -> Survey {
    let root = PathBuf::from(&project.root);
    let levels = merged_settings_paths_for(Some(&root));
    let settings = read_target(&root, HOOKS_FILE);
    let mcp_file = read_target(&root, MCP_FILE);
    // A folder nothing has recorded in has no stated path: its `root` is only
    // its folder name decoded, which is not a directory ThruLine may write into.
    let folder_here = project.path_known() && std::fs::metadata(&root).is_ok_and(|m| m.is_dir());
    let missing_folder = || {
        Refusal::new(
            "folder_missing",
            if project.path_known() {
                "This project’s folder is not on this machine, so nothing can be installed in it."
            } else {
                "ThruLine does not know where this folder is until its first recording, so nothing can be installed in it yet."
            },
        )
    };
    // The hooks and the speakers name no store: when they fire they write to
    // the default one. This HQ must be reading that one, or what they record
    // would never show here.
    let same_store = thruline_core::ingest::default_db_path()
        .map(|default| same_place(&default, &state.db))
        .unwrap_or(false);
    let other_store = || {
        Refusal::new(
            "other_store",
            "The hooks write to the record in ThruLine’s usual place, and this HQ reads a different one, so nothing they recorded would show here. Nothing was added.",
        )
    };

    // Every level Claude Code merges for this folder, the target read as the
    // route read it, the others as the harness would.
    let mut level_roots: Vec<(&'static str, bool, Option<Value>)> = Vec::new();
    for (scope, path) in &levels {
        let is_target = same_place(path, &settings.path);
        let value = if is_target {
            settings.read.as_ref().ok().map(|(v, _)| v.clone())
        } else {
            read_level(path)
        };
        level_roots.push((*scope, is_target, value));
    }
    let unreadable_elsewhere: Vec<String> = level_roots
        .iter()
        .filter(|(_, is_target, v)| !is_target && v.is_none())
        .map(|(s, _, _)| s.to_string())
        .collect();

    let hooks = if !folder_here {
        Standing::Refused(missing_folder())
    } else if !same_store {
        Standing::Refused(other_store())
    } else {
        match &settings.read {
            Err(r) => Standing::Refused(r.clone()),
            Ok((current, _)) => {
                let mut found: Vec<String> = Vec::new();
                let mut all = true;
                for event in thruline_core::sensor::install_set() {
                    let mut here = false;
                    for (scope, _, value) in &level_roots {
                        if value.as_ref().is_some_and(|v| {
                            crate::sensor_entry_present(v, event, &crate::command_suffix(event))
                        }) {
                            here = true;
                            if !found.iter().any(|f| f.as_str() == *scope) {
                                found.push(scope.to_string());
                            }
                        }
                    }
                    all &= here;
                }
                if all {
                    Standing::Installed(found)
                } else if !unreadable_elsewhere.is_empty() {
                    Standing::Refused(Refusal::new(
                        "unreadable_level",
                        format!(
                            "The {} Claude Code settings could not be read, so ThruLine cannot tell whether hooks here would record every step twice. Nothing was added.",
                            scopes_phrase(&unreadable_elsewhere)
                        ),
                    ))
                } else {
                    let mut after = current.clone();
                    match crate::sensor_install_into(&mut after, None) {
                        Err(e) => Standing::Refused(Refusal::new(
                            "unexpected_shape",
                            mask.text(&format!(
                                "{HOOKS_FILE} does not have the shape Claude Code uses ({e}), so ThruLine left it alone. Nothing was added."
                            )),
                        )),
                        Ok((added, _)) => {
                            match refuse_double_recorder_across(&levels, &settings.path, &after) {
                                Ok(()) => Standing::WillAdd(
                                    added.iter().map(|e| e.to_string()).collect(),
                                ),
                                Err(_) => {
                                    let holders: Vec<String> = level_roots
                                        .iter()
                                        .filter(|(_, is_target, _)| !is_target)
                                        .filter(|(_, _, v)| v.as_ref().is_some_and(holds_recorder))
                                        .map(|(s, _, _)| s.to_string())
                                        .collect();
                                    Standing::Refused(Refusal::new(
                                        "another_recorder",
                                        format!(
                                            "Another ThruLine recorder is already in the {} Claude Code settings this folder reads. Two would record every step twice, so nothing was added. In a terminal there, thruline doctor names it and how to remove it.",
                                            if holders.is_empty() { "other".to_string() } else { scopes_phrase(&holders) }
                                        ),
                                    ))
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    let speaker = if !folder_here {
        Standing::Refused(missing_folder())
    } else if !same_store {
        Standing::Refused(other_store())
    } else {
        match &settings.read {
            Err(r) => Standing::Refused(r.clone()),
            Ok((current, _)) => {
                let mut found: Vec<String> = Vec::new();
                // Levels other than the one this route writes that already
                // hold a speaker hook: adding the rest here would split them.
                let mut elsewhere: Vec<String> = Vec::new();
                let mut all = true;
                for (event, verb, _) in crate::SPEAKER_HOOKS {
                    let suffix = crate::speaker_suffix(verb);
                    let mut here = false;
                    for (scope, is_target, value) in &level_roots {
                        if value
                            .as_ref()
                            .is_some_and(|v| crate::sensor_entry_present(v, event, &suffix))
                        {
                            here = true;
                            if !found.iter().any(|f| f.as_str() == *scope) {
                                found.push(scope.to_string());
                            }
                            if !is_target && !elsewhere.iter().any(|f| f.as_str() == *scope) {
                                elsewhere.push(scope.to_string());
                            }
                        }
                    }
                    all &= here;
                }
                if all {
                    Standing::Installed(found)
                } else if !elsewhere.is_empty() {
                    Standing::Refused(Refusal::new(
                        "split",
                        format!(
                            "Some speaker hooks are already in the {} Claude Code settings, and adding the rest here would split them across two files. Nothing was added.",
                            scopes_phrase(&elsewhere)
                        ),
                    ))
                } else {
                    let mut after = current.clone();
                    match crate::init_speaker_install_into(&mut after, None) {
                        Ok((added, _)) => {
                            Standing::WillAdd(added.iter().map(|e| e.to_string()).collect())
                        }
                        Err(e) => Standing::Refused(Refusal::new(
                            "unexpected_shape",
                            mask.text(&format!(
                                "{HOOKS_FILE} does not have the shape Claude Code uses ({e}), so ThruLine left it alone. Nothing was added."
                            )),
                        )),
                    }
                }
            }
        }
    };

    let user_config = std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".claude.json"))
        .and_then(|p| read_level(&p));
    let ours_at = |v: Option<&Value>| {
        v.and_then(|v| v.get("mcpServers"))
            .and_then(|s| s.get(crate::MCP_SERVER_KEY))
            .is_some_and(crate::init_mcp_entry_is_ours)
    };
    let mcp = if !folder_here {
        Standing::Refused(missing_folder())
    } else {
        match &mcp_file.read {
            Err(r) => Standing::Refused(r.clone()),
            Ok((current, _)) => {
                let mut found: Vec<String> = Vec::new();
                if ours_at(Some(current)) {
                    found.push("project".to_string());
                }
                if ours_at(user_config.as_ref()) {
                    found.push("user".to_string());
                }
                let local = user_config
                    .as_ref()
                    .and_then(|v| v.get("projects"))
                    .and_then(|p| p.get(project.root.as_str()));
                if ours_at(local) {
                    found.push("local".to_string());
                }
                if !found.is_empty() {
                    Standing::Installed(found)
                } else if current
                    .get("mcpServers")
                    .and_then(|s| s.get(crate::MCP_SERVER_KEY))
                    .is_some()
                {
                    Standing::Refused(Refusal::new(
                        "name_taken",
                        "A server named thruline is already in .mcp.json, and ThruLine did not write it, so it was left alone.",
                    ))
                } else {
                    let mut after = current.clone();
                    match crate::init_mcp_install_into(&mut after, &state.db) {
                        Ok(true) => {
                            Standing::WillAdd(vec![format!("mcpServers.{}", crate::MCP_SERVER_KEY)])
                        }
                        Ok(false) => Standing::Installed(vec!["project".to_string()]),
                        Err(e) => Standing::Refused(Refusal::new(
                            "unexpected_shape",
                            mask.text(&format!(
                                "{MCP_FILE} does not have the shape Claude Code uses ({e}), so ThruLine left it alone. Nothing was added."
                            )),
                        )),
                    }
                }
            }
        }
    };

    let run = run_capture(folder_here, mask);

    // What a terminal undo would find to remove.
    let undo_project = settings.read.as_ref().is_ok_and(|(v, _)| holds_ours(v))
        || mcp_file.read.as_ref().is_ok_and(|(v, _)| ours_at(Some(v)));
    let undo_user = level_roots
        .iter()
        .any(|(s, _, v)| *s == "user" && v.as_ref().is_some_and(holds_ours))
        || ours_at(user_config.as_ref());

    let mut survey = Survey {
        root,
        db: state.db.clone(),
        levels,
        settings,
        mcp_file,
        hooks,
        speaker,
        mcp,
        run,
        undo_project,
        undo_user,
        plan: String::new(),
    };
    survey.plan = plan_id(&survey);
    survey
}

/// Does this settings document hold any entry `thruline init` writes?
fn holds_ours(v: &Value) -> bool {
    thruline_core::sensor::install_set()
        .into_iter()
        .any(|e| crate::sensor_entry_present(v, e, &crate::command_suffix(e)))
        || crate::SPEAKER_HOOKS
            .iter()
            .any(|(e, verb, _)| crate::sensor_entry_present(v, e, &crate::speaker_suffix(verb)))
}

/// Does this settings document hold a ThruLine recorder on any event?
fn holds_recorder(v: &Value) -> bool {
    v.get("hooks")
        .and_then(Value::as_object)
        .is_some_and(|hooks| {
            hooks.values().any(|entries| {
                crate::doctor::thruline_commands_on(entries)
                    .iter()
                    .any(|c| crate::doctor::is_recorder(c))
            })
        })
}

fn shown_shim_line(dir: &Path) -> String {
    let text = dir.display().to_string();
    let shown = match std::env::var_os("HOME").filter(|h| !h.is_empty()) {
        Some(home) => {
            let home = PathBuf::from(home).display().to_string();
            match text.strip_prefix(&home) {
                Some(rest) if rest.starts_with('/') => format!("$HOME{rest}"),
                _ => text,
            }
        }
        None => text,
    };
    format!("export PATH=\"{shown}:$PATH\"")
}

fn run_capture(folder_here: bool, mask: &mut Masker) -> RunCapture {
    let refused = |r: Refusal| RunCapture {
        standing: Standing::Refused(r),
        dir: None,
        wrappers: Vec::new(),
        path_line: None,
        on_path: None,
    };
    if !folder_here {
        return refused(Refusal::new(
            "folder_missing",
            "This project’s folder is not on this machine, so nothing can be installed in it.",
        ));
    }
    let home = match crate::shim_cmd::thruline_home() {
        Ok(h) => h,
        Err(_) => {
            return refused(Refusal::new(
                "no_home",
                "Neither THRULINE_HOME nor HOME is set, so ThruLine has no folder to keep run capture in.",
            ))
        }
    };
    let dir = home.join("shim");
    let line = mask.text(&shown_shim_line(&dir));
    let with = |standing: Standing, wrappers: Vec<String>, on_path: Option<bool>| RunCapture {
        standing,
        dir: Some(dir.clone()),
        wrappers,
        path_line: Some(line.clone()),
        on_path,
    };
    match std::fs::symlink_metadata(&dir) {
        Ok(meta) if meta.file_type().is_symlink() => {
            return with(
                Standing::Refused(Refusal::new(
                    "symlink",
                    "The shim folder in ThruLine’s home is a link to somewhere else, so ThruLine left it alone. Nothing was added.",
                )),
                Vec::new(),
                None,
            )
        }
        Ok(meta) if !meta.is_dir() => {
            return with(
                Standing::Refused(Refusal::new(
                    "not_a_folder",
                    "The shim folder in ThruLine’s home is not a folder, so ThruLine left it alone. Nothing was added.",
                )),
                Vec::new(),
                None,
            )
        }
        _ => {}
    }
    let manifest = ShimManifest::load(&dir).ok();
    let integrity = shim::verify(&dir, None);
    let this = std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok());
    let same_program = manifest.as_ref().is_some_and(|m| {
        m.thruline_bin
            .canonicalize()
            .ok()
            .zip(this.as_ref())
            .is_some_and(|(a, b)| &a == b)
    });
    let intact = integrity.installed
        && integrity.problem.is_none()
        && integrity.modified.is_empty()
        && integrity.missing.is_empty()
        && integrity.forged.is_empty()
        && !integrity.captures_nothing;
    if intact && same_program {
        let wrappers = manifest
            .map(|m| m.wrappers.keys().cloned().collect())
            .unwrap_or_default();
        return with(
            Standing::Installed(vec!["thruline-home".to_string()]),
            wrappers,
            Some(integrity.on_path),
        );
    }
    // `shim install` refuses a folder holding files it did not write; say so
    // before the click rather than after it.
    let previous: Vec<String> = manifest
        .as_ref()
        .map(|m| m.wrappers.keys().cloned().collect())
        .unwrap_or_default();
    let mut foreign: Vec<String> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|name| {
                    name != shim::MANIFEST
                        && !shim::WRAPPED.contains(&name.as_str())
                        && !previous.contains(name)
                })
                .collect()
        })
        .unwrap_or_default();
    foreign.sort();
    if !foreign.is_empty() {
        return with(
            Standing::Refused(Refusal::new(
                "occupied",
                mask.text(&format!(
                    "The shim folder in ThruLine’s home holds files ThruLine did not write ({}), so it was left alone. Nothing was added.",
                    foreign.join(", ")
                )),
            )),
            Vec::new(),
            None,
        );
    }
    with(
        Standing::WillAdd(shim::WRAPPED.iter().map(|s| s.to_string()).collect()),
        Vec::new(),
        None,
    )
}

/// The plan's digest: everything the click would do, and the bytes of the
/// two project files it would rewrite. A click carrying another digest was
/// made against a page that no longer says what would happen.
fn plan_id(s: &Survey) -> String {
    let mut text = String::from("thruline-install-plan-v1\0");
    for part in [
        s.root.display().to_string(),
        s.db.display().to_string(),
        std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        s.settings
            .read
            .as_ref()
            .map(|(_, d)| d.clone())
            .unwrap_or_else(|r| format!("refused:{}", r.code)),
        s.mcp_file
            .read
            .as_ref()
            .map(|(_, d)| d.clone())
            .unwrap_or_else(|r| format!("refused:{}", r.code)),
        standing_json(&s.hooks).to_string(),
        standing_json(&s.speaker).to_string(),
        standing_json(&s.mcp).to_string(),
        standing_json(&s.run.standing).to_string(),
        s.run.wrappers.join(","),
    ] {
        text.push_str(&part);
        text.push('\0');
    }
    digest(text.as_bytes())
}

fn standing_json(standing: &Standing) -> Value {
    match standing {
        Standing::WillAdd(adds) => {
            json!({"state": "will_add", "adds": adds, "where": [], "refusal": null})
        }
        Standing::Installed(at) => {
            json!({"state": "installed", "adds": [], "where": at, "refusal": null})
        }
        Standing::Refused(r) => {
            json!({"state": "refused", "adds": [], "where": [], "refusal": r})
        }
    }
}

fn part_json(standing: &Standing, file: &str, level: &str) -> Value {
    let mut v = standing_json(standing);
    v["file"] = Value::from(file);
    v["level"] = Value::from(level);
    v
}

fn plan_document(project: &core_hq::HqProject, s: &Survey, mask: &mut Masker) -> Value {
    let mut run = part_json(&s.run.standing, SHIM_SHOWN, "thruline-home");
    run["wrappers"] = json!(s.run.wrappers);
    run["candidates"] = json!(shim::WRAPPED);
    run["path_line"] = json!(s.run.path_line);
    run["on_path"] = json!(s.run.on_path);
    let shim_installed = matches!(s.run.standing, Standing::Installed(_));
    json!({
        "schema_version": SCHEMA_VERSION,
        "project": {"id": project.id, "root": mask.text(&project.shown_root())},
        "plan": s.plan,
        "parts": {
            "hooks": part_json(&s.hooks, HOOKS_FILE, "project-local"),
            "speaker": part_json(&s.speaker, HOOKS_FILE, "project-local"),
            "mcp": part_json(&s.mcp, MCP_FILE, "project"),
            "run_capture": run,
        },
        "undo": {
            "project": s.undo_project.then_some(UNDO_PROJECT),
            "user": s.undo_user.then_some("thruline init --undo"),
            "shim": shim_installed.then_some("thruline shim uninstall"),
        },
        "masking": mask.record,
    })
}

/* ───────────────────────────────── the click ───────────────────────────────── */

/// What became of one part of the click.
fn outcome(
    word: &str,
    file: &str,
    entries: Vec<String>,
    at: Vec<String>,
    refusal: Option<Refusal>,
) -> Value {
    json!({"outcome": word, "file": file, "entries": entries, "where": at, "refusal": refusal})
}

/// The answer for a part the click did not write: already there, refused,
/// or not chosen.
fn untouched(chosen: bool, standing: &Standing, file: &str) -> Value {
    if !chosen {
        return outcome("not_chosen", file, Vec::new(), Vec::new(), None);
    }
    match standing {
        Standing::Installed(at) => outcome("already", file, Vec::new(), at.clone(), None),
        Standing::Refused(r) => outcome("refused", file, Vec::new(), Vec::new(), Some(r.clone())),
        Standing::WillAdd(_) => outcome(
            "failed",
            file,
            Vec::new(),
            Vec::new(),
            Some(Refusal::new("not_written", "It was not written.")),
        ),
    }
}

fn apply(state: &Arc<State>, id: &str, request: &Request, stream: &mut TcpStream) -> Result<()> {
    let _one = match ONE_AT_A_TIME.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::Poisoned(p)) => p.into_inner(),
        Err(TryLockError::WouldBlock) => return refuse(stream, 409, "install_in_progress"),
    };
    let project = match project_of(state, id) {
        Ok(p) => p,
        Err((status, body)) => return json_answer(stream, status, &body),
    };
    let policy = match exclusion_policy(state, &project) {
        Ok(policy) => policy,
        Err(problem) if super::error_is_store_held(&problem) => {
            return super::store_held(state, stream)
        }
        Err(_) => return refuse(stream, 503, "exclusion_policy_unavailable"),
    };
    let mut mask = Masker::default();
    let survey = survey(state, &project, &mut mask);
    if survey.plan != request.plan {
        let doc = plan_document(&project, &survey, &mut mask);
        return presented_answer(
            stream,
            409,
            &json!({"error": "plan_changed", "plan": doc}),
            &policy,
        );
    }
    // Check generated-file lookup on the current plan before any installer writes.
    let before = plan_document(&project, &survey, &mut mask);
    let before_raw = serde_json::to_vec(&before)?;
    if present_plan(&before_raw, &policy).is_err() {
        return refuse(stream, 503, "exclusion_policy_unavailable");
    }

    let will = |s: &Standing| matches!(s, Standing::WillAdd(_));
    let write_hooks = request.hooks && will(&survey.hooks);
    let write_speaker = request.speaker && will(&survey.speaker);
    let mut hooks = untouched(request.hooks, &survey.hooks, HOOKS_FILE);
    let mut speaker = untouched(request.speaker, &survey.speaker, HOOKS_FILE);
    let mut mcp = untouched(request.mcp, &survey.mcp, MCP_FILE);
    let mut run = untouched(request.run_capture, &survey.run.standing, SHIM_SHOWN);
    let mut wrote_files: Vec<&str> = Vec::new();

    if write_hooks || write_speaker {
        match write_settings(&survey, write_hooks, write_speaker) {
            Ok((h, s)) => {
                if write_hooks {
                    hooks = outcome("wrote", HOOKS_FILE, h, vec!["project-local".into()], None);
                }
                if write_speaker {
                    speaker = outcome("wrote", HOOKS_FILE, s, vec!["project-local".into()], None);
                }
                wrote_files.push(HOOKS_FILE);
            }
            Err(e) => {
                let r = Refusal::new(
                    "write_failed",
                    mask.text(&format!("{HOOKS_FILE} was not written: {e:#}")),
                );
                if write_hooks {
                    hooks = outcome(
                        "failed",
                        HOOKS_FILE,
                        Vec::new(),
                        Vec::new(),
                        Some(r.clone()),
                    );
                }
                if write_speaker {
                    speaker = outcome("failed", HOOKS_FILE, Vec::new(), Vec::new(), Some(r));
                }
            }
        }
    }
    if request.mcp && will(&survey.mcp) {
        match write_mcp(&survey) {
            Ok(entry) => {
                mcp = outcome("wrote", MCP_FILE, vec![entry], vec!["project".into()], None);
                wrote_files.push(MCP_FILE);
            }
            Err(e) => {
                mcp = outcome(
                    "failed",
                    MCP_FILE,
                    Vec::new(),
                    Vec::new(),
                    Some(Refusal::new(
                        "write_failed",
                        mask.text(&format!("{MCP_FILE} was not written: {e:#}")),
                    )),
                );
            }
        }
    }
    if request.run_capture && will(&survey.run.standing) {
        match write_shim(&survey) {
            Ok(wrappers) => {
                run = outcome(
                    "wrote",
                    SHIM_SHOWN,
                    wrappers,
                    vec!["thruline-home".into()],
                    None,
                );
                run["path_line"] = json!(survey.run.path_line);
                wrote_files.push("the shim folder in ThruLine’s home");
            }
            Err(e) => {
                run = outcome(
                    "failed",
                    SHIM_SHOWN,
                    Vec::new(),
                    Vec::new(),
                    Some(Refusal::new(
                        "write_failed",
                        mask.text(&format!("Run capture was not written: {e:#}")),
                    )),
                );
            }
        }
    } else if run["outcome"] == "already" {
        run["path_line"] = json!(survey.run.path_line);
    }

    let parts = [&hooks, &speaker, &mcp, &run];
    let changed = !wrote_files.is_empty();
    let refused: Vec<String> = parts
        .iter()
        .filter(|p| p["outcome"] == "refused" || p["outcome"] == "failed")
        .filter_map(|p| p["refusal"]["plain"].as_str().map(str::to_string))
        .collect();
    let plain = if changed {
        let mut s = format!(
            "Wrote {}.",
            scopes_phrase(
                &wrote_files
                    .iter()
                    .map(|f| f.to_string())
                    .collect::<Vec<_>>()
            )
        );
        if let Some(first) = refused.first() {
            s.push(' ');
            s.push_str(first);
        }
        s
    } else if let Some(first) = refused.first() {
        first.clone()
    } else {
        "Nothing changed: every part you chose was already there.".to_string()
    };

    let after = survey_after(state, &project, &mut mask);
    presented_answer(
        stream,
        200,
        &json!({
            "schema_version": SCHEMA_VERSION,
            "project": {"id": project.id, "root": mask.text(&project.shown_root())},
            "changed": changed,
            "parts": {"hooks": hooks, "speaker": speaker, "mcp": mcp, "run_capture": run},
            "plain": mask.text(&plain),
            "after": after,
        }),
        &policy,
    )
}

/* ──────────────────────────── taking the speaker back ──────────────────────────── */

/// The speaker hooks a settings document holds, as a person reads them
/// ("SessionStart brief --hook session-start").
fn speaker_entries_in(v: &Value) -> Vec<String> {
    crate::SPEAKER_HOOKS
        .iter()
        .filter(|(event, verb, _)| {
            crate::sensor_entry_present(v, event, &crate::speaker_suffix(verb))
        })
        .map(|(event, verb, _)| format!("{event} {verb}"))
        .collect()
}

/// Whether a settings document holds any of ThruLine's speaker hooks.
fn holds_speaker(v: &Value) -> bool {
    crate::SPEAKER_HOOKS
        .iter()
        .any(|(event, verb, _)| crate::sensor_entry_present(v, event, &crate::speaker_suffix(verb)))
}

/// C46 §1.2(2), write-address row 30: Settings' speaker switch, Off. Removes
/// exactly ThruLine's three speaker hooks from the one file this route writes,
/// `<project>/.claude/settings.local.json`, through `thruline init`'s own
/// remover (`init_speaker_uninstall_from`), and nothing else: the sensor
/// pack, an MCP server, run capture, another agent's hooks and every other
/// file stay as they are. Speaker hooks at any other level Claude Code reads
/// for this folder (the user's `~/.claude/settings.json`, a shared
/// `.claude/settings.json`, managed settings) are never touched; the answer
/// says where they are and that they are taken off in a terminal. Appends
/// nothing to the record and adds no DDL. Free.
fn remove_speaker(
    state: &Arc<State>,
    id: &str,
    request: &RemoveRequest,
    stream: &mut TcpStream,
) -> Result<()> {
    let _one = match ONE_AT_A_TIME.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::Poisoned(p)) => p.into_inner(),
        Err(TryLockError::WouldBlock) => return refuse(stream, 409, "install_in_progress"),
    };
    let project = match project_of(state, id) {
        Ok(p) => p,
        Err((status, body)) => return json_answer(stream, status, &body),
    };
    let policy = match exclusion_policy(state, &project) {
        Ok(policy) => policy,
        Err(problem) if super::error_is_store_held(&problem) => {
            return super::store_held(state, stream)
        }
        Err(_) => return refuse(stream, 503, "exclusion_policy_unavailable"),
    };
    let mut mask = Masker::default();
    let survey = survey(state, &project, &mut mask);
    if survey.plan != request.plan {
        let doc = plan_document(&project, &survey, &mut mask);
        return presented_answer(
            stream,
            409,
            &json!({"error": "plan_changed", "plan": doc}),
            &policy,
        );
    }
    let before = plan_document(&project, &survey, &mut mask);
    let before_raw = serde_json::to_vec(&before)?;
    if present_plan(&before_raw, &policy).is_err() {
        return refuse(stream, 503, "exclusion_policy_unavailable");
    }
    let current = match &survey.settings.read {
        Ok((current, _)) => current.clone(),
        Err(r) => {
            return presented_answer(
                stream,
                409,
                &json!({"error": "refused", "refusal": r, "plain": mask.text(&r.plain)}),
                &policy,
            )
        }
    };
    // Every other level Claude Code reads for this folder that holds one of
    // ThruLine's speaker hooks: this page does not change those files.
    let mut elsewhere = Vec::new();
    let mut unreadable_elsewhere = Vec::new();
    for (scope, path) in survey
        .levels
        .iter()
        .filter(|(_, path)| !same_place(path, &survey.settings.path))
    {
        match read_level(path) {
            Some(value) if holds_speaker(&value) => elsewhere.push(scope.to_string()),
            Some(_) => {}
            None => unreadable_elsewhere.push(scope.to_string()),
        }
    }
    if !unreadable_elsewhere.is_empty() {
        let plain = format!(
            "The {} Claude Code settings could not be read, so ThruLine cannot tell whether its lines would stay on. Nothing changed here.",
            scopes_phrase(&unreadable_elsewhere)
        );
        let plain = mask.text(&plain);
        let refusal = Refusal::new("unreadable_level", plain.clone());
        return presented_answer(
            stream,
            409,
            &json!({"error": "refused", "plain": plain, "refusal": refusal}),
            &policy,
        );
    }
    if !elsewhere.is_empty() {
        let plain = format!(
            "ThruLine’s lines are turned on in the {} Claude Code settings, which this page does not change, so they stay on. Turn them off in a terminal. Nothing changed here.",
            scopes_phrase(&elsewhere)
        );
        return presented_answer(
            stream,
            409,
            &json!({"error": "elsewhere", "where": elsewhere, "plain": plain}),
            &policy,
        );
    }
    if !holds_speaker(&current) {
        return presented_answer(
            stream,
            409,
            &json!({"error": "not_installed", "plain": "ThruLine’s lines are not turned on for this project. Nothing changed."}),
            &policy,
        );
    }
    let entries = speaker_entries_in(&current);
    let mut root = current;
    let removed = crate::init_speaker_uninstall_from(&mut root);
    let written = (|| -> Result<()> {
        unchanged_since_planned(&survey.settings)?;
        let folder = survey
            .settings
            .path
            .parent()
            .context("the settings file has no folder")?;
        real_folder(folder)?;
        write_replacing(&survey.settings.path, &root)
    })();
    if let Err(e) = written {
        let plain = mask.text(&format!(
            "{HOOKS_FILE} was not written, so ThruLine’s lines stay on: {e:#}"
        ));
        return presented_answer(
            stream,
            500,
            &json!({"error": "write_failed", "plain": plain}),
            &policy,
        );
    }
    let after = survey_after(state, &project, &mut mask);
    presented_answer(
        stream,
        200,
        &json!({
            "schema_version": SCHEMA_VERSION,
            "project": {"id": project.id, "root": mask.text(&project.shown_root())},
            "changed": removed > 0,
            "part": "speaker",
            "file": HOOKS_FILE,
            "removed": entries,
            "plain": format!("Took ThruLine’s lines out of {HOOKS_FILE}. Sessions in this project start without them from their next start."),
            "after": after,
        }),
        &policy,
    )
}

fn survey_after(state: &State, project: &core_hq::HqProject, mask: &mut Masker) -> Value {
    let survey = survey(state, project, mask);
    plan_document(project, &survey, mask)
}

/// The bytes on disk must still be the ones the plan read.
fn unchanged_since_planned(target: &Target) -> Result<()> {
    let planned = target
        .read
        .as_ref()
        .map(|(_, d)| d.clone())
        .map_err(|r| anyhow::anyhow!("{}", r.plain))?;
    let now = read_target_digest(&target.path)?;
    anyhow::ensure!(
        now == planned,
        "it changed while ThruLine was installing; read the plan again"
    );
    Ok(())
}

fn read_target_digest(path: &Path) -> Result<String> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if !meta.is_file() => anyhow::bail!("it is not a regular file"),
        Ok(_) => Ok(digest(&std::fs::read(path)?)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok("absent".to_string()),
        Err(e) => Err(e.into()),
    }
}

/// The folder a file goes in, made when it is absent, and never a link.
fn real_folder(dir: &Path) -> Result<()> {
    match std::fs::symlink_metadata(dir) {
        Ok(meta) if meta.file_type().is_symlink() => anyhow::bail!("its folder is a link"),
        Ok(meta) if !meta.is_dir() => anyhow::bail!("its folder is not a folder"),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => {
            std::fs::create_dir(dir)?;
            let meta = std::fs::symlink_metadata(dir)?;
            anyhow::ensure!(
                meta.is_dir() && !meta.file_type().is_symlink(),
                "its folder could not be made"
            );
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

/// `value` as `file`'s whole content, the way the installers write settings
/// (pretty JSON and a newline): a new file beside it, created exclusively,
/// flushed, given the old file's permissions, then renamed over it. A rename
/// replaces the name and never follows it.
fn write_replacing(file: &Path, value: &Value) -> Result<()> {
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    let mode = match std::fs::symlink_metadata(file) {
        Ok(meta) if !meta.is_file() => anyhow::bail!("it is not a regular file"),
        Ok(meta) => Some(meta.permissions()),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let folder = file.parent().context("it has no folder")?;
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let temp = folder.join(format!(
        ".{name}.{}.{nanos}.thruline-tmp",
        std::process::id()
    ));
    let result = (|| -> Result<()> {
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        out.write_all(text.as_bytes())?;
        out.sync_all()?;
        drop(out);
        if let Some(perm) = mode {
            std::fs::set_permissions(&temp, perm)?;
        }
        std::fs::rename(&temp, file)?;
        if let Ok(dir) = std::fs::File::open(folder) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// The hooks and/or the speaker hooks into the project-local settings, as
/// `thruline init` merges them, under the merged-level recorder guard.
fn write_settings(
    survey: &Survey,
    hooks: bool,
    speaker: bool,
) -> Result<(Vec<String>, Vec<String>)> {
    let (current, _) = survey
        .settings
        .read
        .as_ref()
        .map_err(|r| anyhow::anyhow!("{}", r.plain))?;
    let mut root = current.clone();
    let mut hooks_added = Vec::new();
    let mut speaker_added = Vec::new();
    if hooks {
        let (added, _) = crate::sensor_install_into(&mut root, None)?;
        hooks_added = added.iter().map(|e| e.to_string()).collect();
    }
    if speaker {
        let (added, _) = crate::init_speaker_install_into(&mut root, None)?;
        speaker_added = added.iter().map(|e| e.to_string()).collect();
    }
    refuse_double_recorder_across(&survey.levels, &survey.settings.path, &root)?;
    unchanged_since_planned(&survey.settings)?;
    let folder = survey
        .settings
        .path
        .parent()
        .context("the settings file has no folder")?;
    real_folder(folder)?;
    write_replacing(&survey.settings.path, &root)?;
    Ok((hooks_added, speaker_added))
}

/// The MCP server into the project's `.mcp.json`, as `thruline init` merges
/// it into `~/.claude.json`, naming this HQ's store.
fn write_mcp(survey: &Survey) -> Result<String> {
    let (current, _) = survey
        .mcp_file
        .read
        .as_ref()
        .map_err(|r| anyhow::anyhow!("{}", r.plain))?;
    let mut root = current.clone();
    anyhow::ensure!(
        crate::init_mcp_install_into(&mut root, &survey.db)?,
        "a server named thruline is already there"
    );
    unchanged_since_planned(&survey.mcp_file)?;
    write_replacing(&survey.mcp_file.path, &root)?;
    Ok(format!("mcpServers.{}", crate::MCP_SERVER_KEY))
}

/// The shim folder, exactly as `thruline shim install` run in the project's
/// folder writes it. Returns the wrappers written.
fn write_shim(survey: &Survey) -> Result<Vec<String>> {
    let dir = survey
        .run
        .dir
        .clone()
        .context("ThruLine has no folder to keep run capture in")?;
    let home = crate::shim_cmd::thruline_home()?;
    let args = crate::shim_cmd::InstallArgs {
        dir: None,
        harness: None,
        spool: None,
        session: None,
        repo: Some(survey.root.clone()),
        bin: None,
        wrap: None,
        undo: false,
        json: false,
    };
    let opts = crate::shim_cmd::install_options(&home, dir, &args)?;
    let manifest = shim::install(&opts).context("writing the shim folder")?;
    Ok(manifest.wrappers.keys().cloned().collect())
}
