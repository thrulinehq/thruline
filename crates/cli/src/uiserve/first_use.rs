//! The first-use path (T-2-7) — install, pick a project, read one true thing.
//!
//! WHY THIS FILE EXISTS. The baseline's H2T-1: the documented quickstart never
//! opened a browser and dead-ended at "0 transcript file(s)", and the first
//! true statement anybody reached was about demo data. `thruline ui` on a
//! machine with nothing registered answered with an empty Inbox and a command
//! to copy. This module is the four reads and two writes that turn that into:
//! here are the repositories this machine already has recorded work for → pick
//! one → here is the work it can already see → one click writes it down.
//!
//! WHAT IT MAY DO.
//!
//!   - `GET  /api/first-use`           — what is already visible. Reads the
//!     observed capture roots and the store. Writes nothing.
//!   - `POST /api/first-use/register`  — the picked project's container, and
//!     the recordings that are already on disk for it, read into the store.
//!   - `GET  /api/first-use/proposal`  — the Change this project's most recent
//!     recorded work would make. Derived at read; writes nothing.
//!   - `POST /api/first-use/change`    — that Change, written, with the work
//!     confirmed and every other candidate link merely suggested.
//!
//! WHAT IT MAY NOT DO. It never accepts a path, a command or a title from the
//! browser. The browser sends an opaque `key`; this module re-derives the whole
//! candidate list on every request and matches the key against what IT found,
//! so a crafted request cannot name a directory the observers do not already
//! observe. It never accepts criteria: what counts as done is a separate act on
//! a separate route (`/api/changes/<id>/scope/accept`), and a test asserts that
//! writing the Change leaves the accepted-scope table empty.
//!
//! WHAT IT SERVES. Typed fields only: counts, ISO instants, ids, and text that
//! has already been through the masker (`candidates::derive` masks the opening
//! ask; `mask_recorded` masks every path this module reads off a recording).
//! No sentence is built here — the words live in `ui/src/workspace/hq-copy.js`.

use super::*;

use std::collections::BTreeMap;
use std::time::SystemTime;

use thruline_core::capture_exclusion::{ExclusionPolicy, LEFT_OUT_FIELD};
use thruline_core::change::{
    AssociationBasis, AssociationProvenance, ChangeRequest, NewChange, RequestSource,
};
use thruline_core::ledger::Ledger;
use thruline_core::model::{ProjectRef, Source};

/// How many recordings one pick reads into the store.
///
/// A person waiting on a first screen is the whole point of this path, and an
/// unbounded read of a repository with two thousand recordings would spend the
/// five minutes the card budgets before anything appeared. What is beyond the
/// bound is SAID (`not_read`), never silently dropped.
pub(super) const READ_LIMIT: usize = 25;

/// How many Codex recordings are opened to find which repository they belong
/// to. Codex files are filed by date, not by repository, so the only way to
/// group them is to read the `cwd` each one records. Bounded for the same
/// reason, and the remainder is counted and said.
const CODEX_SCAN: usize = 120;

/// The first bytes of a recording that are read to find the repository it
/// names. The `cwd` is on the first line of both vendors' formats.
const HEAD_BYTES: usize = 64 * 1024;

/// Who a first-use decision is attributed to.
///
/// This path asks for no name, because a text field in front of the first
/// useful screen is the wall `03-USER-LENS.md` rule 8 forbids. What is true is
/// that the person at this machine clicked it, and that is what is written —
/// never a guess at an identity, and never the account name, which is not a
/// fact about who decided anything.
const ACTOR: &str = "you";

/// The address `thruline ui` opens when this store holds nothing recorded.
///
/// `#/start` is the first-use path; the empty string is the app's ordinary
/// landing, which is the Inbox. The store is ASKED (C27,
/// `thruline_core::hq::has_records`): a store holding any recorded row of a
/// listed project opens on the Inbox, picked or not; a store with no project,
/// or only projects with nothing recorded (or recorded only in a temporary
/// folder), opens on First run. A store that cannot
/// be opened (a pending schema upgrade, a file that is not there yet) is not
/// read as "you have nothing": the app decides for itself from
/// `GET /api/first-use` in that case, and this returns the ordinary address
/// rather than a guess.
///
/// It opens the ledger READ-ONLY and migrates nothing (A193): a pending
/// upgrade answers `Err` here, and the empty string with it.
pub(super) fn landing_hash(db: &Path) -> &'static str {
    let Ok(ledger) = thruline_core::ledger::Ledger::open(db) else {
        return "";
    };
    // C27: First run is for a store with nothing recorded. A store with any
    // recorded row opens on the Inbox, which picks the most recently active
    // project itself — whether or not a project was ever "picked" here.
    match thruline_core::hq::has_records(&ledger) {
        Ok(false) => START_ADDRESS,
        _ => "",
    }
}

/// The app's address for the first-use path, as a URL suffix
/// (`ui/src/routes.js` `START_HASH` is the fragment half of it, and a test in
/// that file pins the two against each other by reading this line).
///
/// THE SLASH IS LOAD-BEARING. `http://127.0.0.1:7788/#/start` is the ordinary
/// form of an address with a fragment, and it is also what keeps every reader
/// of the printed line working: three existing harnesses take the port out of
/// it by splitting on `/` (`ui_server_contract.rs`, `hq_phases.rs`,
/// `hq_phases_masking.rs`), and `…:7788#/start` made all three read the port as
/// "7788#" and panic. Measured: 18 tests failed that way before this slash.
pub(super) const START_ADDRESS: &str = "/#/start";

/// One repository this machine already has recordings for.
struct Found {
    /// The repository path, as the recordings themselves record it.
    label: String,
    /// The vendor's own folder name for this repository, when the vendor files
    /// recordings by repository (Claude Code does; Codex does not).
    ///
    /// IT IS A FACT, NOT A GUESS, and that is the whole reason it is carried:
    /// it is the name of a directory that exists on this disk, and it is also —
    /// byte for byte — the project id ingest writes for these recordings
    /// (`crates/core/src/ingest.rs`, `project_id = project.dir_name`). Asking
    /// "is this project already registered" by comparing THIS to the stored id
    /// is an equality between two copies of one vendor string. The alternative,
    /// comparing the stored `root_path`, compares a path to the result of
    /// un-mangling that same folder name by turning every `-` into `/` — which
    /// is wrong for every repository with a `-` in its name (`my-app`,
    /// `web-service`), so the screen told those people their project was not
    /// registered seconds after registering it.
    dir_name: Option<String>,
    source: &'static str,
    /// Newest first.
    files: Vec<PathBuf>,
    first: Option<SystemTime>,
    last: Option<SystemTime>,
    /// Beyond `READ_LIMIT`, counted so the surface can say so.
    not_read: usize,
}

impl Found {
    /// The handle the browser gets. Opaque, derived from what the server found,
    /// and re-derived on every request — so it can only ever name something the
    /// observers already observe.
    fn key(&self) -> String {
        let material = format!("{}\u{0}{}", self.source, self.label);
        thruline_core::chain::sha256_hex(material.as_bytes())[..16].to_string()
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn iso(at: SystemTime) -> Option<String> {
    at.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| thruline_core::ingest::iso8601_utc(d.as_secs() as i64))
}

/// The repository one recording names, read off the recording itself.
///
/// Both vendors write `"cwd"` into the opening line — Claude Code on every
/// line, Codex in `session_meta`. Reading it is a fact from the record; the
/// alternative, inverting the munged directory name, is a guess that is wrong
/// for every path with a `-` in it.
fn repository_of(file: &Path) -> Option<String> {
    use std::io::Read as _;
    let handle = std::fs::File::open(file).ok()?;
    // Bounded by READING LESS, not by cutting after the fact: the byte count
    // is a read limit, so no text is ever truncated here and the masking
    // census has nothing to classify (`crates/core/src/redact.rs`).
    let mut buffer = Vec::new();
    handle
        .take(HEAD_BYTES as u64)
        .read_to_end(&mut buffer)
        .ok()?;
    let text = String::from_utf8_lossy(&buffer);
    let at = text.find("\"cwd\":")?;
    let rest = &text[at + "\"cwd\":".len()..];
    let open = rest.find('"')?;
    let rest = &rest[open + 1..];
    let close = rest.find('"')?;
    let value = &rest[..close];
    if value.is_empty() {
        return None;
    }
    Some(value.to_string())
}

fn jsonl_under(root: &Path, recursive: bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            if recursive {
                jsonl_under(&path, true, out);
            }
            continue;
        }
        if path.extension().is_some_and(|e| e == "jsonl") {
            out.push(path);
        }
    }
}

fn newest_first(mut files: Vec<PathBuf>) -> (Vec<PathBuf>, Option<SystemTime>, Option<SystemTime>) {
    let mut stamped: Vec<(PathBuf, SystemTime)> = files
        .drain(..)
        .filter_map(|p| modified(&p).map(|m| (p, m)))
        .collect();
    stamped.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let last = stamped.first().map(|(_, m)| *m);
    let first = stamped.last().map(|(_, m)| *m);
    (stamped.into_iter().map(|(p, _)| p).collect(), first, last)
}

/// Every repository the observers can already see, newest recording first.
///
/// `notes` carries anything this pass could not reach, in machine terms; the
/// surface turns them into its own sentence. Silence about an unread file would
/// be the failure Rule 12 exists to stop.
fn look(state: &State) -> (Vec<Found>, Vec<String>, u64) {
    let mut notes = Vec::new();
    let mut found: Vec<Found> = Vec::new();
    let home = std::env::var_os("HOME").map(PathBuf::from);

    // ── Claude Code: one directory per repository, already grouped ───────
    if let Some(home) = &home {
        let root = home.join(".claude/projects");
        if let Ok(entries) = std::fs::read_dir(&root) {
            let mut dirs: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_dir())
                .collect();
            dirs.sort();
            for dir in dirs {
                let mut files = Vec::new();
                jsonl_under(&dir, false, &mut files);
                if files.is_empty() {
                    continue;
                }
                let (files, first, last) = newest_first(files);
                let label = files
                    .iter()
                    .find_map(|f| repository_of(f))
                    .unwrap_or_else(|| dir.display().to_string());
                let not_read = files.len().saturating_sub(READ_LIMIT);
                let dir_name = dir
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string());
                found.push(Found {
                    label,
                    dir_name,
                    source: "claude_code",
                    files,
                    first,
                    last,
                    not_read,
                });
            }
        }
    }

    // ── Codex: filed by date, so the repository is read off each file ────
    let mut codex_seen = 0u64;
    if let Some(home) = &home {
        let mut files = Vec::new();
        for name in ["sessions", "archived_sessions"] {
            jsonl_under(&home.join(".codex").join(name), true, &mut files);
        }
        codex_seen = files.len() as u64;
        let (files, _, _) = newest_first(files);
        let scanned = files.len().min(CODEX_SCAN);
        if files.len() > scanned {
            notes.push(format!("codex_not_scanned={}", files.len() - scanned));
        }
        let mut by_repo: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
        for file in files.into_iter().take(scanned) {
            let Some(repo) = repository_of(&file) else {
                continue;
            };
            by_repo.entry(repo).or_default().push(file);
        }
        for (label, files) in by_repo {
            let (files, first, last) = newest_first(files);
            let not_read = files.len().saturating_sub(READ_LIMIT);
            found.push(Found {
                label,
                // Codex files by date, so there is no per-repository folder to
                // read a name off. Nothing is invented in its place.
                dir_name: None,
                source: "codex",
                files,
                first,
                last,
                not_read,
            });
        }
    }

    // ── the directory this server was started in, when it is a repository ──
    if let Ok(cwd) = std::env::current_dir() {
        let here = cwd.display().to_string();
        if cwd.join(".git").exists() && !found.iter().any(|f| f.label == here) {
            found.push(Found {
                label: here,
                // Nothing has recorded anything here yet, so no vendor has
                // filed a folder for it.
                dir_name: None,
                source: "here",
                files: Vec::new(),
                first: None,
                last: None,
                not_read: 0,
            });
        }
    }

    // C21-d: work recorded inside a temporary folder (a scratch directory an
    // agent ran in, with no git repository on disk) is not a repository a
    // person can pick (`thruline_core::hq::is_scratch_root`). It is left out and
    // counted; the note carries the count in machine form.
    let before = found.len();
    // The label is the recorded `cwd` when one was read. Only when it was not
    // (the label is then the vendor folder itself) is the folder name
    // un-mangled and judged — lossily, which is why it is the fallback.
    found.retain(|f| {
        let unread = f.label.contains("/.claude/projects/");
        let root = match (&f.dir_name, unread) {
            (Some(d), true) => thruline_core::hq::decode_folder_name(d, &[]).0,
            _ => f.label.clone(),
        };
        !thruline_core::hq::is_scratch_root(&root)
    });
    let excluded = before - found.len();
    if excluded > 0 {
        notes.push(format!("temp_projects_excluded={excluded}"));
    }

    // Most recently recorded first: the one a person is most likely to mean is
    // the one they were last working in. Ties and the no-recording entry sort
    // by label, so the order is stable between two reads of the same machine.
    found.sort_by(|a, b| b.last.cmp(&a.last).then_with(|| a.label.cmp(&b.label)));
    let _ = state;
    (found, notes, codex_seen)
}

/// Which ledger project already covers this repository, if any.
///
/// TWO EQUALITIES, AND NOT ONE INVERSE. Both compare strings that were derived
/// the same way on both sides; neither tries to undo the vendor's path
/// mangling, because that undoing is not a function — `-home-sam-my-app` is
/// `/home/sam/my-app` and `/home/sam/my/app` and the record cannot say which.
/// Answering "not registered" for the first of those is how this screen came to
/// offer a person a project they had already picked, with no note that they
/// had, which let the same work be written down twice.
///
///   1. the stored root IS the path this server read off a recording (Codex,
///      whose stored root is the real path, and any project picked here with
///      nothing recorded for it yet);
///   2. the stored project id IS the vendor's own folder name (Claude Code —
///      `ingest.rs` writes `project_id = dir_name`), which is the only thing
///      that answers for a path the naive mangle cannot reproduce: measured
///      against the real `~/.claude/projects` on this machine on 2026-09-18,
///      14 of the 26 folders whose recordings state a `cwd`.
fn registered_as(ledger: &Ledger, found: &Found) -> Option<String> {
    let roots = ledger.project_roots().ok()?;
    if let Some((id, _)) = roots.iter().find(|(_, root)| root == &found.label) {
        return Some(id.clone());
    }
    let dir_name = found.dir_name.as_deref()?;
    roots
        .iter()
        .find(|(id, _)| id == dir_name)
        .map(|(id, _)| id.clone())
}

/// How many Changes this project already holds — the fact the pick screen needs
/// to stop offering a second copy of work that is already written down.
fn changes_of(ledger: &Ledger, project: &str) -> Option<u64> {
    thruline_core::change::read_changes(ledger, Some(project))
        .ok()
        .map(|changes| changes.len() as u64)
}

fn candidate_json(found: &Found, ledger: Option<&Ledger>) -> serde_json::Value {
    let (label, _record) = thruline_core::redact::mask_recorded(&found.label);
    let project = ledger.and_then(|l| registered_as(l, found));
    // Counted only for a project that exists; `null` where there is nothing to
    // count, never a 0 that would read as "this one holds no Change".
    let changes = match (ledger, project.as_deref()) {
        (Some(ledger), Some(project)) => changes_of(ledger, project),
        _ => None,
    };
    serde_json::json!({
        "key": found.key(),
        "label": label,
        "source": found.source,
        "recordings": found.files.len() as u64,
        "not_read": found.not_read as u64,
        "first_recorded": found.first.and_then(iso),
        "last_recorded": found.last.and_then(iso),
        "registered": project.is_some(),
        "project": project,
        "changes": changes,
    })
}

fn candidate_policy_root(
    found: &Found,
    project: Option<&str>,
    roots: &BTreeMap<String, String>,
) -> Option<String> {
    if let Some(root) = project
        .and_then(|id| roots.get(id))
        .filter(|root| Path::new(root).is_absolute())
    {
        return Some(root.clone());
    }
    let observed = Path::new(&found.label);
    roots
        .values()
        .filter(|root| Path::new(root).is_absolute() && observed.starts_with(Path::new(root)))
        .max_by_key(|root| root.len())
        .cloned()
        .or_else(|| observed.is_absolute().then(|| found.label.clone()))
}

/// `GET /api/first-use` — what is already visible on this machine.
pub(super) fn overview(state: &State, query: &str, stream: &mut TcpStream) -> Result<()> {
    if !query.is_empty() {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "unknown_query_parameter"}))?
                .as_slice(),
        );
    }
    // `L1.1`: the figure the Inbox gives, from the one function that gives it,
    // taken BEFORE this request opens the ledger — an open connection's
    // shared-memory file is not something this product keeps, and counting it
    // put 32,768 bytes on this screen that `/api/storage` never had.
    let bytes = kept(state).bytes;
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    let registered = ledger.project_ids().context("reading the projects")?;
    let (found, notes, codex_seen) = look(state);
    let candidates = (|| -> Result<(Vec<serde_json::Value>, u64)> {
        let roots = crate::exclusion_serve::project_roots(&ledger)?;
        let mut policies = BTreeMap::new();
        let mut candidates = Vec::with_capacity(found.len());
        let mut left_out = 0u64;
        for candidate in &found {
            let mut row = candidate_json(candidate, Some(&ledger));
            // A relative cwd with no recorded project cannot name an
            // absolute exclusion policy. Keep the already masked candidate.
            if let Some(root) = candidate_policy_root(candidate, row["project"].as_str(), &roots) {
                if !policies.contains_key(&root) {
                    policies.insert(
                        root.clone(),
                        crate::exclusion_serve::policy(&ledger, &root)?,
                    );
                }
                left_out += crate::exclusion_serve::checked_apply(&mut row, &policies[&root])?;
            }
            candidates.push(row);
        }
        Ok((candidates, left_out))
    })();
    let (candidates, left_out) = match candidates {
        Ok(result) => result,
        Err(_) => {
            return respond(
                stream,
                503,
                "application/json; charset=utf-8",
                br#"{"error":"exclusion_policy_unavailable"}"#,
            )
        }
    };
    let mut body = serde_json::json!({
        "schema_version": 1,
        "registered": registered.len() as u64,
        "store": {
            "folder": state.db.parent().unwrap_or(&state.db).display().to_string(),
            "bytes": bytes,
        },
        "candidates": candidates,
        "codex_recordings": codex_seen,
        "excluded_temp_projects": excluded_temp(&notes),
        "diagnostics": diagnostics(&notes),
        "notes": notes,
    });
    if left_out > 0 {
        body[LEFT_OUT_FIELD] = serde_json::json!(left_out);
    }
    respond(
        stream,
        200,
        "application/json; charset=utf-8",
        &serde_json::to_vec(&body)?,
    )
}

fn note_count(notes: &[String], code: &str) -> Option<u64> {
    notes.iter().find_map(|n| {
        n.strip_prefix(code)
            .and_then(|rest| rest.strip_prefix('='))
            .and_then(|v| v.parse().ok())
    })
}

fn excluded_temp(notes: &[String]) -> u64 {
    note_count(notes, "temp_projects_excluded").unwrap_or(0)
}

/// `notes[]` stays as it was (machine form, `code=count`); this is the same
/// facts typed, each with a sentence a person can read. A note this code does
/// not know is still listed, with no sentence invented for it.
fn diagnostics(notes: &[String]) -> Vec<serde_json::Value> {
    notes
        .iter()
        .map(|note| {
            let (code, count) = match note.split_once('=') {
                Some((c, v)) => (c, v.parse::<u64>().ok()),
                None => (note.as_str(), None),
            };
            let sentence = match (code, count) {
                ("codex_not_scanned", Some(n)) => Some(format!(
                    "{n} Codex {} not opened: this screen reads the newest {CODEX_SCAN}.",
                    if n == 1 { "recording was" } else { "recordings were" }
                )),
                ("temp_projects_excluded", Some(n)) => Some(format!(
                    "{n} {} recorded in a temporary folder {} not listed: scratch work is not a project.",
                    if n == 1 { "folder" } else { "folders" },
                    if n == 1 { "is" } else { "are" }
                )),
                _ => None,
            };
            serde_json::json!({"code": code, "count": count, "sentence": sentence})
        })
        .collect()
}

#[derive(serde::Deserialize)]
struct KeyRequest {
    key: String,
}

#[derive(serde::Deserialize)]
struct ProjectRequest {
    project: String,
    /// C26 (b): an HQ thread id. With it, `project` is the HQ project id (a
    /// ledger project id is accepted too) and the Change is that thread.
    #[serde(default)]
    thread: Option<String>,
}

/// `POST /api/first-use/register` — the picked project, read in.
///
/// Two writes and no more: the project container, and whatever the recordings
/// already on disk hold. No Change, no scope, no criteria, no approval — a test
/// asserts the Change table is still empty after this.
pub(super) fn register(state: &State, body: &[u8], stream: &mut TcpStream) -> Result<()> {
    let Ok(request) = serde_json::from_slice::<KeyRequest>(body) else {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "malformed_request"}))?.as_slice(),
        );
    };
    let (found, _notes, _) = look(state);
    let Some(picked) = found.into_iter().find(|f| f.key() == request.key) else {
        // The browser named something this pass did not find. It is never
        // treated as a path: an unknown key is a refusal, not a directory.
        return respond(
            stream,
            404,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "no_such_candidate"}))?.as_slice(),
        );
    };
    let mut ledger = match thruline_core::ledger::Ledger::open(&state.db) {
        Ok(ledger) => ledger,
        Err(thruline_core::error::CoreError::StoreHeld { .. }) => {
            return super::store_held(state, stream)
        }
        Err(problem) => {
            return respond(
                stream,
                503,
                "application/json; charset=utf-8",
                serde_json::to_vec(&serde_json::json!({
                    "error": "store_unavailable",
                    "detail": problem.to_string(),
                }))?
                .as_slice(),
            )
        }
    };

    // C21-d: a person is waiting on this write, and a Claude Code hook may be
    // writing a row at the same moment. Wait for it rather than fail at once.
    if let Err(problem) = ledger.wait_when_busy(super::hq::BUSY_WAIT) {
        return refused(stream, &problem.to_string());
    }
    let started = std::time::Instant::now();
    let candidates: Vec<PathBuf> = picked.files.iter().take(READ_LIMIT).cloned().collect();
    // C34 pause gate (4 of 6): the recordings under a pause in force, or whose
    // session started inside one, are not read, and are counted in the answer.
    // An unreadable record refuses the read: it can be asked again, and reading
    // a folder the person paused cannot be taken back.
    let had_recordings = !candidates.is_empty();
    let (read, paused) = match thruline_core::capture_pause::PauseGate::read(&ledger) {
        Ok(gate) => thruline_core::capture_pause::filter_files(&gate, candidates),
        Err(problem) => return refused(stream, &problem.to_string()),
    };
    if read.is_empty() && had_recordings {
        // Every recording was under a pause: nothing is read, and nothing is
        // written either — not even the project container, which the
        // nothing-recorded branch below would write under a second id.
        return respond(
            stream,
            200,
            "application/json; charset=utf-8",
            &serde_json::to_vec(&serde_json::json!({
                "schema_version": 1,
                "project": registered_as(&ledger, &picked).unwrap_or_default(),
                "read": 0,
                "not_read": picked.not_read as u64,
                "recorded_work": 0,
                "steps": 0,
                "paused_by_you": paused.paused_by_you,
                "started_while_paused": paused.started_while_paused,
            }))?,
        );
    }
    let mut events = 0u64;
    let mut sessions = 0u64;
    if read.is_empty() {
        // A repository with nothing recorded is still a project a person can
        // pick: the next screen's whole job is to say what would put something
        // in it. The container is written and nothing else is.
        let id = thruline_core::ledger::project_id_for(&mangled(&picked.label));
        if let Err(problem) = ledger.upsert_project(
            &ProjectRef {
                id: id.clone(),
                root_path: picked.label.clone(),
                source: match picked.source {
                    "codex" => Source::Codex,
                    _ => Source::ClaudeCode,
                },
                display_name: thruline_core::ledger::display_name_for(&id),
            },
            &thruline_core::ingest::iso8601_utc_now(),
        ) {
            if problem.is_busy() {
                return super::hq::store_busy(stream, started.elapsed());
            }
            return refused(stream, &problem.to_string());
        }
        return respond(
            stream,
            200,
            "application/json; charset=utf-8",
            &serde_json::to_vec(&serde_json::json!({
                "schema_version": 1,
                "project": id,
                "read": 0,
                "not_read": 0,
                "recorded_work": 0,
                "steps": 0,
                "paused_by_you": paused.paused_by_you,
                "started_while_paused": paused.started_while_paused,
            }))?,
        );
    }

    let mirror_root = match thruline_core::ingest::default_mirror_root() {
        Ok(root) => root,
        Err(problem) => return refused(stream, &problem.to_string()),
    };
    let outcome = if picked.source == "codex" {
        thruline_core::ingest::ingest_codex(
            &mut ledger,
            &read,
            &thruline_core::ingest::CodexIngestOptions {
                mirror_root,
                owner_key: None,
            },
        )
    } else {
        thruline_core::ingest::ingest(&mut ledger, &read, &mirror_root)
    };
    let report = match outcome {
        Ok(report) => report,
        Err(problem) if problem.is_busy() => {
            return super::hq::store_busy(stream, started.elapsed())
        }
        Err(problem) => return refused(stream, &problem.to_string()),
    };
    events += report.new_events;
    sessions += report.sessions;

    // Which project the read produced is asked of the store, never predicted:
    // the two vendors derive a project id differently, and a guess here would
    // be a screen pointing at a project that does not exist.
    let project = registered_as(&ledger, &picked)
        .or_else(|| report.projects.first().cloned())
        .unwrap_or_default();
    if project.is_empty() {
        return refused(stream, "the recordings named no project");
    }
    let body = serde_json::to_vec(&serde_json::json!({
        "schema_version": 1,
        "project": project,
        "read": read.len() as u64,
        "not_read": picked.not_read as u64,
        "recorded_work": sessions,
        "steps": events,
        "paused_by_you": paused.paused_by_you,
        "started_while_paused": paused.started_while_paused,
    }))?;
    respond(stream, 200, "application/json; charset=utf-8", &body)
}

/// A repository path in the munged form Claude Code files it under, so a
/// project registered with no recordings and the same project once it has some
/// carry one id rather than two.
fn mangled(path: &str) -> String {
    path.replace('/', "-")
}

fn refused(stream: &mut TcpStream, detail: &str) -> Result<()> {
    respond(
        stream,
        400,
        "application/json; charset=utf-8",
        serde_json::to_vec(&serde_json::json!({"error": "refused", "detail": detail}))?.as_slice(),
    )
}

/// The Change this project's most recent recorded work would make.
///
/// Derived, every time, from the same two readers the rest of the product uses:
/// `candidates::derive` for the opening ask (the person's own words, masked and
/// truncated there), and `change_propose::profile_for_paths` over
/// `change_link::session_changed_paths` for what would be checked. Nothing here
/// re-implements either — a second reading of "what was asked for" would be a
/// second answer to the same question.
fn derive_proposal(ledger: &Ledger, project: &str) -> Result<Option<serde_json::Value>> {
    let derived = thruline_core::candidates::derive(ledger, project)
        .context("reading what this project already recorded")?;
    // Newest recorded work first.
    //
    // WHY TWO SIGNALS. `change_link::session_window` is the reader the rest of
    // the product uses for a session's instants, and it answers `None` for any
    // stamp that is not the exact 20-character `…SSZ` form — which is every
    // Claude Code recording, because the vendor writes milliseconds and ingest
    // keeps the vendor's string verbatim (measured; this task's report, F-1).
    // Ordering on it alone would put every piece of work in an arbitrary order
    // and call it "most recent". So the instant is used where the record has a
    // comparable one, and the recording FILE's last-modified time where it does
    // not — a fact about the machine, stated as one: `recorded_at` is served
    // only from the record, and is null when the record has no comparable
    // instant, so no screen can show a date this ordering inferred.
    let mut order: Vec<(String, Option<String>, Option<SystemTime>)> = Vec::new();
    for (id, file) in ledger
        .sessions_of(project)
        .context("reading the recorded work of this project")?
    {
        let recorded = thruline_core::change_link::session_window(ledger, &id)
            .ok()
            .flatten()
            .map(|window| window.to);
        let touched = modified(Path::new(&file));
        order.push((id, recorded, touched));
    }
    order.sort_by(|a, b| {
        b.1.is_some()
            .cmp(&a.1.is_some())
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| b.2.cmp(&a.2))
            .then_with(|| b.0.cmp(&a.0))
    });
    let Some((session, recorded_at, _)) = order.into_iter().find(|(id, _, _)| {
        derived
            .opening_asks
            .iter()
            .any(|ask| &ask.session_id == id && !ask.text.trim().is_empty())
    }) else {
        return Ok(None);
    };
    let Some(ask) = derived
        .opening_asks
        .iter()
        .find(|ask| ask.session_id == session)
    else {
        return Ok(None);
    };

    let paths = thruline_core::change_link::session_changed_paths(ledger, &session)
        .context("reading which files that work touched")?;
    let root = thruline_core::workspace::established_project_root(ledger, project, None)?;
    let environment =
        thruline_core::change_propose::ProfileEnvironment::from_machine(root.as_deref());
    let profile = thruline_core::change_propose::profile_for_paths(&paths, &environment);

    Ok(Some(serde_json::json!({
        "session": session,
        "recorded_at": recorded_at,
        "title": title_of(&ask.text),
        "request": ask.text,
        "request_event": ask.event_id,
        "files_touched": paths.len() as u64,
        "profile": profile,
        "masking": ask.mask,
    })))
}

/// A proposal repeats one opening ask as both its full request and a short
/// title. Treat those as one entry: if either mentions an excluded path, show
/// the same marker in both while leaving identity, counts and check profile
/// facts intact. The profile's source-path list is another presentation entry.
/// The shared JSON filter resolves generated-file attributes for both.
fn present_proposal(proposed: &mut serde_json::Value, policy: &ExclusionPolicy) -> Result<u64> {
    if policy.is_empty() {
        return Ok(0);
    }
    let request = proposed["request"]
        .as_str()
        .context("the first-use proposal has no request")?;
    let title = proposed["title"]
        .as_str()
        .context("the first-use proposal has no title")?;
    let mut ask = serde_json::json!({"text": request, "title": title});
    let left_out = crate::exclusion_serve::checked_apply(&mut ask, policy)?;
    if left_out > 0 {
        let mark = ["text", "title"]
            .into_iter()
            .filter_map(|field| ask[field].as_str())
            .find(|text| text.starts_with("left out by your exclusion (`"))
            .context("the excluded opening ask has no marker")?
            .to_string();
        proposed["request"] = serde_json::Value::String(mark.clone());
        proposed["title"] = serde_json::Value::String(mark);
    }
    let path_count = match proposed.pointer_mut("/profile/paths_read") {
        Some(paths) if paths.is_array() => crate::exclusion_serve::checked_apply(paths, policy)?,
        _ => 0,
    };
    Ok(left_out + path_count)
}

/// The Change's name, cut out of the request's own first line.
///
/// Never rewritten and never summarised: the words are the person's, and this
/// only decides where to stop. `candidates::derive` has already masked and
/// truncated the text this reads.
fn title_of(request: &str) -> String {
    const MAX: usize = 90;
    let first = request
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    let cut = first
        .char_indices()
        .find(|(_, c)| matches!(c, '.' | '!' | '?' | ';'))
        .map(|(i, _)| i)
        .unwrap_or(first.len());
    let head = first[..cut].trim();
    let head = if head.is_empty() { first } else { head };
    if head.chars().count() <= MAX {
        return head.to_string();
    }
    let end = head
        .char_indices()
        .nth(MAX)
        .map(|(i, _)| i)
        .unwrap_or(head.len());
    format!("{}…", head[..end].trim_end())
}

/// The Change this project already holds for one piece of recorded work.
///
/// Read, never assumed: the store is asked. A Change whose link to this work is
/// merely SUGGESTED is not an answer — a suggestion is not a decision (`B8.8`),
/// and treating one as "already tracked" would silently refuse a person the one
/// write this screen exists to make.
fn already_tracking(ledger: &Ledger, project: &str, session: &str) -> Option<String> {
    thruline_core::change::read_changes(ledger, Some(project))
        .ok()?
        .into_iter()
        .find(|change| {
            change.associations.iter().any(|link| {
                link.session_id == session && link.provenance == AssociationProvenance::Confirmed
            })
        })
        .map(|change| change.id)
}

/// `GET /api/first-use/proposal?project=<id>` — read-only.
pub(super) fn proposal(state: &State, query: &str, stream: &mut TcpStream) -> Result<()> {
    let project = match project_param(query) {
        Ok(Some(project)) => project,
        Ok(None) => return refused(stream, "project is required"),
        Err(problem) => {
            return respond(
                stream,
                400,
                "application/json; charset=utf-8",
                serde_json::to_vec(&serde_json::json!({"error": problem}))?.as_slice(),
            )
        }
    };
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    if !ledger
        .project_ids()
        .context("reading the projects")?
        .iter()
        .any(|id| id == &project)
    {
        return respond(
            stream,
            404,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "no_such_project"}))?.as_slice(),
        );
    }
    let proposed = derive_proposal(&ledger, &project)?;
    let mut body = serde_json::json!({
        "schema_version": 1,
        "project": project,
        "proposed": proposed,
        "labels": change_labels_json(),
    });
    if body["proposed"].is_object() {
        let presented = (|| -> Result<u64> {
            let root = crate::exclusion_serve::project_roots(&ledger)?
                .remove(&project)
                .with_context(|| format!("first-use project {project} has no recorded root"))?;
            let policy = crate::exclusion_serve::policy(&ledger, &root)?;
            present_proposal(&mut body["proposed"], &policy)
        })();
        match presented {
            Ok(count) if count > 0 => body[LEFT_OUT_FIELD] = serde_json::Value::from(count),
            Ok(_) => {}
            Err(_) => {
                return respond(
                    stream,
                    503,
                    "application/json; charset=utf-8",
                    br#"{"error":"exclusion_policy_unavailable"}"#,
                )
            }
        }
    }
    respond(
        stream,
        200,
        "application/json; charset=utf-8",
        &serde_json::to_vec(&body)?,
    )
}

/// `POST /api/first-use/change` — the one click.
///
/// Writes, in one pass: the Change, the link to the work that was ON SCREEN as
/// **confirmed** (a person clicked it), and every other link the T-2-2
/// suggester finds as merely **suggested**. It does not write a scope version,
/// an acceptance, an implementation binding or an approval — what counts as
/// done is a separate click on a separate route.
///
/// The title is re-derived here rather than taken from the request body, so the
/// words written down are the ones the record holds and not the ones a browser
/// sent.
pub(super) fn accept(state: &State, body: &[u8], stream: &mut TcpStream) -> Result<()> {
    let Ok(request) = serde_json::from_slice::<ProjectRequest>(body) else {
        return respond(
            stream,
            400,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "malformed_request"}))?.as_slice(),
        );
    };
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(());
    };
    if let Some(thread) = request.thread.as_deref() {
        return accept_thread(state, &ledger, &request.project, thread, stream);
    }
    let Some(proposed) = derive_proposal(&ledger, &request.project)? else {
        return respond(
            stream,
            409,
            "application/json; charset=utf-8",
            serde_json::to_vec(&serde_json::json!({"error": "nothing_to_propose"}))?.as_slice(),
        );
    };
    let session = proposed["session"].as_str().unwrap_or_default().to_string();
    // ALREADY WRITTEN DOWN IS NOT WRITTEN DOWN TWICE. This route is reachable
    // more than once — a bookmark, a second tab, a back button — and each
    // arrival proposes the same most-recent piece of recorded work. Writing a
    // second Change for it put two identical items in the Inbox with nothing on
    // screen to say why there were two, which is the "one obvious thing to do"
    // the Inbox promises, broken by this product rather than by the record.
    //
    // KEYED ON THE WORK, NEVER ON THE WORDS. Two Changes may share a title
    // honestly; what cannot happen twice is one piece of recorded work being
    // CONFIRMED onto two Changes by the same first-use click. The answer is the
    // Change that already holds it, said as such, and no write at all.
    if let Some(existing) = already_tracking(&ledger, &request.project, &session) {
        let body = serde_json::to_vec(&serde_json::json!({
            "schema_version": thruline_core::change::CHANGE_VIEW_SCHEMA_VERSION,
            "change": existing,
            "already_tracked": true,
        }))?;
        return respond(stream, 200, "application/json; charset=utf-8", &body);
    }
    let title = proposed["title"].as_str().unwrap_or_default().to_string();
    let request_event = proposed["request_event"].as_str().map(str::to_string);
    if title.trim().is_empty() || session.is_empty() {
        return refused(stream, "the record holds no request to name this after");
    }

    let at = thruline_core::ingest::iso8601_utc_now();
    let change = NewChange {
        id: format!("chg_{}", ulid::Ulid::generate()),
        project_id: request.project.clone(),
        opened_at: at.clone(),
        request: ChangeRequest {
            title,
            purpose: String::new(),
            prompt_event_id: request_event,
            requester: ACTOR.to_string(),
            // A person clicked this. `Agent` and `Imported` are not reachable
            // from here, and guessing between them would record a provenance
            // nobody asserted.
            source: RequestSource::Person,
        },
    };
    if let Err(problem) = thruline_core::change::insert_change(&ledger, &change) {
        return refused(stream, &problem.to_string());
    }

    // T-3-2b — the Change is bound to the project folder as it stands now, so
    // every later check has something to be compared against. It observes and
    // claims nothing: a folder this machine cannot read is recorded as unknown.
    // A failure is said on the server's own error stream and does not unwrite
    // the Change; the screen's own answer for "nothing is bound to this Change
    // yet" is what a person would then see.
    if let Err(problem) = thruline_core::examined_inputs::bind_intended(&ledger, &change.id, &at) {
        eprintln!("thruline ui: what this Change is about was not written down: {problem}");
    }

    // The work that was on screen, confirmed — the only association this click
    // is allowed to confirm, because it is the only one a person saw.
    if let Err(problem) = thruline_core::change_link::decide(
        &ledger,
        &change.id,
        &session,
        AssociationProvenance::Confirmed,
        ACTOR,
        "picked on the first screen",
        &at,
    ) {
        return refused(stream, &problem.to_string());
    }

    // Everything else the suggester finds stays SUGGESTED. `B8.8`: a suggestion
    // is never a reason and never a confirmation.
    let mut suggested = 0u64;
    if let Ok(report) = thruline_core::change_link::suggest(&ledger, &change.id) {
        if let Ok(outcome) = thruline_core::change_link::record_suggestions(&ledger, &report, &at) {
            suggested = outcome.written.len() as u64;
        }
    }

    let confirmed = thruline_core::change_propose::confirmed_sessions(&ledger, &change.id)
        .unwrap_or_default()
        .len() as u64;
    let body = serde_json::to_vec(&serde_json::json!({
        "schema_version": thruline_core::change::CHANGE_VIEW_SCHEMA_VERSION,
        "change": change.id,
        "already_tracked": false,
        "confirmed": confirmed,
        "suggested": suggested,
        "basis": AssociationBasis::User.as_str(),
        "opened_at": at,
    }))?;
    respond(stream, 200, "application/json; charset=utf-8", &body)
}

/// `POST /api/first-use/change` with `{project, thread}` — the Change is one
/// HQ thread (C26 ruling (b)).
///
/// The thread is found through the HQ index of the project (never the
/// project-wide proposal derivation the plain form runs). The Change:
///
/// - lives in the ledger project of the thread's FIRST WORK PROMPT, and its
///   request points at that prompt — the thread's identity (its id is the
///   hash of that event id);
/// - is titled with the thread's served title (the C26 title rule, the same
///   text every screen shows);
/// - has every session of the thread in that ledger project CONFIRMED as the
///   person's (basis `user`, actor `you`). A thread can span two recording
///   projects (Claude Code and Codex on one repository); the store allows a
///   Change only the sessions of its own project, so sessions in another one
///   are listed in the answer as `not_confirmed` and not written;
/// - gets no suggested links and nothing else: no scope, no acceptance, no
///   approval.
///
/// A second click for the same thread answers the Change already holding it
/// (`already_tracked: true`, matched on the request's prompt event) and
/// writes nothing.
fn presented_thread_title(title: &str, policy: &ExclusionPolicy) -> Result<(String, u64)> {
    let mut display = serde_json::json!({"title": title});
    let left_out = crate::exclusion_serve::checked_apply(&mut display, policy)?;
    let title = display["title"]
        .as_str()
        .context("first-use title lost its display shape")?
        .to_owned();
    Ok((title, left_out))
}

fn accept_thread(
    state: &State,
    ledger: &thruline_core::ledger::Ledger,
    project: &str,
    thread: &str,
    stream: &mut TcpStream,
) -> Result<()> {
    let json = |stream: &mut TcpStream, status: u16, v: serde_json::Value| -> Result<()> {
        respond(
            stream,
            status,
            "application/json; charset=utf-8",
            &serde_json::to_vec(&v)?,
        )
    };
    if let Err(problem) = ledger.wait_when_busy(super::hq::BUSY_WAIT) {
        return refused(stream, &problem.to_string());
    }
    let projects = match thruline_core::hq::hq_projects(ledger) {
        Ok(p) => p,
        Err(problem) => return refused(stream, &problem.to_string()),
    };
    // The HQ project id, or (as the plain form takes) a ledger project id.
    let Some(hq_project) = projects
        .into_iter()
        .find(|p| p.id == project || p.members.iter().any(|m| m.ledger_project_id == project))
    else {
        return json(stream, 404, serde_json::json!({"error": "no_such_project"}));
    };
    // The server's cached index (warm at start for the newest project), not
    // a fresh build: this click must answer in well under 2 s (C27).
    let index = match super::hq::cached_index(state, ledger, &hq_project) {
        Ok(index) => index,
        Err(problem) => return refused(stream, &problem.to_string()),
    };
    let Some(found) = index.thread_identity(thread) else {
        return json(
            stream,
            404,
            serde_json::json!({
                "error": "no_such_thread",
                "detail": "no thread with this id in this project",
            }),
        );
    };
    let existing = thruline_core::change::read_changes(ledger, Some(&found.ledger_project_id))
        .ok()
        .and_then(|changes| {
            changes.into_iter().find(|c| {
                c.request.prompt_event_id.as_deref() == Some(found.first_prompt_event_id.as_str())
            })
        });
    if let Some(existing) = existing {
        return json(
            stream,
            200,
            serde_json::json!({
                "schema_version": thruline_core::change::CHANGE_VIEW_SCHEMA_VERSION,
                "change": existing.id,
                "already_tracked": true,
                "thread": found.id,
            }),
        );
    }
    if found.title.trim().is_empty() {
        return refused(stream, "the record holds no request to name this after");
    }
    // Resolve and present the title before appending the Change. A failed
    // generated-file lookup must not write a Change and then call it refused.
    let policy = match crate::exclusion_serve::policy(ledger, &hq_project.root) {
        Ok(policy) => policy,
        Err(_) => {
            return json(
                stream,
                503,
                serde_json::json!({"error": "exclusion_policy_unavailable"}),
            )
        }
    };
    let (served_title, left_out) = match presented_thread_title(&found.title, &policy) {
        Ok(presented) => presented,
        Err(_) => {
            return json(
                stream,
                503,
                serde_json::json!({"error": "exclusion_lookup_failed"}),
            )
        }
    };
    let at = thruline_core::ingest::iso8601_utc_now();
    let change = NewChange {
        id: format!("chg_{}", ulid::Ulid::generate()),
        project_id: found.ledger_project_id.clone(),
        opened_at: at.clone(),
        request: ChangeRequest {
            title: found.title.clone(),
            purpose: String::new(),
            prompt_event_id: Some(found.first_prompt_event_id.clone()),
            requester: ACTOR.to_string(),
            source: RequestSource::Person,
        },
    };
    if let Err(problem) = thruline_core::change::insert_change(ledger, &change) {
        if problem.is_busy() {
            return super::hq::store_busy(stream, super::hq::BUSY_WAIT);
        }
        return refused(stream, &problem.to_string());
    }
    // As the plain form does: bind the Change to the folder as it stands.
    if let Err(problem) = thruline_core::examined_inputs::bind_intended(ledger, &change.id, &at) {
        eprintln!("thruline ui: what this Change is about was not written down: {problem}");
    }
    let mut confirmed = Vec::new();
    let mut not_confirmed = Vec::new();
    for (session, ledger_project) in &found.sessions {
        if *ledger_project != found.ledger_project_id {
            not_confirmed.push(serde_json::json!({
                "session": session,
                "ledger_project_id": ledger_project,
                "reason": "other_recording_project",
            }));
            continue;
        }
        if let Err(problem) = thruline_core::change_link::decide(
            ledger,
            &change.id,
            session,
            AssociationProvenance::Confirmed,
            ACTOR,
            "tracked from this thread",
            &at,
        ) {
            return refused(stream, &problem.to_string());
        }
        confirmed.push(session.clone());
    }
    let mut answer = serde_json::json!({
        "schema_version": thruline_core::change::CHANGE_VIEW_SCHEMA_VERSION,
        "change": change.id,
        "already_tracked": false,
        "thread": found.id,
        "title": served_title,
        "confirmed": confirmed.len() as u64,
        "confirmed_sessions": confirmed,
        "not_confirmed": not_confirmed,
        "suggested": 0,
        "basis": AssociationBasis::User.as_str(),
        "opened_at": at,
    });
    if left_out > 0 {
        answer[LEFT_OUT_FIELD] = serde_json::Value::from(left_out);
    }
    json(stream, 200, answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    use thruline_core::capture_exclusion::ExclusionPolicy;
    use thruline_core::capture_hold::ExclusionView;

    #[test]
    fn first_use_candidate_in_a_nested_excluded_folder_uses_the_hq_root() {
        let found = Found {
            label: "/repo/private".into(),
            dir_name: None,
            source: "codex",
            files: Vec::new(),
            first: None,
            last: None,
            not_read: 0,
        };
        let roots = BTreeMap::from([("member".into(), "/repo".into())]);
        assert_eq!(
            candidate_policy_root(&found, None, &roots),
            Some("/repo".into())
        );
        let relative = Found {
            label: "relative/project".into(),
            dir_name: None,
            source: "codex",
            files: Vec::new(),
            first: None,
            last: None,
            not_read: 0,
        };
        assert_eq!(candidate_policy_root(&relative, None, &roots), None);
        let policy = ExclusionPolicy::from_exclusions(
            "/repo",
            &[ExclusionView {
                key: "private".into(),
                covers: "private".into(),
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
        let mut row = candidate_json(&found, None);
        crate::exclusion_serve::checked_apply(&mut row, &policy).unwrap();
        assert_eq!(row["label"], "left out by your exclusion (`private`)");
        assert_eq!(row["source"], "codex");
        assert_eq!(row["left_out_by_exclusion"], 1);
    }

    #[test]
    fn an_excluded_opening_ask_keeps_the_proposals_identity_and_classification() {
        let entry = ExclusionView {
            key: "src/a.rs".into(),
            covers: "src/a.rs".into(),
            note: None,
            built_in: false,
            on: true,
            source: "recorded",
            basis: None,
            since: None,
            by: None,
        };
        let policy = ExclusionPolicy::from_exclusions("/synthetic/repo", &[entry]).unwrap();
        let mut proposed = serde_json::json!({
            "session": "session-1",
            "recorded_at": "2026-10-08T00:00:00Z",
            "title": "Edit src/a",
            "request": "Please edit src/a.rs and keep the private patch",
            "request_event": "event-1",
            "files_touched": 1,
            "profile": {"checks": ["cargo test"], "paths_read": ["src/a.rs", "src/public.rs"]},
            "masking": {"complete": true}
        });
        assert_eq!(present_proposal(&mut proposed, &policy).unwrap(), 2);
        assert_eq!(proposed["title"], "left out by your exclusion (`src/a.rs`)");
        assert_eq!(
            proposed["request"],
            "left out by your exclusion (`src/a.rs`)"
        );
        assert_eq!(proposed["session"], "session-1");
        assert_eq!(proposed["request_event"], "event-1");
        assert_eq!(proposed["files_touched"], 1);
        assert_eq!(proposed["profile"]["checks"][0], "cargo test");
        assert_eq!(
            proposed["profile"]["paths_read"][0],
            "left out by your exclusion (`src/a.rs`)"
        );
        assert_eq!(proposed["profile"]["paths_read"][1], "src/public.rs");
        assert!(!proposed.to_string().contains("private patch"));

        let empty = ExclusionPolicy::from_exclusions("/synthetic/repo", &[]).unwrap();
        let mut unchanged =
            serde_json::json!({"title":"Edit src/a","request":"Please edit src/a.rs"});
        let before = serde_json::to_vec(&unchanged).unwrap();
        assert_eq!(present_proposal(&mut unchanged, &empty).unwrap(), 0);
        assert_eq!(serde_json::to_vec(&unchanged).unwrap(), before);
    }

    #[test]
    fn accepting_a_thread_preflights_the_title_that_the_reply_will_show() {
        let entry = ExclusionView {
            key: "src/a.rs".into(),
            covers: "src/a.rs".into(),
            note: None,
            built_in: false,
            on: true,
            source: "recorded",
            basis: None,
            since: None,
            by: None,
        };
        let policy = ExclusionPolicy::from_exclusions("/synthetic/repo", &[entry]).unwrap();
        assert_eq!(
            presented_thread_title("Inspect src/a.rs and copy its private text", &policy).unwrap(),
            (thruline_core::capture_exclusion::marker("src/a.rs"), 1)
        );
        let clear = ExclusionPolicy::from_exclusions("/synthetic/repo", &[]).unwrap();
        assert_eq!(
            presented_thread_title("Inspect src/a.rs and copy its private text", &clear).unwrap(),
            ("Inspect src/a.rs and copy its private text".into(), 0)
        );
    }

    /// The address the CLI prints and the address the app resolves are the
    /// same string, and the printed one carries the `/` that keeps it a URL.
    #[test]
    fn the_printed_address_is_the_apps_address_with_the_document_slash() {
        assert_eq!(START_ADDRESS, "/#/start");
        assert!(START_ADDRESS.ends_with("#/start"));
        // The port of a printed line still parses when it is split on `/`,
        // which is how three existing harnesses read it.
        let printed = format!("http://127.0.0.1:7788{START_ADDRESS}");
        let rest = printed
            .strip_prefix("http://127.0.0.1:")
            .expect("a loopback URL");
        assert_eq!(rest.split('/').next(), Some("7788"));
    }

    #[test]
    fn a_title_is_the_first_sentence_of_the_request_and_never_a_rewrite() {
        assert_eq!(
            title_of("Fix the retry loop in the uploader. It drops the last entry."),
            "Fix the retry loop in the uploader"
        );
        assert_eq!(
            title_of("  \n\nAdd a health check\nthen deploy"),
            "Add a health check"
        );
    }

    #[test]
    fn a_very_long_first_sentence_is_cut_and_says_it_was_cut() {
        let long = "a".repeat(200);
        let cut = title_of(&long);
        assert!(cut.ends_with('…'), "{cut}");
        assert!(cut.chars().count() <= 91, "{}", cut.chars().count());
    }

    #[test]
    fn a_key_names_what_the_server_found_and_changes_with_it() {
        let a = Found {
            label: "/tmp/a".into(),
            dir_name: None,
            source: "claude_code",
            files: Vec::new(),
            first: None,
            last: None,
            not_read: 0,
        };
        let b = Found {
            label: "/tmp/b".into(),
            dir_name: None,
            source: "claude_code",
            files: Vec::new(),
            first: None,
            last: None,
            not_read: 0,
        };
        let same_path_other_source = Found {
            label: "/tmp/a".into(),
            dir_name: None,
            source: "codex",
            files: Vec::new(),
            first: None,
            last: None,
            not_read: 0,
        };
        assert_ne!(a.key(), b.key());
        assert_ne!(a.key(), same_path_other_source.key());
        assert_eq!(a.key().len(), 16);
        // No part of the path survives into the handle.
        assert!(!a.key().contains("tmp"));
    }

    /// A `-` IN A REPOSITORY NAME IS NOT A `/`. `my-app`, `web-service`,
    /// `uploads-api` — the ordinary way people name repositories. The stored
    /// root for a Claude Code project is the vendor's folder name with every
    /// `-` turned into `/`, so comparing a live path to it answers "no" for all
    /// of them, and the screen offered a person a project they had already
    /// picked with nothing on it to say so.
    #[test]
    fn a_repository_whose_name_holds_a_hyphen_is_still_seen_as_already_picked() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ledger = Ledger::open(dir.path().join("ledger.db")).expect("ledger");
        // Exactly what `ingest.rs` writes for a recording of `/home/sam/my-app`:
        // the id is the vendor's folder name, and the root is that name with
        // every `-` read as a `/` — which is a path nobody has.
        let dir_name = "-home-sam-my-app";
        ledger
            .upsert_project(
                &ProjectRef {
                    id: dir_name.to_string(),
                    root_path: "/home/sam/my/app".to_string(),
                    source: Source::ClaudeCode,
                    display_name: Some("app".to_string()),
                },
                "2026-01-01T00:00:00Z",
            )
            .expect("project");

        let picked = Found {
            label: "/home/sam/my-app".into(),
            dir_name: Some(dir_name.to_string()),
            source: "claude_code",
            files: Vec::new(),
            first: None,
            last: None,
            not_read: 0,
        };
        assert_eq!(
            registered_as(&ledger, &picked).as_deref(),
            Some(dir_name),
            "a project registered seconds ago read as never picked"
        );

        // And it still says no to a repository this store has never seen: the
        // note is a fact about one row, not a note everything gets.
        let stranger = Found {
            label: "/home/sam/other-app".into(),
            dir_name: Some("-home-sam-other-app".to_string()),
            source: "claude_code",
            files: Vec::new(),
            first: None,
            last: None,
            not_read: 0,
        };
        assert_eq!(registered_as(&ledger, &stranger), None);

        // AND THE VENDOR MANGLES MORE THAN `/`. Measured against the real
        // `~/.claude/projects` on this machine on 2026-09-18: of the 26 folders
        // whose recordings state a `cwd`, replacing only `/` reproduces 12 and
        // fails on 14 — every one of those paths holds a `.`, a `_` or a space,
        // and the vendor writes `-` for each. For those, the folder name the
        // vendor chose is the ONLY thing that answers this question.
        ledger
            .upsert_project(
                &ProjectRef {
                    id: "-home-sam-my-app-2".to_string(),
                    root_path: "/home/sam/my/app/2".to_string(),
                    source: Source::ClaudeCode,
                    display_name: None,
                },
                "2026-01-01T00:00:00Z",
            )
            .expect("project");
        let dotted = Found {
            label: "/home/sam/my_app.2".into(),
            dir_name: Some("-home-sam-my-app-2".to_string()),
            source: "claude_code",
            files: Vec::new(),
            first: None,
            last: None,
            not_read: 0,
        };
        assert_ne!(
            mangled(&dotted.label),
            "-home-sam-my-app-2",
            "this case is only a case while the naive mangle cannot reach it"
        );
        assert_eq!(
            registered_as(&ledger, &dotted).as_deref(),
            Some("-home-sam-my-app-2")
        );
    }

    /// The first equality: a project picked with nothing recorded for it yet is
    /// registered under the path this server read, and is still recognised by it
    /// on the next read of the same screen.
    #[test]
    fn a_project_picked_with_nothing_recorded_is_recognised_by_the_path_it_was_registered_under() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ledger = Ledger::open(dir.path().join("ledger.db")).expect("ledger");
        let label = "/home/sam/site-v2";
        let id = thruline_core::ledger::project_id_for(&mangled(label));
        // Registered exactly as `register` registers one: the id is derived,
        // the root is the path the recordings named.

        ledger
            .upsert_project(
                &ProjectRef {
                    id: id.clone(),
                    root_path: label.to_string(),
                    source: Source::ClaudeCode,
                    display_name: None,
                },
                "2026-01-01T00:00:00Z",
            )
            .expect("project");
        let here = Found {
            label: label.into(),
            dir_name: None,
            source: "here",
            files: Vec::new(),
            first: None,
            last: None,
            not_read: 0,
        };
        assert_eq!(registered_as(&ledger, &here).as_deref(), Some(id.as_str()));
    }

    #[test]
    fn the_repository_is_read_off_the_recording_not_guessed_from_the_folder() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("s.jsonl");
        std::fs::write(
            &file,
            "{\"type\":\"user\",\"cwd\":\"/Users/x/my-app\",\"message\":{}}\n",
        )
        .expect("write");
        assert_eq!(repository_of(&file).as_deref(), Some("/Users/x/my-app"));
    }

    #[test]
    fn a_recording_that_names_no_repository_is_not_invented() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("s.jsonl");
        std::fs::write(&file, "{\"type\":\"user\"}\n").expect("write");
        assert_eq!(repository_of(&file), None);
    }
}
