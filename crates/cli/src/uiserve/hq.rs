//! HQ Next — the `/api/hq/` routes.
//!
//! Reads (each opens the ledger, derives at request time, writes nothing):
//!
//!   - `GET /api/hq/projects`                       — every project's summary
//!   - `GET /api/hq/projects/<id>`                  — one project and its threads
//!   - `GET /api/hq/projects/<id>/threads/<tid>`    — one thread, every line
//!   - `GET /api/hq/projects/<id>/statements`       — every claim's verdict, no text
//!   - `GET /api/hq/sessions/<sid>`                 — one session's timeline
//!   - `GET /api/hq/load`                           — the load's progress
//!   - `GET /api/hq/projects/<id>/search?q=…`       — the record search behind
//!     ⌘K's Find rows ([`search`]); the only address here that takes a query
//!
//! One write, same-origin required like every other action route here:
//!
//!   - `POST /api/hq/projects/<id>/load` — read every recording on this machine
//!     that belongs to the project into the ledger, in the background, through
//!     the existing ingest (`thruline_core::ingest::ingest` /
//!     `ingest_codex`), one file at a time so progress can be reported. It
//!     accepts no path from the browser: the files are found from the
//!     project's own root ([`thruline_core::hq::load_plan`]). Ingest appends
//!     only what is new, so running it again adds only what was recorded
//!     since. It refuses (409) while a load is running and (507) when the new
//!     files would not fit in the free space (1.5× their size, plus 2 GiB of
//!     headroom — the A131 rule `thruline ingest` uses).
//!
//! One more write, the told-the-agent acts (C30 R-LEDGER), handled by
//! [`super::person_acts::told_agent`]:
//!
//!   - `POST /api/hq/projects/<id>/threads/<tid>/told-agent` — a person copied
//!     a message for the agent, said they pasted it, or took that back. One
//!     `change_agent_messages` row. Nothing is sent anywhere. The project view
//!     serves each thread's last act as `told_agent`.
//!
//! One more write, the objections (C30 R-LEDGER), handled by
//! [`super::person_acts::objection`]:
//!
//!   - `POST /api/hq/projects/<id>/objections` — a person says one of the
//!     agent's statements is wrong, and why. One `statement_objections` row; the
//!     server hashes the sentence itself. It changes no verdict: the thread view
//!     serves it as `objections[]` beside the claim's unchanged verdict.
//!
//! Two more writes, a person's pause and path exclusions (C30 R-LEDGER), handled
//! by [`super::capture_holds`]. The pause gates act during capture; exclusions
//! act when the recorded material is served or exported.
//!
//!   - `POST /api/hq/projects/<id>/capture/pause` — pause this project's
//!     folder, or resume it. One `capture_holds` row on the `capture_hold`
//!     chain; for the folder the Codex observer watches it also sets the
//!     observer's in-memory flag, as `POST /api/observation` does.
//!   - `POST /api/hq/projects/<id>/capture/exclusions` — leave one path out, or
//!     put it back. One row (two the first time a built-in default is turned
//!     off). The read is `holds` on `GET /api/hq/projects/<id>/capture`.
//!
//! Every string served has been through `thruline_core::redact` inside
//! `thruline_core::hq`. Results are cached per project on its members' chain
//! positions, so a second read of an unchanged project does not rebuild.

use super::*;

use std::collections::HashMap;
use std::time::SystemTime;

use crate::exclusion_serve;
use thruline_core::hq as core_hq;
use thruline_core::ledger::Ledger;

mod inbox_hide;
mod search;

/// The search corpus kept: `(project, index fingerprint, corpus)`.
type KeptCorpus = (String, String, Arc<core_hq::search::Corpus>);

#[derive(Default)]
pub(super) struct HqState {
    load: Arc<Mutex<LoadStatus>>,
    cache: Mutex<HashMap<String, (String, Arc<core_hq::Index>)>>,
    /// One build lock per project: a request that arrives while the index is
    /// being built (by the start-up warm or another request) waits for that
    /// build and then reads the cache, instead of starting a second one.
    builders: Mutex<HashMap<String, ProjectBuild>>,
    /// Each project's list row, kept after its index is dropped, keyed like
    /// the index on the members' chain positions. The list route serves from
    /// here and never builds an index itself.
    listings: Mutex<HashMap<String, (String, core_hq::Listing)>>,
    /// C27 (c): serialized thread views, so a warm read is a copy.
    threads: Mutex<ByteCache<ThreadAt>>,
    /// C27 (c): serialized project views, so a warm read is a copy.
    projects: Mutex<ByteCache<String>>,
    /// C30 R-ROUTES "Record search": the latest project's search corpus,
    /// valid for one index fingerprint.
    search: Mutex<Option<KeptCorpus>>,
    /// One corpus build at a time.
    search_build: Mutex<()>,
    /// A per-test rendezvous around real index builds; absent from the binary.
    #[cfg(test)]
    build_probe: Mutex<Option<Arc<tests::BuildProbe>>>,
}

#[derive(Default)]
struct ProjectBuild {
    lock: Arc<Mutex<()>>,
    /// Only an actual index request opts a project into the full cache.
    requested: bool,
}

/// `(project, thread)`.
type ThreadAt = (String, String);
/// `(key, serialized view)`.
type KeyedBytes = (String, Arc<Vec<u8>>);

/// Serialized views, each valid for one key, bounded by `limit` bytes; the
/// oldest entry is dropped first.
///
/// - **Thread views** are keyed on (project, thread) and valid for one index
///   fingerprint and one answer of [`core_hq::Index::thread_active_at`] — the
///   only clock-dependent part of a thread view. A thread view reads nothing
///   but its index and `events` rows, which are append-only, so the same key
///   always serializes to the same bytes. Bounded by [`THREAD_CACHE_BYTES`].
/// - **Project views** are keyed on the project and valid for one index
///   fingerprint and one answer of [`core_hq::Index::active_threads_at`] —
///   the only clock-dependent part of a project view. A project view reads
///   nothing but its index. Bounded by [`PROJECT_CACHE_BYTES`].
struct ByteCache<K> {
    map: HashMap<K, KeyedBytes>,
    order: std::collections::VecDeque<K>,
    bytes: usize,
    limit: usize,
}

const THREAD_CACHE_BYTES: usize = 256 * 1024 * 1024;
const PROJECT_CACHE_BYTES: usize = 64 * 1024 * 1024;

impl Default for ByteCache<ThreadAt> {
    fn default() -> Self {
        ByteCache::with_limit(THREAD_CACHE_BYTES)
    }
}

impl Default for ByteCache<String> {
    fn default() -> Self {
        ByteCache::with_limit(PROJECT_CACHE_BYTES)
    }
}

impl<K: std::hash::Hash + Eq + Clone> ByteCache<K> {
    fn with_limit(limit: usize) -> Self {
        ByteCache {
            map: HashMap::new(),
            order: std::collections::VecDeque::new(),
            bytes: 0,
            limit,
        }
    }

    fn get(&self, at: &K, key: &str) -> Option<Arc<Vec<u8>>> {
        self.map
            .get(at)
            .filter(|(have, _)| have == key)
            .map(|(_, bytes)| bytes.clone())
    }

    fn put(&mut self, at: K, key: String, bytes: Arc<Vec<u8>>) {
        if bytes.len() > self.limit {
            return;
        }
        if let Some((_, old)) = self.map.remove(&at) {
            self.bytes -= old.len();
            self.order.retain(|o| o != &at);
        }
        while self.bytes + bytes.len() > self.limit {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some((_, old)) = self.map.remove(&oldest) {
                self.bytes -= old.len();
            }
        }
        self.bytes += bytes.len();
        self.order.push_back(at.clone());
        self.map.insert(at, (key, bytes));
    }
}

/// How long a UI write waits for another ThruLine process (a Claude Code
/// hook, a terminal command) to finish writing before it gives up. Only the
/// UI server's write routes wait; `Ledger::open` and every hook path keep
/// their own budgets.
pub(super) const BUSY_WAIT: Duration = Duration::from_secs(30);

/// What a person reads when that wait ran out. Served as `message`.
pub(super) const STORE_BUSY_SENTENCE: &str = "Another ThruLine process was writing to the record (a Claude Code hook or a terminal command), so nothing was changed. Try again in a moment.";

/// `503 {error: "store_busy", waited_ms, detail, message}` — never the raw
/// "database is locked".
pub(super) fn store_busy(stream: &mut TcpStream, waited: Duration) -> Result<()> {
    json(
        stream,
        503,
        &serde_json::json!({
            "error": "store_busy",
            "waited_ms": waited.as_millis() as u64,
            "detail": "the record's write lock was held by another process for the whole wait",
            "message": STORE_BUSY_SENTENCE,
        }),
    )
}

#[derive(Debug, Clone, Default, Serialize)]
struct LoadStatus {
    schema_version: u32,
    running: bool,
    project: Option<String>,
    files_total: u64,
    files_done: u64,
    claude_code_files: u64,
    codex_files: u64,
    bytes_total: u64,
    events_added: u64,
    sessions_read: u64,
    /// C27: files found exactly as the ledger last read them and not opened
    /// (also counted in `files_done`).
    files_unchanged: u64,
    /// Files that were planned and had gone from disk when the load reached
    /// them (also counted in `files_done`); nothing changed for them.
    files_gone: u64,
    /// Files not read because another process held the record's write lock
    /// for the whole wait; each also has a sentence in `errors`.
    store_busy: u64,
    /// C47 Q2: store opens refused while project removal held the database,
    /// across this HQ process. This saturating count remains visible after the
    /// hold is released.
    store_held_refusals: u64,
    /// C34 pause gate: recordings left out of this load, not planned and not
    /// counted in `files_total`, because their folder is paused now.
    paused_by_you: u64,
    /// C34 pause gate: recordings left out because their session started
    /// inside a pause that has since been lifted.
    started_while_paused: u64,
    errors: Vec<String>,
    started_at: Option<String>,
    finished_at: Option<String>,
}

pub(super) fn owns(path: &str) -> bool {
    path == "/api/hq/load"
        || path == "/api/hq/projects"
        || path.starts_with("/api/hq/projects/")
        || path.starts_with("/api/hq/sessions/")
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

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn origin_is_ours(state: &State, headers: &RequestHeaders) -> bool {
    if headers.origin_count != 1 {
        return false;
    }
    let port = port_of(&state.origin);
    headers.origin == format!("http://{}", state.origin)
        || headers.origin == format!("http://localhost:{port}")
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
    let rest = path.trim_start_matches("/api/hq/");
    let parts: Vec<&str> = rest.split('/').collect();
    // The record search is the one address here that takes a query string.
    if let ["projects", id, "search"] = parts.as_slice() {
        return search::handle(state, method, id, query, headers, stream);
    }
    if !query.is_empty() {
        return refuse(stream, 400, "unknown_query_parameter");
    }
    if headers.malformed {
        return refuse(stream, 400, "malformed_request");
    }
    match (method, parts.as_slice()) {
        ("GET", ["load"]) => {
            let status = match state.hq.load.lock() {
                Ok(s) => s.clone(),
                Err(_) => return refuse(stream, 500, "load_status_unavailable"),
            };
            let status = with_version(status, state);
            let raw = serde_json::to_vec(&status)?;
            let Some(project_id) = status.project.as_deref() else {
                return respond(stream, 200, "application/json; charset=utf-8", &raw);
            };
            let Some(ledger) = open(state, stream)? else {
                return Ok(());
            };
            let project = match core_hq::find_project(&ledger, project_id) {
                Ok(Some(project)) => project,
                Ok(None) => return refuse(stream, 503, "exclusion_policy_unavailable"),
                Err(e) => return failed(state, stream, e),
            };
            let policy = match exclusion_serve::policy(&ledger, &project.root) {
                Ok(policy) => policy,
                Err(e) => return failed(state, stream, e),
            };
            let served = exclusion_serve::bytes(&raw, &policy)?;
            respond(stream, 200, "application/json; charset=utf-8", &served)
        }
        ("GET", ["projects"]) => projects(state, stream),
        ("GET", ["projects", id]) => match decode(id) {
            Some(id) => project(state, &id, stream),
            None => refuse(stream, 400, "malformed_request"),
        },
        ("GET", ["projects", id, "inbox-hides"]) => match decode(id) {
            Some(id) => inbox_hide::read(state, &id, stream),
            None => refuse(stream, 400, "malformed_request"),
        },
        ("GET", ["projects", id, "statements"]) => match decode(id) {
            Some(id) => statements(state, &id, stream),
            None => refuse(stream, 400, "malformed_request"),
        },
        ("GET", ["projects", id, "threads", tid]) => match (decode(id), decode(tid)) {
            (Some(id), Some(tid)) => thread(state, &id, &tid, stream),
            _ => refuse(stream, 400, "malformed_request"),
        },
        ("GET", ["sessions", sid]) => match decode(sid) {
            Some(sid) => session(state, &sid, stream),
            None => refuse(stream, 400, "malformed_request"),
        },
        ("POST", ["projects", id, "load"]) => {
            if !origin_is_ours(state, headers) {
                return refuse(stream, 403, "same_origin_required");
            }
            // The load takes no body: nothing the browser could send is read.
            let empty = headers.content_lengths.is_empty()
                || (headers.content_lengths.len() == 1 && headers.content_lengths[0] == "0");
            if !empty || headers.transfer_encoding {
                return refuse(stream, 400, "no_body_expected");
            }
            match decode(id) {
                Some(id) => load(state, &id, stream),
                None => refuse(stream, 400, "malformed_request"),
            }
        }
        ("POST", ["projects", id, "inbox-hides"]) => {
            if !origin_is_ours(state, headers) {
                return refuse(stream, 403, "same_origin_required");
            }
            let Some(id) = decode(id) else {
                return refuse(stream, 400, "malformed_request");
            };
            let Some(body) = super::person_acts::read_body_capped(
                reader,
                headers,
                stream,
                inbox_hide::BODY_MAX,
            )?
            else {
                return Ok(());
            };
            inbox_hide::write(state, &id, &body, stream)
        }
        ("POST", ["projects", id, "threads", tid, "told-agent"]) => {
            if !origin_is_ours(state, headers) {
                return refuse(stream, 403, "same_origin_required");
            }
            let (Some(id), Some(tid)) = (decode(id), decode(tid)) else {
                return refuse(stream, 400, "malformed_request");
            };
            let Some(body) = super::person_acts::read_body_capped(
                reader,
                headers,
                stream,
                super::person_acts::TOLD_AGENT_BODY_MAX,
            )?
            else {
                return Ok(());
            };
            super::person_acts::told_agent(state, &id, &tid, &body, stream)
        }
        ("POST", ["projects", id, "objections"]) => {
            if !origin_is_ours(state, headers) {
                return refuse(stream, 403, "same_origin_required");
            }
            let Some(id) = decode(id) else {
                return refuse(stream, 400, "malformed_request");
            };
            let Some(body) = super::person_acts::read_body_capped(
                reader,
                headers,
                stream,
                super::person_acts::OBJECTION_BODY_MAX,
            )?
            else {
                return Ok(());
            };
            super::person_acts::objection(state, &id, &body, stream)
        }
        ("POST", ["projects", id, "capture", "pause"]) => {
            if !origin_is_ours(state, headers) {
                return refuse(stream, 403, "same_origin_required");
            }
            let Some(id) = decode(id) else {
                return refuse(stream, 400, "malformed_request");
            };
            let Some(body) = super::person_acts::read_body_capped(
                reader,
                headers,
                stream,
                super::capture_holds::PAUSE_BODY_MAX,
            )?
            else {
                return Ok(());
            };
            super::capture_holds::pause(state, &id, &body, stream)
        }
        ("POST", ["projects", id, "capture", "exclusions"]) => {
            if !origin_is_ours(state, headers) {
                return refuse(stream, 403, "same_origin_required");
            }
            let Some(id) = decode(id) else {
                return refuse(stream, 400, "malformed_request");
            };
            let Some(body) = super::person_acts::read_body_capped(
                reader,
                headers,
                stream,
                super::capture_holds::EXCLUSIONS_BODY_MAX,
            )?
            else {
                return Ok(());
            };
            super::capture_holds::exclusion(state, &id, &body, stream)
        }
        (_, ["load"])
        | (_, ["projects"])
        | (_, ["projects", _])
        | (_, ["projects", _, "threads", _])
        | (_, ["projects", _, "threads", _, "told-agent"])
        | (_, ["projects", _, "objections"])
        | (_, ["projects", _, "capture", "pause"])
        | (_, ["projects", _, "capture", "exclusions"])
        | (_, ["projects", _, "inbox-hides"])
        | (_, ["projects", _, "statements"])
        | (_, ["sessions", _])
        | (_, ["projects", _, "load"]) => refuse(stream, 405, "method_not_allowed"),
        _ => refuse(stream, 404, "no_such_route"),
    }
}

fn with_version(mut status: LoadStatus, state: &State) -> LoadStatus {
    status.schema_version = core_hq::SCHEMA_VERSION;
    status.store_held_refusals = state.store_held_refusals.load(Ordering::Relaxed);
    status
}

fn decode(part: &str) -> Option<String> {
    let d = percent_decode(part)?;
    (!d.is_empty() && !d.contains('/')).then_some(d)
}

fn open(state: &State, stream: &mut TcpStream) -> Result<Option<Ledger>> {
    match Ledger::open(&state.db) {
        Ok(l) => Ok(Some(l)),
        Err(thruline_core::error::CoreError::StoreHeld { .. }) => {
            super::store_held(state, stream)?;
            Ok(None)
        }
        Err(problem) => {
            json(
                stream,
                503,
                &serde_json::json!({"error": "store_unavailable", "detail": problem.to_string()}),
            )?;
            Ok(None)
        }
    }
}

/// The index for a project, from the cache when its members have not moved.
pub(super) fn cached_index(
    state: &State,
    ledger: &Ledger,
    project: &core_hq::HqProject,
) -> thruline_core::error::Result<Arc<core_hq::Index>> {
    index_for(&state.hq, ledger, project).map(|(_, idx)| idx)
}

/// The index for a project and the fingerprint it is current for.
fn index_for(
    hq: &HqState,
    ledger: &Ledger,
    project: &core_hq::HqProject,
) -> thruline_core::error::Result<(String, Arc<core_hq::Index>)> {
    let fp = core_hq::fingerprint(ledger, project)?;
    let cached = |hq: &HqState| {
        hq.cache.lock().ok().and_then(|cache| {
            cache
                .get(&project.id)
                .filter(|(have, _)| *have == fp)
                .map(|(_, idx)| idx.clone())
        })
    };
    if let Some(idx) = cached(hq) {
        return Ok((fp, idx));
    }
    let builder = hq
        .builders
        .lock()
        .map(|mut b| {
            let builder = b.entry(project.id.clone()).or_default();
            builder.requested = true;
            builder.lock.clone()
        })
        .unwrap_or_default();
    #[cfg(test)]
    if let Some(probe) = hq.build_probe.lock().expect("build probe").clone() {
        probe.request_waiting(
            project,
            matches!(builder.try_lock(), Err(std::sync::TryLockError::WouldBlock)),
        );
    }
    // A poisoned lock means a build panicked; building again is still right.
    let _building = builder.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(idx) = cached(hq) {
        return Ok((fp, idx));
    }
    let idx = Arc::new(build_index(hq, ledger, project)?);
    if let Ok(mut listings) = hq.listings.lock() {
        listings.insert(project.id.clone(), (fp.clone(), idx.listing()));
    }
    if let Ok(mut cache) = hq.cache.lock() {
        cache.insert(project.id.clone(), (fp.clone(), idx.clone()));
    }
    Ok((fp, idx))
}

/// Make sure a project's list row is current, building its index only for
/// as long as it takes to read the row off it. Only projects explicitly
/// requested through [`index_for`] keep their full index; warming the other
/// projects holds one index at a time. Shares the project's build lock with
/// [`index_for`] and hands that build to an overlapping index request.
fn listing_for(
    hq: &HqState,
    ledger: &Ledger,
    project: &core_hq::HqProject,
) -> thruline_core::error::Result<()> {
    let fp = core_hq::fingerprint(ledger, project)?;
    let current = |hq: &HqState| {
        hq.listings
            .lock()
            .ok()
            .is_some_and(|l| l.get(&project.id).is_some_and(|(have, _)| *have == fp))
    };
    if current(hq) {
        return Ok(());
    }
    let builder = hq
        .builders
        .lock()
        .map(|mut b| b.entry(project.id.clone()).or_default().lock.clone())
        .unwrap_or_default();
    let _building = builder.lock().unwrap_or_else(|p| p.into_inner());
    if current(hq) {
        return Ok(());
    }
    let idx = Arc::new(build_index(hq, ledger, project)?);
    if let Ok(mut listings) = hq.listings.lock() {
        listings.insert(project.id.clone(), (fp.clone(), idx.listing()));
    }
    // Registration of a full-index request and this publication/unlock
    // share the map lock. A request that registered while we were building
    // gets this index; a later request sees an already-unlocked builder.
    // Releasing the build lock inside this guard closes the late-waiter
    // race without retaining an index for every warmed project.
    if let Ok(builders) = hq.builders.lock() {
        if builders.get(&project.id).is_some_and(|b| b.requested) {
            if let Ok(mut cache) = hq.cache.lock() {
                cache.insert(project.id.clone(), (fp, idx));
            }
        }
        drop(_building);
    }
    Ok(())
}

fn build_index(
    _hq: &HqState,
    ledger: &Ledger,
    project: &core_hq::HqProject,
) -> thruline_core::error::Result<core_hq::Index> {
    #[cfg(test)]
    let probe = _hq.build_probe.lock().expect("build probe").clone();
    #[cfg(test)]
    if let Some(probe) = &probe {
        probe.before_build(project);
    }
    #[cfg(test)]
    let started = std::time::Instant::now();
    let result = core_hq::index(ledger, project);
    #[cfg(test)]
    if let Some(probe) = &probe {
        probe.after_build(project, started.elapsed());
    }
    result
}

/// At `thruline ui` start, on a background thread: every project's list row,
/// newest project first, and the newest project's index (kept). Reads only.
/// A request for a project being built meanwhile waits on that same build
/// ([`HqState::builders`]); the list route never waits and never builds — it
/// serves what is ready and a quick row for the rest.
///
/// A store with no project is not touched again: the check is made here,
/// before the thread exists, so an empty machine's store has no connection
/// open behind the first screen (its storage figure counts the store's files,
/// and an open connection adds a `-shm` file to them).
pub(super) fn warm(state: &Arc<State>) {
    let has_project = Ledger::open(&state.db)
        .and_then(|l| l.project_ids())
        .is_ok_and(|ids| !ids.is_empty());
    if !has_project {
        return;
    }
    let state = Arc::clone(state);
    std::thread::spawn(move || {
        let Ok(ledger) = Ledger::open(&state.db) else {
            return;
        };
        let Ok(projects) = core_hq::hq_projects(&ledger) else {
            return;
        };
        let latest = |p: &core_hq::HqProject| -> Option<String> {
            p.members
                .iter()
                .filter_map(|m| {
                    ledger
                        .conn()
                        .query_row(
                            "SELECT MAX(ts) FROM events WHERE project_id = ?1",
                            [&m.ledger_project_id],
                            |r| r.get::<_, Option<String>>(0),
                        )
                        .ok()
                        .flatten()
                })
                .max()
        };
        // Newest first: the project a person is most likely to open is
        // ready first. Its index is kept (its screens need it); every other
        // project keeps only its list row.
        let mut order: Vec<(Option<String>, &core_hq::HqProject)> =
            projects.iter().map(|p| (latest(p), p)).collect();
        order.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
        let started = std::time::Instant::now();
        for (n, (_, project)) in order.iter().enumerate() {
            if state.shutting_down.load(Ordering::SeqCst) {
                return;
            }
            let built = if n == 0 {
                index_for(&state.hq, &ledger, project).map(|_| ())
            } else {
                listing_for(&state.hq, &ledger, project)
            };
            if let Err(e) = built {
                tracing::debug!("warming the HQ list row of {} failed: {e}", project.id);
            }
        }
        tracing::debug!(
            "warmed {} HQ list rows in {:?}",
            order.len(),
            started.elapsed()
        );
    });
}

fn failed(state: &State, stream: &mut TcpStream, problem: impl Into<anyhow::Error>) -> Result<()> {
    let problem = problem.into();
    if super::error_is_store_held(&problem) {
        return super::store_held(state, stream);
    }
    json(
        stream,
        500,
        &serde_json::json!({"error": "read_failed", "detail": problem.to_string()}),
    )
}

/// `GET /api/hq/projects` — every project's list row. Never builds an index:
/// a project whose row the start-up warm (or an earlier read) has computed
/// is served whole (`complete: true`); any other is served from what the
/// database answers at once ([`core_hq::quick_summary`], `complete: false`,
/// index-derived counts `null`). Re-reading later fills them in.
fn projects(state: &State, stream: &mut TcpStream) -> Result<()> {
    let Some(ledger) = open(state, stream)? else {
        return Ok(());
    };
    let (list, excluded) = match core_hq::hq_projects_and_excluded(&ledger) {
        Ok(l) => l,
        Err(e) => return failed(state, stream, e),
    };
    let now = now_ms();
    let mut out = Vec::new();
    let mut complete = 0usize;
    let mut left_out = 0u64;
    for p in &list {
        let fp = match core_hq::fingerprint(&ledger, p) {
            Ok(fp) => fp,
            Err(e) => return failed(state, stream, e),
        };
        let listed = state.hq.listings.lock().ok().and_then(|l| {
            l.get(&p.id)
                .filter(|(have, _)| *have == fp)
                .map(|(_, listing)| listing.at(now))
        });
        let mut row = match listed {
            Some(summary) => {
                complete += 1;
                serde_json::to_value(summary)?
            }
            None => match core_hq::quick_summary(&ledger, p) {
                Ok(q) => serde_json::to_value(q)?,
                Err(e) => return failed(state, stream, e),
            },
        };
        let policy = match exclusion_serve::policy(&ledger, &p.root) {
            Ok(policy) => policy,
            Err(_) => return refuse(stream, 503, "exclusion_policy_unavailable"),
        };
        left_out += match present_listing_row(&mut row, &policy) {
            Ok(count) => count,
            Err(_) => return refuse(stream, 503, "exclusion_policy_unavailable"),
        };
        out.push(row);
    }
    let mut answer = serde_json::json!({
            "schema_version": core_hq::SCHEMA_VERSION,
            "projects": out,
            "excluded_temp_projects": excluded,
            "complete": complete == list.len(),
            "projects_complete": complete,
    });
    if left_out > 0 {
        answer[thruline_core::capture_exclusion::LEFT_OUT_FIELD] = serde_json::json!(left_out);
    }
    json(stream, 200, &answer)
}

fn present_listing_row(
    row: &mut serde_json::Value,
    policy: &thruline_core::capture_exclusion::ExclusionPolicy,
) -> Result<u64> {
    exclusion_serve::checked_apply(row, policy)
}

fn find(
    state: &State,
    id: &str,
    stream: &mut TcpStream,
) -> Result<Option<(Ledger, Arc<core_hq::Index>)>> {
    Ok(find_with_fingerprint(state, id, stream)?.map(|(l, i, _)| (l, i)))
}

/// [`find`], with the fingerprint the index was found under.
fn find_with_fingerprint(
    state: &State,
    id: &str,
    stream: &mut TcpStream,
) -> Result<Option<(Ledger, Arc<core_hq::Index>, String)>> {
    let Some(ledger) = open(state, stream)? else {
        return Ok(None);
    };
    let project = match core_hq::find_project(&ledger, id) {
        Ok(Some(p)) => p,
        Ok(None) => {
            refuse(stream, 404, "no_such_project")?;
            return Ok(None);
        }
        Err(e) => {
            failed(state, stream, e)?;
            return Ok(None);
        }
    };
    match index_for(&state.hq, &ledger, &project) {
        Ok((fp, idx)) => Ok(Some((ledger, idx, fp))),
        Err(e) => {
            failed(state, stream, e)?;
            Ok(None)
        }
    }
}

fn project(state: &State, id: &str, stream: &mut TcpStream) -> Result<()> {
    let Some((ledger, idx, fp)) = find_with_fingerprint(state, id, stream)? else {
        return Ok(());
    };
    // C30 R-LEDGER: each thread's last told-the-agent act, one scan of a small
    // table. It is the only thing in the view that does not come from the index.
    let told = match thruline_core::change_agent_message::last_acts(&ledger) {
        Ok(told) => told,
        Err(e) => return failed(state, stream, e),
    };
    let policy = match exclusion_serve::policy(&ledger, &idx.project().root) {
        Ok(policy) => policy,
        Err(e) => return failed(state, stream, e),
    };
    let bytes = project_bytes(&state.hq, id, &fp, &idx, &told, now_ms())?;
    let served = exclusion_serve::bytes(&bytes, &policy)?;
    respond(stream, 200, "application/json; charset=utf-8", &served)
}

/// Every thread's last told-the-agent act, keyed on thread id.
type Told = std::collections::BTreeMap<String, thruline_core::change_agent_message::ToldAct>;

/// The project view's key: the index fingerprint (the index is fixed by it),
/// the threads active at `now` — the only way the clock reaches
/// [`core_hq::Index::view`] (pinned by core's `hq_project_cache_key.rs`) — and
/// the id of the last told-the-agent act of each of this project's threads
/// that has one (the only other thing the view reads, and it comes from
/// another table: the fingerprint does not move when a person copies a
/// message). The fingerprint and each id are written length-first
/// (`<len>:<text>`), so two different keys can never spell the same string
/// whatever they contain. A project with no act has exactly the key it had
/// before the acts existed.
fn project_key(fp: &str, idx: &core_hq::Index, told: &Told, now: i64) -> String {
    let mut key = format!("{}:{fp}|", fp.len());
    for id in idx.active_threads_at(now) {
        key.push_str(&format!("{}:{id}", id.len()));
    }
    for (thread, act) in told {
        if idx.thread_active_at(thread, now).is_some() {
            key.push_str(&format!(
                "|told:{}:{thread}{}:{}",
                thread.len(),
                act.act_id.len(),
                act.act_id
            ));
        }
    }
    key
}

/// C27 (c): the serialized project view at `now`, from the cache when its
/// key ([`project_key`]) has not moved; the same bytes as
/// `serde_json::to_vec(&idx.view_told(now, told))`.
fn project_bytes(
    hq: &HqState,
    id: &str,
    fp: &str,
    idx: &core_hq::Index,
    told: &Told,
    now: i64,
) -> Result<Arc<Vec<u8>>> {
    let key = project_key(fp, idx, told, now);
    let at = id.to_string();
    if let Some(bytes) = hq.projects.lock().ok().and_then(|c| c.get(&at, &key)) {
        return Ok(bytes);
    }
    let bytes = Arc::new(serde_json::to_vec(&idx.view_told(now, told))?);
    if let Ok(mut c) = hq.projects.lock() {
        c.put(at, key, bytes.clone());
    }
    Ok(bytes)
}

/// `GET /api/hq/projects/<id>/statements` — every claim with its verdict, no
/// text. Derived from the cached index (no database read once it is warm).
fn statements(state: &State, id: &str, stream: &mut TcpStream) -> Result<()> {
    let Some((_, idx)) = find(state, id, stream)? else {
        return Ok(());
    };
    json(stream, 200, &idx.statements())
}

/// The last objection row kept against each event, keyed on the event id.
type ObjectionsLast = std::collections::BTreeMap<String, String>;

/// The thread view's key: the index fingerprint (which fixes the index), whether
/// the thread is active at the instant asked (the one way the clock reaches the
/// view), and — for each of the thread's claim events that has one — the id of
/// the last objection row kept against it (the only other thing the view reads,
/// and it comes from another table: the fingerprint does not move when a person
/// objects). Each part is written length-first, so two different keys can never
/// spell the same string. A thread nobody objected to has exactly the key it had
/// before objections existed.
fn thread_key(fp: &str, active: bool, claim_events: &[&str], last: &ObjectionsLast) -> String {
    let mut key = format!("{fp}|{active}");
    for event in claim_events {
        if let Some(row) = last.get(*event) {
            key.push_str(&format!(
                "|objected:{}:{event}{}:{row}",
                event.len(),
                row.len()
            ));
        }
    }
    key
}

fn thread(state: &State, id: &str, tid: &str, stream: &mut TcpStream) -> Result<()> {
    let Some((ledger, idx, fp)) = find_with_fingerprint(state, id, stream)? else {
        return Ok(());
    };
    let now = now_ms();
    let Some(active) = idx.thread_active_at(tid, now) else {
        return refuse(stream, 404, "no_such_thread");
    };
    let at = (id.to_string(), tid.to_string());
    let policy = match exclusion_serve::policy(&ledger, &idx.project().root) {
        Ok(policy) => policy,
        Err(e) => return failed(state, stream, e),
    };
    // C30 R-LEDGER: the objections are the one thing in this view that comes
    // from another table, so the key carries the last row kept against each of
    // the thread's claim events.
    let last = match thruline_core::statement_objection::last_by_event(&ledger) {
        Ok(last) => last,
        Err(e) => return failed(state, stream, e),
    };
    let claim_events = idx.thread_claim_events(tid).unwrap_or_default();
    let key = thread_key(&fp, active, &claim_events, &last);
    // C34 pause: rows are marked `recorded_while_paused` from the pause record,
    // which is another table the fingerprint does not move for. The key carries
    // the last pause or resume act, and a store with none has the key it had.
    let key = match thruline_core::capture_hold::pause_head(&ledger) {
        Ok(Some(head)) => format!("{key}|paused:{}:{head}", head.len()),
        Ok(None) => key,
        Err(e) => return failed(state, stream, e),
    };
    let warm = state.hq.threads.lock().ok().and_then(|c| c.get(&at, &key));
    if let Some(bytes) = warm {
        let served = exclusion_serve::bytes(&bytes, &policy)?;
        return respond(stream, 200, "application/json; charset=utf-8", &served);
    }
    match idx.thread_view(&ledger, tid, now) {
        Ok(Some(view)) => {
            let bytes = Arc::new(serde_json::to_vec(&view)?);
            if let Ok(mut c) = state.hq.threads.lock() {
                c.put(at, key, bytes.clone());
            }
            let served = exclusion_serve::bytes(&bytes, &policy)?;
            respond(stream, 200, "application/json; charset=utf-8", &served)
        }
        Ok(None) => refuse(stream, 404, "no_such_thread"),
        Err(e) => failed(state, stream, e),
    }
}

fn session(state: &State, sid: &str, stream: &mut TcpStream) -> Result<()> {
    let Some(ledger) = open(state, stream)? else {
        return Ok(());
    };
    match core_hq::session(&ledger, sid) {
        Ok(Some(view)) => {
            // A session can predate HQ membership but still belong to a
            // ledger project whose folder has standing exclusions. Resolve
            // through membership first, then the stored project root; never
            // serve the unfiltered timeline just because HQ has no row.
            let roots = exclusion_serve::project_roots(&ledger)?;
            let root = roots
                .get(&view.ledger_project_id)
                .context("session project has no recorded root")?;
            let policy = match exclusion_serve::policy(&ledger, root) {
                Ok(policy) => policy,
                Err(e) => return failed(state, stream, e),
            };
            let raw = serde_json::to_vec(&view)?;
            let served = exclusion_serve::bytes(&raw, &policy)?;
            respond(stream, 200, "application/json; charset=utf-8", &served)
        }
        Ok(None) => refuse(stream, 404, "no_such_session"),
        Err(e) => failed(state, stream, e),
    }
}

/// Where the sealed mirror goes: `$THRULINE_HOME/mirrors` when that is set
/// (so a scratch store keeps its mirror beside it), else the ingest default.
fn mirror_root() -> Option<PathBuf> {
    match std::env::var_os("THRULINE_HOME") {
        Some(home) => Some(PathBuf::from(home).join("mirrors")),
        None => thruline_core::ingest::default_mirror_root().ok(),
    }
}

fn load(state: &Arc<State>, id: &str, stream: &mut TcpStream) -> Result<()> {
    let Some(ledger) = open(state, stream)? else {
        return Ok(());
    };
    let project = match core_hq::find_project(&ledger, id) {
        Ok(Some(p)) => p,
        Ok(None) => return refuse(stream, 404, "no_such_project"),
        Err(e) => return failed(state, stream, e),
    };
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return refuse(stream, 503, "home_not_set");
    };
    let Some(mirror) = mirror_root() else {
        return refuse(stream, 503, "mirror_root_unavailable");
    };
    let plan = core_hq::load_plan(
        &project.root,
        &home.join(".claude/projects"),
        &thruline_core::ingest::default_codex_roots(),
    );
    // C34 pause gate (3 of 6): recordings under a pause in force, or whose
    // session started inside one, are left out of the plan and counted. An
    // unreadable record refuses the load (it can be asked again); it does not
    // read a folder the person paused.
    let plan = match thruline_core::capture_pause::PauseGate::read(&ledger) {
        Ok(gate) => core_hq::apply_pause_gate(plan, &gate),
        Err(e) => return failed(state, stream, e),
    };
    // Disk: only files the ledger has never seen can grow it by much.
    let known: std::collections::HashSet<String> = ledger
        .conn()
        .prepare("SELECT file_path FROM sessions")
        .and_then(|mut s| {
            s.query_map([], |r| r.get::<_, String>(0))
                .and_then(|rows| rows.collect::<std::result::Result<_, _>>())
        })
        .unwrap_or_default();
    drop(ledger);
    let new_bytes: u64 = plan
        .claude_code
        .iter()
        .chain(plan.codex.iter())
        .filter(|p| !known.contains(&p.display().to_string()))
        .filter_map(|p| std::fs::metadata(p).ok().map(|m| m.len()))
        .sum();
    let need = (new_bytes as f64 * 1.5) as u64;
    if let Some(free) = crate::free_bytes(&mirror).or_else(|| crate::free_bytes(Path::new("/"))) {
        if need > free.saturating_sub(2 * 1_073_741_824) {
            return json(
                stream,
                507,
                &serde_json::json!({
                    "error": "not_enough_disk",
                    "need_bytes": need,
                    "free_bytes": free,
                }),
            );
        }
    }
    {
        let Ok(mut status) = state.hq.load.lock() else {
            return refuse(stream, 500, "load_status_unavailable");
        };
        if status.running {
            return json(stream, 409, &with_version(status.clone(), state));
        }
        *status = LoadStatus {
            schema_version: core_hq::SCHEMA_VERSION,
            running: true,
            project: Some(project.id.clone()),
            files_total: (plan.claude_code.len() + plan.codex.len()) as u64,
            claude_code_files: plan.claude_code.len() as u64,
            codex_files: plan.codex.len() as u64,
            bytes_total: plan.bytes,
            paused_by_you: plan.paused_by_you,
            started_while_paused: plan.started_while_paused,
            started_at: Some(thruline_core::ingest::iso8601_utc_now()),
            ..LoadStatus::default()
        };
    }
    let background = Arc::clone(state);
    std::thread::spawn(move || {
        run_load(
            &background.db,
            &mirror,
            plan,
            &background.hq.load,
            &background.store_held_refusals,
        )
    });
    let snapshot = state.hq.load.lock().map(|s| s.clone()).unwrap_or_default();
    json(stream, 202, &with_version(snapshot, state))
}

fn run_load(
    db: &Path,
    mirror: &Path,
    plan: core_hq::LoadPlan,
    status: &Mutex<LoadStatus>,
    held_refusals: &AtomicU64,
) {
    let note = |f: &dyn Fn(&mut LoadStatus)| {
        if let Ok(mut s) = status.lock() {
            f(&mut s);
        }
    };
    let mut ledger = match Ledger::open(db).and_then(|l| l.wait_when_busy(BUSY_WAIT).map(|()| l)) {
        Ok(l) => l,
        Err(e) => {
            let msg = if matches!(e, thruline_core::error::CoreError::StoreHeld { .. }) {
                super::note_store_held(held_refusals);
                e.to_string()
            } else {
                thruline_core::redact::mask_recorded(&format!("store: {e}")).0
            };
            note(&|s| {
                s.errors.push(msg.clone());
                s.running = false;
                s.finished_at = Some(thruline_core::ingest::iso8601_utc_now());
            });
            return;
        }
    };
    let outcome = core_hq::run_load(&mut ledger, mirror, &plan, &mut |file| {
        let mut errors = file.problems.clone();
        if file.store_busy {
            errors.push(format!(
                "{}: {STORE_BUSY_SENTENCE}",
                file.file
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            ));
        }
        note(&|s| {
            s.files_done += 1;
            s.files_unchanged += u64::from(file.skipped_unopened);
            s.files_gone += u64::from(file.gone);
            s.events_added += file.events_added;
            s.sessions_read += file.sessions;
            s.store_busy += u64::from(file.store_busy);
            s.errors.extend(errors.iter().cloned());
        });
        true
    });
    if outcome.files_done == 0 && !outcome.errors.is_empty() {
        // The mirror could not be opened: nothing was read.
        note(&|s| s.errors.extend(outcome.errors.iter().cloned()));
    }
    note(&|s| {
        s.running = false;
        s.finished_at = Some(thruline_core::ingest::iso8601_utc_now());
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use thruline_core::ingest::{ingest, ingest_codex, CodexIngestOptions};

    #[derive(Default)]
    struct BuildProbeState {
        builds: usize,
        building: bool,
        request_waiting: bool,
        request_blocked: bool,
        released: bool,
        build_times: Vec<Duration>,
    }

    pub(super) struct BuildProbe {
        project: String,
        state: Mutex<BuildProbeState>,
        changed: std::sync::Condvar,
    }

    impl BuildProbe {
        fn new(project: String) -> Self {
            Self {
                project,
                state: Mutex::new(BuildProbeState::default()),
                changed: std::sync::Condvar::new(),
            }
        }

        pub(super) fn before_build(&self, project: &core_hq::HqProject) {
            if project.id != self.project {
                return;
            }
            let mut state = self.state.lock().expect("probe state");
            state.builds += 1;
            if state.builds == 1 {
                state.building = true;
                self.changed.notify_all();
                let (state, _) = self
                    .changed
                    .wait_timeout_while(state, Duration::from_secs(10), |s| !s.released)
                    .expect("release warm build");
                assert!(state.released, "test never released the warm build");
            }
        }

        pub(super) fn request_waiting(&self, project: &core_hq::HqProject, blocked: bool) {
            if project.id == self.project {
                let mut state = self.state.lock().expect("probe state");
                if state.building && !state.released {
                    state.request_waiting = true;
                    state.request_blocked = blocked;
                    self.changed.notify_all();
                }
            }
        }

        pub(super) fn after_build(&self, project: &core_hq::HqProject, elapsed: Duration) {
            if project.id == self.project {
                self.state
                    .lock()
                    .expect("probe state")
                    .build_times
                    .push(elapsed);
            }
        }

        fn wait_for(&self, ready: impl Fn(&BuildProbeState) -> bool) {
            let state = self.state.lock().expect("probe state");
            let (state, _) = self
                .changed
                .wait_timeout_while(state, Duration::from_secs(10), |s| !ready(s))
                .expect("probe rendezvous");
            assert!(ready(&state), "test did not reach the expected build phase");
        }

        fn release(&self) {
            self.state.lock().expect("probe state").released = true;
            self.changed.notify_all();
        }
    }

    /// Generated recordings only. A is newest; B has enough separate
    /// sessions to exercise the normal parallel index reader.
    fn generated_warm_projects(tmp: &Path, ledger: &mut Ledger) -> Vec<core_hq::HqProject> {
        let mut folders = Vec::new();
        for (name, day, sessions) in [("a-newest", "02", 1), ("b-older", "01", 24)] {
            let root = tmp.join(name);
            std::fs::create_dir_all(root.join(".git")).expect("generated repository");
            let root = root.to_string_lossy();
            let folder = tmp
                .join("recordings")
                .join(thruline_core::speaker::mangle_root(&root));
            std::fs::create_dir_all(&folder).expect("generated recordings");
            for session in 0..sessions {
                let sid = format!("{name}-{session:04}");
                let mut body = String::new();
                for turn in 0..40 {
                    for (part, role, content) in [
                        (
                            0,
                            "user",
                            serde_json::json!(format!("Fix parser case {turn} in src/parser.rs.")),
                        ),
                        (
                            1,
                            "assistant",
                            serde_json::json!([{
                                "type": "tool_use", "id": format!("{sid}-edit-{turn}"),
                                "name": "Edit", "input": {
                                    "file_path": format!("{root}/src/parser.rs"),
                                    "old_string": "a", "new_string": "b"
                                }
                            }]),
                        ),
                        (
                            2,
                            "user",
                            serde_json::json!([{
                                "type": "tool_result", "tool_use_id": format!("{sid}-edit-{turn}"),
                                "content": "ok", "is_error": false
                            }]),
                        ),
                    ] {
                        body.push_str(
                            &serde_json::json!({
                                "type": role, "uuid": format!("{sid}-{turn}-{part}"),
                                "parentUuid": null, "isSidechain": false, "sessionId": sid,
                                "timestamp": format!("2026-10-{day}T09:{turn:02}:0{part}.000Z"),
                                "cwd": root, "message": {"role": role, "content": content}
                            })
                            .to_string(),
                        );
                        body.push('\n');
                    }
                }
                std::fs::write(folder.join(format!("{sid}.jsonl")), body)
                    .expect("generated recording");
            }
            folders.push(folder);
        }
        ingest(ledger, &folders, &tmp.join("mirrors")).expect("generated ingest");
        let projects = core_hq::hq_projects(ledger).expect("generated projects");
        assert_eq!(projects.len(), 2);
        projects
    }

    #[test]
    fn an_inbox_request_overlapping_an_older_project_warm_reuses_that_build() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db = tmp.path().join("ledger.db");
        let mut ledger = Ledger::open(&db).expect("generated store");
        let projects = generated_warm_projects(tmp.path(), &mut ledger);
        let newest = projects.iter().find(|p| p.name == "a-newest").expect("A");
        let older = projects.iter().find(|p| p.name == "b-older").expect("B");
        let hq = HqState::default();
        let first = std::time::Instant::now();
        index_for(&hq, &ledger, newest).expect("newest project warm");
        let newest_time = first.elapsed();
        assert_eq!(hq.cache.lock().expect("cache").len(), 1);

        let probe = Arc::new(BuildProbe::new(older.id.clone()));
        *hq.build_probe.lock().expect("build probe") = Some(probe.clone());
        let (fp, idx, request_time) = std::thread::scope(|scope| {
            let warmer = scope.spawn(|| {
                let reader = Ledger::open(&db).expect("warm reader");
                listing_for(&hq, &reader, older).expect("older project warm");
            });
            probe.wait_for(|s| s.building);
            let request = scope.spawn(|| {
                let reader = Ledger::open(&db).expect("request reader");
                let started = std::time::Instant::now();
                let (fp, idx) = index_for(&hq, &reader, older).expect("Inbox index");
                (fp, idx, started.elapsed())
            });
            probe.wait_for(|s| s.request_waiting);
            let blocked = probe.state.lock().expect("probe state").request_blocked;
            probe.release();
            assert!(
                blocked,
                "the request did not overlap the held warm build lock"
            );
            warmer.join().expect("warm thread");
            request.join().expect("request thread")
        });
        let now = core_hq::parse_ms("2026-10-03T00:00:00Z").expect("time");
        let fresh = core_hq::index(&ledger, older).expect("fresh comparison");
        let served = project_bytes(&hq, &older.id, &fp, &idx, &Told::new(), now).expect("view");
        assert!(
            !idx.view(now).threads.is_empty(),
            "generated project has no Inbox threads"
        );
        assert_eq!(
            *served,
            serde_json::to_vec(&fresh.view(now)).expect("fresh bytes")
        );
        let again = index_for(&hq, &ledger, older).expect("cached request").1;
        assert!(Arc::ptr_eq(&idx, &again));
        let state = probe.state.lock().expect("probe state");
        eprintln!(
            "synthetic warm overlap: newest={newest_time:?}, older builds={}, core build times={:?}, waiting Inbox index={request_time:?}",
            state.builds, state.build_times
        );
        assert_eq!(
            state.builds, 1,
            "the waiting Inbox rebuilt the completed warm index"
        );
    }

    #[test]
    fn warming_keeps_only_requested_indexes_and_refreshes_their_fingerprint() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut ledger = Ledger::open(tmp.path().join("ledger.db")).expect("generated store");
        let projects = generated_warm_projects(tmp.path(), &mut ledger);
        let newest = projects.iter().find(|p| p.name == "a-newest").expect("A");
        let older = projects.iter().find(|p| p.name == "b-older").expect("B");
        let hq = HqState::default();
        index_for(&hq, &ledger, newest).expect("newest index");
        listing_for(&hq, &ledger, older).expect("unrequested listing");
        {
            let cache = hq.cache.lock().expect("cache");
            assert_eq!(cache.len(), 1, "warming retained an unrequested full index");
            assert!(!cache.contains_key(&older.id));
        }
        assert!(hq
            .listings
            .lock()
            .expect("listings")
            .contains_key(&older.id));
        let (before_fp, before) = index_for(&hq, &ledger, older).expect("request older index");

        // A new generated session moves only B's fingerprint. The next
        // warm must hand over the new index, never the formerly cached one.
        let folder = tmp
            .path()
            .join("recordings")
            .join(thruline_core::speaker::mangle_root(&older.root));
        let body = std::fs::read_to_string(folder.join("b-older-0000.jsonl"))
            .expect("generated recording")
            .replace("b-older-0000", "b-older-extra")
            .replace("2026-10-01", "2026-10-03");
        std::fs::write(folder.join("b-older-extra.jsonl"), body).expect("new generated session");
        ingest(&mut ledger, &[folder], &tmp.path().join("mirrors")).expect("append generated rows");
        let after_fp = core_hq::fingerprint(&ledger, older).expect("new fingerprint");
        assert_ne!(before_fp, after_fp, "the fixture did not move B's record");
        listing_for(&hq, &ledger, older).expect("updated requested listing");
        let kept = {
            let cache = hq.cache.lock().expect("cache");
            assert_eq!(cache.len(), 2);
            let (fp, idx) = cache.get(&older.id).expect("requested index kept");
            assert_eq!(*fp, after_fp, "warming left the old requested index cached");
            idx.clone()
        };
        let (served_fp, served) = index_for(&hq, &ledger, older).expect("updated request");
        assert_eq!(served_fp, after_fp);
        assert!(
            Arc::ptr_eq(&kept, &served),
            "request rebuilt the newly warmed index"
        );
        assert!(
            !Arc::ptr_eq(&before, &served),
            "request reused the stale index"
        );
        let now = core_hq::parse_ms("2026-10-04T00:00:00Z").expect("time");
        let fresh = core_hq::index(&ledger, older).expect("fresh comparison");
        let bytes = project_bytes(&hq, &older.id, &served_fp, &served, &Told::new(), now)
            .expect("updated view");
        assert_eq!(
            *bytes,
            serde_json::to_vec(&fresh.view(now)).expect("fresh bytes")
        );
        assert_ne!(
            *bytes,
            serde_json::to_vec(&before.view(now)).expect("old bytes")
        );
    }

    #[test]
    fn project_list_hides_a_nested_member_root() {
        use thruline_core::capture_hold::ExclusionView;

        let policy = thruline_core::capture_exclusion::ExclusionPolicy::from_exclusions(
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
        let mut row = serde_json::json!({
            "id": "hq-project",
            "root": "/repo",
            "members": [{"ledger_project_id": "outer", "root": "/repo"},
                        {"ledger_project_id": "nested", "root": "/repo/private"}],
            "threads": 2
        });
        assert_eq!(present_listing_row(&mut row, &policy).unwrap(), 1);
        assert!(!row.to_string().contains("/repo/private"));
        assert_eq!(
            row["members"][1]["root"],
            "left out by your exclusion (`private`)"
        );
        assert_eq!(row["threads"], 2);
        assert_eq!(row["left_out_by_exclusion"], 1);
    }

    fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/tests/fixtures/hq")
    }

    fn ingest_claude(ledger: &mut Ledger, mirrors: &Path) {
        ingest(ledger, &[fixtures().join("claude/-work-app")], mirrors).expect("claude ingest");
    }

    fn ingest_all_codex(ledger: &mut Ledger, mirrors: &Path) {
        let codex: Vec<PathBuf> = std::fs::read_dir(fixtures().join("codex"))
            .expect("codex fixtures")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        ingest_codex(
            ledger,
            &codex,
            &CodexIngestOptions {
                mirror_root: mirrors.to_path_buf(),
                owner_key: Some("hq-test-owner".to_string()),
            },
        )
        .expect("codex ingest");
    }

    /// C27 (c), proof 1: a project view served through the cache is, byte
    /// for byte, `idx.view(now)` at every instant asked — including the
    /// instants on both sides of each thread's active window, asked in an
    /// order that crosses a window edge from one request to the next — and a
    /// second request under the same key is the cached copy itself.
    #[test]
    fn a_cached_project_view_is_the_fresh_view_at_every_instant() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mirrors = tmp.path().join("mirrors");
        let mut ledger = Ledger::open(tmp.path().join("ledger.db")).expect("open");
        ingest_claude(&mut ledger, &mirrors);
        ingest_all_codex(&mut ledger, &mirrors);
        let project = core_hq::hq_projects(&ledger).expect("projects").remove(0);
        let fp = core_hq::fingerprint(&ledger, &project).expect("fingerprint");
        let idx = core_hq::index(&ledger, &project).expect("index");
        let base = core_hq::parse_ms("2026-10-07T00:00:00Z").expect("instant");
        let hq = HqState::default();
        let mut instants = vec![base];
        for t in idx.view(base).threads {
            let last = t
                .last_activity_at
                .as_deref()
                .and_then(core_hq::parse_ms)
                .expect("last activity");
            instants.extend([
                last,
                last + core_hq::AGENT_ACTIVE_MS + 1,
                last - 60_000,
                last - core_hq::AGENT_ACTIVE_MS - 1,
                last + core_hq::AGENT_ACTIVE_MS - 1,
                last + 86_400_000,
            ]);
        }
        // Forwards, then back, so every edge is crossed both ways.
        let back: Vec<i64> = instants.iter().rev().copied().collect();
        instants.extend(back);
        let mut views = std::collections::BTreeSet::new();
        for now in instants {
            let fresh = serde_json::to_vec(&idx.view(now)).expect("json");
            let served =
                project_bytes(&hq, &project.id, &fp, &idx, &Told::new(), now).expect("bytes");
            assert_eq!(
                *served, fresh,
                "the served project view at {now} is not the fresh one"
            );
            let again =
                project_bytes(&hq, &project.id, &fp, &idx, &Told::new(), now).expect("bytes");
            assert!(
                Arc::ptr_eq(&served, &again),
                "a second read under the same key was not the cached copy"
            );
            views.insert(fresh);
        }
        assert!(
            views.len() > 1,
            "no active-window edge changed the view: the test proved nothing"
        );
    }

    /// C27 (c), proof 2 at the function: a record that moves (a new
    /// fingerprint and the index built for it) is never answered with the
    /// view cached for the old one, at the same instant.
    #[test]
    fn a_cached_project_view_never_outlives_a_new_fingerprint() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mirrors = tmp.path().join("mirrors");
        let mut ledger = Ledger::open(tmp.path().join("ledger.db")).expect("open");
        ingest_claude(&mut ledger, &mirrors);
        let project = core_hq::hq_projects(&ledger).expect("projects").remove(0);
        let now = core_hq::parse_ms("2026-10-07T00:00:00Z").expect("instant");
        let hq = HqState::default();
        let fp1 = core_hq::fingerprint(&ledger, &project).expect("fingerprint");
        let idx1 = core_hq::index(&ledger, &project).expect("index");
        let before =
            project_bytes(&hq, &project.id, &fp1, &idx1, &Told::new(), now).expect("bytes");

        ingest_all_codex(&mut ledger, &mirrors);
        let project = core_hq::find_project(&ledger, &project.id)
            .expect("find")
            .expect("same project");
        let fp2 = core_hq::fingerprint(&ledger, &project).expect("fingerprint");
        let idx2 = core_hq::index(&ledger, &project).expect("index");
        let fresh = serde_json::to_vec(&idx2.view(now)).expect("json");
        assert_ne!(fp1, fp2, "the second ingest did not move the record");
        assert_ne!(*before, fresh, "the second ingest did not change the view");
        let after = project_bytes(&hq, &project.id, &fp2, &idx2, &Told::new(), now).expect("bytes");
        assert_eq!(
            *after, fresh,
            "the cached view outlived a change to the record"
        );
    }

    /// C30 R-LEDGER: the told-the-agent acts live in another table, so the
    /// record's fingerprint does not move when one is kept. The served view
    /// must still change with each act (the cache key carries the act ids),
    /// must be the fresh view, and a project with no act must keep the key
    /// and the bytes it had before the acts existed.
    #[test]
    fn a_cached_project_view_never_outlives_a_told_the_agent_act() {
        use thruline_core::change_agent_message as told;
        let tmp = tempfile::tempdir().expect("tempdir");
        let mirrors = tmp.path().join("mirrors");
        let mut ledger = Ledger::open(tmp.path().join("ledger.db")).expect("open");
        ingest_claude(&mut ledger, &mirrors);
        let project = core_hq::hq_projects(&ledger).expect("projects").remove(0);
        let fp = core_hq::fingerprint(&ledger, &project).expect("fingerprint");
        let idx = core_hq::index(&ledger, &project).expect("index");
        let now = core_hq::parse_ms("2026-10-07T00:00:00Z").expect("instant");
        let hq = HqState::default();
        let none = Told::new();
        assert_eq!(
            project_key(&fp, &idx, &none, now),
            format!(
                "{}:{fp}|{}",
                fp.len(),
                idx.active_threads_at(now)
                    .iter()
                    .map(|id| format!("{}:{id}", id.len()))
                    .collect::<String>()
            ),
            "a project with no act keeps the key it had before"
        );
        let before = project_bytes(&hq, &project.id, &fp, &idx, &none, now).expect("bytes");
        assert!(!String::from_utf8_lossy(&before).contains("told_agent"));

        let thread = idx.view(now).threads.remove(0).id;
        let found = idx.thread_identity(&thread).expect("identity");
        let anchor = told::Anchor {
            thread_id: found.id,
            thread_event_id: found.first_prompt_event_id,
            change_id: None,
            version_id: found.current_version_id,
        };
        told::copied(&ledger, &anchor, "sam", "hello", "2026-10-07T12:00:00Z")
            .expect("ledger")
            .expect("kept");
        assert_eq!(
            core_hq::fingerprint(&ledger, &project).expect("fingerprint"),
            fp,
            "an act is not an event: the record's fingerprint does not move"
        );
        let one = told::last_acts(&ledger).expect("acts");
        let copied = project_bytes(&hq, &project.id, &fp, &idx, &one, now).expect("bytes");
        assert_ne!(
            *copied, *before,
            "the cache served the view from before the act"
        );
        assert_eq!(
            *copied,
            serde_json::to_vec(&idx.view_told(now, &one)).expect("json")
        );
        assert!(String::from_utf8_lossy(&copied).contains("\"last_act\":\"copied\""));
        let again = project_bytes(&hq, &project.id, &fp, &idx, &one, now).expect("bytes");
        assert!(
            Arc::ptr_eq(&copied, &again),
            "same acts, same key: the cached copy"
        );

        told::marked_pasted(&ledger, &anchor, "sam", None, "2026-10-07T12:01:00Z")
            .expect("ledger")
            .expect("kept");
        let two = told::last_acts(&ledger).expect("acts");
        let marked = project_bytes(&hq, &project.id, &fp, &idx, &two, now).expect("bytes");
        assert_ne!(
            *marked, *copied,
            "a second act did not move the served view"
        );
        assert!(String::from_utf8_lossy(&marked).contains("\"last_act\":\"marked_pasted\""));
    }

    /// C30 R-LEDGER: the objections live in another table, so the record's
    /// fingerprint does not move when one is kept. The thread view's key must,
    /// for the thread whose claim was objected to and for no other, and a
    /// thread nobody objected to must keep the key it had before.
    #[test]
    fn a_thread_views_key_moves_with_an_objection_to_one_of_its_claims_and_only_then() {
        let mut last = ObjectionsLast::new();
        let events = ["e1", "e2"];
        let plain = "fp|true".to_string();
        assert_eq!(
            thread_key("fp", true, &events, &last),
            plain,
            "no objection, the old key"
        );
        assert_ne!(
            thread_key("fp", false, &events, &last),
            plain,
            "the clock still counts"
        );

        last.insert("elsewhere".into(), "obj_1".into());
        assert_eq!(
            thread_key("fp", true, &events, &last),
            plain,
            "an objection to another thread's claim is not this thread's key"
        );
        last.insert("e2".into(), "obj_2".into());
        let one = thread_key("fp", true, &events, &last);
        assert_ne!(
            one, plain,
            "an objection to this thread's claim moves the key"
        );
        last.insert("e2".into(), "obj_3".into());
        assert_ne!(
            thread_key("fp", true, &events, &last),
            one,
            "a second row against the same claim moves it again"
        );
        // Length-first: two different histories cannot spell one key.
        let mut a = ObjectionsLast::new();
        a.insert("e1".into(), "2:xy".into());
        let mut b = ObjectionsLast::new();
        b.insert("e12".into(), "xy".into());
        assert_ne!(
            thread_key("fp", true, &["e1", "e12"], &a),
            thread_key("fp", true, &["e1", "e12"], &b)
        );
    }

    #[test]
    fn the_project_cache_is_bounded() {
        let mut c: ByteCache<String> = ByteCache::with_limit(10);
        c.put("a".into(), "k".into(), Arc::new(vec![0; 6]));
        c.put("b".into(), "k".into(), Arc::new(vec![0; 6]));
        assert!(c.get(&"a".to_string(), "k").is_none(), "the oldest stays");
        assert!(c.get(&"b".to_string(), "k").is_some());
        c.put("c".into(), "k".into(), Arc::new(vec![0; 11]));
        assert!(
            c.get(&"c".to_string(), "k").is_none(),
            "over the limit kept"
        );
        assert!(c.bytes <= 10);
        assert_eq!(ByteCache::<String>::default().limit, PROJECT_CACHE_BYTES);
    }
}
